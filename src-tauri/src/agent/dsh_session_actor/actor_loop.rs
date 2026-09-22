//! DSH JSON-RPC actor loop.

use super::normalizer::DshEventNormalizer;
use super::protocol::{parse_incoming, DshIncomingMessage, DshRpcRequest};
use crate::agent::adapter::ActorSessionMap;
use crate::agent::session_actor::save_attachment_to_disk;
use crate::agent::session_actor::ActorCommand;
use crate::models::RunStatus;
use crate::web_server::broadcaster::BroadcastEmitter;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Build ACP prompt content for DSH. DSH uses the same base64-backed image
/// block shape as the other ACP session actor integrations.
pub(super) fn prompt_content(
    run_id: &str,
    text: &str,
    attachments: &[crate::agent::session_actor::AttachmentData],
) -> Result<Vec<Value>, String> {
    let mut content = vec![json!({"type": "text", "text": text})];
    let mut file_paths = Vec::new();
    for attachment in attachments {
        if !attachment.media_type.starts_with("image/") {
            if let Some(path) = save_attachment_to_disk(run_id, attachment) {
                file_paths.push(format!("- {} ({})", path, attachment.filename));
            }
            continue;
        }
        if attachment.content_base64.trim().is_empty() {
            return Err(format!(
                "DSH image attachment '{}' has no data",
                attachment.filename
            ));
        }
        content.push(json!({
            "type": "image",
            "data": attachment.content_base64,
            "mimeType": attachment.media_type,
        }));
    }
    if !file_paths.is_empty() {
        content[0]["text"] = json!(format!(
            "{}\n\n[Attached files saved at local paths. Use the appropriate built-in Skill to read them.]\n{}",
            text,
            file_paths.join("\n")
        ));
    }
    Ok(content)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    session_id: String,
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
    provider: String,
    model: String,
    effort: Option<String>,
) {
    let normalizer = DshEventNormalizer::new(Arc::clone(&emitter), run_id.clone(), &cwd);
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut next_id: u64 = 1;

    // Send initialize handshake to DSH SDK
    let mut init_params = json!({
        "cwd": cwd,
        "provider": provider,
        "model": model,
    });
    if let Some(value) = effort.as_deref() {
        init_params["reasoningEffort"] = json!(value);
    }
    let init_req = DshRpcRequest::new(next_id, "initialize", Some(init_params));
    next_id += 1;
    if let Ok(line) = serde_json::to_string(&init_req) {
        let _ = stdin.write_all(format!("{line}\n").as_bytes()).await;
        let _ = stdin.flush().await;
    }

    let cancel_signal = cancel.clone();
    let mut explicitly_stopped = false;
    let mut initialized = false;
    // `session/prompt` returns an admission receipt immediately. Its response
    // does not mean that the agent turn has finished; the matching
    // `session/status: idle` notification is the turn boundary we can use for
    // cancel/steer and for the next follow-up.
    let mut active_prompt_id: Option<u64> = None;
    let mut cancel_request_ids = HashSet::new();
    let mut pending_messages = Vec::new();

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(ActorCommand::SendMessage { text, attachments, work_context_plan, reply, .. }) => {
                        if initialized {
                            normalizer.begin_turn();
                            normalizer.emit_user_message(&text);
                            let prompt = match render_work_prompt(&text, work_context_plan.as_ref()) {
                                Ok(prompt) => prompt,
                                Err(error) => {
                                    let _ = reply.send(Err(error.clone()));
                                    normalizer.fail_run(error);
                                    break;
                                }
                            };
                            let content = match prompt_content(&run_id, &prompt, &attachments) {
                                Ok(content) => content,
                                Err(error) => { let _ = reply.send(Err(error)); continue; }
                            };
                            match send_prompt(&mut stdin, &session_id, content, &mut next_id).await {
                                Ok(id) => {
                                    active_prompt_id = Some(id);
                                    let _ = reply.send(Ok(()));
                                }
                                Err(error) => {
                                    let _ = reply.send(Err(error.clone()));
                                    normalizer.fail_run(error);
                                    break;
                                }
                            }
                        } else {
                            pending_messages.push((text, attachments, work_context_plan, reply));
                        }
                    }
                    Some(ActorCommand::SteerMessage { text, attachments, reply, .. }) => {
                        if initialized {
                            if let Some(active_id) = active_prompt_id.take() {
                                if let Err(error) = send_cancel_request(
                                    &mut stdin,
                                    active_id,
                                    &mut next_id,
                                    &mut cancel_request_ids,
                                )
                                .await
                                {
                                    let _ = reply.send(Err(error.clone()));
                                    normalizer.fail_run(error);
                                    break;
                                }
                            }
                            normalizer.emit_user_message(&format!("[Steer] {text}"));
                            let content = match prompt_content(&run_id, &text, &attachments) {
                                Ok(content) => content,
                                Err(error) => { let _ = reply.send(Err(error)); continue; }
                            };
                            match send_prompt(&mut stdin, &session_id, content, &mut next_id).await {
                                Ok(id) => {
                                    active_prompt_id = Some(id);
                                    let _ = reply.send(Ok(()));
                                }
                                Err(error) => {
                                    let _ = reply.send(Err(error.clone()));
                                    normalizer.fail_run(error);
                                    break;
                                }
                            }
                        } else {
                            pending_messages.push((format!("[Steer] {text}"), attachments, None, reply));
                        }
                    }
                    Some(ActorCommand::CancelTurn { reply }) => {
                        if !initialized {
                            let _ = reply.send(Err("DSH session is still initializing".to_string()));
                            continue;
                        }
                        if let Some(active_id) = active_prompt_id.take() {
                            if let Err(error) = send_cancel_request(
                                &mut stdin,
                                active_id,
                                &mut next_id,
                                &mut cancel_request_ids,
                            )
                            .await
                            {
                                let _ = reply.send(Err(error.clone()));
                                normalizer.fail_run(error);
                                break;
                            }
                        }
                        // A turn cancel is not a session stop. The DSH child
                        // remains alive and can receive the next follow-up.
                        normalizer.emit_run_state("idle", None);
                        let _ = crate::storage::runs::update_status(
                            &run_id,
                            RunStatus::Idle,
                            None,
                            None,
                        );
                        let _ = reply.send(Ok(()));
                    }
                    Some(ActorCommand::Stop { reply, .. }) => {
                        if let Some(active_id) = active_prompt_id.take() {
                            let _ = send_cancel_request(
                                &mut stdin,
                                active_id,
                                &mut next_id,
                                &mut cancel_request_ids,
                            )
                            .await;
                        }
                        explicitly_stopped = true;
                        let _ = reply.send(Ok(()));
                        break;
                    }
                    Some(ActorCommand::SendControl { request, reply }) => {
                        let subtype = request.get("subtype").and_then(|value| value.as_str());
                        if subtype == Some("set_effort") {
                            let value = request
                                .get("effort")
                                .and_then(|value| value.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| "DSH set_effort requires an effort value".to_string());
                            match value {
                                Ok(value) => {
                                    if let Err(error) = crate::storage::runs::update_run_effort(&run_id, value) {
                                        let _ = reply.send(Err(format!("Failed to persist DSH effort: {error}")));
                                    } else {
                                        // The SDK protocol has no live set-effort method. Persisting
                                        // here makes the next DSH session use the selected level.
                                        let (tx, rx) = oneshot::channel();
                                        let _ = tx.send(json!({"ok": true, "restart_required": true}));
                                        let _ = reply.send(Ok((format!("dsh_effort_{}", uuid::Uuid::new_v4()), rx)));
                                    }
                                }
                                Err(error) => { let _ = reply.send(Err(error)); }
                            }
                        } else {
                            let _ = reply.send(Err(format!("Unsupported DSH control subtype: {}", subtype.unwrap_or(""))));
                        }
                    }
                    Some(_) => {
                        log::debug!("[dsh_actor] unhandled command received");
                    }
                    None => {
                        explicitly_stopped = true;
                        break;
                    }
                }
            }
            _ = cancel_signal.cancelled() => {
                if let Some(active_id) = active_prompt_id.take() {
                    let _ = send_cancel_request(
                        &mut stdin,
                        active_id,
                        &mut next_id,
                        &mut cancel_request_ids,
                    )
                    .await;
                }
                explicitly_stopped = true;
                break;
            }
            line_res = stdout_lines.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        if let Ok(msg) = parse_incoming(&line) {
                            match msg {
                                DshIncomingMessage::Notification(notif) => {
                                    if notif.method == "session/status" {
                                        if let Some(st) = notif.params.as_ref().and_then(|p| p.get("status")).and_then(serde_json::Value::as_str) {
                                            if st == "idle" || st == "completed" {
                                                active_prompt_id = None;
                                            }
                                        }
                                    }
                                    normalizer.handle_notification(&notif.method, notif.params.as_ref());
                                }
                                DshIncomingMessage::Response(resp) => {
                                    if resp
                                        .id
                                        .map(|id| cancel_request_ids.remove(&id))
                                        .unwrap_or(false)
                                    {
                                        // DSH may acknowledge cancellation with
                                        // either a result or an error. Neither is
                                        // a failed Work run.
                                        continue;
                                    }
                                    if let Some(err) = resp.error {
                                        normalizer.fail_run(err.message);
                                        break;
                                    }
                                    if resp.id == Some(1) && !initialized {
                                        initialized = true;
                                        let _ = crate::storage::runs::update_status(
                                            &run_id,
                                            RunStatus::Running,
                                            None,
                                            None,
                                        );
                                        normalizer.emit_run_state("running", None);
                                        for (text, attachments, work_context_plan, reply) in pending_messages.drain(..) {
                                            normalizer.begin_turn();
                                            normalizer.emit_user_message(&text);
                                            let prompt = match render_work_prompt(&text, work_context_plan.as_ref()) {
                                                Ok(prompt) => prompt,
                                                Err(error) => {
                                                    let _ = reply.send(Err(error.clone()));
                                                    normalizer.fail_run(error);
                                                    explicitly_stopped = true;
                                                    break;
                                                }
                                            };
                                            let content = match prompt_content(&run_id, &prompt, &attachments) {
                                                Ok(content) => content,
                                                Err(error) => { let _ = reply.send(Err(error)); explicitly_stopped = true; break; }
                                            };
                                            match send_prompt(&mut stdin, &session_id, content, &mut next_id).await {
                                                Ok(id) => {
                                                    active_prompt_id = Some(id);
                                                    let _ = reply.send(Ok(()));
                                                }
                                                Err(error) => {
                                                    let _ = reply.send(Err(error.clone()));
                                                    normalizer.fail_run(error);
                                                    explicitly_stopped = true;
                                                    break;
                                                }
                                            }
                                        }
                                        if explicitly_stopped {
                                            break;
                                        }
                                    }
                                }
                                DshIncomingMessage::Request(request) => {
                                    // The SDK profile is a client-facing JSON-RPC
                                    // stream. It has no Work-side handler for
                                    // server requests; answer explicitly instead
                                    // of leaving the peer blocked indefinitely.
                                    let response = json!({
                                        "jsonrpc": "2.0",
                                        "id": request.id,
                                        "error": {
                                            "code": -32601,
                                            "message": "DSH SDK actor does not accept server requests"
                                        }
                                    });
                                    if let Ok(line) = serde_json::to_string(&response) {
                                        if stdin.write_all(format!("{line}\n").as_bytes()).await.is_err()
                                            || stdin.flush().await.is_err()
                                        {
                                            normalizer.fail_run("Failed to answer DSH server request".to_string());
                                            explicitly_stopped = true;
                                            break;
                                        }
                                    }
                                }
                                DshIncomingMessage::Raw(val) => {
                                    log::debug!("[dsh_actor] raw message: {val:?}");
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        if !explicitly_stopped {
                            let status = child.try_wait().ok().flatten();
                            let detail = status
                                .map(|status| format!("DSH SDK process exited before completing the session ({status})"))
                                .unwrap_or_else(|| "DSH SDK stdout closed before completing the session".to_string());
                            normalizer.fail_run(detail);
                        }
                        break;
                    },
                    Err(e) => {
                        log::warn!("[dsh_actor] stdout error: {e}");
                        normalizer.fail_run(format!("DSH SDK stdout failed: {e}"));
                        break;
                    }
                }
            }
            err_line = stderr_lines.next_line() => {
                match err_line {
                    Ok(Some(line)) if !line.trim().is_empty() => {
                        log::warn!("[dsh_actor stderr] {line}");
                    }
                    Ok(None) | Err(_) => {}
                    _ => {}
                }
            }
        }
    }

    if !explicitly_stopped {
        let _ = child.wait().await;
    } else {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    // Cleanup session map
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

async fn send_prompt(
    stdin: &mut ChildStdin,
    session_id: &str,
    content: Vec<Value>,
    next_id: &mut u64,
) -> Result<u64, String> {
    let req_id = *next_id;
    let request = DshRpcRequest::new(
        req_id,
        "session/prompt",
        Some(json!({
            "sessionId": session_id,
            "contentBlocks": content,
        })),
    );
    *next_id += 1;
    let line = serde_json::to_string(&request)
        .map_err(|error| format!("Failed to encode DSH prompt: {error}"))?;
    stdin
        .write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|error| format!("Failed to send prompt to DSH SDK: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Failed to flush prompt to DSH SDK: {error}"))?;
    Ok(req_id)
}

async fn send_cancel_request(
    stdin: &mut ChildStdin,
    prompt_id: u64,
    next_id: &mut u64,
    cancel_request_ids: &mut HashSet<u64>,
) -> Result<(), String> {
    let request_id = *next_id;
    *next_id += 1;
    let request = DshRpcRequest::new(
        request_id,
        "$/cancelRequest",
        Some(json!({ "id": prompt_id })),
    );
    let line = serde_json::to_string(&request)
        .map_err(|error| format!("Failed to encode DSH cancel request: {error}"))?;
    stdin
        .write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|error| format!("Failed to send DSH cancel request: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Failed to flush DSH cancel request: {error}"))?;
    cancel_request_ids.insert(request_id);
    Ok(())
}

pub(super) fn render_work_prompt(
    text: &str,
    plan: Option<&crate::work::context::WorkContextPlan>,
) -> Result<String, String> {
    let Some(plan) = plan else {
        return Ok(text.to_string());
    };
    if plan.runtime != crate::agent::capability_resolver::RuntimeProviderKind::Dsh {
        return Err(format!(
            "DSH actor received a Work Context Plan for '{}'",
            plan.runtime.as_str()
        ));
    }
    let rendered = plan.render_system_prompt();
    if rendered.trim().is_empty() {
        return Ok(text.to_string());
    }
    Ok(format!("{text}\n\n## Active Work Context Plan\n{rendered}"))
}
