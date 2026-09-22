use super::*;
use crate::work::interaction::InteractionManager;
use crate::work::models::{
    PendingInteractionKind, PendingInteractionState, WorkExecutionMode, WorkPolicy, WorkRunStatus,
    WorkRunTrigger,
};
use crate::work::tasks::TaskManager;
use crate::work::workspace::WorkspaceManager;
use tempfile::TempDir;

fn intent_with_args(args: serde_json::Value) -> ToolIntent {
    ToolIntent {
        task_id: "t".to_string(),
        work_run_id: "r".to_string(),
        session_id: None,
        workspace_id: "ws".to_string(),
        tool_call_id: "c".to_string(),
        tool_name: "work_execute".to_string(),
        action: "export_table".to_string(),
        arguments: args,
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    }
}

#[tokio::test]
async fn ask_questions_persists_answer_and_reuses_the_same_interaction() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();
    let pipeline = ToolPipeline::new(paths.clone());
    let intent = ToolIntent {
        task_id: "task-ask-questions".to_string(),
        work_run_id: "run-ask-questions".to_string(),
        session_id: None,
        workspace_id: "workspace-ask-questions".to_string(),
        tool_call_id: "call-ask-questions".to_string(),
        tool_name: "ask_questions".to_string(),
        action: "ask".to_string(),
        arguments: serde_json::json!({
            "title": "Choose a mode",
            "questions": [{
                "id": "mode",
                "question": "Which mode should we use?",
                "options": [{ "label": "Fast", "value": "fast" }]
            }]
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let policy = WorkPolicy {
        execution_mode: WorkExecutionMode::PlanFirst,
        ..Default::default()
    };

    let pending = pipeline.execute_intent(&intent, &policy).await.unwrap();
    assert_eq!(pending.status, "waiting_input");
    let interaction_id = pending.interaction_id.clone().unwrap();
    let manager = InteractionManager::new(paths.clone());
    let interaction = manager.get_interaction(&interaction_id).unwrap();
    assert_eq!(interaction.kind, PendingInteractionKind::UserInput);
    assert!(!paths
        .inbox_dir()
        .join(format!("{interaction_id}.grant.json"))
        .exists());

    let answer = serde_json::json!({
        "answers": [{ "id": "mode", "type": "select", "value": "fast" }]
    });
    let (resolved, first_responder) = manager
        .resolve_interaction_once(
            &interaction_id,
            PendingInteractionState::Resolved,
            Some(answer.clone()),
        )
        .unwrap();
    assert!(first_responder);
    assert_eq!(resolved.resolution, Some(answer));

    let completed = pipeline.execute_intent(&intent, &policy).await.unwrap();
    assert!(completed.success);
    assert_eq!(completed.status, "success");
    assert_eq!(
        completed.interaction_id.as_deref(),
        Some(interaction_id.as_str())
    );
    let stdout: serde_json::Value = serde_json::from_str(&completed.stdout).unwrap();
    assert_eq!(stdout["answers"][0]["value"], "fast");
}

#[tokio::test]
async fn host_mcp_call_uses_pipeline_and_returns_server_result() {
    use axum::{routing::post, Json, Router};
    use tokio::net::TcpListener;

    async fn handler(Json(request): Json<serde_json::Value>) -> Json<serde_json::Value> {
        let result = if request.get("method").and_then(|value| value.as_str()) == Some("tools/call")
        {
            serde_json::json!({
                "content": [{ "type": "text", "text": "pipeline-host-ok" }]
            })
        } else {
            serde_json::json!({ "capabilities": {} })
        };
        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(serde_json::Value::Null),
            "result": result
        }))
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", post(handler)))
            .await
            .unwrap();
    });

    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();
    std::fs::write(
        paths.work_mcp_config_path(),
        serde_json::json!({
            "mcpServers": {
                "http": { "type": "streamable-http", "url": format!("http://{address}/mcp") }
            }
        })
        .to_string(),
    )
    .unwrap();

    let intent = ToolIntent {
        task_id: "task-mcp-pipeline".to_string(),
        work_run_id: "run-mcp-pipeline".to_string(),
        session_id: None,
        workspace_id: String::new(),
        tool_call_id: "call-mcp-pipeline".to_string(),
        tool_name: "work_mcp_call".to_string(),
        action: "http_echo".to_string(),
        arguments: serde_json::json!({
            "server": "http",
            "tool_name": "http_echo",
            "arguments": { "x": 1 },
            "target": "mcp://http/http_echo"
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let policy = WorkPolicy {
        execution_mode: WorkExecutionMode::FullAccess,
        allow_external_connectors: true,
        ..Default::default()
    };

    let result = ToolPipeline::new(paths)
        .execute_intent(&intent, &policy)
        .await
        .unwrap();
    server.abort();

    assert!(result.success, "MCP pipeline failed: {}", result.stderr);
    let result_json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(result_json["content"][0]["text"], "pipeline-host-ok");
}

#[test]
fn standing_rule_pattern_covers_capability_and_directory_scopes() {
    // work_execute with resource_id -> capability-level exemption
    let intent = intent_with_args(serde_json::json!({"resource_id": "work-excel"}));
    assert_eq!(
        ToolPipeline::standing_rule_pattern(&intent),
        Some("work-excel.*".to_string())
    );

    // camelCase resourceId variant
    let intent = intent_with_args(serde_json::json!({"resourceId": "work-data-analysis"}));
    assert_eq!(
        ToolPipeline::standing_rule_pattern(&intent),
        Some("work-data-analysis.*".to_string())
    );

    // File path -> parent directory glob
    let intent = intent_with_args(serde_json::json!({"path": "scratch/sales.csv"}));
    assert_eq!(
        ToolPipeline::standing_rule_pattern(&intent),
        Some("scratch/*".to_string())
    );

    // Nested path keeps full parent directory
    let intent = intent_with_args(serde_json::json!({"path": "scratch/runs/abc/notes.md"}));
    assert_eq!(
        ToolPipeline::standing_rule_pattern(&intent),
        Some("scratch/runs/abc/*".to_string())
    );

    // Root-level file -> too broad, no proposal
    let intent = intent_with_args(serde_json::json!({"path": "notes.md"}));
    assert_eq!(ToolPipeline::standing_rule_pattern(&intent), None);

    // Free-form command target -> no safe pattern
    let intent = intent_with_args(serde_json::json!({"target": "ls -la"}));
    assert_eq!(ToolPipeline::standing_rule_pattern(&intent), None);
}

#[test]
fn browser_tab_creation_and_close_are_not_read_only_in_unattended_runs() {
    let temp = TempDir::new().unwrap();
    let pipeline = ToolPipeline::new(WorkPaths::new(temp.path().join("data")));
    let base = |action: &str| ToolIntent {
        task_id: "t".to_string(),
        work_run_id: "r".to_string(),
        session_id: None,
        workspace_id: "ws".to_string(),
        tool_call_id: format!("call-{action}"),
        tool_name: "browser_tabs".to_string(),
        action: action.to_string(),
        arguments: serde_json::json!({"action": action}),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Unattended,
        policy_revision: None,
    };

    assert_eq!(
        pipeline.effective_risk_class(&base("list")),
        ToolRiskClass::Read
    );
    assert_eq!(
        pipeline.effective_risk_class(&base("switch")),
        ToolRiskClass::Read
    );
    assert_eq!(
        pipeline.effective_risk_class(&base("new")),
        ToolRiskClass::WriteLocal
    );
    assert_eq!(
        pipeline.effective_risk_class(&base("close")),
        ToolRiskClass::WriteLocal
    );
}

#[test]
fn browser_interactive_writes_require_confirmation_without_an_opt_in_rule() {
    let intent = |tool_name: &str, action: &str, arguments: serde_json::Value| ToolIntent {
        task_id: "t".to_string(),
        work_run_id: "r".to_string(),
        session_id: None,
        workspace_id: "ws".to_string(),
        tool_call_id: format!("call-{tool_name}-{action}"),
        tool_name: tool_name.to_string(),
        action: action.to_string(),
        arguments,
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    assert!(ToolPipeline::browser_interaction_requires_confirmation(
        &intent("browser_click", "click", serde_json::json!({"ref": "e1"}),)
    ));
    assert!(ToolPipeline::browser_interaction_requires_confirmation(
        &intent(
            "browser_type",
            "type",
            serde_json::json!({"selector": "input[name='email']", "text": "user@example.com"}),
        )
    ));
    assert!(ToolPipeline::browser_interaction_requires_confirmation(
        &intent("browser_tabs", "new", serde_json::json!({"action": "new"}),)
    ));
    assert!(!ToolPipeline::browser_interaction_requires_confirmation(
        &intent(
            "browser_tabs",
            "list",
            serde_json::json!({"action": "list"}),
        )
    ));
    assert!(!ToolPipeline::browser_interaction_requires_confirmation(
        &intent("browser_snapshot", "snapshot", serde_json::json!({}),)
    ));
}

#[test]
fn browser_interactive_writes_auto_allowed_in_auto_mode_and_asked_in_direct_mode() {
    use crate::work::models::{ExecutionContext, WorkExecutionMode, WorkPolicy};
    use crate::work::policy::{PolicyEvaluator, WorkPolicyDecision};

    let auto_policy = WorkPolicy {
        execution_mode: WorkExecutionMode::Auto,
        ..Default::default()
    };
    let direct_policy = WorkPolicy {
        execution_mode: WorkExecutionMode::Direct,
        ..Default::default()
    };
    let plan_policy = WorkPolicy {
        execution_mode: WorkExecutionMode::PlanFirst,
        ..Default::default()
    };

    // browser_click in attended Auto mode is allowed
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &auto_policy,
            "browser_click",
            "click",
            ExecutionContext::Attended
        ),
        WorkPolicyDecision::Allow
    );
    // browser_type in attended Auto mode is allowed
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &auto_policy,
            "browser_type",
            "type",
            ExecutionContext::Attended
        ),
        WorkPolicyDecision::Allow
    );
    // browser_click in Direct mode asks for confirmation
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &direct_policy,
            "browser_click",
            "click",
            ExecutionContext::Attended
        ),
        WorkPolicyDecision::Ask
    );
    // browser_click in PlanFirst mode is denied (read-only)
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &plan_policy,
            "browser_click",
            "click",
            ExecutionContext::Attended
        ),
        WorkPolicyDecision::Deny
    );
    // browser_click in Unattended mode asks for confirmation
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &auto_policy,
            "browser_click",
            "click",
            ExecutionContext::Unattended
        ),
        WorkPolicyDecision::Ask
    );
}

#[test]
fn desktop_operator_tools_are_exclusive_and_follow_work_modes() {
    use crate::work::models::{ExecutionContext, WorkExecutionMode, WorkPolicy};
    use crate::work::policy::WorkPolicyDecision;

    assert_eq!(
        ToolPipeline::concurrency_class("desktop_click"),
        crate::work::models::ToolConcurrencyClass::Exclusive
    );
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &WorkPolicy {
                execution_mode: WorkExecutionMode::Auto,
                ..Default::default()
            },
            "desktop_observe",
            "observe",
            ExecutionContext::Attended,
        ),
        WorkPolicyDecision::Allow
    );
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &WorkPolicy {
                execution_mode: WorkExecutionMode::Direct,
                ..Default::default()
            },
            "desktop_click",
            "click",
            ExecutionContext::Attended,
        ),
        WorkPolicyDecision::Ask
    );
    assert_eq!(
        PolicyEvaluator::evaluate_decision(
            &WorkPolicy {
                execution_mode: WorkExecutionMode::PlanFirst,
                ..Default::default()
            },
            "desktop_click",
            "click",
            ExecutionContext::Attended,
        ),
        WorkPolicyDecision::Deny
    );
}

#[test]
fn direct_lark_cli_commands_are_redirected_to_connector_package() {
    assert!(ToolPipeline::direct_connector_cli_guard("lark-cli").is_some());
    assert!(ToolPipeline::direct_connector_cli_guard("/opt/homebrew/bin/lark-cli").is_some());
    assert!(ToolPipeline::direct_connector_cli_guard("lark-cli.cmd").is_some());
    assert!(ToolPipeline::direct_connector_cli_guard("printf").is_none());
}

#[test]
fn denial_reason_attributes_read_only_plan_denial_to_mode_not_target() {
    use crate::work::models::WorkExecutionMode;

    // A write in read-only (PlanFirst) mode must blame the mode, never the file name.
    let intent = ToolIntent {
        task_id: "t".to_string(),
        work_run_id: "r".to_string(),
        session_id: None,
        workspace_id: "ws".to_string(),
        tool_call_id: "call-1".to_string(),
        tool_name: "work_write_file".to_string(),
        action: "write".to_string(),
        arguments: serde_json::json!({"path": "output/readm99999.txt", "content": "123"}),
        input_paths: Vec::new(),
        expected_outputs: vec!["output/readm99999.txt".to_string()],
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let read_only = WorkPolicy {
        execution_mode: WorkExecutionMode::PlanFirst,
        ..Default::default()
    };
    let reason = ToolPipeline::denial_reason(&read_only, &intent, ToolRiskClass::WriteLocal, true);
    assert!(
        reason.contains("read-only"),
        "denial must cite read-only mode, got: {reason}"
    );
    assert!(
        reason.contains("writable mode") || reason.contains("Direct or Auto"),
        "denial must steer toward switching mode, got: {reason}"
    );
    assert!(
        reason.contains("not from the file name or path"),
        "denial must make clear the cause is mode-level, not per-file, got: {reason}"
    );
}

#[test]
fn denial_reason_falls_back_to_generic_when_not_mode_based() {
    use crate::work::models::WorkExecutionMode;
    let intent = intent_with_args(serde_json::json!({}));
    let reason = ToolPipeline::denial_reason(
        &WorkPolicy {
            execution_mode: WorkExecutionMode::Direct,
            ..Default::default()
        },
        &intent,
        ToolRiskClass::External,
        true,
    );
    // External risk → connector-specific message
    assert!(reason.contains("external connector"), "got: {reason}");
    assert!(
        reason.contains("allow_external_connectors"),
        "got: {reason}"
    );
}

#[tokio::test]
async fn directory_access_rejects_relative_input_path_with_no_inbox_items() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();
    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test task", "Goal", Some(WorkPolicy::default()))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let pipeline = ToolPipeline::new(paths.clone());

    // 1. Request access with relative input path "input/Q2-sales.xlsx"
    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-rel-input".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": "input/Q2-sales.xlsx",
            "writable": false
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let res = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();

    // Must return error, NOT waiting_approval
    assert!(!res.success);
    assert_eq!(res.status, "error");
    assert!(res.stderr.contains("relative path") || res.stderr.contains("input/"));
    assert!(res.interaction_id.is_none());

    // Zero interactions created
    let interaction_mgr = InteractionManager::new(paths.clone());
    let items = interaction_mgr.list_pending(Some(&task.id), None).unwrap();
    assert_eq!(
        items.len(),
        0,
        "No Inbox items must be created for relative workspace path"
    );

    // Run must NOT transition to WaitingApproval
    let current_run = task_mgr.get_run(&task.id, &run.id).unwrap();
    assert_ne!(current_run.status, WorkRunStatus::WaitingApproval);
}

#[tokio::test]
async fn standalone_directory_access_returns_clear_error_without_inbox_item() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();
    let run_id = "standalone-access-run";
    paths.ensure_standalone_task_dir(run_id).unwrap();

    let external_dir = temp.path().join("downloads");
    std::fs::create_dir_all(&external_dir).unwrap();
    let pipeline = ToolPipeline::new(paths.clone());
    let intent = ToolIntent {
        task_id: run_id.to_string(),
        work_run_id: run_id.to_string(),
        session_id: None,
        workspace_id: String::new(),
        tool_call_id: "call-standalone-external".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": external_dir.to_string_lossy(),
            "writable": true
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let result = pipeline
        .execute_intent(&intent, &WorkPolicy::default())
        .await
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.status, "error");
    assert!(result.stderr.contains("requires a Workspace"));
    assert!(result.interaction_id.is_none());

    let inbox = InteractionManager::new(paths);
    assert!(inbox.list_pending(Some(run_id), None).unwrap().is_empty());
}

#[tokio::test]
async fn directory_access_rejects_absolute_workspace_input_and_scratch_paths() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();
    let ws_dir = paths.workspace_dir(&ws.id).unwrap();
    let input_file = ws_dir.join("input/Q2-sales.xlsx");
    std::fs::write(&input_file, b"test").unwrap();
    let scratch_dir = ws_dir.join("scratch");

    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test task", "Goal", Some(WorkPolicy::default()))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let pipeline = ToolPipeline::new(paths.clone());

    // 1. Absolute workspace input path
    let intent1 = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-abs-input".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": input_file.to_string_lossy(),
            "writable": false
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let res1 = pipeline
        .execute_intent(&intent1, &task.policy)
        .await
        .unwrap();
    assert!(!res1.success);
    assert_eq!(res1.status, "error");
    assert!(res1.interaction_id.is_none());

    // 2. Absolute workspace scratch directory path
    let intent2 = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-abs-scratch".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": scratch_dir.to_string_lossy(),
            "writable": false
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let res2 = pipeline
        .execute_intent(&intent2, &task.policy)
        .await
        .unwrap();
    assert!(!res2.success);
    assert_eq!(res2.status, "error");
    assert!(res2.interaction_id.is_none());

    // Zero interactions created
    let interaction_mgr = InteractionManager::new(paths.clone());
    let items = interaction_mgr.list_pending(Some(&task.id), None).unwrap();
    assert_eq!(items.len(), 0);
}

#[tokio::test]
async fn directory_access_rejects_external_file() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();

    let external_file = temp.path().join("external_file.txt");
    std::fs::write(&external_file, b"hello").unwrap();

    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test task", "Goal", Some(WorkPolicy::default()))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let pipeline = ToolPipeline::new(paths.clone());
    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-ext-file".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": external_file.to_string_lossy(),
            "writable": false
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let res = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();
    assert!(!res.success);
    assert_eq!(res.status, "error");
    assert!(res.stderr.contains("is a file") || res.stderr.contains("Only directories"));

    let interaction_mgr = InteractionManager::new(paths.clone());
    let items = interaction_mgr.list_pending(Some(&task.id), None).unwrap();
    assert_eq!(items.len(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn directory_access_rejects_symlink_resolving_into_workspace() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();
    let ws_dir = paths.workspace_dir(&ws.id).unwrap();

    let external_link = temp.path().join("external_link_to_ws");
    symlink(&ws_dir, &external_link).unwrap();

    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test task", "Goal", Some(WorkPolicy::default()))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let pipeline = ToolPipeline::new(paths.clone());
    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-symlink-ws".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": external_link.to_string_lossy(),
            "writable": false
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let res = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();
    assert!(!res.success);
    assert_eq!(res.status, "error");

    let interaction_mgr = InteractionManager::new(paths.clone());
    let items = interaction_mgr.list_pending(Some(&task.id), None).unwrap();
    assert_eq!(items.len(), 0);
}

#[tokio::test]
async fn directory_access_valid_external_directory_creates_single_request_and_resumes() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();

    let external_dir = temp.path().join("external_crm_data");
    std::fs::create_dir_all(&external_dir).unwrap();

    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test task", "Goal", Some(WorkPolicy::default()))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let pipeline = ToolPipeline::new(paths.clone());
    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-ext-crm".to_string(),
        tool_name: "work_request_directory_access".to_string(),
        action: "request_access".to_string(),
        arguments: serde_json::json!({
            "path": external_dir.to_string_lossy(),
            "writable": false,
            "purpose": "Read CRM dataset"
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    // 1. First execution creates exactly one AccessRootRequest
    let res1 = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();
    assert_eq!(res1.status, "waiting_approval");
    let interaction_id = res1.interaction_id.unwrap();

    let interaction_mgr = InteractionManager::new(paths.clone());
    let items = interaction_mgr.list_pending(Some(&task.id), None).unwrap();
    assert_eq!(
        items.len(),
        1,
        "Exactly one AccessRootRequest must be created"
    );
    assert_eq!(items[0].interaction_id, interaction_id);
    assert_eq!(items[0].kind, PendingInteractionKind::AccessRootRequest);

    // Run in WaitingApproval
    let run_waiting = task_mgr.get_run(&task.id, &run.id).unwrap();
    assert_eq!(run_waiting.status, WorkRunStatus::WaitingApproval);

    // 2. Approve interaction
    let (resolved, is_new) = interaction_mgr
        .resolve_interaction_once(
            &interaction_id,
            PendingInteractionState::Resolved,
            Some(serde_json::json!({
                "decision": "allowed",
                "outcome": "allowed_once"
            })),
        )
        .unwrap();
    assert!(is_new);
    assert_eq!(resolved.state, PendingInteractionState::Resolved);

    // 3. Resume same WorkRun
    let res2 = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();
    assert!(res2.success);
    assert_eq!(res2.status, "success");

    // Run returned to Running
    let run_resumed = task_mgr.get_run(&task.id, &run.id).unwrap();
    assert_eq!(run_resumed.status, WorkRunStatus::Running);

    // 4. Repeated call with already-authorized root -> Allow directly
    let res3 = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();
    assert!(res3.success);
    assert_eq!(res3.status, "success");
}

#[tokio::test]
async fn standalone_task_executes_write_and_read_file_successfully() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();
    let pipeline = ToolPipeline::new(paths.clone());

    let run_id = "standalone-run-456";
    let _ = paths.ensure_standalone_task_dir(run_id).unwrap();

    let write_intent = ToolIntent {
        task_id: run_id.to_string(),
        work_run_id: run_id.to_string(),
        session_id: None,
        workspace_id: "".to_string(),
        tool_call_id: "call-write-1".to_string(),
        tool_name: "work_write_file".to_string(),
        action: "write".to_string(),
        arguments: serde_json::json!({
            "path": "output/story2.txt",
            "content": "Once upon a time in a starry night..."
        }),
        input_paths: Vec::new(),
        expected_outputs: vec!["output/story2.txt".to_string()],
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let policy = WorkPolicy {
        execution_mode: crate::work::models::WorkExecutionMode::Auto,
        ..Default::default()
    };
    let write_res = pipeline
        .execute_intent(&write_intent, &policy)
        .await
        .unwrap();
    assert!(
        write_res.success,
        "write_res failed: {:?}",
        write_res.stderr
    );
    assert_eq!(write_res.status, "success");

    // Verify file content via read_file intent
    let read_intent = ToolIntent {
        task_id: run_id.to_string(),
        work_run_id: run_id.to_string(),
        session_id: None,
        workspace_id: "".to_string(),
        tool_call_id: "call-read-1".to_string(),
        tool_name: "work_read_file".to_string(),
        action: "read".to_string(),
        arguments: serde_json::json!({
            "path": "output/story2.txt"
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let read_res = pipeline
        .execute_intent(&read_intent, &policy)
        .await
        .unwrap();
    assert!(read_res.success);
    assert_eq!(read_res.stdout, "Once upon a time in a starry night...");

    // Verify standalone artifact registry
    let artifacts = crate::work::artifacts::list_standalone_with_paths(&paths, run_id).unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].path, "output/story2.txt");
}

#[tokio::test]
async fn missing_expected_output_turns_zero_exit_write_into_failure() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();
    let pipeline = ToolPipeline::new(paths.clone());
    let run_id = "standalone-run-missing-output";
    paths.ensure_standalone_task_dir(run_id).unwrap();

    let intent = ToolIntent {
        task_id: run_id.to_string(),
        work_run_id: run_id.to_string(),
        workspace_id: String::new(),
        tool_call_id: "call-missing-output".to_string(),
        tool_name: "work_write_file".to_string(),
        action: "write".to_string(),
        arguments: serde_json::json!({
            "path": "output/actual.txt",
            "content": "created"
        }),
        input_paths: Vec::new(),
        expected_outputs: vec!["output/missing.txt".to_string()],
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
        session_id: None,
    };
    let policy = WorkPolicy {
        execution_mode: WorkExecutionMode::Auto,
        ..Default::default()
    };

    let result = pipeline.execute_intent(&intent, &policy).await.unwrap();
    assert!(!result.success);
    assert_eq!(result.status, "failed");
    assert!(result
        .stderr
        .contains("expected output file 'output/missing.txt'"));
}

#[tokio::test]
async fn full_access_writes_external_read_only_root_without_approval() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Full Access Workspace").unwrap();
    let external_dir = temp.path().join("read_only_external");
    std::fs::create_dir_all(&external_dir).unwrap();
    ws_mgr
        .add_access_root(&ws.id, external_dir.to_str().unwrap(), false)
        .unwrap();

    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(
            &ws.id,
            "Full access task",
            "Write to the configured read-only directory",
            Some(WorkPolicy {
                execution_mode: WorkExecutionMode::FullAccess,
                ..Default::default()
            }),
        )
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();
    let target = external_dir.join("full-access.txt");
    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-full-access-write".to_string(),
        tool_name: "work_write_file".to_string(),
        action: "write".to_string(),
        arguments: serde_json::json!({
            "path": target.to_string_lossy(),
            "content": "FullAccess can write here."
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let result = ToolPipeline::new(paths.clone())
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();

    assert!(
        result.success,
        "FullAccess write failed: {:?}",
        result.stderr
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "FullAccess can write here."
    );
    assert!(
        InteractionManager::new(paths)
            .list_pending(Some(&task.id), None)
            .unwrap()
            .is_empty(),
        "FullAccess must not create an approval for the read-only root"
    );
}

#[tokio::test]
async fn oauth_connector_cli_creates_generic_auth_inbox_interaction() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let package_source = temp.path().join("oauth-cli");
    std::fs::create_dir_all(&package_source).unwrap();
    std::fs::write(
        package_source.join("connector-meta.json"),
        r#"{
          "source": "oauth.cli",
          "name": "OAuth CLI",
          "description": "test OAuth CLI connector",
          "version": "1.0.0",
          "type": "cli",
          "auth_mode": "oauth"
        }"#,
    )
    .unwrap();
    std::fs::write(
        package_source.join("cli.json"),
        r#"{
          "command": "printf",
          "operations": {
            "auth": { "args": ["authenticated"] },
            "status": { "args": ["authenticated"] },
            "sync": { "args": ["sync"], "requiresConfirmation": false }
          }
        }"#,
    )
    .unwrap();

    let installed =
        crate::work::connector_package_manager::install_with_paths(&paths, &package_source)
            .unwrap();
    crate::work::connector_package_manager::set_trusted_with_paths(
        &paths,
        &installed.manifest.id,
        true,
    )
    .unwrap();
    crate::work::connector_package_manager::set_enabled_with_paths(
        &paths,
        &installed.manifest.id,
        true,
    )
    .unwrap();

    let workspace = WorkspaceManager::new(paths.clone())
        .create("Connector auth workspace")
        .unwrap();
    let mut policy = WorkPolicy::default();
    policy.allow_external_connectors = true;
    let task = TaskManager::new(paths.clone())
        .create_task(
            &workspace.id,
            "Connector auth task",
            "Use connector",
            Some(policy),
        )
        .unwrap();
    let run = TaskManager::new(paths.clone())
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();
    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: workspace.id.clone(),
        tool_call_id: "connector-auth-call".into(),
        tool_name: "work_run_connector_cli".into(),
        action: "sync".into(),
        arguments: serde_json::json!({
            "package_id": "oauth.cli",
            "operation": "sync",
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let result = ToolPipeline::new(paths.clone())
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();
    assert_eq!(result.status, "waiting_approval");
    let interaction_id = result.interaction_id.expect("auth interaction id");
    let interaction = InteractionManager::new(paths.clone())
        .get_interaction(&interaction_id)
        .unwrap();
    assert_eq!(
        interaction.kind,
        PendingInteractionKind::ConnectorAuthRequest
    );
    assert_eq!(interaction.payload["connectorId"], "oauth.cli");
    assert_eq!(interaction.payload["runtimeKind"], "cli");
    assert!(interaction.payload.get("authUrl").is_none());
    assert_eq!(
        TaskManager::new(paths)
            .get_run(&task.id, &run.id)
            .unwrap()
            .status,
        WorkRunStatus::WaitingInput
    );
}

#[tokio::test]
async fn work_command_info_executes_via_pipeline() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();

    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test task", "Goal", Some(WorkPolicy::default()))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-preflight-info".to_string(),
        tool_name: "work_command_info".to_string(),
        action: "inspect".to_string(),
        arguments: serde_json::json!({
            "command": "soffice",
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let result = ToolPipeline::new(paths)
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.status, "success");
    let info: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(info["command"], "soffice");
    assert_eq!(info["recommendedChannel"], "structured_host");
}

#[tokio::test]
async fn pip_install_requires_approval_and_enriches_payload() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();

    let auto_policy = WorkPolicy {
        execution_mode: WorkExecutionMode::Auto,
        ..Default::default()
    };
    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test task", "Goal", Some(auto_policy))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-pip-install".to_string(),
        tool_name: "work_run_command".to_string(),
        action: "run".to_string(),
        arguments: serde_json::json!({
            "command": "pip",
            "args": ["install", "formulas"],
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let result = ToolPipeline::new(paths.clone())
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();

    assert_eq!(result.status, "waiting_approval");
    let interaction_id = result.interaction_id.expect("approval interaction id");
    let interaction = InteractionManager::new(paths)
        .get_interaction(&interaction_id)
        .unwrap();
    assert_eq!(interaction.payload["packageManager"], "pip");
    assert_eq!(
        interaction.payload["packages"],
        serde_json::json!(["formulas"])
    );
}

#[tokio::test]
async fn e2e_sandbox_denial_triggers_host_fallback_approval_and_resumes() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();

    let auto_policy = WorkPolicy {
        execution_mode: WorkExecutionMode::Auto,
        ..Default::default()
    };
    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test fallback task", "Goal", Some(auto_policy))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let gate_file =
        std::env::temp_dir().join(format!("gate_fallback_{}.tmp", uuid::Uuid::new_v4()));
    if gate_file.exists() {
        let _ = std::fs::remove_file(&gate_file);
    }
    let gate_str = gate_file.to_string_lossy().to_string();

    let script = format!(
        "if [ -f '{gate_str}' ]; then echo 'host_recovered'; exit 0; else echo 'sandbox: denied operation not permitted' >&2; exit 1; fi"
    );

    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-fallback-retry".to_string(),
        tool_name: "work_run_command".to_string(),
        action: "run".to_string(),
        arguments: serde_json::json!({
            "command": "sh",
            "args": ["-c", script],
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let pipeline = ToolPipeline::new(paths.clone());

    // 1. First execution: runs in sandbox, gate file absent, fails with SandboxDenied
    let result1 = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();

    assert_eq!(result1.status, "waiting_approval");
    assert!(!result1.success);
    let interaction_id = result1.interaction_id.expect("fallback interaction id");

    let interaction_mgr = InteractionManager::new(paths.clone());
    let interaction = interaction_mgr.get_interaction(&interaction_id).unwrap();
    assert_eq!(interaction.kind, PendingInteractionKind::Permission);
    assert_eq!(interaction.payload["executionLane"], "host_fallback");

    let updated_run = task_mgr.get_run(&task.id, &run.id).unwrap();
    assert_eq!(updated_run.status, WorkRunStatus::WaitingApproval);

    // 2. Simulate User Approval in Inbox: user clicks "仅本次在主机运行"
    std::fs::write(&gate_file, b"unlocked").unwrap();
    let (resolved, is_new) = interaction_mgr
        .resolve_interaction_once(
            &interaction_id,
            PendingInteractionState::Resolved,
            Some(serde_json::json!({ "approved": true })),
        )
        .unwrap();
    assert!(is_new);
    assert_eq!(resolved.state, PendingInteractionState::Resolved);

    // 3. Retry with the EXACT same intent (same tool_call_id, args):
    // Pipeline matches the host_fallback grant, forces host execution, and resumes task
    let result2 = pipeline
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();

    assert!(result2.success);
    assert_eq!(result2.status, "success");
    assert!(result2.stdout.contains("host_recovered"));

    let resumed_run = task_mgr.get_run(&task.id, &run.id).unwrap();
    assert_eq!(resumed_run.status, WorkRunStatus::Running);

    let _ = std::fs::remove_file(&gate_file);
}

#[tokio::test]
async fn office_outdir_escaping_workspace_is_rejected() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("Test Workspace").unwrap();

    let auto_policy = WorkPolicy {
        execution_mode: WorkExecutionMode::Auto,
        ..Default::default()
    };
    let task_mgr = TaskManager::new(paths.clone());
    let task = task_mgr
        .create_task(&ws.id, "Test office outdir task", "Goal", Some(auto_policy))
        .unwrap();
    let run = task_mgr
        .start_run(&task.id, None, WorkRunTrigger::Manual)
        .unwrap();

    let intent = ToolIntent {
        task_id: task.id.clone(),
        work_run_id: run.id.clone(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "call-office-outdir-escape".to_string(),
        tool_name: "work_run_command".to_string(),
        action: "run".to_string(),
        arguments: serde_json::json!({
            "command": "soffice",
            "args": ["--headless", "--convert-to", "pdf", "--outdir", "/tmp", "doc.docx"],
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };

    let result = ToolPipeline::new(paths)
        .execute_intent(&intent, &task.policy)
        .await
        .unwrap();

    // Must be rejected because /tmp is outside the workspace
    assert!(!result.success);
    assert_eq!(result.status, "failed");
    assert!(result.stderr.contains("超出已授权的 Workspace 范围"));
}

#[tokio::test]
async fn pipeline_handles_artifact_lifecycle_tools() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();
    let ws = WorkspaceManager::new(paths.clone())
        .create("Test Artifact WS")
        .unwrap();

    // Create an output file
    let ws_dir = paths.workspace_dir(&ws.id).unwrap();
    let out_dir = ws_dir.join("output");
    std::fs::create_dir_all(&out_dir).unwrap();
    let file_path = out_dir.join("test_report.md");
    std::fs::write(&file_path, "# Artifact Content").unwrap();

    let pipeline = ToolPipeline::new(paths.clone());
    let policy = WorkPolicy {
        execution_mode: WorkExecutionMode::Auto,
        ..Default::default()
    };

    // 1. work_register_artifact
    let register_intent = ToolIntent {
        task_id: "test-run".to_string(),
        work_run_id: "test-run".to_string(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "c-reg".to_string(),
        tool_name: "work_register_artifact".to_string(),
        action: "register".to_string(),
        arguments: serde_json::json!({
            "path": "output/test_report.md",
            "title": "Test Report",
            "type": "md"
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let reg_result = pipeline
        .execute_intent(&register_intent, &policy)
        .await
        .unwrap();
    assert!(reg_result.success);
    let artifact: serde_json::Value = serde_json::from_str(&reg_result.stdout).unwrap();
    assert_eq!(artifact["title"], "Test Report");
    let artifact_id = artifact["id"].as_str().unwrap().to_string();

    // 2. work_list_artifacts
    let list_intent = ToolIntent {
        task_id: "test-run".to_string(),
        work_run_id: "test-run".to_string(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "c-list".to_string(),
        tool_name: "work_list_artifacts".to_string(),
        action: "list".to_string(),
        arguments: serde_json::json!({}),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let list_result = pipeline
        .execute_intent(&list_intent, &policy)
        .await
        .unwrap();
    assert!(list_result.success);
    let list: Vec<serde_json::Value> = serde_json::from_str(&list_result.stdout).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], artifact_id);

    // 3. work_validate_artifact
    let val_intent = ToolIntent {
        task_id: "test-run".to_string(),
        work_run_id: "test-run".to_string(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "c-val".to_string(),
        tool_name: "work_validate_artifact".to_string(),
        action: "validate".to_string(),
        arguments: serde_json::json!({ "id": artifact_id }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let val_result = pipeline.execute_intent(&val_intent, &policy).await.unwrap();
    assert!(val_result.success);

    // 4. work_deliver
    let del_intent = ToolIntent {
        task_id: "test-run".to_string(),
        work_run_id: "test-run".to_string(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "c-del".to_string(),
        tool_name: "work_deliver".to_string(),
        action: "deliver".to_string(),
        arguments: serde_json::json!({ "id": artifact_id }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let del_result = pipeline.execute_intent(&del_intent, &policy).await.unwrap();
    assert!(del_result.success);
    let delivered: serde_json::Value = serde_json::from_str(&del_result.stdout).unwrap();
    assert_eq!(delivered["status"], "delivered");
}

#[tokio::test]
async fn work_update_context_rejects_escape_traversal_even_in_full_access() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();
    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr.create("EscapeContextTest").unwrap();

    let pipeline = ToolPipeline::new(paths.clone());
    let intent = ToolIntent {
        task_id: "task-1".to_string(),
        work_run_id: "run-1".to_string(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "c-escape".to_string(),
        tool_name: "work_update_context".to_string(),
        action: "update".to_string(),
        arguments: serde_json::json!({
            "path": "context/../../escape.md",
            "content": "hacked"
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let policy = WorkPolicy {
        execution_mode: WorkExecutionMode::FullAccess,
        ..Default::default()
    };

    let result = pipeline.execute_intent(&intent, &policy).await;
    assert!(
        result.is_err(),
        "Must deny traversal escape even in FullAccess"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("only updates direct files in context/"),
        "Expected direct file restriction, got: {err}"
    );
}

#[tokio::test]
async fn work_update_context_local_folder_writes_to_managed_context_dir() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    // Create a local folder workspace
    let local_project_dir = temp.path().join("my_local_project");
    std::fs::create_dir_all(&local_project_dir).unwrap();

    let ws_mgr = WorkspaceManager::new(paths.clone());
    let ws = ws_mgr
        .create_from_folder(local_project_dir.to_str().unwrap(), Some("LocalFolderTest"))
        .unwrap();

    let pipeline = ToolPipeline::new(paths.clone());
    let interaction_mgr = InteractionManager::new(paths.clone());

    let intent = ToolIntent {
        task_id: "task-local-ctx".to_string(),
        work_run_id: "run-local-ctx".to_string(),
        session_id: None,
        workspace_id: ws.id.clone(),
        tool_call_id: "c-local-ctx".to_string(),
        tool_name: "work_update_context".to_string(),
        action: "update".to_string(),
        arguments: serde_json::json!({
            "path": "context/notes.md",
            "content": "# Managed Notes"
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let policy = WorkPolicy {
        execution_mode: WorkExecutionMode::FullAccess,
        ..Default::default()
    };

    // 1. Propose (must require approval even in FullAccess)
    let pending = pipeline.execute_intent(&intent, &policy).await.unwrap();
    assert_eq!(pending.status, "waiting_approval");
    let interaction_id = pending.interaction_id.unwrap();

    // 2. Approve
    interaction_mgr
        .prepare_resolution_and_issue_grant(
            &interaction_id,
            PendingInteractionState::Resolved,
            Some(crate::work::models::InboxItemStatus::Approved),
            Some(serde_json::json!({"decision": "allow"})),
        )
        .unwrap();
    interaction_mgr
        .commit_resolution(
            &interaction_id,
            PendingInteractionState::Resolved,
            Some(crate::work::models::InboxItemStatus::Approved),
        )
        .unwrap();

    // 3. Execute
    let res = pipeline.execute_intent(&intent, &policy).await.unwrap();
    assert!(res.success);

    // Assert it wrote to managed workspace directory, NOT local_project_dir
    let managed_file = paths
        .workspace_dir(&ws.id)
        .unwrap()
        .join("context")
        .join("notes.md");
    assert!(
        managed_file.exists(),
        "File must exist in managed workspace context dir"
    );
    assert_eq!(
        std::fs::read_to_string(&managed_file).unwrap(),
        "# Managed Notes"
    );

    let local_file = local_project_dir.join("context").join("notes.md");
    assert!(
        !local_file.exists(),
        "File must NOT be written to primary local folder root"
    );
}

#[tokio::test]
async fn work_update_context_rejects_standalone_run() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let pipeline = ToolPipeline::new(paths.clone());
    let intent = ToolIntent {
        task_id: "task-standalone".to_string(),
        work_run_id: "run-standalone".to_string(),
        session_id: None,
        workspace_id: "".to_string(), // Standalone: no workspace_id
        tool_call_id: "c-standalone".to_string(),
        tool_name: "work_update_context".to_string(),
        action: "update".to_string(),
        arguments: serde_json::json!({
            "path": "context/rules.md",
            "content": "test"
        }),
        input_paths: Vec::new(),
        expected_outputs: Vec::new(),
        execution_context: crate::work::models::ExecutionContext::Attended,
        policy_revision: None,
    };
    let policy = WorkPolicy::default();

    let result = pipeline.execute_intent(&intent, &policy).await;
    assert!(
        result.is_err(),
        "Must deny work_update_context in standalone run"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("requires a Workspace"),
        "Expected workspace requirement error, got: {err}"
    );
}
