use crate::models::{now_iso, RunMeta, RunStatus, TaskRun};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path};
use std::sync::{Arc, Mutex, RwLock};

// ── In-memory cache for RunMeta to avoid scanning hundreds of directories on disk ──
static RUNS_CACHE: Lazy<RwLock<Option<HashMap<String, RunMeta>>>> = Lazy::new(|| RwLock::new(None));

#[derive(Clone)]
struct EventSummaryCacheEntry {
    file_len: u64,
    modified: std::time::SystemTime,
    summary: (Option<String>, u32, Option<String>),
}

static EVENT_SUMMARIES_CACHE: Lazy<RwLock<HashMap<std::path::PathBuf, EventSummaryCacheEntry>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn invalidate_runs_cache() {
    if let Ok(mut lock) = RUNS_CACHE.write() {
        *lock = None;
    }
    if let Ok(mut lock) = EVENT_SUMMARIES_CACHE.write() {
        lock.clear();
    }
}

// ── Per-run mutex for serializing read-modify-write on meta.json ──

static META_LOCKS: Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn meta_lock(id: &str) -> Arc<Mutex<()>> {
    META_LOCKS
        .lock()
        .unwrap()
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Execute a read-modify-write on RunMeta under a per-run lock.
/// Covers active-session hot paths that can race (rename, status, sync).
pub fn with_meta<F>(id: &str, f: F) -> Result<(), String>
where
    F: FnOnce(&mut RunMeta) -> Result<(), String>,
{
    let lock = meta_lock(id);
    let _guard = lock.lock().map_err(|e| format!("meta lock: {e}"))?;
    let mut meta = get_run(id).ok_or_else(|| format!("Run {} not found", id))?;
    f(&mut meta)?;
    save_meta(&meta)
}

#[allow(clippy::too_many_arguments)]
pub fn create_run(
    id: &str,
    prompt: &str,
    cwd: &str,
    agent: &str,
    status: RunStatus,
    model: Option<String>,
    parent_run_id: Option<String>,
    remote_host_name: Option<String>,
    remote_cwd: Option<String>,
    remote_host_snapshot: Option<crate::models::RemoteHost>,
    platform_id: Option<String>,
) -> Result<RunMeta, String> {
    create_run_with_context(
        id,
        prompt,
        cwd,
        agent,
        status,
        model,
        parent_run_id,
        remote_host_name,
        remote_cwd,
        remote_host_snapshot,
        platform_id,
        crate::work::models::AppMode::Code,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_run_with_context(
    id: &str,
    prompt: &str,
    cwd: &str,
    agent: &str,
    status: RunStatus,
    model: Option<String>,
    parent_run_id: Option<String>,
    remote_host_name: Option<String>,
    remote_cwd: Option<String>,
    remote_host_snapshot: Option<crate::models::RemoteHost>,
    platform_id: Option<String>,
    app_mode: crate::work::models::AppMode,
    workspace_id: Option<String>,
) -> Result<RunMeta, String> {
    log::debug!(
        "[storage/runs] create_run: id={}, agent={}, model={:?}, parent={:?}, remote={:?}, platform={:?}, prompt_len={}",
        id,
        agent,
        model,
        parent_run_id,
        remote_host_name,
        platform_id,
        prompt.len()
    );
    let dir = super::run_dir(id);
    super::ensure_dir(&dir).map_err(|e| e.to_string())?;

    let settings = super::settings::get_user_settings();

    // Codex uses its own auth (ChatGPT / API key) — skip platform_id fallback
    let skip_platform_fallback = agent == "codex";
    let (resolved_pid, resolved_base_url) = if skip_platform_fallback {
        (platform_id, None)
    } else {
        // Use explicit platform_id if provided, otherwise fall back to global active
        let pid = platform_id.or_else(|| settings.active_platform_id.clone());

        // Resolve base_url: credential → known provider defaults → global
        let base_url = pid
            .as_ref()
            .and_then(|pid| {
                // Try credential's base_url first
                settings
                    .platform_credentials
                    .iter()
                    .find(|c| c.platform_id == *pid)
                    .and_then(|c| c.base_url.clone())
                    .filter(|s| !s.is_empty())
                    // Fallback to known provider defaults (for keyless platforms without credential)
                    .or_else(|| super::settings::get_provider_info(pid).and_then(|i| i.base_url))
            })
            .or_else(|| settings.anthropic_base_url.clone());

        (pid, base_url)
    };

    // Snapshot no_session_persistence from agent settings at creation time
    let agent_settings = super::settings::get_agent_settings(agent);
    let no_session_persistence = agent_settings.no_session_persistence.unwrap_or(false);

    let meta = RunMeta {
        id: id.to_string(),
        prompt: prompt.to_string(),
        cwd: cwd.to_string(),
        agent: agent.to_string(),
        code_standalone_task: false,
        app_mode,
        agent_target: Some(crate::models::AgentTarget::from_legacy(app_mode, agent)),
        workspace_id,
        work_task_id: None,
        work_run_id: None,
        work_execution_context: None,
        work_preset: None,
        auth_mode: settings.auth_mode,
        status,
        started_at: now_iso(),
        ended_at: None,
        exit_code: None,
        error_message: None,
        session_id: None,
        result_subtype: None,
        model,
        effort: None,
        permission_mode: None,
        parent_run_id,
        continuation_context: None,
        name: None,
        remote_host_name,
        remote_cwd,
        remote_host_snapshot,
        platform_id: resolved_pid,
        platform_base_url: resolved_base_url,
        source: None,
        cli_import_watermark: None,
        cli_session_path: None,
        cli_usage_incomplete: None,
        deleted_at: None,
        no_session_persistence,
        execution_path: None,   // Caller sets after create_run
        conversation_ref: None, // Written by runtime events (session_init / thread.started)
        codex_process_seq: if agent == "codex" { Some(0) } else { None },
        codex_imported_rollouts: None,
        pinned: None,
        archived: None,
        unread: None,
    };

    save_meta(&meta)?;
    Ok(meta)
}

pub fn save_meta(meta: &RunMeta) -> Result<(), String> {
    let dir = super::run_dir(&meta.id);
    super::ensure_dir(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("meta.json");
    let tmp = dir.join(format!(
        "meta.json.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    fs::write(&tmp, &json).map_err(|e| format!("write tmp: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    for attempt in 0..3u8 {
        match fs::rename(&tmp, &path) {
            Ok(()) => {
                if let Ok(mut lock) = RUNS_CACHE.write() {
                    if let Some(cache) = lock.as_mut() {
                        cache.insert(meta.id.clone(), meta.clone());
                    }
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 2 => {
                log::debug!(
                    "[storage/runs] save_meta rename PermissionDenied, retry {}",
                    attempt + 1
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(format!("rename: {e}"));
            }
        }
    }
    // All retries exhausted (should not reach here, but clean up just in case)
    let _ = fs::remove_file(&tmp);
    Err("rename: PermissionDenied after 3 retries".to_string())
}

/// Read meta.json without legacy deleted_at filtering (internal use).
fn get_run_raw(id: &str) -> Option<RunMeta> {
    let path = super::run_dir(id).join("meta.json");
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn get_run(id: &str) -> Option<RunMeta> {
    if let Ok(lock) = RUNS_CACHE.read() {
        if let Some(cache) = lock.as_ref() {
            if let Some(meta) = cache.get(id) {
                if meta.deleted_at.is_some() {
                    return None;
                }
                return Some(meta.clone());
            }
        }
    }
    let meta = get_run_raw(id)?;
    if meta.deleted_at.is_some() {
        log::debug!("[storage/runs] get_run: id={} is legacy-deleted", id);
        return None;
    }
    if let Ok(mut lock) = RUNS_CACHE.write() {
        if let Some(cache) = lock.as_mut() {
            cache.insert(meta.id.clone(), meta.clone());
        }
    }
    Some(meta)
}

/// Increment and return the next codex_process_seq for a run.
/// First call returns 1 (create_run sets initial value to Some(0)).
pub fn next_codex_process_seq(run_id: &str) -> Result<u32, String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    let result = AtomicU32::new(0);
    with_meta(run_id, |meta| {
        let next = meta.codex_process_seq.unwrap_or(0) + 1;
        meta.codex_process_seq = Some(next);
        result.store(next, Ordering::Relaxed);
        Ok(())
    })?;
    Ok(result.load(Ordering::Relaxed))
}

pub fn update_session_id(id: &str, session_id: &str) -> Result<(), String> {
    if session_id.is_empty() {
        log::debug!("[storage/runs] update_session_id: empty session_id, skipping");
        return Ok(());
    }
    with_meta(id, |meta| {
        if meta.session_id.as_deref() == Some(session_id) {
            log::debug!("[storage/runs] update_session_id: unchanged, skipping");
            return Ok(());
        }
        log::debug!(
            "[storage/runs] update_session_id: id={}, old={:?}, new={}",
            id,
            meta.session_id,
            session_id
        );
        meta.session_id = Some(session_id.to_string());
        Ok(())
    })
}

pub fn rename_run(id: &str, name: &str) -> Result<(), String> {
    log::debug!("[storage/runs] rename_run: id={}, name={}", id, name);
    with_meta(id, |meta| {
        meta.name = if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        };
        Ok(())
    })
}

/// Update run-level flags (pinned, archived, unread).
/// Only provided (Some) fields are written; None fields are left unchanged.
pub fn set_run_flags(
    id: &str,
    pinned: Option<bool>,
    archived: Option<bool>,
    unread: Option<bool>,
) -> Result<(), String> {
    log::debug!(
        "[storage/runs] set_run_flags: id={}, pinned={:?}, archived={:?}, unread={:?}",
        id,
        pinned,
        archived,
        unread
    );
    with_meta(id, |meta| {
        if let Some(v) = pinned {
            meta.pinned = Some(v);
        }
        if let Some(v) = archived {
            meta.archived = Some(v);
        }
        if let Some(v) = unread {
            meta.unread = Some(v);
        }
        Ok(())
    })
}

pub fn update_run_model(id: &str, model: &str) -> Result<(), String> {
    log::debug!(
        "[storage/runs] update_run_model: id={}, model={}",
        id,
        model
    );
    with_meta(id, |meta| {
        meta.model = Some(model.to_string());
        Ok(())
    })
}

pub fn update_run_effort(id: &str, effort: &str) -> Result<(), String> {
    log::debug!(
        "[storage/runs] update_run_effort: id={}, effort={}",
        id,
        effort
    );
    with_meta(id, |meta| {
        meta.effort = Some(effort.to_string());
        Ok(())
    })
}

pub fn update_run_permission_mode(id: &str, mode: &str) -> Result<(), String> {
    log::debug!(
        "[storage/runs] update_run_permission_mode: id={}, mode={}",
        id,
        mode
    );
    with_meta(id, |meta| {
        meta.permission_mode = Some(mode.to_string());
        Ok(())
    })
}

pub fn update_run_workspace(id: &str, workspace_id: Option<&str>) -> Result<(), String> {
    log::debug!(
        "[storage/runs] update_run_workspace: id={}, workspace_id={:?}",
        id,
        workspace_id
    );
    with_meta(id, |meta| {
        meta.workspace_id = workspace_id.map(|s| s.to_string());
        Ok(())
    })
}

pub fn update_status(
    id: &str,
    status: RunStatus,
    exit_code: Option<i32>,
    error_message: Option<String>,
) -> Result<(), String> {
    log::debug!(
        "[storage/runs] update_status: id={}, status={:?}, exit_code={:?}",
        id,
        status,
        exit_code
    );
    with_meta(id, |meta| {
        meta.status = status.clone();
        let is_terminal = matches!(
            status,
            RunStatus::Completed | RunStatus::Failed | RunStatus::Stopped
        );
        if is_terminal {
            meta.ended_at = Some(now_iso());
        } else {
            meta.ended_at = None;
        }
        meta.exit_code = exit_code;
        meta.error_message = error_message;
        Ok(())
    })
}

/// Persist only error_message and result_subtype without changing status or ended_at.
/// Used in the event loop when a `result` error arrives — the process may still be running,
/// so we only save the error info. EOF cleanup later sets the terminal status.
pub fn persist_result_error(
    id: &str,
    error_message: Option<String>,
    result_subtype: Option<String>,
) -> Result<(), String> {
    log::debug!(
        "[storage/runs] persist_result_error: id={}, error={:?}, subtype={:?}",
        id,
        error_message,
        result_subtype
    );
    with_meta(id, |meta| {
        meta.error_message = error_message;
        meta.result_subtype = result_subtype;
        Ok(())
    })
}

pub fn list_runs() -> Vec<TaskRun> {
    let runs_dir = super::runs_dir();
    if !runs_dir.exists() {
        return vec![];
    }

    let mut runs: Vec<TaskRun> = vec![];
    if let Ok(entries) = fs::read_dir(&runs_dir) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let meta_path = entry.path().join("meta.json");
            if !meta_path.exists() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<RunMeta>(&content) {
                    if meta.deleted_at.is_some() {
                        continue;
                    }
                    // The Code sidebar must not surface Work conversations. Work has
                    // its own workspace-scoped session lookup.
                    if meta.app_mode != crate::work::models::AppMode::Code {
                        continue;
                    }
                    // Compute summary from events
                    let events_path = entry.path().join("events.jsonl");
                    let (last_activity, msg_count, last_preview) = summarize_events(&events_path);

                    // Skip runs with no events that aren't active (running/pending/idle)
                    if msg_count == 0
                        && !matches!(
                            meta.status,
                            RunStatus::Running | RunStatus::Pending | RunStatus::Idle
                        )
                    {
                        // Still include if recent (within last hour)
                        if let Ok(started) = chrono::DateTime::parse_from_rfc3339(&meta.started_at)
                        {
                            let age = chrono::Utc::now().signed_duration_since(started);
                            if age.num_hours() > 1 {
                                continue;
                            }
                        }
                    }

                    runs.push(meta.to_task_run(last_activity, Some(msg_count), last_preview));
                }
            }
        }
    }

    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    runs
}

pub fn run_with_summary(meta: RunMeta) -> TaskRun {
    let events_path = super::run_dir(&meta.id).join("events.jsonl");
    let (last_activity, msg_count, last_preview) = summarize_events(&events_path);
    meta.to_task_run(last_activity, Some(msg_count), last_preview)
}

fn summarize_events(events_path: &std::path::Path) -> (Option<String>, u32, Option<String>) {
    let metadata = match fs::metadata(events_path) {
        Ok(m) => m,
        Err(_) => return (None, 0, None),
    };

    let file_len = metadata.len();
    let modified = metadata
        .modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    if let Ok(cache) = EVENT_SUMMARIES_CACHE.read() {
        if let Some(entry) = cache.get(events_path) {
            if entry.file_len == file_len && entry.modified == modified {
                return entry.summary.clone();
            }
        }
    }

    let content = match fs::read_to_string(events_path) {
        Ok(c) => c,
        Err(_) => return (None, 0, None),
    };

    let mut last_ts: Option<String> = None;
    let mut msg_count: u32 = 0;
    let mut last_preview: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Cheap substring check before full JSON parse for msg_count
        if line.contains("\"user_message\"")
            || line.contains("\"message_complete\"")
            || line.contains("\"type\":\"user\"")
            || line.contains("\"type\":\"assistant\"")
        {
            msg_count += 1;
        }
    }

    // Scan backwards from tail to find the most recent timestamp and message preview
    for line in content.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if last_ts.is_some() && last_preview.is_some() {
            break;
        }
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
            if last_ts.is_none() {
                if let Some(ts) = event
                    .get("ts")
                    .or_else(|| event.get("timestamp"))
                    .and_then(|v| v.as_str())
                {
                    last_ts = Some(ts.to_string());
                }
            }
            if last_preview.is_none() {
                if event.get("_bus").and_then(|v| v.as_bool()).unwrap_or(false) {
                    if let Some(inner) = event.get("event") {
                        let inner_type = inner.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if inner_type == "user_message" || inner_type == "message_complete" {
                            if let Some(text) = inner.get("text").and_then(|t| t.as_str()) {
                                last_preview = Some(truncate_preview(text));
                            }
                        }
                    }
                } else {
                    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if event_type == "user" || event_type == "assistant" {
                        if let Some(text) = event
                            .get("payload")
                            .and_then(|p| p.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            last_preview = Some(truncate_preview(text));
                        }
                    }
                }
            }
        }
    }

    let summary = (last_ts, msg_count, last_preview);
    if let Ok(mut cache) = EVENT_SUMMARIES_CACHE.write() {
        cache.insert(
            events_path.to_path_buf(),
            EventSummaryCacheEntry {
                file_len,
                modified,
                summary: summary.clone(),
            },
        );
    }

    summary
}

fn truncate_preview(text: &str) -> String {
    if text.chars().count() > 100 {
        let end = text
            .char_indices()
            .nth(100)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        format!("{}...", &text[..end])
    } else {
        text.to_string()
    }
}

/// Return all run metadata (cached in memory to avoid repeated multi-file disk scans)
pub fn list_all_run_metas() -> Vec<RunMeta> {
    if let Ok(lock) = RUNS_CACHE.read() {
        if let Some(cache) = lock.as_ref() {
            let mut metas: Vec<RunMeta> = cache.values().cloned().collect();
            metas.sort_by(|a, b| b.started_at.cmp(&a.started_at));
            return metas;
        }
    }
    let runs_dir = super::runs_dir();
    if !runs_dir.exists() {
        if let Ok(mut lock) = RUNS_CACHE.write() {
            *lock = Some(HashMap::new());
        }
        return vec![];
    }
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(&runs_dir) {
        for entry in entries.flatten() {
            let meta_path = entry.path().join("meta.json");
            if !meta_path.exists() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<RunMeta>(&content) {
                    if meta.deleted_at.is_some() {
                        continue;
                    }
                    map.insert(meta.id.clone(), meta);
                }
            }
        }
    }
    let mut metas: Vec<RunMeta> = map.values().cloned().collect();
    metas.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    if let Ok(mut lock) = RUNS_CACHE.write() {
        *lock = Some(map);
    }
    metas
}

/// Reconcile any "running" runs that survived a crash,
/// and migrate old runs missing auth_mode.
pub fn reconcile_orphaned_runs() {
    let runs_dir = super::runs_dir();
    if !runs_dir.exists() {
        return;
    }
    if let Ok(entries) = fs::read_dir(&runs_dir) {
        for entry in entries.flatten() {
            let meta_path = entry.path().join("meta.json");
            if !meta_path.exists() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&meta_path) {
                if let Ok(mut meta) = serde_json::from_str::<RunMeta>(&content) {
                    let mut dirty = false;

                    if matches!(meta.status, RunStatus::Running | RunStatus::Idle) {
                        meta.status = RunStatus::Stopped;
                        meta.ended_at = Some(now_iso());
                        meta.error_message = Some("Recovered after app restart".to_string());
                        dirty = true;
                    }

                    // Pending = start_run created meta but start_session never completed.
                    // On restart these are orphans — mark Failed so they don't linger in nav.
                    if meta.status == RunStatus::Pending {
                        meta.status = RunStatus::Failed;
                        meta.ended_at = Some(now_iso());
                        meta.error_message = Some("Session never started".to_string());
                        dirty = true;
                        log::debug!(
                            "[storage/runs] reconcile: pending orphan {} -> failed",
                            meta.id
                        );
                    }

                    if dirty {
                        let _ = save_meta(&meta);
                    }
                }
            }
        }
    }
}

fn validate_run_id(id: &str) -> Result<(), String> {
    let mut components = Path::new(id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(format!("Invalid run id: {}", id)),
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {}", path.display(), error)),
    }
}

/// Permanently delete runs and all run-scoped data.
///
/// This removes the complete run directory (meta, messages, attachments,
/// artifacts and imported-session indexes), per-run temporary uploads, prompt
/// favorites, and both derived search indexes. The operation refuses active
/// runs and validates IDs before touching the filesystem.
pub fn delete_runs(ids: &[String]) -> Result<u32, String> {
    let unique_ids: Vec<String> = {
        let mut seen = HashSet::new();
        ids.iter()
            .filter(|id| seen.insert(id.as_str()))
            .cloned()
            .collect()
    };

    if unique_ids.is_empty() {
        return Ok(0);
    }

    // Pre-check every run before deleting any data. This prevents a mixed batch
    // from partially deleting when one target is missing or still active.
    for id in &unique_ids {
        validate_run_id(id)?;
        let meta = get_run_raw(id).ok_or_else(|| format!("Run {} not found", id))?;
        if matches!(
            meta.status,
            RunStatus::Running | RunStatus::Pending | RunStatus::Idle
        ) {
            return Err(format!("Cannot delete: run {} is still active", id));
        }
    }

    let mut removed_ids = Vec::with_capacity(unique_ids.len());
    let mut cleanup_errors = Vec::new();

    for id in &unique_ids {
        let run_path = super::run_dir(id);
        match remove_dir_if_exists(&run_path) {
            Ok(()) => {
                removed_ids.push(id.clone());
                crate::storage::events::invalidate_bus_events_cache(id);
            }
            Err(error) => cleanup_errors.push(format!("failed to delete run {}: {}", id, error)),
        }

        // Legacy pipe transport stores decoded uploads outside the run folder.
        // They are keyed by run ID and must be removed with the conversation.
        let upload_path = std::env::temp_dir().join("agentcabin-uploads").join(id);
        if let Err(error) = remove_dir_if_exists(&upload_path) {
            cleanup_errors.push(format!("failed to delete uploads for {}: {}", id, error));
        }

        // Clean up standalone task directory if it exists.
        let standalone_path = crate::work::paths::WorkPaths::app()
            .standalone_task_dir(id)
            .unwrap_or_else(|_| super::data_dir().join("standalone_tasks").join(id));
        if standalone_path.exists() {
            if let Err(error) = remove_dir_if_exists(&standalone_path) {
                cleanup_errors.push(format!(
                    "failed to delete standalone task dir for {}: {}",
                    id, error
                ));
            }
        }

        // Clean up per-run runtime directories (~/.agentcabin/runtime/<mode>/<provider>/<run_id>)
        if let Err(error) =
            crate::agent::capability_resolver::CapabilityResolver::cleanup_all_run_runtimes(
                &super::data_dir(),
                id,
            )
        {
            cleanup_errors.push(format!(
                "failed to cleanup runtime directories for {}: {}",
                id, error
            ));
        }
    }

    if !removed_ids.is_empty() {
        // Imported-session discovery caches the mapping from source transcripts
        // to local run IDs. Drop it so a deleted local import is not treated as
        // still present until the normal cache TTL expires.
        super::cli_sessions::invalidate_imported_cache();
        if let Err(error) = super::favorites::remove_runs(&removed_ids) {
            cleanup_errors.push(format!("failed to clean prompt favorites: {}", error));
        }
        if let Err(error) = super::prompt_index::remove_runs(&removed_ids) {
            cleanup_errors.push(format!("failed to clean prompt index: {}", error));
        }
        if let Err(error) = super::run_index::remove_runs(&removed_ids) {
            cleanup_errors.push(format!("failed to clean run index: {}", error));
        }
        if let Ok(mut lock) = RUNS_CACHE.write() {
            if let Some(cache) = lock.as_mut() {
                for id in &removed_ids {
                    cache.remove(id);
                }
            }
        }
    }

    let count = removed_ids.len() as u32;
    if cleanup_errors.is_empty() {
        log::debug!(
            "[storage/runs] delete_runs: permanently deleted {} runs",
            count
        );
        Ok(count)
    } else {
        Err(format!(
            "Deleted {} run(s), but cleanup was incomplete: {}",
            count,
            cleanup_errors.join("; ")
        ))
    }
}

#[cfg(test)]
mod deletion_tests {
    use super::{remove_dir_if_exists, validate_run_id};

    #[test]
    fn validates_run_ids_before_building_paths() {
        assert!(validate_run_id("run-123").is_ok());
        assert!(validate_run_id("nested/run").is_err());
        assert!(validate_run_id("../outside").is_err());
        assert!(validate_run_id("").is_err());
    }

    #[test]
    fn removes_run_directory_recursively() {
        let temp = tempfile::tempdir().unwrap();
        let run_dir = temp.path().join("run-123");
        std::fs::create_dir_all(run_dir.join("attachments")).unwrap();
        std::fs::write(run_dir.join("events.jsonl"), "conversation data").unwrap();
        std::fs::write(run_dir.join("attachments").join("image.png"), b"attachment").unwrap();

        remove_dir_if_exists(&run_dir).unwrap();

        assert!(!run_dir.exists());
    }

    #[test]
    fn test_summarize_events_caching_and_invalidation() {
        let temp = tempfile::tempdir().unwrap();
        let events_file = temp.path().join("events.jsonl");

        std::fs::write(
            &events_file,
            "{\"type\":\"user\",\"payload\":{\"text\":\"Hello World\"},\"ts\":\"2026-09-10T10:00:00Z\"}\n",
        )
        .unwrap();

        let (ts1, count1, preview1) = super::summarize_events(&events_file);
        assert_eq!(ts1, Some("2026-09-10T10:00:00Z".to_string()));
        assert_eq!(count1, 1);
        assert_eq!(preview1, Some("Hello World".to_string()));

        // Second call should hit cache
        let (ts2, count2, preview2) = super::summarize_events(&events_file);
        assert_eq!(ts2, ts1);
        assert_eq!(count2, count1);
        assert_eq!(preview2, preview1);

        // Append new event
        std::fs::write(
            &events_file,
            "{\"type\":\"user\",\"payload\":{\"text\":\"Hello World\"},\"ts\":\"2026-09-10T10:00:00Z\"}\n{\"type\":\"assistant\",\"payload\":{\"text\":\"Hi there!\"},\"ts\":\"2026-09-10T10:01:00Z\"}\n",
        )
        .unwrap();

        let (ts3, count3, preview3) = super::summarize_events(&events_file);
        assert_eq!(ts3, Some("2026-09-10T10:01:00Z".to_string()));
        assert_eq!(count3, 2);
        assert_eq!(preview3, Some("Hi there!".to_string()));

        // Invalidate cache
        super::invalidate_runs_cache();
        let (_ts4, count4, _preview4) = super::summarize_events(&events_file);
        assert_eq!(count4, 2);
    }
}
