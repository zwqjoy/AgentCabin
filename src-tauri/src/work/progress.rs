//! Authoritative WorkRun Progress Projection.
//!
//! Projects durable multi-source truth:
//!   WorkRun (authoritative lifecycle)
//!     + latest durable WorkTaskState (goal, plan, checkpoint)
//!     + RuntimeFact Ledger (tool lifecycle, step lifecycle)
//!     + PendingInteraction / Inbox (human attention)
//!     + Host Subagent Registry / facts (multi-agent progress)
//!   -> WorkRunProgressView
//!
//! Progress is a pure read-only projection (no new persistent authority entity).

use std::collections::HashMap;

use chrono::Utc;

use crate::models::StructuredTaskStatus;
use crate::work::interaction::InteractionManager;
use crate::work::ledger::WorkRuntimeLedger;
use crate::work::models::{
    PendingInteractionKind, RunHealth, RuntimeFact, WorkCurrentActivity, WorkProgressAgent,
    WorkProgressAttention, WorkProgressToolSummary, WorkRunHealthView, WorkRunProgressPhase,
    WorkRunProgressView, WorkRunStatus, WorkTaskState,
};
use crate::work::paths::WorkPaths;
use crate::work::tasks::TaskManager;

/// User-friendly label for Work tools.
pub fn friendly_tool_label(tool_name: &str) -> &'static str {
    match tool_name {
        "web_search" => "搜索网页",
        "web_open" => "打开页面",
        "web_extract" => "摘取证据",
        "web_cite" => "生成引用",
        "browser_navigate" => "打开网页",
        "browser_snapshot" => "读取页面",
        "browser_take_screenshot" => "截取页面",
        "browser_wait_for" => "等待页面",
        "browser_tabs" => "管理标签页",
        "browser_close" => "关闭浏览器",
        "browser_click" => "点击页面元素",
        "browser_type" => "填写页面内容",
        "browser_select_option" => "选择页面选项",
        "browser_scroll" => "滚动页面",
        "work_run_command" => "运行命令",
        "work_run_connector_cli" => "运行连接器 CLI",
        "work_execute" => "执行能力",
        "work_read_file" => "读取文件",
        "work_list_files" => "列出文件",
        "work_write_file" => "写入文件",
        "work_edit_file" => "编辑文件",
        "work_update_context" => "更新工作区知识",
        "work_set_goal" => "设定目标",
        "work_replace_plan" => "制定计划",
        "work_update_step" => "更新步骤",
        "work_save_checkpoint" => "保存检查点",
        "work_register_artifact" => "注册成果",
        "work_list_artifacts" => "查看成果",
        "work_validate_artifact" => "验证成果",
        "work_deliver" => "交付成果",
        "work_request_directory_access" => "请求目录访问",
        "work_list_tools" => "查看工具",
        "work_discover_capabilities" => "发现能力",
        "work_activate_tools" => "启用工具",
        "work_workspace_info" => "查看工作区",
        "work_delegate" => "委派助手",
        "work_agent_wait" => "等待助手返回",
        "work_agent_status" => "查看助手状态",
        "work_agent_steer" => "指导助手",
        "work_agent_stop" => "停止助手",
        _ => "执行任务",
    }
}

/// Project the authoritative progress view for a given WorkRun.
pub fn project_run_progress(
    paths: &WorkPaths,
    task_id: &str,
    run_id: &str,
) -> Result<WorkRunProgressView, String> {
    let task_manager = TaskManager::new(paths.clone());
    let task = task_manager.get_task(task_id)?;
    let run = task_manager.get_run(task_id, run_id)?;

    // Fail closed if run does not match task_id
    if run.task_id != task_id {
        return Err(format!(
            "Run '{run_id}' does not belong to task '{task_id}' (actual: '{}')",
            run.task_id
        ));
    }

    // 1. Resolve latest durable WorkTaskState:
    // Check session directory first, then run directory, then fallback to persisted run.task_state.
    let durable_task_state = resolve_latest_task_state(paths, &run);

    let goal = durable_task_state
        .goal
        .clone()
        .or_else(|| Some(task.title.clone()));
    let steps = durable_task_state.plan.clone();
    let checkpoint = durable_task_state.checkpoint.clone();

    // 2. Resolve pending human attention & Guardian anomalies
    let interaction_mgr = InteractionManager::new(paths.clone());
    let pending_interactions = interaction_mgr.list_pending(Some(task_id), Some(run_id))?;

    let ledger = WorkRuntimeLedger::open(paths, task_id, run_id)?;
    let facts = ledger.list_facts().map_err(|error| {
        log::error!(
            "[progress] cannot project task={task_id}, run={run_id} because the runtime ledger is unreadable: {error}"
        );
        format!(
            "WorkRun progress projection blocked by unreadable runtime ledger: {error}"
        )
    })?;

    // Run Guardian evaluation for active runs
    let guardian_config = run.guardian_config.clone().unwrap_or_default();
    let guardian_report = crate::work::guardian::Guardian::evaluate_health(
        &facts,
        run.status,
        &guardian_config,
        &Utc::now().to_rfc3339(),
        None,
    );
    crate::work::guardian::Guardian::persist_health_report(&ledger, &guardian_report, None)
        .map_err(|error| {
            log::error!(
                "[progress] cannot project task={task_id}, run={run_id} because Guardian persistence failed: {error}"
            );
            format!(
                "WorkRun progress projection blocked by Guardian persistence failure: {error}"
            )
        })?;

    let latest_guardian_reason = facts.iter().rev().find_map(|f| match f {
        RuntimeFact::GuardianAnomalyDetected { reason, .. } => Some(reason.clone()),
        _ => None,
    });

    let all_steps_completed = !steps.is_empty()
        && steps
            .iter()
            .all(|s| s.status == crate::models::StructuredTaskStatus::Completed);

    let health_reason = if guardian_report.health != RunHealth::Healthy {
        guardian_report
            .reason
            .or_else(|| latest_guardian_reason.clone())
    } else {
        None
    };

    let health_view = WorkRunHealthView {
        status: guardian_report.health,
        reason: health_reason,
        warning_count: guardian_report.warning_count,
        stalled_since: guardian_report.stalled_since,
        budget: Some(guardian_report.budget_view),
        last_meaningful_progress_at: guardian_report.last_meaningful_progress_at,
        liveness: Some(guardian_report.liveness),
    };

    // Pending Inbox records can outlive the actor by a few milliseconds while
    // Stop/Cancel is being persisted. They remain durable for a later resume,
    // but must not make a terminal WorkRun look as if it still needs action.
    let attention = if run.status.is_active() && !pending_interactions.is_empty() {
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
    } else if run.status == WorkRunStatus::WaitingApproval {
        Some(WorkProgressAttention {
            kind: "plan_approval".to_string(),
            count: 1,
        })
    } else if run.status == WorkRunStatus::WaitingInput {
        Some(WorkProgressAttention {
            kind: "user_input".to_string(),
            count: 1,
        })
    } else if run.status == WorkRunStatus::Recoverable {
        Some(WorkProgressAttention {
            kind: "recovery".to_string(),
            count: 1,
        })
    } else if run.status == WorkRunStatus::WaitingDelivery {
        Some(WorkProgressAttention {
            kind: "artifact_delivery".to_string(),
            count: 1,
        })
    } else if !all_steps_completed
        && (!guardian_report.anomalies.is_empty()
            || guardian_report.health == RunHealth::Stalled
            || guardian_report.health == RunHealth::Degraded
            || guardian_report.health == RunHealth::NeedsAttention)
    {
        Some(WorkProgressAttention {
            kind: "guardian_anomaly".to_string(),
            count: guardian_report.anomalies.len().max(1),
        })
    } else {
        None
    };

    // 3. Resolve subagent states: Ledger facts + in-memory Registry overlay
    let agents = resolve_subagents(paths, task_id, run_id)?;

    // 4. Resolve tool lifecycle & active steps from Runtime Ledger facts
    let LedgerFactsResolution {
        tool_summary,
        active_tool,
        active_step: ledger_active_step,
    } = resolve_ledger_facts(paths, task_id, run_id)?;

    // Check for running subagent roles
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

    // Active step: from ledger fact or first in_progress step in plan
    let active_step = ledger_active_step.or_else(|| {
        steps
            .iter()
            .find(|s| s.status == StructuredTaskStatus::InProgress)
            .map(|s| (s.id.clone(), s.text.clone()))
    });

    // 5. Phase derivation
    let phase = if !matches!(
        run.status,
        WorkRunStatus::Completed
            | WorkRunStatus::Failed
            | WorkRunStatus::Cancelled
            | WorkRunStatus::Skipped
    ) && (!pending_interactions.is_empty()
        || run.status == WorkRunStatus::WaitingApproval
        || run.status == WorkRunStatus::WaitingInput)
    {
        WorkRunProgressPhase::WaitingUser
    } else {
        match run.status {
            WorkRunStatus::Completed => WorkRunProgressPhase::Completed,
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

    // 6. Current activity derivation
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
    } else if !run.status.is_active() {
        // Terminal runs do not have a live current activity
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

    Ok(WorkRunProgressView {
        task_id: task_id.to_string(),
        work_run_id: run_id.to_string(),
        session_id: run.session_id.clone(),
        run_status: run.status,
        phase,
        goal,
        goal_spec: run.task_state.goal_spec.clone(),
        steps,
        checkpoint,
        current_activity,
        attention,
        health: Some(health_view),
        agents,
        tool_summary,
        updated_at: Utc::now().to_rfc3339(),
    })
}

/// Resolve the latest durable WorkTaskState:
/// 1. Try session_id's run directory (where pi_core_extension writes work-task-state.json).
/// 2. Try run_id's run directory.
/// 3. Try task directory state path.
/// 4. Fallback to run.task_state.
pub(crate) fn resolve_latest_task_state(
    paths: &WorkPaths,
    run: &crate::work::models::WorkRun,
) -> WorkTaskState {
    if let Some(ref session_id) = run.session_id {
        if let Ok(Some(state)) = crate::work::task_state::load(session_id) {
            return state;
        }
        let session_state_file = paths
            .tasks_dir()
            .join(&run.task_id)
            .join(session_id)
            .join("work-task-state.json");
        if let Ok(Some(state)) = crate::work::task_state::load_from_path(&session_state_file) {
            return state;
        }
    }

    if let Ok(Some(state)) = crate::work::task_state::load(&run.id) {
        return state;
    }

    let run_state_file = paths
        .tasks_dir()
        .join(&run.task_id)
        .join(&run.id)
        .join("work-task-state.json");
    if let Ok(Some(state)) = crate::work::task_state::load_from_path(&run_state_file) {
        return state;
    }

    run.task_state.clone()
}

/// Resolve subagents by reading durable ledger facts and overlaying in-memory registry.
pub(crate) fn resolve_subagents(
    paths: &WorkPaths,
    task_id: &str,
    run_id: &str,
) -> Result<Vec<WorkProgressAgent>, String> {
    let mut map: HashMap<String, WorkProgressAgent> = HashMap::new();

    // 1. Replay historical Ledger facts
    let ledger = WorkRuntimeLedger::open(paths, task_id, run_id)?;
    let facts = ledger.list_facts()?;
    for fact in facts {
        match fact {
            RuntimeFact::SubagentSpawned {
                agent_id,
                child_index,
                role,
                status,
                ..
            } => {
                map.insert(
                    agent_id.clone(),
                    WorkProgressAgent {
                        agent_id,
                        child_index,
                        role,
                        status,
                        result_summary: None,
                        error: None,
                    },
                );
            }
            RuntimeFact::SubagentCompleted {
                agent_id,
                child_index,
                role,
                status,
                summary,
                ..
            } => {
                map.insert(
                    agent_id.clone(),
                    WorkProgressAgent {
                        agent_id,
                        child_index,
                        role,
                        status,
                        result_summary: summary,
                        error: None,
                    },
                );
            }
            RuntimeFact::SubagentFailed {
                agent_id,
                child_index,
                role,
                status,
                error,
                ..
            } => {
                map.insert(
                    agent_id.clone(),
                    WorkProgressAgent {
                        agent_id,
                        child_index,
                        role,
                        status,
                        result_summary: None,
                        error,
                    },
                );
            }
            RuntimeFact::SubagentStopped {
                agent_id,
                child_index,
                role,
                status,
                reason,
                ..
            } => {
                map.insert(
                    agent_id.clone(),
                    WorkProgressAgent {
                        agent_id,
                        child_index,
                        role,
                        status,
                        result_summary: reason,
                        error: None,
                    },
                );
            }
            RuntimeFact::SubagentInterrupted {
                agent_id,
                child_index,
                role,
                status,
                reason,
                ..
            } => {
                map.insert(
                    agent_id.clone(),
                    WorkProgressAgent {
                        agent_id,
                        child_index,
                        role,
                        status,
                        result_summary: reason,
                        error: None,
                    },
                );
            }
            _ => {}
        }
    }

    // 2. Overlay live in-memory registry (scoped to matching task_id if present)
    let live_records = crate::work::subagents::registry().list_for_scope(run_id);
    for r in live_records {
        if let Some(ref record_task) = r.task_id {
            if record_task != task_id {
                continue;
            }
        }
        map.insert(
            r.agent_id.clone(),
            WorkProgressAgent {
                agent_id: r.agent_id,
                child_index: r.child_index,
                role: r.role,
                status: r.status,
                result_summary: r.result_summary,
                error: r.error,
            },
        );
    }

    let mut list: Vec<WorkProgressAgent> = map.into_values().collect();
    list.sort_by_key(|a| a.child_index);
    Ok(list)
}

pub(crate) struct LedgerFactsResolution {
    pub(crate) tool_summary: WorkProgressToolSummary,
    pub(crate) active_tool: Option<(String, String)>,
    pub(crate) active_step: Option<(String, String)>,
}

/// Resolve tool summary, active tool, and active step from Runtime Ledger facts.
pub(crate) fn resolve_ledger_facts(
    paths: &WorkPaths,
    task_id: &str,
    run_id: &str,
) -> Result<LedgerFactsResolution, String> {
    let ledger = WorkRuntimeLedger::open(paths, task_id, run_id)?;
    let facts = ledger.list_facts()?;

    let mut proposed_count = 0;
    let mut tool_names: HashMap<String, String> = HashMap::new(); // tool_call_id -> tool_name
    let mut started_tools: HashMap<String, (String, usize)> = HashMap::new(); // tool_call_id -> (tool_name, seq)
    let mut tool_seq = 0usize;
    let mut completed_count = 0;
    let mut failed_count = 0;

    let mut active_step: Option<(String, String)> = None;

    for fact in facts {
        match fact {
            RuntimeFact::ToolProposed {
                tool_call_id,
                tool_name,
                ..
            } => {
                proposed_count += 1;
                tool_names.insert(tool_call_id, tool_name);
            }
            RuntimeFact::ToolStarted { tool_call_id, .. } => {
                let name = tool_names
                    .get(&tool_call_id)
                    .cloned()
                    .unwrap_or_else(|| "tool".to_string());
                tool_seq += 1;
                started_tools.insert(tool_call_id, (name, tool_seq));
            }
            RuntimeFact::ToolResult {
                tool_call_id,
                success,
                status,
                ..
            } => {
                started_tools.remove(&tool_call_id);
                if success {
                    completed_count += 1;
                } else if !matches!(status.as_str(), "waiting_approval" | "waiting_input") {
                    failed_count += 1;
                }
            }
            RuntimeFact::StepStarted { step_id, title, .. } => {
                active_step = Some((step_id, title.unwrap_or_default()));
            }
            RuntimeFact::StepEnded { step_id, .. } => {
                if let Some((ref cur_step_id, _)) = active_step {
                    if cur_step_id == &step_id {
                        active_step = None;
                    }
                }
            }
            _ => {}
        }
    }

    let running_count = started_tools.len();
    let started_count = completed_count + failed_count + running_count;
    let active_tool = started_tools
        .into_iter()
        .max_by_key(|(_, (_, seq))| *seq)
        .map(|(tool_call_id, (name, _))| (tool_call_id, name));

    let summary = WorkProgressToolSummary {
        proposed: proposed_count.max(started_count),
        started: started_count,
        completed: completed_count,
        failed: failed_count,
        running: running_count,
    };

    Ok(LedgerFactsResolution {
        tool_summary: summary,
        active_tool,
        active_step,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{StructuredTask, StructuredTaskStatus};
    use crate::work::models::{
        CollaborationMode, ExecutionContext, PendingInteractionKind, SideEffectClass,
        ToolConcurrencyClass, WorkRunStatus, WorkRunTrigger, WorkTaskCheckpoint, WorkTaskState,
    };
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, WorkPaths, String, String) {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();

        let tm = TaskManager::new(paths.clone());
        let task = tm
            .create_task("ws-1", "修复登录偶发失败的问题，并验证", "调查并修复", None)
            .unwrap();
        let run = tm
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        (temp, paths, task.id, run.id)
    }

    // Case 1: Running WorkRun + no plan -> phase running
    #[test]
    fn test_case_1_running_work_run_no_plan() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.run_status, WorkRunStatus::Running);
        assert_eq!(progress.phase, WorkRunProgressPhase::Running);
        assert_eq!(progress.goal, Some("修复登录偶发失败的问题，并验证".into()));
        assert!(progress.steps.is_empty());
        assert_eq!(
            progress.current_activity.unwrap().kind,
            "running".to_string()
        );
    }

    // Case 2: Latest WorkTaskState has goal & in_progress step -> projection returns exact state
    #[test]
    fn test_case_2_latest_task_state_with_goal_and_plan() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let state = WorkTaskState {
            version: 1,
            revision: 2,
            goal: Some("分析登录失败根因".to_string()),
            goal_spec: None,
            plan: vec![
                StructuredTask {
                    id: "step-1".to_string(),
                    text: "复现登录超时".to_string(),
                    status: StructuredTaskStatus::Completed,
                },
                StructuredTask {
                    id: "step-2".to_string(),
                    text: "修复重试逻辑".to_string(),
                    status: StructuredTaskStatus::InProgress,
                },
            ],
            checkpoint: Some(WorkTaskCheckpoint {
                summary: "已定位到超时调用".to_string(),
                current_step_id: Some("step-2".to_string()),
                created_at: "2026-08-20T20:00:00Z".to_string(),
            }),
            pending_approval: None,
            updated_at: "2026-08-20T20:00:00Z".to_string(),
        };

        let state_path = paths
            .tasks_dir()
            .join(&task_id)
            .join(&run_id)
            .join("work-task-state.json");
        crate::work::task_state::save_to_path(&state_path, &state).unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.goal, Some("分析登录失败根因".into()));
        assert_eq!(progress.steps.len(), 2);
        assert_eq!(progress.steps[1].status, StructuredTaskStatus::InProgress);
        assert_eq!(progress.checkpoint.unwrap().summary, "已定位到超时调用");
        let cur = progress.current_activity.unwrap();
        assert_eq!(cur.kind, "step");
        assert_eq!(cur.step_id, Some("step-2".to_string()));
        assert_eq!(cur.detail, Some("正在执行：修复重试逻辑".to_string()));
    }

    // Case 3: Persisted WorkRun.task_state stale, run-local latest task_state newer -> latest durable state wins
    #[test]
    fn test_case_3_latest_durable_state_wins_over_stale_persisted_run() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        // Stale state in run object is empty plan
        let session_id = "session-test-case-3";
        let tm = TaskManager::new(paths.clone());
        tm.attach_session_id(&task_id, &run_id, session_id).unwrap();

        let newer_state = WorkTaskState {
            version: 1,
            revision: 5,
            goal: Some("最新目标".to_string()),
            goal_spec: None,
            plan: vec![StructuredTask {
                id: "s1".to_string(),
                text: "最新步骤".to_string(),
                status: StructuredTaskStatus::InProgress,
            }],
            checkpoint: None,
            pending_approval: None,
            updated_at: "2026-08-20T21:00:00Z".to_string(),
        };
        let session_state_path = paths
            .tasks_dir()
            .join(&task_id)
            .join(session_id)
            .join("work-task-state.json");
        crate::work::task_state::save_to_path(&session_state_path, &newer_state).unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.goal, Some("最新目标".into()));
        assert_eq!(progress.steps.len(), 1);
        assert_eq!(progress.steps[0].text, "最新步骤");
    }

    // Case 4: PendingInteraction exists -> waiting_user
    #[test]
    fn test_case_4_pending_interaction_yields_waiting_user() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let interaction_mgr = InteractionManager::new(paths.clone());
        interaction_mgr
            .create_interaction(
                &task_id,
                &run_id,
                "ws-1",
                None,
                None,
                None,
                PendingInteractionKind::PlanApproval,
                "确认计划",
                "等待用户确认3步计划",
                serde_json::json!({ "proposedPlan": [] }),
            )
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.phase, WorkRunProgressPhase::WaitingUser);
        let att = progress.attention.unwrap();
        assert_eq!(att.kind, "plan_approval");
        assert_eq!(att.count, 1);
        assert_eq!(progress.current_activity.unwrap().kind, "waiting_user");
    }

    // Case 5: ToolStarted but no ToolResult -> current tool
    #[test]
    fn test_case_5_tool_started_no_result_yields_active_tool() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-1".to_string(),
                tool_name: "work_run_command".to_string(),
                action: "execute".to_string(),
                arguments_hash: "hash".to_string(),
                expected_outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-1".to_string(),
                execution_id: "exec-1".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.tool_summary.started, 1);
        assert_eq!(progress.tool_summary.running, 1);
        assert_eq!(progress.tool_summary.completed, 0);

        let cur = progress.current_activity.unwrap();
        assert_eq!(cur.kind, "tool");
        assert_eq!(cur.tool_name, Some("work_run_command".to_string()));
        assert_eq!(cur.detail, Some("正在运行命令".to_string()));
    }

    // Case 6: ToolResult success -> no longer running
    #[test]
    fn test_case_6_tool_result_success_clears_running() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-1".to_string(),
                tool_name: "work_run_command".to_string(),
                action: "execute".to_string(),
                arguments_hash: "hash".to_string(),
                expected_outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-1".to_string(),
                execution_id: "exec-1".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "call-1".to_string(),
                success: true,
                status: "ok".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.tool_summary.running, 0);
        assert_eq!(progress.tool_summary.completed, 1);
        assert_eq!(progress.tool_summary.failed, 0);
        assert_ne!(progress.current_activity.unwrap().kind, "tool");
    }

    // Case 7: ToolResult failed -> failure count correct
    #[test]
    fn test_case_7_tool_result_failed_records_failure_count() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-1".to_string(),
                tool_name: "work_execute".to_string(),
                action: "exec".to_string(),
                arguments_hash: "hash".to_string(),
                expected_outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-1".to_string(),
                execution_id: "exec-1".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "call-1".to_string(),
                success: false,
                status: "error".to_string(),
                failure_kind: None,
                exit_code: Some(1),
                error: Some("Command not found".to_string()),
                outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.tool_summary.running, 0);
        assert_eq!(progress.tool_summary.completed, 0);
        assert_eq!(progress.tool_summary.failed, 1);
    }

    // Case 8: Researcher running -> researching
    #[test]
    fn test_case_8_researcher_running_yields_researching_phase() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();
        ledger
            .record(&RuntimeFact::SubagentSpawned {
                agent_id: "res-1".to_string(),
                provider_run_id: "prun-1".to_string(),
                child_index: 1,
                role: "researcher".to_string(),
                task_digest: "dig".to_string(),
                launch_contract_digest: "lcd".to_string(),
                status: "running".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.phase, WorkRunProgressPhase::Researching);
        let cur = progress.current_activity.unwrap();
        assert_eq!(cur.kind, "researching");
        assert_eq!(cur.agent_role, Some("researcher".to_string()));
        assert_eq!(cur.detail, Some("正在并行调查".to_string()));
    }

    // Case 9: Worker running -> implementing
    #[test]
    fn test_case_9_worker_running_yields_implementing_phase() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();
        ledger
            .record(&RuntimeFact::SubagentSpawned {
                agent_id: "worker-1".to_string(),
                provider_run_id: "prun-2".to_string(),
                child_index: 1,
                role: "worker".to_string(),
                task_digest: "dig".to_string(),
                launch_contract_digest: "lcd".to_string(),
                status: "running".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.phase, WorkRunProgressPhase::Implementing);
        let cur = progress.current_activity.unwrap();
        assert_eq!(cur.kind, "implementing");
        assert_eq!(cur.agent_role, Some("worker".to_string()));
        assert_eq!(cur.detail, Some("正在实现修改".to_string()));
    }

    // Case 10: Reviewer running -> reviewing
    #[test]
    fn test_case_10_reviewer_running_yields_reviewing_phase() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();
        // Worker was completed, now Reviewer is running
        ledger
            .record(&RuntimeFact::SubagentSpawned {
                agent_id: "worker-1".to_string(),
                provider_run_id: "prun-2".to_string(),
                child_index: 1,
                role: "worker".to_string(),
                task_digest: "dig".to_string(),
                launch_contract_digest: "lcd".to_string(),
                status: "running".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::SubagentCompleted {
                agent_id: "worker-1".to_string(),
                provider_run_id: "prun-2".to_string(),
                child_index: 1,
                role: "worker".to_string(),
                status: "completed".to_string(),
                summary: Some("已完成修改".to_string()),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::SubagentSpawned {
                agent_id: "rev-1".to_string(),
                provider_run_id: "prun-3".to_string(),
                child_index: 2,
                role: "reviewer".to_string(),
                task_digest: "dig".to_string(),
                launch_contract_digest: "lcd".to_string(),
                status: "running".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.phase, WorkRunProgressPhase::Reviewing);
        let cur = progress.current_activity.unwrap();
        assert_eq!(cur.kind, "reviewing");
        assert_eq!(cur.agent_role, Some("reviewer".to_string()));
        assert_eq!(cur.detail, Some("正在独立审查".to_string()));
    }

    // Case 11: WorkRun completed -> completed regardless of stale session UI
    #[tokio::test]
    async fn test_case_11_work_run_completed_terminal() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
        controller
            .complete_or_fail_run(&task_id, &run_id, WorkRunStatus::Completed, None, None)
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.run_status, WorkRunStatus::Completed);
        assert_eq!(progress.phase, WorkRunProgressPhase::Completed);
        assert!(progress.current_activity.is_none());
    }

    // Case 12: WorkRun failed -> failed
    #[tokio::test]
    async fn test_case_12_work_run_failed_terminal() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
        controller
            .complete_or_fail_run(
                &task_id,
                &run_id,
                WorkRunStatus::Failed,
                Some("Error in tests".to_string()),
                None,
            )
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.run_status, WorkRunStatus::Failed);
        assert_eq!(progress.phase, WorkRunProgressPhase::Failed);
        assert!(progress.current_activity.is_none());
    }

    // Case 13: WorkRun cancelled -> cancelled
    #[tokio::test]
    async fn test_case_13_work_run_cancelled_terminal() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let interaction_mgr = InteractionManager::new(paths.clone());
        interaction_mgr
            .create_interaction(
                &task_id,
                &run_id,
                "ws-1",
                None,
                None,
                None,
                PendingInteractionKind::UserInput,
                "补充信息",
                "等待用户补充信息",
                serde_json::json!({ "question": "继续吗？" }),
            )
            .unwrap();

        let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
        controller
            .complete_or_fail_run(
                &task_id,
                &run_id,
                WorkRunStatus::Cancelled,
                Some("Cancelled by user".to_string()),
                None,
            )
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.run_status, WorkRunStatus::Cancelled);
        assert_eq!(progress.phase, WorkRunProgressPhase::Cancelled);
        assert!(progress.current_activity.is_none());
        assert!(progress.attention.is_none());
    }

    // Case 14: Wrong taskId + runId -> fail closed
    #[test]
    fn test_case_14_wrong_task_or_run_id_fails_closed() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        assert!(project_run_progress(&paths, "nonexistent-task", &run_id).is_err());
        assert!(project_run_progress(&paths, &task_id, "nonexistent-run").is_err());
    }

    #[test]
    fn malformed_runtime_ledger_fails_progress_projection_closed() {
        let (_temp, paths, task_id, run_id) = setup_test_env();
        let ledger_path = paths.task_run_ledger_path(&task_id, &run_id).unwrap();
        std::fs::write(ledger_path, "{not-json}\n").unwrap();

        let error = project_run_progress(&paths, &task_id, &run_id).unwrap_err();
        assert!(error.contains("Invalid runtime ledger"));
    }

    // Case 15: Restart/re-read same persisted inputs -> equivalent projection
    #[test]
    fn test_case_15_restart_re_read_persisted_inputs_reproduces_projection() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();
        ledger
            .record(&RuntimeFact::RunStarted {
                task_id: task_id.clone(),
                work_run_id: run_id.clone(),
                execution_context: ExecutionContext::Attended,
                collaboration_mode: CollaborationMode::Default,
                timestamp: "2026-08-20T10:00:00Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::SubagentSpawned {
                agent_id: "res-1".to_string(),
                provider_run_id: "prun-1".to_string(),
                child_index: 1,
                role: "researcher".to_string(),
                task_digest: "digest".to_string(),
                launch_contract_digest: "lcd".to_string(),
                status: "completed".to_string(),
                timestamp: "2026-08-20T10:01:00Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::SubagentCompleted {
                agent_id: "res-1".to_string(),
                provider_run_id: "prun-1".to_string(),
                child_index: 1,
                role: "researcher".to_string(),
                status: "completed".to_string(),
                summary: Some("找到了日志".to_string()),
                timestamp: "2026-08-20T10:02:00Z".to_string(),
            })
            .unwrap();

        let proj1 = project_run_progress(&paths, &task_id, &run_id).unwrap();
        let proj2 = project_run_progress(&paths, &task_id, &run_id).unwrap();

        assert_eq!(proj1.task_id, proj2.task_id);
        assert_eq!(proj1.work_run_id, proj2.work_run_id);
        assert_eq!(proj1.run_status, proj2.run_status);
        assert_eq!(proj1.phase, proj2.phase);
        assert_eq!(proj1.agents, proj2.agents);
        assert_eq!(proj1.tool_summary, proj2.tool_summary);
    }

    // Case 16: Out-of-order tool completion retains earlier running tool as active
    #[test]
    fn test_case_16_out_of_order_tool_completion_keeps_earlier_active_tool() {
        let (_temp, paths, task_id, run_id) = setup_test_env();

        let ledger = WorkRuntimeLedger::open(&paths, &task_id, &run_id).unwrap();

        // Tool 1 proposed and started
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-1".to_string(),
                tool_name: "work_long_running_task".to_string(),
                action: "execute".to_string(),
                arguments_hash: "hash1".to_string(),
                expected_outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-1".to_string(),
                execution_id: "exec-1".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        // Tool 2 proposed and started later
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-2".to_string(),
                tool_name: "work_quick_check".to_string(),
                action: "execute".to_string(),
                arguments_hash: "hash2".to_string(),
                expected_outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-2".to_string(),
                execution_id: "exec-2".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        // Tool 2 (newer) completes first
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "call-2".to_string(),
                success: true,
                status: "success".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let progress = project_run_progress(&paths, &task_id, &run_id).unwrap();
        assert_eq!(progress.tool_summary.running, 1);
        assert_eq!(progress.tool_summary.completed, 1);
        // Active tool should be tool 1, not None
        let current = progress.current_activity.expect("current activity exists");
        assert_eq!(current.kind, "tool");
        assert_eq!(current.tool_name.as_deref(), Some("work_long_running_task"));
    }
}
