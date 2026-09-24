//! Authoritative WorkRun Lifecycle & Harness Controller.
//!
//! Owns state transitions for WorkRun and WorkTask. Disallows illegal transitions,
//! coordinates human-in-the-loop resumes, and guarantees artifact completion truth.

use crate::work::artifacts;
use crate::work::interaction::InteractionManager;
use crate::work::models::{
    PendingInteractionKind, RuntimeFact, SideEffectClass, WorkArtifactAcceptance,
    WorkRecoveryAction, WorkRun, WorkRunRecovery, WorkRunStatus, WorkTaskState,
};
use crate::work::paths::WorkPaths;
use crate::work::tasks::TaskManager;

#[derive(Debug, Clone)]
pub struct WorkHarnessController {
    paths: WorkPaths,
    task_manager: TaskManager,
    interaction_manager: InteractionManager,
}

impl WorkHarnessController {
    pub fn new(paths: WorkPaths) -> Self {
        let task_manager = TaskManager::new(paths.clone());
        let interaction_manager = InteractionManager::new(paths.clone());
        Self {
            paths,
            task_manager,
            interaction_manager,
        }
    }

    /// Validate whether a transition from `current` to `target` is legal.
    pub fn validate_transition(
        current: WorkRunStatus,
        target: WorkRunStatus,
    ) -> Result<(), String> {
        if current == target {
            return Ok(());
        }

        // Terminal states cannot transition to ANY other state
        if matches!(
            current,
            WorkRunStatus::Completed
                | WorkRunStatus::Failed
                | WorkRunStatus::Cancelled
                | WorkRunStatus::Skipped
        ) {
            return Err(format!(
                "Invalid WorkRun state transition: terminal state {:?} cannot transition to {:?}",
                current, target
            ));
        }

        Ok(())
    }

    /// Update status of a WorkRun with state transition validation.
    pub fn transition_run_status(
        &self,
        task_id: &str,
        run_id: &str,
        next_status: WorkRunStatus,
    ) -> Result<WorkRun, String> {
        let run = self.task_manager.get_run(task_id, run_id)?;
        Self::validate_transition(run.status, next_status)?;

        // All terminal transitions must use the same finalization path. This
        // keeps artifact acceptance, durable child checks, timestamps, token
        // revocation, and browser cleanup from being bypassed by callers that
        // only need to update a status.
        if !next_status.is_active() {
            return self.complete_or_fail_run(task_id, run_id, next_status, None, None);
        }

        self.task_manager
            .update_run_status(task_id, run_id, next_status)
    }

    /// Complete or fail a WorkRun, checking Artifact Completion Truth.
    pub fn complete_or_fail_run(
        &self,
        task_id: &str,
        run_id: &str,
        requested_final_status: WorkRunStatus,
        error_message: Option<String>,
        mut final_state: Option<WorkTaskState>,
    ) -> Result<WorkRun, String> {
        let run = self.task_manager.get_run(task_id, run_id)?;
        Self::validate_transition(run.status, requested_final_status)?;

        // Repeated finalization is idempotent. In particular, a later progress
        // poll must not downgrade a previously terminal run to
        // WaitingDelivery after a user edits or removes an output file.
        if !requested_final_status.is_active() && run.status == requested_final_status {
            return Ok(run);
        }

        let task = self.task_manager.get_task(task_id)?;
        let mut effective_status = requested_final_status;
        let mut effective_error = error_message;

        if requested_final_status == WorkRunStatus::Completed {
            // Check 1: No unresolved pending interactions
            let pending = self
                .interaction_manager
                .list_pending(Some(task_id), Some(run_id))?;
            if !pending.is_empty() {
                let target_status = if pending.iter().any(|interaction| {
                    matches!(
                        interaction.kind,
                        PendingInteractionKind::UserInput
                            | PendingInteractionKind::AppConnectionRequest
                            | PendingInteractionKind::ConnectorAuthRequest
                    )
                }) {
                    WorkRunStatus::WaitingInput
                } else {
                    WorkRunStatus::WaitingApproval
                };
                return self.task_manager.set_run_state(
                    task_id,
                    run_id,
                    target_status,
                    Some(format!(
                        "仍有 {} 个待处理交互，Run 暂不能完成",
                        pending.len()
                    )),
                    final_state,
                );
            }

            // A terminal Session signal is not enough when the Work Ledger
            // still contains a ToolProposed without its ToolResult. This can
            // happen when the process dies between dispatch and persistence;
            // keep the WorkRun recoverable instead of allowing a direct
            // completion call to skip the recovery decision.
            if let Some(recovery) =
                get_run_recovery(&self.paths, &task.workspace_id, task_id, run_id)?
            {
                if let Some(tool_call_id) = recovery.tool_call_id.as_deref() {
                    let side_effect = match recovery.side_effect_class.as_deref() {
                        Some("local_verifiable") => SideEffectClass::LocalVerifiable,
                        Some("external_mutating") => SideEffectClass::ExternalMutating,
                        _ => SideEffectClass::Read,
                    };
                    let recovery_reason = format!(
                        "工具调用 {} 尚未记录结果；完成前必须先完成恢复确认",
                        tool_call_id
                    );
                    let ledger =
                        crate::work::ledger::WorkRuntimeLedger::open(&self.paths, task_id, run_id)?;
                    ledger.record_recovery_event_once(
                        "recovery_detected",
                        run_id,
                        Some(tool_call_id),
                        if side_effect == SideEffectClass::ExternalMutating {
                            "external_unknown_outcome"
                        } else {
                            "completion_gate"
                        },
                        side_effect,
                        recovery.reused_existing_output,
                        None,
                        Some(recovery_reason.clone()),
                    )?;

                    if side_effect == SideEffectClass::ExternalMutating {
                        if recovery.pending_interaction_id.is_none() {
                            let payload = serde_json::json!({
                                "recoveryKey": format!("{}:{}", run_id, tool_call_id),
                                "recoveryAction": "external_unknown_outcome",
                                "originalToolCallId": tool_call_id,
                                "sideEffectClass": "external_mutating",
                                "expectedOutputs": recovery.expected_outputs,
                            });
                            if let Err(error) =
                                self.interaction_manager.create_recovery_interaction(
                                    task_id,
                                    run_id,
                                    &task.workspace_id,
                                    run.session_id.as_deref(),
                                    None,
                                    Some(tool_call_id),
                                    PendingInteractionKind::Permission,
                                    "恢复确认：外部操作结果未知",
                                    &recovery_reason,
                                    payload,
                                )
                            {
                                return self.task_manager.set_run_state(
                                    task_id,
                                    run_id,
                                    WorkRunStatus::Recoverable,
                                    Some(format!(
                                        "{recovery_reason}；恢复 Inbox 创建失败：{error}"
                                    )),
                                    final_state,
                                );
                            }
                        }
                        return self.task_manager.set_run_state(
                            task_id,
                            run_id,
                            WorkRunStatus::WaitingApproval,
                            Some(recovery_reason),
                            final_state,
                        );
                    }

                    return self.task_manager.set_run_state(
                        task_id,
                        run_id,
                        WorkRunStatus::Recoverable,
                        Some(recovery_reason),
                        final_state,
                    );
                }
            }

            // Check 2: current-run artifact acceptance. Missing/invalid output
            // is a recoverable delivery state, never an execution failure.
            let acceptance = artifacts::check_required_artifacts_with_paths(
                &self.paths,
                &task.workspace_id,
                run_id,
                &task,
            );
            if let Ok(report) = acceptance {
                if !report.satisfied {
                    effective_status = WorkRunStatus::WaitingDelivery;
                    effective_error = Some(format_artifact_acceptance_error(&report));
                }
            } else if let Err(error) = acceptance {
                effective_status = WorkRunStatus::WaitingDelivery;
                effective_error = Some(format!("等待必需交付物验收：{error}"));
            }

            // Check 3: Goal acceptance contract & auto-repair gate. A goal is
            // only an explicit artifact-acceptance contract; ordinary chat
            // runs skip generation, verification, and repair entirely.
            let has_strict_artifacts =
                !task.required_artifacts.is_empty() || !task.artifact_requirements.is_empty();
            if effective_status == requested_final_status && has_strict_artifacts {
                if let Some(existing_goal) = run
                    .task_state
                    .goal_spec
                    .clone()
                    .or_else(|| final_state.as_ref().and_then(|s| s.goal_spec.clone()))
                    .or_else(|| {
                        Some(crate::work::goal::GoalBuilder::build(
                            &task.workspace_id,
                            run_id,
                            &task.title,
                            Some(&task),
                        ))
                    })
                {
                    let verifier = crate::work::goal::GoalVerifier::new(
                        &self.paths,
                        &task.workspace_id,
                        task_id,
                        run_id,
                    );
                    let verified_goal = verifier.verify(existing_goal);
                    if verified_goal.status != crate::work::models::GoalStatus::Passed {
                        let (repaired_goal, can_continue) =
                            crate::work::goal::GoalRepairLoop::advance_repair(verified_goal);
                        let mut next_task_state =
                            final_state.unwrap_or_else(|| run.task_state.clone());
                        next_task_state.goal_spec = Some(repaired_goal.clone());

                        if can_continue {
                            effective_status = WorkRunStatus::WaitingDelivery;
                            effective_error =
                                repaired_goal.repair_instruction.clone().or_else(|| {
                                    Some(format!(
                                        "目标尚未完全达成（第 {} 轮自动修复）",
                                        repaired_goal.repair_round
                                    ))
                                });
                            return self.task_manager.set_run_state(
                                task_id,
                                run_id,
                                effective_status,
                                effective_error,
                                Some(next_task_state),
                            );
                        } else {
                            effective_status = WorkRunStatus::WaitingInput;
                            let err_msg =
                                repaired_goal.repair_instruction.clone().unwrap_or_else(|| {
                                    "已达最大自动修复轮次，仍有未达成目标，已转入待办。".to_string()
                                });
                            effective_error = Some(err_msg.clone());

                            let payload = serde_json::json!({
                                "goalId": repaired_goal.goal_id,
                                "repairRound": repaired_goal.repair_round,
                                "maxRepairRounds": repaired_goal.max_repair_rounds,
                                "statement": repaired_goal.statement,
                                "unpassedCriteria": repaired_goal.criteria.iter().filter(|c| c.status != crate::work::models::CriterionStatus::Passed).collect::<Vec<_>>(),
                            });
                            let _ = self.interaction_manager.create_interaction(
                                task_id,
                                run_id,
                                &task.workspace_id,
                                run.session_id.as_deref(),
                                None,
                                None,
                                crate::work::models::PendingInteractionKind::UserInput,
                                "目标验收未通过（需人工确认）",
                                &err_msg,
                                payload,
                            );

                            return self.task_manager.set_run_state(
                                task_id,
                                run_id,
                                effective_status,
                                effective_error,
                                Some(next_task_state),
                            );
                        }
                    } else {
                        let mut next_task_state =
                            final_state.unwrap_or_else(|| run.task_state.clone());
                        next_task_state.goal_spec = Some(verified_goal);
                        final_state = Some(next_task_state);
                    }
                }
            }
        }

        if effective_status.is_active() {
            return self.task_manager.set_run_state(
                task_id,
                run_id,
                effective_status,
                effective_error,
                final_state,
            );
        }

        if !effective_status.is_active() {
            // A WorkRun terminal state closes the current task turn, not
            // necessarily the interactive Pi process. The process may remain
            // idle so the user can send a follow-up, and its bridge lease must
            // stay valid until that actor actually exits. Actor cleanup owns
            // exact-token revocation; stop/replacement has a timeout fallback.
            let run_id_owned = run_id.to_string();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    if let Err(error) = crate::work::browser_operator::browser_operator_manager()
                        .close_context(&run_id_owned)
                        .await
                    {
                        log::warn!(
                            "[work/lifecycle] Failed to close browser context for WorkRun {}: {}",
                            run_id_owned,
                            error
                        );
                    }
                    crate::work::desktop_operator::desktop_operator_manager()
                        .release_for_run(&run_id_owned)
                        .await;
                });
            }
        }

        let failure_reason = if effective_status == WorkRunStatus::Failed {
            Some(
                effective_error
                    .clone()
                    .unwrap_or_else(|| "自动化运行失败".to_string()),
            )
        } else {
            None
        };
        let finished_run = self.task_manager.finish_run(
            task_id,
            run_id,
            effective_status,
            effective_error,
            final_state,
        )?;

        // A failed automation is a product-level decision point, not a
        // transient log line. Keep it in the same durable Inbox used by the
        // rest of Work, but mark it as a run failure so resolution never
        // grants or replays the old tool call.
        if let Some(reason) = failure_reason {
            self.interaction_manager.create_recovery_interaction(
                task_id,
                run_id,
                &task.workspace_id,
                run.session_id.as_deref(),
                None,
                None,
                PendingInteractionKind::Custom("automation_failure".to_string()),
                &format!("自动化运行失败：{}", task.title),
                &reason,
                serde_json::json!({
                    "recoveryKey": format!("run-failure:{task_id}:{run_id}"),
                    "recoveryAction": "run_failure",
                    "failureKind": "automation_failure",
                    "failureReason": reason,
                    "availableActions": ["retry", "cancel"],
                    "retryAvailable": true,
                }),
            )?;
        }

        Ok(finished_run)
    }

    /// Reconcile active runs on application startup with effect-aware recovery.
    pub fn reconcile_on_restart(&self) -> Result<(), String> {
        let tasks = self.task_manager.list_tasks(None)?;
        for task in tasks {
            let runs = self.task_manager.list_runs(&task.id)?;
            for run in runs {
                if !run.status.is_active() {
                    continue;
                }

                // Inspect ledger facts for effect-aware recovery
                let mut needs_attention = false;
                let ledger =
                    crate::work::ledger::WorkRuntimeLedger::open(&self.paths, &task.id, &run.id)?;
                {
                    let facts = ledger.list_facts()?;
                    let mut uncompleted_calls = std::collections::HashMap::new();
                    for fact in &facts {
                        match fact {
                            crate::work::models::RuntimeFact::ToolProposed {
                                tool_call_id,
                                side_effect_class,
                                expected_outputs,
                                ..
                            } => {
                                uncompleted_calls.insert(
                                    tool_call_id.clone(),
                                    (*side_effect_class, expected_outputs.clone()),
                                );
                            }
                            crate::work::models::RuntimeFact::ToolResult {
                                tool_call_id, ..
                            } => {
                                uncompleted_calls.remove(tool_call_id);
                            }
                            _ => {}
                        }
                    }
                    for (call_id, (side_effect, expected_outputs)) in uncompleted_calls {
                        let recovery = ledger.classify_recovery(
                            &call_id,
                            side_effect,
                            &expected_outputs,
                            self.paths.workspace_dir(&task.workspace_id).ok().as_deref(),
                        );
                        match recovery {
                            crate::work::ledger::RecoveryAction::UnknownOutcomeNeedsAttention {
                                reason,
                            } => {
                                needs_attention = true;
                                ledger.record_recovery_event_once(
                                    "recovery_detected",
                                    &run.id,
                                    Some(&call_id),
                                    "external_unknown_outcome",
                                    side_effect,
                                    false,
                                    None,
                                    Some(reason.clone()),
                                )?;
                                self.task_manager.set_run_state(
                                    &task.id,
                                    &run.id,
                                    WorkRunStatus::WaitingApproval,
                                    Some(reason.clone()),
                                    None,
                                )?;
                                self.interaction_manager.create_recovery_interaction(
                                            &task.id,
                                            &run.id,
                                            &task.workspace_id,
                                            run.session_id.as_deref(),
                                            None,
                                            Some(&call_id),
                                            crate::work::models::PendingInteractionKind::Permission,
                                            "Restart Recovery: External Mutation",
                                            &reason,
                                            serde_json::json!({
                                                "recoveryKey": format!("{}:{}", run.id, call_id),
                                                "recoveryAction": "external_unknown_outcome",
                                                "originalToolCallId": call_id,
                                                "sideEffectClass": serde_json::to_value(side_effect).unwrap_or_default(),
                                                "expectedOutputs": expected_outputs,
                                            }),
                                        )?;
                                break;
                            }
                            crate::work::ledger::RecoveryAction::AlreadyCompleted => {
                                ledger.record_recovery_event_once(
                                    "recovery_detected",
                                    &run.id,
                                    Some(&call_id),
                                    "already_completed",
                                    side_effect,
                                    false,
                                    Some("已有持久化 ToolResult".to_string()),
                                    None,
                                )?;
                            }
                            crate::work::ledger::RecoveryAction::VerifiedLocalOutputExists => {
                                match resolve_recovered_local_outputs(
                                    &self.paths,
                                    &task.workspace_id,
                                    &expected_outputs,
                                ) {
                                    Ok(outputs) => {
                                        let mut registration_error = None;
                                        for relative in &outputs {
                                            let path = std::path::Path::new(relative);
                                            let title = path
                                                .file_name()
                                                .and_then(|name| name.to_str())
                                                .unwrap_or("Recovered Artifact");
                                            let artifact_type = path
                                                .extension()
                                                .and_then(|extension| extension.to_str());
                                            if let Err(error) = artifacts::register_with_paths(
                                                &self.paths,
                                                &task.workspace_id,
                                                relative,
                                                title,
                                                artifact_type,
                                                Some(&run.id),
                                            ) {
                                                registration_error = Some(error);
                                                break;
                                            }
                                        }
                                        if let Some(error) = registration_error {
                                            needs_attention = true;
                                            self.task_manager.set_run_state(
                                                &task.id,
                                                &run.id,
                                                WorkRunStatus::Recoverable,
                                                Some(format!(
                                                    "本地输出存在但登记失败，请重新验证：{error}"
                                                )),
                                                None,
                                            )?;
                                        } else if let Err(error) = ledger.record(
                                            &crate::work::models::RuntimeFact::ToolResult {
                                                tool_call_id: call_id.clone(),
                                                success: true,
                                                status: "recovered".to_string(),
                                                failure_kind: None,
                                                exit_code: Some(0),
                                                error: None,
                                                outputs: outputs.clone(),
                                                side_effect_class: side_effect,
                                                timestamp: crate::models::now_iso(),
                                            },
                                        ) {
                                            needs_attention = true;
                                            self.task_manager.set_run_state(
                                                &task.id,
                                                &run.id,
                                                WorkRunStatus::Recoverable,
                                                Some(format!(
                                                    "本地输出已验证，但恢复记录写入失败：{error}"
                                                )),
                                                None,
                                            )?;
                                        } else {
                                            ledger.record_recovery_event_once(
                                                "recovery_resolved",
                                                &run.id,
                                                Some(&call_id),
                                                "reused_local_output",
                                                side_effect,
                                                true,
                                                Some("复用了已验证的本地输出".to_string()),
                                                None,
                                            )?;
                                            needs_attention = true;
                                            self.task_manager.set_run_state(
                                                        &task.id,
                                                        &run.id,
                                                        WorkRunStatus::Recoverable,
                                                        Some("已复用通过验证的本地输出；请继续后续步骤或验证 Run".to_string()),
                                                        None,
                                                    )?;
                                        }
                                    }
                                    Err(error) => {
                                        needs_attention = true;
                                        self.task_manager.set_run_state(
                                                    &task.id,
                                                    &run.id,
                                                    WorkRunStatus::Recoverable,
                                                    Some(format!(
                                                        "本地输出无法安全复用，请验证文件或重试该步骤：{error}"
                                                    )),
                                                    None,
                                                )?;
                                    }
                                }
                            }
                            crate::work::ledger::RecoveryAction::SafeToReplay => {
                                needs_attention = true;
                                ledger.record_recovery_event_once(
                                    "recovery_detected",
                                    &run.id,
                                    Some(&call_id),
                                    "safe_to_replay",
                                    side_effect,
                                    false,
                                    None,
                                    Some("重启前调用未完成；需要明确选择继续或重试".to_string()),
                                )?;
                                self.task_manager.set_run_state(
                                    &task.id,
                                    &run.id,
                                    WorkRunStatus::Recoverable,
                                    Some(format!(
                                        "重启时步骤 {} 尚未完成；可安全重试，但不会自动重放",
                                        call_id
                                    )),
                                    None,
                                )?;
                            }
                        }
                    }

                    // 2b. Guardian anomaly evaluation during startup reconciliation
                    let guardian_config = run.guardian_config.clone().unwrap_or_default();
                    let report = crate::work::guardian::Guardian::evaluate_health(
                        &facts,
                        run.status,
                        &guardian_config,
                        &crate::models::now_iso(),
                        None,
                    );
                    if let Err(error) = crate::work::guardian::Guardian::persist_health_report(
                        &ledger, &report, None,
                    ) {
                        let reason =
                            format!("Guardian report persistence failed during restart: {error}");
                        log::error!("[lifecycle] {reason}");
                        if (run.status == WorkRunStatus::Running
                            || run.status == WorkRunStatus::Queued)
                            && !needs_attention
                        {
                            self.task_manager.set_run_state(
                                &task.id,
                                &run.id,
                                WorkRunStatus::Recoverable,
                                Some(reason.clone()),
                                None,
                            )?;
                        }
                        return Err(reason);
                    }
                    for anomaly in &report.anomalies {
                        if anomaly.is_critical
                            && (run.status == WorkRunStatus::Running
                                || run.status == WorkRunStatus::Queued)
                            && !needs_attention
                        {
                            self.task_manager.set_run_state(
                                &task.id,
                                &run.id,
                                WorkRunStatus::Recoverable,
                                Some(anomaly.reason.clone()),
                                None,
                            )?;
                            needs_attention = true;
                        }
                    }
                }

                let pending_interactions = self
                    .interaction_manager
                    .list_pending(Some(&task.id), Some(&run.id))?;

                if needs_attention {
                    // A durable approval/input that already existed before
                    // restart remains the stronger user-facing state. A
                    // safe-but-unfinished call without such an interaction
                    // stays Recoverable and requires an explicit action.
                    if !pending_interactions.is_empty() || run.task_state.pending_approval.is_some()
                    {
                        let target_status = if pending_interactions.iter().any(|interaction| {
                                matches!(
                                    interaction.kind,
                                    crate::work::models::PendingInteractionKind::UserInput
                                        | crate::work::models::PendingInteractionKind::AppConnectionRequest
                                        | crate::work::models::PendingInteractionKind::ConnectorAuthRequest
                                )
                            }) {
                                WorkRunStatus::WaitingInput
                            } else {
                                WorkRunStatus::WaitingApproval
                            };
                        self.task_manager.set_run_state(
                            &task.id,
                            &run.id,
                            target_status,
                            None,
                            None,
                        )?;
                    }
                    continue;
                }

                // Recovery and delivery states are durable user decisions,
                // not orphaned executions. Keep them visible across later
                // startup scans until an explicit action resolves them.
                if matches!(
                    run.status,
                    WorkRunStatus::Recoverable | WorkRunStatus::WaitingDelivery
                ) {
                    continue;
                }

                // 3. Check pending interactions

                let has_pending_interaction = !pending_interactions.is_empty();
                let has_pending_plan = run.task_state.pending_approval.as_ref().is_some();

                if has_pending_plan || has_pending_interaction {
                    let target_status = if pending_interactions.iter().any(|i| {
                        matches!(
                            i.kind,
                            crate::work::models::PendingInteractionKind::UserInput
                                | crate::work::models::PendingInteractionKind::AppConnectionRequest
                                | crate::work::models::PendingInteractionKind::ConnectorAuthRequest
                        )
                    }) {
                        WorkRunStatus::WaitingInput
                    } else {
                        WorkRunStatus::WaitingApproval
                    };

                    if run.status != target_status {
                        self.task_manager
                            .set_run_status(&task.id, &run.id, target_status)?;
                    }
                    continue;
                }

                // 4. For runs that were running without pending interactions, inspect underlying session meta
                let mut final_status = WorkRunStatus::Failed;
                let mut error_msg = Some("Orphaned run reconciled on app restart".to_string());

                if let Some(session_id) = &run.session_id {
                    if let Some(meta) = crate::storage::runs::get_run(session_id) {
                        match meta.status {
                            crate::models::RunStatus::Completed => {
                                final_status = WorkRunStatus::Completed;
                                error_msg = None;
                            }
                            crate::models::RunStatus::Failed => {
                                final_status = WorkRunStatus::Failed;
                                error_msg = Some(
                                    meta.error_message
                                        .unwrap_or_else(|| "Session failed".to_string()),
                                );
                            }
                            crate::models::RunStatus::Stopped => {
                                final_status = WorkRunStatus::Cancelled;
                                error_msg = Some("Session stopped".to_string());
                            }
                            _ => {}
                        }
                    }
                }

                self.complete_or_fail_run(&task.id, &run.id, final_status, error_msg, None)?;
            }
        }
        Ok(())
    }
}

fn resolve_recovered_local_outputs(
    paths: &WorkPaths,
    workspace_id: &str,
    expected_outputs: &[String],
) -> Result<Vec<String>, String> {
    if expected_outputs.is_empty() {
        return Err("没有声明可验证的本地输出".to_string());
    }

    let workspace_root = paths.workspace_dir(workspace_id)?;
    let mut resolved = Vec::with_capacity(expected_outputs.len());
    for output in expected_outputs {
        let relative = output.trim().trim_start_matches("./");
        let path =
            paths.resolve_workspace_path(workspace_id, std::path::Path::new(relative), false)?;
        let relative = path
            .strip_prefix(&workspace_root)
            .map_err(|_| "本地输出不在当前 Workspace 内".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.starts_with("output/") {
            return Err("本地恢复输出必须位于 output/ 目录".to_string());
        }
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("无法检查输出文件：{error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
            return Err("输出必须是非空普通文件，且不能是符号链接".to_string());
        }
        let artifact_type = std::path::Path::new(&relative)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("file");
        artifacts::ArtifactValidator::validate_file(&path, artifact_type)
            .map_err(|error| format!("输出格式验证失败：{error}"))?;
        resolved.push(relative);
    }
    Ok(resolved)
}

fn format_artifact_acceptance_error(report: &WorkArtifactAcceptance) -> String {
    let details = report
        .checks
        .iter()
        .filter(|check| {
            check.requirement.required
                && !matches!(
                    check.status,
                    crate::work::models::WorkArtifactCheckStatus::Satisfied
                )
        })
        .map(|check| format!("{}: {}", check.requirement.path, check.message))
        .collect::<Vec<_>>();
    if details.is_empty() {
        "等待必需交付物验收".to_string()
    } else {
        format!("等待必需交付物验收：{}", details.join("；"))
    }
}

/// Build the read-only recovery projection consumed by both the Work chat and
/// the Inbox. No state is changed here; recovery actions are explicit commands.
pub fn get_run_recovery(
    paths: &WorkPaths,
    workspace_id: &str,
    task_id: &str,
    run_id: &str,
) -> Result<Option<WorkRunRecovery>, String> {
    let task_manager = TaskManager::new(paths.clone());
    let task = task_manager.get_task(task_id)?;
    if task.workspace_id != workspace_id {
        return Err("Workspace does not own this WorkTask".to_string());
    }
    let run = task_manager.get_run(task_id, run_id)?;
    if run.workspace_id != workspace_id || run.task_id != task_id {
        return Err("WorkRun ownership does not match task/workspace".to_string());
    }

    let interaction_manager = InteractionManager::new(paths.clone());
    let pending = interaction_manager.list_pending(Some(task_id), Some(run_id))?;
    let recovery_interaction = pending.iter().find(|interaction| {
        interaction
            .payload
            .get("recoveryKey")
            .and_then(|value| value.as_str())
            .is_some()
    });

    let ledger = crate::work::ledger::WorkRuntimeLedger::open(paths, task_id, run_id)?;
    let facts = ledger.list_facts()?;
    // Keep every proposed call until its own ToolResult arrives. A single
    // `unfinished` slot loses an earlier call when a later call completes,
    // which could incorrectly make a run look safe to complete.
    let mut unfinished_calls = std::collections::HashMap::new();
    let mut last_recovery = None;
    let mut detected_at = run.started_at.clone();
    for (fact_index, fact) in facts.iter().enumerate() {
        match fact {
            RuntimeFact::RuntimeInterrupted { timestamp, .. } => detected_at = timestamp.clone(),
            RuntimeFact::ToolProposed {
                tool_call_id,
                tool_name,
                action,
                expected_outputs,
                side_effect_class,
                ..
            } => {
                unfinished_calls.insert(
                    tool_call_id.clone(),
                    (
                        tool_name.clone(),
                        action.clone(),
                        expected_outputs.clone(),
                        *side_effect_class,
                        fact_index,
                    ),
                );
            }
            RuntimeFact::ToolResult { tool_call_id, .. } => {
                unfinished_calls.remove(tool_call_id);
            }
            RuntimeFact::RecoveryDetected {
                work_run_id: fact_run_id,
                original_tool_call_id,
                recovery_action,
                side_effect_class,
                reused_existing_output,
                result,
                error,
                timestamp,
            }
            | RuntimeFact::RecoveryActionStarted {
                work_run_id: fact_run_id,
                original_tool_call_id,
                recovery_action,
                side_effect_class,
                reused_existing_output,
                result,
                error,
                timestamp,
            }
            | RuntimeFact::RecoveryResolved {
                work_run_id: fact_run_id,
                original_tool_call_id,
                recovery_action,
                side_effect_class,
                reused_existing_output,
                result,
                error,
                timestamp,
            } if fact_run_id == run_id => {
                detected_at = timestamp.clone();
                last_recovery = Some((
                    original_tool_call_id.clone(),
                    recovery_action.clone(),
                    *side_effect_class,
                    *reused_existing_output,
                    result.clone(),
                    error.clone(),
                ));
            }
            _ => {}
        }
    }

    if recovery_interaction.is_none()
        && !matches!(
            run.status,
            WorkRunStatus::Recoverable | WorkRunStatus::WaitingDelivery
        )
        && unfinished_calls.is_empty()
    {
        return Ok(None);
    }

    let unfinished = unfinished_calls
        .into_iter()
        .max_by_key(|(_, (_, _, _, _, fact_index))| *fact_index)
        .map(
            |(tool_call_id, (tool_name, action, expected_outputs, side_effect_class, _))| {
                (
                    Some(tool_call_id),
                    Some(tool_name),
                    Some(action),
                    expected_outputs,
                    side_effect_class,
                )
            },
        );
    let (tool_call_id, tool_name, action, expected_outputs, side_effect_class) = unfinished
        .map(|value| (value.0, value.1, value.2, value.3, value.4))
        .unwrap_or_else(|| {
            let from_interaction = recovery_interaction.map(|interaction| {
                (
                    interaction.tool_call_id.clone(),
                    interaction
                        .payload
                        .get("toolName")
                        .and_then(|value| value.as_str())
                        .map(String::from),
                    interaction
                        .payload
                        .get("recoveryAction")
                        .and_then(|value| value.as_str())
                        .map(String::from),
                    interaction
                        .payload
                        .get("expectedOutputs")
                        .and_then(|value| serde_json::from_value(value.clone()).ok())
                        .unwrap_or_default(),
                    interaction
                        .payload
                        .get("sideEffectClass")
                        .and_then(|value| serde_json::from_value(value.clone()).ok())
                        .unwrap_or(SideEffectClass::ExternalMutating),
                )
            });
            from_interaction.unwrap_or((None, None, None, Vec::new(), SideEffectClass::Read))
        });

    let (
        _recovery_tool_call_id,
        recovery_action,
        recovery_side_effect_class,
        reused_existing_output,
        _recovery_result,
        recovery_error,
    ) = last_recovery.unwrap_or((
        None,
        "restart_recovery".to_string(),
        side_effect_class,
        false,
        None,
        None,
    ));
    let side_effect_class = recovery_side_effect_class;
    let reason = run
        .error_message
        .clone()
        .or(recovery_error)
        .unwrap_or_else(|| "应用重启后 Run 尚未恢复".to_string());
    let step_id = run
        .task_state
        .checkpoint
        .as_ref()
        .and_then(|checkpoint| checkpoint.current_step_id.clone());
    let step_title = step_id.as_ref().and_then(|id| {
        run.task_state
            .plan
            .iter()
            .find(|step| &step.id == id)
            .map(|step| step.text.clone())
    });
    let acceptance = if run.status == WorkRunStatus::WaitingDelivery {
        Some(artifacts::check_required_artifacts_with_paths(
            paths,
            workspace_id,
            run_id,
            &task,
        )?)
    } else {
        None
    };
    let interrupted_subagent_ids: Vec<String> = Vec::new();
    let interrupted_subagents = 0;
    let active_subagents = 0;
    let available_actions = match run.status {
        WorkRunStatus::WaitingDelivery => vec![
            WorkRecoveryAction::Verify,
            WorkRecoveryAction::Retry,
            WorkRecoveryAction::FromScratch,
            WorkRecoveryAction::Cancel,
        ],
        WorkRunStatus::Recoverable | WorkRunStatus::WaitingApproval => vec![
            WorkRecoveryAction::Continue,
            WorkRecoveryAction::Retry,
            WorkRecoveryAction::FromScratch,
            WorkRecoveryAction::Cancel,
        ],
        WorkRunStatus::Running if tool_call_id.is_some() => vec![
            WorkRecoveryAction::Continue,
            WorkRecoveryAction::Retry,
            WorkRecoveryAction::FromScratch,
            WorkRecoveryAction::Cancel,
        ],
        _ => Vec::new(),
    };

    Ok(Some(WorkRunRecovery {
        task_id: task_id.to_string(),
        work_run_id: run_id.to_string(),
        session_id: run.session_id,
        status: run.status,
        detected_at,
        step_id,
        step_title,
        tool_call_id,
        tool_name,
        action: action.or(Some(recovery_action)),
        side_effect_class: serde_json::to_value(side_effect_class)
            .ok()
            .and_then(|value| value.as_str().map(String::from)),
        reused_existing_output,
        reason,
        recommendation: if run.status == WorkRunStatus::WaitingDelivery {
            "先验证当前 Run 的交付物；验证通过后才会进入 Completed".to_string()
        } else if side_effect_class == SideEffectClass::ExternalMutating {
            "外部副作用结果未知，请确认是否已经完成，再选择继续或重试".to_string()
        } else {
            "选择继续或重试；系统不会在重启后自动重放调用".to_string()
        },
        pending_interaction_id: recovery_interaction
            .map(|interaction| interaction.interaction_id.clone()),
        expected_outputs,
        acceptance,
        pending_interaction_count: pending.len(),
        active_subagents,
        interrupted_subagents,
        interrupted_subagent_ids,
        available_actions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::models::ToolConcurrencyClass;
    use tempfile::TempDir;

    #[test]
    fn disallows_completed_to_running_transition() {
        let err = WorkHarnessController::validate_transition(
            WorkRunStatus::Completed,
            WorkRunStatus::Running,
        );
        assert!(err.is_err());
    }

    #[test]
    fn allows_valid_transitions() {
        assert!(WorkHarnessController::validate_transition(
            WorkRunStatus::Running,
            WorkRunStatus::WaitingApproval
        )
        .is_ok());
        assert!(WorkHarnessController::validate_transition(
            WorkRunStatus::WaitingApproval,
            WorkRunStatus::Running
        )
        .is_ok());
        assert!(WorkHarnessController::validate_transition(
            WorkRunStatus::Running,
            WorkRunStatus::Completed
        )
        .is_ok());
    }

    #[test]
    fn terminal_transition_uses_artifact_acceptance_gate() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("终态验收")
            .unwrap();
        let task_manager = TaskManager::new(paths.clone());
        let mut task = task_manager
            .create_task(&workspace.id, "交付验收", "生成报告", None)
            .unwrap();
        task.required_artifacts = vec!["output/report.md".to_string()];
        task_manager.update_task(&mut task).unwrap();
        let run = task_manager
            .start_run(&task.id, None, crate::work::models::WorkRunTrigger::Manual)
            .unwrap();

        let result = WorkHarnessController::new(paths.clone())
            .transition_run_status(&task.id, &run.id, WorkRunStatus::Completed)
            .unwrap();

        assert_eq!(result.status, WorkRunStatus::WaitingDelivery);
        assert!(result.finished_at.is_none());
        assert_eq!(
            task_manager.get_run(&task.id, &run.id).unwrap().status,
            WorkRunStatus::WaitingDelivery
        );
    }

    #[tokio::test]
    async fn terminal_work_run_keeps_live_actor_lease_until_process_exit() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        let workspace = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("进程租约")
            .unwrap();
        let task_manager = TaskManager::new(paths.clone());
        let task = task_manager
            .create_task(&workspace.id, "当前轮次完成", "保留会话", None)
            .unwrap();
        let run = task_manager
            .start_run(&task.id, None, crate::work::models::WorkRunTrigger::Manual)
            .unwrap();
        let token = format!("live-actor-{}", uuid::Uuid::new_v4());
        let bridge_state = crate::work::internal_bridge::bridge_state();
        bridge_state.tokens.write().await.insert(
            token.clone(),
            crate::work::internal_bridge::ProcessBridgeTokenInfo {
                token: token.clone(),
                run_id: run.id.clone(),
                task_id: Some(task.id.clone()),
                workspace_id: workspace.id.clone(),
                execution_context: crate::work::models::ExecutionContext::Attended,
                proxy_url: None,
            },
        );

        let finished = WorkHarnessController::new(paths)
            .complete_or_fail_run(&task.id, &run.id, WorkRunStatus::Completed, None, None)
            .unwrap();

        assert_eq!(finished.status, WorkRunStatus::Completed);
        assert!(bridge_state.tokens.read().await.contains_key(&token));
        crate::work::internal_bridge::revoke_session_token(&token).await;
    }

    #[test]
    fn recovery_projection_keeps_earlier_unresolved_tool_call() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        let task_manager = TaskManager::new(paths.clone());
        let task = task_manager
            .create_task("ws-1", "恢复多个调用", "验证账本恢复", None)
            .unwrap();
        let run = task_manager
            .start_run(&task.id, None, crate::work::models::WorkRunTrigger::Manual)
            .unwrap();
        let ledger =
            crate::work::ledger::WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();

        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-a".to_string(),
                tool_name: "work_run_command".to_string(),
                action: "execute-a".to_string(),
                arguments_hash: "hash-a".to_string(),
                expected_outputs: vec![],
                side_effect_class: SideEffectClass::LocalVerifiable,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: "2026-08-28T00:00:01Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-a".to_string(),
                execution_id: "exec-a".to_string(),
                timestamp: "2026-08-28T00:00:02Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-b".to_string(),
                tool_name: "work_read_file".to_string(),
                action: "read-b".to_string(),
                arguments_hash: "hash-b".to_string(),
                expected_outputs: vec![],
                side_effect_class: SideEffectClass::Read,
                concurrency_class: ToolConcurrencyClass::ParallelSafe,
                timestamp: "2026-08-28T00:00:03Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-b".to_string(),
                execution_id: "exec-b".to_string(),
                timestamp: "2026-08-28T00:00:04Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "call-b".to_string(),
                success: true,
                status: "success".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec![],
                side_effect_class: SideEffectClass::Read,
                timestamp: "2026-08-28T00:00:05Z".to_string(),
            })
            .unwrap();

        let recovery = get_run_recovery(&paths, "ws-1", &task.id, &run.id)
            .unwrap()
            .expect("the unresolved call must remain recoverable");
        assert_eq!(recovery.tool_call_id, Some("call-a".to_string()));
        assert_eq!(recovery.tool_name, Some("work_run_command".to_string()));
        assert_eq!(recovery.action, Some("execute-a".to_string()));
    }
}
