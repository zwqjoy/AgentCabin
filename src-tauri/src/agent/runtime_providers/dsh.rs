use super::{
    add_code_desktop_use_env, cleanup_managed_directory, ensure_real_directory, project_skills,
    write_managed_file, RuntimeProviderAdapter, RuntimeSpawnConfig,
};
use crate::agent::capability_resolver::{EffectiveCapabilities, RuntimeProviderKind};
use crate::agent::claude_stream;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn capability_mcp_server(
    caps: &EffectiveCapabilities,
) -> Result<Option<crate::agent::capability_resolver::EffectiveMcpServer>, String> {
    if !caps.browser_enabled && !caps.browser_use_enabled && caps.connectors.is_empty() {
        return Ok(None);
    }
    let adapter_path = caps
        .managed_runtime_dir
        .join("agentcabin_capabilities_mcp.mjs");
    write_managed_file(
        &adapter_path,
        include_str!("../../work/dsh_capability_mcp_adapter.mjs"),
        "DSH AgentCabin capability MCP adapter",
    )?;
    let node = crate::agent::runtime_locator::resolve_node().or_else(|err| {
        if crate::agent::runtime_locator::packaged() {
            Err(err)
        } else {
            crate::agent::claude_stream::which_binary("node")
                .ok_or_else(|| "node binary not found".to_string())
        }
    })?;
    Ok(Some(
        crate::agent::capability_resolver::EffectiveMcpServer {
            id: "agentcabin_capabilities".to_string(),
            transport: "stdio".to_string(),
            command: Some(node),
            args: vec![adapter_path.to_string_lossy().into_owned()],
            cwd: None,
            url: None,
            env: HashMap::new(),
            headers: HashMap::new(),
        },
    ))
}

fn work_mcp_server(
    caps: &EffectiveCapabilities,
    extra_env: &HashMap<String, String>,
) -> Result<crate::agent::capability_resolver::EffectiveMcpServer, String> {
    ensure_work_computer_use_modules(&caps.managed_runtime_dir)?;
    let adapter_path = caps.managed_runtime_dir.join("dsh_work_mcp_adapter.mjs");
    write_managed_file(
        &adapter_path,
        include_str!("../../work/dsh_work_mcp_adapter.mjs"),
        "DSH AgentCabin Work MCP adapter",
    )?;
    let node = crate::agent::runtime_locator::resolve_node()?;
    let env = extra_env
        .iter()
        .filter(|(key, _)| {
            key.starts_with("AGENTCABIN_WORK_")
                || matches!(
                    key.as_str(),
                    "AGENTCABIN_BROWSER_USE_ENABLED" | "AGENTCABIN_WEB_ENABLED"
                )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    Ok(crate::agent::capability_resolver::EffectiveMcpServer {
        id: "agentcabin_work".to_string(),
        transport: "stdio".to_string(),
        command: Some(node),
        args: vec![adapter_path.to_string_lossy().into_owned()],
        cwd: None,
        url: None,
        env,
        headers: HashMap::new(),
    })
}

fn ensure_work_computer_use_modules(dir: &Path) -> Result<(), String> {
    let modules = [
        (
            "computer_use_v2_runtime.mjs",
            include_str!("../../work/computer_use_v2_runtime.mjs"),
        ),
        (
            "computer_use_v3_models.mjs",
            include_str!("../../work/computer_use_v3_models.mjs"),
        ),
        (
            "desktop_computer_use_backend.mjs",
            include_str!("../../work/desktop_computer_use_backend.mjs"),
        ),
        (
            "cdp_computer_use_backend.mjs",
            include_str!("../../work/cdp_computer_use_backend.mjs"),
        ),
        (
            "visual_grounding_backend.mjs",
            include_str!("../../work/visual_grounding_backend.mjs"),
        ),
    ];
    for (name, source) in modules {
        write_managed_file(
            &dir.join(name),
            source,
            &format!("DSH Computer Use dependency {name}"),
        )?;
    }
    Ok(())
}

const DSH_ROUTE_ENV: &str = "AGENTCABIN_DSH_API_KEY";

const CAPABILITY_BRIDGE_ENV_KEYS: [&str; 8] = [
    "AGENTCABIN_WORK_BRIDGE_PORT",
    "AGENTCABIN_WORK_BRIDGE_TOKEN",
    "AGENTCABIN_CODE_CONNECTOR_BRIDGE_PORT",
    "AGENTCABIN_CODE_CONNECTOR_BRIDGE_TOKEN",
    "AGENTCABIN_BROWSER_BRIDGE_PORT",
    "AGENTCABIN_BROWSER_BRIDGE_TOKEN",
    "AGENTCABIN_BROWSER_USE_ENABLED",
    "AGENTCABIN_WEB_ENABLED",
];

fn inject_capability_bridge_env(
    caps: &EffectiveCapabilities,
    extra_env: &HashMap<String, String>,
) -> Result<(), String> {
    let path = caps.managed_runtime_dir.join("mcp.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read DSH MCP config for bridge env: {error}"))?;
    let mut config: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse DSH MCP config for bridge env: {error}"))?;
    let Some(server) = config
        .get_mut("mcpServers")
        .and_then(|servers| servers.get_mut("agentcabin_capabilities"))
    else {
        return Ok(());
    };
    let env = server
        .as_object_mut()
        .ok_or_else(|| "Invalid AgentCabin capability MCP server config".to_string())?
        .entry("env")
        .or_insert_with(|| serde_json::json!({}));
    let env = env
        .as_object_mut()
        .ok_or_else(|| "Invalid AgentCabin capability MCP server env".to_string())?;
    for key in CAPABILITY_BRIDGE_ENV_KEYS {
        if let Some(value) = extra_env.get(key) {
            env.insert(key.to_string(), serde_json::Value::String(value.clone()));
        }
    }
    write_managed_file(
        &path,
        serde_json::to_string_pretty(&config)
            .map_err(|error| format!("Failed to serialize DSH MCP config: {error}"))?,
        "DSH MCP config with capability bridge env",
    )
}

fn add_capability_bridge_env(
    mut server: crate::agent::capability_resolver::EffectiveMcpServer,
    extra_env: &HashMap<String, String>,
) -> crate::agent::capability_resolver::EffectiveMcpServer {
    for key in CAPABILITY_BRIDGE_ENV_KEYS {
        if let Some(value) = extra_env.get(key) {
            server.env.insert(key.to_string(), value.clone());
        }
    }
    server
}

#[derive(Debug, Clone)]
pub struct DshRouteConfig {
    pub provider: String,
    pub protocol: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub keyless: bool,
    pub model: crate::models::GlobalProviderModel,
}

fn dsh_reasoning_efforts(model: &crate::models::GlobalProviderModel) -> serde_json::Value {
    if model.supports_reasoning == Some(false) {
        return serde_json::Value::Bool(false);
    }
    let configured = model
        .supported_effort_levels
        .as_deref()
        .filter(|levels| !levels.is_empty());
    let levels = configured.map_or_else(
        || {
            let mut levels = vec!["low", "medium", "high"];
            if model.supports_xhigh == Some(true) {
                levels.push("xhigh");
            }
            levels
        },
        |levels| levels.iter().map(String::as_str).collect(),
    );
    let mut efforts = serde_json::Map::new();
    efforts.insert("off".into(), serde_json::Value::Null);
    for level in levels {
        if matches!(
            level,
            "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        ) {
            efforts.insert(
                level.to_string(),
                serde_json::Value::String(level.to_string()),
            );
        }
    }
    if efforts.len() == 1 {
        serde_json::Value::Bool(false)
    } else {
        serde_json::Value::Object(efforts)
    }
}

fn dsh_model_capability_fields(
    model: &crate::models::GlobalProviderModel,
) -> serde_json::Map<String, serde_json::Value> {
    let mut value = serde_json::Map::new();
    value.insert("id".into(), serde_json::Value::String(model.id.clone()));
    if let Some(name) = model.name.as_ref() {
        value.insert("name".into(), serde_json::Value::String(name.clone()));
    }
    if let Some(context_window) = model.context_window {
        value.insert("contextWindow".into(), context_window.into());
    }
    if let Some(max_tokens) = model.max_tokens {
        value.insert("maxTokens".into(), max_tokens.into());
    }
    value.insert("reasoningEfforts".into(), dsh_reasoning_efforts(model));
    if model.supports_images == Some(true) {
        value.insert("input".into(), serde_json::json!(["text", "image"]));
    }
    value
}

pub struct DshRuntimeAdapter;

impl RuntimeProviderAdapter for DshRuntimeAdapter {
    fn kind(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Dsh
    }

    fn prepare_runtime(&self, caps: &EffectiveCapabilities) -> Result<RuntimeSpawnConfig, String> {
        let dsh_home = &caps.managed_runtime_dir;
        let runtime_home = &caps.managed_home;

        ensure_real_directory(dsh_home, "DSH runtime dir")?;
        ensure_real_directory(runtime_home, "DSH runtime home")?;

        // 1. Project enabled skills into dsh_home/skills
        let projected_skills_dir = dsh_home.join("skills");
        project_skills(&projected_skills_dir, &caps.enabled_skills)?;

        // 2. Project MCP servers into dsh_home/mcp.json (standard MCP format)
        let mcp_config_path = dsh_home.join("mcp.json");
        let mut servers_map = serde_json::Map::new();
        let mut mcp_servers = caps.mcp_servers.clone();
        if let Some(server) = super::code_desktop_mcp_server(caps)? {
            mcp_servers.push(server);
        }
        if let Some(server) = capability_mcp_server(caps)? {
            mcp_servers.push(server);
        }

        for server in &mcp_servers {
            let mut obj = serde_json::Map::new();
            if server.transport == "stdio" {
                if let Some(cmd) = &server.command {
                    obj.insert("command".into(), serde_json::Value::String(cmd.clone()));
                }
                obj.insert(
                    "args".into(),
                    serde_json::Value::Array(
                        server
                            .args
                            .iter()
                            .map(|a| serde_json::Value::String(a.clone()))
                            .collect(),
                    ),
                );
                if let Some(cwd) = &server.cwd {
                    obj.insert(
                        "cwd".into(),
                        serde_json::Value::String(cwd.to_string_lossy().into_owned()),
                    );
                }
                if !server.env.is_empty() {
                    let mut env_map = serde_json::Map::new();
                    for (k, v) in &server.env {
                        env_map.insert(k.clone(), serde_json::Value::String(v.clone()));
                    }
                    obj.insert("env".into(), serde_json::Value::Object(env_map));
                }
            } else {
                obj.insert(
                    "transport".into(),
                    serde_json::Value::String(server.transport.clone()),
                );
                if let Some(url) = &server.url {
                    obj.insert("url".into(), serde_json::Value::String(url.clone()));
                }
                if !server.headers.is_empty() {
                    let mut headers_map = serde_json::Map::new();
                    for (k, v) in &server.headers {
                        headers_map.insert(k.clone(), serde_json::Value::String(v.clone()));
                    }
                    obj.insert("headers".into(), serde_json::Value::Object(headers_map));
                }
            }
            servers_map.insert(server.id.clone(), serde_json::Value::Object(obj));
        }

        let mcp_json = serde_json::json!({
            "mcpServers": servers_map
        });
        let mcp_contents = serde_json::to_string_pretty(&mcp_json)
            .map_err(|e| format!("Failed to serialize DSH MCP config: {e}"))?;
        write_managed_file(&mcp_config_path, mcp_contents, "DSH MCP config")?;

        // 3. Isolated session directory inside managed home
        let sessions_dir = dsh_home.join("sessions");
        ensure_real_directory(&sessions_dir, "DSH sessions dir")?;

        // 4. Build environment
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), claude_stream::augmented_path());
        env.insert(
            "DSH_HOME".to_string(),
            dsh_home.to_string_lossy().to_string(),
        );
        env.insert(
            "DSH_CONFIG_DIR".to_string(),
            dsh_home.to_string_lossy().to_string(),
        );
        env.insert(
            "HOME".to_string(),
            runtime_home.to_string_lossy().to_string(),
        );
        add_code_desktop_use_env(&mut env, caps.app_mode);

        if caps.app_mode == crate::work::models::AppMode::Code {
            let code_profile_dir = crate::storage::profile_bindings::dsh_code_profile_dir()
                .join("profiles")
                .join("sdk-minimal");
            let legacy_profile_dir = crate::storage::data_dir()
                .join("runtime")
                .join("dsh")
                .join("profiles")
                .join("sdk-minimal");
            let persistent_profile_dir = if code_profile_dir.exists() {
                Some(code_profile_dir)
            } else if legacy_profile_dir.exists() {
                Some(legacy_profile_dir)
            } else {
                None
            };
            if let Some(persistent_profile_dir) = persistent_profile_dir {
                let target_profile_dir = dsh_home.join("profiles").join("sdk-minimal");
                let _ = std::fs::create_dir_all(&target_profile_dir);
                let src_modules = persistent_profile_dir.join("node_modules");
                let dst_modules = target_profile_dir.join("node_modules");
                if src_modules.exists() && !dst_modules.exists() {
                    #[cfg(unix)]
                    let _ = std::os::unix::fs::symlink(&src_modules, &dst_modules);
                    #[cfg(windows)]
                    let _ = std::os::windows::fs::symlink_dir(&src_modules, &dst_modules);
                }
                for file_name in ["package.json", "pnpm-workspace.yaml", "pnpm-lock.yaml"] {
                    let src_file = persistent_profile_dir.join(file_name);
                    let dst_file = target_profile_dir.join(file_name);
                    if src_file.is_file() {
                        let _ = std::fs::copy(src_file, dst_file);
                    }
                }
                if src_modules.exists() {
                    env.insert(
                        "NODE_PATH".to_string(),
                        src_modules.to_string_lossy().to_string(),
                    );
                }
            }
        }

        if caps.app_mode == crate::work::models::AppMode::Work {
            ensure_work_computer_use_modules(&dsh_home)?;
            let work_adapter_path = dsh_home.join("dsh_work_mcp_adapter.mjs");
            write_managed_file(
                &work_adapter_path,
                include_str!("../../work/dsh_work_mcp_adapter.mjs"),
                "DSH AgentCabin Work MCP adapter",
            )?;
        }

        let binary = claude_stream::resolve_dsh_path();
        let profile = "web";
        let args = vec!["--profile".to_string(), profile.to_string()];

        Ok(RuntimeSpawnConfig {
            binary,
            args,
            env,
            cwd: PathBuf::new(),
            managed_home: runtime_home.clone(),
        })
    }

    fn cleanup_runtime(&self, caps: &EffectiveCapabilities) -> Result<(), String> {
        cleanup_managed_directory(&caps.managed_runtime_dir, "DSH runtime")
    }
}

impl DshRuntimeAdapter {
    /// Add one AgentCabin-managed provider route to the official DSH SDK profile.
    /// The credential stays in the child environment; only its env reference is
    /// written into the per-run patch file.
    pub fn prepare_runtime_for_route(
        &self,
        caps: &EffectiveCapabilities,
        route: &DshRouteConfig,
    ) -> Result<RuntimeSpawnConfig, String> {
        self.prepare_runtime_for_route_with_env(caps, route, &HashMap::new())
    }

    pub fn prepare_runtime_for_route_with_env(
        &self,
        caps: &EffectiveCapabilities,
        route: &DshRouteConfig,
        extra_env: &HashMap<String, String>,
    ) -> Result<RuntimeSpawnConfig, String> {
        let plugin_data_dir = crate::storage::data_dir();
        self.prepare_runtime_for_route_with_env_and_plugin_data_dir(
            caps,
            route,
            extra_env,
            &plugin_data_dir,
        )
    }

    fn prepare_runtime_for_route_with_env_and_plugin_data_dir(
        &self,
        caps: &EffectiveCapabilities,
        route: &DshRouteConfig,
        extra_env: &HashMap<String, String>,
        plugin_data_dir: &Path,
    ) -> Result<RuntimeSpawnConfig, String> {
        let mut spawn = self.prepare_runtime(caps)?;
        // DSH's MCP launcher uses the server-specific env map. Keep the
        // per-run bridge lease explicit so the capability adapter does not
        // depend on child-process environment inheritance.
        inject_capability_bridge_env(caps, extra_env)?;
        let patch_path = caps
            .managed_runtime_dir
            .join("agentcabin-provider.patch.yml");
        let model_config = dsh_model_capability_fields(&route.model);
        let settings = crate::storage::settings::get_user_settings();
        let compatible = |protocol: &str| {
            matches!(
                protocol,
                "openai-completions" | "openai-responses" | "anthropic-messages"
            )
        };
        let mut providers = serde_json::Map::new();
        let mut provider_credentials = Vec::new();
        for provider in settings
            .global_providers
            .iter()
            .filter(|provider| compatible(&provider.protocol))
        {
            let env_suffix = provider
                .id
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_uppercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            let api_key_env = format!("AGENTCABIN_DSH_API_KEY_{env_suffix}");
            let models = provider
                .models
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|model| serde_json::Value::Object(dsh_model_capability_fields(model)))
                .collect::<Vec<_>>();
            providers.insert(
                provider.id.clone(),
                serde_json::json!({
                    "displayName": provider.name,
                    "apiKeyEnv": api_key_env,
                    "api": provider.protocol,
                    "baseURL": provider.base_url,
                    "models": models,
                }),
            );
            provider_credentials.push((
                api_key_env,
                provider.api_key.clone().or_else(|| {
                    provider
                        .keyless
                        .unwrap_or(false)
                        .then(|| "agentcabin-keyless".to_string())
                }),
            ));
        }
        if providers.is_empty() {
            let env_suffix = route
                .provider
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_uppercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            providers.insert(
                route.provider.clone(),
                serde_json::json!({
                    "displayName": route.provider,
                    "apiKeyEnv": format!("AGENTCABIN_DSH_API_KEY_{env_suffix}"),
                    "api": route.protocol,
                    "baseURL": route.base_url,
                    "models": [serde_json::Value::Object(model_config)],
                }),
            );
        }
        let provider_config = serde_json::json!({"providers": providers});
        let active_plugins = crate::agent::dsh_plugins::get_active_dsh_plugins_for_mode(
            plugin_data_dir,
            caps.app_mode,
        );
        let is_work = caps.app_mode == crate::work::models::AppMode::Work;

        let display_plugin_path = caps
            .managed_runtime_dir
            .join("agentcabin_dsh_display_plugin.mjs");
        write_managed_file(
            &display_plugin_path,
            include_str!("../../work/dsh_display_plugin.mjs"),
            "AgentCabin DSH conversation display plugin",
        )?;

        // Both Code and Work use the official Web Harness. The native actor
        // talks to the bridge below using ACP-shaped stdio frames.
        let profile = "web";
        let mut patch =
            format!("- insert:\n    # {profile} overlay for AgentCabin's managed runtime.\n");

        let display_plugin_yaml =
            serde_yaml::to_string(display_plugin_path.to_string_lossy().as_ref())
                .map_err(|error| format!("Failed to serialize DSH display plugin path: {error}"))?;
        patch.push_str(&format!(
            "    - id: agentcabin-conversation-display\n      name: {}\n",
            display_plugin_yaml.trim_end()
        ));

        // The Web profile already owns the official skill and web stacks.
        // Never insert those plugin ids again: Cordis rejects duplicate ids
        // before the Harness Controller can create a session. Their enabled
        // state and AgentCabin-specific configuration are applied as root
        // overrides below.

        for plugin in active_plugins.iter().filter(|p| {
            p.id != "dsh-skill"
                && p.id != "dsh-web"
                && p.id != "dsh-mcp-client"
                && (!is_work || p.is_safe_in_work())
        }) {
            let reference = plugin.reference().ok_or_else(|| {
                format!(
                    "DSH plugin '{}' has no package or path reference",
                    plugin.id
                )
            })?;
            let id_yaml = serde_yaml::to_string(&plugin.id)
                .map_err(|error| format!("Failed to serialize DSH plugin id: {error}"))?;
            let reference_yaml = serde_yaml::to_string(reference)
                .map_err(|error| format!("Failed to serialize DSH plugin reference: {error}"))?;
            patch.push_str(&format!(
                "    - id: {}\n      name: {}",
                id_yaml.trim_end(),
                reference_yaml.trim_end()
            ));
            patch.push('\n');
            if let Some(cfg) = &plugin.config {
                if let Ok(yaml) = serde_yaml::to_string(cfg) {
                    patch.push_str("      config:\n");
                    for line in yaml.lines() {
                        patch.push_str("        ");
                        patch.push_str(line);
                        patch.push('\n');
                    }
                }
            }
        }

        // dsh-mcp-client deliberately has no implicit config-file discovery;
        // project each resolved AgentCabin server as an explicit plugin row when enabled.
        if is_work || active_plugins.iter().any(|p| p.id == "dsh-mcp-client") {
            let mut mcp_servers = caps.mcp_servers.clone();
            if is_work {
                mcp_servers.push(work_mcp_server(caps, extra_env)?);
            } else {
                if let Some(server) = super::code_desktop_mcp_server(caps)? {
                    mcp_servers.push(server);
                }
                if let Some(server) = capability_mcp_server(caps)? {
                    mcp_servers.push(add_capability_bridge_env(server, extra_env));
                }
            }
            for (index, server) in mcp_servers.iter().enumerate() {
                let server_name = server.id.replace(
                    |c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-',
                    "_",
                );
                let mut cfg = serde_json::Map::new();
                cfg.insert(
                    "serverName".into(),
                    serde_json::Value::String(server_name.clone()),
                );
                let transport = match server.transport.as_str() {
                    "http" => "streamable-http",
                    other => other,
                };
                cfg.insert(
                    "transport".into(),
                    serde_json::Value::String(transport.to_string()),
                );
                if transport == "stdio" {
                    if let Some(command) = &server.command {
                        cfg.insert("command".into(), serde_json::Value::String(command.clone()));
                    }
                    cfg.insert(
                        "args".into(),
                        serde_json::Value::Array(
                            server
                                .args
                                .iter()
                                .map(|a| serde_json::Value::String(a.clone()))
                                .collect(),
                        ),
                    );
                    if let Some(cwd) = &server.cwd {
                        cfg.insert(
                            "cwd".into(),
                            serde_json::Value::String(cwd.to_string_lossy().into_owned()),
                        );
                    }
                    if !server.env.is_empty() {
                        cfg.insert(
                            "env".into(),
                            serde_json::to_value(&server.env)
                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                        );
                    }
                } else {
                    if let Some(url) = &server.url {
                        cfg.insert("url".into(), serde_json::Value::String(url.clone()));
                    }
                    if !server.headers.is_empty() {
                        cfg.insert(
                            "headers".into(),
                            serde_json::to_value(&server.headers)
                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                        );
                    }
                }
                let yaml = serde_yaml::to_string(&serde_json::Value::Object(cfg))
                    .map_err(|e| format!("Failed to serialize DSH MCP server: {e}"))?;
                patch.push_str(&format!("    - id: mcp-server-{index}\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n"));
                for line in yaml.lines() {
                    patch.push_str("        ");
                    patch.push_str(line);
                    patch.push('\n');
                }
            }
        }

        let skill_enabled = active_plugins.iter().any(|plugin| plugin.id == "dsh-skill");
        let profile_overrides = String::new();
        // Do not disable the Web profile's host-plane `skill` or `web`
        // services here. The official per-session presets mount tool-skill and
        // tool-web against those services; starving either dependency makes
        // session/create fail before the first prompt. AgentCabin capability
        // policy is enforced by its authenticated bridge and DSH's own
        // provider configuration, not by removing required host services.
        let provider_yaml = serde_yaml::to_string(&provider_config)
            .map_err(|error| format!("Failed to serialize DSH provider route: {error}"))?;
        // Close the insert block before applying overrides to the
        // Profile rows above.
        patch.push_str(&profile_overrides);

        // The Web profile already owns the official LLM plugin. Override it
        // in place so every catalog provider is visible to Session Controller
        // and no duplicate provider service is registered.
        patch.push_str("- id: llm-pi-ai\n  config:\n");
        for line in provider_yaml.lines() {
            patch.push_str("    ");
            patch.push_str(line);
            patch.push('\n');
        }
        patch.push_str(&format!(
            "- id: agent-default-model\n  config:\n    provider: {}\n    model: {}\n",
            serde_yaml::to_string(&route.provider)
                .map_err(|error| format!("Failed to serialize DSH provider id: {error}"))?
                .trim(),
            serde_yaml::to_string(&route.model.id)
                .map_err(|error| format!("Failed to serialize DSH model id: {error}"))?
                .trim(),
        ));

        if skill_enabled {
            patch.push_str(
                "- id: skill-filesystem\n  config:\n    includeDefaultRoots: false\n    customSkillDirs:\n      - !!js dshHomePath('skills')\n",
            );
        }
        write_managed_file(&patch_path, patch, "DSH provider patch")?;

        let credential = route
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| route.keyless.then(|| "agentcabin-keyless".to_string()))
            .ok_or_else(|| format!("DSH provider '{}' has no API credential", route.provider))?;
        spawn.env.insert(DSH_ROUTE_ENV.to_string(), credential);
        let route_env_suffix = route
            .provider
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>();
        if let Some(route_credential) = spawn.env.get(DSH_ROUTE_ENV).cloned() {
            spawn.env.insert(
                format!("AGENTCABIN_DSH_API_KEY_{route_env_suffix}"),
                route_credential,
            );
        }
        for (key, value) in provider_credentials {
            if let Some(value) = value {
                spawn.env.insert(key, value);
            }
        }
        for (key, value) in extra_env {
            spawn.env.insert(key.clone(), value.clone());
        }
        // Both DSH profiles read this documented system-prompt variable.
        let mut persona = spawn
            .env
            .get("DSH_SYSTEM_PROMPT")
            .cloned()
            .or_else(|| std::env::var("DSH_SYSTEM_PROMPT").ok())
            .unwrap_or_else(|| "You are a helpful software engineer assistant.".into());
        if let Some(work_prompt) = spawn.env.get("AGENTCABIN_WORK_SYSTEM_PROMPT") {
            persona.push_str("\n\n");
            persona.push_str(work_prompt);
        }
        if let Some(display_guidance) = spawn.env.get("AGENTCABIN_DISPLAY_GUIDANCE") {
            persona.push_str("\n\n");
            persona.push_str(display_guidance);
        }
        spawn.env.insert(
            "DSH_SYSTEM_PROMPT".into(),
            format!("{}\n\n{}", persona, caps.capability_guidance()),
        );
        let dsh_binary = spawn.binary.clone();
        let dsh_node_modules = Path::new(&dsh_binary)
            .parent()
            .and_then(Path::parent)
            .map(|path| path.join("node_modules"));
        let bridge_path = caps
            .managed_runtime_dir
            .join("agentcabin_dsh_harness_bridge.mjs");
        write_managed_file(
            &bridge_path,
            include_str!("../dsh_session_actor/harness_controller_bridge.mjs"),
            "AgentCabin DSH Harness Session Controller bridge",
        )?;
        let node = crate::agent::runtime_locator::resolve_node()?;
        spawn.env.insert("AGENTCABIN_DSH_BINARY".into(), dsh_binary);
        if let Some(dsh_node_modules) = dsh_node_modules {
            spawn.env.insert(
                "AGENTCABIN_DSH_NODE_MODULES".into(),
                dsh_node_modules.to_string_lossy().into_owned(),
            );
        }
        spawn.env.insert(
            "AGENTCABIN_DSH_PATCH".into(),
            patch_path.to_string_lossy().into_owned(),
        );
        spawn
            .env
            .insert("AGENTCABIN_DSH_PROVIDER".into(), route.provider.clone());
        spawn
            .env
            .insert("AGENTCABIN_DSH_MODEL".into(), route.model.id.clone());
        if let Some(effort) = crate::storage::runs::get_run(&caps.run_id)
            .and_then(|run| run.effort)
            .filter(|value| !value.trim().is_empty())
        {
            spawn.env.insert("AGENTCABIN_DSH_EFFORT".into(), effort);
        }
        if let Some(permission_mode) = crate::storage::runs::get_run(&caps.run_id)
            .and_then(|run| run.permission_mode)
            .filter(|value| !value.trim().is_empty())
        {
            spawn
                .env
                .insert("AGENTCABIN_DSH_PERMISSION_MODE".into(), permission_mode);
        }
        spawn.binary = node;
        spawn.args = vec![bridge_path.to_string_lossy().into_owned()];
        Ok(spawn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::capability_resolver::{
        EffectiveCapabilities, EffectiveConnector, EffectiveMcpServer, EffectiveSkill,
    };
    use crate::agent::dsh_plugins::{DshPluginManager, DshPluginRegistration, DshPluginSecurity};
    use crate::models::GlobalProviderModel;
    use crate::work::models::{AppMode, WorkBrowserConfig};
    use tempfile::TempDir;

    fn work_capabilities(root: &TempDir) -> EffectiveCapabilities {
        let runtime_dir = root.path().join("runtime/work/dsh/run-patch-test");
        EffectiveCapabilities {
            app_mode: AppMode::Work,
            runtime: RuntimeProviderKind::Dsh,
            run_id: "run-patch-test".into(),
            managed_home: runtime_dir.clone(),
            managed_runtime_dir: runtime_dir,
            enabled_skills: Vec::<EffectiveSkill>::new(),
            mcp_servers: Vec::<EffectiveMcpServer>::new(),
            connectors: Vec::<EffectiveConnector>::new(),
            browser_enabled: false,
            browser_use_enabled: false,
            browser_config: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            prohibited_discovery_paths: vec![],
            detected_prohibited_paths: vec![],
            diagnostics: vec![],
            strict_mode: true,
        }
    }

    #[test]
    fn work_patch_uses_mcp_bridge_and_filters_code_only_plugins() {
        let root = TempDir::new().unwrap();
        let plugin_data = root.path().join("plugin-data");
        let manager = DshPluginManager::new(&plugin_data);
        manager
            .register_plugin(DshPluginRegistration {
                id: "community-tool".into(),
                name: "@example/community-tool".into(),
                package: Some("@example/community-tool".into()),
                path: None,
                version: None,
                description: None,
                enabled: true,
                security: DshPluginSecurity::CodeOnly,
                config: None,
            })
            .unwrap();

        let caps = work_capabilities(&root);
        let route = DshRouteConfig {
            provider: "AgentCabin Test".into(),
            protocol: "openai-completions".into(),
            base_url: "http://127.0.0.1:8787/v1".into(),
            api_key: Some("secret-must-stay-in-env".into()),
            keyless: false,
            model: GlobalProviderModel {
                id: "test-model".into(),
                ..GlobalProviderModel::default()
            },
        };
        let spawn = DshRuntimeAdapter
            .prepare_runtime_for_route_with_env_and_plugin_data_dir(
                &caps,
                &route,
                &HashMap::new(),
                &plugin_data,
            )
            .unwrap();
        let patch = std::fs::read_to_string(
            caps.managed_runtime_dir
                .join("agentcabin-provider.patch.yml"),
        )
        .unwrap();

        assert!(patch.contains("name: '@deepseek-ai/dsh-mcp-client'"));
        assert!(patch.contains("agentcabin_work"));
        assert!(!patch.contains("agentcabin-work-plugin"));
        assert!(!patch.contains("id: acp"));
        assert!(patch.contains("id: llm-pi-ai"));
        assert!(patch.contains("reasoningEfforts:"));
        assert!(!patch.contains("agentcabin-work-mcp"));
        assert!(!patch.contains("community-tool"));
        assert!(!patch.contains("secret-must-stay-in-env"));
        assert_eq!(
            spawn.env.get(DSH_ROUTE_ENV),
            Some(&"secret-must-stay-in-env".to_string())
        );
        for dependency in [
            "dsh_work_mcp_adapter.mjs",
            "computer_use_v2_runtime.mjs",
            "computer_use_v3_models.mjs",
            "desktop_computer_use_backend.mjs",
            "cdp_computer_use_backend.mjs",
            "visual_grounding_backend.mjs",
        ] {
            assert!(
                caps.managed_runtime_dir.join(dependency).is_file(),
                "missing DSH Computer Use dependency: {dependency}"
            );
        }
    }

    #[test]
    fn code_web_access_uses_agentcabin_provider_instead_of_deepseek_plugin() {
        let root = TempDir::new().unwrap();
        let plugin_data = root.path().join("plugin-data");
        let mut caps = work_capabilities(&root);
        caps.app_mode = AppMode::Code;
        caps.browser_enabled = true;
        caps.browser_config = Some(WorkBrowserConfig {
            enabled: true,
            provider: "duckduckgo".into(),
            max_results: 5,
            endpoint_url: None,
            allowed_hosts: Vec::new(),
        });
        let route = DshRouteConfig {
            provider: "AgentCabin Test".into(),
            protocol: "openai-completions".into(),
            base_url: "http://127.0.0.1:8787/v1".into(),
            api_key: Some("test-route-key".into()),
            keyless: false,
            model: GlobalProviderModel {
                id: "test-model".into(),
                ..GlobalProviderModel::default()
            },
        };

        let extra_env = HashMap::from([
            ("AGENTCABIN_WORK_BRIDGE_PORT".into(), "49321".into()),
            ("AGENTCABIN_WORK_BRIDGE_TOKEN".into(), "bridge-token".into()),
        ]);
        DshRuntimeAdapter
            .prepare_runtime_for_route_with_env_and_plugin_data_dir(
                &caps,
                &route,
                &extra_env,
                &plugin_data,
            )
            .unwrap();

        let patch = std::fs::read_to_string(
            caps.managed_runtime_dir
                .join("agentcabin-provider.patch.yml"),
        )
        .unwrap();
        let mcp = std::fs::read_to_string(caps.managed_runtime_dir.join("mcp.json")).unwrap();
        assert!(!patch.contains("id: web-search-deepseek\n  disabled: true"));
        let insert_block = patch.split("- id: llm-pi-ai").next().unwrap_or(&patch);
        assert!(!insert_block.contains("    - id: skill\n"));
        assert!(!insert_block.contains("    - id: web\n"));
        assert!(patch.contains("AGENTCABIN_WORK_BRIDGE_TOKEN"));
        assert!(patch.contains("bridge-token"));
        assert!(mcp.contains("agentcabin_capabilities"));
        assert!(mcp.contains("agentcabin_capabilities_mcp.mjs"));
        assert!(mcp.contains("AGENTCABIN_WORK_BRIDGE_PORT"));
        assert!(mcp.contains("bridge-token"));
    }
}
