use std::collections::HashMap;
use std::path::Path;

use super::ensure_work_enabled;
use crate::work::apps::models::{
    AppCatalogItem, AppConnection, AuthorizeResponse, ConnectionStatus,
};
use crate::work::models::{
    WorkBrowserHealth, WorkBrowserSummary, WorkCapabilityMatch, WorkConnectorHealth,
    WorkConnectorSummary, WorkResourceSummary,
};
use crate::work::{
    apps, browser, connector_package, connector_package_manager, connectors, mcp, paths::WorkPaths,
    resources,
};

#[tauri::command]
pub fn work_list_resources(runtime: Option<String>) -> Result<Vec<WorkResourceSummary>, String> {
    ensure_work_enabled()?;
    resources::list_resources_for_runtime(runtime.as_deref())
}

#[tauri::command]
pub fn work_set_resource_enabled(id: String, enabled: bool) -> Result<WorkResourceSummary, String> {
    ensure_work_enabled()?;
    resources::set_resource_enabled(&id, enabled)
}

#[tauri::command]
pub async fn work_install_community_skill(
    source: String,
    skill_id: String,
) -> Result<WorkResourceSummary, String> {
    ensure_work_enabled()?;
    resources::install_community_skill(&source, &skill_id).await
}

#[tauri::command]
pub async fn work_import_skill_zip(
    zip_path: String,
    slug: String,
) -> Result<WorkResourceSummary, String> {
    ensure_work_enabled()?;
    resources::import_skill_zip(&zip_path, &slug).await
}

#[tauri::command]
pub fn work_discover_capabilities(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<WorkCapabilityMatch>, String> {
    ensure_work_enabled()?;
    resources::discover(&query, limit)
}

#[tauri::command]
pub fn work_list_connectors() -> Result<Vec<WorkConnectorSummary>, String> {
    ensure_work_enabled()?;
    connectors::list()
}

#[tauri::command]
pub fn work_save_connector(
    name: String,
    transport: String,
    command: Option<String>,
    args: Vec<String>,
    url: Option<String>,
    env_vars: Option<HashMap<String, String>>,
    headers: Option<HashMap<String, String>>,
) -> Result<WorkConnectorSummary, String> {
    ensure_work_enabled()?;
    connectors::upsert(
        &name,
        &transport,
        command.as_deref(),
        &args,
        url.as_deref(),
        env_vars.as_ref(),
        headers.as_ref(),
    )
}

#[tauri::command]
pub fn work_toggle_connector(name: String, enabled: bool) -> Result<WorkConnectorSummary, String> {
    ensure_work_enabled()?;
    connectors::toggle(&name, enabled)
}

#[tauri::command]
pub fn work_remove_connector(name: String) -> Result<(), String> {
    ensure_work_enabled()?;
    connectors::remove(&name)
}

#[tauri::command]
pub fn work_list_connector_packages(
) -> Result<Vec<connector_package::ConnectorPackageSummary>, String> {
    ensure_work_enabled()?;
    connector_package_manager::list()
}

#[tauri::command]
pub fn work_list_connector_catalog() -> Result<Vec<connector_package::ConnectorCatalogItem>, String>
{
    ensure_work_enabled()?;
    connector_package_manager::list_builtin_catalog()
}

#[tauri::command]
pub fn work_validate_connector_package(
    source: String,
) -> Result<connector_package::ConnectorPackageManifest, String> {
    ensure_work_enabled()?;
    connector_package_manager::validate_directory(Path::new(&source))
}

#[tauri::command]
pub fn work_install_connector_package(
    source: String,
) -> Result<connector_package::ConnectorPackageSummary, String> {
    ensure_work_enabled()?;
    connector_package_manager::install(Path::new(&source))
}

#[tauri::command]
pub fn work_trust_connector_package(
    package_id: String,
    trusted: bool,
) -> Result<connector_package::ConnectorPackageSummary, String> {
    ensure_work_enabled()?;
    connector_package_manager::set_trusted(&package_id, trusted)
}

#[tauri::command]
pub fn work_enable_connector_package(
    package_id: String,
    enabled: bool,
) -> Result<connector_package::ConnectorPackageSummary, String> {
    ensure_work_enabled()?;
    connector_package_manager::set_enabled(&package_id, enabled)
}

#[tauri::command]
pub fn work_uninstall_connector_package(package_id: String) -> Result<(), String> {
    ensure_work_enabled()?;
    connector_package_manager::uninstall(&package_id)
}

#[tauri::command]
pub async fn work_install_mcp_adapter() -> Result<String, String> {
    ensure_work_enabled()?;
    mcp::install_adapter().await
}

#[tauri::command]
pub fn work_is_mcp_adapter_installed() -> Result<bool, String> {
    ensure_work_enabled()?;
    let paths = crate::work::paths::WorkPaths::app();
    paths.ensure_layout()?;
    Ok(connectors::is_adapter_installed(&paths))
}

#[tauri::command]
pub async fn work_test_connector(name: String) -> Result<WorkConnectorHealth, String> {
    ensure_work_enabled()?;
    mcp::test(&name).await
}

#[tauri::command]
pub fn work_get_browser_config() -> Result<WorkBrowserSummary, String> {
    ensure_work_enabled()?;
    browser::get_config()
}

#[tauri::command]
pub fn work_save_browser_config(
    provider: String,
    enabled: bool,
    max_results: Option<u32>,
    api_key: Option<String>,
    endpoint_url: Option<String>,
    allowed_hosts: Option<Vec<String>>,
) -> Result<WorkBrowserSummary, String> {
    ensure_work_enabled()?;
    browser::save_config(
        &provider,
        enabled,
        max_results,
        api_key.as_deref(),
        endpoint_url.as_deref(),
        allowed_hosts,
    )
}

#[tauri::command]
pub async fn work_test_browser() -> Result<WorkBrowserHealth, String> {
    ensure_work_enabled()?;
    browser::test().await
}

#[tauri::command]
pub fn work_install_browser_adapter() -> Result<String, String> {
    ensure_work_enabled()?;
    resources::install_browser_adapter()
}

#[tauri::command]
pub fn work_is_browser_adapter_installed() -> Result<bool, String> {
    ensure_work_enabled()?;
    let paths = crate::work::paths::WorkPaths::app();
    paths.ensure_layout()?;
    Ok(browser::adapter_installed(&paths))
}

#[tauri::command]
pub async fn work_apps_catalog() -> Result<Vec<AppCatalogItem>, String> {
    ensure_work_enabled()?;
    apps::list_catalog().await
}

#[tauri::command]
pub fn work_apps_connections() -> Result<Vec<AppConnection>, String> {
    ensure_work_enabled()?;
    apps::list_connections()
}

#[tauri::command]
pub async fn work_app_authorize(
    app_id: String,
    redirect_url: Option<String>,
    user_id: Option<String>,
) -> Result<AuthorizeResponse, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let response =
        apps::authorize_with_paths(&paths, &app_id, redirect_url.as_deref(), user_id.as_deref())
            .await?;
    connector_package_manager::sync_auth_status_from_app_with_paths(
        &paths,
        &app_id,
        ConnectionStatus::Pending,
    )?;
    Ok(response)
}

/// Start the auth flow for a generic Connector Package. The Package id is the
/// product-level identity; the existing Apps provider remains the Host-side
/// OAuth implementation and no credential is returned to Pi.
#[tauri::command]
pub async fn work_start_connector_auth(
    package_id: String,
    account_id: Option<String>,
) -> Result<AuthorizeResponse, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    if package_id.trim().eq_ignore_ascii_case("feishu") {
        connector_package_manager::ensure_builtin_feishu_package(&paths)?;
    }
    let package = connector_package_manager::get_with_paths(&paths, &package_id)?;
    if !package.state.trusted || !package.state.enabled {
        return Err(format!(
            "Connector Package '{}' must be trusted and enabled before authentication",
            package.manifest.display_name
        ));
    }
    match package.manifest.auth.kind {
        connector_package::ConnectorAuthKind::OAuth2 => {
            let response = apps::authorize_with_paths(
                &paths,
                &package.manifest.id,
                None,
                account_id.as_deref(),
            )
            .await?;
            connector_package_manager::set_auth_status_with_paths(
                &paths,
                &package.manifest.id,
                connector_package::ConnectorAuthStatus::Pending,
            )?;
            Ok(response)
        }
        connector_package::ConnectorAuthKind::ApiKey => Err(format!(
            "Connector Package '{}' requires API key configuration; the Package provider has not declared a host setup flow",
            package.manifest.display_name
        )),
        connector_package::ConnectorAuthKind::Cli if package.manifest.id == "feishu" => {
            let status = tokio::task::spawn_blocking(move || {
                apps::lark_cli::start_browser_auth(&paths, None, None)
            })
            .await
            .map_err(|error| format!("飞书授权任务启动失败: {error}"))??;
            Ok(AuthorizeResponse {
                authorization_url: status.url.unwrap_or_default(),
                connection_id: "conn_feishu_cli".into(),
                expires_at: None,
            })
        }
        connector_package::ConnectorAuthKind::Cli => Err(format!(
            "Connector Package '{}' uses CLI auth; run its declared auth operation from the task",
            package.manifest.display_name
        )),
        connector_package::ConnectorAuthKind::None => {
            Err("Connector Package does not require authentication".into())
        }
    }
}

#[tauri::command]
pub fn work_configure_connector_token(
    package_id: String,
    values: HashMap<String, String>,
) -> Result<connector_package::ConnectorPackageSummary, String> {
    ensure_work_enabled()?;
    connector_package_manager::configure_token_auth(&package_id, values.into_iter().collect())
}

#[tauri::command]
pub async fn work_app_status(
    app_id: String,
    connection_id: Option<String>,
) -> Result<ConnectionStatus, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let status = apps::check_status_with_paths(&paths, &app_id, connection_id.as_deref()).await?;
    connector_package_manager::sync_auth_status_from_app_with_paths(&paths, &app_id, status)?;
    Ok(status)
}

#[tauri::command]
pub async fn work_app_disconnect(app_id: String, account_id: Option<String>) -> Result<(), String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    apps::disconnect_with_paths(&paths, &app_id, account_id.as_deref()).await?;
    if account_id.is_none() {
        connector_package_manager::sync_auth_status_from_app_with_paths(
            &paths,
            &app_id,
            ConnectionStatus::Disconnected,
        )?;
    }
    Ok(())
}

#[tauri::command]
pub fn work_app_connect_native(
    app_id: String,
    token: String,
    alias: Option<String>,
    email: Option<String>,
) -> Result<AppConnection, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    apps::connect_native_with_paths(&paths, &app_id, &token, alias.as_deref(), email.as_deref())
}

#[tauri::command]
pub fn work_lark_cli_check() -> Result<apps::lark_cli::LarkCliInfo, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    Ok(apps::lark_cli::check_lark_cli_installed(&paths))
}

#[tauri::command]
pub fn work_lark_cli_install() -> Result<String, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    apps::lark_cli::install_lark_cli(&paths)
}

#[tauri::command]
pub fn work_lark_skills_mount() -> Result<Vec<String>, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    apps::lark_cli::mount_lark_skills_to_profile(&paths)
}

#[tauri::command]
pub async fn work_lark_auth_start(
    alias: Option<String>,
    email: Option<String>,
) -> Result<apps::lark_cli::LarkAuthStatus, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    tokio::task::spawn_blocking(move || {
        apps::lark_cli::start_browser_auth(&paths, alias.as_deref(), email.as_deref())
    })
    .await
    .map_err(|error| format!("飞书授权任务启动失败: {error}"))?
}

#[tauri::command]
pub async fn work_lark_auth_status(
    task_id: Option<String>,
) -> Result<apps::lark_cli::LarkAuthStatus, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    tokio::task::spawn_blocking(move || apps::lark_cli::auth_status(&paths, task_id.as_deref()))
        .await
        .map_err(|error| format!("飞书授权状态查询失败: {error}"))?
}

#[tauri::command]
pub fn work_apps_get_provider_config() -> Result<apps::composio::ComposioConfigInfo, String> {
    ensure_work_enabled()?;
    apps::get_provider_config()
}

#[tauri::command]
pub fn work_apps_save_provider_config(api_key: String) -> Result<(), String> {
    ensure_work_enabled()?;
    apps::save_provider_config(&api_key)
}

#[tauri::command]
pub fn work_app_set_default_account(
    workspace_id: String,
    app_id: String,
    account_id: String,
) -> Result<(), String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    crate::work::apps::storage::set_workspace_default_account(
        &paths,
        &workspace_id,
        &app_id,
        &account_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn work_get_capability_center_projection(
    runtime: Option<String>,
) -> Result<crate::work::capability_projection::CapabilityCenterProjection, String> {
    let paths = WorkPaths::app();
    crate::work::capability_projection::build_capability_center_projection(
        &paths,
        runtime.as_deref(),
    )
}

#[tauri::command]
pub fn work_search_capabilities(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<crate::work::capability_projection::CapabilityCenterItem>, String> {
    let paths = WorkPaths::app();
    crate::work::capability_projection::search_capability_center(&paths, &query, limit)
}

#[tauri::command]
pub async fn work_get_run_effective_capabilities(
    run_id: String,
) -> Result<crate::work::capability_projection::RunEffectiveCapabilitiesView, String> {
    let data_dir = crate::storage::data_dir();
    crate::work::capability_projection::get_run_effective_capabilities(&data_dir, &run_id).await
}
