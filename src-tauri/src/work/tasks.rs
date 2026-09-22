use chrono::Utc;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::work::models::{
    ExecutionContext, TaskStandingRule, WorkExecutionMode, WorkPolicy, WorkRun, WorkRunStatus,
    WorkRunTrigger, WorkScheduleConfig, WorkTask, WorkTaskSource, WorkTaskState, WorkTaskStatus,
};
use crate::work::paths::{validate_workspace_id, WorkPaths};

type TaskMutationGate = Arc<Mutex<()>>;

/// All TaskManager instances in the desktop process share a gate per task.
/// Work state is persisted as separate task/run files, so instance-local locks
/// cannot protect a check-then-write sequence across scheduler, UI, and
/// lifecycle callers.
static TASK_MUTATION_GATES: Lazy<Mutex<HashMap<PathBuf, TaskMutationGate>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
pub struct TaskManager {
    paths: WorkPaths,
}

impl TaskManager {
    pub fn new(paths: WorkPaths) -> Self {
        Self { paths }
    }

    fn mutation_gate(&self, task_id: &str) -> Result<TaskMutationGate, String> {
        let key = self.paths.tasks_dir().join(task_id);
        let mut gates = TASK_MUTATION_GATES
            .lock()
            .map_err(|error| format!("Task mutation gate is poisoned: {error}"))?;
        Ok(gates
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone())
    }

    fn with_task_mutation_lock<T>(
        &self,
        task_id: &str,
        operation: impl FnOnce(&Self) -> Result<T, String>,
    ) -> Result<T, String> {
        let gate = self.mutation_gate(task_id)?;
        let _guard = gate
            .lock()
            .map_err(|error| format!("Task mutation lock is poisoned: {error}"))?;
        operation(self)
    }

    /// Create a new WorkTask (default source: Manual).
    pub fn create_task(
        &self,
        workspace_id: &str,
        title: &str,
        instructions: &str,
        policy: Option<WorkPolicy>,
    ) -> Result<WorkTask, String> {
        self.create_task_with_source_and_schedule(
            workspace_id,
            title,
            instructions,
            policy,
            WorkTaskSource::Manual,
            None,
        )
    }

    /// Create a task and persist its schedule in the same task manifest write.
    /// Invalid schedules are rejected before the task is written, so callers
    /// never receive a half-created unscheduled task.
    pub fn create_task_with_schedule(
        &self,
        workspace_id: &str,
        title: &str,
        instructions: &str,
        policy: Option<WorkPolicy>,
        schedule: Option<WorkScheduleConfig>,
    ) -> Result<WorkTask, String> {
        self.create_task_with_source_and_schedule(
            workspace_id,
            title,
            instructions,
            policy,
            WorkTaskSource::Manual,
            schedule,
        )
    }

    /// Create a new WorkTask with an explicit source.
    pub fn create_task_with_source(
        &self,
        workspace_id: &str,
        title: &str,
        instructions: &str,
        policy: Option<WorkPolicy>,
        source: WorkTaskSource,
    ) -> Result<WorkTask, String> {
        self.create_task_with_source_and_schedule(
            workspace_id,
            title,
            instructions,
            policy,
            source,
            None,
        )
    }

    fn create_task_with_source_and_schedule(
        &self,
        workspace_id: &str,
        title: &str,
        instructions: &str,
        policy: Option<WorkPolicy>,
        source: WorkTaskSource,
        mut schedule: Option<WorkScheduleConfig>,
    ) -> Result<WorkTask, String> {
        self.paths.ensure_layout()?;
        validate_workspace_id(workspace_id)?;

        let id = format!("task-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        // Inherit workspace default policy when no explicit policy is provided.
        let policy = match policy {
            Some(p) => p,
            None => {
                let ws_mgr = crate::work::workspace::WorkspaceManager::new(self.paths.clone());
                ws_mgr
                    .get(workspace_id)
                    .map(|ws| ws.default_policy)
                    .unwrap_or_default()
            }
        };

        if let Some(config) = schedule.as_mut() {
            if config.enabled && config.next_run_at.is_none() {
                config.next_run_at =
                    crate::work::scheduler::calculate_next_run(config, Utc::now())?;
            }
        }

        let task = WorkTask {
            id: id.clone(),
            workspace_id: workspace_id.to_string(),
            title: title.to_string(),
            instructions: instructions.to_string(),
            status: WorkTaskStatus::Active,
            source,
            policy,
            schedule,
            required_artifacts: Vec::new(),
            artifact_requirements: Vec::new(),
            run_count: 0,
            last_run_id: None,
            last_run_at: None,
            created_at: now.clone(),
            updated_at: now,
        };

        self.save_task(&task)?;
        Ok(task)
    }

    /// Delete a WorkTask and all its runs.
    pub fn delete_task(&self, task_id: &str) -> Result<(), String> {
        let task_dir = self.paths.tasks_dir().join(task_id);
        if !task_dir.exists() {
            return Err(format!("Task not found: {task_id}"));
        }
        fs::remove_dir_all(&task_dir).map_err(|e| format!("Failed to delete task {task_id}: {e}"))
    }

    /// Duplicate a task configuration without copying execution history.
    /// Schedules are copied disabled so a duplicate never starts an unattended
    /// run as a side effect of the UI action.
    pub fn duplicate_task(&self, task_id: &str) -> Result<WorkTask, String> {
        let source = self.get_task(task_id)?;
        let now = Utc::now().to_rfc3339();
        let mut duplicate = source.clone();
        duplicate.id = format!("task-{}", Uuid::new_v4());
        duplicate.title = format!("{}（副本）", source.title);
        duplicate.status = WorkTaskStatus::Active;
        duplicate.schedule = source.schedule.map(|mut schedule| {
            schedule.enabled = false;
            schedule.next_run_at = None;
            schedule.last_run_at = None;
            schedule
        });
        duplicate.run_count = 0;
        duplicate.last_run_id = None;
        duplicate.last_run_at = None;
        duplicate.created_at = now.clone();
        duplicate.updated_at = now;
        self.save_task(&duplicate)?;
        Ok(duplicate)
    }

    /// Get a WorkTask by ID.
    pub fn get_task(&self, id: &str) -> Result<WorkTask, String> {
        let manifest_path = self.paths.task_manifest_path(id)?;
        if !manifest_path.exists() {
            return Err(format!("Task not found: {id}"));
        }
        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read task {id}: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse task {id}: {e}"))
    }

    /// List all WorkTasks (optionally filtered by workspace_id).
    pub fn list_tasks(&self, workspace_id_filter: Option<&str>) -> Result<Vec<WorkTask>, String> {
        self.paths.ensure_layout()?;
        let tasks_dir = self.paths.tasks_dir();
        let entries = fs::read_dir(&tasks_dir).map_err(|e| e.to_string())?;

        let mut tasks = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let task_id = entry.file_name().to_string_lossy().into_owned();
            let Ok(task) = self.get_task(&task_id) else {
                continue;
            };

            if let Some(filter) = workspace_id_filter {
                if task.workspace_id != filter {
                    continue;
                }
            }

            tasks.push(task);
        }

        tasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(tasks)
    }

    /// Update an existing WorkTask.
    pub fn update_task(&self, task: &mut WorkTask) -> Result<(), String> {
        let task_id = task.id.clone();
        self.with_task_mutation_lock(&task_id, |manager| manager.update_task_unlocked(task))
    }

    fn update_task_unlocked(&self, task: &mut WorkTask) -> Result<(), String> {
        task.updated_at = Utc::now().to_rfc3339();
        if let Some(schedule) = task.schedule.as_mut() {
            if schedule.enabled {
                // Validate every enabled schedule on edit, even when a caller
                // supplied a stale next_run_at. This prevents invalid Cron
                // expressions from being persisted as apparently valid tasks.
                let next = crate::work::scheduler::calculate_next_run(schedule, Utc::now())?;
                if schedule.next_run_at.is_none() {
                    schedule.next_run_at = next;
                }
            } else {
                schedule.next_run_at = None;
            }
        }
        self.save_task(task)
    }

    /// Add a standing approval rule to a Task's policy.
    pub fn add_standing_rule(
        &self,
        task_id: &str,
        rule: TaskStandingRule,
    ) -> Result<WorkTask, String> {
        match self.get_task(task_id) {
            Ok(mut task) => {
                task.policy.standing_rules.retain(|r| r.id != rule.id);
                task.policy.standing_rules.push(rule);
                self.update_task(&mut task)?;
                Ok(task)
            }
            Err(_) => {
                let standalone_dir = self.paths.ensure_standalone_task_dir(task_id)?;
                let rules_path = standalone_dir.join("standing_rules.json");
                let mut rules: Vec<TaskStandingRule> = if rules_path.exists() {
                    let content =
                        std::fs::read_to_string(&rules_path).map_err(|e| e.to_string())?;
                    serde_json::from_str(&content).unwrap_or_default()
                } else {
                    Vec::new()
                };
                rules.retain(|r| r.id != rule.id);
                rules.push(rule);
                let serialized = serde_json::to_string_pretty(&rules).map_err(|e| e.to_string())?;
                std::fs::write(&rules_path, serialized).map_err(|e| e.to_string())?;

                Ok(WorkTask {
                    id: task_id.to_string(),
                    workspace_id: String::new(),
                    title: "Standalone Task".to_string(),
                    instructions: String::new(),
                    status: WorkTaskStatus::Active,
                    source: WorkTaskSource::Manual,
                    policy: WorkPolicy {
                        execution_mode: WorkExecutionMode::Direct,
                        max_automated_steps: 50,
                        allow_external_connectors: true,
                        standing_rules: rules,
                        guardian_config: None,
                    },
                    schedule: None,
                    required_artifacts: Vec::new(),
                    artifact_requirements: Vec::new(),
                    run_count: 1,
                    last_run_id: Some(task_id.to_string()),
                    last_run_at: None,
                    created_at: String::new(),
                    updated_at: String::new(),
                })
            }
        }
    }

    /// Start a new WorkRun for a WorkTask.
    pub fn start_run(
        &self,
        task_id: &str,
        session_id: Option<&str>,
        trigger: WorkRunTrigger,
    ) -> Result<WorkRun, String> {
        self.start_run_internal(
            task_id,
            session_id,
            trigger,
            None,
            ExecutionContext::Attended,
        )
    }

    /// Start a new Unattended WorkRun triggered by the scheduler.
    /// `scheduled_for` is the UTC fire-time and acts as the persistent idempotency key.
    pub fn start_scheduled_run(
        &self,
        task_id: &str,
        scheduled_for: &str,
    ) -> Result<WorkRun, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            if let Some(existing) = manager
                .list_runs(task_id)?
                .into_iter()
                .find(|run| run.scheduled_for.as_deref() == Some(scheduled_for))
            {
                if existing.status.is_active() {
                    return Ok(existing);
                }
                return Err(format!(
                    "Scheduled fire '{scheduled_for}' already has terminal WorkRun '{}'",
                    existing.id
                ));
            }
            if manager.has_active_run(task_id)? {
                return Err(format!(
                    "Task '{task_id}' already has an active run in progress"
                ));
            }
            manager.start_run_internal_unlocked(
                task_id,
                None,
                WorkRunTrigger::Scheduled,
                Some(scheduled_for),
                ExecutionContext::Unattended,
            )
        })
    }

    /// Attach the Work session actor run created by the shared Work runtime.
    /// The WorkRun remains the product-level source of truth while the session
    /// id is only the execution handle used for resume and interaction replies.
    pub fn attach_session_id(
        &self,
        task_id: &str,
        run_id: &str,
        session_id: &str,
    ) -> Result<WorkRun, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            let mut run = manager.get_run(task_id, run_id)?;
            if run.session_id.as_deref() != Some(session_id) {
                run.session_id = Some(session_id.to_string());
                manager.save_run(&run)?;
            }
            Ok(run)
        })
    }

    /// Move a non-terminal run into a durable waiting/running state without
    /// changing its timestamps. This is used by WorkHarnessController.
    pub(crate) fn set_run_status(
        &self,
        task_id: &str,
        run_id: &str,
        status: WorkRunStatus,
    ) -> Result<WorkRun, String> {
        self.set_run_state(task_id, run_id, status, None, None)
    }

    /// Persist a non-terminal state without manufacturing a finished
    /// timestamp. Recovery and delivery acceptance use this path so a run can
    /// be resumed after a restart and remains visible as unfinished.
    pub(crate) fn set_run_state(
        &self,
        task_id: &str,
        run_id: &str,
        status: WorkRunStatus,
        error_message: Option<String>,
        task_state: Option<WorkTaskState>,
    ) -> Result<WorkRun, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            manager.set_run_state_unlocked(task_id, run_id, status, error_message, task_state)
        })
    }

    fn set_run_state_unlocked(
        &self,
        task_id: &str,
        run_id: &str,
        status: WorkRunStatus,
        error_message: Option<String>,
        task_state: Option<WorkTaskState>,
    ) -> Result<WorkRun, String> {
        if status == WorkRunStatus::Completed {
            return Err(
                "WorkRun cannot enter Completed through a non-terminal state update; use the lifecycle finalizer"
                    .to_string(),
            );
        }
        let mut run = self.get_run(task_id, run_id)?;
        run.status = status;
        if error_message.is_some() {
            run.error_message = error_message;
        }
        if let Some(state) = task_state {
            run.task_state = state;
        }
        self.save_run(&run)?;

        let mut task = self.get_task(task_id)?;
        task.status = match status {
            WorkRunStatus::WaitingApproval
            | WorkRunStatus::WaitingInput
            | WorkRunStatus::Recoverable
            | WorkRunStatus::WaitingDelivery => WorkTaskStatus::NeedsAttention,
            WorkRunStatus::Queued | WorkRunStatus::Running => WorkTaskStatus::InRun,
            WorkRunStatus::Completed => WorkTaskStatus::Completed,
            WorkRunStatus::Failed | WorkRunStatus::Cancelled => WorkTaskStatus::NeedsAttention,
            WorkRunStatus::Skipped => WorkTaskStatus::Scheduled,
        };
        self.update_task_unlocked(&mut task)?;
        Ok(run)
    }

    /// Locate the durable product run behind a Work session actor.
    pub fn find_run_by_session_id(&self, session_id: &str) -> Result<Option<WorkRun>, String> {
        for task in self.list_tasks(None)? {
            for run in self.list_runs(&task.id)? {
                if run.session_id.as_deref() == Some(session_id) {
                    return Ok(Some(run));
                }
            }
        }
        Ok(None)
    }

    /// Internal run-creation helper.
    pub fn start_run_internal(
        &self,
        task_id: &str,
        session_id: Option<&str>,
        trigger: WorkRunTrigger,
        scheduled_for: Option<&str>,
        execution_context: ExecutionContext,
    ) -> Result<WorkRun, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            if manager.has_active_run(task_id)? {
                return Err(format!(
                    "Task '{task_id}' already has an active run in progress"
                ));
            }
            manager.start_run_internal_unlocked(
                task_id,
                session_id,
                trigger,
                scheduled_for,
                execution_context,
            )
        })
    }

    fn start_run_internal_unlocked(
        &self,
        task_id: &str,
        session_id: Option<&str>,
        trigger: WorkRunTrigger,
        scheduled_for: Option<&str>,
        execution_context: ExecutionContext,
    ) -> Result<WorkRun, String> {
        let mut task = self.get_task(task_id)?;
        let run_id = format!("run-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let run = WorkRun {
            id: run_id.clone(),
            task_id: task_id.to_string(),
            workspace_id: task.workspace_id.clone(),
            session_id: session_id.map(|s| s.to_string()),
            trigger,
            status: WorkRunStatus::Running,
            execution_context,
            scheduled_for: scheduled_for.map(|s| s.to_string()),
            skipped_reason: None,
            task_state: WorkTaskState {
                version: 1,
                revision: 0,
                goal: Some(task.title.clone()),
                // A generated GoalSpec is an acceptance contract, not a
                // default decoration for every chat run. Build it only when
                // the task explicitly declares deliverables that need
                // verification; non-strict conversations stay answer-only.
                goal_spec: (!task.required_artifacts.is_empty()
                    || !task.artifact_requirements.is_empty())
                .then(|| {
                    crate::work::goal::GoalBuilder::build(
                        &task.workspace_id,
                        &run_id,
                        &task.title,
                        Some(&task),
                    )
                }),
                plan: Vec::new(),
                checkpoint: None,
                pending_approval: None,
                updated_at: now.clone(),
            },
            error_message: None,
            started_at: now.clone(),
            finished_at: None,
            duration_ms: None,
            // Keep total-duration and optional budget limits opt-in. Guardian
            // still uses its default liveness/loop thresholds when projecting
            // health, but a normal Work run is not assigned a hidden budget.
            guardian_config: task.policy.guardian_config.clone(),
        };

        self.save_run(&run)?;

        // Update task pointers
        task.run_count += 1;
        task.last_run_id = Some(run_id);
        task.last_run_at = Some(now);
        task.status = WorkTaskStatus::InRun;
        self.update_task_unlocked(&mut task)?;

        Ok(run)
    }

    /// Record a Skipped run (scheduler decided not to start).
    /// Creates a durable record so the scheduler does not re-evaluate the same fire time.
    pub fn record_skipped_run(
        &self,
        task_id: &str,
        workspace_id: &str,
        scheduled_for: &str,
        reason: &str,
    ) -> Result<WorkRun, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            manager.record_skipped_run_unlocked(task_id, workspace_id, scheduled_for, reason)
        })
    }

    fn record_skipped_run_unlocked(
        &self,
        task_id: &str,
        workspace_id: &str,
        scheduled_for: &str,
        reason: &str,
    ) -> Result<WorkRun, String> {
        let run_id = format!("run-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let run = WorkRun {
            id: run_id,
            task_id: task_id.to_string(),
            workspace_id: workspace_id.to_string(),
            session_id: None,
            trigger: WorkRunTrigger::Scheduled,
            status: WorkRunStatus::Skipped,
            execution_context: ExecutionContext::Unattended,
            scheduled_for: Some(scheduled_for.to_string()),
            skipped_reason: Some(reason.to_string()),
            task_state: WorkTaskState::default(),
            error_message: None,
            started_at: now.clone(),
            finished_at: Some(now),
            duration_ms: Some(0),
            guardian_config: None,
        };

        self.save_run(&run)?;
        Ok(run)
    }

    /// Check whether the task currently has an active (Running / WaitingApproval /
    /// WaitingInput / Queued) run. Used by the scheduler concurrency gate.
    pub fn has_active_run(&self, task_id: &str) -> Result<bool, String> {
        let runs = self.list_runs(task_id)?;
        Ok(runs.iter().any(|r| r.status.is_active()))
    }

    /// Check whether a run already exists for a given `(task_id, scheduled_for)` pair.
    /// This is the persistent idempotency check: prevents duplicate fires across
    /// scheduler restarts and tick races.
    pub fn run_exists_for_fire(&self, task_id: &str, scheduled_for: &str) -> Result<bool, String> {
        let runs = self.list_runs(task_id)?;
        Ok(runs
            .iter()
            .any(|r| r.scheduled_for.as_deref() == Some(scheduled_for)))
    }

    /// Serializes schedule fire conditions, concurrency gates, and idempotency
    /// for one task. The durable Run is written before the schedule pointer is
    /// advanced, so a restart can repair the pointer from the claimed fire.
    pub fn claim_and_evaluate_schedule_fire(
        &self,
        task_id: &str,
        fire_key: &str,
        next_after: Option<String>,
    ) -> Result<crate::work::scheduler::ScheduleOutcome, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            manager.claim_and_evaluate_schedule_fire_unlocked(task_id, fire_key, next_after)
        })
    }

    /// Atomically consume a missed scheduled fire during startup recovery.
    /// The schedule pointer is rechecked under the task gate so a stale
    /// startup snapshot cannot create a compensating Run after the schedule
    /// was edited or another reconciler already consumed it.
    pub fn claim_missed_schedule_fire(
        &self,
        task_id: &str,
        fire_key: &str,
        next_after: Option<String>,
    ) -> Result<Option<WorkRun>, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            let mut task = manager.get_task(task_id)?;
            let current_fire = task
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.next_run_at.as_deref());
            let schedule_is_current_and_enabled = task
                .schedule
                .as_ref()
                .is_some_and(|schedule| schedule.enabled && current_fire == Some(fire_key));
            if task.status == WorkTaskStatus::Archived
                || !schedule_is_current_and_enabled
                || manager.run_exists_for_fire(task_id, fire_key)?
            {
                return Ok(None);
            }

            let skipped = manager.record_skipped_run_unlocked(
                task_id,
                &task.workspace_id,
                fire_key,
                "missed",
            )?;
            if let Some(schedule) = task.schedule.as_mut() {
                schedule.last_run_at = Some(fire_key.to_string());
                schedule.next_run_at = next_after;
            }
            manager.update_task_unlocked(&mut task)?;
            Ok(Some(skipped))
        })
    }

    fn claim_and_evaluate_schedule_fire_unlocked(
        &self,
        task_id: &str,
        fire_key: &str,
        next_after: Option<String>,
    ) -> Result<crate::work::scheduler::ScheduleOutcome, String> {
        let mut task = self.get_task(task_id)?;

        // 1. Idempotency gate
        if self.run_exists_for_fire(task_id, fire_key)? {
            // If the process stopped after writing the Run but before
            // advancing the task manifest, repair the stale due pointer.
            if task
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.next_run_at.as_deref())
                == Some(fire_key)
                && task
                    .schedule
                    .as_ref()
                    .is_some_and(|schedule| schedule.enabled)
                && task.status != WorkTaskStatus::Archived
            {
                if let Some(schedule) = task.schedule.as_mut() {
                    schedule.last_run_at = Some(fire_key.to_string());
                    schedule.next_run_at = next_after;
                }
                self.update_task_unlocked(&mut task)?;
            }
            return Ok(crate::work::scheduler::ScheduleOutcome::None);
        }

        // The initial scheduler read happens before the task gate is acquired.
        // Recheck the schedule under the gate so an edit/archive racing with
        // the tick cannot start a fire that is no longer due.
        let schedule_is_current_and_enabled = task.schedule.as_ref().is_some_and(|schedule| {
            schedule.enabled && schedule.next_run_at.as_deref() == Some(fire_key)
        });
        if task.status == WorkTaskStatus::Archived || !schedule_is_current_and_enabled {
            return Ok(crate::work::scheduler::ScheduleOutcome::None);
        }

        // 2. Concurrency gate
        if self.has_active_run(task_id)? {
            let skipped = self.record_skipped_run_unlocked(
                task_id,
                &task.workspace_id,
                fire_key,
                "previous_run_active",
            )?;
            if let Some(schedule) = task.schedule.as_mut() {
                schedule.last_run_at = Some(fire_key.to_string());
                schedule.next_run_at = next_after;
            }
            self.update_task_unlocked(&mut task)?;
            return Ok(crate::work::scheduler::ScheduleOutcome::Skipped(Box::new(
                skipped,
            )));
        }

        // 3. Max runs gate
        if task
            .schedule
            .as_ref()
            .and_then(|s| s.max_runs)
            .is_some_and(|max| task.run_count >= max)
        {
            let skipped = self.record_skipped_run_unlocked(
                task_id,
                &task.workspace_id,
                fire_key,
                "max_runs_reached",
            )?;
            if let Some(schedule) = task.schedule.as_mut() {
                schedule.last_run_at = Some(fire_key.to_string());
                schedule.next_run_at = next_after;
            }
            self.update_task_unlocked(&mut task)?;
            return Ok(crate::work::scheduler::ScheduleOutcome::Skipped(Box::new(
                skipped,
            )));
        }

        // 4. Write the durable Run while the task gate is held. If the second
        // manifest write fails, a later pass sees the Run and repairs the
        // schedule instead of creating another run for this fire.
        let _run = self.start_run_internal_unlocked(
            task_id,
            None,
            WorkRunTrigger::Scheduled,
            Some(fire_key),
            ExecutionContext::Unattended,
        )?;
        let mut latest_task = self.get_task(task_id)?;
        if let Some(schedule) = latest_task.schedule.as_mut() {
            schedule.last_run_at = Some(fire_key.to_string());
            schedule.next_run_at = next_after;
        } else {
            return Err(format!(
                "Task '{task_id}' lost its schedule while claiming fire '{fire_key}'"
            ));
        }
        self.update_task_unlocked(&mut latest_task)?;
        Ok(crate::work::scheduler::ScheduleOutcome::Fire {
            task_id: latest_task.id,
            fire_key: fire_key.to_string(),
        })
    }

    /// List all Skipped runs with reason "missed" across all tasks.
    /// Used by the frontend "Needs Attention" panel.
    pub fn list_missed_runs(&self) -> Result<Vec<WorkRun>, String> {
        self.paths.ensure_layout()?;
        let tasks_dir = self.paths.tasks_dir();
        let entries = fs::read_dir(&tasks_dir).map_err(|e| e.to_string())?;

        let mut missed = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let task_id = entry.file_name().to_string_lossy().into_owned();
            let Ok(runs) = self.list_runs(&task_id) else {
                continue;
            };
            for run in runs {
                if run.skipped_reason.as_deref() == Some("missed") {
                    missed.push(run);
                }
            }
        }

        missed.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(missed)
    }

    /// Complete or fail a WorkRun (persistence repository level).
    pub(crate) fn finish_run(
        &self,
        task_id: &str,
        run_id: &str,
        final_status: WorkRunStatus,
        error_message: Option<String>,
        final_state: Option<WorkTaskState>,
    ) -> Result<WorkRun, String> {
        self.with_task_mutation_lock(task_id, |manager| {
            manager.finish_run_unlocked(task_id, run_id, final_status, error_message, final_state)
        })
    }

    fn finish_run_unlocked(
        &self,
        task_id: &str,
        run_id: &str,
        final_status: WorkRunStatus,
        error_message: Option<String>,
        final_state: Option<WorkTaskState>,
    ) -> Result<WorkRun, String> {
        let mut run = self.get_run(task_id, run_id)?;
        let task = self.get_task(task_id)?;

        // Keep the repository safe even when a caller bypasses the higher-level
        // controller. A completed run is terminal only after the current run's
        // required artifacts have been physically validated and delivered.
        if final_status == WorkRunStatus::Completed {
            let acceptance = match crate::work::artifacts::check_required_artifacts_with_paths(
                &self.paths,
                &task.workspace_id,
                run_id,
                &task,
            ) {
                Ok(acceptance) => acceptance,
                Err(error) => {
                    return self.set_run_state_unlocked(
                        task_id,
                        run_id,
                        WorkRunStatus::WaitingDelivery,
                        Some(format!("等待必需交付物验收：{error}")),
                        final_state,
                    )
                }
            };
            if !acceptance.satisfied {
                let details = acceptance
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
                let message = if details.is_empty() {
                    "等待必需交付物验收".to_string()
                } else {
                    format!("等待必需交付物验收：{}", details.join("；"))
                };
                return self.set_run_state_unlocked(
                    task_id,
                    run_id,
                    WorkRunStatus::WaitingDelivery,
                    Some(message),
                    final_state,
                );
            }
        }

        if final_status.is_active() {
            return self.set_run_state_unlocked(
                task_id,
                run_id,
                final_status,
                error_message,
                final_state,
            );
        }
        let now = Utc::now();
        let finished_at_str = now.to_rfc3339();

        let started_at_dt = chrono::DateTime::parse_from_rfc3339(&run.started_at).ok();
        let duration_ms =
            started_at_dt.map(|dt| now.signed_duration_since(dt).num_milliseconds().max(0) as u64);

        run.status = final_status;
        run.error_message = error_message;
        run.finished_at = Some(finished_at_str);
        run.duration_ms = duration_ms;
        if let Some(state) = final_state {
            run.task_state = state;
        }

        self.save_run(&run)?;

        // Update Task status after the run is durably terminal. Artifact truth
        // was checked above before a Completed record could be written.
        let mut task = task;
        if final_status == WorkRunStatus::Completed {
            task.status = WorkTaskStatus::Completed;
        } else if final_status == WorkRunStatus::Failed || final_status == WorkRunStatus::Cancelled
        {
            task.status = WorkTaskStatus::NeedsAttention;
        } else if final_status == WorkRunStatus::Skipped {
            task.status = WorkTaskStatus::Scheduled;
        }
        self.update_task_unlocked(&mut task)?;

        Ok(run)
    }

    /// Update status of a WorkRun without marking it finished/closed.
    pub(crate) fn update_run_status(
        &self,
        task_id: &str,
        run_id: &str,
        new_status: WorkRunStatus,
    ) -> Result<WorkRun, String> {
        self.set_run_state(task_id, run_id, new_status, None, None)
    }

    /// Get a specific WorkRun.
    pub fn get_run(&self, task_id: &str, run_id: &str) -> Result<WorkRun, String> {
        let runs_dir = self.paths.task_runs_dir(task_id)?;
        let path = runs_dir.join(format!("{run_id}.json"));
        if !path.exists() {
            return Err(format!("WorkRun {run_id} not found for task {task_id}"));
        }
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read run {run_id}: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse run {run_id}: {e}"))
    }

    /// Locate a WorkRun by its product-level id across all durable tasks.
    /// Browser controls receive this id, while actor controls need the
    /// session_id attached during Work startup.
    pub fn find_run_by_id(&self, run_id: &str) -> Result<Option<WorkRun>, String> {
        for task in self.list_tasks(None)? {
            if let Ok(run) = self.get_run(&task.id, run_id) {
                return Ok(Some(run));
            }
        }
        Ok(None)
    }

    /// List all WorkRuns for a task.
    pub fn list_runs(&self, task_id: &str) -> Result<Vec<WorkRun>, String> {
        let runs_dir = self.paths.task_runs_dir(task_id)?;
        if !runs_dir.exists() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(&runs_dir).map_err(|e| e.to_string())?;
        let mut runs = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .starts_with('.')
            {
                let content = fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read run at {path:?}: {e}"))?;
                if let Ok(run) = serde_json::from_str::<WorkRun>(&content) {
                    runs.push(run);
                }
            }
        }
        runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(runs)
    }

    /// List WorkTaskRunSummary items for a task, with duration, delivered artifacts count, and goal status.
    pub fn list_task_run_summaries(
        &self,
        task_id: &str,
    ) -> Result<Vec<crate::work::models::WorkTaskRunSummary>, String> {
        let runs = self.list_runs(task_id)?;
        let summaries = runs
            .into_iter()
            .map(|run| {
                let delivered_artifacts_count = crate::work::artifacts::list_with_paths(
                    &self.paths,
                    &run.workspace_id,
                    Some(&run.id),
                )
                .map(|list| {
                    list.into_iter()
                        .filter(|a| {
                            a.status == crate::work::models::WorkArtifactStatus::Delivered
                                || a.status == crate::work::models::WorkArtifactStatus::Validated
                        })
                        .count()
                })
                .unwrap_or(0);

                let goal_status = run.task_state.goal_spec.as_ref().map(|g| g.status);
                let goal_passed = goal_status == Some(crate::work::models::GoalStatus::Passed);

                crate::work::models::WorkTaskRunSummary {
                    run_id: run.id,
                    task_id: run.task_id,
                    workspace_id: run.workspace_id,
                    trigger: run.trigger,
                    status: run.status,
                    duration_ms: run.duration_ms,
                    delivered_artifacts_count,
                    goal_passed,
                    goal_status,
                    error_message: run.error_message,
                    created_at: run.scheduled_for.unwrap_or_else(|| run.started_at.clone()),
                    started_at: Some(run.started_at),
                    finished_at: run.finished_at,
                }
            })
            .collect();

        Ok(summaries)
    }

    /// Global automation statistics.
    pub fn get_automation_stats(&self) -> Result<crate::work::models::WorkAutomationStats, String> {
        let tasks = self.list_tasks(None)?;
        let total_tasks = tasks.len();
        let enabled_schedules = tasks
            .iter()
            .filter(|t| t.schedule.as_ref().map(|s| s.enabled).unwrap_or(false))
            .count();
        let running_tasks = tasks
            .iter()
            .filter(|t| t.status == WorkTaskStatus::InRun)
            .count();
        let needs_attention_tasks = tasks
            .iter()
            .filter(|t| t.status == WorkTaskStatus::NeedsAttention)
            .count();

        let now = Utc::now();
        let today_prefix = now.format("%Y-%m-%d").to_string();
        let mut completed_runs_today = 0;
        for task in &tasks {
            if let Ok(runs) = self.list_runs(&task.id) {
                completed_runs_today += runs
                    .iter()
                    .filter(|r| {
                        r.status == WorkRunStatus::Completed
                            && r.started_at.starts_with(&today_prefix)
                    })
                    .count();
            }
        }

        Ok(crate::work::models::WorkAutomationStats {
            total_tasks,
            enabled_schedules,
            running_tasks,
            needs_attention_tasks,
            completed_runs_today,
        })
    }

    fn save_task(&self, task: &WorkTask) -> Result<(), String> {
        let manifest_path = self.paths.task_manifest_path(&task.id)?;
        let parent = manifest_path
            .parent()
            .ok_or_else(|| "Missing task parent dir".to_string())?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;

        let bytes = serde_json::to_vec_pretty(task)
            .map_err(|e| format!("Failed to serialize task: {e}"))?;
        let temp_path = parent.join(format!(".task.{}.tmp", Uuid::new_v4()));

        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|e| e.to_string())?;

        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;

        fs::rename(&temp_path, &manifest_path).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn save_run(&self, run: &WorkRun) -> Result<(), String> {
        let runs_dir = self.paths.task_runs_dir(&run.task_id)?;
        fs::create_dir_all(&runs_dir).map_err(|e| e.to_string())?;
        let run_path = runs_dir.join(format!("{}.json", run.id));

        let bytes =
            serde_json::to_vec_pretty(run).map_err(|e| format!("Failed to serialize run: {e}"))?;
        let temp_path = runs_dir.join(format!(".run.{}.tmp", Uuid::new_v4()));

        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|e| e.to_string())?;

        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;

        fs::rename(&temp_path, &run_path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Reconcile active/orphaned WorkRuns across app restarts so that waiting runs
/// remain recoverable and non-waiting orphans are cleaned up.
pub fn reconcile_active_runs(paths: &WorkPaths) -> Result<(), String> {
    let controller = crate::work::lifecycle::WorkHarnessController::new(paths.clone());
    controller.reconcile_on_restart()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn restart_restores_waiting_state_from_durable_approval_or_inbox() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let interaction_mgr = crate::work::interaction::InteractionManager::new(paths.clone());

        let task = manager
            .create_task("ws-1", "Test Task", "Description", None)
            .unwrap();
        let run = manager
            .start_run(&task.id, Some("sess-1"), WorkRunTrigger::Manual)
            .unwrap();

        let _ = interaction_mgr.create_interaction(
            &task.id,
            &run.id,
            "ws-1",
            Some("sess-1"),
            None,
            Some("call-1"),
            crate::work::models::PendingInteractionKind::Permission,
            "Approve Tool",
            "Details",
            serde_json::json!({}),
        );

        reconcile_active_runs(&paths).unwrap();

        let updated_run = manager.get_run(&task.id, &run.id).unwrap();
        assert_eq!(updated_run.status, WorkRunStatus::WaitingApproval);
    }

    #[test]
    fn task_and_run_lifecycle_round_trip() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths);

        // Create task
        let task = manager
            .create_task(
                "ws-1",
                "Generate Financial Report",
                "Extract revenue from input/",
                None,
            )
            .unwrap();
        assert_eq!(task.status, WorkTaskStatus::Active);
        assert_eq!(task.run_count, 0);

        // Start run
        let run = manager
            .start_run(&task.id, Some("session-101"), WorkRunTrigger::Manual)
            .unwrap();
        assert_eq!(run.status, WorkRunStatus::Running);
        assert!(run.guardian_config.is_none());

        let updated_task = manager.get_task(&task.id).unwrap();
        assert_eq!(updated_task.status, WorkTaskStatus::InRun);
        assert_eq!(updated_task.run_count, 1);
        assert_eq!(updated_task.last_run_id, Some(run.id.clone()));

        // Finish run
        let finished_run = manager
            .finish_run(&task.id, &run.id, WorkRunStatus::Completed, None, None)
            .unwrap();
        assert_eq!(finished_run.status, WorkRunStatus::Completed);
        assert!(finished_run.duration_ms.is_some());

        let final_task = manager.get_task(&task.id).unwrap();
        assert_eq!(final_task.status, WorkTaskStatus::Completed);
    }

    #[test]
    fn goal_spec_is_created_only_for_tasks_with_explicit_artifact_requirements() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths);

        let plain_task = manager
            .create_task("ws-plain", "Answer a question", "No files needed", None)
            .unwrap();
        let plain_run = manager
            .start_run(
                &plain_task.id,
                Some("session-plain"),
                WorkRunTrigger::Manual,
            )
            .unwrap();
        assert!(plain_run.task_state.goal_spec.is_none());

        let mut artifact_task = manager
            .create_task("ws-artifact", "Create a report", "Deliver the report", None)
            .unwrap();
        artifact_task.required_artifacts = vec!["output/report.xlsx".to_string()];
        manager.update_task(&mut artifact_task).unwrap();

        let artifact_run = manager
            .start_run(
                &artifact_task.id,
                Some("session-artifact"),
                WorkRunTrigger::Manual,
            )
            .unwrap();
        assert!(artifact_run.task_state.goal_spec.is_some());
    }

    #[test]
    fn invalid_schedule_is_rejected_before_task_manifest_is_written() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let schedule = WorkScheduleConfig {
            kind: crate::work::models::WorkScheduleKind::Cron,
            enabled: true,
            timezone: "UTC".into(),
            fire_at: None,
            time_of_day: None,
            day_of_week: None,
            cron_expression: Some("not a cron expression".into()),
            next_run_at: None,
            last_run_at: None,
            max_runs: None,
            run_on_startup: false,
        };

        assert!(manager
            .create_task_with_schedule(
                "ws-1",
                "Invalid",
                "Should not persist",
                None,
                Some(schedule)
            )
            .is_err());
        assert!(manager.list_tasks(None).unwrap().is_empty());
    }

    #[test]
    fn invalid_schedule_edit_does_not_replace_existing_manifest() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let task = manager
            .create_task("ws-1", "Valid", "Keep this task", None)
            .unwrap();
        let original = manager.get_task(&task.id).unwrap();
        let mut edited = original.clone();
        edited.schedule = Some(WorkScheduleConfig {
            kind: crate::work::models::WorkScheduleKind::Cron,
            enabled: true,
            timezone: "UTC".into(),
            fire_at: None,
            time_of_day: None,
            day_of_week: None,
            cron_expression: Some("bad cron".into()),
            next_run_at: None,
            last_run_at: None,
            max_runs: None,
            run_on_startup: false,
        });

        assert!(manager.update_task(&mut edited).is_err());
        assert_eq!(manager.get_task(&task.id).unwrap(), original);
    }

    #[test]
    fn test_add_standing_rule_standalone() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());

        let standalone_run_id = "standalone-run-test-123";
        let _ = paths.ensure_standalone_task_dir(standalone_run_id).unwrap();

        let rule = TaskStandingRule {
            id: "rule-1".to_string(),
            tool_name: "work_write_file".to_string(),
            target_pattern: "output/*".to_string(),
            risk_class: crate::work::models::ToolRiskClass::WriteLocal,
            granted_at: "2026-08-16T00:00:00Z".to_string(),
        };

        let result = manager.add_standing_rule(standalone_run_id, rule.clone());
        assert!(result.is_ok());
        let task = result.unwrap();
        assert_eq!(task.id, standalone_run_id);
        assert_eq!(task.policy.standing_rules.len(), 1);
        assert_eq!(task.policy.standing_rules[0].target_pattern, "output/*");

        // Verify the file was written
        let standalone_dir = paths.standalone_task_dir(standalone_run_id).unwrap();
        let rules_path = standalone_dir.join("standing_rules.json");
        assert!(rules_path.exists());
    }

    #[test]
    fn test_add_standing_rule_standalone_creates_dir_if_missing() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());

        let standalone_run_id = "standalone-run-not-precreated";
        let rule = TaskStandingRule {
            id: "rule-auto-dir".to_string(),
            tool_name: "browser_type".to_string(),
            target_pattern: "#kw".to_string(),
            risk_class: crate::work::models::ToolRiskClass::WriteLocal,
            granted_at: "2026-08-16T00:00:00Z".to_string(),
        };

        let result = manager.add_standing_rule(standalone_run_id, rule);
        assert!(result.is_ok());
        let task = result.unwrap();
        assert_eq!(task.id, standalone_run_id);
        assert_eq!(task.policy.standing_rules.len(), 1);
        assert_eq!(task.policy.standing_rules[0].target_pattern, "#kw");

        let standalone_dir = paths.standalone_task_dir(standalone_run_id).unwrap();
        assert!(standalone_dir.join("standing_rules.json").exists());
    }

    #[test]
    fn test_automation_stats_and_task_run_summaries() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());

        // Create tasks
        let t1 = manager
            .create_task_with_schedule(
                "ws-1",
                "每日分析",
                "生成日常简报",
                None,
                Some(WorkScheduleConfig {
                    kind: crate::work::models::WorkScheduleKind::Daily,
                    enabled: true,
                    timezone: "UTC".into(),
                    fire_at: None,
                    time_of_day: Some("09:00".into()),
                    day_of_week: None,
                    cron_expression: None,
                    next_run_at: Some("2026-08-31T09:00:00Z".into()),
                    last_run_at: None,
                    max_runs: None,
                    run_on_startup: false,
                }),
            )
            .unwrap();

        let _t2 = manager
            .create_task("ws-1", "一次性任务", "测试", None)
            .unwrap();

        let run = manager
            .start_run_internal(
                &t1.id,
                None,
                WorkRunTrigger::Scheduled,
                None,
                ExecutionContext::Unattended,
            )
            .unwrap();

        let stats = manager.get_automation_stats().unwrap();
        assert_eq!(stats.total_tasks, 2);
        assert_eq!(stats.enabled_schedules, 1);
        assert_eq!(stats.running_tasks, 1);

        let summaries = manager.list_task_run_summaries(&t1.id).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].run_id, run.id);
        assert_eq!(summaries[0].trigger, WorkRunTrigger::Scheduled);
        assert_eq!(summaries[0].status, WorkRunStatus::Running);
    }

    #[test]
    fn duplicate_task_copies_configuration_without_history_or_enabled_schedule() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths);
        let task = manager
            .create_task_with_schedule(
                "ws-duplicate",
                "每日产品简报",
                "读取资料库并生成简报",
                None,
                Some(WorkScheduleConfig {
                    kind: crate::work::models::WorkScheduleKind::Daily,
                    enabled: true,
                    timezone: "UTC".to_string(),
                    fire_at: None,
                    time_of_day: Some("09:00".to_string()),
                    day_of_week: None,
                    cron_expression: None,
                    next_run_at: Some("2026-09-01T09:00:00Z".to_string()),
                    last_run_at: Some("2026-08-31T09:00:00Z".to_string()),
                    max_runs: Some(10),
                    run_on_startup: true,
                }),
            )
            .unwrap();
        let run = manager
            .start_run(&task.id, Some("session-duplicate"), WorkRunTrigger::Manual)
            .unwrap();
        manager
            .finish_run(&task.id, &run.id, WorkRunStatus::Cancelled, None, None)
            .unwrap();

        let duplicate = manager.duplicate_task(&task.id).unwrap();
        assert_ne!(duplicate.id, task.id);
        assert_eq!(duplicate.title, "每日产品简报（副本）");
        assert_eq!(duplicate.instructions, task.instructions);
        assert_eq!(duplicate.run_count, 0);
        assert!(duplicate.last_run_id.is_none());
        assert!(duplicate.last_run_at.is_none());
        let schedule = duplicate.schedule.unwrap();
        assert!(!schedule.enabled);
        assert!(schedule.next_run_at.is_none());
        assert!(schedule.last_run_at.is_none());
        assert!(manager.list_runs(&duplicate.id).unwrap().is_empty());
    }
}
