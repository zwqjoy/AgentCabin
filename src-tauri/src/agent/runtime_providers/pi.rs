use super::{
    cleanup_managed_directory, ensure_real_directory, project_skills, write_managed_file,
    RuntimeProviderAdapter, RuntimeSpawnConfig,
};
use crate::agent::capability_resolver::{EffectiveCapabilities, RuntimeProviderKind};
use crate::agent::claude_stream;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct PiRuntimeAdapter;

impl RuntimeProviderAdapter for PiRuntimeAdapter {
    fn kind(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Pi
    }

    fn prepare_runtime(&self, caps: &EffectiveCapabilities) -> Result<RuntimeSpawnConfig, String> {
        let data_dir = crate::storage::data_dir();
        if caps.app_mode == crate::work::models::AppMode::Code {
            crate::storage::profile_bindings::migrate_legacy_pi_code_profile_if_needed()?;
        }
        let pi_home = &caps.managed_runtime_dir;
        ensure_real_directory(pi_home, "Pi runtime dir")?;

        // Project enabled skills into pi_home/skills
        let projected_skills_dir = pi_home.join("skills");
        project_skills(&projected_skills_dir, &caps.enabled_skills)?;

        // Project MCP servers into pi_home/mcp.json
        let mcp_config_path = pi_home.join("mcp.json");
        let mut servers_map = serde_json::Map::new();
        for server in &caps.mcp_servers {
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
            .map_err(|e| format!("Failed to serialize Pi MCP config: {e}"))?;
        write_managed_file(&mcp_config_path, mcp_contents, "Pi MCP config")?;

        // Ensure per-run sessions directory exists so Pi CLI stores all session state inside
        // this per-run runtime sandbox without polluting profiles/work or profiles/code/pi.
        let sessions_dir = pi_home.join("sessions");
        ensure_real_directory(&sessions_dir, "Pi sessions dir")?;

        let mode_str = caps.app_mode.as_str();
        let profile_dir = if caps.app_mode == crate::work::models::AppMode::Code {
            crate::storage::profile_bindings::pi_code_profile_dir_with_root(&data_dir)
        } else {
            data_dir.join("profiles").join(mode_str)
        };

        // Prepare settings.json with bundled npmCommand
        let profile_settings = profile_dir.join("settings.json");
        let mut settings: serde_json::Value = if profile_settings.is_file() {
            std::fs::read_to_string(&profile_settings)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        if let (Ok(node), Ok(npm_cli)) = (
            crate::agent::runtime_locator::resolve_node(),
            crate::agent::runtime_locator::resolve_npm_cli(),
        ) {
            let cache_dir = profile_dir.join("npm-cache");
            let cmd = vec![
                serde_json::Value::String(node),
                serde_json::Value::String(npm_cli),
                serde_json::Value::String("--cache".into()),
                serde_json::Value::String(cache_dir.to_string_lossy().into_owned()),
            ];
            if let Some(obj) = settings.as_object_mut() {
                if !obj.contains_key("npmCommand") {
                    obj.insert("npmCommand".into(), serde_json::Value::Array(cmd));
                }
            }
        }
        if let Ok(content) = serde_json::to_string_pretty(&settings) {
            write_managed_file(&pi_home.join("settings.json"), content, "Pi settings")?;
        }

        // For Work mode, project agents templates
        if caps.app_mode == crate::work::models::AppMode::Work {
            let src_agents = profile_dir.join("agents");
            if src_agents.is_dir() {
                let target_agents = pi_home.join("agents");
                ensure_real_directory(&target_agents, "Pi agents dir")?;
                if let Ok(entries) = std::fs::read_dir(&src_agents) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_file() {
                            if let Some(file_name) = p.file_name() {
                                if let Ok(content) = std::fs::read(&p) {
                                    write_managed_file(
                                        &target_agents.join(file_name),
                                        content,
                                        "Pi agent template",
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut env = HashMap::new();
        env.insert("PATH".to_string(), claude_stream::augmented_path());
        // Keep Pi's user-level lookup inside this per-run projection. Inheriting
        // the host HOME would re-enable ~/.pi and other native discovery paths.
        env.insert("HOME".to_string(), pi_home.to_string_lossy().to_string());

        let binary = claude_stream::resolve_pi_path();
        let args = vec!["rpc".to_string()];

        Ok(RuntimeSpawnConfig {
            binary,
            args,
            env,
            cwd: PathBuf::new(),
            managed_home: pi_home.clone(),
        })
    }

    fn cleanup_runtime(&self, caps: &EffectiveCapabilities) -> Result<(), String> {
        cleanup_managed_directory(&caps.managed_runtime_dir, "Pi runtime")
    }
}
