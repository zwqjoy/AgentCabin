//! Loopback-only HTTP server for headless CoreRuntime.
//!
//! Electron (P4) connects here instead of going through Tauri IPC.
//! Contract:
//!   GET  /health          — liveness
//!   POST /invoke          — `{ method, params }` → `{ result }` or `{ error }`
//!   GET  /events          — SSE stream of core events (A+B broadcaster channels)
//!
//! Auth: `Authorization: Bearer <token>` or `?token=<token>`.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use super::runtime::CoreRuntime;

#[derive(Clone)]
struct CoreServerState {
    runtime: Arc<CoreRuntime>,
}

#[derive(Debug, Deserialize)]
struct InvokeRequest {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct InvokeResponse {
    result: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

/// JSON printed to stdout on startup so Electron can discover port + token.
#[derive(Debug, Serialize)]
pub struct CoreStartupInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

/// Bind to 127.0.0.1:0, start serving, return the actual port.
pub async fn start(runtime: Arc<CoreRuntime>) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind failed: {e}"))?;
    let port = listener
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| format!("local_addr failed: {e}"))?;

    runtime
        .effective_port
        .store(port, std::sync::atomic::Ordering::Relaxed);

    let state = CoreServerState { runtime };
    let app = Router::new()
        .route("/health", get(health))
        .route("/invoke", post(invoke))
        .route("/events", get(events_sse))
        .with_state(state);

    log::info!("[core/server] listening on 127.0.0.1:{port}");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("[core/server] serve error: {e}");
        }
    });

    Ok(port)
}

async fn health(State(state): State<CoreServerState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "port": state.runtime.port(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn invoke(
    State(state): State<CoreServerState>,
    headers: HeaderMap,
    Json(body): Json<InvokeRequest>,
) -> Response {
    if !authorize(&state.runtime, &headers, None).await {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Forbidden".into(),
            }),
        )
            .into_response();
    }

    match state.runtime.invoke(&body.method, body.params).await {
        Ok(result) => (StatusCode::OK, Json(InvokeResponse { result })).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error }),
        )
            .into_response(),
    }
}

async fn events_sse(
    State(state): State<CoreServerState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if !authorize(&state.runtime, &headers, query.token.as_deref()).await {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    let broadcaster = state.runtime.broadcaster.clone();
    let mut rx_a = broadcaster.subscribe_a();
    let mut rx_b = broadcaster.subscribe_b();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                msg = rx_a.recv() => {
                    match msg {
                        Ok(m) => {
                            let envelope = serde_json::json!({
                                "event": m.event_name,
                                "payload": m.payload,
                                "seq": m.seq,
                                "run_id": m.run_id,
                            });
                            if tx.send(envelope.to_string()).is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("[core/sse] A-channel lagged by {n}");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                msg = rx_b.recv() => {
                    match msg {
                        Ok(m) => {
                            let envelope = serde_json::json!({
                                "event": m.event_name,
                                "payload": m.payload,
                                "seq": m.seq,
                                "run_id": m.run_id,
                            });
                            if tx.send(envelope.to_string()).is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("[core/sse] B-channel lagged by {n}");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    let sse_stream = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|data| {
            (
                Ok::<Event, std::convert::Infallible>(Event::default().data(data)),
                rx,
            )
        })
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn authorize(runtime: &CoreRuntime, headers: &HeaderMap, query_token: Option<&str>) -> bool {
    let expected = runtime.token().await;
    if expected.is_empty() {
        return false;
    }

    if let Some(qt) = query_token {
        if qt == expected {
            return true;
        }
    }

    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            if let Some(token) = s.strip_prefix("Bearer ") {
                return token == expected;
            }
        }
    }

    false
}
