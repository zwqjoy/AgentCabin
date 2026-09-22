//! Pi RPC event and attachment helpers.

use crate::agent::pi_rpc_protocol::{PiRpc, PiRpcImage};
use crate::agent::session_actor::AttachmentData;
use crate::agent::session_protocol::{LifecycleSignal, SessionProtocol};
use crate::models::{AttachmentMeta, BusEvent, RunStatus};
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use std::sync::Arc;

pub(super) fn pi_images(attachments: &[AttachmentData]) -> Vec<PiRpcImage> {
    attachments
        .iter()
        .filter(|attachment| {
            attachment.media_type.starts_with("image/") && !attachment.content_base64.is_empty()
        })
        .map(|attachment| PiRpcImage {
            data: attachment.content_base64.clone(),
            mime_type: attachment.media_type.clone(),
        })
        .collect()
}

pub(super) fn emit_user_message(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    text: &str,
    attachments: &[AttachmentData],
) {
    let attachments = attachments
        .iter()
        .map(|attachment| AttachmentMeta {
            name: attachment.filename.clone(),
            mime_type: attachment.media_type.clone(),
            size: 0,
        })
        .collect();
    emitter.persist_and_emit(
        run_id,
        &BusEvent::UserMessage {
            run_id: run_id.to_string(),
            text: text.to_string(),
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            client_uuid: None,
            attachments,
        },
    );
}

pub(super) fn mark_running(emitter: &Arc<BroadcastEmitter>, run_id: &str) {
    if let Err(error) = storage::runs::update_status(run_id, RunStatus::Running, None, None) {
        log::warn!(
            "[pi_rpc_actor] failed to persist running status: run_id={}, error={}",
            run_id,
            error
        );
    } else {
        emit_status_changed(emitter, run_id, RunStatus::Running);
    }
    emitter.persist_and_emit(
        run_id,
        &BusEvent::RunState {
            run_id: run_id.to_string(),
            state: "running".to_string(),
            exit_code: None,
            error: None,
        },
    );
}

pub(super) fn mark_idle(emitter: &Arc<BroadcastEmitter>, run_id: &str) {
    if let Err(error) = storage::runs::update_status(run_id, RunStatus::Idle, None, None) {
        log::warn!(
            "[pi_rpc_actor] failed to persist idle status: run_id={}, error={}",
            run_id,
            error
        );
    } else {
        emit_status_changed(emitter, run_id, RunStatus::Idle);
    }
}

pub(super) async fn handle_stdout_line(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    protocol: &mut PiRpc,
    line: &str,
    session_persistence_enabled: bool,
    stdin: &mut tokio::process::ChildStdin,
) -> Option<LifecycleSignal> {
    let mut parsed = protocol.parse_line(run_id, line);
    handle_retry_line(run_id, line, protocol, &mut parsed.lifecycle);
    let transient_work_error = matches!(parsed.lifecycle, Some(LifecycleSignal::TurnCompleted))
        && is_work_run(run_id)
        && protocol.has_pending_provider_error();

    for frame in &parsed.immediate_frames {
        if let Err(error) = super::actor_loop::write_json_line(stdin, frame).await {
            log::warn!(
                "[pi_rpc_actor] failed to write immediate response frame: {}",
                error
            );
        }
    }

    if session_persistence_enabled {
        if let Some(session_id) = parsed.thread_id.as_deref() {
            if let Err(error) = storage::runs::update_session_id(run_id, session_id) {
                log::warn!("[pi_rpc_actor] failed to persist session id: {}", error);
            }
        }
    }

    for event in parsed.events {
        let event = if session_persistence_enabled {
            event
        } else {
            remove_session_id(event)
        };
        if let BusEvent::SessionInit {
            model: Some(model), ..
        } = &event
        {
            if let Err(error) = storage::runs::update_run_model(run_id, model) {
                log::warn!("[pi_rpc_actor] failed to persist active model: {}", error);
            }
        }
        emitter.persist_and_emit(run_id, &event);
    }

    match parsed.lifecycle {
        Some(LifecycleSignal::TurnStarted) => {
            if matches!(
                protocol.work_retry_state,
                crate::agent::pi_rpc_protocol::WorkRetryState::WaitingForRetryStart(_)
            ) {
                protocol.work_retry_state = crate::agent::pi_rpc_protocol::WorkRetryState::Retrying(
                    std::time::Instant::now() + std::time::Duration::from_secs(120),
                );
            }
            mark_running(emitter, run_id);
            Some(LifecycleSignal::TurnStarted)
        }
        Some(LifecycleSignal::TurnCompleted) => {
            // Pi emits `agent_end` for a provider error immediately before
            // `auto_retry_start`. Treating that transient boundary as Work
            // completion races the retry and makes the next tool call fail
            // with "This WorkRun is no longer active". Keep Work running
            // until a retry produces a normal settled turn.
            if transient_work_error {
                protocol.work_retry_state =
                    crate::agent::pi_rpc_protocol::WorkRetryState::WaitingForRetryStart(
                        std::time::Instant::now() + std::time::Duration::from_secs(15),
                    );
                mark_running(emitter, run_id);
                emitter.persist_and_emit(
                    run_id,
                    &BusEvent::RunState {
                        run_id: run_id.to_string(),
                        state: "running".to_string(),
                        exit_code: None,
                        error: None,
                    },
                );
                // Return None — do NOT signal TurnCompleted to actor_loop.
                // Returning TurnCompleted would cause actor_loop to call
                // handle_turn_settled(Success), prematurely closing the
                // WorkRun before the auto-retry resumes execution.
                return None;
            }
            mark_idle(emitter, run_id);
            protocol.work_retry_state = crate::agent::pi_rpc_protocol::WorkRetryState::None;
            emitter.persist_and_emit(
                run_id,
                &BusEvent::RunState {
                    run_id: run_id.to_string(),
                    state: "idle".to_string(),
                    exit_code: None,
                    error: None,
                },
            );
            Some(LifecycleSignal::TurnCompleted)
        }
        Some(LifecycleSignal::TurnFailed(error)) => {
            protocol.work_retry_state = crate::agent::pi_rpc_protocol::WorkRetryState::None;
            if let Err(persist_error) =
                storage::runs::update_status(run_id, RunStatus::Idle, None, error.clone())
            {
                log::warn!(
                    "[pi_rpc_actor] failed to persist idle-after-error status: run_id={}, error={}",
                    run_id,
                    persist_error
                );
            } else {
                emit_status_changed(emitter, run_id, RunStatus::Idle);
            }
            emitter.persist_and_emit(
                run_id,
                &BusEvent::RunState {
                    run_id: run_id.to_string(),
                    state: "idle".to_string(),
                    exit_code: None,
                    error: error.clone(),
                },
            );
            Some(LifecycleSignal::TurnFailed(error))
        }
        None => None,
    }
}

fn handle_retry_line(
    run_id: &str,
    line: &str,
    protocol: &mut PiRpc,
    lifecycle: &mut Option<LifecycleSignal>,
) {
    handle_retry_line_inner(is_work_run(run_id), line, protocol, lifecycle);
}

fn handle_retry_line_inner(
    is_work: bool,
    line: &str,
    protocol: &mut PiRpc,
    lifecycle: &mut Option<LifecycleSignal>,
) {
    if is_work {
        let raw: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        if raw["type"] == "auto_retry_start" {
            protocol.work_retry_state = crate::agent::pi_rpc_protocol::WorkRetryState::Retrying(
                std::time::Instant::now() + std::time::Duration::from_secs(120),
            );
        } else if raw["type"] == "auto_retry_end" {
            protocol.work_retry_state = crate::agent::pi_rpc_protocol::WorkRetryState::None;
            if raw["success"] == false {
                *lifecycle = Some(LifecycleSignal::TurnFailed(Some(
                    raw["finalError"]
                        .as_str()
                        .unwrap_or("Pi automatic retry exhausted")
                        .to_string(),
                )));
            }
        }
    }
}

fn is_work_run(run_id: &str) -> bool {
    storage::runs::get_run(run_id)
        .is_some_and(|run| run.app_mode == crate::work::models::AppMode::Work)
}

fn remove_session_id(event: BusEvent) -> BusEvent {
    match event {
        BusEvent::SessionInit {
            run_id,
            model,
            model_options,
            tools,
            cwd,
            slash_commands,
            commands_loaded,
            mcp_servers,
            permission_mode,
            api_key_source,
            claude_code_version,
            output_style,
            agents,
            skills,
            plugins,
            plugin_errors,
            fast_mode_state,
            capabilities,
            ..
        } => BusEvent::SessionInit {
            run_id,
            session_id: None,
            model,
            model_options,
            tools,
            cwd,
            slash_commands,
            commands_loaded,
            mcp_servers,
            permission_mode,
            api_key_source,
            claude_code_version,
            output_style,
            agents,
            skills,
            plugins,
            plugin_errors,
            fast_mode_state,
            capabilities,
        },
        other => other,
    }
}

pub(super) fn mark_failed(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    exit_code: Option<i32>,
    error: String,
) {
    if let Err(persist_error) =
        storage::runs::update_status(run_id, RunStatus::Failed, exit_code, Some(error.clone()))
    {
        log::warn!(
            "[pi_rpc_actor] failed to persist failed status: run_id={}, error={}",
            run_id,
            persist_error
        );
    } else {
        emit_status_changed(emitter, run_id, RunStatus::Failed);
    }
    emitter.persist_and_emit(
        run_id,
        &BusEvent::RunState {
            run_id: run_id.to_string(),
            state: "failed".to_string(),
            exit_code,
            error: Some(error),
        },
    );
}

fn emit_status_changed(emitter: &Arc<BroadcastEmitter>, run_id: &str, status: RunStatus) {
    emitter.emit_realtime(
        "agentcabin:status-changed",
        &serde_json::json!({"run_id": run_id, "status": status.to_string()}),
        Some(run_id),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn retry_events_update_work_retry_state_and_lifecycle() {
        use super::handle_retry_line_inner;
        use crate::agent::pi_rpc_protocol::{PiRpc, WorkRetryState};
        use crate::agent::session_protocol::LifecycleSignal;

        let mut protocol = PiRpc::default();
        let mut lifecycle = None;

        // Non-work run ignores auto_retry_start
        handle_retry_line_inner(
            false,
            r#"{"type":"auto_retry_start"}"#,
            &mut protocol,
            &mut lifecycle,
        );
        assert_eq!(protocol.work_retry_state, WorkRetryState::None);

        // Work run transitions to Retrying
        handle_retry_line_inner(
            true,
            r#"{"type":"auto_retry_start"}"#,
            &mut protocol,
            &mut lifecycle,
        );
        assert!(matches!(
            protocol.work_retry_state,
            WorkRetryState::Retrying(_)
        ));

        // auto_retry_end success resets retry state without failing turn
        protocol.work_retry_state = WorkRetryState::Retrying(std::time::Instant::now());
        lifecycle = None;
        handle_retry_line_inner(
            true,
            r#"{"type":"auto_retry_end","success":true}"#,
            &mut protocol,
            &mut lifecycle,
        );
        assert_eq!(protocol.work_retry_state, WorkRetryState::None);
        assert_eq!(lifecycle, None);

        // auto_retry_end failure resets retry state and fails turn
        handle_retry_line_inner(
            true,
            r#"{"type":"auto_retry_end","success":false,"finalError":"Rate limit exhausted"}"#,
            &mut protocol,
            &mut lifecycle,
        );
        assert_eq!(protocol.work_retry_state, WorkRetryState::None);
        assert_eq!(
            lifecycle,
            Some(LifecycleSignal::TurnFailed(Some(
                "Rate limit exhausted".to_string()
            )))
        );
    }
}
