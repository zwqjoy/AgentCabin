use super::{
    add_code_desktop_use_env, cleanup_managed_directory, ensure_real_directory,
    mcp_servers_toml_table, project_skills, write_managed_file, RuntimeProviderAdapter,
    RuntimeSpawnConfig,
};
use crate::agent::capability_resolver::{EffectiveCapabilities, RuntimeProviderKind};
use crate::agent::claude_stream;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct CodexRuntimeAdapter;

impl RuntimeProviderAdapter for CodexRuntimeAdapter {
    fn kind(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Codex
    }

    fn prepare_runtime(&self, caps: &EffectiveCapabilities) -> Result<RuntimeSpawnConfig, String> {
        let codex_home = &caps.managed_runtime_dir;
        ensure_real_directory(codex_home, "Codex home")?;

        // 1. Project enabled skills into CODEX_HOME/skills/
        let projected_skills_dir = codex_home.join("skills");
        project_skills(&projected_skills_dir, &caps.enabled_skills)?;

        // 2. Generate central config.toml under CODEX_HOME
        let config_toml_path = codex_home.join("config.toml");
        let mut config = toml::value::Table::new();
        config.insert(
            "suppress_unstable_features_warning".into(),
            toml::Value::Boolean(true),
        );
        let mut mcp_servers = caps.mcp_servers.clone();
        if let Some(server) = super::code_desktop_mcp_server(caps)? {
            mcp_servers.push(server);
        }
        if !mcp_servers.is_empty() {
            config.insert(
                "mcp_servers".into(),
                toml::Value::Table(mcp_servers_toml_table(&mcp_servers, "http_headers")),
            );
        }
        let config_contents = toml::to_string(&toml::Value::Table(config))
            .map_err(|e| format!("Failed to serialize Codex config.toml: {e}"))?;
        write_managed_file(&config_toml_path, config_contents, "Codex config.toml")?;

        let mut env = HashMap::new();
        env.insert("PATH".to_string(), claude_stream::augmented_path());
        env.insert(
            "CODEX_HOME".to_string(),
            codex_home.to_string_lossy().to_string(),
        );
        env.insert("HOME".to_string(), codex_home.to_string_lossy().to_string());
        add_code_desktop_use_env(&mut env, caps.app_mode);

        let binary = claude_stream::which_binary("codex").unwrap_or_else(|| "codex".to_string());
        let args = vec![
            "app-server".to_string(),
            "--enable".to_string(),
            "default_mode_request_user_input".to_string(),
            "-c".to_string(),
            "suppress_unstable_features_warning=true".to_string(),
        ];

        Ok(RuntimeSpawnConfig {
            binary,
            args,
            env,
            cwd: PathBuf::new(),
            managed_home: codex_home.clone(),
        })
    }

    fn cleanup_runtime(&self, caps: &EffectiveCapabilities) -> Result<(), String> {
        cleanup_managed_directory(&caps.managed_runtime_dir, "Codex runtime")
    }
}
