use chrono::Utc;
use once_cell::sync::Lazy;
use std::fs;
use std::io::Write;
use std::sync::Mutex;
use uuid::Uuid;

use crate::work::models::{InboxItem, InboxItemPayload, InboxItemStatus, InboxItemType};
use crate::work::paths::{validate_workspace_id, WorkPaths};

static INBOX_RESOLVE_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone)]
pub struct InboxManager {
    paths: WorkPaths,
}

impl InboxManager {
    pub fn new(paths: WorkPaths) -> Self {
        Self { paths }
    }

    /// Create and persist a new pending Inbox item.
    #[allow(clippy::too_many_arguments)]
    pub fn create_item(
        &self,
        task_id: &str,
        run_id: &str,
        workspace_id: &str,
        item_type: InboxItemType,
        title: &str,
        description: &str,
        payload: InboxItemPayload,
    ) -> Result<InboxItem, String> {
        self.paths.ensure_layout()?;
        validate_workspace_id(task_id)?;
        if !workspace_id.trim().is_empty() {
            validate_workspace_id(workspace_id)?;
        }

        let id = format!("inbox-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let item = InboxItem {
            id: id.clone(),
            task_id: task_id.to_string(),
            run_id: run_id.to_string(),
            workspace_id: workspace_id.to_string(),
            item_type,
            status: InboxItemStatus::Pending,
            title: title.to_string(),
            description: description.to_string(),
            payload,
            response: None,
            created_at: now,
            resolved_at: None,
        };

        self.save_item(&item)?;
        Ok(item)
    }

    /// Get a single Inbox item by ID.
    pub fn get_item(&self, id: &str) -> Result<InboxItem, String> {
        let path = self.paths.inbox_item_path(id)?;
        if !path.exists() {
            return Err(format!("Inbox item not found: {id}"));
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read inbox item {id}: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse inbox item {id}: {e}"))
    }

    /// List all Inbox items (optionally filtered by pending status or task_id).
    pub fn list_items(
        &self,
        only_pending: bool,
        task_id_filter: Option<&str>,
    ) -> Result<Vec<InboxItem>, String> {
        self.paths.ensure_layout()?;
        let inbox_dir = self.paths.inbox_dir();
        let entries = fs::read_dir(&inbox_dir).map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(item) = serde_json::from_str::<InboxItem>(&content) else {
                continue;
            };

            if only_pending && item.status != InboxItemStatus::Pending {
                continue;
            }

            if let Some(task_id) = task_id_filter {
                if item.task_id != task_id {
                    continue;
                }
            }

            items.push(item);
        }

        // Sort newest first
        items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(items)
    }

    /// Resolve an inbox item (First-Responder-Wins).
    ///
    /// Repeated resolution attempts are idempotent: they return the already
    /// resolved item and never rewrite the response.  The caller can therefore
    /// safely retry after a reconnect without executing an approval twice.
    pub fn resolve_item_once(
        &self,
        id: &str,
        new_status: InboxItemStatus,
        response_payload: Option<serde_json::Value>,
    ) -> Result<(InboxItem, bool), String> {
        let _guard = INBOX_RESOLVE_MUTEX.lock().map_err(|e| e.to_string())?;

        if new_status == InboxItemStatus::Pending {
            return Err("Cannot resolve an item back to pending".to_string());
        }

        let mut item = self.get_item(id)?;
        if item.status != InboxItemStatus::Pending {
            return Ok((item, false));
        }

        item.status = new_status;
        item.response = response_payload;
        item.resolved_at = Some(Utc::now().to_rfc3339());

        self.save_item(&item)?;
        Ok((item, true))
    }

    /// Resolve an inbox item. Kept as the simple API used by existing callers.
    pub fn resolve_item(
        &self,
        id: &str,
        new_status: InboxItemStatus,
        response_payload: Option<serde_json::Value>,
    ) -> Result<InboxItem, String> {
        self.resolve_item_once(id, new_status, response_payload)
            .map(|(item, _)| item)
    }

    /// Delete an inbox item file from disk.
    pub fn delete_item(&self, id: &str) -> Result<(), String> {
        let path = self.paths.inbox_item_path(id)?;
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Failed to delete inbox item {id}: {e}"))?;
        }
        Ok(())
    }

    /// Clear all inbox items (or filtered by status / workspace).
    pub fn clear_items(
        &self,
        workspace_id: Option<&str>,
        only_resolved: bool,
    ) -> Result<usize, String> {
        let items = self.list_items(false, None)?;
        let mut count = 0;
        for item in items {
            if let Some(ws_id) = workspace_id {
                if !ws_id.trim().is_empty() && item.workspace_id != ws_id {
                    continue;
                }
            }
            if only_resolved && item.status == InboxItemStatus::Pending {
                continue;
            }
            if self.delete_item(&item.id).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }

    fn save_item(&self, item: &InboxItem) -> Result<(), String> {
        let path = self.paths.inbox_item_path(&item.id)?;
        let bytes = serde_json::to_vec_pretty(item)
            .map_err(|e| format!("Failed to serialize inbox item: {e}"))?;

        let parent = path
            .parent()
            .ok_or_else(|| "Missing parent directory".to_string())?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;

        let temp_path = parent.join(format!(".{}.tmp", Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|e| e.to_string())?;

        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;

        fs::rename(&temp_path, &path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn creates_and_resolves_inbox_item_with_first_responder_wins() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = InboxManager::new(paths);

        let item = manager
            .create_item(
                "task-1",
                "run-1",
                "ws-1",
                InboxItemType::PermissionRequest,
                "Approve Slack Send",
                "Send summary to #general",
                InboxItemPayload {
                    tool_name: Some("slack_send".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(item.status, InboxItemStatus::Pending);

        // List pending
        let pending = manager.list_items(true, None).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, item.id);

        // First responder approves
        let resolved = manager
            .resolve_item(
                &item.id,
                InboxItemStatus::Approved,
                Some(serde_json::json!({"allow": true})),
            )
            .unwrap();
        assert_eq!(resolved.status, InboxItemStatus::Approved);
        assert!(resolved.resolved_at.is_some());

        // Second responder attempt is idempotent: returns (already_resolved_item, false)
        let (second_item, is_first) = manager
            .resolve_item_once(&item.id, InboxItemStatus::Rejected, None)
            .unwrap();
        assert!(!is_first);
        assert_eq!(second_item.status, InboxItemStatus::Approved);
    }
}
