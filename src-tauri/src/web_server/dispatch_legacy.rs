use serde_json::{json, Value};
use std::time::Instant;

use crate::agent::session_actor::{ActorCommand, AttachmentData};
use crate::models::SessionMode;
use crate::web_server::state::AppState;

fn reject_work_run_on_web(run_id: &str) -> Result<(), String> {
    if crate::storage::runs::get_run(run_id)
        .is_some_and(|run| run.app_mode == crate::work::models::AppMode::Work)
    {
        return Err("Work Mode requires the desktop App transport".into());
    }
    Ok(())
}

fn reject_work_runs_on_web(_method: &str, params: &Value) -> Result<(), String> {
    let mut run_ids = Vec::new();
    for key in ["id", "run_id", "source_run_id", "target_run_id"] {
        if let Some(run_id) = params.get(key).and_then(Value::as_str) {
            run_ids.push(run_id);
        }
    }
    if let Some(ids) = params.get("ids").and_then(Value::as_array) {
        for val in ids {
            if let Some(run_id) = val.as_str() {
                run_ids.push(run_id);
            }
        }
    }
    for run_id in run_ids {
        reject_work_run_on_web(run_id)?;
    }
    Ok(())
}

/// Dispatch a JSON-RPC method call to the corresponding command handler.
/// Returns Ok(result_value) or Err(error_string).
pub async fn dispatch_command(
    method: &str,
    params: Value,
    state: &AppState,
) -> Result<Value, String> {
    let start = Instant::now();
    // Normalize camelCase → snake_case for top-level param keys only
    let params = normalize_top_level_keys(params);

    log::debug!("[dispatch] method={}", method);
    if !state.allow_work {
        reject_work_runs_on_web(method, &params)?;
    }

    // Shortcut alias for the capabilities command family (registered below).
    use crate::commands::capabilities as caps;

    let result = match method {
        // ── Runs ──
        "list_runs" => {
            let runs = crate::commands::runs::list_runs().await?;
            serde_json::to_value(runs).map_err(|e| e.to_string())
        }
        "get_run" => {
            let id = extract_str(&params, "id")?;
            let run = crate::commands::runs::get_run(id)?;
            serde_json::to_value(run).map_err(|e| e.to_string())
        }
        "start_run" => {
            let prompt = extract_str(&params, "prompt")?;
            let cwd = extract_str(&params, "cwd")?;
            let agent = extract_str(&params, "agent")?;
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from);
            let remote_host_name = params
                .get("remote_host_name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let platform_id = params
                .get("platform_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let execution_path = params
                .get("execution_path")
                .and_then(|v| v.as_str())
                .map(String::from);
            let continuation_context = params
                .get("continuation_context")
                .and_then(|v| v.as_str())
                .map(String::from);
            let code_standalone_task = params
                .get("code_standalone_task")
                .and_then(|value| value.as_bool());
            let run = crate::commands::runs::start_run(
                prompt,
                cwd,
                agent,
                model,
                remote_host_name,
                platform_id,
                execution_path,
                continuation_context,
                code_standalone_task,
            )?;
            serde_json::to_value(run).map_err(|e| e.to_string())
        }
        "rename_run" => {
            let id = extract_str(&params, "id")?;
            let name = extract_str(&params, "name")?;
            crate::commands::runs::rename_run(id, name)?;
            Ok(json!(true))
        }
        "delete_runs" | "soft_delete_runs" => {
            let ids: Vec<String> = params
                .get("ids")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            // Keep the legacy method name as an alias, but deletion is now
            // always permanent. Archiving is the separate data-preserving path.
            let count = crate::commands::runs::delete_runs(ids)?;
            Ok(json!(count))
        }
        "set_run_flags" => {
            let id = extract_str(&params, "id")?;
            let flags: crate::commands::runs::RunFlags = params
                .get("flags")
                .ok_or_else(|| "set_run_flags: missing 'flags'".to_string())
                .and_then(|v| {
                    serde_json::from_value(v.clone()).map_err(|e| format!("set_run_flags: {e}"))
                })?;
            // Archiving must not leave a live session attached, otherwise the
            // archived conversation keeps reading as active and cannot be deleted.
            crate::commands::runs::set_run_flags_impl(
                &id,
                flags,
                &state.sessions,
                &state.process_map,
            )
            .await?;
            Ok(json!(true))
        }
        "reveal_in_finder" => {
            let path = extract_str(&params, "path")?;
            crate::commands::runs::reveal_in_finder(path)?;
            Ok(json!(true))
        }
        "update_run_model" => {
            let id = extract_str(&params, "id")?;
            let model = extract_str(&params, "model")?;
            crate::commands::runs::update_run_model_impl(id, model).await?;
            Ok(json!(true))
        }
        "update_run_effort" => {
            let id = extract_str(&params, "id")?;
            let effort = extract_str(&params, "effort")?;
            crate::commands::runs::update_run_effort_impl(id, effort).await?;
            Ok(json!(true))
        }
        "update_run_permission_mode" => {
            let id = extract_str(&params, "id")?;
            let permission_mode = extract_str(&params, "permission_mode")?;
            crate::commands::runs::update_run_permission_mode_impl(id, permission_mode).await?;
            Ok(json!(true))
        }
        "stop_run" => {
            let id = extract_str(&params, "id")?;
            let result = stop_run_impl(id, state).await?;
            Ok(json!(result))
        }
        "search_prompts" => {
            let query = extract_str(&params, "query")?;
            let max_results = params
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let result = crate::commands::runs::search_prompts(query, max_results).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "search_runs" => {
            let filters_val = params
                .get("filters")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let filters: crate::models::RunSearchFilters = serde_json::from_value(filters_val)
                .map_err(|e| format!("Invalid filters: {}", e))?;
            let result = crate::commands::history::search_runs(filters).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_run_files" => {
            let run_id = extract_str(&params, "run_id")?;
            let result = crate::commands::history::get_run_files(run_id).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── Prompt Favorites ──
        "add_prompt_favorite" => {
            let run_id = extract_str(&params, "run_id")?;
            let seq = extract_u64(&params, "seq")?;
            let text = extract_str(&params, "text")?;
            let result = crate::commands::runs::add_prompt_favorite(run_id, seq, text)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_prompt_favorite" => {
            let run_id = extract_str(&params, "run_id")?;
            let seq = extract_u64(&params, "seq")?;
            crate::commands::runs::remove_prompt_favorite(run_id, seq)?;
            Ok(json!(true))
        }
        "update_prompt_favorite_tags" => {
            let run_id = extract_str(&params, "run_id")?;
            let seq = extract_u64(&params, "seq")?;
            let tags: Vec<String> = params
                .get("tags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            crate::storage::favorites::update_favorite_tags(&run_id, seq, tags)?;
            Ok(json!(true))
        }
        "update_prompt_favorite_note" => {
            let run_id = extract_str(&params, "run_id")?;
            let seq = extract_u64(&params, "seq")?;
            let note = extract_str(&params, "note")?;
            crate::commands::runs::update_prompt_favorite_note(run_id, seq, note)?;
            Ok(json!(true))
        }
        "list_prompt_favorites" => {
            let result = crate::commands::runs::list_prompt_favorites()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_prompt_tags" => {
            let result = crate::commands::runs::list_prompt_tags()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── Events ──
        "get_run_events" => {
            let id = extract_str(&params, "id")?;
            let since_seq = params.get("since_seq").and_then(|v| v.as_u64());
            let events = crate::commands::events::get_run_events(id, since_seq)?;
            serde_json::to_value(events).map_err(|e| e.to_string())
        }
        "get_bus_events" => {
            let id = extract_str(&params, "id")?;
            let since_seq = params.get("since_seq").and_then(|v| v.as_u64());
            let events = crate::commands::session::get_bus_events(id, since_seq).await?;
            Ok(Value::Array(events))
        }

        // ── Artifacts ──
        "get_run_artifacts" => {
            let id = extract_str(&params, "id")?;
            let result = crate::commands::artifacts::get_run_artifacts(id)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "export_conversation" => {
            let run_id = extract_str(&params, "run_id")?;
            let md = crate::commands::export::export_conversation(run_id)?;
            Ok(json!(md))
        }
        "write_html_export" => {
            let path = extract_str(&params, "path")?;
            let content = extract_str(&params, "content")?;
            crate::commands::export::write_html_export(path, content).await?;
            Ok(json!(null))
        }

        // ── Settings ──
        "get_user_settings" => {
            let settings = crate::storage::settings::get_user_settings();
            let mut val = serde_json::to_value(settings).map_err(|e| e.to_string())?;
            // Strip token for WS clients (security: don't expose token over WS)
            if let Some(obj) = val.as_object_mut() {
                obj.remove("web_server_token");
            }
            Ok(val)
        }
        "update_user_settings" => {
            let patch = params.get("patch").cloned().unwrap_or(params.clone());
            let result = crate::commands::settings::update_user_settings_with_rotation(
                patch,
                &state.token_version,
                &state.ws_shutdown,
                &state.token,
            )
            .await?;
            let mut val = serde_json::to_value(result).map_err(|e| e.to_string())?;
            if let Some(obj) = val.as_object_mut() {
                obj.remove("web_server_token");
            }
            Ok(val)
        }
        "get_project_preferences" => {
            let cwd = extract_str(&params, "cwd")?;
            let agent = extract_str(&params, "agent")?;
            let remote_host_name = params
                .get("remote_host_name")
                .and_then(Value::as_str)
                .map(String::from);
            let result =
                crate::commands::settings::get_project_preferences(cwd, remote_host_name, agent)?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        }
        "update_project_preferences" => {
            let cwd = extract_str(&params, "cwd")?;
            let agent = extract_str(&params, "agent")?;
            let remote_host_name = params
                .get("remote_host_name")
                .and_then(Value::as_str)
                .map(String::from);
            let patch = params.get("patch").cloned().unwrap_or_else(|| json!({}));
            let result = crate::commands::settings::update_project_preferences(
                cwd,
                remote_host_name,
                agent,
                patch,
            )?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        }
        "get_agent_settings" => {
            let agent = extract_str(&params, "agent")?;
            let settings = crate::commands::settings::get_agent_settings(agent);
            serde_json::to_value(settings).map_err(|e| e.to_string())
        }
        "update_agent_settings" => {
            let agent = extract_str(&params, "agent")?;
            let patch = params.get("patch").cloned().unwrap_or(json!({}));
            let result = crate::commands::settings::update_agent_settings(agent, patch)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_grok_cli_config" => {
            let result = crate::commands::control::get_grok_cli_config()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "update_grok_cli_config" => {
            let patch = params.get("patch").cloned().unwrap_or(json!({}));
            let patch = serde_json::from_value(patch).map_err(|e| e.to_string())?;
            let result = crate::commands::control::update_grok_cli_config(patch)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── Files ──
        "agents_md_exists" => {
            let cwd = extract_str(&params, "cwd")?;
            let exists = crate::commands::files::agents_md_exists(cwd)?;
            Ok(json!(exists))
        }
        "read_text_file" => {
            let path = extract_str(&params, "path")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let content = crate::commands::files::read_text_file(path, cwd)?;
            Ok(json!(content))
        }
        "stat_text_file" => {
            let path = extract_str(&params, "path")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let size = crate::commands::files::stat_text_file(path, cwd)?;
            Ok(json!(size))
        }
        "write_text_file" => {
            let path = extract_str(&params, "path")?;
            let content = extract_str(&params, "content")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::files::write_text_file(path, content, cwd)?;
            Ok(json!(true))
        }
        "read_task_output" => {
            let path = extract_str(&params, "path")?;
            let content = crate::commands::files::read_task_output(path)?;
            Ok(json!(content))
        }
        "list_memory_files" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::files::list_memory_files(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── FS ──
        "list_directory" => {
            let path = extract_str(&params, "path")?;
            let show_hidden = params.get("show_hidden").and_then(|v| v.as_bool());
            let result = crate::commands::fs::list_directory(path, show_hidden)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "check_is_directory" => {
            let path = extract_str(&params, "path")?;
            Ok(json!(crate::commands::fs::check_is_directory(path)))
        }
        "check_path_exists" => {
            let path = extract_str(&params, "path")?;
            Ok(json!(crate::commands::fs::check_path_exists(path)))
        }
        "read_file_base64" => {
            let path = extract_str(&params, "path")?;
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::fs::read_file_base64(path, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_remote_directory" => {
            let host_name = extract_str(&params, "host_name")?;
            let path = extract_str(&params, "path")?;
            let show_hidden = params.get("show_hidden").and_then(|v| v.as_bool());
            let result =
                crate::commands::remote_fs::list_remote_directory(host_name, path, show_hidden)
                    .await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "resolve_remote_home" => {
            let host_name = extract_str(&params, "host_name")?;
            let result = crate::commands::remote_fs::resolve_remote_home(host_name).await?;
            Ok(json!(result))
        }

        // ── Git ──
        "get_git_summary" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::git::get_git_summary(cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_git_branch" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::git::get_git_branch(cwd).await?;
            Ok(json!(result))
        }
        "list_git_branches" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::git::list_git_branches(cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "checkout_git_branch" => {
            let cwd = extract_str(&params, "cwd")?;
            let branch = extract_str(&params, "branch")?;
            let result = crate::commands::git::checkout_git_branch(cwd, branch).await?;
            Ok(json!(result))
        }
        "create_git_branch" => {
            let cwd = extract_str(&params, "cwd")?;
            let branch = extract_str(&params, "branch")?;
            let result = crate::commands::git::create_git_branch(cwd, branch).await?;
            Ok(json!(result))
        }
        "run_terminal_command" => {
            let cwd = extract_str(&params, "cwd")?;
            let command = extract_str(&params, "command")?;
            let result = crate::commands::git::run_terminal_command(cwd, command).await?;
            Ok(json!(result))
        }
        "pty_create" => {
            let session_id = extract_str(&params, "session_id")?;
            let cwd = extract_str(&params, "cwd")?;
            let cols = extract_u64(&params, "cols")? as u16;
            let rows = extract_u64(&params, "rows")? as u16;
            crate::commands::pty::pty_create_impl(session_id, cwd, cols, rows, None)?;
            Ok(json!(true))
        }
        "pty_write" => {
            let session_id = extract_str(&params, "session_id")?;
            let data = extract_str(&params, "data")?;
            crate::commands::pty::pty_write_impl(session_id, data)?;
            Ok(json!(true))
        }
        "pty_resize" => {
            let session_id = extract_str(&params, "session_id")?;
            let cols = extract_u64(&params, "cols")? as u16;
            let rows = extract_u64(&params, "rows")? as u16;
            crate::commands::pty::pty_resize_impl(session_id, cols, rows)?;
            Ok(json!(true))
        }
        "pty_kill" => {
            let session_id = extract_str(&params, "session_id")?;
            crate::commands::pty::pty_kill_impl(session_id, None)?;
            Ok(json!(true))
        }
        "get_git_diff" => {
            let cwd = extract_str(&params, "cwd")?;
            let staged = params
                .get("staged")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let file = params
                .get("file")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result = crate::commands::git::get_git_diff(cwd, staged, file).await?;
            Ok(json!(result))
        }
        "get_git_status" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::git::get_git_status(cwd).await?;
            Ok(json!(result))
        }

        // ── Worktrees ──
        "get_git_project" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::worktrees::get_git_project(cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_git_worktrees" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::worktrees::list_git_worktrees(cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "create_git_worktree" => {
            let cwd = extract_str(&params, "cwd")?;
            let branch = extract_str(&params, "branch")?;
            let path = opt_typed::<String>(&params, "path")?;
            let result = crate::commands::worktrees::create_git_worktree(cwd, branch, path).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_git_worktree" => {
            let cwd = extract_str(&params, "cwd")?;
            let path = extract_str(&params, "path")?;
            let force = params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            crate::commands::worktrees::remove_git_worktree(cwd, path, force).await?;
            Ok(json!(true))
        }

        // ── Teams ──
        "list_teams" => {
            let result = crate::commands::teams::list_teams()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_team_config" => {
            let name = extract_str(&params, "name")?;
            let result = crate::commands::teams::get_team_config(name)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_team_tasks" => {
            let team_name = extract_str(&params, "team_name")?;
            let result = crate::commands::teams::list_team_tasks(team_name)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_team_task" => {
            let team_name = extract_str(&params, "team_name")?;
            let task_id = extract_str(&params, "task_id")?;
            let result = crate::commands::teams::get_team_task(team_name, task_id)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_team_inbox" => {
            let team_name = extract_str(&params, "team_name")?;
            let member_name = extract_str(&params, "member_name")?;
            let result = crate::commands::teams::get_team_inbox(team_name, member_name)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_all_team_inboxes" => {
            let name = extract_str(&params, "name")?;
            let result = crate::commands::teams::get_all_team_inboxes(name)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "delete_team" => {
            let name = extract_str(&params, "name")?;
            crate::commands::teams::delete_team(name)?;
            Ok(json!(true))
        }

        // ── Plugins / Skills ──
        "list_marketplaces" => {
            let result = crate::commands::plugins::list_marketplaces()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_marketplace_plugins" => {
            let result = crate::commands::plugins::list_marketplace_plugins()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_standalone_skills" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::list_standalone_skills(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_grok_skills" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::list_grok_skills(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_project_commands" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::list_project_commands(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_installed_plugins" => {
            let result = crate::commands::claude_plugins::list_claude_installed_plugins().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_claude_installed_plugins" => {
            let result = crate::commands::claude_plugins::list_claude_installed_plugins().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_claude_available_plugins" => {
            let result = crate::commands::claude_plugins::list_claude_available_plugins().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_claude_marketplaces" => {
            let result = crate::commands::claude_plugins::list_claude_marketplaces().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "ensure_claude_official_marketplace" => {
            let result =
                crate::commands::claude_plugins::ensure_claude_official_marketplace().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "sync_claude_official_marketplace" => {
            let result =
                crate::commands::claude_plugins::sync_claude_official_marketplace().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "work_get_projection" => {
            let run_id =
                extract_str(&params, "run_id").or_else(|_| extract_str(&params, "runId"))?;
            let result = crate::commands::work::work_get_projection(run_id).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "work_get_run_progress" => {
            let task_id =
                extract_str(&params, "task_id").or_else(|_| extract_str(&params, "taskId"))?;
            let run_id =
                extract_str(&params, "run_id").or_else(|_| extract_str(&params, "runId"))?;
            let result = crate::commands::work::work_get_run_progress(task_id, run_id).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_skill_content" => {
            let path = extract_str(&params, "path")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::get_skill_content(path, cwd)?;
            Ok(json!(result))
        }
        "create_skill" => {
            let name = extract_str(&params, "name")?;
            let description = extract_str(&params, "description")?;
            let content = extract_str(&params, "content")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result =
                crate::commands::plugins::create_skill(name, description, content, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "create_grok_skill" => {
            let name = extract_str(&params, "name")?;
            let description = extract_str(&params, "description")?;
            let content = extract_str(&params, "content")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::create_grok_skill(
                name,
                description,
                content,
                scope,
                cwd,
            )?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "update_skill" => {
            let path = extract_str(&params, "path")?;
            let content = extract_str(&params, "content")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::plugins::update_skill(path, content, cwd)?;
            Ok(json!(true))
        }
        "delete_skill" => {
            let path = extract_str(&params, "path")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::plugins::delete_skill(path, cwd)?;
            Ok(json!(true))
        }
        // ── Codex Skills ──
        "list_codex_skills" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::list_codex_skills(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "create_codex_skill" => {
            let name = extract_str(&params, "name")?;
            let description = extract_str(&params, "description")?;
            let content = extract_str(&params, "content")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::create_codex_skill(
                name,
                description,
                content,
                scope,
                cwd,
            )?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "delete_codex_skill" => {
            let path = extract_str(&params, "path")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::plugins::delete_codex_skill(path, cwd)?;
            Ok(json!(true))
        }
        "toggle_codex_skill" => {
            let skill_path = extract_str(&params, "skill_path")?;
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::plugins::toggle_codex_skill(skill_path, enabled, cwd)?;
            Ok(json!(true))
        }
        "list_prompt_templates" => {
            let result = crate::commands::prompt_templates::list_prompt_templates()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "create_prompt_template" => {
            let name = extract_str(&params, "name")?;
            let description = params
                .get("description")
                .and_then(|value| value.as_str())
                .map(String::from);
            let content = extract_str(&params, "content")?;
            let result = crate::commands::prompt_templates::create_prompt_template(
                name,
                description,
                content,
            )?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "update_prompt_template" => {
            let id = extract_str(&params, "id")?;
            let name = extract_str(&params, "name")?;
            let description = params
                .get("description")
                .and_then(|value| value.as_str())
                .map(String::from);
            let content = extract_str(&params, "content")?;
            let result = crate::commands::prompt_templates::update_prompt_template(
                id,
                name,
                description,
                content,
            )?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "delete_prompt_template" => {
            let id = extract_str(&params, "id")?;
            let result = crate::commands::prompt_templates::delete_prompt_template(id)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_codex_installed_plugins" => {
            let result = crate::commands::codex_plugins::list_codex_installed_plugins().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_codex_available_plugins" => {
            let result = crate::commands::codex_plugins::list_codex_available_plugins().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_codex_marketplaces" => {
            let result = crate::commands::codex_plugins::list_codex_marketplaces().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "add_codex_marketplace" => {
            let source = extract_str(&params, "source")?;
            let result = crate::commands::codex_plugins::add_codex_marketplace(source).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_codex_marketplace" => {
            let name = extract_str(&params, "name")?;
            let result = crate::commands::codex_plugins::remove_codex_marketplace(name).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "upgrade_codex_marketplace" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result = crate::commands::codex_plugins::upgrade_codex_marketplace(name).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "install_codex_plugin" => {
            let plugin_id = extract_str(&params, "plugin_id")?;
            let marketplace = params
                .get("marketplace")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result =
                crate::commands::codex_plugins::install_codex_plugin(plugin_id, marketplace)
                    .await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "uninstall_codex_plugin" => {
            let plugin_id = extract_str(&params, "plugin_id")?;
            let marketplace = params
                .get("marketplace")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result =
                crate::commands::codex_plugins::uninstall_codex_plugin(plugin_id, marketplace)
                    .await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "toggle_codex_plugin" => {
            let plugin_id = extract_str(&params, "plugin_id")?;
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| "enabled (bool) is required".to_string())?;
            crate::commands::codex_plugins::toggle_codex_plugin(plugin_id, enabled)?;
            Ok(json!(true))
        }
        "install_plugin" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::install_plugin(name, scope, cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "uninstall_plugin" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::uninstall_plugin(name, scope, cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "enable_plugin" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::enable_plugin(name, scope, cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "disable_plugin" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::disable_plugin(name, scope, cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "update_plugin" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::update_plugin(name, scope, cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "add_marketplace" => {
            let source = extract_str(&params, "source")?;
            let result = crate::commands::plugins::add_marketplace(source).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_marketplace" => {
            let name = extract_str(&params, "name")?;
            let result = crate::commands::plugins::remove_marketplace(name).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "update_marketplace" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result = crate::commands::plugins::update_marketplace(name).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "check_community_health" => {
            let result = crate::commands::plugins::check_community_health().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "search_community_skills" => {
            let query = extract_str(&params, "query")?;
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let result = crate::commands::plugins::search_community_skills(query, limit).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_community_skill_detail" => {
            let source = extract_str(&params, "source")?;
            let skill_id = extract_str(&params, "skill_id")?;
            let result =
                crate::commands::plugins::get_community_skill_detail(source, skill_id).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "install_community_skill" => {
            let source = extract_str(&params, "source")?;
            let skill_id = extract_str(&params, "skill_id")?;
            let scope = extract_str(&params, "scope")?;
            let target_agent = params
                .get("target_agent")
                .and_then(|v| v.as_str())
                .map(String::from);
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::plugins::install_community_skill(
                source,
                skill_id,
                scope,
                target_agent,
                cwd,
            )
            .await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── CLI Config ──
        "get_cli_config" => {
            let result = crate::commands::cli_config::get_cli_config()?;
            Ok(result)
        }
        "get_project_cli_config" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::cli_config::get_project_cli_config(cwd)?;
            Ok(result)
        }
        "update_cli_config" => {
            let patch = params.get("patch").cloned().unwrap_or(params.clone());
            let result =
                crate::commands::cli_config::update_cli_config_impl(&state.sessions, patch).await?;
            Ok(result)
        }
        // ── Codex Config (web dispatch) ──
        "get_codex_config" => {
            let result = crate::commands::cli_config::get_codex_config()?;
            Ok(result)
        }
        "get_project_codex_config" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::cli_config::get_project_codex_config(cwd)?;
            Ok(result)
        }
        "update_codex_config" => {
            let patch = params.get("patch").cloned().unwrap_or(params.clone());
            let result = crate::commands::cli_config::update_codex_config(patch)?;
            Ok(result)
        }
        // ── Codex Hooks ──
        "get_codex_hooks" => {
            let result = crate::commands::cli_config::get_codex_hooks()?;
            Ok(result)
        }
        "update_codex_hooks" => {
            let hooks = params.get("hooks").cloned().unwrap_or(params.clone());
            let result = crate::commands::cli_config::update_codex_hooks(hooks)?;
            Ok(json!(result))
        }

        // ── CLI Permissions ──
        "get_cli_permissions" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::cli_settings::get_cli_permissions(cwd).await?;
            Ok(result)
        }
        "update_cli_permissions" => {
            let scope = extract_str(&params, "scope")?;
            let category = extract_str(&params, "category")?;
            let rules_val = params
                .get("rules")
                .ok_or_else(|| "Missing required parameter: rules".to_string())?;
            let rules: Vec<String> = serde_json::from_value(rules_val.clone())
                .map_err(|e| format!("Invalid rules: {}", e))?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::cli_settings::update_cli_permissions(scope, category, rules, cwd)
                .await?;
            Ok(json!(true))
        }

        // ── MCP ──
        "list_configured_mcp_servers" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::list_configured_mcp_servers(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "add_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let transport = extract_str(&params, "transport")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let config_json = params
                .get("config_json")
                .and_then(|v| v.as_str())
                .map(String::from);
            let url = params.get("url").and_then(|v| v.as_str()).map(String::from);
            let env_vars: Option<std::collections::HashMap<String, String>> = params
                .get("env_vars")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let headers: Option<std::collections::HashMap<String, String>> = params
                .get("headers")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let result = crate::commands::mcp::add_mcp_server(
                name,
                transport,
                scope,
                cwd,
                config_json,
                url,
                env_vars,
                headers,
            )
            .await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::remove_mcp_server(name, scope, cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "toggle_mcp_server_config" => {
            let name = extract_str(&params, "name")?;
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::toggle_mcp_server_config(name, enabled, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        // ── Codex MCP ──
        "list_codex_mcp_servers" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::list_codex_mcp_servers(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "add_codex_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let config = params
                .get("config")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let result = crate::commands::mcp::add_codex_mcp_server(name, config)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_codex_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::remove_codex_mcp_server(name, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        // ── Grok MCP ──
        "list_grok_mcp_servers" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::list_grok_mcp_servers(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "add_grok_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let config = params
                .get("config")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let scope = params
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("user")
                .to_string();
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::add_grok_mcp_server(name, config, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_grok_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::remove_grok_mcp_server(name, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        // ── Pi MCP ──
        "list_pi_mcp_servers" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::list_pi_mcp_servers(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "add_pi_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let config = params
                .get("config")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let scope = params
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("user")
                .to_string();
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::add_pi_mcp_server(name, config, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "remove_pi_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::remove_pi_mcp_server(name, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "toggle_pi_mcp_server" => {
            let name = extract_str(&params, "name")?;
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let scope = extract_str(&params, "scope")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::mcp::toggle_pi_mcp_server(name, enabled, scope, cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "check_mcp_registry_health" => {
            let result = crate::commands::mcp::check_mcp_registry_health().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "search_mcp_registry" => {
            let query = extract_str(&params, "query")?;
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let cursor = params
                .get("cursor")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result = crate::commands::mcp::search_mcp_registry(query, limit, cursor).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── Diagnostics ──
        "check_agent_cli" => {
            let agent = extract_str(&params, "agent")?;
            let result = crate::commands::diagnostics::check_agent_cli(agent).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "check_codex_auth" => {
            let result = crate::commands::diagnostics::check_codex_auth().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "check_pi_auth" => {
            let result = crate::commands::onboarding::check_pi_auth().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "check_project_init" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::diagnostics::check_project_init(cwd)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "run_diagnostics" => {
            let cwd = extract_str(&params, "cwd")?;
            let result = crate::commands::diagnostics::run_diagnostics(cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_cli_dist_tags" => {
            let result = crate::commands::diagnostics::get_cli_dist_tags().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "detect_local_proxy" => {
            let proxy_id = extract_str(&params, "proxy_id")?;
            let base_url = extract_str(&params, "base_url")?;
            let result =
                crate::commands::diagnostics::detect_local_proxy(proxy_id, base_url).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "test_api_connectivity" => {
            let api_key = extract_str(&params, "api_key")?;
            let base_url = extract_str(&params, "base_url")?;
            let auth_env_var = extract_str(&params, "auth_env_var")?;
            let model = extract_str(&params, "model")?;
            let result = crate::commands::diagnostics::test_api_connectivity(
                api_key,
                base_url,
                auth_env_var,
                model,
            )
            .await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "test_remote_host" => {
            let host = extract_str(&params, "host")?;
            let user = extract_str(&params, "user")?;
            let port = params
                .get("port")
                .and_then(|v| v.as_u64())
                .map(|n| n as u16);
            let key_path = params
                .get("key_path")
                .and_then(|v| v.as_str())
                .map(String::from);
            let remote_claude_path = params
                .get("remote_claude_path")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result = crate::commands::diagnostics::test_remote_host(
                host,
                user,
                port,
                key_path,
                remote_claude_path,
            )
            .await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "check_ssh_key" => {
            let result = crate::commands::diagnostics::check_ssh_key()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "generate_ssh_key" => {
            let result = crate::commands::diagnostics::generate_ssh_key()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── Stats ──
        "get_usage_overview" => {
            let days = params
                .get("days")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let result = crate::commands::stats::get_usage_overview(days)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_global_usage_overview" => {
            let days = params
                .get("days")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let result = crate::commands::stats::get_global_usage_overview(days)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "clear_usage_cache" => {
            crate::commands::stats::clear_usage_cache()?;
            Ok(json!(true))
        }
        "get_heatmap_daily" => {
            let scope = extract_str(&params, "scope")?;
            let result = crate::commands::stats::get_heatmap_daily(scope)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_changelog" => {
            let result = crate::commands::stats::get_changelog().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── Onboarding ──
        "check_auth_status" => {
            let result = crate::commands::onboarding::check_auth_status().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "detect_install_methods" => {
            let agent = extract_str(&params, "agent").unwrap_or_else(|_| "claude".into());
            let result = crate::commands::onboarding::detect_install_methods(agent).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_auth_overview" => {
            let result = crate::commands::onboarding::get_auth_overview().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "set_cli_api_key" => {
            let key = extract_str(&params, "key")?;
            crate::commands::onboarding::set_cli_api_key(key).await?;
            Ok(json!(true))
        }
        "remove_cli_api_key" => {
            crate::commands::onboarding::remove_cli_api_key().await?;
            Ok(json!(true))
        }

        // ── Agents ──
        "list_agents" => {
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::agents::list_agents(cwd).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "list_codex_agents" => {
            let result = crate::commands::agents::list_codex_agents()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "read_agent_file" => {
            let scope = extract_str(&params, "scope")?;
            let file_name = extract_str(&params, "file_name")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            let result = crate::commands::agents::read_agent_file(scope, file_name, cwd)?;
            Ok(json!(result))
        }
        "create_agent_file" => {
            let scope = extract_str(&params, "scope")?;
            let file_name = extract_str(&params, "file_name")?;
            let content = extract_str(&params, "content")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::agents::create_agent_file(scope, file_name, content, cwd)?;
            Ok(json!(true))
        }
        "update_agent_file" => {
            let scope = extract_str(&params, "scope")?;
            let file_name = extract_str(&params, "file_name")?;
            let content = extract_str(&params, "content")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::agents::update_agent_file(scope, file_name, content, cwd)?;
            Ok(json!(true))
        }
        "delete_agent_file" => {
            let scope = extract_str(&params, "scope")?;
            let file_name = extract_str(&params, "file_name")?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).map(String::from);
            crate::commands::agents::delete_agent_file(scope, file_name, cwd)?;
            Ok(json!(true))
        }

        // ── CLI Sync ──
        "discover_cli_sessions" => {
            let cwd = extract_str(&params, "cwd")?;
            let agent = params
                .get("agent")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result = crate::commands::cli_sync::discover_cli_sessions(cwd, agent).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        // ── Clipboard (partial browser support) ──
        "read_clipboard_file" => {
            let path = extract_str(&params, "path")?;
            let as_text = params
                .get("as_text")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let result = crate::commands::clipboard::read_clipboard_file(path, as_text)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "save_temp_attachment" => {
            let name = extract_str(&params, "name")?;
            let content_base64 = extract_str(&params, "content_base64")?;
            let path = crate::commands::clipboard::save_temp_attachment(name, content_base64)?;
            Ok(json!(path))
        }

        // ── Web Server Status ──
        "get_web_server_status" => {
            let port = state
                .effective_port
                .load(std::sync::atomic::Ordering::Relaxed);
            Ok(crate::web_server::build_status(
                port,
                state.bind_addr.as_str(),
                &None,
            ))
        }

        // ── Session management ──
        "start_session" => {
            let run_id = extract_str(&params, "run_id")?;
            let mode: Option<SessionMode> = params
                .get("mode")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let initial_message = params
                .get("initial_message")
                .and_then(|v| v.as_str())
                .map(String::from);
            let attachments: Option<Vec<AttachmentData>> = params
                .get("attachments")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let platform_id = params
                .get("platform_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let permission_mode_override = params
                .get("permission_mode_override")
                .and_then(|v| v.as_str())
                .map(String::from);
            // Route through the unified session dispatcher (same as the Tauri
            // `commands::session_dispatch::start_session` command). It handles
            // Work routing, DSH, Grok and Pi natively and delegates Claude /
            // Codex back to the legacy impl below. Calling the legacy impl
            // directly here dropped DSH onto the plain Claude Code CLI path
            // (no DSH provider patch / credentials → "Not logged in").
            crate::commands::session_dispatch::start_session_impl(
                &state.emitter,
                &state.sessions,
                &state.spawn_locks,
                &state.cancel_token,
                run_id,
                mode,
                session_id,
                initial_message,
                attachments,
                platform_id,
                permission_mode_override,
                None,
            )
            .await?;
            Ok(json!(true))
        }
        "send_session_message" => {
            let run_id = extract_str(&params, "run_id")?;
            let message = extract_str(&params, "message")?;
            let attachments: Vec<AttachmentData> = params
                .get("attachments")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            log::debug!(
                "[dispatch] send_session_message: run_id={}, msg_len={}, attachments={}",
                run_id,
                message.len(),
                attachments.len()
            );
            let cmd_tx = {
                let map = state.sessions.lock().await;
                map.get(&run_id)
                    .map(|h| h.cmd_tx.clone())
                    .ok_or_else(|| format!("Session {} not found", run_id))?
            };
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::SendMessage {
                    text: message,
                    attachments,
                    skills: Vec::new(),
                    work_context_plan: None,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| "Actor dead".to_string())?;
            reply_rx
                .await
                .map_err(|_| "Actor dropped reply".to_string())??;
            Ok(json!(true))
        }
        "steer_session_message" => {
            let run_id = extract_str(&params, "run_id")?;
            let message = extract_str(&params, "message")?;
            let attachments: Vec<AttachmentData> = params
                .get("attachments")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let cmd_tx = {
                let map = state.sessions.lock().await;
                map.get(&run_id)
                    .map(|h| h.cmd_tx.clone())
                    .ok_or_else(|| format!("Session {} not found", run_id))?
            };
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::SteerMessage {
                    text: message,
                    attachments,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| "Actor dead".to_string())?;
            reply_rx
                .await
                .map_err(|_| "Actor dropped reply".to_string())??;
            Ok(json!(true))
        }
        "stop_session" => {
            let run_id = extract_str(&params, "run_id")?;
            crate::commands::session::stop_session_impl(
                &state.emitter,
                &state.sessions,
                &state.spawn_locks,
                run_id,
            )
            .await?;
            Ok(json!(true))
        }
        "send_session_control" => {
            let run_id = extract_str(&params, "run_id")?;
            let subtype = extract_str(&params, "subtype")?;
            let ctrl_params = params.get("params").cloned();
            log::debug!(
                "[dispatch] send_session_control: run_id={}, subtype={}",
                run_id,
                subtype
            );
            let cmd_tx = {
                let map = state.sessions.lock().await;
                map.get(&run_id)
                    .map(|h| h.cmd_tx.clone())
                    .ok_or_else(|| format!("Session {} not found", run_id))?
            };
            let mut request = json!({ "subtype": subtype });
            if let Some(p) = ctrl_params {
                if let Some(obj) = p.as_object() {
                    for (k, v) in obj {
                        request[k] = v.clone();
                    }
                }
            }
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::SendControl {
                    request,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| "Actor dead".to_string())?;
            // A provider/tool handler can keep the actor busy long enough that
            // it cannot dequeue this control immediately. Keep legacy/Web
            // transport from hanging the same way as the Tauri command.
            let (request_id, response_rx) =
                tokio::time::timeout(std::time::Duration::from_secs(2), reply_rx)
                    .await
                    .map_err(|_| "Timeout waiting for actor to accept control request".to_string())?
                    .map_err(|_| "Actor dropped reply".to_string())??;
            match tokio::time::timeout(std::time::Duration::from_secs(10), response_rx).await {
                Ok(Ok(response)) => {
                    log::debug!(
                        "[dispatch] control response received for req_id={}",
                        request_id
                    );
                    Ok(response)
                }
                Ok(Err(_)) => Err("Control response channel closed".to_string()),
                Err(_) => Err("Timeout waiting for control response".to_string()),
            }
        }
        "fork_session" => {
            let run_id = extract_str(&params, "run_id")?;
            let new_id = crate::commands::session::fork_session_impl(
                &state.emitter,
                &state.sessions,
                &state.spawn_locks,
                run_id,
            )
            .await?;
            Ok(json!(new_id))
        }
        "get_continuation_context" => {
            let run_id = extract_str(&params, "run_id")?;
            let anchor_id = params
                .get("anchor_id")
                .and_then(|value| value.as_str())
                .map(String::from);
            let context = crate::commands::session::get_continuation_context(run_id, anchor_id)?;
            Ok(json!(context))
        }
        "copy_run_history" => {
            let source_run_id = extract_str(&params, "source_run_id")?;
            let target_run_id = extract_str(&params, "target_run_id")?;
            let anchor_id = params
                .get("anchor_id")
                .and_then(|value| value.as_str())
                .map(String::from);
            crate::commands::session::copy_run_history(source_run_id, target_run_id, anchor_id)?;
            Ok(json!(true))
        }
        "switch_agent" => {
            let run_id = extract_str(&params, "run_id")?;
            let target_agent = extract_str(&params, "target_agent")?;
            let result = crate::commands::session::switch_agent_impl(
                &state.spawn_locks,
                run_id,
                target_agent,
            )
            .await?;
            Ok(json!(result))
        }
        "approve_session_tool" => {
            let run_id = extract_str(&params, "run_id")?;
            let tool_name = extract_str(&params, "tool_name")?;
            crate::commands::session::approve_session_tool_impl(
                &state.emitter,
                &state.sessions,
                &state.spawn_locks,
                &state.cancel_token,
                run_id,
                tool_name,
            )
            .await?;
            Ok(json!(true))
        }
        "respond_permission" => {
            let run_id = extract_str(&params, "run_id")?;
            let request_id = extract_str(&params, "request_id")?;
            let behavior = extract_str(&params, "behavior")?;
            let updated_permissions: Option<Vec<Value>> = params
                .get("updated_permissions")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let updated_input = params.get("updated_input").cloned();
            let deny_message = params
                .get("deny_message")
                .and_then(|v| v.as_str())
                .map(String::from);
            let interrupt = params.get("interrupt").and_then(|v| v.as_bool());
            log::debug!(
                "[dispatch] respond_permission: run_id={}, req_id={}, behavior={}",
                run_id,
                request_id,
                behavior
            );
            let cmd_tx = {
                let map = state.sessions.lock().await;
                map.get(&run_id)
                    .map(|h| h.cmd_tx.clone())
                    .ok_or_else(|| format!("Session {} not found", run_id))?
            };
            let mut response = if behavior == "allow" {
                let input_val = updated_input.unwrap_or_else(|| json!({}));
                json!({
                    "behavior": "allow",
                    "updatedInput": input_val,
                })
            } else {
                let msg = deny_message.unwrap_or_else(|| "User denied permission".to_string());
                let mut deny_obj = json!({
                    "behavior": "deny",
                    "message": msg,
                });
                if interrupt == Some(true) {
                    deny_obj["interrupt"] = json!(true);
                }
                deny_obj
            };
            if let Some(perms) = updated_permissions {
                if behavior == "allow" && !perms.is_empty() {
                    response["updatedPermissions"] = Value::Array(perms);
                }
            }
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::RespondPermission {
                    request_id,
                    response,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| "Actor dead".to_string())?;
            reply_rx
                .await
                .map_err(|_| "Actor dropped reply".to_string())??;
            Ok(json!(true))
        }
        "respond_hook_callback" => {
            let run_id = extract_str(&params, "run_id")?;
            let request_id = extract_str(&params, "request_id")?;
            let decision = extract_str(&params, "decision")?;
            let updated_input = params.get("updated_input").cloned();
            log::debug!(
                "[dispatch] respond_hook_callback: run_id={}, req_id={}, decision={}, has_updated_input={}",
                run_id,
                request_id,
                decision,
                updated_input.is_some(),
            );
            let cmd_tx = {
                let map = state.sessions.lock().await;
                map.get(&run_id)
                    .map(|h| h.cmd_tx.clone())
                    .ok_or_else(|| format!("Session {} not found", run_id))?
            };
            let mut response = json!({ "decision": decision });
            if decision == "allow" {
                if let Some(input) = updated_input {
                    response["updatedInput"] = input;
                }
            }
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::RespondHookCallback {
                    request_id,
                    response,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| "Actor dead".to_string())?;
            reply_rx
                .await
                .map_err(|_| "Actor dropped reply".to_string())??;
            Ok(json!(true))
        }
        "cancel_control_request" => {
            let run_id = extract_str(&params, "run_id")?;
            let request_id = extract_str(&params, "request_id")?;
            log::debug!(
                "[dispatch] cancel_control_request: run_id={}, req_id={}",
                run_id,
                request_id
            );
            let cmd_tx = {
                let map = state.sessions.lock().await;
                map.get(&run_id)
                    .map(|h| h.cmd_tx.clone())
                    .ok_or_else(|| format!("Session {} not found", run_id))?
            };
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::CancelControlRequest {
                    request_id,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| "Actor dead".to_string())?;
            reply_rx
                .await
                .map_err(|_| "Actor dropped reply".to_string())??;
            Ok(json!(true))
        }

        "respond_user_input" => {
            let run_id = extract_str(&params, "run_id")?;
            let request_id = extract_str(&params, "request_id")?;
            let answers = params
                .get("answers")
                .cloned()
                .ok_or_else(|| "respond_user_input: missing 'answers'".to_string())?;
            crate::commands::session::respond_user_input_impl(
                &state.sessions,
                run_id,
                request_id,
                answers,
            )
            .await?;
            Ok(json!(true))
        }

        "respond_elicitation" => {
            let run_id = extract_str(&params, "run_id")?;
            let request_id = extract_str(&params, "request_id")?;
            let action = extract_str(&params, "action")?;
            let content = params.get("content").cloned();
            log::debug!(
                "[dispatch] respond_elicitation: run_id={}, req_id={}, action={}",
                run_id,
                request_id,
                action
            );
            if !matches!(action.as_str(), "accept" | "decline" | "cancel") {
                return Err(format!("Invalid elicitation action: {}", action));
            }
            let response = match action.as_str() {
                "accept" => {
                    let c = content.unwrap_or(json!({}));
                    if !c.is_object() {
                        return Err("content must be a JSON object for accept".into());
                    }
                    json!({"action": "accept", "content": c})
                }
                other => json!({"action": other}),
            };
            let cmd_tx = {
                let map = state.sessions.lock().await;
                map.get(&run_id)
                    .map(|h| h.cmd_tx.clone())
                    .ok_or_else(|| format!("Session {} not found", run_id))?
            };
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::RespondElicitation {
                    request_id,
                    response,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| "Actor dead".to_string())?;
            reply_rx
                .await
                .map_err(|_| "Actor dropped reply".to_string())??;
            Ok(json!(true))
        }

        // ── CLI Info ──
        "get_cli_info" => {
            let force = params
                .get("force_refresh")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match crate::agent::control::get_cli_info(&state.cli_info_cache, force).await {
                Ok(info) => serde_json::to_value(info).map_err(|e| e.to_string()),
                Err(e) => {
                    log::warn!(
                        "[dispatch] CLI info failed ({}): {}, using fallback",
                        e.code,
                        e.message
                    );
                    serde_json::to_value(crate::agent::control::fallback_cli_info())
                        .map_err(|e| e.to_string())
                }
            }
        }

        "get_pi_models" => {
            let models = crate::commands::control::get_pi_models().await?;
            serde_json::to_value(models).map_err(|e| e.to_string())
        }

        "get_grok_status" => {
            let status = crate::commands::control::get_grok_status().await?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        }

        "get_grok_models" => {
            let models = crate::commands::control::get_grok_models().await?;
            serde_json::to_value(models).map_err(|e| e.to_string())
        }

        // Headless core-server has no Tauri managed state, so the Codex catalog
        // cache lives in a process-wide static (same TTL semantics as the
        // managed CodexInfoCache on the Tauri IPC path).
        "get_codex_models" => {
            static CODEX_CACHE: std::sync::OnceLock<crate::agent::codex_control::CodexInfoCache> =
                std::sync::OnceLock::new();
            let cache = CODEX_CACHE.get_or_init(crate::agent::codex_control::CodexInfoCache::new);
            let force = params
                .get("force_refresh")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let list = match crate::agent::codex_control::get_codex_models(cache, force).await {
                Ok(list) => list,
                Err(e) => {
                    log::warn!(
                        "[dispatch] codex models failed ({}): {}, using fallback",
                        e.code,
                        e.message
                    );
                    crate::agent::codex_control::fallback_models()
                }
            };
            serde_json::to_value(list).map_err(|e| e.to_string())
        }

        "check_vscode_available" => {
            serde_json::to_value(crate::commands::editors::check_vscode_available())
                .map_err(|e| e.to_string())
        }
        "open_project_in_vscode" => {
            let cwd = extract_str(&params, "cwd")?;
            crate::commands::editors::open_project_in_vscode(cwd)?;
            Ok(json!(true))
        }

        "list_dsh_plugins" => Ok(serde_json::json!([])),

        "toggle_dsh_plugin" => Err("DSH plugins are no longer supported".to_string()),

        "register_dsh_plugin" => Err("DSH plugins are no longer supported".to_string()),

        "unregister_dsh_plugin" => Ok(serde_json::json!(true)),

        "update_dsh_plugin" => Err("DSH plugins are no longer supported".to_string()),

        // ── Provider testing (global settings screen) ──
        "test_global_provider" => {
            let provider: crate::models::GlobalProviderCredential =
                typed_param(&params, "provider")?;
            let model = extract_str(&params, "model")?;
            let result =
                crate::commands::diagnostics::test_global_provider(provider, model).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        "list_global_provider_models" => {
            let provider: crate::models::GlobalProviderCredential =
                typed_param(&params, "provider")?;
            let models =
                crate::commands::diagnostics::list_global_provider_models(provider).await?;
            serde_json::to_value(models).map_err(|e| e.to_string())
        }

        "test_pi_provider" => {
            let provider: crate::models::PiProviderCredential = typed_param(&params, "provider")?;
            let result = crate::commands::diagnostics::test_pi_provider(provider).await?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }

        "get_pi_extension_ui_state" => {
            let run_id = extract_str(&params, "run_id")?;
            crate::commands::session::get_pi_extension_ui_state_inner(&state.sessions, run_id).await
        }

        // ── CLI Sync (additional) ──
        "sync_cli_session" => {
            let run_id = extract_str(&params, "run_id")?;
            // Dispatch by RunMeta.agent — Codex runs use codex_sessions::sync_session.
            let meta = crate::storage::runs::get_run(&run_id)
                .ok_or_else(|| format!("run {} not found", run_id))?;
            let agent = meta.agent.clone();
            let writer = state.writer.clone();
            let result = tokio::task::spawn_blocking(move || match agent.as_str() {
                "codex" => crate::storage::codex_sessions::sync_session(&run_id, writer),
                _ => crate::storage::cli_sessions::sync_session(&run_id, writer),
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))?;
            let sync_result = result?;
            serde_json::to_value(sync_result).map_err(|e| e.to_string())
        }
        "import_cli_session" => {
            let session_id = extract_str(&params, "session_id")?;
            let cwd = extract_str(&params, "cwd")?;
            let agent = params
                .get("agent")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| "claude".to_string());
            let writer = state.writer.clone();
            let result = tokio::task::spawn_blocking(move || match agent.as_str() {
                "codex" => {
                    crate::storage::codex_sessions::import_session(&session_id, &cwd, writer)
                }
                _ => crate::storage::cli_sessions::import_session(&session_id, &cwd, writer),
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))?;
            let import_result = result?;
            serde_json::to_value(import_result).map_err(|e| e.to_string())
        }

        // ── Capabilities (settings/capability page, stateless commands) ──
        "get_web_access_binding" => to_json(caps::get_web_access_binding()?),
        "set_web_access_binding" => to_json(caps::set_web_access_binding(extract_bool(
            &params, "enabled",
        )?)),
        "get_browser_config" => to_json(caps::get_browser_config()?),
        "save_browser_config" => to_json(caps::save_browser_config(
            extract_str(&params, "provider")?,
            extract_bool(&params, "enabled")?,
            opt_typed::<u32>(&params, "max_results")?,
            opt_typed::<String>(&params, "api_key")?,
            opt_typed::<String>(&params, "endpoint_url")?,
            opt_typed::<Vec<String>>(&params, "allowed_hosts")?,
        )),
        "test_browser" => to_json(caps::test_browser().await?),
        "list_browser_sessions" => to_json(caps::list_browser_sessions().await?),
        "get_browser_session" => {
            to_json(caps::get_browser_session(extract_str(&params, "run_id")?).await?)
        }
        "get_browser_traces" => {
            to_json(caps::get_browser_traces(extract_str(&params, "run_id")?).await?)
        }
        "list_embedded_browsers" => to_json(caps::list_embedded_browsers().await?),
        "unregister_embedded_browser" => {
            to_json(caps::unregister_embedded_browser(extract_str(&params, "endpoint")?).await?)
        }
        "register_embedded_browser" => to_json(
            caps::register_embedded_browser(
                serde_json::from_value(
                    params
                        .get("payload")
                        .cloned()
                        .ok_or_else(|| "missing required param: payload".to_string())?,
                )
                .map_err(|e| format!("invalid embedded registration: {e}"))?,
            )
            .await?,
        ),
        "browser_user_interact" => to_json(
            caps::browser_user_interact(
                extract_str(&params, "run_id")?,
                extract_str(&params, "action")?,
                params
                    .get("params")
                    .cloned()
                    .ok_or_else(|| "missing required param: params".to_string())?,
            )
            .await?,
        ),
        "get_browser_use_binding" => to_json(caps::get_browser_use_binding()?),
        "prepare_browser_runtime" => {
            to_json(caps::prepare_browser_runtime_impl(state.emitter.clone()).await?)
        }
        "set_browser_use_binding" => to_json(caps::set_browser_use_binding(extract_bool(
            &params, "enabled",
        )?)),
        "get_desktop_use_binding" => to_json(caps::get_desktop_use_binding()?),
        "set_desktop_use_binding" => to_json(caps::set_desktop_use_binding(extract_bool(
            &params, "enabled",
        )?)),
        "get_desktop_use_status" => to_json(caps::get_desktop_use_status().await?),
        "refresh_desktop_use_status" => to_json(caps::refresh_desktop_use_status().await?),
        "request_desktop_use_permissions" => {
            to_json(caps::request_desktop_use_permissions().await?)
        }
        "open_desktop_permission_pane" => {
            to_json(caps::open_desktop_permission_pane(extract_str(&params, "kind")?).await?)
        }

        // Skills
        "get_skill_bindings" => to_json(caps::get_skill_bindings()?),
        "get_pi_skill_bindings" => to_json(caps::get_pi_skill_bindings()?),
        "list_skills" => to_json(caps::list_skills(opt_typed::<String>(&params, "cwd")?)?),
        "list_pi_skills" => to_json(caps::list_pi_skills(opt_typed::<String>(&params, "cwd")?)?),
        "list_pi_commands" => to_json(caps::list_pi_commands(opt_typed::<String>(
            &params, "cwd",
        )?)?),
        "toggle_skill_binding" => to_json(caps::toggle_skill_binding(
            extract_str(&params, "skill_id")?,
            extract_bool(&params, "enabled")?,
        )),
        "toggle_pi_skill_binding" => to_json(caps::toggle_pi_skill_binding(
            extract_str(&params, "skill_id")?,
            extract_bool(&params, "enabled")?,
        )),
        "create_pi_skill" => to_json(caps::create_pi_skill(
            extract_str(&params, "name")?,
            extract_str(&params, "description")?,
            extract_str(&params, "content")?,
            extract_str(&params, "scope")?,
            opt_typed::<String>(&params, "cwd")?,
        )),
        "delete_pi_skill" => to_json(caps::delete_pi_skill(extract_str(&params, "path")?)),
        "create_shared_skill" => to_json(caps::create_shared_skill(
            extract_str(&params, "name")?,
            extract_str(&params, "description")?,
            extract_str(&params, "content")?,
            extract_str(&params, "scope")?,
            opt_typed::<String>(&params, "cwd")?,
        )),
        "delete_shared_skill" => to_json(caps::delete_shared_skill(extract_str(&params, "path")?)),

        // Pi extensions / profiles
        "list_pi_installed_plugins" => {
            to_json(caps::list_pi_installed_plugins(opt_typed::<String>(
                &params, "cwd",
            )?)?)
        }
        "list_pi_shared_extensions" => to_json(caps::list_pi_shared_extensions(extract_str(
            &params, "mode",
        )?)?),
        "get_pi_profile_info" => to_json(caps::get_pi_profile_info(extract_str(&params, "mode")?)?),
        "list_pi_profile_extensions" => to_json(caps::list_pi_profile_extensions(
            extract_str(&params, "mode")?,
            opt_typed::<String>(&params, "cwd")?,
        )?),
        "install_pi_extension" => to_json(
            caps::install_pi_extension(
                extract_str(&params, "source")?,
                extract_str(&params, "scope")?,
                opt_typed::<String>(&params, "cwd")?,
            )
            .await?,
        ),
        "install_pi_profile_extension" => to_json(
            caps::install_pi_profile_extension(
                extract_str(&params, "mode")?,
                extract_str(&params, "source")?,
                extract_str(&params, "scope")?,
                opt_typed::<String>(&params, "cwd")?,
            )
            .await?,
        ),
        "install_pi_shared_extension" => {
            to_json(caps::install_pi_shared_extension(extract_str(&params, "source")?).await?)
        }
        "install_work_pi_extension" => to_json(
            caps::install_work_pi_extension(
                extract_str(&params, "package_name")?,
                opt_typed::<String>(&params, "display_name")?,
                opt_typed::<String>(&params, "description")?,
            )
            .await?,
        ),
        "uninstall_pi_extension" => to_json(
            caps::uninstall_pi_extension(
                extract_str(&params, "source")?,
                extract_str(&params, "scope")?,
                opt_typed::<String>(&params, "cwd")?,
            )
            .await?,
        ),
        "uninstall_pi_profile_extension" => to_json(
            caps::uninstall_pi_profile_extension(
                extract_str(&params, "mode")?,
                extract_str(&params, "source")?,
                extract_str(&params, "scope")?,
                opt_typed::<String>(&params, "cwd")?,
            )
            .await?,
        ),
        "uninstall_pi_shared_extension" => to_json(
            caps::uninstall_pi_shared_extension(extract_str(&params, "extension_id")?).await?,
        ),
        "uninstall_work_pi_extension" => to_json(caps::uninstall_work_pi_extension(extract_str(
            &params, "id",
        )?)),
        "update_pi_extension" => to_json(
            caps::update_pi_extension(
                extract_str(&params, "source")?,
                extract_str(&params, "scope")?,
                opt_typed::<String>(&params, "cwd")?,
            )
            .await?,
        ),
        "update_pi_profile_extension" => to_json(
            caps::update_pi_profile_extension(
                extract_str(&params, "mode")?,
                extract_str(&params, "source")?,
                extract_str(&params, "scope")?,
                opt_typed::<String>(&params, "cwd")?,
            )
            .await?,
        ),
        "update_pi_shared_extension" => {
            to_json(caps::update_pi_shared_extension(extract_str(&params, "extension_id")?).await?)
        }
        "toggle_pi_extension" => to_json(caps::toggle_pi_extension(
            extract_str(&params, "source")?,
            extract_str(&params, "scope")?,
            extract_bool(&params, "enabled")?,
            opt_typed::<String>(&params, "cwd")?,
        )),
        "toggle_pi_profile_extension" => to_json(caps::toggle_pi_profile_extension(
            extract_str(&params, "mode")?,
            extract_str(&params, "source")?,
            extract_str(&params, "scope")?,
            extract_bool(&params, "enabled")?,
            opt_typed::<String>(&params, "cwd")?,
        )),
        "toggle_pi_shared_extension" => to_json(caps::toggle_pi_shared_extension(
            extract_str(&params, "mode")?,
            extract_str(&params, "extension_id")?,
            extract_bool(&params, "enabled")?,
        )),
        "fetch_pi_packages" => to_json(
            caps::fetch_pi_packages(
                opt_typed::<String>(&params, "query")?,
                opt_typed::<String>(&params, "sort")?,
                opt_typed::<String>(&params, "package_type")?,
            )
            .await?,
        ),

        // MCP catalog / bindings
        "get_mcp_bindings" => to_json(caps::get_mcp_bindings()?),
        "list_mcp_catalog" => to_json(caps::list_mcp_catalog()?),
        "save_mcp_catalog_server" => to_json(caps::save_mcp_catalog_server(extract_typed(
            &params, "server",
        )?)),
        "delete_mcp_catalog_server" => to_json(caps::delete_mcp_catalog_server(extract_str(
            &params,
            "server_id",
        )?)),
        "set_mcp_binding" => to_json(caps::set_mcp_binding(
            extract_str(&params, "server_id")?,
            extract_bool(&params, "enabled")?,
            opt_typed::<String>(&params, "secret_ref")?,
            opt_typed::<bool>(&params, "clear_secret_ref")?,
        )),

        // Connector catalog / bindings
        "get_connector_bindings" => to_json(caps::get_connector_bindings()?),
        "list_connector_catalog" => to_json(caps::list_connector_catalog()?),
        "save_connector_catalog_item" => to_json(caps::save_connector_catalog_item(extract_typed(
            &params, "item",
        )?)),
        "delete_connector_catalog_item" => to_json(caps::delete_connector_catalog_item(
            extract_str(&params, "connector_id")?,
        )),
        "set_connector_binding" => to_json(caps::set_connector_binding(
            extract_str(&params, "connector_id")?,
            opt_typed::<String>(&params, "connection_id")?,
            opt_typed::<bool>(&params, "clear_connection_id")?,
            extract_bool(&params, "enabled")?,
            opt_typed::<String>(&params, "permission_profile")?,
        )),

        // Host secrets
        "list_host_secret_refs" => to_json(caps::list_host_secret_refs()?),
        "set_host_secret" => to_json(caps::set_host_secret(
            extract_str(&params, "secret_ref")?,
            extract_typed(&params, "secret_map")?,
        )),
        "delete_host_secret" => to_json(caps::delete_host_secret(extract_str(
            &params,
            "secret_ref",
        )?)),

        // ── Agent Plugins (WorkBuddy Experts & Teams) ──
        "list_agent_plugins" => to_json(crate::commands::agent_plugins::list_agent_plugins()?),
        "install_agent_plugin" => {
            let source = extract_str(&params, "source")?;
            to_json(crate::commands::agent_plugins::install_agent_plugin(source).await?)
        }
        "update_agent_plugin" => {
            let plugin_id = extract_str(&params, "plugin_id")?;
            to_json(crate::commands::agent_plugins::update_agent_plugin(plugin_id).await?)
        }
        "uninstall_agent_plugin" => {
            let plugin_id = extract_str(&params, "plugin_id")?;
            crate::commands::agent_plugins::uninstall_agent_plugin(plugin_id)?;
            to_json(serde_json::json!({ "ok": true }))
        }
        "set_agent_plugin_trust" => {
            let plugin_id = extract_str(&params, "plugin_id")?;
            let trusted = extract_bool(&params, "trusted")?;
            to_json(crate::commands::agent_plugins::set_agent_plugin_trust(
                plugin_id, trusted,
            )?)
        }
        "get_agent_plugin_bindings" => {
            to_json(crate::commands::agent_plugins::get_agent_plugin_bindings()?)
        }
        "set_agent_plugin_binding" => {
            let plugin_id = extract_str(&params, "plugin_id")?;
            let enabled = extract_bool(&params, "enabled")?;
            crate::commands::agent_plugins::set_agent_plugin_binding(plugin_id, enabled)?;
            to_json(serde_json::json!({ "ok": true }))
        }
        "sync_from_twork" => to_json(crate::commands::agent_plugins::sync_from_twork().await?),

        // ── Desktop-capability commands (P6 parity) ──
        // The pure/headless-compatible ones are implemented here; capture /
        // hotkey / update-check stay rejected here because the Electron main
        // process intercepts them before the call ever reaches the core.
        "get_clipboard_files" => to_json(crate::commands::clipboard::get_clipboard_files()?),
        "run_pi_logout" => to_json(crate::commands::onboarding::run_pi_logout().await?),
        "run_codex_logout" => to_json(crate::commands::onboarding::run_codex_logout().await?),
        "run_codex_subscription_login" => {
            to_json(crate::agent::codex_subscription_bridge::sign_in().await?)
        }
        "reopen_codex_subscription_login" => {
            to_json(crate::agent::codex_subscription_bridge::reopen_sign_in().await?)
        }
        "run_codex_subscription_logout" => {
            to_json(crate::agent::codex_subscription_bridge::clear_subscription_auth()?)
        }
        "run_claude_login" => to_json(
            crate::commands::onboarding::run_claude_login_impl(state.emitter.clone()).await?,
        ),
        "run_codex_login" => {
            to_json(crate::commands::onboarding::run_codex_login_impl(state.emitter.clone()).await?)
        }
        "run_grok_login" => {
            to_json(crate::commands::onboarding::run_grok_login_impl(state.emitter.clone()).await?)
        }
        "send_chat_message" => {
            let attachments: Option<Vec<crate::models::Attachment>> =
                opt_typed(&params, "attachments")?;
            crate::commands::chat::send_chat_message_inner(
                state.process_map.clone(),
                state.emitter.clone(),
                state.cancel_token.clone(),
                extract_str(&params, "run_id")?,
                extract_str(&params, "message")?,
                attachments,
                opt_typed(&params, "model")?,
                opt_typed(&params, "client_uuid")?,
                opt_typed(&params, "permission_mode_override")?,
            )
            .await?;
            to_json(serde_json::json!({ "ok": true }))
        }

        // ── Custom Pets ──
        "list_custom_pets" => {
            let result = crate::commands::pets::list_custom_pets()?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_custom_pet" => {
            let id = extract_str(&params, "id")?;
            let result = crate::commands::pets::get_custom_pet(id)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "inspect_custom_pet_zip" => {
            let zip_base64 = extract_str(&params, "zip_base64")
                .or_else(|_| extract_str(&params, "zipBase64"))?;
            let result = crate::commands::pets::inspect_custom_pet_zip(zip_base64)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "inspect_custom_pet_image" => {
            let image_base64 = extract_str(&params, "image_base64")
                .or_else(|_| extract_str(&params, "imageBase64"))?;
            let image_name = extract_str(&params, "image_name")
                .or_else(|_| extract_str(&params, "imageName"))?;
            let result = crate::commands::pets::inspect_custom_pet_image(image_base64, image_name)?;
            Ok(json!(result))
        }
        "create_custom_pet" => {
            let input_val = params
                .get("input")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let input: crate::commands::pets::CreateCustomPetInput =
                serde_json::from_value(input_val).map_err(|e| e.to_string())?;
            let result = crate::commands::pets::create_custom_pet(input)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "open_custom_pets_folder" => {
            crate::commands::pets::open_custom_pets_folder()?;
            Ok(json!(true))
        }

        // capture_screenshot / update_screenshot_hotkey / check_for_updates:
        // intercepted by the Electron main process (P5/P6). On the remotely
        // reachable web server they remain rejected as desktop-only.
        "capture_screenshot" | "update_screenshot_hotkey" | "check_for_updates" => {
            Err("desktop only".to_string())
        }

        // ── Explicitly blocked ──
        "load_run_data" => Err("unknown method".to_string()),

        // ── IPC-only (not exposed over WS) ──
        // In Electron the main process answers this itself; the web server
        // never exposes its auth token to pages.
        "get_web_server_token" => Err("desktop only".to_string()),

        _ => Err(format!("unknown method: {}", method)),
    };

    let elapsed = start.elapsed();
    if elapsed.as_millis() > 100 {
        log::debug!(
            "[dispatch] method={} took {}ms",
            method,
            elapsed.as_millis()
        );
    }

    result
}

// ── Parameter extraction helpers ──

fn extract_str(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| format!("missing required param: {}", key))
}

fn extract_u64(params: &Value, key: &str) -> Result<u64, String> {
    params
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("missing required param: {}", key))
}

fn extract_bool(params: &Value, key: &str) -> Result<bool, String> {
    params
        .get(key)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| format!("missing required param: {}", key))
}

fn extract_typed<T: serde::de::DeserializeOwned>(params: &Value, key: &str) -> Result<T, String> {
    let v = params
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing required param: {}", key))?;
    serde_json::from_value(v).map_err(|e| format!("invalid param {}: {}", key, e))
}

/// Serialize a command result to a JSON `Value` for the dispatcher response.
fn to_json<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// Required param: deserialize by key, error when absent.
fn typed_param<T: serde::de::DeserializeOwned>(params: &Value, key: &str) -> Result<T, String> {
    let value = params
        .get(key)
        .ok_or_else(|| format!("missing param: {key}"))?;
    serde_json::from_value(value.clone()).map_err(|e| format!("invalid param {key}: {e}"))
}

/// Optional param: absent key or explicit null both map to `None`.
fn opt_typed<T: serde::de::DeserializeOwned>(
    params: &Value,
    key: &str,
) -> Result<Option<T>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => extract_typed::<Option<T>>(params, key),
    }
}

/// Normalize top-level camelCase keys to snake_case.
/// Does NOT recurse into nested objects (preserving CLI protocol payloads).
fn normalize_top_level_keys(params: Value) -> Value {
    match params {
        Value::Object(map) => {
            let normalized: serde_json::Map<String, Value> = map
                .into_iter()
                .map(|(k, v)| (camel_to_snake(&k), v))
                .collect();
            Value::Object(normalized)
        }
        other => other,
    }
}

// ── Inline _impl functions for State-dependent commands ──

/// stop_run logic extracted from commands::runs::stop_run
async fn stop_run_impl(id: String, state: &AppState) -> Result<bool, String> {
    use crate::agent::session_actor::ActorCommand;
    use crate::models::RunStatus;

    log::debug!("[dispatch] stop_run_impl: id={}", id);

    // Try actor session first
    let actor_stopped = {
        let handle = {
            let mut map = state.sessions.lock().await;
            map.remove(&id)
        };
        if let Some(handle) = handle {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            if handle
                .cmd_tx
                .send(ActorCommand::Stop {
                    reason: crate::agent::session_actor::RuntimeStopReason::UserStop,
                    reply: reply_tx,
                })
                .await
                .is_ok()
            {
                let _ = reply_rx.await;
            }
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(5), handle.join_handle).await;
            true
        } else {
            false
        }
    };

    if actor_stopped {
        if let Err(e) = crate::storage::runs::update_status(
            &id,
            RunStatus::Stopped,
            None,
            Some("Stopped by user".to_string()),
        ) {
            log::warn!("[dispatch] stop_run: failed to update status: {}", e);
        }
        return Ok(true);
    }

    // Fall through to pipe mode (Codex)
    crate::agent::stream::stop_process(&state.process_map, &id).await;
    if let Err(e) = crate::storage::runs::update_status(
        &id,
        RunStatus::Stopped,
        None,
        Some("Stopped by user".to_string()),
    ) {
        log::warn!("[dispatch] stop_run: failed to update status: {}", e);
    }
    Ok(true)
}

/// Convert a camelCase string to snake_case
fn camel_to_snake(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("runId"), "run_id");
        assert_eq!(camel_to_snake("sessionId"), "session_id");
        assert_eq!(camel_to_snake("sinceSeq"), "since_seq");
        assert_eq!(camel_to_snake("run_id"), "run_id"); // already snake_case
        assert_eq!(camel_to_snake("id"), "id");
    }

    #[test]
    fn test_update_cli_permissions_missing_rules_param() {
        // Simulate the dispatch param extraction for update_cli_permissions.
        // When "rules" key is absent, dispatch should produce an error.
        let params = json!({ "scope": "user", "category": "allow" });
        let result: Result<Vec<String>, String> = params
            .get("rules")
            .ok_or_else(|| "Missing required parameter: rules".to_string())
            .and_then(|v| {
                serde_json::from_value::<Vec<String>>(v.clone())
                    .map_err(|e| format!("Invalid rules: {}", e))
            });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Missing required parameter: rules");
    }

    #[test]
    fn test_update_cli_permissions_invalid_rules_type() {
        // "rules" present but wrong type (string instead of array)
        let params = json!({ "scope": "user", "category": "allow", "rules": "not-an-array" });
        let result: Result<Vec<String>, String> = params
            .get("rules")
            .ok_or_else(|| "Missing required parameter: rules".to_string())
            .and_then(|v| {
                serde_json::from_value::<Vec<String>>(v.clone())
                    .map_err(|e| format!("Invalid rules: {}", e))
            });
        assert!(result.is_err());
        assert!(result.unwrap_err().starts_with("Invalid rules:"));
    }

    #[test]
    fn test_normalize_top_level_only() {
        let input = json!({
            "runId": "abc",
            "params": {
                "nestedCamel": "should_not_change"
            }
        });
        let output = normalize_top_level_keys(input);
        assert_eq!(output.get("run_id").unwrap(), "abc");
        // Nested keys should NOT be converted
        let nested = output.get("params").unwrap();
        assert!(nested.get("nestedCamel").is_some());
        assert!(nested.get("nested_camel").is_none());
    }

    #[tokio::test]
    async fn respond_user_input_is_registered_on_the_json_rpc_bridge() {
        let runtime = std::sync::Arc::new(crate::core::CoreRuntime::new());
        let state = runtime.app_state(std::sync::Arc::new("127.0.0.1".into()), None);
        let result = dispatch_command(
            "respond_user_input",
            json!({ "runId": "missing-run", "requestId": "question-1" }),
            &state,
        )
        .await;

        assert_eq!(result.unwrap_err(), "respond_user_input: missing 'answers'");
    }

    #[test]
    fn test_search_runs_filters_deserialization() {
        let raw = serde_json::json!({
            "filters": {
                "dateFrom": "2024-01-01",
                "costMin": 0.5,
                "statuses": ["completed", "failed"],
                "sortBy": "cost"
            }
        });
        let params = normalize_top_level_keys(raw);
        let filters_val = params.get("filters").unwrap().clone();
        let filters: crate::models::RunSearchFilters = serde_json::from_value(filters_val).unwrap();
        assert_eq!(filters.date_from.unwrap(), "2024-01-01");
        assert!(filters.cost_min.unwrap() > 0.4);
        assert_eq!(
            filters.statuses.unwrap(),
            vec![
                crate::models::RunStatus::Completed,
                crate::models::RunStatus::Failed
            ]
        );
        assert_eq!(filters.sort_by.unwrap(), "cost");
    }

    #[tokio::test]
    async fn test_pty_and_editor_dispatch() {
        let runtime = std::sync::Arc::new(crate::core::CoreRuntime::new());
        let state = runtime.app_state(std::sync::Arc::new("127.0.0.1".into()), None);

        // 1. check_vscode_available
        let res = dispatch_command("check_vscode_available", serde_json::json!({}), &state).await;
        assert!(res.is_ok(), "check_vscode_available failed: {:?}", res);

        // 2. pty lifecycle
        let session_id = "test-pty-dispatch-session";
        let create_params = serde_json::json!({
            "sessionId": session_id,
            "cwd": std::env::current_dir().unwrap().to_string_lossy().to_string(),
            "cols": 80,
            "rows": 24
        });
        let res = dispatch_command("pty_create", create_params, &state).await;
        assert!(res.is_ok(), "pty_create failed: {:?}", res);

        let write_params = serde_json::json!({
            "sessionId": session_id,
            "data": "echo hello\n"
        });
        let res = dispatch_command("pty_write", write_params, &state).await;
        assert!(res.is_ok(), "pty_write failed: {:?}", res);

        let resize_params = serde_json::json!({
            "sessionId": session_id,
            "cols": 100,
            "rows": 30
        });
        let res = dispatch_command("pty_resize", resize_params, &state).await;
        assert!(res.is_ok(), "pty_resize failed: {:?}", res);

        let kill_params = serde_json::json!({
            "sessionId": session_id
        });
        let res = dispatch_command("pty_kill", kill_params, &state).await;
        assert!(res.is_ok(), "pty_kill failed: {:?}", res);
    }
}
