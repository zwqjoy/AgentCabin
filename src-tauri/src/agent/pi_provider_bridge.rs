//! AgentCabin's explicit Pi Provider Bridge.
//!
//! Pi's provider registry is normally populated from `models.json`.  That file is
//! part of Pi's agent directory, so writing it into a second directory would make
//! AgentCabin-managed sessions lose the user's native Pi extensions, skills,
//! prompts, trust decisions, and sessions.  Instead, generate a small extension
//! outside Pi's profile and load it explicitly for managed-provider processes.

use crate::models::{GlobalProviderCredential, GlobalProviderModel, PiProviderCredential};
use crate::storage;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) const API_KEY_ENV: &str = "AGENTCABIN_PI_API_KEY";
const BASE_URL_ENV: &str = "AGENTCABIN_PI_BASE_URL";
const API_ENV: &str = "AGENTCABIN_PI_API";
const MODELS_ENV: &str = "AGENTCABIN_PI_MODELS_JSON";
const PROVIDER_NAME_ENV: &str = "AGENTCABIN_PI_PROVIDER_NAME";
const BRIDGE_FILE_NAME: &str = "pi-provider-bridge.mjs";

/// The bridge is deliberately stored outside `~/.pi/agent`. It is an
/// AgentCabin runtime asset, not a user Pi extension.
pub(crate) fn bridge_path() -> PathBuf {
    storage::data_dir().join(BRIDGE_FILE_NAME)
}

const BRIDGE_SOURCE: &str = r#"const providersJson = process.env.AGENTCABIN_PI_PROVIDERS_JSON;
let providers = [];
if (providersJson) {
  try {
    providers = JSON.parse(providersJson);
  } catch (error) {
    console.error(`AgentCabin Provider Bridge received invalid providers metadata: ${error}`);
  }
}

const fallbackBaseUrl = process.env.AGENTCABIN_PI_BASE_URL;
const fallbackApi = process.env.AGENTCABIN_PI_API;
let fallbackModels = [];
try {
  fallbackModels = JSON.parse(process.env.AGENTCABIN_PI_MODELS_JSON || "[]");
} catch {}

export default function (pi) {
  if (Array.isArray(providers)) {
    for (const p of providers) {
      if (!p.id || !p.baseUrl || !p.api || !p.models?.length) continue;
      const apiKey = p.apiKeyEnv && process.env[p.apiKeyEnv]
        ? `$${p.apiKeyEnv}`
        : (p.apiKey || "PROXY_MANAGED");
      pi.registerProvider(p.id, {
        name: p.name || p.id,
        baseUrl: p.baseUrl,
        api: p.api,
        apiKey: apiKey,
        models: p.models,
      });
    }
  }

  if (fallbackBaseUrl && fallbackApi && fallbackModels.length) {
    const fallbackProviderName = process.env.AGENTCABIN_PI_PROVIDER_NAME || "AgentCabin";
    const fallbackApiKey = process.env.AGENTCABIN_PI_API_KEY
      ? "$AGENTCABIN_PI_API_KEY"
      : "PROXY_MANAGED";
    pi.registerProvider("agentcabin", {
      name: fallbackProviderName,
      baseUrl: fallbackBaseUrl,
      api: fallbackApi,
      apiKey: fallbackApiKey,
      models: fallbackModels,
    });
  }
}
"#;

fn known_subscription_context_window(provider_id: &str, model_id: &str) -> Option<u64> {
    crate::agent::codex_subscription_bridge::is_codex_subscription_id(provider_id)
        .then(|| crate::agent::codex_control::known_effective_context_window(model_id))
        .flatten()
}

fn resolve_model_thinking_level_map(model: &GlobalProviderModel) -> Option<Value> {
    if !model.supports_reasoning.unwrap_or(true) {
        return None;
    }
    if let Some(levels) = &model.supported_effort_levels {
        let mut map = serde_json::Map::new();
        for level in &["off", "minimal", "low", "medium", "high"] {
            if levels.iter().any(|l| l == level) {
                map.insert((*level).to_string(), Value::String((*level).to_string()));
            } else {
                map.insert((*level).to_string(), Value::Null);
            }
        }
        let has_max = levels.iter().any(|l| l == "max");
        let has_xhigh = levels.iter().any(|l| l == "xhigh");
        if has_max {
            map.insert("max".to_string(), Value::String("max".to_string()));
            if !has_xhigh {
                map.insert("xhigh".to_string(), Value::String("max".to_string()));
            }
        }
        if has_xhigh {
            map.insert("xhigh".to_string(), Value::String("xhigh".to_string()));
        } else if !has_max {
            map.insert("xhigh".to_string(), Value::Null);
        }
        return Some(Value::Object(map));
    }
    if model.supports_xhigh == Some(true) {
        return Some(json!({"xhigh": "xhigh"}));
    }
    None
}

pub(crate) fn provider_models(provider: &PiProviderCredential) -> Vec<Value> {
    let mut configured = provider.models.clone();
    if configured.is_empty() && !provider.model.trim().is_empty() {
        configured.push(GlobalProviderModel {
            id: provider.model.clone(),
            name: None,
            context_window: provider.context_window,
            max_tokens: None,
            supports_reasoning: None,
            supports_xhigh: None,
            supported_effort_levels: None,
            supports_images: None,
        });
    }

    configured
        .into_iter()
        .map(|model| {
            let id = model.id.clone();
            let known_subscription_context = known_subscription_context_window(&provider.id, &id);
            let input = if model.supports_images.unwrap_or(false) {
                json!(["text", "image"])
            } else {
                json!(["text"])
            };
            let mut value = json!({
                "id": id,
                "name": model.name.clone().unwrap_or_else(|| model.id.clone()),
                "reasoning": model.supports_reasoning.unwrap_or(true),
                "input": input,
                "cost": {
                    "input": 0,
                    "output": 0,
                    "cacheRead": 0,
                    "cacheWrite": 0
                },
                "contextWindow": model.context_window
                    .or_else(|| (model.id == provider.model).then_some(provider.context_window).flatten())
                    .or(known_subscription_context)
                    .unwrap_or(128000),
                "maxTokens": model.max_tokens.unwrap_or(16384)
            });
            // Keep the selected provider model's declared context window authoritative.
            if let Some(context_window) = model.context_window {
                value["contextWindow"] = json!(context_window);
            }
            if let Some(thinking_level_map) = resolve_model_thinking_level_map(&model) {
                value["thinkingLevelMap"] = thinking_level_map;
            }
            value
        })
        .collect()
}

fn build_provider_env(provider: &PiProviderCredential) -> Result<HashMap<String, String>, String> {
    if provider.base_url.trim().is_empty() {
        return Err("Pi managed provider requires a Base URL".into());
    }
    if provider.model.trim().is_empty() {
        return Err("Pi managed provider requires a model".into());
    }
    if !matches!(
        provider.api.as_str(),
        "openai-completions" | "openai-responses" | "anthropic-messages"
    ) {
        return Err(format!("Unsupported Pi provider API: {}", provider.api));
    }

    let models = serde_json::to_string(&provider_models(provider))
        .map_err(|error| format!("Failed to encode Pi provider models: {}", error))?;
    let mut env = HashMap::from([
        (BASE_URL_ENV.to_string(), provider.base_url.clone()),
        (API_ENV.to_string(), provider.api.clone()),
        (MODELS_ENV.to_string(), models),
        (PROVIDER_NAME_ENV.to_string(), provider.name.clone()),
    ]);
    if let Some(api_key) = provider
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        env.insert(API_KEY_ENV.to_string(), api_key.to_string());
    }
    Ok(env)
}

pub(crate) fn global_provider_models(provider: &GlobalProviderCredential) -> Vec<Value> {
    let configured = provider.models.clone().unwrap_or_default();
    configured
        .into_iter()
        .map(|model| {
            let id = model.id.clone();
            let known_subscription_context = known_subscription_context_window(&provider.id, &id);
            let input = if model.supports_images.unwrap_or(false) {
                json!(["text", "image"])
            } else {
                json!(["text"])
            };
            let mut value = json!({
                "id": id,
                "name": model.name.clone().unwrap_or_else(|| model.id.clone()),
                "reasoning": model.supports_reasoning.unwrap_or(true),
                "input": input,
                "cost": {
                    "input": 0,
                    "output": 0,
                    "cacheRead": 0,
                    "cacheWrite": 0
                },
                "contextWindow": model
                    .context_window
                    .or(known_subscription_context)
                    .unwrap_or(128000),
                "maxTokens": model.max_tokens.unwrap_or(16384)
            });
            if let Some(thinking_level_map) = resolve_model_thinking_level_map(&model) {
                value["thinkingLevelMap"] = thinking_level_map;
            }
            value
        })
        .collect()
}

pub(crate) fn prepare_global_providers_env(
    providers: &[GlobalProviderCredential],
    fallback_provider: Option<&PiProviderCredential>,
) -> Result<HashMap<String, String>, String> {
    let bridge_path = bridge_path();
    if let Some(parent) = bridge_path.parent() {
        storage::ensure_dir(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&bridge_path, BRIDGE_SOURCE)
        .map_err(|error| format!("Failed to write Pi Provider Bridge: {}", error))?;

    let mut env = HashMap::new();

    let mut providers_payload = Vec::new();
    for p in providers {
        if p.base_url.trim().is_empty() {
            continue;
        }
        let models_val = global_provider_models(p);
        if models_val.is_empty() {
            continue;
        }
        let env_key_name = format!(
            "AGENTCABIN_PI_KEY_{}",
            p.id.replace(|c: char| !c.is_alphanumeric(), "_")
                .to_ascii_uppercase()
        );
        let has_key = if let Some(api_key) = p.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            env.insert(env_key_name.clone(), api_key.to_string());
            true
        } else {
            false
        };

        providers_payload.push(json!({
            "id": p.id,
            "name": p.name,
            "baseUrl": p.base_url,
            "api": p.protocol,
            "apiKeyEnv": if has_key { Some(env_key_name) } else { None },
            "apiKey": if !has_key { Some("PROXY_MANAGED") } else { None },
            "models": models_val,
        }));
    }

    if !providers_payload.is_empty() {
        let payload_json = serde_json::to_string(&providers_payload)
            .map_err(|e| format!("Failed to encode global providers JSON: {}", e))?;
        env.insert("AGENTCABIN_PI_PROVIDERS_JSON".to_string(), payload_json);
    }

    if let Some(fallback) = fallback_provider {
        let fallback_env = build_provider_env(fallback)?;
        env.extend(fallback_env);
    }

    Ok(env)
}

#[allow(dead_code)]
pub(crate) fn prepare_provider_env(
    provider: &PiProviderCredential,
) -> Result<HashMap<String, String>, String> {
    prepare_global_providers_env(&[], Some(provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> PiProviderCredential {
        PiProviderCredential {
            id: "custom-openai".into(),
            name: "Custom OpenAI-compatible".into(),
            base_url: "https://example.test/v1".into(),
            api: "openai-completions".into(),
            model: "model-a".into(),
            models: vec![],
            context_window: Some(131072),
            api_key: Some("secret".into()),
        }
    }

    #[test]
    fn bridge_source_has_no_provider_secret() {
        assert!(!BRIDGE_SOURCE.contains("secret"));
        assert!(BRIDGE_SOURCE.contains("registerProvider(\"agentcabin\""));
        assert!(BRIDGE_SOURCE.contains("\"$AGENTCABIN_PI_API_KEY\""));
    }

    #[test]
    fn provider_models_keep_selected_model_metadata() {
        let models = provider_models(&provider());
        assert_eq!(models[0]["id"], "model-a");
        assert_eq!(models[0]["contextWindow"], 131072);
        assert_eq!(models[0]["input"], json!(["text"]));
    }

    #[test]
    fn subscription_provider_models_use_effective_codex_context_when_missing() {
        let mut subscription = provider();
        subscription.id =
            crate::agent::codex_subscription_bridge::CODEX_SUBSCRIPTION_PROVIDER_ID.into();
        subscription.model = "gpt-5.6-sol".into();
        subscription.context_window = None;

        let models = provider_models(&subscription);

        assert_eq!(models[0]["contextWindow"], 258400);
    }

    #[test]
    fn subscription_global_models_use_effective_codex_context_when_missing() {
        let subscription = GlobalProviderCredential {
            id: crate::agent::codex_subscription_bridge::CODEX_SUBSCRIPTION_PROVIDER_ID.into(),
            name: "OpenAI / ChatGPT subscription".into(),
            protocol: "openai-responses".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            api_key: None,
            auth_env_var: None,
            env_key: None,
            models: Some(vec![GlobalProviderModel {
                id: "gpt-5.6-luna".into(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: Some(true),
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: Some("gpt-5.6-luna".into()),
            supports_developer_role: Some(true),
            supports_reasoning_effort: Some(true),
            extra_env: None,
            keyless: Some(true),
        };

        let models = global_provider_models(&subscription);

        assert_eq!(models[0]["contextWindow"], 258400);
    }

    #[test]
    fn provider_models_expose_xhigh_style_effort_levels_per_model() {
        let mut configured = provider();
        configured.models = vec![GlobalProviderModel {
            id: "model-a".into(),
            name: None,
            context_window: Some(131072),
            max_tokens: None,
            supports_reasoning: Some(true),
            supports_xhigh: Some(true),
            supported_effort_levels: None,
            supports_images: None,
        }];

        let models = provider_models(&configured);

        assert_eq!(models[0]["thinkingLevelMap"], json!({"xhigh": "xhigh"}));
    }

    #[test]
    fn provider_models_hide_high_only_when_explicitly_unsupported() {
        let mut configured = provider();
        configured.models = vec![GlobalProviderModel {
            id: "model-a".into(),
            name: None,
            context_window: Some(131072),
            max_tokens: None,
            supports_reasoning: Some(true),
            supports_xhigh: Some(true),
            supported_effort_levels: Some(vec!["low".into(), "medium".into(), "xhigh".into()]),
            supports_images: None,
        }];

        let models = provider_models(&configured);

        assert_eq!(
            models[0]["thinkingLevelMap"],
            json!({
                "off": null,
                "minimal": null,
                "low": "low",
                "medium": "medium",
                "high": null,
                "xhigh": "xhigh"
            })
        );
    }

    #[test]
    fn provider_env_contains_metadata_but_not_secret_in_model_json() {
        let env = build_provider_env(&provider()).unwrap();
        assert!(!env.contains_key("PI_CODING_AGENT_DIR"));
        assert_eq!(
            env.get(BASE_URL_ENV).map(String::as_str),
            Some("https://example.test/v1")
        );
        assert_eq!(
            env.get(API_ENV).map(String::as_str),
            Some("openai-completions")
        );
        assert_eq!(env.get(API_KEY_ENV).map(String::as_str), Some("secret"));
        assert!(!env.get(MODELS_ENV).unwrap().contains("secret"));
    }
}
