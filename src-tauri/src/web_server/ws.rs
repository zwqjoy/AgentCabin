use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::web_server::auth;
use crate::web_server::dispatch;
use crate::web_server::state::AppState;

/// Max events to buffer during replay before triggering _full_reload
const REPLAY_BUFFER_CAPACITY: usize = 4096;
/// Minimum interval between _full_reload signals per run (seconds)
const FULL_RELOAD_COOLDOWN_SECS: u64 = 30;

type WsSink = Arc<Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>;

#[derive(Deserialize, Default)]
pub struct WsQuery {
    token: Option<String>,
}

/// WebSocket upgrade handler with authentication.
/// Extracts auth info from headers (cookie) and query params (token) before upgrade.
pub async fn ws_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let auth_subject = auth::authenticate_ws(&state, &headers, query.token.as_deref()).await;
    let Some(auth_subject) = auth_subject else {
        log::debug!("[ws] upgrade rejected: auth failed");
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    };
    log::debug!("[ws] upgrade accepted");
    ws.on_upgrade(move |socket| handle_ws(socket, state, auth_subject))
}

/// Buffered event during replay — stored until replay completes so we can merge
#[derive(Debug, Clone)]
struct BufferedEvent {
    envelope: Value,
    seq: u64,
}

/// Per-connection WS session state
struct WsSession {
    /// run_id → last_seq checkpoint for each subscribed run
    subscriptions: HashMap<String, u64>,
    /// run_id → buffered events received during active replay
    replay_buffer: HashMap<String, Vec<BufferedEvent>>,
    /// run_id set currently being replayed (reentry guard)
    replaying: std::collections::HashSet<String>,
    /// Timestamp of last _full_reload signal per run_id (cooldown)
    full_reload_cooldown: HashMap<String, std::time::Instant>,
}

impl WsSession {
    fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
            replay_buffer: HashMap::new(),
            replaying: std::collections::HashSet::new(),
            full_reload_cooldown: HashMap::new(),
        }
    }
}

/// Send an RPC result response over WebSocket
async fn send_result(ws_tx: &WsSink, id: &Option<String>, result: Value) {
    let resp = json!({"id": id, "result": result});
    let _ = ws_tx
        .lock()
        .await
        .send(Message::Text(resp.to_string()))
        .await;
}

/// Send an RPC error response over WebSocket
async fn send_error(ws_tx: &WsSink, id: &Option<String>, error: &str) {
    let resp = json!({"id": id, "error": error});
    let _ = ws_tx
        .lock()
        .await
        .send(Message::Text(resp.to_string()))
        .await;
}

/// Handle a single WebSocket connection
async fn handle_ws(socket: WebSocket, state: AppState, auth_subject: auth::WsAuthSubject) {
    let (ws_tx, mut ws_rx) = socket.split();
    let session = Arc::new(Mutex::new(WsSession::new()));

    // Subscribe to broadcast channels
    let mut a_rx = state.broadcaster.subscribe_a();
    let mut b_rx = state.broadcaster.subscribe_b();

    // Shared write access for sending from multiple tasks
    let ws_tx: WsSink = Arc::new(Mutex::new(ws_tx));

    let ws_tx_a = ws_tx.clone();
    let ws_tx_b = ws_tx.clone();
    let session_a = session.clone();
    let session_b = session.clone();
    let state_for_read = state.clone();

    // Task: forward A-class broadcast events to WS client
    let a_forward = tokio::spawn(async move {
        loop {
            match a_rx.recv().await {
                Ok(msg) => {
                    let Some(run_id) = &msg.run_id else { continue };
                    let Some(seq) = msg.seq else { continue };

                    let mut sess = session_a.lock().await;

                    // Check if client is subscribed to this run
                    let Some(&checkpoint) = sess.subscriptions.get(run_id) else {
                        continue;
                    };

                    // If replay is in progress, buffer the event
                    if sess.replaying.contains(run_id) {
                        let buf = sess
                            .replay_buffer
                            .entry(run_id.clone())
                            .or_insert_with(Vec::new);
                        if buf.len() < REPLAY_BUFFER_CAPACITY {
                            let envelope = json!({
                                "event": msg.event_name,
                                "seq": seq,
                                "run_id": run_id,
                                "payload": msg.payload,
                            });
                            buf.push(BufferedEvent { envelope, seq });
                            log::trace!(
                                "[ws] a_forward: buffered event seq={} for run={} (replay in progress)",
                                seq,
                                run_id
                            );
                        } else {
                            log::warn!(
                                "[ws] a_forward: replay buffer overflow for run={}, will trigger _full_reload",
                                run_id
                            );
                            // Mark overflow — will be handled after replay completes
                        }
                        continue;
                    }

                    // Normal path: skip if already sent
                    if seq <= checkpoint {
                        continue;
                    }
                    drop(sess);

                    // Build envelope (include run_id for frontend checkpoint tracking)
                    let envelope = json!({
                        "event": msg.event_name,
                        "seq": seq,
                        "run_id": run_id,
                        "payload": msg.payload,
                    });

                    let text = envelope.to_string();
                    if ws_tx_a
                        .lock()
                        .await
                        .send(Message::Text(text))
                        .await
                        .is_err()
                    {
                        break; // Client disconnected
                    }

                    // Update checkpoint
                    let mut sess = session_a.lock().await;
                    if let Some(cp) = sess.subscriptions.get_mut(run_id) {
                        *cp = (*cp).max(seq); // Monotonic update
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("[ws] A-channel lagged by {} events, triggering catchup", n);
                    // Catchup: replay from checkpoint for all subscribed runs
                    let mut sess = session_a.lock().await;
                    let subs: Vec<(String, u64)> = sess
                        .subscriptions
                        .iter()
                        .map(|(k, &v)| (k.clone(), v))
                        .collect();

                    // Mark only non-already-replaying runs (avoid conflicting replays)
                    let mut runs_to_replay = Vec::new();
                    for (run_id, last_seq) in &subs {
                        if !sess.replaying.contains(run_id.as_str()) {
                            sess.replaying.insert(run_id.clone());
                            sess.replay_buffer.entry(run_id.clone()).or_default();
                            runs_to_replay.push((run_id.clone(), *last_seq));
                        }
                    }
                    drop(sess);

                    for (run_id, last_seq) in &runs_to_replay {
                        if let Err(e) =
                            replay_events(&state_for_read, &ws_tx_a, &session_a, run_id, *last_seq)
                                .await
                        {
                            log::error!("[ws] catchup replay failed for run={}: {}", run_id, e);
                        }
                    }

                    // Flush buffers after replay
                    for (run_id, _) in &runs_to_replay {
                        if let Err(overflow) =
                            flush_replay_buffer(&ws_tx_a, &session_a, run_id).await
                        {
                            if overflow {
                                send_full_reload(&ws_tx_a, &session_a, run_id).await;
                            }
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Task: forward B-class broadcast events to WS client
    let b_forward = tokio::spawn(async move {
        loop {
            match b_rx.recv().await {
                Ok(msg) => {
                    // For run-scoped B events, check subscription
                    if let Some(run_id) = &msg.run_id {
                        let sess = session_b.lock().await;
                        if !sess.subscriptions.contains_key(run_id) {
                            continue; // Not subscribed
                        }
                        drop(sess);
                    }
                    // Global events (no run_id) are always sent

                    let envelope = json!({
                        "event": msg.event_name,
                        "payload": msg.payload,
                    });

                    let text = envelope.to_string();
                    if ws_tx_b
                        .lock()
                        .await
                        .send(Message::Text(text))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::debug!("[ws] B-channel lagged by {} events (expected, skipping)", n);
                    // B-class lag is expected and harmless — just continue
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Task: read client messages and dispatch commands
    let ws_tx_cmd = ws_tx.clone();
    let session_cmd = session.clone();
    let state_cmd = state.clone();

    let cmd_loop = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(text) => {
                    let text_str: &str = &text;
                    let parsed: Value = match serde_json::from_str(text_str) {
                        Ok(v) => v,
                        Err(e) => {
                            send_error(&ws_tx_cmd, &None, &format!("invalid JSON: {}", e)).await;
                            continue;
                        }
                    };

                    let id = parsed.get("id").and_then(|v| v.as_str()).map(String::from);
                    let method = parsed.get("method").and_then(|v| v.as_str()).unwrap_or("");
                    let params = parsed.get("params").cloned().unwrap_or(json!({}));

                    log::debug!("[ws] dispatch: method={}, id={:?}", method, id);

                    // Handle internal protocol methods
                    match method {
                        "_subscribe" => {
                            let run_id =
                                params.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
                            let last_seq =
                                params.get("last_seq").and_then(|v| v.as_u64()).unwrap_or(0);

                            if run_id.is_empty() {
                                send_error(&ws_tx_cmd, &id, "run_id required").await;
                                continue;
                            }

                            log::debug!(
                                "[ws] _subscribe: run_id={}, last_seq={}",
                                run_id,
                                last_seq
                            );

                            // Register subscription + mark replay in progress
                            let (already_replaying, effective_seq) = {
                                let mut sess = session_cmd.lock().await;
                                let already = sess.replaying.contains(run_id);
                                // Monotonic checkpoint: don't regress
                                let old_seq = sess.subscriptions.get(run_id).copied().unwrap_or(0);
                                let effective_seq = old_seq.max(last_seq);
                                sess.subscriptions.insert(run_id.to_string(), effective_seq);
                                if !already {
                                    sess.replaying.insert(run_id.to_string());
                                    sess.replay_buffer.entry(run_id.to_string()).or_default();
                                }
                                (already, effective_seq)
                            };

                            // Reentry: another _subscribe for the same run is already replaying
                            if already_replaying {
                                log::debug!(
                                    "[ws] _subscribe: already replaying run={}, checkpoint updated",
                                    run_id
                                );
                                send_result(
                                    &ws_tx_cmd,
                                    &id,
                                    json!({"status": "already_replaying"}),
                                )
                                .await;
                                continue;
                            }

                            // Replay events since effective_seq (not last_seq — avoids re-sending)
                            if let Err(e) = replay_events(
                                &state_cmd,
                                &ws_tx_cmd,
                                &session_cmd,
                                run_id,
                                effective_seq,
                            )
                            .await
                            {
                                log::error!("[ws] replay failed for run={}: {}", run_id, e);
                                // Clean up replay state so run isn't stuck
                                let mut sess = session_cmd.lock().await;
                                sess.replay_buffer.remove(run_id);
                                sess.replaying.remove(run_id);
                                drop(sess);
                                send_error(&ws_tx_cmd, &id, "replay failed").await;
                                continue;
                            }

                            // Flush buffer after replay
                            match flush_replay_buffer(&ws_tx_cmd, &session_cmd, run_id).await {
                                Err(true) => {
                                    // Buffer overflow — send _full_reload
                                    send_full_reload(&ws_tx_cmd, &session_cmd, run_id).await;
                                }
                                Err(false) => {
                                    log::error!(
                                        "[ws] flush_replay_buffer send error for run={}",
                                        run_id
                                    );
                                }
                                Ok(()) => {}
                            }

                            send_result(&ws_tx_cmd, &id, json!({"ok": true})).await;
                        }
                        "_unsubscribe" => {
                            let run_id =
                                params.get("run_id").and_then(|v| v.as_str()).unwrap_or("");

                            log::debug!("[ws] _unsubscribe: run_id={}", run_id);

                            let mut sess = session_cmd.lock().await;
                            sess.subscriptions.remove(run_id);
                            sess.replay_buffer.remove(run_id);
                            sess.replaying.remove(run_id);
                            sess.full_reload_cooldown.remove(run_id);

                            send_result(&ws_tx_cmd, &id, json!({"ok": true})).await;
                        }
                        _ => {
                            // Dispatch to command handler
                            let result =
                                dispatch::dispatch_command(method, params, &state_cmd).await;

                            match result {
                                Ok(value) => send_result(&ws_tx_cmd, &id, value).await,
                                Err(err) => send_error(&ws_tx_cmd, &id, &err).await,
                            }
                        }
                    }
                }
                Message::Close(_) => {
                    log::debug!("[ws] client sent close frame");
                    break;
                }
                _ => {} // Ignore binary, ping, pong
            }
        }
    });

    // Extract session_id for heartbeat expiry checking
    let ws_session_id = match &auth_subject {
        auth::WsAuthSubject::Session(sid) => Some(sid.clone()),
        auth::WsAuthSubject::QueryToken => None,
    };

    // Task: periodic heartbeat + token rotation + session expiry check
    let state_hb = state.clone();
    let ws_tx_hb = ws_tx.clone();

    let heartbeat_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        let session_token_ver = state_hb
            .token_version
            .load(std::sync::atomic::Ordering::Relaxed);
        let mut shutdown_rx = state_hb.ws_shutdown.subscribe();
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Check token version mismatch
                    let current_ver = state_hb
                        .token_version
                        .load(std::sync::atomic::Ordering::Relaxed);
                    if current_ver != session_token_ver {
                        log::debug!("[ws] heartbeat: token_version mismatch, closing");
                        let _ = ws_tx_hb
                            .lock()
                            .await
                            .send(Message::Close(Some(
                                axum::extract::ws::CloseFrame {
                                    code: 4401,
                                    reason: "token_version_mismatch".into(),
                                },
                            )))
                            .await;
                        return;
                    }
                    // Check session expiry (cookie-auth connections only)
                    if let Some(ref sid) = ws_session_id {
                        let expired = {
                            let sessions = state_hb.http_sessions.lock().await;
                            sessions
                                .get(sid.as_str())
                                .map(|e| chrono::Utc::now() >= e.expires_at)
                                .unwrap_or(true)
                        };
                        if expired {
                            log::debug!("[ws] heartbeat: session expired, closing");
                            let _ = ws_tx_hb
                                .lock()
                                .await
                                .send(Message::Close(Some(
                                    axum::extract::ws::CloseFrame {
                                        code: 4401,
                                        reason: "session_expired".into(),
                                    },
                                )))
                                .await;
                            return;
                        }
                    }
                    // Send ping keepalive
                    let _ = ws_tx_hb.lock().await.send(Message::Ping(vec![])).await;
                }
                _ = shutdown_rx.recv() => {
                    log::debug!("[ws] token rotated, closing connection");
                    let _ = ws_tx_hb
                        .lock()
                        .await
                        .send(Message::Close(Some(
                            axum::extract::ws::CloseFrame {
                                code: 4401,
                                reason: "token_rotated".into(),
                            },
                        )))
                        .await;
                    return;
                }
            }
        }
    });

    // Wait for any task to finish (client disconnect or channel close)
    tokio::select! {
        _ = a_forward => {},
        _ = b_forward => {},
        _ = cmd_loop => {},
        _ = heartbeat_task => {
            log::debug!("[ws] heartbeat/shutdown triggered connection close");
        },
    }

    log::debug!("[ws] connection closed");
}

/// Flush the replay buffer for a run: send buffered events with seq > checkpoint,
/// update checkpoint monotonically, then clear replay state.
///
/// Returns Ok(()) on success, Err(true) on buffer overflow, Err(false) on send failure.
async fn flush_replay_buffer(
    ws_tx: &WsSink,
    session: &Arc<Mutex<WsSession>>,
    run_id: &str,
) -> Result<(), bool> {
    let mut total_flushed = 0u64;
    let mut max_seq = 0u64;

    // Loop: each iteration drains the buffer while `replaying` stays true.
    // a_forward keeps buffering new events during sends (no out-of-order risk).
    // When buffer is empty, we atomically clear `replaying` in the same lock — no window.
    // Converges fast: each iteration's buffer is smaller (typically 2-3 rounds max).
    loop {
        let (buffer, checkpoint) = {
            let mut sess = session.lock().await;
            let buffer = sess.replay_buffer.remove(run_id).unwrap_or_default();

            // Empty → atomically clear replaying + update checkpoint and exit
            if buffer.is_empty() {
                sess.replaying.remove(run_id);
                if max_seq > 0 {
                    if let Some(cp) = sess.subscriptions.get_mut(run_id) {
                        *cp = (*cp).max(max_seq);
                    }
                }
                break;
            }

            // Overflow → clear replaying and signal full_reload
            if buffer.len() >= REPLAY_BUFFER_CAPACITY {
                sess.replaying.remove(run_id);
                log::debug!(
                    "[ws] flush_replay_buffer: overflow for run={}, buffer_len={}",
                    run_id,
                    buffer.len()
                );
                return Err(true);
            }

            let checkpoint = sess.subscriptions.get(run_id).copied().unwrap_or(0);
            (buffer, checkpoint)
        };
        // replaying still true → a_forward continues buffering, no out-of-order

        let effective_checkpoint = checkpoint.max(max_seq);
        for event in &buffer {
            if event.seq <= effective_checkpoint {
                continue;
            }
            let text = event.envelope.to_string();
            if ws_tx.lock().await.send(Message::Text(text)).await.is_err() {
                let mut sess = session.lock().await;
                sess.replaying.remove(run_id);
                return Err(false);
            }
            max_seq = max_seq.max(event.seq);
            total_flushed += 1;
        }

        // Update checkpoint after this batch (before next iteration's lock)
        {
            let mut sess = session.lock().await;
            if let Some(cp) = sess.subscriptions.get_mut(run_id) {
                *cp = (*cp).max(max_seq);
            }
        }
    }

    if total_flushed > 0 {
        log::debug!(
            "[ws] flush_replay_buffer: run={}, flushed={} events, new_checkpoint={}",
            run_id,
            total_flushed,
            max_seq
        );
    }

    Ok(())
}

/// Send a _full_reload event to the client, respecting cooldown.
async fn send_full_reload(ws_tx: &WsSink, session: &Arc<Mutex<WsSession>>, run_id: &str) {
    let mut sess = session.lock().await;

    // Check cooldown
    if let Some(last) = sess.full_reload_cooldown.get(run_id) {
        if last.elapsed().as_secs() < FULL_RELOAD_COOLDOWN_SECS {
            log::debug!(
                "[ws] _full_reload cooldown active for run={}, skipping",
                run_id
            );
            return;
        }
    }
    sess.full_reload_cooldown
        .insert(run_id.to_string(), std::time::Instant::now());
    drop(sess);

    let envelope = json!({
        "event": "_full_reload",
        "run_id": run_id,
    });
    log::debug!("[ws] sending _full_reload for run={}", run_id);
    let _ = ws_tx
        .lock()
        .await
        .send(Message::Text(envelope.to_string()))
        .await;
}

/// Replay A-class events from events.jsonl since `last_seq` for a given run.
async fn replay_events(
    _state: &AppState,
    ws_tx: &WsSink,
    session: &Arc<Mutex<WsSession>>,
    run_id: &str,
    last_seq: u64,
) -> Result<(), String> {
    // list_bus_events already filters by since_seq > last_seq
    let run_id_for_read = run_id.to_string();
    let events = tokio::task::spawn_blocking(move || {
        crate::storage::events::list_bus_events(&run_id_for_read, Some(last_seq))
    })
    .await
    .map_err(|error| format!("bus event replay task failed: {}", error))?;

    let replayed = events.len();
    for event in &events {
        let seq = event.get("_seq").and_then(|v| v.as_u64()).unwrap_or(0);

        let envelope = json!({
            "event": "bus-event",
            "seq": seq,
            "run_id": run_id,
            "payload": event,
        });

        let text = envelope.to_string();
        if ws_tx.lock().await.send(Message::Text(text)).await.is_err() {
            return Err("WS send failed during replay".to_string());
        }

        // Update checkpoint (monotonic)
        if seq > 0 {
            let mut sess = session.lock().await;
            if let Some(cp) = sess.subscriptions.get_mut(run_id) {
                *cp = (*cp).max(seq);
            }
        }
    }

    log::debug!(
        "[ws] replay: run_id={}, from_seq={}, replayed={} events",
        run_id,
        last_seq,
        replayed
    );

    Ok(())
}
