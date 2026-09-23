use std::sync::Arc;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::tasks::start_work_task_run;
use super::{ensure_work_enabled, resume_work_session_after_approval, INBOX_DELIVERY_LOCK};
use crate::agent::adapter::ActorSessionMap;
use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::agent::session_actor::AttachmentData;
use crate::agent::spawn_locks::SpawnLocks;
use crate::models::TaskRun;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::models::{
    InboxItemStatus, WorkPreset, WorkRecoveryAction, WorkRun, WorkRunProgressView, WorkRunRecovery,
    WorkRunStatus, WorkRunTrigger, WorkTask, WorkTaskSource,
};
use crate::work::runtime::WorkRuntimeAdapter;
use crate::work::{artifacts, paths::WorkPaths, session, tasks::TaskManager, workspace};

#[tauri::command]
pub async fn work_get_session(workspace_id: String) -> Result<Option<TaskRun>, String> {
    ensure_work_enabled()?;
    tokio::task::spawn_blocking(move || session::latest_session(&workspace_id))
        .await
        .map_err(|error| format!("读取 Work 会话失败: {error}"))?
}

#[tauri::command]
pub async fn work_list_sessions(workspace_id: String) -> Result<Vec<TaskRun>, String> {
    ensure_work_enabled()?;
    tokio::task::spawn_blocking(move || session::list_sessions(&workspace_id))
        .await
        .map_err(|error| format!("读取 Work 对话列表失败: {error}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn work_start_session(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    workspace_id: String,
    message: String,
    model: Option<String>,
    attachments: Option<Vec<AttachmentData>>,
    preset: Option<WorkPreset>,
    runtime: Option<String>,
) -> Result<TaskRun, String> {
    ensure_work_enabled()?;
    let runtime = runtime
        .as_deref()
        .map(RuntimeProviderKind::try_from_agent_str)
        .transpose()?;
    session::start(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &workspace_id,
        &message,
        model,
        attachments,
        preset.unwrap_or_default(),
        runtime,
    )
    .await
}

/// List standalone (workspace-less) Work task sessions.
#[tauri::command]
pub async fn work_list_standalone_sessions() -> Result<Vec<TaskRun>, String> {
    ensure_work_enabled()?;
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::task::spawn_blocking(session::list_standalone_sessions),
    )
    .await
    .map_err(|_| "列出独立任务超时，runs 目录可能较慢（如 Spotlight 索引期间）".to_string())?
    .map_err(|error| format!("读取 Work 快速任务列表失败: {error}"))?
}

/// List the newest Work conversations across standalone and active workspaces.
#[tauri::command]
pub async fn work_list_recent_sessions(limit: Option<usize>) -> Result<Vec<TaskRun>, String> {
    ensure_work_enabled()?;
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::task::spawn_blocking(move || session::list_recent_sessions(limit.unwrap_or(20))),
    )
    .await
    .map_err(|_| "读取最近 Work 对话超时".to_string())?
    .map_err(|error| format!("读取最近 Work 对话失败: {error}"))?
}

/// List archived Work conversations from the complete run metadata index.
#[tauri::command]
pub async fn work_list_archived_sessions(limit: Option<usize>) -> Result<Vec<TaskRun>, String> {
    ensure_work_enabled()?;
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::task::spawn_blocking(move || session::list_archived_sessions(limit.unwrap_or(50))),
    )
    .await
    .map_err(|_| "读取已归档 Work 对话超时".to_string())?
    .map_err(|error| format!("读取已归档 Work 对话失败: {error}"))?
}

/// Start a standalone Work task with a single message. The task runs in its
/// own per-run directory under `standalone_tasks/<run_id>/`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn work_start_standalone_session(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    message: String,
    model: Option<String>,
    attachments: Option<Vec<AttachmentData>>,
    permission_mode: Option<crate::work::models::WorkExecutionMode>,
    preset: Option<WorkPreset>,
    runtime: Option<String>,
) -> Result<TaskRun, String> {
    ensure_work_enabled()?;
    let runtime = runtime
        .as_deref()
        .map(RuntimeProviderKind::try_from_agent_str)
        .transpose()?;
    session::start_standalone(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &message,
        model,
        attachments,
        permission_mode,
        preset.unwrap_or_default(),
        runtime,
    )
    .await
}

#[tauri::command]
pub async fn work_resume_session(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    run_id: String,
    message: Option<String>,
    attachments: Option<Vec<AttachmentData>>,
) -> Result<TaskRun, String> {
    ensure_work_enabled()?;
    session::resume(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &run_id,
        message.as_deref(),
        attachments,
    )
    .await
}

#[tauri::command]
pub async fn work_send_message(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    run_id: String,
    message: String,
    attachments: Option<Vec<AttachmentData>>,
) -> Result<(), String> {
    ensure_work_enabled()?;
    session::send_message(
        emitter.inner(),
        sessions.inner(),
        &run_id,
        &message,
        attachments,
    )
    .await
}

#[tauri::command]
pub async fn work_stop_session(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    run_id: String,
) -> Result<TaskRun, String> {
    ensure_work_enabled()?;
    session::stop(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        &run_id,
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn work_continue_session(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    workspace_id: String,
    run_id: String,
    anchor_id: Option<String>,
    model: Option<String>,
) -> Result<TaskRun, String> {
    ensure_work_enabled()?;
    session::continue_session(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &workspace_id,
        &run_id,
        anchor_id.as_deref(),
        model,
    )
    .await
}

#[tauri::command]
pub fn work_list_runs(task_id: String) -> Result<Vec<WorkRun>, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.list_runs(&task_id)
}

#[tauri::command]
pub fn work_get_run(task_id: String, run_id: String) -> Result<WorkRun, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.get_run(&task_id, &run_id)
}

#[tauri::command]
pub async fn work_get_projection(
    run_id: String,
) -> Result<crate::work::models::WorkProjection, String> {
    ensure_work_enabled()?;
    tokio::task::spawn_blocking(move || {
        let paths = WorkPaths::app();
        crate::work::projection::project_work_projection(&paths, &run_id)
    })
    .await
    .map_err(|error| format!("读取 Work 统一投影失败: {error}"))?
}

#[tauri::command]
pub async fn work_get_run_progress(
    task_id: String,
    run_id: String,
) -> Result<WorkRunProgressView, String> {
    ensure_work_enabled()?;
    tokio::task::spawn_blocking(move || {
        let paths = WorkPaths::app();
        crate::work::progress::project_run_progress(&paths, &task_id, &run_id).or_else(|_| {
            crate::work::projection::project_work_projection(&paths, &run_id)
                .map(|p| p.to_progress_view())
        })
    })
    .await
    .map_err(|error| format!("读取 Work 运行进度失败: {error}"))?
}

/// Task receipt: artifacts + real sources + real file changes + run meta.
/// A pure projection over the Ledger/registry — safe to call at any time.
#[tauri::command]
pub fn work_get_run_receipt(
    task_id: String,
    run_id: String,
) -> Result<crate::work::models::WorkRunReceipt, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    crate::work::receipt::project_run_receipt(&paths, &task_id, &run_id)
}

/// Receipt for a standalone (workspace-less) Work chat run.
#[tauri::command]
pub fn work_get_standalone_run_receipt(
    run_id: String,
) -> Result<crate::work::models::WorkRunReceipt, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    crate::work::receipt::project_standalone_receipt(&paths, &run_id)
}

pub(crate) fn parse_work_recovery_action(value: &str) -> Result<WorkRecoveryAction, String> {
    match value.trim() {
        "continue" => Ok(WorkRecoveryAction::Continue),
        "retry" => Ok(WorkRecoveryAction::Retry),
        "verify" => Ok(WorkRecoveryAction::Verify),
        "retry_subagent" => Ok(WorkRecoveryAction::RetrySubagent),
        "from_scratch" => Ok(WorkRecoveryAction::FromScratch),
        "cancel" => Ok(WorkRecoveryAction::Cancel),
        other => Err(format!("不支持的 Work 恢复操作：{other}")),
    }
}

fn work_recovery_action_name(action: WorkRecoveryAction) -> &'static str {
    match action {
        WorkRecoveryAction::Continue => "continue",
        WorkRecoveryAction::Retry => "retry",
        WorkRecoveryAction::Verify => "verify",
        WorkRecoveryAction::RetrySubagent => "retry_subagent",
        WorkRecoveryAction::FromScratch => "from_scratch",
        WorkRecoveryAction::Cancel => "cancel",
    }
}

fn recovery_side_effect_class(view: &WorkRunRecovery) -> crate::work::models::SideEffectClass {
    match view.side_effect_class.as_deref() {
        Some("local_verifiable") => crate::work::models::SideEffectClass::LocalVerifiable,
        Some("external_mutating") => crate::work::models::SideEffectClass::ExternalMutating,
        _ => crate::work::models::SideEffectClass::Read,
    }
}

fn validate_work_recovery_scope(
    paths: &WorkPaths,
    workspace_id: &str,
    task_id: &str,
    run_id: &str,
) -> Result<(WorkTask, WorkRun), String> {
    let workspace = workspace::WorkspaceManager::new(paths.clone()).get(workspace_id)?;
    if workspace.id != workspace_id {
        return Err("Workspace identity does not match request".to_string());
    }
    let manager = TaskManager::new(paths.clone());
    let task = manager.get_task(task_id)?;
    if task.workspace_id != workspace_id {
        return Err("WorkTask does not belong to the requested Workspace".to_string());
    }
    let run = manager.get_run(task_id, run_id)?;
    if run.task_id != task_id || run.workspace_id != workspace_id {
        return Err("WorkRun ownership does not match task/workspace".to_string());
    }
    Ok((task, run))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn recover_work_run_locked(
    paths: &WorkPaths,
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    workspace_id: &str,
    task_id: &str,
    run_id: &str,
    action: WorkRecoveryAction,
    subagent_id: Option<&str>,
) -> Result<WorkRun, String> {
    let (_task, run) = validate_work_recovery_scope(paths, workspace_id, task_id, run_id)?;
    let view = crate::work::lifecycle::get_run_recovery(paths, workspace_id, task_id, run_id)?
        .ok_or_else(|| "当前 WorkRun 没有可执行的恢复事项".to_string())?;
    if !view.available_actions.contains(&action) {
        return Err(format!(
            "恢复操作 '{}' 不适用于当前 Run 状态 {:?}",
            work_recovery_action_name(action),
            run.status
        ));
    }

    let ledger = crate::work::ledger::WorkRuntimeLedger::open(paths, task_id, run_id)?;
    let side_effect = recovery_side_effect_class(&view);
    let action_name = work_recovery_action_name(action);
    if side_effect == crate::work::models::SideEffectClass::ExternalMutating
        && matches!(
            action,
            WorkRecoveryAction::Continue | WorkRecoveryAction::Retry
        )
        && view.action.as_deref() == Some("external_unknown_outcome")
        && view.pending_interaction_id.is_none()
    {
        return Err("外部副作用结果未知，必须先通过 Inbox 恢复确认".to_string());
    }
    ledger.record_recovery_event_once(
        "recovery_action_started",
        run_id,
        view.tool_call_id.as_deref(),
        action_name,
        side_effect,
        view.reused_existing_output,
        None,
        None,
    )?;

    let interaction_manager = crate::work::interaction::InteractionManager::new(paths.clone());
    if let Some(interaction_id) = view.pending_interaction_id.as_deref() {
        let (state, inbox_status, decision) = match action {
            WorkRecoveryAction::Continue => (
                crate::work::models::PendingInteractionState::Resolved,
                InboxItemStatus::Approved,
                "external_completed",
            ),
            WorkRecoveryAction::Retry => (
                crate::work::models::PendingInteractionState::Cancelled,
                InboxItemStatus::Rejected,
                "external_not_completed",
            ),
            WorkRecoveryAction::Cancel | WorkRecoveryAction::FromScratch => (
                crate::work::models::PendingInteractionState::Cancelled,
                InboxItemStatus::Cancelled,
                "cancelled",
            ),
            _ => return Err("该恢复事项仍等待外部结果确认".to_string()),
        };
        interaction_manager.commit_resolution_with_response(
            interaction_id,
            state,
            Some(inbox_status),
            Some(serde_json::json!({
                "recoveryDecision": decision,
                "action": action_name,
            })),
        )?;
    }

    // Close the original uncertain proposal with an explicit durable outcome.
    // Otherwise the next restart would see the same ToolProposed-without-result
    // and create the same recovery prompt again, even after the user chose a
    // decision. Any retry must use a new tool call id and pass through the
    // ordinary Policy/Inbox path.
    if let Some(tool_call_id) = view.tool_call_id.as_deref() {
        let (success, status, error) = match action {
            WorkRecoveryAction::Continue => (true, "recovery_completed".to_string(), None),
            WorkRecoveryAction::Retry => (
                false,
                "recovery_retry".to_string(),
                Some("用户确认原调用未完成，后续将以新的调用重新评估".to_string()),
            ),
            WorkRecoveryAction::Cancel | WorkRecoveryAction::FromScratch => (
                false,
                "recovery_cancelled".to_string(),
                Some("用户放弃了重启前未完成的调用".to_string()),
            ),
            WorkRecoveryAction::Verify | WorkRecoveryAction::RetrySubagent => {
                (false, String::new(), None)
            }
        };
        if !status.is_empty() {
            ledger.record(&crate::work::models::RuntimeFact::ToolResult {
                tool_call_id: tool_call_id.to_string(),
                success,
                status,
                failure_kind: None,
                exit_code: None,
                error,
                outputs: view.expected_outputs.clone(),
                side_effect_class: side_effect,
                timestamp: crate::models::now_iso(),
            })?;
        }
    }

    match action {
        WorkRecoveryAction::Verify => {
            let completed = crate::work::lifecycle::WorkHarnessController::new(paths.clone())
                .complete_or_fail_run(task_id, run_id, WorkRunStatus::Completed, None, None)?;
            ledger.record_recovery_event_once(
                "recovery_resolved",
                run_id,
                view.tool_call_id.as_deref(),
                action_name,
                side_effect,
                view.reused_existing_output,
                Some(if completed.status == WorkRunStatus::Completed {
                    "交付物验证通过，Run 已完成".to_string()
                } else {
                    "交付物仍未满足验收条件".to_string()
                }),
                (completed.status != WorkRunStatus::Completed).then(|| {
                    completed
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "交付物仍未满足验收条件".to_string())
                }),
            )?;
            Ok(completed)
        }
        WorkRecoveryAction::Cancel => {
            let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
            if let Some(session_id) = run.session_id.as_deref() {
                session::stop(emitter, sessions, spawn_locks, session_id).await?;
            }
            let cancelled =
                controller.transition_run_status(task_id, run_id, WorkRunStatus::Cancelled)?;
            ledger.record_recovery_event_once(
                "recovery_abandoned",
                run_id,
                view.tool_call_id.as_deref(),
                action_name,
                side_effect,
                view.reused_existing_output,
                Some("用户取消了恢复".to_string()),
                None,
            )?;
            Ok(cancelled)
        }
        WorkRecoveryAction::FromScratch => {
            let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
            if let Some(session_id) = run.session_id.as_deref() {
                session::stop(emitter, sessions, spawn_locks, session_id).await?;
            }
            controller.transition_run_status(task_id, run_id, WorkRunStatus::Cancelled)?;
            ledger.record_recovery_event_once(
                "recovery_abandoned",
                run_id,
                view.tool_call_id.as_deref(),
                action_name,
                side_effect,
                view.reused_existing_output,
                Some("用户选择从头开始，旧 Run 已取消".to_string()),
                None,
            )?;
            start_work_task_run(
                paths,
                emitter,
                sessions,
                spawn_locks,
                cancel_token,
                task_id,
                WorkRunTrigger::Manual,
                crate::work::models::ExecutionContext::Attended,
                None,
            )
            .await
        }
        WorkRecoveryAction::RetrySubagent => {
            let id = subagent_id
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "重试子任务需要 subagentId".to_string())?;
            let interrupted =
                crate::work::subagents::interrupted_child_ids_from_ledger(paths, task_id, run_id)?;
            if !interrupted.iter().any(|candidate| candidate == id) {
                return Err("该子任务不属于当前 Run，或没有可重试的中断记录".to_string());
            }
            let session_id = run
                .session_id
                .as_deref()
                .ok_or_else(|| "恢复 Run 没有关联可继续的 Work 会话".to_string())?;
            let session_meta = crate::storage::runs::get_run(session_id)
                .ok_or_else(|| "未找到对应的 Work 会话".to_string())?;
            let adapter = crate::work::runtime::get_pi_work_runtime(&session_meta.agent)
                .map_err(|e| format!("Work Runtime 不支持该会话: {e}"))?;
            if !adapter.capabilities().supports_subagents {
                return Err(format!(
                    "Work Runtime '{}' 不支持子任务重试",
                    adapter.provider().as_str()
                ));
            }
            let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
            let running =
                controller.transition_run_status(task_id, run_id, WorkRunStatus::Running)?;
            let message = format!(
                "子任务 {id} 在应用重启时中断。请通过现有 Work 子代理能力重新委派/重试该子任务；不要直接复用旧子代理进程。"
            );
            if let Err(error) = resume_work_session_after_approval(
                emitter,
                sessions,
                spawn_locks,
                cancel_token,
                session_id,
                &message,
            )
            .await
            {
                let restore_result =
                    controller.transition_run_status(task_id, run_id, WorkRunStatus::Recoverable);
                if let Err(restore_error) = restore_result {
                    return Err(format!(
                        "子任务恢复消息发送失败：{error}；恢复状态写回失败：{restore_error}"
                    ));
                }
                return Err(format!("子任务恢复消息发送失败：{error}"));
            }
            ledger.record_recovery_event_once(
                "recovery_resolved",
                run_id,
                view.tool_call_id.as_deref(),
                action_name,
                side_effect,
                view.reused_existing_output,
                Some(format!("已请求 Work 运行时重新委派子任务 {id}")),
                None,
            )?;
            Ok(running)
        }
        WorkRecoveryAction::Continue | WorkRecoveryAction::Retry => {
            let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
            let running =
                controller.transition_run_status(task_id, run_id, WorkRunStatus::Running)?;
            let session_id = match running.session_id.as_deref() {
                Some(session_id) => session_id,
                None => {
                    let fallback = if run.status == WorkRunStatus::WaitingDelivery {
                        WorkRunStatus::WaitingDelivery
                    } else {
                        WorkRunStatus::Recoverable
                    };
                    controller.transition_run_status(task_id, run_id, fallback)?;
                    return Err("恢复 Run 没有关联可继续的 Work 会话".to_string());
                }
            };
            let message = if action == WorkRecoveryAction::Continue {
                "已确认重启前的外部操作已经完成。请不要重复调用该操作，基于已完成结果继续后续步骤。"
            } else if side_effect == crate::work::models::SideEffectClass::ExternalMutating {
                "已确认重启前的外部操作尚未完成。请重新评估并通过现有 Policy/Inbox 流程发起新的调用；不要直接重放旧调用。"
            } else if run.status == WorkRunStatus::WaitingDelivery {
                "交付物尚未通过验收。请重新生成或修复必需文件，并登记到当前 WorkRun 后再验证。"
            } else {
                "请从持久化检查点继续；重启前未完成的调用不会自动重放，必要时重新经过 Policy/Inbox。"
            };
            if let Err(error) = resume_work_session_after_approval(
                emitter,
                sessions,
                spawn_locks,
                cancel_token,
                session_id,
                message,
            )
            .await
            {
                let restore_result =
                    controller.transition_run_status(task_id, run_id, WorkRunStatus::Recoverable);
                if let Err(restore_error) = restore_result {
                    return Err(format!(
                        "Work 会话恢复失败：{error}；恢复状态写回失败：{restore_error}"
                    ));
                }
                return Err(format!("Work 会话恢复失败：{error}"));
            }
            ledger.record_recovery_event_once(
                "recovery_resolved",
                run_id,
                view.tool_call_id.as_deref(),
                action_name,
                side_effect,
                view.reused_existing_output,
                Some("已恢复 Work 会话，后续调用仍经过现有 ToolPipeline".to_string()),
                None,
            )?;
            Ok(running)
        }
    }
}

#[tauri::command]
pub fn work_get_run_recovery(
    workspace_id: String,
    task_id: String,
    run_id: String,
) -> Result<Option<WorkRunRecovery>, String> {
    ensure_work_enabled()?;
    crate::work::lifecycle::get_run_recovery(&WorkPaths::app(), &workspace_id, &task_id, &run_id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn work_recover_run(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    workspace_id: String,
    task_id: String,
    run_id: String,
    action: String,
    subagent_id: Option<String>,
) -> Result<WorkRun, String> {
    ensure_work_enabled()?;
    let _guard = INBOX_DELIVERY_LOCK.lock().await;
    recover_work_run_locked(
        &WorkPaths::app(),
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &workspace_id,
        &task_id,
        &run_id,
        parse_work_recovery_action(&action)?,
        subagent_id.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn work_verify_recovered_output(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    workspace_id: String,
    task_id: String,
    run_id: String,
) -> Result<WorkRun, String> {
    ensure_work_enabled()?;
    let _guard = INBOX_DELIVERY_LOCK.lock().await;
    recover_work_run_locked(
        &WorkPaths::app(),
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &workspace_id,
        &task_id,
        &run_id,
        WorkRecoveryAction::Verify,
        None,
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn work_retry_subagent(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    workspace_id: String,
    task_id: String,
    run_id: String,
    subagent_id: String,
) -> Result<WorkRun, String> {
    ensure_work_enabled()?;
    let _guard = INBOX_DELIVERY_LOCK.lock().await;
    recover_work_run_locked(
        &WorkPaths::app(),
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &workspace_id,
        &task_id,
        &run_id,
        WorkRecoveryAction::RetrySubagent,
        Some(&subagent_id),
    )
    .await
}

#[tauri::command]
pub async fn work_steer(
    sessions: tauri::State<'_, ActorSessionMap>,
    task_id: String,
    run_id: String,
    instruction: String,
) -> Result<crate::work::models::RuntimeInputItem, String> {
    work_steer_impl(&sessions, task_id, run_id, instruction).await
}

/// Headless-capable inner impl of [`work_steer`] — takes the session map
/// directly instead of a `tauri::State` extract.
pub(crate) async fn work_steer_impl(
    sessions: &ActorSessionMap,
    task_id: String,
    run_id: String,
    instruction: String,
) -> Result<crate::work::models::RuntimeInputItem, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let task_manager = TaskManager::new(paths.clone());
    let run = task_manager.get_run(&task_id, &run_id)?;

    let input_mgr = crate::work::input::RuntimeInputManager::open(&paths, &task_id, &run_id)?;
    let item = input_mgr.queue_input(
        &task_id,
        &run_id,
        crate::work::models::RuntimeInputKind::Steer,
        &instruction,
    )?;

    // If session actor is running, steer it directly
    if let Some(session_id) = run.session_id.as_deref() {
        let map = sessions.lock().await;
        if map.contains_key(session_id) {
            drop(map);
            if let Err(error) = crate::commands::session::steer_session_message_inner(
                sessions,
                session_id.to_string(),
                instruction,
                None,
            )
            .await
            {
                return Err(format!(
                    "指令已写入 Work 输入队列，但发送给活动会话失败：{error}"
                ));
            }
        }
    }

    Ok(item)
}

#[tauri::command]
pub fn work_follow_up(
    task_id: String,
    run_id: String,
    instruction: String,
) -> Result<crate::work::models::RuntimeInputItem, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let input_mgr = crate::work::input::RuntimeInputManager::open(&paths, &task_id, &run_id)?;
    input_mgr.queue_input(
        &task_id,
        &run_id,
        crate::work::models::RuntimeInputKind::FollowUp,
        &instruction,
    )
}

#[tauri::command]
pub fn work_inject_context(
    task_id: String,
    run_id: String,
    content: String,
) -> Result<crate::work::models::RuntimeInputItem, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let input_mgr = crate::work::input::RuntimeInputManager::open(&paths, &task_id, &run_id)?;
    input_mgr.queue_input(
        &task_id,
        &run_id,
        crate::work::models::RuntimeInputKind::Inject,
        &content,
    )
}

#[tauri::command]
pub async fn work_list_subagents(
    parent_scope: String,
    task_id: Option<String>,
) -> Result<Vec<crate::work::subagents::WorkSubagentRecord>, String> {
    ensure_work_enabled()?;
    let mut map: std::collections::HashMap<String, crate::work::subagents::WorkSubagentRecord> =
        std::collections::HashMap::new();

    // 1. Replay historical Ledger facts if task_id is present
    if let Some(t_id) = task_id.as_deref() {
        let paths = WorkPaths::app();
        if let Ok(ledger) =
            crate::work::ledger::WorkRuntimeLedger::open(&paths, t_id, &parent_scope)
        {
            if let Ok(facts) = ledger.list_facts() {
                for fact in facts {
                    match fact {
                        crate::work::models::RuntimeFact::SubagentSpawned {
                            agent_id,
                            provider_run_id,
                            child_index,
                            role,
                            status,
                            ..
                        } => {
                            map.insert(
                                agent_id.clone(),
                                crate::work::subagents::WorkSubagentRecord {
                                    agent_id,
                                    provider_run_id,
                                    child_index,
                                    role,
                                    parent_scope: parent_scope.clone(),
                                    workspace_id: None,
                                    task_id: Some(t_id.to_string()),
                                    launch_contract_digest: String::new(),
                                    status,
                                    created_at: String::new(),
                                    updated_at: String::new(),
                                    error: None,
                                    result_summary: None,
                                },
                            );
                        }
                        crate::work::models::RuntimeFact::SubagentCompleted {
                            agent_id,
                            status,
                            summary,
                            ..
                        } => {
                            if let Some(entry) = map.get_mut(&agent_id) {
                                entry.status = status;
                                entry.result_summary = summary;
                            }
                        }
                        crate::work::models::RuntimeFact::SubagentFailed {
                            agent_id,
                            status,
                            error,
                            ..
                        } => {
                            if let Some(entry) = map.get_mut(&agent_id) {
                                entry.status = status;
                                entry.error = error;
                            }
                        }
                        crate::work::models::RuntimeFact::SubagentStopped {
                            agent_id,
                            status,
                            reason,
                            ..
                        } => {
                            if let Some(entry) = map.get_mut(&agent_id) {
                                entry.status = status;
                                entry.error = reason;
                            }
                        }
                        crate::work::models::RuntimeFact::SubagentInterrupted {
                            agent_id,
                            status,
                            reason,
                            ..
                        } => {
                            if let Some(entry) = map.get_mut(&agent_id) {
                                entry.status = status;
                                entry.error = reason;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // 2. Overlay in-memory live registry records (live state wins)
    let live_records = crate::work::subagents::registry().list_for_scope(&parent_scope);
    for record in live_records {
        if let Some(t_id) = task_id.as_deref() {
            if let Some(ref record_task) = record.task_id {
                if record_task != t_id {
                    continue;
                }
            }
        }
        map.insert(record.agent_id.clone(), record);
    }

    // 3. Stably sort by child_index ascending, then by agent_id
    let mut result: Vec<crate::work::subagents::WorkSubagentRecord> = map.into_values().collect();
    result.sort_by(|a, b| {
        a.child_index
            .cmp(&b.child_index)
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });

    Ok(result)
}

#[tauri::command]
pub fn work_assign_session_workspace(
    run_id: String,
    workspace_id: String,
) -> Result<TaskRun, String> {
    ensure_work_enabled()?;
    let mut run = crate::storage::runs::get_run(&run_id)
        .ok_or_else(|| format!("Run {} not found", run_id))?;
    if run.app_mode != crate::work::models::AppMode::Work {
        return Err(format!("Run {} does not belong to Work mode", run_id));
    }
    if run.workspace_id.is_some() {
        if run.workspace_id.as_deref() == Some(workspace_id.as_str()) {
            return Ok(run.to_task_run(None, None, None));
        }
        return Err("This Work session is already assigned to another Workspace".into());
    }
    if matches!(
        run.status,
        crate::models::RunStatus::Pending | crate::models::RunStatus::Running
    ) {
        return Err("请等待当前回合完成后再收编；正在执行的会话不能切换工作区".into());
    }
    crate::work::workspace::manager().open(&workspace_id)?;
    let paths = WorkPaths::app();
    let workspace_root = paths.workspace_dir(&workspace_id)?;

    // Copy the standalone output/scratch data before changing the run's cwd.
    // The source directory is intentionally retained so a failed metadata
    // write can be retried without losing the original session files.
    artifacts::migrate_standalone_to_workspace(&run_id, &workspace_id)?;

    let tm = TaskManager::new(paths.clone());

    let task_id = match &run.work_task_id {
        Some(tid)
            if tm
                .get_task(tid)
                .map(|task| task.workspace_id == workspace_id)
                .unwrap_or(false) =>
        {
            tid.clone()
        }
        _ => {
            let prompt = if run.prompt.trim().is_empty() {
                "Work Task"
            } else {
                &run.prompt
            };
            let title = prompt.lines().next().unwrap_or(prompt).trim();
            let title = if title.chars().count() > 80 {
                format!("{}...", title.chars().take(77).collect::<String>())
            } else {
                title.to_string()
            };
            let task = tm.create_task_with_source(
                &workspace_id,
                &title,
                prompt,
                None,
                WorkTaskSource::Dialog,
            )?;
            run.work_task_id = Some(task.id.clone());
            task.id
        }
    };

    let work_run_id = match &run.work_run_id {
        Some(work_run_id) if tm.get_run(&task_id, work_run_id).is_ok() => work_run_id.clone(),
        _ => {
            let work_run = tm.start_run_internal(
                &task_id,
                Some(&run.id),
                WorkRunTrigger::Manual,
                None,
                crate::work::models::ExecutionContext::Attended,
            )?;
            run.work_run_id = Some(work_run.id.clone());
            work_run.id
        }
    };
    let current_work_run = tm.get_run(&task_id, &work_run_id)?;
    let lifecycle = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
    match run.status {
        crate::models::RunStatus::Idle
            if current_work_run.status.is_active()
                && current_work_run.status != WorkRunStatus::WaitingInput =>
        {
            tm.set_run_status(&task_id, &work_run_id, WorkRunStatus::WaitingInput)?;
        }
        crate::models::RunStatus::Completed if current_work_run.status.is_active() => {
            lifecycle.complete_or_fail_run(
                &task_id,
                &work_run_id,
                WorkRunStatus::Completed,
                None,
                None,
            )?;
        }
        crate::models::RunStatus::Failed if current_work_run.status.is_active() => {
            lifecycle.complete_or_fail_run(
                &task_id,
                &work_run_id,
                WorkRunStatus::Failed,
                run.error_message.clone(),
                None,
            )?;
        }
        crate::models::RunStatus::Stopped if current_work_run.status.is_active() => {
            lifecycle.complete_or_fail_run(
                &task_id,
                &work_run_id,
                WorkRunStatus::Cancelled,
                Some("Session stopped".into()),
                None,
            )?;
        }
        _ => {}
    }
    tm.attach_session_id(&task_id, &work_run_id, &run.id)?;

    run.work_task_id = Some(task_id);
    run.work_run_id = Some(work_run_id);
    run.workspace_id = Some(workspace_id);
    run.cwd = workspace_root.to_string_lossy().into_owned();
    run.work_execution_context = Some(crate::work::models::ExecutionContext::Attended);
    crate::storage::runs::save_meta(&run)?;

    let meta = crate::storage::runs::get_run(&run_id)
        .ok_or_else(|| format!("Work run {} disappeared after assigning workspace", run_id))?;
    Ok(meta.to_task_run(None, None, None))
}
