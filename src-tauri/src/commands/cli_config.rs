use crate::agent::adapter::ActorSessionMap;
use crate::storage::cli_config;
use serde_json::{json, Value};
use tauri::State;

#[tauri::command]
pub fn get_cli_config() -> Result<Value, String> {
    log::debug!("[cli_config] get_cli_config");
    Ok(cli_config::load_cli_config())
}

#[tauri::command]
pub fn get_project_cli_config(cwd: String) -> Result<Value, String> {
    log::debug!("[cli_config] get_project_cli_config cwd={}", cwd);
    Ok(cli_config::load_project_cli_config(&cwd))
}

/// Update CLI config patch.
/// Shared by the Tauri and WebSocket transports.
pub(crate) async fn update_cli_config_impl(
    _sessions: &ActorSessionMap,
    patch: Value,
) -> Result<Value, String> {
    log::debug!("[cli_config] update_cli_config patch={}", patch);
    cli_config::update_cli_config(patch)
}

#[tauri::command]
pub async fn update_cli_config(
    sessions: State<'_, ActorSessionMap>,
    patch: Value,
) -> Result<Value, String> {
    update_cli_config_impl(sessions.inner(), patch).await
}

// ── Codex config commands ──

/// Returns { config: {}, warning?: string }
#[tauri::command]
pub fn get_codex_config() -> Result<Value, String> {
    log::debug!("[cli_config] get_codex_config");
    let (config, warning) = cli_config::load_codex_config();
    let mut result = serde_json::Map::new();
    result.insert("config".to_string(), config);
    if let Some(w) = warning {
        result.insert("warning".to_string(), Value::String(w));
    }
    Ok(Value::Object(result))
}

#[tauri::command]
pub fn get_project_codex_config(cwd: String) -> Result<Value, String> {
    log::debug!("[cli_config] get_project_codex_config cwd={}", cwd);
    Ok(cli_config::load_project_codex_config(&cwd))
}

#[tauri::command]
pub fn update_codex_config(patch: Value) -> Result<Value, String> {
    log::debug!("[cli_config] update_codex_config patch={}", patch);
    cli_config::update_codex_config(patch)
}

/// Durably toggle one `[features].<name>` flag (nested write, preserves the rest of the table).
/// `enabled=None` clears the override (back to Codex's default). Effective next session.
#[tauri::command]
pub fn set_codex_feature(name: String, enabled: Option<bool>) -> Result<Value, String> {
    log::debug!("[cli_config] set_codex_feature {} = {:?}", name, enabled);
    cli_config::set_codex_feature(&name, enabled)
}

// ── Codex hooks commands ──

/// Returns { hooks: {}, warning?: string }
#[tauri::command]
pub fn get_codex_hooks() -> Result<Value, String> {
    log::debug!("[cli_config] get_codex_hooks");
    let (hooks, warning) = cli_config::load_codex_hooks();
    let mut result = json!({ "hooks": hooks });
    if let Some(w) = warning {
        result["warning"] = w.into();
    }
    Ok(result)
}

#[tauri::command]
pub fn update_codex_hooks(hooks: Value) -> Result<Value, String> {
    log::debug!("[cli_config] update_codex_hooks");
    cli_config::update_codex_hooks(hooks)
}
