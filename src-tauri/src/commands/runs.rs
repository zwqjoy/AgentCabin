use crate::agent::adapter::ActorSessionMap;
use crate::models::{
    AgentTarget, ExecutionPath, PromptFavorite, PromptSearchResult, RunStatus, TaskRun,
};
use crate::process_ext::HideConsole;
use crate::storage;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Validate that an agent supports the requested execution path.
/// Pi and Grok require bidirectional session actors for interactive RPC/ACP.
fn validate_agent_path(agent: &str, path: &ExecutionPath) -> Result<(), String> {
    match agent {
        "pi" if *path == ExecutionPath::PipeExec => Err(
            "Pi Agent requires execution_path=session_actor (RPC mode); pipe_exec cannot display permission prompts".to_string(),
        ),
        "grok" if *path == ExecutionPath::PipeExec => Err(
            "Grok Build requires execution_path=session_actor (ACP mode); pipe_exec cannot handle bidirectional permission requests".to_string(),
        ),
        "dsh" if *path == ExecutionPath::PipeExec => Err(
            "DSH requires execution_path=session_actor (RPC mode); pipe_exec cannot handle bidirectional RPC requests".to_string(),
        ),
        "claude" | "codex" | "pi" | "grok" | "dsh" => Ok(()),
        _ => Err(format!(
            "unknown agent '{}': supported agents are 'claude', 'codex', 'pi', 'grok', and 'dsh'",
            agent
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_agent_path;
    use crate::models::ExecutionPath;

    #[test]
    fn rpc_agents_require_session_actor() {
        assert!(validate_agent_path("pi", &ExecutionPath::SessionActor).is_ok());
        assert!(validate_agent_path("pi", &ExecutionPath::PipeExec).is_err());
        assert!(validate_agent_path("grok", &ExecutionPath::SessionActor).is_ok());
        assert!(validate_agent_path("grok", &ExecutionPath::PipeExec).is_err());
        assert!(validate_agent_path("dsh", &ExecutionPath::SessionActor).is_ok());
        assert!(validate_agent_path("dsh", &ExecutionPath::PipeExec).is_err());
        assert!(validate_agent_path("claude", &ExecutionPath::PipeExec).is_ok());
    }
}

#[tauri::command]
pub async fn list_runs() -> Result<Vec<TaskRun>, String> {
    let runs = tokio::task::spawn_blocking(storage::runs::list_runs)
        .await
        .map_err(|e| format!("list_runs task failed: {}", e))?;
    log::debug!("[runs] list_runs: count={}", runs.len());
    Ok(runs)
}

#[tauri::command]
pub fn get_run(id: String) -> Result<TaskRun, String> {
    log::debug!("[runs] get_run: id={}", id);
    let meta = storage::runs::get_run(&id).ok_or_else(|| format!("Run {} not found", id))?;
    // Skip expensive events file scan — msg_count and last_preview are only
    // needed for sidebar listing (already computed by list_runs/summarize_events).
    // This avoids reading + parsing the entire events.jsonl on every session switch.
    Ok(meta.to_task_run(None, None, None))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn start_run(
    prompt: String,
    cwd: String,
    agent: String,
    model: Option<String>,
    remote_host_name: Option<String>,
    platform_id: Option<String>,
    execution_path: Option<String>,
    continuation_context: Option<String>,
) -> Result<TaskRun, String> {
    let requested_model = model.clone();
    let user_settings = storage::settings::get_user_settings();
    let model = storage::settings::resolve_model_for_agent(
        &user_settings,
        &agent,
        requested_model.as_deref(),
    );
    log::debug!(
        "[runs] start_run: agent={}, requested_model={:?}, model={:?}, remote={:?}, platform={:?}, path={:?}, continuation_len={:?}, prompt_len={}, cwd={}",
        agent,
        requested_model,
        model,
        remote_host_name,
        platform_id,
        execution_path,
        continuation_context.as_ref().map(|value| value.len()),
        prompt.len(),
        cwd
    );

    let path: ExecutionPath = match execution_path {
        Some(s) => serde_json::from_value(serde_json::Value::String(s.clone())).map_err(|_| {
            format!(
                "invalid execution_path '{}': expected 'session_actor' or 'pipe_exec'",
                s
            )
        })?,
        None => {
            let use_session_actor = agent == "claude"
                || agent == "pi"
                || agent == "grok"
                || agent == "dsh"
                || (agent == "codex"
                    && storage::settings::get_user_settings()
                        .codex_transport
                        .as_deref()
                        != Some("exec")
                    && crate::commands::session::codex_appserver_supported());
            if use_session_actor {
                ExecutionPath::SessionActor
            } else {
                ExecutionPath::PipeExec
            }
        }
    };
    validate_agent_path(&agent, &path)?;

    let (remote_cwd, remote_host_snapshot) = if let Some(ref name) = remote_host_name {
        let settings = storage::settings::get_user_settings();
        let host = settings
            .remote_hosts
            .iter()
            .find(|h| h.name == *name)
            .ok_or_else(|| format!("Remote host '{}' not found in settings", name))?;
        let effective = if !cwd.is_empty() && cwd != "/" {
            Some(cwd.clone())
        } else {
            host.remote_cwd.clone()
        };
        (effective, Some(host.clone()))
    } else {
        (None, None)
    };

    let id = uuid::Uuid::new_v4().to_string();
    let mut meta = storage::runs::create_run(
        &id,
        &prompt,
        &cwd,
        &agent,
        RunStatus::Pending,
        model,
        None,
        remote_host_name,
        remote_cwd,
        remote_host_snapshot,
        // Claude platform credentials are unrelated to Pi/Grok/Dsh native providers.
        if matches!(agent.as_str(), "pi" | "grok" | "dsh") {
            None
        } else {
            platform_id
        },
    )?;
    meta.execution_path = Some(path);
    meta.continuation_context = continuation_context.filter(|value| !value.trim().is_empty());
    storage::runs::save_meta(&meta)?;
    log::debug!("[runs] start_run: created id={}", id);
    Ok(meta.to_task_run(None, None, None))
}

#[tauri::command]
pub fn rename_run(id: String, name: String) -> Result<(), String> {
    log::debug!("[runs] rename_run: id={}, name={}", id, name);
    storage::runs::rename_run(&id, &name)
}

#[tauri::command]
pub fn delete_runs(ids: Vec<String>) -> Result<u32, String> {
    log::debug!("[cmd/runs] delete_runs: ids={:?}", ids);
    storage::runs::delete_runs(&ids)
}

/// Partial update of run-level flags (pinned, archived, unread).
/// Only fields present in `flags` are written; absent fields are left unchanged.
#[derive(Debug, Deserialize)]
pub struct RunFlags {
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(default)]
    pub unread: Option<bool>,
}

#[tauri::command]
pub async fn set_run_flags(
    id: String,
    flags: RunFlags,
    sessions: tauri::State<'_, ActorSessionMap>,
    process_map: tauri::State<'_, crate::agent::stream::ProcessMap>,
) -> Result<(), String> {
    log::debug!(
        "[runs] set_run_flags: id={}, pinned={:?}, archived={:?}, unread={:?}",
        id,
        flags.pinned,
        flags.archived,
        flags.unread
    );
    set_run_flags_impl(&id, flags, &sessions, &process_map).await
}

/// Shared flag-update path for the desktop command and the web transport.
///
/// Archiving only hides the conversation from the sidebar; a run that is still
/// running/pending/idle keeps its session attached. That left archived chats
/// reported as "still active" and blocked permanent deletion, so archiving now
/// lands the run on a terminal status first. Only active runs are stopped —
/// completed/failed/stopped runs keep their existing status.
pub async fn set_run_flags_impl(
    id: &str,
    flags: RunFlags,
    sessions: &ActorSessionMap,
    process_map: &crate::agent::stream::ProcessMap,
) -> Result<(), String> {
    if flags.archived == Some(true) && run_is_active(id) {
        log::debug!("[runs] set_run_flags: stopping active run before archiving {id}");
        stop_run_live(id, sessions, process_map).await?;
    }

    storage::runs::set_run_flags(id, flags.pinned, flags.archived, flags.unread)
}

/// Whether the persisted run status counts as active for stop/delete guards.
pub(crate) fn run_is_active(id: &str) -> bool {
    storage::runs::get_run(id).is_some_and(|meta| {
        matches!(
            meta.status,
            RunStatus::Running | RunStatus::Pending | RunStatus::Idle
        )
    })
}

/// Stop the live session/process for a run and persist a terminal status.
/// Shared by `stop_run` and by archiving, which must not leave a run active.
async fn stop_run_live(
    id: &str,
    sessions: &ActorSessionMap,
    process_map: &crate::agent::stream::ProcessMap,
) -> Result<bool, String> {
    let actor_stopped = super::session_dispatch::stop_actor(
        sessions,
        id,
        crate::agent::session_actor::RuntimeStopReason::UserStop,
    )
    .await
    .map(|o| o.was_active)
    .unwrap_or(false);
    if actor_stopped {
        log::debug!("[runs] stop_run: stopped session actor for id={}", id);
    } else {
        crate::agent::stream::stop_process(process_map, id).await;
    }
    if let Err(e) = storage::runs::update_status(
        id,
        RunStatus::Stopped,
        None,
        Some("Stopped by user".to_string()),
    ) {
        log::warn!("[runs] stop_run: failed to update status: {}", e);
    }
    Ok(true)
}

/// Reveal a file or folder in the OS file manager.
/// macOS: `open -R` (selects in Finder), Windows: `explorer /select,`,
/// Linux: `xdg-open` (opens parent dir, no selection support).
#[tauri::command]
pub fn reveal_in_finder(path: String) -> Result<(), String> {
    log::debug!("[runs] reveal_in_finder: path={}", path);
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("open")
            .args(["-R", &path])
            .hide_console()
            .output()
            .map_err(|e| format!("open -R failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "open -R failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .hide_console()
            .spawn()
            .map_err(|e| format!("explorer failed: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let target = if p.is_dir() {
            path.clone()
        } else {
            p.parent()
                .map(|d| d.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone())
        };
        std::process::Command::new("xdg-open")
            .arg(&target)
            .hide_console()
            .spawn()
            .map_err(|e| format!("xdg-open failed: {e}"))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("reveal_in_finder: unsupported platform".to_string())
    }
}

pub(crate) async fn update_run_model_impl(id: String, model: String) -> Result<(), String> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("Model must not be empty".to_string());
    }
    log::debug!("[runs] update_run_model: id={}, model={}", id, model);
    storage::runs::get_run(&id).ok_or_else(|| format!("Run {} not found", id))?;
    storage::runs::update_run_model(&id, &model)
}

#[tauri::command]
pub async fn update_run_model(id: String, model: String) -> Result<(), String> {
    update_run_model_impl(id, model).await
}

pub(crate) async fn update_run_effort_impl(id: String, effort: String) -> Result<(), String> {
    let effort = effort.trim().to_string();
    if effort.is_empty() {
        return Err("Effort must not be empty".to_string());
    }
    storage::runs::get_run(&id).ok_or_else(|| format!("Run {} not found", id))?;
    storage::runs::update_run_effort(&id, &effort)
}

#[tauri::command]
pub async fn update_run_effort(id: String, effort: String) -> Result<(), String> {
    update_run_effort_impl(id, effort).await
}

pub(crate) async fn update_run_permission_mode_impl(
    id: String,
    permission_mode: String,
) -> Result<(), String> {
    let permission_mode = permission_mode.trim().to_string();
    if permission_mode.is_empty() {
        return Err("Permission mode must not be empty".to_string());
    }
    log::debug!(
        "[runs] update_run_permission_mode: id={}, permission_mode={}",
        id,
        permission_mode
    );
    storage::runs::get_run(&id).ok_or_else(|| format!("Run {} not found", id))?;
    storage::runs::update_run_permission_mode(&id, &permission_mode)
}

#[tauri::command]
pub async fn update_run_permission_mode(id: String, permission_mode: String) -> Result<(), String> {
    update_run_permission_mode_impl(id, permission_mode).await
}

#[tauri::command]
pub async fn stop_run(
    id: String,
    sessions: tauri::State<'_, ActorSessionMap>,
    process_map: tauri::State<'_, crate::agent::stream::ProcessMap>,
) -> Result<bool, String> {
    log::debug!("[runs] stop_run: id={}", id);
    stop_run_live(&id, &sessions, &process_map).await
}

#[tauri::command]
pub async fn search_prompts(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<PromptSearchResult>, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(vec![]);
    }
    log::debug!("[runs] search_prompts: query={}", query);

    tokio::task::spawn_blocking(move || {
        let entries = storage::prompt_index::build_or_update_index()?;
        let query_lower = query.to_lowercase();
        let matched: Vec<_> = entries
            .into_iter()
            .filter(|e| e.text.to_lowercase().contains(&query_lower))
            .collect();
        let metas = storage::runs::list_all_run_metas();
        let meta_map: HashMap<String, _> = metas.into_iter().map(|m| (m.id.clone(), m)).collect();
        let favs = storage::favorites::list_favorites();
        let fav_set: HashSet<(String, u64)> = favs.into_iter().map(|f| (f.run_id, f.seq)).collect();
        let mut results: Vec<_> = matched
            .into_iter()
            .filter_map(|entry| {
                let meta = meta_map.get(&entry.run_id)?;
                let target = meta
                    .agent_target
                    .unwrap_or_else(|| AgentTarget::from_legacy(meta.app_mode, &meta.agent));
                if !target.is_native() {
                    return None;
                }
                Some(PromptSearchResult {
                    run_id: entry.run_id.clone(),
                    run_name: meta.name.clone(),
                    run_prompt: meta.prompt.clone(),
                    agent: meta.agent.clone(),
                    agent_target: Some(target),
                    model: meta.model.clone(),
                    status: meta.status.clone(),
                    started_at: meta.started_at.clone(),
                    matched_text: entry.text,
                    matched_seq: entry.seq,
                    matched_ts: entry.ts,
                    matched_event_id: entry.event_id,
                    is_favorite: fav_set.contains(&(entry.run_id, entry.seq)),
                })
            })
            .collect();
        results.sort_by(|a, b| b.matched_ts.cmp(&a.matched_ts));
        results.truncate(limit.unwrap_or(100));
        log::debug!("[runs] search_prompts: {} results", results.len());
        Ok(results)
    })
    .await
    .map_err(|e| format!("search task failed: {e}"))?
}

#[tauri::command]
pub fn add_prompt_favorite(
    run_id: String,
    seq: u64,
    text: String,
) -> Result<PromptFavorite, String> {
    storage::favorites::add_favorite(&run_id, seq, &text)
}

#[tauri::command]
pub fn remove_prompt_favorite(run_id: String, seq: u64) -> Result<(), String> {
    storage::favorites::remove_favorite(&run_id, seq)
}

#[tauri::command]
pub fn update_prompt_favorite_tags(
    run_id: String,
    seq: u64,
    tags: Vec<String>,
) -> Result<(), String> {
    storage::favorites::update_favorite_tags(&run_id, seq, tags)
}

#[tauri::command]
pub fn update_prompt_favorite_note(run_id: String, seq: u64, note: String) -> Result<(), String> {
    storage::favorites::update_favorite_note(&run_id, seq, &note)
}

#[tauri::command]
pub fn list_prompt_favorites() -> Result<Vec<PromptFavorite>, String> {
    Ok(storage::favorites::list_favorites())
}

#[tauri::command]
pub fn list_prompt_tags() -> Result<Vec<String>, String> {
    Ok(storage::favorites::list_all_tags())
}
