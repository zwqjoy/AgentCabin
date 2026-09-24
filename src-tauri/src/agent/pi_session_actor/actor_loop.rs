//! Pi RPC mailbox and process loop.

use super::events::{
    emit_user_message, handle_stdout_line, mark_failed, mark_idle, mark_running, pi_images,
};
use crate::agent::adapter::ActorSessionMap;
use crate::agent::pi_features::state::PiMessageQueueState;
use crate::agent::pi_rpc_protocol::{PiRpc, PiStreamingBehavior};
use crate::agent::session_actor::RalphCancelResult;
use crate::agent::session_actor::{save_attachment_to_disk, ActorCommand, AttachmentData};
use crate::agent::session_protocol::{LifecycleSignal, PendingKind, SessionProtocol};
use crate::models::{max_attachment_size, BusEvent, PiExtensionUiMethod, RalphCompleteReason};
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::models::AppMode;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct PiRalphLoop {
    prompt: String,
    work_context_plan: Option<crate::work::context::WorkContextPlan>,
    iteration: u32,
    max_iterations: u32,
    completion_promise: Option<String>,
    promise_matched: bool,
    cancel_pending: bool,
    consecutive_failures: u32,
}

async fn dispatch_ralph_prompt(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    protocol: &mut PiRpc,
    stdin: &mut ChildStdin,
    prompt: &str,
    work_context_plan: Option<&crate::work::context::WorkContextPlan>,
) -> Result<(), String> {
    let work_context = work_turn_context(run_id, work_context_plan)?;
    let effective_prompt = format!("{prompt}{work_context}");
    let frame = protocol.frame_prompt(&effective_prompt, &[], None);
    write_json_line(stdin, &frame).await?;
    emit_user_message(emitter, run_id, prompt, &[]);
    mark_running(emitter, run_id);
    Ok(())
}

/// Maximum consecutive auto-continuations when a work-mode turn is truncated
/// by the output-length limit. Beyond this, we stop and surface the problem
/// to the user instead of burning tokens silently.
const MAX_LENGTH_CONTINUATIONS: u32 = 3;

const LENGTH_CONTINUATION_PROMPT: &str = "[系统] 上一轮回复因达到输出长度上限被截断。请继续执行当前任务，并严格遵守：\
1) 所有数据计算、汇总、文件生成都必须通过 work_run_command（运行脚本） / work_write_file 等工具完成，禁止在思考中手工计算数字；\
2) 本轮输出保持简短，聚焦下一个工具调用；\
3) 复用 scratch/ 与 output/ 中已有的中间结果，不要重复读取全量数据。";

fn is_work_run(run_id: &str) -> bool {
    storage::runs::get_run(run_id).is_some_and(|run| run.app_mode == AppMode::Work)
}

/// Auto-continue a work-mode turn that was truncated by the output-length
/// limit, by injecting a follow-up prompt that steers the model toward using
/// tools instead of hand-computing in its head.
async fn dispatch_length_continuation(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    protocol: &mut PiRpc,
    stdin: &mut ChildStdin,
    work_context_plan: Option<&crate::work::context::WorkContextPlan>,
) -> Result<(), String> {
    let work_context = work_turn_context(run_id, work_context_plan)?;
    let effective_prompt = format!("{LENGTH_CONTINUATION_PROMPT}{work_context}");
    let frame = protocol.frame_prompt(&effective_prompt, &[], Some(PiStreamingBehavior::FollowUp));
    write_json_line(stdin, &frame).await?;
    mark_running(emitter, run_id);
    Ok(())
}

fn emit_length_give_up(emitter: &Arc<BroadcastEmitter>, run_id: &str) {
    emitter.persist_and_emit(
        run_id,
        &BusEvent::CommandOutput {
            run_id: run_id.to_string(),
            content: "[auto-continue] 连续多次达到输出长度上限，自动继续已停止。建议：拆分任务、要求模型用工具完成计算，或更换更强模型。".to_string(),
        },
    );
}

fn emit_ralph_complete(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    reason: RalphCompleteReason,
    iteration: u32,
) {
    emitter.persist_and_emit(
        run_id,
        &BusEvent::RalphComplete {
            run_id: run_id.to_string(),
            reason,
            iteration,
        },
    );
}

fn format_work_turn_context(plan: &crate::work::context::WorkContextPlan) -> String {
    let rendered = plan.render_system_prompt();
    if !rendered.is_empty() {
        return format!("\n\n[Active Work Context Plan]\n{}", rendered);
    }
    String::new()
}

fn work_turn_context(
    run_id: &str,
    plan: Option<&crate::work::context::WorkContextPlan>,
) -> Result<String, String> {
    let run = storage::runs::get_run(run_id)
        .ok_or_else(|| format!("Work message run metadata is unavailable: {run_id}"))?;
    if run.app_mode != AppMode::Work {
        return Ok(String::new());
    }

    let expected_runtime =
        crate::agent::capability_resolver::RuntimeProviderKind::try_from_agent_str(&run.agent)
            .map_err(|error| format!("Invalid Work runtime provider: {error}"))?;
    if expected_runtime != crate::agent::capability_resolver::RuntimeProviderKind::Pi {
        return Err(format!(
            "Pi actor cannot execute Work runtime '{}'",
            expected_runtime.as_str()
        ));
    }
    let plan = plan.ok_or_else(|| {
        format!("Work message for run {run_id} is missing its turn-scoped Context Plan")
    })?;
    if plan.runtime != expected_runtime {
        return Err(format!(
            "Work message Context Plan runtime '{}' does not match run runtime '{}'",
            plan.runtime.as_str(),
            expected_runtime.as_str()
        ));
    }
    work_turn_context_for_run(run_id, true, Some(plan))
}

fn work_turn_context_for_run(
    run_id: &str,
    is_work: bool,
    plan: Option<&crate::work::context::WorkContextPlan>,
) -> Result<String, String> {
    if !is_work {
        return Ok(String::new());
    }

    let plan = plan.ok_or_else(|| {
        format!("Work message for run {run_id} is missing its turn-scoped Context Plan")
    })?;
    if plan.run_id != run_id {
        return Err(format!(
            "Work message Context Plan belongs to run {}, expected {run_id}",
            plan.run_id
        ));
    }

    let rendered = format_work_turn_context(plan);
    if rendered.is_empty() {
        return Err(format!(
            "Work message for run {run_id} has an empty Context Plan"
        ));
    }
    Ok(rendered)
}

fn work_attachment_context(run_id: &str, attachments: &[AttachmentData]) -> String {
    let is_work = storage::runs::get_run(run_id).is_some_and(|run| run.app_mode == AppMode::Work);
    if !is_work || attachments.is_empty() {
        return String::new();
    }

    let mut paths = Vec::new();
    for attachment in attachments {
        let raw_size = (attachment.content_base64.len() as u64) * 3 / 4;
        if raw_size > max_attachment_size(&attachment.media_type) {
            log::warn!(
                "[pi_rpc_actor] skipping oversized Work attachment: {}",
                attachment.filename
            );
            continue;
        }
        if let Some(path) = save_attachment_to_disk(run_id, attachment) {
            paths.push(format!("- {} ({})", path, attachment.filename));
        }
    }
    if paths.is_empty() {
        return String::new();
    }
    format!(
        "\n\n[本条消息附带的文件已保存到以下临时路径。需要读取时，请使用这些路径；它们不属于 Workspace 的 input/。]\n{}",
        paths.join("\n")
    )
}

fn handle_lifecycle_settlement_error(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    phase: &str,
    error: &str,
) {
    log::error!("[pi/actor] {phase} settlement failed for Work run {run_id}: {error}");
    crate::storage::runs::persist_result_error(
        run_id,
        Some(format!("{phase} failure: {error}")),
        None,
    )
    .ok();
    crate::storage::runs::update_status(
        run_id,
        crate::models::RunStatus::Failed,
        None,
        Some(error.to_string()),
    )
    .ok();
    emitter.persist_and_emit(
        run_id,
        &crate::models::BusEvent::RunState {
            run_id: run_id.to_string(),
            state: "failed".to_string(),
            exit_code: None,
            error: Some(error.to_string()),
        },
    );
    let paths = crate::work::paths::WorkPaths::app();
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    tag: Arc<()>,
    mut child: Child,
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut cmd_rx: mpsc::Receiver<ActorCommand>,
    mut protocol: PiRpc,
    managed_provider: bool,
    permission_agent_dir: Option<PathBuf>,
    session_persistence_enabled: bool,
    browser_token: Option<String>,
    connector_token: Option<String>,
    desktop_token: Option<String>,
    work_bridge_token: Option<String>,
    cancel: CancellationToken,
    shutdown_tx: oneshot::Sender<()>,
) {
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut stderr_open = true;
    let mut recent_stderr: Vec<String> = Vec::new();
    let mut explicitly_stopped = false;
    let mut stop_reason: Option<crate::agent::session_actor::RuntimeStopReason> = None;
    let mut is_streaming = false;
    let mut ralph_loop: Option<PiRalphLoop> = None;
    let mut active_work_context_plan: Option<crate::work::context::WorkContextPlan> = None;
    let mut length_continuations: u32 = 0;
    let mut control_waiters: std::collections::HashMap<String, oneshot::Sender<Value>> =
        std::collections::HashMap::new();
    let mut pending_efforts: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut feature_refresh_deadline: Option<Instant> = None;
    let mut feature_refresh_tick = tokio::time::interval(Duration::from_millis(50));

    loop {
        tokio::select! {
            _ = feature_refresh_tick.tick() => {
                match protocol.work_retry_state {
                    crate::agent::pi_rpc_protocol::WorkRetryState::WaitingForRetryStart(deadline)
                        if std::time::Instant::now() >= deadline =>
                    {
                        protocol.work_retry_state = crate::agent::pi_rpc_protocol::WorkRetryState::None;
                        let error = "Pi automatic retry did not start within 15 seconds".to_string();
                        stop_reason = Some(crate::agent::session_actor::RuntimeStopReason::ProviderCrash(error.clone()));
                        mark_failed(&emitter, &run_id, None, error);
                        explicitly_stopped = true;
                        shutdown_pi_process(&emitter, &run_id, &mut protocol, &mut stdin, &mut child, is_streaming).await;
                        break;
                    }
                    crate::agent::pi_rpc_protocol::WorkRetryState::Retrying(deadline)
                        if std::time::Instant::now() >= deadline =>
                    {
                        protocol.work_retry_state = crate::agent::pi_rpc_protocol::WorkRetryState::None;
                        let error = "Pi retry did not finish within 120 seconds".to_string();
                        stop_reason = Some(crate::agent::session_actor::RuntimeStopReason::ProviderCrash(error.clone()));
                        mark_failed(&emitter, &run_id, None, error);
                        explicitly_stopped = true;
                        shutdown_pi_process(&emitter, &run_id, &mut protocol, &mut stdin, &mut child, is_streaming).await;
                        break;
                    }
                    _ => {}
                }
                if feature_refresh_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    let frame = protocol.frame_get_entries(None);
                    if let Err(error) = write_json_line(&mut stdin, &frame).await {
                        emitter.persist_and_emit(
                            &run_id,
                            &BusEvent::CommandOutput {
                                run_id: run_id.clone(),
                                content: format!("[pi rpc] get_entries refresh failed: {error}"),
                            },
                        );
                    }
                    feature_refresh_deadline = None;
                }
            }
            command = cmd_rx.recv() => {
                match command {
                    Some(ActorCommand::SendMessage {
                        text,
                        attachments,
                        skills,
                        work_context_plan,
                        reply,
                    }) => {
                        if ralph_loop.is_some() {
                            let _ = reply.send(Err(
                                "Cancel the active Ralph loop before sending a message".to_string(),
                            ));
                            continue;
                        }
                        let work_context = match work_turn_context(
                            &run_id,
                            work_context_plan.as_ref(),
                        ) {
                            Ok(context) => context,
                            Err(error) => {
                                mark_failed(&emitter, &run_id, None, error.clone());
                                stop_reason = Some(crate::agent::session_actor::RuntimeStopReason::ProviderCrash(error.clone()));
                                let _ = reply.send(Err(error));
                                continue;
                            }
                        };
                        active_work_context_plan = if is_work_run(&run_id) {
                            work_context_plan
                        } else {
                            None
                        };
                        let mut effective_text = text.clone();
                        if !skills.is_empty() {
                            let skill_refs: Vec<String> =
                                skills.iter().map(|s| format!("@{}", s.name)).collect();
                            if !effective_text.is_empty() {
                                effective_text = format!("{} {}", skill_refs.join(" "), effective_text);
                            } else {
                                effective_text = skill_refs.join(" ");
                            }
                        }
                        effective_text.push_str(&work_context);
                        effective_text.push_str(&work_attachment_context(&run_id, &attachments));
                        let images = pi_images(&attachments);
                        let behavior = is_streaming.then_some(PiStreamingBehavior::FollowUp);
                        let frame = protocol.frame_prompt(&effective_text, &images, behavior);
                        match write_json_line(&mut stdin, &frame).await {
                            Ok(()) => {
                                // Treat the turn as busy immediately after the prompt is written.
                                // A second message can arrive before Pi emits agent_start.
                                is_streaming = true;
                                // A new user message starts a fresh task segment:
                                // reset the length-continuation counter so prior
                                // truncations don't count against the new turn.
                                length_continuations = 0;
                                emit_user_message(&emitter, &run_id, &text, &attachments);
                                mark_running(&emitter, &run_id);
                                let _ = reply.send(Ok(()));
                            }
                            Err(error) => {
                                mark_failed(&emitter, &run_id, None, error.clone());
                                stop_reason = Some(crate::agent::session_actor::RuntimeStopReason::ProviderCrash(
                                    format!("Pi RPC transport failed: {error}")
                                ));
                                let _ = reply.send(Err(error));
                                explicitly_stopped = true;
                                shutdown_pi_process(
                                    &emitter,
                                    &run_id,
                                    &mut protocol,
                                    &mut stdin,
                                    &mut child,
                                    is_streaming,
                                )
                                .await;
                                break;
                            }
                        }
                    }
                    Some(ActorCommand::SteerMessage { reply, .. }) => {
                        let _ = reply.send(Err(
                            "Claude steer is not available for Pi sessions".to_string(),
                        ));
                    }
                    Some(ActorCommand::CancelTurn { reply }) => {
                        let _ = reply.send(Err(
                            "Turn cancellation without ending the session is not available for Pi sessions".to_string(),
                        ));
                    }
                    Some(ActorCommand::SendControl { request, reply }) => {
                        let pending_effort = if matches!(
                            request.get("subtype").and_then(Value::as_str),
                            Some("set_effort") | Some("set_thinking_level")
                        )
                        {
                            request
                                .get("effort")
                                .or_else(|| request.get("level"))
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(|value| {
                                    if value == "none" {
                                        "off".to_string()
                                    } else {
                                        value.to_string()
                                    }
                                })
                        } else {
                            None
                        };
                        let request_id = format!("agentcabin_pi_ctrl_{}", uuid::Uuid::new_v4());
                        match handle_control(
                            &run_id,
                            &mut protocol,
                            &mut stdin,
                            &request,
                            &emitter,
                            managed_provider,
                            permission_agent_dir.as_deref(),
                            active_work_context_plan.as_ref(),
                        )
                        .await
                        {
                            Ok(response) => {
                                if let Some(wire_id) = response.get("__pending_rpc_id").and_then(Value::as_str) {
                                    let (response_tx, response_rx) = oneshot::channel();
                                    control_waiters.insert(wire_id.to_string(), response_tx);
                                    if let Some(effort) = pending_effort {
                                        pending_efforts.insert(wire_id.to_string(), effort);
                                    }
                                    let _ = reply.send(Ok((request_id, response_rx)));
                                } else {
                                    let (response_tx, response_rx) = oneshot::channel();
                                    let _ = response_tx.send(response);
                                    let _ = reply.send(Ok((request_id, response_rx)));
                                }
                            }
                            Err(error) => {
                                let _ = reply.send(Err(error));
                            }
                        }
                    }
                    Some(ActorCommand::Stop { reason, reply }) => {
                        explicitly_stopped = true;
                        stop_reason = Some(reason);
                        shutdown_pi_process(
                            &emitter,
                            &run_id,
                            &mut protocol,
                            &mut stdin,
                            &mut child,
                            is_streaming,
                        )
                        .await;
                        let _ = reply.send(Ok(()));
                        break;
                    }
                    Some(ActorCommand::RespondPermission { request_id, response, reply }) => {
                        let result = write_interactive_response(
                            &mut protocol,
                            &mut stdin,
                            PendingKind::Permission,
                            &request_id,
                            response,
                        ).await;
                        let _ = reply.send(result);
                    }
                    Some(ActorCommand::CancelControlRequest { request_id, reply }) => {
                        let result = write_interactive_response(
                            &mut protocol,
                            &mut stdin,
                            PendingKind::UserInput,
                            &request_id,
                            json!({"cancelled": true}),
                        ).await;
                        let _ = reply.send(result);
                    }
                    Some(ActorCommand::RespondHookCallback { request_id, response, reply }) => {
                        let result = write_interactive_response(
                            &mut protocol,
                            &mut stdin,
                            PendingKind::Permission,
                            &request_id,
                            response,
                        ).await;
                        let _ = reply.send(result);
                    }
                    Some(ActorCommand::RespondElicitation { request_id, response, reply }) => {
                        let is_pending = protocol
                            .host()
                            .pending_requests()
                            .iter()
                            .any(|r| r.id == request_id);
                        if !is_pending {
                            let _ = reply.send(Err(format!(
                                "Request '{}' already resolved or unknown",
                                request_id
                            )));
                        } else {
                            let result = write_interactive_response(
                                &mut protocol,
                                &mut stdin,
                                PendingKind::Elicitation,
                                &request_id,
                                response,
                            )
                            .await;
                            if result.is_ok() {
                                protocol.host_mut().remove_pending_request(&request_id);
                                emitter.persist_and_emit(
                                    &run_id,
                                    &BusEvent::PiExtensionUiRequestResolved {
                                        run_id: run_id.clone(),
                                        request_id: request_id.clone(),
                                    },
                                );
                                emitter.persist_and_emit(
                                    &run_id,
                                    &BusEvent::PiExtensionStateSync {
                                        run_id: run_id.clone(),
                                        snapshot: protocol.host_snapshot(),
                                    },
                                );
                            }
                            let _ = reply.send(result);
                        }
                    }
                    Some(ActorCommand::RespondUserInput { request_id, response, reply }) => {
                        let result = write_interactive_response(
                            &mut protocol,
                            &mut stdin,
                            PendingKind::UserInput,
                            &request_id,
                            response,
                        ).await;
                        let _ = reply.send(result);
                    }
                    Some(ActorCommand::StartRalphLoop {
                        prompt,
                        max_iterations,
                        completion_promise,
                        work_context_plan,
                        reply,
                    }) => {
                        if ralph_loop.is_some() {
                            let _ = reply.send(Err("Ralph loop already active".into()));
                        } else if is_streaming {
                            let _ = reply.send(Err(
                                "Cannot start Ralph loop while Pi is responding".into(),
                            ));
                        } else if let Err(error) = work_turn_context(
                            &run_id,
                            work_context_plan.as_ref(),
                        ) {
                            let _ = reply.send(Err(error));
                        } else {
                            active_work_context_plan = if is_work_run(&run_id) {
                                work_context_plan.clone()
                            } else {
                                None
                            };
                            let started_at = crate::models::now_iso();
                            emitter.persist_and_emit(
                                &run_id,
                                &BusEvent::RalphStarted {
                                    run_id: run_id.clone(),
                                    prompt: prompt.clone(),
                                    max_iterations,
                                    completion_promise: completion_promise.clone(),
                                    started_at,
                                },
                            );
                            match dispatch_ralph_prompt(
                                &emitter,
                                &run_id,
                                &mut protocol,
                                &mut stdin,
                                &prompt,
                                active_work_context_plan.as_ref(),
                            )
                            .await
                            {
                                Ok(()) => {
                                    ralph_loop = Some(PiRalphLoop {
                                        prompt,
                                        work_context_plan: active_work_context_plan.clone(),
                                        iteration: 0,
                                        max_iterations,
                                        completion_promise,
                                        promise_matched: false,
                                        cancel_pending: false,
                                        consecutive_failures: 0,
                                    });
                                    is_streaming = true;
                                    let _ = reply.send(Ok(()));
                                }
                                Err(error) => {
                                    emit_ralph_complete(
                                        &emitter,
                                        &run_id,
                                        RalphCompleteReason::FailStopped,
                                        0,
                                    );
                                    let _ = reply.send(Err(error));
                                }
                            }
                        }
                    }
                    Some(ActorCommand::CancelRalphLoop { reply }) => {
                        match ralph_loop.as_mut() {
                            None => {
                                let _ = reply.send(Err("No active ralph loop".into()));
                            }
                            Some(ralph) if is_streaming => {
                                ralph.cancel_pending = true;
                                let _ = reply.send(Ok(RalphCancelResult {
                                    iteration: ralph.iteration,
                                    immediate: false,
                                }));
                            }
                            Some(ralph) => {
                                let iteration = ralph.iteration;
                                ralph_loop = None;
                                emit_ralph_complete(
                                    &emitter,
                                    &run_id,
                                    RalphCompleteReason::Cancelled,
                                    iteration,
                                );
                                let _ = reply.send(Ok(RalphCancelResult {
                                    iteration,
                                    immediate: true,
                                }));
                            }
                        }
                    }
                    None => {
                        explicitly_stopped = true;
                        shutdown_pi_process(
                            &emitter,
                            &run_id,
                            &mut protocol,
                            &mut stdin,
                            &mut child,
                            is_streaming,
                        )
                        .await;
                        break;
                    }
                }
            }
            result = stdout_lines.next_line() => {
                match result {
                    Ok(Some(line)) => {
                        if let Some(ralph) = ralph_loop.as_mut() {
                            if let Some(promise) = ralph.completion_promise.as_deref() {
                                if line.contains(promise) {
                                    ralph.promise_matched = true;
                                }
                            }
                        }
                        let signal = handle_stdout_line(
                            &emitter,
                            &run_id,
                            &mut protocol,
                            &line,
                            session_persistence_enabled,
                            &mut stdin,
                        ).await;
                        if protocol.take_feature_state_refresh_request() {
                            feature_refresh_deadline = Some(Instant::now() + Duration::from_millis(150));
                        }
                        if let Some((response_id, response)) = protocol.take_control_response() {
                            if let Some(effort) = pending_efforts.remove(&response_id) {
                                if response.get("success").and_then(Value::as_bool) == Some(true) {
                                    if let Err(error) = storage::runs::update_run_effort(&run_id, &effort) {
                                        log::warn!(
                                            "[pi_rpc_actor] failed to persist acknowledged thinking level: run_id={}, error={}",
                                            run_id,
                                            error
                                        );
                                    }
                                }
                            }
                            if let Some(waiter) = control_waiters.remove(&response_id) {
                                let _ = waiter.send(response);
                            }
                            // Control responses include slash commands, fork,
                            // clone, and switch_session. Refreshing after any
                            // acknowledged control keeps structured extension
                            // and tree state authoritative without parsing text.
                            feature_refresh_deadline =
                                Some(Instant::now() + Duration::from_millis(150));
                        }
                        if let Some(signal) = signal {
                            match signal {
                                LifecycleSignal::TurnStarted => {
                                    is_streaming = true;
                                }
                                LifecycleSignal::TurnCompleted => {
                                    feature_refresh_deadline =
                                        Some(Instant::now() + Duration::from_millis(150));

                                    // Work-mode auto-continuation: if the turn was
                                    // truncated by the output-length limit, inject a
                                    // follow-up that steers the model toward using tools
                                    // instead of hand-computing in its head. Limited to
                                    // MAX_LENGTH_CONTINUATIONS to avoid silent token burn.
                                    let mut auto_continued = false;
                                    if ralph_loop.is_none()
                                        && is_work_run(&run_id)
                                        && protocol.last_turn_stop_reason() == Some("length")
                                    {
                                        if length_continuations < MAX_LENGTH_CONTINUATIONS {
                                            length_continuations += 1;
                                            match dispatch_length_continuation(
                                                &emitter,
                                                &run_id,
                                                &mut protocol,
                                                &mut stdin,
                                                active_work_context_plan.as_ref(),
                                            )
                                            .await
                                            {
                                                Ok(()) => auto_continued = true,
                                                Err(_) => auto_continued = false,
                                            }
                                        } else {
                                            emit_length_give_up(&emitter, &run_id);
                                        }
                                    } else if protocol.last_turn_stop_reason() != Some("length") {
                                        // Normal completion resets the counter so a later
                                        // truncation gets a fresh budget.
                                        length_continuations = 0;
                                    }

                                    is_streaming = auto_continued;
                                    if let Some(mut ralph) = ralph_loop.take() {
                                        ralph.iteration += 1;
                                        ralph.consecutive_failures = 0;
                                        let complete_reason = if ralph.cancel_pending {
                                            Some(RalphCompleteReason::Cancelled)
                                        } else if ralph.promise_matched {
                                            Some(RalphCompleteReason::CompletionPromise)
                                        } else if ralph.max_iterations > 0
                                            && ralph.iteration >= ralph.max_iterations
                                        {
                                            Some(RalphCompleteReason::MaxIterations)
                                        } else {
                                            None
                                        };
                                        if let Some(reason) = complete_reason {
                                            emit_ralph_complete(
                                                &emitter,
                                                &run_id,
                                                reason,
                                                ralph.iteration,
                                            );
                                        } else {
                                            emitter.persist_and_emit(
                                                &run_id,
                                                &BusEvent::RalphIteration {
                                                    run_id: run_id.clone(),
                                                    iteration: ralph.iteration,
                                                    max_iterations: ralph.max_iterations,
                                                },
                                            );
                                            ralph.promise_matched = false;
                                            let prompt = ralph.prompt.clone();
                                            match dispatch_ralph_prompt(
                                                &emitter,
                                                &run_id,
                                                &mut protocol,
                                                &mut stdin,
                                                &prompt,
                                                ralph.work_context_plan.as_ref(),
                                            )
                                            .await
                                            {
                                                Ok(()) => {
                                                    is_streaming = true;
                                                    ralph_loop = Some(ralph);
                                                }
                                                Err(_) => emit_ralph_complete(
                                                    &emitter,
                                                    &run_id,
                                                    RalphCompleteReason::FailStopped,
                                                    ralph.iteration,
                                                ),
                                            }
                                        }
                                    }
                                    if !is_streaming && is_work_run(&run_id) {
                                        if let Err(error) = crate::work::session::handle_turn_settled(
                                            &emitter,
                                            &run_id,
                                            crate::work::session::RuntimeTurnOutcome::Success,
                                        )
                                        .await
                                        {
                                            handle_lifecycle_settlement_error(
                                                &emitter,
                                                &run_id,
                                                "Turn completion",
                                                &error,
                                            );
                                        }
                                    }
                                }
                                LifecycleSignal::TurnFailed(ref error) => {
                                    is_streaming = false;
                                    if let Some(mut ralph) = ralph_loop.take() {
                                        if ralph.cancel_pending {
                                            emit_ralph_complete(
                                                &emitter,
                                                &run_id,
                                                RalphCompleteReason::Cancelled,
                                                ralph.iteration,
                                            );
                                        } else {
                                            ralph.consecutive_failures += 1;
                                            if ralph.consecutive_failures >= 3 {
                                                emit_ralph_complete(
                                                    &emitter,
                                                    &run_id,
                                                    RalphCompleteReason::FailStopped,
                                                    ralph.iteration,
                                                );
                                            } else {
                                                let prompt = ralph.prompt.clone();
                                                match dispatch_ralph_prompt(
                                                    &emitter,
                                                    &run_id,
                                                &mut protocol,
                                                &mut stdin,
                                                &prompt,
                                                ralph.work_context_plan.as_ref(),
                                            )
                                                .await
                                                {
                                                    Ok(()) => {
                                                        is_streaming = true;
                                                        ralph_loop = Some(ralph);
                                                    }
                                                    Err(_) => emit_ralph_complete(
                                                        &emitter,
                                                        &run_id,
                                                        RalphCompleteReason::FailStopped,
                                                        ralph.iteration,
                                                    ),
                                                }
                                            }
                                        }
                                    }
                                    if !is_streaming && is_work_run(&run_id) {
                                        if let Err(settle_err) = crate::work::session::handle_turn_settled(
                                            &emitter,
                                            &run_id,
                                            crate::work::session::RuntimeTurnOutcome::Failed(error.clone()),
                                        )
                                        .await
                                        {
                                            handle_lifecycle_settlement_error(
                                                &emitter,
                                                &run_id,
                                                "Turn failure",
                                                &settle_err,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            result = stderr_lines.next_line(), if stderr_open => {
                match result {
                    Ok(Some(line)) => {
                        if !line.trim().is_empty() {
                            if recent_stderr.len() >= 10 {
                                recent_stderr.remove(0);
                            }
                            recent_stderr.push(line.clone());
                            emitter.persist_and_emit(
                                &run_id,
                                &BusEvent::CommandOutput {
                                    run_id: run_id.clone(),
                                    content: format!("[pi rpc stderr] {}", line),
                                },
                            );
                        }
                    }
                    Ok(None) | Err(_) => stderr_open = false,
                }
            }
            _ = cancel.cancelled() => {
                explicitly_stopped = true;
                // The global CancellationToken is fired by App shutdown, not by
                // the user clicking Stop. UserStop is delivered via ActorCommand::Stop
                // which sets the reason explicitly before this branch is reached.
                stop_reason = Some(crate::agent::session_actor::RuntimeStopReason::AppShutdown);
                shutdown_pi_process(
                    &emitter,
                    &run_id,
                    &mut protocol,
                    &mut stdin,
                    &mut child,
                    is_streaming,
                )
                .await;
                break;
            }
        }
    }

    if !explicitly_stopped {
        if stderr_open {
            while let Ok(Ok(Some(line))) =
                tokio::time::timeout(Duration::from_millis(50), stderr_lines.next_line()).await
            {
                if !line.trim().is_empty() {
                    if recent_stderr.len() >= 10 {
                        recent_stderr.remove(0);
                    }
                    recent_stderr.push(line);
                }
            }
        }
        let status = child.wait().await.ok();
        let code = status.as_ref().and_then(|value| value.code());
        let err_msg = if !recent_stderr.is_empty() {
            format!(
                "Pi RPC process exited unexpectedly (code {:?}): {}",
                code,
                recent_stderr.join("\n")
            )
        } else {
            format!("Pi RPC process exited unexpectedly (code {:?})", code)
        };
        mark_failed(&emitter, &run_id, code, err_msg.clone());
        stop_reason = Some(crate::agent::session_actor::RuntimeStopReason::ProviderCrash(err_msg));
    }

    control_waiters.clear();

    let (cancel_frames, cancelled_ids) = protocol.cancel_all_pending();
    if !cancel_frames.is_empty() {
        let _ = write_frames(&mut stdin, &cancel_frames).await;
        for req_id in cancelled_ids {
            emitter.persist_and_emit(
                &run_id,
                &BusEvent::PiExtensionUiRequestResolved {
                    run_id: run_id.clone(),
                    request_id: req_id,
                },
            );
        }
    }

    // Always publish terminal empty state, even when there were no pending
    // requests. This prevents stale queue/dialog UI after Stop or process exit.
    emitter.persist_and_emit(
        &run_id,
        &BusEvent::PiQueueUpdated {
            run_id: run_id.clone(),
            state: PiMessageQueueState::default(),
        },
    );
    emitter.persist_and_emit(
        &run_id,
        &BusEvent::PiExtensionStateSync {
            run_id: run_id.clone(),
            snapshot: protocol.host_snapshot(),
        },
    );

    cleanup_actor(
        &sessions,
        &run_id,
        &tag,
        browser_token.as_deref(),
        connector_token.as_deref(),
        desktop_token.as_deref(),
        work_bridge_token.as_deref(),
    )
    .await;
    if is_work_run(&run_id) {
        let final_stop_reason =
            // Any exit without an explicit stop reason (e.g. process crashed before
            // sending any stop command) should preserve the WorkRun status for
            // restart recovery, not cancel it as if the user clicked Stop.
            stop_reason.unwrap_or(crate::agent::session_actor::RuntimeStopReason::AppShutdown);
        if let Err(error) =
            crate::work::session::handle_process_stopped(&emitter, &run_id, final_stop_reason).await
        {
            handle_lifecycle_settlement_error(&emitter, &run_id, "Process stopped", &error);
        }
    }
    let _ = shutdown_tx.send(());
}

#[allow(clippy::too_many_arguments)]
async fn handle_control(
    run_id: &str,
    protocol: &mut PiRpc,
    stdin: &mut ChildStdin,
    request: &Value,
    emitter: &Arc<BroadcastEmitter>,
    _managed_provider: bool,
    permission_agent_dir: Option<&Path>,
    work_context_plan: Option<&crate::work::context::WorkContextPlan>,
) -> Result<Value, String> {
    let subtype = request.get("subtype").and_then(Value::as_str).unwrap_or("");

    match subtype {
        "interrupt" | "abort" => {
            let frames = protocol.frame_interrupt();
            write_frames(stdin, &frames).await?;
            // The abort frame is acknowledged once it has been written. Pi may
            // emit agent_end a little later, but the UI should stop presenting
            // this turn as running immediately.
            mark_idle(emitter, run_id);
        }
        "steer" | "pi_steer" => {
            let text = request
                .get("text")
                .or_else(|| request.get("message"))
                .and_then(Value::as_str)
                .ok_or_else(|| "Pi steer requires a text parameter".to_string())?;
            let frames = protocol.frame_steer(text);
            let pending_id = frames.first().and_then(|frame| frame.get("id")).cloned();
            write_frames(stdin, &frames).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "follow_up" | "pi_follow_up" => {
            let text = request
                .get("text")
                .or_else(|| request.get("message"))
                .and_then(Value::as_str)
                .ok_or_else(|| "Pi follow_up requires a text parameter".to_string())?;
            let frame = protocol.frame_follow_up(text, &[]);
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "clear_queue" | "pi_clear_queue" => {
            let frame = protocol.frame_clear_queue();
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "compact" => {
            let frame = protocol.frame_compact();
            write_json_line(stdin, &frame).await?;
        }
        "new_session" => {
            let frame = protocol.frame_new_session();
            write_json_line(stdin, &frame).await?;
        }
        "clone" | "pi_clone" => {
            // Pi's clone response only reports whether the operation was cancelled;
            // it does not include the new session id. Commands are processed in order,
            // so request state immediately afterwards and let the normal stdout path
            // persist the cloned session id on the run.
            let frame = protocol.frame_clone();
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "fork" | "pi_fork" => {
            let entry_id = request
                .get("entry_id")
                .or_else(|| request.get("entryId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Pi fork requires an entry_id parameter".to_string())?;
            let frame = protocol.frame_fork(entry_id);
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "switch_session" | "pi_switch_session" => {
            let session_path = request
                .get("session_path")
                .or_else(|| request.get("sessionPath"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Pi switch_session requires a session_path parameter".to_string())?;
            let frame = protocol.frame_switch_session(session_path);
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "pi_get_entries" => {
            let since = request.get("since").and_then(Value::as_str);
            let frame = protocol.frame_get_entries(since);
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "pi_get_tree" => {
            let frame = protocol.frame_get_tree();
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "pi_get_fork_messages" => {
            let frame = protocol.frame_get_fork_messages();
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "pi_get_state" => {
            let frame = protocol.frame_get_state();
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "model_list" => {
            // Pi answers the startup `get_available_models` asynchronously. The
            // picker can ask for the catalog before that response has arrived,
            // so an empty cache must trigger a correlated refresh instead of
            // being returned as a permanent empty list.
            if !protocol.available_models().is_empty() {
                return Ok(json!({"data": protocol.available_models()}));
            }
            let frame = protocol.frame_get_available_models();
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "mcp_status" => {
            let refresh = protocol.frame_get_state();
            let _ = write_json_line(stdin, &refresh).await;
            let req_cwd = request.get("cwd").and_then(Value::as_str);
            let configured = crate::storage::mcp_registry::list_pi_configured(req_cwd);
            let live = protocol.mcp_servers();
            let runtime_available =
                crate::storage::mcp_registry::is_pi_mcp_adapter_installed(req_cwd);
            return Ok(json!({
                "servers": live,
                "configured": configured,
                "runtime_available": runtime_available,
            }));
        }
        "mcp_toggle" => {
            let server_name = request
                .get("server_name")
                .or_else(|| request.get("serverName"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            let enabled = request
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let scope = request
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let req_cwd = request.get("cwd").and_then(Value::as_str);
            let res = crate::storage::mcp_registry::toggle_pi_server_config(
                server_name,
                enabled,
                scope,
                req_cwd,
            )?;
            let refresh = protocol.frame_get_state();
            let _ = write_json_line(stdin, &refresh).await;
            return Ok(serde_json::to_value(res).unwrap_or_default());
        }
        "mcp_reconnect" => {
            let req_cwd = request.get("cwd").and_then(Value::as_str);
            let runtime_available =
                crate::storage::mcp_registry::is_pi_mcp_adapter_installed(req_cwd);
            if !runtime_available {
                return Ok(json!({
                    "success": false,
                    "message": "Pi MCP runtime unavailable: pi-mcp-adapter extension is not installed",
                    "runtime_available": false,
                }));
            }
            let refresh = protocol.frame_get_state();
            let _ = write_json_line(stdin, &refresh).await;
            return Ok(json!({
                "success": true,
                "message": "Refreshed Pi MCP state",
                "runtime_available": true,
            }));
        }
        "set_model" => {
            let raw_model = request
                .get("model")
                .or_else(|| request.get("modelId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Pi set_model requires a model parameter".to_string())?;
            let explicit_provider = request
                .get("provider")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let user_settings = storage::settings::get_user_settings();
            let (provider, model_id) = resolve_set_model_target(
                raw_model,
                explicit_provider,
                &user_settings.global_providers,
                user_settings.pi_provider.as_ref(),
            );
            write_json_line(
                stdin,
                &json!({
                    "id": format!("agentcabin-pi-model-{}", uuid::Uuid::new_v4()),
                    "type": "set_model",
                    "provider": provider,
                    "modelId": model_id,
                }),
            )
            .await?;
            // Re-read authoritative state so the frontend model label and run metadata converge.
            write_json_line(
                stdin,
                &json!({
                    "id": format!("agentcabin-pi-state-{}", uuid::Uuid::new_v4()),
                    "type": "get_state",
                }),
            )
            .await?;

            // A model switch is scoped to the active run. Project defaults are updated only by
            // the explicit frontend action, so another conversation cannot inherit this switch.
            let canonical_run_model =
                if provider != "agentcabin" && !raw_model.starts_with(&format!("{}/", provider)) {
                    format!("{}/{}", provider, model_id)
                } else {
                    raw_model.to_string()
                };
            storage::runs::update_run_model(run_id, &canonical_run_model)
                .map_err(|error| format!("Failed to persist Pi run model: {}", error))?;
        }
        "set_thinking_level" | "set_effort" => {
            let level = request
                .get("level")
                .or_else(|| request.get("effort"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Pi set_thinking_level requires a level parameter".to_string())?;
            let level = if level == "none" { "off" } else { level };
            if !matches!(
                level,
                "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            ) {
                return Err(format!("Unsupported Pi thinking level '{}'", level));
            }
            let frame = json!({
                "id": format!("agentcabin-pi-thinking-{}", uuid::Uuid::new_v4()),
                "type": "set_thinking_level",
                "level": level,
            });
            let pending_id = frame.get("id").cloned();
            write_json_line(stdin, &frame).await?;
            return Ok(json!({"__pending_rpc_id": pending_id}));
        }
        "is_alive" => {
            return Ok(json!({"ok": true, "alive": true}));
        }
        "goal_get" | "goal_set" | "goal_clear" => {
            return Err("Pi Goal state is owned by the @narumitw/pi-goal extension; use execute_slash_command".to_string());
        }
        "set_permission_mode" | "permission_mode" => {
            let mode = request
                .get("mode")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|mode| !mode.is_empty())
                .ok_or_else(|| "Pi permission mode requires a mode parameter".to_string())?;
            let agent_dir = permission_agent_dir.ok_or_else(|| {
                "Pi permission mode cannot be applied: managed agent profile is unavailable"
                    .to_string()
            })?;
            let mut response = apply_permission_mode_control(protocol, agent_dir, mode)?;
            if response["mode"] == "auto_approve" {
                let resolved =
                    resolve_pending_permission_requests(protocol, stdin, emitter, run_id).await?;
                response["resolvedPendingRequests"] = json!(resolved);
            }
            return Ok(response);
        }
        "execute_slash_command" | "slash_command" => {
            let cmd_name = request
                .get("command")
                .or_else(|| request.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| "execute_slash_command requires a command name".to_string())?;

            let args = request
                .get("args")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();

            return dispatch_pi_slash_command(
                run_id,
                protocol,
                stdin,
                cmd_name,
                args,
                emitter,
                work_context_plan,
            )
            .await;
        }
        "get_extension_ui_state" | "get_pi_extension_ui_state" => {
            return Ok(serde_json::to_value(protocol.host_snapshot()).unwrap_or_default());
        }
        "extension_ui_response" => {
            let req_id = request
                .get("request_id")
                .or_else(|| request.get("requestId"))
                .or_else(|| request.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    "extension_ui_response requires a non-empty request_id / id".to_string()
                })?;
            let is_pending = protocol
                .host()
                .pending_requests()
                .iter()
                .any(|r| r.id == req_id);
            if !is_pending {
                return Err(format!("Request '{}' already resolved or unknown", req_id));
            }
            write_interactive_response(
                protocol,
                stdin,
                PendingKind::Elicitation,
                req_id,
                request.clone(),
            )
            .await?;
            protocol.host_mut().remove_pending_request(req_id);
            emitter.persist_and_emit(
                run_id,
                &BusEvent::PiExtensionUiRequestResolved {
                    run_id: run_id.to_string(),
                    request_id: req_id.to_string(),
                },
            );
            emitter.persist_and_emit(
                run_id,
                &BusEvent::PiExtensionStateSync {
                    run_id: run_id.to_string(),
                    snapshot: protocol.host_snapshot(),
                },
            );
            return Ok(json!({"ok": true}));
        }
        other => {
            return Err(format!("Pi RPC control '{}' is not supported yet", other));
        }
    }

    Ok(json!({"ok": true}))
}

pub(crate) fn resolve_set_model_target(
    raw_model: &str,
    explicit_provider: Option<&str>,
    global_providers: &[crate::models::GlobalProviderCredential],
    pi_provider: Option<&crate::models::PiProviderCredential>,
) -> (String, String) {
    if let Some(provider) = explicit_provider {
        let model_id = raw_model
            .strip_prefix(&format!("{}/", provider))
            .unwrap_or(raw_model);
        return (provider.to_string(), model_id.to_string());
    }

    // A raw_model may itself be a model ID that contains a slash (e.g. "zhanlu/kimi-k3")
    // hosted under a custom global provider, without any provider prefix. Resolve the full
    // ID against known provider models FIRST. Otherwise a bare ID whose first segment
    // collides with a builtin/global provider name (e.g. "deepseek/deepseek-v4" hosted by
    // a custom provider) would be mis-split below and rerouted to the builtin provider.
    for gp in global_providers {
        if let Some(models) = &gp.models {
            // 1. Exact model ID match
            if let Some(m) = models.iter().find(|m| m.id == raw_model) {
                return (gp.id.clone(), m.id.clone());
            }
            // 2. Case-insensitive model ID match
            if let Some(m) = models.iter().find(|m| m.id.eq_ignore_ascii_case(raw_model)) {
                return (gp.id.clone(), m.id.clone());
            }
            // 3. Display name match
            if let Some(m) = models.iter().find(|m| {
                m.name
                    .as_deref()
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case(raw_model))
            }) {
                return (gp.id.clone(), m.id.clone());
            }
            // 4. Model ID suffix match (e.g. "vendor/model-name" matched by "model-name")
            if let Some(m) = models.iter().find(|m| {
                m.id.split('/')
                    .next_back()
                    .is_some_and(|seg| seg.eq_ignore_ascii_case(raw_model))
            }) {
                return (gp.id.clone(), m.id.clone());
            }
        }
    }

    const BUILTIN_PROVIDERS: &[&str] = &[
        "anthropic",
        "openai",
        "google",
        "bedrock",
        "mistral",
        "ollama",
        "azure-openai-responses",
        "openai-codex-responses",
        "deepseek",
        "agentcabin",
        "newapi",
    ];

    if let Some((provider, model_id)) = raw_model.split_once('/') {
        let is_known = BUILTIN_PROVIDERS
            .iter()
            .any(|&bp| bp.eq_ignore_ascii_case(provider))
            || global_providers
                .iter()
                .any(|gp| gp.id == provider || gp.name.eq_ignore_ascii_case(provider));
        if is_known {
            let matched_provider = global_providers
                .iter()
                .find(|gp| gp.id == provider || gp.name.eq_ignore_ascii_case(provider))
                .map(|gp| gp.id.as_str())
                .unwrap_or(provider);
            return (matched_provider.to_string(), model_id.to_string());
        }
    }

    if let Some(pi_p) = pi_provider {
        if pi_p
            .models
            .iter()
            .any(|m| m.id == raw_model || m.id.eq_ignore_ascii_case(raw_model))
            || pi_p.model == raw_model
            || pi_p.model.eq_ignore_ascii_case(raw_model)
        {
            return ("agentcabin".to_string(), raw_model.to_string());
        }
    }

    if pi_provider.is_some() {
        return ("agentcabin".to_string(), raw_model.to_string());
    }

    if let Some(first_gp) = global_providers.first() {
        return (first_gp.id.clone(), raw_model.to_string());
    }

    ("agentcabin".to_string(), raw_model.to_string())
}

fn format_pi_slash_prompt(command: &str, args: &str) -> String {
    if args.is_empty() {
        format!("/{}", command)
    } else {
        format!("/{} {}", command, args)
    }
}

async fn dispatch_pi_slash_command(
    run_id: &str,
    protocol: &mut PiRpc,
    stdin: &mut ChildStdin,
    command: &str,
    args: &str,
    emitter: &Arc<BroadcastEmitter>,
    work_context_plan: Option<&crate::work::context::WorkContextPlan>,
) -> Result<Value, String> {
    if !protocol.commands_loaded() {
        return Err(format!(
            "Command '{}' is not available until Pi command discovery completes (CommandUnavailable)",
            command
        ));
    }

    let resolved_command = protocol.slash_commands().iter().find_map(|c| {
        c.get("name")
            .and_then(Value::as_str)
            .filter(|name| name.eq_ignore_ascii_case(command))
            .map(str::to_string)
            .or_else(|| {
                c.get("aliases").and_then(Value::as_array).and_then(|arr| {
                    arr.iter().find_map(|alias| {
                        alias
                            .as_str()
                            .filter(|alias| alias.eq_ignore_ascii_case(command))
                            .map(|_| {
                                c.get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or(command)
                                    .to_string()
                            })
                    })
                })
            })
    });
    let resolved_command = resolved_command.or_else(|| {
        let skill_command = format!("skill:{}", command);
        protocol.slash_commands().iter().find_map(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .filter(|name| name.eq_ignore_ascii_case(&skill_command))
                .map(str::to_string)
        })
    });

    let Some(resolved_command) = resolved_command else {
        return Err(format!(
            "Command '{}' is not available in the current Pi session (CommandUnavailable)",
            command
        ));
    };

    let prompt_text = format_pi_slash_prompt(&resolved_command, args);
    let work_context = work_turn_context(run_id, work_context_plan)?;
    let effective_prompt = format!("{prompt_text}{work_context}");
    let frames = protocol.frame_user_turn(&effective_prompt, &[], &[], &Default::default());
    write_frames(stdin, &frames).await?;
    // Skill commands carry the user's actual instruction in `args`, but this path bypasses
    // ActorCommand::SendMessage and therefore has no normal UserMessage emission. Persist the
    // complete slash prompt so the timeline shows what the user submitted.
    if resolved_command
        .get(.."skill:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("skill:"))
    {
        emit_user_message(emitter, run_id, &prompt_text, &[]);
        mark_running(emitter, run_id);
    }
    let pending_id = frames.first().and_then(|frame| frame.get("id")).cloned();
    Ok(json!({"__pending_rpc_id": pending_id}))
}

fn apply_permission_mode_control(
    protocol: &mut PiRpc,
    agent_dir: &Path,
    mode: &str,
) -> Result<Value, String> {
    apply_permission_mode_control_with(
        protocol,
        mode,
        |mode| crate::agent::pi_permission::sync_for_control(agent_dir, mode),
        // Project ownership is persisted by the caller. The actor only applies
        // the mode to this Pi process and its managed extension config.
        |_| Ok(()),
    )
}

fn pending_permission_approvals(protocol: &PiRpc) -> Vec<(String, String)> {
    protocol
        .host()
        .pending_requests()
        .iter()
        .filter(|request| {
            request.method == PiExtensionUiMethod::Select
                && (request
                    .title
                    .as_deref()
                    .is_some_and(|title| title.starts_with("Permission Required"))
                    || request
                        .message
                        .as_deref()
                        .is_some_and(|message| message.starts_with("Permission Required")))
        })
        .filter_map(|request| {
            request
                .options
                .iter()
                .find(|option| option.as_str() == "Yes")
                .map(|value| (request.id.clone(), value.clone()))
        })
        .collect()
}

async fn resolve_pending_permission_requests(
    protocol: &mut PiRpc,
    stdin: &mut ChildStdin,
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
) -> Result<usize, String> {
    let approvals = pending_permission_approvals(protocol);
    let mut resolved = 0;
    for (request_id, value) in approvals {
        write_interactive_response(
            protocol,
            stdin,
            PendingKind::Elicitation,
            &request_id,
            json!({"action": "accept", "content": {"value": value}}),
        )
        .await?;
        if protocol
            .host_mut()
            .remove_pending_request(&request_id)
            .is_some()
        {
            resolved += 1;
            emitter.persist_and_emit(
                run_id,
                &BusEvent::PiExtensionUiRequestResolved {
                    run_id: run_id.to_string(),
                    request_id,
                },
            );
        }
    }
    if resolved > 0 {
        emitter.persist_and_emit(
            run_id,
            &BusEvent::PiExtensionStateSync {
                run_id: run_id.to_string(),
                snapshot: protocol.host_snapshot(),
            },
        );
    }
    Ok(resolved)
}

fn apply_permission_mode_control_with<Sync, Persist>(
    protocol: &mut PiRpc,
    mode: &str,
    sync: Sync,
    persist: Persist,
) -> Result<Value, String>
where
    Sync: FnOnce(&str) -> Result<(), String>,
    Persist: FnOnce(&str) -> Result<(), String>,
{
    let mode = crate::agent::pi_permission::normalize_permission_mode(mode);

    sync(mode)?;
    persist(mode)?;
    protocol.configure_permission_mode(Some(mode));

    // The extension re-reads config in before_agent_start. Returning directly
    // is intentional: sending `/reload` through RPC turns it into a user prompt.
    Ok(json!({
        "ok": true,
        "mode": mode,
        "appliesOnNextTurn": true,
    }))
}

async fn write_interactive_response(
    protocol: &mut PiRpc,
    stdin: &mut ChildStdin,
    kind: PendingKind,
    request_id: &str,
    response: Value,
) -> Result<(), String> {
    let frames = protocol.frame_response(kind, request_id, response);
    write_frames(stdin, &frames).await
}

async fn write_frames(stdin: &mut ChildStdin, frames: &[Value]) -> Result<(), String> {
    for frame in frames {
        write_json_line(stdin, frame).await?;
    }
    Ok(())
}

const PI_SHUTDOWN_GRACE: Duration = Duration::from_millis(750);

async fn wait_for_pi_child(child: &mut Child) -> bool {
    matches!(
        tokio::time::timeout(PI_SHUTDOWN_GRACE, child.wait()).await,
        Ok(Ok(_))
    )
}

/// Stop Pi cooperatively before forcing the process down. The order matters:
/// abort an active turn, resolve pending Extension UI, allow Pi a bounded chance
/// to exit, then kill if needed. Host state is cleared only after the process is gone.
async fn shutdown_pi_process(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    protocol: &mut PiRpc,
    stdin: &mut ChildStdin,
    child: &mut Child,
    is_streaming: bool,
) {
    if is_streaming {
        let abort_frames = protocol.frame_interrupt();
        let _ = write_frames(stdin, &abort_frames).await;
    }

    let (cancel_frames, cancelled_ids) = protocol.pending_cancellation_frames();
    if !cancel_frames.is_empty() {
        let _ = write_frames(stdin, &cancel_frames).await;
        for request_id in cancelled_ids {
            emitter.persist_and_emit(
                run_id,
                &BusEvent::PiExtensionUiRequestResolved {
                    run_id: run_id.to_string(),
                    request_id,
                },
            );
        }
    }

    let _ = stdin.shutdown().await;
    if !wait_for_pi_child(child).await {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    protocol.clear_host();
}

pub(super) async fn write_json_line(stdin: &mut ChildStdin, message: &Value) -> Result<(), String> {
    let mut line = serde_json::to_string(message).map_err(|error| error.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|error| format!("Pi RPC stdin write failed: {}", error))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Pi RPC stdin flush failed: {}", error))
}

async fn cleanup_actor(
    sessions: &ActorSessionMap,
    run_id: &str,
    tag: &Arc<()>,
    browser_token: Option<&str>,
    connector_token: Option<&str>,
    desktop_token: Option<&str>,
    work_bridge_token: Option<&str>,
) {
    // Code's browser bridge and the process-wide Playwright worker are not
    // owned by the Pi child process. Clean both up when the actor exits
    // naturally as well as when the user explicitly stops/replaces it. Work
    // uses the WorkRun id for its browser context, so these calls are no-ops
    // for the Work session id and its lifecycle remains Work-owned.
    if let Some(token) = browser_token {
        crate::browser_runtime::revoke_token(token).await;
    }
    if let Some(token) = connector_token {
        crate::code_connector_runtime::revoke_token(token).await;
    }
    if let Some(token) = desktop_token {
        crate::desktop_runtime::revoke_token(token).await;
    }
    if let Some(token) = work_bridge_token {
        // WorkRun completion is turn-scoped; this process-scoped lease remains
        // valid while the Pi actor is idle and is revoked only after Pi exits.
        crate::work::internal_bridge::revoke_session_token(token).await;
    }
    let _ = crate::work::browser_operator::browser_operator_manager()
        .close_context(run_id)
        .await;

    let mut map = sessions.lock().await;
    let remove = map
        .get(run_id)
        .map(|handle| Arc::ptr_eq(&handle.tag, tag))
        .unwrap_or(false);
    if remove {
        map.remove(run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::session_protocol::SessionProtocol;
    use std::process::Stdio;
    use tokio::process::Command;

    #[test]
    fn shutdown_frames_abort_before_extension_ui_cancellation() {
        let mut protocol = PiRpc::new();
        protocol.parse_line(
            "test-run",
            r#"{"type":"extension_ui_request","id":"ui-1","method":"confirm"}"#,
        );

        let abort = protocol.frame_interrupt();
        let (cancel, ids) = protocol.pending_cancellation_frames();
        assert_eq!(abort.len(), 2);
        assert_eq!(abort[0]["type"], "clear_queue");
        assert_eq!(abort[1]["type"], "abort");
        assert_eq!(cancel[0]["type"], "extension_ui_response");
        assert_eq!(cancel[0]["cancelled"], true);
        assert_eq!(ids, vec!["ui-1"]);
        assert_eq!(protocol.host().pending_requests().len(), 1);

        protocol.clear_host();
        assert!(protocol.host().pending_requests().is_empty());
    }

    #[test]
    fn only_pending_pi_permission_selects_are_auto_approved() {
        let mut protocol = PiRpc::new();
        protocol.parse_line(
            "test-run",
            r#"{"type":"extension_ui_request","id":"permission-1","method":"select","title":"Permission Required","options":["Yes","No"]}"#,
        );
        protocol.parse_line(
            "test-run",
            r#"{"type":"extension_ui_request","id":"question-1","method":"select","title":"Choose a model","options":["Yes","No"]}"#,
        );

        assert_eq!(
            pending_permission_approvals(&protocol),
            vec![("permission-1".to_string(), "Yes".to_string())]
        );
    }

    #[tokio::test]
    async fn stop_or_interrupt_control_prioritizes_clear_queue_before_abort() {
        use tokio::io::AsyncReadExt;

        let mut protocol = PiRpc::new();
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("cat is available for actor pipe test");

        let mut stdin = child.stdin.take().expect("child stdin");
        let mut stdout = child.stdout.take().expect("child stdout");
        let emitter = BroadcastEmitter::mock();

        let result = handle_control(
            "test-run",
            &mut protocol,
            &mut stdin,
            &json!({"subtype": "interrupt"}),
            &emitter,
            false,
            None,
            None,
        )
        .await;
        assert!(result.is_ok());

        drop(stdin);
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.expect("read stdout");
        let _ = child.wait().await;

        let output = String::from_utf8(buf).expect("valid utf8");
        let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 2);
        let frame0: Value = serde_json::from_str(lines[0]).expect("valid json frame 0");
        let frame1: Value = serde_json::from_str(lines[1]).expect("valid json frame 1");
        assert_eq!(frame0["type"], "clear_queue");
        assert_eq!(frame1["type"], "abort");
    }

    #[tokio::test]
    async fn explicit_clear_queue_control_writes_clear_queue_rpc_frame() {
        use tokio::io::AsyncReadExt;

        let mut protocol = PiRpc::new();
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("cat is available for actor pipe test");

        let mut stdin = child.stdin.take().expect("child stdin");
        let mut stdout = child.stdout.take().expect("child stdout");
        let emitter = BroadcastEmitter::mock();

        let result = handle_control(
            "test-run",
            &mut protocol,
            &mut stdin,
            &json!({"subtype": "clear_queue"}),
            &emitter,
            false,
            None,
            None,
        )
        .await
        .expect("clear_queue control succeeds");

        assert!(result.get("__pending_rpc_id").is_some());

        drop(stdin);
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.expect("read stdout");
        let _ = child.wait().await;

        let output = String::from_utf8(buf).expect("valid utf8");
        let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 1);
        let frame: Value = serde_json::from_str(lines[0]).expect("valid json frame");
        assert_eq!(frame["type"], "clear_queue");
    }

    #[tokio::test]
    async fn effort_control_waits_for_pi_rpc_response() {
        use tokio::io::AsyncReadExt;

        let mut protocol = PiRpc::new();
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("cat is available for actor pipe test");

        let mut stdin = child.stdin.take().expect("child stdin");
        let mut stdout = child.stdout.take().expect("child stdout");
        let emitter = BroadcastEmitter::mock();

        let result = handle_control(
            "test-run",
            &mut protocol,
            &mut stdin,
            &json!({"subtype": "set_effort", "effort": "high"}),
            &emitter,
            false,
            None,
            None,
        )
        .await
        .expect("effort control writes successfully");

        let pending_id = result
            .get("__pending_rpc_id")
            .and_then(Value::as_str)
            .expect("effort control should expose the Pi RPC id");
        assert!(pending_id.starts_with("agentcabin-pi-thinking-"));

        drop(stdin);
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.expect("read stdout");
        let _ = child.wait().await;

        let output = String::from_utf8(buf).expect("valid utf8");
        let frame: Value = serde_json::from_str(output.trim()).expect("valid json frame");
        assert_eq!(frame["type"], "set_thinking_level");
        assert_eq!(frame["level"], "high");
    }

    #[test]
    fn permission_mode_control_applies_without_an_rpc_prompt() {
        let mut protocol = PiRpc::new();
        let mut synced = None;
        let mut persisted = None;

        let response = apply_permission_mode_control_with(
            &mut protocol,
            "auto_approve",
            |mode| {
                synced = Some(mode.to_string());
                Ok(())
            },
            |mode| {
                persisted = Some(mode.to_string());
                Ok(())
            },
        )
        .expect("permission mode applies without sending a Pi prompt");

        assert_eq!(synced.as_deref(), Some("auto_approve"));
        assert_eq!(persisted.as_deref(), Some("auto_approve"));
        assert_eq!(response["ok"], true);
        assert_eq!(response["mode"], "auto_approve");
        assert_eq!(response["appliesOnNextTurn"], true);
        assert!(response.get("__pending_rpc_id").is_none());
    }

    #[test]
    fn slash_skill_prompt_preserves_the_user_text_for_timeline() {
        assert_eq!(
            format_pi_slash_prompt("skill:grilling", "analyze this plan"),
            "/skill:grilling analyze this plan"
        );
        assert_eq!(
            format_pi_slash_prompt("skill:grilling", ""),
            "/skill:grilling"
        );
    }

    #[test]
    fn qualified_model_id_splits_across_providers() {
        assert_eq!(
            resolve_set_model_target("deepseek/deepseek-v4-flash", None, &[], None),
            ("deepseek".to_string(), "deepseek-v4-flash".to_string(),)
        );
        assert_eq!(
            resolve_set_model_target("unqualified-model", None, &[], None),
            ("agentcabin".to_string(), "unqualified-model".to_string())
        );
        assert_eq!(
            resolve_set_model_target("newapi/claude-3-7-sonnet", Some("newapi"), &[], None),
            ("newapi".to_string(), "claude-3-7-sonnet".to_string())
        );
    }

    #[test]
    fn native_provider_model_ids_still_split_when_not_managed() {
        assert_eq!(
            resolve_set_model_target("anthropic/claude-sonnet", None, &[], None),
            ("anthropic".to_string(), "claude-sonnet".to_string())
        );
    }

    #[test]
    fn model_id_with_slashes_resolves_to_correct_global_provider() {
        let global_providers: Vec<crate::models::GlobalProviderCredential> =
            serde_json::from_value(json!([{
                "id": "provider-1788769121841",
                "name": "NEWAPI",
                "base_url": "https://api.example.com",
                "protocol": "openai-completions",
                "models": [{
                    "id": "zhanlu/kimi-k3",
                    "name": "zhanlu/kimi-k3",
                    "context_window": 128000,
                    "supports_reasoning": true
                }]
            }]))
            .unwrap();

        // When fully qualified with provider ID:
        assert_eq!(
            resolve_set_model_target(
                "provider-1788769121841/zhanlu/kimi-k3",
                None,
                &global_providers,
                None
            ),
            (
                "provider-1788769121841".to_string(),
                "zhanlu/kimi-k3".to_string()
            )
        );

        // When raw model ID with slashes is passed without provider prefix:
        assert_eq!(
            resolve_set_model_target("zhanlu/kimi-k3", None, &global_providers, None),
            (
                "provider-1788769121841".to_string(),
                "zhanlu/kimi-k3".to_string()
            )
        );
    }

    #[test]
    fn bare_model_id_with_builtin_prefix_stays_on_custom_provider() {
        // A custom global provider hosts a model whose ID starts with a builtin provider
        // name segment. The bare ID must resolve to the custom provider instead of being
        // mis-split and rerouted to the builtin "deepseek" provider.
        let global_providers: Vec<crate::models::GlobalProviderCredential> =
            serde_json::from_value(json!([{
                "id": "provider-custom",
                "name": "Custom Gateway",
                "base_url": "https://api.example.com",
                "protocol": "openai-completions",
                "models": [{
                    "id": "deepseek/deepseek-v4",
                    "name": "deepseek/deepseek-v4",
                    "context_window": 128000,
                    "supports_reasoning": true
                }]
            }]))
            .unwrap();

        // Bare model ID whose first segment collides with the builtin "deepseek" provider:
        assert_eq!(
            resolve_set_model_target("deepseek/deepseek-v4", None, &global_providers, None),
            (
                "provider-custom".to_string(),
                "deepseek/deepseek-v4".to_string()
            )
        );

        // Fully qualified ID still resolves correctly:
        assert_eq!(
            resolve_set_model_target(
                "provider-custom/deepseek/deepseek-v4",
                None,
                &global_providers,
                None
            ),
            (
                "provider-custom".to_string(),
                "deepseek/deepseek-v4".to_string()
            )
        );
    }

    #[test]
    fn bare_model_resolves_via_display_name_and_trailing_segment() {
        let global_providers: Vec<crate::models::GlobalProviderCredential> =
            serde_json::from_value(json!([{
                "id": "provider-newapi-123",
                "name": "NewAPI",
                "base_url": "https://api.example.com",
                "protocol": "openai-completions",
                "models": [
                    {
                        "id": "Qwen/Qwen3.8-27B",
                        "name": "Qwen3.8-27B",
                        "context_window": 128000,
                        "supports_reasoning": true
                    },
                    {
                        "id": "deepseek-ai/DeepSeek-V3",
                        "name": "DeepSeek V3",
                        "context_window": 64000,
                        "supports_reasoning": false
                    }
                ]
            }]))
            .unwrap();

        // 1. Matches via display name ("Qwen3.8-27B") -> returns actual model id ("Qwen/Qwen3.8-27B")
        assert_eq!(
            resolve_set_model_target("Qwen3.8-27B", None, &global_providers, None),
            (
                "provider-newapi-123".to_string(),
                "Qwen/Qwen3.8-27B".to_string()
            )
        );

        // 2. Matches via trailing segment ("DeepSeek-V3") -> returns actual model id
        assert_eq!(
            resolve_set_model_target("DeepSeek-V3", None, &global_providers, None),
            (
                "provider-newapi-123".to_string(),
                "deepseek-ai/DeepSeek-V3".to_string()
            )
        );

        // 3. Provider name as prefix ("NewAPI/Qwen3.8-27B") resolves to provider ID
        assert_eq!(
            resolve_set_model_target("NewAPI/Qwen3.8-27B", None, &global_providers, None),
            ("provider-newapi-123".to_string(), "Qwen3.8-27B".to_string())
        );

        // 4. Unknown model with no pi_provider falls back to the first global provider instead of phantom "agentcabin"
        assert_eq!(
            resolve_set_model_target("unknown-model", None, &global_providers, None),
            (
                "provider-newapi-123".to_string(),
                "unknown-model".to_string()
            )
        );
    }

    #[test]
    fn permission_mode_control_normalizes_claude_edit_mode() {
        let mut protocol = PiRpc::new();
        let mut synced = None;
        let mut persisted = None;

        let response = apply_permission_mode_control_with(
            &mut protocol,
            "acceptEdits",
            |mode| {
                synced = Some(mode.to_string());
                Ok(())
            },
            |mode| {
                persisted = Some(mode.to_string());
                Ok(())
            },
        )
        .expect("Claude edit mode should map to the Pi edit policy");

        assert_eq!(synced.as_deref(), Some("accept_edits"));
        assert_eq!(persisted.as_deref(), Some("accept_edits"));
        assert_eq!(response["mode"], "accept_edits");
    }

    #[tokio::test]
    async fn shutdown_wait_is_bounded_before_force_kill() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 5"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("shell is available for shutdown test");

        let started = Instant::now();
        let exited = wait_for_pi_child(&mut child).await;
        assert!(!exited);
        assert!(started.elapsed() < Duration::from_secs(2));
        child.kill().await.expect("test child can be killed");
        child.wait().await.expect("test child can be reaped");
    }

    #[tokio::test]
    async fn actor_cleanup_revokes_only_its_work_bridge_lease() {
        let run_id = format!("actor-cleanup-{}", uuid::Uuid::new_v4());
        let old_token = format!("actor-old-{}", uuid::Uuid::new_v4());
        let replacement_token = format!("actor-replacement-{}", uuid::Uuid::new_v4());
        let state = crate::work::internal_bridge::bridge_state();
        let make_info = |token: &str| crate::work::internal_bridge::ProcessBridgeTokenInfo {
            token: token.to_string(),
            run_id: run_id.clone(),
            task_id: None,
            workspace_id: "workspace-actor-cleanup".to_string(),
            execution_context: crate::work::models::ExecutionContext::Attended,
            proxy_url: None,
        };
        {
            let mut tokens = state.tokens.write().await;
            tokens.insert(old_token.clone(), make_info(&old_token));
            tokens.insert(replacement_token.clone(), make_info(&replacement_token));
        }

        cleanup_actor(
            &crate::agent::adapter::new_actor_session_map(),
            &run_id,
            &Arc::new(()),
            None,
            None,
            None,
            Some(&old_token),
        )
        .await;

        let tokens = state.tokens.read().await;
        assert!(!tokens.contains_key(&old_token));
        assert!(tokens.contains_key(&replacement_token));
        drop(tokens);
        crate::work::internal_bridge::revoke_session_token(&replacement_token).await;
    }

    #[test]
    fn work_turn_context_injects_active_context_plan_for_work_runs() {
        let plan = crate::work::context::WorkContextPlan {
            version: 1,
            workspace_id: Some("ws-test".to_string()),
            task_id: None,
            run_id: "turn-ctx-1".to_string(),
            session_id: Some("session-1".to_string()),
            runtime: crate::agent::capability_resolver::RuntimeProviderKind::Pi,
            preset: Some(crate::work::models::WorkPreset::Office),
            segments: vec![crate::work::context::WorkContextSegment {
                id: "task_state:goal".to_string(),
                kind: crate::work::context::WorkContextKind::CurrentGoal,
                source: crate::work::context::WorkContextSource::TaskState,
                title: "Active Goal".to_string(),
                required: true,
                selection: crate::work::context::WorkContextSelection::Selected,
                reason: None,
                render_priority: 70,
                budget_priority: 70,
                estimated_tokens: 30,
                ref_id: None,
                content: Some("Active Goal: Compile Financial Statements".to_string()),
                metadata: std::collections::BTreeMap::new(),
            }],
            estimated_tokens: 30,
            created_at: "2026-08-15T00:00:00Z".to_string(),
        };

        let turn_ctx = work_turn_context_for_run(&plan.run_id, true, Some(&plan)).unwrap();
        assert!(turn_ctx.contains("[Active Work Context Plan]"));
        assert!(turn_ctx.contains("Active Goal: Compile Financial Statements"));
    }

    #[test]
    fn work_turn_context_rejects_missing_or_mismatched_plan() {
        assert!(work_turn_context_for_run("turn-ctx-1", true, None).is_err());

        let mut plan = crate::work::context::WorkContextPlan {
            version: 1,
            workspace_id: None,
            task_id: None,
            run_id: "other-run".to_string(),
            session_id: None,
            runtime: crate::agent::capability_resolver::RuntimeProviderKind::Pi,
            preset: None,
            segments: vec![],
            estimated_tokens: 0,
            created_at: "2026-08-15T00:00:00Z".to_string(),
        };
        assert!(work_turn_context_for_run("turn-ctx-1", true, Some(&plan)).is_err());

        plan.run_id = "turn-ctx-1".to_string();
        assert!(work_turn_context_for_run("turn-ctx-1", true, Some(&plan)).is_err());
    }

    #[test]
    fn non_work_turn_does_not_require_a_context_plan() {
        assert_eq!(
            work_turn_context_for_run("code-run", false, None).unwrap(),
            ""
        );
    }

    #[tokio::test]
    async fn actor_mailbox_keeps_the_context_plan_with_its_turn() {
        let make_plan = |content: &str| crate::work::context::WorkContextPlan {
            version: 1,
            workspace_id: None,
            task_id: None,
            run_id: "queued-work-run".to_string(),
            session_id: None,
            runtime: crate::agent::capability_resolver::RuntimeProviderKind::Pi,
            preset: None,
            segments: vec![crate::work::context::WorkContextSegment {
                id: "turn:context".to_string(),
                kind: crate::work::context::WorkContextKind::Conversation,
                source: crate::work::context::WorkContextSource::UserPrompt,
                title: "Turn Context".to_string(),
                required: true,
                selection: crate::work::context::WorkContextSelection::Selected,
                reason: None,
                render_priority: 1,
                budget_priority: 1,
                estimated_tokens: 1,
                ref_id: None,
                content: Some(content.to_string()),
                metadata: std::collections::BTreeMap::new(),
            }],
            estimated_tokens: 1,
            created_at: "2026-08-15T00:00:00Z".to_string(),
        };
        let plan_a = make_plan("context-a");
        let plan_b = make_plan("context-b");
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);

        for (text, plan) in [("message-a", plan_a), ("message-b", plan_b)] {
            let (reply, _) = tokio::sync::oneshot::channel();
            tx.send(ActorCommand::SendMessage {
                text: text.to_string(),
                attachments: Vec::new(),
                skills: Vec::new(),
                work_context_plan: Some(plan),
                reply,
            })
            .await
            .unwrap();
        }

        for (expected_text, expected_context) in
            [("message-a", "context-a"), ("message-b", "context-b")]
        {
            let Some(ActorCommand::SendMessage {
                text,
                work_context_plan: Some(plan),
                ..
            }) = rx.recv().await
            else {
                panic!("expected a queued Work message with a Context Plan");
            };
            assert_eq!(text, expected_text);
            assert_eq!(plan.segments[0].content.as_deref(), Some(expected_context));
        }
    }
}
