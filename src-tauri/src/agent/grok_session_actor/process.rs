use super::actor_loop;
use crate::agent::adapter::{self, ActorSessionMap};
use crate::agent::claude_stream::which_binary;
use crate::agent::session_actor::{ActorCommand, SessionActorHandle};
use crate::models::GlobalProviderModel;
use crate::process_ext::HideConsole;
use crate::web_server::broadcaster::BroadcastEmitter;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Resolve the Grok Build executable used by both ACP sessions and settings diagnostics.
/// Persistent AgentCabin settings win; `GROK_BINARY` remains an environment-level escape hatch.
pub fn resolve_grok_path() -> String {
    if let Some(custom) = crate::storage::settings::load()
        .user
        .grok_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return custom;
    }
    if let Ok(value) = std::env::var("GROK_BINARY") {
        let value = value.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    which_binary("grok").unwrap_or_else(|| "grok".to_string())
}

fn build_spawn_args(permission_mode: &str, effort: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "--no-auto-update".to_string(),
        "--permission-mode".to_string(),
        permission_mode.to_string(),
    ];
    args.push("agent".to_string());
    if let Some(effort) = effort.map(str::trim).filter(|effort| !effort.is_empty()) {
        args.extend(["--reasoning-effort".to_string(), effort.to_string()]);
    }
    args.push("stdio".to_string());
    args
}

fn resolve_permission_mode(persisted_mode: Option<&str>, override_mode: Option<&str>) -> String {
    override_mode
        .or(persisted_mode)
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .unwrap_or("default")
        .to_string()
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    model: Option<String>,
    resume_session_id: Option<String>,
    continuation_context: Option<String>,
    permission_mode_override: Option<String>,
    cancel: CancellationToken,
    app_mode: crate::work::models::AppMode,
    desktop_runtime: Option<(u16, String)>,
) -> Result<mpsc::Sender<ActorCommand>, String> {
    let caps = crate::agent::capability_resolver::CapabilityResolver::resolve(
        &crate::storage::data_dir(),
        app_mode,
        crate::agent::capability_resolver::RuntimeProviderKind::Grok,
        &cwd,
        &run_id,
    )
    .await?;

    let adapter = crate::agent::runtime_providers::get_adapter(
        crate::agent::capability_resolver::RuntimeProviderKind::Grok,
    );
    let spawn_cfg = adapter.prepare_runtime(&caps)?;
    let _ = crate::work::capability_projection::record_launch_snapshot(
        &caps,
        &caps.managed_runtime_dir,
    );

    let persisted_settings = crate::storage::settings::get_agent_settings("grok");
    let mut grok_plugin_dirs = super::plugin_dirs::resolve_grok_plugin_dirs(
        persisted_settings.grok_plugin_dirs.as_deref(),
        Path::new(&cwd),
    );
    let projected_skills_path = caps.managed_runtime_dir.join("skills");
    if projected_skills_path.is_dir() {
        let path_str = projected_skills_path.to_string_lossy().to_string();
        if !grok_plugin_dirs.contains(&path_str) {
            grok_plugin_dirs.push(path_str);
        }
    }

    log::debug!(
        "[grok] resolved {} explicitly selected plugin dirs for session",
        grok_plugin_dirs.len()
    );
    let user_settings = crate::storage::settings::get_user_settings();
    let adapter_settings =
        adapter::build_adapter_settings(&persisted_settings, &user_settings, model.clone());
    // Grok exposes permission policy as a process-start option. A session-scoped
    // override is used by the UI when it restarts the current ACP actor; the
    // persisted agent setting remains the fallback for ordinary starts.
    let permission_mode = resolve_permission_mode(
        adapter_settings.permission_mode.as_deref(),
        permission_mode_override.as_deref(),
    );
    let mut cmd = Command::new(&spawn_cfg.binary);
    let managed_grok_home = caps.managed_runtime_dir.clone();
    let effective_grok_home = Some(managed_grok_home.clone());
    let mut custom_provider_active = false;
    let mut custom_model_capabilities: Option<GlobalProviderModel> = None;
    cmd.args(build_spawn_args(
        &permission_mode,
        persisted_settings.effort.as_deref(),
    ))
    .current_dir(&cwd)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .env_clear()
    .envs(&spawn_cfg.env)
    .env_remove("CLAUDECODE")
    .hide_console()
    .kill_on_drop(true);
    if let Some((port, token)) = desktop_runtime.as_ref() {
        cmd.env("AGENTCABIN_DESKTOP_BRIDGE_PORT", port.to_string());
        cmd.env("AGENTCABIN_DESKTOP_BRIDGE_TOKEN", token);
    }

    // When the user bound a global provider to Grok, route the spawned Grok process
    // through an AgentCabin-managed config (GROK_HOME) instead of xAI's default API.
    // Grok Build does NOT recognize `XAI_BASE_URL`; its model routing lives in
    // `[model.<id>]` (base_url / env_key / api_backend). Generating a managed config
    // keeps the user's real `~/.grok/config.toml` untouched and makes the bound
    // provider authoritative for this session.
    //
    // The model is resolved from the authoritative persisted run metadata, falling
    // back to the binding's declared model. `run.model` is the explicit per-session
    // selection (picked in the composer / hot-switched via update_run_model); using
    // the persisted value guarantees the managed config matches what the user last
    // selected even when a warm session is restarted without a fresh model argument.
    let settings = crate::storage::settings::load();
    let grok_binding = settings
        .user
        .agent_provider_bindings
        .as_ref()
        .map(|bindings| bindings.grok.clone());
    if let Some(binding) = grok_binding {
        if binding.mode == "custom" {
            if let Some(provider_id) = binding.provider_id.as_deref() {
                if let Some(provider) = settings
                    .user
                    .global_providers
                    .iter()
                    .find(|candidate| candidate.id == provider_id)
                    .cloned()
                {
                    let provider =
                        crate::agent::codex_subscription_bridge::prepare_global_provider(&provider)
                            .await?;
                    let persisted_model = crate::storage::runs::get_run(&run_id)
                        .and_then(|meta| meta.model)
                        .filter(|value| !value.trim().is_empty());
                    let run_model = persisted_model
                        .as_deref()
                        .or(model.as_deref().map(str::trim).filter(|v| !v.is_empty()))
                        .or(binding.model.as_deref())
                        .map(str::trim)
                        .filter(|value| !value.is_empty());
                    let provider_default_model = provider
                        .models
                        .as_ref()
                        .and_then(|models| models.first().map(|m| m.id.as_str()));
                    let resolved_model = run_model.or(provider_default_model).unwrap_or("default");
                    let provider_model = provider
                        .models
                        .as_ref()
                        .and_then(|models| {
                            models
                                .iter()
                                .find(|candidate| candidate.id == resolved_model)
                        })
                        .cloned()
                        .or_else(|| {
                            provider
                                .supports_reasoning_effort
                                .unwrap_or(false)
                                .then(|| GlobalProviderModel {
                                    id: resolved_model.to_string(),
                                    name: None,
                                    context_window: None,
                                    max_tokens: None,
                                    supports_reasoning: Some(true),
                                    supports_xhigh: None,
                                    supported_effort_levels: None,
                                    supports_images: None,
                                })
                        });

                    let grok_home = super::config::write_managed_config_at(
                        &managed_grok_home,
                        &provider,
                        resolved_model,
                        &caps.mcp_servers,
                    )?;
                    cmd.env("GROK_HOME", &grok_home);
                    custom_provider_active = true;
                    custom_model_capabilities = provider_model;
                    // Grok's `[model.<id>].env_key` points at XAI_API_KEY, so
                    // inject the bound provider's key under that name. This also
                    // satisfies Grok's `after_initialize` auth probe which checks
                    // XAI_API_KEY to pick the `xai.api_key` auth method.
                    if let Some(api_key) =
                        provider.api_key.as_ref().filter(|k| !k.trim().is_empty())
                    {
                        cmd.env("XAI_API_KEY", api_key);
                    }
                    // Defensive: clear any user-set XAI_BASE_URL so it cannot
                    // leak into child env (Grok ignores it, but be explicit).
                    cmd.env_remove("XAI_BASE_URL");

                    if let Some(extra) = &provider.extra_env {
                        for (k, v) in extra {
                            if !crate::agent::capability_resolver::is_reserved_isolation_env(k) {
                                cmd.env(k, v);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "Grok Build CLI was not found. Install it, configure the Grok CLI path in AgentCabin, or set GROK_BINARY."
                    .to_string()
            } else {
                format!("Failed to start Grok Build ACP mode: {error}")
            }
        })?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Grok ACP stdin was not captured".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Grok ACP stdout was not captured".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Grok ACP stderr was not captured".to_string())?;

    // Resolve the model the actor loop will pass to `session/new _meta.modelId`.
    // This MUST match the model written into the managed config above so Grok's
    // reported currentModelId stays consistent with `[model.<id>]`. Prefer the
    // persisted run model (authoritative, updated by handleModelChange) over the
    // spawn-time argument, which may lag behind when a warm session is restarted.
    let actor_model = {
        let persisted = crate::storage::runs::get_run(&run_id)
            .and_then(|meta| meta.model)
            .filter(|value| !value.trim().is_empty());
        persisted.or_else(|| model.clone().filter(|v| !v.trim().is_empty()))
    };
    log::debug!(
        "[grok] spawn_actor: run_id={}, actor_model={:?}, spawn_model_arg={:?}",
        run_id,
        actor_model,
        model
    );

    let tag = Arc::new(());
    let (cmd_tx, cmd_rx) = mpsc::channel::<ActorCommand>(64);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (start_tx, start_rx) = oneshot::channel();
    let task_tag = tag.clone();
    let task_sessions = sessions.clone();
    let task_run_id = run_id.clone();
    let actor_desktop_token = desktop_runtime.as_ref().map(|(_, token)| token.clone());

    let join_handle = tokio::spawn(async move {
        if start_rx.await.is_err() {
            return;
        }
        actor_loop::run_actor(
            emitter,
            task_sessions,
            task_run_id,
            cwd,
            effective_grok_home,
            custom_provider_active,
            custom_model_capabilities,
            actor_model,
            permission_mode,
            resume_session_id,
            continuation_context,
            grok_plugin_dirs,
            task_tag,
            child,
            stdin,
            stdout,
            stderr,
            cmd_rx,
            cancel,
            shutdown_tx,
            actor_desktop_token,
        )
        .await;
    });

    let sender = cmd_tx.clone();
    sessions.lock().await.insert(
        run_id.clone(),
        SessionActorHandle {
            cmd_tx,
            run_id: run_id.clone(),
            tag,
            join_handle,
            shutdown_rx,
            work_bridge_token: None,
            desktop_runtime_token: desktop_runtime.as_ref().map(|(_, token)| token.clone()),
        },
    );

    if start_tx.send(()).is_err() {
        sessions.lock().await.remove(&run_id);
        if let Some((_, token)) = desktop_runtime.as_ref() {
            crate::desktop_runtime::revoke_token(token).await;
        }
        return Err("Grok ACP actor failed to enter its process loop".to_string());
    }

    Ok(sender)
}

#[cfg(test)]
mod tests {
    use super::{build_spawn_args, resolve_permission_mode};

    #[test]
    fn session_override_wins_over_persisted_permission_policy() {
        assert_eq!(
            resolve_permission_mode(Some("bypassPermissions"), Some("default")),
            "default"
        );
    }

    #[test]
    fn starts_grok_stdio_with_persisted_permission_policy() {
        assert_eq!(
            build_spawn_args("acceptEdits", None),
            vec![
                "--no-auto-update".to_string(),
                "--permission-mode".to_string(),
                "acceptEdits".to_string(),
                "agent".to_string(),
                "stdio".to_string(),
            ]
        );
    }

    #[test]
    fn default_mode_leaves_grok_cli_on_its_interactive_default() {
        assert_eq!(
            build_spawn_args("default", None),
            vec![
                "--no-auto-update".to_string(),
                "--permission-mode".to_string(),
                "default".to_string(),
                "agent".to_string(),
                "stdio".to_string(),
            ]
        );
    }

    #[test]
    fn supports_the_three_grok_permission_modes() {
        assert_eq!(build_spawn_args("default", None).len(), 5);
        for mode in ["auto", "acceptEdits", "bypassPermissions"] {
            let args = build_spawn_args(mode, None);
            assert_eq!(args.get(2).map(String::as_str), Some(mode));
        }
    }

    #[test]
    fn adds_startup_reasoning_effort_without_changing_stdio_command() {
        assert_eq!(
            build_spawn_args("default", Some("high")),
            vec![
                "--no-auto-update".to_string(),
                "--permission-mode".to_string(),
                "default".to_string(),
                "agent".to_string(),
                "--reasoning-effort".to_string(),
                "high".to_string(),
                "stdio".to_string(),
            ]
        );
    }
}
