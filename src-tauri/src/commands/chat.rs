use crate::agent::spawn::build_agent_command;
use crate::agent::stream::{run_agent, ProcessMap};
use crate::models::{
    max_attachment_size, Attachment, AttachmentMeta, BusEvent, ConversationRef, RunEventType,
    RunStatus,
};
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use std::fs;
use std::sync::Arc;

fn safe_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let truncated = if cleaned.len() > 120 {
        &cleaned[..120]
    } else {
        &cleaned
    };
    if truncated.is_empty() {
        "attachment.bin".to_string()
    } else {
        truncated.to_string()
    }
}

fn extension_for_mime(mime: &str) -> &str {
    if mime.starts_with("image/png") {
        return ".png";
    }
    if mime.starts_with("image/jpeg") {
        return ".jpg";
    }
    if mime.starts_with("image/webp") {
        return ".webp";
    }
    if mime.starts_with("image/gif") {
        return ".gif";
    }
    if mime.starts_with("application/pdf") {
        return ".pdf";
    }
    if mime.starts_with("text/markdown") {
        return ".md";
    }
    if mime.starts_with("text/plain") {
        return ".txt";
    }
    if mime.contains("json") {
        return ".json";
    }
    ""
}

/// Headless-compatible implementation of the legacy one-shot chat transport.
/// All desktop integration flows through the BroadcastEmitter, so the same
/// body serves the Tauri IPC command and the core-server /invoke dispatch.
#[allow(clippy::too_many_arguments)]
pub async fn send_chat_message_inner(
    process_map: ProcessMap,
    emitter: Arc<BroadcastEmitter>,
    cancel_token: tokio_util::sync::CancellationToken,
    run_id: String,
    message: String,
    attachments: Option<Vec<Attachment>>,
    model: Option<String>,
    client_uuid: Option<String>,
    permission_mode_override: Option<String>,
) -> Result<(), String> {
    log::debug!(
        "[chat] send_chat_message: run_id={}, msg_len={}, attachments={}, client_uuid={:?}, permission_override={:?}",
        run_id,
        message.len(),
        attachments.as_ref().map_or(0, |a| a.len()),
        client_uuid,
        permission_mode_override
    );
    let run = storage::runs::get_run(&run_id).ok_or_else(|| format!("Run {} not found", run_id))?;

    // The chat IPC remains the compatibility entry point for pipe agents.
    // Pi permissions require a bidirectional RPC transport, so never route a
    // Pi run through this legacy one-shot command.
    let exec_path = run.resolved_execution_path();
    if exec_path != crate::models::ExecutionPath::PipeExec {
        return Err(format!(
            "send_chat_message requires execution_path=pipe_exec, got {:?} for run {}",
            exec_path, run_id
        ));
    }
    if run.agent == "pi" {
        return Err(
            "Pi Agent requires an RPC session (execution_path=session_actor) to display permission prompts; start a new Pi session".to_string(),
        );
    }

    // Resume validation: reject if a resumable identity exists but persistence is disabled.
    if run.conversation_ref.is_some() || (run.agent == "pi" && run.session_id.is_some()) {
        let agent_settings = storage::settings::get_agent_settings(&run.agent);
        if agent_settings.no_session_persistence.unwrap_or(false) {
            return Err("Cannot resume: session persistence is disabled".to_string());
        }
    }

    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("message is required".to_string());
    }

    // Handle attachments. Disk paths are used for the legacy Claude/Codex pipe
    // transport and for non-image file breadcrumbs.
    let attachments = attachments.unwrap_or_default();
    let mut attachment_paths: Vec<(String, String, String, u64)> = vec![]; // (path, name, type, size)
    let mut attachment_metas: Vec<AttachmentMeta> = vec![];

    if !attachments.is_empty() {
        let upload_dir = std::env::temp_dir()
            .join("agentcabin-uploads")
            .join(&run_id);
        fs::create_dir_all(&upload_dir).map_err(|e| e.to_string())?;

        for att in attachments.iter().take(8) {
            if att.content_base64.is_empty() {
                continue;
            }
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&att.content_base64)
                .map_err(|e| e.to_string())?;
            if bytes.is_empty() {
                continue;
            }
            let limit = max_attachment_size(&att.mime_type) as usize;
            if bytes.len() > limit {
                log::warn!(
                    "[chat] skipping oversized attachment: {} ({} bytes > {} limit)",
                    att.name,
                    bytes.len(),
                    limit
                );
                continue;
            }

            let base = safe_filename(&att.name);
            let ext = extension_for_mime(&att.mime_type);
            let filename = format!(
                "{}-{}-{}{}",
                chrono::Utc::now().timestamp_millis(),
                &uuid::Uuid::new_v4().to_string()[..6],
                base,
                ext
            );
            let full_path = upload_dir.join(&filename);
            fs::write(&full_path, &bytes).map_err(|e| e.to_string())?;
            attachment_paths.push((
                full_path.to_string_lossy().to_string(),
                att.name.clone(),
                att.mime_type.clone(),
                att.size,
            ));
            attachment_metas.push(AttachmentMeta {
                name: att.name.clone(),
                mime_type: att.mime_type.clone(),
                size: att.size,
            });
        }
    }

    // Build prompt with attachments
    let attachment_text = if !attachment_paths.is_empty() {
        let files: Vec<String> = attachment_paths
            .iter()
            .map(|(path, name, mime, size)| {
                format!("- {} ({}, {} bytes) => {}", name, mime, size, path)
            })
            .collect();
        format!(
            "\n\nAttached files:\n{}\nUse these local file paths directly when needed.",
            files.join("\n")
        )
    } else {
        String::new()
    };
    let full_prompt = format!("{}{}", message, attachment_text);

    // Add user event (legacy events.jsonl)
    let att_json: Vec<serde_json::Value> = attachment_paths
        .iter()
        .map(|(path, name, mime, size)| {
            serde_json::json!({ "name": name, "type": mime, "size": size, "path": path })
        })
        .collect();

    if let Err(e) = storage::events::append_event(
        &run_id,
        RunEventType::User,
        serde_json::json!({
            "text": message,
            "source": "ui_chat",
            "attachments": att_json
        }),
    ) {
        log::warn!("[chat] failed to log user event: {}", e);
    }

    // Emit UserMessage bus event
    emitter.persist_and_emit(
        &run_id,
        &BusEvent::UserMessage {
            run_id: run_id.clone(),
            text: message.clone(),
            uuid: None,
            client_uuid: client_uuid.clone(),
            attachments: attachment_metas,
        },
    );

    log::debug!(
        "[chat] starting message transport: run_id={}, agent={}",
        run_id,
        run.agent
    );
    if let Err(e) = storage::runs::update_status(&run_id, RunStatus::Running, None, None) {
        log::warn!("[chat] failed to update status to Running: {}", e);
    }

    // Build unified adapter settings
    let agent_settings = storage::settings::get_agent_settings(&run.agent);
    let user_settings = storage::settings::get_user_settings();
    let mut adapter_settings =
        crate::agent::adapter::build_adapter_settings(&agent_settings, &user_settings, model);
    // The composer value is authoritative for this first pipe-exec message. Without this
    // per-message override, an immediate send can race the async settings persistence and
    // spawn the CLI with the previous permission policy.
    crate::agent::adapter::apply_permission_mode_override(
        &mut adapter_settings,
        permission_mode_override.as_deref(),
    );

    // Pipe-exec is a compatibility transport, but it must use the same managed
    // capability projection as session-actor runs. In particular, never let a
    // legacy one-shot Codex/Claude process inherit the host HOME or discover
    // native provider directories.
    let app_mode = run.app_mode;
    let runtime_provider =
        crate::agent::capability_resolver::RuntimeProviderKind::try_from_agent_str(&run.agent)?;
    let caps = crate::agent::capability_resolver::CapabilityResolver::resolve(
        &crate::storage::data_dir(),
        app_mode,
        runtime_provider,
        &run.cwd,
        &run_id,
    )
    .await?;
    let runtime_adapter = crate::agent::runtime_providers::get_adapter(runtime_provider);
    let spawn_cfg = runtime_adapter.prepare_runtime(&caps)?;

    // The managed config is the only user-level config visible to the child.
    // Passing --ignore-user-config would also ignore that managed projection.
    if run.agent == "codex" {
        adapter_settings.ignore_user_config = false;
    }

    let effective_platform_id = if user_settings.auth_mode == "cli" {
        None
    } else {
        run.platform_id.as_deref()
    };
    let resolved_auth = crate::commands::session::resolve_auth_env_for_platform(
        &None,
        &user_settings,
        effective_platform_id,
    );
    crate::agent::adapter::clear_model_if_provider_overrides(
        &mut adapter_settings,
        &run.model,
        &agent_settings.model,
        &resolved_auth.models,
    );
    if run.agent == "codex" {
        if let Some(provider) = adapter_settings.codex_provider.clone() {
            let provider =
                crate::agent::codex_subscription_bridge::prepare_codex_provider(&provider).await?;
            adapter_settings.codex_provider = Some(
                crate::agent::codex_chat_bridge::prepare_provider(&provider, &cancel_token).await?,
            );
        }
    }

    // Resolve resume identity for the relevant agent.
    let resume_tid = run.conversation_ref.as_ref().and_then(|r| match r {
        ConversationRef::CodexThread(tid) => Some(tid.as_str()),
        _ => None,
    });
    // Image attachments become native CLI image/file arguments for Claude/Codex.
    let image_paths: Vec<String> = attachment_paths
        .iter()
        .filter(|(_, _, mime, _)| mime.starts_with("image/"))
        .map(|(path, _, _, _)| path.clone())
        .collect();

    // Claude/Codex retain the existing one-shot command builder.
    let (command, mut args) = build_agent_command(
        &run.agent,
        &full_prompt,
        &adapter_settings,
        true, // print mode
        resume_tid,
        &image_paths,
    )?;

    if run.agent == "claude" {
        // build_agent_command deliberately keeps the prompt last. Insert the
        // managed config flags before it so Claude parses them as options.
        let prompt_arg = if full_prompt.is_empty() {
            None
        } else {
            args.pop()
        };
        args.push("--mcp-config".into());
        args.push(
            caps.managed_home
                .join("mcp.json")
                .to_string_lossy()
                .into_owned(),
        );
        args.push("--strict-mcp-config".into());
        // HOME is the per-run managed HOME, so `user` means the projected
        // AgentCabin skills under that HOME, never the host ~/.claude.
        crate::commands::session::add_claude_managed_provider_args(&mut args, true);
        if let Some(prompt) = prompt_arg {
            args.push(prompt);
        }
    }

    if run.agent == "codex" {
        let effective_model = args
            .iter()
            .position(|a| a == "--model")
            .and_then(|i| args.get(i + 1))
            .cloned();
        if let Some(ref m) = effective_model {
            if let Err(e) = storage::runs::update_run_model(&run_id, m) {
                log::warn!("[chat] failed to record codex model: {}", e);
            }
        }
    }

    // Build a minimal managed environment. This map is passed to run_agent,
    // which clears the inherited environment before spawning the provider.
    let mut extra_env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if run.agent == "claude" {
        if let Some(key) = resolved_auth.api_key {
            extra_env.insert("ANTHROPIC_API_KEY".into(), key);
        }
        if let Some(token) = resolved_auth.auth_token {
            extra_env.insert("ANTHROPIC_AUTH_TOKEN".into(), token);
        }
        if let Some(base_url) = resolved_auth.base_url.as_deref() {
            if let Some(normalized) =
                crate::commands::session::normalize_claude_cli_base_url(Some(base_url))
            {
                extra_env.insert("ANTHROPIC_BASE_URL".into(), normalized);
            }
        }
        if let Some(models) = resolved_auth.models.as_deref() {
            for (key, value) in crate::commands::session::resolve_model_tiers(models) {
                extra_env.insert(key.into(), value);
            }
        }
        if let Some(auth_extra) = resolved_auth.extra_env {
            for (k, v) in auth_extra {
                if !crate::agent::capability_resolver::is_reserved_isolation_env(&k) {
                    extra_env.insert(k, v);
                }
            }
        }
    }

    // Codex third-party provider: supply the API key via the env var named by env_key.
    if let Some(p) = &adapter_settings.codex_provider {
        if let Some((k, v)) = crate::agent::spawn::codex_provider_env(p) {
            extra_env.insert(k, v);
        }
    }
    // Keep the runtime projection authoritative if an auth preset happens to
    // contain PATH/HOME-like keys.
    extra_env.extend(spawn_cfg.env);

    let em = emitter.clone();
    let pm = process_map.clone();
    let run_id_clone = run_id.clone();
    let agent_clone = run.agent.clone();
    let cwd = run.cwd.clone();

    tokio::spawn(async move {
        let result = run_agent(
            pm,
            run_id_clone.clone(),
            command,
            args,
            cwd,
            agent_clone,
            Some(em.clone()),
            extra_env,
        )
        .await;

        if let Err(e) = result {
            if let Err(e2) = storage::runs::update_status(
                &run_id_clone,
                RunStatus::Failed,
                Some(1),
                Some(e.clone()),
            ) {
                log::warn!("[chat] failed to update status to Failed: {}", e2);
            }
            em.persist_and_emit(
                &run_id_clone,
                &BusEvent::RunState {
                    run_id: run_id_clone.clone(),
                    state: "failed".to_string(),
                    exit_code: Some(1),
                    error: Some(e.clone()),
                },
            );
            em.emit_realtime(
                "chat-done",
                &crate::models::ChatDone {
                    ok: false,
                    code: 1,
                    error: None,
                },
                Some(&run_id_clone),
            );
        }
    });

    Ok(())
}

/// Tauri IPC entry point — thin wrapper over the headless-compatible impl.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn send_chat_message(
    process_map: tauri::State<'_, ProcessMap>,
    emitter: tauri::State<'_, Arc<BroadcastEmitter>>,
    cancel_token: tauri::State<'_, tokio_util::sync::CancellationToken>,
    run_id: String,
    message: String,
    attachments: Option<Vec<Attachment>>,
    model: Option<String>,
    client_uuid: Option<String>,
    permission_mode_override: Option<String>,
) -> Result<(), String> {
    send_chat_message_inner(
        process_map.inner().clone(),
        emitter.inner().clone(),
        cancel_token.inner().clone(),
        run_id,
        message,
        attachments,
        model,
        client_uuid,
        permission_mode_override,
    )
    .await
}
