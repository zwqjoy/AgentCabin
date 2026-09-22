//! Golden Acceptance Harness for Work Mode.
//!
//! Provides `GoldenHarness` and reusable assertion helpers that execute
//! full business scenario lifecycles across Workspace, Run, Policy,
//! Interaction, ToolPipeline, Ledger, Recovery, Artifact, and Subagents.

use std::sync::{Arc, Mutex, MutexGuard};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::models::{RunMeta, RunStatus};
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::artifacts::{self, ArtifactValidator};
use crate::work::interaction::InteractionManager;
use crate::work::ledger::WorkRuntimeLedger;
use crate::work::lifecycle::WorkHarnessController;
use crate::work::models::{
    AppMode, ApprovalGrant, ExecutionContext, InboxItemStatus, PendingInteractionState,
    RuntimeFact, WorkArtifactSummary, WorkExecutionMode, WorkPolicy, WorkProjection, WorkRun,
    WorkRunStatus, WorkRunTrigger, WorkTask, WorkTaskCheckpoint, WorkWorkspace,
};
use crate::work::paths::WorkPaths;
use crate::work::pipeline::{ToolIntent, ToolPipeline, ToolResult};
use crate::work::projection::project_work_projection;
use crate::work::tasks::TaskManager;
use crate::work::workspace::WorkspaceManager;

/// Global test mutex to serialize tests that touch `AGENTCABIN_DATA_DIR`
/// and process-wide singletons.
pub static GOLDEN_TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldenScenarioResult {
    pub scenario: String,
    pub passed: bool,
    pub final_run_status: String,
    pub assertions: GoldenScenarioAssertions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldenScenarioAssertions {
    pub approval_persisted: bool,
    pub duplicate_executions: usize,
    pub artifact_satisfied: bool,
    pub pending_interactions: usize,
}

pub struct GoldenHarness {
    pub temp_dir: tempfile::TempDir,
    pub paths: WorkPaths,
    pub workspace: WorkWorkspace,
    pub task: WorkTask,
    pub run: WorkRun,
    pub tool_pipeline: ToolPipeline,
    pub task_manager: TaskManager,
    pub interaction_manager: InteractionManager,
    pub controller: WorkHarnessController,
    pub emitter: Arc<BroadcastEmitter>,
    _env_guard: MutexGuard<'static, ()>,
}

impl GoldenHarness {
    /// Initialize a new isolated Golden test harness with production components.
    pub fn new(
        workspace_name: &str,
        task_title: &str,
        task_instructions: &str,
        execution_mode: WorkExecutionMode,
    ) -> Result<Self, String> {
        let guard = GOLDEN_TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_dir = tempfile::TempDir::new().map_err(|e| e.to_string())?;
        std::env::set_var("AGENTCABIN_DATA_DIR", temp_dir.path());
        crate::storage::runs::invalidate_runs_cache();
        crate::work::subagents::registry().reset_for_test();

        let paths = WorkPaths::new(temp_dir.path().to_path_buf());
        paths.ensure_layout()?;

        let ws_mgr = WorkspaceManager::new(paths.clone());
        let workspace = ws_mgr.create(workspace_name)?;

        let task_mgr = TaskManager::new(paths.clone());
        let mut policy = WorkPolicy::default_for_workspace();
        policy.execution_mode = execution_mode;

        let task =
            task_mgr.create_task(&workspace.id, task_title, task_instructions, Some(policy))?;

        let run = task_mgr.start_run(&task.id, None, WorkRunTrigger::Manual)?;

        // Also persist RunMeta into runs storage so session and projection modules have authority
        let meta = RunMeta {
            id: run.id.clone(),
            prompt: task_instructions.to_string(),
            cwd: paths
                .workspace_dir(&workspace.id)?
                .to_string_lossy()
                .to_string(),
            agent: "pi".to_string(),
            app_mode: AppMode::Work,
            agent_target: Some(crate::models::AgentTarget::Work),
            workspace_id: Some(workspace.id.clone()),
            work_task_id: Some(task.id.clone()),
            work_run_id: Some(run.id.clone()),
            work_execution_context: Some(ExecutionContext::Attended),
            work_preset: None,
            auth_mode: "default".to_string(),
            status: RunStatus::Running,
            started_at: crate::models::now_iso(),
            ended_at: None,
            exit_code: None,
            error_message: None,
            session_id: None,
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
        };
        crate::storage::runs::save_meta(&meta)?;

        let interaction_mgr = InteractionManager::new(paths.clone());
        let controller = WorkHarnessController::new(paths.clone());
        let tool_pipeline = ToolPipeline::new(paths.clone());
        let emitter = BroadcastEmitter::mock();

        Ok(Self {
            temp_dir,
            paths,
            workspace,
            task,
            run,
            tool_pipeline,
            task_manager: task_mgr,
            interaction_manager: interaction_mgr,
            controller,
            emitter,
            _env_guard: guard,
        })
    }

    /// Execute a tool intent through the real Harness ToolPipeline.
    pub async fn tool(&self, intent: &ToolIntent) -> Result<ToolResult, String> {
        self.tool_pipeline
            .execute_intent(intent, &self.task.policy)
            .await
    }

    /// Approve an interaction using the production InteractionManager / Grant issuance.
    pub fn approve(&self, interaction_id: &str) -> Result<ApprovalGrant, String> {
        let (_delivering, issued_grant, is_first) = self
            .interaction_manager
            .prepare_resolution_and_issue_grant(
                interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
                Some(serde_json::json!({"decision": "allow"})),
            )?;
        if is_first {
            self.interaction_manager.commit_resolution(
                interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Approved),
            )?;
        }
        issued_grant.ok_or_else(|| "No grant issued upon approval".to_string())
    }

    /// Reject an interaction using the production InteractionManager.
    pub fn reject(&self, interaction_id: &str) -> Result<ApprovalGrant, String> {
        let (_delivering, issued_grant, is_first) = self
            .interaction_manager
            .prepare_resolution_and_issue_grant(
                interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Rejected),
                Some(serde_json::json!({"decision": "deny"})),
            )?;
        if is_first {
            self.interaction_manager.commit_resolution(
                interaction_id,
                PendingInteractionState::Resolved,
                Some(InboxItemStatus::Rejected),
            )?;
        }
        issued_grant.ok_or_else(|| "No grant issued upon rejection".to_string())
    }

    /// Simulate a durable application restart:
    /// - Discard/rebuild managers and controllers
    /// - Invalidate caches
    /// - Execute `reconcile_on_restart`
    /// - Reload task and run from disk
    pub fn restart(&mut self) -> Result<(), String> {
        crate::storage::runs::invalidate_runs_cache();

        self.task_manager = TaskManager::new(self.paths.clone());
        self.interaction_manager = InteractionManager::new(self.paths.clone());
        self.controller = WorkHarnessController::new(self.paths.clone());
        self.tool_pipeline = ToolPipeline::new(self.paths.clone());

        self.controller.reconcile_on_restart()?;

        self.task = self.task_manager.get_task(&self.task.id)?;
        self.run = self.task_manager.get_run(&self.task.id, &self.run.id)?;
        Ok(())
    }

    /// Project the unified authoritative WorkProjection.
    pub fn projection(&self) -> Result<WorkProjection, String> {
        project_work_projection(&self.paths, &self.run.id)
    }

    /// List durable facts recorded in the ledger for this run.
    pub fn facts(&self) -> Result<Vec<RuntimeFact>, String> {
        let ledger = WorkRuntimeLedger::open(&self.paths, &self.task.id, &self.run.id)?;
        ledger.list_facts()
    }

    /// List all registered artifacts for this workspace.
    pub fn artifacts(&self) -> Result<Vec<WorkArtifactSummary>, String> {
        artifacts::list_with_paths(&self.paths, &self.workspace.id, None)
    }

    /// Execute the Completion Gate via `WorkHarnessController::complete_or_fail_run`.
    pub fn complete(&mut self) -> Result<WorkRun, String> {
        let updated = self.controller.complete_or_fail_run(
            &self.task.id,
            &self.run.id,
            WorkRunStatus::Completed,
            None,
            None,
        )?;
        self.run = updated.clone();
        Ok(updated)
    }

    /// Get the latest state of the WorkRun from disk.
    pub fn get_run(&self) -> WorkRun {
        self.task_manager
            .get_run(&self.task.id, &self.run.id)
            .unwrap_or_else(|_| self.run.clone())
    }

    /// Save a checkpoint to durable task state.
    pub fn save_checkpoint(
        &mut self,
        summary: &str,
        current_step_id: Option<&str>,
    ) -> Result<(), String> {
        let checkpoint = WorkTaskCheckpoint {
            summary: summary.to_string(),
            current_step_id: current_step_id.map(String::from),
            created_at: crate::models::now_iso(),
        };
        let mut task_state = self.run.task_state.clone();
        task_state.checkpoint = Some(checkpoint);
        let updated = self.task_manager.set_run_state(
            &self.task.id,
            &self.run.id,
            self.run.status,
            None,
            Some(task_state),
        )?;
        self.run = updated;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Reusable Golden Assertions
// ---------------------------------------------------------------------------

pub fn assert_run_status(harness: &GoldenHarness, expected: WorkRunStatus) {
    let current = harness.get_run();
    assert_eq!(
        current.status, expected,
        "Run status mismatch: expected {:?}, got {:?}",
        expected, current.status
    );
}

pub fn assert_no_pending_interactions(harness: &GoldenHarness) {
    let pending = harness
        .interaction_manager
        .list_pending(Some(&harness.task.id), Some(&harness.run.id))
        .expect("failed to list pending interactions");
    assert!(
        pending.is_empty(),
        "Expected 0 pending interactions, found {}: {:?}",
        pending.len(),
        pending
    );
}

pub fn assert_artifact_count(harness: &GoldenHarness, expected: usize) {
    let list = harness.artifacts().expect("failed to list artifacts");
    assert_eq!(
        list.len(),
        expected,
        "Expected {} artifacts, found {}",
        expected,
        list.len()
    );
}

pub fn assert_artifact_valid(harness: &GoldenHarness, rel_path: &str, expected_type: &str) {
    let list = harness.artifacts().expect("failed to list artifacts");
    let norm_path = rel_path.trim().replace('\\', "/");
    let artifact = list
        .iter()
        .find(|a| a.path == norm_path)
        .unwrap_or_else(|| panic!("Artifact '{}' not found in workspace registry", norm_path));

    assert_eq!(
        artifact.run_id.as_deref(),
        Some(harness.run.id.as_str()),
        "Artifact '{}' must be bound to current run '{}'",
        norm_path,
        harness.run.id
    );
    assert!(
        artifact.sha256.is_some() && !artifact.sha256.as_ref().unwrap().is_empty(),
        "Artifact '{}' must have a non-empty sha256 checksum",
        norm_path
    );

    let ws_dir = harness
        .paths
        .workspace_dir(&harness.workspace.id)
        .expect("cannot resolve workspace dir");
    let full_path = ws_dir.join(&norm_path);
    assert!(
        full_path.exists(),
        "Artifact file '{}' does not exist on disk",
        full_path.display()
    );

    let validated_size = ArtifactValidator::validate_file(&full_path, expected_type)
        .unwrap_or_else(|e| panic!("ArtifactValidator failed for '{}': {}", norm_path, e));
    assert!(validated_size > 0, "Validated artifact size must be > 0");
}

pub fn assert_no_duplicate_tool_started(harness: &GoldenHarness) {
    let facts = harness.facts().expect("failed to list facts");
    let mut started_ids = std::collections::HashSet::new();
    for fact in &facts {
        if let RuntimeFact::ToolStarted { tool_call_id, .. } = fact {
            assert!(
                started_ids.insert(tool_call_id.clone()),
                "Duplicate ToolStarted detected in durable ledger for tool_call_id '{}'",
                tool_call_id
            );
        }
    }
}

pub fn assert_tool_executed_once(harness: &GoldenHarness, tool_call_id: &str) {
    let facts = harness.facts().expect("failed to list facts");
    let exec_count = facts
        .iter()
        .filter(|fact| match fact {
            RuntimeFact::ToolResult {
                tool_call_id: id,
                success,
                ..
            } => id == tool_call_id && *success,
            _ => false,
        })
        .count();
    assert_eq!(
        exec_count, 1,
        "Expected tool_call_id '{}' to have executed successfully exactly once, got {}",
        tool_call_id, exec_count
    );
}
