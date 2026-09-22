use super::{
    add_code_desktop_use_env, cleanup_managed_directory, ensure_real_directory,
    mcp_servers_toml_table, project_skills, write_managed_file, RuntimeProviderAdapter,
    RuntimeSpawnConfig,
};
use crate::agent::capability_resolver::{EffectiveCapabilities, RuntimeProviderKind};
use crate::agent::claude_stream;
use crate::agent::grok_session_actor::process::resolve_grok_path;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct GrokRuntimeAdapter;

impl RuntimeProviderAdapter for GrokRuntimeAdapter {
    fn kind(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Grok
    }

    fn prepare_runtime(&self, caps: &EffectiveCapabilities) -> Result<RuntimeSpawnConfig, String> {
        let grok_home = &caps.managed_runtime_dir;
        let runtime_home = &caps.managed_home;

        ensure_real_directory(grok_home, "Grok home")?;
        ensure_real_directory(runtime_home, "Grok runtime home")?;

        // 1. Project enabled skills into grok_home/skills
        let projected_skills_dir = grok_home.join("skills");
        project_skills(&projected_skills_dir, &caps.enabled_skills)?;

        // 2. Generate isolated .claude/settings.json in runtime_home to prevent defaultMode override
        let claude_dir = runtime_home.join(".claude");
        ensure_real_directory(&claude_dir, "Grok runtime Claude dir")?;
        write_managed_file(
            &claude_dir.join("settings.json"),
            "{\n  \"permissions\": {\n    \"defaultMode\": \"default\"\n  }\n}\n",
            "Grok runtime Claude settings",
        )?;

        // 3. Generate isolated GROK_HOME/config.toml with MCP servers
        let config_toml_path = grok_home.join("config.toml");
        let mut config = toml::value::Table::new();
        let mut mcp_servers = caps.mcp_servers.clone();
        if let Some(server) = super::code_desktop_mcp_server(caps)? {
            mcp_servers.push(server);
        }
        if !mcp_servers.is_empty() {
            config.insert(
                "mcp_servers".into(),
                toml::Value::Table(mcp_servers_toml_table(&mcp_servers, "headers")),
            );
        }
        let config_contents = toml::to_string(&toml::Value::Table(config))
            .map_err(|e| format!("Failed to serialize Grok config.toml: {e}"))?;
        write_managed_file(&config_toml_path, config_contents, "Grok config.toml")?;

        // Note: No copying or symlinking of ~/.claude or ~/.grok files!
        // All configuration is isolated to managed_runtime_dir.

        let mut env = HashMap::new();
        env.insert("PATH".to_string(), claude_stream::augmented_path());
        env.insert(
            "GROK_HOME".to_string(),
            grok_home.to_string_lossy().to_string(),
        );
        env.insert(
            "HOME".to_string(),
            runtime_home.to_string_lossy().to_string(),
        );
        add_code_desktop_use_env(&mut env, caps.app_mode);

        let binary = resolve_grok_path();
        let args = vec![
            "--no-auto-update".to_string(),
            "--permission-mode".to_string(),
            "default".to_string(),
            "agent".to_string(),
            "stdio".to_string(),
        ];

        Ok(RuntimeSpawnConfig {
            binary,
            args,
            env,
            cwd: PathBuf::new(),
            managed_home: runtime_home.clone(),
        })
    }

    fn cleanup_runtime(&self, caps: &EffectiveCapabilities) -> Result<(), String> {
        cleanup_managed_directory(&caps.managed_runtime_dir, "Grok runtime")
    }
}
