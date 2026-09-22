//! AgentCabin-managed Grok Build configuration.
//!
//! Grok Build routes requests through its `[model.<id>]` config layer, not through
//! `XAI_BASE_URL` (which Grok does not recognize). When the user binds a global
//! provider to Grok, AgentCabin generates a dedicated config under a managed
//! `GROK_HOME` directory so the user's real `~/.grok/config.toml` stays untouched.
//!
//! The managed model entry uses the upstream model name (e.g. `glm-5.2`) as both
//! the Grok model id (`[model.<id>]`) and the `model` field. This keeps
//! `run.model` ↔ `session/new _meta.modelId` ↔ `[model.<id>]` consistent without
//! a translation layer — Grok reports the same id back, so `apply_model_update`
//! never overwrites the user's selection with an internal alias.
//!
//! Data flow:
//!
//! ```text
//! Grok Build process
//!   GROK_HOME → ~/.agentcabin/runtime/<app-mode>/grok/<run-id>/config.toml
//!     [model."glm-5.2"]
//!       model = "glm-5.2"
//!       base_url = "http://127.0.0.1:3000/v1"
//!       api_backend = "chat_completions"   (from provider.protocol)
//!       env_key = "XAI_API_KEY"            (AgentCabin injects the real key)
//!   → POST http://127.0.0.1:3000/v1/chat/completions
//! ```

use crate::agent::capability_resolver::EffectiveMcpServer;
use crate::models::GlobalProviderCredential;
use std::path::{Path, PathBuf};

const CONFIG_FILE_NAME: &str = "config.toml";

/// The env var the managed config reads the API key from. AgentCabin always injects
/// the bound provider's key under this name so Grok's auth probe (`XAI_API_KEY`
/// check in `after_initialize`) stays consistent and picks `xai.api_key` auth.
pub(crate) const DEFAULT_ENV_KEY: &str = "XAI_API_KEY";

/// Map AgentCabin provider protocol → Grok `api_backend`.
/// Grok's default (when `api_backend` is omitted) is `chat_completions`, so
/// `openai-completions` maps to the default path.
fn protocol_to_api_backend(protocol: &str) -> &'static str {
    match protocol {
        "openai-responses" => "responses",
        "anthropic-messages" => "messages",
        _ => "chat_completions",
    }
}

/// Normalize `base_url` for Grok's expectations:
/// - OpenAI protocols: ensure `/v1` suffix (Grok appends `/chat/completions` or `/responses`)
/// - Anthropic protocol: strip `/v1` (Grok appends `/v1/messages`)
fn normalize_base_url(base_url: &str, protocol: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    match protocol {
        "anthropic-messages" => trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string(),
        _ => {
            if trimmed.ends_with("/v1") {
                trimmed.to_string()
            } else {
                match reqwest::Url::parse(trimmed) {
                    Ok(url) if url.path().is_empty() || url.path() == "/" => {
                        format!("{trimmed}/v1")
                    }
                    _ => trimmed.to_string(),
                }
            }
        }
    }
}

/// Build the TOML content for the managed Grok config.
///
/// The upstream model name is used as the `[model.<id>]` key (auto-quoted by the
/// `toml` crate when it contains dots/slashes) so `run.model` and Grok's reported
/// `currentModelId` stay identical without a translation layer.
#[cfg(test)]
fn build_config_toml(provider: &GlobalProviderCredential, model: &str) -> String {
    build_config_toml_with_mcp(provider, model, &[])
}

/// Build the complete managed Grok config, including the per-run MCP projection.
fn build_config_toml_with_mcp(
    provider: &GlobalProviderCredential,
    model: &str,
    mcp_servers: &[EffectiveMcpServer],
) -> String {
    let api_backend = protocol_to_api_backend(&provider.protocol);
    let base_url = normalize_base_url(&provider.base_url, &provider.protocol);

    let mut root = toml::value::Table::new();

    let mut models_table = toml::value::Table::new();
    models_table.insert("default".into(), toml::Value::String(model.to_string()));
    root.insert("models".into(), toml::Value::Table(models_table));

    let mut model_entry = toml::value::Table::new();
    model_entry.insert("model".into(), toml::Value::String(model.to_string()));
    // `name` is what Grok reports back as `availableModels[].name`, which the UI
    // shows as the model display name. Keep it identical to the model id so the
    // chat header reflects the user's selection (e.g. "deepseek/deepseek-v4-flash"),
    // not the provider name. Mirrors how Pi/Claude surface the model id directly.
    model_entry.insert("name".into(), toml::Value::String(model.to_string()));
    model_entry.insert("base_url".into(), toml::Value::String(base_url));
    model_entry.insert(
        "env_key".into(),
        toml::Value::String(DEFAULT_ENV_KEY.to_string()),
    );
    model_entry.insert(
        "api_backend".into(),
        toml::Value::String(api_backend.to_string()),
    );

    // Grok reads reasoning support from the managed model entry. Preserve the
    // per-model Provider declaration when present, with the provider-level flag
    // as a backwards-compatible fallback for older settings.
    let configured_model = provider
        .models
        .as_ref()
        .and_then(|models| models.iter().find(|candidate| candidate.id == model));
    let supports_reasoning = configured_model
        .and_then(|candidate| candidate.supports_reasoning)
        .or(provider.supports_reasoning_effort)
        .unwrap_or(false);
    if supports_reasoning {
        model_entry.insert(
            "supports_reasoning_effort".into(),
            toml::Value::Boolean(true),
        );
        let effort_levels = configured_model
            .and_then(|candidate| candidate.supported_effort_levels.clone())
            .filter(|levels| !levels.is_empty())
            .unwrap_or_else(|| {
                ["low", "medium", "high", "xhigh"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            });
        model_entry.insert(
            "reasoning_efforts".into(),
            toml::Value::Array(effort_levels.into_iter().map(toml::Value::String).collect()),
        );
    }

    let mut model_table = toml::value::Table::new();
    model_table.insert(model.to_string(), toml::Value::Table(model_entry));
    root.insert("model".into(), toml::Value::Table(model_table));

    if !mcp_servers.is_empty() {
        root.insert(
            "mcp_servers".into(),
            toml::Value::Table(crate::agent::runtime_providers::mcp_servers_toml_table(
                mcp_servers,
                "headers",
            )),
        );
    }

    toml::to_string(&toml::Value::Table(root)).unwrap_or_default()
}

/// Write a complete managed config into the already-created run directory.
/// The caller sets `GROK_HOME=<home>` on the spawned process.
pub(crate) fn write_managed_config_at(
    home: &Path,
    provider: &GlobalProviderCredential,
    model: &str,
    mcp_servers: &[EffectiveMcpServer],
) -> Result<PathBuf, String> {
    crate::agent::runtime_providers::ensure_real_directory(home, "GROK_HOME")?;

    let content = build_config_toml_with_mcp(provider, model, mcp_servers);
    crate::agent::runtime_providers::write_managed_file(
        &home.join(CONFIG_FILE_NAME),
        content,
        "Grok config.toml",
    )?;

    Ok(home.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{GlobalProviderCredential, GlobalProviderModel};

    fn provider(protocol: &str, base_url: &str) -> GlobalProviderCredential {
        GlobalProviderCredential {
            id: "test".into(),
            name: "Test Provider".into(),
            protocol: protocol.into(),
            base_url: base_url.into(),
            api_key: Some("secret".into()),
            auth_env_var: None,
            env_key: None,
            models: None,
            test_model: None,
            supports_developer_role: None,
            supports_reasoning_effort: None,
            extra_env: None,
            keyless: None,
        }
    }

    #[test]
    fn openai_completions_maps_to_chat_completions() {
        assert_eq!(
            protocol_to_api_backend("openai-completions"),
            "chat_completions"
        );
    }

    #[test]
    fn openai_responses_maps_to_responses() {
        assert_eq!(protocol_to_api_backend("openai-responses"), "responses");
    }

    #[test]
    fn anthropic_messages_maps_to_messages() {
        assert_eq!(protocol_to_api_backend("anthropic-messages"), "messages");
    }

    #[test]
    fn normalize_adds_v1_for_openai_when_missing() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:3000", "openai-completions"),
            "http://127.0.0.1:3000/v1"
        );
    }

    #[test]
    fn normalize_keeps_v1_for_openai_when_present() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:3000/v1", "openai-completions"),
            "http://127.0.0.1:3000/v1"
        );
    }

    #[test]
    fn normalize_strips_v1_for_anthropic() {
        assert_eq!(
            normalize_base_url("https://api.anthropic.com/v1", "anthropic-messages"),
            "https://api.anthropic.com"
        );
    }

    #[test]
    fn config_toml_uses_model_name_as_id_and_routes_correctly() {
        let toml = build_config_toml(
            &provider("openai-completions", "http://127.0.0.1:3000/v1"),
            "glm-5.2",
        );
        // Model name with a dot must be quoted as a TOML key.
        assert!(toml.contains("default = \"glm-5.2\""));
        assert!(toml.contains("[model.\"glm-5.2\"]"));
        assert!(toml.contains("model = \"glm-5.2\""));
        assert!(toml.contains("base_url = \"http://127.0.0.1:3000/v1\""));
        assert!(toml.contains("api_backend = \"chat_completions\""));
        assert!(toml.contains("env_key = \"XAI_API_KEY\""));
    }

    #[test]
    fn config_toml_handles_slash_in_model_name() {
        let toml = build_config_toml(
            &provider("openai-completions", "http://127.0.0.1:3000/v1"),
            "deepseek/deepseek-v4-flash",
        );
        assert!(toml.contains("default = \"deepseek/deepseek-v4-flash\""));
        assert!(toml.contains("[model.\"deepseek/deepseek-v4-flash\"]"));
    }

    #[test]
    fn config_toml_preserves_provider_reasoning_capabilities() {
        let mut configured = provider("openai-completions", "http://127.0.0.1:3000/v1");
        configured.models = Some(vec![GlobalProviderModel {
            id: "glm-5.2".into(),
            name: Some("GLM 5.2".into()),
            context_window: None,
            max_tokens: None,
            supports_reasoning: Some(true),
            supports_xhigh: Some(true),
            supported_effort_levels: Some(vec!["low".into(), "medium".into(), "high".into()]),
            supports_images: Some(true),
        }]);

        let toml = build_config_toml(&configured, "glm-5.2");

        assert!(toml.contains("supports_reasoning_effort = true"));
        assert!(toml.contains("reasoning_efforts = [\"low\", \"medium\", \"high\"]"));
    }
}
