//! Shared Code/Work Browser Runtime bridge.
//!
//! The Node worker is process-wide and connects to Electron's authenticated
//! embedded CDP relay. This bridge only provides Code with a narrow call
//! surface. Work deliberately does not use it for authorization: Work calls
//! the same manager through its Harness ToolPipeline so policy, Inbox approval,
//! and the runtime ledger remain Work-owned.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;

const BRIDGE_READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct BrowserSessionToken {
    run_id: String,
    output_root: PathBuf,
}

#[derive(Clone)]
pub struct BrowserRuntimeState {
    tokens: Arc<RwLock<HashMap<String, BrowserSessionToken>>>,
    effective_port: Arc<AtomicU16>,
    ready: Arc<Notify>,
}

impl Default for BrowserRuntimeState {
    fn default() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            effective_port: Arc::new(AtomicU16::new(0)),
            ready: Arc::new(Notify::new()),
        }
    }
}

static STATE: std::sync::OnceLock<BrowserRuntimeState> = std::sync::OnceLock::new();

pub fn state() -> &'static BrowserRuntimeState {
    STATE.get_or_init(BrowserRuntimeState::default)
}

async fn wait_for_port(runtime: &BrowserRuntimeState) -> Result<u16, String> {
    let notified = runtime.ready.notified();
    let port = runtime.effective_port.load(Ordering::Acquire);
    if port != 0 {
        return Ok(port);
    }
    tokio::time::timeout(BRIDGE_READY_TIMEOUT, notified)
        .await
        .map_err(|_| "Shared Browser Runtime bridge did not become ready in time".to_string())?;
    let port = runtime.effective_port.load(Ordering::Acquire);
    if port == 0 {
        Err("Shared Browser Runtime bridge is unavailable".to_string())
    } else {
        Ok(port)
    }
}

/// Register one Code browser session and return the bridge address/token.
pub async fn register_session(run_id: &str, output_root: &Path) -> Result<(u16, String), String> {
    let runtime = state();
    let port = wait_for_port(runtime).await?;
    std::fs::create_dir_all(output_root)
        .map_err(|error| format!("Failed to create browser output directory: {error}"))?;
    let token = format!("cbr-{}", Uuid::new_v4());
    let info = BrowserSessionToken {
        run_id: run_id.to_string(),
        output_root: output_root.to_path_buf(),
    };
    let mut tokens = runtime.tokens.write().await;
    tokens.retain(|_, existing| existing.run_id != run_id);
    tokens.insert(token.clone(), info);
    crate::work::browser_operator::browser_session_manager()
        .get_or_create_session(run_id, "code")
        .await;
    Ok((port, token))
}

pub async fn revoke_session_tokens(run_id: &str) {
    revoke_session_tokens_from(state(), run_id).await;
}

async fn revoke_session_tokens_from(runtime: &BrowserRuntimeState, run_id: &str) {
    runtime
        .tokens
        .write()
        .await
        .retain(|_, info| info.run_id != run_id);
}

/// Close the run-scoped Browser session after an explicit Browser close.
/// Actor stop/replacement must only revoke its runtime credentials.
pub async fn close_browser_session(run_id: &str) {
    let _ = crate::work::browser_operator::browser_session_manager()
        .close_session(run_id)
        .await;
}

/// Revoke one exact session token. Actor cleanup uses this form so an older
/// actor being replaced cannot revoke the replacement actor's fresh token.
pub async fn revoke_token(token: &str) {
    state().tokens.write().await.remove(token);
}

/// Provision the two shared Pi extension entrypoints used by Code.
///
/// They live beside the common npm packages so ESM dependency resolution can
/// find the shared `typebox` dependency. Work keeps profile-local copies for
/// its isolated profile, but both copies contain the same protocol adapter
/// source.
pub fn ensure_code_adapters(
    paths: &crate::work::paths::WorkPaths,
) -> Result<(PathBuf, PathBuf), String> {
    let dir = paths
        .pi_system_dir()
        .join("npm")
        .join("node_modules")
        .join("agentcabin-browser-runtime");
    std::fs::create_dir_all(&dir).map_err(|error| {
        format!("Failed to create shared Browser Runtime adapter directory: {error}")
    })?;
    let web_entry = dir.join("web.mjs");
    let browser_entry = dir.join("browser.mjs");
    std::fs::write(&web_entry, include_str!("work/pi_browser_adapter.mjs"))
        .map_err(|error| format!("Failed to provision shared Web adapter: {error}"))?;
    std::fs::write(
        &browser_entry,
        include_str!("work/pi_browser_operator_adapter.mjs"),
    )
    .map_err(|error| format!("Failed to provision shared Browser adapter: {error}"))?;
    Ok((web_entry, browser_entry))
}

/// Start the loopback-only authenticated bridge.
pub async fn start_bridge() -> Result<u16, String> {
    let runtime = state().clone();
    if let port @ 1..=u16::MAX = runtime.effective_port.load(Ordering::Acquire) {
        return Ok(port);
    }

    let router = Router::new()
        .route("/internal/browser/call", post(call_browser_tool))
        .with_state(runtime.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("Failed to bind Browser Runtime bridge: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Failed to read Browser Runtime bridge address: {error}"))?
        .port();
    runtime.effective_port.store(port, Ordering::Release);
    runtime.ready.notify_waiters();
    log::info!("[browser/bridge] Shared Browser Runtime listening on 127.0.0.1:{port}");

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            log::error!("[browser/bridge] Browser Runtime server error: {error}");
        }
    });
    Ok(port)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserCallPayload {
    #[serde(default)]
    tool_call_id: Option<String>,
    method: String,
    #[serde(default)]
    params: Value,
    /// Live Eval may authorize one exact temporary loopback origin for its
    /// fixture. Normal Code/Work browser calls leave this unset.
    #[serde(default)]
    eval_fixture_origin: Option<String>,
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
    runtime: &BrowserRuntimeState,
    headers: &HeaderMap,
) -> Result<BrowserSessionToken, (StatusCode, Json<Value>)> {
    let token = bearer_token(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Missing or invalid Browser Runtime bearer token"})),
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
                Json(json!({"error": "Forbidden: invalid Browser Runtime session token"})),
            )
        })
}

fn payload_string(params: &Map<String, Value>, name: &str) -> Option<String> {
    params
        .get(name)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn scoped_output_path(root: &Path, filename: &str) -> Result<PathBuf, String> {
    let relative = Path::new(filename);
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(
            "Browser screenshot filename must be relative to the session output directory"
                .to_string(),
        );
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(
            "Browser screenshot filename cannot escape the session output directory".to_string(),
        );
    }
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|error| format!("Failed to resolve browser output directory: {error}"))?;
    let target = canonical_root.join(relative);
    if let Ok(metadata) = std::fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() {
            return Err("Browser screenshot filename cannot overwrite a symbolic link".to_string());
        }
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create browser screenshot directory: {error}"))?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|error| format!("Failed to resolve browser screenshot directory: {error}"))?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(
                "Browser screenshot filename cannot escape the session output directory"
                    .to_string(),
            );
        }
    }
    Ok(target)
}

async fn call_browser_tool(
    State(runtime): State<BrowserRuntimeState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<BrowserCallPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session = authenticate(&runtime, &headers).await?;
    if !crate::storage::profile_bindings::is_browser_use_enabled() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Browser Use 已在能力中心停用；请重新启用后再开始浏览器操作"
            })),
        ));
    }
    let mut params = payload.params.as_object().cloned().unwrap_or_default();
    let canonical_method = payload.method.as_str();

    if canonical_method == "browser_take_screenshot" {
        if let Some(filename) =
            payload_string(&params, "filename").or_else(|| payload_string(&params, "outputPath"))
        {
            let target = scoped_output_path(&session.output_root, &filename)
                .map_err(|error| (StatusCode::BAD_REQUEST, Json(json!({"error": error}))))?;
            params.remove("filename");
            params.insert(
                "outputPath".to_string(),
                Value::String(target.to_string_lossy().into_owned()),
            );
        }
    }

    let manager = crate::work::browser_operator::browser_operator_manager();
    let eval_tabs = canonical_method == "browser_tabs"
        && params
            .get("action")
            .and_then(Value::as_str)
            .is_some_and(|action| action == "new");
    let (execution_method, execution_params) =
        if canonical_method == "browser_navigate" || eval_tabs {
            if let Some(origin) = payload.eval_fixture_origin.as_deref() {
                params.insert("allowOrigin".to_string(), Value::String(origin.to_string()));
                (
                    if canonical_method == "browser_navigate" {
                        "browser_eval_navigate"
                    } else {
                        "browser_eval_tabs"
                    },
                    Value::Object(params),
                )
            } else {
                (canonical_method, Value::Object(params))
            }
        } else {
            (canonical_method, Value::Object(params))
        };
    match manager
        .execute(&session.run_id, execution_method, execution_params)
        .await
    {
        Ok(value) => Ok(Json(json!({
            "success": true,
            "status": "success",
            "failureKind": Value::Null,
            "exitCode": 0,
            "stdout": serde_json::to_string(&value).unwrap_or_default(),
            "stderr": "",
            "outputs": [],
            "toolCallId": payload.tool_call_id,
        }))),
        Err(error) => Ok(Json(json!({
            "success": false,
            "status": "failed",
            "failureKind": "browser_runtime",
            "exitCode": -1,
            "stdout": "",
            "stderr": error,
            "outputs": [],
            "toolCallId": payload.tool_call_id,
        }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn revoking_run_tokens_does_not_close_browser_session() {
        let runtime = BrowserRuntimeState::default();
        let run_id = format!("browser-runtime-revoke-{}", Uuid::new_v4());
        runtime.tokens.write().await.insert(
            "test-token".to_string(),
            BrowserSessionToken {
                run_id: run_id.clone(),
                output_root: PathBuf::new(),
            },
        );
        let manager = crate::work::browser_operator::browser_session_manager();
        manager.get_or_create_session(&run_id, "code").await;

        revoke_session_tokens_from(&runtime, &run_id).await;

        assert!(!runtime.tokens.read().await.contains_key("test-token"));
        let session = manager
            .get_session(&run_id)
            .await
            .expect("browser session should remain available");
        assert_ne!(
            session.status,
            crate::work::models::BrowserSessionStatus::Closed
        );
    }

    #[tokio::test]
    async fn explicit_browser_close_marks_the_run_session_closed() {
        let run_id = format!("browser-runtime-close-{}", Uuid::new_v4());
        let manager = crate::work::browser_operator::browser_session_manager();
        manager.get_or_create_session(&run_id, "code").await;

        close_browser_session(&run_id).await;

        let session = manager
            .get_session(&run_id)
            .await
            .expect("explicitly closed browser session should be retained");
        assert_eq!(
            session.status,
            crate::work::models::BrowserSessionStatus::Closed
        );
    }
}
