//! Authenticated loopback-only internal HTTP bridge for Work Host Executor.
//!
//! The bridge is intentionally separate from the configurable Web Server. It
//! binds to loopback on an ephemeral port and issues a per-Pi-process bearer
//! token. Workspace, task, and Product WorkRun identity are all taken from the
//! authenticated token; the Pi extension cannot choose them in its payload.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;

use crate::work::models::ExecutionContext;
use crate::work::paths::WorkPaths;

const BRIDGE_READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeSubject {
    #[default]
    Root,
}

#[derive(Debug, Clone)]
pub struct ProcessBridgeTokenInfo {
    pub token: String,
    /// Product-level WorkRun ID, never the underlying Pi Run ID.
    pub run_id: String,
    pub task_id: Option<String>,
    pub workspace_id: String,
    pub execution_context: ExecutionContext,
    /// System proxy bound by the host when this Pi process was launched.
    ///
    /// The Work Pi child receives the same value through its environment, but
    /// the authenticated bridge must use the host-bound copy because the
    /// bridge handlers run in the host process.
    pub proxy_url: Option<String>,
    pub subject: BridgeSubject,
}

#[derive(Clone)]
pub struct InternalBridgeState {
    pub tokens: Arc<RwLock<HashMap<String, ProcessBridgeTokenInfo>>>,
    pub effective_port: Arc<AtomicU16>,
    ready: Arc<Notify>,
}

impl Default for InternalBridgeState {
    fn default() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            effective_port: Arc::new(AtomicU16::new(0)),
            ready: Arc::new(Notify::new()),
        }
    }
}

static BRIDGE_STATE: std::sync::OnceLock<InternalBridgeState> = std::sync::OnceLock::new();

pub fn bridge_state() -> &'static InternalBridgeState {
    BRIDGE_STATE.get_or_init(InternalBridgeState::default)
}

async fn wait_for_port(state: &InternalBridgeState) -> Result<u16, String> {
    let notified = state.ready.notified();
    let port = state.effective_port.load(Ordering::Acquire);
    if port != 0 {
        return Ok(port);
    }

    tokio::time::timeout(BRIDGE_READY_TIMEOUT, notified)
        .await
        .map_err(|_| "Internal Work bridge did not become ready in time".to_string())?;
    let port = state.effective_port.load(Ordering::Acquire);
    if port == 0 {
        Err("Internal Work bridge is unavailable".to_string())
    } else {
        Ok(port)
    }
}

/// Register a root Work Runtime process and generate a unique bearer token.
pub async fn register_session_token(
    run_id: &str,
    workspace_id: &str,
    task_id: Option<&str>,
    execution_context: ExecutionContext,
) -> Result<(u16, String), String> {
    register_session_token_with_subject_and_proxy(
        run_id,
        workspace_id,
        task_id,
        execution_context,
        BridgeSubject::Root,
        None,
    )
    .await
}

/// Register a root Pi process with the proxy configuration selected by the
/// host for this Work launch.
pub async fn register_session_token_with_proxy(
    run_id: &str,
    workspace_id: &str,
    task_id: Option<&str>,
    execution_context: ExecutionContext,
    proxy_url: Option<String>,
) -> Result<(u16, String), String> {
    register_session_token_with_subject_and_proxy(
        run_id,
        workspace_id,
        task_id,
        execution_context,
        BridgeSubject::Root,
        proxy_url,
    )
    .await
}

/// Register a Work Runtime process with an explicit subject identity.
pub async fn register_session_token_with_subject(
    run_id: &str,
    workspace_id: &str,
    task_id: Option<&str>,
    execution_context: ExecutionContext,
    subject: BridgeSubject,
) -> Result<(u16, String), String> {
    register_session_token_with_subject_and_proxy(
        run_id,
        workspace_id,
        task_id,
        execution_context,
        subject,
        None,
    )
    .await
}

/// Register a Work Runtime process while preserving host-selected network
/// policy across the authenticated bridge.
pub async fn register_session_token_with_subject_and_proxy(
    run_id: &str,
    workspace_id: &str,
    task_id: Option<&str>,
    execution_context: ExecutionContext,
    subject: BridgeSubject,
    proxy_url: Option<String>,
) -> Result<(u16, String), String> {
    let state = bridge_state();
    let port = wait_for_port(state).await?;
    let token = format!("wbt-{}", Uuid::new_v4());
    let info = ProcessBridgeTokenInfo {
        token: token.clone(),
        run_id: run_id.to_string(),
        task_id: task_id.map(String::from),
        workspace_id: workspace_id.to_string(),
        execution_context,
        proxy_url,
        subject,
    };
    state.tokens.write().await.insert(token.clone(), info);
    Ok((port, token))
}

/// Start the loopback-only HTTP server.
pub async fn start_internal_bridge() -> Result<u16, String> {
    let state = bridge_state().clone();
    if let port @ 1..=u16::MAX = state.effective_port.load(Ordering::Acquire) {
        return Ok(port);
    }

    let router = Router::new()
        .route("/internal/work/execute", post(internal_work_execute))
        .route("/internal/work/tool_pipeline", post(internal_tool_pipeline))
        .route(
            "/internal/work/task_state/update",
            post(internal_task_state_update),
        )
        .route("/internal/work/task_state", get(internal_task_state_get))
        .route("/internal/work/files/list", post(internal_files_list))
        .route("/internal/work/mcp/:server_name", post(internal_mcp_proxy))
        .route("/internal/work/inputs/claim", post(internal_claim_input))
        .route("/internal/work/inputs/queue", post(internal_queue_input))
        .route(
            "/internal/work/inbox_status/:item_id",
            get(internal_inbox_status),
        )
        .route("/internal/work/apps/list", get(internal_apps_list))
        .route("/internal/work/apps/call", post(internal_apps_call))
        .route("/internal/work/web/search", post(internal_web_search))
        .route("/internal/work/web/fetch", post(internal_web_fetch))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind internal Work bridge on 127.0.0.1:0: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local address for internal Work bridge: {e}"))?
        .port();

    state.effective_port.store(port, Ordering::Release);
    state.ready.notify_waiters();
    log::info!("[work/bridge] Internal authenticated bridge listening on 127.0.0.1:{port}");

    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router).await {
            log::error!("[work/bridge] Internal bridge server error: {err}");
        }
    });

    Ok(port)
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}

async fn authenticate_token(
    state: &InternalBridgeState,
    headers: &HeaderMap,
) -> Result<ProcessBridgeTokenInfo, (StatusCode, Json<serde_json::Value>)> {
    let token = extract_bearer_token(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Missing or invalid Bearer token"})),
        )
    })?;

    let tokens = state.tokens.read().await;
    tokens.get(&token).cloned().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Forbidden: invalid session token"})),
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalExecutePayload {
    pub resource_id: String,
    pub action: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub input_paths: Vec<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

fn error_result(status: &str, message: impl Into<String>) -> Json<serde_json::Value> {
    Json(json!({
        "success": false,
        "status": status,
        "failureKind": serde_json::Value::Null,
        "exitCode": -1,
        "stdout": "",
        "stderr": message.into(),
    }))
}

const MCP_PROXY_WAIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

fn validate_mcp_server_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 80
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err("MCP server name is invalid".into());
    }
    Ok(name)
}

fn validate_mcp_tool_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 || name.chars().any(char::is_control) {
        return Err("MCP tool name is invalid".into());
    }
    Ok(name)
}

fn stable_mcp_tool_call_id(
    run_id: &str,
    server_name: &str,
    method: &str,
    request_id: &serde_json::Value,
    params: &serde_json::Value,
) -> String {
    let mut hasher = Sha256::new();
    let value = serde_json::json!({
        "runId": run_id,
        "server": server_name,
        "method": method,
        "requestId": request_id,
        "params": params,
    });
    hasher.update(serde_json::to_vec(&value).unwrap_or_default());
    format!("mcp-{:x}", hasher.finalize())
}

fn bounded_mcp_message(message: &str) -> String {
    let mut bounded = message.chars().take(2_000).collect::<String>();
    if message.chars().count() > 2_000 {
        bounded.push('…');
    }
    bounded
}

fn mcp_rpc_result(id: serde_json::Value, result: serde_json::Value) -> Json<serde_json::Value> {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    }))
}

fn mcp_rpc_error(
    id: serde_json::Value,
    code: i64,
    message: impl Into<String>,
) -> Json<serde_json::Value> {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": bounded_mcp_message(&message.into()),
        },
    }))
}

fn mcp_tool_result_message(result: &crate::work::pipeline::ToolResult) -> String {
    if result.stderr.trim().is_empty() {
        format!("MCP Host execution ended with status '{}'.", result.status)
    } else {
        bounded_mcp_message(&result.stderr)
    }
}

fn mcp_json_result(stdout: &str, context: &str) -> Result<serde_json::Value, String> {
    let value = serde_json::from_str::<serde_json::Value>(stdout)
        .map_err(|error| format!("MCP {context} returned invalid JSON: {error}"))?;
    if !value.is_object() {
        return Err(format!("MCP {context} result must be a JSON object"));
    }
    Ok(value)
}

async fn execute_mcp_pipeline(
    paths: &WorkPaths,
    ctx: &TrustedWorkContext,
    intent: &crate::work::pipeline::ToolIntent,
    policy: &crate::work::models::WorkPolicy,
) -> Result<crate::work::pipeline::ToolResult, String> {
    let pipeline = crate::work::pipeline::ToolPipeline::new(paths.clone());
    let interaction_mgr = crate::work::interaction::InteractionManager::new(paths.clone());
    let mut result = pipeline
        .execute_intent_with_proxy(intent, policy, ctx.token_info.proxy_url.as_deref())
        .await?;
    let mut resume_count = 0u8;

    while matches!(result.status.as_str(), "waiting_approval" | "waiting_input") {
        let interaction_id = result
            .interaction_id
            .clone()
            .ok_or_else(|| "MCP Host execution is waiting without an interaction id".to_string())?;
        let deadline = Instant::now() + MCP_PROXY_WAIT_TIMEOUT;
        loop {
            let interaction = interaction_mgr.get_interaction(&interaction_id)?;
            match interaction.state {
                crate::work::models::PendingInteractionState::Resolved => break,
                crate::work::models::PendingInteractionState::Cancelled => {
                    result.success = false;
                    result.status = "denied".into();
                    result.exit_code = Some(1);
                    result.stderr = "MCP tool call was denied in Work Inbox.".into();
                    return Ok(result);
                }
                crate::work::models::PendingInteractionState::Pending
                | crate::work::models::PendingInteractionState::Delivering => {
                    if Instant::now() >= deadline {
                        result.success = false;
                        result.status = "timeout".into();
                        result.exit_code = Some(1);
                        result.stderr =
                            "Timed out while waiting for the Work Inbox decision for this MCP call."
                                .into();
                        return Ok(result);
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }

        // The pipeline owns replay protection. A resolved approval is the
        // only state that allows this same intent to be resumed; if another
        // request already completed it while this handler was waiting, the
        // pipeline returns the persisted terminal result instead of replaying.
        resume_count = resume_count.saturating_add(1);
        if resume_count > 2 {
            result.success = false;
            result.status = "failed".into();
            result.exit_code = Some(1);
            result.stderr = "MCP approval did not produce a terminal execution result.".into();
            return Ok(result);
        }
        result = pipeline
            .execute_intent_with_proxy(intent, policy, ctx.token_info.proxy_url.as_deref())
            .await?;
    }

    Ok(result)
}

async fn internal_mcp_proxy(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Path(server_name): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let server_name = match validate_mcp_server_name(&server_name) {
        Ok(name) => name,
        Err(error) => return Ok(mcp_rpc_error(serde_json::Value::Null, -32602, error)),
    };
    let Some(request_object) = request.as_object() else {
        return Ok(mcp_rpc_error(
            serde_json::Value::Null,
            -32600,
            "MCP request must be a JSON object",
        ));
    };
    let id = request_object
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = request_object
        .get("method")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let params = request_object
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match method {
        "initialize" => Ok(mcp_rpc_result(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "AgentCabin Work Host MCP bridge", "version": "1.0.0" },
            }),
        )),
        "notifications/initialized" => Ok(mcp_rpc_result(id, json!({}))),
        "tools/list" => {
            let (task_id, policy) = resolve_policy_for_context(&ctx);
            let intent = crate::work::pipeline::ToolIntent {
                task_id,
                work_run_id: ctx.token_info.run_id.clone(),
                session_id: None,
                workspace_id: ctx.token_info.workspace_id.clone(),
                tool_call_id: stable_mcp_tool_call_id(
                    &ctx.token_info.run_id,
                    server_name,
                    method,
                    &id,
                    &params,
                ),
                tool_name: "work_mcp_list".into(),
                action: "list".into(),
                arguments: json!({
                    "server": server_name,
                    "target": format!("mcp://{server_name}"),
                }),
                input_paths: Vec::new(),
                expected_outputs: Vec::new(),
                execution_context: ctx.token_info.execution_context,
                policy_revision: None,
            };
            let paths = WorkPaths::app();
            match execute_mcp_pipeline(&paths, &ctx, &intent, &policy).await {
                Ok(result) if result.success => match mcp_json_result(&result.stdout, "tools/list")
                {
                    Ok(value) => Ok(mcp_rpc_result(id, value)),
                    Err(error) => Ok(mcp_rpc_error(id, -32000, error)),
                },
                Ok(result) => Ok(mcp_rpc_error(id, -32000, mcp_tool_result_message(&result))),
                Err(error) => Ok(mcp_rpc_error(id, -32000, error)),
            }
        }
        "tools/call" => {
            let Some(params_object) = params.as_object() else {
                return Ok(mcp_rpc_error(
                    id,
                    -32602,
                    "MCP tools/call params must be an object",
                ));
            };
            let tool_name = match params_object
                .get("name")
                .and_then(|value| value.as_str())
                .and_then(|value| validate_mcp_tool_name(value).ok())
            {
                Some(name) => name,
                None => {
                    return Ok(mcp_rpc_error(
                        id,
                        -32602,
                        "MCP tools/call requires a valid name",
                    ))
                }
            };
            let arguments = params_object
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                return Ok(mcp_rpc_error(
                    id,
                    -32602,
                    "MCP tools/call arguments must be an object",
                ));
            }

            let (task_id, policy) = resolve_policy_for_context(&ctx);
            let intent = crate::work::pipeline::ToolIntent {
                task_id,
                work_run_id: ctx.token_info.run_id.clone(),
                session_id: None,
                workspace_id: ctx.token_info.workspace_id.clone(),
                tool_call_id: stable_mcp_tool_call_id(
                    &ctx.token_info.run_id,
                    server_name,
                    method,
                    &id,
                    &params,
                ),
                tool_name: "work_mcp_call".into(),
                action: tool_name.into(),
                arguments: json!({
                    "server": server_name,
                    "tool_name": tool_name,
                    "arguments": arguments,
                    "target": format!("mcp://{server_name}/{tool_name}"),
                }),
                input_paths: Vec::new(),
                expected_outputs: Vec::new(),
                execution_context: ctx.token_info.execution_context,
                policy_revision: None,
            };
            let paths = WorkPaths::app();
            match execute_mcp_pipeline(&paths, &ctx, &intent, &policy).await {
                Ok(result) if result.success => match mcp_json_result(&result.stdout, "tools/call")
                {
                    Ok(value) => Ok(mcp_rpc_result(id, value)),
                    Err(error) => Ok(mcp_rpc_error(id, -32000, error)),
                },
                Ok(result) => {
                    // An MCP server-level tool error is a valid tools/call
                    // result and must remain visible as `isError`, while a
                    // Host/policy/transport failure is a JSON-RPC error.
                    match serde_json::from_str::<serde_json::Value>(&result.stdout) {
                        Ok(mut value) if value.get("content").is_some() => {
                            if let Some(object) = value.as_object_mut() {
                                object.insert("isError".into(), serde_json::Value::Bool(true));
                            }
                            Ok(mcp_rpc_result(id, value))
                        }
                        _ => Ok(mcp_rpc_error(id, -32000, mcp_tool_result_message(&result))),
                    }
                }
                Err(error) => Ok(mcp_rpc_error(id, -32000, error)),
            }
        }
        _ => Ok(mcp_rpc_error(
            id,
            -32601,
            format!("MCP method '{method}' is not supported"),
        )),
    }
}

pub async fn revoke_session_token(token: &str) {
    let state = bridge_state();
    let removed = state.tokens.write().await.remove(token);
    if let Some(info) = removed {
        let has_active = state
            .tokens
            .read()
            .await
            .values()
            .any(|candidate| candidate.run_id == info.run_id);
        if !has_active {
            crate::work::desktop_operator::desktop_operator_manager()
                .release_for_run(&info.run_id)
                .await;
        }
    }
}

pub async fn revoke_run_tokens(run_id: &str) {
    let state = bridge_state();
    state
        .tokens
        .write()
        .await
        .retain(|_, info| info.run_id != run_id);
    crate::work::coordinator::coordinator().cleanup_run(run_id);
    crate::work::desktop_operator::desktop_operator_manager()
        .release_for_run(run_id)
        .await;
}

pub fn revoke_run_tokens_sync(run_id: &str) {
    let state = bridge_state();
    if let Ok(mut tokens) = state.tokens.try_write() {
        tokens.retain(|_, info| info.run_id != run_id);
    } else {
        let r_id = run_id.to_string();
        tokio::spawn(async move {
            revoke_run_tokens(&r_id).await;
        });
    }
    crate::work::coordinator::coordinator().cleanup_run(run_id);
    let r_id = run_id.to_string();
    tokio::spawn(async move {
        crate::work::desktop_operator::desktop_operator_manager()
            .release_for_run(&r_id)
            .await;
    });
}

pub async fn is_run_active(run_id: &str) -> bool {
    let state = bridge_state();
    state
        .tokens
        .read()
        .await
        .values()
        .any(|info| info.run_id == run_id)
}

#[derive(Debug, Clone)]
pub struct TrustedWorkContext {
    pub token_info: ProcessBridgeTokenInfo,
    pub task: Option<crate::work::models::WorkTask>,
    pub work_run: Option<crate::work::models::WorkRun>,
    pub workspace: Option<crate::work::models::WorkWorkspace>,
}

pub async fn authenticate_work_context(
    state: &InternalBridgeState,
    headers: &HeaderMap,
) -> Result<TrustedWorkContext, (StatusCode, Json<serde_json::Value>)> {
    let token_info = authenticate_token(state, headers).await?;
    trusted_work_context_for_token(token_info, &crate::work::paths::WorkPaths::app())
}

fn lookup_context_error(resource: &str, error: String) -> (StatusCode, Json<serde_json::Value>) {
    let missing = error.to_ascii_lowercase().contains("not found");
    let status = if missing {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    log::warn!("[work/bridge] Failed to verify authenticated {resource}: {error}");
    let message = if missing {
        format!("Authenticated {resource} is unavailable")
    } else {
        format!("Unable to verify authenticated {resource}")
    };
    (status, Json(json!({ "error": message })))
}

fn trusted_work_context_for_token(
    token_info: ProcessBridgeTokenInfo,
    paths: &crate::work::paths::WorkPaths,
) -> Result<TrustedWorkContext, (StatusCode, Json<serde_json::Value>)> {
    // Standalone Work uses an empty workspace id as its canonical authority
    // marker. Its task_id carries the run/ledger namespace so lifecycle facts
    // survive restarts, but it is not a persisted WorkTask id and must never
    // be resolved through TaskManager. Without this check, a standalone run
    // id is incorrectly looked up as a Task and every subsequent bridge call
    // fails with "WorkTask unavailable: Task not found".
    if token_info.workspace_id.trim().is_empty() {
        return Ok(TrustedWorkContext {
            token_info,
            task: None,
            work_run: None,
            workspace: None,
        });
    }

    let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
        .get(&token_info.workspace_id)
        .map_err(|error| lookup_context_error("Workspace", error))?;

    let task_id = match token_info.task_id.as_deref() {
        None => {
            return Ok(TrustedWorkContext {
                token_info,
                task: None,
                work_run: None,
                workspace: Some(workspace),
            });
        }
        Some(task_id) if !task_id.trim().is_empty() => task_id,
        Some(_) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Authenticated WorkTask ID is invalid" })),
            ));
        }
    };

    let task_manager = crate::work::tasks::TaskManager::new(paths.clone());
    let task = match task_manager.get_task(task_id) {
        Ok(task) => task,
        Err(error) => return Err(lookup_context_error("WorkTask", error)),
    };

    if task.workspace_id != token_info.workspace_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Authenticated Work session does not belong to this Workspace"
            })),
        ));
    }

    let work_run = match task_manager.get_run(task_id, &token_info.run_id) {
        Ok(run) => {
            if !run.status.is_active() {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "This WorkRun is no longer active"})),
                ));
            }
            Some(run)
        }
        Err(error) => return Err(lookup_context_error("WorkRun", error)),
    };

    Ok(TrustedWorkContext {
        token_info,
        task: Some(task),
        work_run,
        workspace: Some(workspace),
    })
}

fn resolve_policy_for_context(
    ctx: &TrustedWorkContext,
) -> (String, crate::work::models::WorkPolicy) {
    if let Some(task) = ctx.task.as_ref() {
        (task.id.clone(), task.policy.clone())
    } else if let Some(workspace) = ctx.workspace.as_ref() {
        (
            ctx.token_info.run_id.clone(),
            workspace.default_policy.clone(),
        )
    } else {
        let task_id = ctx.token_info.run_id.clone();
        let execution_mode = crate::storage::runs::get_run(&task_id)
            .and_then(|m| m.permission_mode)
            .and_then(|mode| crate::work::models::WorkExecutionMode::from_permission_mode(&mode))
            .unwrap_or(crate::work::models::WorkExecutionMode::Auto);

        // Standalone Work has no Workspace policy to inherit. Preserve its
        // established connector default and run-scoped standing rules while
        // parsing the same canonical execution mode used by workspace runs.
        let paths = crate::work::paths::WorkPaths::app();
        let standing_rules = paths
            .standalone_task_dir(&task_id)
            .ok()
            .map(|dir| dir.join("standing_rules.json"))
            .filter(|path| path.exists())
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();

        let policy = crate::work::models::WorkPolicy {
            execution_mode,
            allow_external_connectors: true,
            standing_rules,
            ..crate::work::models::WorkPolicy::default()
        };
        (task_id, policy)
    }
}

async fn internal_work_execute(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalExecutePayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    if payload.resource_id.trim().is_empty() || payload.action.trim().is_empty() {
        return Ok(error_result(
            "denied",
            "resourceId and action are required for Work execution",
        ));
    }

    let mut args = payload.arguments;
    if !args.is_object() {
        args = serde_json::json!({});
    }
    if let Some(obj) = args.as_object_mut() {
        obj.insert(
            "resource_id".to_string(),
            serde_json::json!(payload.resource_id),
        );
        obj.insert("action".to_string(), serde_json::json!(payload.action));
    }

    let (task_id, policy) = resolve_policy_for_context(&ctx);

    let intent = crate::work::pipeline::ToolIntent {
        task_id,
        work_run_id: ctx.token_info.run_id.clone(),
        session_id: None,
        workspace_id: ctx.token_info.workspace_id.clone(),
        tool_call_id: payload
            .tool_use_id
            .clone()
            .unwrap_or_else(|| format!("tool-{}", uuid::Uuid::new_v4())),
        tool_name: "work_execute".to_string(),
        action: payload.action,
        arguments: args,
        input_paths: payload.input_paths,
        expected_outputs: payload.expected_outputs,
        execution_context: ctx.token_info.execution_context,
        policy_revision: None,
    };

    let pipeline = crate::work::pipeline::ToolPipeline::new(crate::work::paths::WorkPaths::app());
    match pipeline
        .execute_intent_with_proxy(&intent, &policy, ctx.token_info.proxy_url.as_deref())
        .await
    {
        Ok(result) => Ok(Json(json!({
            "success": result.success,
            "status": result.status,
            "failureKind": result.failure_kind,
            "exitCode": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "outputs": result.outputs,
            "inboxItemId": result.interaction_id,
            "startedAt": result.started_at,
            "finishedAt": result.finished_at,
        }))),
        Err(error) => Ok(error_result("failed", error)),
    }
}

async fn internal_inbox_status(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let inbox = crate::work::inbox::InboxManager::new(crate::work::paths::WorkPaths::app());
    let item = inbox
        .get_item(&item_id)
        .map_err(|error| (StatusCode::NOT_FOUND, Json(json!({"error": error}))))?;
    let (expected_task_id, _) = resolve_policy_for_context(&ctx);
    if item.workspace_id != ctx.token_info.workspace_id
        || item.run_id != ctx.token_info.run_id
        || item.task_id != expected_task_id
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Inbox item does not belong to this Work session"})),
        ));
    }

    Ok(Json(json!({
        "status": item.status,
        "response": item.response,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalToolPipelinePayload {
    pub tool_call_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub input_paths: Vec<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
}

async fn internal_tool_pipeline(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalToolPipelinePayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let (task_id, policy) = resolve_policy_for_context(&ctx);

    let intent = crate::work::pipeline::ToolIntent {
        task_id,
        work_run_id: ctx.token_info.run_id.clone(),
        session_id: None,
        workspace_id: ctx.token_info.workspace_id.clone(),
        tool_call_id: payload.tool_call_id,
        tool_name: payload.tool_name,
        action: payload.action,
        arguments: payload.arguments,
        input_paths: payload.input_paths,
        expected_outputs: payload.expected_outputs,
        execution_context: ctx.token_info.execution_context,
        policy_revision: None,
    };

    let pipeline = crate::work::pipeline::ToolPipeline::new(crate::work::paths::WorkPaths::app());
    match pipeline
        .execute_intent_with_proxy(&intent, &policy, ctx.token_info.proxy_url.as_deref())
        .await
    {
        Ok(result) => Ok(Json(serde_json::to_value(result).unwrap_or_default())),
        Err(err) => Ok(error_result("failed", err)),
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InternalTaskStateUpdatePayload {
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub plan: Option<Vec<crate::models::StructuredTask>>,
    #[serde(default)]
    pub step: Option<TaskStateStepUpdate>,
    #[serde(default)]
    pub checkpoint: Option<crate::work::models::WorkTaskCheckpoint>,
    #[serde(default)]
    pub pending_approval: Option<Option<crate::work::models::WorkPendingApproval>>,
    #[serde(default)]
    pub state: Option<crate::work::models::WorkTaskState>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStateStepUpdate {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub text: Option<String>,
}

async fn internal_task_state_update(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalTaskStateUpdatePayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;

    let run_id = &ctx.token_info.run_id;
    let _state_lock = crate::work::task_state::lock(run_id).await;
    let mut task_state = crate::work::task_state::load(run_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?
        .unwrap_or_default();

    if let Some(expected_rev) = payload.expected_revision {
        if task_state.revision != expected_rev {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": format!(
                        "Task state revision conflict: expected {}, current {}",
                        expected_rev, task_state.revision
                    ),
                    "currentRevision": task_state.revision,
                })),
            ));
        }
    }

    if let Some(explicit_state) = payload.state {
        task_state = explicit_state;
    } else {
        if let Some(goal) = payload.goal {
            let trimmed = goal.trim();
            if trimmed.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Work goal cannot be empty."})),
                ));
            }
            task_state.goal = Some(trimmed.to_string());
        }

        if let Some(plan) = payload.plan {
            if let Some(cp) = task_state.checkpoint.as_mut() {
                if let Some(current_id) = &cp.current_step_id {
                    if !plan.iter().any(|s| &s.id == current_id) {
                        cp.current_step_id = None;
                    }
                }
            }
            task_state.pending_approval = None;
            task_state.plan = plan;
        }

        if let Some(step_update) = payload.step {
            let step_id = step_update.id.trim();
            let target = task_state
                .plan
                .iter_mut()
                .find(|s| s.id == step_id)
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(json!({"error": format!("Work plan step not found: {step_id}")})),
                    )
                })?;

            if let Some(text) = step_update.text {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Work plan step text cannot be empty."})),
                    ));
                }
                target.text = trimmed.to_string();
            }

            let status_str = step_update.status.trim();
            let status = match status_str {
                "pending" => crate::models::StructuredTaskStatus::Pending,
                "in_progress" => crate::models::StructuredTaskStatus::InProgress,
                "completed" => crate::models::StructuredTaskStatus::Completed,
                other => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": format!("Unsupported Work plan status: {other}")})),
                    ));
                }
            };

            if status == crate::models::StructuredTaskStatus::InProgress {
                for s in task_state.plan.iter_mut() {
                    if s.id != step_id
                        && s.status == crate::models::StructuredTaskStatus::InProgress
                    {
                        s.status = crate::models::StructuredTaskStatus::Pending;
                    }
                }
            }

            let target = task_state
                .plan
                .iter_mut()
                .find(|s| s.id == step_id)
                .unwrap();
            target.status = status;
        }

        if let Some(checkpoint) = payload.checkpoint {
            if let Some(step_id) = &checkpoint.current_step_id {
                if !task_state.plan.iter().any(|s| &s.id == step_id) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!({"error": format!("Work checkpoint step not found: {step_id}")}),
                        ),
                    ));
                }
            }
            task_state.checkpoint = Some(checkpoint);
        }

        if let Some(pending_approval) = payload.pending_approval {
            task_state.pending_approval = pending_approval;
        }
    }

    task_state.revision += 1;
    task_state.updated_at = crate::models::now_iso();
    crate::work::task_state::save(run_id, &task_state)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    if let Some(emitter) = crate::web_server::broadcaster::shared_emitter() {
        let paths = WorkPaths::app();
        if let Ok(projection) = crate::work::projection::project_work_projection(&paths, run_id) {
            emitter.persist_and_emit(
                run_id,
                &crate::models::BusEvent::WorkProjectionChanged {
                    run_id: run_id.to_string(),
                    projection,
                },
            );
        }
    }

    Ok(Json(json!({
        "ok": true,
        "workTaskState": task_state,
        "work_task_state": task_state,
    })))
}

async fn internal_task_state_get(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let run_id = &ctx.token_info.run_id;
    let task_state = crate::work::task_state::load(run_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?
        .unwrap_or_default();

    Ok(Json(json!({
        "ok": true,
        "workTaskState": task_state,
        "work_task_state": task_state,
    })))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct InternalFilesListPayload {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    area: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    max_entries: Option<usize>,
}

/// Host-owned file listing for runtimes that do not execute the Pi Work
/// extension. The runtime receives only logical paths; resolution and the
/// Workspace boundary stay in the Work Host.
async fn internal_files_list(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalFilesListPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;

    if payload.path.is_some() && payload.area.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Use either path or area, not both." })),
        ));
    }

    let max_entries = payload.max_entries.unwrap_or(100).clamp(1, 500);
    let paths = WorkPaths::app();
    let workspace_id = ctx.token_info.workspace_id.trim();
    let raw_path = match (
        payload.path.as_deref(),
        payload.area.as_deref(),
        payload.prefix.as_deref(),
    ) {
        (Some(path), _, _) => Some(path.to_string()),
        (None, Some(area), Some(prefix)) => Some(format!("{area}/{prefix}")),
        (None, Some(area), None) => Some(area.to_string()),
        (None, None, Some(prefix)) => Some(prefix.to_string()),
        (None, None, None) => None,
    }
    .map(|value| value.trim().trim_matches('/').replace('\\', "/"))
    .filter(|value| !value.is_empty());

    if workspace_id.is_empty() {
        let run_root = paths
            .standalone_task_dir(&ctx.token_info.run_id)
            .map_err(|error| (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))))?;
        let target = match raw_path {
            Some(ref raw) => paths
                .resolve_standalone_path(&ctx.token_info.run_id, FsPath::new(&raw), false)
                .map_err(|error| (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))))?,
            None => run_root.clone(),
        };
        let metadata = std::fs::metadata(&target).map_err(|error| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Cannot inspect Work path: {error}") })),
            )
        })?;
        if !metadata.is_dir() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "The requested Work path is not a directory." })),
            ));
        }
        let mut files = Vec::new();
        collect_bridge_file_paths(&target, &run_root, &mut files, max_entries)?;
        return Ok(Json(json!({
            "ok": true,
            "root": raw_path.as_deref().unwrap_or("."),
            "files": files,
            "truncated": files.len() >= max_entries,
        })));
    }

    let area = payload
        .area
        .as_deref()
        .or_else(|| {
            payload
                .path
                .as_deref()
                .and_then(|path| path.split('/').next())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(area) = area {
        if !matches!(area, "input" | "scratch" | "output" | "context") {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Unknown Work Workspace area." })),
            ));
        }
    }

    let mut files = crate::work::files::list_with_paths(&paths, workspace_id, area)
        .map_err(|error| (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))))?;
    let requested_prefix = payload
        .path
        .as_deref()
        .map(str::to_string)
        .or_else(|| {
            payload.prefix.as_deref().map(|prefix| match area {
                Some(area) => format!("{area}/{prefix}"),
                None => prefix.to_string(),
            })
        })
        .as_deref()
        .map(|value| value.trim().trim_matches('/').replace('\\', "/"))
        .filter(|value| !value.is_empty());
    if let Some(prefix) = requested_prefix.as_deref() {
        files.retain(|file| file.path == prefix || file.path.starts_with(&format!("{prefix}/")));
    }
    files.truncate(max_entries);
    let file_paths = files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "ok": true,
        "root": requested_prefix.unwrap_or_else(|| area.unwrap_or(".").to_string()),
        "files": file_paths,
        "truncated": files.len() >= max_entries,
    })))
}

fn collect_bridge_file_paths(
    directory: &FsPath,
    root: &FsPath,
    files: &mut Vec<String>,
    max_entries: usize,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if files.len() >= max_entries {
        return Ok(());
    }
    let entries = std::fs::read_dir(directory).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Cannot list Work directory: {error}") })),
        )
    })?;
    for entry in entries {
        if files.len() >= max_entries {
            break;
        }
        let entry = entry.map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Cannot inspect Work directory entry: {error}") })),
            )
        })?;
        let path: PathBuf = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Cannot inspect Work file type: {error}") })),
            )
        })?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_bridge_file_paths(&path, root, files, max_entries)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalClaimInputPayload {
    pub kind: crate::work::models::RuntimeInputKind,
}

async fn internal_claim_input(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalClaimInputPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let task_id = ctx
        .task
        .as_ref()
        .map(|t| t.id.as_str())
        .unwrap_or(&ctx.token_info.run_id);
    let input_mgr = crate::work::input::RuntimeInputManager::open(
        &crate::work::paths::WorkPaths::app(),
        task_id,
        &ctx.token_info.run_id,
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    let item = input_mgr
        .claim_next(payload.kind)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    Ok(Json(json!({
        "item": item
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalQueueInputPayload {
    pub kind: crate::work::models::RuntimeInputKind,
    pub content: String,
}

async fn internal_queue_input(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalQueueInputPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let task_id = ctx
        .task
        .as_ref()
        .map(|t| t.id.as_str())
        .unwrap_or(&ctx.token_info.run_id);
    let input_mgr = crate::work::input::RuntimeInputManager::open(
        &crate::work::paths::WorkPaths::app(),
        task_id,
        &ctx.token_info.run_id,
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    let item = input_mgr
        .queue_input(
            task_id,
            &ctx.token_info.run_id,
            payload.kind,
            &payload.content,
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;

    Ok(Json(json!({
        "item": item
    })))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalAppCallPayload {
    pub app_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    pub account_id: Option<String>,
    pub tool_use_id: Option<String>,
}

async fn internal_apps_list(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let paths = crate::work::paths::WorkPaths::app();
    internal_apps_list_with_paths(state, headers, &paths).await
}

pub(crate) async fn internal_apps_list_with_paths(
    state: InternalBridgeState,
    headers: HeaderMap,
    paths: &crate::work::paths::WorkPaths,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;

    let connections = crate::work::apps::storage::list_connections(paths).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to read app connections: {error}") })),
        )
    })?;
    let provider = crate::work::apps::provider::resolve_provider(paths);
    let catalog = provider.catalog().await.map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("Failed to load app catalog: {error}") })),
        )
    })?;
    let tools = provider.list_tools(None).await.map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("Failed to load app tools: {error}") })),
        )
    })?;

    let items: Vec<serde_json::Value> = catalog
        .into_iter()
        .map(|item| {
            let conn = connections
                .iter()
                .find(|c| c.app_id.eq_ignore_ascii_case(&item.app_id));
            let default_account = crate::work::apps::storage::get_workspace_default_account(
                paths,
                &ctx.token_info.workspace_id,
                &item.app_id,
            )
            .unwrap_or(None);
            json!({
                "appId": item.app_id,
                "displayName": item.display_name,
                "description": item.description,
                "categories": item.categories,
                "capabilities": item.capabilities,
                "connected": conn.map(|c| c.status == crate::work::apps::models::ConnectionStatus::Connected).unwrap_or(false),
                "status": conn.map(|c| format!("{:?}", c.status).to_lowercase()).unwrap_or_else(|| "disconnected".into()),
                "accounts": conn.map(|c| c.accounts.clone()).unwrap_or_default(),
                "defaultAccountId": default_account,
            })
        })
        .collect();

    Ok(Json(json!({ "apps": items, "tools": tools })))
}

async fn internal_apps_call(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalAppCallPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let paths = crate::work::paths::WorkPaths::app();
    internal_apps_call_with_paths(state, headers, payload, &paths).await
}

pub(crate) async fn internal_apps_call_with_paths(
    state: InternalBridgeState,
    headers: HeaderMap,
    payload: InternalAppCallPayload,
    paths: &crate::work::paths::WorkPaths,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let (task_id, policy) = resolve_policy_for_context(&ctx);
    let norm_app_id = payload.app_id.trim().to_lowercase();
    let conn = crate::work::apps::storage::get_connection(paths, &norm_app_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to read app connection: {e}") })),
        )
    })?;

    let is_connected = match &conn {
        Some(c) => {
            c.status == crate::work::apps::models::ConnectionStatus::Connected
                && !c.accounts.is_empty()
        }
        None => false,
    };

    if !is_connected {
        // Connection initiation, durable Inbox creation, and the waiting
        // transition are all owned by ToolPipeline. The bridge only adapts
        // the authoritative ToolResult back to the Pi wire contract.
        let intent = crate::work::pipeline::ToolIntent {
            task_id,
            work_run_id: ctx.token_info.run_id.clone(),
            session_id: None,
            workspace_id: ctx.token_info.workspace_id.clone(),
            tool_call_id: payload
                .tool_use_id
                .unwrap_or_else(|| format!("bridge-app-{}", Uuid::new_v4())),
            tool_name: "work_call_app".to_string(),
            action: payload.tool_name.clone(),
            arguments: json!({
                "app_id": norm_app_id,
                "tool_name": payload.tool_name,
                "arguments": payload.arguments,
            }),
            input_paths: Vec::new(),
            expected_outputs: Vec::new(),
            execution_context: ctx.token_info.execution_context,
            policy_revision: None,
        };
        let pipeline = crate::work::pipeline::ToolPipeline::new(paths.clone());
        return map_app_pipeline_result(
            &norm_app_id,
            &payload.tool_name,
            pipeline
                .execute_intent_with_proxy(&intent, &policy, ctx.token_info.proxy_url.as_deref())
                .await,
        );
    }

    let conn = conn.unwrap();

    // Resolve account
    let workspace_default_account = crate::work::apps::storage::get_workspace_default_account(
        paths,
        &ctx.token_info.workspace_id,
        &norm_app_id,
    )
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to read default app account: {error}") })),
        )
    })?;
    let selected_account_id = if let Some(acc) = payload.account_id {
        Some(acc)
    } else if let Some(def) = workspace_default_account {
        Some(def)
    } else if conn.accounts.len() == 1 {
        Some(conn.accounts[0].account_id.clone())
    } else if conn.accounts.is_empty() {
        return Ok(Json(json!({
            "status": "needs_connection",
            "appId": norm_app_id,
            "message": format!("App '{}' has no connected accounts.", norm_app_id)
        })));
    } else {
        return Ok(Json(json!({
            "status": "needs_account_selection",
            "appId": norm_app_id,
            "accounts": conn.accounts,
            "message": format!(
                "App '{}' has multiple connected accounts. Please specify account_id.",
                norm_app_id
            )
        })));
    };

    log::info!(
        "[work/bridge] Routing app tool '{}/{}' for account '{:?}' through Work ToolPipeline on run '{}'",
        norm_app_id,
        payload.tool_name,
        selected_account_id,
        ctx.token_info.run_id
    );

    let intent = crate::work::pipeline::ToolIntent {
        task_id,
        work_run_id: ctx.token_info.run_id.clone(),
        session_id: None,
        workspace_id: ctx.token_info.workspace_id.clone(),
        tool_call_id: payload
            .tool_use_id
            .unwrap_or_else(|| format!("bridge-app-{}", Uuid::new_v4())),
        tool_name: "work_call_app".to_string(),
        action: payload.tool_name.clone(),
        arguments: json!({
            "app_id": norm_app_id,
            "tool_name": payload.tool_name,
            "arguments": payload.arguments,
            "account_id": selected_account_id,
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: ctx.token_info.execution_context,
        policy_revision: None,
    };
    let pipeline = crate::work::pipeline::ToolPipeline::new(paths.clone());
    map_app_pipeline_result(
        &norm_app_id,
        &intent.action,
        pipeline
            .execute_intent_with_proxy(&intent, &policy, ctx.token_info.proxy_url.as_deref())
            .await,
    )
}

fn map_app_pipeline_result(
    app_id: &str,
    tool_name: &str,
    result: Result<crate::work::pipeline::ToolResult, String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match result {
        Ok(result) if result.success => {
            let result_data =
                serde_json::from_str::<serde_json::Value>(&result.stdout).map_err(|error| {
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({
                            "error": format!("Invalid connected app result: {error}"),
                            "appId": app_id,
                            "tool": tool_name,
                        })),
                    )
                })?;
            Ok(Json(json!({
                "status": "success",
                "result": result_data
            })))
        }
        Ok(result) if result.status == "waiting_input" => {
            let metadata = serde_json::from_str::<serde_json::Value>(&result.stdout)
                .unwrap_or_else(|_| json!({}));
            Ok(Json(json!({
                "status": "needs_connection",
                "appId": metadata.get("appId").and_then(|value| value.as_str()).unwrap_or(app_id),
                "connectionId": metadata.get("connectionId"),
                "inboxItemId": result.interaction_id,
                "message": format!("App '{}' is not connected. An authorization request has been added to Inbox.", app_id)
            })))
        }
        Ok(result) if result.status == "denied" => Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": result.stderr,
                "appId": app_id,
                "tool": tool_name,
            })),
        )),
        Ok(result) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": if result.stderr.is_empty() { result.status } else { result.stderr },
                "appId": app_id,
                "tool": tool_name,
            })),
        )),
        Err(error) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": error,
                "appId": app_id,
                "tool": tool_name,
            })),
        )),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalWebSearchPayload {
    pub query: String,
    #[serde(default)]
    pub max_results: Option<u32>,
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

async fn internal_web_search(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalWebSearchPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let paths = WorkPaths::app();
    let (task_id, policy) = resolve_policy_for_context(&ctx);
    let tool_call_id = payload
        .tool_use_id
        .unwrap_or_else(|| format!("bridge-web-search-{}", Uuid::new_v4()));
    let intent = crate::work::pipeline::ToolIntent {
        task_id,
        work_run_id: ctx.token_info.run_id.clone(),
        session_id: None,
        workspace_id: ctx.token_info.workspace_id.clone(),
        tool_call_id,
        tool_name: "web_search".to_string(),
        action: "search".to_string(),
        arguments: json!({
            "query": payload.query,
            "max_results": payload.max_results,
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: ctx.token_info.execution_context,
        policy_revision: None,
    };
    let pipeline = crate::work::pipeline::ToolPipeline::new(paths);
    match pipeline
        .execute_intent_with_proxy(&intent, &policy, ctx.token_info.proxy_url.as_deref())
        .await
    {
        Ok(result) if result.success => {
            let results =
                serde_json::from_str::<serde_json::Value>(&result.stdout).map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({ "error": format!("Invalid Web search result: {e}") })),
                    )
                })?;
            Ok(Json(json!({ "ok": true, "results": results })))
        }
        Ok(result) => Ok(Json(json!({
            "ok": false,
            "error": if result.stderr.is_empty() { result.status } else { result.stderr },
        }))),
        Err(err) => Ok(Json(json!({ "ok": false, "error": err }))),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalWebFetchPayload {
    pub url: String,
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

async fn internal_web_fetch(
    State(state): State<InternalBridgeState>,
    headers: HeaderMap,
    Json(payload): Json<InternalWebFetchPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = authenticate_work_context(&state, &headers).await?;
    let paths = WorkPaths::app();
    let (task_id, policy) = resolve_policy_for_context(&ctx);
    let tool_call_id = payload
        .tool_use_id
        .unwrap_or_else(|| format!("bridge-web-open-{}", Uuid::new_v4()));
    let intent = crate::work::pipeline::ToolIntent {
        task_id,
        work_run_id: ctx.token_info.run_id.clone(),
        session_id: None,
        workspace_id: ctx.token_info.workspace_id.clone(),
        tool_call_id,
        tool_name: "web_open".to_string(),
        action: "open".to_string(),
        arguments: json!({ "url": payload.url }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: ctx.token_info.execution_context,
        policy_revision: None,
    };
    let pipeline = crate::work::pipeline::ToolPipeline::new(paths);
    match pipeline
        .execute_intent_with_proxy(&intent, &policy, ctx.token_info.proxy_url.as_deref())
        .await
    {
        Ok(result) if result.success => {
            let fetched =
                serde_json::from_str::<serde_json::Value>(&result.stdout).map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({ "error": format!("Invalid Web fetch result: {e}") })),
                    )
                })?;
            Ok(Json(json!({
                "ok": true,
                "finalUrl": fetched.get("finalUrl"),
                "title": fetched.get("title"),
                "contentType": fetched.get("contentType"),
                "text": fetched.get("text"),
            })))
        }
        Ok(result) => Ok(Json(json!({
            "ok": false,
            "error": if result.stderr.is_empty() { result.status } else { result.stderr },
        }))),
        Err(err) => Ok(Json(json!({ "ok": false, "error": err }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::executor::{WorkExecutionResult, WorkExecutionStatus};

    #[test]
    fn execution_payload_wires_resource_and_input_paths() {
        let payload: InternalExecutePayload = serde_json::from_value(serde_json::json!({
            "resourceId": "builtin.transform",
            "action": "apply",
            "arguments": {},
            "inputPaths": ["input/source.txt"]
        }))
        .unwrap();

        assert_eq!(payload.resource_id, "builtin.transform");
        assert_eq!(payload.input_paths, vec!["input/source.txt"]);
    }

    #[test]
    fn internal_execute_payload_uses_camel_case_and_has_no_workspace_field() {
        let payload: InternalExecutePayload = serde_json::from_value(serde_json::json!({
            "resourceId": "builtin.transform",
            "action": "run",
            "inputPaths": ["input/source.txt"],
            "expectedOutputs": ["output/result.txt"],
        }))
        .unwrap();

        assert_eq!(payload.resource_id, "builtin.transform");
        assert_eq!(payload.input_paths, vec!["input/source.txt"]);
    }

    #[test]
    fn execution_result_wire_contract_is_camel_case_and_fail_closed() {
        let result = WorkExecutionResult {
            execution_id: "execution-1".to_string(),
            resource_id: "builtin.transform".to_string(),
            action: "run".to_string(),
            status: WorkExecutionStatus::Failed,
            failure_kind: None,
            exit_code: Some(7),
            stdout: String::new(),
            stderr: "boom".to_string(),
            outputs: Vec::new(),
            started_at: "2026-08-13T00:00:00Z".to_string(),
            finished_at: "2026-08-13T00:00:01Z".to_string(),
        };
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["status"], "failed");
        assert_eq!(wire["exitCode"], 7);
        assert!(wire.get("exit_code").is_none());
    }

    #[tokio::test]
    async fn session_token_binds_host_proxy_for_native_web_bridge() {
        let port = start_internal_bridge().await.unwrap();
        let (registered_port, token) = register_session_token_with_proxy(
            "native-web-proxy-run",
            "native-web-proxy-workspace",
            None,
            ExecutionContext::Attended,
            Some("http://127.0.0.1:7897".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(registered_port, port);
        let info = bridge_state()
            .tokens
            .read()
            .await
            .get(&token)
            .cloned()
            .unwrap();
        assert_eq!(info.proxy_url.as_deref(), Some("http://127.0.0.1:7897"));
        bridge_state().tokens.write().await.remove(&token);
    }

    #[tokio::test]
    async fn standalone_context_keeps_ledger_namespace_out_of_task_lookup() {
        start_internal_bridge().await.unwrap();
        let run_id = format!("standalone-auth-{}", Uuid::new_v4());
        let (_, token) = register_session_token_with_subject(
            &run_id,
            "",
            Some(&run_id),
            ExecutionContext::Attended,
            BridgeSubject::Root,
        )
        .await
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );

        let context = authenticate_work_context(bridge_state(), &headers)
            .await
            .unwrap();
        assert!(context.task.is_none());
        assert!(context.work_run.is_none());
        assert_eq!(context.token_info.workspace_id, "");
        assert_eq!(context.token_info.task_id.as_deref(), Some(run_id.as_str()));

        revoke_session_token(&token).await;
    }

    #[tokio::test]
    async fn revoking_an_actor_lease_does_not_revoke_a_replacement_lease() {
        let state = bridge_state();
        let run_id = format!("token-replacement-{}", Uuid::new_v4());
        let old_token = format!("old-{}", Uuid::new_v4());
        let replacement_token = format!("replacement-{}", Uuid::new_v4());

        let make_info = |token: &str| ProcessBridgeTokenInfo {
            token: token.to_string(),
            run_id: run_id.clone(),
            task_id: None,
            workspace_id: "workspace-token-replacement".to_string(),
            execution_context: ExecutionContext::Attended,
            proxy_url: None,
            subject: BridgeSubject::Root,
        };
        {
            let mut tokens = state.tokens.write().await;
            tokens.insert(old_token.clone(), make_info(&old_token));
            tokens.insert(replacement_token.clone(), make_info(&replacement_token));
        }

        // The old actor exits after the replacement has registered. Its
        // process-owned cleanup must be exact-token scoped.
        revoke_session_token(&old_token).await;

        let tokens = state.tokens.read().await;
        assert!(!tokens.contains_key(&old_token));
        assert!(tokens.contains_key(&replacement_token));
        drop(tokens);
        revoke_session_token(&replacement_token).await;
    }

    #[test]
    fn test_payload_canonicalization_overrides_conflicting_nested_arguments() {
        let payload: InternalExecutePayload = serde_json::from_value(serde_json::json!({
            "resourceId": "work-data-analysis",
            "action": "compare_periods",
            "arguments": {
                "resource_id": "malicious_override",
                "action": "delete_all",
                "file": "input/data.xlsx"
            }
        }))
        .unwrap();

        let mut args = payload.arguments.clone();
        if let Some(obj) = args.as_object_mut() {
            obj.insert(
                "resource_id".to_string(),
                serde_json::json!(payload.resource_id),
            );
            obj.insert("action".to_string(), serde_json::json!(payload.action));
        }

        assert_eq!(args["resource_id"], "work-data-analysis");
        assert_eq!(args["action"], "compare_periods");
        assert_eq!(args["file"], "input/data.xlsx");
    }

    #[test]
    fn test_resolve_policy_for_context_standalone() {
        use super::ProcessBridgeTokenInfo;
        use crate::work::models::{ExecutionContext, WorkExecutionMode};

        let ctx = super::TrustedWorkContext {
            token_info: ProcessBridgeTokenInfo {
                token: "token-123".to_string(),
                run_id: "non-existent-standalone-run".to_string(),
                task_id: None,
                workspace_id: String::new(),
                execution_context: ExecutionContext::Attended,
                proxy_url: None,
                subject: BridgeSubject::Root,
            },
            task: None,
            work_run: None,
            workspace: None,
        };
        let (task_id, policy) = super::resolve_policy_for_context(&ctx);
        assert_eq!(task_id, "non-existent-standalone-run");
        // Defaults to Auto when no run is in storage
        assert_eq!(policy.execution_mode, WorkExecutionMode::Auto);
    }

    #[test]
    fn taskless_workspace_bridge_context_uses_workspace_default_policy() {
        use crate::work::models::WorkExecutionMode;

        let temp = tempfile::tempdir().unwrap();
        let paths = crate::work::paths::WorkPaths::new(temp.path().to_path_buf());
        let workspaces = crate::work::workspace::WorkspaceManager::new(paths.clone());
        let mut workspace = workspaces.create("Workspace chat policy test").unwrap();
        workspace.default_policy.execution_mode = WorkExecutionMode::FullAccess;
        workspace.default_policy.allow_external_connectors = true;
        workspaces.update(&mut workspace).unwrap();

        let context = trusted_work_context_for_token(
            test_bridge_token(&workspace.id, None, "workspace-chat-run"),
            &paths,
        )
        .unwrap();
        let (policy_scope, policy) = resolve_policy_for_context(&context);

        assert_eq!(policy_scope, "workspace-chat-run");
        assert_eq!(policy, workspace.default_policy);
        assert_eq!(policy.execution_mode, WorkExecutionMode::FullAccess);
        assert!(policy.allow_external_connectors);
    }

    fn test_bridge_token(
        workspace_id: &str,
        task_id: Option<&str>,
        run_id: &str,
    ) -> ProcessBridgeTokenInfo {
        ProcessBridgeTokenInfo {
            token: "test-token".to_string(),
            run_id: run_id.to_string(),
            task_id: task_id.map(str::to_string),
            workspace_id: workspace_id.to_string(),
            execution_context: crate::work::models::ExecutionContext::Attended,
            proxy_url: None,
            subject: BridgeSubject::Root,
        }
    }

    #[test]
    fn task_manifest_read_failure_rejects_bridge_context() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::work::paths::WorkPaths::new(temp.path().to_path_buf());
        let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("Bridge auth test")
            .unwrap();
        let task_manifest = paths.task_manifest_path("task-broken").unwrap();
        std::fs::create_dir_all(&task_manifest).unwrap();

        let error = trusted_work_context_for_token(
            test_bridge_token(&workspace.id, Some("task-broken"), "run-1"),
            &paths,
        )
        .unwrap_err();

        assert_eq!(error.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn work_run_read_failure_rejects_bridge_context() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::work::paths::WorkPaths::new(temp.path().to_path_buf());
        let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("Bridge run auth test")
            .unwrap();
        let tasks = crate::work::tasks::TaskManager::new(paths.clone());
        let task = tasks.create_task(&workspace.id, "Task", "", None).unwrap();
        let run = tasks
            .start_run(&task.id, None, crate::work::models::WorkRunTrigger::Manual)
            .unwrap();
        let run_path = paths
            .task_runs_dir(&task.id)
            .unwrap()
            .join(format!("{}.json", run.id));
        std::fs::write(run_path, "not-json").unwrap();

        let error = trusted_work_context_for_token(
            test_bridge_token(&workspace.id, Some(&task.id), &run.id),
            &paths,
        )
        .unwrap_err();

        assert_eq!(error.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn task_bound_bridge_context_rejects_missing_work_run() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::work::paths::WorkPaths::new(temp.path().to_path_buf());
        let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("Bridge missing run test")
            .unwrap();
        let task = crate::work::tasks::TaskManager::new(paths.clone())
            .create_task(&workspace.id, "Task", "", None)
            .unwrap();

        let error = trusted_work_context_for_token(
            test_bridge_token(&workspace.id, Some(&task.id), "run-missing"),
            &paths,
        )
        .unwrap_err();

        assert_eq!(error.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_internal_apps_list_and_call_lifecycle() {
        let state = bridge_state();
        let run_id = "test-run-apps-bridge-1";
        let workspace_id = "";

        let root_token = "root-token-apps-test";
        let root_info = ProcessBridgeTokenInfo {
            token: root_token.to_string(),
            run_id: run_id.to_string(),
            task_id: Some(run_id.to_string()),
            // This bridge lifecycle test uses an isolated paths root and does
            // not create a persisted WorkTask. Model it as standalone Work so
            // it exercises the same authority shape as the runtime fixture.
            workspace_id: workspace_id.to_string(),
            execution_context: crate::work::models::ExecutionContext::Attended,
            proxy_url: None,
            subject: BridgeSubject::Root,
        };
        state
            .tokens
            .write()
            .await
            .insert(root_token.to_string(), root_info);
        state.effective_port.store(8080, Ordering::Release);

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", root_token).parse().unwrap(),
        );

        // 1. Setup isolated TempDir paths
        let temp = tempfile::TempDir::new().unwrap();
        let paths = crate::work::paths::WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();

        // 1. List apps via bridge
        let list_res = internal_apps_list_with_paths(state.clone(), headers.clone(), &paths)
            .await
            .unwrap();
        let apps_val = list_res.0.get("apps").and_then(|v| v.as_array()).unwrap();
        assert_eq!(apps_val.len(), 7);
        assert!(apps_val.iter().any(|a| a["appId"] == "feishu"));
        assert!(apps_val.iter().any(|a| a["appId"] == "gmail"));

        // 2. Call app when not connected -> returns needs_connection
        let call_unconn_req = InternalAppCallPayload {
            app_id: "gmail".into(),
            tool_name: "search_emails".into(),
            arguments: serde_json::json!({ "query": "hello" }),
            account_id: None,
            tool_use_id: Some("call-1".into()),
        };
        let call_res =
            internal_apps_call_with_paths(state.clone(), headers.clone(), call_unconn_req, &paths)
                .await
                .unwrap();

        assert_eq!(call_res.0["status"], "needs_connection");
        assert_eq!(call_res.0["appId"], "gmail");
        let inbox_id = call_res.0["inboxItemId"].as_str().unwrap();
        let inbox_path = paths.inbox_item_path(inbox_id).unwrap();
        let inbox: crate::work::models::InboxItem =
            serde_json::from_str(&std::fs::read_to_string(inbox_path).unwrap()).unwrap();
        assert!(inbox.payload.auth_url.is_none());
        let interaction_path = paths
            .inbox_dir()
            .join(format!("{inbox_id}.interaction.json"));
        let interaction: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(interaction_path).unwrap()).unwrap();
        assert!(interaction["payload"].get("authUrl").is_none());

        // 3. Connect gmail with 1 account
        let conn = crate::work::apps::models::AppConnection {
            connection_id: "conn-gmail-test".into(),
            app_id: "gmail".into(),
            provider: "native".into(),
            status: crate::work::apps::models::ConnectionStatus::Connected,
            accounts: vec![crate::work::apps::models::AppAccount {
                account_id: "acc_gmail_1".into(),
                alias: Some("工作邮箱".into()),
                display_name: Some("user@test.com".into()),
                email: Some("user@test.com".into()),
                status: crate::work::apps::models::ConnectionStatus::Connected,
                created_at: chrono::Utc::now().to_rfc3339(),
                last_used_at: None,
            }],
            last_checked_at: Some(chrono::Utc::now().to_rfc3339()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        crate::work::apps::storage::save_connection(&paths, &conn).unwrap();

        // 4. Call app now -> reaches the real provider boundary. The isolated
        // test has no Composio credential, so it must fail closed instead of
        // returning the old synthetic success payload.
        let call_conn_req = InternalAppCallPayload {
            app_id: "gmail".into(),
            tool_name: "search_emails".into(),
            arguments: serde_json::json!({ "query": "hello" }),
            account_id: None,
            tool_use_id: Some("call-2".into()),
        };
        let call_res_conn =
            internal_apps_call_with_paths(state.clone(), headers.clone(), call_conn_req, &paths)
                .await
                .unwrap_err();

        assert_eq!(call_res_conn.0, StatusCode::BAD_GATEWAY);

        // 5. Add second account and set workspace default account
        let mut multi_conn = conn.clone();
        multi_conn
            .accounts
            .push(crate::work::apps::models::AppAccount {
                account_id: "acc_gmail_2".into(),
                alias: Some("个人邮箱".into()),
                display_name: Some("personal@test.com".into()),
                email: Some("personal@test.com".into()),
                status: crate::work::apps::models::ConnectionStatus::Connected,
                created_at: chrono::Utc::now().to_rfc3339(),
                last_used_at: None,
            });
        crate::work::apps::storage::save_connection(&paths, &multi_conn).unwrap();

        // Without default set and without payload.account_id -> returns needs_account_selection
        let call_multi_req = InternalAppCallPayload {
            app_id: "gmail".into(),
            tool_name: "search_emails".into(),
            arguments: serde_json::json!({ "query": "hello" }),
            account_id: None,
            tool_use_id: Some("call-3".into()),
        };
        let call_res_multi =
            internal_apps_call_with_paths(state.clone(), headers.clone(), call_multi_req, &paths)
                .await
                .unwrap();
        assert_eq!(call_res_multi.0["status"], "needs_account_selection");

        // Set workspace default account to acc_gmail_2
        crate::work::apps::storage::set_workspace_default_account(
            &paths,
            workspace_id,
            "gmail",
            "acc_gmail_2",
        )
        .unwrap();

        let call_res_def = internal_apps_call_with_paths(
            state.clone(),
            headers.clone(),
            InternalAppCallPayload {
                app_id: "gmail".into(),
                tool_name: "search_emails".into(),
                arguments: serde_json::json!({ "query": "hello" }),
                account_id: None,
                tool_use_id: Some("call-4".into()),
            },
            &paths,
        )
        .await
        .unwrap_err();
        assert_eq!(call_res_def.0, StatusCode::BAD_GATEWAY);

        // Cleanup
        crate::work::apps::storage::remove_connection(&paths, "gmail").unwrap();
    }

    #[tokio::test]
    async fn test_internal_task_state_crud_and_revision_conflict() {
        let temp = tempfile::TempDir::new().unwrap();
        std::env::set_var("AGENTCABIN_DATA_DIR", temp.path());
        let run_id = format!("test-run-task-state-{}", uuid::Uuid::new_v4());
        let token = format!("test-token-{}", uuid::Uuid::new_v4());
        let state = InternalBridgeState::default();
        {
            let mut tokens = state.tokens.write().await;
            tokens.insert(
                token.clone(),
                ProcessBridgeTokenInfo {
                    token: token.clone(),
                    run_id: run_id.clone(),
                    task_id: None,
                    // Workspace-less standalone context; no Workspace
                    // manifest is required for this task-state bridge test.
                    workspace_id: String::new(),
                    execution_context: ExecutionContext::Attended,
                    proxy_url: None,
                    subject: BridgeSubject::Root,
                },
            );
        }

        let mut headers = HeaderMap::new();
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());

        // 1. Initial get: should return default empty state
        let get_res = internal_task_state_get(State(state.clone()), headers.clone())
            .await
            .unwrap();
        assert_eq!(get_res.0["ok"], true);
        assert_eq!(get_res.0["workTaskState"]["revision"], 0);

        // 2. Set goal
        let update_goal = InternalTaskStateUpdatePayload {
            goal: Some("新目标".to_string()),
            ..Default::default()
        };
        let res_goal =
            internal_task_state_update(State(state.clone()), headers.clone(), Json(update_goal))
                .await
                .unwrap();
        assert_eq!(res_goal.0["ok"], true);
        assert_eq!(res_goal.0["workTaskState"]["goal"], "新目标");
        assert_eq!(res_goal.0["workTaskState"]["revision"], 1);

        // 3. Replace plan
        let update_plan = InternalTaskStateUpdatePayload {
            plan: Some(vec![crate::models::StructuredTask {
                id: "s1".to_string(),
                text: "步骤一".to_string(),
                status: crate::models::StructuredTaskStatus::Pending,
            }]),
            ..Default::default()
        };
        let res_plan =
            internal_task_state_update(State(state.clone()), headers.clone(), Json(update_plan))
                .await
                .unwrap();
        assert_eq!(res_plan.0["workTaskState"]["revision"], 2);
        assert_eq!(
            res_plan.0["workTaskState"]["plan"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        // 4. Update step status
        let update_step = InternalTaskStateUpdatePayload {
            step: Some(TaskStateStepUpdate {
                id: "s1".to_string(),
                status: "in_progress".to_string(),
                text: None,
            }),
            ..Default::default()
        };
        let res_step =
            internal_task_state_update(State(state.clone()), headers.clone(), Json(update_step))
                .await
                .unwrap();
        assert_eq!(res_step.0["workTaskState"]["revision"], 3);
        assert_eq!(
            res_step.0["workTaskState"]["plan"][0]["status"],
            "in_progress"
        );

        // 5. Optimistic concurrency conflict check: expectedRevision = 2 (actual is 3) -> 409 Conflict
        let conflict_update = InternalTaskStateUpdatePayload {
            expected_revision: Some(2),
            goal: Some("冲突目标".to_string()),
            ..Default::default()
        };
        let conflict_err = internal_task_state_update(
            State(state.clone()),
            headers.clone(),
            Json(conflict_update),
        )
        .await
        .unwrap_err();
        assert_eq!(conflict_err.0, StatusCode::CONFLICT);

        // 6. Successful update with expectedRevision = 3
        let valid_rev_update = InternalTaskStateUpdatePayload {
            expected_revision: Some(3),
            goal: Some("最终目标".to_string()),
            ..Default::default()
        };
        let valid_res = internal_task_state_update(
            State(state.clone()),
            headers.clone(),
            Json(valid_rev_update),
        )
        .await
        .unwrap();
        assert_eq!(valid_res.0["workTaskState"]["revision"], 4);
        assert_eq!(valid_res.0["workTaskState"]["goal"], "最终目标");

        // Clean up run dir
        let _ = crate::storage::runs::delete_runs(&[run_id]);
        std::env::remove_var("AGENTCABIN_DATA_DIR");
    }
}
