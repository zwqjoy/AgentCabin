//! Harness Tool Pipeline.
//!
//! Consequential Work tool calls flow through:
//! Tool Call -> Normalize ToolIntent -> Pre-execute policy -> Security/trust guards
//! -> Human approval if required -> Execution -> Post-execute validation
//! -> Authoritative ToolResult -> Runtime Ledger / Artifact registration.

use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::work::executor::{
    ExecutionFailureKind, WorkExecutionRequest, WorkExecutionResult, WorkExecutionStatus,
    WorkExecutor,
};
use crate::work::interaction::InteractionManager;
use crate::work::ledger::WorkRuntimeLedger;
use crate::work::models::{
    ApprovalOutcome, PendingInteractionKind, PendingInteractionState, RuntimeFact, SideEffectClass,
    ToolConcurrencyClass, ToolRiskClass, WorkExecutionMode, WorkPolicy,
};
use crate::work::paths::WorkPaths;
use crate::work::policy::{PolicyEvaluator, WorkPolicyDecision};
use crate::work::resources;

mod types;
pub use types::{ToolIntent, ToolResult};

pub(crate) fn validate_and_resolve_context_target(
    paths: &WorkPaths,
    workspace_id: &str,
    rel_path: &str,
) -> Result<PathBuf, String> {
    if workspace_id.trim().is_empty() {
        return Err("Workspace context update requires a Workspace".into());
    }
    let norm = rel_path.trim().replace('\\', "/");
    if !norm.starts_with("context/") {
        return Err(format!(
            "work_update_context only updates files in context/, got '{rel_path}'"
        ));
    }
    let file_part = &norm["context/".len()..];
    if file_part.is_empty()
        || file_part == "."
        || file_part == ".."
        || file_part.contains('/')
        || file_part.contains('\\')
        || file_part == "artifacts.json"
    {
        return Err(format!(
            "work_update_context only updates direct files in context/, got '{rel_path}'"
        ));
    }
    paths.resolve_workspace_path(workspace_id, std::path::Path::new(&norm), true)
}

/// The unified Harness Tool Pipeline.
#[derive(Clone)]
pub struct ToolPipeline {
    paths: WorkPaths,
    executor: Arc<WorkExecutor>,
}

impl ToolPipeline {
    pub fn new(paths: WorkPaths) -> Self {
        Self {
            paths,
            executor: Arc::new(WorkExecutor::new_restricted_host()),
        }
    }

    pub fn with_executor(paths: WorkPaths, executor: WorkExecutor) -> Self {
        Self {
            paths,
            executor: Arc::new(executor),
        }
    }

    /// Classify concurrency for a tool.
    pub fn concurrency_class(tool_name: &str) -> ToolConcurrencyClass {
        let name_lower = tool_name.to_lowercase();
        if name_lower.starts_with("work_read")
            || name_lower.starts_with("work_list")
            || name_lower.starts_with("work_inspect")
            || name_lower == "web_search"
            || name_lower == "web_extract"
            || name_lower.starts_with("work_workspace_info")
            || name_lower.starts_with("library_")
        {
            ToolConcurrencyClass::ParallelSafe
        } else if name_lower.starts_with("desktop_")
            || name_lower.contains("gui")
            || name_lower.contains("terminal_exclusive")
        {
            ToolConcurrencyClass::Exclusive
        } else {
            ToolConcurrencyClass::Serial
        }
    }

    /// Classify side effect class for a tool.
    pub fn side_effect_class(tool_name: &str) -> SideEffectClass {
        Self::side_effect_class_for_risk(PolicyEvaluator::classify_tool_risk(tool_name))
    }

    /// Map an explicitly resolved risk class to its side effect class.
    pub fn side_effect_class_for_risk(risk: ToolRiskClass) -> SideEffectClass {
        match risk {
            ToolRiskClass::Read => SideEffectClass::Read,
            ToolRiskClass::WriteLocal | ToolRiskClass::Exec => SideEffectClass::LocalVerifiable,
            ToolRiskClass::External => SideEffectClass::ExternalMutating,
        }
    }

    fn direct_connector_cli_guard(command: &str) -> Option<&'static str> {
        let executable = Path::new(command)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(command)
            .to_ascii_lowercase();
        if matches!(executable.as_str(), "lark-cli" | "lark-cli.cmd") {
            Some(
                "Direct lark-cli execution is disabled in Work. Use work_run_connector_cli with package_id 'feishu'; AgentCabin will select its application-managed cli-connector-packages binary and the declared user config paths.",
            )
        } else {
            None
        }
    }

    fn is_browser_operator_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "browser_navigate"
                | "browser_snapshot"
                | "browser_take_screenshot"
                | "browser_wait_for"
                | "browser_tabs"
                | "browser_close"
                | "browser_click"
                | "browser_type"
                | "browser_select_option"
                | "browser_scroll"
                | "browser_cdp_observe"
                | "browser_cdp_act"
        )
    }

    fn is_desktop_operator_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "desktop_list_apps"
                | "desktop_probe_app"
                | "desktop_open_app"
                | "desktop_observe"
                | "desktop_screenshot"
                | "desktop_click"
                | "desktop_type"
                | "desktop_key"
                | "desktop_scroll"
                | "desktop_act_batch"
                | "desktop_release"
                | "launch_app"
                | "find_roots"
                | "observe_ui"
                | "search_ui"
                | "expand_ui"
                | "inspect_ui"
                | "act_ui"
                | "read_text"
                | "wait_for"
        )
    }

    fn desktop_interaction_requires_confirmation(intent: &ToolIntent) -> bool {
        matches!(
            intent.tool_name.as_str(),
            "desktop_open_app"
                | "desktop_click"
                | "desktop_type"
                | "desktop_key"
                | "desktop_scroll"
                | "desktop_act_batch"
                | "launch_app"
                | "act_ui"
        )
    }

    /// Browser interactive operations (click, type, select, tab create/close)
    /// modify web page state and are classified as local writes. In attended
    /// Auto and FullAccess modes, they execute automatically so that autonomous
    /// web exploration does not stall on every click. In Direct mode or Unattended
    /// execution, they require human approval.
    fn browser_interaction_requires_confirmation(intent: &ToolIntent) -> bool {
        match intent.tool_name.as_str() {
            "browser_click" | "browser_type" | "browser_select_option" | "browser_cdp_act" => true,
            "browser_tabs" => matches!(
                intent
                    .arguments
                    .get("action")
                    .and_then(|value| value.as_str())
                    .unwrap_or("list"),
                "new" | "close"
            ),
            _ => false,
        }
    }

    /// Build a semantic denial reason so the model attributes the refusal to its actual
    /// cause (execution mode / connector policy) instead of guessing a bogus file-name or
    /// path policy. The target is included to identify which call was refused, but the
    /// wording makes clear the restriction is mode-level, not per-file.
    fn denial_reason(
        policy: &WorkPolicy,
        intent: &ToolIntent,
        effective_risk: ToolRiskClass,
        browser_use_enabled: bool,
    ) -> String {
        let target = intent.target_string();
        if Self::is_browser_operator_tool(&intent.tool_name) && !browser_use_enabled {
            return "Browser Use 当前未启用。请在能力中心的“浏览器操作 / Browser Use”中全局开启后重试。"
                .to_string();
        }
        if Self::is_desktop_operator_tool(&intent.tool_name)
            && !crate::work::desktop_operator::is_enabled()
        {
            return "电脑控制当前未启用或 native backend 尚未就绪。请在设置 > 电脑控制中检查状态。开发调试可设置 AGENTCABIN_DESKTOP_USE_ENABLED=1。"
                .to_string();
        }
        match effective_risk {
            risk if policy.execution_mode == WorkExecutionMode::PlanFirst
                && risk != ToolRiskClass::Read =>
            {
                format!(
                    "Denied: the current execution mode is read-only (plan/discuss), so mutating operations are disabled. The target '{target}' cannot be written in this mode. This restriction comes from the read-only mode, not from the file name or path. Switch to a writable mode (e.g. Direct or Auto) to perform the write."
                )
            }
            ToolRiskClass::External => format!(
                "Denied: external connector '{target}' is not enabled by the current WorkPolicy (allow_external_connectors is off). Enable external connectors to allow this call."
            ),
            _ => format!("WorkPolicy denied execution of '{target}'"),
        }
    }

    /// Resolve the effective risk class for an intent. Capability executions
    /// (`work_execute`) drill into the action-level declaration from the resource
    /// manifest; anything unknown fails closed at the tool-level classification.
    fn effective_risk_class(&self, intent: &ToolIntent) -> ToolRiskClass {
        if intent.tool_name == "work_run_connector_cli" {
            return crate::work::connector_package_manager::cli_operation_risk(&intent.arguments);
        }
        if intent.tool_name == "browser_tabs" {
            let action = intent
                .arguments
                .get("action")
                .and_then(|value| value.as_str())
                .unwrap_or("list");
            return if matches!(action, "list" | "switch") {
                ToolRiskClass::Read
            } else {
                ToolRiskClass::WriteLocal
            };
        }
        if intent.tool_name == "browser_cdp_observe" {
            return ToolRiskClass::Read;
        }
        if intent.tool_name == "browser_cdp_act" {
            return ToolRiskClass::WriteLocal;
        }
        if intent.tool_name != "work_execute" {
            return PolicyEvaluator::classify_tool_risk(&intent.tool_name);
        }
        let resource_id = intent
            .arguments
            .get("resource_id")
            .or_else(|| intent.arguments.get("resourceId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let action = intent
            .arguments
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or(intent.action.as_str())
            .trim();
        if resource_id.is_empty() || action.is_empty() {
            return ToolRiskClass::Exec;
        }
        match resources::get_resource_with_paths(&self.paths, resource_id) {
            Some((manifest, _, _)) => {
                PolicyEvaluator::classify_capability_action_risk(&manifest, action)
            }
            None => ToolRiskClass::Exec,
        }
    }

    fn is_user_input_tool(tool_name: &str) -> bool {
        matches!(tool_name, "ask_questions" | "work_ask_questions")
    }

    /// Derive a task-scoped standing-rule target pattern for a permission request, so the
    /// human can promote a one-shot approval into a durable rule instead of approving every
    /// call. Returns None when no safe pattern exists (e.g. free-form command targets).
    fn standing_rule_pattern(intent: &ToolIntent) -> Option<String> {
        // Capability executions (work_execute): exempt the whole capability, e.g. "work-excel.*"
        if let Some(res_id) = intent
            .arguments
            .get("resource_id")
            .or_else(|| intent.arguments.get("resourceId"))
            .and_then(|v| v.as_str())
        {
            let res_id = res_id.trim();
            if !res_id.is_empty() {
                return Some(format!("{res_id}.*"));
            }
        }
        if intent.tool_name == "work_update_context" {
            return None;
        }
        // File mutations (work_write_file / work_edit_file / ...): exempt the parent
        // directory, e.g. "scratch/*". Root-level files get no pattern (too broad).
        if let Some(path) = intent.arguments.get("path").and_then(|v| v.as_str()) {
            let trimmed = path.trim_matches('/');
            if let Some((dir, _)) = trimmed.rsplit_once('/') {
                if !dir.is_empty() {
                    return Some(format!("{dir}/*"));
                }
            }
        }
        if matches!(
            intent.tool_name.as_str(),
            "browser_click" | "browser_type" | "browser_select_option"
        ) {
            if let Some(selector) = intent
                .arguments
                .get("selector")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
            {
                return Some(selector.to_string());
            }
        }
        None
    }

    /// Execute a single tool intent through the pipeline.
    pub async fn execute_intent(
        &self,
        intent: &ToolIntent,
        policy: &WorkPolicy,
    ) -> Result<ToolResult, String> {
        self.execute_intent_with_proxy(intent, policy, None).await
    }

    /// Execute a tool intent while keeping any host-bound network proxy
    /// outside the Pi process. The proxy is supplied only by the authenticated
    /// internal bridge and is consumed by Host-owned Web execution.
    pub async fn execute_intent_with_proxy(
        &self,
        intent: &ToolIntent,
        policy: &WorkPolicy,
        proxy_url: Option<&str>,
    ) -> Result<ToolResult, String> {
        let started_at = Utc::now().to_rfc3339();
        let target = intent.target_string();
        // Asking the root user is a control-flow interaction, not an external
        // side effect. Keep it outside the normal permission policy so Direct,
        // Auto, and FullAccess modes all expose the same question contract.
        let effective_risk = if Self::is_user_input_tool(&intent.tool_name) {
            ToolRiskClass::Read
        } else {
            self.effective_risk_class(intent)
        };
        let side_effect = Self::side_effect_class_for_risk(effective_risk);
        let concurrency = Self::concurrency_class(&intent.tool_name);
        let args_hash = intent.compute_arguments_hash();

        // 1. Open ledger and record ToolProposed
        let ledger = WorkRuntimeLedger::open(&self.paths, &intent.task_id, &intent.work_run_id)?;
        ledger.record(&RuntimeFact::ToolProposed {
            tool_call_id: intent.tool_call_id.clone(),
            tool_name: intent.tool_name.clone(),
            action: intent.action.clone(),
            arguments_hash: args_hash.clone(),
            expected_outputs: intent.expected_outputs.clone(),
            side_effect_class: side_effect,
            concurrency_class: concurrency,
            timestamp: started_at.clone(),
        })?;

        if Self::is_user_input_tool(&intent.tool_name) {
            let interaction_mgr = InteractionManager::new(self.paths.clone());

            if let Some(existing) = interaction_mgr.find_by_tool_call(
                &intent.task_id,
                &intent.work_run_id,
                &intent.tool_call_id,
                &args_hash,
            )? {
                match existing.state {
                    PendingInteractionState::Pending | PendingInteractionState::Delivering => {
                        let result = ToolResult {
                            tool_call_id: intent.tool_call_id.clone(),
                            tool_name: intent.tool_name.clone(),
                            action: intent.action.clone(),
                            success: false,
                            status: "waiting_input".to_string(),
                            failure_kind: None,
                            exit_code: None,
                            stdout: String::new(),
                            stderr: "Work is waiting for the root user's answers.".to_string(),
                            outputs: Vec::new(),
                            interaction_id: Some(existing.interaction_id),
                            side_effect_class: side_effect,
                            started_at: started_at.clone(),
                            finished_at: Utc::now().to_rfc3339(),
                        };
                        return Ok(result);
                    }
                    PendingInteractionState::Resolved => {
                        let response = existing.resolution.unwrap_or_else(|| serde_json::json!({}));
                        let stdout = serde_json::to_string(&response).map_err(|error| {
                            format!("Failed to serialize ask_questions response: {error}")
                        })?;
                        let result = ToolResult {
                            tool_call_id: intent.tool_call_id.clone(),
                            tool_name: intent.tool_name.clone(),
                            action: intent.action.clone(),
                            success: true,
                            status: "success".to_string(),
                            failure_kind: None,
                            exit_code: Some(0),
                            stdout,
                            stderr: String::new(),
                            outputs: Vec::new(),
                            interaction_id: Some(existing.interaction_id),
                            side_effect_class: side_effect,
                            started_at: started_at.clone(),
                            finished_at: Utc::now().to_rfc3339(),
                        };
                        ledger.record(&RuntimeFact::ToolResult {
                            tool_call_id: result.tool_call_id.clone(),
                            success: true,
                            status: result.status.clone(),
                            failure_kind: None,
                            exit_code: result.exit_code,
                            error: None,
                            outputs: Vec::new(),
                            side_effect_class: side_effect,
                            timestamp: result.finished_at.clone(),
                        })?;
                        return Ok(result);
                    }
                    PendingInteractionState::Cancelled => {
                        let result = ToolResult {
                            tool_call_id: intent.tool_call_id.clone(),
                            tool_name: intent.tool_name.clone(),
                            action: intent.action.clone(),
                            success: false,
                            status: "cancelled".to_string(),
                            failure_kind: None,
                            exit_code: Some(1),
                            stdout: String::new(),
                            stderr: "The root user cancelled the question.".to_string(),
                            outputs: Vec::new(),
                            interaction_id: Some(existing.interaction_id),
                            side_effect_class: side_effect,
                            started_at: started_at.clone(),
                            finished_at: Utc::now().to_rfc3339(),
                        };
                        ledger.record(&RuntimeFact::ToolResult {
                            tool_call_id: result.tool_call_id.clone(),
                            success: false,
                            status: result.status.clone(),
                            failure_kind: None,
                            exit_code: result.exit_code,
                            error: Some(result.stderr.clone()),
                            outputs: Vec::new(),
                            side_effect_class: side_effect,
                            timestamp: result.finished_at.clone(),
                        })?;
                        return Ok(result);
                    }
                }
            }

            let questions = intent
                .arguments
                .get("questions")
                .and_then(|value| value.as_array())
                .filter(|questions| !questions.is_empty())
                .ok_or_else(|| "ask_questions requires a non-empty questions array".to_string())?;
            let first_question = questions
                .first()
                .and_then(|question| question.get("question"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|question| !question.is_empty())
                .unwrap_or("请回答以下问题");
            let title = intent
                .arguments
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or("需要你的选择");
            let interaction = interaction_mgr.create_interaction(
                &intent.task_id,
                &intent.work_run_id,
                &intent.workspace_id,
                intent.session_id.as_deref(),
                None,
                Some(&intent.tool_call_id),
                PendingInteractionKind::UserInput,
                title,
                "请回答以下问题，回答后 Work 将继续执行。",
                serde_json::json!({
                    "toolName": "ask_questions",
                    "action": intent.action.clone(),
                    "argumentsHash": args_hash.clone(),
                    "workspaceId": intent.workspace_id.clone(),
                    "parameters": intent.arguments.clone(),
                    "question": first_question,
                }),
            )?;
            ledger.record(&RuntimeFact::ApprovalRequested {
                interaction_id: interaction.interaction_id.clone(),
                tool_call_id: Some(intent.tool_call_id.clone()),
                kind: PendingInteractionKind::UserInput,
                timestamp: Utc::now().to_rfc3339(),
            })?;
            let task_mgr = crate::work::tasks::TaskManager::new(self.paths.clone());
            if task_mgr
                .get_run(&intent.task_id, &intent.work_run_id)
                .is_ok()
            {
                crate::work::lifecycle::WorkHarnessController::new(self.paths.clone())
                    .transition_run_status(
                        &intent.task_id,
                        &intent.work_run_id,
                        crate::work::models::WorkRunStatus::WaitingInput,
                    )?;
            }
            let result = ToolResult {
                tool_call_id: intent.tool_call_id.clone(),
                tool_name: intent.tool_name.clone(),
                action: intent.action.clone(),
                success: false,
                status: "waiting_input".to_string(),
                failure_kind: None,
                exit_code: None,
                stdout: String::new(),
                stderr: "Work is waiting for the root user's answers.".to_string(),
                outputs: Vec::new(),
                interaction_id: Some(interaction.interaction_id),
                side_effect_class: side_effect,
                started_at: started_at.clone(),
                finished_at: Utc::now().to_rfc3339(),
            };
            ledger.record(&RuntimeFact::ToolResult {
                tool_call_id: result.tool_call_id.clone(),
                success: false,
                status: result.status.clone(),
                failure_kind: None,
                exit_code: None,
                error: Some(result.stderr.clone()),
                outputs: Vec::new(),
                side_effect_class: side_effect,
                timestamp: result.finished_at.clone(),
            })?;
            return Ok(result);
        }

        // 2. Pre-execute policy evaluation (action-level risk for capability executions)
        let has_standing_rule = intent.tool_name != "work_update_context"
            && PolicyEvaluator::find_matching_rule(policy, &intent.tool_name, &target).is_some();
        let mut decision = if has_standing_rule {
            WorkPolicyDecision::Allow
        } else {
            PolicyEvaluator::evaluate_with_risk(
                policy,
                crate::work::models::CollaborationMode::Default,
                effective_risk,
                &intent.tool_name,
                &target,
                intent.execution_context,
            )
        };

        if intent.tool_name == "work_update_context" {
            let rel_path = intent
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            validate_and_resolve_context_target(&self.paths, &intent.workspace_id, rel_path)?;
            decision = WorkPolicyDecision::Ask;
        }

        if intent.tool_name == "work_run_command" && !has_standing_rule {
            let cmd_str = intent
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cmd_args: Vec<String> = intent
                .arguments
                .get("args")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| a.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let cwd_str = intent
                .arguments
                .get("cwd")
                .and_then(|v| v.as_str())
                .unwrap_or("scratch");
            let classification =
                PolicyEvaluator::classify_command_risk(cmd_str, &cmd_args, cwd_str, None);
            decision = PolicyEvaluator::evaluate_command_risk(
                policy,
                &classification,
                &intent.tool_name,
                &target,
            );
        }

        if Self::browser_interaction_requires_confirmation(intent)
            && !has_standing_rule
            && policy.execution_mode != WorkExecutionMode::FullAccess
            && policy.execution_mode != WorkExecutionMode::Auto
            && decision == WorkPolicyDecision::Allow
        {
            decision = WorkPolicyDecision::Ask;
        }

        if Self::desktop_interaction_requires_confirmation(intent)
            && !has_standing_rule
            && policy.execution_mode != WorkExecutionMode::FullAccess
            && policy.execution_mode != WorkExecutionMode::Auto
            && decision == WorkPolicyDecision::Allow
        {
            decision = WorkPolicyDecision::Ask;
        }

        // Keep the profile-level Browser Use switch authoritative even when a
        // stale runtime still attempts to call a previously registered tool.
        if Self::is_browser_operator_tool(&intent.tool_name)
            && !crate::storage::profile_bindings::is_browser_use_enabled_with_root(
                self.paths.data_root(),
            )
        {
            decision = WorkPolicyDecision::Deny;
        }

        if Self::is_desktop_operator_tool(&intent.tool_name)
            && !crate::work::desktop_operator::is_enabled()
        {
            decision = WorkPolicyDecision::Deny;
        }

        // If tool is work_request_directory_access, validate external access root and evaluate access root existence
        if intent.tool_name == "work_request_directory_access" {
            let path_str = intent
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let canonical_target = match self
                .paths
                .validate_external_access_root(&intent.workspace_id, path_str)
            {
                Ok(canon) => canon,
                Err(err_msg) => {
                    let result = ToolResult::error(
                        &intent.tool_call_id,
                        &intent.tool_name,
                        &intent.action,
                        &err_msg,
                    );
                    ledger.record(&RuntimeFact::ToolResult {
                        tool_call_id: intent.tool_call_id.clone(),
                        success: false,
                        status: "error".to_string(),
                        failure_kind: None,
                        exit_code: Some(-1),
                        error: Some(result.stderr.clone()),
                        outputs: Vec::new(),
                        side_effect_class: side_effect,
                        timestamp: Utc::now().to_rfc3339(),
                    })?;
                    return Ok(result);
                }
            };

            let ws_mgr = crate::work::workspace::WorkspaceManager::new(self.paths.clone());
            if let Ok(ws) = ws_mgr.get(&intent.workspace_id) {
                let writable = intent
                    .arguments
                    .get("writable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let already_authorized = ws.access_roots.iter().any(|root| {
                    let root_path = std::path::Path::new(&root.path);
                    let canon_root = root_path
                        .canonicalize()
                        .unwrap_or_else(|_| root_path.to_path_buf());
                    canonical_target == canon_root && (!writable || root.writable)
                });
                if already_authorized {
                    decision = WorkPolicyDecision::Allow;
                } else {
                    decision = WorkPolicyDecision::Ask;
                }
            } else {
                decision = WorkPolicyDecision::Ask;
            }
        }
        if intent.tool_name == "work_request_directory_access"
            && policy.execution_mode == WorkExecutionMode::FullAccess
        {
            decision = WorkPolicyDecision::Allow;
        }

        // OAuth-backed Connector Packages use the same durable Inbox/Human
        // Loop as ordinary approvals. This turns a missing connection into a
        // first-class auth card and lets the Pi tool retry after the Host has
        // verified the connection, even when a standing rule would otherwise
        // allow the tool call directly.
        let connector_auth_needed = if intent.tool_name == "work_run_connector_cli" {
            crate::work::connector_package_manager::cli_requires_host_auth(
                &self.paths,
                &intent.arguments,
            )?
        } else {
            false
        };
        let connector_cli_confirmation = if intent.tool_name == "work_run_connector_cli" {
            crate::work::connector_package_manager::cli_requires_confirmation(
                &self.paths,
                &intent.arguments,
            )?
        } else {
            false
        };
        let pending_app_connection_id = if intent.tool_name == "work_call_app"
            && !matches!(decision, WorkPolicyDecision::Deny)
        {
            match crate::work::apps::storage::get_connection(
                &self.paths,
                &intent
                    .arguments
                    .get("app_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase(),
            ) {
                Ok(Some(connection))
                    if connection.status
                        == crate::work::apps::models::ConnectionStatus::Pending =>
                {
                    Some(connection.connection_id)
                }
                Ok(_) => None,
                Err(error) => {
                    let mut result = ToolResult::error(
                        &intent.tool_call_id,
                        &intent.tool_name,
                        &intent.action,
                        &format!("Failed to read app connection state: {error}"),
                    );
                    result.side_effect_class = side_effect;
                    ledger.record(&RuntimeFact::ToolResult {
                        tool_call_id: intent.tool_call_id.clone(),
                        success: false,
                        status: result.status.clone(),
                        failure_kind: result.failure_kind,
                        exit_code: result.exit_code,
                        error: Some(result.stderr.clone()),
                        outputs: Vec::new(),
                        side_effect_class: side_effect,
                        timestamp: result.finished_at.clone(),
                    })?;
                    return Ok(result);
                }
            }
        } else {
            None
        };
        let app_connection_needed = intent.tool_name == "work_call_app"
            && !matches!(decision, WorkPolicyDecision::Deny)
            && (pending_app_connection_id.is_some()
                || match intent
                    .arguments
                    .get("app_id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    Some(app_id) => match crate::work::apps::storage::get_connection(
                        &self.paths,
                        &app_id.to_lowercase(),
                    ) {
                        Ok(Some(connection)) => {
                            connection.status
                                != crate::work::apps::models::ConnectionStatus::Connected
                                || connection.accounts.is_empty()
                        }
                        Ok(None) => true,
                        Err(_) => false,
                    },
                    None => false,
                });
        if (connector_auth_needed || connector_cli_confirmation || app_connection_needed)
            && !matches!(decision, WorkPolicyDecision::Deny)
        {
            decision = WorkPolicyDecision::Ask;
        }

        let mut force_host_execution = false;
        match decision {
            WorkPolicyDecision::Deny => {
                let browser_use_enabled =
                    crate::storage::profile_bindings::is_browser_use_enabled_with_root(
                        self.paths.data_root(),
                    );
                let reason =
                    Self::denial_reason(policy, intent, effective_risk, browser_use_enabled);
                let result = ToolResult::denied(
                    &intent.tool_call_id,
                    &intent.tool_name,
                    &intent.action,
                    &reason,
                );
                ledger.record(&RuntimeFact::ToolResult {
                    tool_call_id: intent.tool_call_id.clone(),
                    success: false,
                    status: "denied".to_string(),
                    failure_kind: None,
                    exit_code: Some(-1),
                    error: Some(result.stderr.clone()),
                    outputs: Vec::new(),
                    side_effect_class: side_effect,
                    timestamp: Utc::now().to_rfc3339(),
                })?;
                return Ok(result);
            }
            WorkPolicyDecision::Ask => {
                let interaction_mgr = InteractionManager::new(self.paths.clone());
                // A retried request (for example, after an HTTP transport
                // reconnect) must join the existing durable decision instead
                // of creating a second Inbox card for the same intent.
                if let Some(existing) = interaction_mgr
                    .list_pending(Some(&intent.task_id), Some(&intent.work_run_id))?
                    .into_iter()
                    .find(|interaction| {
                        interaction.tool_call_id.as_deref() == Some(intent.tool_call_id.as_str())
                            && interaction
                                .payload
                                .get("argumentsHash")
                                .and_then(|value| value.as_str())
                                == Some(args_hash.as_str())
                    })
                {
                    let waiting_for_input = matches!(
                        &existing.kind,
                        PendingInteractionKind::AppConnectionRequest
                            | PendingInteractionKind::ConnectorAuthRequest
                    );
                    return Ok(ToolResult {
                        tool_call_id: intent.tool_call_id.clone(),
                        tool_name: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        success: false,
                        status: if waiting_for_input {
                            "waiting_input".to_string()
                        } else {
                            "waiting_approval".to_string()
                        },
                        failure_kind: None,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: "A durable Work interaction is already pending for this tool call."
                            .to_string(),
                        outputs: Vec::new(),
                        interaction_id: Some(existing.interaction_id),
                        side_effect_class: side_effect,
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    });
                }
                let matching_grant = if connector_auth_needed || app_connection_needed {
                    None
                } else {
                    interaction_mgr.find_matching_grant(
                        &intent.task_id,
                        &intent.work_run_id,
                        &intent.tool_call_id,
                        &intent.tool_name,
                        &intent.action,
                        &args_hash,
                        &intent.workspace_id,
                    )?
                };

                if let Some(grant) = matching_grant {
                    if grant.outcome == ApprovalOutcome::AllowedOnce && !grant.consumed {
                        // Valid one-shot grant: consume it atomically
                        interaction_mgr.consume_grant(&grant.grant_id)?;
                        if grant.execution_lane.as_deref() == Some("host_fallback") {
                            force_host_execution = true;
                        }
                        ledger.record(&RuntimeFact::GrantConsumed {
                            grant_id: grant.grant_id,
                            tool_call_id: intent.tool_call_id.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                        })?;
                        let task_mgr = crate::work::tasks::TaskManager::new(self.paths.clone());
                        if task_mgr
                            .get_run(&intent.task_id, &intent.work_run_id)
                            .is_ok()
                        {
                            let controller = crate::work::lifecycle::WorkHarnessController::new(
                                self.paths.clone(),
                            );
                            controller.transition_run_status(
                                &intent.task_id,
                                &intent.work_run_id,
                                crate::work::models::WorkRunStatus::Running,
                            )?;
                        }
                    } else if grant.outcome == ApprovalOutcome::Rejected {
                        let result = ToolResult::denied(
                            &intent.tool_call_id,
                            &intent.tool_name,
                            &intent.action,
                            "User rejected approval for this operation.",
                        );
                        ledger.record(&RuntimeFact::ToolResult {
                            tool_call_id: intent.tool_call_id.clone(),
                            success: false,
                            status: "denied".to_string(),
                            failure_kind: None,
                            exit_code: Some(-1),
                            error: Some(result.stderr.clone()),
                            outputs: Vec::new(),
                            side_effect_class: side_effect,
                            timestamp: Utc::now().to_rfc3339(),
                        })?;
                        return Ok(result);
                    } else {
                        return Ok(ToolResult::denied(
                            &intent.tool_call_id,
                            &intent.tool_name,
                            &intent.action,
                            "Approval grant is expired or already consumed.",
                        ));
                    }
                } else {
                    let app_connection_id = if app_connection_needed {
                        if let Some(connection_id) = pending_app_connection_id.clone() {
                            Some(connection_id)
                        } else {
                            let app_id = intent
                                .arguments
                                .get("app_id")
                                .and_then(|value| value.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    "work_call_app requires a non-empty app_id".to_string()
                                })?;
                            match crate::work::apps::authorize_with_paths(
                                &self.paths,
                                app_id,
                                None,
                                None,
                            )
                            .await
                            {
                                Ok(response) => Some(response.connection_id),
                                Err(error) => {
                                    let mut result = ToolResult::error(
                                        &intent.tool_call_id,
                                        &intent.tool_name,
                                        &intent.action,
                                        &format!(
                                            "Failed to start app authorization for '{}': {}",
                                            app_id, error
                                        ),
                                    );
                                    result.side_effect_class = side_effect;
                                    ledger.record(&RuntimeFact::ToolResult {
                                        tool_call_id: intent.tool_call_id.clone(),
                                        success: false,
                                        status: result.status.clone(),
                                        failure_kind: result.failure_kind,
                                        exit_code: result.exit_code,
                                        error: Some(result.stderr.clone()),
                                        outputs: Vec::new(),
                                        side_effect_class: side_effect,
                                        timestamp: result.finished_at.clone(),
                                    })?;
                                    return Ok(result);
                                }
                            }
                        }
                    } else {
                        None
                    };

                    let interaction_kind = if connector_auth_needed {
                        PendingInteractionKind::ConnectorAuthRequest
                    } else if app_connection_needed {
                        PendingInteractionKind::AppConnectionRequest
                    } else if intent.tool_name == "work_request_directory_access" {
                        PendingInteractionKind::AccessRootRequest
                    } else {
                        PendingInteractionKind::Permission
                    };

                    let mut payload_json = serde_json::json!({
                        "toolName": intent.tool_name,
                        "action": intent.action,
                        "argumentsHash": args_hash,
                        "workspaceId": intent.workspace_id,
                        "parameters": intent.arguments,
                        "inputPaths": intent.input_paths,
                        "expectedOutputs": intent.expected_outputs,
                    });

                    if intent.tool_name == "work_request_directory_access" {
                        if let Some(path) = intent.arguments.get("path").and_then(|v| v.as_str()) {
                            payload_json["path"] = serde_json::json!(path);
                        }
                        if let Some(writable) =
                            intent.arguments.get("writable").and_then(|v| v.as_bool())
                        {
                            payload_json["writable"] = serde_json::json!(writable);
                        }
                        if let Some(purpose) =
                            intent.arguments.get("purpose").and_then(|v| v.as_str())
                        {
                            payload_json["purpose"] = serde_json::json!(purpose);
                        }
                    }

                    if intent.tool_name == "work_run_command" {
                        let cmd_str = intent
                            .arguments
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let cmd_args: Vec<String> = intent
                            .arguments
                            .get("args")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|a| a.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let cwd_str = intent
                            .arguments
                            .get("cwd")
                            .and_then(|v| v.as_str())
                            .unwrap_or("scratch");
                        let class = PolicyEvaluator::classify_command_risk(
                            cmd_str, &cmd_args, cwd_str, None,
                        );
                        match class {
                            crate::work::models::CommandRiskClassification::DependencyInstall {
                                package_manager,
                                packages,
                                source,
                            } => {
                                payload_json["packageManager"] = serde_json::json!(package_manager);
                                payload_json["packages"] = serde_json::json!(packages);
                                if let Some(src) = source {
                                    payload_json["source"] = serde_json::json!(src);
                                }
                            }
                            crate::work::models::CommandRiskClassification::Destructive {
                                reason,
                            } => {
                                payload_json["destructiveReason"] = serde_json::json!(reason);
                            }
                            _ => {}
                        }
                    }

                    if connector_auth_needed {
                        if let Some(package_id) = payload_json
                            .get("parameters")
                            .and_then(|value| value.get("package_id"))
                            .and_then(|value| value.as_str())
                        {
                            payload_json["connectorId"] = serde_json::json!(package_id);
                            payload_json["runtimeKind"] = serde_json::json!("cli");
                            payload_json["provider"] = serde_json::json!("host-oauth");
                        }
                    }

                    if app_connection_needed {
                        if let Some(app_id) = intent
                            .arguments
                            .get("app_id")
                            .and_then(|value| value.as_str())
                        {
                            payload_json["appId"] = serde_json::json!(app_id.trim().to_lowercase());
                        }
                        if let Some(tool_name) = intent
                            .arguments
                            .get("tool_name")
                            .and_then(|value| value.as_str())
                        {
                            payload_json["providerToolName"] = serde_json::json!(tool_name);
                        }
                        payload_json["connectionId"] = serde_json::json!(app_connection_id);
                        payload_json["provider"] = serde_json::json!("host-provider");
                    }

                    // Attach a standing-rule proposal so the approval UI can offer
                    // "approve & always allow" instead of one-shot grants only.
                    if interaction_kind == PendingInteractionKind::Permission {
                        if let Some(pattern) = Self::standing_rule_pattern(intent) {
                            payload_json["standingRuleProposal"] = serde_json::json!({
                                "toolName": intent.tool_name,
                                "targetPattern": pattern,
                                "scope": "task",
                                "riskClass": effective_risk,
                            });
                        }
                    }

                    let interaction_title = if connector_auth_needed {
                        format!("连接器认证请求：{target}")
                    } else if app_connection_needed {
                        format!("应用连接请求：{target}")
                    } else if intent.tool_name == "work_update_context" {
                        format!("更新工作区知识：{target}")
                    } else {
                        format!("Approval Needed: {target}")
                    };
                    let interaction = interaction_mgr.create_interaction(
                        &intent.task_id,
                        &intent.work_run_id,
                        &intent.workspace_id,
                        intent.session_id.as_deref(),
                        None,
                        Some(&intent.tool_call_id),
                        interaction_kind.clone(),
                        &interaction_title,
                        if connector_auth_needed {
                            "当前连接器尚未完成授权，请在 Inbox 中完成连接后继续任务。"
                        } else if app_connection_needed {
                            "当前应用尚未完成授权，请在 Inbox 中完成连接后继续任务。"
                        } else if intent.tool_name == "work_update_context" {
                            "任务请求更新 Workspace 知识（context/），请确认写入内容。"
                        } else {
                            "Execution is paused pending human approval."
                        },
                        payload_json,
                    )?;

                    ledger.record(&RuntimeFact::ApprovalRequested {
                        interaction_id: interaction.interaction_id.clone(),
                        tool_call_id: Some(intent.tool_call_id.clone()),
                        kind: interaction_kind,
                        timestamp: Utc::now().to_rfc3339(),
                    })?;

                    let task_mgr = crate::work::tasks::TaskManager::new(self.paths.clone());
                    if task_mgr
                        .get_run(&intent.task_id, &intent.work_run_id)
                        .is_ok()
                    {
                        let controller =
                            crate::work::lifecycle::WorkHarnessController::new(self.paths.clone());
                        controller.transition_run_status(
                            &intent.task_id,
                            &intent.work_run_id,
                            if connector_auth_needed || app_connection_needed {
                                crate::work::models::WorkRunStatus::WaitingInput
                            } else {
                                crate::work::models::WorkRunStatus::WaitingApproval
                            },
                        )?;
                    }

                    let result = ToolResult {
                        tool_call_id: intent.tool_call_id.clone(),
                        tool_name: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        success: false,
                        status: if app_connection_needed {
                            "waiting_input".to_string()
                        } else {
                            "waiting_approval".to_string()
                        },
                        failure_kind: None,
                        exit_code: None,
                        stderr: if connector_auth_needed {
                            "Connector Package authorization is required before execution."
                                .to_string()
                        } else if app_connection_needed {
                            "Connected app authorization is required before execution.".to_string()
                        } else {
                            "WorkPolicy requires Inbox approval before execution.".to_string()
                        },
                        stdout: if app_connection_needed {
                            serde_json::json!({
                                "appId": intent.arguments.get("app_id"),
                                "connectionId": app_connection_id,
                            })
                            .to_string()
                        } else {
                            String::new()
                        },
                        outputs: Vec::new(),
                        interaction_id: Some(interaction.interaction_id),
                        side_effect_class: side_effect,
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    };
                    // The policy decision is itself the durable outcome of
                    // this invocation. Recording it closes the original
                    // proposal before a later resume issues a new tool call
                    // id; otherwise completion/recovery would keep treating
                    // an approval request that never started as an orphaned
                    // execution.
                    ledger.record(&RuntimeFact::ToolResult {
                        tool_call_id: result.tool_call_id.clone(),
                        success: false,
                        status: result.status.clone(),
                        failure_kind: result.failure_kind,
                        exit_code: result.exit_code,
                        error: Some(result.stderr.clone()),
                        outputs: result.outputs.clone(),
                        side_effect_class: side_effect,
                        timestamp: result.finished_at.clone(),
                    })?;
                    return Ok(result);
                }
            }
            WorkPolicyDecision::Allow => {
                // A sandbox-fallback approval is retried with the exact same
                // intent. Auto mode normally reaches Allow directly, so it
                // must consume this special grant here rather than only in
                // the ordinary Ask branch above.
                if intent.tool_name == "work_run_command"
                    && policy.execution_mode != WorkExecutionMode::FullAccess
                {
                    let interaction_mgr = InteractionManager::new(self.paths.clone());
                    if let Some(existing) = interaction_mgr
                        .list_pending(Some(&intent.task_id), Some(&intent.work_run_id))?
                        .into_iter()
                        .find(|interaction| {
                            interaction.tool_call_id.as_deref()
                                == Some(intent.tool_call_id.as_str())
                                && interaction
                                    .payload
                                    .get("argumentsHash")
                                    .and_then(|value| value.as_str())
                                    == Some(args_hash.as_str())
                                && interaction
                                    .payload
                                    .get("executionLane")
                                    .and_then(|value| value.as_str())
                                    == Some("host_fallback")
                        })
                    {
                        return Ok(ToolResult {
                            tool_call_id: intent.tool_call_id.clone(),
                            tool_name: intent.tool_name.clone(),
                            action: intent.action.clone(),
                            success: false,
                            status: "waiting_approval".to_string(),
                            failure_kind: None,
                            exit_code: None,
                            stdout: String::new(),
                            stderr: "Work is waiting for one-shot host execution approval."
                                .to_string(),
                            outputs: Vec::new(),
                            interaction_id: Some(existing.interaction_id),
                            side_effect_class: side_effect,
                            started_at: started_at.clone(),
                            finished_at: Utc::now().to_rfc3339(),
                        });
                    }
                    if let Some(grant) = interaction_mgr.find_matching_grant(
                        &intent.task_id,
                        &intent.work_run_id,
                        &intent.tool_call_id,
                        &intent.tool_name,
                        &intent.action,
                        &args_hash,
                        &intent.workspace_id,
                    )? {
                        if grant.execution_lane.as_deref() == Some("host_fallback") {
                            interaction_mgr.consume_grant(&grant.grant_id)?;
                            force_host_execution = true;
                            ledger.record(&RuntimeFact::GrantConsumed {
                                grant_id: grant.grant_id,
                                tool_call_id: intent.tool_call_id.clone(),
                                timestamp: Utc::now().to_rfc3339(),
                            })?;
                            let task_mgr = crate::work::tasks::TaskManager::new(self.paths.clone());
                            if task_mgr
                                .get_run(&intent.task_id, &intent.work_run_id)
                                .is_ok()
                            {
                                let controller = crate::work::lifecycle::WorkHarnessController::new(
                                    self.paths.clone(),
                                );
                                controller.transition_run_status(
                                    &intent.task_id,
                                    &intent.work_run_id,
                                    crate::work::models::WorkRunStatus::Running,
                                )?;
                            }
                        }
                    }
                }
            }
        }

        // 3. Serialize serial/exclusive calls before declaring that execution
        // started. This closes the retry race where two identical external
        // calls could both pass a pre-dispatch idempotency check.
        let concurrency = Self::concurrency_class(&intent.tool_name);
        let gate = crate::work::coordinator::coordinator().get_or_create_gate(&intent.work_run_id);
        let _permit = gate.acquire(concurrency).await;

        // A durable final ToolResult means this exact call id has already
        // reached an authoritative outcome. Never replay it implicitly. The
        // original result body is intentionally not reconstructed here: a
        // caller must use the persisted Work run record/recovery UI rather
        // than turning a transport retry into a second external mutation.
        if let Some(RuntimeFact::ToolResult {
            success,
            status,
            failure_kind,
            exit_code,
            error,
            outputs,
            side_effect_class,
            timestamp,
            ..
        }) = ledger.get_tool_result(&intent.tool_call_id)
        {
            if !matches!(status.as_str(), "waiting_approval" | "waiting_input") {
                return Ok(ToolResult {
                    tool_call_id: intent.tool_call_id.clone(),
                    tool_name: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    success,
                    status,
                    failure_kind,
                    exit_code,
                    stdout: String::new(),
                    stderr: error.unwrap_or_else(|| {
                        format!(
                            "Tool call '{}' already has a durable result; implicit replay was blocked.",
                            intent.tool_call_id
                        )
                    }),
                    outputs,
                    interaction_id: None,
                    side_effect_class,
                    started_at: started_at.clone(),
                    finished_at: timestamp,
                });
            }
        }

        // Durable file-change pre-classification: observe the target BEFORE
        // dispatch mutates it. The fact itself is only written after the tool
        // result succeeds, so the ledger never records a speculative change.
        let durable_file_change: Option<crate::work::models::WorkFileChangeKind> =
            match intent.tool_name.as_str() {
                // An edit can only apply to existing content.
                "work_edit_file" => Some(crate::work::models::WorkFileChangeKind::Modified),
                "work_update_context" => {
                    let rel_path = intent
                        .arguments
                        .get("path")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim();
                    if rel_path.is_empty() {
                        None
                    } else {
                        let existed = validate_and_resolve_context_target(
                            &self.paths,
                            &intent.workspace_id,
                            rel_path,
                        )
                        .map(|target| target.exists())
                        .unwrap_or(false);
                        Some(if existed {
                            crate::work::models::WorkFileChangeKind::Modified
                        } else {
                            crate::work::models::WorkFileChangeKind::Created
                        })
                    }
                }
                "work_write_file" => {
                    let rel_path = intent
                        .arguments
                        .get("path")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim();
                    if rel_path.is_empty() {
                        None
                    } else {
                        let existed = if policy.execution_mode == WorkExecutionMode::FullAccess {
                            self.paths
                                .resolve_full_access_target(
                                    &intent.workspace_id,
                                    Some(&intent.work_run_id),
                                    rel_path,
                                    true,
                                )
                                .map(|target| target.exists())
                                .unwrap_or(false)
                        } else {
                            self.paths
                                .resolve_and_confine_target(
                                    &intent.workspace_id,
                                    Some(&intent.work_run_id),
                                    rel_path,
                                    true,
                                )
                                .map(|target| target.exists())
                                .unwrap_or(false)
                        };
                        Some(if existed {
                            crate::work::models::WorkFileChangeKind::Modified
                        } else {
                            crate::work::models::WorkFileChangeKind::Created
                        })
                    }
                }
                _ => None,
            };

        let execution_id = format!("exec-{}", uuid::Uuid::new_v4());
        ledger.record(&RuntimeFact::ToolStarted {
            tool_call_id: intent.tool_call_id.clone(),
            execution_id: execution_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
        })?;

        // 4. Dispatch execution (to native handlers or Host Executor)
        let mut exec_result = match self
            .dispatch_execution(intent, policy, proxy_url, force_host_execution)
            .await
        {
            Ok(result) => result,
            Err(err_msg) => {
                let finished_at = Utc::now().to_rfc3339();
                WorkExecutionResult {
                    execution_id: execution_id.clone(),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Failed,
                    failure_kind: Some(ExecutionFailureKind::CapabilityFailure),
                    exit_code: Some(-1),
                    stdout: String::new(),
                    stderr: err_msg,
                    outputs: Vec::new(),
                    started_at: started_at.clone(),
                    finished_at,
                }
            }
        };
        drop(_permit);

        // WorkBuddy-style default permissions keep ordinary commands inside
        // the sandbox, but a command that is incompatible with the OS sandbox
        // can be retried once on the host after an explicit Inbox approval.
        // The approval is bound to this exact tool call and argument hash; the
        // retry cannot silently widen the command or become a standing rule.
        if !force_host_execution
            && intent.tool_name == "work_run_command"
            && policy.execution_mode != WorkExecutionMode::FullAccess
            && matches!(
                exec_result.failure_kind,
                Some(
                    ExecutionFailureKind::SandboxDenied
                        | ExecutionFailureKind::SandboxInfrastructureFailure
                )
            )
        {
            let interaction_mgr = InteractionManager::new(self.paths.clone());
            let mut payload_json = serde_json::json!({
                "toolName": intent.tool_name,
                "action": intent.action,
                "argumentsHash": args_hash,
                "workspaceId": intent.workspace_id,
                "parameters": intent.arguments,
                "inputPaths": intent.input_paths,
                "expectedOutputs": intent.expected_outputs,
                "executionLane": "host_fallback",
                "sandboxFailureKind": exec_result.failure_kind,
                "sandboxStderr": exec_result.stderr,
            });
            payload_json["fallbackReason"] =
                serde_json::json!("该命令无法在 Work 沙盒中运行。是否仅对这一次调用允许主机执行？");

            let interaction = interaction_mgr.create_interaction(
                &intent.task_id,
                &intent.work_run_id,
                &intent.workspace_id,
                intent.session_id.as_deref(),
                None,
                Some(&intent.tool_call_id),
                PendingInteractionKind::Permission,
                &format!("需要主机执行授权：{target}"),
                "Work 沙盒阻止了这次命令。批准后将使用完全相同的命令、参数和工作目录，仅执行一次主机重试。",
                payload_json,
            )?;
            ledger.record(&RuntimeFact::ApprovalRequested {
                interaction_id: interaction.interaction_id.clone(),
                tool_call_id: Some(intent.tool_call_id.clone()),
                kind: PendingInteractionKind::Permission,
                timestamp: Utc::now().to_rfc3339(),
            })?;

            let task_mgr = crate::work::tasks::TaskManager::new(self.paths.clone());
            if task_mgr
                .get_run(&intent.task_id, &intent.work_run_id)
                .is_ok()
            {
                crate::work::lifecycle::WorkHarnessController::new(self.paths.clone())
                    .transition_run_status(
                        &intent.task_id,
                        &intent.work_run_id,
                        crate::work::models::WorkRunStatus::WaitingApproval,
                    )?;
            }

            let fallback_result = ToolResult {
                tool_call_id: intent.tool_call_id.clone(),
                tool_name: intent.tool_name.clone(),
                action: intent.action.clone(),
                success: false,
                status: "waiting_approval".to_string(),
                failure_kind: exec_result.failure_kind,
                exit_code: exec_result.exit_code,
                stdout: exec_result.stdout,
                stderr: format!(
                    "沙盒阻止了命令执行，等待一次性主机执行授权：{}",
                    exec_result.stderr
                ),
                outputs: Vec::new(),
                interaction_id: Some(interaction.interaction_id),
                side_effect_class: side_effect,
                started_at: started_at.clone(),
                finished_at: Utc::now().to_rfc3339(),
            };
            ledger.record(&RuntimeFact::ToolResult {
                tool_call_id: fallback_result.tool_call_id.clone(),
                success: false,
                status: fallback_result.status.clone(),
                failure_kind: fallback_result.failure_kind,
                exit_code: fallback_result.exit_code,
                error: Some(fallback_result.stderr.clone()),
                outputs: Vec::new(),
                side_effect_class: side_effect,
                timestamp: fallback_result.finished_at.clone(),
            })?;
            return Ok(fallback_result);
        }

        // Validate obligations: expected_outputs are obligations
        if exec_result.status == WorkExecutionStatus::Success && !intent.expected_outputs.is_empty()
        {
            for expected in &intent.expected_outputs {
                let expected_path = if intent.workspace_id.trim().is_empty() {
                    self.paths.resolve_standalone_path(
                        &intent.work_run_id,
                        Path::new(expected),
                        false,
                    )
                } else {
                    self.paths.resolve_workspace_path(
                        &intent.workspace_id,
                        Path::new(expected),
                        false,
                    )
                };
                let obligation_error = match expected_path {
                    Ok(path) if path.exists() => None,
                    Ok(_) => Some(format!(
                        "Obligation failed: expected output file '{expected}' was not created on disk"
                    )),
                    Err(error) => Some(format!(
                        "Obligation failed: expected output file '{expected}' could not be resolved: {error}"
                    )),
                };
                if let Some(err_msg) = obligation_error {
                    exec_result.status = WorkExecutionStatus::Failed;
                    exec_result.failure_kind = Some(ExecutionFailureKind::CapabilityFailure);
                    exec_result.exit_code = Some(1);
                    if !exec_result.stderr.is_empty() {
                        exec_result.stderr.push('\n');
                    }
                    exec_result.stderr.push_str(&err_msg);
                }
            }
        }

        let finished_at = Utc::now().to_rfc3339();

        let mut final_result = ToolResult {
            tool_call_id: intent.tool_call_id.clone(),
            tool_name: intent.tool_name.clone(),
            action: intent.action.clone(),
            success: exec_result.status == WorkExecutionStatus::Success,
            status: match exec_result.status {
                WorkExecutionStatus::Success => "success".to_string(),
                WorkExecutionStatus::Failed => "failed".to_string(),
                WorkExecutionStatus::TimedOut => "timeout".to_string(),
                WorkExecutionStatus::Denied => "denied".to_string(),
                WorkExecutionStatus::Cancelled => "cancelled".to_string(),
            },
            failure_kind: exec_result.failure_kind,
            exit_code: exec_result.exit_code,
            stdout: exec_result.stdout,
            stderr: exec_result.stderr,
            outputs: exec_result.outputs,
            interaction_id: None,
            side_effect_class: side_effect,
            started_at,
            finished_at: finished_at.clone(),
        };

        // Auto-register ONLY verified existing outputs under output/
        // (file reads legitimately list their path in outputs but never
        // produce artifacts, so they must not re-trigger registration.)
        if final_result.success && intent.tool_name != "work_read_file" {
            for out in &final_result.outputs {
                let normalized = out.trim().replace('\\', "/");
                if normalized.starts_with("output/") {
                    let file_path = if intent.workspace_id.trim().is_empty() {
                        self.paths.resolve_standalone_path(
                            &intent.work_run_id,
                            Path::new(&normalized),
                            false,
                        )
                    } else {
                        self.paths.resolve_workspace_path(
                            &intent.workspace_id,
                            Path::new(&normalized),
                            false,
                        )
                    };
                    let file_path = match file_path {
                        Ok(path) => path,
                        Err(error) => {
                            final_result.success = false;
                            final_result.status = "failed".to_string();
                            final_result.failure_kind =
                                Some(ExecutionFailureKind::CapabilityFailure);
                            final_result.exit_code = Some(1);
                            final_result.stderr = format!(
                                "Failed to resolve output artifact '{}': {}",
                                normalized, error
                            );
                            break;
                        }
                    };
                    if file_path.exists() {
                        let file_name = std::path::Path::new(&normalized)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Artifact");
                        let producer = Some(crate::work::models::ArtifactProducer {
                            run_id: intent.work_run_id.clone(),
                            producer_tool_call_id: Some(intent.tool_call_id.clone()),
                            execution_id: Some(execution_id.clone()),
                        });
                        let sources = crate::work::artifacts::extract_sources_from_ledger(
                            &self.paths,
                            &intent.task_id,
                            &intent.work_run_id,
                            &[],
                        );
                        let reg_res = if intent.workspace_id.trim().is_empty() {
                            crate::work::artifacts::register_standalone_with_paths(
                                &self.paths,
                                &intent.work_run_id,
                                &normalized,
                                file_name,
                                None,
                            )
                        } else {
                            crate::work::artifacts::register_with_provenance_and_paths(
                                &self.paths,
                                &intent.workspace_id,
                                &normalized,
                                file_name,
                                None,
                                Some(&intent.work_run_id),
                                producer,
                                sources,
                            )
                        };
                        if let Err(err) = reg_res {
                            final_result.success = false;
                            final_result.status = "failed".to_string();
                            final_result.failure_kind =
                                Some(ExecutionFailureKind::CapabilityFailure);
                            final_result.exit_code = Some(1);
                            final_result.stderr =
                                format!("Failed to register artifact '{}': {}", normalized, err);
                            break;
                        }
                    }
                }
            }
        }

        // 4.5 Durable file-change fact (Changes view in the Run Receipt).
        if final_result.success {
            if let Some(change_kind) = durable_file_change {
                let changed_path = intent
                    .arguments
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim()
                    .replace('\\', "/");
                if !changed_path.is_empty() {
                    ledger.record(&RuntimeFact::FileChanged {
                        tool_call_id: intent.tool_call_id.clone(),
                        path: changed_path,
                        change_kind,
                        timestamp: finished_at.clone(),
                    })?;
                }
            }
        }

        // 5. Log authoritative ToolResult to ledger
        ledger.record(&RuntimeFact::ToolResult {
            tool_call_id: intent.tool_call_id.clone(),
            success: final_result.success,
            status: final_result.status.clone(),
            failure_kind: final_result.failure_kind,
            exit_code: final_result.exit_code,
            error: if !final_result.success {
                Some(final_result.stderr.clone())
            } else {
                None
            },
            outputs: final_result.outputs.clone(),
            side_effect_class: side_effect,
            timestamp: finished_at,
        })?;

        // 6. Live Guardian Watchdog Evaluation. Guardian state is part of the
        // runtime safety boundary: a corrupt/unwritable ledger must fail the
        // execution visibly instead of allowing a successful tool result to
        // leave the Run looking healthy.
        let task_mgr = crate::work::tasks::TaskManager::new(self.paths.clone());
        let facts = ledger.list_facts().map_err(|error| {
            log::error!(
                "[pipeline] cannot evaluate Guardian because the runtime ledger is unreadable: {error}"
            );
            format!("Guardian health evaluation blocked by unreadable runtime ledger: {error}")
        })?;
        // Direct pipeline callers (including standalone tasks and low-level
        // bridge tests) may not have a product WorkRun manifest. Preserve
        // Guardian ledger evaluation for those calls while only projecting a
        // Recoverable lifecycle state when the WorkRun actually exists.
        let run = match task_mgr.get_run(&intent.task_id, &intent.work_run_id) {
            Ok(run) => Some(run),
            Err(error) if error.contains("not found") => None,
            Err(error) => return Err(error),
        };
        let run_status = run
            .as_ref()
            .map(|run| run.status)
            .unwrap_or(crate::work::models::WorkRunStatus::Running);
        let guardian_config = run
            .as_ref()
            .and_then(|run| run.guardian_config.clone())
            .unwrap_or_default();
        let now = Utc::now().to_rfc3339();
        let report = crate::work::guardian::Guardian::evaluate_health(
            &facts,
            run_status,
            &guardian_config,
            &now,
            None,
        );
        if let Err(error) = crate::work::guardian::Guardian::persist_health_report(
            &ledger,
            &report,
            Some(intent.tool_call_id.as_str()),
        ) {
            let reason = format!("Guardian health report could not be persisted: {error}");
            log::error!("[pipeline] {reason}");
            return if run.is_some() {
                match task_mgr.set_run_state(
                    &intent.task_id,
                    &intent.work_run_id,
                    crate::work::models::WorkRunStatus::Recoverable,
                    Some(reason.clone()),
                    None,
                ) {
                    Ok(_) => Err(reason),
                    Err(state_error) => Err(format!(
                        "{reason}; additionally failed to mark WorkRun recoverable: {state_error}"
                    )),
                }
            } else {
                Err(reason)
            };
        }
        if let Some(anomaly) = report.anomalies.iter().find(|anomaly| anomaly.is_critical) {
            if run_status == crate::work::models::WorkRunStatus::Running && run.is_some() {
                task_mgr.set_run_state(
                    &intent.task_id,
                    &intent.work_run_id,
                    crate::work::models::WorkRunStatus::Recoverable,
                    Some(anomaly.reason.clone()),
                    None,
                )?;
            }
        }

        Ok(final_result)
    }

    /// Dispatch execution to host executor or native Work tools.
    async fn dispatch_execution(
        &self,
        intent: &ToolIntent,
        policy: &WorkPolicy,
        proxy_url: Option<&str>,
        force_host_execution: bool,
    ) -> Result<WorkExecutionResult, String> {
        let started_at = Utc::now().to_rfc3339();
        let full_access = policy.execution_mode == WorkExecutionMode::FullAccess;
        match intent.tool_name.as_str() {
            "desktop_list_apps" | "desktop_probe_app" | "desktop_open_app" | "desktop_observe"
            | "desktop_screenshot" | "desktop_click" | "desktop_type" | "desktop_key"
            | "desktop_scroll" | "desktop_act_batch" | "desktop_release" => {
                let payload = crate::work::desktop_operator::desktop_operator_manager()
                    .execute(
                        &intent.work_run_id,
                        &intent.tool_name,
                        intent.arguments.clone(),
                    )
                    .await?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&payload)
                        .map_err(|error| format!("Failed to serialize desktop result: {error}"))?,
                    stderr: String::new(),
                    outputs: Vec::new(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "library_list" | "library_search" => {
                if intent.workspace_id.trim().is_empty() {
                    return Err("Library tools require a Workspace".to_string());
                }
                let query = intent
                    .arguments
                    .get("query")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let collection = intent
                    .arguments
                    .get("collection")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let category = intent
                    .arguments
                    .get("category")
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok());
                let items = crate::work::library::LibraryManager::new(self.paths.clone())
                    .list_items_filtered(Some(&intent.workspace_id), category, query, collection)?;
                let max_results = intent
                    .arguments
                    .get("max_results")
                    .or_else(|| intent.arguments.get("maxResults"))
                    .and_then(|value| value.as_u64())
                    .unwrap_or(50)
                    .min(200) as usize;
                let items = items.into_iter().take(max_results).collect::<Vec<_>>();
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&items)
                        .map_err(|error| format!("Failed to serialize Library result: {error}"))?,
                    stderr: String::new(),
                    outputs: items
                        .iter()
                        .map(|item| format!("library:{}", item.id))
                        .collect(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "library_read" => {
                if intent.workspace_id.trim().is_empty() {
                    return Err("Library tools require a Workspace".to_string());
                }
                let id = intent
                    .arguments
                    .get("id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "library_read requires a non-empty id".to_string())?;
                let max_chars = intent
                    .arguments
                    .get("max_chars")
                    .or_else(|| intent.arguments.get("maxChars"))
                    .and_then(|value| value.as_u64())
                    .unwrap_or(200_000)
                    .min(1_000_000) as usize;
                let item = crate::work::library::LibraryManager::new(self.paths.clone())
                    .get_item_scoped(id, Some(&intent.workspace_id))?;
                let content = item.content.chars().take(max_chars).collect::<String>();
                let truncated = item.content.chars().count() > max_chars;
                let payload = serde_json::json!({
                    "item": item,
                    "content": content,
                    "truncated": truncated,
                });
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&payload)
                        .map_err(|error| format!("Failed to serialize Library item: {error}"))?,
                    stderr: String::new(),
                    outputs: vec![format!("library:{id}")],
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_request_directory_access" => {
                let path_str = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let canonical_target = self
                    .paths
                    .validate_external_access_root(&intent.workspace_id, path_str)?;
                let writable = intent
                    .arguments
                    .get("writable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let ws_mgr = crate::work::workspace::WorkspaceManager::new(self.paths.clone());
                let ws = ws_mgr.get(&intent.workspace_id)?;

                let already_authorized = ws.access_roots.iter().any(|root| {
                    let root_path = std::path::Path::new(&root.path);
                    let canon_root = root_path
                        .canonicalize()
                        .unwrap_or_else(|_| root_path.to_path_buf());
                    canonical_target == canon_root && (!writable || root.writable)
                });

                if !already_authorized {
                    let canon_str = canonical_target.to_string_lossy().into_owned();
                    ws_mgr.add_access_root(&intent.workspace_id, &canon_str, writable)?;
                }

                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: format!(
                        "Authorized access root attached: {path_str} (writable: {writable})"
                    ),
                    stderr: String::new(),
                    outputs: Vec::new(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_register_artifact" => {
                let rel_path = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let title_arg = intent
                    .arguments
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let fallback_title = std::path::Path::new(rel_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Artifact");
                let title = title_arg.unwrap_or(fallback_title);
                let artifact_type = intent
                    .arguments
                    .get("artifact_type")
                    .or_else(|| intent.arguments.get("type"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());

                let artifact = if intent.workspace_id.trim().is_empty() {
                    crate::work::artifacts::register_standalone_with_paths(
                        &self.paths,
                        &intent.work_run_id,
                        rel_path,
                        title,
                        artifact_type,
                    )?
                } else {
                    let producer = Some(crate::work::models::ArtifactProducer {
                        run_id: intent.work_run_id.clone(),
                        producer_tool_call_id: Some(intent.tool_call_id.clone()),
                        execution_id: None,
                    });
                    crate::work::artifacts::register_with_provenance_and_paths(
                        &self.paths,
                        &intent.workspace_id,
                        rel_path,
                        title,
                        artifact_type,
                        Some(&intent.work_run_id),
                        producer,
                        Vec::new(),
                    )?
                };

                let stdout = serde_json::to_string(&artifact)
                    .map_err(|e| format!("Failed to serialize artifact: {e}"))?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout,
                    stderr: String::new(),
                    outputs: vec![artifact.path.clone()],
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_list_artifacts" => {
                let artifacts = if intent.workspace_id.trim().is_empty() {
                    crate::work::artifacts::list_standalone_with_paths(
                        &self.paths,
                        &intent.work_run_id,
                    )?
                } else {
                    crate::work::artifacts::list_with_paths(
                        &self.paths,
                        &intent.workspace_id,
                        Some(&intent.work_run_id),
                    )?
                };

                let stdout = serde_json::to_string(&artifacts)
                    .map_err(|e| format!("Failed to serialize artifacts list: {e}"))?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout,
                    stderr: String::new(),
                    outputs: Vec::new(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_validate_artifact" => {
                let id_opt = intent
                    .arguments
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let path_opt = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());

                let artifact_id = if let Some(id) = id_opt {
                    id.to_string()
                } else if let Some(p) = path_opt {
                    let artifacts = if intent.workspace_id.trim().is_empty() {
                        crate::work::artifacts::list_standalone_with_paths(
                            &self.paths,
                            &intent.work_run_id,
                        )?
                    } else {
                        crate::work::artifacts::list_with_paths(
                            &self.paths,
                            &intent.workspace_id,
                            Some(&intent.work_run_id),
                        )?
                    };
                    artifacts
                        .into_iter()
                        .find(|a| a.path == p || a.path.ends_with(p))
                        .map(|a| a.id)
                        .ok_or_else(|| format!("Artifact not found with path: {p}"))?
                } else {
                    return Err("work_validate_artifact requires 'id' or 'path'".to_string());
                };

                let summary = if intent.workspace_id.trim().is_empty() {
                    crate::work::artifacts::validate_standalone(&intent.work_run_id, &artifact_id)?
                } else {
                    crate::work::artifacts::validate_with_paths(
                        &self.paths,
                        &intent.workspace_id,
                        &artifact_id,
                        Some(&intent.work_run_id),
                    )?
                };

                let stdout = serde_json::to_string(&summary)
                    .map_err(|e| format!("Failed to serialize validated artifact: {e}"))?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout,
                    stderr: String::new(),
                    outputs: vec![summary.path.clone()],
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_deliver" => {
                let id_opt = intent
                    .arguments
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let path_opt = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());

                let artifact_id = if let Some(id) = id_opt {
                    id.to_string()
                } else if let Some(p) = path_opt {
                    let artifacts = if intent.workspace_id.trim().is_empty() {
                        crate::work::artifacts::list_standalone_with_paths(
                            &self.paths,
                            &intent.work_run_id,
                        )?
                    } else {
                        crate::work::artifacts::list_with_paths(
                            &self.paths,
                            &intent.workspace_id,
                            Some(&intent.work_run_id),
                        )?
                    };
                    artifacts
                        .into_iter()
                        .find(|a| a.path == p || a.path.ends_with(p))
                        .map(|a| a.id)
                        .ok_or_else(|| format!("Artifact not found with path: {p}"))?
                } else {
                    return Err("work_deliver requires 'id' or 'path'".to_string());
                };

                let summary = if intent.workspace_id.trim().is_empty() {
                    crate::work::artifacts::deliver_standalone(&intent.work_run_id, &artifact_id)?
                } else {
                    crate::work::artifacts::deliver_with_paths(
                        &self.paths,
                        &intent.workspace_id,
                        &artifact_id,
                        Some(&intent.work_run_id),
                    )?
                };

                let stdout = serde_json::to_string(&summary)
                    .map_err(|e| format!("Failed to serialize delivered artifact: {e}"))?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout,
                    stderr: String::new(),
                    outputs: vec![summary.path.clone()],
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_update_context" => {
                let rel_path = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = intent
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let target_path = validate_and_resolve_context_target(
                    &self.paths,
                    &intent.workspace_id,
                    rel_path,
                )?;

                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                crate::work::models::atomic_replace_file(&target_path, content.as_bytes())?;

                let normalized = rel_path.trim().replace('\\', "/");
                let outputs = vec![normalized];

                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: format!("Updated context file {target_path:?}"),
                    stderr: String::new(),
                    outputs,
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_write_file" => {
                let rel_path = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = intent
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let target_path = if full_access {
                    self.paths.resolve_full_access_target(
                        &intent.workspace_id,
                        Some(&intent.work_run_id),
                        rel_path,
                        true,
                    )?
                } else {
                    self.paths.resolve_and_confine_target(
                        &intent.workspace_id,
                        Some(&intent.work_run_id),
                        rel_path,
                        true,
                    )?
                };
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                crate::work::models::atomic_replace_file(&target_path, content.as_bytes())?;

                let normalized = rel_path.trim().replace('\\', "/");
                let outputs = vec![normalized.clone()];
                if normalized.starts_with("output/") {
                    let file_name = std::path::Path::new(&normalized)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Artifact");
                    let registration = if intent.workspace_id.trim().is_empty() {
                        crate::work::artifacts::register_standalone_with_paths(
                            &self.paths,
                            &intent.work_run_id,
                            &normalized,
                            file_name,
                            None,
                        )
                    } else {
                        crate::work::artifacts::register_with_paths(
                            &self.paths,
                            &intent.workspace_id,
                            &normalized,
                            file_name,
                            None,
                            Some(&intent.work_run_id),
                        )
                    };
                    if let Err(error) = registration {
                        return Err(format!(
                            "Output file was written, but Artifact registration failed: {error}"
                        ));
                    }
                }

                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: format!(
                        "Successfully wrote {} bytes to {}",
                        content.len(),
                        normalized
                    ),
                    stderr: String::new(),
                    outputs,
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_edit_file" => {
                let rel_path = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let old_text = intent
                    .arguments
                    .get("old_text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new_text = intent
                    .arguments
                    .get("new_text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let target_path = if full_access {
                    self.paths.resolve_full_access_target(
                        &intent.workspace_id,
                        Some(&intent.work_run_id),
                        rel_path,
                        true,
                    )?
                } else {
                    self.paths.resolve_and_confine_target(
                        &intent.workspace_id,
                        Some(&intent.work_run_id),
                        rel_path,
                        true,
                    )?
                };
                let existing = std::fs::read_to_string(&target_path).map_err(|e| e.to_string())?;
                let occurrences = existing.matches(old_text).count();
                let normalized = rel_path.trim().replace('\\', "/");
                if occurrences == 0 {
                    return Err(format!("old_text not found in {normalized}"));
                }
                if occurrences > 1 {
                    return Err(format!(
                        "old_text appears {occurrences} times in {normalized}; must be unique"
                    ));
                }
                let updated = existing.replacen(old_text, new_text, 1);
                crate::work::models::atomic_replace_file(&target_path, updated.as_bytes())?;

                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: format!("Successfully edited {}", normalized),
                    stderr: String::new(),
                    outputs: vec![normalized],
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_read_file" => {
                let rel_path = intent
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let target_path = if full_access {
                    self.paths.resolve_full_access_target(
                        &intent.workspace_id,
                        Some(&intent.work_run_id),
                        rel_path,
                        false,
                    )?
                } else {
                    self.paths.resolve_and_confine_target(
                        &intent.workspace_id,
                        Some(&intent.work_run_id),
                        rel_path,
                        false,
                    )?
                };
                let content = std::fs::read_to_string(&target_path).map_err(|e| e.to_string())?;
                let capped = if content.len() > 1024 * 1024 {
                    format!("{}... [truncated]", &content[..1024 * 1024])
                } else {
                    content
                };
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: capped,
                    stderr: String::new(),
                    // Record the path actually read so the Run Receipt can
                    // aggregate real input files from the ledger.
                    outputs: vec![rel_path.trim().replace('\\', "/")],
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_command_info" => {
                let command_name = intent
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let info = crate::work::command_info::inspect_command(command_name).await;
                let stdout =
                    serde_json::to_string_pretty(&info).unwrap_or_else(|_| format!("{:?}", info));
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout,
                    stderr: String::new(),
                    outputs: Vec::new(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_run_connector_cli" => {
                let package_id = intent
                    .arguments
                    .get("package_id")
                    .or_else(|| intent.arguments.get("packageId"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                let operation = intent
                    .arguments
                    .get("operation")
                    .or_else(|| intent.arguments.get("action"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                let extra_args: Vec<String> = intent
                    .arguments
                    .get("args")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let invocation = crate::work::connector_package_manager::resolve_cli_invocation(
                    &self.paths,
                    package_id,
                    operation,
                    &extra_args,
                )?;
                let mut result = crate::work::executor::command_runner::run_command(
                    &self.paths,
                    &intent.workspace_id,
                    Some(&intent.work_run_id),
                    &intent.tool_name,
                    &invocation.operation,
                    &invocation.command,
                    &invocation.args,
                    &invocation.cwd,
                    invocation.timeout_seconds,
                    &intent.expected_outputs,
                    &invocation.home_paths,
                    full_access,
                    false,
                )
                .await?;
                result.stdout = crate::work::connector_package_manager::redact_cli_output(
                    &result.stdout,
                    &invocation.redaction_fields,
                );
                result.stderr = crate::work::connector_package_manager::redact_cli_output(
                    &result.stderr,
                    &invocation.redaction_fields,
                );
                if result.status == WorkExecutionStatus::Success {
                    if let Err(error) = crate::work::connector_package_manager::record_cli_result(
                        &self.paths,
                        &invocation,
                        &result,
                    ) {
                        result.status = WorkExecutionStatus::Failed;
                        result.failure_kind = Some(ExecutionFailureKind::CapabilityFailure);
                        result.exit_code = Some(1);
                        if !result.stderr.is_empty() {
                            result.stderr.push('\n');
                        }
                        result.stderr.push_str(&format!(
                            "Failed to persist Connector Package CLI state: {error}"
                        ));
                    }
                }
                Ok(result)
            }
            "web_search" => {
                let query = intent
                    .arguments
                    .get("query")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "web_search requires a non-empty query".to_string())?;
                let max_results = intent
                    .arguments
                    .get("max_results")
                    .or_else(|| intent.arguments.get("maxResults"))
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u32);
                let results = crate::work::web::search::execute_web_search(
                    &self.paths,
                    query,
                    max_results,
                    proxy_url,
                )
                .await?;
                // Record surfaced URLs so the Run Receipt's Sources view is
                // backed by the durable ledger, not by model narration.
                let surfaced_urls = results
                    .iter()
                    .map(|item| item.url.trim().to_string())
                    .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                    .collect::<Vec<_>>();
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&results).map_err(|error| {
                        format!("Failed to serialize web search result: {error}")
                    })?,
                    stderr: String::new(),
                    outputs: surfaced_urls,
                    started_at: started_at.clone(),
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "web_open" | "web_fetch" => {
                let url = intent
                    .arguments
                    .get("url")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "web_open requires a non-empty url".to_string())?;
                let result = crate::work::web::fetch::execute_web_fetch(url, proxy_url).await?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&result).map_err(|error| {
                        format!("Failed to serialize web fetch result: {error}")
                    })?,
                    stderr: String::new(),
                    // The fetch succeeded, so this URL is a genuinely accessed source.
                    outputs: vec![url.to_string()],
                    started_at: started_at.clone(),
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_mcp_list" => {
                let server = intent
                    .arguments
                    .get("server")
                    .or_else(|| intent.arguments.get("serverName"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "work_mcp_list requires a non-empty server".to_string())?;
                if server.chars().count() > 80 || server.chars().any(char::is_control) {
                    return Err("work_mcp_list server name is invalid".into());
                }
                let result =
                    crate::work::mcp::list_tools_with_paths(&self.paths, server, proxy_url).await?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&result).map_err(|error| {
                        format!("Failed to serialize MCP tools/list result: {error}")
                    })?,
                    stderr: String::new(),
                    outputs: Vec::new(),
                    started_at: started_at.clone(),
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_mcp_call" => {
                let server = intent
                    .arguments
                    .get("server")
                    .or_else(|| intent.arguments.get("serverName"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "work_mcp_call requires a non-empty server".to_string())?;
                if server.chars().count() > 80 || server.chars().any(char::is_control) {
                    return Err("work_mcp_call server name is invalid".into());
                }
                let tool_name = intent
                    .arguments
                    .get("tool_name")
                    .or_else(|| intent.arguments.get("toolName"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "work_mcp_call requires a non-empty tool_name".to_string())?;
                let call_arguments = intent
                    .arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                if !call_arguments.is_object() {
                    return Err("work_mcp_call arguments must be an object".into());
                }
                let result = crate::work::mcp::call_with_paths(
                    &self.paths,
                    server,
                    tool_name,
                    &call_arguments,
                    proxy_url,
                )
                .await?;
                let server_reported_error = result
                    .get("isError")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: if server_reported_error {
                        WorkExecutionStatus::Failed
                    } else {
                        WorkExecutionStatus::Success
                    },
                    failure_kind: server_reported_error
                        .then_some(ExecutionFailureKind::CapabilityFailure),
                    exit_code: Some(if server_reported_error { 1 } else { 0 }),
                    stdout: serde_json::to_string(&result).map_err(|error| {
                        format!("Failed to serialize MCP tools/call result: {error}")
                    })?,
                    stderr: if server_reported_error {
                        "MCP Server reported a tool execution error".into()
                    } else {
                        String::new()
                    },
                    outputs: Vec::new(),
                    started_at: started_at.clone(),
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "work_call_app" => {
                let app_id = intent
                    .arguments
                    .get("app_id")
                    .or_else(|| intent.arguments.get("appId"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "work_call_app requires a non-empty app_id".to_string())?
                    .to_lowercase();
                let provider_tool_name = intent
                    .arguments
                    .get("tool_name")
                    .or_else(|| intent.arguments.get("toolName"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(intent.action.as_str());
                if provider_tool_name.is_empty() {
                    return Err("work_call_app requires a non-empty provider tool name".into());
                }
                let call_arguments = intent
                    .arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let connection = crate::work::apps::storage::get_connection(&self.paths, &app_id)?
                    .ok_or_else(|| format!("App '{app_id}' is not connected"))?;
                if connection.status != crate::work::apps::models::ConnectionStatus::Connected
                    || connection.accounts.is_empty()
                {
                    return Err(format!("App '{app_id}' has no connected account"));
                }
                let workspace_default_account = if intent.workspace_id.trim().is_empty() {
                    None
                } else {
                    crate::work::apps::storage::get_workspace_default_account(
                        &self.paths,
                        &intent.workspace_id,
                        &app_id,
                    )?
                };
                let requested_account = intent
                    .arguments
                    .get("account_id")
                    .or_else(|| intent.arguments.get("accountId"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or(workspace_default_account)
                    .or_else(|| {
                        (connection.accounts.len() == 1)
                            .then(|| connection.accounts[0].account_id.clone())
                    });
                let account_id = requested_account.ok_or_else(|| {
                    format!(
                        "App '{app_id}' has multiple connected accounts; an account_id is required"
                    )
                })?;
                if !connection
                    .accounts
                    .iter()
                    .any(|account| account.account_id == account_id)
                {
                    return Err(format!(
                        "Account '{}' is not connected to app '{}'",
                        account_id, app_id
                    ));
                }
                let provider = crate::work::apps::provider::resolve_provider_for(
                    &self.paths,
                    Some(&connection.provider),
                )?;
                let result = provider
                    .call_tool(
                        &app_id,
                        provider_tool_name,
                        &call_arguments,
                        Some(&account_id),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&result)
                        .map_err(|error| format!("Failed to serialize app tool result: {error}"))?,
                    stderr: String::new(),
                    outputs: Vec::new(),
                    started_at: started_at.clone(),
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "browser_navigate" => {
                let url = intent
                    .arguments
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let wait_ms = intent.arguments.get("wait_ms").and_then(|v| v.as_u64());
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_navigate",
                        serde_json::json!({ "url": url, "wait_ms": wait_ms }),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        // A successful navigation is a real page visit; record
                        // the URL so Sources/Browser history can show it.
                        outputs: if url.starts_with("http://") || url.starts_with("https://") {
                            vec![url.to_string()]
                        } else {
                            Vec::new()
                        },
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_cdp_observe" | "browser_cdp_act" => {
                let manager = crate::work::browser_operator::browser_operator_manager();
                let val = manager
                    .execute(
                        &intent.work_run_id,
                        &intent.tool_name,
                        intent.arguments.clone(),
                    )
                    .await?;
                Ok(WorkExecutionResult {
                    execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                    resource_id: intent.tool_name.clone(),
                    action: intent.action.clone(),
                    status: WorkExecutionStatus::Success,
                    failure_kind: None,
                    exit_code: Some(0),
                    stdout: serde_json::to_string(&val).unwrap_or_default(),
                    stderr: String::new(),
                    outputs: Vec::new(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                })
            }
            "browser_snapshot" => {
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_snapshot",
                        serde_json::json!({}),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_take_screenshot" => {
                let filename = intent.arguments.get("filename").and_then(|v| v.as_str());
                let full_page = intent.arguments.get("full_page").and_then(|v| v.as_bool());

                // Enforce output path constraint strictly within output/
                let resolved_path = if let Some(fname) = filename {
                    let safe_name = std::path::Path::new(fname)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .filter(|name| !name.is_empty())
                        .ok_or_else(|| {
                            "browser_take_screenshot filename must name a file".to_string()
                        })?;
                    let ws_dir = if intent.workspace_id.trim().is_empty() {
                        self.paths.standalone_task_dir(&intent.work_run_id)?
                    } else {
                        self.paths.workspace_dir(&intent.workspace_id)?
                    };
                    let out_dir = ws_dir.join("output");
                    std::fs::create_dir_all(&out_dir).map_err(|error| {
                        format!(
                            "Failed to create screenshot output directory {}: {error}",
                            out_dir.display()
                        )
                    })?;
                    Some(out_dir.join(safe_name).to_string_lossy().into_owned())
                } else {
                    None
                };

                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_take_screenshot",
                        serde_json::json!({
                            "outputPath": resolved_path.clone(),
                            "fullPage": full_page,
                        }),
                    )
                    .await
                {
                    Ok(val) => {
                        let outputs = if let Some(p) = resolved_path {
                            let file_name = std::path::Path::new(&p)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            vec![format!("output/{}", file_name)]
                        } else {
                            Vec::new()
                        };
                        Ok(WorkExecutionResult {
                            execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                            resource_id: intent.tool_name.clone(),
                            action: intent.action.clone(),
                            status: WorkExecutionStatus::Success,
                            failure_kind: None,
                            exit_code: Some(0),
                            stdout: serde_json::to_string(&val).unwrap_or_default(),
                            stderr: String::new(),
                            outputs,
                            started_at: started_at.clone(),
                            finished_at: Utc::now().to_rfc3339(),
                        })
                    }
                    Err(err) => Err(err),
                }
            }
            "browser_wait_for" => {
                let ms = intent.arguments.get("ms").and_then(|v| v.as_u64());
                let load_state = intent
                    .arguments
                    .get("load_state")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_wait_for",
                        serde_json::json!({ "ms": ms, "load_state": load_state }),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_tabs" => {
                let action = intent
                    .arguments
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list");
                let index = intent
                    .arguments
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize);
                let url = intent
                    .arguments
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_tabs",
                        serde_json::json!({
                            "action": action,
                            "index": index,
                            "url": url,
                        }),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_close" => {
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(&intent.work_run_id, "browser_close", serde_json::json!({}))
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_click" => {
                let ref_id = intent
                    .arguments
                    .get("ref")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let selector = intent
                    .arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let button = intent
                    .arguments
                    .get("button")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let double_click = intent
                    .arguments
                    .get("double_click")
                    .and_then(|v| v.as_bool());
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_click",
                        serde_json::json!({
                            "ref": ref_id,
                            "selector": selector,
                            "button": button,
                            "double_click": double_click,
                        }),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_type" => {
                let ref_id = intent
                    .arguments
                    .get("ref")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let text = intent
                    .arguments
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let selector = intent
                    .arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let clear = intent.arguments.get("clear").and_then(|v| v.as_bool());
                let press_enter = intent
                    .arguments
                    .get("press_enter")
                    .and_then(|v| v.as_bool());
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_type",
                        serde_json::json!({
                            "ref": ref_id,
                            "text": text,
                            "selector": selector,
                            "clear": clear,
                            "press_enter": press_enter,
                        }),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_select_option" => {
                let ref_id = intent
                    .arguments
                    .get("ref")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let value = intent
                    .arguments
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let selector = intent
                    .arguments
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_select_option",
                        serde_json::json!({
                            "ref": ref_id,
                            "value": value,
                            "selector": selector,
                        }),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "browser_scroll" => {
                let direction = intent
                    .arguments
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let amount = intent.arguments.get("amount").and_then(|v| v.as_u64());
                let ref_id = intent
                    .arguments
                    .get("ref")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let manager = crate::work::browser_operator::browser_operator_manager();
                match manager
                    .execute(
                        &intent.work_run_id,
                        "browser_scroll",
                        serde_json::json!({
                            "direction": direction,
                            "amount": amount,
                            "ref": ref_id,
                        }),
                    )
                    .await
                {
                    Ok(val) => Ok(WorkExecutionResult {
                        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
                        resource_id: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        status: WorkExecutionStatus::Success,
                        failure_kind: None,
                        exit_code: Some(0),
                        stdout: serde_json::to_string(&val).unwrap_or_default(),
                        stderr: String::new(),
                        outputs: Vec::new(),
                        started_at: started_at.clone(),
                        finished_at: Utc::now().to_rfc3339(),
                    }),
                    Err(err) => Err(err),
                }
            }
            "work_run_command" => {
                let command = intent
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let args: Vec<String> = intent
                    .arguments
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| a.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(error) = Self::direct_connector_cli_guard(command) {
                    return Err(error.to_string());
                }
                let cwd_default = if intent.workspace_id.trim().is_empty() {
                    "."
                } else {
                    "scratch"
                };
                let cwd_rel = intent
                    .arguments
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or(cwd_default);
                let timeout_secs = intent
                    .arguments
                    .get("timeout_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(120);
                crate::work::executor::command_runner::run_command(
                    &self.paths,
                    &intent.workspace_id,
                    Some(&intent.work_run_id),
                    &intent.tool_name,
                    &intent.action,
                    command,
                    &args,
                    cwd_rel,
                    timeout_secs,
                    &intent.expected_outputs,
                    &[],
                    full_access,
                    force_host_execution,
                )
                .await
            }
            _ => {
                let resource_id = if intent.tool_name == "work_execute" {
                    intent
                        .arguments
                        .get("resource_id")
                        .or_else(|| intent.arguments.get("resourceId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&intent.tool_name)
                        .to_string()
                } else {
                    intent.tool_name.clone()
                };

                let action = if intent.tool_name == "work_execute" {
                    intent
                        .arguments
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&intent.action)
                        .to_string()
                } else {
                    intent.action.clone()
                };

                let request = WorkExecutionRequest {
                    workspace_id: intent.workspace_id.clone(),
                    resource_id: resource_id.clone(),
                    action,
                    arguments: intent.arguments.clone(),
                    input_paths: intent.input_paths.clone(),
                    expected_outputs: intent.expected_outputs.clone(),
                    task_id: Some(intent.task_id.clone()),
                    work_run_id: Some(intent.work_run_id.clone()),
                };

                let (manifest, directory, is_builtin) =
                    resources::get_resource_with_paths(&self.paths, &request.resource_id)
                        .ok_or_else(|| {
                            format!("Work capability '{}' not found", request.resource_id)
                        })?;

                PolicyEvaluator::evaluate_execution(&manifest, &request, is_builtin)?;

                self.executor
                    .execute(&self.paths, &manifest, &directory, &request, is_builtin)
                    .await
            }
        }
    }

    /// Execute a batch of tool calls with concurrency control while preserving model call order.
    pub async fn execute_batch(
        &self,
        intents: Vec<ToolIntent>,
        policy: &WorkPolicy,
    ) -> Vec<ToolResult> {
        let total = intents.len();
        let mut results: Vec<Option<ToolResult>> = vec![None; total];
        let mut interrupted = false;
        let mut i = 0;

        while i < total {
            if interrupted {
                results[i] = Some(ToolResult::synthetic_interrupted(
                    &intents[i].tool_call_id,
                    &intents[i].tool_name,
                    &intents[i].action,
                    "Previous tool in batch failed or was cancelled",
                ));
                i += 1;
                continue;
            }

            let concurrency = Self::concurrency_class(&intents[i].tool_name);
            if concurrency == ToolConcurrencyClass::ParallelSafe {
                let start = i;
                while i < total
                    && Self::concurrency_class(&intents[i].tool_name)
                        == ToolConcurrencyClass::ParallelSafe
                {
                    i += 1;
                }
                let end = i;

                let mut join_set = tokio::task::JoinSet::new();
                #[allow(clippy::needless_range_loop)]
                for idx in start..end {
                    let intent = intents[idx].clone();
                    let policy_clone = policy.clone();
                    let this = self.clone();

                    join_set.spawn(async move {
                        let res = this.execute_intent(&intent, &policy_clone).await;
                        (idx, res)
                    });
                }

                while let Some(res) = join_set.join_next().await {
                    if let Ok((idx, task_res)) = res {
                        match task_res {
                            Ok(tool_res) => {
                                if !tool_res.success && tool_res.status == "interrupted" {
                                    interrupted = true;
                                }
                                results[idx] = Some(tool_res);
                            }
                            Err(e) => {
                                results[idx] = Some(ToolResult::denied(
                                    &intents[idx].tool_call_id,
                                    &intents[idx].tool_name,
                                    &intents[idx].action,
                                    &e,
                                ));
                            }
                        }
                    }
                }
            } else {
                let intent = &intents[i];
                let res = match self.execute_intent(intent, policy).await {
                    Ok(r) => {
                        if !r.success && r.status == "interrupted" {
                            interrupted = true;
                        }
                        r
                    }
                    Err(err) => ToolResult {
                        tool_call_id: intent.tool_call_id.clone(),
                        tool_name: intent.tool_name.clone(),
                        action: intent.action.clone(),
                        success: false,
                        status: "failed".to_string(),
                        failure_kind: None,
                        exit_code: Some(-1),
                        stdout: String::new(),
                        stderr: err,
                        outputs: Vec::new(),
                        interaction_id: None,
                        side_effect_class: Self::side_effect_class(&intent.tool_name),
                        started_at: Utc::now().to_rfc3339(),
                        finished_at: Utc::now().to_rfc3339(),
                    },
                };
                results[i] = Some(res);
                i += 1;
            }
        }

        results
            .into_iter()
            .enumerate()
            .map(|(idx, r)| {
                r.unwrap_or_else(|| {
                    ToolResult::denied(
                        &intents[idx].tool_call_id,
                        &intents[idx].tool_name,
                        &intents[idx].action,
                        "Execution failed",
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
