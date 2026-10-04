//! Pi RPC process factory.

use crate::agent::adapter::{ActorSessionMap, AdapterSettings};
use crate::agent::claude_stream::{augmented_path, bundled_pi_package_path, resolve_pi_path};
use crate::agent::pi_extensions;
use crate::agent::pi_provider_bridge;
use crate::agent::pi_rpc_protocol::PiRpc;
use crate::agent::session_actor::{ActorCommand, SessionActorHandle};
use crate::agent::session_protocol::{SessionProtocol, StartupCtx};
use crate::process_ext::HideConsole;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::sandbox::{
    ExecutionCommand, WorkSandboxLauncher, WORK_NETWORK_POLICY_ENV, WORK_PROVIDER_NETWORK_POLICY,
};
use crate::work::system_packages::is_system_managed_source;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PiSessionLaunch {
    Resume(String),
    Fork(String),
}

/// Work's explicit FullAccess mode mirrors Codex's dangerous/full-access
/// Work's explicit FullAccess mode mirrors Codex's dangerous/full-access
/// choice: the Pi process itself must not remain inside the Work Seatbelt.
/// Keep this scoped to isolated Work sessions; a non-Work Pi session may use
/// the same provider-facing permission name for unrelated reasons.
fn is_work_full_access(settings: &AdapterSettings) -> bool {
    settings.pi_agent_dir.is_some() && settings.pi_work_full_access
}

fn build_rpc_args(
    settings: &AdapterSettings,
    launch: Option<&PiSessionLaunch>,
) -> Result<Vec<String>, String> {
    if matches!(launch, Some(PiSessionLaunch::Fork(_))) && settings.no_session_persistence {
        return Err("Cannot fork a Pi session when session persistence is disabled".to_string());
    }

    let mut args = vec!["--mode".to_string(), "rpc".to_string()];
    let isolated_work_profile = settings.pi_agent_dir.is_some();
    let managed_code_profile = settings.pi_code_profile_dir.is_some();
    if isolated_work_profile {
        args.extend(
            [
                "--no-extensions",
                "--no-skills",
                "--no-prompt-templates",
                "--no-themes",
                "--no-context-files",
            ]
            .into_iter()
            .map(str::to_string),
        );
    } else {
        // Pi Code keeps project context files, but its extension set is owned by
        // AgentCabin. Only the shared packages explicitly bound to Code are
        // injected below; native ~/.pi/agent extensions never leak in.
        args.push("--no-skills".to_string());
        if managed_code_profile {
            args.push("--no-extensions".to_string());
        }
    }
    let mut explicit_extensions = std::collections::HashSet::new();
    let pi_profile_dir = settings
        .pi_agent_dir
        .as_deref()
        .or(settings.pi_code_profile_dir.as_deref());
    if settings.pi_provider.is_some() || !settings.global_providers.is_empty() {
        args.push("-e".to_string());
        args.push(
            pi_provider_bridge::bridge_path()
                .to_string_lossy()
                .into_owned(),
        );
        if settings.pi_provider.is_some() {
            args.push("--provider".to_string());
            args.push("agentcabin".to_string());
        }
    }
    // Run metadata is the explicit per-session selection. The managed provider model is only
    // the fallback; preferring it here would overwrite a model selected in the composer before
    // the session starts.
    let model = settings
        .model
        .as_deref()
        .or_else(|| {
            settings
                .pi_provider
                .as_ref()
                .map(|provider| provider.model.as_str())
        })
        .filter(|value| !value.trim().is_empty());
    if let Some(model) = model {
        let (provider, model_id) =
            crate::agent::pi_session_actor::actor_loop::resolve_set_model_target(
                model,
                None,
                &settings.global_providers,
                settings.pi_provider.as_ref(),
            );
        if let Some(pos) = args
            .windows(2)
            .position(|pair| pair == ["--provider", "agentcabin"])
        {
            args[pos + 1] = provider;
        } else if !args.windows(2).any(|pair| pair[0] == "--provider") {
            args.push("--provider".to_string());
            args.push(provider);
        }
        args.push("--model".to_string());
        args.push(model_id);
    }
    if let Some(system_prompt) = settings
        .system_prompt
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        args.push("--system-prompt".to_string());
        args.push(system_prompt.to_string());
    } else if let Some(append_system_prompt) = settings
        .append_system_prompt
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        args.push("--append-system-prompt".to_string());
        args.push(append_system_prompt.to_string());
    }
    if let Some(extension) = settings
        .pi_work_extension
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        push_explicit_extension(&mut args, &mut explicit_extensions, extension);
    }
    for source in &settings.pi_shared_extension_sources {
        if !source.trim().is_empty() {
            push_explicit_extension(&mut args, &mut explicit_extensions, source);
        }
    }
    if isolated_work_profile {
        // Load the Native Web and Subagents adapters in Work mode. A stale
        // pi-web-access entry from an older profile must never be loaded as a
        // second, unreviewed browser provider.
        if let Some(adapter) = settings
            .pi_work_browser_adapter
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            push_explicit_extension(&mut args, &mut explicit_extensions, adapter);
        }
        for source in &settings.pi_work_package_sources {
            if is_system_managed_source(source) {
                continue;
            }
            if !source.trim().is_empty() {
                push_explicit_extension(&mut args, &mut explicit_extensions, source);
            }
        }
        if let Some(adapter) = settings
            .pi_work_mcp_adapter
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            push_explicit_extension(&mut args, &mut explicit_extensions, adapter);
        }
    }
    for source in &settings.pi_work_skill_sources {
        if !source.trim().is_empty() {
            args.push("--skill".to_string());
            args.push(source.to_string());
        }
    }
    if let Some(effort) = settings
        .effort
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        args.push("--thinking".to_string());
        args.push(if effort == "none" { "off" } else { effort }.to_string());
    }

    match launch {
        Some(PiSessionLaunch::Resume(session_id)) => {
            args.push("--session".to_string());
            args.push(session_id.trim().to_string());
        }
        Some(PiSessionLaunch::Fork(session_id)) => {
            // Let Pi create and persist the new session before entering RPC mode.
            // This reliably exposes a new session id through the initial state event.
            args.push("--fork".to_string());
            args.push(session_id.trim().to_string());
        }
        None if settings.no_session_persistence => {
            args.push("--no-session".to_string());
        }
        None => {}
    }

    if !isolated_work_profile
        && settings.pi_permission_system_enabled
        && pi_extensions::should_load_explicitly_for_agent_dir(
            pi_profile_dir,
            "npm:@gotgenes/pi-permission-system",
        )
    {
        push_explicit_extension(
            &mut args,
            &mut explicit_extensions,
            bundled_or_registry(
                "@gotgenes/pi-permission-system",
                "npm:@gotgenes/pi-permission-system@31.1.1",
            ),
        );
    }

    if !isolated_work_profile
        && settings.pi_goal_enabled
        && pi_extensions::should_load_explicitly_for_agent_dir(
            pi_profile_dir,
            "npm:@narumitw/pi-goal",
        )
    {
        push_explicit_extension(
            &mut args,
            &mut explicit_extensions,
            bundled_or_registry("@narumitw/pi-goal", "npm:@narumitw/pi-goal@0.54.4"),
        );
    }
    if !isolated_work_profile
        && settings.pi_plan_mode_enabled
        && pi_extensions::should_load_explicitly_for_agent_dir(
            pi_profile_dir,
            "npm:@narumitw/pi-plan-mode",
        )
    {
        push_explicit_extension(
            &mut args,
            &mut explicit_extensions,
            bundled_or_registry(
                "@narumitw/pi-plan-mode",
                "npm:@narumitw/pi-plan-mode@0.56.0",
            ),
        );
    }
    if !isolated_work_profile
        && settings.pi_context_prune_enabled
        && pi_extensions::should_load_explicitly_for_agent_dir(
            pi_profile_dir,
            "npm:pi-context-prune",
        )
    {
        push_explicit_extension(
            &mut args,
            &mut explicit_extensions,
            bundled_or_registry("pi-context-prune", "npm:pi-context-prune@1.4.0"),
        );
    }
    // Multi-edit is part of the managed Pi Code runtime, not a user extension.
    // Always load the pinned copy for Code and never project it into Work.
    if !isolated_work_profile {
        push_explicit_extension(
            &mut args,
            &mut explicit_extensions,
            bundled_or_registry("pi-mono-multi-edit", "npm:pi-mono-multi-edit@2.0.0"),
        );
    }
    if !isolated_work_profile
        && settings.pi_lsp_enabled
        && pi_extensions::should_load_explicitly_for_agent_dir(
            pi_profile_dir,
            "npm:@narumitw/pi-lsp",
        )
    {
        push_explicit_extension(
            &mut args,
            &mut explicit_extensions,
            bundled_or_registry("@narumitw/pi-lsp", "npm:@narumitw/pi-lsp@0.49.7"),
        );
    }

    Ok(args)
}

fn bundled_or_registry(package_name: &str, registry_source: &str) -> String {
    bundled_pi_package_path(package_name).unwrap_or_else(|| registry_source.to_string())
}

fn push_explicit_extension(
    args: &mut Vec<String>,
    sources: &mut std::collections::HashSet<String>,
    source: impl Into<String>,
) {
    let source = source.into();
    if sources.insert(source.clone()) {
        args.push("-e".to_string());
        args.push(source);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    settings: &AdapterSettings,
    resume_session_id: Option<String>,
    extra_env: HashMap<String, String>,
    cancel: CancellationToken,
) -> Result<mpsc::Sender<ActorCommand>, String> {
    let launch = resume_session_id
        .filter(|value| !value.trim().is_empty())
        .map(PiSessionLaunch::Resume);
    spawn_actor_with_launch(
        emitter, sessions, run_id, cwd, settings, launch, extra_env, cancel,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_fork_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    settings: &AdapterSettings,
    source_session_id: String,
    extra_env: HashMap<String, String>,
    cancel: CancellationToken,
) -> Result<mpsc::Sender<ActorCommand>, String> {
    spawn_actor_with_launch(
        emitter,
        sessions,
        run_id,
        cwd,
        settings,
        Some(PiSessionLaunch::Fork(source_session_id)),
        extra_env,
        cancel,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn spawn_actor_with_launch(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    settings: &AdapterSettings,
    launch: Option<PiSessionLaunch>,
    extra_env: HashMap<String, String>,
    cancel: CancellationToken,
) -> Result<mpsc::Sender<ActorCommand>, String> {
    let mut prepared_settings = settings.clone();
    if let Some(provider) = prepared_settings.pi_provider.clone() {
        prepared_settings.pi_provider =
            Some(crate::agent::codex_subscription_bridge::prepare_pi_provider(&provider).await?);
    }
    let mut prepared_global_providers =
        Vec::with_capacity(prepared_settings.global_providers.len());
    for provider in &prepared_settings.global_providers {
        prepared_global_providers.push(
            crate::agent::codex_subscription_bridge::prepare_global_provider(provider).await?,
        );
    }
    prepared_settings.global_providers = prepared_global_providers;
    let settings = &prepared_settings;

    super::register_session_map(&sessions);

    let browser_token = extra_env.get("AGENTCABIN_BROWSER_BRIDGE_TOKEN").cloned();
    let connector_token = extra_env
        .get("AGENTCABIN_CODE_CONNECTOR_BRIDGE_TOKEN")
        .cloned();
    let desktop_token = extra_env.get("AGENTCABIN_DESKTOP_BRIDGE_TOKEN").cloned();
    let work_bridge_token = extra_env.get("AGENTCABIN_WORK_BRIDGE_TOKEN").cloned();

    if settings.no_session_persistence
        && matches!(launch.as_ref(), Some(PiSessionLaunch::Resume(_)))
    {
        return Err("Cannot resume a Pi RPC session when session persistence is disabled".into());
    }

    // Use the same resolver as diagnostics so a configured non-standard Pi
    // installation is honored by the RPC transport too.
    let binary = resolve_pi_path();
    let managed_provider_env =
        if !settings.global_providers.is_empty() || settings.pi_provider.is_some() {
            pi_provider_bridge::prepare_global_providers_env(
                &settings.global_providers,
                settings.pi_provider.as_ref(),
            )?
        } else {
            HashMap::new()
        };
    let mut permission_env = managed_provider_env.clone();
    let permission_agent_dir = settings
        .pi_agent_dir
        .as_deref()
        .or(settings.pi_code_profile_dir.as_deref())
        .or_else(|| extra_env.get("PI_CODING_AGENT_DIR").map(String::as_str))
        .map(PathBuf::from)
        .or_else(|| crate::storage::home_dir().map(|home| PathBuf::from(home).join(".pi/agent")));
    if let Some(agent_dir) = settings
        .pi_agent_dir
        .as_deref()
        .or(settings.pi_code_profile_dir.as_deref())
    {
        permission_env.insert("PI_CODING_AGENT_DIR".to_string(), agent_dir.to_string());
    }
    crate::agent::pi_permission::sync_for_spawn(settings, &permission_env);
    let args = build_rpc_args(settings, launch.as_ref())?;

    log::debug!(
        "[pi_rpc_actor] spawn: run_id={}, binary={}, args={:?}, cwd={}",
        run_id,
        binary,
        args,
        cwd
    );

    let is_work = settings.pi_workspace_root.is_some();
    let path = augmented_path();
    let mut launch_env = vec![("PATH".to_string(), path.clone())];
    if let Some(agent_dir) = settings.pi_agent_dir.as_deref() {
        launch_env.push(("PI_CODING_AGENT_DIR".to_string(), agent_dir.to_string()));
    }
    if is_work && (settings.pi_provider.is_some() || !settings.global_providers.is_empty()) {
        // The provider is an explicit Work capability. Without this marker the
        // Work sandbox correctly denies all network, which also denies the
        // model request itself because Pi performs that request in the child.
        launch_env.push((
            WORK_NETWORK_POLICY_ENV.to_string(),
            WORK_PROVIDER_NETWORK_POLICY.to_string(),
        ));
    } else if is_work && extra_env.contains_key("AGENTCABIN_WORK_PROXY_URL") {
        launch_env.push((WORK_NETWORK_POLICY_ENV.to_string(), "proxy".to_string()));
    }
    for (key, value) in &extra_env {
        launch_env.push((key.clone(), value.clone()));
    }
    for (key, value) in &managed_provider_env {
        launch_env.push((key.clone(), value.clone()));
    }

    let (program, launch_args, launch_cwd, confined_env) =
        if is_work && is_work_full_access(settings) {
            log::warn!(
                "[pi_rpc_actor] Work FullAccess: launching Pi without Work OS sandbox, run_id={}",
                run_id
            );
            (
                PathBuf::from(&binary),
                args.iter().cloned().map(Into::into).collect(),
                PathBuf::from(&cwd),
                launch_env,
            )
        } else if is_work {
            let workspace_root = settings
                .pi_workspace_root
                .as_deref()
                .map(PathBuf::from)
                .ok_or_else(|| "Work Pi launch is missing workspace root".to_string())?;
            let work_profile = settings
                .pi_agent_dir
                .as_deref()
                .map(PathBuf::from)
                .ok_or_else(|| "Work Pi launch is missing Work profile directory".to_string())?;
            let mut writable_roots = vec![workspace_root];
            // Pi resolves the explicitly loaded Work system package through
            // npm and npm writes its metadata/logs to this app-owned cache.
            // The Work profile itself remains read-only; only this narrow
            // cache directory is writable inside the Seatbelt.
            writable_roots.push(
                crate::work::paths::WorkPaths::app()
                    .work_profile_dir()
                    .join("npm-cache"),
            );
            for (path, writable) in &settings.pi_work_access_roots {
                if *writable {
                    writable_roots.push(PathBuf::from(path));
                }
            }
            for key in [
                "AGENTCABIN_WORK_RUN_DIR",
                "AGENTCABIN_WORK_BROWSER_RUN_DIR",
                "AGENTCABIN_MANAGED_STATE_DIR",
            ] {
                if let Some(path) = extra_env.get(key).filter(|value| !value.trim().is_empty()) {
                    writable_roots.push(PathBuf::from(path));
                }
            }
            let mut read_only_roots = vec![crate::work::paths::WorkPaths::app().work_profile_dir()];
            // The bundled Pi launcher is a shell script that execs the
            // application-managed Node binary from its sibling runtime
            // directory. Seatbelt needs process-exec permission for that
            // directory as well as file-read access; allowing only the Pi
            // script itself makes Work fail with EPERM before RPC starts.
            if let Ok(node_path) = crate::agent::runtime_locator::resolve_node() {
                if let Some(node_dir) = PathBuf::from(node_path).parent() {
                    read_only_roots.push(node_dir.to_path_buf());
                }
            }
            // Pi's common system packages are managed outside the isolated
            // Work profile. Work receives read-only access to that dependency
            // tree so native interaction extensions and their dependencies
            // can be resolved without enabling arbitrary host extensions.
            read_only_roots.push(
                crate::work::paths::WorkPaths::app()
                    .pi_system_dir()
                    .join("npm")
                    .join("node_modules"),
            );
            read_only_roots.extend(
                settings
                    .pi_work_access_roots
                    .iter()
                    .filter(|(_, writable)| !*writable)
                    .map(|(path, _)| PathBuf::from(path)),
            );
            if let Some(path) = settings.pi_work_attachment_root.as_deref() {
                read_only_roots.push(PathBuf::from(path));
            }
            for source in settings
                .pi_work_package_sources
                .iter()
                .chain(settings.pi_shared_extension_sources.iter())
                .chain(settings.pi_work_skill_sources.iter())
                .chain(settings.pi_work_extension.iter())
                .chain(settings.pi_work_mcp_adapter.iter())
                .chain(settings.pi_work_browser_adapter.iter())
            {
                if source.starts_with('/') {
                    read_only_roots.push(PathBuf::from(source));
                }
            }
            if settings.pi_provider.is_some() || !settings.global_providers.is_empty() {
                // Managed Work providers are loaded by Pi as an explicit extension,
                // but the bridge lives in AgentCabin's data directory rather than
                // under the Work Profile. It must be readable inside the OS sandbox.
                read_only_roots.push(pi_provider_bridge::bridge_path());
            }

            let raw_command = ExecutionCommand {
                program: PathBuf::from(&binary),
                args: args.iter().cloned().map(Into::into).collect(),
                current_dir: PathBuf::from(&cwd),
                envs: launch_env,
            };
            let confined = WorkSandboxLauncher::confine_work_command(
                &raw_command,
                &work_profile,
                &writable_roots,
                &read_only_roots,
            )
            .map_err(|error| format!("Failed to start Work Pi inside OS sandbox: {error}"))?;
            log::debug!(
                "[pi_rpc_actor] Work OS sandbox: program={}, cwd={}, rw_roots={}, ro_roots={}",
                confined.program.display(),
                confined.current_dir.display(),
                writable_roots.len() + 2,
                read_only_roots.len()
            );
            (
                confined.program,
                confined.args,
                confined.current_dir,
                confined.envs,
            )
        } else {
            (
                PathBuf::from(&binary),
                args.iter().cloned().map(Into::into).collect(),
                PathBuf::from(&cwd),
                Vec::new(),
            )
        };

    let mut command = Command::new(&program);
    // A Work Pi process must receive only the environment assembled by the
    // launcher. Inheriting the desktop process environment would make a
    // provider/API token (or an operator-controlled AgentCabin variable)
    // reachable before the Work policy and Host bridges are involved.
    if is_work && !is_work_full_access(settings) {
        command.env_clear();
    }
    command
        .args(&launch_args)
        .current_dir(&launch_cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PATH", path)
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("CLAUDECODE")
        .hide_console()
        .kill_on_drop(true);
    for (key, value) in confined_env {
        command.env(key, value);
    }
    if !is_work {
        for (key, value) in &extra_env {
            command.env(key, value);
        }
        for (key, value) in &managed_provider_env {
            command.env(key, value);
        }
        if let Some(code_dir) = settings.pi_code_profile_dir.as_deref() {
            // Keep Pi's config/state isolated without changing HOME. Pi is
            // commonly launched through a shebang and the user's version
            // manager (for example Volta) needs the host HOME to resolve its
            // default Node runtime in packaged builds.
            command.env("PI_CODING_AGENT_DIR", code_dir);
        }
    } else if let Some(agent_dir) = settings.pi_agent_dir.as_deref() {
        command.env("PI_CODING_AGENT_DIR", agent_dir);
        command.env("HOME", agent_dir);
    }

    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "Pi Coding Agent was not found in PATH".to_string()
        } else {
            format!("Failed to start Pi RPC mode: {}", error)
        }
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Pi RPC stdin was not captured".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Pi RPC stdout was not captured".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Pi RPC stderr was not captured".to_string())?;

    let mut protocol = PiRpc::new();
    protocol.configure_features(
        settings.pi_plan_mode_enabled,
        settings.pi_goal_enabled,
        settings.pi_permission_system_enabled,
    );
    protocol.configure_permission_mode(settings.permission_mode.as_deref());
    let resume_session_id = match launch.as_ref() {
        Some(PiSessionLaunch::Resume(session_id)) => Some(session_id.clone()),
        _ => None,
    };
    let startup_ctx = StartupCtx {
        cwd: cwd.clone(),
        resume_thread_id: resume_session_id,
        model: settings.model.clone(),
        effort: settings.effort.clone(),
        ..StartupCtx::default()
    };
    for message in protocol.startup_messages(&startup_ctx) {
        if let Err(error) = write_json_line(&mut stdin, &message).await {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
    }

    let tag = Arc::new(());
    let (cmd_tx, cmd_rx) = mpsc::channel::<ActorCommand>(64);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (start_tx, start_rx) = oneshot::channel();
    let task_tag = tag.clone();
    let task_run_id = run_id.clone();
    let task_sessions = sessions.clone();
    let actor_desktop_token = desktop_token.clone();
    let actor_work_bridge_token = work_bridge_token.clone();
    let persistent = !settings.no_session_persistence;
    let managed_provider = settings.pi_provider.is_some();
    let join_handle = tokio::spawn(async move {
        if start_rx.await.is_err() {
            return;
        }
        super::actor_loop::run_actor(
            emitter,
            task_sessions,
            task_run_id,
            task_tag,
            child,
            stdin,
            stdout,
            stderr,
            cmd_rx,
            protocol,
            managed_provider,
            permission_agent_dir,
            persistent,
            browser_token,
            connector_token,
            actor_desktop_token,
            actor_work_bridge_token,
            cancel,
            shutdown_tx,
        )
        .await;
    });

    let sender = cmd_tx.clone();
    let map_key = run_id.clone();
    sessions.lock().await.insert(
        map_key.clone(),
        SessionActorHandle {
            cmd_tx,
            run_id,
            tag,
            join_handle,
            shutdown_rx,
            work_bridge_token: work_bridge_token.clone(),
            desktop_runtime_token: desktop_token.clone(),
        },
    );
    if start_tx.send(()).is_err() {
        sessions.lock().await.remove(&map_key);
        if let Some(token) = work_bridge_token.as_deref() {
            crate::work::internal_bridge::revoke_session_token(token).await;
        }
        if let Some(token) = desktop_token.as_deref() {
            crate::desktop_runtime::revoke_token(token).await;
        }
        return Err("Pi RPC actor failed to enter its process loop".to_string());
    }
    Ok(sender)
}

async fn write_json_line(
    stdin: &mut tokio::process::ChildStdin,
    message: &serde_json::Value,
) -> Result<(), String> {
    let mut line = serde_json::to_string(message).map_err(|error| error.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|error| format!("Pi RPC stdin write failed: {}", error))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Pi RPC stdin flush failed: {}", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_settings() -> AdapterSettings {
        AdapterSettings {
            model: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            permission_mode: None,
            append_system_prompt: None,
            max_budget_usd: None,
            fallback_model: None,
            system_prompt: None,
            tool_set: None,
            add_dirs: vec![],
            json_schema: None,
            include_partial_messages: false,
            cli_debug: None,
            no_session_persistence: false,
            max_turns: None,
            effort: None,
            betas: vec![],
            agents_json: None,
            ephemeral: false,
            profile: None,
            ignore_user_config: false,
            ignore_rules: false,
            web_search: false,
            codex_provider: None,
            pi_provider: None,
            pi_code_profile_dir: None,
            pi_agent_dir: None,
            pi_work_extension: None,
            pi_work_mcp_adapter: None,
            pi_work_browser_adapter: None,
            pi_shared_extension_sources: vec![],
            pi_work_package_sources: vec![],
            pi_work_skill_sources: vec![],
            pi_workspace_root: None,
            pi_work_access_roots: vec![],
            pi_work_attachment_root: None,
            pi_work_full_access: false,
            pi_permission_system_enabled: false,
            pi_plan_mode_enabled: false,
            pi_goal_enabled: false,
            pi_context_prune_enabled: false,
            pi_multi_edit_enabled: false,
            pi_lsp_enabled: false,
            global_providers: vec![],
        }
    }

    #[test]
    fn native_fork_uses_fork_flag_instead_of_resuming_source() {
        let args = build_rpc_args(
            &make_settings(),
            Some(&PiSessionLaunch::Fork("source-session".into())),
        )
        .unwrap();

        assert!(args
            .windows(2)
            .any(|pair| pair == ["--fork", "source-session"]));
        assert!(!args.contains(&"--session".to_string()));
    }

    #[test]
    fn pi_sessions_always_start_in_rpc_mode() {
        let args = build_rpc_args(&make_settings(), None).unwrap();
        assert_eq!(args.first().map(String::as_str), Some("--mode"));
        assert_eq!(args.get(1).map(String::as_str), Some("rpc"));
        assert!(!args.windows(2).any(|pair| pair == ["--mode", "json"]));
    }

    #[test]
    fn full_access_is_only_unconfined_for_work_sessions() {
        let mut settings = make_settings();
        settings.permission_mode = Some("bypassPermissions".into());
        assert!(!is_work_full_access(&settings));

        settings.pi_agent_dir = Some("/work/profile".into());
        assert!(!is_work_full_access(&settings));

        settings.pi_work_full_access = true;
        assert!(is_work_full_access(&settings));

        settings.pi_work_full_access = false;
        settings.permission_mode = Some("auto".into());
        assert!(!is_work_full_access(&settings));
    }

    #[test]
    fn thinking_level_is_passed_as_pi_thinking_flag() {
        let mut settings = make_settings();
        settings.effort = Some("high".into());

        let args = build_rpc_args(&settings, None).unwrap();

        assert!(args.windows(2).any(|pair| pair == ["--thinking", "high"]));
    }

    #[test]
    fn work_extension_is_loaded_from_an_explicit_entry_point() {
        let mut settings = make_settings();
        settings.pi_work_extension = Some("/work/profile/extensions/core.mjs".into());

        let args = build_rpc_args(&settings, None).unwrap();

        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "/work/profile/extensions/core.mjs"]));
    }

    #[test]
    fn native_interaction_extensions_are_explicit_in_code_and_work_sessions() {
        for work_profile in [false, true] {
            let mut settings = make_settings();
            if work_profile {
                settings.pi_agent_dir = Some("/work/profile".into());
            }
            settings.pi_shared_extension_sources = vec![
                "/managed/npm/node_modules/@juicesharp/rpiv-ask-user-question".into(),
                "/managed/npm/node_modules/@juicesharp/rpiv-todo".into(),
            ];

            let args = build_rpc_args(&settings, None).unwrap();

            for source in &settings.pi_shared_extension_sources {
                assert!(
                    args.windows(2).any(|pair| pair == ["-e", source.as_str()]),
                    "native Pi interaction extension {source} was not explicitly loaded"
                );
            }
        }
    }

    #[test]
    fn work_sessions_disable_discovery_and_load_only_explicit_packages() {
        let mut settings = make_settings();
        settings.pi_agent_dir = Some("/work/profile".into());
        settings.pi_work_extension = Some("/work/profile/extensions/core.mjs".into());
        settings.pi_work_browser_adapter = Some("/work/profile/extensions/browser.mjs".into());
        settings.pi_work_mcp_adapter = Some("/work/profile/extensions/mcp-adapter.mjs".into());
        settings.pi_work_package_sources = vec!["npm:@acme/pi-office".into()];
        settings.pi_work_skill_sources = vec!["/work/profile/skills/research".into()];

        let args = build_rpc_args(&settings, None).unwrap();

        for flag in [
            "--no-extensions",
            "--no-skills",
            "--no-prompt-templates",
            "--no-themes",
            "--no-context-files",
        ] {
            assert!(args.contains(&flag.to_string()), "missing {flag}");
        }
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "/work/profile/extensions/core.mjs"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "/work/profile/extensions/browser.mjs"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "/work/profile/extensions/mcp-adapter.mjs"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:@acme/pi-office"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--skill", "/work/profile/skills/research"]));
    }

    #[test]
    fn work_browser_adapter_loads_native_adapter_and_package_sources() {
        let mut settings = make_settings();
        settings.pi_agent_dir = Some("/work/profile".into());
        settings.pi_work_browser_adapter = Some("/work/profile/extensions/browser.mjs".into());
        settings.pi_work_package_sources = vec![
            "npm:pi-web-access@0.23.0".into(),
            "npm:@acme/pi-office".into(),
        ];

        let args = build_rpc_args(&settings, None).unwrap();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "/work/profile/extensions/browser.mjs"]));
        assert!(!args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:pi-web-access@0.23.0"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:@acme/pi-office"]));
    }

    #[test]
    fn work_loads_arbitrary_package_sources_in_rpc_extension() {
        let mut settings = make_settings();
        settings.pi_agent_dir = Some("/work/profile".into());
        settings.pi_work_package_sources = vec![
            "npm:@acme/pi-custom@1.0.0".into(),
            "npm:pi-mcp-adapter@2.22.0".into(),
            "npm:pi-web-access@0.23.0".into(),
        ];

        let args = build_rpc_args(&settings, None).unwrap();

        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:@acme/pi-custom@1.0.0"]));
        assert!(!args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:pi-mcp-adapter@2.22.0"]));
        assert!(!args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:pi-web-access@0.23.0"]));
    }

    #[test]
    fn work_sessions_never_inherit_code_feature_extensions() {
        let mut settings = make_settings();
        settings.pi_agent_dir = Some("/work/profile".into());
        settings.pi_permission_system_enabled = true;
        settings.pi_plan_mode_enabled = true;
        settings.pi_goal_enabled = true;
        settings.pi_context_prune_enabled = true;
        settings.pi_multi_edit_enabled = true;
        settings.pi_lsp_enabled = true;

        let args = build_rpc_args(&settings, None).unwrap();

        for extension in [
            "npm:@gotgenes/pi-permission-system",
            "npm:@narumitw/pi-goal",
            "npm:@narumitw/pi-plan-mode",
            "npm:pi-context-prune",
            "npm:pi-mono-multi-edit",
            "npm:@narumitw/pi-lsp",
        ] {
            assert!(
                !args.windows(2).any(|pair| pair == ["-e", extension]),
                "Work unexpectedly inherited Code extension {extension}"
            );
            let package = extension.trim_start_matches("npm:");
            if let Some(bundled) = bundled_pi_package_path(package) {
                assert!(
                    !args.windows(2).any(|pair| pair == ["-e", bundled.as_str()]),
                    "Work unexpectedly inherited the bundled Code extension {package}"
                );
            }
        }
    }

    /// Assert that `package_name` is loaded explicitly, preferring the copy
    /// shipped with the app. The registry spec is only the fallback; when it is
    /// used Pi installs the package from the npm registry at every session
    /// start, which is what makes a first Pi turn slow.
    fn assert_explicit_extension(args: &[String], package_name: &str, registry_source: &str) {
        let expected =
            bundled_pi_package_path(package_name).unwrap_or_else(|| registry_source.into());
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-e", expected.as_str()]),
            "expected explicit `-e {expected}` in {args:?}"
        );
    }

    #[test]
    fn code_sessions_keep_existing_discovery_behavior() {
        let args = build_rpc_args(&make_settings(), None).unwrap();

        assert!(!args.contains(&"--no-extensions".to_string()));
        assert!(!args.contains(&"--no-context-files".to_string()));
        assert_explicit_extension(&args, "pi-mono-multi-edit", "npm:pi-mono-multi-edit@2.0.0");
    }

    #[test]
    fn code_sessions_cannot_disable_managed_multi_edit() {
        let mut settings = make_settings();
        settings.pi_multi_edit_enabled = false;

        let args = build_rpc_args(&settings, None).unwrap();

        assert_explicit_extension(&args, "pi-mono-multi-edit", "npm:pi-mono-multi-edit@2.0.0");
    }

    #[test]
    fn pinned_extensions_resolve_to_the_bundled_copy_when_present() {
        // Guards the `bundled_or_registry` preference: a checked-out tree or a
        // packaged app ships every pinned extension, so Pi must never be given a
        // registry spec that would trigger an online install.
        for (package_name, registry_source) in [
            (
                "@gotgenes/pi-permission-system",
                "npm:@gotgenes/pi-permission-system@31.1.1",
            ),
            ("@narumitw/pi-goal", "npm:@narumitw/pi-goal@0.54.4"),
            (
                "@narumitw/pi-plan-mode",
                "npm:@narumitw/pi-plan-mode@0.56.0",
            ),
            ("pi-mono-multi-edit", "npm:pi-mono-multi-edit@2.0.0"),
        ] {
            let source = bundled_or_registry(package_name, registry_source);
            assert_ne!(
                source, registry_source,
                "{package_name} fell back to an online npm install"
            );
            assert!(
                source.ends_with(package_name),
                "{package_name} resolved to an unexpected path: {source}"
            );
        }
    }

    #[test]
    fn code_profile_settings_prevent_duplicate_feature_extensions() {
        let profile = tempfile::tempdir().unwrap();
        std::fs::write(
            profile.path().join("settings.json"),
            r#"{"packages":["npm:@gotgenes/pi-permission-system","npm:@narumitw/pi-goal"]}"#,
        )
        .unwrap();

        let mut settings = make_settings();
        settings.pi_code_profile_dir = Some(profile.path().to_string_lossy().into_owned());
        settings.pi_permission_system_enabled = true;
        settings.pi_goal_enabled = true;
        settings.pi_plan_mode_enabled = true;

        let args = build_rpc_args(&settings, None).unwrap();

        assert!(!args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:@gotgenes/pi-permission-system"]));
        assert!(!args
            .windows(2)
            .any(|pair| pair == ["-e", "npm:@narumitw/pi-goal"]));
        assert_explicit_extension(
            &args,
            "@narumitw/pi-plan-mode",
            "npm:@narumitw/pi-plan-mode@0.56.0",
        );
    }

    #[test]
    fn managed_code_loads_only_explicit_shared_extensions() {
        let profile = tempfile::tempdir().unwrap();
        let mut settings = make_settings();
        settings.pi_code_profile_dir = Some(profile.path().to_string_lossy().into_owned());
        settings.pi_shared_extension_sources =
            vec!["/Users/test/.agentcabin/pi/npm/node_modules/pi-lens".into()];

        let args = build_rpc_args(&settings, None).unwrap();

        assert!(args.contains(&"--no-extensions".to_string()));
        assert!(args
            .windows(2)
            .any(|pair| { pair == ["-e", "/Users/test/.agentcabin/pi/npm/node_modules/pi-lens"] }));
    }

    #[test]
    fn explicit_run_model_overrides_managed_provider_default() {
        let mut settings = make_settings();
        settings.model = Some("glm-5.2".into());
        settings.pi_provider = Some(crate::models::PiProviderCredential {
            id: "custom-openai".into(),
            name: "Custom OpenAI-compatible".into(),
            base_url: "https://example.test/v1".into(),
            api: "openai-completions".into(),
            model: "deepseek/deepseek-v4-flash".into(),
            models: vec![],
            context_window: None,
            api_key: None,
        });

        let args = build_rpc_args(&settings, None).unwrap();

        assert!(args.windows(2).any(|pair| pair == ["--model", "glm-5.2"]));
        assert!(!args
            .windows(2)
            .any(|pair| pair == ["--model", "deepseek/deepseek-v4-flash"]));
    }

    #[test]
    fn legacy_none_thinking_level_is_migrated_to_pi_off() {
        let mut settings = make_settings();
        settings.effort = Some("none".into());

        let args = build_rpc_args(&settings, None).unwrap();

        assert!(args.windows(2).any(|pair| pair == ["--thinking", "off"]));
        assert!(!args.windows(2).any(|pair| pair == ["--thinking", "none"]));
    }

    #[test]
    fn normal_resume_still_uses_session_flag() {
        let args = build_rpc_args(
            &make_settings(),
            Some(&PiSessionLaunch::Resume("source-session".into())),
        )
        .unwrap();

        assert!(args
            .windows(2)
            .any(|pair| pair == ["--session", "source-session"]));
        assert!(!args.contains(&"--fork".to_string()));
    }

    #[test]
    fn work_browser_adapter_is_explicitly_reloaded_on_resume() {
        let mut settings = make_settings();
        settings.pi_agent_dir = Some("/work/profile".into());
        settings.pi_work_extension = Some("/work/profile/extensions/core.mjs".into());
        settings.pi_work_browser_adapter = Some("/work/profile/extensions/browser.mjs".into());

        let args = build_rpc_args(
            &settings,
            Some(&PiSessionLaunch::Resume("existing-work-session".into())),
        )
        .unwrap();

        assert!(args
            .windows(2)
            .any(|pair| pair == ["-e", "/work/profile/extensions/browser.mjs"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--session", "existing-work-session"]));
    }

    #[test]
    fn fork_rejects_disabled_session_persistence() {
        let mut settings = make_settings();
        settings.no_session_persistence = true;

        let error = build_rpc_args(
            &settings,
            Some(&PiSessionLaunch::Fork("source-session".into())),
        )
        .unwrap_err();
        assert!(error.contains("session persistence is disabled"));
    }
}
