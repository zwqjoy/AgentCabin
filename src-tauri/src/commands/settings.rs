use crate::agent::adapter::ActorSessionMap;
use crate::models::{AgentSettings, ProjectModelPreference, UserSettings};
use crate::storage;
use std::sync::atomic::Ordering;

/// Shared logic for updating user settings with token rotation detection.
/// Used by both IPC (Tauri command) and WS (dispatch) paths.
pub async fn update_user_settings_with_rotation(
    patch: serde_json::Value,
    token_ver: &std::sync::atomic::AtomicU64,
    shutdown: &tokio::sync::broadcast::Sender<()>,
    live_token: &tokio::sync::RwLock<String>,
) -> Result<UserSettings, String> {
    let old = storage::settings::get_user_settings();
    let new_settings = storage::settings::update_user_settings(patch)?;
    if old.web_server_token != new_settings.web_server_token {
        match &new_settings.web_server_token {
            Some(new_tok) => *live_token.write().await = new_tok.clone(),
            None => *live_token.write().await = String::new(),
        }
        token_ver.fetch_add(1, Ordering::Relaxed);
        log::debug!("[web_server] token rotated, updating in-memory + disconnecting WS clients");
        let _ = shutdown.send(());
    }
    Ok(new_settings)
}

fn default_model_only_patch(patch: &serde_json::Value) -> Option<&str> {
    let object = patch.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.get("default_model")?.as_str()
}

#[tauri::command]
pub fn get_user_settings() -> UserSettings {
    log::debug!("[settings] get_user_settings");
    storage::settings::get_user_settings()
}

#[tauri::command]
pub async fn update_user_settings(
    app: tauri::AppHandle,
    patch: serde_json::Value,
    sessions: tauri::State<'_, ActorSessionMap>,
    token_ver: tauri::State<'_, crate::SharedTokenVersion>,
    shutdown: tauri::State<'_, crate::WsShutdownSender>,
    live_token: tauri::State<'_, crate::SharedLiveToken>,
) -> Result<UserSettings, String> {
    log::debug!("[settings] update_user_settings");

    // Legacy chat model routing writes every non-Codex model into the Claude-global
    // `default_model`. When the selected run is Pi, update_run_model has already persisted the
    // Pi-scoped choice; live switching is handled separately by the capability-gated session
    // control call. Detect that exact one-field follow-up write and drop it, so a Pi model never
    // becomes Claude's default model.
    if let Some(model) = default_model_only_patch(&patch) {
        let pi_settings = storage::settings::get_agent_settings("pi");
        if pi_settings.model.as_deref() == Some(model) {
            let has_matching_active_pi = {
                let map = sessions.lock().await;
                map.keys().any(|run_id| {
                    storage::runs::get_run(run_id)
                        .is_some_and(|run| run.agent == "pi" && run.model.as_deref() == Some(model))
                })
            };
            if has_matching_active_pi {
                log::debug!(
                    "[settings] ignored legacy Claude default_model write for active Pi model {}",
                    model
                );
                return Ok(storage::settings::get_user_settings());
            }
        }
    }

    let settings =
        update_user_settings_with_rotation(patch.clone(), &token_ver, &shutdown, &live_token)
            .await?;
    crate::pet::apply_settings_change(&app, &settings, &patch);
    Ok(settings)
}

#[tauri::command]
pub fn get_agent_settings(agent: String) -> AgentSettings {
    log::debug!("[settings] get_agent_settings: agent={}", agent);
    storage::settings::get_agent_settings(&agent)
}

#[tauri::command]
pub fn get_project_preferences(
    cwd: String,
    remote_host_name: Option<String>,
    agent: String,
) -> Result<ProjectModelPreference, String> {
    storage::project_preferences::get(&cwd, remote_host_name.as_deref(), &agent)
}

#[tauri::command]
pub fn update_project_preferences(
    cwd: String,
    remote_host_name: Option<String>,
    agent: String,
    patch: serde_json::Value,
) -> Result<ProjectModelPreference, String> {
    storage::project_preferences::update(&cwd, remote_host_name.as_deref(), &agent, patch)
}

#[tauri::command]
pub fn update_agent_settings(
    agent: String,
    patch: serde_json::Value,
) -> Result<AgentSettings, String> {
    log::debug!("[settings] update_agent_settings: agent={}", agent);

    // The legacy chat route clears agent-scoped effort after reading it because Claude
    // historically moved effort into ~/.claude/settings.json. Pi intentionally keeps its
    // thinking level in AgentSettings and uses "off" (not an empty string) to disable it.
    // Ignore that legacy empty migration patch so switching to Pi cannot erase its setting.
    if agent == "pi"
        && patch
            .get("effort")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.is_empty())
        && patch.as_object().is_some_and(|object| object.len() == 1)
    {
        log::debug!("[settings] ignored legacy empty Pi effort migration");
        return Ok(storage::settings::get_agent_settings(&agent));
    }

    storage::settings::update_agent_settings(&agent, patch)
}
