//! Subagent authority tracking and lifecycle ledger for AgentCabin Work Mode.
//!
//! Responsibility Boundary:
//! - Subagent orchestration and child process execution are owned by `pi-subagents`.
//! - AgentCabin Work Harness owns: authority, role allowlist, active tracking,
//!   launch digest verification, WorkRun association, and durable Ledger facts.
//!
//! Absolute Rules:
//! - Never spawn child Pi directly from this module.
//! - Never manage child transcripts or session format.
//! - Child Pi sessions do NOT create new Product WorkRuns.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::work::ledger::WorkRuntimeLedger;
use crate::work::models::RuntimeFact;
use crate::work::paths::WorkPaths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkSubagentRecord {
    pub agent_id: String,
    pub provider_run_id: String,
    pub child_index: u32,
    pub role: String,
    pub parent_scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub launch_contract_digest: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSpawnRequest {
    pub agent_id: String,
    pub provider_run_id: String,
    pub child_index: u32,
    pub role: String,
    pub task_digest: String,
    pub launch_contract_digest: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusRequest {
    pub agent_id: String,
    pub status: String, // "completed", "failed", "stopped", "interrupted"
    pub summary: Option<String>,
    pub error: Option<String>,
    pub reason: Option<String>,
}

pub struct SubagentRegistry {
    records: RwLock<HashMap<(String, String), WorkSubagentRecord>>,
}

enum DurableChildState {
    Pending {
        provider_run_id: String,
        child_index: u32,
        role: String,
    },
    Interrupted,
    Terminal,
}

impl Default for SubagentRegistry {
    fn default() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }
}

static SUBAGENT_REGISTRY: OnceLock<Arc<SubagentRegistry>> = OnceLock::new();

pub fn registry() -> &'static Arc<SubagentRegistry> {
    SUBAGENT_REGISTRY.get_or_init(|| Arc::new(SubagentRegistry::default()))
}

impl SubagentRegistry {
    pub fn register_spawn(
        &self,
        paths: &WorkPaths,
        parent_scope: &str,
        workspace_id: Option<&str>,
        task_id: Option<&str>,
        request: RegisterSpawnRequest,
    ) -> Result<WorkSubagentRecord, String> {
        let now = Utc::now().to_rfc3339();
        let record = WorkSubagentRecord {
            agent_id: request.agent_id.clone(),
            provider_run_id: request.provider_run_id.clone(),
            child_index: request.child_index,
            role: request.role.clone(),
            parent_scope: parent_scope.to_string(),
            workspace_id: workspace_id.map(String::from),
            task_id: task_id.map(String::from),
            launch_contract_digest: request.launch_contract_digest.clone(),
            status: "running".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            result_summary: None,
            error: None,
        };

        let mut lock = self.records.write().map_err(|e| e.to_string())?;
        let key = (parent_scope.to_string(), request.agent_id.clone());
        if let Some(existing) = lock.get(&key) {
            if !matches!(
                existing.status.as_str(),
                "completed" | "failed" | "stopped" | "interrupted" | "cancelled"
            ) {
                return Err(format!(
                    "Subagent '{}' is already registered for parent scope '{}'",
                    request.agent_id, parent_scope
                ));
            }
        }

        // A Work child is not live authority until its durable spawn fact has
        // been recorded. Do not let the in-memory registry get ahead of the
        // ledger: parent completion must remain recoverable after a restart.
        // Hold the registry write lock through this check/write/insert so a
        // concurrent duplicate cannot append a spawn fact that is rejected
        // only after the durable write.
        if let Some(tid) = task_id {
            let ledger = WorkRuntimeLedger::open(paths, tid, parent_scope)?;
            let fact = RuntimeFact::SubagentSpawned {
                agent_id: request.agent_id.clone(),
                provider_run_id: request.provider_run_id.clone(),
                child_index: request.child_index,
                role: request.role.clone(),
                task_digest: request.task_digest.clone(),
                launch_contract_digest: request.launch_contract_digest.clone(),
                status: "running".to_string(),
                timestamp: now.clone(),
            };
            ledger.record(&fact)?;
        }

        lock.insert(key, record.clone());
        Ok(record)
    }

    pub fn update_status(
        &self,
        paths: &WorkPaths,
        parent_scope: &str,
        task_id: Option<&str>,
        request: UpdateStatusRequest,
    ) -> Result<WorkSubagentRecord, String> {
        let now = Utc::now().to_rfc3339();
        let mut lock = self.records.write().map_err(|e| e.to_string())?;
        let key = (parent_scope.to_string(), request.agent_id.clone());
        let record = lock.get_mut(&key).ok_or_else(|| {
            format!(
                "Subagent '{}' not found for parent scope '{}'",
                request.agent_id, parent_scope
            )
        })?;

        if matches!(
            record.status.as_str(),
            "completed" | "failed" | "stopped" | "interrupted"
        ) {
            if record.status == request.status {
                return Ok(record.clone());
            }
            return Err(format!(
                "Subagent '{}' is already terminal with status '{}'",
                request.agent_id, record.status
            ));
        }

        // Build the new record separately and only publish it after the
        // durable terminal fact succeeds. This prevents a failed ledger write
        // from making the in-memory parent gate believe that a child ended.
        let mut updated = record.clone();
        updated.status = request.status.clone();
        updated.updated_at = now.clone();
        if let Some(summary) = &request.summary {
            updated.result_summary = Some(summary.clone());
        }
        if let Some(err) = &request.error {
            updated.error = Some(err.clone());
        }

        // Record terminal fact in Work ledger if task_id is present
        if let Some(tid) = task_id {
            let ledger = WorkRuntimeLedger::open(paths, tid, parent_scope)?;
            let fact = match request.status.as_str() {
                "completed" => RuntimeFact::SubagentCompleted {
                    agent_id: updated.agent_id.clone(),
                    provider_run_id: updated.provider_run_id.clone(),
                    child_index: updated.child_index,
                    role: updated.role.clone(),
                    status: "completed".to_string(),
                    summary: request.summary.clone(),
                    timestamp: now.clone(),
                },
                "failed" => RuntimeFact::SubagentFailed {
                    agent_id: updated.agent_id.clone(),
                    provider_run_id: updated.provider_run_id.clone(),
                    child_index: updated.child_index,
                    role: updated.role.clone(),
                    status: "failed".to_string(),
                    error: request.error.clone(),
                    timestamp: now.clone(),
                },
                "stopped" => RuntimeFact::SubagentStopped {
                    agent_id: updated.agent_id.clone(),
                    provider_run_id: updated.provider_run_id.clone(),
                    child_index: updated.child_index,
                    role: updated.role.clone(),
                    status: "stopped".to_string(),
                    reason: request.reason.clone(),
                    timestamp: now.clone(),
                },
                _ => RuntimeFact::SubagentInterrupted {
                    agent_id: updated.agent_id.clone(),
                    provider_run_id: updated.provider_run_id.clone(),
                    child_index: updated.child_index,
                    role: updated.role.clone(),
                    status: request.status.clone(),
                    reason: request.reason.clone(),
                    timestamp: now.clone(),
                },
            };
            ledger.record(&fact)?;
        }

        *record = updated.clone();
        drop(lock);

        if matches!(
            request.status.as_str(),
            "completed" | "failed" | "stopped" | "interrupted"
        ) {
            crate::work::internal_bridge::revoke_subagent_tokens_sync(
                parent_scope,
                &updated.agent_id,
            );
            crate::work::internal_bridge::revoke_subagent_tokens_sync(
                parent_scope,
                &updated.provider_run_id,
            );
        }

        Ok(updated)
    }

    pub fn find_record(
        &self,
        parent_scope: &str,
        agent_id_or_provider_run_id: &str,
    ) -> Option<WorkSubagentRecord> {
        let lock = match self.records.read() {
            Ok(guard) => guard,
            Err(_) => return None,
        };
        lock.values()
            .find(|r| {
                r.parent_scope == parent_scope
                    && (r.agent_id == agent_id_or_provider_run_id
                        || r.provider_run_id == agent_id_or_provider_run_id)
            })
            .cloned()
    }

    pub fn list_for_scope(&self, parent_scope: &str) -> Vec<WorkSubagentRecord> {
        let lock = match self.records.read() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        let mut list: Vec<WorkSubagentRecord> = lock
            .values()
            .filter(|r| r.parent_scope == parent_scope)
            .cloned()
            .collect();
        list.sort_by_key(|a| a.child_index);
        list
    }

    pub fn has_active_children(&self, parent_scope: &str) -> bool {
        let lock = match self.records.read() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        lock.values().any(|r| {
            r.parent_scope == parent_scope
                && (r.status == "running" || r.status == "pending" || r.status == "queued")
        })
    }

    pub fn mark_all_terminal_for_task(
        &self,
        task_id: &str,
        terminal_status: &str,
        reason: Option<String>,
    ) {
        let mut lock = match self.records.write() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let now = Utc::now().to_rfc3339();
        for record in lock.values_mut() {
            if record.task_id.as_deref() == Some(task_id)
                && !matches!(
                    record.status.as_str(),
                    "completed" | "failed" | "stopped" | "interrupted" | "cancelled"
                )
            {
                record.status = terminal_status.to_string();
                record.updated_at = now.clone();
                record.error = reason.clone();
                crate::work::internal_bridge::revoke_subagent_tokens_sync(
                    &record.parent_scope,
                    &record.agent_id,
                );
                crate::work::internal_bridge::revoke_subagent_tokens_sync(
                    &record.parent_scope,
                    &record.provider_run_id,
                );
            }
        }
    }

    pub fn interrupt_all_for_scope(
        &self,
        paths: &WorkPaths,
        parent_scope: &str,
        task_id: Option<&str>,
        terminal_status: &str,
        reason: Option<String>,
    ) -> Result<(), String> {
        let mut lock = self.records.write().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        let mut to_record = Vec::new();

        for record in lock.values_mut() {
            if record.parent_scope == parent_scope
                && !matches!(
                    record.status.as_str(),
                    "completed" | "failed" | "stopped" | "interrupted" | "cancelled"
                )
            {
                to_record.push((
                    record.agent_id.clone(),
                    record.provider_run_id.clone(),
                    record.child_index,
                    record.role.clone(),
                ));
            }
        }
        if let Some(tid) = task_id {
            if !to_record.is_empty() {
                let ledger = WorkRuntimeLedger::open(paths, tid, parent_scope)?;
                for (agent_id, provider_run_id, child_index, role) in to_record {
                    let fact = match terminal_status {
                        "stopped" => RuntimeFact::SubagentStopped {
                            agent_id,
                            provider_run_id,
                            child_index,
                            role,
                            status: "stopped".to_string(),
                            reason: reason.clone(),
                            timestamp: now.clone(),
                        },
                        _ => RuntimeFact::SubagentInterrupted {
                            agent_id,
                            provider_run_id,
                            child_index,
                            role,
                            status: terminal_status.to_string(),
                            reason: reason.clone(),
                            timestamp: now.clone(),
                        },
                    };
                    ledger.record(&fact)?;
                }
            }
        }
        // Publish only after all durable writes succeed. Holding the registry lock
        // prevents a concurrent child completion from overwriting this snapshot.
        for record in lock.values_mut().filter(|r| r.parent_scope == parent_scope) {
            if matches!(
                record.status.as_str(),
                "completed" | "failed" | "stopped" | "interrupted" | "cancelled"
            ) {
                continue;
            }
            record.status = terminal_status.to_string();
            record.updated_at = now.clone();
            record.error = reason.clone();
            crate::work::internal_bridge::revoke_subagent_tokens_sync(
                parent_scope,
                &record.agent_id,
            );
            crate::work::internal_bridge::revoke_subagent_tokens_sync(
                parent_scope,
                &record.provider_run_id,
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn reset_for_test(&self) {
        if let Ok(mut lock) = self.records.write() {
            lock.clear();
        }
    }
}

/// Recover durable child lifecycle truth after an application restart. The
/// in-memory registry is intentionally not replayed; unfinished children are
/// marked interrupted so they cannot be mistaken for live authority or auto-resumed.
pub fn reconcile_unfinished_from_ledger(
    paths: &WorkPaths,
    task_id: &str,
    parent_scope: &str,
) -> Result<Vec<String>, String> {
    let ledger = WorkRuntimeLedger::open(paths, task_id, parent_scope)?;
    let facts = ledger.list_facts()?;
    // Track the latest lifecycle generation for each agent. A retry may reuse
    // the same agent id after an Interrupted fact; an old terminal fact must
    // not hide the newer Spawned fact from reconciliation.
    let mut latest: HashMap<String, DurableChildState> = HashMap::new();
    for fact in facts {
        match fact {
            RuntimeFact::SubagentSpawned {
                agent_id,
                provider_run_id,
                child_index,
                role,
                ..
            } => {
                latest.insert(
                    agent_id,
                    DurableChildState::Pending {
                        provider_run_id,
                        child_index,
                        role,
                    },
                );
            }
            RuntimeFact::SubagentCompleted { agent_id, .. }
            | RuntimeFact::SubagentFailed { agent_id, .. }
            | RuntimeFact::SubagentStopped { agent_id, .. } => {
                if let Some(entry) = latest.get_mut(&agent_id) {
                    *entry = DurableChildState::Terminal;
                }
            }
            RuntimeFact::SubagentInterrupted { agent_id, .. } => {
                if let Some(entry) = latest.get_mut(&agent_id) {
                    *entry = DurableChildState::Interrupted;
                }
            }
            _ => {}
        }
    }

    let mut interrupted = Vec::new();
    for (agent_id, state) in latest {
        if let DurableChildState::Pending {
            provider_run_id,
            child_index,
            role,
        } = state
        {
            ledger.record(&RuntimeFact::SubagentInterrupted {
                agent_id: agent_id.clone(),
                provider_run_id,
                child_index,
                role,
                status: "interrupted".to_string(),
                reason: Some(
                    "Application restarted before child terminal fact was recorded".to_string(),
                ),
                timestamp: Utc::now().to_rfc3339(),
            })?;
            interrupted.push(agent_id);
        }
    }
    Ok(interrupted)
}

/// Read the durable set of interrupted children without mutating the ledger.
/// This is separate from startup reconciliation because a read-only recovery
/// projection must not create new facts merely by being opened.
pub fn interrupted_child_ids_from_ledger(
    paths: &WorkPaths,
    task_id: &str,
    parent_scope: &str,
) -> Result<Vec<String>, String> {
    let ledger = WorkRuntimeLedger::open(paths, task_id, parent_scope)?;
    let facts = ledger.list_facts()?;
    // As with startup reconciliation, interpret facts by latest generation so
    // an explicit retry can reuse the same agent id safely.
    let mut latest: HashMap<String, DurableChildState> = HashMap::new();
    for fact in facts {
        match fact {
            RuntimeFact::SubagentSpawned {
                agent_id,
                provider_run_id,
                child_index,
                role,
                ..
            } => {
                latest.insert(
                    agent_id,
                    DurableChildState::Pending {
                        provider_run_id,
                        child_index,
                        role,
                    },
                );
            }
            RuntimeFact::SubagentInterrupted { agent_id, .. } => {
                if let Some(state) = latest.get_mut(&agent_id) {
                    *state = DurableChildState::Interrupted;
                }
            }
            RuntimeFact::SubagentCompleted { agent_id, .. }
            | RuntimeFact::SubagentFailed { agent_id, .. }
            | RuntimeFact::SubagentStopped { agent_id, .. } => {
                if let Some(state) = latest.get_mut(&agent_id) {
                    *state = DurableChildState::Terminal;
                }
            }
            _ => {}
        }
    }
    let mut result = latest
        .into_iter()
        .filter_map(|(agent_id, state)| {
            matches!(
                state,
                DurableChildState::Pending { .. } | DurableChildState::Interrupted
            )
            .then_some(agent_id)
        })
        .collect::<Vec<_>>();
    result.sort();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn host_records_match_frontend_wire_fixture() {
        let records: Vec<_> = (0..3)
            .map(|i| WorkSubagentRecord {
                agent_id: format!("agent-{i}"),
                provider_run_id: format!("provider-{i}"),
                child_index: i,
                role: "researcher".into(),
                parent_scope: "parent-run".into(),
                workspace_id: None,
                task_id: None,
                launch_contract_digest: "digest".into(),
                status: "running".into(),
                created_at: "2026-09-05T05:43:54Z".into(),
                updated_at: "2026-09-05T05:43:54Z".into(),
                result_summary: None,
                error: None,
            })
            .collect();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/lib/utils/__fixtures__/work-subagents.json"
        ))
        .unwrap();
        assert_eq!(serde_json::to_value(records).unwrap(), fixture);
    }

    #[test]
    fn registers_and_tracks_subagents() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();

        let reg = SubagentRegistry::default();
        let parent_scope = "run-123";

        assert!(!reg.has_active_children(parent_scope));

        let spawned = reg
            .register_spawn(
                &paths,
                parent_scope,
                Some("ws-1"),
                Some("task-1"),
                RegisterSpawnRequest {
                    agent_id: "agent-1".into(),
                    provider_run_id: "prun-1".into(),
                    child_index: 0,
                    role: "researcher".into(),
                    task_digest: "digest-task".into(),
                    launch_contract_digest: "digest-contract".into(),
                },
            )
            .unwrap();

        assert_eq!(spawned.status, "running");
        assert!(reg.has_active_children(parent_scope));
        assert_eq!(reg.list_for_scope(parent_scope).len(), 1);

        let completed = reg
            .update_status(
                &paths,
                parent_scope,
                Some("task-1"),
                UpdateStatusRequest {
                    agent_id: "agent-1".into(),
                    status: "completed".into(),
                    summary: Some("Done research".into()),
                    error: None,
                    reason: None,
                },
            )
            .unwrap();

        assert_eq!(completed.status, "completed");
        assert_eq!(completed.result_summary, Some("Done research".into()));
        assert!(!reg.has_active_children(parent_scope));
    }

    #[test]
    fn failed_spawn_fact_does_not_register_in_memory_child() {
        let temp = TempDir::new().unwrap();
        let blocked_root = temp.path().join("blocked-root");
        std::fs::write(&blocked_root, b"not a directory").unwrap();
        let paths = WorkPaths::new(blocked_root);
        let reg = SubagentRegistry::default();

        let result = reg.register_spawn(
            &paths,
            "run-failed-spawn",
            None,
            Some("task-failed-spawn"),
            RegisterSpawnRequest {
                agent_id: "agent-failed-spawn".into(),
                provider_run_id: "provider-failed-spawn".into(),
                child_index: 0,
                role: "worker".into(),
                task_digest: "task".into(),
                launch_contract_digest: "contract".into(),
            },
        );

        assert!(result.is_err());
        assert!(reg
            .find_record("run-failed-spawn", "agent-failed-spawn")
            .is_none());
        assert!(!reg.has_active_children("run-failed-spawn"));
    }

    #[test]
    fn failed_terminal_fact_does_not_publish_in_memory_status() {
        let temp = TempDir::new().unwrap();
        let valid_paths = WorkPaths::new(temp.path().join("valid"));
        valid_paths.ensure_layout().unwrap();
        let blocked_root = temp.path().join("blocked-root");
        std::fs::write(&blocked_root, b"not a directory").unwrap();
        let blocked_paths = WorkPaths::new(blocked_root);
        let reg = SubagentRegistry::default();

        reg.register_spawn(
            &valid_paths,
            "run-failed-update",
            None,
            None,
            RegisterSpawnRequest {
                agent_id: "agent-failed-update".into(),
                provider_run_id: "provider-failed-update".into(),
                child_index: 0,
                role: "worker".into(),
                task_digest: "task".into(),
                launch_contract_digest: "contract".into(),
            },
        )
        .unwrap();

        let result = reg.update_status(
            &blocked_paths,
            "run-failed-update",
            Some("task-failed-update"),
            UpdateStatusRequest {
                agent_id: "agent-failed-update".into(),
                status: "completed".into(),
                summary: Some("should not publish".into()),
                error: None,
                reason: None,
            },
        );

        assert!(result.is_err());
        let current = reg
            .find_record("run-failed-update", "agent-failed-update")
            .unwrap();
        assert!(reg
            .interrupt_all_for_scope(
                &blocked_paths,
                "run-failed-update",
                Some("task-failed-update"),
                "interrupted",
                Some("parent failed".into()),
            )
            .is_err());
        assert!(reg.has_active_children("run-failed-update"));
        assert_eq!(current.status, "running");
        assert!(reg.has_active_children("run-failed-update"));
    }

    #[test]
    fn scopes_are_part_of_subagent_identity_and_terminal_updates_are_idempotent() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        let reg = SubagentRegistry::default();
        let request = || RegisterSpawnRequest {
            agent_id: "same-agent-id".into(),
            provider_run_id: "provider-run".into(),
            child_index: 0,
            role: "researcher".into(),
            task_digest: "task".into(),
            launch_contract_digest: "contract".into(),
        };

        reg.register_spawn(&paths, "parent-a", None, None, request())
            .unwrap();
        reg.register_spawn(&paths, "parent-b", None, None, request())
            .unwrap();

        let foreign_update = reg.update_status(
            &paths,
            "parent-c",
            None,
            UpdateStatusRequest {
                agent_id: "same-agent-id".into(),
                status: "completed".into(),
                summary: None,
                error: None,
                reason: None,
            },
        );
        assert!(foreign_update.is_err());

        let update = || UpdateStatusRequest {
            agent_id: "same-agent-id".into(),
            status: "completed".into(),
            summary: Some("done".into()),
            error: None,
            reason: None,
        };
        reg.update_status(&paths, "parent-a", None, update())
            .unwrap();
        let repeated = reg
            .update_status(&paths, "parent-a", None, update())
            .unwrap();
        assert_eq!(repeated.status, "completed");
        assert!(!reg.has_active_children("parent-a"));
        assert!(reg.has_active_children("parent-b"));
    }

    #[test]
    fn restart_reconciles_spawn_without_terminal_fact_as_interrupted() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        let reg = SubagentRegistry::default();
        reg.register_spawn(
            &paths,
            "run-restart",
            None,
            Some("task-restart"),
            RegisterSpawnRequest {
                agent_id: "agent-restart".into(),
                provider_run_id: "provider-restart".into(),
                child_index: 0,
                role: "worker".into(),
                task_digest: "task".into(),
                launch_contract_digest: "contract".into(),
            },
        )
        .unwrap();

        let interrupted =
            reconcile_unfinished_from_ledger(&paths, "task-restart", "run-restart").unwrap();
        assert_eq!(interrupted, vec!["agent-restart"]);
        let facts = WorkRuntimeLedger::open(&paths, "task-restart", "run-restart")
            .unwrap()
            .list_facts()
            .unwrap();
        assert!(facts.iter().any(|fact| matches!(
            fact,
            RuntimeFact::SubagentInterrupted { agent_id, .. } if agent_id == "agent-restart"
        )));
        assert!(
            reconcile_unfinished_from_ledger(&paths, "task-restart", "run-restart")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn retry_generation_reopens_the_same_agent_id_after_interruption() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();
        let ledger = WorkRuntimeLedger::open(&paths, "task-generation", "run-generation").unwrap();

        let spawn = |provider_run_id: &str| RuntimeFact::SubagentSpawned {
            agent_id: "agent-generation".to_string(),
            provider_run_id: provider_run_id.to_string(),
            child_index: 0,
            role: "worker".to_string(),
            task_digest: "task".to_string(),
            launch_contract_digest: "contract".to_string(),
            status: "running".to_string(),
            timestamp: provider_run_id.to_string(),
        };
        ledger.record(&spawn("provider-1")).unwrap();
        ledger
            .record(&RuntimeFact::SubagentInterrupted {
                agent_id: "agent-generation".to_string(),
                provider_run_id: "provider-1".to_string(),
                child_index: 0,
                role: "worker".to_string(),
                status: "interrupted".to_string(),
                reason: Some("restart".to_string()),
                timestamp: "interrupted".to_string(),
            })
            .unwrap();
        assert_eq!(
            interrupted_child_ids_from_ledger(&paths, "task-generation", "run-generation").unwrap(),
            vec!["agent-generation"]
        );

        ledger.record(&spawn("provider-2")).unwrap();
        assert_eq!(
            interrupted_child_ids_from_ledger(&paths, "task-generation", "run-generation").unwrap(),
            vec!["agent-generation"]
        );
        ledger
            .record(&RuntimeFact::SubagentCompleted {
                agent_id: "agent-generation".to_string(),
                provider_run_id: "provider-2".to_string(),
                child_index: 0,
                role: "worker".to_string(),
                status: "completed".to_string(),
                summary: None,
                timestamp: "completed".to_string(),
            })
            .unwrap();
        assert!(
            interrupted_child_ids_from_ledger(&paths, "task-generation", "run-generation")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn interrupt_all_for_scope_clears_active_children_and_persists_facts() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        paths.ensure_layout().unwrap();

        let reg = SubagentRegistry::default();
        let parent_scope = "run-stop-test";

        reg.register_spawn(
            &paths,
            parent_scope,
            Some("ws-1"),
            Some("task-1"),
            RegisterSpawnRequest {
                agent_id: "agent-1".into(),
                provider_run_id: "prun-1".into(),
                child_index: 0,
                role: "worker".into(),
                task_digest: "digest-task".into(),
                launch_contract_digest: "digest-contract".into(),
            },
        )
        .unwrap();

        assert!(reg.has_active_children(parent_scope));

        // When user stops or provider crashes, interrupt_all_for_scope is invoked
        reg.interrupt_all_for_scope(
            &paths,
            parent_scope,
            Some("task-1"),
            "stopped",
            Some("Session stopped by user".into()),
        )
        .unwrap();

        assert!(!reg.has_active_children(parent_scope));
        let record = reg.find_record(parent_scope, "agent-1").unwrap();
        assert_eq!(record.status, "stopped");

        // Verify durable ledger fact
        let ledger = WorkRuntimeLedger::open(&paths, "task-1", parent_scope).unwrap();
        let facts = ledger.list_facts().unwrap();
        assert!(facts.iter().any(
            |f| matches!(f, RuntimeFact::SubagentStopped { agent_id, .. } if agent_id == "agent-1")
        ));
    }
}
