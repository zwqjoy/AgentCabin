use crate::storage::agent_plugins::{self, AgentPluginBinding, AgentPluginSummary};

static EXPERT_CONFIG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

#[tauri::command]
pub fn resolve_session_expert(
    plugin_id: String,
) -> Result<crate::storage::session_experts::SessionExpert, String> {
    crate::storage::session_experts::resolve_with_root(&crate::storage::data_dir(), &plugin_id)
}

#[tauri::command]
pub fn get_session_expert(
    run_id: String,
) -> Result<Option<crate::storage::session_experts::SessionExpert>, String> {
    if crate::storage::runs::get_run(&run_id).is_none() {
        return Err("对话不存在".into());
    }
    crate::storage::session_experts::get_with_root(&crate::storage::data_dir(), &run_id)
}

#[tauri::command]
pub async fn set_session_expert(
    run_id: String,
    plugin_id: Option<String>,
) -> Result<Option<crate::storage::session_experts::SessionExpert>, String> {
    let _guard = EXPERT_CONFIG_LOCK.lock().await;
    let run = crate::storage::runs::get_run(&run_id).ok_or("对话不存在")?;
    let root = crate::storage::data_dir();
    let previous = crate::storage::session_experts::get_with_root(&root, &run_id)?;
    let selected = plugin_id
        .as_deref()
        .map(|id| crate::storage::session_experts::resolve_with_root(&root, id))
        .transpose()?;
    crate::storage::session_experts::save_with_root(&root, &run_id, selected.as_ref())?;
    let result = async {
        let provider =
            crate::agent::capability_resolver::RuntimeProviderKind::try_from_agent_str(&run.agent)?;
        let caps = crate::agent::capability_resolver::CapabilityResolver::resolve(
            &root,
            run.app_mode,
            provider,
            &run.cwd,
            &run_id,
        )
        .await?;
        crate::agent::runtime_providers::get_adapter(provider).prepare_runtime(&caps)?;
        Ok::<_, String>(selected.clone())
    }
    .await;
    if result.is_err() {
        crate::storage::session_experts::save_with_root(&root, &run_id, previous.as_ref())?;
        if let Ok(provider) =
            crate::agent::capability_resolver::RuntimeProviderKind::try_from_agent_str(&run.agent)
        {
            if let Ok(caps) = crate::agent::capability_resolver::CapabilityResolver::resolve(
                &root,
                run.app_mode,
                provider,
                &run.cwd,
                &run_id,
            )
            .await
            {
                let _ =
                    crate::agent::runtime_providers::get_adapter(provider).prepare_runtime(&caps);
            }
        }
    }
    result
}

/// Reconcile current selection before every Pi turn, including queued follow-ups.
pub(crate) async fn refresh_session_expert_runtime(run_id: &str) -> Result<(), String> {
    let _guard = EXPERT_CONFIG_LOCK.lock().await;
    let Some(run) = crate::storage::runs::get_run(run_id) else {
        return Ok(());
    };
    if run.agent != "pi" {
        return Ok(());
    }
    let caps = crate::agent::capability_resolver::CapabilityResolver::resolve(
        &crate::storage::data_dir(),
        run.app_mode,
        crate::agent::capability_resolver::RuntimeProviderKind::Pi,
        &run.cwd,
        run_id,
    )
    .await?;
    crate::agent::runtime_providers::get_adapter(
        crate::agent::capability_resolver::RuntimeProviderKind::Pi,
    )
    .prepare_runtime(&caps)?;
    Ok(())
}
