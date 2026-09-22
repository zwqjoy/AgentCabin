use super::{
    add_code_desktop_use_env, cleanup_managed_directory, ensure_real_directory, project_skills,
    write_managed_file, RuntimeProviderAdapter, RuntimeSpawnConfig,
};
use crate::agent::capability_resolver::{EffectiveCapabilities, RuntimeProviderKind};
use crate::agent::claude_stream;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct ClaudeRuntimeAdapter;

impl RuntimeProviderAdapter for ClaudeRuntimeAdapter {
    fn kind(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Claude
    }

    fn prepare_runtime(&self, caps: &EffectiveCapabilities) -> Result<RuntimeSpawnConfig, String> {
        let managed_home = &caps.managed_home;
        let claude_dir = managed_home.join(".claude");
        ensure_real_directory(&claude_dir, "Claude managed dir")?;

        // 1. Project enabled skills into managed_home/.claude/skills
        let projected_skills_dir = claude_dir.join("skills");
        project_skills(&projected_skills_dir, &caps.enabled_skills)?;

        // 2. Generate managed settings.json prohibiting unmanaged overrides
        let settings_path = claude_dir.join("settings.json");
        let settings_json = serde_json::json!({
            "permissions": {
                "defaultMode": "default"
            }
        });
        let settings_contents = serde_json::to_string_pretty(&settings_json)
            .map_err(|e| format!("Failed to serialize Claude managed settings: {e}"))?;
        write_managed_file(&settings_path, settings_contents, "Claude managed settings")?;

        // 2. Generate managed MCP config
        let mcp_config_path = managed_home.join("mcp.json");
        let mut mcp_servers = caps.mcp_servers.clone();
        if let Some(server) = super::code_desktop_mcp_server(caps)? {
            mcp_servers.push(server);
        }
        let mut servers_map = serde_json::Map::new();
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
                    "type".into(),
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
            .map_err(|e| format!("Failed to serialize Claude MCP config: {e}"))?;
        write_managed_file(&mcp_config_path, mcp_contents, "Claude MCP config")?;

        // 3. Build args and env
        let args = vec![
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--permission-prompt-tool".to_string(),
            "stdio".to_string(),
            "--mcp-config".to_string(),
            mcp_config_path.to_string_lossy().to_string(),
            "--strict-mcp-config".to_string(),
        ];

        let mut env = HashMap::new();
        env.insert("PATH".to_string(), claude_stream::augmented_path());
        env.insert(
            "HOME".to_string(),
            managed_home.to_string_lossy().to_string(),
        );
        env.insert(
            "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING".to_string(),
            "1".to_string(),
        );
        add_code_desktop_use_env(&mut env, caps.app_mode);

        let binary = claude_stream::resolve_claude_path();

        Ok(RuntimeSpawnConfig {
            binary,
            args,
            env,
            cwd: PathBuf::new(),
            managed_home: managed_home.clone(),
        })
    }

    fn cleanup_runtime(&self, caps: &EffectiveCapabilities) -> Result<(), String> {
        cleanup_managed_directory(&caps.managed_runtime_dir, "Claude runtime")
    }
}
