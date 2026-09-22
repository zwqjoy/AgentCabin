//! DSH Code/Work actor over AgentCabin's ACP-shaped Harness bridge.
//!
//! Both surfaces now use the official DSH Web Harness and Session Controller
//! internally. The bridge keeps the native actor boundary stable while making
//! `session/selectModel` the shared next-request selection primitive.

use super::actor_loop::{prompt_content, render_work_prompt};
use super::normalizer::{acp_assistant_message_chunk, DshEventNormalizer};
use super::protocol::{parse_incoming, DshIncomingMessage, DshRpcRequest};
use crate::agent::adapter::ActorSessionMap;
use crate::agent::session_actor::ActorCommand;
use crate::models::RunStatus;
use crate::web_server::broadcaster::BroadcastEmitter;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct PendingMessage {
    text: String,
    attachments: Vec<crate::agent::session_actor::AttachmentData>,
    work_context_plan: Option<crate::work::context::WorkContextPlan>,
    reply: oneshot::Sender<Result<(), String>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    requested_session_id: Option<String>,
    tag: Arc<()>,
    mut child: Child,
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut cmd_rx: mpsc::Receiver<ActorCommand>,
    cancel: CancellationToken,
    shutdown_tx: oneshot::Sender<()>,
    mut desktop_runtime_token: Option<String>,
    mut work_bridge_token: Option<String>,
    cwd: String,
    mut model: String,
    effort: Option<String>,
) {
    let normalizer = DshEventNormalizer::new(Arc::clone(&emitter), run_id.clone(), &cwd);
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut next_id = 1_u64;
    let init_request_id = next_id;
    let mut open_request_id = None;
    let mut initialized = false;
    let mut session_ready = false;
    let mut session_id = None;
    let mut active_prompt_id = None;
    let mut cancel_requested = false;
    let mut pending_messages = VecDeque::new();
    let mut control_waiters: HashMap<u64, oneshot::Sender<Value>> = HashMap::new();
    let mut assistant_message_id = None;
    let mut assistant_text = String::new();
    let mut explicitly_stopped = false;
    let mut stderr_tail = VecDeque::new();

    if let Err(error) = send_request(
        &mut stdin,
        DshRpcRequest::new(
            init_request_id,
            "initialize",
            Some(json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false
                },
                "clientInfo": {
                    "name": "agentcabin",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        ),
    )
    .await
    {
        normalizer.fail_run(error);
        explicitly_stopped = true;
    } else {
        next_id += 1;
    }

    let cancel_signal = cancel.clone();
    while !explicitly_stopped {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(ActorCommand::SendMessage { text, attachments, work_context_plan, reply, .. }) => {
                        pending_messages.push_back(PendingMessage { text, attachments, work_context_plan, reply });
                        if let Err(error) = flush_pending(
                            &run_id,
                            &mut stdin,
                            session_id.as_deref(),
                            session_ready,
                            &mut active_prompt_id,
                            &mut pending_messages,
                            &mut next_id,
                            &normalizer,
                        ).await {
                            normalizer.fail_run(error);
                            explicitly_stopped = true;
                        }
                    }
                    Some(ActorCommand::SteerMessage { text, attachments, reply, .. }) => {
                        let pending = PendingMessage {
                            text: format!("[Steer] {text}"),
                            attachments,
                            work_context_plan: None,
                            reply,
                        };
                        if active_prompt_id.is_some() {
                            if !cancel_requested {
                                if let Err(error) = send_session_cancel(
                                    &mut stdin,
                                    session_id.as_deref().unwrap_or_default(),
                                ).await {
                                    let _ = pending.reply.send(Err(error.clone()));
                                    normalizer.fail_run(error);
                                    explicitly_stopped = true;
                                    continue;
                                }
                                cancel_requested = true;
                            }
                            pending_messages.push_front(pending);
                        } else {
                            pending_messages.push_front(pending);
                            if let Err(error) = flush_pending(
                                &run_id,
                                &mut stdin,
                                session_id.as_deref(),
                                session_ready,
                                &mut active_prompt_id,
                                &mut pending_messages,
                                &mut next_id,
                                &normalizer,
                            ).await {
                                normalizer.fail_run(error);
                                explicitly_stopped = true;
                            }
                        }
                    }
                    Some(ActorCommand::CancelTurn { reply }) => {
                        if let Some(session_id) = session_id.as_deref() {
                            if active_prompt_id.is_some() && !cancel_requested {
                                if let Err(error) = send_session_cancel(&mut stdin, session_id).await {
                                    let _ = reply.send(Err(error.clone()));
                                    normalizer.fail_run(error);
                                    explicitly_stopped = true;
                                    continue;
                                }
                                cancel_requested = true;
                            }
                            let _ = reply.send(Ok(()));
                        } else {
                            let _ = reply.send(Err("DSH ACP session is still initializing".to_string()));
                        }
                    }
                    Some(ActorCommand::Stop { reply, .. }) => {
                        if let Some(session_id) = session_id.as_deref() {
                            if active_prompt_id.is_some() {
                                let _ = send_session_cancel(&mut stdin, session_id).await;
                            }
                        }
                        fail_pending(&mut pending_messages, "DSH ACP session stopped");
                        explicitly_stopped = true;
                        let _ = reply.send(Ok(()));
                    }
                    Some(ActorCommand::SendControl { request, reply }) => {
                        let subtype = request.get("subtype").and_then(Value::as_str);
                        if matches!(subtype, Some("set_model") | Some("set_provider")) {
                            let Some(session_id) = session_id.as_deref() else {
                                let _ = reply.send(Err("DSH ACP session is still initializing".to_string()));
                                continue;
                            };
                            let selected_model = request
                                .get("model")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| "DSH set_model requires a model value".to_string());
                            match selected_model {
                                Ok(selected_model) => {
                                    let request_id = next_id;
                                    next_id += 1;
                                    if let Err(error) = send_request(
                                        &mut stdin,
                                        DshRpcRequest::new(
                                            request_id,
                                            "session/selectModel",
                                            Some(json!({
                                                "sessionId": session_id,
                                                "model": selected_model,
                                            })),
                                        ),
                                    )
                                    .await
                                    {
                                        let _ = reply.send(Err(error));
                                    } else {
                                        let persisted_model = selected_model
                                            .rsplit_once('/')
                                            .map(|(_, model)| model)
                                            .unwrap_or(selected_model);
                                        model = persisted_model.to_string();
                                        let _ = crate::storage::runs::update_run_model(&run_id, selected_model);
                                        let (tx, rx) = oneshot::channel();
                                        let _ = tx.send(json!({"ok": true, "restart_required": false}));
                                        let _ = reply.send(Ok((format!("dsh_model_{request_id}"), rx)));
                                    }
                                }
                                Err(error) => { let _ = reply.send(Err(error)); }
                            }
                        } else if subtype == Some("set_permission_mode") {
                            let mode = request
                                .get("mode")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| "DSH set_permission_mode requires a mode value".to_string());
                            let preset = mode.and_then(|mode| match mode {
                                // DSH's native permission presets are a sandbox + approval
                                // bundle. `default` and `acceptEdits` both retain the
                                // workspace-write/ask safety boundary; full access maps to
                                // DSH's explicit danger-full-access preset.
                                "default" | "acceptEdits" => Ok("workspace-write"),
                                "bypassPermissions" | "dontAsk" => Ok("danger-full-access"),
                                other => Err(format!("Unsupported DSH permission mode: {other}")),
                            });
                            match preset {
                                Ok(preset) => {
                                    if let Some(session_id) = session_id.as_deref() {
                                        let request_id = next_id;
                                        next_id += 1;
                                        let (response_tx, response_rx) = oneshot::channel();
                                        control_waiters.insert(request_id, response_tx);
                                        let result = send_request(
                                            &mut stdin,
                                            DshRpcRequest::new(
                                                request_id,
                                                "session/set_config_option",
                                                Some(json!({
                                                    "sessionId": session_id,
                                                    "configId": "permission_preset",
                                                    "value": preset,
                                                })),
                                            ),
                                        ).await;
                                        match result {
                                            Ok(()) => {
                                                let _ = reply.send(Ok((format!("dsh_permission_{request_id}"), response_rx)));
                                            }
                                            Err(error) => {
                                                control_waiters.remove(&request_id);
                                                let _ = reply.send(Err(error));
                                            }
                                        }
                                    } else {
                                        let _ = reply.send(Err("DSH ACP session is still initializing".to_string()));
                                    }
                                }
                                Err(error) => { let _ = reply.send(Err(error)); }
                            }
                        } else if subtype == Some("set_effort") {
                            let value = request
                                .get("effort")
                                .or_else(|| request.get("level"))
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| "DSH set_effort requires an effort value".to_string());
                            match value {
                                Ok(value) => {
                                    if let Some(session_id) = session_id.as_deref() {
                                        let request_id = next_id;
                                        next_id += 1;
                                        let (response_tx, response_rx) = oneshot::channel();
                                        control_waiters.insert(request_id, response_tx);
                                        let result = send_request(
                                            &mut stdin,
                                            DshRpcRequest::new(
                                                request_id,
                                                "session/set_config_option",
                                                Some(json!({
                                                    "sessionId": session_id,
                                                    "configId": "reasoning_effort",
                                                    "value": value,
                                                })),
                                            ),
                                        ).await;
                                        match result {
                                            Ok(()) => {
                                                let _ = reply.send(Ok((format!("dsh_effort_{request_id}"), response_rx)));
                                            }
                                            Err(error) => {
                                                control_waiters.remove(&request_id);
                                                let _ = reply.send(Err(error));
                                            }
                                        }
                                    } else {
                                        let _ = reply.send(Err("DSH ACP session is still initializing".to_string()));
                                    }
                                }
                                Err(error) => { let _ = reply.send(Err(error)); }
                            }
                        } else {
                            let _ = reply.send(Err(format!("Unsupported DSH control subtype: {}", subtype.unwrap_or(""))));
                        }
                    }
                    Some(_) => {
                        log::debug!("[dsh_acp_actor] unhandled command received");
                    }
                    None => {
                        fail_pending(&mut pending_messages, "DSH ACP actor mailbox closed");
                        explicitly_stopped = true;
                    }
                }
            }
            _ = cancel_signal.cancelled() => {
                if let Some(session_id) = session_id.as_deref() {
                    if active_prompt_id.is_some() {
                        let _ = send_session_cancel(&mut stdin, session_id).await;
                    }
                }
                fail_pending(&mut pending_messages, "DSH ACP session cancelled");
                explicitly_stopped = true;
            }
            line_res = stdout_lines.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let Ok(msg) = parse_incoming(&line) else { continue; };
                        match msg {
                            DshIncomingMessage::Notification(notif) => {
                                if notif.method == "session/update" {
                                    if let Some((message_id, delta)) = acp_assistant_message_chunk(notif.params.as_ref()) {
                                        if assistant_message_id.as_deref() != Some(message_id.as_str()) {
                                            assistant_message_id = Some(message_id);
                                            assistant_text.clear();
                                        }
                                        assistant_text.push_str(&delta);
                                    }
                                }
                                normalizer.handle_notification(&notif.method, notif.params.as_ref());
                            }
                            DshIncomingMessage::Request(request) => {
                                let result = if request.method == "session/request_permission" {
                                    json!({"outcome": {"outcome": "cancelled"}})
                                } else {
                                    json!({"error": {"code": -32601, "message": format!("AgentCabin Work does not expose ACP client method '{}'.", request.method)}})
                                };
                                if let Err(error) = send_json_response(&mut stdin, request.id, result).await {
                                    normalizer.fail_run(error);
                                    explicitly_stopped = true;
                                }
                            }
                            DshIncomingMessage::Response(response) => {
                                let Some(response_id) = response.id else { continue; };
                                if let Some(waiter) = control_waiters.remove(&response_id) {
                                    let value = if let Some(error) = response.error {
                                        json!({
                                            "success": false,
                                            "error": {"message": error.message, "code": error.code},
                                        })
                                    } else {
                                        json!({
                                            "success": true,
                                            "result": response.result.unwrap_or_else(|| json!({})),
                                        })
                                    };
                                    let _ = waiter.send(value);
                                    continue;
                                }
                                if response_id == init_request_id && !initialized {
                                    if let Some(error) = response.error {
                                        normalizer.fail_run(format!("DSH ACP initialize failed: {}", error.message));
                                        explicitly_stopped = true;
                                        continue;
                                    }
                                    initialized = true;
                                    let method = if requested_session_id.is_some() { "session/resume" } else { "session/new" };
                                    let params = if let Some(existing) = requested_session_id.as_deref() {
                                        json!({"sessionId": existing, "cwd": cwd, "mcpServers": []})
                                    } else {
                                        json!({"cwd": cwd, "mcpServers": []})
                                    };
                                    let request_id = next_id;
                                    next_id += 1;
                                    open_request_id = Some(request_id);
                                    if let Err(error) = send_request(&mut stdin, DshRpcRequest::new(request_id, method, Some(params))).await {
                                        normalizer.fail_run(error);
                                        explicitly_stopped = true;
                                    }
                                    continue;
                                }
                                if open_request_id == Some(response_id) {
                                    open_request_id = None;
                                    if let Some(error) = response.error.as_ref() {
                                        normalizer.fail_run(format!("DSH ACP session open failed: {}", error.message));
                                        explicitly_stopped = true;
                                        continue;
                                    }
                                    session_id = response
                                        .result
                                        .as_ref()
                                        .and_then(|result| result.get("sessionId"))
                                        .and_then(Value::as_str)
                                        .map(str::to_string)
                                        .or(requested_session_id.clone());
                                    let Some(session_id_value) = session_id.as_deref() else {
                                        normalizer.fail_run("DSH ACP session/new did not return a sessionId".to_string());
                                        explicitly_stopped = true;
                                        continue;
                                    };
                                    let _ = crate::storage::runs::update_session_id(&run_id, session_id_value);
                                    session_ready = true;
                                    let _ = crate::storage::runs::update_status(&run_id, RunStatus::Running, None, None);
                                    if let Some(value) = effort.as_deref() {
                                        let request_id = next_id;
                                        next_id += 1;
                                        if let Err(error) = send_request(
                                            &mut stdin,
                                            DshRpcRequest::new(
                                                request_id,
                                                "session/set_config_option",
                                                Some(json!({
                                                    "sessionId": session_id_value,
                                                    "configId": "reasoning_effort",
                                                    "value": value,
                                                })),
                                            ),
                                        ).await {
                                            log::warn!("[dsh_acp_actor] failed to apply initial effort: {error}");
                                        }
                                    }
                                    if let Err(error) = flush_pending(
                                        &run_id,
                                        &mut stdin,
                                        Some(session_id_value),
                                        true,
                                        &mut active_prompt_id,
                                        &mut pending_messages,
                                        &mut next_id,
                                        &normalizer,
                                    ).await {
                                        normalizer.fail_run(error);
                                        explicitly_stopped = true;
                                    } else if active_prompt_id.is_none() {
                                        let _ = crate::storage::runs::update_status(&run_id, RunStatus::Idle, None, None);
                                        normalizer.emit_run_state("idle", None);
                                    }
                                    continue;
                                }
                                if active_prompt_id != Some(response_id) {
                                    log::debug!("[dsh_acp_actor] response for unknown request id {response_id}");
                                    continue;
                                }
                                active_prompt_id = None;
                                let was_cancelled = cancel_requested;
                                cancel_requested = false;
                                if !assistant_text.is_empty() {
                                    normalizer.emit_assistant_complete(
                                        assistant_message_id.as_deref().unwrap_or("dsh-acp-assistant"),
                                        &std::mem::take(&mut assistant_text),
                                        Some(model.clone()),
                                    );
                                }
                                assistant_message_id = None;
                                if let Some(error) = response.error {
                                    if !was_cancelled && error.code != -32800 {
                                        normalizer.fail_run(format!("DSH ACP prompt failed: {}", error.message));
                                        explicitly_stopped = true;
                                        continue;
                                    }
                                }
                                let _ = crate::storage::runs::update_status(&run_id, RunStatus::Idle, None, None);
                                normalizer.emit_run_state("idle", None);
                                if let Err(error) = flush_pending(
                                    &run_id,
                                    &mut stdin,
                                    session_id.as_deref(),
                                    session_ready,
                                    &mut active_prompt_id,
                                    &mut pending_messages,
                                    &mut next_id,
                                    &normalizer,
                                ).await {
                                    normalizer.fail_run(error);
                                    explicitly_stopped = true;
                                }
                            }
                            DshIncomingMessage::Raw(value) => {
                                log::debug!("[dsh_acp_actor] raw message: {value:?}");
                            }
                        }
                    }
                    Ok(None) => {
                        if !explicitly_stopped {
                            // stdout and stderr can close in either order. Drain
                            // the remaining diagnostic lines briefly so a Web
                            // Harness boot failure is visible to the user rather
                            // than being reduced to an unhelpful exit status.
                            loop {
                                match tokio::time::timeout(
                                    std::time::Duration::from_millis(100),
                                    stderr_lines.next_line(),
                                ).await {
                                    Ok(Ok(Some(line))) => push_stderr(&mut stderr_tail, line),
                                    _ => break,
                                }
                            }
                            let status = child.wait().await.ok();
                            let mut detail = status
                                .map(|status| format!("DSH ACP process exited before completing the session ({status})"))
                                .unwrap_or_else(|| "DSH ACP stdout closed before completing the session".to_string());
                            if !stderr_tail.is_empty() {
                                detail.push_str(": ");
                                detail.push_str(&stderr_tail.into_iter().collect::<Vec<_>>().join(" | "));
                            }
                            normalizer.fail_run(detail);
                        }
                        break;
                    }
                    Err(error) => {
                        normalizer.fail_run(format!("DSH ACP stdout failed: {error}"));
                        explicitly_stopped = true;
                    }
                }
            }
            err_line = stderr_lines.next_line() => {
                match err_line {
                    Ok(Some(line)) if !line.trim().is_empty() => {
                        log::warn!("[dsh_acp_actor stderr] {line}");
                        push_stderr(&mut stderr_tail, line);
                    }
                    Ok(None) | Err(_) => {}
                    _ => {}
                }
            }
        }
    }

    if explicitly_stopped {
        let _ = child.start_kill();
    }
    let _ = child.wait().await;
    fail_pending(&mut pending_messages, "DSH ACP actor stopped");

    {
        let mut map = sessions.lock().await;
        if let Some(handle) = map.get(&run_id) {
            if Arc::ptr_eq(&handle.tag, &tag) {
                map.remove(&run_id);
            }
        }
    }

    if let Some(token) = desktop_runtime_token.take() {
        crate::desktop_runtime::revoke_token(&token).await;
    }
    if let Some(token) = work_bridge_token.take() {
        crate::work::internal_bridge::revoke_session_token(&token).await;
    }
    let _ = shutdown_tx.send(());
}

fn push_stderr(lines: &mut VecDeque<String>, line: String) {
    const MAX_LINES: usize = 12;
    const MAX_LINE_CHARS: usize = 500;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let sanitized = trimmed.chars().take(MAX_LINE_CHARS).collect::<String>();
    if lines.len() == MAX_LINES {
        lines.pop_front();
    }
    lines.push_back(sanitized);
}

async fn flush_pending(
    run_id: &str,
    stdin: &mut ChildStdin,
    session_id: Option<&str>,
    session_ready: bool,
    active_prompt_id: &mut Option<u64>,
    pending_messages: &mut VecDeque<PendingMessage>,
    next_id: &mut u64,
    normalizer: &DshEventNormalizer,
) -> Result<(), String> {
    if !session_ready || active_prompt_id.is_some() {
        return Ok(());
    }
    let Some(message) = pending_messages.pop_front() else {
        return Ok(());
    };
    let Some(session_id) = session_id else {
        let _ = message
            .reply
            .send(Err("DSH ACP session is not ready".to_string()));
        return Ok(());
    };
    normalizer.begin_turn();
    normalizer.emit_user_message(&message.text);
    let prompt = match render_work_prompt(&message.text, message.work_context_plan.as_ref()) {
        Ok(prompt) => prompt,
        Err(error) => {
            let _ = message.reply.send(Err(error.clone()));
            return Err(error);
        }
    };
    let content = prompt_content(run_id, &prompt, &message.attachments)?;
    match send_prompt(stdin, session_id, content, next_id).await {
        Ok(id) => {
            *active_prompt_id = Some(id);
            let _ = crate::storage::runs::update_status(
                normalizer.run_id(),
                RunStatus::Running,
                None,
                None,
            );
            normalizer.emit_run_state("running", None);
            let _ = message.reply.send(Ok(()));
            Ok(())
        }
        Err(error) => {
            let _ = message.reply.send(Err(error.clone()));
            Err(error)
        }
    }
}

async fn send_prompt(
    stdin: &mut ChildStdin,
    session_id: &str,
    content: Vec<Value>,
    next_id: &mut u64,
) -> Result<u64, String> {
    let id = *next_id;
    *next_id += 1;
    send_request(
        stdin,
        DshRpcRequest::new(
            id,
            "session/prompt",
            Some(json!({
                "sessionId": session_id,
                "prompt": content,
            })),
        ),
    )
    .await?;
    Ok(id)
}

async fn send_session_cancel(stdin: &mut ChildStdin, session_id: &str) -> Result<(), String> {
    send_json_line(
        stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": {"sessionId": session_id},
        }),
    )
    .await
}

async fn send_request(stdin: &mut ChildStdin, request: DshRpcRequest) -> Result<(), String> {
    let value = serde_json::to_value(request)
        .map_err(|error| format!("Failed to encode DSH ACP request: {error}"))?;
    send_json_line(stdin, value).await
}

async fn send_json_response(stdin: &mut ChildStdin, id: Value, value: Value) -> Result<(), String> {
    let response = if value.get("error").is_some() {
        json!({"jsonrpc": "2.0", "id": id, "error": value["error"]})
    } else {
        json!({"jsonrpc": "2.0", "id": id, "result": value})
    };
    send_json_line(stdin, response).await
}

async fn send_json_line(stdin: &mut ChildStdin, value: Value) -> Result<(), String> {
    let line = serde_json::to_string(&value)
        .map_err(|error| format!("Failed to encode DSH ACP message: {error}"))?;
    stdin
        .write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|error| format!("Failed to send DSH ACP message: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Failed to flush DSH ACP message: {error}"))
}

fn fail_pending(pending_messages: &mut VecDeque<PendingMessage>, error: &str) {
    while let Some(message) = pending_messages.pop_front() {
        let _ = message.reply.send(Err(error.to_string()));
    }
}
