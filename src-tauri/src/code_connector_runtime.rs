//! Code-side bridge for Connector Package CLI operations.
//!
//! Connector Packages are shared by Code and Work, but their execution
//! authority is not. Work routes the same operation through its Harness and
//! Inbox policy; Code uses this narrow, per-session authenticated bridge and
//! Pi's permission system. The Host still owns package trust, enablement,
//! operation validation, process execution, authentication state, and output
//! redaction in both modes.

use axum::extract::State;
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

/// Provision the AgentCabin-owned structured questionnaire for Code Pi.
/// Work registers the same public capability in its core extension, so this
/// adapter is only loaded for Code sessions.
pub fn ensure_code_ask_questions_adapter(
    paths: &crate::work::paths::WorkPaths,
) -> Result<PathBuf, String> {
    let dir = paths
        .pi_system_dir()
        .join("npm")
        .join("node_modules")
        .join("agentcabin-ask-questions");
    std::fs::create_dir_all(&dir).map_err(|error| {
        format!("Failed to create Code ask_questions adapter directory: {error}")
    })?;
    let entry = dir.join("ask_questions.mjs");
    std::fs::write(&entry, include_str!("work/pi_ask_questions_extension.mjs"))
        .map_err(|error| format!("Failed to provision Code ask_questions adapter: {error}"))?;
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
