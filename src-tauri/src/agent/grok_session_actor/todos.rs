use crate::models::{StructuredTask, StructuredTaskStatus};
use serde_json::Value;

/// Parse ACP's `plan` update as a full replacement snapshot.
///
/// A missing/non-array `entries` field or any malformed entry rejects the whole frame so corrupt
/// updates never clear good state. A valid empty array is authoritative and clears stale tasks.
pub(super) fn parse_plan(update: &Value) -> Option<Vec<StructuredTask>> {
    let entries = update.get("entries")?.as_array()?;
    let mut tasks = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let text = entry
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let status = match entry.get("status").and_then(Value::as_str)? {
            "pending" => StructuredTaskStatus::Pending,
            "in_progress" => StructuredTaskStatus::InProgress,
            "completed" => StructuredTaskStatus::Completed,
            _ => return None,
        };
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| index.to_string());
        tasks.push(StructuredTask {
            id,
            text: text.to_string(),
            status,
        });
    }
    Some(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_snapshot_statuses() {
        let tasks = parse_plan(&json!({
            "sessionUpdate": "plan",
            "entries": [
                {"content": "inspect", "status": "completed", "priority": "high"},
                {"content": "implement", "status": "in_progress", "priority": "medium"},
                {"content": "validate", "status": "pending", "priority": "low"}
            ]
        }))
        .unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].status, StructuredTaskStatus::Completed);
        assert_eq!(tasks[1].status, StructuredTaskStatus::InProgress);
        assert_eq!(tasks[2].status, StructuredTaskStatus::Pending);
    }

    #[test]
    fn valid_empty_snapshot_clears_state() {
        assert_eq!(parse_plan(&json!({"entries": []})), Some(Vec::new()));
    }

    #[test]
    fn malformed_snapshot_is_ignored_instead_of_clearing_state() {
        assert!(parse_plan(&json!({"entries": "bad"})).is_none());
        assert!(parse_plan(&json!({
            "entries": [{"content": "keep prior state", "status": "cancelled"}]
        }))
        .is_none());
        assert!(parse_plan(&json!({
            "entries": [{"content": "", "status": "pending"}]
        }))
        .is_none());
    }
}
