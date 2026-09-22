//! First-class Work runtime input semantics (Steer, FollowUp, Inject).
//!
//! Exposes three distinct interaction concepts:
//! - Steer: modify the nearest next step of an active WorkRun without creating a new WorkRun.
//! - FollowUp: queue a new user instruction for a later turn.
//! - Inject: add model-facing runtime context to the next step WITHOUT waking an idle agent.

use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

use crate::work::models::{RuntimeInputItem, RuntimeInputKind, RuntimeInputState};
use crate::work::paths::WorkPaths;

pub struct RuntimeInputManager {
    inputs_path: PathBuf,
    lock: Mutex<()>,
}

impl RuntimeInputManager {
    pub fn open(paths: &WorkPaths, task_id: &str, run_id: &str) -> Result<Self, String> {
        let run_dir = paths.task_runs_dir(task_id)?;
        fs::create_dir_all(&run_dir).map_err(|e| e.to_string())?;
        let inputs_path = run_dir.join(format!("{run_id}.inputs.jsonl"));
        Ok(Self {
            inputs_path,
            lock: Mutex::new(()),
        })
    }

    pub fn for_path(inputs_path: PathBuf) -> Self {
        if let Some(parent) = inputs_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Self {
            inputs_path,
            lock: Mutex::new(()),
        }
    }

    /// Queue a new input item.
    pub fn queue_input(
        &self,
        task_id: &str,
        work_run_id: &str,
        kind: RuntimeInputKind,
        content: &str,
    ) -> Result<RuntimeInputItem, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let id = format!("input-{}", Uuid::new_v4());
        let item = RuntimeInputItem {
            id,
            task_id: task_id.to_string(),
            work_run_id: work_run_id.to_string(),
            kind,
            state: RuntimeInputState::Queued,
            content: content.to_string(),
            created_at: Utc::now().to_rfc3339(),
            claimed_at: None,
            cancelled_at: None,
        };

        let json_line = serde_json::to_string(&item).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.inputs_path)
            .map_err(|e| e.to_string())?;

        writeln!(file, "{json_line}").map_err(|e| e.to_string())?;
        file.sync_data().map_err(|e| e.to_string())?;
        Ok(item)
    }

    /// List all input items.
    pub fn list_inputs(&self) -> Result<Vec<RuntimeInputItem>, String> {
        if !self.inputs_path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.inputs_path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);

        let mut items = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(item) = serde_json::from_str::<RuntimeInputItem>(trimmed) {
                items.push(item);
            }
        }
        Ok(items)
    }

    /// Claim the next available queued item of a specific kind.
    pub fn claim_next(&self, kind: RuntimeInputKind) -> Result<Option<RuntimeInputItem>, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let mut items = self.list_inputs()?;
        let now = Utc::now().to_rfc3339();

        let mut claimed = None;
        for item in &mut items {
            if item.state == RuntimeInputState::Queued && item.kind == kind {
                item.state = RuntimeInputState::Claimed;
                item.claimed_at = Some(now.clone());
                claimed = Some(item.clone());
                break;
            }
        }

        if claimed.is_some() {
            self.save_all(&items)?;
        }
        Ok(claimed)
    }

    /// Cancel a queued input item.
    pub fn cancel(&self, id: &str, _reason: &str) -> Result<bool, String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        let mut items = self.list_inputs()?;
        let now = Utc::now().to_rfc3339();

        let mut changed = false;
        for item in &mut items {
            if item.id == id && item.state == RuntimeInputState::Queued {
                item.state = RuntimeInputState::Cancelled;
                item.cancelled_at = Some(now.clone());
                changed = true;
                break;
            }
        }

        if changed {
            self.save_all(&items)?;
        }
        Ok(changed)
    }

    fn save_all(&self, items: &[RuntimeInputItem]) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.inputs_path)
            .map_err(|e| e.to_string())?;

        for item in items {
            let json_line = serde_json::to_string(item).map_err(|e| e.to_string())?;
            writeln!(file, "{json_line}").map_err(|e| e.to_string())?;
        }
        file.sync_data().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn queues_claims_and_cancels_runtime_inputs() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.inputs.jsonl");
        let manager = RuntimeInputManager::for_path(path);

        // Queue Steer
        let steer = manager
            .queue_input("t1", "r1", RuntimeInputKind::Steer, "Check data first")
            .unwrap();
        assert_eq!(steer.state, RuntimeInputState::Queued);

        // Queue FollowUp
        let follow_up = manager
            .queue_input(
                "t1",
                "r1",
                RuntimeInputKind::FollowUp,
                "Generate summary after",
            )
            .unwrap();

        // Queue Inject
        let inject = manager
            .queue_input("t1", "r1", RuntimeInputKind::Inject, "file context changed")
            .unwrap();

        // Claim steer
        let claimed_steer = manager.claim_next(RuntimeInputKind::Steer).unwrap();
        assert!(claimed_steer.is_some());
        assert_eq!(claimed_steer.unwrap().id, steer.id);

        // Claim again should be none
        assert!(manager
            .claim_next(RuntimeInputKind::Steer)
            .unwrap()
            .is_none());

        // Cancel follow_up
        assert!(manager.cancel(&follow_up.id, "User retracted").unwrap());

        // Injected context remains queued
        let items = manager.list_inputs().unwrap();
        let found_inject = items.iter().find(|i| i.id == inject.id).unwrap();
        assert_eq!(found_inject.state, RuntimeInputState::Queued);
    }
}
