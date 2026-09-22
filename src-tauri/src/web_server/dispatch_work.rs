//! Headless Work command bridge for the core server (Electron / no Tauri).
//!
//! Work Mode commands are Tauri-IPC-only in the desktop app: `#[tauri::command]`
//! wrappers in `commands/work/*` over `crate::work::*` managers, never added to
//! the web-server dispatcher (Work was blocked over the WS transport). With the
//! headless core server (P4) the Electron renderer reaches us through
//! `CoreRuntime::invoke`, so this module routes `work_*` calls:
//!
//! - Plain commands (no `tauri::State`) are invoked directly.
//! - Session-lifecycle commands call the same inner impls the Tauri wrappers
//!   delegate to, passing the state held by [`CoreRuntime`] instead of
//!   `tauri::State` extracts.
//! - Desktop-only bits (notifications) are skipped.
//!
//! Params accept both snake_case and camelCase keys: Tauri converts camelCase
//! JS args to snake_case Rust params, the headless transport has no such
//! conversion and the renderer sends camelCase (`workspaceId`).

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::core::runtime::CoreRuntime;
use crate::work::models::WorkRecoveryAction;
use crate::work::paths::WorkPaths;

/// Extract a typed parameter by snake_case name, falling back to its camelCase
/// form. A missing or `null` value deserializes from JSON `null`, which maps
/// `Option<T>` params to `None` and fails required ones with a clear error.
fn param<T: DeserializeOwned>(params: &Value, snake: &str) -> Result<T, String> {
    let mut camel = String::with_capacity(snake.len());
    let mut capitalize = false;
    for ch in snake.chars() {
        if ch == '_' {
            capitalize = true;
        } else if capitalize {
            camel.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            camel.push(ch);
        }
    }
    let value = params
        .get(snake)
        .or_else(|| params.get(&camel))
        .cloned()
        .unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|error| format!("参数 '{snake}' 无效或缺失: {error}"))
}

fn to_json<T: serde::Serialize>(result: Result<T, String>) -> Result<Value, String> {
    result.and_then(|value| serde_json::to_value(value).map_err(|e| e.to_string()))
}

/// Match arms for the plain synchronous `work_*` commands. Each arm extracts
/// its typed params then invokes the Tauri command function directly (the
/// `#[tauri::command]` attribute keeps the original fn callable).
macro_rules! work_sync {
    ($method:expr, $params:expr, $( $name:literal => $($func:ident)::+ ( $( $arg:ident : $ty:ty ),* ) ),* $(,)?) => {
        match $method {
            $( $name => Some({
                $( let $arg: $ty = param($params, stringify!($arg))?; )*
                to_json($($func)::+($($arg),*))
            }),)*
            _ => None,
        }
    };
}

/// Same as [`work_sync`] for async commands.
macro_rules! work_async {
    ($method:expr, $params:expr, $( $name:literal => $($func:ident)::+ ( $( $arg:ident : $ty:ty ),* ) ),* $(,)?) => {
        match $method {
            $( $name => Some({
                $( let $arg: $ty = param($params, stringify!($arg))?; )*
                to_json($($func)::+($($arg),*).await)
            }),)*
            _ => None,
        }
    };
}

/// Route a `work_*` method. Returns `None` for methods this bridge does not
/// own (the caller falls back to the regular web-server dispatcher).
pub async fn dispatch(
    method: &str,
    params: &Value,
    runtime: &CoreRuntime,
) -> Option<Result<Value, String>> {
    match work_dispatch(method, params, runtime).await {
        Ok(Some(value)) => Some(Ok(value)),
        Ok(None) => {
            if method.starts_with("work_") {
                Some(Err(format!(
                    "Work 命令 '{method}' 尚未接入 headless core 桥接"
                )))
            } else {
                None
            }
        }
        Err(error) => Some(Err(error)),
    }
}

async fn work_dispatch(
    method: &str,
    params: &Value,
    rt: &CoreRuntime,
) -> Result<Option<Value>, String> {
    // ── Plain sync commands ──
    if let Some(result) = work_sync!(
        method,
        params,
        // profile / rules (commands/work/workspaces.rs)
        "work_get_profile" => crate::commands::work::workspaces::work_get_profile(),
        "work_get_rules_info" => crate::commands::work::workspaces::work_get_rules_info(),
        "work_get_context_plan" => crate::commands::work::workspaces::work_get_context_plan(run_id: String),
        "work_save_rules" => crate::commands::work::workspaces::work_save_rules(content: String),
        // workspaces
        "work_create_workspace" => crate::commands::work::workspaces::work_create_workspace(name: String),
        "work_create_workspace_from_folder" => crate::commands::work::workspaces::work_create_workspace_from_folder(folder_path: String, name: Option<String>),
        "work_relink_workspace_folder" => crate::commands::work::workspaces::work_relink_workspace_folder(id: String, folder_path: String),
        "work_list_workspaces" => crate::commands::work::workspaces::work_list_workspaces(),
        "work_list_archived_workspaces" => crate::commands::work::workspaces::work_list_archived_workspaces(),
        "work_get_workspace" => crate::commands::work::workspaces::work_get_workspace(id: String),
        "work_rename_workspace" => crate::commands::work::workspaces::work_rename_workspace(id: String, name: String),
        "work_archive_workspace" => crate::commands::work::workspaces::work_archive_workspace(id: String),
        "work_restore_workspace" => crate::commands::work::workspaces::work_restore_workspace(id: String),
        "work_delete_workspace" => crate::commands::work::workspaces::work_delete_workspace(id: String),
        "work_list_access_roots" => crate::commands::work::workspaces::work_list_access_roots(workspace_id: String),
        "work_add_access_root" => crate::commands::work::workspaces::work_add_access_root(workspace_id: String, path: String, writable: bool),
        "work_set_access_root_writable" => crate::commands::work::workspaces::work_set_access_root_writable(workspace_id: String, path: String, writable: bool),
        "work_remove_access_root" => crate::commands::work::workspaces::work_remove_access_root(workspace_id: String, path: String),
        // files
        "work_list_files" => crate::commands::work::workspaces::work_list_files(workspace_id: String, area: Option<String>),
        "work_import_file" => crate::commands::work::workspaces::work_import_file(workspace_id: String, source_path: String),
        "work_open_file" => crate::commands::work::workspaces::work_open_file(workspace_id: String, path: String),
        "work_read_context_file" => crate::commands::work::workspaces::work_read_context_file(workspace_id: String, path: String),
        "work_save_context_file" => crate::commands::work::workspaces::work_save_context_file(workspace_id: String, path: String, content: String, overwrite: bool),
        "work_remove_context_file" => crate::commands::work::workspaces::work_remove_context_file(workspace_id: String, path: String),
        "work_remove_file" => crate::commands::work::workspaces::work_remove_file(workspace_id: String, path: String),
        "work_cleanup_scratch" => crate::commands::work::workspaces::work_cleanup_scratch(workspace_id: String, retention_days: u32, dry_run: bool),
        "work_open_workspace" => crate::commands::work::workspaces::work_open_workspace(id: String),
        "work_open_directory" => crate::commands::work::workspaces::work_open_directory(workspace_id: String, path: String),
        // library
        "work_list_library_items" => crate::commands::work::workspaces::work_list_library_items(workspace_id: Option<String>, category: Option<crate::work::models::LibraryCategory>, query: Option<String>),
        "work_get_library_item" => crate::commands::work::workspaces::work_get_library_item(id: String, workspace_id: Option<String>),
        "work_save_library_item" => crate::commands::work::workspaces::work_save_library_item(item: crate::work::models::LibraryItem),
        "work_delete_library_item" => crate::commands::work::workspaces::work_delete_library_item(id: String, workspace_id: Option<String>),
        "work_create_library_item_from_artifact" => crate::commands::work::workspaces::work_create_library_item_from_artifact(workspace_id: String, artifact_id: String, title: String, category: crate::work::models::LibraryCategory),
        "work_add_library_file" => crate::commands::work::workspaces::work_add_library_file(workspace_id: Option<String>, source_path: String, title: Option<String>, category: crate::work::models::LibraryCategory, collection: Option<String>, tags: Option<Vec<String>>),
        "work_add_library_directory" => crate::commands::work::workspaces::work_add_library_directory(workspace_id: Option<String>, source_path: String, category: crate::work::models::LibraryCategory, collection: Option<String>, tags: Option<Vec<String>>),
        "work_rename_library_item" => crate::commands::work::workspaces::work_rename_library_item(id: String, workspace_id: Option<String>, title: String),
        // artifacts (commands/work/artifacts.rs)
        "work_list_artifacts" => crate::commands::work::artifacts::work_list_artifacts(workspace_id: String, run_id: Option<String>),
        "work_register_artifact" => crate::commands::work::artifacts::work_register_artifact(workspace_id: String, path: String, title: String, artifact_type: Option<String>, run_id: Option<String>),
        "work_update_office_artifact" => crate::commands::work::artifacts::work_update_office_artifact(workspace_id: String, artifact_id: String, content_base64: String, run_id: Option<String>),
        "work_create_office_artifact" => crate::commands::work::artifacts::work_create_office_artifact(workspace_id: String, path: String, title: String, artifact_type: String, content_base64: String, run_id: Option<String>),
        "work_validate_artifact" => crate::commands::work::artifacts::work_validate_artifact(workspace_id: String, artifact_id: String, run_id: Option<String>),
        "work_deliver" => crate::commands::work::artifacts::work_deliver(workspace_id: String, artifact_id: String, run_id: Option<String>),
        "work_delete_artifact" => crate::commands::work::artifacts::work_delete_artifact(workspace_id: String, artifact_id: String),
        "work_export_artifact" => crate::commands::work::artifacts::work_export_artifact(workspace_id: String, artifact_id: String, destination_path: String, run_id: Option<String>),
        "work_copy_artifact_to_primary" => crate::commands::work::artifacts::work_copy_artifact_to_primary(workspace_id: String, artifact_id: String, run_id: Option<String>),
        "work_set_artifact_storage_mode" => crate::commands::work::artifacts::work_set_artifact_storage_mode(workspace_id: String, mode: crate::work::models::WorkArtifactStorageMode),
        "work_list_standalone_artifacts" => crate::commands::work::artifacts::work_list_standalone_artifacts(run_id: String),
        "work_register_standalone_artifact" => crate::commands::work::artifacts::work_register_standalone_artifact(run_id: String, path: String, title: String, artifact_type: Option<String>),
        "work_update_standalone_office_artifact" => crate::commands::work::artifacts::work_update_standalone_office_artifact(run_id: String, artifact_id: String, content_base64: String),
        "work_create_standalone_office_artifact" => crate::commands::work::artifacts::work_create_standalone_office_artifact(run_id: String, path: String, title: String, artifact_type: String, content_base64: String),
        "work_validate_standalone_artifact" => crate::commands::work::artifacts::work_validate_standalone_artifact(run_id: String, artifact_id: String),
        "work_deliver_standalone_artifact" => crate::commands::work::artifacts::work_deliver_standalone_artifact(run_id: String, artifact_id: String),
        "work_delete_standalone_artifact" => crate::commands::work::artifacts::work_delete_standalone_artifact(run_id: String, artifact_id: String),
        "work_export_standalone_artifact" => crate::commands::work::artifacts::work_export_standalone_artifact(run_id: String, artifact_id: String, destination_path: String),
        "work_get_artifact_acceptance" => crate::commands::work::artifacts::work_get_artifact_acceptance(run_id: String),
        // capabilities (commands/work/capabilities.rs, sync half)
        "work_list_resources" => crate::commands::work::capabilities::work_list_resources(runtime: Option<String>),
        "work_set_resource_enabled" => crate::commands::work::capabilities::work_set_resource_enabled(id: String, enabled: bool),
        "work_discover_capabilities" => crate::commands::work::capabilities::work_discover_capabilities(query: String, limit: Option<usize>),
        "work_list_connectors" => crate::commands::work::capabilities::work_list_connectors(),
        "work_save_connector" => crate::commands::work::capabilities::work_save_connector(name: String, transport: String, command: Option<String>, args: Vec<String>, url: Option<String>, env_vars: Option<HashMap<String, String>>, headers: Option<HashMap<String, String>>),
        "work_toggle_connector" => crate::commands::work::capabilities::work_toggle_connector(name: String, enabled: bool),
        "work_remove_connector" => crate::commands::work::capabilities::work_remove_connector(name: String),
        "work_list_connector_packages" => crate::commands::work::capabilities::work_list_connector_packages(),
        "work_apps_connections" => crate::commands::work::capabilities::work_apps_connections(),
        "work_list_connector_catalog" => crate::commands::work::capabilities::work_list_connector_catalog(),
        "work_validate_connector_package" => crate::commands::work::capabilities::work_validate_connector_package(source: String),
        "work_configure_connector_token" => crate::commands::work::capabilities::work_configure_connector_token(package_id: String, values: HashMap<String, String>),
        "work_install_connector_package" => crate::commands::work::capabilities::work_install_connector_package(source: String),
        "work_trust_connector_package" => crate::commands::work::capabilities::work_trust_connector_package(package_id: String, trusted: bool),
        "work_enable_connector_package" => crate::commands::work::capabilities::work_enable_connector_package(package_id: String, enabled: bool),
        "work_uninstall_connector_package" => crate::commands::work::capabilities::work_uninstall_connector_package(package_id: String),
        "work_is_mcp_adapter_installed" => crate::commands::work::capabilities::work_is_mcp_adapter_installed(),
        "work_get_browser_config" => crate::commands::work::capabilities::work_get_browser_config(),
        "work_save_browser_config" => crate::commands::work::capabilities::work_save_browser_config(provider: String, enabled: bool, max_results: Option<u32>, api_key: Option<String>, endpoint_url: Option<String>, allowed_hosts: Option<Vec<String>>),
        "work_install_browser_adapter" => crate::commands::work::capabilities::work_install_browser_adapter(),
        "work_is_browser_adapter_installed" => crate::commands::work::capabilities::work_is_browser_adapter_installed(),
        "work_app_connect_native" => crate::commands::work::capabilities::work_app_connect_native(app_id: String, token: String, alias: Option<String>, email: Option<String>),
        "work_lark_cli_check" => crate::commands::work::capabilities::work_lark_cli_check(),
        "work_lark_cli_install" => crate::commands::work::capabilities::work_lark_cli_install(),
        "work_lark_skills_mount" => crate::commands::work::capabilities::work_lark_skills_mount(),
        "work_apps_get_provider_config" => crate::commands::work::capabilities::work_apps_get_provider_config(),
        "work_apps_save_provider_config" => crate::commands::work::capabilities::work_apps_save_provider_config(api_key: String),
        "work_app_set_default_account" => crate::commands::work::capabilities::work_app_set_default_account(workspace_id: String, app_id: String, account_id: String),
        "work_get_capability_center_projection" => crate::commands::work::capabilities::work_get_capability_center_projection(runtime: Option<String>),
        "work_search_capabilities" => crate::commands::work::capabilities::work_search_capabilities(query: String, limit: Option<usize>),
        // tasks (commands/work/tasks.rs, sync half)
        "work_create_task" => crate::commands::work::tasks::work_create_task(workspace_id: String, title: String, instructions: String, policy: Option<crate::work::models::WorkPolicy>, schedule: Option<crate::work::models::WorkScheduleConfig>, required_artifacts: Option<Vec<String>>, artifact_requirements: Option<Vec<crate::work::models::WorkArtifactRequirement>>),
        "work_delete_task" => crate::commands::work::tasks::work_delete_task(id: String),
        "work_duplicate_task" => crate::commands::work::tasks::work_duplicate_task(id: String),
        "work_get_task" => crate::commands::work::tasks::work_get_task(id: String),
        "work_list_tasks" => crate::commands::work::tasks::work_list_tasks(workspace_id: Option<String>),
        "work_update_task" => crate::commands::work::tasks::work_update_task(task: crate::work::models::WorkTask),
        "work_set_workspace_default_policy" => crate::commands::work::tasks::work_set_workspace_default_policy(workspace_id: String, policy: crate::work::models::WorkPolicy),
        "work_set_workspace_model_preferences" => crate::commands::work::tasks::work_set_workspace_model_preferences(workspace_id: String, model: Option<String>, effort: Option<String>),
        "work_add_standing_rule" => crate::commands::work::tasks::work_add_standing_rule(task_id: String, rule: crate::work::models::TaskStandingRule),
        "work_get_automation_stats" => crate::commands::work::tasks::work_get_automation_stats(),
        "work_list_task_runs" => crate::commands::work::tasks::work_list_task_runs(task_id: String),
        "work_finish_run" => crate::commands::work::tasks::work_finish_run(task_id: String, run_id: String, status: crate::work::models::WorkRunStatus, error_message: Option<String>, task_state: Option<crate::work::models::WorkTaskState>),
        "work_get_goal_spec" => crate::commands::work::tasks::work_get_goal_spec(task_id: String, run_id: String),
        "work_verify_goal" => crate::commands::work::tasks::work_verify_goal(task_id: String, run_id: String),
        "work_list_runs" => crate::commands::work::conversations::work_list_runs(task_id: String),
        // conversations / interactions (commands/work/conversations.rs, sync half)
        "work_get_run" => crate::commands::work::conversations::work_get_run(task_id: String, run_id: String),
        "work_get_run_receipt" => crate::commands::work::conversations::work_get_run_receipt(task_id: String, run_id: String),
        "work_get_standalone_run_receipt" => crate::commands::work::conversations::work_get_standalone_run_receipt(run_id: String),
        "work_get_run_recovery" => crate::commands::work::conversations::work_get_run_recovery(workspace_id: String, task_id: String, run_id: String),
        "work_follow_up" => crate::commands::work::conversations::work_follow_up(task_id: String, run_id: String, instruction: String),
        "work_inject_context" => crate::commands::work::conversations::work_inject_context(task_id: String, run_id: String, content: String),
        "work_assign_session_workspace" => crate::commands::work::conversations::work_assign_session_workspace(run_id: String, workspace_id: String),
        "work_get_inbox_item" => crate::commands::work::interactions::work_get_inbox_item(id: String),
        "work_get_inbox_session_run" => crate::commands::work::interactions::work_get_inbox_session_run(id: String),
        "work_list_inbox_items" => crate::commands::work::interactions::work_list_inbox_items(only_pending: Option<bool>, task_id: Option<String>),
        "work_evaluate_tool_risk" => crate::commands::work::interactions::work_evaluate_tool_risk(tool_name: String),
        "work_check_confirmation_required" => crate::commands::work::interactions::work_check_confirmation_required(policy: crate::work::models::WorkPolicy, tool_name: String, target: String)
    ) {
        return result.map(Some);
    }

    // ── Plain async commands ──
    if let Some(result) = work_async!(
        method,
        params,
        // capabilities (commands/work/capabilities.rs, async half)
        "work_install_community_skill" => crate::commands::work::capabilities::work_install_community_skill(source: String, skill_id: String),
        "work_import_skill_zip" => crate::commands::work::capabilities::work_import_skill_zip(zip_path: String, slug: String),
        "work_install_mcp_adapter" => crate::commands::work::capabilities::work_install_mcp_adapter(),
        "work_test_connector" => crate::commands::work::capabilities::work_test_connector(name: String),
        "work_test_browser" => crate::commands::work::capabilities::work_test_browser(),
        "work_apps_catalog" => crate::commands::work::capabilities::work_apps_catalog(),
        "work_app_authorize" => crate::commands::work::capabilities::work_app_authorize(app_id: String, redirect_url: Option<String>, user_id: Option<String>),
        "work_start_connector_auth" => crate::commands::work::capabilities::work_start_connector_auth(package_id: String, account_id: Option<String>),
        "work_app_status" => crate::commands::work::capabilities::work_app_status(app_id: String, connection_id: Option<String>),
        "work_app_disconnect" => crate::commands::work::capabilities::work_app_disconnect(app_id: String, account_id: Option<String>),
        "work_lark_auth_start" => crate::commands::work::capabilities::work_lark_auth_start(alias: Option<String>, email: Option<String>),
        "work_lark_auth_status" => crate::commands::work::capabilities::work_lark_auth_status(task_id: Option<String>),
        "work_get_run_effective_capabilities" => crate::commands::work::capabilities::work_get_run_effective_capabilities(run_id: String),
        // sessions read paths (commands/work/conversations.rs, async half)
        "work_get_session" => crate::commands::work::conversations::work_get_session(workspace_id: String),
        "work_list_sessions" => crate::commands::work::conversations::work_list_sessions(workspace_id: String),
        "work_list_recent_sessions" => crate::commands::work::conversations::work_list_recent_sessions(limit: Option<usize>),
        "work_list_archived_sessions" => crate::commands::work::conversations::work_list_archived_sessions(limit: Option<usize>),
        "work_list_standalone_sessions" => crate::commands::work::conversations::work_list_standalone_sessions(),
        "work_get_projection" => crate::commands::work::conversations::work_get_projection(run_id: String),
        "work_get_run_progress" => crate::commands::work::conversations::work_get_run_progress(task_id: String, run_id: String),
        "work_list_subagents" => crate::commands::work::conversations::work_list_subagents(parent_scope: String, task_id: Option<String>),
        "work_delete_inbox_item" => crate::commands::work::interactions::work_delete_inbox_item(id: String),
        "work_clear_inbox_items" => crate::commands::work::interactions::work_clear_inbox_items(workspace_id: Option<String>, only_resolved: Option<bool>)
    ) {
        return result.map(Some);
    }

    // ── Stateful commands: session lifecycle, runs, steering, recovery ──
    // These mirror the `#[tauri::command]` wrappers, passing the state held by
    // CoreRuntime instead of `tauri::State` extracts. Each arm returns
    // `Result<Option<Value>, String>` via `to_json(...).map(Some)`.
    match method {
        "work_start_session" => {
            let workspace_id: String = param(params, "workspace_id")?;
            let message: String = param(params, "message")?;
            let model: Option<String> = param(params, "model")?;
            let attachments: Option<Vec<crate::agent::session_actor::AttachmentData>> =
                param(params, "attachments")?;
            let preset: Option<crate::work::models::WorkPreset> = param(params, "preset")?;
            let runtime: Option<String> = param(params, "runtime")?;
            let runtime_kind = runtime
                .as_deref()
                .map(RuntimeProviderKind::try_from_agent_str)
                .transpose()?;
            to_json(
                crate::work::session::start(
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &workspace_id,
                    &message,
                    model,
                    attachments,
                    preset.unwrap_or_default(),
                    runtime_kind,
                )
                .await,
            )
            .map(Some)
        }
        "work_start_standalone_session" => {
            let message: String = param(params, "message")?;
            let model: Option<String> = param(params, "model")?;
            let attachments: Option<Vec<crate::agent::session_actor::AttachmentData>> =
                param(params, "attachments")?;
            let permission_mode: Option<crate::work::models::WorkExecutionMode> =
                param(params, "permission_mode")?;
            let preset: Option<crate::work::models::WorkPreset> = param(params, "preset")?;
            let runtime: Option<String> = param(params, "runtime")?;
            let runtime_kind = runtime
                .as_deref()
                .map(RuntimeProviderKind::try_from_agent_str)
                .transpose()?;
            to_json(
                crate::work::session::start_standalone(
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &message,
                    model,
                    attachments,
                    permission_mode,
                    preset.unwrap_or_default(),
                    runtime_kind,
                )
                .await,
            )
            .map(Some)
        }
        "work_resume_session" => {
            let run_id: String = param(params, "run_id")?;
            let message: Option<String> = param(params, "message")?;
            let attachments: Option<Vec<crate::agent::session_actor::AttachmentData>> =
                param(params, "attachments")?;
            to_json(
                crate::work::session::resume(
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &run_id,
                    message.as_deref(),
                    attachments,
                )
                .await,
            )
            .map(Some)
        }
        "work_send_message" => {
            let run_id: String = param(params, "run_id")?;
            let message: String = param(params, "message")?;
            let attachments: Option<Vec<crate::agent::session_actor::AttachmentData>> =
                param(params, "attachments")?;
            to_json(
                crate::work::session::send_message(
                    &rt.emitter,
                    &rt.sessions,
                    &run_id,
                    &message,
                    attachments,
                )
                .await,
            )
            .map(Some)
        }
        "cancel_session_turn" => {
            let run_id: String = param(params, "run_id")?;
            to_json(crate::commands::session::cancel_session_turn_inner(&rt.sessions, run_id).await)
                .map(Some)
        }
        "work_stop_session" => {
            let run_id: String = param(params, "run_id")?;
            to_json(
                crate::work::session::stop(&rt.emitter, &rt.sessions, &rt.spawn_locks, &run_id)
                    .await,
            )
            .map(Some)
        }
        "work_continue_session" => {
            let workspace_id: String = param(params, "workspace_id")?;
            let run_id: String = param(params, "run_id")?;
            let anchor_id: Option<String> = param(params, "anchor_id")?;
            let model: Option<String> = param(params, "model")?;
            to_json(
                crate::work::session::continue_session(
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &workspace_id,
                    &run_id,
                    anchor_id.as_deref(),
                    model,
                )
                .await,
            )
            .map(Some)
        }
        "work_start_run" => {
            let task_id: String = param(params, "task_id")?;
            let trigger: Option<crate::work::models::WorkRunTrigger> = param(params, "trigger")?;
            to_json(
                crate::commands::work::tasks::start_work_task_run(
                    &WorkPaths::app(),
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &task_id,
                    trigger.unwrap_or(crate::work::models::WorkRunTrigger::Manual),
                    crate::work::models::ExecutionContext::Attended,
                    None,
                )
                .await,
            )
            .map(Some)
        }
        "work_retry_task_run" => {
            let _guard = crate::commands::work::INBOX_DELIVERY_LOCK.lock().await;
            let task_id: String = param(params, "task_id")?;
            let run_id: String = param(params, "run_id")?;
            to_json(
                crate::commands::work::tasks::retry_failed_work_run(
                    &WorkPaths::app(),
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &task_id,
                    &run_id,
                )
                .await,
            )
            .map(Some)
        }
        "work_trigger_goal_repair" => {
            let _guard = crate::commands::work::INBOX_DELIVERY_LOCK.lock().await;
            let task_id: String = param(params, "task_id")?;
            let run_id: String = param(params, "run_id")?;
            to_json(
                crate::commands::work::tasks::trigger_goal_repair_impl(
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    task_id,
                    run_id,
                )
                .await,
            )
            .map(Some)
        }
        "work_recover_run" => {
            let _guard = crate::commands::work::INBOX_DELIVERY_LOCK.lock().await;
            let workspace_id: String = param(params, "workspace_id")?;
            let task_id: String = param(params, "task_id")?;
            let run_id: String = param(params, "run_id")?;
            let action: String = param(params, "action")?;
            let subagent_id: Option<String> = param(params, "subagent_id")?;
            to_json(
                crate::commands::work::conversations::recover_work_run_locked(
                    &WorkPaths::app(),
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &workspace_id,
                    &task_id,
                    &run_id,
                    crate::commands::work::conversations::parse_work_recovery_action(&action)?,
                    subagent_id.as_deref(),
                )
                .await,
            )
            .map(Some)
        }
        "work_verify_recovered_output" => {
            let _guard = crate::commands::work::INBOX_DELIVERY_LOCK.lock().await;
            let workspace_id: String = param(params, "workspace_id")?;
            let task_id: String = param(params, "task_id")?;
            let run_id: String = param(params, "run_id")?;
            to_json(
                crate::commands::work::conversations::recover_work_run_locked(
                    &WorkPaths::app(),
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &workspace_id,
                    &task_id,
                    &run_id,
                    WorkRecoveryAction::Verify,
                    None,
                )
                .await,
            )
            .map(Some)
        }
        "work_retry_subagent" => {
            let _guard = crate::commands::work::INBOX_DELIVERY_LOCK.lock().await;
            let workspace_id: String = param(params, "workspace_id")?;
            let task_id: String = param(params, "task_id")?;
            let run_id: String = param(params, "run_id")?;
            let subagent_id: String = param(params, "subagent_id")?;
            to_json(
                crate::commands::work::conversations::recover_work_run_locked(
                    &WorkPaths::app(),
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    &workspace_id,
                    &task_id,
                    &run_id,
                    WorkRecoveryAction::RetrySubagent,
                    Some(&subagent_id),
                )
                .await,
            )
            .map(Some)
        }
        "work_steer" => {
            let task_id: String = param(params, "task_id")?;
            let run_id: String = param(params, "run_id")?;
            let instruction: String = param(params, "instruction")?;
            to_json(
                crate::commands::work::conversations::work_steer_impl(
                    &rt.sessions,
                    task_id,
                    run_id,
                    instruction,
                )
                .await,
            )
            .map(Some)
        }
        "work_resolve_pending_approval" => {
            let run_id: String = param(params, "run_id")?;
            let decision: String = param(params, "decision")?;
            to_json(
                crate::commands::work::interactions::resolve_pending_approval_impl(
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    run_id,
                    decision,
                )
                .await,
            )
            .map(Some)
        }
        "work_resolve_inbox_item" => {
            let id: String = param(params, "id")?;
            let status: crate::work::models::InboxItemStatus = param(params, "status")?;
            let response: Option<Value> = param(params, "response")?;
            to_json(
                crate::commands::work::interactions::resolve_inbox_item_impl(
                    &rt.emitter,
                    &rt.sessions,
                    &rt.spawn_locks,
                    &rt.cancel_token,
                    id,
                    status,
                    response,
                )
                .await,
            )
            .map(Some)
        }
        "work_create_inbox_item" => {
            let task_id: String = param(params, "task_id")?;
            let run_id: String = param(params, "run_id")?;
            let workspace_id: String = param(params, "workspace_id")?;
            let item_type: crate::work::models::InboxItemType = param(params, "item_type")?;
            let title: String = param(params, "title")?;
            let description: String = param(params, "description")?;
            let payload: crate::work::models::InboxItemPayload = param(params, "payload")?;
            to_json(crate::commands::work::interactions::create_inbox_item_impl(
                task_id,
                run_id,
                workspace_id,
                item_type,
                title,
                description,
                payload,
            ))
            .map(Some)
        }
        _ => Ok(None),
    }
}
