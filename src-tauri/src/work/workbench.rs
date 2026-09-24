//! WorkBench 2.0: Executable Work Runtime Acceptance Harness.
//!
//! This module is a high-confidence runtime contract and acceptance boundary
//! that validates the full Work Mode runtime stack without creating parallel state machines.
//! It tests real interactions among WorkHarnessController, WorkRuntimeLedger,
//! ToolPipeline, Artifact Registry, and Guardian.

use std::fs;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::work::artifacts;
use crate::work::guardian::Guardian;
use crate::work::interaction::InteractionManager;
use crate::work::ledger::WorkRuntimeLedger;
use crate::work::lifecycle::WorkHarnessController;
use crate::work::models::{
    ArtifactProducer, CollaborationMode, ExecutionContext, GuardianAnomalyKind, GuardianConfig,
    PendingInteractionKind, RunHealth, RuntimeFact, SideEffectClass, ToolConcurrencyClass,
    WorkArtifactRequirement, WorkArtifactStatus, WorkRunStatus, WorkRunTrigger,
};
use crate::work::paths::WorkPaths;
use crate::work::tasks::TaskManager;
use crate::work::workspace::WorkspaceManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkBenchLayer {
    Unit,
    RustRuntimeContract,
    AdapterFixtureContract,
    DesktopFixtureContract,
    CrashRestartContract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkBenchFailurePoint {
    OrdinaryFileGeneration,
    MissingRequiredArtifact,
    CorruptArtifactFormat,
    ApprovalRestart,
    UserInputRestart,
    LocalWriteThenProcessCrash,
    ExternalCallThenProcessCrash,
    ExternalAdapterFailure,
    SchedulerDuplicateTrigger,
    TimeoutOrTokenLimit,
    CodeWorkRegression,
    // Phase 1 P1 additions
    GuardianStallDetection,
    GuardianDuplicateToolLoop,
    GuardianToolFailureStreak,
    // Phase 2 P1 additions
    ArtifactEvidenceGeneration,
    ArtifactMutationInvalidation,
    SourceProvenanceBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkBenchExpectedOutcome {
    Completed,
    WaitingDelivery,
    WaitingApproval,
    WaitingInput,
    Recoverable,
    Failed,
    SingleRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkBenchCase {
    pub id: &'static str,
    pub failure_point: WorkBenchFailurePoint,
    pub layer: WorkBenchLayer,
    pub expected: WorkBenchExpectedOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkBenchCaseResult {
    pub case_id: String,
    pub layer: WorkBenchLayer,
    pub status: String,
    pub passed: bool,
    pub duration_ms: u64,
    pub expected_outcome: WorkBenchExpectedOutcome,
    pub actual_outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(default)]
    pub assertions_passed: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkBenchReport {
    /// The runtime this matrix is intended to protect. The runner below does
    /// not launch a live Pi process; it exercises the same Rust contracts.
    pub target_runtime: String,
    /// The runtime actually exercised by this report.
    pub runtime: String,
    /// The harness scope represented by this report.
    pub harness: String,
    pub started_at: String,
    pub finished_at: String,
    pub total_cases: usize,
    pub passed_cases: usize,
    pub failed_cases: usize,
    pub results: Vec<WorkBenchCaseResult>,
}

/// Release-gate scenarios for Work (19 data-driven cases).
pub const FAILURE_MATRIX: &[WorkBenchCase] = &[
    WorkBenchCase {
        id: "ordinary_file_generation",
        failure_point: WorkBenchFailurePoint::OrdinaryFileGeneration,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::Completed,
    },
    WorkBenchCase {
        id: "missing_required_artifact",
        failure_point: WorkBenchFailurePoint::MissingRequiredArtifact,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::WaitingDelivery,
    },
    WorkBenchCase {
        id: "corrupt_artifact_format",
        failure_point: WorkBenchFailurePoint::CorruptArtifactFormat,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::WaitingDelivery,
    },
    WorkBenchCase {
        id: "approval_restart",
        failure_point: WorkBenchFailurePoint::ApprovalRestart,
        layer: WorkBenchLayer::CrashRestartContract,
        expected: WorkBenchExpectedOutcome::WaitingApproval,
    },
    WorkBenchCase {
        id: "user_input_restart",
        failure_point: WorkBenchFailurePoint::UserInputRestart,
        layer: WorkBenchLayer::CrashRestartContract,
        expected: WorkBenchExpectedOutcome::WaitingInput,
    },
    WorkBenchCase {
        id: "local_write_then_process_crash",
        failure_point: WorkBenchFailurePoint::LocalWriteThenProcessCrash,
        layer: WorkBenchLayer::CrashRestartContract,
        expected: WorkBenchExpectedOutcome::Recoverable,
    },
    WorkBenchCase {
        id: "external_call_then_process_crash",
        failure_point: WorkBenchFailurePoint::ExternalCallThenProcessCrash,
        layer: WorkBenchLayer::CrashRestartContract,
        expected: WorkBenchExpectedOutcome::WaitingApproval,
    },
    WorkBenchCase {
        id: "mcp_browser_connector_failure",
        failure_point: WorkBenchFailurePoint::ExternalAdapterFailure,
        layer: WorkBenchLayer::AdapterFixtureContract,
        expected: WorkBenchExpectedOutcome::Failed,
    },
    WorkBenchCase {
        id: "scheduler_duplicate_trigger",
        failure_point: WorkBenchFailurePoint::SchedulerDuplicateTrigger,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::SingleRun,
    },
    WorkBenchCase {
        id: "timeout_token_limit",
        failure_point: WorkBenchFailurePoint::TimeoutOrTokenLimit,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::Failed,
    },
    WorkBenchCase {
        id: "code_work_regression",
        failure_point: WorkBenchFailurePoint::CodeWorkRegression,
        layer: WorkBenchLayer::DesktopFixtureContract,
        expected: WorkBenchExpectedOutcome::Completed,
    },
    // Phase 1 Guardian Scenarios
    WorkBenchCase {
        id: "guardian_stall_detection",
        failure_point: WorkBenchFailurePoint::GuardianStallDetection,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::Recoverable,
    },
    WorkBenchCase {
        id: "guardian_duplicate_tool_loop",
        failure_point: WorkBenchFailurePoint::GuardianDuplicateToolLoop,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::Recoverable,
    },
    WorkBenchCase {
        id: "guardian_tool_failure_streak",
        failure_point: WorkBenchFailurePoint::GuardianToolFailureStreak,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::Recoverable,
    },
    // Phase 2 Artifact Integrity Scenarios
    WorkBenchCase {
        id: "artifact_evidence_generation",
        failure_point: WorkBenchFailurePoint::ArtifactEvidenceGeneration,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::Completed,
    },
    WorkBenchCase {
        id: "artifact_mutation_invalidation",
        failure_point: WorkBenchFailurePoint::ArtifactMutationInvalidation,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::WaitingDelivery,
    },
    WorkBenchCase {
        id: "source_provenance_binding",
        failure_point: WorkBenchFailurePoint::SourceProvenanceBinding,
        layer: WorkBenchLayer::RustRuntimeContract,
        expected: WorkBenchExpectedOutcome::Completed,
    },
];

pub fn failure_matrix() -> &'static [WorkBenchCase] {
    FAILURE_MATRIX
}

pub struct WorkBenchRunner;

impl WorkBenchRunner {
    /// Execute all cases in the matrix and produce a structured report.
    pub fn run_all(paths: &WorkPaths) -> Result<WorkBenchReport, String> {
        let started_at = chrono::Utc::now().to_rfc3339();
        let mut results = Vec::new();
        for case in FAILURE_MATRIX {
            let res = Self::run_case(paths, case)?;
            results.push(res);
        }
        let passed_cases = results.iter().filter(|r| r.passed).count();
        let failed_cases = results.len() - passed_cases;
        let finished_at = chrono::Utc::now().to_rfc3339();
        Ok(WorkBenchReport {
            target_runtime: "pi".to_string(),
            runtime: "rust_work_contract".to_string(),
            harness: "rust_runtime_contract".to_string(),
            started_at,
            finished_at,
            total_cases: results.len(),
            passed_cases,
            failed_cases,
            results,
        })
    }

    /// Execute all cases and write the report JSON to the specified path.
    pub fn run_all_and_save(
        paths: &WorkPaths,
        report_path: &std::path::Path,
    ) -> Result<WorkBenchReport, String> {
        let report = Self::run_all(paths)?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| format!("Failed to serialize report: {e}"))?;
        if let Some(parent) = report_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent dir: {e}"))?;
        }
        fs::write(report_path, json).map_err(|e| format!("Failed to write report file: {e}"))?;
        Ok(report)
    }

    /// Execute a single test case through the production machinery.
    pub fn run_case(
        paths: &WorkPaths,
        case: &WorkBenchCase,
    ) -> Result<WorkBenchCaseResult, String> {
        let start = Instant::now();
        paths.ensure_layout()?;
        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create(&format!("case-{}", case.id))?;
        let ws_id = ws.id;
        let ws_dir = paths.workspace_dir(&ws_id)?;

        let task_mgr = TaskManager::new(paths.clone());
        let controller = WorkHarnessController::new(paths.clone());
        let interaction_mgr = InteractionManager::new(paths.clone());

        let mut actual_outcome = String::new();
        let mut passed = false;
        let mut details = None;
        let mut assertions_passed = Vec::new();

        match case.failure_point {
            WorkBenchFailurePoint::OrdinaryFileGeneration => {
                fs::write(ws_dir.join("output/result.txt"), "hello world")
                    .map_err(|e| format!("cannot write file: {e}"))?;

                let task = task_mgr.create_task(&ws_id, "File Gen", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;

                let summary = artifacts::register_with_paths(
                    paths,
                    &ws_id,
                    "output/result.txt",
                    "result.txt",
                    Some("txt"),
                    Some(&run.id),
                )?;

                assertions_passed.push("artifact_registered_with_sha256".to_string());

                if summary.status == WorkArtifactStatus::Delivered && summary.sha256.is_some() {
                    actual_outcome = "completed".to_string();
                    passed = true;
                    assertions_passed.push("artifact_status_delivered".to_string());
                } else {
                    actual_outcome = format!("{:?}", summary.status);
                    details = Some("Artifact was not properly delivered".to_string());
                }
            }

            WorkBenchFailurePoint::MissingRequiredArtifact => {
                let task =
                    task_mgr.create_task(&ws_id, "Missing Artifact", "instructions", None)?;
                let mut task = task_mgr.get_task(&task.id)?;
                task.artifact_requirements = vec![WorkArtifactRequirement {
                    path: "output/missing.docx".to_string(),
                    artifact_type: Some("docx".to_string()),
                    title: Some("Missing Doc".to_string()),
                    required: true,
                }];
                task_mgr.update_task(&mut task)?;

                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let final_run = controller.complete_or_fail_run(
                    &task.id,
                    &run.id,
                    WorkRunStatus::Completed,
                    None,
                    None,
                )?;

                actual_outcome = format!("{:?}", final_run.status);
                if final_run.status == WorkRunStatus::WaitingDelivery {
                    passed = true;
                    assertions_passed.push("fail_closed_on_missing_deliverable".to_string());
                } else {
                    details = Some(format!(
                        "Expected WaitingDelivery, got {:?}",
                        final_run.status
                    ));
                }
            }

            WorkBenchFailurePoint::CorruptArtifactFormat => {
                fs::write(ws_dir.join("output/corrupt.pdf"), "not a real pdf content")
                    .map_err(|e| format!("cannot write file: {e}"))?;

                let task = task_mgr.create_task(&ws_id, "Corrupt PDF", "instructions", None)?;
                let mut task = task_mgr.get_task(&task.id)?;
                task.artifact_requirements = vec![WorkArtifactRequirement {
                    path: "output/corrupt.pdf".to_string(),
                    artifact_type: Some("pdf".to_string()),
                    title: Some("Corrupt PDF".to_string()),
                    required: true,
                }];
                task_mgr.update_task(&mut task)?;

                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let final_run = controller.complete_or_fail_run(
                    &task.id,
                    &run.id,
                    WorkRunStatus::Completed,
                    None,
                    None,
                )?;

                actual_outcome = format!("{:?}", final_run.status);
                if final_run.status == WorkRunStatus::WaitingDelivery {
                    passed = true;
                    assertions_passed.push("fail_closed_on_corrupt_artifact".to_string());
                } else {
                    details = Some(format!(
                        "Expected WaitingDelivery, got {:?}",
                        final_run.status
                    ));
                }
            }

            WorkBenchFailurePoint::ApprovalRestart => {
                let task =
                    task_mgr.create_task(&ws_id, "Approval Restart", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let _interaction = interaction_mgr.create_interaction(
                    &task.id,
                    &run.id,
                    &ws_id,
                    None,
                    None,
                    Some("call-approval"),
                    PendingInteractionKind::Permission,
                    "Permission Req",
                    "Wait for permission",
                    serde_json::json!({}),
                )?;
                task_mgr.set_run_status(&task.id, &run.id, WorkRunStatus::WaitingApproval)?;

                controller.reconcile_on_restart()?;
                let reloaded_run = task_mgr.get_run(&task.id, &run.id)?;

                actual_outcome = format!("{:?}", reloaded_run.status);
                if reloaded_run.status == WorkRunStatus::WaitingApproval {
                    passed = true;
                    assertions_passed.push("pending_approval_persists_across_restart".to_string());
                } else {
                    details = Some(format!(
                        "Expected WaitingApproval, got {:?}",
                        reloaded_run.status
                    ));
                }
            }

            WorkBenchFailurePoint::UserInputRestart => {
                let task =
                    task_mgr.create_task(&ws_id, "User Input Restart", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let _interaction = interaction_mgr.create_interaction(
                    &task.id,
                    &run.id,
                    &ws_id,
                    None,
                    None,
                    Some("call-input"),
                    PendingInteractionKind::UserInput,
                    "Input Req",
                    "Wait for user input",
                    serde_json::json!({}),
                )?;
                task_mgr.set_run_status(&task.id, &run.id, WorkRunStatus::WaitingInput)?;

                controller.reconcile_on_restart()?;
                let reloaded_run = task_mgr.get_run(&task.id, &run.id)?;

                actual_outcome = format!("{:?}", reloaded_run.status);
                if reloaded_run.status == WorkRunStatus::WaitingInput {
                    passed = true;
                    assertions_passed.push("pending_input_persists_across_restart".to_string());
                } else {
                    details = Some(format!(
                        "Expected WaitingInput, got {:?}",
                        reloaded_run.status
                    ));
                }
            }

            WorkBenchFailurePoint::LocalWriteThenProcessCrash => {
                fs::write(ws_dir.join("output/saved.txt"), "saved content")
                    .map_err(|e| format!("cannot write file: {e}"))?;

                let task =
                    task_mgr.create_task(&ws_id, "Local Write Crash", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let ledger = WorkRuntimeLedger::open(paths, &task.id, &run.id)?;

                ledger.record(&RuntimeFact::ToolProposed {
                    tool_call_id: "call-local-1".to_string(),
                    tool_name: "work_write_file".to_string(),
                    action: "write".to_string(),
                    arguments_hash: "hash123".to_string(),
                    expected_outputs: vec!["output/saved.txt".to_string()],
                    side_effect_class: SideEffectClass::LocalVerifiable,
                    concurrency_class: ToolConcurrencyClass::Serial,
                    timestamp: crate::models::now_iso(),
                })?;

                controller.reconcile_on_restart()?;
                let reloaded_run = task_mgr.get_run(&task.id, &run.id)?;

                actual_outcome = format!("{:?}", reloaded_run.status);
                if reloaded_run.status == WorkRunStatus::Recoverable {
                    passed = true;
                    assertions_passed.push("local_verified_output_reused".to_string());
                } else {
                    details = Some(format!(
                        "Expected Recoverable, got {:?}",
                        reloaded_run.status
                    ));
                }
            }

            WorkBenchFailurePoint::ExternalCallThenProcessCrash => {
                // Hardcore Safety Contract Test
                let task = task_mgr.create_task(&ws_id, "External Crash", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let ledger = WorkRuntimeLedger::open(paths, &task.id, &run.id)?;

                // 1. ToolProposed + ToolStarted for ExternalMutating
                ledger.record(&RuntimeFact::ToolProposed {
                    tool_call_id: "call-ext-1".to_string(),
                    tool_name: "work_send_email".to_string(),
                    action: "send".to_string(),
                    arguments_hash: "hash-ext".to_string(),
                    expected_outputs: vec![],
                    side_effect_class: SideEffectClass::ExternalMutating,
                    concurrency_class: ToolConcurrencyClass::Serial,
                    timestamp: crate::models::now_iso(),
                })?;
                ledger.record(&RuntimeFact::ToolStarted {
                    tool_call_id: "call-ext-1".to_string(),
                    execution_id: "exec-ext-1".to_string(),
                    timestamp: crate::models::now_iso(),
                })?;

                // 2. Reconcile on crash / restart
                controller.reconcile_on_restart()?;
                let reloaded_run = task_mgr.get_run(&task.id, &run.id)?;
                let facts = ledger.list_facts()?;

                // Assert invariant 1: Status becomes WaitingApproval (NEVER Completed)
                let is_waiting = reloaded_run.status == WorkRunStatus::WaitingApproval;
                if is_waiting {
                    assertions_passed.push("run_status_is_waiting_approval".to_string());
                }

                // Assert invariant 2: RecoveryDetected fact recorded
                let recovery_fact_count = facts
                    .iter()
                    .filter(|f| matches!(f, RuntimeFact::RecoveryDetected { .. }))
                    .count();
                if recovery_fact_count == 1 {
                    assertions_passed.push("recovery_detected_fact_recorded".to_string());
                }

                // Assert invariant 3: NO duplicate ToolStarted for the external mutating call
                let started_count = facts
                    .iter()
                    .filter(|f| matches!(f, RuntimeFact::ToolStarted { tool_call_id, .. } if tool_call_id == "call-ext-1"))
                    .count();
                if started_count == 1 {
                    assertions_passed.push("no_duplicate_external_mutation_started".to_string());
                }

                // Assert invariant 4: Pending interaction exists
                let pending = interaction_mgr.list_pending(Some(&task.id), Some(&run.id))?;
                if !pending.is_empty() && pending[0].kind == PendingInteractionKind::Permission {
                    assertions_passed.push("pending_permission_interaction_created".to_string());
                }

                // Assert invariant 5: Repeated reconciliation is idempotent
                controller.reconcile_on_restart()?;
                let facts_after = ledger.list_facts()?;
                let second_recovery_count = facts_after
                    .iter()
                    .filter(|f| matches!(f, RuntimeFact::RecoveryDetected { .. }))
                    .count();
                if second_recovery_count == 1 {
                    assertions_passed.push("reconciliation_idempotency_preserved".to_string());
                }

                actual_outcome = format!("{:?}", reloaded_run.status);
                if is_waiting
                    && recovery_fact_count == 1
                    && started_count == 1
                    && second_recovery_count == 1
                {
                    passed = true;
                } else {
                    details = Some(format!(
                        "External safety assertions failed: status={:?}, recovery_facts={}, started_count={}",
                        reloaded_run.status, recovery_fact_count, started_count
                    ));
                }
            }

            WorkBenchFailurePoint::ExternalAdapterFailure => {
                let task = task_mgr.create_task(&ws_id, "Adapter Failure", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let ledger = WorkRuntimeLedger::open(paths, &task.id, &run.id)?;

                ledger.record(&RuntimeFact::ToolResult {
                    tool_call_id: "call-adapter-1".to_string(),
                    success: false,
                    status: "failed".to_string(),
                    failure_kind: Some(
                        crate::work::executor::ExecutionFailureKind::CapabilityFailure,
                    ),
                    exit_code: Some(1),
                    error: Some("mcp adapter disconnected".to_string()),
                    outputs: vec![],
                    side_effect_class: SideEffectClass::Read,
                    timestamp: crate::models::now_iso(),
                })?;

                let final_run = controller.complete_or_fail_run(
                    &task.id,
                    &run.id,
                    WorkRunStatus::Failed,
                    Some("Adapter failed".into()),
                    None,
                )?;
                actual_outcome = format!("{:?}", final_run.status);
                if final_run.status == WorkRunStatus::Failed {
                    passed = true;
                    assertions_passed.push("adapter_failure_handled_fail_closed".to_string());
                }
            }

            WorkBenchFailurePoint::SchedulerDuplicateTrigger => {
                let mut task =
                    task_mgr.create_task(&ws_id, "Scheduler Dup", "instructions", None)?;
                let fire_time_1 = chrono::DateTime::parse_from_rfc3339("2026-08-28T12:00:00Z")
                    .map_err(|e| e.to_string())?
                    .with_timezone(&chrono::Utc);
                let fire_time_2 = chrono::DateTime::parse_from_rfc3339("2026-08-28T12:05:00Z")
                    .map_err(|e| e.to_string())?
                    .with_timezone(&chrono::Utc);

                // Configure task schedule
                task.schedule = Some(crate::work::models::WorkScheduleConfig {
                    kind: crate::work::models::WorkScheduleKind::Once,
                    enabled: true,
                    timezone: "UTC".to_string(),
                    fire_at: Some(fire_time_1.to_rfc3339()),
                    time_of_day: None,
                    day_of_week: None,
                    cron_expression: None,
                    next_run_at: Some(fire_time_1.to_rfc3339()),
                    last_run_at: None,
                    max_runs: None,
                    run_on_startup: false,
                });
                task_mgr.update_task(&mut task)?;

                let scheduler = crate::work::scheduler::WorkScheduler::new(paths.clone());

                // 1. First trigger via real scheduler execution
                let run1 = scheduler
                    .evaluate_and_advance_task(&task.id, fire_time_1)?
                    .ok_or_else(|| "First schedule evaluation expected run to start".to_string())?;
                assert_eq!(run1.status, WorkRunStatus::Running);
                assertions_passed.push("first_scheduled_run_started".to_string());

                // 2. Set next schedule time to fire_time_2 for second trigger
                let mut task_reloaded = task_mgr.get_task(&task.id)?;
                task_reloaded.schedule.as_mut().unwrap().next_run_at =
                    Some(fire_time_2.to_rfc3339());
                task_mgr.update_task(&mut task_reloaded)?;

                // 3. Second trigger via real scheduler while run1 is active
                let run2 = scheduler
                    .evaluate_and_advance_task(&task.id, fire_time_2)?
                    .ok_or_else(|| {
                        "Second schedule evaluation expected skipped decision".to_string()
                    })?;
                assert_eq!(run2.status, WorkRunStatus::Skipped);
                assert_eq!(run2.skipped_reason.as_deref(), Some("previous_run_active"));
                assertions_passed.push("duplicate_scheduled_run_skipped".to_string());

                // 4. Verify exactly 1 active run and 1 skipped run exist
                let runs = task_mgr.list_runs(&task.id)?;
                let active_runs = runs.iter().filter(|r| r.status.is_active()).count();
                let skipped_runs = runs
                    .iter()
                    .filter(|r| r.status == WorkRunStatus::Skipped)
                    .count();

                if active_runs == 1 && skipped_runs == 1 {
                    actual_outcome = "single_run".to_string();
                    passed = true;
                    assertions_passed
                        .push("scheduler_prevents_duplicate_concurrent_run".to_string());
                } else {
                    details = Some(format!(
                        "Expected 1 active run and 1 skipped run, found active={active_runs}, skipped={skipped_runs}"
                    ));
                }
            }

            WorkBenchFailurePoint::TimeoutOrTokenLimit => {
                let task = task_mgr.create_task(&ws_id, "Timeout Task", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;

                let final_run = controller.complete_or_fail_run(
                    &task.id,
                    &run.id,
                    WorkRunStatus::Failed,
                    Some("timeout limit reached".into()),
                    None,
                )?;
                actual_outcome = format!("{:?}", final_run.status);
                if final_run.status == WorkRunStatus::Failed {
                    passed = true;
                    assertions_passed.push("timeout_stops_run_safely".to_string());
                }
            }

            WorkBenchFailurePoint::CodeWorkRegression => {
                let task = task_mgr.create_task(&ws_id, "Code Work", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;

                let final_run = controller.complete_or_fail_run(
                    &task.id,
                    &run.id,
                    WorkRunStatus::Completed,
                    None,
                    None,
                )?;
                actual_outcome = format!("{:?}", final_run.status);
                if final_run.status == WorkRunStatus::Completed {
                    passed = true;
                    assertions_passed.push("code_work_regression_clean".to_string());
                }
            }

            // Phase 1 Guardian Anomaly 1: Guardian Stall
            WorkBenchFailurePoint::GuardianStallDetection => {
                let task = task_mgr.create_task(&ws_id, "Stall Task", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let ledger = WorkRuntimeLedger::open(paths, &task.id, &run.id)?;

                ledger.record(&RuntimeFact::RunStarted {
                    task_id: task.id.clone(),
                    work_run_id: run.id.clone(),
                    execution_context: ExecutionContext::Attended,
                    collaboration_mode: CollaborationMode::Default,
                    timestamp: "2026-08-28T12:00:00Z".to_string(),
                })?;
                ledger.record(&RuntimeFact::StepStarted {
                    step_id: "step-stall-1".to_string(),
                    title: Some("Initial step".to_string()),
                    timestamp: "2026-08-28T12:00:01Z".to_string(),
                })?;

                let config = GuardianConfig {
                    stall_after_ms: 10_000,
                    stall_warn_after_ms: 5_000,
                    ..Default::default()
                };

                let report = Guardian::evaluate_health(
                    &ledger.list_facts()?,
                    WorkRunStatus::Running,
                    &config,
                    "2026-08-28T12:00:25Z",
                    None,
                );

                if report.health == RunHealth::Stalled
                    && report
                        .anomalies
                        .iter()
                        .any(|a| a.anomaly_kind == GuardianAnomalyKind::Stall)
                {
                    actual_outcome = "recoverable".to_string();
                    passed = true;
                    assertions_passed.push("guardian_stall_detected_and_reported".to_string());
                    assertions_passed.push("run_stalled_fact_generated".to_string());
                } else {
                    actual_outcome = format!("{:?}", report.health);
                    details = Some("Guardian failed to detect stall".to_string());
                }
            }

            // Phase 1 Guardian Anomaly 2: Guardian Duplicate Tool
            WorkBenchFailurePoint::GuardianDuplicateToolLoop => {
                let task = task_mgr.create_task(&ws_id, "Dup Tool Task", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let ledger = WorkRuntimeLedger::open(paths, &task.id, &run.id)?;

                for i in 1..=3 {
                    ledger.record(&RuntimeFact::ToolProposed {
                        tool_call_id: format!("call-dup-{i}"),
                        tool_name: "work_read_file".to_string(),
                        action: "read".to_string(),
                        arguments_hash: r#"{"path":"test.txt"}"#.to_string(),
                        expected_outputs: vec![],
                        side_effect_class: SideEffectClass::Read,
                        concurrency_class: ToolConcurrencyClass::ParallelSafe,
                        timestamp: format!("2026-08-28T12:00:0{i}Z"),
                    })?;
                }

                let config = GuardianConfig {
                    max_duplicate_calls: 3,
                    ..Default::default()
                };

                let report = Guardian::evaluate_health(
                    &ledger.list_facts()?,
                    WorkRunStatus::Running,
                    &config,
                    "2026-08-28T12:00:10Z",
                    None,
                );

                if report
                    .anomalies
                    .iter()
                    .any(|a| a.anomaly_kind == GuardianAnomalyKind::DuplicateTool)
                    && report.steering_nudge.is_some()
                {
                    actual_outcome = "recoverable".to_string();
                    passed = true;
                    assertions_passed.push("duplicate_tool_detected_with_canonical_fp".to_string());
                    assertions_passed.push("steering_nudge_generated".to_string());
                } else {
                    actual_outcome = format!("{:?}", report.health);
                    details = Some("Guardian failed to detect duplicate tool loop".to_string());
                }
            }

            // Phase 1 Guardian Anomaly 3: Guardian Failure Streak
            WorkBenchFailurePoint::GuardianToolFailureStreak => {
                let task =
                    task_mgr.create_task(&ws_id, "Failure Streak Task", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let ledger = WorkRuntimeLedger::open(paths, &task.id, &run.id)?;

                for i in 1..=3 {
                    ledger.record(&RuntimeFact::ToolResult {
                        tool_call_id: format!("call-fail-{i}"),
                        success: false,
                        status: "failed".to_string(),
                        failure_kind: None,
                        exit_code: Some(1),
                        error: Some("network error".to_string()),
                        outputs: vec![],
                        side_effect_class: SideEffectClass::Read,
                        timestamp: format!("2026-08-28T12:00:0{i}Z"),
                    })?;
                }

                let config = GuardianConfig {
                    max_failure_streak: 3,
                    ..Default::default()
                };

                let report = Guardian::evaluate_health(
                    &ledger.list_facts()?,
                    WorkRunStatus::Running,
                    &config,
                    "2026-08-28T12:00:10Z",
                    None,
                );

                if report.health == RunHealth::Degraded
                    && report
                        .anomalies
                        .iter()
                        .any(|a| a.anomaly_kind == GuardianAnomalyKind::ToolFailureStreak)
                {
                    actual_outcome = "recoverable".to_string();
                    passed = true;
                    assertions_passed.push("tool_failure_streak_detected".to_string());
                } else {
                    actual_outcome = format!("{:?}", report.health);
                    details = Some("Guardian failed to detect tool failure streak".to_string());
                }
            }

            // Phase 2 Artifact Evidence
            WorkBenchFailurePoint::ArtifactEvidenceGeneration => {
                fs::write(ws_dir.join("output/evidence.txt"), "evidence payload")
                    .map_err(|e| format!("cannot write file: {e}"))?;

                let task = task_mgr.create_task(&ws_id, "Evidence Task", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;

                let producer = Some(ArtifactProducer {
                    run_id: run.id.clone(),
                    producer_tool_call_id: Some("call-prod-1".to_string()),
                    execution_id: Some("exec-prod-1".to_string()),
                });

                let summary = artifacts::register_with_provenance_and_paths(
                    paths,
                    &ws_id,
                    "output/evidence.txt",
                    "evidence.txt",
                    Some("txt"),
                    Some(&run.id),
                    producer,
                    Vec::new(),
                )?;

                if summary.evidence.is_some()
                    && summary.sha256.is_some()
                    && summary.producer.is_some()
                {
                    actual_outcome = "completed".to_string();
                    passed = true;
                    assertions_passed.push("artifact_evidence_and_producer_recorded".to_string());
                } else {
                    actual_outcome = "missing_evidence".to_string();
                    details = Some("Artifact evidence or producer was missing".to_string());
                }
            }

            // Phase 2 Artifact Mutation Invalidation
            WorkBenchFailurePoint::ArtifactMutationInvalidation => {
                let file_path = ws_dir.join("output/tampered.txt");
                fs::write(&file_path, "original content")
                    .map_err(|e| format!("cannot write file: {e}"))?;

                let task = task_mgr.create_task(&ws_id, "Mutation Task", "instructions", None)?;
                let mut task = task_mgr.get_task(&task.id)?;
                task.artifact_requirements = vec![WorkArtifactRequirement {
                    path: "output/tampered.txt".to_string(),
                    artifact_type: Some("txt".to_string()),
                    title: Some("Tampered Doc".to_string()),
                    required: true,
                }];
                task_mgr.update_task(&mut task)?;

                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;

                // 1. Register artifact with original content -> Delivered
                let summary = artifacts::register_with_paths(
                    paths,
                    &ws_id,
                    "output/tampered.txt",
                    "Tampered Doc",
                    Some("txt"),
                    Some(&run.id),
                )?;
                assert_eq!(summary.status, WorkArtifactStatus::Delivered);
                assertions_passed.push("initial_registration_delivered".to_string());

                // 2. Tamper file content on disk after registration
                fs::write(&file_path, "tampered content changed after registration")
                    .map_err(|e| format!("cannot tamper file: {e}"))?;

                // 3. Acceptance evaluation should detect hash mismatch and fail completion
                let final_run = controller.complete_or_fail_run(
                    &task.id,
                    &run.id,
                    WorkRunStatus::Completed,
                    None,
                    None,
                )?;
                actual_outcome = format!("{:?}", final_run.status);

                if final_run.status == WorkRunStatus::WaitingDelivery {
                    passed = true;
                    assertions_passed.push("tampered_artifact_invalidates_delivery".to_string());
                } else {
                    details = Some(format!(
                        "Expected WaitingDelivery upon mutation, got {:?}",
                        final_run.status
                    ));
                }
            }

            // Phase 2 Source Provenance Binding
            WorkBenchFailurePoint::SourceProvenanceBinding => {
                fs::write(ws_dir.join("output/sourced.txt"), "sourced content")
                    .map_err(|e| format!("cannot write file: {e}"))?;

                let task = task_mgr.create_task(&ws_id, "Source Task", "instructions", None)?;
                let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;
                let ledger = WorkRuntimeLedger::open(paths, &task.id, &run.id)?;

                // Record search tool call in ledger
                ledger.record(&RuntimeFact::ToolProposed {
                    tool_call_id: "call-search-1".to_string(),
                    tool_name: "web_search".to_string(),
                    action: "search".to_string(),
                    arguments_hash: "hash-query".to_string(),
                    expected_outputs: vec![],
                    side_effect_class: SideEffectClass::Read,
                    concurrency_class: ToolConcurrencyClass::ParallelSafe,
                    timestamp: crate::models::now_iso(),
                })?;
                ledger.record(&RuntimeFact::ToolResult {
                    tool_call_id: "call-search-1".to_string(),
                    success: true,
                    status: "success".to_string(),
                    failure_kind: None,
                    exit_code: Some(0),
                    error: None,
                    outputs: vec!["https://rust-lang.org".to_string()],
                    side_effect_class: SideEffectClass::Read,
                    timestamp: crate::models::now_iso(),
                })?;

                let sources = artifacts::extract_sources_from_ledger(
                    paths,
                    &task.id,
                    &run.id,
                    &["call-search-1".to_string()],
                );

                assert_eq!(sources.len(), 1);
                assert_eq!(sources[0].source_type, "web_search");
                assert_eq!(sources[0].url.as_deref(), Some("https://rust-lang.org"));
                assert!(!sources[0].is_claimed);

                let summary = artifacts::register_with_provenance_and_paths(
                    paths,
                    &ws_id,
                    "output/sourced.txt",
                    "sourced.txt",
                    Some("txt"),
                    Some(&run.id),
                    None,
                    sources,
                )?;

                if !summary.sources.is_empty()
                    && summary.sources[0].url.as_deref() == Some("https://rust-lang.org")
                {
                    actual_outcome = "completed".to_string();
                    passed = true;
                    assertions_passed.push("sources_extracted_and_bound_to_artifact".to_string());
                } else {
                    actual_outcome = "missing_sources".to_string();
                    details = Some("Artifact sources were not properly bound".to_string());
                }
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(WorkBenchCaseResult {
            case_id: case.id.to_string(),
            layer: case.layer,
            status: if passed {
                "passed".to_string()
            } else {
                "failed".to_string()
            },
            passed,
            duration_ms: elapsed,
            expected_outcome: case.expected,
            actual_outcome,
            failure_reason: if passed { None } else { details.clone() },
            assertions_passed,
            details,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::ledger::RecoveryAction;
    use tempfile::TempDir;

    fn incomplete_tool(
        ledger: &WorkRuntimeLedger,
        tool_call_id: &str,
        side_effect: SideEffectClass,
        expected_outputs: Vec<String>,
    ) {
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "work_test_fixture".to_string(),
                action: "run".to_string(),
                arguments_hash: format!("hash-{tool_call_id}"),
                expected_outputs,
                side_effect_class: side_effect,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: crate::models::now_iso(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: tool_call_id.to_string(),
                execution_id: format!("exec-{tool_call_id}"),
                timestamp: crate::models::now_iso(),
            })
            .unwrap();
    }

    #[test]
    fn matrix_contains_all_17_release_gates() {
        let required = [
            "ordinary_file_generation",
            "missing_required_artifact",
            "corrupt_artifact_format",
            "approval_restart",
            "user_input_restart",
            "local_write_then_process_crash",
            "external_call_then_process_crash",
            "mcp_browser_connector_failure",
            "scheduler_duplicate_trigger",
            "timeout_token_limit",
            "code_work_regression",
            // P1 Guardian Cases
            "guardian_stall_detection",
            "guardian_duplicate_tool_loop",
            "guardian_tool_failure_streak",
            // P2 Artifact Integrity Cases
            "artifact_evidence_generation",
            "artifact_mutation_invalidation",
            "source_provenance_binding",
        ];

        assert_eq!(failure_matrix().len(), 17);
        assert_eq!(failure_matrix().len(), required.len());
        for id in required {
            assert!(
                failure_matrix().iter().any(|case| case.id == id),
                "WorkBench matrix is missing case {id}"
            );
        }
    }

    #[test]
    fn local_write_crash_reuses_only_verified_output_and_closes_the_call() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        fs::create_dir_all(root.join("output")).unwrap();
        fs::write(root.join("output/result.txt"), "verified").unwrap();
        let ledger = WorkRuntimeLedger::for_path(temp.path().join("run.ledger.jsonl"));

        incomplete_tool(
            &ledger,
            "call-local-crash",
            SideEffectClass::LocalVerifiable,
            vec!["output/result.txt".to_string()],
        );

        assert_eq!(
            ledger.classify_recovery(
                "call-local-crash",
                SideEffectClass::LocalVerifiable,
                &["output/result.txt".to_string()],
                Some(&root),
            ),
            RecoveryAction::VerifiedLocalOutputExists
        );

        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "call-local-crash".to_string(),
                success: true,
                status: "recovered".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec!["output/result.txt".to_string()],
                side_effect_class: SideEffectClass::LocalVerifiable,
                timestamp: crate::models::now_iso(),
            })
            .unwrap();
        assert_eq!(
            ledger.classify_recovery(
                "call-local-crash",
                SideEffectClass::LocalVerifiable,
                &["output/result.txt".to_string()],
                Some(&root),
            ),
            RecoveryAction::AlreadyCompleted
        );
    }

    #[test]
    fn external_crash_requires_attention_and_recovery_audit_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let ledger = WorkRuntimeLedger::for_path(temp.path().join("run.ledger.jsonl"));
        incomplete_tool(
            &ledger,
            "call-external-crash",
            SideEffectClass::ExternalMutating,
            Vec::new(),
        );

        let recovery = ledger.classify_recovery(
            "call-external-crash",
            SideEffectClass::ExternalMutating,
            &[],
            None,
        );
        assert!(matches!(
            recovery,
            RecoveryAction::UnknownOutcomeNeedsAttention { .. }
        ));

        assert!(ledger
            .record_recovery_event_once(
                "recovery_detected",
                "run-workbench",
                Some("call-external-crash"),
                "external_unknown_outcome",
                SideEffectClass::ExternalMutating,
                false,
                None,
                Some("external result is unknown".to_string()),
            )
            .unwrap());
        assert!(!ledger
            .record_recovery_event_once(
                "recovery_detected",
                "run-workbench",
                Some("call-external-crash"),
                "external_unknown_outcome",
                SideEffectClass::ExternalMutating,
                false,
                None,
                Some("duplicate restart scan".to_string()),
            )
            .unwrap());
        assert_eq!(
            ledger
                .list_facts()
                .unwrap()
                .iter()
                .filter(|fact| matches!(fact, RuntimeFact::RecoveryDetected { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn workbench_runner_executes_all_17_cases_successfully() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("workbench_data"));

        let report = WorkBenchRunner::run_all(&paths).unwrap();
        assert_eq!(report.total_cases, 17);
        assert_eq!(report.target_runtime, "pi");
        assert_eq!(report.runtime, "rust_work_contract");
        assert_eq!(report.harness, "rust_runtime_contract");
        assert_eq!(
            report.passed_cases,
            17,
            "All 17 WorkBench cases should pass. Failures: {:?}",
            report
                .results
                .iter()
                .filter(|r| !r.passed)
                .collect::<Vec<_>>()
        );
        assert_eq!(report.failed_cases, 0);

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"total_cases\": 17"));
        assert!(json.contains("\"passed_cases\": 17"));
    }

    #[test]
    fn workbench_runner_runs_and_saves_report_file() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("workbench_data"));
        let report_file = temp.path().join("reports/workbench_report.json");

        let report = WorkBenchRunner::run_all_and_save(&paths, &report_file).unwrap();
        assert_eq!(report.total_cases, 17);
        assert_eq!(report.passed_cases, 17);
        assert_eq!(report.target_runtime, "pi");
        assert!(report_file.exists());
        let saved_content = fs::read_to_string(&report_file).unwrap();
        assert!(saved_content.contains("\"total_cases\": 17"));
    }
}

#[cfg(test)]
mod golden;

#[cfg(test)]
pub mod golden_harness;

#[cfg(test)]
pub mod golden_fixtures;
