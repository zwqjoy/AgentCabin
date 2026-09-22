//! Backend-only scheduled Work trigger layer.
//!
//! This module deliberately does not contain an agent runtime.  It calculates
//! fire times, applies durable idempotency/concurrency gates, creates the
//! product WorkRun, and delegates execution to the existing Work session
//! service.

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::spawn_locks::SpawnLocks;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::models::{
    WorkRun, WorkRunTrigger, WorkScheduleKind, WorkTask, WorkTaskSource, WorkTaskStatus,
};
use crate::work::{paths::WorkPaths, profile, tasks::TaskManager};
use tokio_util::sync::CancellationToken;

const TICK: StdDuration = StdDuration::from_secs(15);
const MAX_STARTUP_MISSED_CHECKS: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleDecision {
    Fire(String),
    Missed(String),
    None,
}

#[derive(Debug, Clone)]
pub enum ScheduleOutcome {
    Fire { task_id: String, fire_key: String },
    Skipped(Box<WorkRun>),
    None,
}

#[derive(Clone)]
pub struct WorkScheduler {
    paths: WorkPaths,
}

impl WorkScheduler {
    pub fn new(paths: WorkPaths) -> Self {
        Self { paths }
    }

    /// Start the in-process scheduler. It stops when the app cancellation token
    /// is cancelled; when the app exits, no scheduler remains behind.
    pub fn spawn(
        &self,
        emitter: Arc<BroadcastEmitter>,
        sessions: ActorSessionMap,
        spawn_locks: SpawnLocks,
        cancel: CancellationToken,
    ) {
        let scheduler = self.clone();
        tauri::async_runtime::spawn(async move {
            if profile::is_enabled() {
                if let Err(error) = scheduler
                    .reconcile_startup_and_start(&emitter, &sessions, &spawn_locks)
                    .await
                {
                    log::warn!("[work/scheduler] startup reconciliation failed: {error}");
                }
            }

            let mut interval = tokio::time::interval(TICK);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if !profile::is_enabled() {
                            continue;
                        }
                        if let Err(error) = scheduler.tick(&emitter, &sessions, &spawn_locks).await {
                            log::warn!("[work/scheduler] tick failed: {error}");
                        }
                    }
                }
            }
            log::debug!("[work/scheduler] stopped");
        });
    }

    /// Evaluates scheduler concurrency and fire gates for a single task at `now`.
    /// Returns:
    /// - Ok(ScheduleOutcome::Skipped(run)) if skipped due to concurrency or max runs
    /// - Ok(ScheduleOutcome::Fire { task_id, fire_key }) if task is due and eligible to run
    /// - Ok(ScheduleOutcome::None) if not due, disabled, archived, or already ran
    pub fn evaluate_task_fire_gate(
        &self,
        task_id: &str,
        now: DateTime<Utc>,
    ) -> Result<ScheduleOutcome, String> {
        let manager = TaskManager::new(self.paths.clone());
        let task = manager.get_task(task_id)?;
        let Some(schedule) = task.schedule.as_ref() else {
            return Ok(ScheduleOutcome::None);
        };
        if !schedule.enabled || matches!(task.status, WorkTaskStatus::Archived) {
            return Ok(ScheduleOutcome::None);
        }
        let Some(next) = schedule.next_run_at.as_deref() else {
            return Ok(ScheduleOutcome::None);
        };
        let fire = parse_utc(next)?;
        if fire > now {
            return Ok(ScheduleOutcome::None);
        }
        let fire_key = fire.to_rfc3339();
        let next_after = next_fire_after(&task, fire)?;

        manager.claim_and_evaluate_schedule_fire(task_id, &fire_key, next_after)
    }

    pub async fn tick(
        &self,
        emitter: &Arc<BroadcastEmitter>,
        sessions: &ActorSessionMap,
        spawn_locks: &SpawnLocks,
    ) -> Result<Vec<WorkRun>, String> {
        let manager = TaskManager::new(self.paths.clone());
        let now = Utc::now();
        let mut started = Vec::new();
        for task in manager.list_tasks(None)? {
            match self.evaluate_task_fire_gate(&task.id, now)? {
                ScheduleOutcome::Fire { task_id, fire_key } => {
                    let paths = self.paths.clone();
                    let emitter = Arc::clone(emitter);
                    let sessions = sessions.clone();
                    let spawn_locks = spawn_locks.clone();
                    let cancel_token = CancellationToken::new();
                    let fire_key_clone = fire_key.clone();

                    tokio::spawn(async move {
                        if let Err(error) = crate::commands::work::start_work_task_run(
                            &paths,
                            &emitter,
                            &sessions,
                            &spawn_locks,
                            &cancel_token,
                            &task_id,
                            WorkRunTrigger::Scheduled,
                            crate::work::models::ExecutionContext::Unattended,
                            Some(&fire_key_clone),
                        )
                        .await
                        {
                            log::error!(
                                "[work/scheduler] scheduled task {} failed to start: {}",
                                task_id,
                                error
                            );
                        }
                    });
                }
                ScheduleOutcome::Skipped(skipped_run) => {
                    started.push(*skipped_run);
                }
                ScheduleOutcome::None => {}
            }
        }
        Ok(started)
    }

    /// Consume one due fire per task during startup and send it through the
    /// same WorkRun/session path as the normal scheduler tick. The old
    /// `reconcile_startup` API remains available for read-only migration and
    /// tests, but the application path must not turn a missed automation into
    /// a permanently skipped run.
    pub async fn reconcile_startup_and_start(
        &self,
        emitter: &Arc<BroadcastEmitter>,
        sessions: &ActorSessionMap,
        spawn_locks: &SpawnLocks,
    ) -> Result<usize, String> {
        let manager = TaskManager::new(self.paths.clone());
        let now = Utc::now();
        let mut started = 0;
        for task in manager.list_tasks(None)? {
            let outcome = self.evaluate_task_fire_gate(&task.id, now)?;
            if let ScheduleOutcome::Fire { task_id, fire_key } = outcome {
                let paths = self.paths.clone();
                let emitter = Arc::clone(emitter);
                let sessions = sessions.clone();
                let spawn_locks = spawn_locks.clone();
                let fire_key = fire_key.clone();
                let task_id_for_log = task_id.clone();
                tokio::spawn(async move {
                    let cancel_token = CancellationToken::new();
                    if let Err(error) = crate::commands::work::start_work_task_run(
                        &paths,
                        &emitter,
                        &sessions,
                        &spawn_locks,
                        &cancel_token,
                        &task_id,
                        WorkRunTrigger::Scheduled,
                        crate::work::models::ExecutionContext::Unattended,
                        Some(&fire_key),
                    )
                    .await
                    {
                        log::error!(
                            "[work/scheduler] startup scheduled task {} failed to start: {}",
                            task_id_for_log,
                            error
                        );
                    }
                });
                started += 1;
            }
        }
        Ok(started)
    }

    /// Evaluates scheduler concurrency and fire gates for a single task at `now`
    /// and returns the started or skipped run.
    pub fn evaluate_and_advance_task(
        &self,
        task_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<WorkRun>, String> {
        let manager = TaskManager::new(self.paths.clone());
        match self.evaluate_task_fire_gate(task_id, now)? {
            ScheduleOutcome::Fire { task_id, fire_key } => {
                let runs = manager.list_runs(&task_id)?;
                let active = runs
                    .into_iter()
                    .find(|r| r.scheduled_for.as_deref() == Some(&fire_key));
                Ok(active)
            }
            ScheduleOutcome::Skipped(run) => Ok(Some(*run)),
            ScheduleOutcome::None => Ok(None),
        }
    }

    pub fn reconcile_startup(&self) -> Result<usize, String> {
        let manager = TaskManager::new(self.paths.clone());
        let now = Utc::now();
        let mut count = 0;
        for task in manager.list_tasks(None)? {
            let Some(schedule) = task.schedule.as_ref() else {
                continue;
            };
            if !schedule.enabled || matches!(task.status, WorkTaskStatus::Archived) {
                continue;
            }
            let Some(next) = schedule.next_run_at.as_deref() else {
                continue;
            };
            let fire = parse_utc(next)?;
            if fire < now {
                // Pass `now` (not `fire`) to push next_run_at to the future and prevent catch-up loop.
                let fire_key = fire.to_rfc3339();
                let next_after = next_fire_after(&task, now)?;
                if manager
                    .claim_missed_schedule_fire(&task.id, &fire_key, next_after)?
                    .is_some()
                {
                    count += 1;
                }
                if count >= MAX_STARTUP_MISSED_CHECKS * manager.list_tasks(None)?.len().max(1) {
                    break;
                }
            }
        }
        Ok(count)
    }
}

pub fn calculate_next_run(
    schedule: &crate::work::models::WorkScheduleConfig,
    now: DateTime<Utc>,
) -> Result<Option<String>, String> {
    let task = WorkTask {
        id: "calc".into(),
        workspace_id: "ws-calc".into(),
        title: "calc".into(),
        instructions: String::new(),
        status: WorkTaskStatus::Active,
        source: WorkTaskSource::Manual,
        policy: Default::default(),
        schedule: Some(schedule.clone()),
        required_artifacts: Vec::new(),
        artifact_requirements: Vec::new(),
        run_count: 0,
        last_run_id: None,
        last_run_at: None,
        created_at: String::new(),
        updated_at: String::new(),
    };
    next_fire_after(&task, now)
}

fn next_fire_after(task: &WorkTask, after: DateTime<Utc>) -> Result<Option<String>, String> {
    let schedule = task
        .schedule
        .as_ref()
        .ok_or_else(|| "Task has no schedule".to_string())?;
    let timezone: Tz = schedule
        .timezone
        .parse()
        .map_err(|_| format!("Unknown timezone: {}", schedule.timezone))?;
    let local_after = after.with_timezone(&timezone);

    match schedule.kind {
        WorkScheduleKind::Once => {
            if let Some(fire_str) = schedule.fire_at.as_deref() {
                let dt = parse_utc(fire_str)?;
                if dt > after {
                    return Ok(Some(dt.to_rfc3339()));
                }
            }
            Ok(None)
        }
        WorkScheduleKind::Daily | WorkScheduleKind::Weekly => {
            let time = parse_time(
                schedule
                    .time_of_day
                    .as_deref()
                    .ok_or("Daily/weekly schedule needs time_of_day")?,
            )?;
            for days in 0..=8 {
                let date = local_after.date_naive() + Duration::days(days);
                if schedule.kind == WorkScheduleKind::Weekly
                    && schedule.day_of_week != Some(date.weekday().num_days_from_monday() as u8)
                {
                    continue;
                }
                if let Some(local) = timezone
                    .from_local_datetime(&NaiveDateTime::new(date, time))
                    .single()
                {
                    let utc = local.with_timezone(&Utc);
                    if utc > after {
                        return Ok(Some(utc.to_rfc3339()));
                    }
                }
            }
            Err("Unable to calculate next daily/weekly fire".into())
        }
        WorkScheduleKind::Cron => {
            if let Some(expr_str) = schedule.cron_expression.as_deref() {
                let local_after = after.with_timezone(&timezone).naive_local();
                if let Some(next_local_naive) = parse_simple_cron_next(expr_str, local_after) {
                    if let Some(next_local) =
                        timezone.from_local_datetime(&next_local_naive).single()
                    {
                        return Ok(Some(next_local.with_timezone(&Utc).to_rfc3339()));
                    }
                }
            }
            Err("Unsupported or invalid cron expression".into())
        }
    }
}

/// Simple 5-field cron parser for standard expressions (e.g. "0 9 * * *", "*/15 * * * *").
fn parse_simple_cron_next(expr: &str, after: NaiveDateTime) -> Option<NaiveDateTime> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    // Advance minute by minute up to 31 days max to find matching minute
    let mut check = after + Duration::minutes(1);
    check = check.with_second(0).unwrap_or(check);

    for _ in 0..(31 * 24 * 60) {
        let weekday = check.weekday().num_days_from_sunday() as u8;
        let weekday_matches =
            match_cron_field(parts[4], weekday) || (weekday == 0 && match_cron_field(parts[4], 7));
        if match_cron_field(parts[0], check.minute() as u8)
            && match_cron_field(parts[1], check.hour() as u8)
            && match_cron_field(parts[2], check.day() as u8)
            && match_cron_field(parts[3], check.month() as u8)
            && weekday_matches
        {
            return Some(check);
        }
        check += Duration::minutes(1);
    }
    None
}

fn match_cron_field(pattern: &str, value: u8) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(step_str) = pattern.strip_prefix("*/") {
        if let Ok(step) = step_str.parse::<u8>() {
            return step > 0 && value.is_multiple_of(step);
        }
    }
    for sub in pattern.split(',') {
        if let Ok(single) = sub.parse::<u8>() {
            if single == value {
                return true;
            }
        } else if let Some((start_s, end_s)) = sub.split_once('-') {
            if let (Ok(start), Ok(end)) = (start_s.parse::<u8>(), end_s.parse::<u8>()) {
                if value >= start && value <= end {
                    return true;
                }
            }
        }
    }
    false
}

fn parse_utc(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|e| format!("Invalid UTC schedule time: {e}"))
}

fn parse_time(value: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| format!("Invalid time_of_day: {value}; expected HH:MM"))
}

#[cfg(test)]
mod tests {
    use super::{calculate_next_run, WorkScheduleKind, WorkScheduler};
    use crate::work::models::{WorkRunStatus, WorkScheduleConfig};
    use crate::work::paths::WorkPaths;
    use crate::work::tasks::TaskManager;
    use chrono::{DateTime, Utc};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use tempfile::TempDir;

    fn schedule(kind: WorkScheduleKind, time: &str, day: Option<u8>) -> WorkScheduleConfig {
        WorkScheduleConfig {
            kind,
            enabled: true,
            timezone: "Asia/Shanghai".into(),
            fire_at: None,
            time_of_day: Some(time.into()),
            day_of_week: day,
            cron_expression: None,
            next_run_at: None,
            last_run_at: None,
            max_runs: None,
            run_on_startup: false,
        }
    }

    #[test]
    fn daily_returns_same_day_if_time_is_in_future() {
        // 2026-08-13 00:00:00 UTC = 08:00:00 Asia/Shanghai.
        // Daily schedule for 09:00 should return 2026-08-13 09:00:00 Asia/Shanghai (01:00 UTC)
        let now = DateTime::parse_from_rfc3339("2026-08-13T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = calculate_next_run(&schedule(WorkScheduleKind::Daily, "09:00", None), now)
            .unwrap()
            .unwrap();
        assert_eq!(next, "2026-08-13T01:00:00+00:00");
    }

    #[test]
    fn once_schedule_returns_fire_at_if_in_future() {
        let now = DateTime::parse_from_rfc3339("2026-08-13T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut cfg = schedule(WorkScheduleKind::Once, "00:00", None);
        cfg.fire_at = Some("2026-08-15T10:00:00Z".to_string());
        let next = calculate_next_run(&cfg, now).unwrap().unwrap();
        assert_eq!(next, "2026-08-15T10:00:00+00:00");
    }

    #[test]
    fn cron_schedule_parses_and_calculates_next_fire() {
        let now = DateTime::parse_from_rfc3339("2026-08-13T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut cfg = schedule(WorkScheduleKind::Cron, "00:00", None);
        cfg.cron_expression = Some("0 9 * * *".to_string()); // 09:00 Asia/Shanghai every day
        let next = calculate_next_run(&cfg, now).unwrap().unwrap();
        assert_eq!(next, "2026-08-13T01:00:00+00:00");
    }

    #[test]
    fn cron_schedule_accepts_sunday_as_zero_or_seven() {
        let now = DateTime::parse_from_rfc3339("2026-08-13T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        for sunday in ["0", "7"] {
            let mut cfg = schedule(WorkScheduleKind::Cron, "00:00", None);
            cfg.timezone = "UTC".into();
            cfg.cron_expression = Some(format!("0 9 * * {sunday}"));
            let next = calculate_next_run(&cfg, now).unwrap().unwrap();
            assert_eq!(next, "2026-08-16T09:00:00+00:00");
        }
    }

    #[test]
    fn concurrent_same_fire_claims_only_one_run() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let fire = DateTime::parse_from_rfc3339("2026-08-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut task = manager
            .create_task("ws-scheduler-race", "Race", "Run once", None)
            .unwrap();
        task.schedule = Some(WorkScheduleConfig {
            kind: WorkScheduleKind::Once,
            enabled: true,
            timezone: "UTC".to_string(),
            fire_at: Some(fire.to_rfc3339()),
            time_of_day: None,
            day_of_week: None,
            cron_expression: None,
            next_run_at: Some(fire.to_rfc3339()),
            last_run_at: None,
            max_runs: None,
            run_on_startup: false,
        });
        manager.update_task(&mut task).unwrap();

        let scheduler = Arc::new(WorkScheduler::new(paths.clone()));
        let barrier = Arc::new(Barrier::new(2));
        let handles = (0..2)
            .map(|_| {
                let scheduler = Arc::clone(&scheduler);
                let barrier = Arc::clone(&barrier);
                let task_id = task.id.clone();
                thread::spawn(move || {
                    barrier.wait();
                    scheduler.evaluate_and_advance_task(&task_id, fire)
                })
            })
            .collect::<Vec<_>>();

        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_some()).count(), 1);

        let runs = manager.list_runs(&task.id).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, WorkRunStatus::Running);
        assert_eq!(
            runs[0].scheduled_for.as_deref(),
            Some(fire.to_rfc3339().as_str())
        );
        assert_eq!(
            manager
                .get_task(&task.id)
                .unwrap()
                .schedule
                .and_then(|schedule| schedule.next_run_at),
            None
        );
    }

    #[test]
    fn one_shot_fire_skipped_by_active_run_is_consumed() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let fire = DateTime::parse_from_rfc3339("2026-08-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut task = manager
            .create_task("ws-scheduler-once", "Once", "Run once", None)
            .unwrap();
        task.schedule = Some(WorkScheduleConfig {
            kind: WorkScheduleKind::Once,
            enabled: true,
            timezone: "UTC".to_string(),
            fire_at: Some(fire.to_rfc3339()),
            time_of_day: None,
            day_of_week: None,
            cron_expression: None,
            next_run_at: Some(fire.to_rfc3339()),
            last_run_at: None,
            max_runs: None,
            run_on_startup: false,
        });
        manager.update_task(&mut task).unwrap();
        manager
            .start_run(&task.id, None, crate::work::models::WorkRunTrigger::Manual)
            .unwrap();

        let scheduler = WorkScheduler::new(paths);
        let skipped = scheduler
            .evaluate_and_advance_task(&task.id, fire)
            .unwrap()
            .unwrap();
        assert_eq!(skipped.status, WorkRunStatus::Skipped);
        assert_eq!(
            skipped.skipped_reason.as_deref(),
            Some("previous_run_active")
        );
        assert_eq!(
            manager
                .get_task(&task.id)
                .unwrap()
                .schedule
                .and_then(|schedule| schedule.next_run_at),
            None
        );
    }

    #[test]
    fn concurrent_startup_reconciliation_records_one_missed_fire() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = TaskManager::new(paths.clone());
        let fire = DateTime::parse_from_rfc3339("2026-08-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut task = manager
            .create_task("ws-scheduler-missed-race", "Missed", "Recover once", None)
            .unwrap();
        task.schedule = Some(WorkScheduleConfig {
            kind: WorkScheduleKind::Once,
            enabled: true,
            timezone: "UTC".to_string(),
            fire_at: Some(fire.to_rfc3339()),
            time_of_day: None,
            day_of_week: None,
            cron_expression: None,
            next_run_at: Some(fire.to_rfc3339()),
            last_run_at: None,
            max_runs: None,
            run_on_startup: false,
        });
        manager.update_task(&mut task).unwrap();

        let scheduler = Arc::new(WorkScheduler::new(paths));
        let barrier = Arc::new(Barrier::new(2));
        let handles = (0..2)
            .map(|_| {
                let scheduler = Arc::clone(&scheduler);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    scheduler.reconcile_startup()
                })
            })
            .collect::<Vec<_>>();
        let counts = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(counts.iter().sum::<usize>(), 1);
        let runs = manager.list_runs(&task.id).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, WorkRunStatus::Skipped);
        assert_eq!(runs[0].skipped_reason.as_deref(), Some("missed"));
        assert_eq!(
            manager
                .get_task(&task.id)
                .unwrap()
                .schedule
                .and_then(|schedule| schedule.next_run_at),
            None
        );
    }
}
