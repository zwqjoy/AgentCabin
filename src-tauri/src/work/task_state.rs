use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::agent::spawn_locks::SpawnLocks;
use crate::models::BusEvent;
use crate::storage;
use crate::work::models::WorkTaskState;
use once_cell::sync::Lazy;

static TASK_STATE_LOCKS: Lazy<SpawnLocks> = Lazy::new(SpawnLocks::new);

/// Acquire per-run lock for mutating WorkTaskState to serialize read-modify-write.
pub async fn lock(run_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
    TASK_STATE_LOCKS.acquire(run_id).await
}

const STATE_FILE_NAME: &str = "work-task-state.json";

pub fn state_path(run_id: &str) -> PathBuf {
    storage::run_dir(run_id).join(STATE_FILE_NAME)
}

pub fn load(run_id: &str) -> Result<Option<WorkTaskState>, String> {
    load_from_path(&state_path(run_id))
}

pub fn save(run_id: &str, state: &WorkTaskState) -> Result<(), String> {
    let run_dir = storage::run_dir(run_id);
    storage::ensure_dir(&run_dir)
        .map_err(|error| format!("创建 Work Run 状态目录失败: {error}"))?;
    save_to_path(&state_path(run_id), state)
}

pub fn bus_events(run_id: &str, state: &WorkTaskState) -> [BusEvent; 2] {
    [
        BusEvent::WorkTaskState {
            run_id: run_id.to_string(),
            state: state.clone(),
        },
        BusEvent::StructuredTaskState {
            run_id: run_id.to_string(),
            tasks: state.plan.clone(),
        },
    ]
}

/// Rebuild the Run-local state file from the latest durable Work snapshot.
/// Continue calls this after copying the source event prefix, so an earlier
/// continuation anchor never receives state created after that reply.
pub fn restore_from_events(run_id: &str) -> Result<Option<WorkTaskState>, String> {
    let state = storage::events::list_bus_events(run_id, None)
        .into_iter()
        .rev()
        .find(|event| event.get("type").and_then(|value| value.as_str()) == Some("work_task_state"))
        .and_then(|event| event.get("state").cloned())
        .map(serde_json::from_value::<WorkTaskState>)
        .transpose()
        .map_err(|error| format!("读取 Work Task 事件失败: {error}"))?;

    if let Some(state) = state.as_ref() {
        save(run_id, state)?;
    }
    Ok(state)
}

pub fn load_from_path(path: &Path) -> Result<Option<WorkTaskState>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(path).map_err(|error| format!("读取 Work Task 状态失败: {error}"))?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| format!("解析 Work Task 状态失败: {error}"))
}

pub fn save_to_path(path: &Path, state: &WorkTaskState) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Work Task 状态路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建 Work Task 目录失败: {error}"))?;
    let temporary = parent.join(format!(".{STATE_FILE_NAME}.{}.tmp", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("序列化 Work Task 状态失败: {error}"))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("创建 Work Task 临时文件失败: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("写入 Work Task 状态失败: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("设置 Work Task 状态权限失败: {error}"))?;
    }

    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("提交 Work Task 状态失败: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_from_path, save_to_path};
    use crate::models::{StructuredTask, StructuredTaskStatus};
    use crate::work::models::{WorkTaskCheckpoint, WorkTaskState};

    #[test]
    fn atomically_round_trips_work_task_state() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("state.json");
        let state = WorkTaskState {
            version: 1,
            revision: 4,
            goal: Some("完成研究报告".to_string()),
            goal_spec: None,
            plan: vec![StructuredTask {
                id: "research".to_string(),
                text: "检索来源".to_string(),
                status: StructuredTaskStatus::InProgress,
            }],
            checkpoint: Some(WorkTaskCheckpoint {
                summary: "已找到官方来源".to_string(),
                current_step_id: Some("research".to_string()),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            }),
            pending_approval: None,
            updated_at: "2026-08-13T00:00:00Z".to_string(),
        };

        save_to_path(&path, &state).unwrap();
        assert_eq!(load_from_path(&path).unwrap(), Some(state));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
