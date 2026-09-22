//! Golden Business Acceptance Scenarios for Work Mode.
//!
//! Validates real business end-to-end chains across:
//! 1. Contract Archive: Approval -> Execution -> Restart -> Recovery -> Manifest Artifact -> Completed
//! 2. Sales Multi-Artifact: Partial Artifacts -> WaitingDelivery -> All Artifacts -> Completed
//! 3. Multi-Agent Completion: Active Children -> Parked Settlement -> All Terminal -> Completed
//! 4. Multi-Agent Recovery: Running Child -> App Crash -> Restart -> Interrupted Child Detected -> Recoverable

use std::fs;

use crate::work::ledger::WorkRuntimeLedger;
use crate::work::models::{
    ApprovalOutcome, ExecutionContext, RuntimeFact, SideEffectClass, ToolConcurrencyClass,
    WorkExecutionMode, WorkRunStatus,
};
use crate::work::pipeline::ToolIntent;
use crate::work::session::RuntimeTurnOutcome;
use crate::work::subagents::{RegisterSpawnRequest, UpdateStatusRequest};
use crate::work::workbench::golden_fixtures;
use crate::work::workbench::golden_harness::{
    assert_artifact_count, assert_artifact_valid, assert_no_duplicate_tool_started,
    assert_no_pending_interactions, assert_run_status, assert_tool_executed_once, GoldenHarness,
    GoldenScenarioAssertions, GoldenScenarioResult,
};
use crate::work::workspace::WorkspaceManager;

#[tokio::test]
async fn golden_contract_archive_approval_restart_resume() {
    let mut harness = GoldenHarness::new(
        "ws-contract-archive",
        "Contract Archive",
        "Archive 12 contracts by client with external directory approval and restart recovery",
        WorkExecutionMode::Auto,
    )
    .expect("failed to initialize golden harness");

    // 1. Setup external contracts fixture (12 contracts across clients A, B, C)
    let ext_temp = tempfile::TempDir::new().expect("failed to create external temp dir");
    let external_dir = ext_temp
        .path()
        .canonicalize()
        .expect("canonicalize external temp dir");
    let contract_files = golden_fixtures::setup_contract_fixtures(&external_dir)
        .expect("failed to setup contract fixtures");
    assert_eq!(contract_files.len(), 12);
    let contracts_dir = external_dir.join("contracts");
    let contracts_dir_str = contracts_dir.to_str().expect("valid utf-8 path");

    // 2. Verify: before approval, workspace has no access roots and reading external path is denied
    let ws_mgr = WorkspaceManager::new(harness.paths.clone());
    let ws_initial = ws_mgr.get(&harness.workspace.id).unwrap();
    assert!(
        ws_initial.access_roots.is_empty(),
        "Access roots must be empty before approval"
    );

    let unauth_read_intent = ToolIntent {
        task_id: harness.task.id.clone(),
        work_run_id: harness.run.id.clone(),
        session_id: None,
        workspace_id: harness.workspace.id.clone(),
        tool_call_id: "call-unauth-read".to_string(),
        tool_name: "work_read_file".to_string(),
        action: "read".to_string(),
        arguments: serde_json::json!({
            "path": contracts_dir.join(&contract_files[0]).to_str().unwrap()
        }),
        input_paths: vec![],
        expected_outputs: vec![],
        execution_context: ExecutionContext::Attended,
        policy_revision: None,
    };
    let unauth_res = harness.tool(&unauth_read_intent).await.unwrap();
    assert!(
        !unauth_res.success,
        "Reading external contract without authorization must fail"
    );
    assert!(
        unauth_res
            .stderr
            .contains("not within an authorized external directory"),
        "Expected unauthorized external directory error, got: {}",
        unauth_res.stderr
    );

    // 3. Request external directory access -> WaitingApproval
    let req_access_intent = ToolIntent {
        task_id: harness.task.id.clone(),
        work_run_id: harness.run.id.clone(),
        session_id: None,
        workspace_id: harness.workspace.id.clone(),
        tool_call_id: "call-request-access".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": contracts_dir_str,
            "writable": false,
            "purpose": "Archive contracts by client"
        }),
        input_paths: vec![],
        expected_outputs: vec![],
        execution_context: ExecutionContext::Attended,
        policy_revision: None,
    };
    let req_res = harness.tool(&req_access_intent).await.unwrap();
    assert_eq!(req_res.status, "waiting_approval");
    assert_run_status(&harness, WorkRunStatus::WaitingApproval);
    let interaction_id = req_res.interaction_id.expect("must produce interaction_id");

    // 4. Human approves: issue durable AllowedOnce grant and resume tool execution
    let grant = harness
        .approve(&interaction_id)
        .expect("must issue approval grant");
    assert_eq!(grant.outcome, ApprovalOutcome::AllowedOnce);
    assert!(!grant.consumed);

    // Re-execute with same tool_call_id: consumes grant and attaches access root
    let approved_res = harness.tool(&req_access_intent).await.unwrap();
    assert!(
        approved_res.success,
        "Tool execution with valid grant must succeed, got status: {}, stderr: {}, stdout: {}",
        approved_res.status, approved_res.stderr, approved_res.stdout
    );
    assert_eq!(approved_res.status, "success");

    // Verify Access Root is now authorized in the workspace
    let ws_after = ws_mgr.get(&harness.workspace.id).unwrap();
    assert_eq!(ws_after.access_roots.len(), 1);
    assert_run_status(&harness, WorkRunStatus::Running);

    // 5. Process first half of contracts (indices 0..6: A-001..4, B-001..2)
    for i in 0..6 {
        let contract_name = &contract_files[i];
        let client = &contract_name[0..1];
        let ext_path = contracts_dir.join(contract_name);

        let read_intent = ToolIntent {
            task_id: harness.task.id.clone(),
            work_run_id: harness.run.id.clone(),
            session_id: None,
            workspace_id: harness.workspace.id.clone(),
            tool_call_id: format!("call-read-{}", contract_name),
            tool_name: "work_read_file".to_string(),
            action: "read".to_string(),
            arguments: serde_json::json!({
                "path": ext_path.to_str().unwrap()
            }),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };
        let read_res = harness.tool(&read_intent).await.unwrap();
        assert!(
            read_res.success,
            "Failed reading contract {contract_name}: status={}, stderr={}, stdout={}",
            read_res.status, read_res.stderr, read_res.stdout
        );
        let content = read_res.stdout;

        let dest_rel_path = format!("output/archive/{}/{}", client, contract_name);
        let write_intent = ToolIntent {
            task_id: harness.task.id.clone(),
            work_run_id: harness.run.id.clone(),
            session_id: None,
            workspace_id: harness.workspace.id.clone(),
            tool_call_id: format!("call-write-{}", contract_name),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": dest_rel_path,
                "content": content
            }),
            input_paths: vec![],
            expected_outputs: vec![dest_rel_path],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };
        let write_res = harness.tool(&write_intent).await.unwrap();
        assert!(
            write_res.success,
            "Failed writing archived contract {contract_name}"
        );
    }

    // 6. Save Checkpoint
    harness
        .save_checkpoint("Processed 6/12 contracts", Some("contract-B-002"))
        .expect("must save checkpoint");

    // 7. Fault Injection: simulate crash during processing of contract 7 (index 6: B-003.txt)
    // The file is written to disk, and ToolProposed is recorded, but the process crashes
    // before ToolResult is written.
    let injected_name = &contract_files[6];
    let injected_client = &injected_name[0..1];
    let injected_rel_path = format!("output/archive/{}/{}", injected_client, injected_name);
    let injected_dest_full = harness
        .paths
        .workspace_dir(&harness.workspace.id)
        .unwrap()
        .join(&injected_rel_path);
    if let Some(parent) = injected_dest_full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let ext_contract_content = fs::read_to_string(contracts_dir.join(injected_name)).unwrap();
    // fault injection: file written to disk before crash
    fs::write(&injected_dest_full, &ext_contract_content).unwrap();

    let injected_call_id = format!("call-write-{}", injected_name);
    let ledger =
        WorkRuntimeLedger::open(&harness.paths, &harness.task.id, &harness.run.id).unwrap();
    // fault injection: durable ToolProposed + ToolStarted without ToolResult
    ledger
        .record(&RuntimeFact::ToolProposed {
            tool_call_id: injected_call_id.clone(),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments_hash: "hash-fault-inject-b003".to_string(),
            expected_outputs: vec![injected_rel_path.clone()],
            side_effect_class: SideEffectClass::LocalVerifiable,
            concurrency_class: ToolConcurrencyClass::Serial,
            timestamp: crate::models::now_iso(),
        })
        .unwrap();
    ledger
        .record(&RuntimeFact::ToolStarted {
            tool_call_id: injected_call_id.clone(),
            execution_id: format!("exec-crash-{}", injected_name),
            timestamp: crate::models::now_iso(),
        })
        .unwrap();

    // 8. Restart & Reconcile
    harness
        .restart()
        .expect("restart and reconcile must succeed");
    // System automatically recovers verified local output and transitions to Recoverable
    assert_run_status(&harness, WorkRunStatus::Recoverable);

    // Verify recovery fact in ledger
    let facts = harness.facts().unwrap();
    let has_recovered_tool_result = facts.iter().any(|f| match f {
        RuntimeFact::ToolResult {
            tool_call_id,
            status,
            success,
            ..
        } => tool_call_id == &injected_call_id && status == "recovered" && *success,
        _ => false,
    });
    assert!(
        has_recovered_tool_result,
        "Durable ledger must contain recovered ToolResult for injected call"
    );

    // 9. Resume: Agent transitions run back to Running
    harness
        .controller
        .transition_run_status(&harness.task.id, &harness.run.id, WorkRunStatus::Running)
        .expect("must resume running");
    assert_run_status(&harness, WorkRunStatus::Running);

    // Agent inspects completed work from durable ledger: contracts 0..6 (1..7) are done.
    // Contracts 0..6 MUST NOT be re-executed! Agent proceeds with remaining contracts 7..12 (8..12).
    for i in 7..12 {
        let contract_name = &contract_files[i];
        let client = &contract_name[0..1];
        let ext_path = contracts_dir.join(contract_name);

        let read_intent = ToolIntent {
            task_id: harness.task.id.clone(),
            work_run_id: harness.run.id.clone(),
            session_id: None,
            workspace_id: harness.workspace.id.clone(),
            tool_call_id: format!("call-read-{}", contract_name),
            tool_name: "work_read_file".to_string(),
            action: "read".to_string(),
            arguments: serde_json::json!({
                "path": ext_path.to_str().unwrap()
            }),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };
        let read_res = harness.tool(&read_intent).await.unwrap();
        assert!(read_res.success);

        let dest_rel_path = format!("output/archive/{}/{}", client, contract_name);
        let write_intent = ToolIntent {
            task_id: harness.task.id.clone(),
            work_run_id: harness.run.id.clone(),
            session_id: None,
            workspace_id: harness.workspace.id.clone(),
            tool_call_id: format!("call-write-{}", contract_name),
            tool_name: "work_write_file".to_string(),
            action: "write".to_string(),
            arguments: serde_json::json!({
                "path": dest_rel_path,
                "content": read_res.stdout
            }),
            input_paths: vec![],
            expected_outputs: vec![dest_rel_path],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };
        let write_res = harness.tool(&write_intent).await.unwrap();
        assert!(write_res.success);
    }

    // 10. Generate output/archive_manifest.json
    let manifest_rel_path = "output/archive_manifest.json";
    let manifest_json = serde_json::json!({
        "totalArchived": 12,
        "clients": ["A", "B", "C"],
        "files": contract_files.iter().map(|f| {
            let client = &f[0..1];
            format!("output/archive/{}/{}", client, f)
        }).collect::<Vec<_>>(),
        "archivedAt": crate::models::now_iso(),
    });
    let manifest_intent = ToolIntent {
        task_id: harness.task.id.clone(),
        work_run_id: harness.run.id.clone(),
        session_id: None,
        workspace_id: harness.workspace.id.clone(),
        tool_call_id: "call-write-archive-manifest".to_string(),
        tool_name: "work_write_file".to_string(),
        action: "write".to_string(),
        arguments: serde_json::json!({
            "path": manifest_rel_path,
            "content": serde_json::to_string_pretty(&manifest_json).unwrap()
        }),
        input_paths: vec![],
        expected_outputs: vec![manifest_rel_path.to_string()],
        execution_context: ExecutionContext::Attended,
        policy_revision: None,
    };
    let manifest_res = harness.tool(&manifest_intent).await.unwrap();
    assert!(manifest_res.success);

    // 11. Completion Gate
    let final_run = harness
        .complete()
        .expect("complete_or_fail_run must succeed");
    assert_eq!(final_run.status, WorkRunStatus::Completed);
    assert_run_status(&harness, WorkRunStatus::Completed);

    // 12. Final Assertions
    assert_no_pending_interactions(&harness);
    assert_no_duplicate_tool_started(&harness);

    // Verify all 12 contract files are in output/archive/
    let ws_dir = harness.paths.workspace_dir(&harness.workspace.id).unwrap();
    for f in &contract_files {
        let client = &f[0..1];
        let p = ws_dir.join(format!("output/archive/{}/{}", client, f));
        assert!(
            p.exists(),
            "Archived contract file must exist: {}",
            p.display()
        );
    }

    // Verify archive_manifest.json artifact
    assert_artifact_valid(&harness, manifest_rel_path, "json");
    let manifest_file_content = fs::read_to_string(ws_dir.join(manifest_rel_path)).unwrap();
    let parsed_manifest: serde_json::Value = serde_json::from_str(&manifest_file_content).unwrap();
    assert_eq!(parsed_manifest["totalArchived"], 12);
    assert_eq!(parsed_manifest["files"].as_array().unwrap().len(), 12);

    // Tool executed once assertions for specific samples
    assert_tool_executed_once(&harness, "call-write-A-001.txt");
    assert_tool_executed_once(&harness, "call-write-B-001.txt");
    assert_tool_executed_once(&harness, "call-write-C-004.txt");

    // Produce structured result
    let scenario_result = GoldenScenarioResult {
        scenario: "golden_contract_archive_approval_restart_resume".to_string(),
        passed: true,
        final_run_status: "completed".to_string(),
        assertions: GoldenScenarioAssertions {
            approval_persisted: true,
            duplicate_executions: 0,
            artifact_satisfied: true,
            pending_interactions: 0,
        },
        details: Some(
            "All 12 contracts archived, restart recovery succeeded, manifest valid".to_string(),
        ),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&scenario_result).unwrap()
    );
}

#[tokio::test]
async fn golden_sales_multi_artifact_delivery_gate() {
    let mut harness = GoldenHarness::new(
        "ws-sales-analysis",
        "Quarterly Sales Review",
        "Produce Excel summary, PowerPoint review, and Word analysis",
        WorkExecutionMode::Auto,
    )
    .expect("failed to initialize golden harness");

    // 1. Setup CSV input fixtures
    let ws_dir = harness.paths.workspace_dir(&harness.workspace.id).unwrap();
    golden_fixtures::setup_sales_input_fixtures(&ws_dir).expect("failed to setup sales inputs");

    // 2. Configure Required Artifacts on Task
    harness.task.required_artifacts = vec![
        "output/sales_summary.xlsx".to_string(),
        "output/management_review.pptx".to_string(),
        "output/analysis.docx".to_string(),
    ];
    harness
        .task_manager
        .update_task(&mut harness.task)
        .expect("must update task with required artifacts");

    // 3. Agent reads CSV inputs through ToolPipeline
    for month_file in &["input/january.csv", "input/february.csv", "input/march.csv"] {
        let read_intent = ToolIntent {
            task_id: harness.task.id.clone(),
            work_run_id: harness.run.id.clone(),
            session_id: None,
            workspace_id: harness.workspace.id.clone(),
            tool_call_id: format!("call-read-{}", month_file.replace('/', "-")),
            tool_name: "work_read_file".to_string(),
            action: "read".to_string(),
            arguments: serde_json::json!({
                "path": month_file
            }),
            input_paths: vec![],
            expected_outputs: vec![],
            execution_context: ExecutionContext::Attended,
            policy_revision: None,
        };
        let res = harness.tool(&read_intent).await.unwrap();
        assert!(res.success, "Failed reading input {}", month_file);
    }

    // 4. Agent generates and registers the first 2 required artifacts (xlsx and pptx)
    let xlsx_bytes = golden_fixtures::minimal_xlsx();
    let xlsx_full_path = ws_dir.join("output/sales_summary.xlsx");
    if let Some(parent) = xlsx_full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&xlsx_full_path, &xlsx_bytes).unwrap();

    let reg_xlsx_intent = ToolIntent {
        task_id: harness.task.id.clone(),
        work_run_id: harness.run.id.clone(),
        session_id: None,
        workspace_id: harness.workspace.id.clone(),
        tool_call_id: "call-register-sales-xlsx".to_string(),
        tool_name: "work_register_artifact".to_string(),
        action: "register".to_string(),
        arguments: serde_json::json!({
            "path": "output/sales_summary.xlsx",
            "title": "Sales Summary Workbook",
            "artifact_type": "xlsx"
        }),
        input_paths: vec![],
        expected_outputs: vec!["output/sales_summary.xlsx".to_string()],
        execution_context: ExecutionContext::Attended,
        policy_revision: None,
    };
    let reg_xlsx_res = harness.tool(&reg_xlsx_intent).await.unwrap();
    assert!(
        reg_xlsx_res.success,
        "status: {}, stderr: {}, stdout: {}",
        reg_xlsx_res.status, reg_xlsx_res.stderr, reg_xlsx_res.stdout
    );

    let pptx_bytes = golden_fixtures::minimal_pptx();
    let pptx_full_path = ws_dir.join("output/management_review.pptx");
    fs::write(&pptx_full_path, &pptx_bytes).unwrap();

    let reg_pptx_intent = ToolIntent {
        task_id: harness.task.id.clone(),
        work_run_id: harness.run.id.clone(),
        session_id: None,
        workspace_id: harness.workspace.id.clone(),
        tool_call_id: "call-register-review-pptx".to_string(),
        tool_name: "work_register_artifact".to_string(),
        action: "register".to_string(),
        arguments: serde_json::json!({
            "path": "output/management_review.pptx",
            "title": "Management Review Slides",
            "artifact_type": "pptx"
        }),
        input_paths: vec![],
        expected_outputs: vec!["output/management_review.pptx".to_string()],
        execution_context: ExecutionContext::Attended,
        policy_revision: None,
    };
    let reg_pptx_res = harness.tool(&reg_pptx_intent).await.unwrap();
    assert!(reg_pptx_res.success);

    // 5. Attempt Completion: MUST be intercepted by Completion Gate -> WaitingDelivery
    let first_complete = harness
        .complete()
        .expect("complete attempt must return run");
    assert_eq!(
        first_complete.status,
        WorkRunStatus::WaitingDelivery,
        "Run must be WaitingDelivery because analysis.docx is missing"
    );
    assert_run_status(&harness, WorkRunStatus::WaitingDelivery);

    // 6. Agent generates and registers the 3rd required artifact (docx)
    let docx_bytes = golden_fixtures::minimal_docx();
    let docx_full_path = ws_dir.join("output/analysis.docx");
    fs::write(&docx_full_path, &docx_bytes).unwrap();

    let reg_docx_intent = ToolIntent {
        task_id: harness.task.id.clone(),
        work_run_id: harness.run.id.clone(),
        session_id: None,
        workspace_id: harness.workspace.id.clone(),
        tool_call_id: "call-register-analysis-docx".to_string(),
        tool_name: "work_register_artifact".to_string(),
        action: "register".to_string(),
        arguments: serde_json::json!({
            "path": "output/analysis.docx",
            "title": "Written Analysis Document",
            "artifact_type": "docx"
        }),
        input_paths: vec![],
        expected_outputs: vec!["output/analysis.docx".to_string()],
        execution_context: ExecutionContext::Attended,
        policy_revision: None,
    };
    let reg_docx_res = harness.tool(&reg_docx_intent).await.unwrap();
    assert!(reg_docx_res.success);

    // 7. Second Completion Attempt: ALL 3 required artifacts present and valid -> Completed!
    let second_complete = harness
        .complete()
        .expect("second complete attempt must succeed");
    assert_eq!(
        second_complete.status,
        WorkRunStatus::Completed,
        "Run must be Completed after all 3 required artifacts are registered"
    );
    assert_run_status(&harness, WorkRunStatus::Completed);

    // 8. Assertions
    assert_artifact_count(&harness, 3);
    assert_artifact_valid(&harness, "output/sales_summary.xlsx", "xlsx");
    assert_artifact_valid(&harness, "output/management_review.pptx", "pptx");
    assert_artifact_valid(&harness, "output/analysis.docx", "docx");
    assert_no_pending_interactions(&harness);

    let scenario_result = GoldenScenarioResult {
        scenario: "golden_sales_multi_artifact_delivery_gate".to_string(),
        passed: true,
        final_run_status: "completed".to_string(),
        assertions: GoldenScenarioAssertions {
            approval_persisted: false,
            duplicate_executions: 0,
            artifact_satisfied: true,
            pending_interactions: 0,
        },
        details: Some(
            "Blocked at WaitingDelivery with 2/3 artifacts, completed after all 3 registered"
                .to_string(),
        ),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&scenario_result).unwrap()
    );
}

#[tokio::test]
async fn golden_multi_agent_parent_waits_for_all_children() {
    let harness = GoldenHarness::new(
        "ws-multi-agent",
        "Collaborative Strategy Report",
        "Root agent coordinates Researcher, Worker, and Reviewer child subagents",
        WorkExecutionMode::Auto,
    )
    .expect("failed to initialize golden harness");

    let reg = crate::work::subagents::registry();

    // 1. Register 3 running child subagents
    let child1 = reg
        .register_spawn(
            &harness.paths,
            &harness.run.id,
            Some(&harness.workspace.id),
            Some(&harness.task.id),
            RegisterSpawnRequest {
                agent_id: "agent-researcher".to_string(),
                provider_run_id: "prov-researcher-1".to_string(),
                child_index: 0,
                role: "Researcher".to_string(),
                task_digest: "digest-researcher".to_string(),
                launch_contract_digest: "contract-researcher".to_string(),
            },
        )
        .expect("must register researcher spawn");
    assert_eq!(child1.status, "running");

    let child2 = reg
        .register_spawn(
            &harness.paths,
            &harness.run.id,
            Some(&harness.workspace.id),
            Some(&harness.task.id),
            RegisterSpawnRequest {
                agent_id: "agent-worker".to_string(),
                provider_run_id: "prov-worker-1".to_string(),
                child_index: 1,
                role: "Worker".to_string(),
                task_digest: "digest-worker".to_string(),
                launch_contract_digest: "contract-worker".to_string(),
            },
        )
        .expect("must register worker spawn");
    assert_eq!(child2.status, "running");

    let child3 = reg
        .register_spawn(
            &harness.paths,
            &harness.run.id,
            Some(&harness.workspace.id),
            Some(&harness.task.id),
            RegisterSpawnRequest {
                agent_id: "agent-reviewer".to_string(),
                provider_run_id: "prov-reviewer-1".to_string(),
                child_index: 2,
                role: "Reviewer".to_string(),
                task_digest: "digest-reviewer".to_string(),
                launch_contract_digest: "contract-reviewer".to_string(),
            },
        )
        .expect("must register reviewer spawn");
    assert_eq!(child3.status, "running");

    // 2. Root Agent finishes turn with Success settlement
    crate::work::session::handle_turn_settled(
        &harness.emitter,
        &harness.run.id,
        RuntimeTurnOutcome::Success,
    )
    .await
    .expect("handle_turn_settled must succeed");

    // Parent settlement MUST be parked because children are active
    assert!(reg.has_active_children(&harness.run.id));
    assert_run_status(&harness, WorkRunStatus::Running);
    assert_ne!(harness.get_run().status, WorkRunStatus::Completed);

    // 3. Researcher finishes
    reg.update_status(
        &harness.paths,
        &harness.run.id,
        Some(&harness.task.id),
        UpdateStatusRequest {
            agent_id: "agent-researcher".to_string(),
            status: "completed".to_string(),
            summary: Some("Market research concluded".to_string()),
            error: None,
            reason: None,
        },
    )
    .expect("must update researcher status");

    crate::work::session::try_finalize_parent_run(&harness.run.id)
        .await
        .expect("try_finalize_parent_run must succeed");
    // Parent still has Worker and Reviewer active: remains Running
    assert!(reg.has_active_children(&harness.run.id));
    assert_run_status(&harness, WorkRunStatus::Running);
    assert_ne!(harness.get_run().status, WorkRunStatus::Completed);

    // 4. Worker finishes
    reg.update_status(
        &harness.paths,
        &harness.run.id,
        Some(&harness.task.id),
        UpdateStatusRequest {
            agent_id: "agent-worker".to_string(),
            status: "completed".to_string(),
            summary: Some("Data synthesis finished".to_string()),
            error: None,
            reason: None,
        },
    )
    .expect("must update worker status");

    crate::work::session::try_finalize_parent_run(&harness.run.id)
        .await
        .expect("try_finalize_parent_run must succeed");
    // Parent still has Reviewer active: remains Running
    assert!(reg.has_active_children(&harness.run.id));
    assert_run_status(&harness, WorkRunStatus::Running);
    assert_ne!(harness.get_run().status, WorkRunStatus::Completed);

    // 5. Reviewer finishes
    reg.update_status(
        &harness.paths,
        &harness.run.id,
        Some(&harness.task.id),
        UpdateStatusRequest {
            agent_id: "agent-reviewer".to_string(),
            status: "completed".to_string(),
            summary: Some("Executive review approved".to_string()),
            error: None,
            reason: None,
        },
    )
    .expect("must update reviewer status");

    // All children are terminal now
    assert!(!reg.has_active_children(&harness.run.id));

    // 6. Normal parent-finalization path
    crate::work::session::try_finalize_parent_run(&harness.run.id)
        .await
        .expect("try_finalize_parent_run must succeed");

    // Parent is now Completed!
    assert_run_status(&harness, WorkRunStatus::Completed);

    // 7. Assertions
    let facts = harness.facts().expect("failed to list facts");
    let spawned_count = facts
        .iter()
        .filter(|f| matches!(f, RuntimeFact::SubagentSpawned { .. }))
        .count();
    let completed_count = facts
        .iter()
        .filter(|f| matches!(f, RuntimeFact::SubagentCompleted { .. }))
        .count();
    assert_eq!(spawned_count, 3, "SubagentSpawned fact count must be 3");
    assert_eq!(completed_count, 3, "SubagentCompleted fact count must be 3");

    let children_in_scope = reg.list_for_scope(&harness.run.id);
    assert_eq!(children_in_scope.len(), 3);
    assert!(
        children_in_scope.iter().all(|c| c.status == "completed"),
        "All registered children must have terminal status 'completed'"
    );

    let proj = harness.projection().expect("must build work projection");
    assert_eq!(proj.status, WorkRunStatus::Completed);
    assert_eq!(proj.agents.len(), 3);
    assert!(proj.agents.iter().all(|s| s.status == "completed"));

    let scenario_result = GoldenScenarioResult {
        scenario: "golden_multi_agent_parent_waits_for_all_children".to_string(),
        passed: true,
        final_run_status: "completed".to_string(),
        assertions: GoldenScenarioAssertions {
            approval_persisted: false,
            duplicate_executions: 0,
            artifact_satisfied: true,
            pending_interactions: 0,
        },
        details: Some(
            "Parent parked until all 3 children reached terminal status, then completed cleanly"
                .to_string(),
        ),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&scenario_result).unwrap()
    );
}

#[tokio::test]
async fn golden_multi_agent_restart_with_running_child_is_recoverable() {
    let mut harness = GoldenHarness::new(
        "ws-multi-agent-recovery",
        "Multi-Agent Crash Recovery",
        "Parent recovery when process crashes while a child is still running",
        WorkExecutionMode::Auto,
    )
    .expect("failed to initialize golden harness");

    let reg = crate::work::subagents::registry();

    // 1. Spawn 3 children
    reg.register_spawn(
        &harness.paths,
        &harness.run.id,
        Some(&harness.workspace.id),
        Some(&harness.task.id),
        RegisterSpawnRequest {
            agent_id: "agent-researcher".to_string(),
            provider_run_id: "prov-researcher-1".to_string(),
            child_index: 0,
            role: "Researcher".to_string(),
            task_digest: "digest-researcher".to_string(),
            launch_contract_digest: "contract-researcher".to_string(),
        },
    )
    .unwrap();

    reg.register_spawn(
        &harness.paths,
        &harness.run.id,
        Some(&harness.workspace.id),
        Some(&harness.task.id),
        RegisterSpawnRequest {
            agent_id: "agent-worker".to_string(),
            provider_run_id: "prov-worker-1".to_string(),
            child_index: 1,
            role: "Worker".to_string(),
            task_digest: "digest-worker".to_string(),
            launch_contract_digest: "contract-worker".to_string(),
        },
    )
    .unwrap();

    reg.register_spawn(
        &harness.paths,
        &harness.run.id,
        Some(&harness.workspace.id),
        Some(&harness.task.id),
        RegisterSpawnRequest {
            agent_id: "agent-reviewer".to_string(),
            provider_run_id: "prov-reviewer-1".to_string(),
            child_index: 2,
            role: "Reviewer".to_string(),
            task_digest: "digest-reviewer".to_string(),
            launch_contract_digest: "contract-reviewer".to_string(),
        },
    )
    .unwrap();

    // 2. Researcher and Worker reach terminal completed state
    reg.update_status(
        &harness.paths,
        &harness.run.id,
        Some(&harness.task.id),
        UpdateStatusRequest {
            agent_id: "agent-researcher".to_string(),
            status: "completed".to_string(),
            summary: Some("Done".to_string()),
            error: None,
            reason: None,
        },
    )
    .unwrap();

    reg.update_status(
        &harness.paths,
        &harness.run.id,
        Some(&harness.task.id),
        UpdateStatusRequest {
            agent_id: "agent-worker".to_string(),
            status: "completed".to_string(),
            summary: Some("Done".to_string()),
            error: None,
            reason: None,
        },
    )
    .unwrap();

    // Reviewer is STILL RUNNING!
    assert!(reg.has_active_children(&harness.run.id));

    // 3. Simulate App / Process Crash:
    // Memory registry is completely wiped clean!
    crate::work::subagents::registry().reset_for_test();
    crate::work::session::reset_pending_settlements_for_test().await;
    assert!(!crate::work::subagents::registry().has_active_children(&harness.run.id));

    // Durable Ledger remains intact on disk
    let pre_restart_facts = harness.facts().unwrap();
    assert!(pre_restart_facts.iter().any(|f| matches!(f, RuntimeFact::SubagentSpawned { agent_id, .. } if agent_id == "agent-reviewer")));
    assert!(!pre_restart_facts.iter().any(|f| matches!(f, RuntimeFact::SubagentCompleted { agent_id, .. } if agent_id == "agent-reviewer")));

    // 4. Restart & Reconcile
    harness.restart().expect("restart must succeed");

    // 5. Must Assert:
    // Reviewer is identified as interrupted/unfinished from durable ledger
    // Parent != Completed
    // Parent = Recoverable
    assert_run_status(&harness, WorkRunStatus::Recoverable);
    assert_ne!(harness.get_run().status, WorkRunStatus::Completed);

    let post_restart_facts = harness.facts().unwrap();
    let reviewer_interrupted = post_restart_facts.iter().any(|f| match f {
        RuntimeFact::SubagentInterrupted { agent_id, .. } => agent_id == "agent-reviewer",
        _ => false,
    });
    assert!(
        reviewer_interrupted,
        "Reviewer must be recorded as SubagentInterrupted in durable ledger"
    );

    // Verify durable ledger is authority: calling complete_or_fail_run directly MUST FAIL CLOSED
    let direct_complete_attempt = harness
        .controller
        .complete_or_fail_run(
            &harness.task.id,
            &harness.run.id,
            WorkRunStatus::Completed,
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        direct_complete_attempt.status,
        WorkRunStatus::Recoverable,
        "Parent must remain Recoverable and not mistakenly complete when child was interrupted"
    );

    let scenario_result = GoldenScenarioResult {
        scenario: "golden_multi_agent_restart_with_running_child_is_recoverable".to_string(),
        passed: true,
        final_run_status: "recoverable".to_string(),
        assertions: GoldenScenarioAssertions {
            approval_persisted: false,
            duplicate_executions: 0,
            artifact_satisfied: false,
            pending_interactions: 0,
        },
        details: Some("Running child correctly detected as interrupted from ledger; parent marked Recoverable and fail-closed".to_string()),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&scenario_result).unwrap()
    );
}
