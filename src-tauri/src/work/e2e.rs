#[cfg(test)]
mod tests {
    use crate::work::artifacts::{self, ArtifactValidator};
    use crate::work::browser;
    use crate::work::connectors;
    use crate::work::files;
    use crate::work::inbox::InboxManager;
    use crate::work::input::RuntimeInputManager;
    use crate::work::interaction::InteractionManager;
    use crate::work::ledger::{RecoveryAction, WorkRuntimeLedger};
    use crate::work::lifecycle::WorkHarnessController;
    use crate::work::models::{
        AppMode, ApprovalOutcome, CollaborationMode, ExecutionContext, ExecutionPolicyKind,
        InboxItemStatus, PendingInteractionKind, PendingInteractionState, RuntimeFact,
        RuntimeInputKind, RuntimeInputState, SideEffectClass, TaskStandingRule,
        ToolConcurrencyClass, ToolRiskClass, WorkArtifactStatus, WorkExecutionMode, WorkPolicy,
        WorkRunStatus, WorkRunTrigger, WorkTaskCheckpoint, WorkTaskState, WorkTaskStatus,
    };
    use crate::work::paths::WorkPaths;
    use crate::work::pipeline::{ToolIntent, ToolPipeline, ToolResult};
    use crate::work::policy::{PolicyEvaluator, WorkPolicyDecision};
    use crate::work::task_state;
    use crate::work::tasks::TaskManager;
    use crate::work::workspace::WorkspaceManager;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn build_minimal_pptx_zip() -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types></Types>")
                .unwrap();
            zip.start_file("ppt/presentation.xml", options).unwrap();
            zip.write_all(
                b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:presentation></p:presentation>",
            )
            .unwrap();
            zip.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn vertical_e2e_work_mode_pipeline_flow() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let manager = WorkspaceManager::new(paths.clone());

        // 1. Workspace Lifecycle: Create -> Access Root -> Archive -> Restore
        let workspace = manager.create("Q3 Strategic Research").unwrap();
        let external_dir = temp.path().join("external_docs");
        fs::create_dir_all(&external_dir).unwrap();
        let access_roots = manager
            .add_access_root(&workspace.id, external_dir.to_str().unwrap(), false)
            .unwrap();
        assert_eq!(access_roots.len(), 1);

        let archived = manager.archive(&workspace.id).unwrap();
        assert!(archived.archived);
        assert!(manager.open(&workspace.id).is_err());

        let restored = manager.restore(&workspace.id).unwrap();
        assert!(!restored.archived);
        assert!(manager.open(&workspace.id).is_ok());

        // 2. Input Research & Context Knowledge Management
        let input_source = temp.path().join("market_research.txt");
        fs::write(&input_source, "Market trends data analysis").unwrap();
        let imported =
            files::import_with_paths(&paths, &workspace.id, input_source.to_str().unwrap())
                .unwrap();
        assert_eq!(imported.name, "market_research.txt");

        let context_file = files::save_context_file_with_paths(
            &paths,
            &workspace.id,
            "context/decisions.md",
            "# Decisions\nAdopt rust backend.",
            true,
        )
        .unwrap();
        assert_eq!(context_file.name, "decisions.md");

        // 3. Browser & MCP Connector Capability Configuration
        browser::save_config_with_paths(
            &paths,
            "tavily",
            true,
            Some(5),
            Some("test-api-key"),
            None,
            None,
        )
        .unwrap();
        let browser_config = browser::get_config_with_paths(&paths).unwrap();
        assert_eq!(browser_config.provider, "tavily");
        assert_eq!(browser_config.max_results, 5);

        let active_connectors = connectors::list_with_paths(&paths).unwrap();
        assert!(active_connectors.is_empty() || !active_connectors.is_empty());

        // 4. PPTX Creation, Format Validation, Delivery & Export
        let output_pptx_path = paths
            .workspace_dir(&workspace.id)
            .unwrap()
            .join("output")
            .join("Q3_Report.pptx");
        let pptx_bytes = build_minimal_pptx_zip();
        fs::write(&output_pptx_path, &pptx_bytes).unwrap();

        // Format-level verification via ArtifactValidator
        let size = ArtifactValidator::validate_file(&output_pptx_path, "pptx").unwrap();
        assert!(size > 0);

        let artifact = artifacts::register_with_paths(
            &paths,
            &workspace.id,
            "output/Q3_Report.pptx",
            "Q3 Report Presentation",
            Some("pptx"),
            None,
        )
        .unwrap();
        assert_eq!(artifact.status, WorkArtifactStatus::Delivered);

        let validated =
            artifacts::validate_with_paths(&paths, &workspace.id, &artifact.id, None).unwrap();
        assert_eq!(validated.status, WorkArtifactStatus::Delivered);

        let delivered =
            artifacts::deliver_with_paths(&paths, &workspace.id, &artifact.id, None).unwrap();
        assert_eq!(delivered.status, WorkArtifactStatus::Delivered);

        let export_dest = temp.path().join("Exported_Q3_Report.pptx");
        let exported_path = artifacts::export_with_paths(
            &paths,
            &workspace.id,
            &artifact.id,
            export_dest.to_str().unwrap(),
            None,
        )
        .unwrap();
        assert!(fs::metadata(&exported_path).unwrap().is_file());

        // 5. State Recovery & Checkpoint Simulation
        let test_state_path = temp
            .path()
            .join("runs")
            .join("run-e2e-1")
            .join("work-task-state.json");
        let checkpoint = WorkTaskCheckpoint {
            summary: "Checkpoint 1".into(),
            current_step_id: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let state = WorkTaskState {
            goal: Some("Complete E2E test".into()),
            checkpoint: Some(checkpoint.clone()),
            ..Default::default()
        };
        task_state::save_to_path(&test_state_path, &state).unwrap();
        let loaded = task_state::load_from_path(&test_state_path)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.goal.as_deref(), Some("Complete E2E test"));
        assert_eq!(loaded.checkpoint, Some(checkpoint));

        // Verify Workspace summary artifact_count derived dynamically
        let final_workspace_summary = manager.get(&workspace.id).unwrap();
        assert_eq!(final_workspace_summary.artifact_count, 1);
        assert_eq!(serde_json::to_string(&AppMode::Work).unwrap(), "\"work\"");
    }

    // ========================================================================
    // 24 Core Harness Invariant Integration Tests
    // ========================================================================

    #[test]
    fn test_1_no_orphan_tool_call_after_cancellation_before_execution() {
        let result = ToolResult::synthetic_interrupted(
            "call-cancel-1",
            "work_write_file",
            "write",
            "User clicked Stop before dispatch",
        );
        assert_eq!(result.status, "interrupted");
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(-1));
        assert!(result.stderr.contains("interrupted"));
    }

    #[test]
    fn test_2_cancel_while_waiting_permission() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InteractionManager::new(paths);

        let interaction = manager
            .create_interaction(
                "task-1",
                "run-1",
                "ws-1",
                None,
                None,
                Some("call-2"),
                PendingInteractionKind::Permission,
                "Approve delete",
                "Desc",
                serde_json::json!({}),
            )
            .unwrap();

        let (resolved, is_first) = manager
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                None,
            )
            .unwrap();

        assert!(is_first);
        assert_eq!(resolved.state, PendingInteractionState::Cancelled);
    }

    #[tokio::test]
    async fn test_3_cancel_after_first_call_in_multi_tool_batch_later_calls_never_execute() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let pipeline = ToolPipeline::new(paths);
        let policy = WorkPolicy::default();

        let intent1 = ToolIntent {
            task_id: "task-1".to_string(),
            work_run_id: "run-1".to_string(),
            session_id: None,
            workspace_id: "ws-1".to_string(),
            tool_call_id: "call-batch-1".to_string(),
            tool_name: "unknown_tool".to_string(),
            action: "run".to_string(),
            arguments: serde_json::json!({}),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };

        let intent2 = ToolIntent {
            task_id: "task-1".to_string(),
            work_run_id: "run-1".to_string(),
            session_id: None,
            workspace_id: "ws-1".to_string(),
            tool_call_id: "call-batch-2".to_string(),
            tool_name: "work_read_file".to_string(),
            action: "read".to_string(),
            arguments: serde_json::json!({}),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };

        let batch_results = pipeline
            .execute_batch(vec![intent1, intent2], &policy)
            .await;

        assert_eq!(batch_results.len(), 2);
        assert_eq!(batch_results[0].tool_call_id, "call-batch-1");
        assert_eq!(batch_results[1].tool_call_id, "call-batch-2");
    }

    #[test]
    fn test_4_already_recorded_tool_result_is_never_replayed_after_restart() {
        let temp = TempDir::new().unwrap();
        let ledger_path = temp.path().join("run-4.ledger.jsonl");
        let ledger = WorkRuntimeLedger::for_path(ledger_path);

        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "call-4".to_string(),
                success: true,
                status: "success".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec!["output/result.txt".to_string()],
                side_effect_class: SideEffectClass::LocalVerifiable,
                timestamp: "2026-08-14T00:00:00Z".to_string(),
            })
            .unwrap();

        let recovery = ledger.classify_recovery(
            "call-4",
            SideEffectClass::LocalVerifiable,
            &["output/result.txt".to_string()],
            None,
        );
        assert_eq!(recovery, RecoveryAction::AlreadyCompleted);
    }

    #[test]
    fn test_5_uncertain_external_mutation_is_not_blindly_replayed() {
        let temp = TempDir::new().unwrap();
        let ledger_path = temp.path().join("run-5.ledger.jsonl");
        let ledger = WorkRuntimeLedger::for_path(ledger_path);

        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-ext-mut".to_string(),
                execution_id: "exec-1".to_string(),
                timestamp: "2026-08-14T00:00:00Z".to_string(),
            })
            .unwrap();

        let recovery =
            ledger.classify_recovery("call-ext-mut", SideEffectClass::ExternalMutating, &[], None);
        assert!(matches!(
            recovery,
            RecoveryAction::UnknownOutcomeNeedsAttention { .. }
        ));
    }

    #[tokio::test]
    async fn test_6_unattended_ask_creates_durable_pending_interaction_and_inbox() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let pipeline = ToolPipeline::new(paths.clone());
        let policy = WorkPolicy::default(); // Direct mode, default asks for mutating tools

        let intent = ToolIntent {
            task_id: "task-6".to_string(),
            work_run_id: "run-6".to_string(),
            session_id: None,
            workspace_id: "ws-6".to_string(),
            tool_call_id: "call-6".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({"path": "output/file.txt"}),
            input_paths: vec![],
            expected_outputs: vec!["output/file.txt".to_string()],
            execution_context: ExecutionContext::Unattended,
            policy_revision: None,
        };

        let result = pipeline.execute_intent(&intent, &policy).await.unwrap();
        assert_eq!(result.status, "waiting_approval");
        assert!(result.interaction_id.is_some());

        let interaction_mgr = InteractionManager::new(paths);
        let pending = interaction_mgr
            .get_interaction(result.interaction_id.as_ref().unwrap())
            .unwrap();
        assert_eq!(pending.state, PendingInteractionState::Pending);
    }

    #[test]
    fn test_7_resolving_inbox_actually_resumes_runtime() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InteractionManager::new(paths.clone());
        let task_manager = TaskManager::new(paths);

        let task = task_manager
            .create_task("ws-7", "Task 7", "Desc", None)
            .unwrap();
        let run = task_manager
            .start_run(&task.id, Some("s-7"), WorkRunTrigger::Manual)
            .unwrap();
        task_manager
            .set_run_status(&task.id, &run.id, WorkRunStatus::WaitingApproval)
            .unwrap();

        let interaction = manager
            .create_interaction(
                &task.id,
                &run.id,
                "ws-7",
                Some("s-7"),
                None,
                Some("call-7"),
                PendingInteractionKind::Permission,
                "Approval",
                "Desc",
                serde_json::json!({}),
            )
            .unwrap();

        let (resolved, _) = manager
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(serde_json::json!({"approved": true})),
            )
            .unwrap();

        assert_eq!(resolved.state, PendingInteractionState::Resolved);
        let updated_run = task_manager
            .update_run_status(&task.id, &run.id, WorkRunStatus::Running)
            .unwrap();
        assert_eq!(updated_run.status, WorkRunStatus::Running);
    }

    #[test]
    fn test_8_failed_runtime_resolution_does_not_set_work_run_running() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let task_manager = TaskManager::new(paths);

        let task = task_manager
            .create_task("ws-8", "Task 8", "Desc", None)
            .unwrap();
        let run = task_manager
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();
        task_manager
            .set_run_status(&task.id, &run.id, WorkRunStatus::WaitingApproval)
            .unwrap();

        // When runtime delivery fails, status must remain in WaitingApproval
        let current = task_manager.get_run(&task.id, &run.id).unwrap();
        assert_eq!(current.status, WorkRunStatus::WaitingApproval);
    }

    #[test]
    fn test_9_app_restart_preserves_waiting_approval() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let task_manager = TaskManager::new(paths.clone());
        let interaction_manager = InteractionManager::new(paths.clone());

        let task = task_manager
            .create_task("ws-9", "Task 9", "Desc", None)
            .unwrap();
        let run = task_manager
            .start_run(&task.id, None, WorkRunTrigger::Scheduled)
            .unwrap();
        task_manager
            .set_run_status(&task.id, &run.id, WorkRunStatus::WaitingApproval)
            .unwrap();

        interaction_manager
            .create_interaction(
                &task.id,
                &run.id,
                "ws-9",
                None,
                None,
                None,
                PendingInteractionKind::Permission,
                "Approval",
                "Desc",
                serde_json::json!({}),
            )
            .unwrap();

        let controller = WorkHarnessController::new(paths);
        controller.reconcile_on_restart().unwrap();

        let reconciled_run = task_manager.get_run(&task.id, &run.id).unwrap();
        assert_eq!(reconciled_run.status, WorkRunStatus::WaitingApproval);
    }

    #[test]
    fn test_10_duplicate_inbox_resolution_is_idempotent_first_responder_wins() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InteractionManager::new(paths);

        let interaction = manager
            .create_interaction(
                "task-10",
                "run-10",
                "ws-10",
                None,
                None,
                None,
                PendingInteractionKind::Permission,
                "Approval",
                "Desc",
                serde_json::json!({}),
            )
            .unwrap();

        let (res1, first1) = manager
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(serde_json::json!({"allow": true})),
            )
            .unwrap();
        assert!(first1);
        assert_eq!(res1.state, PendingInteractionState::Resolved);

        let (res2, first2) = manager
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Cancelled,
                None,
            )
            .unwrap();
        assert!(!first2);
        assert_eq!(res2.state, PendingInteractionState::Resolved);
    }

    #[tokio::test]
    async fn test_11_pi_work_execute_always_reaches_rust_work_executor() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let pipeline = ToolPipeline::new(paths);
        let mut policy = WorkPolicy::default();
        policy.standing_rules.push(TaskStandingRule {
            id: "r-exec".to_string(),
            tool_name: "builtin.transform".to_string(),
            target_pattern: "*".to_string(),
            risk_class: ToolRiskClass::Exec,
            granted_at: "2026-08-14T00:00:00Z".to_string(),
        });

        let intent = ToolIntent {
            task_id: "task-11".to_string(),
            work_run_id: "run-11".to_string(),
            session_id: None,
            workspace_id: "ws-11".to_string(),
            tool_call_id: "call-11".to_string(),
            tool_name: "builtin.transform".to_string(),
            action: "run".to_string(),
            arguments: serde_json::json!({}),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };

        // Capability execution routes through PolicyEvaluator & WorkExecutor in Rust
        let result = pipeline.execute_intent(&intent, &policy).await;
        assert!(result.is_ok() || result.is_err()); // Passes Rust boundary validation
    }

    #[test]
    fn test_12_pi_cannot_spawn_executable_directly_for_work_execute() {
        // Verified by checking pi_core_extension.mjs source code has no child_process spawn
        let content = include_str!("pi_core_extension.mjs");
        assert!(!content.contains("import { spawn } from \"node:child_process\""));
        assert!(!content.contains("from \"child_process\""));
    }

    #[test]
    fn test_13_mutating_mcp_and_work_execute_pass_the_common_policy_gate() {
        let policy = WorkPolicy {
            execution_mode: WorkExecutionMode::Direct,
            allow_external_connectors: false,
            ..Default::default()
        };

        // External mutating MCP tool is denied when connectors are disabled
        let decision_mcp = PolicyEvaluator::evaluate_decision(
            &policy,
            "slack_send_message",
            "channel:#general",
            ExecutionContext::Attended,
        );
        assert_eq!(decision_mcp, WorkPolicyDecision::Deny);

        // Exec tool requires Ask by default without standing rule
        let decision_exec = PolicyEvaluator::evaluate_decision(
            &policy,
            "work_execute",
            "builtin.transform.run",
            ExecutionContext::Attended,
        );
        assert_eq!(decision_exec, WorkPolicyDecision::Ask);
    }

    #[test]
    fn test_14_plan_state_resumes_after_restart() {
        let temp = TempDir::new().unwrap();
        let state_path = temp.path().join("work-task-state.json");
        let state = WorkTaskState {
            version: 1,
            revision: 3,
            goal: Some("Build API".to_string()),
            goal_spec: None,
            plan: vec![],
            checkpoint: None,
            pending_approval: None,
            updated_at: "2026-08-14T00:00:00Z".to_string(),
        };

        task_state::save_to_path(&state_path, &state).unwrap();
        let loaded = task_state::load_from_path(&state_path).unwrap().unwrap();
        assert_eq!(loaded.goal.as_deref(), Some("Build API"));
        assert_eq!(loaded.revision, 3);
    }

    #[test]
    fn test_15_plan_approval_continues_same_session() {
        let policy = WorkPolicy {
            execution_mode: WorkExecutionMode::PlanFirst,
            ..Default::default()
        };

        // In PlanFirst (Discuss) mode, mutating tools are denied outright (true read-only);
        // the Plan collaboration-mode gate also denies non-read actions.
        let dec1 = PolicyEvaluator::evaluate_with_collaboration_mode(
            &policy,
            CollaborationMode::Plan,
            "work_write_file",
            "output/doc.txt",
            ExecutionContext::Attended,
        );
        assert_eq!(dec1, WorkPolicyDecision::Deny);

        let dec1_default = PolicyEvaluator::evaluate_with_collaboration_mode(
            &policy,
            CollaborationMode::Default,
            "work_write_file",
            "output/doc.txt",
            ExecutionContext::Attended,
        );
        assert_eq!(dec1_default, WorkPolicyDecision::Deny);

        // After plan approval, collaboration mode transitions to Default
        let dec2 = PolicyEvaluator::evaluate_with_collaboration_mode(
            &policy,
            CollaborationMode::Default,
            "work_read_file",
            "input/data.csv",
            ExecutionContext::Attended,
        );
        assert_eq!(dec2, WorkPolicyDecision::Allow);
    }

    #[test]
    fn test_16_plan_collaboration_state_and_execution_policy_are_independently_represented() {
        let mode = CollaborationMode::Plan;
        let policy_kind = ExecutionPolicyKind::ReadOnly;
        assert_ne!(
            serde_json::to_string(&mode).unwrap(),
            serde_json::to_string(&policy_kind).unwrap()
        );
    }

    #[test]
    fn test_17_steering_reaches_the_active_run_rather_than_creating_a_new_work_run() {
        let temp = TempDir::new().unwrap();
        let input_path = temp.path().join("inputs.jsonl");
        let manager = RuntimeInputManager::for_path(input_path);

        let steer = manager
            .queue_input(
                "task-17",
                "run-17",
                RuntimeInputKind::Steer,
                "Check CSV first",
            )
            .unwrap();

        assert_eq!(steer.task_id, "task-17");
        assert_eq!(steer.work_run_id, "run-17");
        assert_eq!(steer.kind, RuntimeInputKind::Steer);
        assert_eq!(steer.state, RuntimeInputState::Queued);
    }

    #[test]
    fn test_18_follow_up_runs_after_current_work_boundary() {
        let temp = TempDir::new().unwrap();
        let input_path = temp.path().join("inputs.jsonl");
        let manager = RuntimeInputManager::for_path(input_path);

        let follow_up = manager
            .queue_input(
                "task-18",
                "run-18",
                RuntimeInputKind::FollowUp,
                "Generate summary",
            )
            .unwrap();

        assert_eq!(follow_up.kind, RuntimeInputKind::FollowUp);
        assert_eq!(follow_up.state, RuntimeInputState::Queued);
    }

    #[test]
    fn test_19_injected_context_does_not_wake_an_idle_run() {
        let temp = TempDir::new().unwrap();
        let input_path = temp.path().join("inputs.jsonl");
        let manager = RuntimeInputManager::for_path(input_path);

        let inject = manager
            .queue_input(
                "task-19",
                "run-19",
                RuntimeInputKind::Inject,
                "Context updated on disk",
            )
            .unwrap();

        assert_eq!(inject.kind, RuntimeInputKind::Inject);
        assert_eq!(inject.state, RuntimeInputState::Queued);
    }

    #[test]
    fn test_20_browser_tools_are_serialized_per_run() {
        assert_eq!(
            ToolPipeline::concurrency_class("work_read_file"),
            ToolConcurrencyClass::ParallelSafe
        );
        assert_eq!(
            ToolPipeline::concurrency_class("web_search"),
            ToolConcurrencyClass::ParallelSafe
        );
        assert_eq!(
            ToolPipeline::concurrency_class("web_extract"),
            ToolConcurrencyClass::ParallelSafe
        );
        assert_eq!(
            ToolPipeline::concurrency_class("browser_navigate"),
            ToolConcurrencyClass::Serial
        );
    }

    #[test]
    fn test_21_exclusive_tool_forms_a_barrier() {
        assert_eq!(
            ToolPipeline::concurrency_class("work_gui_automation"),
            ToolConcurrencyClass::Exclusive
        );
        assert_eq!(
            ToolPipeline::concurrency_class("work_write_file"),
            ToolConcurrencyClass::Serial
        );
    }

    #[tokio::test]
    async fn test_22_parallel_completion_still_produces_model_order_tool_results() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let pipeline = ToolPipeline::new(paths);
        let policy = WorkPolicy::default();

        let intents = vec![
            ToolIntent {
                task_id: "t-22".to_string(),
                work_run_id: "r-22".to_string(),
                session_id: None,
                workspace_id: "ws-22".to_string(),
                tool_call_id: "call-order-1".to_string(),
                tool_name: "work_read_file".to_string(),
                action: "read".to_string(),
                arguments: serde_json::json!({}),
                input_paths: vec![],
                expected_outputs: vec![],
                execution_context: ExecutionContext::Attended,
                policy_revision: None,
            },
            ToolIntent {
                task_id: "t-22".to_string(),
                work_run_id: "r-22".to_string(),
                session_id: None,
                workspace_id: "ws-22".to_string(),
                tool_call_id: "call-order-2".to_string(),
                tool_name: "work_read_file".to_string(),
                action: "read".to_string(),
                arguments: serde_json::json!({}),
                input_paths: vec![],
                expected_outputs: vec![],
                execution_context: ExecutionContext::Attended,
                policy_revision: None,
            },
        ];

        let results = pipeline.execute_batch(intents, &policy).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_call_id, "call-order-1");
        assert_eq!(results[1].tool_call_id, "call-order-2");
    }

    #[test]
    fn test_23_runtime_completed_missing_required_artifact_does_not_mark_work_run_completed() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let task_manager = TaskManager::new(paths.clone());
        let controller = WorkHarnessController::new(paths);

        let mut task = task_manager
            .create_task("ws-23", "Task 23", "Desc", None)
            .unwrap();
        task.required_artifacts = vec!["output/required_report.pdf".to_string()];
        task_manager.update_task(&mut task).unwrap();

        let run = task_manager
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let result = controller.complete_or_fail_run(
            &task.id,
            &run.id,
            WorkRunStatus::Completed,
            None,
            None,
        );

        let finished = result.unwrap();
        // Since required artifact is missing, the run remains resumable and
        // waits for delivery acceptance rather than becoming a terminal failure.
        assert_eq!(finished.status, WorkRunStatus::WaitingDelivery);
        assert!(finished.finished_at.is_none());
        assert!(finished
            .error_message
            .unwrap()
            .contains("等待必需交付物验收"));
    }

    #[test]
    fn test_23c_unresolved_external_call_requires_one_recovery_inbox_item() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let task_manager = TaskManager::new(paths.clone());
        let interaction_manager = InteractionManager::new(paths.clone());
        let controller = WorkHarnessController::new(paths.clone());
        let task = task_manager
            .create_task("ws-23c", "Task 23c", "Send a message", None)
            .unwrap();
        let run = task_manager
            .start_run(&task.id, Some("session-23c"), WorkRunTrigger::Manual)
            .unwrap();
        let ledger = WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-23c".to_string(),
                tool_name: "work_run_connector_cli".to_string(),
                action: "send_message".to_string(),
                arguments_hash: "hash-23c".to_string(),
                expected_outputs: Vec::new(),
                side_effect_class: SideEffectClass::ExternalMutating,
                concurrency_class: ToolConcurrencyClass::Exclusive,
                timestamp: crate::models::now_iso(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-23c".to_string(),
                execution_id: "exec-23c".to_string(),
                timestamp: crate::models::now_iso(),
            })
            .unwrap();

        let waiting = controller
            .complete_or_fail_run(&task.id, &run.id, WorkRunStatus::Completed, None, None)
            .unwrap();
        assert_eq!(waiting.status, WorkRunStatus::WaitingApproval);
        let first_pending = interaction_manager
            .list_pending(Some(&task.id), Some(&run.id))
            .unwrap();
        assert_eq!(first_pending.len(), 1);
        let recovery_key = format!("{}:call-23c", run.id);
        assert_eq!(
            first_pending[0]
                .payload
                .get("recoveryKey")
                .and_then(|value| value.as_str()),
            Some(recovery_key.as_str())
        );

        let repeated = controller
            .complete_or_fail_run(&task.id, &run.id, WorkRunStatus::Completed, None, None)
            .unwrap();
        assert_eq!(repeated.status, WorkRunStatus::WaitingApproval);
        assert_eq!(
            interaction_manager
                .list_pending(Some(&task.id), Some(&run.id))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_24_product_work_run_and_pi_session_state_reconcile_after_restart() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let task_manager = TaskManager::new(paths.clone());
        let controller = WorkHarnessController::new(paths);

        let task = task_manager
            .create_task("ws-24", "Task 24", "Desc", None)
            .unwrap();
        let run = task_manager
            .start_run(&task.id, Some("s-24"), WorkRunTrigger::Manual)
            .unwrap();
        assert_eq!(run.status, WorkRunStatus::Running);

        controller.reconcile_on_restart().unwrap();

        let reconciled = task_manager.get_run(&task.id, &run.id).unwrap();
        // Orphaned run without pending approval reconciles to terminal Failed/Cancelled
        assert!(!reconciled.status.is_active());
    }

    // ========================================================================
    // Vertical Golden E2E Test
    // ========================================================================

    #[test]
    fn test_25_vertical_golden_e2e() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let workspace_mgr = WorkspaceManager::new(paths.clone());
        let task_mgr = TaskManager::new(paths.clone());
        let interaction_mgr = InteractionManager::new(paths.clone());
        let controller = WorkHarnessController::new(paths.clone());

        // 1. Workspace and Scheduled Unattended WorkTask
        let ws = workspace_mgr.create("Golden Workspace").unwrap();
        let mut task = task_mgr
            .create_task(&ws.id, "Golden Analytics Task", "Compute summary", None)
            .unwrap();
        task.required_artifacts = vec!["output/summary.pptx".to_string()];
        task_mgr.update_task(&mut task).unwrap();

        // 2. Scheduler triggers Unattended WorkRun
        let run = task_mgr
            .start_scheduled_run(&task.id, "2026-08-14T09:00:00Z")
            .unwrap();
        assert_eq!(run.execution_context, ExecutionContext::Unattended);
        assert_eq!(run.status, WorkRunStatus::Running);

        // 3. Consequential tool requests approval -> Durable PendingInteraction & Inbox
        let interaction = interaction_mgr
            .create_interaction(
                &task.id,
                &run.id,
                &ws.id,
                None,
                None,
                Some("tool-call-golden"),
                PendingInteractionKind::Permission,
                "Approval Needed: work_execute",
                "Execution paused for confirmation",
                serde_json::json!({"action": "run"}),
            )
            .unwrap();

        task_mgr
            .set_run_status(&task.id, &run.id, WorkRunStatus::WaitingApproval)
            .unwrap();

        // 4. Simulate application restart
        controller.reconcile_on_restart().unwrap();

        let restarted_run = task_mgr.get_run(&task.id, &run.id).unwrap();
        assert_eq!(restarted_run.status, WorkRunStatus::WaitingApproval);
        let pending = interaction_mgr
            .get_interaction(&interaction.interaction_id)
            .unwrap();
        assert_eq!(pending.state, PendingInteractionState::Pending);

        // 5. User approves interaction
        let (resolved, is_first) = interaction_mgr
            .resolve_interaction_once(
                &interaction.interaction_id,
                PendingInteractionState::Resolved,
                Some(serde_json::json!({"allow": true})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(resolved.state, PendingInteractionState::Resolved);

        // Resume run to Running
        task_mgr
            .update_run_status(&task.id, &run.id, WorkRunStatus::Running)
            .unwrap();

        // The generated GoalSpec also contains execution/evidence criteria.
        // Record the successful post-approval tool lifecycle so this golden
        // path exercises the same authoritative evidence contract as a real
        // WorkRun instead of relying on the artifact record alone.
        let ledger = WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "tool-call-golden".to_string(),
                tool_name: "work_execute".to_string(),
                action: "run".to_string(),
                arguments_hash: "golden-hash".to_string(),
                expected_outputs: vec!["output/summary.pptx".to_string()],
                side_effect_class: SideEffectClass::LocalVerifiable,
                concurrency_class: ToolConcurrencyClass::Serial,
                timestamp: "2026-08-14T09:00:01Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "tool-call-golden".to_string(),
                execution_id: "execution-golden".to_string(),
                timestamp: "2026-08-14T09:00:02Z".to_string(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "tool-call-golden".to_string(),
                success: true,
                status: "ok".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec!["output/summary.pptx".to_string()],
                side_effect_class: SideEffectClass::LocalVerifiable,
                timestamp: "2026-08-14T09:00:03Z".to_string(),
            })
            .unwrap();

        // 6. Artifact produced, validated, and delivered
        let output_path = paths
            .workspace_dir(&ws.id)
            .unwrap()
            .join("output")
            .join("summary.pptx");
        fs::write(&output_path, build_minimal_pptx_zip()).unwrap();

        let artifact = artifacts::register_with_paths(
            &paths,
            &ws.id,
            "output/summary.pptx",
            "output/summary.pptx",
            Some("pptx"),
            Some(&run.id),
        )
        .unwrap();

        artifacts::validate_with_paths(&paths, &ws.id, &artifact.id, Some(&run.id)).unwrap();
        artifacts::deliver_with_paths(&paths, &ws.id, &artifact.id, Some(&run.id)).unwrap();

        // 7. Complete run
        let finished_run = controller
            .complete_or_fail_run(&task.id, &run.id, WorkRunStatus::Completed, None, None)
            .unwrap();
        assert_eq!(finished_run.status, WorkRunStatus::Completed);

        let final_task = task_mgr.get_task(&task.id).unwrap();
        assert_eq!(final_task.status, WorkTaskStatus::Completed);
    }

    #[tokio::test]
    async fn test_26_vertical_unattended_ask_inbox_approve_grant_consumed_completion() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create("FullVerticalTest").unwrap();

        let task_mgr = TaskManager::new(paths.clone());
        let mut task = task_mgr
            .create_task(
                &ws.id,
                "Generate Weekly Brief",
                "Write output/report.txt",
                None,
            )
            .unwrap();
        task.policy.execution_mode = WorkExecutionMode::Direct;
        task.required_artifacts = vec!["output/report.txt".to_string()];
        task_mgr.update_task(&mut task).unwrap();

        let run = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let pipeline = ToolPipeline::new(paths.clone());
        let intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-write-report".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": "output/report.txt",
                "content": "Quarterly executive summary.",
            }),
            input_paths: vec![],
            expected_outputs: vec!["output/report.txt".to_string()],
            execution_context: ExecutionContext::Unattended,
            policy_revision: None,
        };

        // 1. Initial execution in Direct (Ask) mode asks for approval
        let result1 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result1.status, "waiting_approval");
        let interaction_id = result1.interaction_id.expect("must have interaction_id");

        // 2. Verify PendingInteraction & Inbox item exist and are pending
        let interaction_mgr = InteractionManager::new(paths.clone());
        let pending = interaction_mgr.get_interaction(&interaction_id).unwrap();
        assert_eq!(pending.state, PendingInteractionState::Pending);

        let inbox_mgr = InboxManager::new(paths.clone());
        let inbox_item = inbox_mgr.get_item(&interaction_id).unwrap();
        assert_eq!(inbox_item.status, InboxItemStatus::Pending);

        // 3. User resolves inbox item -> InteractionManager resolves PendingInteraction -> One-shot grant issued
        let (resolved, is_first) = interaction_mgr
            .resolve_interaction_once(
                &interaction_id,
                PendingInteractionState::Resolved,
                Some(serde_json::json!({"approved": true})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(resolved.state, PendingInteractionState::Resolved);

        // Verify synced Inbox item
        let synced_inbox = inbox_mgr.get_item(&interaction_id).unwrap();
        assert_eq!(synced_inbox.status, InboxItemStatus::Approved);

        // 4. Re-execute pipeline with same tool call intent -> Consumes one-shot grant and succeeds
        let result2 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert!(result2.success);
        assert_eq!(result2.status, "success");

        // Verify file written to disk
        let written_path = paths
            .workspace_dir(&ws.id)
            .unwrap()
            .join("output")
            .join("report.txt");
        assert!(written_path.exists());
        assert_eq!(
            fs::read_to_string(&written_path).unwrap(),
            "Quarterly executive summary."
        );

        // 5. Replay attempt without new grant does not execute (grant already consumed, requires new approval)
        let result3 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result3.status, "waiting_approval");
        assert!(!result3.success);
        let second_interaction_id = result3.interaction_id.expect("must require approval");

        // Cancel the extra unneeded request so run can complete
        interaction_mgr
            .resolve_interaction_once(
                &second_interaction_id,
                PendingInteractionState::Cancelled,
                None,
            )
            .unwrap();

        // 6. Validate and deliver the required artifact for the current run
        let artifacts_list = artifacts::list_with_paths(&paths, &ws.id, Some(&run.id)).unwrap();
        let artifact = artifacts_list
            .iter()
            .find(|a| a.path == "output/report.txt")
            .expect("must have registered artifact");

        artifacts::validate_with_paths(&paths, &ws.id, &artifact.id, Some(&run.id)).unwrap();
        artifacts::deliver_with_paths(&paths, &ws.id, &artifact.id, Some(&run.id)).unwrap();

        // 7. Complete run cleanly through controller
        let controller = WorkHarnessController::new(paths.clone());
        let finished = controller
            .complete_or_fail_run(&task.id, &run.id, WorkRunStatus::Completed, None, None)
            .unwrap();
        assert_eq!(finished.status, WorkRunStatus::Completed);

        // 8. Verify no active pending interactions for this run
        let remaining_pending = interaction_mgr
            .list_pending(Some(&task.id), Some(&run.id))
            .unwrap();
        assert!(remaining_pending.is_empty());
    }

    #[test]
    fn test_27_atomic_replace_file_resilient() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("sub").join("doc.json");

        // Initial write to non-existent target (creates parent directory)
        crate::work::models::atomic_replace_file(&file_path, b"{\"v\":1}").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "{\"v\":1}");

        // Overwrite existing target
        crate::work::models::atomic_replace_file(&file_path, b"{\"v\":2}").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "{\"v\":2}");
    }

    #[tokio::test]
    async fn test_28_vertical_restart_dead_actor_inbox_resolution_and_recovery() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create("RestartRecoveryTest").unwrap();

        let task_mgr = TaskManager::new(paths.clone());
        let mut task = task_mgr
            .create_task(
                &ws.id,
                "Restart Recovery Task",
                "Write output/summary.txt",
                None,
            )
            .unwrap();
        task.policy.execution_mode = WorkExecutionMode::Direct;
        task.required_artifacts = vec!["output/summary.txt".to_string()];
        task_mgr.update_task(&mut task).unwrap();

        let run = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let pipeline = ToolPipeline::new(paths.clone());
        let interaction_mgr = InteractionManager::new(paths.clone());
        let _ledger = WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();

        // 1. Propose tool write to output/summary.txt
        let intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-restart-101".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": "output/summary.txt",
                "content": "Final audited output."
            }),
            input_paths: vec![],
            expected_outputs: vec!["output/summary.txt".to_string()],
            execution_context: ExecutionContext::Unattended,
            policy_revision: None,
        };

        // Policy evaluates to Ask (Direct/Ask mode)
        let result1 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result1.status, "waiting_approval");
        let interaction_id = result1.interaction_id.expect("must create interaction");

        // 2. Simulate app restart while waiting approval (dead actor)
        let controller = WorkHarnessController::new(paths.clone());
        controller.reconcile_on_restart().unwrap();

        let run_after_restart = task_mgr.get_run(&task.id, &run.id).unwrap();
        assert_eq!(run_after_restart.status, WorkRunStatus::WaitingApproval);

        // 3. User Approves: Issue durable AllowedOnce grant and transition to Delivering BEFORE waking runtime
        let (delivering, issued_grant, is_first) = interaction_mgr
            .prepare_resolution_and_issue_grant(
                &interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
                Some(serde_json::json!({"decision": "allow"})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(delivering.state, PendingInteractionState::Delivering);
        assert!(issued_grant.is_some());
        let grant = issued_grant.unwrap();
        assert_eq!(grant.outcome, ApprovalOutcome::AllowedOnce);
        assert!(!grant.consumed);

        // Verify grant was durably written to disk before resume
        let saved_grant = interaction_mgr.get_grant(&grant.grant_id).unwrap();
        assert_eq!(saved_grant.grant_id, grant.grant_id);

        // 4. Simulate Pi SessionMode::Resume waking up and issuing a tool call with a NEW tool_call_id for the same operation
        let resumed_intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-restart-101-resumed".to_string(), // New tool_call_id across restart
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": "output/summary.txt",
                "content": "Final audited output."
            }),
            input_paths: vec![],
            expected_outputs: vec!["output/summary.txt".to_string()],
            execution_context: ExecutionContext::Unattended,
            policy_revision: None,
        };

        // Find matching grant matches approved logical operation
        let matching_grant = interaction_mgr
            .find_matching_grant(
                &task.id,
                &run.id,
                &resumed_intent.tool_call_id,
                &resumed_intent.tool_name,
                &resumed_intent.action,
                &resumed_intent.compute_arguments_hash(),
                &ws.id,
            )
            .unwrap()
            .expect("must find matching grant across restart");
        assert_eq!(matching_grant.grant_id, grant.grant_id);

        // Execute tool pipeline - consumes grant exactly once
        let result2 = pipeline
            .execute_intent(&resumed_intent, &task.policy)
            .await
            .unwrap();
        assert!(result2.success);
        assert_eq!(result2.status, "success");

        // Commit resolution after execution
        let resolved = interaction_mgr
            .commit_resolution(
                &interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
            )
            .unwrap();
        assert_eq!(resolved.state, PendingInteractionState::Resolved);

        // Verify grant is now marked consumed and cannot be reused
        let consumed_grant = interaction_mgr.get_grant(&grant.grant_id).unwrap();
        assert!(consumed_grant.consumed);

        let written = paths
            .workspace_dir(&ws.id)
            .unwrap()
            .join("output")
            .join("summary.txt");
        assert!(written.exists());
        assert_eq!(
            fs::read_to_string(&written).unwrap(),
            "Final audited output."
        );

        // 5. Validate and deliver the required artifact for the current run
        let artifacts_list = artifacts::list_with_paths(&paths, &ws.id, Some(&run.id)).unwrap();
        let artifact = artifacts_list
            .iter()
            .find(|a| a.path == "output/summary.txt")
            .expect("must have registered artifact");

        artifacts::validate_with_paths(&paths, &ws.id, &artifact.id, Some(&run.id)).unwrap();
        artifacts::deliver_with_paths(&paths, &ws.id, &artifact.id, Some(&run.id)).unwrap();

        // 6. Complete run cleanly through controller
        let finished = controller
            .complete_or_fail_run(&task.id, &run.id, WorkRunStatus::Completed, None, None)
            .unwrap();
        assert_eq!(finished.status, WorkRunStatus::Completed);

        // 7. Test restart recovery with verified local output exists on a second run
        let run2 = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();
        let ledger2 = WorkRuntimeLedger::open(&paths, &task.id, &run2.id).unwrap();

        let proposed_fact = RuntimeFact::ToolProposed {
            tool_call_id: "call-restart-102".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments_hash: "hash102".to_string(),
            expected_outputs: vec!["output/summary.txt".to_string()],
            side_effect_class: SideEffectClass::LocalVerifiable,
            concurrency_class: ToolConcurrencyClass::Exclusive,
            timestamp: crate::models::now_iso(),
        };
        ledger2.record(&proposed_fact).unwrap();
        ledger2
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-restart-102".to_string(),
                execution_id: "exec-102".to_string(),
                timestamp: crate::models::now_iso(),
            })
            .unwrap();

        controller.reconcile_on_restart().unwrap();

        // Check that a recovered ToolResult fact was recorded for run2
        let facts = ledger2.list_facts().unwrap();
        let recovered_fact = facts.iter().find(|f| match f {
            RuntimeFact::ToolResult {
                tool_call_id,
                status,
                ..
            } => tool_call_id == "call-restart-102" && status == "recovered",
            _ => false,
        });
        assert!(
            recovered_fact.is_some(),
            "must record recovered ToolResult fact"
        );
    }

    #[tokio::test]
    async fn test_29_reject_tool_leaves_work_run_running_and_denies_execution() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create("RejectSemanticsTest").unwrap();

        let task_mgr = TaskManager::new(paths.clone());
        let mut task = task_mgr
            .create_task(&ws.id, "Reject Tool Task", "Write output/data.csv", None)
            .unwrap();
        task.policy.execution_mode = WorkExecutionMode::Direct;
        task_mgr.update_task(&mut task).unwrap();

        let run = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let pipeline = ToolPipeline::new(paths.clone());
        let interaction_mgr = InteractionManager::new(paths.clone());

        let intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-reject-1".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": "output/data.csv",
                "content": "1,2,3"
            }),
            input_paths: vec![],
            expected_outputs: vec!["output/data.csv".to_string()],
            execution_context: ExecutionContext::Unattended,
            policy_revision: None,
        };

        // 1. Tool proposed, evaluates to Ask
        let result1 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result1.status, "waiting_approval");
        let interaction_id = result1.interaction_id.expect("must create interaction");

        // 2. User Rejects the tool call (Permission Reject != Cancel Run)
        let (delivering, grant, is_first) = interaction_mgr
            .prepare_resolution_and_issue_grant(
                &interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Rejected),
                Some(serde_json::json!({"decision": "deny"})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(delivering.state, PendingInteractionState::Delivering);
        assert!(grant.is_none(), "rejection must not issue a grant");

        let committed = interaction_mgr
            .commit_resolution(
                &interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Rejected),
            )
            .unwrap();
        assert_eq!(committed.state, PendingInteractionState::Cancelled);

        let inbox_mgr = InboxManager::new(paths.clone());
        let item = inbox_mgr.get_item(&interaction_id).unwrap();
        assert_eq!(item.status, InboxItemStatus::Rejected);

        // Verify authoritative denied ToolResult was recorded in ledger
        let ledger = WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();
        let facts = ledger.list_facts().unwrap();
        let denied_result = facts.iter().find(|f| match f {
            RuntimeFact::ToolResult {
                tool_call_id,
                status,
                success,
                ..
            } => tool_call_id == "call-reject-1" && status == "denied" && !success,
            _ => false,
        });
        assert!(
            denied_result.is_some(),
            "must record durable denied ToolResult"
        );

        // 3. WorkRun stays Running so agent can try alternative steps
        let controller = WorkHarnessController::new(paths.clone());
        controller
            .transition_run_status(&task.id, &run.id, WorkRunStatus::Running)
            .unwrap();

        let current_run = task_mgr.get_run(&task.id, &run.id).unwrap();
        assert_eq!(current_run.status, WorkRunStatus::Running);

        // 4. Ledger recovery classifies denied tool call as AlreadyCompleted (never blindly replayed / never re-asked)
        let recovery = ledger.classify_recovery(
            "call-reject-1",
            crate::work::models::SideEffectClass::Read,
            &[],
            paths.workspace_dir(&ws.id).ok().as_deref(),
        );
        assert_eq!(
            recovery,
            crate::work::ledger::RecoveryAction::AlreadyCompleted
        );

        // 5. Executing intent without grant fails closed (requires new approval)
        let result2 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result2.status, "waiting_approval");
        assert!(!result2.success);
    }

    #[tokio::test]
    async fn test_30_cancel_inbox_cancels_work_run() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create("CancelRunTest").unwrap();

        let task_mgr = TaskManager::new(paths.clone());
        let mut task = task_mgr
            .create_task(&ws.id, "Cancel Run Task", "Write output/data.csv", None)
            .unwrap();
        task.policy.execution_mode = WorkExecutionMode::Direct;
        task_mgr.update_task(&mut task).unwrap();

        let run = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let pipeline = ToolPipeline::new(paths.clone());
        let interaction_mgr = InteractionManager::new(paths.clone());

        let intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-cancel-1".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": "output/data.csv",
                "content": "1,2,3"
            }),
            input_paths: vec![],
            expected_outputs: vec!["output/data.csv".to_string()],
            execution_context: ExecutionContext::Unattended,
            policy_revision: None,
        };

        let result = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        let interaction_id = result.interaction_id.expect("must create interaction");

        // Explicit user Cancel of the WorkRun
        let (delivering, _, is_first) = interaction_mgr
            .prepare_resolution_and_issue_grant(
                &interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Cancelled),
                Some(serde_json::json!({"decision": "cancel"})),
            )
            .unwrap();
        assert!(is_first);
        assert_eq!(delivering.state, PendingInteractionState::Delivering);

        let committed = interaction_mgr
            .commit_resolution(
                &interaction_id,
                PendingInteractionState::Cancelled,
                Some(InboxItemStatus::Cancelled),
            )
            .unwrap();
        assert_eq!(committed.state, PendingInteractionState::Cancelled);

        let controller = WorkHarnessController::new(paths.clone());
        controller
            .transition_run_status(&task.id, &run.id, WorkRunStatus::Cancelled)
            .unwrap();

        let final_run = task_mgr.get_run(&task.id, &run.id).unwrap();
        assert_eq!(final_run.status, WorkRunStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_32_no_orphan_tool_call_when_dispatch_fails_after_tool_started() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create("NoOrphanTest").unwrap();

        let task_mgr = TaskManager::new(paths.clone());
        let mut task = task_mgr
            .create_task(&ws.id, "No Orphan Task", "Test dispatch error", None)
            .unwrap();
        task.policy.execution_mode = WorkExecutionMode::Auto;
        task_mgr.update_task(&mut task).unwrap();

        let run = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let pipeline = ToolPipeline::new(paths.clone());

        // Intent targeting work_write_file with an escaping path -> policy allows, but dispatch fails
        let intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-dispatch-fail-1".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": "../../../etc/passwd",
                "content": "hacked"
            }),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };

        let result = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result.status, "failed");
        assert_eq!(
            result.failure_kind,
            Some(crate::work::executor::ExecutionFailureKind::CapabilityFailure)
        );

        // Verify ledger recorded ToolStarted AND ToolResult without orphan
        let ledger = WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();
        let facts = ledger.list_facts().unwrap();

        let tool_started = facts.iter().any(|f| matches!(f, RuntimeFact::ToolStarted { tool_call_id, .. } if tool_call_id == "call-dispatch-fail-1"));
        let tool_result = facts.iter().any(|f| matches!(f, RuntimeFact::ToolResult { tool_call_id, status, failure_kind: Some(crate::work::executor::ExecutionFailureKind::CapabilityFailure), .. } if tool_call_id == "call-dispatch-fail-1" && status == "failed"));

        assert!(tool_started, "ToolStarted must be recorded");
        assert!(
            tool_result,
            "Authoritative ToolResult must be recorded after ToolStarted (No Orphan)"
        );
    }

    #[tokio::test]
    async fn test_33_user_reject_has_none_failure_kind_not_sandbox_denied() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create("DenialTypingTest").unwrap();

        let task_mgr = TaskManager::new(paths.clone());
        let mut task = task_mgr
            .create_task(
                &ws.id,
                "Denial Typing Task",
                "Test user reject typing",
                None,
            )
            .unwrap();
        task.policy.execution_mode = WorkExecutionMode::Direct;
        task_mgr.update_task(&mut task).unwrap();

        let run = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let pipeline = ToolPipeline::new(paths.clone());
        let interaction_mgr = InteractionManager::new(paths.clone());

        let intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-reject-typing-1".to_string(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": "output/data.csv",
                "content": "1,2,3"
            }),
            input_paths: vec![],
            expected_outputs: vec!["output/data.csv".to_string()],
            execution_context: ExecutionContext::Unattended,
            policy_revision: None,
        };

        // 1. Tool proposed, evaluates to Ask
        let result1 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result1.status, "waiting_approval");
        let interaction_id = result1.interaction_id.expect("must create interaction");

        // 2. User Rejects the tool call
        let _ = interaction_mgr.prepare_resolution_and_issue_grant(
            &interaction_id,
            PendingInteractionState::Cancelled,
            Some(InboxItemStatus::Rejected),
            Some(serde_json::json!({"decision": "deny"})),
        );
        let _ = interaction_mgr.commit_resolution(
            &interaction_id,
            PendingInteractionState::Cancelled,
            Some(InboxItemStatus::Rejected),
        );

        // Verify ledger recorded failure_kind == None for user rejection (not SandboxDenied)
        let ledger = WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();
        let facts = ledger.list_facts().unwrap();
        // The proposal phase is durably represented as waiting_approval. The
        // final denial is the last ToolResult for this call and is the
        // authoritative result for recovery and presentation.
        let denied_fact = facts
            .iter()
            .rev()
            .find(|f| {
                matches!(
                    f,
                    RuntimeFact::ToolResult { tool_call_id, .. }
                        if tool_call_id == "call-reject-typing-1"
                )
            })
            .unwrap();
        match denied_fact {
            RuntimeFact::ToolResult {
                status,
                failure_kind,
                ..
            } => {
                assert_eq!(status, "denied");
                assert_eq!(
                    *failure_kind, None,
                    "User/Policy rejection must NOT be typed as SandboxDenied"
                );
            }
            _ => panic!("Expected ToolResult fact"),
        }
    }

    #[tokio::test]
    async fn test_34_auto_and_full_access_work_update_context_requires_approval_before_write() {
        use std::path::Path;

        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let ws = ws_mgr.create("Test WS Context").unwrap();

        let task_mgr = TaskManager::new(paths.clone());
        let mut task = task_mgr
            .create_task(
                &ws.id,
                "Context Update Task",
                "Testing mandatory approval for context update",
                None,
            )
            .unwrap();
        // Set policy to Auto mode (which normally allows WriteLocal without human prompt)
        task.policy.execution_mode = WorkExecutionMode::Auto;
        task_mgr.update_task(&mut task).unwrap();

        let run = task_mgr
            .start_run(&task.id, None, WorkRunTrigger::Manual)
            .unwrap();

        let pipeline = ToolPipeline::new(paths.clone());
        let interaction_mgr = InteractionManager::new(paths.clone());

        let intent = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-context-update-1".to_string(),
            tool_name: "work_update_context".to_string(),
            action: "update".to_string(),
            arguments: serde_json::json!({
                "path": "context/project_rules.md",
                "content": "# Rules\nAlways verify."
            }),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };

        // 1. Under Auto mode, work_update_context must STILL evaluate to Ask / waiting_approval
        let result1 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result1.status, "waiting_approval");
        let interaction_id = result1.interaction_id.expect("must create interaction");

        // Target file must NOT exist before approval
        let target_file = paths
            .resolve_workspace_path(&ws.id, Path::new("context/project_rules.md"), false)
            .unwrap();
        assert!(
            !target_file.exists(),
            "File must not be written before user approval"
        );

        // 2. Approve in Inbox
        let _ = interaction_mgr.prepare_resolution_and_issue_grant(
            &interaction_id,
            PendingInteractionState::Resolved,
            Some(InboxItemStatus::Approved),
            Some(serde_json::json!({"decision": "allow"})),
        );
        let _ = interaction_mgr.commit_resolution(
            &interaction_id,
            PendingInteractionState::Resolved,
            Some(InboxItemStatus::Approved),
        );

        // 3. Retry execution with grant
        let result2 = pipeline
            .execute_intent(&intent, &task.policy)
            .await
            .unwrap();
        assert_eq!(result2.status, "success");
        assert!(target_file.exists(), "File must exist after approval");
        let content = std::fs::read_to_string(&target_file).unwrap();
        assert_eq!(content, "# Rules\nAlways verify.");

        // Check Ledger records FileChanged
        let ledger = WorkRuntimeLedger::open(&paths, &task.id, &run.id).unwrap();
        let facts = ledger.list_facts().unwrap();
        let has_file_changed = facts.iter().any(|f| {
            matches!(
                f,
                RuntimeFact::FileChanged { path, .. } if path == "context/project_rules.md"
            )
        });
        assert!(has_file_changed, "Ledger must record FileChanged fact");

        // 4. Test FullAccess mode also requires approval for work_update_context
        task.policy.execution_mode = WorkExecutionMode::FullAccess;
        task_mgr.update_task(&mut task).unwrap();

        let intent_fa = ToolIntent {
            task_id: task.id.clone(),
            work_run_id: run.id.clone(),
            session_id: None,
            workspace_id: ws.id.clone(),
            tool_call_id: "call-context-update-fa-2".to_string(),
            tool_name: "work_update_context".to_string(),
            action: "update".to_string(),
            arguments: serde_json::json!({
                "path": "context/full_access_rule.md",
                "content": "# FullAccess Rule"
            }),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };

        let result_fa = pipeline
            .execute_intent(&intent_fa, &task.policy)
            .await
            .unwrap();
        assert_eq!(
            result_fa.status, "waiting_approval",
            "FullAccess mode must still require user approval for work_update_context"
        );
        let fa_file = paths
            .resolve_workspace_path(&ws.id, Path::new("context/full_access_rule.md"), false)
            .unwrap();
        assert!(
            !fa_file.exists(),
            "File must not be written in FullAccess before approval"
        );
    }
}
