//! Tauri commands for DeepSeek Harness (DSH) plugins.

use crate::agent::dsh_plugins::{self, DshPluginManifest, DshPluginRegistration};

#[tauri::command]
pub fn list_dsh_plugins() -> Vec<DshPluginManifest> {
    let data_dir = crate::storage::data_dir();
    dsh_plugins::list_dsh_plugins(&data_dir)
}

#[tauri::command]
pub fn toggle_dsh_plugin(plugin_id: String, enabled: bool) -> Result<DshPluginManifest, String> {
    let data_dir = crate::storage::data_dir();
    dsh_plugins::toggle_dsh_plugin(&data_dir, &plugin_id, enabled)
}

#[tauri::command]
pub fn register_dsh_plugin(
    registration: DshPluginRegistration,
) -> Result<DshPluginManifest, String> {
    let data_dir = crate::storage::data_dir();
    dsh_plugins::DshPluginManager::new(data_dir).register_plugin(registration)
}

#[tauri::command]
pub fn unregister_dsh_plugin(plugin_id: String) -> Result<(), String> {
    let data_dir = crate::storage::data_dir();
    dsh_plugins::DshPluginManager::new(data_dir).unregister_plugin(&plugin_id)
}

#[tauri::command]
pub fn update_dsh_plugin(plugin_id: String) -> Result<DshPluginManifest, String> {
    let data_dir = crate::storage::data_dir();
    dsh_plugins::DshPluginManager::new(data_dir).update_plugin(&plugin_id)
}
