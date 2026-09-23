use std::collections::HashMap;
use std::sync::Arc;

use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::agent::session_actor::AttachmentData;
use crate::agent::spawn_locks::SpawnLocks;
use crate::commands::session_dispatch;
use crate::models::{ExecutionPath, RunMeta, RunStatus, SessionMode, TaskRun};
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::lifecycle::WorkHarnessController;
use crate::work::models::{
    AppMode, ExecutionContext, WorkExecutionMode, WorkPreset, WorkRunStatus,
};
use crate::work::paths::WorkPaths;
use crate::work::profile;
use crate::work::runtime::router as work_runtime_router;
use crate::work::workspace;

const DEFAULT_PERMISSION_MODE: &str = "default";
const MAX_MESSAGE_CHARS: usize = 200_000;

/// Resolve the effective permission mode for a Work run. Standalone tasks
/// (workspace_id = None) default to "auto" (自动执行) so a
/// one-off task never blocks on an approval card. Workspace tasks keep the
/// existing "default" mode.
pub fn default_permission_mode(workspace_id: Option<&str>) -> &'static str {
    if workspace_id.is_some() {
        "default"
    } else {
        "auto"
    }
}

fn execution_mode_cli(mode: WorkExecutionMode) -> &'static str {
    match mode {
        WorkExecutionMode::PlanFirst => "plan",
        WorkExecutionMode::Direct => "default",
        WorkExecutionMode::Auto => "auto",
        WorkExecutionMode::FullAccess => "bypassPermissions",
    }
}

pub fn latest_session(workspace_id: &str) -> Result<Option<TaskRun>, String> {
    Ok(list_sessions(workspace_id)?.into_iter().next())
}

pub fn list_sessions(workspace_id: &str) -> Result<Vec<TaskRun>, String> {
    let _workspace = workspace::manager().get(workspace_id)?;
    let mut runs: Vec<RunMeta> = storage::runs::list_all_run_metas()
        .into_iter()
        .filter(|run| {
            run.app_mode == AppMode::Work && run.workspace_id.as_deref() == Some(workspace_id)
        })
        .collect();
    runs.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(runs
        .into_iter()
        .map(storage::runs::run_with_summary)
        .collect())
}

/// List standalone (workspace-less) Work task sessions, newest first.
pub fn list_standalone_sessions() -> Result<Vec<TaskRun>, String> {
    let mut runs: Vec<RunMeta> = storage::runs::list_all_run_metas()
        .into_iter()
        .filter(|run| run.app_mode == AppMode::Work && run.workspace_id.is_none())
        .collect();
    runs.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(runs
        .into_iter()
        .map(storage::runs::run_with_summary)
        .collect())
}

/// List the newest Work conversations across standalone and active workspaces.
/// The aggregation stays in the core so the sidebar does not fan out one IPC
/// request per workspace just to render its Recent section.
pub fn list_recent_sessions(limit: usize) -> Result<Vec<TaskRun>, String> {
    let mut sessions = list_standalone_sessions()?;
    for workspace in workspace::manager().list()? {
        sessions.extend(list_sessions(&workspace.id)?);
    }
    sessions.sort_by(|left, right| {
        right
            .last_activity_at
            .as_deref()
            .unwrap_or(&right.started_at)
            .cmp(left.last_activity_at.as_deref().unwrap_or(&left.started_at))
    });
    sessions.retain(|session| session.archived != Some(true));
    sessions.truncate(limit);
    Ok(sessions)
}

/// List archived Work conversations from the complete run metadata index.
/// This projection is independent of workspace expansion and workspace load state.
pub fn list_archived_sessions(limit: usize) -> Result<Vec<TaskRun>, String> {
    let mut sessions: Vec<TaskRun> = storage::runs::list_all_run_metas()
        .into_iter()
        .filter(|run| run.app_mode == AppMode::Work && run.archived == Some(true))
        .map(storage::runs::run_with_summary)
        .collect();
    sessions.sort_by(|left, right| {
        right
            .last_activity_at
            .as_deref()
            .unwrap_or(&right.started_at)
            .cmp(left.last_activity_at.as_deref().unwrap_or(&left.started_at))
    });
    sessions.truncate(limit);
    Ok(sessions)
}

#[derive(Debug, Clone)]
pub enum RuntimeTurnOutcome {
    Success,
    Failed(Option<String>),
}

/// Event-driven lifecycle turn handler. Replaces 500ms polling.
/// When Pi RPC finishes a turn, this is called immediately.
pub async fn handle_turn_settled(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    outcome: RuntimeTurnOutcome,
) -> Result<(), String> {
    let _guard = SETTLEMENT_LOCKS.acquire(run_id).await;
    let meta = match crate::storage::runs::get_run(run_id) {
        Some(meta) if meta.app_mode == AppMode::Work => meta,
        _ => return Ok(()),
    };

    if matches!(meta.status, RunStatus::Stopped | RunStatus::Failed) {
        log::info!(
            "[work/session] Run {run_id} is already {:?}; ignoring settled turn",
            meta.status
        );
        return Ok(());
    }

    let paths = WorkPaths::app();

    // If this run belongs to an automation task, handle automation completion / repair
    if let Some(_task_id) = meta.work_task_id.as_deref() {
        // Always insert into pending first, then immediately attempt finalization.
        // This eliminates the lost-wakeup race where a child could complete between
        // has_active_children check and pending.insert, leaving the parent stuck.
        {
            let mut pending = PENDING_PARENT_SETTLEMENTS.lock().await;
            pending.insert(run_id.to_string(), (emitter.clone(), outcome));
        }
        return try_finalize_parent_run_locked(run_id).await;
    } else {
        // Interactive conversation:
        // A single turn settling returns the conversation to Idle.
        // It does NOT finalize the conversation as Completed!
        log::debug!("[work/session] Interactive turn settled for run {run_id}");
    }

    if let Ok(projection) = crate::work::projection::project_work_projection(&paths, run_id) {
        emitter.persist_and_emit(
            run_id,
            &crate::models::BusEvent::WorkProjectionChanged {
                run_id: run_id.to_string(),
                projection,
            },
        );
    }

    Ok(())
}

#[allow(clippy::type_complexity)]
static PENDING_PARENT_SETTLEMENTS: Lazy<
    tokio::sync::Mutex<HashMap<String, (Arc<BroadcastEmitter>, RuntimeTurnOutcome)>>,
> = Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));

#[cfg(test)]
pub async fn reset_pending_settlements_for_test() {
    let mut pending = PENDING_PARENT_SETTLEMENTS.lock().await;
    pending.clear();
}

static SETTLEMENT_LOCKS: Lazy<SpawnLocks> = Lazy::new(SpawnLocks::new);

/// Result of a user stop request.
#[derive(Debug, Clone, PartialEq)]
pub struct StopAcceptance {
    pub accepted: bool,
    pub previous_status: Option<RunStatus>,
}

/// Serialize acceptance of a user stop with the final completion commit.
/// A completion committed before this point stays complete; an accepted stop
/// prevents an older, already dequeued success from committing afterward.
pub async fn request_user_stop(run_id: &str) -> Result<StopAcceptance, String> {
    let _guard = SETTLEMENT_LOCKS.acquire(run_id).await;
    if let Some(meta) = storage::runs::get_run(run_id) {
        let prev = meta.status.clone();
        if meta.app_mode == AppMode::Work {
            if matches!(meta.status, RunStatus::Completed | RunStatus::Failed) {
                return Ok(StopAcceptance {
                    accepted: false,
                    previous_status: Some(prev),
                });
            }
            if let Some(task_id) = meta.work_task_id.as_deref() {
                let paths = WorkPaths::app();
                if let Ok(task_run) =
                    crate::work::tasks::TaskManager::new(paths).get_run(task_id, run_id)
                {
                    if !task_run.status.is_active() {
                        return Ok(StopAcceptance {
                            accepted: false,
                            previous_status: Some(prev),
                        });
                    }
                }
            }
            storage::runs::update_status(run_id, RunStatus::Stopped, None, None)?;
            PENDING_PARENT_SETTLEMENTS.lock().await.remove(run_id);
            return Ok(StopAcceptance {
                accepted: true,
                previous_status: Some(prev),
            });
        }
        return Ok(StopAcceptance {
            accepted: false,
            previous_status: Some(prev),
        });
    }
    Ok(StopAcceptance {
        accepted: false,
        previous_status: None,
    })
}

/// Re-attempt settling a parent automation run if it was parked waiting for child subagents.
pub async fn try_finalize_parent_run(parent_run_id: &str) -> Result<(), String> {
    let _guard = SETTLEMENT_LOCKS.acquire(parent_run_id).await;
    try_finalize_parent_run_locked(parent_run_id).await
}

async fn try_finalize_parent_run_locked(parent_run_id: &str) -> Result<(), String> {
    if crate::work::subagents::registry().has_active_children(parent_run_id) {
        log::info!(
            "[work/session] Run {parent_run_id} still has active children; parking settlement"
        );
        return Ok(());
    }
    let pending_item = {
        let mut pending = PENDING_PARENT_SETTLEMENTS.lock().await;
        pending.remove(parent_run_id)
    };
    if let Some((emitter, outcome)) = pending_item {
        log::info!("[work/session] Finalizing pending parent settlement for {parent_run_id}");
        settle_automation_run(&emitter, parent_run_id, outcome).await?;
    }
    Ok(())
}

async fn settle_automation_run(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    outcome: RuntimeTurnOutcome,
) -> Result<(), String> {
    let meta = match crate::storage::runs::get_run(run_id) {
        Some(meta) if meta.app_mode == AppMode::Work => meta,
        _ => return Ok(()),
    };

    let task_id = match meta.work_task_id.as_deref() {
        Some(tid) => tid,
        None => return Ok(()),
    };

    if matches!(meta.status, RunStatus::Stopped | RunStatus::Failed) {
        return Ok(());
    }

    let paths = WorkPaths::app();
    let controller = WorkHarnessController::new(paths.clone());

    // A stop/crash may have won before the queued settlement acquired the lock.
    if !crate::work::tasks::TaskManager::new(paths.clone())
        .get_run(task_id, run_id)?
        .status
        .is_active()
    {
        return Ok(());
    }

    match outcome {
        RuntimeTurnOutcome::Success => {
            let completed_run = controller.complete_or_fail_run(
                task_id,
                run_id,
                WorkRunStatus::Completed,
                None,
                None,
            );
            match completed_run {
                Ok(final_run) => {
                    let repair_instruction = final_run
                        .task_state
                        .goal_spec
                        .as_ref()
                        .filter(|goal| {
                            final_run.status == WorkRunStatus::WaitingDelivery
                                && goal.repair_round > 0
                                && goal.repair_instruction.is_some()
                        })
                        .and_then(|goal| goal.repair_instruction.clone());

                    if let Some(instruction) = repair_instruction {
                        if let Err(error) = controller.transition_run_status(
                            task_id,
                            run_id,
                            WorkRunStatus::Running,
                        ) {
                            log::error!(
                                "[work/session] Failed to enter repair execution for WorkRun {run_id}: {error}"
                            );
                        } else if let Some(sessions) =
                            crate::agent::pi_session_actor::shared_sessions()
                        {
                            let actor_is_alive = {
                                let map = sessions.lock().await;
                                map.contains_key(run_id)
                            };
                            if actor_is_alive {
                                // This may run inside the actor itself. Awaiting its
                                // mailbox reply here would deadlock that actor.
                                let emitter = emitter.clone();
                                let sessions = sessions.clone();
                                let run_id = run_id.to_string();
                                tokio::spawn(async move {
                                    if let Err(error) = send_message(
                                        &emitter,
                                        &sessions,
                                        &run_id,
                                        &instruction,
                                        None,
                                    )
                                    .await
                                    {
                                        log::error!(
                                            "[work/session] Repair dispatch failed: {error}"
                                        );
                                    }
                                });
                            }
                        }
                    }

                    let task_title = crate::work::tasks::TaskManager::new(paths.clone())
                        .get_task(task_id)
                        .map(|t| t.title)
                        .unwrap_or_else(|_| format!("任务 {}", task_id));
                    let (title, message) = match final_run.status {
                        WorkRunStatus::Completed => {
                            ("Work 任务已完成", format!("「{}」执行完成", task_title))
                        }
                        WorkRunStatus::WaitingDelivery => (
                            "Work 任务等待交付物验收",
                            format!("「{}」需要补齐或验证必需交付物", task_title),
                        ),
                        WorkRunStatus::Recoverable => (
                            "Work 任务需要恢复",
                            format!("「{}」需要处理恢复事项", task_title),
                        ),
                        _ => return Ok(()),
                    };
                    if let Some(app) = emitter.app_opt() {
                        crate::agent::notify::notify_if_background(app, title, &message);
                    }
                }
                Err(error) => {
                    log::error!(
                        "[work/session] Failed to persist completion for WorkRun {run_id}: {error}"
                    );
                    return Err(error);
                }
            }
        }
        RuntimeTurnOutcome::Failed(error_msg) => {
            if let Err(error) = controller.complete_or_fail_run(
                task_id,
                run_id,
                WorkRunStatus::Failed,
                error_msg,
                None,
            ) {
                log::error!(
                    "[work/session] Failed to persist failure for WorkRun {run_id}: {error}"
                );
                return Err(error);
            }
            let task_title = crate::work::tasks::TaskManager::new(paths.clone())
                .get_task(task_id)
                .map(|t| t.title)
                .unwrap_or_else(|_| format!("任务 {}", task_id));
            if let Some(app) = emitter.app_opt() {
                crate::agent::notify::notify_if_background(
                    app,
                    "Work 任务执行失败",
                    &format!("「{}」执行失败", task_title),
                );
            }
        }
    }

    if let Ok(projection) = crate::work::projection::project_work_projection(&paths, run_id) {
        emitter.persist_and_emit(
            run_id,
            &crate::models::BusEvent::WorkProjectionChanged {
                run_id: run_id.to_string(),
                projection,
            },
        );
    }

    Ok(())
}

/// Handle sudden process exit / stop with semantic reason.
pub async fn handle_process_stopped(
    emitter: &Arc<BroadcastEmitter>,
    run_id: &str,
    reason: crate::agent::session_actor::RuntimeStopReason,
) -> Result<(), String> {
    let _guard = SETTLEMENT_LOCKS.acquire(run_id).await;
    let meta = match crate::storage::runs::get_run(run_id) {
        Some(meta) if meta.app_mode == AppMode::Work => meta,
        _ => return Ok(()),
    };

    if meta.status == RunStatus::Completed {
        return Ok(());
    }

    let paths = WorkPaths::app();

    match reason {
        crate::agent::session_actor::RuntimeStopReason::UserStop => {
            // 1. Remove pending settlement so stale Success cannot finalize run later
            {
                let mut pending = PENDING_PARENT_SETTLEMENTS.lock().await;
                pending.remove(run_id);
            }
            if let Some(task_id) = meta.work_task_id.as_deref() {
                let task_manager = crate::work::tasks::TaskManager::new(paths.clone());
                if let Ok(task_run) = task_manager.get_run(task_id, run_id) {
                    if !task_run.status.is_active() {
                        return Ok(());
                    }
                }
                // 2. Stop active children and persist durable facts
                if crate::work::subagents::registry().has_active_children(run_id) {
                    crate::work::subagents::registry().interrupt_all_for_scope(
                        &paths,
                        run_id,
                        Some(task_id),
                        "stopped",
                        Some("Session stopped by user".to_string()),
                    )?;
                }
                // 3. Cancel run
                let controller = WorkHarnessController::new(paths.clone());
                controller.complete_or_fail_run(
                    task_id,
                    run_id,
                    WorkRunStatus::Cancelled,
                    Some("Session stopped by user".to_string()),
                    None,
                )?;
            }
        }
        crate::agent::session_actor::RuntimeStopReason::ProviderCrash(err) => {
            // 1. Remove pending settlement
            {
                let mut pending = PENDING_PARENT_SETTLEMENTS.lock().await;
                pending.remove(run_id);
            }
            if let Some(task_id) = meta.work_task_id.as_deref() {
                let task_manager = crate::work::tasks::TaskManager::new(paths.clone());
                if let Ok(task_run) = task_manager.get_run(task_id, run_id) {
                    if !task_run.status.is_active() {
                        return Ok(());
                    }
                }
                // 2. Interrupt active children
                if crate::work::subagents::registry().has_active_children(run_id) {
                    crate::work::subagents::registry().interrupt_all_for_scope(
                        &paths,
                        run_id,
                        Some(task_id),
                        "interrupted",
                        Some(format!("Parent provider crashed: {err}")),
                    )?;
                }
                // 3. Fail run
                let controller = WorkHarnessController::new(paths.clone());
                controller.complete_or_fail_run(
                    task_id,
                    run_id,
                    WorkRunStatus::Failed,
                    Some(err),
                    None,
                )?;
            }
        }
        crate::agent::session_actor::RuntimeStopReason::Superseded => {
            // Actor superseded by new actor (start/resume/fork/compact).
            // Do not mutate WorkRun status so state is preserved.
        }
        crate::agent::session_actor::RuntimeStopReason::AppShutdown => {
            // App is terminating, keep current status for restart reconciliation.
        }
    }

    if let Ok(projection) = crate::work::projection::project_work_projection(&paths, run_id) {
        emitter.persist_and_emit(
            run_id,
            &crate::models::BusEvent::WorkProjectionChanged {
                run_id: run_id.to_string(),
                projection,
            },
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn start_work_session(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    workspace_id: &str,
    task_id: Option<&str>,
    work_run_id: Option<&str>,
    execution_context: ExecutionContext,
    message: &str,
    model: Option<String>,
    attachments: Option<Vec<AttachmentData>>,
    preset: WorkPreset,
    runtime: Option<RuntimeProviderKind>,
) -> Result<TaskRun, String> {
    start_work_session_with_overrides(
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        workspace_id,
        task_id,
        work_run_id,
        execution_context,
        message,
        model,
        attachments,
        preset,
        runtime.map(|runtime| crate::work::runtime::WorkLaunchOverrides {
            runtime: Some(runtime),
            ..Default::default()
        }),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn start_work_session_with_overrides(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    workspace_id: &str,
    task_id: Option<&str>,
    work_run_id: Option<&str>,
    execution_context: ExecutionContext,
    message: &str,
    model: Option<String>,
    attachments: Option<Vec<AttachmentData>>,
    preset: WorkPreset,
    overrides: Option<crate::work::runtime::WorkLaunchOverrides>,
) -> Result<TaskRun, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let provider = match overrides.as_ref().and_then(|o| o.runtime) {
        Some(p) => p,
        None => profile::resolve_new_work_runtime_from_storage()?,
    };
    // Validate the provider before any provider-specific preparation runs.
    // This keeps an unsupported Work runtime from triggering Pi setup.
    work_runtime_router::route(provider).map_err(String::from)?;
    let target = workspace::manager().open(workspace_id)?;
    let primary_work_root = workspace::manager().resolve_primary_work_root(&target)?;
    let work_root_str = primary_work_root.to_string_lossy().into_owned();
    let message = normalize_message(message)?;
    let id = work_run_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut launch_overrides = overrides.unwrap_or_default();
    if launch_overrides.model.is_none() {
        launch_overrides.model = model.or_else(|| target.default_model.clone());
    }
    if launch_overrides.effort.is_none() {
        launch_overrides.effort = target.default_effort.clone();
    }
    let effective_model = launch_overrides.model.clone();
    let permission_mode = execution_mode_cli(target.default_policy.execution_mode);
    let mut meta = storage::runs::create_run_with_context(
        &id,
        &message,
        &work_root_str,
        provider.as_str(),
        RunStatus::Pending,
        effective_model,
        None,
        None,
        None,
        None,
        None,
        AppMode::Work,
        Some(workspace_id.to_string()),
    )?;
    meta.execution_path = Some(ExecutionPath::SessionActor);
    meta.work_task_id = task_id.map(|s| s.to_string());
    meta.work_run_id = Some(id.clone());
    meta.work_execution_context = Some(execution_context);
    meta.work_preset = Some(preset);
    meta.permission_mode = Some(permission_mode.to_string());
    storage::runs::save_meta(&meta)?;

    let run = meta.to_task_run(None, None, None);
    session_dispatch::start_session_impl_with_overrides(
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        run.id.clone(),
        Some(SessionMode::New),
        None,
        Some(message.trim().to_string()),
        attachments,
        None,
        Some(permission_mode.to_string()),
        None,
        Some(launch_overrides),
    )
    .await?;

    Ok(run)
}

#[allow(clippy::too_many_arguments)]
pub async fn start(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    workspace_id: &str,
    message: &str,
    model: Option<String>,
    attachments: Option<Vec<AttachmentData>>,
    preset: WorkPreset,
    runtime: Option<RuntimeProviderKind>,
) -> Result<TaskRun, String> {
    let normalized = normalize_message(message)?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;

    let runtime_check = match runtime {
        Some(provider) => work_runtime_router::route(provider)
            .map(|_| ())
            .map_err(String::from),
        None => profile::resolve_new_work_runtime_from_storage().and_then(|provider| {
            work_runtime_router::route(provider)
                .map(|_| ())
                .map_err(String::from)
        }),
    };
    runtime_check?;

    start_work_session(
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        workspace_id,
        None,
        None,
        ExecutionContext::Attended,
        &normalized,
        model,
        attachments,
        preset,
        runtime,
    )
    .await
}

/// Start a standalone (workspace-less) Work task session. The run gets its
/// own per-run directory under `standalone_tasks/<run_id>/` which serves as
/// both the agent's cwd and the output area for any deliverables. Standalone
/// tasks default to "auto" permission mode (自动执行) so they never block on an
/// approval card. They carry no WorkTask/WorkRun ledger — the session run is
/// the single source of truth.
#[allow(clippy::too_many_arguments)]
pub async fn start_standalone(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    message: &str,
    model: Option<String>,
    attachments: Option<Vec<AttachmentData>>,
    permission_mode: Option<crate::work::models::WorkExecutionMode>,
    preset: WorkPreset,
    runtime: Option<RuntimeProviderKind>,
) -> Result<TaskRun, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let provider = runtime.unwrap_or(profile::resolve_new_work_runtime_from_storage()?);
    // Validate the provider before creating the standalone task directory.
    work_runtime_router::route(provider).map_err(String::from)?;
    let normalized = normalize_message(message)?;

    let id = Uuid::new_v4().to_string();
    let task_dir = paths.ensure_standalone_task_dir(&id)?;

    let mut meta = storage::runs::create_run_with_context(
        &id,
        &normalized,
        task_dir.to_str().unwrap_or(""),
        provider.as_str(),
        RunStatus::Pending,
        model,
        None,
        None,
        None,
        None,
        None,
        AppMode::Work,
        None, // workspace_id = None marks this as a standalone task
    )?;
    meta.execution_path = Some(ExecutionPath::SessionActor);
    meta.work_execution_context = Some(ExecutionContext::Attended);
    meta.work_preset = Some(preset);
    let selected_mode = permission_mode.unwrap_or(crate::work::models::WorkExecutionMode::Auto);
    let cli_perm_mode = execution_mode_cli(selected_mode);
    meta.permission_mode = Some(cli_perm_mode.to_string());
    storage::runs::save_meta(&meta)?;

    let run = meta.to_task_run(None, None, None);
    session_dispatch::start_session_impl(
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        run.id.clone(),
        Some(SessionMode::New),
        None,
        Some(normalized.trim().to_string()),
        attachments,
        None,
        Some(cli_perm_mode.to_string()),
        None,
    )
    .await?;

    Ok(run)
}

/// Create an idle Work conversation with the selected historical prefix as
/// continuation context. This intentionally does not use Pi's native fork
/// protocol: the UI timeline id is a durable bus-message id, not necessarily
/// the entry id from Pi's session file.
#[allow(clippy::too_many_arguments)]
pub async fn continue_session(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    workspace_id: &str,
    source_run_id: &str,
    anchor_id: Option<&str>,
    model: Option<String>,
) -> Result<TaskRun, String> {
    let ws_trimmed = workspace_id.trim();
    let is_standalone = ws_trimmed.is_empty();
    let source = ensure_work_run(
        source_run_id,
        if is_standalone {
            None
        } else {
            Some(ws_trimmed)
        },
    )?;
    // Validate that the source run's runtime is still supported in Work mode.
    // Fails closed if the agent string is unknown or unsupported.
    let adapter = work_runtime_router::route_agent_str(&source.agent)
        .map_err(|e| format!("Work Continue 不支持该 Runtime: {}", e))?;
    let provider = adapter.provider();
    if !adapter.capabilities().supports_continuation {
        return Err(format!(
            "Work Runtime '{}' 不支持延续会话",
            provider.as_str()
        ));
    }
    if matches!(source.status, RunStatus::Pending | RunStatus::Running) {
        return Err("Work 会话仍在执行中，请等待本轮完成后再继续".to_string());
    }

    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let context = crate::commands::session::continuation_context_for_run(source_run_id, anchor_id)?;

    if is_standalone {
        let new_id = Uuid::new_v4().to_string();
        let task_dir = paths.ensure_standalone_task_dir(&new_id)?;
        let work_root_str = task_dir.to_str().unwrap_or("").to_string();

        let mut meta = storage::runs::create_run_with_context(
            &new_id,
            "",
            &work_root_str,
            provider.as_str(),
            RunStatus::Pending,
            model
                .filter(|value| !value.trim().is_empty())
                .or(source.model.clone()),
            Some(source.id.clone()),
            source.remote_host_name.clone(),
            source.remote_cwd.clone(),
            source.remote_host_snapshot.clone(),
            source.platform_id.clone(),
            AppMode::Work,
            None,
        )?;
        meta.execution_path = Some(ExecutionPath::SessionActor);
        meta.work_execution_context = Some(ExecutionContext::Attended);
        meta.work_preset = source.work_preset;
        meta.continuation_context = Some(context);
        meta.permission_mode = source.permission_mode.clone();
        storage::runs::save_meta(&meta)?;

        if let Err(error) = storage::events::copy_bus_events_until(&source.id, &new_id, anchor_id) {
            storage::runs::update_status(&new_id, RunStatus::Failed, None, Some(error.clone()))
                .ok();
            storage::runs::delete_runs(std::slice::from_ref(&new_id)).ok();
            return Err(error);
        }
        if let Err(error) = crate::work::task_state::restore_from_events(&new_id) {
            storage::runs::update_status(&new_id, RunStatus::Failed, None, Some(error.clone()))
                .ok();
            storage::runs::delete_runs(std::slice::from_ref(&new_id)).ok();
            return Err(error);
        }

        let perm_mode = meta
            .permission_mode
            .as_deref()
            .unwrap_or(DEFAULT_PERMISSION_MODE);
        if let Err(error) = session_dispatch::start_session_impl(
            emitter,
            sessions,
            spawn_locks,
            cancel_token,
            new_id.clone(),
            Some(SessionMode::New),
            None,
            None,
            None,
            None,
            Some(perm_mode.to_string()),
            None,
        )
        .await
        {
            storage::runs::update_status(&new_id, RunStatus::Failed, None, Some(error.clone()))
                .ok();
            storage::runs::delete_runs(std::slice::from_ref(&new_id)).ok();
            return Err(error);
        }

        let meta = storage::runs::get_run(&new_id)
            .ok_or_else(|| format!("Work continuation run {} disappeared after startup", new_id))?;
        return Ok(meta.to_task_run(None, None, None));
    }

    let target = workspace::manager().open(ws_trimmed)?;
    let primary_work_root = workspace::manager().resolve_primary_work_root(&target)?;
    let work_root_str = primary_work_root.to_string_lossy().into_owned();

    let new_id = Uuid::new_v4().to_string();
    let mut meta = storage::runs::create_run_with_context(
        &new_id,
        "",
        &work_root_str,
        provider.as_str(),
        RunStatus::Pending,
        model
            .filter(|value| !value.trim().is_empty())
            .or(source.model.clone()),
        Some(source.id.clone()),
        source.remote_host_name.clone(),
        source.remote_cwd.clone(),
        source.remote_host_snapshot.clone(),
        source.platform_id.clone(),
        AppMode::Work,
        Some(ws_trimmed.to_string()),
    )?;
    meta.execution_path = Some(ExecutionPath::SessionActor);
    meta.work_task_id = None; // Continuation runs are interactive workspace turns; prevents ghost WorkRuns
    meta.work_run_id = Some(new_id.clone());
    meta.work_execution_context = Some(ExecutionContext::Attended);
    meta.continuation_context = Some(context);
    meta.permission_mode = source.permission_mode.clone();
    storage::runs::save_meta(&meta)?;

    if let Err(error) = storage::events::copy_bus_events_until(&source.id, &new_id, anchor_id) {
        storage::runs::update_status(&new_id, RunStatus::Failed, None, Some(error.clone())).ok();
        storage::runs::delete_runs(std::slice::from_ref(&new_id)).ok();
        return Err(error);
    }
    if let Err(error) = crate::work::task_state::restore_from_events(&new_id) {
        storage::runs::update_status(&new_id, RunStatus::Failed, None, Some(error.clone())).ok();
        storage::runs::delete_runs(std::slice::from_ref(&new_id)).ok();
        return Err(error);
    }

    let perm_mode = meta
        .permission_mode
        .as_deref()
        .unwrap_or(DEFAULT_PERMISSION_MODE);
    if let Err(error) = session_dispatch::start_session_impl(
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        new_id.clone(),
        Some(SessionMode::New),
        None,
        None,
        None,
        None,
        Some(perm_mode.to_string()),
        None,
    )
    .await
    {
        storage::runs::update_status(&new_id, RunStatus::Failed, None, Some(error.clone())).ok();
        storage::runs::delete_runs(std::slice::from_ref(&new_id)).ok();
        return Err(error);
    }

    let meta = storage::runs::get_run(&new_id)
        .ok_or_else(|| format!("Work continuation run {} disappeared after startup", new_id))?;
    Ok(meta.to_task_run(None, None, None))
}

pub async fn resume(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    run_id: &str,
    message: Option<&str>,
    attachments: Option<Vec<AttachmentData>>,
) -> Result<TaskRun, String> {
    let mut run = ensure_work_run(run_id, None)?;
    // Validate runtime router & capabilities before creating or mutating any records
    let adapter = work_runtime_router::route_agent_str(&run.agent)
        .map_err(|e| format!("Work Resume 不支持该 Runtime: {e}"))?;
    if !adapter.capabilities().supports_resume {
        return Err(format!(
            "Work Runtime '{}' 不支持恢复会话",
            adapter.provider().as_str()
        ));
    }

    let message = message.map(normalize_message).transpose()?;
    let mut meta_changed = false;

    if let Some(ref ws_id) = run.workspace_id {
        let workspace = workspace::manager().open(ws_id)?;
        let current_root = workspace::manager().resolve_primary_work_root(&workspace)?;
        let current_root_str = current_root.to_string_lossy().into_owned();
        if run.cwd != current_root_str {
            run.cwd = current_root_str;
            meta_changed = true;
        }
    }

    if run.work_run_id.is_none() {
        run.work_run_id = Some(run_id.to_string());
        meta_changed = true;
    }

    if run.work_execution_context.is_none() {
        run.work_execution_context = Some(ExecutionContext::Attended);
        meta_changed = true;
    }

    if meta_changed {
        storage::runs::save_meta(&run)?;
    }

    let effective_mode = run
        .permission_mode
        .as_deref()
        .unwrap_or_else(|| default_permission_mode(run.workspace_id.as_deref()));

    session_dispatch::start_session_impl(
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        run_id.to_string(),
        Some(SessionMode::Resume),
        None,
        message,
        attachments,
        None,
        Some(effective_mode.to_string()),
        None,
    )
    .await?;

    let meta = storage::runs::get_run(run_id)
        .ok_or_else(|| format!("Work run {} disappeared after resume", run_id))?;
    Ok(meta.to_task_run(None, None, None))
}

pub async fn send_message(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    run_id: &str,
    message: &str,
    attachments: Option<Vec<AttachmentData>>,
) -> Result<(), String> {
    let _run = ensure_work_run(run_id, None)?;
    let message = normalize_message(message)?;
    let run_meta = storage::runs::get_run(run_id)
        .ok_or_else(|| format!("Work run {run_id} disappeared before sending a message"))?;
    let work_context_plan =
        session_dispatch::assemble_work_context_plan_for_turn(&run_meta, &message).await?;
    crate::work::context::save(run_id, &work_context_plan)
        .map_err(|error| format!("Work Context Plan persistence failed: {error}"))?;
    emitter.persist_and_emit(
        run_id,
        &crate::models::BusEvent::WorkContextPlanUpdated {
            run_id: run_id.to_string(),
            plan: work_context_plan.clone(),
        },
    );
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let cmd_tx = {
        let map = sessions.lock().await;
        map.get(run_id)
            .map(|handle| handle.cmd_tx.clone())
            .ok_or_else(|| format!("Work session {} not found", run_id))?
    };
    cmd_tx
        .send(crate::agent::session_actor::ActorCommand::SendMessage {
            text: message,
            attachments: attachments.unwrap_or_default(),
            skills: Vec::new(),
            work_context_plan: Some(work_context_plan),
            reply: reply_tx,
        })
        .await
        .map_err(|_| "Work 会话 actor 已退出".to_string())?;
    reply_rx
        .await
        .map_err(|_| "Work 会话 actor 未返回消息结果".to_string())??;
    Ok(())
}

pub async fn stop(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    run_id: &str,
) -> Result<TaskRun, String> {
    let _run = ensure_work_run(run_id, None)?;
    session_dispatch::stop_session_impl(emitter, sessions, spawn_locks, run_id.to_string()).await?;
    let meta = storage::runs::get_run(run_id)
        .ok_or_else(|| format!("Work run {} not found after stop", run_id))?;
    Ok(meta.to_task_run(None, None, None))
}

pub fn validate_work_run_meta(run: &RunMeta, workspace_id: Option<&str>) -> Result<(), String> {
    if run.app_mode != AppMode::Work {
        return Err(format!("Run {} does not belong to Work mode", run.id));
    }
    if let Some(expected) = workspace_id {
        if run.workspace_id.as_deref() != Some(expected) {
            return Err("Work session does not belong to the selected Workspace".into());
        }
    }
    Ok(())
}

pub fn ensure_work_run(run_id: &str, workspace_id: Option<&str>) -> Result<RunMeta, String> {
    let run = storage::runs::get_run(run_id).ok_or_else(|| format!("Run {} not found", run_id))?;
    validate_work_run_meta(&run, workspace_id)?;
    Ok(run)
}

fn normalize_message(message: &str) -> Result<String, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Work 消息不能为空".into());
    }
    if message.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!("Work 消息不能超过 {MAX_MESSAGE_CHARS} 个字符"));
    }
    Ok(message.to_string())
}

#[cfg(test)]
fn lifecycle_session_status(
    status: &crate::models::RunStatus,
    error_message: Option<&str>,
) -> crate::models::RunStatus {
    if matches!(status, crate::models::RunStatus::Idle) && error_message.is_some() {
        crate::models::RunStatus::Failed
    } else {
        status.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{execution_mode_cli, lifecycle_session_status, normalize_message, WorkPaths};

    #[test]
    fn workspace_execution_modes_map_to_runtime_permission_modes() {
        use crate::work::models::WorkExecutionMode;

        assert_eq!(execution_mode_cli(WorkExecutionMode::PlanFirst), "plan");
        assert_eq!(execution_mode_cli(WorkExecutionMode::Direct), "default");
        assert_eq!(execution_mode_cli(WorkExecutionMode::Auto), "auto");
        assert_eq!(
            execution_mode_cli(WorkExecutionMode::FullAccess),
            "bypassPermissions"
        );
    }

    #[test]
    fn treats_idle_without_error_as_completed_and_idle_with_error_as_failed() {
        assert_eq!(
            lifecycle_session_status(&crate::models::RunStatus::Idle, None),
            crate::models::RunStatus::Idle
        );
        assert_eq!(
            lifecycle_session_status(&crate::models::RunStatus::Idle, Some("provider error")),
            crate::models::RunStatus::Failed
        );
        assert_eq!(
            lifecycle_session_status(&crate::models::RunStatus::Completed, None),
            crate::models::RunStatus::Completed
        );
    }

    #[test]
    fn normalizes_work_messages() {
        assert_eq!(normalize_message("  hello  ").unwrap(), "hello");
        assert!(normalize_message("\n\t").is_err());
    }

    #[test]
    fn rejects_oversized_work_messages() {
        let message = "x".repeat(super::MAX_MESSAGE_CHARS + 1);
        assert!(normalize_message(&message).is_err());
    }

    #[test]
    fn ensure_work_run_rejects_non_work_runs() {
        let meta = crate::models::RunMeta {
            id: "run-code-mode".to_string(),
            prompt: "test".to_string(),
            cwd: "/tmp".to_string(),
            agent: "pi".to_string(),
            code_standalone_task: false,
            app_mode: crate::work::models::AppMode::Code,
            agent_target: Some(crate::models::AgentTarget::PiCode),
            workspace_id: None,
            work_task_id: None,
            work_run_id: None,
            work_execution_context: None,
            work_preset: None,
            auth_mode: "default".to_string(),
            status: crate::models::RunStatus::Pending,
            started_at: "2026-08-15T00:00:00Z".to_string(),
            ended_at: None,
            exit_code: None,
            error_message: None,
            session_id: None,
            result_subtype: None,
            model: None,
            effort: None,
            permission_mode: None,
            parent_run_id: None,
            continuation_context: None,
            name: None,
            remote_host_name: None,
            remote_cwd: None,
            remote_host_snapshot: None,
            platform_id: None,
            platform_base_url: None,
            source: None,
            cli_import_watermark: None,
            cli_session_path: None,
            cli_usage_incomplete: None,
            deleted_at: None,
            no_session_persistence: false,
            execution_path: None,
            conversation_ref: None,
            codex_process_seq: None,
            codex_imported_rollouts: None,
            pinned: None,
            archived: None,
            unread: None,
        };
        let err = super::validate_work_run_meta(&meta, None).unwrap_err();
        assert!(err.contains("does not belong to Work mode"));
    }

    #[test]
    fn ensure_work_run_rejects_mismatched_workspace() {
        let meta = crate::models::RunMeta {
            id: "run-ws-test".to_string(),
            prompt: "test".to_string(),
            cwd: "/tmp".to_string(),
            agent: "pi".to_string(),
            code_standalone_task: false,
            app_mode: crate::work::models::AppMode::Work,
            agent_target: Some(crate::models::AgentTarget::Work),
            workspace_id: Some("ws-a".to_string()),
            work_task_id: None,
            work_run_id: None,
            work_execution_context: None,
            work_preset: None,
            auth_mode: "default".to_string(),
            status: crate::models::RunStatus::Pending,
            started_at: "2026-08-15T00:00:00Z".to_string(),
            ended_at: None,
            exit_code: None,
            error_message: None,
            session_id: None,
            result_subtype: None,
            model: None,
            effort: None,
            permission_mode: None,
            parent_run_id: None,
            continuation_context: None,
            name: None,
            remote_host_name: None,
            remote_cwd: None,
            remote_host_snapshot: None,
            platform_id: None,
            platform_base_url: None,
            source: None,
            cli_import_watermark: None,
            cli_session_path: None,
            cli_usage_incomplete: None,
            deleted_at: None,
            no_session_persistence: false,
            execution_path: None,
            conversation_ref: None,
            codex_process_seq: None,
            codex_imported_rollouts: None,
            pinned: None,
            archived: None,
            unread: None,
        };
        let err = super::validate_work_run_meta(&meta, Some("ws-b")).unwrap_err();
        assert!(err.contains("does not belong to the selected Workspace"));
    }

    #[test]
    fn work_scope_resolves_accurately() {
        let standalone_scope = crate::work::models::WorkScope::new("run-1", None, None);
        assert!(standalone_scope.is_standalone());
        assert!(!standalone_scope.is_automation());

        let ws_chat_scope =
            crate::work::models::WorkScope::new("run-2", Some("ws-1".to_string()), None);
        assert!(!ws_chat_scope.is_standalone());
        assert!(!ws_chat_scope.is_automation());

        let automation_scope = crate::work::models::WorkScope::new(
            "run-3",
            Some("ws-1".to_string()),
            Some("task-1".to_string()),
        );
        assert!(!automation_scope.is_standalone());
        assert!(automation_scope.is_automation());
    }

    #[tokio::test]
    async fn settlement_locks_coordinate_cleanly() {
        use super::{request_user_stop, try_finalize_parent_run, SETTLEMENT_LOCKS};

        let run_id = "test-settlement-coordination-run";

        // Verify lock acquisition and release
        {
            let _guard = SETTLEMENT_LOCKS.acquire(run_id).await;
        }

        // Verify that try_finalize_parent_run acquires lock and completes without deadlock
        let finalize_res = try_finalize_parent_run(run_id).await;
        assert!(finalize_res.is_ok());

        // Verify that request_user_stop acquires lock and completes without deadlock
        let stop_res = request_user_stop(run_id).await;
        assert!(stop_res.is_ok());
    }

    static ENV_LOCK: once_cell::sync::Lazy<std::sync::Mutex<()>> =
        once_cell::sync::Lazy::new(|| std::sync::Mutex::new(()));

    fn setup_test_env() -> (
        std::sync::MutexGuard<'static, ()>,
        tempfile::TempDir,
        WorkPaths,
    ) {
        let guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        std::env::set_var("AGENTCABIN_DATA_DIR", temp.path());
        crate::storage::runs::invalidate_runs_cache();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        (guard, temp, paths)
    }

    #[tokio::test]
    async fn barrier_concurrency_stop_and_success_racing() {
        use super::{
            request_user_stop, BroadcastEmitter, RuntimeTurnOutcome, PENDING_PARENT_SETTLEMENTS,
        };
        use crate::models::{RunMeta, RunStatus};
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let (_env_guard, _temp, _paths) = setup_test_env();
        let run_id = "test-barrier-racing-run";
        let meta = RunMeta {
            id: run_id.to_string(),
            prompt: "racing test".to_string(),
            cwd: "/tmp".to_string(),
            agent: "pi".to_string(),
            code_standalone_task: false,
            app_mode: crate::work::models::AppMode::Work,
            agent_target: Some(crate::models::AgentTarget::Work),
            workspace_id: None,
            work_task_id: None,
            work_run_id: None,
            work_execution_context: None,
            work_preset: None,
            auth_mode: "default".to_string(),
            status: RunStatus::Running,
            started_at: "2026-08-15T00:00:00Z".to_string(),
            ended_at: None,
            exit_code: None,
            error_message: None,
            session_id: None,
            result_subtype: None,
            model: None,
            effort: None,
            permission_mode: None,
            parent_run_id: None,
            continuation_context: None,
            name: None,
            remote_host_name: None,
            remote_cwd: None,
            remote_host_snapshot: None,
            platform_id: None,
            platform_base_url: None,
            source: None,
            cli_import_watermark: None,
            cli_session_path: None,
            cli_usage_incomplete: None,
            deleted_at: None,
            no_session_persistence: false,
            execution_path: None,
            conversation_ref: None,
            codex_process_seq: None,
            codex_imported_rollouts: None,
            pinned: None,
            archived: None,
            unread: None,
        };
        crate::storage::runs::save_meta(&meta).unwrap();

        // Seed a pending settlement for this run
        let dummy_emitter = BroadcastEmitter::mock();
        {
            let mut pending = PENDING_PARENT_SETTLEMENTS.lock().await;
            pending.insert(
                run_id.to_string(),
                (dummy_emitter.clone(), RuntimeTurnOutcome::Success),
            );
        }

        let barrier = Arc::new(Barrier::new(2));
        let b1 = barrier.clone();
        let b2 = barrier.clone();

        let stop_handle = tokio::spawn(async move {
            b1.wait().await;
            request_user_stop(run_id).await
        });

        let finalize_handle = tokio::spawn(async move {
            b2.wait().await;
            super::try_finalize_parent_run(run_id).await
        });

        let (stop_res, finalize_res) = tokio::join!(stop_handle, finalize_handle);
        let stop_acceptance = stop_res.unwrap().expect("stop must succeed");
        finalize_res.unwrap().expect("finalize must succeed");

        let current_meta = crate::storage::runs::get_run(run_id).expect("run must exist");
        if stop_acceptance.accepted {
            // If stop was accepted, current status must be Stopped, and pending map must be cleared
            assert_eq!(current_meta.status, RunStatus::Stopped);
            let pending = PENDING_PARENT_SETTLEMENTS.lock().await;
            assert!(!pending.contains_key(run_id));
        } else {
            // If stop was not accepted, it must be because it was already completed/failed
            assert!(matches!(
                stop_acceptance.previous_status,
                Some(RunStatus::Completed) | Some(RunStatus::Failed)
            ));
        }
    }

    #[tokio::test]
    async fn handle_process_stopped_error_propagation() {
        use super::{handle_process_stopped, BroadcastEmitter};
        use crate::models::{RunMeta, RunStatus};

        let (_env_guard, _temp, _paths) = setup_test_env();
        let run_id = "test-process-stopped-propagation";
        // Create a Work run with a non-existent task_id so controller fails during complete_or_fail_run
        let meta = RunMeta {
            id: run_id.to_string(),
            prompt: "error prop test".to_string(),
            cwd: "/tmp".to_string(),
            agent: "pi".to_string(),
            code_standalone_task: false,
            app_mode: crate::work::models::AppMode::Work,
            agent_target: Some(crate::models::AgentTarget::Work),
            workspace_id: None,
            work_task_id: Some("non-existent-task-id-123".to_string()),
            work_run_id: None,
            work_execution_context: None,
            work_preset: None,
            auth_mode: "default".to_string(),
            status: RunStatus::Running,
            started_at: "2026-08-15T00:00:00Z".to_string(),
            ended_at: None,
            exit_code: None,
            error_message: None,
            session_id: None,
            result_subtype: None,
            model: None,
            effort: None,
            permission_mode: None,
            parent_run_id: None,
            continuation_context: None,
            name: None,
            remote_host_name: None,
            remote_cwd: None,
            remote_host_snapshot: None,
            platform_id: None,
            platform_base_url: None,
            source: None,
            cli_import_watermark: None,
            cli_session_path: None,
            cli_usage_incomplete: None,
            deleted_at: None,
            no_session_persistence: false,
            execution_path: None,
            conversation_ref: None,
            codex_process_seq: None,
            codex_imported_rollouts: None,
            pinned: None,
            archived: None,
            unread: None,
        };
        crate::storage::runs::save_meta(&meta).unwrap();

        let dummy_emitter = BroadcastEmitter::mock();
        let res = handle_process_stopped(
            &dummy_emitter,
            run_id,
            crate::agent::session_actor::RuntimeStopReason::UserStop,
        )
        .await;

        // Since the task does not exist, controller.complete_or_fail_run returns Err and handle_process_stopped bubbles it up via `?`
        assert!(
            res.is_err(),
            "Expected handle_process_stopped to propagate error, but got: {:?}",
            res
        );
    }
}
