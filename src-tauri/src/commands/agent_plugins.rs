use crate::storage::agent_plugins::{self, AgentPluginBinding, AgentPluginSummary};

#[tauri::command]
pub fn list_agent_plugins() -> Result<Vec<AgentPluginSummary>, String> {
    Ok(agent_plugins::list_agent_plugins())
}

#[tauri::command]
pub async fn install_agent_plugin(source: String) -> Result<AgentPluginSummary, String> {
    tokio::task::spawn_blocking(move || agent_plugins::install_agent_plugin(&source))
        .await
        .map_err(|error| format!("Agent Plugin installation task failed: {error}"))?
}

#[tauri::command]
pub async fn update_agent_plugin(plugin_id: String) -> Result<AgentPluginSummary, String> {
    tokio::task::spawn_blocking(move || agent_plugins::update_agent_plugin(&plugin_id))
        .await
        .map_err(|error| format!("Agent Plugin update task failed: {error}"))?
}

#[tauri::command]
pub fn uninstall_agent_plugin(plugin_id: String) -> Result<(), String> {
    agent_plugins::uninstall_agent_plugin(&plugin_id)
}

#[tauri::command]
pub fn set_agent_plugin_trust(
    plugin_id: String,
    trusted: bool,
) -> Result<AgentPluginSummary, String> {
    agent_plugins::set_agent_plugin_trust(&plugin_id, trusted)
}

#[tauri::command]
pub fn get_agent_plugin_bindings() -> Result<Vec<AgentPluginBinding>, String> {
    agent_plugins::get_agent_plugin_bindings()
}

#[tauri::command]
pub fn set_agent_plugin_binding(plugin_id: String, enabled: bool) -> Result<(), String> {
    agent_plugins::set_agent_plugin_binding(&plugin_id, enabled)
}

#[tauri::command]
pub async fn sync_from_twork() -> Result<crate::work::twork_migration::TworkSyncReport, String> {
    tokio::task::spawn_blocking(crate::work::twork_migration::sync_from_twork_if_present)
        .await
        .map_err(|error| format!("T-Work sync task failed: {error}"))?
}
