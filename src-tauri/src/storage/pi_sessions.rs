//! Storage module for discovering and importing Pi Agent CLI sessions.

use crate::storage::cli_sessions_common::{CliSessionSummary, DiscoverResult, ImportResult};
use crate::storage::events::EventWriter;
use std::sync::Arc;

pub fn discover_sessions(target_cwd: &str) -> Result<DiscoverResult, String> {
    log::debug!("[pi_sessions] discover_sessions: target_cwd={}", target_cwd);

    let mut sessions = Vec::new();
    let home = match crate::storage::dirs_next() {
        Some(h) => h,
        None => {
            return Ok(DiscoverResult {
                sessions: vec![],
                total: 0,
                truncated: false,
            })
        }
    };

    let session_dirs = vec![
        home.join(".pi").join("agent").join("sessions"),
        home.join(".pi").join("sessions"),
    ];

    for dir in session_dirs {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            let session_id = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or_default()
                                .to_string();
                            let cwd = val
                                .get("cwd")
                                .and_then(|v| v.as_str())
                                .unwrap_or(target_cwd)
                                .to_string();
                            let first_prompt = val
                                .get("prompt")
                                .or_else(|| val.get("first_prompt"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("Pi Session")
                                .to_string();
                            let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);

                            sessions.push(CliSessionSummary {
                                agent: "pi".to_string(),
                                session_id,
                                cwd,
                                first_prompt,
                                started_at: chrono::Utc::now().to_rfc3339(),
                                last_activity_at: chrono::Utc::now().to_rfc3339(),
                                message_count: 1,
                                model: val.get("model").and_then(|v| v.as_str()).map(String::from),
                                cli_version: None,
                                file_size,
                                file_path: path.to_string_lossy().to_string(),
                                rollout_paths: vec![],
                                has_subagents: false,
                                already_imported: false,
                                existing_run_id: None,
                            });
                        }
                    }
                }
            }
        }
    }

    let total = sessions.len();
    Ok(DiscoverResult {
        sessions,
        total,
        truncated: false,
    })
}

pub fn import_session(
    session_id: &str,
    cwd: &str,
    _writer: Arc<EventWriter>,
) -> Result<ImportResult, String> {
    log::debug!(
        "[pi_sessions] import_session: session_id={}, cwd={}",
        session_id,
        cwd
    );
    Err(format!(
        "Importing Pi session {} is not supported",
        session_id
    ))
}
