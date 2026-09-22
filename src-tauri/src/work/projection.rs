//! WorkProjection: unified single-authority projection for Work runs.
//!
//! Replaces multi-layer frontend state derivation and dual-track resolution
//! (WorkRunProgressView vs progressSnapshot) with a single authoritative projection
//! that supports both interactive sessions and automated tasks.

use chrono::Utc;

use crate::models::StructuredTaskStatus;
use crate::work::artifacts;
use crate::work::guardian::Guardian;
use crate::work::interaction::InteractionManager;
use crate::work::ledger::WorkRuntimeLedger;
use crate::work::models::{
    PendingInteractionKind, RunHealth, RuntimeFact, WorkAvailableActions, WorkCurrentActivity,
    WorkProgressAttention, WorkProgressToolSummary, WorkProjection, WorkRunHealthView,
    WorkRunProgressPhase, WorkRunStatus,
};
use crate::work::paths::WorkPaths;
use crate::work::progress::{
    friendly_tool_label, resolve_ledger_facts, resolve_subagents, LedgerFactsResolution,
};
use crate::work::tasks::TaskManager;

/// Project the authoritative unified WorkProjection for any run (interactive or automated).
pub fn project_work_projection(paths: &WorkPaths, run_id: &str) -> Result<WorkProjection, String> {
    let meta = crate::storage::runs::get_run(run_id);
    let task_manager = TaskManager::new(paths.clone());

    // 1. Resolve scope & underlying Run authority:
    let (
        workspace_id,
        automation_task_id,
        found_work_run,
        runtime_status,
        automation_status,
        raw_status,
    ) = if let Some(ref m) = meta {
        let ws = m.workspace_id.clone();
        let target_run_id = m.work_run_id.as_deref().unwrap_or(run_id);
        let mut auto_task = m.work_task_id.clone();
        let mut work_run = if let Some(ref tid) = auto_task {
            task_manager.get_run(tid, target_run_id).ok()
        } else {
            None
        };
        // Case 3 fallback: If work_run not found directly by auto_task, search task_manager by run_id
        if work_run.is_none() {
            if let Ok(Some(found)) = task_manager.find_run_by_id(target_run_id) {
                if auto_task.is_none() {
                    auto_task = Some(found.task_id.clone());
                }
                work_run = Some(found);
            }
        }
        let auto_status = work_run.as_ref().map(|r| r.status);
        let status = if let Some(s) = auto_status {
            s
        } else {
            match m.status {
                crate::models::RunStatus::Completed => WorkRunStatus::Completed,
                crate::models::RunStatus::Failed => WorkRunStatus::Failed,
                crate::models::RunStatus::Stopped => WorkRunStatus::Cancelled,
                crate::models::RunStatus::Running => WorkRunStatus::Running,
                crate::models::RunStatus::Pending => WorkRunStatus::Queued,
                crate::models::RunStatus::Idle => WorkRunStatus::Completed,
            }
        };
        (
            ws,
            auto_task,
            work_run,
            m.status.clone(),
            auto_status,
            status,
        )
    } else {
        // Fallback: search tasks for this run_id
        let tasks = task_manager.list_tasks(None).unwrap_or_default();
        let mut resolved = None;
        for t in tasks {
            if let Ok(r) = task_manager.get_run(&t.id, run_id) {
                let run_meta_status = match r.status {
                    WorkRunStatus::Running
                    | WorkRunStatus::WaitingApproval
                    | WorkRunStatus::WaitingInput
                    | WorkRunStatus::WaitingDelivery
                    | WorkRunStatus::Recoverable => crate::models::RunStatus::Running,
                    WorkRunStatus::Completed => crate::models::RunStatus::Completed,
                    WorkRunStatus::Failed => crate::models::RunStatus::Failed,
                    WorkRunStatus::Cancelled | WorkRunStatus::Skipped => {
                        crate::models::RunStatus::Stopped
                    }
                    WorkRunStatus::Queued => crate::models::RunStatus::Pending,
                };
                resolved = Some((
                    Some(r.workspace_id.clone()),
                    Some(t.id),
                    Some(r.clone()),
                    run_meta_status,
                    Some(r.status),
                    r.status,
                ));
                break;
            }
        }
        resolved.ok_or_else(|| format!("Run '{run_id}' not found in storage or tasks"))?
    };

    // Ledger namespace scope: if an automation task exists, use it; otherwise run_id.
    let ledger_task_scope = automation_task_id.as_deref().unwrap_or(run_id);

    // 2. Resolve durable task state (goal, plan, checkpoint)
    let durable_task_state = crate::work::task_state::load(run_id)
        .ok()
        .flatten()
        .or_else(|| {
            if let Some(ref run) = found_work_run {
                if let Ok(Some(s)) = crate::work::task_state::load(&run.id) {
                    Some(s)
                } else {
                    Some(run.task_state.clone())
                }
            } else {
                None
            }
        })
        .unwrap_or_default();

    let goal = durable_task_state
        .goal
        .clone()
        .or_else(|| {
            found_work_run
                .as_ref()
                .and_then(|r| r.task_state.goal.clone())
        })
        .or_else(|| meta.as_ref().map(|m| m.prompt.clone()));
    let goal_spec = durable_task_state.goal_spec.clone().or_else(|| {
        found_work_run
            .as_ref()
            .and_then(|r| r.task_state.goal_spec.clone())
    });
    let plan = durable_task_state.plan;
    let checkpoint = durable_task_state.checkpoint;

    // 3. Resolve pending human interactions
    let interaction_mgr = InteractionManager::new(paths.clone());
    let pending_interactions = interaction_mgr
        .list_pending(automation_task_id.as_deref(), Some(run_id))
        .or_else(|_| interaction_mgr.list_pending(None, Some(run_id)))
        .unwrap_or_default();

    // 4. Resolve artifacts
    let artifacts = if let Some(ref ws) = workspace_id {
        if !ws.trim().is_empty() {
            artifacts::list_with_paths(paths, ws, Some(run_id)).unwrap_or_default()
        } else {
            artifacts::list_standalone_with_paths(paths, run_id).unwrap_or_default()
        }
    } else {
        artifacts::list_standalone_with_paths(paths, run_id).unwrap_or_default()
    };

    // 5. Resolve subagents
    let agents = resolve_subagents(paths, ledger_task_scope, run_id).unwrap_or_default();

    // 6. Resolve ledger facts, tools, and active step
    let LedgerFactsResolution {
        tool_summary,
        active_tool,
        active_step: ledger_active_step,
    } = resolve_ledger_facts(paths, ledger_task_scope, run_id).unwrap_or(LedgerFactsResolution {
        tool_summary: WorkProgressToolSummary::default(),
        active_tool: None,
        active_step: None,
    });

    let has_running_reviewer = agents
        .iter()
        .any(|a| a.role == "reviewer" && a.status == "running");
    let has_running_worker = agents
        .iter()
        .any(|a| a.role == "worker" && a.status == "running");
    let has_running_researcher = agents
        .iter()
        .any(|a| a.role == "researcher" && a.status == "running");
    let total_researchers = agents.iter().filter(|a| a.role == "researcher").count();
    let completed_researchers = agents
        .iter()
        .filter(|a| a.role == "researcher" && a.status == "completed")
        .count();

    let active_step = ledger_active_step.or_else(|| {
        plan.iter()
            .find(|s| s.status == StructuredTaskStatus::InProgress)
            .map(|s| (s.id.clone(), s.text.clone()))
    });

    // 7. Guardian evaluation & Health
    let (health_view, latest_guardian_reason) =
        if let Ok(ledger) = WorkRuntimeLedger::open(paths, ledger_task_scope, run_id) {
            if let Ok(facts) = ledger.list_facts() {
                let guardian_config = found_work_run
                    .as_ref()
                    .and_then(|r| r.guardian_config.clone())
                    .unwrap_or_default();
                let report = Guardian::evaluate_health(
                    &facts,
                    raw_status,
                    &guardian_config,
                    &Utc::now().to_rfc3339(),
                    None,
                );
                let latest_reason = facts
                    .iter()
                    .rev()
                    .find_map(|f| match f {
                        RuntimeFact::GuardianAnomalyDetected { reason, .. } => Some(reason.clone()),
                        _ => None,
                    })
                    .or_else(|| report.reason.clone());
                (
                    Some(WorkRunHealthView {
                        status: report.health,
                        reason: report.reason.or(latest_reason.clone()),
                        warning_count: report.warning_count,
                        stalled_since: report.stalled_since,
                        budget: Some(report.budget_view),
                        last_meaningful_progress_at: report.last_meaningful_progress_at,
                        liveness: Some(report.liveness),
                    }),
                    latest_reason,
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

    // 8. Refine status for active interactions
    let status = if raw_status.is_active() && !pending_interactions.is_empty() {
        let has_approval = pending_interactions.iter().any(|i| {
            matches!(
                i.kind,
                PendingInteractionKind::Permission
                    | PendingInteractionKind::PlanApproval
                    | PendingInteractionKind::AccessRootRequest
                    | PendingInteractionKind::AppConnectionRequest
                    | PendingInteractionKind::ConnectorAuthRequest
            )
        });
        if has_approval {
            WorkRunStatus::WaitingApproval
        } else {
            WorkRunStatus::WaitingInput
        }
    } else {
        raw_status
    };

    // 9. Derive phase
    let phase = if runtime_status == crate::models::RunStatus::Idle && automation_task_id.is_none()
    {
        WorkRunProgressPhase::Idle
    } else if !matches!(
        status,
        WorkRunStatus::Completed
            | WorkRunStatus::Failed
            | WorkRunStatus::Cancelled
            | WorkRunStatus::Skipped
    ) && (!pending_interactions.is_empty()
        || status == WorkRunStatus::WaitingApproval
        || status == WorkRunStatus::WaitingInput)
    {
        WorkRunProgressPhase::WaitingUser
    } else {
        match status {
            WorkRunStatus::Completed => {
                if automation_task_id.is_none() {
                    WorkRunProgressPhase::Idle
                } else {
                    WorkRunProgressPhase::Completed
                }
            }
            WorkRunStatus::Failed => WorkRunProgressPhase::Failed,
            WorkRunStatus::Cancelled | WorkRunStatus::Skipped => WorkRunProgressPhase::Cancelled,
            WorkRunStatus::Queued => WorkRunProgressPhase::Queued,
            WorkRunStatus::Recoverable => WorkRunProgressPhase::Recoverable,
            WorkRunStatus::WaitingDelivery => WorkRunProgressPhase::AwaitingDelivery,
            WorkRunStatus::Running => {
                if has_running_reviewer {
                    WorkRunProgressPhase::Reviewing
                } else if has_running_worker {
                    WorkRunProgressPhase::Implementing
                } else if has_running_researcher {
                    WorkRunProgressPhase::Researching
                } else {
                    WorkRunProgressPhase::Running
                }
            }
            WorkRunStatus::WaitingApproval | WorkRunStatus::WaitingInput => {
                WorkRunProgressPhase::WaitingUser
            }
        }
    };

    // 10. Derive current activity
    let current_activity = if phase == WorkRunProgressPhase::WaitingUser {
        Some(WorkCurrentActivity {
            kind: "waiting_user".to_string(),
            step_id: None,
            tool_name: None,
            agent_role: None,
            detail: Some("等待你的确认".to_string()),
        })
    } else if phase == WorkRunProgressPhase::Recoverable {
        let recovery_detail = latest_guardian_reason
            .clone()
            .unwrap_or_else(|| "执行已中断，等待恢复选择".to_string());
        Some(WorkCurrentActivity {
            kind: "recovery".to_string(),
            step_id: active_step.as_ref().map(|(id, _)| id.clone()),
            tool_name: active_tool.as_ref().map(|(name, _)| name.clone()),
            agent_role: None,
            detail: Some(recovery_detail),
        })
    } else if phase == WorkRunProgressPhase::AwaitingDelivery {
        Some(WorkCurrentActivity {
            kind: "artifact_delivery".to_string(),
            step_id: active_step.as_ref().map(|(id, _)| id.clone()),
            tool_name: None,
            agent_role: None,
            detail: Some("等待必需交付物验收".to_string()),
        })
    } else if phase == WorkRunProgressPhase::Idle || !status.is_active() {
        None
    } else if has_running_reviewer {
        Some(WorkCurrentActivity {
            kind: "reviewing".to_string(),
            step_id: None,
            tool_name: None,
            agent_role: Some("reviewer".to_string()),
            detail: Some("正在独立审查".to_string()),
        })
    } else if has_running_worker {
        Some(WorkCurrentActivity {
            kind: "implementing".to_string(),
            step_id: None,
            tool_name: None,
            agent_role: Some("worker".to_string()),
            detail: Some("正在实现修改".to_string()),
        })
    } else if has_running_researcher {
        let detail = if total_researchers > 1 {
            format!(
                "正在并行调查 · {}/{} 已完成",
                completed_researchers, total_researchers
            )
        } else {
            "正在并行调查".to_string()
        };
        Some(WorkCurrentActivity {
            kind: "researching".to_string(),
            step_id: None,
            tool_name: None,
            agent_role: Some("researcher".to_string()),
            detail: Some(detail),
        })
    } else if let Some((_call_id, tool_name)) = active_tool {
        let detail = format!("正在{}", friendly_tool_label(&tool_name));
        Some(WorkCurrentActivity {
            kind: "tool".to_string(),
            step_id: None,
            tool_name: Some(tool_name),
            agent_role: None,
            detail: Some(detail),
        })
    } else if let Some((step_id, step_title)) = active_step {
        Some(WorkCurrentActivity {
            kind: "step".to_string(),
            step_id: Some(step_id),
            tool_name: None,
            agent_role: None,
            detail: Some(format!("正在执行：{step_title}")),
        })
    } else {
        Some(WorkCurrentActivity {
            kind: "running".to_string(),
            step_id: None,
            tool_name: None,
            agent_role: None,
            detail: Some("正在处理任务".to_string()),
        })
    };

    // 11. Derive attention
    let attention = if status.is_active() && !pending_interactions.is_empty() {
        let first = &pending_interactions[0];
        let kind = match &first.kind {
            PendingInteractionKind::Permission => "permission",
            PendingInteractionKind::UserInput => "user_input",
            PendingInteractionKind::PlanApproval => "plan_approval",
            PendingInteractionKind::ArtifactValidation => "artifact_validation",
            PendingInteractionKind::AccessRootRequest => "access_root_request",
            PendingInteractionKind::AppConnectionRequest => "app_connection_request",
            PendingInteractionKind::ConnectorAuthRequest => "connector_auth_request",
            PendingInteractionKind::Custom(s) => s.as_str(),
        };
        Some(WorkProgressAttention {
            kind: kind.to_string(),
            count: pending_interactions.len(),
        })
    } else if status == WorkRunStatus::WaitingApproval {
        Some(WorkProgressAttention {
            kind: "plan_approval".to_string(),
            count: 1,
        })
    } else if status == WorkRunStatus::WaitingInput {
        Some(WorkProgressAttention {
            kind: "user_input".to_string(),
            count: 1,
        })
    } else if status == WorkRunStatus::Recoverable {
        Some(WorkProgressAttention {
            kind: "recovery".to_string(),
            count: 1,
        })
    } else if status == WorkRunStatus::WaitingDelivery {
        Some(WorkProgressAttention {
            kind: "artifact_delivery".to_string(),
            count: 1,
        })
    } else if let Some(ref h) = health_view {
        if h.status == RunHealth::Stalled
            || h.status == RunHealth::Degraded
            || h.status == RunHealth::NeedsAttention
        {
            Some(WorkProgressAttention {
                kind: "guardian_anomaly".to_string(),
                count: h.warning_count.max(1) as usize,
            })
        } else {
            None
        }
    } else {
        None
    };

    // 12. Derive available actions
    let is_terminal = if automation_task_id.is_some() {
        matches!(
            status,
            WorkRunStatus::Completed
                | WorkRunStatus::Failed
                | WorkRunStatus::Cancelled
                | WorkRunStatus::Skipped
        )
    } else {
        matches!(
            runtime_status,
            crate::models::RunStatus::Failed | crate::models::RunStatus::Stopped
        )
    };
    let can_send = phase == WorkRunProgressPhase::Idle
        || is_terminal
        || phase == WorkRunProgressPhase::WaitingUser;
    let can_stop = status == WorkRunStatus::Running && phase != WorkRunProgressPhase::Idle;
    let can_resume = if automation_task_id.is_some() {
        is_terminal || phase == WorkRunProgressPhase::WaitingUser
    } else {
        is_terminal && runtime_status != crate::models::RunStatus::Idle
    };
    let can_recover =
        status == WorkRunStatus::Recoverable || phase == WorkRunProgressPhase::Recoverable;

    let session_id = meta
        .as_ref()
        .and_then(|m| m.session_id.clone())
        .or_else(|| found_work_run.as_ref().and_then(|r| r.session_id.clone()));

    let task_id = automation_task_id.clone().unwrap_or_default();
    let work_run_id = found_work_run
        .as_ref()
        .map(|r| r.id.clone())
        .or_else(|| meta.as_ref().and_then(|m| m.work_run_id.clone()))
        .unwrap_or_else(|| run_id.to_string());

    Ok(WorkProjection {
        run_id: run_id.to_string(),
        workspace_id,
        automation_task_id: automation_task_id.clone(),
        task_id,
        work_run_id,
        session_id,
        runtime_status,
        automation_status,
        run_status: status,
        status,
        phase,
        goal,
        goal_spec,
        plan: plan.clone(),
        steps: plan,
        checkpoint,
        current_activity,
        attention,
        health: health_view,
        agents,
        tool_summary,
        artifacts,
        pending_interactions,
        available_actions: WorkAvailableActions {
            can_send,
            can_stop,
            can_resume,
            can_recover,
        },
        updated_at: Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RunMeta, RunStatus, StructuredTask};
    use crate::work::models::{AppMode, WorkTaskCheckpoint, WorkTaskState};
    use tempfile::TempDir;

    static ENV_LOCK: once_cell::sync::Lazy<std::sync::Mutex<()>> =
        once_cell::sync::Lazy::new(|| std::sync::Mutex::new(()));

    fn setup_env() -> (std::sync::MutexGuard<'static, ()>, TempDir, WorkPaths) {
        let guard = ENV_LOCK.lock().unwrap();
        let temp = TempDir::new().unwrap();
        std::env::set_var("AGENTCABIN_DATA_DIR", temp.path());
        crate::storage::runs::invalidate_runs_cache();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        (guard, temp, paths)
    }

    fn make_test_meta(id: &str, prompt: &str, status: RunStatus) -> RunMeta {
        RunMeta {
            id: id.to_string(),
            prompt: prompt.to_string(),
            cwd: "/tmp".to_string(),
            agent: "pi".to_string(),
            app_mode: AppMode::Work,
            agent_target: None,
            workspace_id: Some("ws-test".to_string()),
            work_task_id: None,
            work_run_id: Some(id.to_string()),
            work_execution_context: None,
            work_preset: None,
            auth_mode: "default".to_string(),
            status,
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            exit_code: None,
            error_message: None,
            session_id: Some(id.to_string()),
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
        }
    }

    #[test]
    fn test_project_interactive_run_projection() {
        let (_guard, _temp, paths) = setup_env();
        let run_id = "run-interactive-1";
        let meta = make_test_meta(run_id, "测试工作区交互式对话", RunStatus::Running);
        crate::storage::runs::save_meta(&meta).unwrap();

        let projection = project_work_projection(&paths, run_id).unwrap();
        assert_eq!(projection.run_id, run_id);
        assert_eq!(projection.workspace_id, Some("ws-test".to_string()));
        assert_eq!(projection.automation_task_id, None);
        assert_eq!(projection.status, WorkRunStatus::Running);
        assert_eq!(projection.phase, WorkRunProgressPhase::Running);
        assert_eq!(projection.goal, Some("测试工作区交互式对话".to_string()));
        assert!(projection.plan.is_empty());
        assert!(projection.available_actions.can_stop);
    }

    #[test]
    fn test_interactive_settled_turn_yields_idle() {
        let (_guard, _temp, paths) = setup_env();
        let run_id = "run-interactive-settled";
        let mut meta = make_test_meta(run_id, "问一个问题", RunStatus::Idle);
        meta.ended_at = Some(Utc::now().to_rfc3339());
        meta.exit_code = Some(0);
        crate::storage::runs::save_meta(&meta).unwrap();

        let projection = project_work_projection(&paths, run_id).unwrap();
        // Interactive runs that completed a turn are Idle waiting for next user input
        assert_eq!(projection.phase, WorkRunProgressPhase::Idle);
        assert_eq!(projection.runtime_status, RunStatus::Idle);
        assert_eq!(projection.automation_status, None);
        assert!(projection.available_actions.can_send);
        assert!(!projection.available_actions.can_stop);
        assert!(!projection.available_actions.can_resume);
    }

    #[test]
    fn test_task_state_update_in_projection() {
        let (_guard, _temp, paths) = setup_env();
        let run_id = "run-task-state-projection";
        let meta = make_test_meta(run_id, "初始目标", RunStatus::Running);
        crate::storage::runs::save_meta(&meta).unwrap();

        // Save durable task state
        let state = WorkTaskState {
            version: 1,
            revision: 3,
            goal: Some("更新后的高阶目标".to_string()),
            goal_spec: None,
            plan: vec![
                StructuredTask {
                    id: "step-1".to_string(),
                    text: "编写代码".to_string(),
                    status: StructuredTaskStatus::Completed,
                },
                StructuredTask {
                    id: "step-2".to_string(),
                    text: "运行测试".to_string(),
                    status: StructuredTaskStatus::InProgress,
                },
            ],
            checkpoint: Some(WorkTaskCheckpoint {
                summary: "代码编写完毕，进入测试阶段".to_string(),
                current_step_id: Some("step-2".to_string()),
                created_at: Utc::now().to_rfc3339(),
            }),
            pending_approval: None,
            updated_at: Utc::now().to_rfc3339(),
        };
        crate::work::task_state::save(run_id, &state).unwrap();

        let projection = project_work_projection(&paths, run_id).unwrap();
        assert_eq!(projection.goal, Some("更新后的高阶目标".to_string()));
        assert_eq!(projection.plan.len(), 2);
        assert_eq!(projection.plan[1].status, StructuredTaskStatus::InProgress);
        assert_eq!(
            projection.checkpoint.as_ref().unwrap().summary,
            "代码编写完毕，进入测试阶段"
        );
        let cur = projection.current_activity.as_ref().unwrap();
        assert_eq!(cur.kind, "step");
        assert_eq!(cur.step_id.as_deref(), Some("step-2"));

        // Check backwards-compatibility conversion
        let progress_view = projection.to_progress_view();
        assert_eq!(progress_view.steps.len(), 2);
        assert_eq!(progress_view.goal, Some("更新后的高阶目标".to_string()));
    }

    #[test]
    fn test_project_work_run_with_distinct_identities() {
        let (_guard, _temp, paths) = setup_env();
        let tm = TaskManager::new(paths.clone());
        let task = tm
            .create_task("ws-test", "自动化测试任务", "instructions", None)
            .unwrap();
        let work_run = tm
            .start_run_internal(
                &task.id,
                None,
                crate::work::models::WorkRunTrigger::Manual,
                None,
                crate::work::models::ExecutionContext::Attended,
            )
            .unwrap();

        // Simulate session run where session_id / run.id is different from work_run_id
        let session_run_id = "session-xyz-123";
        let mut meta = make_test_meta(session_run_id, "运行任务", RunStatus::Running);
        meta.work_task_id = Some(task.id.clone());
        meta.work_run_id = Some(work_run.id.clone());
        crate::storage::runs::save_meta(&meta).unwrap();

        let projection = project_work_projection(&paths, session_run_id).unwrap();
        assert_eq!(projection.run_id, session_run_id);
        assert_eq!(projection.task_id, task.id);
        assert_eq!(projection.work_run_id, work_run.id);
        assert_eq!(projection.automation_task_id, Some(task.id.clone()));
        // Distinct identities: task_id != work_run_id != run_id
        assert_ne!(projection.task_id, projection.work_run_id);
        assert_ne!(projection.work_run_id, session_run_id);
    }

    #[test]
    fn test_project_work_run_recover_task_id_from_manifest() {
        let (_guard, _temp, paths) = setup_env();
        let tm = TaskManager::new(paths.clone());
        let task = tm
            .create_task("ws-test", "Manifest Recover Task", "instructions", None)
            .unwrap();
        let work_run = tm
            .start_run_internal(
                &task.id,
                None,
                crate::work::models::WorkRunTrigger::Manual,
                None,
                crate::work::models::ExecutionContext::Attended,
            )
            .unwrap();

        // Meta exists but work_task_id is missing (e.g. legacy or partially hydrated run)
        let mut meta = make_test_meta(&work_run.id, "运行任务", RunStatus::Running);
        meta.work_task_id = None;
        meta.work_run_id = Some(work_run.id.clone());
        crate::storage::runs::save_meta(&meta).unwrap();

        let projection = project_work_projection(&paths, &work_run.id).unwrap();
        // Recovers taskId from TaskManager manifest
        assert_eq!(projection.task_id, task.id);
        assert_eq!(projection.work_run_id, work_run.id);
        assert_eq!(projection.automation_task_id, Some(task.id));
    }

    #[test]
    fn test_project_interactive_run_empty_task_id_no_forgery() {
        let (_guard, _temp, paths) = setup_env();
        let run_id = "df8edb58-9039-4307-abe6-4b6b9344e8c4";
        let meta = make_test_meta(run_id, "交互式聊天", RunStatus::Running);
        crate::storage::runs::save_meta(&meta).unwrap();

        let projection = project_work_projection(&paths, run_id).unwrap();
        assert_eq!(projection.run_id, run_id);
        // Does NOT forge task_id = run_id
        assert_eq!(projection.task_id, "");
        assert_eq!(projection.automation_task_id, None);
        assert_eq!(projection.work_run_id, run_id);
    }
}
