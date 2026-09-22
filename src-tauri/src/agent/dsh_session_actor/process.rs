//! DSH child process spawn boundary.

use super::{acp_actor, actor_loop};
use crate::agent::adapter::ActorSessionMap;
use crate::agent::capability_resolver::{CapabilityResolver, RuntimeProviderKind};
use crate::agent::runtime_providers::dsh::{DshRouteConfig, DshRuntimeAdapter};
use crate::agent::session_actor::{ActorCommand, SessionActorHandle};
use crate::process_ext::HideConsole;
use crate::web_server::broadcaster::BroadcastEmitter;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DshProtocol {
    Sdk,
    Acp,
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    requested_model: Option<String>,
    cancel_token: CancellationToken,
    desktop_runtime: Option<(u16, String)>,
    extra_env: HashMap<String, String>,
    work_bridge_token: Option<String>,
    session_id: Option<String>,
    app_mode: crate::work::models::AppMode,
) -> Result<mpsc::Sender<ActorCommand>, String> {
    spawn_actor_with_protocol(
        emitter,
        sessions,
        run_id,
        cwd,
        requested_model,
        cancel_token,
        desktop_runtime,
        extra_env,
        work_bridge_token,
        session_id,
        // Both surfaces use the same official Harness Session Controller
        // bridge. The bridge keeps an ACP-shaped stdio boundary for this
        // actor, but owns the real Web Harness session internally.
        DshProtocol::Acp,
        app_mode,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_actor_with_protocol(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    requested_model: Option<String>,
    cancel_token: CancellationToken,
    desktop_runtime: Option<(u16, String)>,
    extra_env: HashMap<String, String>,
    work_bridge_token: Option<String>,
    session_id: Option<String>,
    protocol: DshProtocol,
    app_mode: crate::work::models::AppMode,
) -> Result<mpsc::Sender<ActorCommand>, String> {
    let caps = CapabilityResolver::resolve(
        &crate::storage::data_dir(),
        app_mode,
        RuntimeProviderKind::Dsh,
        &cwd,
        &run_id,
    )
    .await?;

    let route = resolve_route(requested_model.as_deref())?;
    let spawn_cfg =
        DshRuntimeAdapter.prepare_runtime_for_route_with_env(&caps, &route, &extra_env)?;
    let effort = crate::storage::runs::get_run(&run_id)
        .and_then(|run| run.effort)
        .or_else(|| crate::storage::settings::get_agent_settings("dsh").effort)
        .filter(|value| !value.trim().is_empty());

    let mut cmd = Command::new(&spawn_cfg.binary);
    cmd.args(&spawn_cfg.args);
    let work_dir = if Path::new(&cwd).is_dir() {
        Path::new(&cwd)
    } else {
        &spawn_cfg.managed_home
    };
    cmd.current_dir(work_dir);
    for (k, v) in &spawn_cfg.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.hide_console();
    cmd.kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn DSH CLI ({}): {e}", spawn_cfg.binary))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to capture stdin for DSH CLI".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout for DSH CLI".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture stderr for DSH CLI".to_string())?;

    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let tag = Arc::new(());

    let actor_tag = Arc::clone(&tag);
    let actor_run_id = run_id.clone();
    let actor_session_id = session_id.clone().unwrap_or_else(|| run_id.clone());
    let actor_emitter = Arc::clone(&emitter);
    let actor_sessions = sessions.clone();
    let desktop_token = desktop_runtime.as_ref().map(|(_, t)| t.clone());
    let actor_desktop_token = desktop_token.clone();
    let actor_work_bridge_token = work_bridge_token.clone();

    let join_handle = tokio::spawn(async move {
        if protocol == DshProtocol::Acp {
            acp_actor::run_actor(
                actor_emitter,
                actor_sessions,
                actor_run_id,
                if session_id.is_some() {
                    session_id
                } else {
                    None
                },
                actor_tag,
                child,
                stdin,
                stdout,
                stderr,
                cmd_rx,
                cancel_token,
                shutdown_tx,
                actor_desktop_token,
                actor_work_bridge_token,
                cwd,
                route.model.id,
                effort.clone(),
            )
            .await;
        } else {
            actor_loop::run_actor(
                actor_emitter,
                actor_sessions,
                actor_run_id,
                actor_session_id,
                actor_tag,
                child,
                stdin,
                stdout,
                stderr,
                cmd_rx,
                cancel_token,
                shutdown_tx,
                actor_desktop_token,
                actor_work_bridge_token,
                cwd,
                "agentcabin".to_string(),
                route.model.id,
                effort.clone(),
            )
            .await;
        }
    });

    let handle = SessionActorHandle {
        cmd_tx: cmd_tx.clone(),
        run_id: run_id.clone(),
        tag,
        join_handle,
        shutdown_rx,
        work_bridge_token,
        desktop_runtime_token: desktop_token,
    };

    sessions.lock().await.insert(run_id, handle);

    Ok(cmd_tx)
}

fn resolve_route(requested_model: Option<&str>) -> Result<DshRouteConfig, String> {
    let settings = crate::storage::settings::get_user_settings();
    let requested = requested_model
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let compatible = |protocol: &str| {
        matches!(
            protocol,
            "openai-completions" | "openai-responses" | "anthropic-messages"
        )
    };

    // DSH's settings picker is backed by the canonical per-agent binding. Keep
    // a bare Work model request on that selected provider instead of letting a
    // same-named model from another global provider win by accident. An
    // explicit `provider-id/model-id` request remains authoritative below.
    let dsh_binding = settings
        .agent_provider_bindings
        .as_ref()
        .map(|bindings| &bindings.dsh)
        .filter(|binding| binding.mode == "custom");
    let bound_provider = dsh_binding
        .and_then(|binding| binding.provider_id.as_deref())
        .and_then(|provider_id| {
            settings
                .global_providers
                .iter()
                .find(|provider| provider.id == provider_id)
        })
        .filter(|provider| compatible(&provider.protocol));

    let explicitly_selected = requested.and_then(|requested| {
        settings.global_providers.iter().find_map(|provider| {
            if !compatible(&provider.protocol) {
                return None;
            }
            let prefix = format!("{}/", provider.id);
            requested.strip_prefix(&prefix).and_then(|model_id| {
                provider
                    .models
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .find(|model| model.id == model_id)
                    .map(|model| (provider, model))
            })
        })
    });

    let bare_match = requested.and_then(|requested| {
        let mut matches = settings.global_providers.iter().filter_map(|provider| {
            if !compatible(&provider.protocol) {
                return None;
            }
            provider
                .models
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|model| model.id == requested || model.name.as_deref() == Some(requested))
                .map(|model| (provider, model))
        });
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    });

    let bound_match = bound_provider.and_then(|provider| {
        let requested_model = requested.and_then(|requested| {
            if requested.contains('/') {
                requested
                    .strip_prefix(&format!("{}/", provider.id))
                    .or_else(|| requested.strip_prefix(&format!("{}/", provider.name)))
            } else {
                Some(requested)
            }
        });
        let binding_model = dsh_binding.and_then(|binding| {
            binding
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        });
        requested_model
            .into_iter()
            .chain(binding_model)
            .find_map(|candidate| {
                provider
                    .models
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .find(|model| model.id == candidate || model.name.as_deref() == Some(candidate))
            })
            .or_else(|| {
                provider
                    .models
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .find(|model| !model.id.trim().is_empty())
            })
            .map(|model| (provider, model))
    });

    let fallback = settings.global_providers.iter().find_map(|provider| {
        if !compatible(&provider.protocol) {
            return None;
        }
        provider
            .models
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|model| !model.id.trim().is_empty())
            .map(|model| (provider, model))
    });

    let (provider, model) = explicitly_selected
        .or(bound_match)
        .or(bare_match)
        .or(fallback)
        .ok_or_else(|| "DSH requires at least one compatible global Provider model".to_string())?;
    Ok(DshRouteConfig {
        // The official Session Controller addresses providers by their stable
        // provider id, not by the display name shown in Settings.
        provider: provider.id.clone(),
        protocol: provider.protocol.clone(),
        base_url: provider.base_url.clone(),
        api_key: provider.api_key.clone(),
        keyless: provider.keyless.unwrap_or(false),
        model: model.clone(),
    })
}
