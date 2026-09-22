//! Minimal durable Work Runtime Ledger.
//!
//! Records durable facts required for deterministic recovery, audit, and
//! reconciliation without duplicating full Pi transcripts.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::work::models::{RuntimeFact, SideEffectClass};
use crate::work::paths::WorkPaths;

type LedgerLock = Arc<Mutex<()>>;

/// A ledger is opened by the pipeline, progress projection, and lifecycle
/// reconciler independently. Share one lock per path so their read-check-write
/// idempotency operations are serialized across instances.
static LEDGER_LOCKS: Lazy<Mutex<HashMap<PathBuf, LedgerLock>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn shared_ledger_lock(path: &Path) -> LedgerLock {
    let mut locks = LEDGER_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Tool was already executed and completed with an authoritative ToolResult.
    AlreadyCompleted,
    /// Deterministic read or verifiable local operation that is safe to retry.
    SafeToReplay,
    /// Local output verified to exist on disk as expected.
    VerifiedLocalOutputExists,
    /// External mutation or uncertain state: NEVER blindly replayed.
    UnknownOutcomeNeedsAttention { reason: String },
}

pub struct WorkRuntimeLedger {
    ledger_path: PathBuf,
    lock: LedgerLock,
}

impl WorkRuntimeLedger {
    pub fn open(paths: &WorkPaths, task_id: &str, run_id: &str) -> Result<Self, String> {
        let ledger_path = paths.task_run_ledger_path(task_id, run_id)?;
        if let Some(parent) = ledger_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Ok(Self {
            lock: shared_ledger_lock(&ledger_path),
            ledger_path,
        })
    }

    pub fn for_path(ledger_path: PathBuf) -> Self {
        if let Some(parent) = ledger_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Self {
            lock: shared_ledger_lock(&ledger_path),
            ledger_path,
        }
    }

    /// Record a durable fact in the ledger.
    pub fn record(&self, fact: &RuntimeFact) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        self.append_fact_locked(fact)
    }

    fn append_fact_locked(&self, fact: &RuntimeFact) -> Result<(), String> {
        let json_line = serde_json::to_string(fact)
            .map_err(|e| format!("Failed to serialize runtime fact: {e}"))?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.ledger_path)
            .map_err(|e| {
                format!(
                    "Failed to open runtime ledger {}: {e}",
                    self.ledger_path.display()
                )
            })?;

        writeln!(file, "{json_line}").map_err(|e| e.to_string())?;
        file.sync_data().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Append a recovery audit event only once for the same run/tool/action.
    /// The existence check and append share the ledger mutex, making repeated
    /// startup reconciliation and double-clicked recovery actions harmless.
    #[allow(clippy::too_many_arguments)]
    pub fn record_recovery_event_once(
        &self,
        event: &str,
        work_run_id: &str,
        original_tool_call_id: Option<&str>,
        recovery_action: &str,
        side_effect_class: SideEffectClass,
        reused_existing_output: bool,
        result: Option<String>,
        error: Option<String>,
    ) -> Result<bool, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let facts = self.read_facts()?;
        let exists = facts.iter().any(|fact| match fact {
            RuntimeFact::RecoveryDetected {
                work_run_id: id,
                original_tool_call_id: call_id,
                recovery_action: action,
                ..
            }
            | RuntimeFact::RecoveryActionStarted {
                work_run_id: id,
                original_tool_call_id: call_id,
                recovery_action: action,
                ..
            }
            | RuntimeFact::RecoveryResolved {
                work_run_id: id,
                original_tool_call_id: call_id,
                recovery_action: action,
                ..
            }
            | RuntimeFact::RecoveryAbandoned {
                work_run_id: id,
                original_tool_call_id: call_id,
                recovery_action: action,
                ..
            } => {
                event_matches(fact, event)
                    && id == work_run_id
                    && call_id.as_deref() == original_tool_call_id
                    && action == recovery_action
            }
            _ => false,
        });
        if exists {
            return Ok(false);
        }

        let timestamp = crate::models::now_iso();
        let fact = match event {
            "recovery_detected" => RuntimeFact::RecoveryDetected {
                work_run_id: work_run_id.to_string(),
                original_tool_call_id: original_tool_call_id.map(str::to_string),
                recovery_action: recovery_action.to_string(),
                side_effect_class,
                reused_existing_output,
                result,
                error,
                timestamp,
            },
            "recovery_action_started" => RuntimeFact::RecoveryActionStarted {
                work_run_id: work_run_id.to_string(),
                original_tool_call_id: original_tool_call_id.map(str::to_string),
                recovery_action: recovery_action.to_string(),
                side_effect_class,
                reused_existing_output,
                result,
                error,
                timestamp,
            },
            "recovery_resolved" => RuntimeFact::RecoveryResolved {
                work_run_id: work_run_id.to_string(),
                original_tool_call_id: original_tool_call_id.map(str::to_string),
                recovery_action: recovery_action.to_string(),
                side_effect_class,
                reused_existing_output,
                result,
                error,
                timestamp,
            },
            "recovery_abandoned" => RuntimeFact::RecoveryAbandoned {
                work_run_id: work_run_id.to_string(),
                original_tool_call_id: original_tool_call_id.map(str::to_string),
                recovery_action: recovery_action.to_string(),
                side_effect_class,
                reused_existing_output,
                result,
                error,
                timestamp,
            },
            _ => return Err(format!("Unknown recovery event: {event}")),
        };
        self.append_fact_locked(&fact)?;
        Ok(true)
    }

    /// Append a Guardian anomaly fact only once for the same anomaly kind and tool/step.
    pub fn record_guardian_event_once(
        &self,
        anomaly_kind: crate::work::models::GuardianAnomalyKind,
        step_id: Option<&str>,
        tool_call_id: Option<&str>,
        reason: &str,
        threshold: Option<&str>,
        suggested_action: Option<&str>,
    ) -> Result<bool, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let facts = self.read_facts()?;
        let exists = facts.iter().any(|fact| match fact {
            RuntimeFact::GuardianAnomalyDetected {
                anomaly_kind: k,
                step_id: sid,
                tool_call_id: cid,
                ..
            } => *k == anomaly_kind && sid.as_deref() == step_id && cid.as_deref() == tool_call_id,
            _ => false,
        });
        if exists {
            return Ok(false);
        }

        let fact = RuntimeFact::GuardianAnomalyDetected {
            anomaly_kind,
            step_id: step_id.map(str::to_string),
            tool_call_id: tool_call_id.map(str::to_string),
            reason: reason.to_string(),
            threshold: threshold.map(str::to_string),
            suggested_action: suggested_action.map(str::to_string),
            timestamp: crate::models::now_iso(),
        };
        self.append_fact_locked(&fact)?;
        Ok(true)
    }

    /// Append a fact generated by Guardian only once. The comparison ignores
    /// volatile timestamps, which makes concurrent progress/lifecycle
    /// projections idempotent even when they evaluated the same snapshot.
    pub fn record_guardian_fact_once(&self, fact: &RuntimeFact) -> Result<bool, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let facts = self.read_facts()?;
        if guardian_fact_already_recorded(&facts, fact) {
            return Ok(false);
        }
        self.append_fact_locked(fact)?;
        Ok(true)
    }

    /// Append a LoopDetected fact idempotently.
    pub fn record_loop_detected_once(
        &self,
        fingerprint: &str,
        repeat_count: u32,
        tool_name: &str,
    ) -> Result<bool, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let facts = self.read_facts()?;
        let exists = facts.iter().any(|fact| match fact {
            RuntimeFact::LoopDetected {
                fingerprint: fp,
                repeat_count: rc,
                ..
            } => fp == fingerprint && *rc >= repeat_count,
            _ => false,
        });
        if exists {
            return Ok(false);
        }

        let fact = RuntimeFact::LoopDetected {
            fingerprint: fingerprint.to_string(),
            repeat_count,
            tool_name: tool_name.to_string(),
            timestamp: crate::models::now_iso(),
        };
        self.append_fact_locked(&fact)?;
        Ok(true)
    }

    /// Read all recorded facts in chronological order.
    pub fn list_facts(&self) -> Result<Vec<RuntimeFact>, String> {
        self.read_facts()
    }

    fn read_facts(&self) -> Result<Vec<RuntimeFact>, String> {
        if !self.ledger_path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.ledger_path)
            .map_err(|e| format!("Failed to read ledger {}: {e}", self.ledger_path.display()))?;
        let reader = BufReader::new(file);

        let mut facts = Vec::new();
        for (line_number, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| e.to_string())?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let fact = serde_json::from_str::<RuntimeFact>(trimmed).map_err(|error| {
                format!(
                    "Invalid runtime ledger {} at line {}: {error}",
                    self.ledger_path.display(),
                    line_number + 1
                )
            })?;
            facts.push(fact);
        }
        Ok(facts)
    }

    /// Was this tool call proposed by the model?
    pub fn was_tool_proposed(&self, tool_call_id: &str) -> bool {
        let Ok(facts) = self.list_facts() else {
            return false;
        };
        facts.iter().any(|f| match f {
            RuntimeFact::ToolProposed {
                tool_call_id: id, ..
            } => id == tool_call_id,
            _ => false,
        })
    }

    /// Was this tool execution started?
    pub fn was_tool_started(&self, tool_call_id: &str) -> bool {
        let Ok(facts) = self.list_facts() else {
            return false;
        };
        facts.iter().any(|f| match f {
            RuntimeFact::ToolStarted {
                tool_call_id: id, ..
            } => id == tool_call_id,
            _ => false,
        })
    }

    /// Was an authoritative result recorded for this tool call?
    pub fn get_tool_result(&self, tool_call_id: &str) -> Option<RuntimeFact> {
        let Ok(facts) = self.list_facts() else {
            return None;
        };
        facts.into_iter().rev().find(|f| match f {
            RuntimeFact::ToolResult {
                tool_call_id: id, ..
            } => id == tool_call_id,
            _ => false,
        })
    }

    /// Is the run marked as interrupted?
    pub fn is_interrupted(&self) -> bool {
        let Ok(facts) = self.list_facts() else {
            return false;
        };
        facts
            .iter()
            .any(|f| matches!(f, RuntimeFact::RuntimeInterrupted { .. }))
    }

    /// Determine recovery action based on side effects.
    ///
    /// READ / idempotent: safe to retry.
    /// LOCAL with verifiable output: check if output exists and is non-empty.
    /// EXTERNAL mutation: NEVER blindly replay if ToolResult is missing.
    pub fn classify_recovery(
        &self,
        tool_call_id: &str,
        side_effect: SideEffectClass,
        expected_outputs: &[String],
        workspace_root: Option<&std::path::Path>,
    ) -> RecoveryAction {
        let facts = match self.list_facts() {
            Ok(facts) => facts,
            Err(error) => {
                return RecoveryAction::UnknownOutcomeNeedsAttention {
                    reason: format!(
                        "Runtime ledger could not be read; recovery for tool call '{tool_call_id}' is blocked until the ledger is repaired: {error}"
                    ),
                }
            }
        };

        if facts.iter().rev().any(|fact| {
            matches!(
                fact,
                RuntimeFact::ToolResult {
                    tool_call_id: id, ..
                } if id == tool_call_id
            )
        }) {
            return RecoveryAction::AlreadyCompleted;
        }

        let started = facts.iter().any(|fact| {
            matches!(
                fact,
                RuntimeFact::ToolStarted {
                    tool_call_id: id, ..
                } if id == tool_call_id
            )
        });

        match side_effect {
            SideEffectClass::Read => RecoveryAction::SafeToReplay,
            SideEffectClass::LocalVerifiable => {
                if !started {
                    return RecoveryAction::SafeToReplay;
                }
                // If expected outputs exist and are non-empty, consider local output verified
                if let Some(root) = workspace_root {
                    if !expected_outputs.is_empty()
                        && expected_outputs
                            .iter()
                            .all(|out| verified_local_output(root, out))
                    {
                        return RecoveryAction::VerifiedLocalOutputExists;
                    }
                }
                RecoveryAction::SafeToReplay
            }
            SideEffectClass::ExternalMutating => {
                if !started {
                    // Tool call was only proposed, never started
                    RecoveryAction::SafeToReplay
                } else {
                    // Execution was started on external side effect but outcome was not recorded
                    RecoveryAction::UnknownOutcomeNeedsAttention {
                        reason: format!(
                            "External mutating tool call '{tool_call_id}' was started but result was not durably recorded. Manual confirmation or reconciliation is required before retry."
                        ),
                    }
                }
            }
        }
    }
}

fn verified_local_output(root: &std::path::Path, raw_output: &str) -> bool {
    let relative = raw_output.trim().trim_start_matches("./");
    let path = std::path::Path::new(relative);
    if path.is_absolute()
        || path.components().next()
            != Some(std::path::Component::Normal(std::ffi::OsStr::new("output")))
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return false;
    }

    let output_root = root.join("output");
    let candidate = root.join(path);
    let Ok(canonical_root) = fs::canonicalize(output_root) else {
        return false;
    };
    let Ok(canonical_candidate) = fs::canonicalize(&candidate) else {
        return false;
    };
    if !canonical_candidate.starts_with(canonical_root) {
        return false;
    }

    let Ok(metadata) = fs::symlink_metadata(candidate) else {
        return false;
    };
    metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() > 0
}

fn event_matches(fact: &RuntimeFact, event: &str) -> bool {
    matches!(
        (event, fact),
        ("recovery_detected", RuntimeFact::RecoveryDetected { .. })
            | (
                "recovery_action_started",
                RuntimeFact::RecoveryActionStarted { .. }
            )
            | ("recovery_resolved", RuntimeFact::RecoveryResolved { .. })
            | ("recovery_abandoned", RuntimeFact::RecoveryAbandoned { .. })
    )
}

fn guardian_fact_already_recorded(facts: &[RuntimeFact], candidate: &RuntimeFact) -> bool {
    match candidate {
        // State transitions are a sequence, not a set. Compare with the
        // latest transition only so a later recovery can legitimately record
        // the same transition again in a new anomaly cycle.
        RuntimeFact::RunHealthChanged { .. } => facts
            .iter()
            .rev()
            .find(|fact| matches!(fact, RuntimeFact::RunHealthChanged { .. }))
            .is_some_and(|existing| same_guardian_fact(existing, candidate)),
        RuntimeFact::RuntimeLivenessChanged { .. } => facts
            .iter()
            .rev()
            .find(|fact| matches!(fact, RuntimeFact::RuntimeLivenessChanged { .. }))
            .is_some_and(|existing| same_guardian_fact(existing, candidate)),
        _ => facts
            .iter()
            .any(|existing| same_guardian_fact(existing, candidate)),
    }
}

fn same_guardian_fact(existing: &RuntimeFact, candidate: &RuntimeFact) -> bool {
    match (existing, candidate) {
        (
            RuntimeFact::RunHealthChanged {
                previous: existing_previous,
                current: existing_current,
                reason: existing_reason,
                ..
            },
            RuntimeFact::RunHealthChanged {
                previous: candidate_previous,
                current: candidate_current,
                reason: candidate_reason,
                ..
            },
        ) => {
            existing_previous == candidate_previous
                && existing_current == candidate_current
                && existing_reason == candidate_reason
        }
        (
            RuntimeFact::RuntimeLivenessChanged {
                previous: existing_previous,
                current: existing_current,
                reason: existing_reason,
                ..
            },
            RuntimeFact::RuntimeLivenessChanged {
                previous: candidate_previous,
                current: candidate_current,
                reason: candidate_reason,
                ..
            },
        ) => {
            existing_previous == candidate_previous
                && existing_current == candidate_current
                && existing_reason == candidate_reason
        }
        (
            RuntimeFact::RunStalled {
                idle_seconds: existing_idle,
                last_progress_fact: existing_progress,
                ..
            },
            RuntimeFact::RunStalled {
                idle_seconds: candidate_idle,
                last_progress_fact: candidate_progress,
                ..
            },
        ) => existing_idle >= candidate_idle && existing_progress == candidate_progress,
        (
            RuntimeFact::LoopDetected {
                fingerprint: existing_fingerprint,
                repeat_count: existing_count,
                ..
            },
            RuntimeFact::LoopDetected {
                fingerprint: candidate_fingerprint,
                repeat_count: candidate_count,
                ..
            },
        ) => existing_fingerprint == candidate_fingerprint && existing_count >= candidate_count,
        (
            RuntimeFact::ToolFailureStreakDetected {
                consecutive_failures: existing_count,
                ..
            },
            RuntimeFact::ToolFailureStreakDetected {
                consecutive_failures: candidate_count,
                ..
            },
        ) => existing_count >= candidate_count,
        (
            RuntimeFact::BudgetWarning {
                budget_kind: existing_kind,
                ..
            },
            RuntimeFact::BudgetWarning {
                budget_kind: candidate_kind,
                ..
            },
        ) => existing_kind == candidate_kind,
        (
            RuntimeFact::BudgetExceeded {
                budget_kind: existing_kind,
                ..
            },
            RuntimeFact::BudgetExceeded {
                budget_kind: candidate_kind,
                ..
            },
        ) => existing_kind == candidate_kind,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::models::{CollaborationMode, ExecutionContext, ToolConcurrencyClass};
    use tempfile::TempDir;

    #[test]
    fn ledger_appends_and_reads_chronological_facts() {
        let temp = TempDir::new().unwrap();
        let ledger_file = temp.path().join("run-1.ledger.jsonl");
        let ledger = WorkRuntimeLedger::for_path(ledger_file);

        ledger
            .record(&RuntimeFact::RunStarted {
                task_id: "task-1".to_string(),
                work_run_id: "run-1".to_string(),
                execution_context: ExecutionContext::Attended,
                collaboration_mode: CollaborationMode::Default,
                timestamp: "2026-08-14T00:00:00Z".to_string(),
            })
            .unwrap();

        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "call-1".to_string(),
                tool_name: "work_read_file".to_string(),
                action: "read".to_string(),
                arguments_hash: "hash1".to_string(),
                expected_outputs: Vec::new(),
                side_effect_class: SideEffectClass::Read,
                concurrency_class: ToolConcurrencyClass::ParallelSafe,
                timestamp: "2026-08-14T00:00:01Z".to_string(),
            })
            .unwrap();

        let facts = ledger.list_facts().unwrap();
        assert_eq!(facts.len(), 2);
        assert!(ledger.was_tool_proposed("call-1"));
        assert!(!ledger.was_tool_started("call-1"));
        assert!(!ledger.is_interrupted());
    }

    #[test]
    fn recovery_audit_events_are_idempotent_per_run_call_and_action() {
        let temp = TempDir::new().unwrap();
        let ledger = WorkRuntimeLedger::for_path(temp.path().join("run.ledger.jsonl"));

        assert!(ledger
            .record_recovery_event_once(
                "recovery_detected",
                "run-1",
                Some("call-1"),
                "external_unknown_outcome",
                SideEffectClass::ExternalMutating,
                false,
                None,
                Some("unknown".to_string()),
            )
            .unwrap());
        assert!(!ledger
            .record_recovery_event_once(
                "recovery_detected",
                "run-1",
                Some("call-1"),
                "external_unknown_outcome",
                SideEffectClass::ExternalMutating,
                false,
                None,
                Some("same operation after a restart".to_string()),
            )
            .unwrap());

        let facts = ledger.list_facts().unwrap();
        assert_eq!(
            facts
                .iter()
                .filter(|fact| matches!(fact, RuntimeFact::RecoveryDetected { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn effect_aware_recovery_rejects_blind_replay_of_external_mutations() {
        let temp = TempDir::new().unwrap();
        let ledger_file = temp.path().join("run-ext.ledger.jsonl");
        let ledger = WorkRuntimeLedger::for_path(ledger_file);

        // Scenario 1: External tool proposed but never started -> safe to replay
        let action1 =
            ledger.classify_recovery("call-ext-1", SideEffectClass::ExternalMutating, &[], None);
        assert_eq!(action1, RecoveryAction::SafeToReplay);

        // Scenario 2: External tool started, but no ToolResult logged -> UnknownOutcomeNeedsAttention
        ledger
            .record(&RuntimeFact::ToolStarted {
                tool_call_id: "call-ext-2".to_string(),
                execution_id: "exec-99".to_string(),
                timestamp: "2026-08-14T00:00:02Z".to_string(),
            })
            .unwrap();

        let action2 =
            ledger.classify_recovery("call-ext-2", SideEffectClass::ExternalMutating, &[], None);
        assert!(matches!(
            action2,
            RecoveryAction::UnknownOutcomeNeedsAttention { .. }
        ));

        // Scenario 3: Result recorded -> AlreadyCompleted
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "call-ext-2".to_string(),
                success: true,
                status: "success".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec![],
                side_effect_class: SideEffectClass::ExternalMutating,
                timestamp: "2026-08-14T00:00:03Z".to_string(),
            })
            .unwrap();

        let action3 =
            ledger.classify_recovery("call-ext-2", SideEffectClass::ExternalMutating, &[], None);
        assert_eq!(action3, RecoveryAction::AlreadyCompleted);
    }

    #[test]
    fn malformed_ledger_fails_closed_for_reads_and_recovery() {
        let temp = TempDir::new().unwrap();
        let ledger_path = temp.path().join("run-corrupt.ledger.jsonl");
        std::fs::write(&ledger_path, "{not-json}\n").unwrap();
        let ledger = WorkRuntimeLedger::for_path(ledger_path);

        let error = ledger.list_facts().unwrap_err();
        assert!(error.contains("Invalid runtime ledger"));
        assert!(error.contains("line 1"));
        assert!(matches!(
            ledger.classify_recovery("call-corrupt", SideEffectClass::ExternalMutating, &[], None,),
            RecoveryAction::UnknownOutcomeNeedsAttention { .. }
        ));
    }

    #[test]
    fn guardian_events_are_recorded_idempotently() {
        let temp = TempDir::new().unwrap();
        let ledger = WorkRuntimeLedger::for_path(temp.path().join("run-guardian.ledger.jsonl"));

        let recorded1 = ledger
            .record_guardian_event_once(
                crate::work::models::GuardianAnomalyKind::ToolFailureStreak,
                Some("step-1"),
                Some("call-fail-3"),
                "3 consecutive failures",
                Some("3"),
                Some("Check credentials"),
            )
            .unwrap();
        assert!(recorded1);

        let recorded2 = ledger
            .record_guardian_event_once(
                crate::work::models::GuardianAnomalyKind::ToolFailureStreak,
                Some("step-1"),
                Some("call-fail-3"),
                "3 consecutive failures",
                Some("3"),
                Some("Check credentials"),
            )
            .unwrap();
        assert!(!recorded2, "Duplicate Guardian fact must not be recorded");

        let facts = ledger.list_facts().unwrap();
        assert_eq!(facts.len(), 1);
        assert!(matches!(
            facts[0],
            RuntimeFact::GuardianAnomalyDetected { .. }
        ));
    }

    #[test]
    fn guardian_facts_are_idempotent_across_ledger_handles() {
        let temp = TempDir::new().unwrap();
        let ledger_path = temp.path().join("run-guardian-health.ledger.jsonl");
        let first = WorkRuntimeLedger::for_path(ledger_path.clone());
        let second = WorkRuntimeLedger::for_path(ledger_path);
        let fact = RuntimeFact::RunHealthChanged {
            previous: crate::work::models::RunHealth::Healthy,
            current: crate::work::models::RunHealth::Stalled,
            reason: "no progress".to_string(),
            timestamp: "2026-08-29T00:00:00Z".to_string(),
        };

        assert!(first.record_guardian_fact_once(&fact).unwrap());

        let duplicate_with_new_timestamp = RuntimeFact::RunHealthChanged {
            previous: crate::work::models::RunHealth::Healthy,
            current: crate::work::models::RunHealth::Stalled,
            reason: "no progress".to_string(),
            timestamp: "2026-08-29T00:00:15Z".to_string(),
        };
        assert!(!second
            .record_guardian_fact_once(&duplicate_with_new_timestamp)
            .unwrap());
        assert_eq!(second.list_facts().unwrap().len(), 1);
    }

    #[test]
    fn repeated_health_transition_after_recovery_is_not_dropped() {
        let temp = TempDir::new().unwrap();
        let ledger = WorkRuntimeLedger::for_path(temp.path().join("run-health-cycle.ledger.jsonl"));

        let stalled = RuntimeFact::RunHealthChanged {
            previous: crate::work::models::RunHealth::Healthy,
            current: crate::work::models::RunHealth::Stalled,
            reason: "no progress".to_string(),
            timestamp: "2026-08-29T00:00:00Z".to_string(),
        };
        let recovered = RuntimeFact::RunHealthChanged {
            previous: crate::work::models::RunHealth::Stalled,
            current: crate::work::models::RunHealth::Healthy,
            reason: "progress resumed".to_string(),
            timestamp: "2026-08-29T00:01:00Z".to_string(),
        };
        let stalled_again = RuntimeFact::RunHealthChanged {
            previous: crate::work::models::RunHealth::Healthy,
            current: crate::work::models::RunHealth::Stalled,
            reason: "no progress".to_string(),
            timestamp: "2026-08-29T00:02:00Z".to_string(),
        };

        assert!(ledger.record_guardian_fact_once(&stalled).unwrap());
        assert!(ledger.record_guardian_fact_once(&recovered).unwrap());
        assert!(ledger.record_guardian_fact_once(&stalled_again).unwrap());
        assert_eq!(
            ledger
                .list_facts()
                .unwrap()
                .iter()
                .filter(|fact| matches!(fact, RuntimeFact::RunHealthChanged { .. }))
                .count(),
            3
        );
    }
}
