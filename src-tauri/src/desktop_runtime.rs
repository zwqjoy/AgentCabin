//! Shared Code desktop runtime bridge.
//!
//! Work calls the desktop operator through its policy pipeline. Code gets the
//! V2 adapters reach the Host-owned native backend through this loopback-only,
//! per-run bearer-token bridge, matching the Code Browser Runtime pattern.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;

const BRIDGE_READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct DesktopSessionToken {
    run_id: String,
}

#[derive(Clone)]
pub struct DesktopRuntimeState {
    tokens: Arc<RwLock<HashMap<String, DesktopSessionToken>>>,
    effective_port: Arc<AtomicU16>,
    ready: Arc<Notify>,
}

impl Default for DesktopRuntimeState {
    fn default() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            effective_port: Arc::new(AtomicU16::new(0)),
            ready: Arc::new(Notify::new()),
        }
    }
}

static STATE: std::sync::OnceLock<DesktopRuntimeState> = std::sync::OnceLock::new();

pub fn state() -> &'static DesktopRuntimeState {
    STATE.get_or_init(DesktopRuntimeState::default)
}

async fn wait_for_port(runtime: &DesktopRuntimeState) -> Result<u16, String> {
    let notified = runtime.ready.notified();
    let port = runtime.effective_port.load(Ordering::Acquire);
    if port != 0 {
        return Ok(port);
    }
    tokio::time::timeout(BRIDGE_READY_TIMEOUT, notified)
        .await
        .map_err(|_| "Code Desktop Runtime bridge did not become ready in time".to_string())?;
    let port = runtime.effective_port.load(Ordering::Acquire);
    if port == 0 {
        Err("Code Desktop Runtime bridge is unavailable".to_string())
    } else {
        Ok(port)
    }
}

/// Register a Code run and return the loopback bridge address/token.
pub async fn register_session(run_id: &str) -> Result<(u16, String), String> {
    let runtime = state();
    let port = wait_for_port(runtime).await?;
    let token = format!("dbr-{}", Uuid::new_v4());
    let mut tokens = runtime.tokens.write().await;
    tokens.retain(|_, existing| existing.run_id != run_id);
    tokens.insert(
        token.clone(),
        DesktopSessionToken {
            run_id: run_id.to_string(),
        },
    );
    Ok((port, token))
}

pub async fn revoke_session(run_id: &str) {
    state()
        .tokens
        .write()
        .await
        .retain(|_, info| info.run_id != run_id);
    crate::work::desktop_operator::desktop_operator_manager()
        .release_for_run(run_id)
        .await;
}

/// Revoke one exact token so an older actor cannot release a replacement run's
/// token. The desktop lease is shared by run id and is released with that token.
pub async fn revoke_token(token: &str) {
    let run_id = state()
        .tokens
        .write()
        .await
        .remove(token)
        .map(|info| info.run_id);
    if let Some(run_id) = run_id {
        crate::work::desktop_operator::desktop_operator_manager()
            .release_for_run(&run_id)
            .await;
    }
}

pub async fn is_run_active(run_id: &str) -> bool {
    state()
        .tokens
        .read()
        .await
        .values()
        .any(|info| info.run_id == run_id)
}

/// Start the loopback-only authenticated Code Desktop Runtime bridge.
pub async fn start_bridge() -> Result<u16, String> {
    let runtime = state().clone();
    if let port @ 1..=u16::MAX = runtime.effective_port.load(Ordering::Acquire) {
        return Ok(port);
    }

    let router = Router::new()
        .route("/internal/desktop/call", post(call_desktop_tool))
        .with_state(runtime.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("Failed to bind Code Desktop Runtime bridge: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Failed to read Code Desktop Runtime address: {error}"))?
        .port();
    runtime.effective_port.store(port, Ordering::Release);
    runtime.ready.notify_waiters();
    log::info!("[desktop/bridge] Code Desktop Runtime listening on 127.0.0.1:{port}");

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            log::error!("[desktop/bridge] Code Desktop Runtime server error: {error}");
        }
    });
    Ok(port)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopCallPayload {
    #[serde(default)]
    tool_call_id: Option<String>,
    method: String,
    #[serde(default)]
    params: Value,
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
    runtime: &DesktopRuntimeState,
    headers: &HeaderMap,
) -> Result<DesktopSessionToken, (StatusCode, Json<Value>)> {
    let token = bearer_token(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Missing or invalid Code Desktop Runtime bearer token"})),
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
                Json(json!({"error": "Forbidden: invalid Code Desktop Runtime session token"})),
            )
        })
}

async fn call_desktop_tool(
    State(runtime): State<DesktopRuntimeState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<DesktopCallPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session = authenticate(&runtime, &headers).await?;
    if !crate::work::desktop_operator::is_enabled() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Desktop Use is disabled; enable it in Settings > Computer Control, or set AGENTCABIN_DESKTOP_USE_ENABLED=1 for development"
            })),
        ));
    }

    let manager = crate::work::desktop_operator::desktop_operator_manager();
    match manager
        .execute(&session.run_id, &payload.method, payload.params)
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
            "failureKind": "desktop_runtime",
            "exitCode": -1,
            "stdout": "",
            "stderr": error,
            "outputs": [],
            "toolCallId": payload.tool_call_id,
        }))),
    }
}
