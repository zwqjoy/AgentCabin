//! Code-side bridge for Connector Package CLI operations.
//!
//! Connector Packages are shared by Code and Work, but their execution
//! authority is not. Work routes the same operation through its Harness and
//! Inbox policy; Code uses this narrow, per-session authenticated bridge and
//! Pi's permission system. The Host still owns package trust, enablement,
//! operation validation, process execution, authentication state, and output
//! redaction in both modes.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;

const BRIDGE_READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct CodeConnectorSession {
    run_id: String,
    plugin_mcp_servers: std::collections::HashSet<String>,
}

#[derive(Clone)]
pub struct CodeConnectorRuntimeState {
    tokens: Arc<RwLock<HashMap<String, CodeConnectorSession>>>,
    effective_port: Arc<AtomicU16>,
    ready: Arc<Notify>,
}

impl Default for CodeConnectorRuntimeState {
    fn default() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            effective_port: Arc::new(AtomicU16::new(0)),
            ready: Arc::new(Notify::new()),
        }
    }
}

static STATE: std::sync::OnceLock<CodeConnectorRuntimeState> = std::sync::OnceLock::new();

pub fn state() -> &'static CodeConnectorRuntimeState {
    STATE.get_or_init(CodeConnectorRuntimeState::default)
}

async fn wait_for_port(runtime: &CodeConnectorRuntimeState) -> Result<u16, String> {
    let notified = runtime.ready.notified();
    let port = runtime.effective_port.load(Ordering::Acquire);
    if port != 0 {
        return Ok(port);
    }
    tokio::time::timeout(BRIDGE_READY_TIMEOUT, notified)
        .await
        .map_err(|_| "Code Connector bridge did not become ready in time".to_string())?;
    let port = runtime.effective_port.load(Ordering::Acquire);
    if port == 0 {
        Err("Code Connector bridge is unavailable".to_string())
    } else {
        Ok(port)
    }
}

/// Register one Code Pi session and return the bridge address/token.
pub async fn register_session(run_id: &str) -> Result<(u16, String), String> {
    let runtime = state();
    let port = wait_for_port(runtime).await?;
    let token = format!("cct-{}", Uuid::new_v4());
    let session = CodeConnectorSession {
        run_id: run_id.to_string(),
        plugin_mcp_servers: crate::storage::agent_plugins::list_enabled_mcp_servers_with_root(
            &crate::storage::data_dir(),
        )
        .into_iter()
        .map(|server| server.id)
        .collect(),
    };
    let mut tokens = runtime.tokens.write().await;
    tokens.retain(|_, existing| existing.run_id != run_id);
    tokens.insert(token.clone(), session);
    Ok((port, token))
}

/// Revoke every connector token for a run before a replacement session starts.
pub async fn revoke_session(run_id: &str) {
    state()
        .tokens
        .write()
        .await
        .retain(|_, session| session.run_id != run_id);
}

/// Revoke one exact token. This prevents an older actor from revoking a fresh
/// replacement session that happens to use the same run id.
pub async fn revoke_token(token: &str) {
    state().tokens.write().await.remove(token);
}

/// Provision the Code-only Connector Package adapter beside AgentCabin's
/// common Pi system dependencies so its ESM imports resolve predictably.
pub fn ensure_code_adapter(paths: &crate::work::paths::WorkPaths) -> Result<PathBuf, String> {
    let dir = paths
        .pi_system_dir()
        .join("npm")
        .join("node_modules")
        .join("agentcabin-connector-runtime");
    std::fs::create_dir_all(&dir).map_err(|error| {
        format!("Failed to create shared Connector Runtime adapter directory: {error}")
    })?;
    let entry = dir.join("connector.mjs");
    std::fs::write(&entry, include_str!("work/pi_connector_adapter.mjs"))
        .map_err(|error| format!("Failed to provision Code Connector adapter: {error}"))?;
    Ok(entry)
}

/// Start the loopback-only authenticated bridge.
pub async fn start_bridge() -> Result<u16, String> {
    let runtime = state().clone();
    if let port @ 1..=u16::MAX = runtime.effective_port.load(Ordering::Acquire) {
        return Ok(port);
    }

    let router = Router::new()
        .route("/internal/code/connector_cli", post(call_connector_cli))
        .route(
            "/internal/code/mcp/:server_name",
            post(call_agent_plugin_mcp),
        )
        .with_state(runtime.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("Failed to bind Code Connector bridge: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Failed to read Code Connector bridge address: {error}"))?
        .port();
    runtime.effective_port.store(port, Ordering::Release);
    runtime.ready.notify_waiters();
    log::info!("[code/connector-bridge] listening on 127.0.0.1:{port}");

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            log::error!("[code/connector-bridge] server error: {error}");
        }
    });
    Ok(port)
}

fn mcp_rpc_result(id: Value, result: Value) -> Json<Value> {
    Json(json!({"jsonrpc":"2.0", "id":id, "result":result}))
}

fn mcp_rpc_error(id: Value, code: i64, message: impl Into<String>) -> Json<Value> {
    let mut message = message.into().chars().take(2000).collect::<String>();
    if message.chars().count() == 2000 {
        message.push('…');
    }
    Json(json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}}))
}

async fn call_agent_plugin_mcp(
    State(runtime): State<CodeConnectorRuntimeState>,
    headers: HeaderMap,
    Path(server_name): Path<String>,
    axum::Json(request): axum::Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session = authenticate(&runtime, &headers).await?;
    if !server_name.starts_with("agent-plugin--")
        || server_name.len() > 180
        || !server_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
    {
        return Ok(mcp_rpc_error(
            Value::Null,
            -32602,
            "Invalid Agent Plugin MCP server name",
        ));
    }
    let paths = crate::work::paths::WorkPaths::app();
    let expert_config = crate::storage::agent_plugins::selected_expert_mcp_config_with_root(
        paths.data_root(),
        &session.run_id,
        &server_name,
    )
    .map_err(|error| bridge_error(StatusCode::BAD_REQUEST, error))?;
    if !session.plugin_mcp_servers.contains(&server_name) && expert_config.is_none() {
        return Ok(mcp_rpc_error(
            Value::Null,
            -32602,
            "Agent Plugin MCP server is not enabled for this session",
        ));
    }
    let Some(object) = request.as_object() else {
        return Ok(mcp_rpc_error(
            Value::Null,
            -32600,
            "MCP request must be an object",
        ));
    };
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    match method {
        "initialize" => Ok(mcp_rpc_result(
            id,
            json!({
                "protocolVersion":"2024-11-05",
                "capabilities":{"tools":{"listChanged":false}},
                "serverInfo":{"name":"AgentCabin Code Plugin MCP bridge","version":"1.0.0"}
            }),
        )),
        "notifications/initialized" => Ok(mcp_rpc_result(id, json!({}))),
        "tools/list" | "tools/call" => {
            let selected = if let Some(config) = expert_config {
                config
            } else {
                let config_path =
                    crate::storage::agent_plugins::agent_plugin_mcp_config_path_with_root(
                        paths.data_root(),
                        "code",
                    )
                    .map_err(|error| bridge_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
                let metadata = std::fs::symlink_metadata(&config_path).map_err(|_| {
                    bridge_error(
                        StatusCode::NOT_FOUND,
                        "Agent Plugin MCP config is unavailable",
                    )
                })?;
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.len() > 256 * 1024
                {
                    return Err(bridge_error(
                        StatusCode::BAD_REQUEST,
                        "Agent Plugin MCP config is invalid",
                    ));
                }
                let root: Value =
                    serde_json::from_slice(&std::fs::read(&config_path).map_err(|error| {
                        bridge_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
                    })?)
                    .map_err(|error| bridge_error(StatusCode::BAD_REQUEST, error.to_string()))?;
                let Some(servers) = root.get("mcpServers").and_then(Value::as_object) else {
                    return Ok(mcp_rpc_error(
                        id,
                        -32602,
                        "Agent Plugin MCP server is unavailable",
                    ));
                };
                let Some(selected) = servers.get(&server_name).cloned() else {
                    return Ok(mcp_rpc_error(
                        id,
                        -32602,
                        "Agent Plugin MCP server is unavailable",
                    ));
                };
                selected
            };
            let mut selected_root = json!({ "mcpServers": { server_name.clone(): selected } });
            crate::work::mcp::resolve_agent_plugin_environment(&mut selected_root, &|name| {
                std::env::var_os(name)
            })
            .map_err(|error| bridge_error(StatusCode::BAD_REQUEST, error))?;
            let Some(config) = selected_root
                .get("mcpServers")
                .and_then(Value::as_object)
                .and_then(|servers| servers.get(&server_name))
                .and_then(Value::as_object)
            else {
                return Ok(mcp_rpc_error(
                    id,
                    -32602,
                    "Agent Plugin MCP server is unavailable",
                ));
            };
            let config = config.clone();
            let result = if method == "tools/list" {
                crate::work::mcp::list_tools_with_config(&paths, &server_name, &config, None).await
            } else {
                let Some(call) = params.as_object() else {
                    return Ok(mcp_rpc_error(
                        id,
                        -32602,
                        "tools/call params must be an object",
                    ));
                };
                let Some(tool) = call.get("name").and_then(Value::as_str).filter(|name| {
                    !name.trim().is_empty()
                        && name.len() <= 200
                        && !name.chars().any(char::is_control)
                }) else {
                    return Ok(mcp_rpc_error(
                        id,
                        -32602,
                        "tools/call requires a valid tool name",
                    ));
                };
                let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
                if !arguments.is_object() {
                    return Ok(mcp_rpc_error(
                        id,
                        -32602,
                        "tools/call arguments must be an object",
                    ));
                }
                crate::work::mcp::call_with_config(
                    &paths,
                    &server_name,
                    &config,
                    tool,
                    &arguments,
                    None,
                )
                .await
            };
            match result {
                Ok(value) => Ok(mcp_rpc_result(id, value)),
                Err(error) => Ok(mcp_rpc_error(id, -32000, error)),
            }
        }
        _ => Ok(mcp_rpc_error(
            id,
            -32601,
            format!("Unsupported MCP method: {method}"),
        )),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorCallPayload {
    #[serde(default)]
    tool_call_id: Option<String>,
    package_id: String,
    operation: String,
    #[serde(default)]
    args: Vec<String>,
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn authenticate(
    runtime: &CodeConnectorRuntimeState,
    headers: &HeaderMap,
) -> Result<CodeConnectorSession, (StatusCode, Json<Value>)> {
    let token = bearer_token(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Missing or invalid Code Connector bearer token"})),
        )
    })?;
    runtime
        .tokens
        .read()
        .await
        .get(token)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "Forbidden: invalid Code Connector session token"})),
            )
        })
}

fn bridge_error(status: StatusCode, error: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": error.into()})))
}

async fn call_connector_cli(
    State(runtime): State<CodeConnectorRuntimeState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<ConnectorCallPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session = authenticate(&runtime, &headers).await?;
    let paths = crate::work::paths::WorkPaths::app();
    paths
        .ensure_layout()
        .map_err(|error| bridge_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    if !crate::storage::profile_bindings::is_connector_enabled(&payload.package_id) {
        return Err(bridge_error(
            StatusCode::FORBIDDEN,
            format!(
                "Connector Package '{}' is disabled in the Capability Center",
                payload.package_id
            ),
        ));
    }

    let invocation = crate::work::connector_package_manager::resolve_cli_invocation(
        &paths,
        &payload.package_id,
        &payload.operation,
        &payload.args,
    )
    .map_err(|error| bridge_error(StatusCode::BAD_REQUEST, error))?;

    // Code has no Work Workspace boundary. The command is nevertheless safe
    // to run with host access here because its executable, static arguments,
    // user arguments, auth state, and enabled/trusted package all passed the
    // Connector Package validator above. Pi's permission system supplies the
    // user-facing confirmation for this registered external tool.
    paths
        .ensure_standalone_task_dir(&session.run_id)
        .map_err(|error| bridge_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let mut result = crate::work::executor::command_runner::run_command(
        &paths,
        "",
        Some(&session.run_id),
        "work_run_connector_cli",
        &invocation.operation,
        &invocation.command,
        &invocation.args,
        &invocation.cwd,
        invocation.timeout_seconds,
        &[],
        &invocation.home_paths,
        true,
        false,
    )
    .await
    .map_err(|error| bridge_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    result.stdout = crate::work::connector_package_manager::redact_cli_output(
        &result.stdout,
        &invocation.redaction_fields,
    );
    result.stderr = crate::work::connector_package_manager::redact_cli_output(
        &result.stderr,
        &invocation.redaction_fields,
    );
    if result.status == crate::work::executor::WorkExecutionStatus::Success {
        if let Err(error) =
            crate::work::connector_package_manager::record_cli_result(&paths, &invocation, &result)
        {
            result.status = crate::work::executor::WorkExecutionStatus::Failed;
            result.failure_kind =
                Some(crate::work::executor::ExecutionFailureKind::CapabilityFailure);
            result.exit_code = Some(1);
            if !result.stderr.is_empty() {
                result.stderr.push('\n');
            }
            result.stderr.push_str(&format!(
                "Failed to persist Connector Package CLI state: {error}"
            ));
        }
    }

    let mut response = serde_json::to_value(result)
        .map_err(|error| bridge_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    if let Some(tool_call_id) = payload.tool_call_id {
        if let Some(object) = response.as_object_mut() {
            object.insert("toolCallId".to_string(), Value::String(tool_call_id));
        }
    }
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_source_is_embedded_and_payload_uses_camel_case() {
        assert!(include_str!("work/pi_connector_adapter.mjs")
            .contains("AGENTCABIN_CODE_CONNECTOR_BRIDGE_TOKEN"));
        let payload: ConnectorCallPayload = serde_json::from_value(json!({
            "toolCallId": "tool-1",
            "packageId": "feishu",
            "operation": "searchUser",
            "args": ["--query", "alice"]
        }))
        .unwrap();
        assert_eq!(payload.tool_call_id.as_deref(), Some("tool-1"));
        assert_eq!(payload.package_id, "feishu");
        assert_eq!(payload.operation, "searchUser");
        assert_eq!(payload.args, vec!["--query", "alice"]);
    }
}
