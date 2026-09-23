use std::sync::Arc;
use tauri::State;
use tokio_util::sync::CancellationToken;

use super::{ensure_work_enabled, resume_work_session_after_approval, INBOX_DELIVERY_LOCK};
use crate::agent::adapter::ActorSessionMap;
use crate::agent::spawn_locks::SpawnLocks;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::models::{
    TaskStandingRule, WorkArtifactRequirement, WorkPolicy, WorkPreset, WorkRun, WorkRunStatus,
    WorkRunTrigger, WorkScheduleConfig, WorkTask, WorkTaskState, WorkWorkspace,
};
use crate::work::policy::PolicyEvaluator;
use crate::work::{paths::WorkPaths, session, tasks::TaskManager, workspace};

#[tauri::command]
pub fn work_create_task(
    workspace_id: String,
    title: String,
    instructions: String,
    policy: Option<WorkPolicy>,
    schedule: Option<WorkScheduleConfig>,
    required_artifacts: Option<Vec<String>>,
    artifact_requirements: Option<Vec<WorkArtifactRequirement>>,
) -> Result<WorkTask, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    let mut task = manager.create_task_with_schedule(
        &workspace_id,
        &title,
        &instructions,
        policy,
        schedule,
    )?;
    task.required_artifacts = required_artifacts
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    task.artifact_requirements = artifact_requirements.unwrap_or_default();
    manager.update_task(&mut task)?;
    Ok(task)
}

#[tauri::command]
pub fn work_delete_task(id: String) -> Result<(), String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.delete_task(&id)
}

#[tauri::command]
pub fn work_duplicate_task(id: String) -> Result<WorkTask, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.duplicate_task(&id)
}

#[tauri::command]
pub fn work_get_task(id: String) -> Result<WorkTask, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.get_task(&id)
}

#[tauri::command]
pub fn work_list_tasks(workspace_id: Option<String>) -> Result<Vec<WorkTask>, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.list_tasks(workspace_id.as_deref())
}

#[tauri::command]
pub fn work_update_task(task: WorkTask) -> Result<(), String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    // Enforce the workspace autonomy ceiling: a task's execution_mode may be
    // stricter than the workspace default but never more permissive.
    let mut task = task;
    let ws_mgr = workspace::WorkspaceManager::new(paths.clone());
    if let Ok(ws) = ws_mgr.get(&task.workspace_id) {
        task.policy.execution_mode = PolicyEvaluator::clamp_execution_mode_to_ceiling(
            task.policy.execution_mode,
            ws.default_policy.execution_mode,
        );
    }
    let manager = TaskManager::new(paths);
    manager.update_task(&mut task)
}

/// Set the workspace-level default policy. New tasks inherit this; existing
/// tasks are NOT retroactively modified (they keep their own policy).
#[tauri::command]
pub fn work_set_workspace_default_policy(
    workspace_id: String,
    policy: WorkPolicy,
) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let ws_mgr = workspace::WorkspaceManager::new(paths);
    let mut ws = ws_mgr.get(&workspace_id)?;
    ws.default_policy = policy;
    ws_mgr.update(&mut ws)?;
    Ok(ws)
}

#[tauri::command]
pub fn work_set_workspace_model_preferences(
    workspace_id: String,
    model: Option<String>,
    effort: Option<String>,
) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    let ws_mgr = workspace::WorkspaceManager::new(WorkPaths::app());
    let mut ws = ws_mgr.get(&workspace_id)?;
    ws.default_model = model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    ws.default_effort = effort
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    ws_mgr.update(&mut ws)?;
    Ok(ws)
}

#[tauri::command]
pub fn work_add_standing_rule(task_id: String, rule: TaskStandingRule) -> Result<WorkTask, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.add_standing_rule(&task_id, rule)
}

#[tauri::command]
pub fn work_get_automation_stats() -> Result<crate::work::models::WorkAutomationStats, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.get_automation_stats()
}

#[tauri::command]
pub fn work_list_task_runs(
    task_id: String,
) -> Result<Vec<crate::work::models::WorkTaskRunSummary>, String> {
    ensure_work_enabled()?;
    let manager = TaskManager::new(WorkPaths::app());
    manager.list_task_run_summaries(&task_id)
}

use crate::work::models::ExecutionContext;

#[allow(clippy::too_many_arguments)]
pub async fn start_work_task_run(
    paths: &WorkPaths,
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    task_id: &str,
    trigger: WorkRunTrigger,
    execution_context: ExecutionContext,
    scheduled_for: Option<&str>,
) -> Result<WorkRun, String> {
    start_work_task_run_with_overrides(
        paths,
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        task_id,
        trigger,
        execution_context,
        scheduled_for,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn start_work_task_run_with_overrides(
    paths: &WorkPaths,
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    task_id: &str,
    trigger: WorkRunTrigger,
    execution_context: ExecutionContext,
    scheduled_for: Option<&str>,
    overrides: Option<crate::work::runtime::WorkLaunchOverrides>,
) -> Result<WorkRun, String> {
    let tm = TaskManager::new(paths.clone());
    let task = tm.get_task(task_id)?;

    let run = if trigger == WorkRunTrigger::Scheduled {
        let sched_time =
            scheduled_for.ok_or_else(|| "Scheduled run requires scheduled_for time".to_string())?;
        let runs = tm.list_runs(task_id)?;
        if let Some(existing) = runs
            .into_iter()
            .find(|r| r.scheduled_for.as_deref() == Some(sched_time) && r.status.is_active())
        {
            existing
        } else {
            if tm.has_active_run(task_id)? {
                return Err(format!(
                    "Task '{task_id}' already has an active run in progress"
                ));
            }
            tm.start_scheduled_run(task_id, sched_time)?
        }
    } else {
        if tm.has_active_run(task_id)? {
            return Err(format!(
                "Task '{task_id}' already has an active run in progress"
            ));
        }
        tm.start_run_internal(task_id, None, trigger, None, execution_context)?
    };

    // Create the durable WorkRun before validating provider availability. If
    // the profile is invalid or its provider is unsupported, the failure must
    // still be visible as a failed WorkRun (especially for scheduled runs),
    // rather than disappearing as an untracked async error.
    let provider = match overrides.as_ref().and_then(|o| o.runtime) {
        Some(p) => p,
        None => match crate::work::profile::resolve_new_work_runtime_from_storage() {
            Ok(provider) => provider,
            Err(error) => {
                return Err(mark_work_run_failed(paths, task_id, &run.id, error));
            }
        },
    };
    if let Err(error) = crate::work::runtime::get_pi_work_runtime(provider.as_str()) {
        return Err(mark_work_run_failed(
            paths,
            task_id,
            &run.id,
            error.to_string(),
        ));
    }

    let prompt = if task.instructions.trim().is_empty() {
        task.title.clone()
    } else {
        format!(
            "Task Goal: {}\n\nInstructions:\n{}",
            task.title, task.instructions
        )
    };

    match session::start_work_session_with_overrides(
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        &task.workspace_id,
        Some(task_id),
        Some(&run.id),
        execution_context,
        &prompt,
        None,
        None,
        WorkPreset::default(),
        overrides,
    )
    .await
    {
        Ok(session_run) => {
            let attached_run = match attach_work_session(&tm, task_id, &run.id, &session_run.id) {
                Ok(attached_run) => attached_run,
                Err(error) => {
                    if let Err(stop_error) =
                        session::stop(emitter, sessions, spawn_locks, &session_run.id).await
                    {
                        log::warn!(
                            "[work/task] Failed to stop unbound Work session {}: {stop_error}",
                            session_run.id
                        );
                    }
                    return Err(mark_work_run_failed(paths, task_id, &run.id, error));
                }
            };

            Ok(attached_run)
        }
        Err(error) => Err(mark_work_run_failed(paths, task_id, &run.id, error)),
    }
}

pub(crate) fn mark_work_run_failed(
    paths: &WorkPaths,
    task_id: &str,
    work_run_id: &str,
    failure: String,
) -> String {
    let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
    match controller.complete_or_fail_run(
        task_id,
        work_run_id,
        WorkRunStatus::Failed,
        Some(failure.clone()),
        None,
    ) {
        Ok(_) => failure,
        Err(cleanup_error) => {
            log::error!(
                "[work/task] Failed to persist WorkRun failure after startup error: {cleanup_error}"
            );
            format!("{failure}; additionally failed to persist WorkRun failure: {cleanup_error}")
        }
    }
}

/// Start a fresh attended run for a terminal failed/cancelled automation.
/// The original run remains immutable history; retry is intentionally a new
/// WorkRun so its ledger and artifacts cannot be confused with the failure.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn retry_failed_work_run(
    paths: &WorkPaths,
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    task_id: &str,
    run_id: &str,
) -> Result<WorkRun, String> {
    let task_manager = TaskManager::new(paths.clone());
    let task = task_manager.get_task(task_id)?;
    let run = task_manager.get_run(task_id, run_id)?;
    if run.task_id != task_id || run.workspace_id != task.workspace_id {
        return Err("WorkRun ownership does not match WorkTask".to_string());
    }
    if !matches!(run.status, WorkRunStatus::Failed | WorkRunStatus::Cancelled) {
        return Err(format!(
            "只有失败或已取消的 Run 可以重试，当前状态为 {:?}",
            run.status
        ));
    }

    start_work_task_run(
        paths,
        emitter,
        sessions,
        spawn_locks,
        cancel_token,
        task_id,
        WorkRunTrigger::Manual,
        ExecutionContext::Attended,
        None,
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn work_retry_task_run(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    task_id: String,
    run_id: String,
) -> Result<WorkRun, String> {
    ensure_work_enabled()?;
    let _guard = INBOX_DELIVERY_LOCK.lock().await;
    retry_failed_work_run(
        &WorkPaths::app(),
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        &task_id,
        &run_id,
    )
    .await
}

pub(crate) fn attach_work_session(
    task_manager: &TaskManager,
    task_id: &str,
    work_run_id: &str,
    session_id: &str,
) -> Result<WorkRun, String> {
    task_manager
        .attach_session_id(task_id, work_run_id, session_id)
        .map_err(|error| {
            format!(
                "Failed to attach Work session '{session_id}' to WorkRun '{work_run_id}': {error}"
            )
        })
}

#[tauri::command]
pub async fn work_start_run(
    emitter: tauri::State<'_, Arc<BroadcastEmitter>>,
    sessions: tauri::State<'_, ActorSessionMap>,
    spawn_locks: tauri::State<'_, SpawnLocks>,
    cancel_token: tauri::State<'_, CancellationToken>,
    task_id: String,
    _session_id: Option<String>,
    trigger: Option<WorkRunTrigger>,
) -> Result<WorkRun, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let trig = trigger.unwrap_or(WorkRunTrigger::Manual);
    start_work_task_run(
        &paths,
        &emitter,
        &sessions,
        &spawn_locks,
        &cancel_token,
        &task_id,
        trig,
        ExecutionContext::Attended,
        None,
    )
    .await
}

#[tauri::command]
pub fn work_finish_run(
    task_id: String,
    run_id: String,
    status: WorkRunStatus,
    error_message: Option<String>,
    task_state: Option<WorkTaskState>,
) -> Result<WorkRun, String> {
    ensure_work_enabled()?;
    let controller = crate::work::lifecycle::WorkHarnessController::new(WorkPaths::app());
    controller.complete_or_fail_run(&task_id, &run_id, status, error_message, task_state)
}

/// Get the current GoalSpec for a WorkRun.
#[tauri::command]
pub fn work_get_goal_spec(
    task_id: String,
    run_id: String,
) -> Result<Option<crate::work::models::GoalSpec>, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let task_manager = crate::work::tasks::TaskManager::new(paths);
    let run = task_manager.get_run(&task_id, &run_id)?;
    Ok(run.task_state.goal_spec)
}

/// Force trigger a machine-first verification on the GoalSpec of a WorkRun.
#[tauri::command]
pub fn work_verify_goal(
    task_id: String,
    run_id: String,
) -> Result<crate::work::models::GoalSpec, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let task_manager = crate::work::tasks::TaskManager::new(paths.clone());
    let run = task_manager.get_run(&task_id, &run_id)?;
    let goal_spec = if let Some(goal) = run.task_state.goal_spec.clone() {
        goal
    } else {
        let task = task_manager.get_task(&task_id)?;
        if task.required_artifacts.is_empty() && task.artifact_requirements.is_empty() {
            return Err("当前 Run 未配置显式交付物验收标准".to_string());
        }
        crate::work::goal::GoalBuilder::build(&run.workspace_id, &run_id, "", Some(&task))
    };
    let verifier =
        crate::work::goal::GoalVerifier::new(&paths, &run.workspace_id, &task_id, &run_id);
    let verified = verifier.verify(goal_spec);
    let mut state = run.task_state.clone();
    state.goal_spec = Some(verified.clone());
    task_manager.set_run_state(
        &task_id,
        &run_id,
        run.status,
        run.error_message,
        Some(state),
    )?;
    Ok(verified)
}

/// Advance the goal auto-repair loop manually or from frontend.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn work_trigger_goal_repair(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    task_id: String,
    run_id: String,
) -> Result<crate::work::models::GoalSpec, String> {
    trigger_goal_repair_impl(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        task_id,
        run_id,
    )
    .await
}

/// Headless-capable inner impl of [`work_trigger_goal_repair`] — takes
/// concrete state values instead of `tauri::State` extracts.
pub(crate) async fn trigger_goal_repair_impl(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    task_id: String,
    run_id: String,
) -> Result<crate::work::models::GoalSpec, String> {
    ensure_work_enabled()?;
    let _guard = INBOX_DELIVERY_LOCK.lock().await;
    let paths = WorkPaths::app();
    let task_manager = crate::work::tasks::TaskManager::new(paths.clone());
    let run = task_manager.get_run(&task_id, &run_id)?;
    let goal_spec = if let Some(goal) = run.task_state.goal_spec.clone() {
        goal
    } else {
        let task = task_manager.get_task(&task_id)?;
        if task.required_artifacts.is_empty() && task.artifact_requirements.is_empty() {
            return Err("当前 Run 未配置显式交付物验收标准".to_string());
        }
        crate::work::goal::GoalBuilder::build(&run.workspace_id, &run_id, "", Some(&task))
    };
    let verifier =
        crate::work::goal::GoalVerifier::new(&paths, &run.workspace_id, &task_id, &run_id);
    let verified = verifier.verify(goal_spec);
    let (repaired, can_continue) = crate::work::goal::GoalRepairLoop::advance_repair(verified);
    let mut state = run.task_state.clone();
    state.goal_spec = Some(repaired.clone());
    let previous_state = run.task_state.clone();
    let previous_error = run.error_message.clone();

    if can_continue {
        let session_id = run.session_id.as_deref().ok_or_else(|| {
            "目标修复需要关联的 Work 会话，但当前 Run 没有 session_id".to_string()
        })?;
        task_manager.set_run_state(
            &task_id,
            &run_id,
            WorkRunStatus::Running,
            repaired.repair_instruction.clone(),
            Some(state),
        )?;
        if let Some(instruction) = repaired.repair_instruction.as_deref() {
            if let Err(error) = resume_work_session_after_approval(
                emitter,
                sessions,
                spawn_locks,
                cancel_token,
                session_id,
                instruction,
            )
            .await
            {
                let restore_error = task_manager.set_run_state(
                    &task_id,
                    &run_id,
                    run.status,
                    previous_error,
                    Some(previous_state),
                );
                return Err(match restore_error {
                    Ok(_) => format!("目标修复已记录，但 Work 会话恢复失败：{error}"),
                    Err(restore_error) => format!(
                        "目标修复已记录，但 Work 会话恢复失败：{error}；原状态恢复失败：{restore_error}"
                    ),
                });
            }
        }
    } else {
        task_manager.set_run_state(
            &task_id,
            &run_id,
            WorkRunStatus::WaitingInput,
            repaired.repair_instruction.clone(),
            Some(state),
        )?;
    }
    Ok(repaired)
}
