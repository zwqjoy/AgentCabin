pub mod agent;
pub mod browser_runtime;
pub mod code_connector_runtime;
pub mod commands;
pub mod core;
pub mod desktop_runtime;
pub mod hooks;
pub mod models;
pub mod pet;
pub mod pi_context_runtime;
pub mod pricing;
pub mod process_ext;
pub mod storage;
pub mod web_server;
pub mod work;

use agent::adapter::{new_actor_session_map, ActorSessionMap};
use agent::codex_control::CodexInfoCache;
use agent::control::CliInfoCache;
use agent::spawn_locks::SpawnLocks;
use agent::stream::new_process_map;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::Arc;
use storage::events::EventWriter;
use tauri::tray::TrayIconEvent;
use tauri::Manager;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Effective web server port (may differ from configured port if busy)
pub type EffectiveWebPort = Arc<AtomicU16>;
/// Web-server-specific cancel token for restart support
pub type WebServerCancel = Arc<tokio::sync::Mutex<CancellationToken>>;
/// Token version — shared between IPC and web server for rotation detection
pub type SharedTokenVersion = Arc<AtomicU64>;
/// WS shutdown broadcast — token rotation triggers disconnect of all WS clients
pub type WsShutdownSender = Arc<broadcast::Sender<()>>;
/// Live token — hot-swappable via RwLock for immediate login/logout on rotation
pub type SharedLiveToken = Arc<tokio::sync::RwLock<String>>;
/// Mutex to serialize web server start/stop operations
pub type WebServerLock = Arc<tokio::sync::Mutex<()>>;
/// JoinHandle for the serve task — await during stop to ensure port release
pub type WebServerHandle = Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>;
/// Generation counter — each spawn_server increments; stale tasks check before cleanup.
/// Newtype to avoid Tauri manage() collision with SharedTokenVersion (both Arc<AtomicU64>).
#[derive(Clone)]
pub struct WebServerGeneration(pub Arc<AtomicU64>);
/// Effective bind address — reflects actual running state (not settings).
/// Newtype to avoid Tauri manage() collision with SharedLiveToken (both Arc<RwLock<String>>).
#[derive(Clone)]
pub struct EffectiveWebBind(pub Arc<tokio::sync::RwLock<String>>);
/// Startup warning — populated when origins are degraded or other non-fatal startup issues.
#[derive(Clone)]
pub struct WebServerWarning(pub Arc<tokio::sync::RwLock<Option<String>>>);

/// One-shot gate to prevent concurrent shutdown tasks.
/// CAS ensures only the first caller proceeds; subsequent quit/close events are no-ops.
pub struct ShutdownGate(AtomicBool);

impl Default for ShutdownGate {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownGate {
    pub fn new() -> Self {
        Self(AtomicBool::new(false))
    }
    /// Returns `true` if this call entered the gate (first caller wins).
    pub fn try_enter(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

pub fn run() {
    // Initialize logging — our crate at debug level by default
    // Override with RUST_LOG env var, e.g. RUST_LOG=warn cargo tauri dev
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("agentcabin_desktop_lib=debug,warn"),
    )
    .format_timestamp_millis()
    .init();

    log::info!("AgentCabin Desktop starting");

    // Set up Windows Job Object so child processes are killed on crash/force-quit.
    // No-op on non-Windows.
    process_ext::setup_job_kill_on_close();

    // Reconcile orphaned runs on startup
    storage::runs::reconcile_orphaned_runs();
    if let Err(error) = work::tasks::reconcile_active_runs(&work::paths::WorkPaths::app()) {
        // Do not hide a recovery read/write failure. The affected WorkRun is
        // deliberately left non-terminal and the progress/recovery commands
        // will surface the same error instead of claiming a false completion.
        log::error!("[work/lifecycle] Startup reconciliation failed: {error}");
    }
    if let Err(error) = storage::profile_bindings::migrate_legacy_pi_code_profile_if_needed() {
        log::warn!("[pi/profile] Legacy Code Pi profile migration skipped: {error}");
    }
    storage::profile_bindings::migrate_legacy_skills_if_needed().ok();
    if let Err(error) = work::twork_migration::sync_from_twork_if_present() {
        log::warn!("[twork/sync] Startup sync from T-Work skipped or partial: {error}");
    }

    // Start internal loopback-only authenticated Work bridge
    tauri::async_runtime::spawn(async {
        if let Err(e) = work::internal_bridge::start_internal_bridge().await {
            log::error!("[work/bridge] Failed to start internal bridge: {e}");
        }
    });

    // Shared Code/Work Browser Runtime bridge. Work still routes
    // calls through its own Harness ToolPipeline; Code uses this bridge
    // directly with per-session bearer tokens.
    tauri::async_runtime::spawn(async {
        if let Err(e) = browser_runtime::start_bridge().await {
            log::error!("[browser/bridge] Failed to start browser bridge: {e}");
        }
    });

    // Code's native desktop tools use the same loopback/bearer-token pattern
    // as Browser Runtime. Work continues to authorize the same operator via
    // its Work Tool Pipeline instead of this bridge.
    tauri::async_runtime::spawn(async {
        if let Err(e) = desktop_runtime::start_bridge().await {
            log::error!("[desktop/bridge] Failed to start desktop bridge: {e}");
        }
    });

    // Code Connector Package CLI bridge. Work continues to use its separate
    // authenticated Harness bridge and policy pipeline.
    tauri::async_runtime::spawn(async {
        if let Err(e) = code_connector_runtime::start_bridge().await {
            log::error!("[code/connector-bridge] Failed to start Code Connector bridge: {e}");
        }
    });

    // Clean up legacy hook-bridge (removed: was redundant with stream-json mode)
    hooks::setup::cleanup_hook_bridge();

    // Global cancellation token — shared with all session actors for graceful shutdown
    let cancel_token = CancellationToken::new();
    let cancel_for_exit = cancel_token.clone();

    // Shared flag: true if system tray was successfully created
    let tray_ok = Arc::new(AtomicBool::new(false));
    let tray_ok_for_event = tray_ok.clone();

    // Web server shared state
    let ws_shutdown_sender: WsShutdownSender = Arc::new(broadcast::channel::<()>(1).0);
    let shared_token_version: SharedTokenVersion = Arc::new(AtomicU64::new(0));
    let shared_live_token: SharedLiveToken = {
        use rand::Rng;
        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        log::debug!("[app] ephemeral web token generated (masked)");
        Arc::new(tokio::sync::RwLock::new(token))
    };
    let effective_web_port: EffectiveWebPort = Arc::new(AtomicU16::new(0));
    let ws_cancel: WebServerCancel = Arc::new(tokio::sync::Mutex::new(CancellationToken::new()));
    let ws_lock: WebServerLock = Arc::new(tokio::sync::Mutex::new(()));
    let ws_handle: WebServerHandle = Arc::new(tokio::sync::Mutex::new(None));
    let ws_generation = WebServerGeneration(Arc::new(AtomicU64::new(0)));
    let ws_effective_bind = EffectiveWebBind(Arc::new(tokio::sync::RwLock::new(String::new())));
    let ws_warning = WebServerWarning(Arc::new(tokio::sync::RwLock::new(None)));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .manage(new_process_map())
        .manage(new_actor_session_map())
        .manage(CliInfoCache::new())
        .manage(CodexInfoCache::new())
        // Managed writer = the process-wide singleton, so the free-function
        // append_event path shares its per-run locks + seq source (audit #1).
        .manage(crate::storage::events::global_writer())
        .manage(SpawnLocks::new())
        .manage(ShutdownGate::new())
        .manage(cancel_token)
        .manage(ws_shutdown_sender)
        .manage(shared_token_version)
        .manage(shared_live_token)
        .manage(effective_web_port)
        .manage(ws_cancel)
        .manage(ws_lock)
        .manage(ws_handle)
        .manage(ws_generation)
        .manage(ws_effective_bind)
        .manage(ws_warning)
        .manage(commands::pty::global_registry())
        // NOTE: Currently ~60 IPC commands. If approaching 80+, consider grouping
        // into Tauri command modules or using a single dispatch command with typed payloads.
        .invoke_handler(tauri::generate_handler![
            commands::runs::list_runs,
            commands::runs::get_run,
            commands::runs::start_run,
            commands::runs::stop_run,
            commands::runs::update_run_model,
            commands::runs::update_run_effort,
            commands::runs::update_run_permission_mode,
            commands::runs::rename_run,
            commands::runs::delete_runs,
            commands::runs::set_run_flags,
            commands::runs::reveal_in_finder,
            commands::runs::search_prompts,
            commands::history::search_runs,
            commands::history::get_run_files,
            commands::runs::add_prompt_favorite,
            commands::runs::remove_prompt_favorite,
            commands::runs::update_prompt_favorite_tags,
            commands::runs::update_prompt_favorite_note,
            commands::runs::list_prompt_favorites,
            commands::runs::list_prompt_tags,
            commands::chat::send_chat_message,
            commands::events::get_run_events,
            commands::artifacts::get_run_artifacts,
            commands::settings::get_user_settings,
            commands::settings::update_user_settings,
            commands::settings::get_project_preferences,
            commands::settings::update_project_preferences,
            commands::settings::get_agent_settings,
            commands::settings::update_agent_settings,
            commands::pets::list_custom_pets,
            commands::pets::get_custom_pet,
            commands::pets::inspect_custom_pet_zip,
            commands::pets::inspect_custom_pet_image,
            commands::pets::create_custom_pet,
            commands::pets::open_custom_pets_folder,
            commands::fs::list_directory,
            commands::fs::check_is_directory,
            commands::fs::check_path_exists,
            commands::fs::read_file_base64,
            commands::remote_fs::list_remote_directory,
            commands::remote_fs::resolve_remote_home,
            commands::git::get_git_summary,
            commands::git::get_git_branch,
            commands::git::list_git_branches,
            commands::git::checkout_git_branch,
            commands::git::create_git_branch,
            commands::git::run_terminal_command,
            commands::git::get_git_diff,
            commands::git::get_git_status,
            commands::git::create_git_turn_snapshot,
            commands::git::diff_git_turn_snapshot,
            commands::git::apply_git_turn_patch,
            commands::git::persist_turn_file_summary,
            commands::worktrees::get_git_project,
            commands::worktrees::list_git_worktrees,
            commands::worktrees::create_git_worktree,
            commands::worktrees::remove_git_worktree,
            commands::work::work_get_profile,
            commands::work::work_get_rules_info,
            commands::work::work_get_context_plan,
            commands::work::work_save_rules,
            commands::work::work_create_workspace,
            commands::work::work_create_workspace_from_folder,
            commands::work::work_relink_workspace_folder,
            commands::work::work_list_workspaces,
            commands::work::work_list_archived_workspaces,
            commands::work::work_get_workspace,
            commands::work::work_rename_workspace,
            commands::work::work_archive_workspace,
            commands::work::work_restore_workspace,
            commands::work::work_delete_workspace,
            commands::work::work_list_access_roots,
            commands::work::work_add_access_root,
            commands::work::work_set_access_root_writable,
            commands::work::work_remove_access_root,
            commands::work::work_list_resources,
            commands::work::work_set_resource_enabled,
            commands::work::work_install_community_skill,
            commands::work::work_import_skill_zip,
            commands::work::work_discover_capabilities,
            commands::work::work_get_capability_center_projection,
            commands::work::work_search_capabilities,
            commands::work::work_get_run_effective_capabilities,
            commands::work::work_list_connectors,
            commands::work::work_save_connector,
            commands::work::work_toggle_connector,
            commands::work::work_remove_connector,
            commands::work::work_list_connector_packages,
            commands::work::work_list_connector_catalog,
            commands::work::work_validate_connector_package,
            commands::work::work_install_connector_package,
            commands::work::work_trust_connector_package,
            commands::work::work_enable_connector_package,
            commands::work::work_uninstall_connector_package,
            commands::work::work_install_mcp_adapter,
            commands::work::work_is_mcp_adapter_installed,
            commands::work::work_test_connector,
            commands::work::work_get_browser_config,
            commands::work::work_save_browser_config,
            commands::work::work_test_browser,
            commands::work::work_install_browser_adapter,
            commands::work::work_is_browser_adapter_installed,
            commands::work::work_apps_catalog,
            commands::work::work_apps_connections,
            commands::work::work_apps_get_provider_config,
            commands::work::work_apps_save_provider_config,
            commands::work::work_app_authorize,
            commands::work::work_app_connect_native,
            commands::work::work_lark_cli_check,
            commands::work::work_lark_cli_install,
            commands::work::work_lark_skills_mount,
            commands::work::work_lark_auth_start,
            commands::work::work_lark_auth_status,
            commands::work::work_start_connector_auth,
            commands::work::work_configure_connector_token,
            commands::work::work_app_status,
            commands::work::work_app_disconnect,
            commands::work::work_app_set_default_account,
            commands::work::work_list_files,
            commands::work::work_import_file,
            commands::work::work_open_file,
            commands::work::work_read_context_file,
            commands::work::work_save_context_file,
            commands::work::work_remove_context_file,
            commands::work::work_remove_file,
            commands::work::work_cleanup_scratch,
            commands::work::work_open_workspace,
            commands::work::work_open_directory,
            commands::work::work_list_artifacts,
            commands::work::work_register_artifact,
            commands::work::work_update_office_artifact,
            commands::work::work_create_office_artifact,
            commands::work::work_validate_artifact,
            commands::work::work_deliver,
            commands::work::work_delete_artifact,
            commands::work::work_export_artifact,
            commands::work::work_copy_artifact_to_primary,
            commands::work::work_set_artifact_storage_mode,
            commands::work::work_list_standalone_artifacts,
            commands::work::work_register_standalone_artifact,
            commands::work::work_update_standalone_office_artifact,
            commands::work::work_create_standalone_office_artifact,
            commands::work::work_validate_standalone_artifact,
            commands::work::work_deliver_standalone_artifact,
            commands::work::work_delete_standalone_artifact,
            commands::work::work_export_standalone_artifact,
            commands::work::work_get_artifact_acceptance,
            commands::work::work_get_session,
            commands::work::work_list_sessions,
            commands::work::work_start_session,
            commands::work::work_start_standalone_session,
            commands::work::work_list_standalone_sessions,
            commands::work::work_list_recent_sessions,
            commands::work::work_resume_session,
            commands::work::work_send_message,
            commands::work::work_stop_session,
            commands::work::work_continue_session,
            commands::work::work_create_task,
            commands::work::work_delete_task,
            commands::work::work_duplicate_task,
            commands::work::work_get_task,
            commands::work::work_list_tasks,
            commands::work::work_update_task,
            commands::work::work_set_workspace_default_policy,
            commands::work::work_set_workspace_model_preferences,
            commands::work::work_add_standing_rule,
            commands::work::work_start_run,
            commands::work::work_retry_task_run,
            commands::work::work_finish_run,
            commands::work::work_resolve_pending_approval,
            commands::work::work_list_runs,
            commands::work::work_list_task_runs,
            commands::work::work_get_automation_stats,
            commands::work::work_get_run,
            commands::work::work_get_projection,
            commands::work::work_get_run_progress,
            commands::work::work_get_run_receipt,
            commands::work::work_get_standalone_run_receipt,
            commands::work::work_get_goal_spec,
            commands::work::work_verify_goal,
            commands::work::work_trigger_goal_repair,
            commands::work::work_get_run_recovery,
            commands::work::work_recover_run,
            commands::work::work_verify_recovered_output,
            commands::work::work_retry_subagent,
            commands::work::work_list_subagents,
            commands::work::work_create_inbox_item,
            commands::work::work_get_inbox_item,
            commands::work::work_get_inbox_session_run,
            commands::work::work_list_inbox_items,
            commands::work::work_resolve_inbox_item,
            commands::work::work_delete_inbox_item,
            commands::work::work_clear_inbox_items,
            commands::work::work_assign_session_workspace,
            commands::work::work_steer,
            commands::work::work_follow_up,
            commands::work::work_inject_context,
            commands::work::work_list_library_items,
            commands::work::work_get_library_item,
            commands::work::work_save_library_item,
            commands::work::work_delete_library_item,
            commands::work::work_create_library_item_from_artifact,
            commands::work::work_add_library_file,
            commands::work::work_add_library_directory,
            commands::work::work_rename_library_item,
            commands::work::work_evaluate_tool_risk,
            commands::work::work_check_confirmation_required,
            commands::export::export_conversation,
            commands::export::write_html_export,
            commands::files::agents_md_exists,
            commands::files::read_text_file,
            commands::files::stat_text_file,
            commands::files::write_text_file,
            commands::files::read_task_output,
            commands::files::list_memory_files,
            commands::stats::get_usage_overview,
            commands::stats::get_global_usage_overview,
            commands::stats::clear_usage_cache,
            commands::stats::get_heatmap_daily,
            commands::stats::get_changelog,
            commands::diagnostics::check_agent_cli,
            commands::diagnostics::resolve_capabilities_diagnostics,
            commands::diagnostics::check_codex_auth,
            commands::diagnostics::get_chatgpt_subscription_rate_limits,
            commands::diagnostics::run_codex_doctor,
            commands::diagnostics::test_remote_host,
            commands::diagnostics::get_cli_dist_tags,
            commands::diagnostics::check_project_init,
            commands::diagnostics::check_ssh_key,
            commands::diagnostics::generate_ssh_key,
            commands::diagnostics::run_diagnostics,
            commands::diagnostics::detect_local_proxy,
            commands::diagnostics::test_api_connectivity,
            commands::diagnostics::test_global_provider,
            commands::diagnostics::list_global_provider_models,
            commands::diagnostics::test_pi_provider,
            commands::editors::check_vscode_available,
            commands::editors::open_project_in_vscode,
            commands::session_dispatch::start_session,
            commands::session_dispatch::send_session_message,
            commands::session_dispatch::steer_session_message,
            commands::session_dispatch::cancel_session_turn,
            commands::session_dispatch::stop_session,
            commands::session_dispatch::send_session_control,
            commands::session::broadcast_mcp_toggle,
            commands::session::get_bus_events,
            commands::session::fork_session,
            commands::session::fork_pi_session_at,
            commands::session::clone_pi_session,
            commands::session::get_continuation_context,
            commands::session::copy_run_history,
            commands::session::switch_agent,
            commands::session::side_question,
            commands::session::start_ralph_loop,
            commands::session::cancel_ralph_loop,
            commands::session::approve_session_tool,
            commands::session::cancel_control_request,
            commands::session::respond_permission,
            commands::session::respond_hook_callback,
            commands::session::respond_elicitation,
            commands::session::get_pi_extension_ui_state,
            commands::session::respond_user_input,
            commands::control::get_cli_info,
            commands::control::get_codex_models,
            commands::control::get_pi_models,
            commands::control::get_grok_status,
            commands::control::get_grok_models,
            commands::control::get_grok_cli_config,
            commands::control::update_grok_cli_config,
            commands::teams::list_teams,
            commands::teams::get_team_config,
            commands::teams::list_team_tasks,
            commands::teams::get_team_task,
            commands::teams::get_team_inbox,
            commands::teams::get_all_team_inboxes,
            commands::teams::delete_team,
            commands::plugins::list_marketplaces,
            commands::plugins::list_marketplace_plugins,
            commands::plugins::list_standalone_skills,
            commands::plugins::list_grok_skills,
            commands::plugins::list_project_commands,
            commands::plugins::get_skill_content,
            commands::claude_plugins::list_claude_installed_plugins,
            commands::claude_plugins::list_claude_available_plugins,
            commands::claude_plugins::list_claude_marketplaces,
            commands::claude_plugins::ensure_claude_official_marketplace,
            commands::claude_plugins::sync_claude_official_marketplace,
            commands::pi_extensions::list_pi_installed_plugins,
            commands::pi_extensions::list_pi_profile_extensions,
            commands::pi_extensions::get_pi_profile_info,
            commands::pi_extensions::install_pi_extension,
            commands::pi_extensions::uninstall_pi_extension,
            commands::pi_extensions::update_pi_extension,
            commands::pi_extensions::toggle_pi_extension,
            commands::pi_extensions::install_pi_profile_extension,
            commands::pi_extensions::uninstall_pi_profile_extension,
            commands::pi_extensions::update_pi_profile_extension,
            commands::pi_extensions::toggle_pi_profile_extension,
            commands::pi_extensions::list_pi_shared_extensions,
            commands::pi_extensions::install_pi_shared_extension,
            commands::pi_extensions::uninstall_pi_shared_extension,
            commands::pi_extensions::update_pi_shared_extension,
            commands::pi_extensions::toggle_pi_shared_extension,
            commands::agent_plugins::list_agent_plugins,
            commands::agent_plugins::install_agent_plugin,
            commands::agent_plugins::update_agent_plugin,
            commands::agent_plugins::uninstall_agent_plugin,
            commands::agent_plugins::set_agent_plugin_trust,
            commands::agent_plugins::get_agent_plugin_bindings,
            commands::agent_plugins::set_agent_plugin_binding,
            commands::agent_plugins::sync_from_twork,
            commands::plugins::install_plugin,
            commands::plugins::uninstall_plugin,
            commands::plugins::enable_plugin,
            commands::plugins::disable_plugin,
            commands::plugins::update_plugin,
            commands::plugins::add_marketplace,
            commands::plugins::remove_marketplace,
            commands::plugins::update_marketplace,
            commands::plugins::create_skill,
            commands::plugins::create_grok_skill,
            commands::plugins::update_skill,
            commands::plugins::delete_skill,
            commands::plugins::list_codex_skills,
            commands::plugins::create_codex_skill,
            commands::plugins::delete_codex_skill,
            commands::plugins::toggle_codex_skill,
            commands::dsh_plugins::list_dsh_plugins,
            commands::dsh_plugins::toggle_dsh_plugin,
            commands::dsh_plugins::register_dsh_plugin,
            commands::dsh_plugins::unregister_dsh_plugin,
            commands::dsh_plugins::update_dsh_plugin,
            commands::prompt_templates::list_prompt_templates,
            commands::prompt_templates::create_prompt_template,
            commands::prompt_templates::update_prompt_template,
            commands::prompt_templates::delete_prompt_template,
            commands::capabilities::list_skills,
            commands::capabilities::create_shared_skill,
            commands::capabilities::delete_shared_skill,
            commands::capabilities::toggle_skill_binding,
            commands::capabilities::get_skill_bindings,
            commands::capabilities::list_pi_skills,
            commands::capabilities::list_pi_commands,
            commands::capabilities::create_pi_skill,
            commands::capabilities::delete_pi_skill,
            commands::capabilities::toggle_pi_skill_binding,
            commands::capabilities::get_pi_skill_bindings,
            commands::capabilities::get_web_access_binding,
            commands::capabilities::set_web_access_binding,
            commands::capabilities::get_browser_config,
            commands::capabilities::save_browser_config,
            commands::capabilities::test_browser,
            commands::capabilities::get_browser_use_binding,
            commands::capabilities::set_browser_use_binding,
            commands::capabilities::get_desktop_use_status,
            commands::capabilities::refresh_desktop_use_status,
            commands::capabilities::request_desktop_use_permissions,
            commands::capabilities::open_desktop_permission_pane,
            commands::capabilities::get_desktop_use_binding,
            commands::capabilities::set_desktop_use_binding,
            commands::capabilities::prepare_browser_runtime,
            commands::capabilities::get_browser_session,
            commands::capabilities::list_browser_sessions,
            commands::capabilities::control_browser_session,
            commands::capabilities::browser_user_interact,
            commands::capabilities::get_browser_traces,
            commands::capabilities::register_embedded_browser,
            commands::capabilities::unregister_embedded_browser,
            commands::capabilities::list_embedded_browsers,
            commands::pi_extensions::list_mcp_catalog,
            commands::pi_extensions::save_mcp_catalog_server,
            commands::pi_extensions::delete_mcp_catalog_server,
            commands::pi_extensions::get_mcp_bindings,
            commands::pi_extensions::set_mcp_binding,
            commands::pi_extensions::list_connector_catalog,
            commands::pi_extensions::save_connector_catalog_item,
            commands::pi_extensions::delete_connector_catalog_item,
            commands::pi_extensions::get_connector_bindings,
            commands::pi_extensions::set_connector_binding,
            commands::pi_extensions::list_host_secret_refs,
            commands::pi_extensions::set_host_secret,
            commands::pi_extensions::delete_host_secret,
            commands::pi_extensions::fetch_pi_packages,
            commands::pi_extensions::install_work_pi_extension,
            commands::pi_extensions::uninstall_work_pi_extension,
            commands::codex_plugins::list_codex_installed_plugins,
            commands::codex_plugins::list_codex_available_plugins,
            commands::codex_plugins::list_codex_marketplaces,
            commands::codex_plugins::add_codex_marketplace,
            commands::codex_plugins::remove_codex_marketplace,
            commands::codex_plugins::upgrade_codex_marketplace,
            commands::codex_plugins::install_codex_plugin,
            commands::codex_plugins::uninstall_codex_plugin,
            commands::codex_plugins::toggle_codex_plugin,
            commands::plugins::check_community_health,
            commands::plugins::search_community_skills,
            commands::plugins::get_community_skill_detail,
            commands::plugins::install_community_skill,
            commands::plugins::import_skill_zip,
            commands::agents::list_agents,
            commands::agents::read_agent_file,
            commands::agents::create_agent_file,
            commands::agents::update_agent_file,
            commands::agents::delete_agent_file,
            commands::agents::list_codex_agents,
            commands::clipboard::get_clipboard_files,
            commands::clipboard::read_clipboard_file,
            commands::clipboard::save_temp_attachment,
            commands::mcp::list_configured_mcp_servers,
            commands::mcp::add_mcp_server,
            commands::mcp::remove_mcp_server,
            commands::mcp::toggle_mcp_server_config,
            commands::mcp::get_disabled_mcp_servers,
            commands::mcp::check_mcp_registry_health,
            commands::mcp::search_mcp_registry,
            commands::mcp::list_codex_mcp_servers,
            commands::mcp::list_grok_mcp_servers,
            commands::mcp::add_codex_mcp_server,
            commands::mcp::remove_codex_mcp_server,
            commands::mcp::add_grok_mcp_server,
            commands::mcp::remove_grok_mcp_server,
            commands::mcp::list_pi_mcp_servers,
            commands::mcp::add_pi_mcp_server,
            commands::mcp::remove_pi_mcp_server,
            commands::mcp::toggle_pi_mcp_server,
            commands::cli_config::get_cli_config,
            commands::cli_config::get_project_cli_config,
            commands::cli_config::update_cli_config,
            commands::cli_config::get_codex_config,
            commands::cli_config::get_project_codex_config,
            commands::cli_config::update_codex_config,
            commands::cli_config::set_codex_feature,
            commands::cli_config::get_codex_hooks,
            commands::cli_config::update_codex_hooks,
            commands::cli_settings::get_cli_permissions,
            commands::cli_settings::update_cli_permissions,
            commands::onboarding::check_auth_status,
            commands::onboarding::detect_install_methods,
            commands::onboarding::run_claude_login,
            commands::onboarding::run_codex_login,
            commands::onboarding::run_codex_subscription_login,
            commands::onboarding::reopen_codex_subscription_login,
            commands::onboarding::run_grok_login,
            commands::onboarding::run_codex_logout,
            commands::onboarding::run_codex_subscription_logout,
            commands::onboarding::check_pi_auth,
            commands::onboarding::run_pi_logout,
            commands::onboarding::get_auth_overview,
            commands::onboarding::set_cli_api_key,
            commands::onboarding::remove_cli_api_key,
            commands::screenshot::capture_screenshot,
            commands::screenshot::update_screenshot_hotkey,
            commands::cli_sync::discover_cli_sessions,
            commands::cli_sync::import_cli_session,
            commands::cli_sync::sync_cli_session,
            commands::updates::check_for_updates,
            commands::web_server::get_web_server_status,
            commands::web_server::get_web_server_token,
            commands::web_server::regenerate_web_server_token,
            commands::web_server::restart_web_server,
            commands::web_server::get_local_ip,
            commands::preview::open_preview_window,
            commands::preview::close_preview_window,
            commands::pty::pty_create,
            commands::pty::pty_write,
            commands::pty::pty_resize,
            commands::pty::pty_kill,
            pet::save_pet_position,
            pet::load_pet_position,
            pet::toggle_pet_window,
            pet::update_pet_always_on_top,
            pet::update_pet_scale,
            pet::set_pet_dragging,
            pet::snap_pet_to_edge,
        ])
        .setup(move |app| {
            // Register before showing the pet Webview so its first ready event
            // cannot race with startup initialization.
            pet::register_ready_listener(app.handle());

            // Initialize Desktop Pet window state/position if enabled
            pet::init_pet_window(app.handle());

            // Recover the user's real shell PATH off the hot path, so CLI detection works
            // when the app is launched from Finder/Dock (which provides only a minimal PATH).
            // Spawning a shell can take a moment; do it on a background thread so startup
            // isn't blocked and the cache is warm before the user reaches onboarding.
            std::thread::spawn(crate::agent::claude_stream::prime_path_cache);

            // Set up broadcast emitter (requires AppHandle, so must be in setup)
            let broadcaster = web_server::broadcaster::EventBroadcaster::new();
            let writer = app.state::<Arc<EventWriter>>().inner().clone();
            let emitter = Arc::new(web_server::broadcaster::BroadcastEmitter::new(
                writer,
                app.handle().clone(),
                broadcaster.clone(),
            ));
            app.manage(broadcaster);
            let scheduler_emitter = emitter.clone();
            web_server::broadcaster::register_shared_emitter(emitter.clone());
            crate::work::browser_operator::browser_session_manager()
                .set_event_emitter(emitter.clone());
            app.manage(emitter);

            // Start web server (non-blocking, spawns async task)
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match web_server::start_server(&app_handle).await {
                    Ok(true) => log::debug!("[app] web server started"),
                    Ok(false) => log::debug!("[app] web server disabled"),
                    Err(e) => log::error!("[app] web server failed to start: {}", e),
                }
            });

            // Start team file watcher for ~/.claude/teams/ and ~/.claude/tasks/
            let cancel = app.state::<CancellationToken>().inner().clone();
            hooks::team_watcher::start_team_watcher(app.handle().clone(), cancel.clone());

            // Start WorkScheduler for scheduled Work tasks
            let scheduler_sessions = app.state::<ActorSessionMap>().inner().clone();
            let scheduler_locks = app.state::<SpawnLocks>().inner().clone();
            let scheduler_cancel = cancel.clone();
            let work_scheduler =
                crate::work::scheduler::WorkScheduler::new(crate::work::paths::WorkPaths::app());
            work_scheduler.spawn(
                scheduler_emitter,
                scheduler_sessions,
                scheduler_locks,
                scheduler_cancel,
            );
            app.manage(work_scheduler);

            // System tray — hide-to-tray on close, left-click to show
            // Non-fatal: if tray library is unavailable (e.g. some Linux desktops),
            // the app still works but window close = quit instead of hide-to-tray.
            match setup_tray(app) {
                Ok(_) => {
                    tray_ok.store(true, Ordering::Relaxed);
                }
                Err(e) => {
                    log::warn!("[app] tray unavailable: {e}, window close = quit");
                }
            }

            // Global shortcut plugin — must be registered inside setup() with a handler
            // so the event dispatch loop is properly initialized
            {
                use tauri_plugin_global_shortcut::ShortcutState;
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(|app, _shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                commands::screenshot::handle_global_shortcut(app);
                            }
                        })
                        .build(),
                )?;
            }

            // Register screenshot hotkey from settings (must come after plugin init)
            commands::screenshot::init_screenshot_hotkey(app.handle());

            // Real Agent Smoke Runner one-shot mode
            #[cfg(feature = "work-smoke")]
            if let Ok(config_path) = std::env::var("AGENTCABIN_WORK_SMOKE_CONFIG") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
                let smoke_app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let exit_code = crate::work::smoke::runner::run_smoke_mode(
                        smoke_app_handle.clone(),
                        config_path,
                    )
                    .await;
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    std::process::exit(exit_code);
                });
            }

            Ok(())
        })
        .on_window_event(move |window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // The pet has no decorations/close affordance, but if a platform
                    // surfaces a close request, hide it instead of destroying the
                    // configured Webview so settings can reopen it later.
                    if window.label() == "pet" {
                        api.prevent_close();
                        let _ = window.hide();
                        return;
                    }
                    // Only intercept close for the main window
                    if window.label() != "main" {
                        return;
                    }
                    api.prevent_close(); // always prevent default close
                    if tray_ok_for_event.load(Ordering::Relaxed) {
                        // Hide to tray instead of quitting
                        let _ = window.hide();
                        log::debug!("[app] window hidden to tray");
                    } else {
                        // No tray — graceful shutdown
                        log::debug!("[app] tray unavailable, starting graceful shutdown");
                        let app = window.app_handle().clone();
                        if let Some(gate) = app.try_state::<ShutdownGate>() {
                            if !gate.try_enter() {
                                return; // shutdown already in progress
                            }
                        }
                        if let Some(ct) = app.try_state::<CancellationToken>() {
                            ct.cancel();
                        }
                        tauri::async_runtime::spawn(async move {
                            graceful_shutdown_actors(&app).await;
                            app.exit(0);
                        });
                    }
                }
                tauri::WindowEvent::Destroyed if window.label() == "main" => {
                    // Safety fallback: cancel actors if main window is truly destroyed (e.g. app.exit()).
                    // Skip for secondary windows (e.g. preview) — destroying them must not shut down the app.
                    cancel_for_exit.cancel();
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        // macOS: clicking the dock icon when all windows are hidden should reopen the window
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows,
            ..
        } = event
        {
            if !has_visible_windows {
                show_main_window(app_handle);
                log::debug!("[app] reopened window from dock click");
            }
        }

        let _ = (app_handle, event); // suppress unused warnings on non-macOS
    });
}

/// Restore the main window: unminimize if needed, then show and focus.
fn show_main_window(handle: &impl tauri::Manager<tauri::Wry>) {
    if let Some(w) = handle.get_webview_window("main") {
        if w.is_minimized().unwrap_or(false) {
            let _ = w.unminimize();
        }
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Create system tray with Show/Quit menu. Left-click shows the window.
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder};

    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &separator, &quit])?;

    let tray_icon_bytes = include_bytes!("../icons/tray-icon.png");
    let tray_img =
        tauri::image::Image::from_bytes(tray_icon_bytes).expect("failed to load tray icon");

    TrayIconBuilder::new()
        .icon(tray_img)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                show_main_window(app);
            }
            "quit" => {
                if let Some(gate) = app.try_state::<ShutdownGate>() {
                    if !gate.try_enter() {
                        return; // shutdown already in progress
                    }
                }
                if let Some(ct) = app.try_state::<CancellationToken>() {
                    ct.cancel();
                }
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    graceful_shutdown_actors(&app).await;
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    log::debug!("[app] system tray created");
    Ok(())
}

/// Graceful shutdown: wait for actors to self-clean, then force-kill remaining processes.
///
/// Two-phase approach:
/// - Phase 1: Wait up to 3s for actors to exit (cancel token already fired → handle_stop → kill+wait).
/// - Phase 2: Drain remaining actors, try_send Stop, join with 2s timeout, abort if stuck.
/// - Then drain ProcessMap (stream processes).
async fn graceful_shutdown_actors(app: &tauri::AppHandle) {
    use crate::agent::adapter::ActorSessionMap;
    use crate::agent::session_actor::ActorCommand;
    use crate::agent::stream::ProcessMap;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);

    // ── Phase 1: Wait for actors to self-cleanup (cancel already fired) ──
    if let Some(sessions) = app.try_state::<ActorSessionMap>() {
        loop {
            let count = sessions.lock().await.len();
            if count == 0 {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                log::warn!(
                    "[app] graceful shutdown: {} actors still alive, force stopping",
                    count
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // ── Phase 2: Force-stop remaining actors ──
        let remaining: Vec<_> = {
            let mut map = sessions.lock().await;
            map.drain().collect()
        };
        for (run_id, handle) in remaining {
            log::debug!("[app] force stopping actor: {}", run_id);
            // try_send avoids blocking if mailbox is full (bounded channel, 64 slots)
            let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
            let _ = handle.cmd_tx.try_send(ActorCommand::Stop {
                reason: crate::agent::session_actor::RuntimeStopReason::AppShutdown,
                reply: reply_tx,
            });
            // Get AbortHandle before consuming JoinHandle in timeout
            let abort = handle.join_handle.abort_handle();
            match tokio::time::timeout(std::time::Duration::from_secs(2), handle.join_handle).await
            {
                Ok(Ok(())) => {
                    log::debug!("[app] actor {} exited cleanly", run_id);
                }
                Ok(Err(e)) => {
                    log::warn!("[app] actor {} join error: {}", run_id, e);
                }
                Err(_) => {
                    log::warn!("[app] actor {} did not exit in 2s, aborting task", run_id);
                    abort.abort();
                }
            }
        }
    }

    // ── Kill remaining stream processes ──
    // ProcessMap lock is only held briefly (run_agent/stop_process do remove-then-await),
    // but we keep a timeout as a defensive fallback.
    if let Some(process_map) = app.try_state::<ProcessMap>() {
        let to_kill = match tokio::time::timeout(std::time::Duration::from_secs(1), async {
            let mut map = process_map.lock().await;
            map.drain().collect::<Vec<_>>()
        })
        .await
        {
            Ok(vec) => vec,
            Err(_) => {
                log::warn!(
                    "[app] graceful shutdown: ProcessMap lock timeout, \
                     skipping (kill_on_drop / Job Object may handle)"
                );
                Vec::new()
            }
        };
        for (run_id, mut child) in to_kill {
            log::debug!("[app] graceful shutdown: killing stream process {}", run_id);
            let _ = child.kill().await;
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
        }
    }

    log::debug!("[app] graceful shutdown complete");
}

/// Headless core-server entry point (`--core-server` CLI flag).
///
/// Binds a loopback-only HTTP server, prints startup JSON to stdout, and waits
/// for SIGINT/SIGTERM. Electron (P4) parses the stdout line to discover port
/// and auth token.
pub fn run_core_server() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("agentcabin_desktop_lib=debug,warn"),
    )
    .format_timestamp_millis()
    .init();

    log::info!("[core] AgentCabin Core server starting");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    rt.block_on(async {
        let runtime = Arc::new(core::CoreRuntime::new());
        runtime.startup();
        runtime.spawn_bridges();
        runtime.spawn_scheduler();
        runtime.prime_path_cache();

        let port = core::server::start(runtime.clone())
            .await
            .expect("core server failed to start");
        let token = runtime.token().await;

        let info = core::server::CoreStartupInfo {
            port,
            token,
            pid: std::process::id(),
        };
        // Electron main parses this line from stdout.
        println!(
            "AGENTCABIN_CORE_READY {}",
            serde_json::to_string(&info).unwrap()
        );

        // Wait for shutdown signal.
        let cancel = runtime.cancel_token.clone();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("[core] received SIGINT");
            }
            _ = cancel.cancelled() => {
                log::info!("[core] cancel token fired");
            }
        }

        runtime.shutdown().await;
        log::info!("[core] shutdown complete");
    });
}
