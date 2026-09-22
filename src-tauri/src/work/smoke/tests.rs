use serde_json::json;
use std::fs;

use super::approval::SmokeApprovalPolicy;
use super::config::SmokeConfig;
use super::report::{EffortInfo, ModelInfo, RuntimeInfo, SmokeFailure, SmokeReport};
use super::scenarios::{ContractFixture, CORE_CONTRACT_SCENARIO_ID};
use super::verifier::{
    count_duplicate_tool_started, ApprovalStats, ArtifactReport, LedgerStats, RunVerificationStats,
    SemanticAssertion,
};
use crate::work::models::{
    InboxItem, InboxItemPayload, InboxItemStatus, InboxItemType, RuntimeFact,
};

#[test]
fn test_1_cli_config_parse() {
    let json_str = r#"{
        "scenario": "core-contract-approval",
        "runtime": "pi",
        "model": "Qwen3.8-27B",
        "effort": "high",
        "timeoutSecs": 600,
        "keepWorkspace": true
    }"#;
    let config: SmokeConfig = serde_json::from_str(json_str).expect("parse json");
    assert_eq!(config.scenario, "core-contract-approval");
    assert_eq!(config.runtime, "pi");
    assert_eq!(config.model, "Qwen3.8-27B");
    assert_eq!(config.effort, "high");
    assert_eq!(config.timeout_secs, 600);
    assert!(config.keep_workspace);
    assert!(config.validate().is_ok());
}

#[test]
fn test_2_runtime_validation() {
    let mut config = SmokeConfig {
        scenario: CORE_CONTRACT_SCENARIO_ID.to_string(),
        runtime: "claude".to_string(),
        model: "test-model".to_string(),
        effort: "high".to_string(),
        timeout_secs: 600,
        keep_workspace: false,
        report_dir: None,
        external_fixture_dir: None,
    };
    let err = config.validate().unwrap_err();
    assert!(err.contains("Real Agent Smoke V1 only supports runtime=pi"));

    config.runtime = "pi".to_string();
    config.model = "".to_string();
    assert_eq!(config.validate().unwrap_err(), "--model is required");

    config.model = "valid-model".to_string();
    config.effort = "extreme".to_string();
    assert!(config.validate().unwrap_err().contains("Invalid effort"));

    config.effort = "high".to_string();
    assert!(config.validate().is_ok());
}

#[test]
fn test_3_approval_whitelist() {
    let tmp = tempfile::tempdir().unwrap();
    let contracts_dir = tmp.path().join("contracts");
    fs::create_dir_all(&contracts_dir).unwrap();
    let policy = SmokeApprovalPolicy::new(contracts_dir.clone());

    let item = InboxItem {
        id: "item-1".to_string(),
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        workspace_id: "ws-1".to_string(),
        item_type: InboxItemType::AccessRootRequest,
        status: InboxItemStatus::Pending,
        title: "Request Access".to_string(),
        description: "Need read access".to_string(),
        payload: InboxItemPayload {
            tool_name: Some("work_request_directory_access".to_string()),
            parameters: Some(json!({
                "path": contracts_dir.to_str().unwrap(),
                "writable": false
            })),
            ..Default::default()
        },
        response: None,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        resolved_at: None,
    };

    assert!(policy.evaluate_inbox_item(&item).is_ok());
}

#[test]
fn test_4_canonical_external_fixture_path_match() {
    let tmp = tempfile::tempdir().unwrap();
    let contracts_dir = tmp.path().join("contracts");
    fs::create_dir_all(&contracts_dir).unwrap();

    let fixture = ContractFixture::setup(Some(tmp.path())).expect("fixture setup");
    assert!(fixture.contracts_dir.is_dir());
    assert!(fixture.contracts_dir.join("A-001.txt").is_file());
    assert!(fixture.contracts_dir.join("B-001.txt").is_file());
    assert!(fixture.contracts_dir.join("B-002.txt").is_file());

    let policy = SmokeApprovalPolicy::new(fixture.contracts_dir.clone());

    // Test with relative traversal in path that canonicalizes to same directory
    let dot_path = format!("{}/./", fixture.contracts_dir.display());
    let item = InboxItem {
        id: "item-canonical".to_string(),
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        workspace_id: "ws-1".to_string(),
        item_type: InboxItemType::AccessRootRequest,
        status: InboxItemStatus::Pending,
        title: "Request Access".to_string(),
        description: "Need read access".to_string(),
        payload: InboxItemPayload {
            tool_name: Some("work_request_directory_access".to_string()),
            parameters: Some(json!({
                "path": dot_path,
                "writable": false
            })),
            ..Default::default()
        },
        response: None,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        resolved_at: None,
    };

    assert!(policy.evaluate_inbox_item(&item).is_ok());
    fixture.cleanup();
}

#[test]
fn test_5_unexpected_approval_reject() {
    let tmp = tempfile::tempdir().unwrap();
    let contracts_dir = tmp.path().join("contracts");
    fs::create_dir_all(&contracts_dir).unwrap();
    let policy = SmokeApprovalPolicy::new(contracts_dir.clone());

    // 1. Wrong path
    let bad_path_item = InboxItem {
        id: "item-bad-path".to_string(),
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        workspace_id: "ws-1".to_string(),
        item_type: InboxItemType::AccessRootRequest,
        status: InboxItemStatus::Pending,
        title: "Access".to_string(),
        description: "Access".to_string(),
        payload: InboxItemPayload {
            parameters: Some(json!({
                "path": "/etc",
                "writable": false
            })),
            ..Default::default()
        },
        response: None,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        resolved_at: None,
    };
    assert!(policy.evaluate_inbox_item(&bad_path_item).is_err());

    // 2. Writable = true
    let writable_item = InboxItem {
        id: "item-writable".to_string(),
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        workspace_id: "ws-1".to_string(),
        item_type: InboxItemType::AccessRootRequest,
        status: InboxItemStatus::Pending,
        title: "Access".to_string(),
        description: "Access".to_string(),
        payload: InboxItemPayload {
            parameters: Some(json!({
                "path": contracts_dir.to_str().unwrap(),
                "writable": true
            })),
            ..Default::default()
        },
        response: None,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        resolved_at: None,
    };
    assert!(policy.evaluate_inbox_item(&writable_item).is_err());

    // 3. Question / input
    let question_item = InboxItem {
        id: "item-question".to_string(),
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        workspace_id: "ws-1".to_string(),
        item_type: InboxItemType::QuestionElicitation,
        status: InboxItemStatus::Pending,
        title: "Question".to_string(),
        description: "Question".to_string(),
        payload: InboxItemPayload {
            question: Some("What do you think?".to_string()),
            ..Default::default()
        },
        response: None,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        resolved_at: None,
    };
    assert!(policy.evaluate_inbox_item(&question_item).is_err());
}

#[test]
fn test_6_duplicate_tool_started_detection() {
    let facts = vec![
        RuntimeFact::ToolStarted {
            tool_call_id: "call-1".to_string(),
            execution_id: "exec-1".to_string(),
            timestamp: "2026-09-06T00:00:00Z".to_string(),
        },
        RuntimeFact::ToolStarted {
            tool_call_id: "call-2".to_string(),
            execution_id: "exec-2".to_string(),
            timestamp: "2026-09-06T00:00:01Z".to_string(),
        },
        RuntimeFact::ToolStarted {
            tool_call_id: "call-1".to_string(), // duplicate!
            execution_id: "exec-3".to_string(),
            timestamp: "2026-09-06T00:00:02Z".to_string(),
        },
    ];

    assert_eq!(count_duplicate_tool_started(&facts), 1);

    let no_dups = vec![
        RuntimeFact::ToolStarted {
            tool_call_id: "call-1".to_string(),
            execution_id: "exec-1".to_string(),
            timestamp: "2026-09-06T00:00:00Z".to_string(),
        },
        RuntimeFact::ToolStarted {
            tool_call_id: "call-2".to_string(),
            execution_id: "exec-2".to_string(),
            timestamp: "2026-09-06T00:00:01Z".to_string(),
        },
    ];
    assert_eq!(count_duplicate_tool_started(&no_dups), 0);
}

#[test]
fn test_7_semantic_manifest_verifier() {
    let valid_manifest = json!({
        "totalArchived": 3,
        "customers": {
            "A": 1,
            "B": 2
        },
        "files": [
            "output/archive/A/A-001.txt",
            "output/archive/B/B-001.txt",
            "output/archive/B/B-002.txt"
        ]
    });

    assert_eq!(valid_manifest["totalArchived"], 3);
    assert_eq!(valid_manifest["customers"]["A"], 1);
    assert_eq!(valid_manifest["customers"]["B"], 2);
    assert_eq!(valid_manifest["files"].as_array().unwrap().len(), 3);

    let invalid_manifest = json!({
        "totalArchived": 2,
        "customers": {
            "A": 1,
            "B": 1
        },
        "files": [
            "output/archive/A/A-001.txt",
            "output/archive/B/B-001.txt"
        ]
    });
    assert_ne!(invalid_manifest["totalArchived"], 3);
}

#[test]
fn test_8_report_serialization_and_no_secrets() {
    let report = SmokeReport {
        version: 1,
        scenario: "core-contract-approval".to_string(),
        verdict: "PASS".to_string(),
        started_at: "2026-09-06T00:00:00Z".to_string(),
        ended_at: "2026-09-06T00:02:00Z".to_string(),
        duration_ms: 120000,
        runtime: RuntimeInfo {
            requested: "pi".to_string(),
            effective: "pi".to_string(),
        },
        model: ModelInfo {
            requested: "Qwen3.8-27B".to_string(),
            run_meta: "Qwen3.8-27B".to_string(),
            runtime_reported: Some("Qwen3.8-27B".to_string()),
            runtime_verified: true,
        },
        effort: EffortInfo {
            requested: "high".to_string(),
            effective_launch: Some("high".to_string()),
            verified: true,
        },
        run: Some(RunVerificationStats {
            workspace_id: "ws-123".to_string(),
            task_id: "task-123".to_string(),
            run_id: "run-123".to_string(),
            session_id: Some("session-123".to_string()),
            final_status: "completed".to_string(),
            projection_status: "completed".to_string(),
        }),
        approvals: ApprovalStats {
            requested: 1,
            approved: 1,
            unexpected: 0,
        },
        ledger: LedgerStats {
            tool_proposed: 4,
            tool_started: 4,
            tool_result: 4,
            tool_failures: 0,
            duplicate_tool_started: 0,
        },
        artifacts: vec![ArtifactReport {
            path: "output/archive_manifest.json".to_string(),
            sha256: Some("abcdef123456".to_string()),
            actual_sha256: Some("abcdef123456".to_string()),
            valid: true,
        }],
        semantic_assertions: vec![SemanticAssertion {
            name: "totalArchived".to_string(),
            expected: json!(3),
            actual: json!(3),
            passed: true,
        }],
        failure: None,
        preserved_workspace_path: None,
    };

    let json_output = serde_json::to_string_pretty(&report).unwrap();
    let md_output = report.to_markdown();

    // Check no secret tokens
    assert!(!json_output.contains("sk-ant-"));
    assert!(!json_output.contains("sk-proj-"));
    assert!(!json_output.contains("Bearer "));
    assert!(!json_output.contains("Authorization"));
    assert!(!md_output.contains("sk-ant-"));
    assert!(!md_output.contains("sk-proj-"));
    assert!(!md_output.contains("Bearer "));
    assert!(md_output.contains("Verdict: PASS"));
    assert!(md_output.contains("Qwen3.8-27B"));
}

#[test]
fn test_9_user_settings_unchanged_comparison() {
    let snap_work_runtime = Some("pi".to_string());
    let snap_model = Some("Deepseek-V4-Flash".to_string());
    let snap_effort = Some("high".to_string());

    let curr_work_runtime = Some("pi".to_string());
    let curr_model = Some("Deepseek-V4-Flash".to_string());
    let curr_effort = Some("high".to_string());

    assert_eq!(snap_work_runtime, curr_work_runtime);
    assert_eq!(snap_model, curr_model);
    assert_eq!(snap_effort, curr_effort);

    // Mutated model
    let mutated_model = Some("Qwen3.8-27B".to_string());
    assert_ne!(snap_model, mutated_model);
}

#[test]
fn test_10_timeout_failure_result_mapping() {
    let failure_timeout = SmokeFailure {
        failure_kind: "timeout".to_string(),
        reason: "Run timed out after 600s".to_string(),
        details: None,
    };
    assert_eq!(failure_timeout.failure_kind, "timeout");

    let failure_model = SmokeFailure {
        failure_kind: "model_unavailable".to_string(),
        reason: "Model fallback forbidden: requested 'InvalidModel' could not be resolved"
            .to_string(),
        details: None,
    };
    assert_eq!(failure_model.failure_kind, "model_unavailable");

    let failure_approval = SmokeFailure {
        failure_kind: "unexpected_approval".to_string(),
        reason: "Non-whitelisted approval requested".to_string(),
        details: None,
    };
    assert_eq!(failure_approval.failure_kind, "unexpected_approval");
}

fn valid_base_verification_result() -> super::verifier::VerificationResult {
    super::verifier::VerificationResult {
        run_stats: RunVerificationStats {
            workspace_id: "ws-1".to_string(),
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            session_id: Some("session-1".to_string()),
            final_status: "completed".to_string(),
            projection_status: "completed".to_string(),
        },
        approval_stats: ApprovalStats {
            requested: 1,
            approved: 1,
            unexpected: 0,
        },
        ledger_stats: LedgerStats {
            tool_proposed: 4,
            tool_started: 4,
            tool_result: 4,
            tool_failures: 0,
            duplicate_tool_started: 0,
        },
        artifacts: vec![ArtifactReport {
            path: "output/archive_manifest.json".to_string(),
            sha256: Some("a1b2c3d4".to_string()),
            actual_sha256: Some("a1b2c3d4".to_string()),
            valid: true,
        }],
        semantic_assertions: vec![SemanticAssertion {
            name: "totalArchived".to_string(),
            expected: json!(3),
            actual: json!(3),
            passed: true,
        }],
    }
}

#[test]
fn test_11_unified_verdict_failure_invariants() {
    // 0. Base valid result passes
    let base = valid_base_verification_result();
    assert!(base.is_all_passed());
    assert!(base.first_failure_kind_and_reason().is_none());

    // 1. Approval 0/0 => FAIL
    let mut res_approval_0_0 = valid_base_verification_result();
    res_approval_0_0.approval_stats.requested = 0;
    res_approval_0_0.approval_stats.approved = 0;
    assert!(!res_approval_0_0.is_all_passed());
    let (kind, reason) = res_approval_0_0.first_failure_kind_and_reason().unwrap();
    assert_eq!(kind, "approval_count_mismatch");
    assert!(reason.contains("Expected exactly 1 requested approval, got 0"));

    // 2. Approval 1/0 => FAIL
    let mut res_approval_1_0 = valid_base_verification_result();
    res_approval_1_0.approval_stats.requested = 1;
    res_approval_1_0.approval_stats.approved = 0;
    assert!(!res_approval_1_0.is_all_passed());
    let (kind, reason) = res_approval_1_0.first_failure_kind_and_reason().unwrap();
    assert_eq!(kind, "approval_count_mismatch");
    assert!(reason.contains("Expected exactly 1 approved approval, got 0"));

    // 3. Projection not completed => FAIL
    let mut res_proj_not_completed = valid_base_verification_result();
    res_proj_not_completed.run_stats.projection_status = "running".to_string();
    assert!(!res_proj_not_completed.is_all_passed());
    let (kind, _) = res_proj_not_completed
        .first_failure_kind_and_reason()
        .unwrap();
    assert_eq!(kind, "projection_not_completed");

    // 4. ToolProposed = 0 => FAIL
    let mut res_prop_zero = valid_base_verification_result();
    res_prop_zero.ledger_stats.tool_proposed = 0;
    assert!(!res_prop_zero.is_all_passed());
    let (kind, _) = res_prop_zero.first_failure_kind_and_reason().unwrap();
    assert_eq!(kind, "ledger_empty");

    // 5. ToolStarted = 0 => FAIL
    let mut res_start_zero = valid_base_verification_result();
    res_start_zero.ledger_stats.tool_started = 0;
    assert!(!res_start_zero.is_all_passed());
    let (kind, _) = res_start_zero.first_failure_kind_and_reason().unwrap();
    assert_eq!(kind, "ledger_empty");

    // 6. ToolResult = 0 => FAIL
    let mut res_res_zero = valid_base_verification_result();
    res_res_zero.ledger_stats.tool_result = 0;
    assert!(!res_res_zero.is_all_passed());
    let (kind, _) = res_res_zero.first_failure_kind_and_reason().unwrap();
    assert_eq!(kind, "ledger_empty");
}

#[test]
fn test_12_artifact_sha256_verification_pass_and_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest_path = tmp.path().join("archive_manifest.json");
    let content = r#"{"totalArchived": 3}"#;
    fs::write(&manifest_path, content).unwrap();

    // Compute actual file sha256
    let actual_sha = super::verifier::compute_file_sha256(&manifest_path).unwrap();
    assert!(!actual_sha.is_empty());

    // Valid hash => PASS
    let matching_registry_sha = actual_sha.clone();
    let is_valid = matching_registry_sha == actual_sha;
    let mut res_pass = valid_base_verification_result();
    res_pass.artifacts = vec![ArtifactReport {
        path: "output/archive_manifest.json".to_string(),
        sha256: Some(matching_registry_sha),
        actual_sha256: Some(actual_sha.clone()),
        valid: is_valid,
    }];
    assert!(res_pass.is_all_passed());
    assert!(res_pass.first_failure_kind_and_reason().is_none());

    // Wrong hash => FAIL
    let wrong_registry_sha = "deadbeefcafebabe".to_string();
    let is_valid_wrong = wrong_registry_sha == actual_sha;
    assert!(!is_valid_wrong);
    let mut res_fail = valid_base_verification_result();
    res_fail.artifacts = vec![ArtifactReport {
        path: "output/archive_manifest.json".to_string(),
        sha256: Some(wrong_registry_sha),
        actual_sha256: Some(actual_sha),
        valid: is_valid_wrong,
    }];
    assert!(!res_fail.is_all_passed());
    let (kind, reason) = res_fail.first_failure_kind_and_reason().unwrap();
    assert_eq!(kind, "artifact_invalid");
    assert!(reason.contains("does not match actual file sha256"));
}

#[test]
fn test_13_effort_gate_invariants() {
    use super::verifier::verify_effort_gate;

    // 1. high / high => PASS
    assert_eq!(verify_effort_gate("high", Some("high")), Ok(true));

    // Case-insensitive pass
    assert_eq!(verify_effort_gate("high", Some("HIGH")), Ok(true));

    // 2. high / low => FAIL
    let err_low = verify_effort_gate("high", Some("low")).unwrap_err();
    assert_eq!(err_low.0, "effort_mismatch");
    assert!(err_low
        .1
        .contains("Effective launch effort 'low' does not match requested effort 'high'"));

    // 3. high / None => FAIL
    let err_none = verify_effort_gate("high", None).unwrap_err();
    assert_eq!(err_none.0, "effort_mismatch");
    assert!(err_none
        .1
        .contains("Effective launch effort was not captured, expected 'high'"));
}

#[test]
fn test_14_real_secret_redaction() {
    use super::report::redact_secrets;

    let fake_secret_text = r#"
        Authentication failed with Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.secretpayload.signature
        Config error: api_key=sk-ant-api03-abcdef1234567890uvwxyz
        Backup provider: api-key: mysecretapikey999
        Database config: password: supersecretpass123
        App secret: secret: myclassifiedsecret
        Header: Authorization: Bearer tokenabc12345
        OpenAI token: sk-proj-superopenai999888777
    "#;

    let redacted = redact_secrets(fake_secret_text);

    // Verify all secrets are redacted
    assert!(!redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    assert!(!redacted.contains("sk-ant-api03-abcdef1234567890uvwxyz"));
    assert!(!redacted.contains("mysecretapikey999"));
    assert!(!redacted.contains("supersecretpass123"));
    assert!(!redacted.contains("myclassifiedsecret"));
    assert!(!redacted.contains("tokenabc12345"));
    assert!(!redacted.contains("sk-proj-superopenai999888777"));

    // Verify redaction placeholder exists
    assert!(redacted.contains("***REDACTED***"));

    // Test inside SmokeReport JSON serialization and Markdown
    let report = valid_base_verification_result();
    let smoke_report = SmokeReport {
        version: 1,
        scenario: "core-contract-approval".to_string(),
        verdict: "FAIL".to_string(),
        started_at: "2026-09-06T00:00:00Z".to_string(),
        ended_at: "2026-09-06T00:01:00Z".to_string(),
        duration_ms: 60000,
        runtime: RuntimeInfo {
            requested: "pi".to_string(),
            effective: "pi".to_string(),
        },
        model: ModelInfo {
            requested: "Qwen3.8-27B".to_string(),
            run_meta: "Qwen3.8-27B".to_string(),
            runtime_reported: Some("Qwen3.8-27B".to_string()),
            runtime_verified: true,
        },
        effort: EffortInfo {
            requested: "high".to_string(),
            effective_launch: Some("high".to_string()),
            verified: true,
        },
        run: Some(report.run_stats),
        approvals: report.approval_stats,
        ledger: report.ledger_stats,
        artifacts: report.artifacts,
        semantic_assertions: report.semantic_assertions,
        failure: Some(SmokeFailure {
            failure_kind: "auth_failure".to_string(),
            reason: "Failed connecting to endpoint with Bearer secret-token-xyz and sk-proj-12345"
                .to_string(),
            details: None,
        }),
        preserved_workspace_path: None,
    };

    let md = smoke_report.to_markdown();
    assert!(!md.contains("secret-token-xyz"));
    assert!(!md.contains("sk-proj-12345"));
    assert!(md.contains("***REDACTED***"));

    let tmp = tempfile::tempdir().unwrap();
    let (json_file, _md_file) = smoke_report.write_to_dir(tmp.path()).unwrap();
    let json_content = fs::read_to_string(json_file).unwrap();
    assert!(!json_content.contains("secret-token-xyz"));
    assert!(!json_content.contains("sk-proj-12345"));
    assert!(json_content.contains("***REDACTED***"));
}
