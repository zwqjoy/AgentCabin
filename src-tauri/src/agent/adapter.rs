use crate::agent::session_actor::SessionActorHandle;
use crate::models::{AgentSettings, SessionMode, UserSettings};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// ── Session map (actor-based) ──

pub type ActorSessionMap = Arc<Mutex<HashMap<String, SessionActorHandle>>>;

pub fn new_actor_session_map() -> ActorSessionMap {
    Arc::new(Mutex::new(HashMap::new()))
}

// ── Adapter settings ──

#[derive(Clone)]
pub struct AdapterSettings {
    pub model: Option<String>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub permission_mode: Option<String>,
    pub append_system_prompt: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub fallback_model: Option<String>,
    pub system_prompt: Option<String>,
    pub tool_set: Option<String>,
    pub add_dirs: Vec<String>,
    pub json_schema: Option<String>,
    pub include_partial_messages: bool,
    pub cli_debug: Option<String>,
    pub no_session_persistence: bool,
    pub max_turns: Option<u32>,
    pub effort: Option<String>,
    pub betas: Vec<String>,
    pub agents_json: Option<String>,
    /// Codex per-session flags. Ignored by Claude spawn path.
    pub ephemeral: bool,
    pub profile: Option<String>,
    pub ignore_user_config: bool,
    pub ignore_rules: bool,
    /// Codex `--search` — enable the native web_search tool. New sessions only.
    pub web_search: bool,
    /// Codex third-party provider (OpenAI Responses API). Injected as `-c model_providers.*`
    /// overrides + an env var at spawn. None = plain `codex login`. Codex-only.
    pub codex_provider: Option<crate::models::CodexProviderCredential>,
    /// Pi provider registered through AgentCabin's explicit Provider Bridge at spawn.
    pub pi_provider: Option<crate::models::PiProviderCredential>,
    /// Code-only Pi profile directory (~/.agentcabin/profiles/code/pi).
    pub pi_code_profile_dir: Option<String>,
    /// Work-only Pi profile directory. `None` keeps the existing Code behavior.
    pub pi_agent_dir: Option<String>,
    /// Work-only built-in extension entry point. `None` keeps the existing Code behavior.
    pub pi_work_extension: Option<String>,
    /// Work-only MCP adapter bridge. It injects the app-owned MCP config without global discovery.
    pub pi_work_mcp_adapter: Option<String>,
    /// Work-only Browser adapter bridge. It is loaded only when the Work
    /// Browser config is enabled and has a Work-owned provider secret.
    pub pi_work_browser_adapter: Option<String>,
    /// AgentCabin-shared Pi extension package paths explicitly enabled for the
    /// current profile. Package installation is shared; activation is not.
    pub pi_shared_extension_sources: Vec<String>,
    /// Work-only Pi package/extension sources explicitly allowed for this session.
    pub pi_work_package_sources: Vec<String>,
    /// Work-only skill files/directories explicitly allowed for this session.
    pub pi_work_skill_sources: Vec<String>,
    /// Work-only Workspace root used to add native Pi permission boundaries.
    pub pi_workspace_root: Option<String>,
    /// Work-only external directory permissions, represented as (path, writable).
    pub pi_work_access_roots: Vec<(String, bool)>,
    /// Work-only current-message attachment directory, kept outside the Workspace.
    pub pi_work_attachment_root: Option<String>,
    /// Explicit Work policy decision to launch Pi outside the OS sandbox.
    /// Kept separate from Code-mode Pi `permission_mode` so Work isolation
    /// cannot erase or inherit this decision accidentally.
    pub pi_work_full_access: bool,
    /// Pi `-e npm:pi-permission-system` — load the built-in permission system.
    /// This is always true for Pi Code sessions; Work clears it while applying its own sandbox.
    pub pi_permission_system_enabled: bool,
    pub pi_plan_mode_enabled: bool,
    pub pi_goal_enabled: bool,
    pub pi_context_prune_enabled: bool,
    pub pi_multi_edit_enabled: bool,
    pub pi_lsp_enabled: bool,
    pub global_providers: Vec<crate::models::GlobalProviderCredential>,
}

/// Map AgentCabin permission mode names to Claude CLI `--permission-mode` values.
pub(crate) fn map_permission_mode(mode: &str) -> String {
    match mode {
        "ask" | "ask-all" | "ask_all" => "default".to_string(),
        "auto_read" => "acceptEdits".to_string(),
        "auto_all" | "auto-grant" | "auto_grant" => "bypassPermissions".to_string(),
        "auto" => "auto".to_string(),
        "delegate" => "acceptEdits".to_string(), // CLI v2.1.81+ alias for acceptEdits
        "dont_ask" => "dontAsk".to_string(),
        "plan" => "plan".to_string(),
        // CLI names passed through unchanged
        "default" | "acceptEdits" | "bypassPermissions" | "dontAsk" => mode.to_string(),
        other => {
            log::warn!(
                "[adapter] unknown permission_mode '{}', passing through to CLI",
                other
            );
            other.to_string()
        }
    }
}

/// Apply a session/message-scoped permission mode without changing persisted settings.
/// Both session actors and the legacy pipe transport use this helper so app-facing aliases
/// cannot be handled differently depending on which transport wins the startup race.
pub(crate) fn apply_permission_mode_override(
    settings: &mut AdapterSettings,
    override_mode: Option<&str>,
) {
    if let Some(mode) = override_mode.filter(|mode| !mode.trim().is_empty()) {
        settings.permission_mode = Some(map_permission_mode(mode));
    }
}

/// When a third-party provider supplies a default model (injected as `ANTHROPIC_MODEL` env var),
/// clear `adapter.model` if it only came from the `user.default_model` fallback.
/// This prevents the `--model` CLI flag from overriding the provider's env var.
/// Explicit choices (UI model_override or agent config) are preserved.
pub fn clear_model_if_provider_overrides(
    adapter: &mut AdapterSettings,
    model_override: &Option<String>,
    agent_model: &Option<String>,
    provider_models: &Option<Vec<String>>,
) {
    if provider_models.as_ref().is_none_or(|m| m.is_empty()) {
        return;
    }
    let has_explicit_ui = model_override.as_ref().is_some_and(|m| !m.is_empty());
    let has_explicit_agent = agent_model.as_ref().is_some_and(|m| !m.is_empty());
    if !has_explicit_ui && !has_explicit_agent {
        log::debug!(
            "[adapter] provider has default_model, clearing --model flag (was {:?}) to let ANTHROPIC_MODEL env var take effect",
            adapter.model
        );
        adapter.model = None;
    }
}

/// Build a unified `AdapterSettings` from agent + user settings.
/// Agent-level settings take priority over user-level.
/// `model_override` (from UI per-message) takes highest priority.
pub fn build_adapter_settings(
    agent: &AgentSettings,
    user: &UserSettings,
    model_override: Option<String>,
) -> AdapterSettings {
    let explicit_model = model_override.filter(|model| !model.trim().is_empty());
    // A managed Codex provider owns the model namespace. Keep an explicit run/composer choice
    // authoritative, otherwise use the provider default before any stale native Codex setting.
    let mut codex_provider = if agent.agent == "codex" {
        user.codex_provider.clone()
    } else {
        None
    };
    if let (Some(provider), Some(selected)) = (&mut codex_provider, &explicit_model) {
        provider.model = selected.clone();
    }
    let managed_codex_model = codex_provider
        .as_ref()
        .map(|provider| provider.model.trim())
        .filter(|model| !model.is_empty())
        .map(str::to_string);
    let model = explicit_model
        .or(managed_codex_model)
        .or_else(|| agent.model.clone())
        .or_else(|| user.default_model.clone());

    let allowed_tools = if agent.allowed_tools.is_empty() {
        user.allowed_tools.clone()
    } else {
        agent.allowed_tools.clone()
    };

    let disallowed_tools = agent.disallowed_tools.clone().unwrap_or_default();

    // Permission mode priority: agent.permission_mode > agent.plan_mode > user.permission_mode
    let has_agent_perm = agent
        .permission_mode
        .as_ref()
        .is_some_and(|pm| !pm.is_empty());
    let permission_mode = if has_agent_perm {
        Some(map_permission_mode(agent.permission_mode.as_ref().unwrap()))
    } else if agent.plan_mode.unwrap_or(false) {
        Some("plan".to_string())
    } else {
        let raw = &user.permission_mode;
        if raw.is_empty() {
            None
        } else {
            Some(map_permission_mode(raw))
        }
    };

    let append_system_prompt = agent.append_system_prompt.clone();

    // Budget: agent-level overrides user-level
    let max_budget_usd = agent.max_budget_usd.or(user.max_budget_usd);

    // Fallback model: agent-level overrides user-level
    let fallback_model = agent
        .fallback_model
        .clone()
        .or_else(|| user.fallback_model.clone());

    let system_prompt = agent.system_prompt.clone();
    let tool_set = agent.tool_set.clone();
    let add_dirs = agent.add_dirs.clone().unwrap_or_default();
    let json_schema = agent
        .json_schema
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());
    // Default to true: stream sessions need partial messages for real-time streaming.
    // Users can explicitly set false in agent config to disable.
    let include_partial_messages = agent.include_partial_messages.unwrap_or(true);
    let cli_debug = agent.cli_debug.clone();
    let no_session_persistence = agent.no_session_persistence.unwrap_or(false);
    let max_turns = agent.max_turns;
    let effort = agent.effort.clone();
    let betas = agent.betas.clone().unwrap_or_default();
    let agents_json = agent.agents_json.clone();
    // Codex per-session flags. `profile` empty string is treated as unset
    // (UI saves "" to clear the value).
    let ephemeral = agent.ephemeral.unwrap_or(false);
    let profile = agent.profile.clone().filter(|s| !s.is_empty());
    let ignore_user_config = agent.ignore_user_config.unwrap_or(false);
    let ignore_rules = agent.ignore_rules.unwrap_or(false);
    let web_search = agent.web_search.unwrap_or(false);
    let pi_provider = if agent.agent == "pi" {
        user.pi_provider.clone()
    } else {
        None
    };

    // Mutual exclusion: system_prompt takes priority over append_system_prompt
    if system_prompt.is_some() && append_system_prompt.is_some() {
        log::warn!("[adapter] both system_prompt and append_system_prompt are set; system_prompt takes priority");
    }

    log::debug!(
        "[adapter] build_adapter_settings: model={:?}, perm={:?}, allowed={}, disallowed={}, budget={:?}, fallback={:?}, sys_prompt={}chars, append_sys={}chars, tool_set={:?}, add_dirs={}, json_schema={}, partial={}, debug={:?}, no_persist={}, max_turns={:?}, effort={:?}, betas={}, agents_json={}, codex_flags={{ephemeral={}, profile={:?}, ignore_user_config={}, ignore_rules={}, web_search={}}}",
        model,
        permission_mode,
        allowed_tools.len(),
        disallowed_tools.len(),
        max_budget_usd,
        fallback_model,
        system_prompt.as_ref().map_or(0, |s| s.len()),
        append_system_prompt.as_ref().map_or(0, |s| s.len()),
        tool_set,
        add_dirs.len(),
        json_schema.is_some(),
        include_partial_messages,
        cli_debug,
        no_session_persistence,
        max_turns,
        effort,
        betas.len(),
        agents_json.is_some(),
        ephemeral,
        profile,
        ignore_user_config,
        ignore_rules,
        web_search,
    );

    // The permission system is a safety boundary, not an optional user feature. Work explicitly
    // clears this internal flag later when it installs its own policy.
    let pi_permission_system_enabled = agent.agent == "pi";
    let pi_plan_mode_enabled = if agent.agent == "pi" {
        agent.pi_plan_mode_enabled.unwrap_or(true)
    } else {
        false
    };
    let pi_goal_enabled = if agent.agent == "pi" {
        agent.pi_goal_enabled.unwrap_or(true)
    } else {
        false
    };
    let pi_context_prune_enabled = if agent.agent == "pi" {
        agent.pi_context_prune_enabled.unwrap_or(false)
    } else {
        false
    };
    // Multi-edit is an AgentCabin-managed Pi Code baseline. Keep the persisted
    // field for backward-compatible settings deserialization, but never allow
    // an old `false` value to disable this system capability.
    let pi_multi_edit_enabled = agent.agent == "pi";
    let pi_lsp_enabled = if agent.agent == "pi" {
        agent.pi_lsp_enabled.unwrap_or(false)
    } else {
        false
    };

    AdapterSettings {
        model,
        allowed_tools,
        disallowed_tools,
        permission_mode,
        append_system_prompt,
        max_budget_usd,
        fallback_model,
        system_prompt,
        tool_set,
        add_dirs,
        json_schema,
        include_partial_messages,
        cli_debug,
        no_session_persistence,
        max_turns,
        effort,
        betas,
        agents_json,
        ephemeral,
        profile,
        ignore_user_config,
        ignore_rules,
        web_search,
        codex_provider,
        pi_provider,
        pi_code_profile_dir: None,
        pi_agent_dir: None,
        pi_work_extension: None,
        pi_work_mcp_adapter: None,
        pi_work_browser_adapter: None,
        pi_shared_extension_sources: Vec::new(),
        pi_work_package_sources: Vec::new(),
        pi_work_skill_sources: Vec::new(),
        pi_workspace_root: None,
        pi_work_access_roots: Vec::new(),
        pi_work_attachment_root: None,
        pi_work_full_access: false,
        pi_permission_system_enabled,
        pi_plan_mode_enabled,
        pi_goal_enabled,
        pi_context_prune_enabled,
        pi_multi_edit_enabled,
        pi_lsp_enabled,
        global_providers: user.global_providers.clone(),
    }
}

/// Add historical context to a fresh continuation without turning it into a user message.
/// System prompt wins over append-system-prompt, so merge into the selected field accordingly.
pub fn append_continuation_context(settings: &mut AdapterSettings, context: Option<&str>) {
    let Some(context) = context.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };

    if let Some(system_prompt) = settings.system_prompt.take() {
        settings.system_prompt = Some(format!("{system_prompt}\n\n{context}"));
    } else if let Some(append_prompt) = settings.append_system_prompt.take() {
        settings.append_system_prompt = Some(format!("{append_prompt}\n\n{context}"));
    } else {
        settings.append_system_prompt = Some(context.to_string());
    }
}

/// Build CLI args for settings flags (shared between stream and pipe modes).
/// Returns Vec of args to append. `print_mode` controls print-only flags.
pub fn build_settings_args(settings: &AdapterSettings, print_mode: bool) -> Vec<String> {
    let mut args = Vec::new();

    // Model
    if let Some(ref m) = settings.model {
        if !m.is_empty() {
            args.push("--model".into());
            args.push(m.clone());
        }
    }

    // Allowed tools (filter out tools that require per-use approval)
    const NEVER_ALLOW_TOOLS: &[&str] = &["ExitPlanMode", "EnterPlanMode"];
    let filtered_tools: Vec<&String> = settings
        .allowed_tools
        .iter()
        .filter(|t| {
            if NEVER_ALLOW_TOOLS.contains(&t.as_str()) {
                log::warn!(
                    "[adapter] filtered '{}' from --allowedTools (requires per-use approval)",
                    t
                );
                false
            } else {
                true
            }
        })
        .collect();
    if !filtered_tools.is_empty() {
        args.push("--allowedTools".into());
        args.push(
            filtered_tools
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .join(","),
        );
    }

    // Disallowed tools
    if !settings.disallowed_tools.is_empty() {
        args.push("--disallowed-tools".into());
        args.push(settings.disallowed_tools.join(","));
    }

    // Permission mode
    if let Some(ref perm) = settings.permission_mode {
        args.push("--permission-mode".into());
        args.push(perm.clone());
    }

    // System prompt takes priority over append_system_prompt
    if let Some(ref sp) = settings.system_prompt {
        args.push("--system-prompt".into());
        args.push(sp.clone());
    } else if let Some(ref asp) = settings.append_system_prompt {
        args.push("--append-system-prompt".into());
        args.push(asp.clone());
    }

    // Tool set (--tools)
    if let Some(ref ts) = settings.tool_set {
        args.push("--tools".into());
        args.push(ts.clone());
    }

    // Additional directories
    for dir in &settings.add_dirs {
        args.push("--add-dir".into());
        args.push(dir.clone());
    }

    // CLI debug
    if let Some(ref dbg) = settings.cli_debug {
        if dbg.is_empty() {
            args.push("--debug".into());
        } else {
            args.push("--debug".into());
            args.push(dbg.clone());
        }
    }

    // No session persistence
    if settings.no_session_persistence {
        args.push("--no-session-persistence".into());
    }

    // Budget and fallback apply in both stream and print modes for stream sessions,
    // but only in print mode for pipe (single-shot)
    if let Some(budget) = settings.max_budget_usd {
        args.push("--max-budget-usd".into());
        args.push(budget.to_string());
    }
    if let Some(ref fb) = settings.fallback_model {
        args.push("--fallback-model".into());
        args.push(fb.clone());
    }

    // Max turns
    if let Some(turns) = settings.max_turns {
        args.push("--max-turns".into());
        args.push(turns.to_string());
    }

    // Effort level
    if let Some(ref eff) = settings.effort {
        if !eff.is_empty() {
            args.push("--effort".into());
            args.push(eff.clone());
        }
    }

    // Betas
    if !settings.betas.is_empty() {
        args.push("--betas".into());
        args.push(settings.betas.join(","));
    }

    // Custom agent definitions
    if let Some(ref agents) = settings.agents_json {
        if !agents.is_empty() {
            args.push("--agents".into());
            args.push(agents.clone());
        }
    }

    // Print-only flags
    if print_mode {
        if let Some(ref schema) = settings.json_schema {
            args.push("--json-schema".into());
            args.push(schema.clone());
        }
        // NOTE: --include-partial-messages is NOT added here because it requires
        // --output-format=stream-json (CLI error otherwise). Only spawn_cli_process
        // (session.rs) adds it, since it's the only caller in stream-json mode.
    }

    args
}

/// Validate settings + mode combination before spawning.
pub fn validate_session_params(
    settings: &AdapterSettings,
    mode: &SessionMode,
) -> Result<(), String> {
    if settings.no_session_persistence && !matches!(mode, SessionMode::New) {
        return Err("Cannot resume/continue with no_session_persistence enabled".into());
    }
    Ok(())
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
    fn test_build_args_defaults_empty() {
        let s = make_settings();
        let args = build_settings_args(&s, false);
        assert!(args.is_empty(), "all-None settings should produce no args");
    }

    #[test]
    fn pi_permission_system_is_enabled_for_pi_code() {
        let agent = crate::models::AgentSettings::default_for("pi");
        let settings =
            build_adapter_settings(&agent, &crate::models::UserSettings::default(), None);

        assert!(settings.pi_permission_system_enabled);
    }

    #[test]
    fn pi_multi_edit_cannot_be_disabled_for_pi_code() {
        let mut agent = crate::models::AgentSettings::default_for("pi");
        agent.pi_multi_edit_enabled = Some(false);
        let settings =
            build_adapter_settings(&agent, &crate::models::UserSettings::default(), None);

        assert!(settings.pi_multi_edit_enabled);
    }

    #[test]
    fn test_build_args_system_prompt_overrides_append() {
        let mut s = make_settings();
        s.system_prompt = Some("replace".into());
        s.append_system_prompt = Some("append".into());
        let args = build_settings_args(&s, false);
        assert!(args.contains(&"--system-prompt".to_string()));
        assert!(!args.contains(&"--append-system-prompt".to_string()));
    }

    #[test]
    fn test_build_args_append_when_no_system_prompt() {
        let mut s = make_settings();
        s.append_system_prompt = Some("append text".into());
        let args = build_settings_args(&s, false);
        assert!(args.contains(&"--append-system-prompt".to_string()));
        assert!(!args.contains(&"--system-prompt".to_string()));
    }

    #[test]
    fn continuation_context_is_a_system_prompt_not_a_user_turn() {
        let mut settings = make_settings();
        append_continuation_context(&mut settings, Some("historical context"));

        assert_eq!(
            settings.append_system_prompt.as_deref(),
            Some("historical context")
        );
        assert!(build_settings_args(&settings, false)
            .windows(2)
            .any(|pair| pair == ["--append-system-prompt", "historical context"]));
    }

    #[test]
    fn test_build_args_json_schema_print_only() {
        let mut s = make_settings();
        s.json_schema = Some(r#"{"type":"object"}"#.into());
        assert!(
            !build_settings_args(&s, false).contains(&"--json-schema".to_string()),
            "json_schema should not appear in non-print mode"
        );
        assert!(
            build_settings_args(&s, true).contains(&"--json-schema".to_string()),
            "json_schema should appear in print mode"
        );
    }

    #[test]
    fn test_build_args_include_partial_never_emitted() {
        // --include-partial-messages requires --output-format=stream-json which only
        // spawn_cli_process (session.rs) uses. build_settings_args must never emit it.
        let mut s = make_settings();
        s.include_partial_messages = true;
        assert!(
            !build_settings_args(&s, false).contains(&"--include-partial-messages".to_string()),
            "include_partial_messages should not appear in non-print mode"
        );
        assert!(
            !build_settings_args(&s, true).contains(&"--include-partial-messages".to_string()),
            "include_partial_messages should not appear in print mode either (only session.rs adds it)"
        );
    }

    #[test]
    fn test_validate_no_persistence_resume_conflict() {
        let mut s = make_settings();
        s.no_session_persistence = true;
        assert!(validate_session_params(&s, &SessionMode::New).is_ok());
        assert!(validate_session_params(&s, &SessionMode::Resume).is_err());
        assert!(validate_session_params(&s, &SessionMode::Continue).is_err());
        assert!(validate_session_params(&s, &SessionMode::Fork).is_err());
    }

    #[test]
    fn test_validate_normal_modes_ok() {
        let s = make_settings();
        assert!(validate_session_params(&s, &SessionMode::New).is_ok());
        assert!(validate_session_params(&s, &SessionMode::Resume).is_ok());
        assert!(validate_session_params(&s, &SessionMode::Continue).is_ok());
        assert!(validate_session_params(&s, &SessionMode::Fork).is_ok());
    }

    #[test]
    fn test_build_args_add_dirs() {
        let mut s = make_settings();
        s.add_dirs = vec!["/path/a".into(), "/path/b".into()];
        let args = build_settings_args(&s, false);
        let add_dir_count = args.iter().filter(|a| *a == "--add-dir").count();
        assert_eq!(add_dir_count, 2);
        assert!(args.contains(&"/path/a".to_string()));
        assert!(args.contains(&"/path/b".to_string()));
    }

    #[test]
    fn test_build_args_cli_debug_empty_filter() {
        let mut s = make_settings();
        s.cli_debug = Some("".into());
        let args = build_settings_args(&s, false);
        assert!(args.contains(&"--debug".to_string()));
        // Empty filter → just "--debug" with no arg after it
        assert_eq!(args.iter().filter(|a| *a == "--debug").count(), 1);
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_build_args_cli_debug_with_filter() {
        let mut s = make_settings();
        s.cli_debug = Some("api".into());
        let args = build_settings_args(&s, false);
        assert!(args.contains(&"--debug".to_string()));
        assert!(args.contains(&"api".to_string()));
    }

    #[test]
    fn test_build_args_all_flags() {
        let mut s = make_settings();
        s.model = Some("opus".into());
        s.allowed_tools = vec!["Read".into(), "Write".into()];
        s.disallowed_tools = vec!["Bash".into()];
        s.permission_mode = Some("plan".into());
        s.system_prompt = Some("Be helpful".into());
        s.tool_set = Some("extended".into());
        s.add_dirs = vec!["/extra".into()];
        s.cli_debug = Some("verbose".into());
        s.no_session_persistence = true;
        s.max_budget_usd = Some(5.0);
        s.fallback_model = Some("haiku".into());
        s.json_schema = Some(r#"{"type":"object"}"#.into());
        s.include_partial_messages = true;
        s.max_turns = Some(20);
        s.effort = Some("high".into());
        s.betas = vec!["context-1m-2025-08-07".into()];
        s.agents_json = Some(r#"[{"description":"test"}]"#.into());

        let args = build_settings_args(&s, true);

        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"opus".to_string()));
        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"--disallowed-tools".to_string()));
        assert!(args.contains(&"--permission-mode".to_string()));
        assert!(args.contains(&"plan".to_string()));
        assert!(args.contains(&"--system-prompt".to_string()));
        assert!(args.contains(&"Be helpful".to_string()));
        assert!(!args.contains(&"--append-system-prompt".to_string()));
        assert!(args.contains(&"--tools".to_string()));
        assert!(args.contains(&"extended".to_string()));
        assert!(args.contains(&"--add-dir".to_string()));
        assert!(args.contains(&"/extra".to_string()));
        assert!(args.contains(&"--debug".to_string()));
        assert!(args.contains(&"verbose".to_string()));
        assert!(args.contains(&"--no-session-persistence".to_string()));
        assert!(args.contains(&"--max-budget-usd".to_string()));
        assert!(args.contains(&"5".to_string()));
        assert!(args.contains(&"--fallback-model".to_string()));
        assert!(args.contains(&"haiku".to_string()));
        assert!(args.contains(&"--json-schema".to_string()));
        assert!(args.contains(&"--max-turns".to_string()));
        assert!(args.contains(&"20".to_string()));
        assert!(args.contains(&"--effort".to_string()));
        assert!(args.contains(&"high".to_string()));
        assert!(args.contains(&"--betas".to_string()));
        assert!(args.contains(&"context-1m-2025-08-07".to_string()));
        assert!(args.contains(&"--agents".to_string()));
        assert!(args.contains(&r#"[{"description":"test"}]"#.to_string()));
        // --include-partial-messages is NOT emitted by build_settings_args
        // (only spawn_cli_process adds it, since it requires --output-format=stream-json)
        assert!(!args.contains(&"--include-partial-messages".to_string()));
    }

    #[test]
    fn test_build_args_effort() {
        let mut s = make_settings();
        s.effort = Some("low".into());
        let args = build_settings_args(&s, false);
        assert!(args.contains(&"--effort".to_string()));
        assert!(args.contains(&"low".to_string()));
    }

    #[test]
    fn test_build_args_effort_empty_skipped() {
        let mut s = make_settings();
        s.effort = Some("".into());
        let args = build_settings_args(&s, false);
        assert!(!args.contains(&"--effort".to_string()));
    }

    #[test]
    fn test_build_args_betas() {
        let mut s = make_settings();
        s.betas = vec!["context-1m-2025-08-07".into()];
        let args = build_settings_args(&s, false);
        assert!(args.contains(&"--betas".to_string()));
        assert!(args.contains(&"context-1m-2025-08-07".to_string()));
    }

    #[test]
    fn test_build_args_agents_json() {
        let mut s = make_settings();
        s.agents_json = Some(r#"[{"description":"reviewer"}]"#.into());
        let args = build_settings_args(&s, false);
        assert!(args.contains(&"--agents".to_string()));
        assert!(args.contains(&r#"[{"description":"reviewer"}]"#.to_string()));
    }

    #[test]
    fn test_build_args_agents_json_empty_skipped() {
        let mut s = make_settings();
        s.agents_json = Some("".into());
        let args = build_settings_args(&s, false);
        assert!(!args.contains(&"--agents".to_string()));
    }

    // ── map_permission_mode tests ──

    #[test]
    fn test_map_permission_mode_plan() {
        assert_eq!(map_permission_mode("plan"), "plan");
    }

    #[test]
    fn test_map_permission_mode_all_known() {
        assert_eq!(map_permission_mode("ask"), "default");
        assert_eq!(map_permission_mode("ask-all"), "default");
        assert_eq!(map_permission_mode("ask_all"), "default");
        assert_eq!(map_permission_mode("auto_read"), "acceptEdits");
        assert_eq!(map_permission_mode("auto_all"), "bypassPermissions");
        assert_eq!(map_permission_mode("auto-grant"), "bypassPermissions");
        assert_eq!(map_permission_mode("auto_grant"), "bypassPermissions");
        assert_eq!(map_permission_mode("auto"), "auto");
        assert_eq!(map_permission_mode("delegate"), "acceptEdits");
        assert_eq!(map_permission_mode("dont_ask"), "dontAsk");
        assert_eq!(map_permission_mode("plan"), "plan");
        // CLI names pass through
        assert_eq!(map_permission_mode("default"), "default");
        assert_eq!(map_permission_mode("acceptEdits"), "acceptEdits");
        assert_eq!(
            map_permission_mode("bypassPermissions"),
            "bypassPermissions"
        );
        assert_eq!(map_permission_mode("dontAsk"), "dontAsk");
    }

    #[test]
    fn test_permission_mode_override_uses_the_same_mapping_for_all_transports() {
        let mut settings = make_settings();

        apply_permission_mode_override(&mut settings, Some("auto_all"));
        assert_eq!(
            settings.permission_mode.as_deref(),
            Some("bypassPermissions")
        );

        apply_permission_mode_override(&mut settings, Some(""));
        assert_eq!(
            settings.permission_mode.as_deref(),
            Some("bypassPermissions")
        );
    }

    // ── build_adapter_settings priority tests ──

    fn make_user_settings() -> UserSettings {
        UserSettings {
            default_agent: "claude".to_string(),
            default_model: None,
            allowed_tools: vec![],
            working_directory: None,
            provider_mode: "cli".to_string(),
            auth_mode: "cli".to_string(),
            anthropic_api_key: None,
            anthropic_base_url: None,
            auth_env_var: None,
            permission_mode: String::new(),
            max_budget_usd: None,
            fallback_model: None,
            keybinding_overrides: vec![],
            remote_hosts: vec![],
            platform_credentials: vec![],
            active_platform_id: None,
            global_providers: vec![],
            agent_provider_bindings: None,
            codex_provider: None,
            codex_transport: None,
            pi_provider: None,
            ui_zoom: None,
            onboarding_completed: false,
            work_mode_enabled: true,
            web_server_enabled: None,
            web_server_token: None,
            web_server_port: None,
            web_server_bind: None,
            web_server_allowed_origins: None,
            web_server_tunnel_url: None,
            claude_path: None,
            codex_path: None,
            pi_path: None,
            grok_path: None,
            dsh_path: None,
            enabled_agents: None,
            code_default_runtime: None,
            work_default_runtime: None,
            worktree_root: None,
            worktree_branch_prefix: "agentcabin/".to_string(),
            worktree_auto_cleanup: true,
            worktree_cleanup_limit: 15,
            worktree_managed_paths: vec![],
            pet_enabled: false,
            pet_scale: 1.0,
            pet_always_on_top: true,
            pet_id: "agentcabin-bot".to_string(),
            pet_patrol_enabled: true,
            pet_patrol_pause_min: 3,
            pet_snap_to_edge: false,
            pet_click_interaction_enabled: true,
            pet_grokbot_color: "blue".to_string(),
            pet_grokbot_shape: "blob".to_string(),
            pet_grokbot_parts: vec![],
            pet_grokbot_accessories: vec![],
            updated_at: crate::models::now_iso(),
        }
    }

    #[test]
    fn test_adapter_agent_permission_mode_takes_priority() {
        let mut agent = AgentSettings::default_for("codex");
        agent.permission_mode = Some("auto_all".to_string());
        agent.plan_mode = Some(true); // plan_mode should be overridden
        let mut user = make_user_settings();
        user.permission_mode = "ask".to_string(); // user perm should be overridden

        let adapter = build_adapter_settings(&agent, &user, None);
        assert_eq!(
            adapter.permission_mode.as_deref(),
            Some("bypassPermissions")
        );
    }

    #[test]
    fn pi_provider_is_scoped_to_pi_adapter() {
        let mut user = make_user_settings();
        user.pi_provider = Some(crate::models::PiProviderCredential {
            id: "custom-openai".into(),
            name: "Custom OpenAI-compatible".into(),
            base_url: "https://example.test/v1".into(),
            api: "openai-completions".into(),
            model: "model-a".into(),
            models: vec![],
            context_window: None,
            api_key: Some("secret".into()),
        });

        let pi = build_adapter_settings(&AgentSettings::default_for("pi"), &user, None);
        assert_eq!(
            pi.pi_provider.as_ref().map(|provider| provider.id.as_str()),
            Some("custom-openai")
        );

        let claude = build_adapter_settings(&AgentSettings::default_for("claude"), &user, None);
        assert!(claude.pi_provider.is_none());
    }

    fn managed_codex_provider(model: &str) -> crate::models::CodexProviderCredential {
        crate::models::CodexProviderCredential {
            id: "new-api".into(),
            name: "NewAPI".into(),
            base_url: "http://127.0.0.1:3000/v1".into(),
            env_key: "OPENAI_API_KEY".into(),
            wire_api: "chat".into(),
            model: model.into(),
            api_key: Some("secret".into()),
            supports_reasoning_effort: None,
            supports_websockets: None,
        }
    }

    #[test]
    fn managed_codex_model_beats_a_stale_native_agent_model() {
        let mut agent = AgentSettings::default_for("codex");
        agent.model = Some("gpt-5.6-luna".into());
        let mut user = make_user_settings();
        user.codex_provider = Some(managed_codex_provider("DeepSeek-V4-Flash"));

        let adapter = build_adapter_settings(&agent, &user, None);

        assert_eq!(adapter.model.as_deref(), Some("DeepSeek-V4-Flash"));
        assert_eq!(
            adapter
                .codex_provider
                .as_ref()
                .map(|provider| provider.model.as_str()),
            Some("DeepSeek-V4-Flash")
        );
    }

    #[test]
    fn explicit_codex_run_model_updates_the_runtime_provider_model() {
        let agent = AgentSettings::default_for("codex");
        let mut user = make_user_settings();
        user.codex_provider = Some(managed_codex_provider("deepseek/deepseek-v4-flash"));

        let adapter = build_adapter_settings(&agent, &user, Some("DeepSeek-V4-Flash".into()));

        assert_eq!(adapter.model.as_deref(), Some("DeepSeek-V4-Flash"));
        assert_eq!(
            adapter
                .codex_provider
                .as_ref()
                .map(|provider| provider.model.as_str()),
            Some("DeepSeek-V4-Flash")
        );
    }

    #[test]
    fn test_adapter_plan_mode_takes_priority_over_user() {
        let mut agent = AgentSettings::default_for("codex");
        agent.plan_mode = Some(true);
        let mut user = make_user_settings();
        user.permission_mode = "auto_all".to_string();

        let adapter = build_adapter_settings(&agent, &user, None);
        assert_eq!(adapter.permission_mode.as_deref(), Some("plan"));
    }

    #[test]
    fn test_adapter_user_permission_mode_fallback() {
        let agent = AgentSettings::default_for("codex");
        let mut user = make_user_settings();
        user.permission_mode = "auto_all".to_string();

        let adapter = build_adapter_settings(&agent, &user, None);
        assert_eq!(
            adapter.permission_mode.as_deref(),
            Some("bypassPermissions")
        );
    }

    #[test]
    fn test_adapter_empty_agent_permission_mode_falls_through() {
        let mut agent = AgentSettings::default_for("codex");
        agent.permission_mode = Some(String::new()); // empty → falls through
        let mut user = make_user_settings();
        user.permission_mode = "ask".to_string();

        let adapter = build_adapter_settings(&agent, &user, None);
        // Empty agent.permission_mode → None → falls through to user
        assert_eq!(adapter.permission_mode.as_deref(), Some("default"));
    }

    #[test]
    fn test_adapter_propagates_codex_flags() {
        let mut agent = AgentSettings::default_for("codex");
        agent.ephemeral = Some(true);
        agent.profile = Some("dev".into());
        agent.ignore_user_config = Some(true);
        agent.ignore_rules = Some(true);
        agent.web_search = Some(true);
        let user = make_user_settings();

        let adapter = build_adapter_settings(&agent, &user, None);
        assert!(adapter.ephemeral);
        assert_eq!(adapter.profile.as_deref(), Some("dev"));
        assert!(adapter.ignore_user_config);
        assert!(adapter.ignore_rules);
        assert!(adapter.web_search);
    }

    #[test]
    fn test_adapter_codex_flags_default_off() {
        let agent = AgentSettings::default_for("codex");
        let user = make_user_settings();
        let adapter = build_adapter_settings(&agent, &user, None);
        assert!(!adapter.ephemeral);
        assert_eq!(adapter.profile, None);
        assert!(!adapter.ignore_user_config);
        assert!(!adapter.ignore_rules);
        assert!(!adapter.web_search);
    }

    #[test]
    fn test_adapter_codex_profile_empty_filtered_to_none() {
        let mut agent = AgentSettings::default_for("codex");
        agent.profile = Some(String::new()); // empty string from UI clear
        let user = make_user_settings();
        let adapter = build_adapter_settings(&agent, &user, None);
        assert_eq!(adapter.profile, None);
    }
}
