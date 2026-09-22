use crate::models::{
    AgentProviderBinding, AgentProviderBindings, AgentSettings, AllSettings,
    CodexProviderCredential, GlobalProviderCredential, GlobalProviderModel, PiProviderCredential,
    PlatformCredential, UserSettings,
};
use once_cell::sync::Lazy;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

static SETTINGS_CACHE: Lazy<RwLock<Option<AllSettings>>> = Lazy::new(|| RwLock::new(None));

pub fn invalidate_cache() {
    if let Ok(mut lock) = SETTINGS_CACHE.write() {
        *lock = None;
    }
}

fn settings_path() -> PathBuf {
    super::data_dir().join("settings.json")
}

pub fn load() -> AllSettings {
    if let Ok(lock) = SETTINGS_CACHE.read() {
        if let Some(cached) = lock.as_ref() {
            return cached.clone();
        }
    }
    load_from_disk_and_cache()
}

fn load_from_disk_and_cache() -> AllSettings {
    let path = settings_path();
    let loaded = if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(mut settings) => {
                    log::debug!("[storage/settings] loaded settings from {}", path.display());
                    // Keep both the old platform fields and the canonical provider model in sync.
                    let platform_changed = migrate_platform_credentials(&mut settings);
                    let global_changed = migrate_global_provider_settings(&mut settings);
                    let mut agents_changed = false;
                    if settings.user.enabled_agents.is_none() {
                        settings.user.enabled_agents =
                            Some(vec!["pi".to_string(), "dsh".to_string()]);
                        agents_changed = true;
                    } else if let Some(ref mut agents) = settings.user.enabled_agents {
                        if !agents.iter().any(|a| a == "pi") {
                            agents.insert(0, "pi".to_string());
                            agents_changed = true;
                        }
                        if !agents.iter().any(|a| a == "dsh") {
                            agents.push("dsh".to_string());
                            agents_changed = true;
                        }
                    }
                    if platform_changed || global_changed || agents_changed {
                        log::info!("[storage/settings] migrated provider settings, saving");
                        let _ = save(&settings);
                    }
                    settings
                }
                Err(e) => {
                    log::warn!("[storage/settings] failed to parse settings: {}", e);
                    let defaults = AllSettings::default();
                    let _ = save(&defaults);
                    defaults
                }
            },
            Err(e) => {
                log::warn!("[storage/settings] failed to read settings: {}", e);
                let defaults = AllSettings::default();
                let _ = save(&defaults);
                defaults
            }
        }
    } else {
        log::debug!("[storage/settings] using default settings");
        let defaults = AllSettings::default();
        let _ = save(&defaults);
        defaults
    };

    if let Ok(mut lock) = SETTINGS_CACHE.write() {
        *lock = Some(loaded.clone());
    }
    loaded
}

/// Known provider defaults for migration.
/// Must match the values in platform-presets.ts.
struct ProviderDefaults {
    base_url: Option<&'static str>,
    models: Option<Vec<String>>,
    extra_env: Option<std::collections::HashMap<String, String>>,
    key_optional: bool,
    auth_env_var: Option<&'static str>,
}

/// Known provider defaults exposed for auth resolution fallback.
pub(crate) struct ProviderInfo {
    pub base_url: Option<String>,
    pub models: Option<Vec<String>>,
    pub extra_env: Option<std::collections::HashMap<String, String>>,
    pub key_optional: bool,
    pub auth_env_var: Option<String>,
}

pub(crate) fn is_key_optional_platform(pid: &str) -> bool {
    known_provider_defaults(pid).is_some_and(|d| d.key_optional)
}

pub(crate) fn get_provider_info(pid: &str) -> Option<ProviderInfo> {
    known_provider_defaults(pid).map(|d| ProviderInfo {
        base_url: d.base_url.map(|s| s.to_string()),
        models: d.models,
        extra_env: d.extra_env,
        key_optional: d.key_optional,
        auth_env_var: d.auth_env_var.map(|s| s.to_string()),
    })
}

fn known_provider_defaults(pid: &str) -> Option<ProviderDefaults> {
    use std::collections::HashMap;
    match pid {
        "deepseek" => Some(ProviderDefaults {
            base_url: Some("https://api.deepseek.com/anthropic"),
            // 2 models → [opus+sonnet]=v4-pro[1m], [haiku]=v4-flash (per official CC guide)
            models: Some(vec![
                "deepseek-v4-pro[1m]".to_string(),
                "deepseek-v4-flash".to_string(),
            ]),
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "600000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "kimi" => Some(ProviderDefaults {
            base_url: Some("https://api.moonshot.cn/anthropic"),
            models: Some(vec!["kimi-k2.5".to_string(), "kimi-k2".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "kimi-coding" => Some(ProviderDefaults {
            base_url: Some("https://api.kimi.com/coding/"),
            // No model override — Kimi Coding routes server-side regardless of ANTHROPIC_MODEL
            models: None,
            extra_env: None,
            key_optional: false,
            // Official docs use ANTHROPIC_API_KEY (x-api-key), not Bearer token
            auth_env_var: Some("ANTHROPIC_API_KEY"),
        }),
        "zhipu" => Some(ProviderDefaults {
            base_url: Some("https://open.bigmodel.cn/api/anthropic"),
            // Tier map per z.ai/bigmodel docs: opus=glm-5.1, sonnet=glm-5-turbo, haiku=glm-4.5-air
            models: Some(vec![
                "glm-5.1".to_string(),
                "glm-5-turbo".to_string(),
                "glm-4.5-air".to_string(),
            ]),
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "3000000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "zhipu-intl" => Some(ProviderDefaults {
            base_url: Some("https://api.z.ai/api/anthropic"),
            models: Some(vec![
                "glm-5.1".to_string(),
                "glm-5-turbo".to_string(),
                "glm-4.5-air".to_string(),
            ]),
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "3000000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "bailian" => Some(ProviderDefaults {
            base_url: Some("https://coding.dashscope.aliyuncs.com/apps/anthropic"),
            models: Some(vec![
                "qwen3.5-plus".to_string(),
                "qwen3-coder-next".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "bailian-api" => Some(ProviderDefaults {
            base_url: Some("https://dashscope.aliyuncs.com/apps/anthropic"),
            models: Some(vec![
                "qwen3.5-plus".to_string(),
                "qwen3-coder-next".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "doubao" => Some(ProviderDefaults {
            base_url: Some("https://ark.cn-beijing.volces.com/api/coding"),
            models: Some(vec!["doubao-seed-code-preview-latest".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "minimax" => Some(ProviderDefaults {
            base_url: Some("https://api.minimax.io/anthropic"),
            models: Some(vec!["MiniMax-M3".to_string()]),
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "3000000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "minimax-cn" => Some(ProviderDefaults {
            base_url: Some("https://api.minimaxi.com/anthropic"),
            models: Some(vec!["MiniMax-M3".to_string()]),
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "3000000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "mimo" => Some(ProviderDefaults {
            base_url: Some("https://api.xiaomimimo.com/anthropic"),
            models: Some(vec!["mimo-v2.5-pro".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "mimo-tp" => Some(ProviderDefaults {
            base_url: Some("https://token-plan-cn.xiaomimimo.com/anthropic"),
            models: Some(vec!["mimo-v2.5-pro".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "siliconflow" => Some(ProviderDefaults {
            base_url: Some("https://api.siliconflow.com/"),
            models: None,
            extra_env: None,
            key_optional: false,
            // Official docs use ANTHROPIC_API_KEY (x-api-key), not Bearer token
            auth_env_var: Some("ANTHROPIC_API_KEY"),
        }),
        "hunyuan" => Some(ProviderDefaults {
            base_url: Some("https://api.hunyuan.cloud.tencent.com/anthropic"),
            models: Some(vec![
                "hunyuan-2.0-thinking-20251109".to_string(),
                "hunyuan-2.0-instruct-20251111".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "stepfun" => Some(ProviderDefaults {
            base_url: Some("https://api.stepfun.ai/step_plan"),
            models: Some(vec!["step-3.7-flash".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "longcat" => Some(ProviderDefaults {
            base_url: Some("https://api.longcat.chat/anthropic"),
            models: Some(vec!["LongCat-2.0-Preview".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "iflytek" => Some(ProviderDefaults {
            base_url: Some("https://maas-coding-api.cn-huabei-1.xf-yun.com/anthropic"),
            models: Some(vec!["astron-code-latest".to_string()]),
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "600000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "tencent-coding" => Some(ProviderDefaults {
            // Tencent TokenHub Coding Plan — distinct from `hunyuan` (PAYG). Serves multiple
            // models (e.g. glm-5); no fixed default, user picks via ANTHROPIC_MODEL.
            base_url: Some("https://api.lkeap.cloud.tencent.com/coding/anthropic"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "openrouter" => Some(ProviderDefaults {
            base_url: Some("https://openrouter.ai/api"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "aihubmix" => Some(ProviderDefaults {
            base_url: Some("https://aihubmix.com"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "zenmux" => Some(ProviderDefaults {
            base_url: Some("https://zenmux.ai/api/anthropic"),
            models: None,
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "30000000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "vercel" => Some(ProviderDefaults {
            base_url: Some("https://ai-gateway.vercel.sh"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "requesty" => Some(ProviderDefaults {
            // Model uses provider/model-name format (e.g. anthropic/claude-sonnet-4-5) — user picks
            base_url: Some("https://router.requesty.ai"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "fireworks" => Some(ProviderDefaults {
            // Base ends at /inference — SDK appends /v1/messages. Model uses
            // accounts/fireworks/models/<name> format, so no fixed default.
            base_url: Some("https://api.fireworks.ai/inference"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "deepinfra" => Some(ProviderDefaults {
            // Hosts OSS models via Anthropic protocol — model uses org/name format
            // (e.g. deepseek-ai/DeepSeek-V3.1-Terminus), so no fixed default.
            base_url: Some("https://api.deepinfra.com/anthropic"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "novita" => Some(ProviderDefaults {
            base_url: Some("https://api.novita.ai/anthropic"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "ccswitch" => Some(ProviderDefaults {
            base_url: Some("http://127.0.0.1:15721"),
            models: None,
            extra_env: None,
            key_optional: true,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN"),
        }),
        "ccr" => Some(ProviderDefaults {
            base_url: Some("http://127.0.0.1:3456"),
            models: Some(vec!["claude-sonnet-4-6".to_string()]),
            extra_env: None,
            key_optional: true,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN"),
        }),
        "ollama" => Some(ProviderDefaults {
            base_url: Some("http://localhost:11434"),
            models: None,
            extra_env: None,
            key_optional: true,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN"),
        }),
        _ => None,
    }
}

/// Migrate stale platform credential data. Returns true if any changes were made.
///
/// Fixes:
/// - Incorrect auth_env_var for providers that need ANTHROPIC_API_KEY (x-api-key header)
/// - Old "minimax" credentials using minimaxi.com → rename to "minimax-cn" preset
/// - Missing models/extra_env on existing credentials (needed for ANTHROPIC_MODEL injection)
fn migrate_platform_credentials(settings: &mut AllSettings) -> bool {
    let auth_fixes: &[(&str, &str)] = &[
        ("deepseek", "ANTHROPIC_AUTH_TOKEN"),
        ("zhipu", "ANTHROPIC_AUTH_TOKEN"),
        ("zhipu-intl", "ANTHROPIC_AUTH_TOKEN"),
        ("doubao", "ANTHROPIC_AUTH_TOKEN"),
        ("minimax", "ANTHROPIC_AUTH_TOKEN"),
        ("minimax-cn", "ANTHROPIC_AUTH_TOKEN"),
        ("mimo", "ANTHROPIC_AUTH_TOKEN"),
        ("bailian", "ANTHROPIC_AUTH_TOKEN"),
        ("aihubmix", "ANTHROPIC_AUTH_TOKEN"),
        // These two use x-api-key per official docs — migrate stale AUTH_TOKEN creds to API_KEY
        ("kimi-coding", "ANTHROPIC_API_KEY"),
        ("siliconflow", "ANTHROPIC_API_KEY"),
    ];
    let mut changed = false;

    for cred in &mut settings.user.platform_credentials {
        // Fix auth_env_var
        for &(pid, correct) in auth_fixes {
            if cred.platform_id == pid && cred.auth_env_var.as_deref() != Some(correct) {
                log::info!(
                    "[storage/settings] migrating auth_env_var for '{}': {:?} → {}",
                    pid,
                    cred.auth_env_var,
                    correct
                );
                cred.auth_env_var = Some(correct.to_string());
                changed = true;
            }
        }

        // Migrate old "minimax" credentials that used minimaxi.com → "minimax-cn"
        if cred.platform_id == "minimax" {
            if let Some(ref url) = cred.base_url {
                if url.contains("api.minimaxi.com") {
                    log::info!(
                        "[storage/settings] migrating minimax credential with minimaxi.com to minimax-cn"
                    );
                    cred.platform_id = "minimax-cn".to_string();
                    changed = true;
                }
            }
        }

        // Populate base_url, models, and extra_env from known provider defaults if missing.
        // base_url is CRITICAL — without it, ANTHROPIC_BASE_URL is not set and
        // requests go to Anthropic's default endpoint instead of the third-party provider.
        if let Some(defaults) = known_provider_defaults(&cred.platform_id) {
            if cred.base_url.as_ref().is_none_or(|s| s.is_empty()) {
                if let Some(url) = defaults.base_url {
                    log::info!(
                        "[storage/settings] migrating base_url for '{}': {}",
                        cred.platform_id,
                        url
                    );
                    cred.base_url = Some(url.to_string());
                    changed = true;
                }
            }
            if cred.models.is_none() {
                if let Some(models) = defaults.models {
                    log::info!(
                        "[storage/settings] migrating models for '{}': {:?}",
                        cred.platform_id,
                        models
                    );
                    cred.models = Some(models);
                    changed = true;
                }
            }
            if cred.extra_env.is_none() {
                if let Some(extra) = defaults.extra_env {
                    log::info!(
                        "[storage/settings] migrating extra_env for '{}': {:?}",
                        cred.platform_id,
                        extra
                    );
                    cred.extra_env = Some(extra);
                    changed = true;
                }
            }
        }
    }

    // If active_platform_id was "minimax" but was migrated to "minimax-cn", update it
    if settings.user.active_platform_id.as_deref() == Some("minimax") {
        // Check if the minimax credential was migrated to minimax-cn
        let has_minimax_cn = settings
            .user
            .platform_credentials
            .iter()
            .any(|c| c.platform_id == "minimax-cn");
        let has_minimax = settings
            .user
            .platform_credentials
            .iter()
            .any(|c| c.platform_id == "minimax");
        if has_minimax_cn && !has_minimax {
            log::info!(
                "[storage/settings] migrating active_platform_id from minimax to minimax-cn"
            );
            settings.user.active_platform_id = Some("minimax-cn".to_string());
            changed = true;
        }
    }

    // Also fix the global auth_env_var if it was set by one of these providers
    // (only if active_platform_id matches a provider that needs fixing)
    if let Some(ref pid) = settings.user.active_platform_id {
        for &(fix_pid, correct) in auth_fixes {
            if pid == fix_pid && settings.user.auth_env_var.as_deref() != Some(correct) {
                log::info!(
                    "[storage/settings] migrating global auth_env_var for active platform '{}': {:?} → {}",
                    pid,
                    settings.user.auth_env_var,
                    correct
                );
                settings.user.auth_env_var = Some(correct.to_string());
                changed = true;
            }
        }
    }

    changed
}

fn cli_binding() -> AgentProviderBinding {
    AgentProviderBinding {
        mode: "cli".to_string(),
        provider_id: None,
        models: None,
        model: None,
    }
}

fn custom_binding(provider_id: String, model: Option<String>) -> AgentProviderBinding {
    AgentProviderBinding {
        mode: "custom".to_string(),
        provider_id: Some(provider_id),
        models: model.clone().map(|value| vec![value]),
        model,
    }
}

fn unique_provider_id(providers: &[GlobalProviderCredential], requested: &str) -> String {
    let base = if requested.trim().is_empty() {
        "custom"
    } else {
        requested.trim()
    };
    if !providers.iter().any(|p| p.id == base) {
        return base.to_string();
    }
    for suffix in ["-codex", "-pi", "-2", "-3", "-4"] {
        let candidate = format!("{}{}", base, suffix);
        if !providers.iter().any(|p| p.id == candidate) {
            return candidate;
        }
    }
    format!("{}-{}", base, providers.len() + 1)
}

fn upsert_global_provider(
    providers: &mut Vec<GlobalProviderCredential>,
    provider: GlobalProviderCredential,
) {
    if let Some(existing) = providers.iter_mut().find(|p| p.id == provider.id) {
        *existing = provider;
    } else {
        providers.push(provider);
    }
}

fn model_ids(models: &Option<Vec<GlobalProviderModel>>) -> Option<Vec<String>> {
    models.as_ref().map(|items| {
        items
            .iter()
            .map(|model| model.id.clone())
            .filter(|id| !id.trim().is_empty())
            .collect()
    })
}

fn model_records(models: Option<Vec<String>>) -> Option<Vec<GlobalProviderModel>> {
    models.map(|items| {
        items
            .into_iter()
            .filter(|id| !id.trim().is_empty())
            .map(|id| GlobalProviderModel {
                id,
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            })
            .collect()
    })
}

fn first_model_id(models: &Option<Vec<GlobalProviderModel>>) -> Option<String> {
    models
        .as_ref()
        .and_then(|items| items.iter().find(|model| !model.id.trim().is_empty()))
        .map(|model| model.id.clone())
}

fn provider_model_catalog(provider: &GlobalProviderCredential) -> Option<&[GlobalProviderModel]> {
    provider
        .models
        .as_deref()
        .filter(|models| models.iter().any(|model| !model.id.trim().is_empty()))
}

fn resolve_provider_model(
    provider: &GlobalProviderCredential,
    requested: Option<&str>,
) -> Option<String> {
    let requested = requested.map(str::trim).filter(|model| !model.is_empty());
    let Some(catalog) = provider_model_catalog(provider) else {
        return requested
            .map(str::to_string)
            .or_else(|| first_model_id(&provider.models));
    };

    requested
        .filter(|model| {
            catalog
                .iter()
                .any(|configured| configured.id.trim() == *model)
        })
        .map(str::to_string)
        .or_else(|| {
            catalog
                .iter()
                .find(|model| !model.id.trim().is_empty())
                .map(|model| model.id.clone())
        })
}

/// Keep an Agent's selected/allowed models inside the canonical Provider catalog.
///
/// The global Provider model list is authoritative when it is present. Older settings stored
/// the selected model in the legacy Pi/Codex projection and in the Agent binding, so deleting a
/// model without repairing those fields could re-introduce it into a Runtime catalog.
fn normalize_custom_binding(
    binding: &mut AgentProviderBinding,
    providers: &[GlobalProviderCredential],
) {
    if binding.mode != "custom" {
        return;
    }

    let Some(provider_id) = binding.provider_id.as_deref() else {
        *binding = cli_binding();
        return;
    };
    let Some(provider) = providers.iter().find(|provider| provider.id == provider_id) else {
        // A deleted Provider must not leave a custom binding pointing at a non-existent
        // profile. Falling back to native CLI auth keeps all Runtime projections coherent.
        *binding = cli_binding();
        return;
    };
    let Some(catalog) = provider_model_catalog(provider) else {
        // `None`/empty means the Provider has no catalog yet. Preserve legacy/manual model
        // values because there is no authoritative list against which to validate them.
        return;
    };

    let catalog_ids: Vec<&str> = catalog
        .iter()
        .map(|model| model.id.trim())
        .filter(|id| !id.is_empty())
        .collect();
    if catalog_ids.is_empty() {
        return;
    }

    if let Some(enabled_models) = binding.models.as_mut() {
        enabled_models.retain(|model| catalog_ids.contains(&model.trim()));
        enabled_models.dedup();
        if enabled_models.is_empty() {
            enabled_models.push(catalog_ids[0].to_string());
        }
    }

    let default_is_valid = binding.model.as_deref().is_some_and(|model| {
        let model = model.trim();
        !model.is_empty()
            && catalog_ids.contains(&model)
            && binding
                .models
                .as_ref()
                .is_none_or(|enabled| enabled.iter().any(|item| item.trim() == model))
    });
    let needs_default = binding.model.is_none() && binding.models.is_some() || !default_is_valid;
    if needs_default {
        binding.model = binding
            .models
            .as_ref()
            .and_then(|models| models.first())
            .cloned()
            .or_else(|| catalog_ids.first().map(|id| (*id).to_string()));
    }
}

fn normalize_custom_bindings(
    bindings: &mut AgentProviderBindings,
    providers: &[GlobalProviderCredential],
) {
    normalize_custom_binding(&mut bindings.claude, providers);
    normalize_custom_binding(&mut bindings.codex, providers);
    normalize_custom_binding(&mut bindings.pi, providers);
    normalize_custom_binding(&mut bindings.grok, providers);
    normalize_custom_binding(&mut bindings.dsh, providers);
}

fn normalize_provider_test_model(provider: &mut GlobalProviderCredential) {
    let Some(catalog) = provider_model_catalog(provider) else {
        return;
    };
    let test_model_is_valid = provider.test_model.as_deref().is_some_and(|model| {
        catalog
            .iter()
            .any(|configured| configured.id.trim() == model.trim())
    });
    let fallback = catalog
        .iter()
        .find(|model| !model.id.trim().is_empty())
        .map(|model| model.id.clone());
    if !test_model_is_valid {
        provider.test_model = fallback;
    }
}

/// Resolve a model for a new Runtime run from the current canonical Provider catalog.
///
/// This is also a last line of defense for an idle/pending run whose frontend still submitted a
/// model removed in another settings view. A missing catalog remains backward-compatible with
/// legacy/manual Provider configurations.
pub fn resolve_model_for_agent(
    settings: &UserSettings,
    agent: &str,
    requested: Option<&str>,
) -> Option<String> {
    let requested = requested.map(str::trim).filter(|model| !model.is_empty());
    let binding = settings
        .agent_provider_bindings
        .as_ref()
        .and_then(|bindings| match agent {
            "claude" => Some(&bindings.claude),
            "codex" => Some(&bindings.codex),
            "pi" => Some(&bindings.pi),
            "grok" => Some(&bindings.grok),
            "dsh" => Some(&bindings.dsh),
            _ => None,
        });
    if binding.is_none_or(|binding| binding.mode != "custom") {
        return requested.map(str::to_string);
    }

    let binding = binding.expect("checked above");
    let provider = binding.provider_id.as_deref().and_then(|provider_id| {
        settings
            .global_providers
            .iter()
            .find(|p| p.id == provider_id)
    });
    let Some(provider) = provider else {
        return requested.map(str::to_string);
    };
    let Some(catalog) = provider_model_catalog(provider) else {
        return requested.map(str::to_string).or_else(|| {
            binding
                .model
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(str::to_string)
        });
    };

    let catalog_ids: Vec<&str> = catalog
        .iter()
        .map(|model| model.id.trim())
        .filter(|id| !id.is_empty())
        .collect();
    let allowed_ids: Vec<&str> = match binding.models.as_deref() {
        Some(enabled) if !enabled.is_empty() => enabled
            .iter()
            .map(|model| model.trim())
            .filter(|id| !id.is_empty() && catalog_ids.contains(id))
            .collect(),
        _ => catalog_ids.clone(),
    };
    if allowed_ids.is_empty() {
        return catalog_ids.first().map(|model| (*model).to_string());
    }

    // A composite "provider-id/model-id" request explicitly targets a specific
    // global provider, not the bound provider. Return it as-is so the Pi
    // bridge can route to the correct provider without binding interference.
    if requested.is_some_and(|m| m.contains('/')) {
        return requested.map(str::to_string);
    }

    requested
        .filter(|model| allowed_ids.contains(model))
        .map(str::to_string)
        .or_else(|| {
            binding
                .model
                .as_deref()
                .map(str::trim)
                .filter(|model| allowed_ids.contains(model))
                .map(str::to_string)
        })
        .or_else(|| allowed_ids.first().map(|model| (*model).to_string()))
}

fn trim_provider_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    trimmed.to_string()
}

fn normalize_anthropic_provider_base_url(base_url: &str) -> String {
    let trimmed = trim_provider_base_url(base_url);
    if trimmed.is_empty() {
        return trimmed;
    }
    let Ok(mut url) = reqwest::Url::parse(&trimmed) else {
        return trimmed.strip_suffix("/v1").unwrap_or(&trimmed).to_string();
    };
    if url.path() == "/v1" {
        url.set_path("");
        return url.to_string().trim_end_matches('/').to_string();
    }
    trimmed
}

fn normalize_openai_provider_base_url(base_url: &str) -> String {
    let trimmed = trim_provider_base_url(base_url);
    if trimmed.is_empty() {
        return trimmed;
    }
    match reqwest::Url::parse(&trimmed) {
        Ok(url) if url.path().is_empty() || url.path() == "/" => format!("{}/v1", trimmed),
        _ => trimmed,
    }
}

fn provider_to_platform_credential(provider: &GlobalProviderCredential) -> PlatformCredential {
    PlatformCredential {
        platform_id: provider.id.clone(),
        api_key: provider.api_key.clone(),
        base_url: Some(normalize_anthropic_provider_base_url(&provider.base_url)),
        auth_env_var: provider
            .auth_env_var
            .clone()
            .or_else(|| Some("ANTHROPIC_AUTH_TOKEN".to_string())),
        name: Some(provider.name.clone()),
        models: model_ids(&provider.models),
        extra_env: provider.extra_env.clone(),
    }
}

fn provider_to_codex_credential(
    provider: &GlobalProviderCredential,
    model: Option<&str>,
) -> CodexProviderCredential {
    CodexProviderCredential {
        id: provider.id.clone(),
        name: provider.name.clone(),
        base_url: normalize_openai_provider_base_url(&provider.base_url),
        env_key: provider
            .env_key
            .clone()
            .unwrap_or_else(|| "OPENAI_API_KEY".to_string()),
        wire_api: if provider.protocol == "openai-completions" {
            "chat".to_string()
        } else {
            "responses".to_string()
        },
        model: resolve_provider_model(provider, model).unwrap_or_default(),
        api_key: provider.api_key.clone(),
        supports_reasoning_effort: provider.supports_reasoning_effort,
        supports_websockets: None,
    }
}

fn provider_to_pi_credential(
    provider: &GlobalProviderCredential,
    model: Option<&str>,
) -> PiProviderCredential {
    let selected_model = resolve_provider_model(provider, model).unwrap_or_default();
    let context_window = provider
        .models
        .as_ref()
        .and_then(|models| models.iter().find(|item| item.id == selected_model))
        .and_then(|item| item.context_window);
    let mut models = provider.models.clone().unwrap_or_default();
    if !selected_model.is_empty() && models.is_empty() {
        models.push(GlobalProviderModel {
            id: selected_model.clone(),
            name: None,
            context_window,
            max_tokens: None,
            supports_reasoning: None,
            supports_xhigh: None,
            supported_effort_levels: None,
            supports_images: None,
        });
    }
    let base_url = if provider.protocol == "anthropic-messages" {
        normalize_anthropic_provider_base_url(&provider.base_url)
    } else {
        normalize_openai_provider_base_url(&provider.base_url)
    };

    PiProviderCredential {
        id: provider.id.clone(),
        name: provider.name.clone(),
        base_url,
        api: provider.protocol.clone(),
        model: selected_model,
        models,
        context_window,
        api_key: provider.api_key.clone(),
    }
}

fn selected_provider<'a>(
    providers: &'a [GlobalProviderCredential],
    binding: &AgentProviderBinding,
) -> Option<&'a GlobalProviderCredential> {
    if binding.mode != "custom" {
        return None;
    }
    binding
        .provider_id
        .as_deref()
        .and_then(|id| providers.iter().find(|p| p.id == id))
}

/// Materialize canonical settings into the legacy fields consumed by older UI and runtime code.
/// This is deliberately lossless: the canonical profiles remain the source of truth, while the
/// old fields are kept as a compatibility projection until all consumers have migrated.
fn materialize_legacy_provider_settings(settings: &mut AllSettings) {
    let Some(mut bindings) = settings.user.agent_provider_bindings.clone() else {
        return;
    };
    for provider in &mut settings.user.global_providers {
        normalize_provider_test_model(provider);
    }
    let providers = &settings.user.global_providers;
    normalize_custom_bindings(&mut bindings, providers);
    settings.user.agent_provider_bindings = Some(bindings.clone());

    let anthropic_providers: Vec<PlatformCredential> = providers
        .iter()
        .filter(|p| p.protocol == "anthropic-messages")
        .map(provider_to_platform_credential)
        .collect();
    if !anthropic_providers.is_empty() {
        settings.user.platform_credentials = anthropic_providers;
    }

    if let Some(provider) = selected_provider(providers, &bindings.claude)
        .filter(|p| p.protocol == "anthropic-messages")
    {
        settings.user.auth_mode = "api".to_string();
        settings.user.active_platform_id = Some(provider.id.clone());
        settings.user.anthropic_api_key = provider.api_key.clone();
        settings.user.anthropic_base_url = Some(provider.base_url.clone());
        settings.user.auth_env_var = provider
            .auth_env_var
            .clone()
            .or_else(|| Some("ANTHROPIC_AUTH_TOKEN".to_string()));
    } else if bindings.claude.mode == "cli" {
        settings.user.auth_mode = "cli".to_string();
        settings.user.active_platform_id = None;
        settings.user.anthropic_base_url = None;
        settings.user.auth_env_var = None;
    } else {
        // A custom binding without a valid profile must never fall back to a stale
        // legacy provider after the canonical profile was removed.
        settings.user.auth_mode = "api".to_string();
        settings.user.active_platform_id = None;
        settings.user.anthropic_api_key = None;
        settings.user.anthropic_base_url = None;
        settings.user.auth_env_var = None;
    }

    if let Some(provider) = selected_provider(providers, &bindings.codex).filter(|p| {
        matches!(
            p.protocol.as_str(),
            "openai-responses" | "openai-completions"
        )
    }) {
        settings.user.codex_provider = Some(provider_to_codex_credential(
            provider,
            bindings.codex.model.as_deref(),
        ));
    } else {
        settings.user.codex_provider = None;
    }

    if let Some(provider) = selected_provider(providers, &bindings.pi) {
        settings.user.pi_provider = Some(provider_to_pi_credential(
            provider,
            bindings.pi.model.as_deref(),
        ));
    } else {
        settings.user.pi_provider = None;
    }
}

/// Create canonical profiles from the pre-existing agent-specific provider fields once.
fn migrate_global_provider_settings(settings: &mut AllSettings) -> bool {
    let before = serde_json::to_value(&settings.user).ok();
    let mut providers = settings.user.global_providers.clone();
    let mut claude_id = settings.user.active_platform_id.clone();
    let mut codex_id = None;
    let mut pi_id = None;

    if providers.is_empty() {
        for credential in &settings.user.platform_credentials {
            let id = unique_provider_id(&providers, &credential.platform_id);
            if claude_id.as_deref() == Some(credential.platform_id.as_str()) {
                claude_id = Some(id.clone());
            }
            providers.push(GlobalProviderCredential {
                id,
                name: credential
                    .name
                    .clone()
                    .unwrap_or_else(|| credential.platform_id.clone()),
                protocol: "anthropic-messages".to_string(),
                base_url: credential.base_url.clone().unwrap_or_default(),
                api_key: credential.api_key.clone(),
                auth_env_var: credential.auth_env_var.clone(),
                env_key: None,
                models: model_records(credential.models.clone()),
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: credential.extra_env.clone(),
                keyless: None,
            });
        }

        // Older versions could store a direct API key without a platform credential.
        if settings.user.auth_mode != "cli"
            && (settings.user.anthropic_api_key.is_some()
                || settings.user.anthropic_base_url.is_some())
        {
            let id = unique_provider_id(&providers, "claude-custom");
            claude_id = Some(id.clone());
            providers.push(GlobalProviderCredential {
                id,
                name: "Claude Custom".to_string(),
                protocol: "anthropic-messages".to_string(),
                base_url: settings.user.anthropic_base_url.clone().unwrap_or_default(),
                api_key: settings.user.anthropic_api_key.clone(),
                auth_env_var: settings.user.auth_env_var.clone(),
                env_key: None,
                models: None,
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: None,
                keyless: None,
            });
        }

        if let Some(credential) = settings.user.codex_provider.clone() {
            let id = unique_provider_id(&providers, &credential.id);
            codex_id = Some(id.clone());
            providers.push(GlobalProviderCredential {
                id,
                name: credential.name,
                protocol: "openai-responses".to_string(),
                base_url: credential.base_url,
                api_key: credential.api_key,
                auth_env_var: None,
                env_key: Some(credential.env_key),
                models: model_records(
                    (!credential.model.is_empty()).then_some(vec![credential.model]),
                ),
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: None,
                keyless: None,
            });
        }

        if let Some(credential) = settings.user.pi_provider.clone() {
            let id = unique_provider_id(&providers, &credential.id);
            pi_id = Some(id.clone());
            providers.push(GlobalProviderCredential {
                id,
                name: credential.name,
                protocol: credential.api,
                base_url: credential.base_url,
                api_key: credential.api_key,
                auth_env_var: None,
                env_key: None,
                models: model_records(
                    (!credential.model.is_empty()).then_some(vec![credential.model]),
                ),
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: None,
                keyless: None,
            });
        }
        settings.user.global_providers = providers;
    }

    // Anthropic's CLI appends `/v1/messages` itself. Repair old canonical profiles that were
    // saved by the protocol-agnostic normalizer with a trailing `/v1`.
    for provider in &mut settings.user.global_providers {
        if provider.protocol == "anthropic-messages" {
            provider.base_url = normalize_anthropic_provider_base_url(&provider.base_url);
        }
    }

    if settings.user.agent_provider_bindings.is_none() {
        let claude = if settings.user.auth_mode == "cli" {
            cli_binding()
        } else if let Some(id) = claude_id.or_else(|| {
            settings
                .user
                .global_providers
                .iter()
                .find(|p| p.protocol == "anthropic-messages")
                .map(|p| p.id.clone())
        }) {
            custom_binding(id, None)
        } else {
            cli_binding()
        };
        let codex = settings
            .user
            .codex_provider
            .as_ref()
            .and(codex_id)
            .map(|id| custom_binding(id, None))
            .unwrap_or_else(cli_binding);
        let pi = settings
            .user
            .pi_provider
            .as_ref()
            .and(pi_id)
            .map(|id| custom_binding(id, None))
            .unwrap_or_else(cli_binding);
        let grok = cli_binding();
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude,
            codex,
            pi,
            grok,
            dsh: cli_binding(),
        });
    }

    // AgentCabin 0.2.6 briefly allowed Chat Completions providers in the Codex picker while the
    // settings-page compatibility filter still rejected them during save. That produced a
    // misleading successful save with `mode=custom` but no provider id. Repair only the
    // unambiguous case so configurations with multiple compatible providers still require an
    // explicit user choice.
    if let Some(bindings) = settings.user.agent_provider_bindings.as_mut() {
        if bindings.codex.mode == "custom" && bindings.codex.provider_id.is_none() {
            let compatible = settings
                .user
                .global_providers
                .iter()
                .filter(|provider| {
                    matches!(
                        provider.protocol.as_str(),
                        "openai-responses" | "openai-completions"
                    )
                })
                .collect::<Vec<_>>();
            if let [provider] = compatible.as_slice() {
                let models = provider
                    .models
                    .as_ref()
                    .map(|models| {
                        models
                            .iter()
                            .map(|model| model.id.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                bindings.codex.provider_id = Some(provider.id.clone());
                bindings.codex.model = models.first().cloned();
                bindings.codex.models = (!models.is_empty()).then_some(models);
            }
        }
    }

    materialize_legacy_provider_settings(settings);
    before != serde_json::to_value(&settings.user).ok()
}

/// Keep the canonical projection current when an older screen still writes a legacy field.
fn sync_global_from_legacy(settings: &mut AllSettings) {
    if settings.user.global_providers.is_empty() {
        return;
    }
    let mut providers = settings.user.global_providers.clone();
    for credential in &settings.user.platform_credentials {
        upsert_global_provider(
            &mut providers,
            GlobalProviderCredential {
                id: credential.platform_id.clone(),
                name: credential
                    .name
                    .clone()
                    .unwrap_or_else(|| credential.platform_id.clone()),
                protocol: "anthropic-messages".to_string(),
                base_url: normalize_anthropic_provider_base_url(
                    credential.base_url.as_deref().unwrap_or_default(),
                ),
                api_key: credential.api_key.clone(),
                auth_env_var: credential.auth_env_var.clone(),
                env_key: None,
                models: model_records(credential.models.clone()),
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: credential.extra_env.clone(),
                keyless: None,
            },
        );
    }
    if let Some(credential) = settings.user.codex_provider.clone() {
        upsert_global_provider(
            &mut providers,
            GlobalProviderCredential {
                id: credential.id,
                name: credential.name,
                protocol: "openai-responses".to_string(),
                base_url: credential.base_url,
                api_key: credential.api_key,
                auth_env_var: None,
                env_key: Some(credential.env_key),
                models: model_records(
                    (!credential.model.is_empty()).then_some(vec![credential.model]),
                ),
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: None,
                keyless: None,
            },
        );
    }
    if let Some(credential) = settings.user.pi_provider.clone() {
        // A Pi model switch updates the legacy `pi_provider.model` field. Preserve the
        // canonical provider's complete model catalog instead of replacing it with that one
        // selected model. This is important because the Pi runtime writes its current model
        // back through this compatibility path after `set_model`.
        let mut models = providers
            .iter()
            .find(|provider| provider.id == credential.id)
            .and_then(|provider| provider.models.clone())
            .unwrap_or_default();
        // A non-empty canonical catalog is authoritative. Legacy Pi settings may still carry
        // a deleted model, so importing every legacy entry here would undo a global-model
        // removal the next time any old settings field is saved.
        if models.is_empty() {
            for model in &credential.models {
                if !models.iter().any(|existing| existing.id == model.id) {
                    models.push(model.clone());
                }
            }
        }
        if models.is_empty() && !credential.model.is_empty() {
            models.push(GlobalProviderModel {
                id: credential.model.clone(),
                name: None,
                context_window: credential.context_window,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            });
        }
        upsert_global_provider(
            &mut providers,
            GlobalProviderCredential {
                id: credential.id,
                name: credential.name,
                protocol: credential.api,
                base_url: credential.base_url,
                api_key: credential.api_key,
                auth_env_var: None,
                env_key: None,
                models: (!models.is_empty()).then_some(models),
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: None,
                keyless: None,
            },
        );
    }
    settings.user.global_providers = providers;
    let mut bindings =
        settings
            .user
            .agent_provider_bindings
            .clone()
            .unwrap_or(AgentProviderBindings {
                claude: cli_binding(),
                codex: cli_binding(),
                pi: cli_binding(),
                grok: cli_binding(),
                dsh: cli_binding(),
            });
    bindings.claude = if settings.user.auth_mode == "cli" {
        cli_binding()
    } else {
        custom_binding(
            settings
                .user
                .active_platform_id
                .clone()
                .or_else(|| {
                    settings
                        .user
                        .global_providers
                        .iter()
                        .find(|p| p.protocol == "anthropic-messages")
                        .map(|p| p.id.clone())
                })
                .unwrap_or_else(|| "claude-custom".to_string()),
            None,
        )
    };
    if settings.user.codex_provider.is_none() {
        bindings.codex = cli_binding();
    } else if let Some(provider) = settings.user.codex_provider.as_ref() {
        bindings.codex = custom_binding(
            provider.id.clone(),
            (!provider.model.is_empty()).then_some(provider.model.clone()),
        );
    }
    if settings.user.pi_provider.is_none() {
        bindings.pi = cli_binding();
    } else if let Some(provider) = settings.user.pi_provider.as_ref() {
        // Keep the Agent's enabled-model selection when a legacy Pi model update is synced.
        // Replacing it with `[provider.model]` makes the chat picker collapse to one model even
        // though the canonical Provider still contains the full catalog.
        let canonical_models = settings
            .user
            .global_providers
            .iter()
            .find(|item| item.id == provider.id)
            .and_then(|item| item.models.as_ref())
            .map(|models| {
                models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut enabled_models = bindings.pi.models.clone().unwrap_or_else(|| {
            if !provider.models.is_empty() {
                provider
                    .models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect()
            } else {
                canonical_models.clone()
            }
        });
        enabled_models.retain(|model| canonical_models.iter().any(|id| id == model));
        if !provider.model.is_empty()
            && canonical_models.iter().any(|id| id == &provider.model)
            && !enabled_models.iter().any(|model| model == &provider.model)
        {
            enabled_models.push(provider.model.clone());
        }
        bindings.pi = AgentProviderBinding {
            mode: "custom".to_string(),
            provider_id: Some(provider.id.clone()),
            models: (!enabled_models.is_empty()).then_some(enabled_models),
            model: (!provider.model.is_empty()).then_some(provider.model.clone()),
        };
    }
    normalize_custom_bindings(&mut bindings, &settings.user.global_providers);
    settings.user.agent_provider_bindings = Some(bindings);
}

/// Atomic JSON-to-file write with 0600 perms. Mirrors storage::runs::save_meta:
/// write to a unique tmp file, lock perms, rename. Used by save() to avoid
/// leaving settings.json truncated on crash (API keys must not be lost).
fn write_atomic_0600(path: &std::path::Path, json: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| "path has no parent".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| "path has no filename".to_string())?;
    let tmp = dir.join(format!(
        "{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&tmp, json).map_err(|e| format!("write tmp: {e}"))?;

    // Restrict perms on the tmp file BEFORE rename so the destination is never
    // briefly world-readable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)) {
            log::warn!(
                "[storage/settings] failed to set 0600 perms on tmp settings.json: {}",
                e
            );
        }
    }

    // Rename with PermissionDenied retry (Windows AV may briefly lock the target).
    for attempt in 0..3u8 {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 2 => {
                log::debug!(
                    "[storage/settings] save rename PermissionDenied, retry {}",
                    attempt + 1
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(format!("rename: {e}"));
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err("rename: PermissionDenied after 3 retries".to_string())
}

pub fn save(settings: &AllSettings) -> Result<(), String> {
    log::debug!("[storage/settings] saving settings");
    let path = settings_path();
    super::ensure_dir(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    write_atomic_0600(&path, &json)?;
    if let Ok(mut lock) = SETTINGS_CACHE.write() {
        *lock = Some(settings.clone());
    }
    Ok(())
}

pub fn get_user_settings() -> UserSettings {
    let all = load();
    all.user
}

/// Save web server config fields. Called by restart_with_config on success.
pub fn save_web_server_config(
    enabled: bool,
    port: u16,
    bind: &str,
    allowed_origins: &Option<Vec<String>>,
    tunnel_url: &Option<String>,
) -> Result<(), String> {
    let mut all = load();
    all.user.web_server_enabled = Some(enabled);
    all.user.web_server_port = Some(port);
    all.user.web_server_bind = Some(bind.to_string());
    all.user.web_server_allowed_origins = allowed_origins.clone();
    all.user.web_server_tunnel_url = tunnel_url.clone();
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    log::debug!(
        "[storage/settings] web_server config saved: enabled={}, port={}, bind={}, tunnel={:?}",
        enabled,
        port,
        bind,
        tunnel_url,
    );
    Ok(())
}

/// Set only web_server_enabled, preserving all other web server fields.
pub fn set_web_server_enabled(enabled: bool) -> Result<(), String> {
    let mut all = load();
    all.user.web_server_enabled = Some(enabled);
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    log::debug!("[storage/settings] web_server_enabled set to {}", enabled);
    Ok(())
}

/// Partial disable: only set enabled=false, never touch other web server fields.
/// Used by the disable path to ensure disable always succeeds regardless of form state.
pub fn save_web_server_partial_disable() -> Result<(), String> {
    let mut all = load();
    all.user.web_server_enabled = Some(false);
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    log::debug!("[storage/settings] web_server partial disable saved");
    Ok(())
}

fn validate_ui_zoom(v: &serde_json::Value) -> Result<Option<f64>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let f = v
        .as_f64()
        .ok_or_else(|| "ui_zoom must be a number".to_string())?;
    if !(0.75..=1.5).contains(&f) {
        return Err(format!("ui_zoom must be between 0.75 and 1.5, got {}", f));
    }
    Ok(Some(f))
}

fn apply_pet_settings_patch(settings: &mut UserSettings, patch: &serde_json::Value) {
    if let Some(enabled) = patch.get("pet_enabled").and_then(|v| v.as_bool()) {
        settings.pet_enabled = enabled;
    }
    if let Some(scale) = patch.get("pet_scale").and_then(|v| v.as_f64()) {
        settings.pet_scale = crate::pet::window::normalize_pet_scale(scale);
    }
    if let Some(always_on_top) = patch.get("pet_always_on_top").and_then(|v| v.as_bool()) {
        settings.pet_always_on_top = always_on_top;
    }
    if let Some(id) = patch
        .get("pet_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        settings.pet_id = id.to_string();
    }
    if let Some(patrol_enabled) = patch.get("pet_patrol_enabled").and_then(|v| v.as_bool()) {
        settings.pet_patrol_enabled = patrol_enabled;
    }
    if let Some(pause_min) = patch.get("pet_patrol_pause_min").and_then(|v| v.as_u64()) {
        settings.pet_patrol_pause_min = crate::pet::window::normalize_pet_patrol_pause_min(
            pause_min.min(u32::MAX as u64) as u32,
        );
    }
    if let Some(snap_to_edge) = patch.get("pet_snap_to_edge").and_then(|v| v.as_bool()) {
        settings.pet_snap_to_edge = snap_to_edge;
    }
    if let Some(click_interaction_enabled) = patch
        .get("pet_click_interaction_enabled")
        .and_then(|v| v.as_bool())
    {
        settings.pet_click_interaction_enabled = click_interaction_enabled;
    }
    if let Some(color) = patch
        .get("pet_grokbot_color")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.pet_grokbot_color = color.to_string();
    }
    if let Some(shape) = patch
        .get("pet_grokbot_shape")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.pet_grokbot_shape = shape.to_string();
    }
    for (key, target) in [
        ("pet_grokbot_parts", &mut settings.pet_grokbot_parts),
        (
            "pet_grokbot_accessories",
            &mut settings.pet_grokbot_accessories,
        ),
    ] {
        if let Some(value) = patch.get(key) {
            *target = value
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::trim))
                        .filter(|item| !item.is_empty())
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();
        }
    }
}

pub fn update_user_settings(patch: serde_json::Value) -> Result<UserSettings, String> {
    let mut all = load();
    let canonical_provider_patch =
        patch.get("global_providers").is_some() || patch.get("agent_provider_bindings").is_some();
    let legacy_provider_patch = patch.get("platform_credentials").is_some()
        || patch.get("active_platform_id").is_some()
        || patch.get("auth_mode").is_some()
        || patch.get("anthropic_api_key").is_some()
        || patch.get("anthropic_base_url").is_some()
        || patch.get("auth_env_var").is_some()
        || patch.get("codex_provider").is_some()
        || patch.get("pi_provider").is_some();
    if let Some(agent) = patch.get("default_agent").and_then(|v| v.as_str()) {
        all.user.default_agent = agent.to_string();
    }
    if let Some(model) = patch.get("default_model") {
        all.user.default_model = model.as_str().map(|s| s.to_string());
    }
    if let Some(tools) = patch.get("allowed_tools").and_then(|v| v.as_array()) {
        all.user.allowed_tools = tools
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(wd) = patch.get("working_directory") {
        all.user.working_directory = wd.as_str().map(|s| s.to_string());
    }
    if let Some(mode) = patch.get("provider_mode").and_then(|v| v.as_str()) {
        all.user.provider_mode = mode.to_string();
    }
    if let Some(mode) = patch.get("auth_mode").and_then(|v| v.as_str()) {
        all.user.auth_mode = mode.to_string();
    }
    if let Some(key) = patch.get("anthropic_api_key") {
        all.user.anthropic_api_key = key
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
    }
    if let Some(url) = patch.get("anthropic_base_url") {
        all.user.anthropic_base_url = url
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
    }
    if let Some(v) = patch.get("auth_env_var") {
        all.user.auth_env_var = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
    }
    if let Some(mode) = patch.get("permission_mode").and_then(|v| v.as_str()) {
        all.user.permission_mode = mode.to_string();
    }
    if let Some(v) = patch.get("max_budget_usd") {
        all.user.max_budget_usd = if v.is_null() { None } else { v.as_f64() };
    }
    if let Some(v) = patch.get("fallback_model") {
        all.user.fallback_model = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("keybinding_overrides") {
        if v.is_null() {
            all.user.keybinding_overrides = vec![];
        } else {
            all.user.keybinding_overrides = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid keybinding_overrides: {}", e))?;
        }
    }
    if let Some(v) = patch.get("remote_hosts") {
        if v.is_null() {
            all.user.remote_hosts = vec![];
        } else {
            all.user.remote_hosts = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid remote_hosts: {}", e))?;
        }
    }
    if let Some(v) = patch.get("platform_credentials") {
        if v.is_null() {
            all.user.platform_credentials = vec![];
        } else {
            all.user.platform_credentials = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid platform_credentials: {}", e))?;
        }
    }
    if let Some(v) = patch.get("active_platform_id") {
        all.user.active_platform_id = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("pi_provider") {
        all.user.pi_provider = if v.is_null() {
            None
        } else {
            let provider: crate::models::PiProviderCredential =
                serde_json::from_value(v.clone())
                    .map_err(|e| format!("Invalid pi_provider: {}", e))?;
            Some(provider)
        };
    }
    if let Some(v) = patch.get("codex_provider") {
        all.user.codex_provider = if v.is_null() {
            None
        } else {
            let provider: crate::models::CodexProviderCredential =
                serde_json::from_value(v.clone())
                    .map_err(|e| format!("Invalid codex_provider: {}", e))?;
            Some(provider)
        };
    }
    if let Some(v) = patch.get("global_providers") {
        all.user.global_providers = if v.is_null() {
            vec![]
        } else {
            serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid global_providers: {}", e))?
        };
        for provider in &mut all.user.global_providers {
            if provider.protocol == "anthropic-messages" {
                provider.base_url = normalize_anthropic_provider_base_url(&provider.base_url);
            }
        }
    }
    if let Some(v) = patch.get("agent_provider_bindings") {
        all.user.agent_provider_bindings = if v.is_null() {
            None
        } else {
            Some(
                serde_json::from_value(v.clone())
                    .map_err(|e| format!("Invalid agent_provider_bindings: {}", e))?,
            )
        };
    }
    if canonical_provider_patch {
        materialize_legacy_provider_settings(&mut all);
    } else if legacy_provider_patch {
        // The old settings panels are still available during the migration window. Keep
        // canonical profiles updated when one of those panels writes a legacy field.
        sync_global_from_legacy(&mut all);
    }
    if let Some(v) = patch.get("ui_zoom") {
        all.user.ui_zoom = validate_ui_zoom(v)?;
        log::debug!("[storage/settings] ui_zoom patched: {:?}", all.user.ui_zoom);
    }
    if let Some(v) = patch.get("onboarding_completed") {
        all.user.onboarding_completed = v.as_bool().unwrap_or(false);
    }
    if let Some(v) = patch
        .get("work_mode_enabled")
        .and_then(|value| value.as_bool())
    {
        all.user.work_mode_enabled = v;
    }
    if let Some(v) = patch.get("claude_path") {
        all.user.claude_path = v
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        // The resolved-path cache must re-evaluate the override on the next spawn. (#155)
        crate::agent::claude_stream::invalidate_claude_path_cache();
    }
    if let Some(v) = patch.get("codex_path") {
        all.user.codex_path = v
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        crate::agent::claude_stream::invalidate_codex_path_cache();
    }
    if let Some(v) = patch.get("pi_path") {
        all.user.pi_path = v
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        crate::agent::claude_stream::invalidate_pi_path_cache();
    }
    if let Some(v) = patch.get("dsh_path") {
        all.user.dsh_path = v
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        // DSH is cached for the same reason as the other local runtimes. A
        // path edit must take effect on the very next Work/Code launch.
        crate::agent::claude_stream::invalidate_dsh_path_cache();
    }
    if let Some(v) = patch.get("enabled_agents") {
        if v.is_null() {
            all.user.enabled_agents = Some(vec!["pi".to_string(), "dsh".to_string()]);
        } else if let Some(arr) = v.as_array() {
            let mut agents: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            if !agents.iter().any(|a| a == "pi") {
                agents.insert(0, "pi".to_string());
            }
            if !agents.iter().any(|a| a == "dsh") {
                agents.push("dsh".to_string());
            }
            all.user.enabled_agents = Some(agents);
        }
    }
    if let Some(v) = patch.get("code_default_runtime") {
        all.user.code_default_runtime = v
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    }
    if let Some(v) = patch.get("work_default_runtime") {
        all.user.work_default_runtime = v
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    }
    if let Some(v) = patch.get("worktree_root") {
        all.user.worktree_root = v
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
    }
    if let Some(v) = patch.get("worktree_branch_prefix") {
        if let Some(value) = v.as_str() {
            let trimmed = value.trim();
            all.user.worktree_branch_prefix = if trimmed.is_empty() {
                String::new()
            } else if trimmed.ends_with('/') {
                trimmed.to_string()
            } else {
                format!("{trimmed}/")
            };
        }
    }
    if let Some(v) = patch
        .get("worktree_auto_cleanup")
        .and_then(|value| value.as_bool())
    {
        all.user.worktree_auto_cleanup = v;
    }
    if let Some(v) = patch.get("worktree_cleanup_limit") {
        if let Some(limit) = v.as_u64() {
            all.user.worktree_cleanup_limit = limit.clamp(1, 1000) as u32;
        }
    }
    if let Some(v) = patch.get("worktree_managed_paths") {
        all.user.worktree_managed_paths = if v.is_null() {
            vec![]
        } else {
            v.as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
    }
    apply_pet_settings_patch(&mut all.user, &patch);
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    Ok(all.user)
}

pub fn get_agent_settings(agent: &str) -> AgentSettings {
    log::debug!("[storage/settings] get_agent_settings: agent={}", agent);
    let all = load();
    all.agents
        .get(agent)
        .cloned()
        .unwrap_or_else(|| AgentSettings::default_for(agent))
}

/// Apply a JSON patch to AgentSettings (pure function, no I/O).
fn apply_agent_patch(settings: &mut AgentSettings, patch: &serde_json::Value) {
    if let Some(model) = patch.get("model") {
        settings.model = model.as_str().map(|s| s.to_string());
    }
    if let Some(tools) = patch.get("allowed_tools").and_then(|v| v.as_array()) {
        settings.allowed_tools = tools
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(wd) = patch.get("working_directory") {
        settings.working_directory = wd.as_str().map(|s| s.to_string());
    }
    if let Some(v) = patch.get("plan_mode") {
        settings.plan_mode = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("disallowed_tools") {
        settings.disallowed_tools = if v.is_null() {
            None
        } else {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        };
    }
    if let Some(v) = patch.get("append_system_prompt") {
        settings.append_system_prompt = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("max_budget_usd") {
        settings.max_budget_usd = if v.is_null() { None } else { v.as_f64() };
    }
    if let Some(v) = patch.get("fallback_model") {
        settings.fallback_model = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("system_prompt") {
        settings.system_prompt = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("tool_set") {
        settings.tool_set = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("add_dirs") {
        settings.add_dirs = if v.is_null() {
            None
        } else {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        };
    }
    if let Some(v) = patch.get("json_schema") {
        settings.json_schema = if v.is_null() { None } else { Some(v.clone()) };
    }
    if let Some(v) = patch.get("include_partial_messages") {
        settings.include_partial_messages = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("cli_debug") {
        settings.cli_debug = if v.is_null() {
            None
        } else {
            // Allow empty string (means "--debug" with no filter)
            v.as_str().map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("no_session_persistence") {
        settings.no_session_persistence = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("effort") {
        settings.effort = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("permission_mode") {
        settings.permission_mode = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("grok_plugin_dirs") {
        settings.grok_plugin_dirs = if v.is_null() {
            None
        } else {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect()
            })
        };
    }
    if let Some(v) = patch.get("ephemeral") {
        settings.ephemeral = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("profile") {
        // Empty string clears the value (UI sets "" to unset profile).
        settings.profile = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("ignore_user_config") {
        settings.ignore_user_config = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("ignore_rules") {
        settings.ignore_rules = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("pi_plan_mode_enabled") {
        settings.pi_plan_mode_enabled = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("pi_goal_enabled") {
        settings.pi_goal_enabled = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("pi_todo_enabled") {
        settings.pi_todo_enabled = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("pi_context_prune_enabled") {
        settings.pi_context_prune_enabled = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("pi_subagents_enabled") {
        settings.pi_subagents_enabled = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("pi_multi_edit_enabled") {
        settings.pi_multi_edit_enabled = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("pi_lsp_enabled") {
        settings.pi_lsp_enabled = if v.is_null() { None } else { v.as_bool() };
    }
}

pub fn update_agent_settings(
    agent: &str,
    patch: serde_json::Value,
) -> Result<AgentSettings, String> {
    let mut all = load();
    let mut settings = all
        .agents
        .get(agent)
        .cloned()
        .unwrap_or_else(|| AgentSettings::default_for(agent));
    apply_agent_patch(&mut settings, &patch);
    settings.updated_at = crate::models::now_iso();
    all.agents.insert(agent.to_string(), settings.clone());
    save(&all)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AllSettings, CodexProviderCredential, PiProviderCredential, PlatformCredential,
    };

    fn make_settings_with_cred(cred: PlatformCredential) -> AllSettings {
        let mut s = AllSettings::default();
        s.user.platform_credentials.push(cred);
        s
    }

    #[test]
    fn dsh_provider_binding_is_preserved_and_legacy_settings_default_it() {
        let bindings: AgentProviderBindings = serde_json::from_value(serde_json::json!({
            "claude": { "mode": "cli" },
            "codex": { "mode": "cli" },
            "pi": { "mode": "cli" },
            "grok": { "mode": "cli" },
            "dsh": {
                "mode": "custom",
                "provider_id": "shared-provider",
                "models": ["dsh-model"],
                "model": "dsh-model"
            }
        }))
        .expect("DSH binding should deserialize");
        assert_eq!(bindings.dsh.mode, "custom");
        assert_eq!(bindings.dsh.provider_id.as_deref(), Some("shared-provider"));
        assert_eq!(bindings.dsh.model.as_deref(), Some("dsh-model"));

        let legacy: AgentProviderBindings = serde_json::from_value(serde_json::json!({
            "claude": { "mode": "cli" },
            "codex": { "mode": "cli" },
            "pi": { "mode": "cli" },
            "grok": { "mode": "cli" }
        }))
        .expect("legacy bindings should remain readable");
        assert_eq!(legacy.dsh.mode, "cli");
    }

    #[test]
    fn migrate_empty_base_url_fills_from_defaults() {
        // Credential has base_url = "" (empty string), known defaults have a base_url.
        // Migration should populate the empty base_url from defaults.
        let cred = PlatformCredential {
            platform_id: "ollama".to_string(),
            api_key: None,
            base_url: Some(String::new()), // empty string
            auth_env_var: None,
            name: None,
            models: None,
            extra_env: None,
        };
        let mut settings = make_settings_with_cred(cred);
        let changed = migrate_platform_credentials(&mut settings);

        assert!(changed, "migration should have made changes");
        assert_eq!(
            settings.user.platform_credentials[0].base_url.as_deref(),
            Some("http://localhost:11434"),
            "empty base_url should be filled from defaults"
        );
    }

    #[test]
    fn anthropic_provider_base_url_does_not_keep_v1_suffix() {
        assert_eq!(
            normalize_anthropic_provider_base_url("http://127.0.0.1:3000"),
            "http://127.0.0.1:3000"
        );
        assert_eq!(
            normalize_anthropic_provider_base_url("http://127.0.0.1:3000/v1"),
            "http://127.0.0.1:3000"
        );
        assert_eq!(
            normalize_anthropic_provider_base_url("https://api.example.com/anthropic/"),
            "https://api.example.com/anthropic"
        );
    }

    #[test]
    fn pi_provider_base_url_uses_protocol_specific_normalization() {
        let anthropic = GlobalProviderCredential {
            id: "custom-claude".into(),
            name: "Custom Claude".into(),
            protocol: "anthropic-messages".into(),
            base_url: "http://127.0.0.1:3000".into(),
            api_key: Some("secret".into()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".into()),
            env_key: None,
            models: Some(vec![GlobalProviderModel {
                id: "glm-5.2".into(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: Some("glm-5.2".into()),
            supports_developer_role: None,
            supports_reasoning_effort: None,
            extra_env: None,
            keyless: None,
        };
        let pi_anthropic = provider_to_pi_credential(&anthropic, Some("glm-5.2"));
        assert_eq!(pi_anthropic.base_url, "http://127.0.0.1:3000");

        let openai_responses = GlobalProviderCredential {
            protocol: "openai-responses".into(),
            base_url: "http://127.0.0.1:3000".into(),
            ..anthropic.clone()
        };
        let pi_openai_responses = provider_to_pi_credential(&openai_responses, Some("glm-5.2"));
        assert_eq!(pi_openai_responses.base_url, "http://127.0.0.1:3000/v1");

        let openai_completions = GlobalProviderCredential {
            protocol: "openai-completions".into(),
            ..anthropic
        };
        let pi_openai_completions = provider_to_pi_credential(&openai_completions, Some("glm-5.2"));
        assert_eq!(pi_openai_completions.base_url, "http://127.0.0.1:3000/v1");
    }

    #[test]
    fn migrate_existing_anthropic_provider_removes_v1_from_canonical_url() {
        let mut settings = AllSettings::default();
        settings.user.global_providers = vec![GlobalProviderCredential {
            id: "custom-claude".into(),
            name: "Custom Claude".into(),
            protocol: "anthropic-messages".into(),
            base_url: "http://127.0.0.1:3000/v1".into(),
            api_key: Some("secret".into()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".into()),
            env_key: None,
            models: Some(vec![GlobalProviderModel {
                id: "claude-test".into(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: Some("claude-test".into()),
            supports_developer_role: None,
            supports_reasoning_effort: None,
            extra_env: None,
            keyless: None,
        }];
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude: custom_binding("custom-claude".into(), None),
            codex: cli_binding(),
            pi: cli_binding(),
            grok: cli_binding(),
            dsh: cli_binding(),
        });

        assert!(migrate_global_provider_settings(&mut settings));
        assert_eq!(
            settings.user.global_providers[0].base_url,
            "http://127.0.0.1:3000"
        );
        assert_eq!(
            settings.user.platform_credentials[0].base_url.as_deref(),
            Some("http://127.0.0.1:3000")
        );
    }

    #[test]
    fn syncing_pi_model_preserves_canonical_provider_model_catalog() {
        let mut settings = AllSettings::default();
        settings
            .user
            .global_providers
            .push(GlobalProviderCredential {
                id: "provider-openai".into(),
                name: "OpenAI".into(),
                protocol: "openai-completions".into(),
                base_url: "http://127.0.0.1:3000/v1".into(),
                api_key: Some("secret".into()),
                auth_env_var: None,
                env_key: None,
                models: Some(vec![
                    GlobalProviderModel {
                        id: "glm-5.2".into(),
                        name: None,
                        context_window: None,
                        max_tokens: None,
                        supports_reasoning: None,
                        supports_xhigh: None,
                        supported_effort_levels: None,
                        supports_images: None,
                    },
                    GlobalProviderModel {
                        id: "Qwen3.6-27B".into(),
                        name: None,
                        context_window: None,
                        max_tokens: None,
                        supports_reasoning: None,
                        supports_xhigh: None,
                        supported_effort_levels: None,
                        supports_images: None,
                    },
                ]),
                test_model: None,
                supports_developer_role: None,
                supports_reasoning_effort: None,
                extra_env: None,
                keyless: None,
            });
        settings.user.pi_provider = Some(PiProviderCredential {
            id: "provider-openai".into(),
            name: "OpenAI".into(),
            base_url: "http://127.0.0.1:3000/v1".into(),
            api: "openai-completions".into(),
            model: "Qwen3.6-27B".into(),
            models: vec![],
            context_window: None,
            api_key: Some("secret".into()),
        });
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude: cli_binding(),
            codex: cli_binding(),
            pi: AgentProviderBinding {
                mode: "custom".into(),
                provider_id: Some("provider-openai".into()),
                models: Some(vec!["glm-5.2".into(), "Qwen3.6-27B".into()]),
                model: Some("glm-5.2".into()),
            },
            grok: cli_binding(),
            dsh: cli_binding(),
        });

        sync_global_from_legacy(&mut settings);

        let models = settings.user.global_providers[0].models.as_ref().unwrap();
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["glm-5.2", "Qwen3.6-27B"]
        );
        assert_eq!(
            settings
                .user
                .agent_provider_bindings
                .as_ref()
                .unwrap()
                .pi
                .models
                .as_ref()
                .unwrap(),
            &vec!["glm-5.2".to_string(), "Qwen3.6-27B".to_string()]
        );
    }

    #[test]
    fn migrate_legacy_agent_providers_without_losing_credentials() {
        let mut settings = AllSettings::default();
        settings.user.auth_mode = "api".to_string();
        settings.user.active_platform_id = Some("openrouter".to_string());
        settings.user.platform_credentials.push(PlatformCredential {
            platform_id: "openrouter".to_string(),
            api_key: Some("anthropic-key".to_string()),
            base_url: Some("https://openrouter.ai/api/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("OpenRouter".to_string()),
            models: Some(vec!["anthropic/claude-sonnet".to_string()]),
            extra_env: None,
        });
        settings.user.codex_provider = Some(CodexProviderCredential {
            id: "vercel".to_string(),
            name: "Vercel".to_string(),
            base_url: "https://ai-gateway.vercel.sh/v1".to_string(),
            env_key: "AI_GATEWAY_API_KEY".to_string(),
            wire_api: "responses".to_string(),
            model: "openai/gpt-5.5".to_string(),
            api_key: Some("codex-key".to_string()),
            supports_reasoning_effort: None,
            supports_websockets: None,
        });
        settings.user.pi_provider = Some(PiProviderCredential {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api: "openai-completions".to_string(),
            model: "openai/gpt-4.1".to_string(),
            models: vec![],
            context_window: None,
            api_key: Some("pi-key".to_string()),
        });

        assert!(migrate_global_provider_settings(&mut settings));
        assert_eq!(settings.user.global_providers.len(), 3);
        assert_eq!(
            settings.user.global_providers[0].api_key.as_deref(),
            Some("anthropic-key")
        );
        assert_eq!(
            settings
                .user
                .agent_provider_bindings
                .as_ref()
                .unwrap()
                .claude
                .mode,
            "custom"
        );
        assert_eq!(
            settings
                .user
                .agent_provider_bindings
                .as_ref()
                .unwrap()
                .codex
                .mode,
            "custom"
        );
        assert_eq!(
            settings
                .user
                .agent_provider_bindings
                .as_ref()
                .unwrap()
                .pi
                .mode,
            "custom"
        );
        assert_eq!(
            settings
                .user
                .codex_provider
                .as_ref()
                .unwrap()
                .api_key
                .as_deref(),
            Some("codex-key")
        );
        assert_eq!(
            settings
                .user
                .pi_provider
                .as_ref()
                .unwrap()
                .api_key
                .as_deref(),
            Some("pi-key")
        );
    }

    #[test]
    fn canonical_provider_binding_materializes_legacy_runtime_fields() {
        let mut settings = AllSettings::default();
        settings.user.global_providers = vec![GlobalProviderCredential {
            id: "shared-openai".to_string(),
            name: "Shared OpenAI".to_string(),
            protocol: "openai-responses".to_string(),
            base_url: "https://example.test/v1".to_string(),
            api_key: Some("shared-key".to_string()),
            auth_env_var: None,
            env_key: Some("EXAMPLE_API_KEY".to_string()),
            models: Some(vec![GlobalProviderModel {
                id: "example-model".to_string(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: None,
            supports_developer_role: None,
            supports_reasoning_effort: None,
            extra_env: None,
            keyless: None,
        }];
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude: cli_binding(),
            codex: custom_binding("shared-openai".to_string(), None),
            pi: cli_binding(),
            grok: cli_binding(),
            dsh: cli_binding(),
        });

        materialize_legacy_provider_settings(&mut settings);
        let codex = settings.user.codex_provider.as_ref().unwrap();
        assert_eq!(codex.env_key, "EXAMPLE_API_KEY");
        assert_eq!(codex.model, "example-model");
        assert_eq!(codex.api_key.as_deref(), Some("shared-key"));
        assert!(settings.user.pi_provider.is_none());
    }

    #[test]
    fn removing_provider_model_repairs_all_agent_bindings_and_runtime_projections() {
        let provider = GlobalProviderCredential {
            id: "shared-provider".to_string(),
            name: "Shared Provider".to_string(),
            protocol: "openai-completions".to_string(),
            base_url: "https://example.test/v1".to_string(),
            api_key: Some("shared-key".to_string()),
            auth_env_var: None,
            env_key: Some("EXAMPLE_API_KEY".to_string()),
            models: Some(vec![GlobalProviderModel {
                id: "model-kept".to_string(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: Some("model-deleted".to_string()),
            supports_developer_role: None,
            supports_reasoning_effort: None,
            extra_env: None,
            keyless: None,
        };
        let stale_binding = || AgentProviderBinding {
            mode: "custom".to_string(),
            provider_id: Some("shared-provider".to_string()),
            models: Some(vec!["model-deleted".to_string(), "model-kept".to_string()]),
            model: Some("model-deleted".to_string()),
        };
        let mut settings = AllSettings::default();
        settings.user.global_providers = vec![provider];
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude: stale_binding(),
            codex: stale_binding(),
            pi: stale_binding(),
            grok: stale_binding(),
            dsh: stale_binding(),
        });

        materialize_legacy_provider_settings(&mut settings);

        let bindings = settings.user.agent_provider_bindings.as_ref().unwrap();
        for binding in [
            &bindings.claude,
            &bindings.codex,
            &bindings.pi,
            &bindings.grok,
        ] {
            assert_eq!(
                binding.models.as_deref(),
                Some(["model-kept".to_string()].as_slice())
            );
            assert_eq!(binding.model.as_deref(), Some("model-kept"));
        }
        assert_eq!(
            settings.user.codex_provider.as_ref().unwrap().model,
            "model-kept"
        );
        assert_eq!(
            settings.user.pi_provider.as_ref().unwrap().model,
            "model-kept"
        );
        assert_eq!(
            settings.user.pi_provider.as_ref().unwrap().models[0].id,
            "model-kept"
        );
        assert_eq!(
            settings.user.global_providers[0].test_model.as_deref(),
            Some("model-kept")
        );
        assert_eq!(
            resolve_model_for_agent(&settings.user, "pi", Some("model-deleted")),
            Some("model-kept".to_string())
        );
    }

    #[test]
    fn syncing_legacy_pi_settings_does_not_reintroduce_deleted_canonical_model() {
        let mut settings = AllSettings::default();
        settings.user.global_providers = vec![GlobalProviderCredential {
            id: "shared-provider".to_string(),
            name: "Shared Provider".to_string(),
            protocol: "openai-completions".to_string(),
            base_url: "https://example.test/v1".to_string(),
            api_key: None,
            auth_env_var: None,
            env_key: None,
            models: Some(vec![GlobalProviderModel {
                id: "model-kept".to_string(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: None,
            supports_developer_role: None,
            supports_reasoning_effort: None,
            extra_env: None,
            keyless: None,
        }];
        settings.user.pi_provider = Some(PiProviderCredential {
            id: "shared-provider".to_string(),
            name: "Shared Provider".to_string(),
            base_url: "https://example.test/v1".to_string(),
            api: "openai-completions".to_string(),
            model: "model-deleted".to_string(),
            models: vec![GlobalProviderModel {
                id: "model-deleted".to_string(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }],
            context_window: None,
            api_key: None,
        });
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude: cli_binding(),
            codex: cli_binding(),
            pi: AgentProviderBinding {
                mode: "custom".to_string(),
                provider_id: Some("shared-provider".to_string()),
                models: Some(vec!["model-deleted".to_string(), "model-kept".to_string()]),
                model: Some("model-deleted".to_string()),
            },
            grok: cli_binding(),
            dsh: cli_binding(),
        });

        sync_global_from_legacy(&mut settings);

        let models = settings.user.global_providers[0].models.as_ref().unwrap();
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["model-kept"]
        );
        assert_eq!(
            settings
                .user
                .agent_provider_bindings
                .as_ref()
                .unwrap()
                .pi
                .model
                .as_deref(),
            Some("model-kept")
        );
    }

    #[test]
    fn chat_completions_binding_materializes_codex_bridge_marker() {
        let mut settings = AllSettings::default();
        settings.user.global_providers = vec![GlobalProviderCredential {
            id: "chat-gateway".to_string(),
            name: "Chat Gateway".to_string(),
            protocol: "openai-completions".to_string(),
            base_url: "https://example.test/v1".to_string(),
            api_key: Some("chat-key".to_string()),
            auth_env_var: None,
            env_key: Some("CHAT_API_KEY".to_string()),
            models: Some(vec![GlobalProviderModel {
                id: "chat-model".to_string(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: None,
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: None,
            supports_developer_role: None,
            supports_reasoning_effort: None,
            extra_env: None,
            keyless: None,
        }];
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude: cli_binding(),
            codex: custom_binding("chat-gateway".to_string(), None),
            pi: cli_binding(),
            grok: cli_binding(),
            dsh: cli_binding(),
        });

        materialize_legacy_provider_settings(&mut settings);
        let codex = settings.user.codex_provider.as_ref().unwrap();
        assert_eq!(codex.wire_api, "chat");
        assert_eq!(codex.model, "chat-model");
        assert_eq!(codex.base_url, "https://example.test/v1");
    }

    #[test]
    fn repairs_incomplete_codex_binding_created_by_the_old_ui_filter() {
        let mut settings = AllSettings::default();
        settings.user.global_providers = vec![GlobalProviderCredential {
            id: "newapi-chat".to_string(),
            name: "NewAPI OpenAI".to_string(),
            protocol: "openai-completions".to_string(),
            base_url: "http://127.0.0.1:3000/v1".to_string(),
            api_key: Some("chat-key".to_string()),
            auth_env_var: None,
            env_key: None,
            models: Some(vec![GlobalProviderModel {
                id: "DeepSeek-V4-Flash".to_string(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: Some(true),
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }]),
            test_model: None,
            supports_developer_role: None,
            supports_reasoning_effort: Some(true),
            extra_env: None,
            keyless: None,
        }];
        settings.user.agent_provider_bindings = Some(AgentProviderBindings {
            claude: cli_binding(),
            codex: AgentProviderBinding {
                mode: "custom".to_string(),
                provider_id: None,
                models: Some(vec![]),
                model: None,
            },
            pi: cli_binding(),
            grok: cli_binding(),
            dsh: cli_binding(),
        });

        assert!(migrate_global_provider_settings(&mut settings));
        let binding = &settings
            .user
            .agent_provider_bindings
            .as_ref()
            .unwrap()
            .codex;
        assert_eq!(binding.provider_id.as_deref(), Some("newapi-chat"));
        assert_eq!(
            binding.models.as_ref().unwrap(),
            &vec!["DeepSeek-V4-Flash".to_string()]
        );
        assert_eq!(binding.model.as_deref(), Some("DeepSeek-V4-Flash"));
        assert_eq!(
            settings.user.codex_provider.as_ref().unwrap().wire_api,
            "chat"
        );
    }

    #[test]
    fn global_provider_accepts_legacy_string_models() {
        let provider: GlobalProviderCredential = serde_json::from_value(serde_json::json!({
            "id": "legacy",
            "name": "Legacy",
            "protocol": "openai-responses",
            "base_url": "https://example.test/v1",
            "models": ["model-a", "model-b"]
        }))
        .expect("legacy string model list should deserialize");
        assert_eq!(
            provider
                .models
                .unwrap()
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["model-a", "model-b"]
        );
    }

    #[test]
    fn provider_info_ccswitch() {
        let info = get_provider_info("ccswitch").expect("ccswitch should have provider info");
        assert!(info.key_optional);
        assert_eq!(info.base_url.as_deref(), Some("http://127.0.0.1:15721"));
        assert_eq!(info.auth_env_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn provider_info_ccr() {
        let info = get_provider_info("ccr").expect("ccr should have provider info");
        assert!(info.key_optional);
        assert_eq!(info.base_url.as_deref(), Some("http://127.0.0.1:3456"));
        assert_eq!(
            info.models
                .as_ref()
                .and_then(|m| m.first())
                .map(|s| s.as_str()),
            Some("claude-sonnet-4-6")
        );
    }

    #[test]
    fn apply_agent_patch_effort_set_and_clear() {
        let mut s = AgentSettings::default_for("claude");
        assert_eq!(s.effort, None);

        // Set effort to "high"
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "high" }));
        assert_eq!(s.effort, Some("high".to_string()));

        // Clear with empty string
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "" }));
        assert_eq!(s.effort, None);

        // Set then clear with null
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "low" }));
        assert_eq!(s.effort, Some("low".to_string()));
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": null }));
        assert_eq!(s.effort, None);

        // Absent key doesn't touch existing value
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "medium" }));
        apply_agent_patch(&mut s, &serde_json::json!({ "model": "opus" }));
        assert_eq!(s.effort, Some("medium".to_string()));
    }

    #[test]
    fn apply_agent_patch_codex_flags_set_and_clear() {
        let mut s = AgentSettings::default_for("codex");
        assert_eq!(s.ephemeral, None);
        assert_eq!(s.profile, None);
        assert_eq!(s.ignore_user_config, None);
        assert_eq!(s.ignore_rules, None);

        // Set all four
        apply_agent_patch(
            &mut s,
            &serde_json::json!({
                "ephemeral": true,
                "profile": "dev",
                "ignore_user_config": true,
                "ignore_rules": true,
            }),
        );
        assert_eq!(s.ephemeral, Some(true));
        assert_eq!(s.profile.as_deref(), Some("dev"));
        assert_eq!(s.ignore_user_config, Some(true));
        assert_eq!(s.ignore_rules, Some(true));

        // Clear booleans with false (explicit off), profile with null
        apply_agent_patch(
            &mut s,
            &serde_json::json!({
                "ephemeral": false,
                "profile": null,
                "ignore_user_config": false,
                "ignore_rules": false,
            }),
        );
        assert_eq!(s.ephemeral, Some(false));
        assert_eq!(s.profile, None);
        assert_eq!(s.ignore_user_config, Some(false));
        assert_eq!(s.ignore_rules, Some(false));

        // Clear profile with empty string
        apply_agent_patch(&mut s, &serde_json::json!({ "profile": "ci" }));
        assert_eq!(s.profile.as_deref(), Some("ci"));
        apply_agent_patch(&mut s, &serde_json::json!({ "profile": "" }));
        assert_eq!(s.profile, None);

        // Absent keys preserve existing values
        apply_agent_patch(&mut s, &serde_json::json!({ "ephemeral": true }));
        apply_agent_patch(&mut s, &serde_json::json!({ "model": "gpt-5" }));
        assert_eq!(s.ephemeral, Some(true));
    }

    #[test]
    fn validate_ui_zoom_rejects_invalid() {
        assert!(validate_ui_zoom(&serde_json::json!(0.1)).is_err());
        assert!(validate_ui_zoom(&serde_json::json!(5.0)).is_err());
        assert!(validate_ui_zoom(&serde_json::json!("abc")).is_err());
    }

    #[test]
    fn validate_ui_zoom_accepts_valid() {
        assert_eq!(
            validate_ui_zoom(&serde_json::json!(1.0)).unwrap(),
            Some(1.0)
        );
        assert_eq!(
            validate_ui_zoom(&serde_json::json!(0.75)).unwrap(),
            Some(0.75)
        );
        assert_eq!(
            validate_ui_zoom(&serde_json::json!(1.5)).unwrap(),
            Some(1.5)
        );
        assert_eq!(validate_ui_zoom(&serde_json::json!(null)).unwrap(), None);
    }

    #[test]
    fn apply_pet_settings_patch_persists_defaults_and_normalizes_scale() {
        let mut settings = UserSettings::default();
        apply_pet_settings_patch(
            &mut settings,
            &serde_json::json!({
                "pet_enabled": true,
                "pet_scale": 9.0,
                "pet_always_on_top": false,
                "pet_id": "builtin-otter",
                "pet_patrol_enabled": false,
                "pet_patrol_pause_min": 99,
                "pet_snap_to_edge": true,
                "pet_click_interaction_enabled": false,
                "pet_grokbot_color": "pink",
                "pet_grokbot_shape": "cloud",
                "pet_grokbot_parts": ["antenna", "", 42],
                "pet_grokbot_accessories": ["glasses", "cape"],
            }),
        );

        assert!(settings.pet_enabled);
        assert_eq!(settings.pet_scale, 2.0);
        assert!(!settings.pet_always_on_top);
        assert_eq!(settings.pet_id, "builtin-otter");
        assert!(!settings.pet_patrol_enabled);
        assert_eq!(settings.pet_patrol_pause_min, 30);
        assert!(settings.pet_snap_to_edge);
        assert!(!settings.pet_click_interaction_enabled);
        assert_eq!(settings.pet_grokbot_color, "pink");
        assert_eq!(settings.pet_grokbot_shape, "cloud");
        assert_eq!(settings.pet_grokbot_parts, vec!["antenna"]);
        assert_eq!(settings.pet_grokbot_accessories, vec!["glasses", "cape"]);
    }

    #[test]
    fn is_key_optional_known_platforms() {
        assert!(is_key_optional_platform("ccswitch"));
        assert!(is_key_optional_platform("ccr"));
        assert!(is_key_optional_platform("ollama"));
        assert!(!is_key_optional_platform("deepseek"));
        assert!(!is_key_optional_platform("unknown-platform"));
    }

    #[test]
    fn apply_agent_patch_permission_mode_set_and_clear() {
        let mut s = AgentSettings::default_for("codex");
        assert_eq!(s.permission_mode, None);

        // Set permission_mode to "plan"
        apply_agent_patch(&mut s, &serde_json::json!({ "permission_mode": "plan" }));
        assert_eq!(s.permission_mode, Some("plan".to_string()));

        // Clear with empty string
        apply_agent_patch(&mut s, &serde_json::json!({ "permission_mode": "" }));
        assert_eq!(s.permission_mode, None);

        // Set then clear with null
        apply_agent_patch(
            &mut s,
            &serde_json::json!({ "permission_mode": "auto_all" }),
        );
        assert_eq!(s.permission_mode, Some("auto_all".to_string()));
        apply_agent_patch(&mut s, &serde_json::json!({ "permission_mode": null }));
        assert_eq!(s.permission_mode, None);

        // Absent key doesn't touch existing value
        apply_agent_patch(&mut s, &serde_json::json!({ "permission_mode": "ask" }));
        apply_agent_patch(&mut s, &serde_json::json!({ "model": "opus" }));
        assert_eq!(s.permission_mode, Some("ask".to_string()));
    }

    #[test]
    fn apply_agent_patch_grok_plugin_dirs_set_and_clear() {
        let mut s = AgentSettings::default_for("grok");

        apply_agent_patch(
            &mut s,
            &serde_json::json!({
                "grok_plugin_dirs": [
                    "/Users/example/.claude/plugins/cache/one/1.0.0",
                    "",
                    "  /Users/example/.claude/plugins/cache/two/1.0.0  "
                ]
            }),
        );
        assert_eq!(
            s.grok_plugin_dirs,
            Some(vec![
                "/Users/example/.claude/plugins/cache/one/1.0.0".to_string(),
                "/Users/example/.claude/plugins/cache/two/1.0.0".to_string(),
            ])
        );

        apply_agent_patch(&mut s, &serde_json::json!({ "grok_plugin_dirs": null }));
        assert_eq!(s.grok_plugin_dirs, None);
    }

    #[test]
    fn write_atomic_writes_content_and_replaces_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        // First write to a fresh path
        write_atomic_0600(&path, "{\"a\":1}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":1}");

        // Overwrite — must replace cleanly with no leftover tmp file
        write_atomic_0600(&path, "{\"a\":2,\"b\":3}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":2,\"b\":3}");

        // No stray .tmp files left behind in the dir
        let leftover_tmps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftover_tmps.is_empty(),
            "no .tmp files should remain after successful writes, found: {:?}",
            leftover_tmps
                .iter()
                .map(|e| e.file_name())
                .collect::<Vec<_>>()
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_sets_0600_perms() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_atomic_0600(&path, "{\"secret\":\"value\"}").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "destination must be owner-only readable/writable"
        );
    }

    #[test]
    fn write_atomic_preserves_existing_file_when_serialize_path_fails() {
        // Sanity: if the rename fails (e.g. dir does not exist), the original
        // file (if any) is untouched. We simulate this by pointing at a path
        // whose parent does not exist.
        let dir = tempfile::tempdir().unwrap();
        let good_path = dir.path().join("settings.json");
        write_atomic_0600(&good_path, "original").unwrap();

        let bad_path = dir.path().join("missing-subdir").join("settings.json");
        let result = write_atomic_0600(&bad_path, "new");
        assert!(result.is_err(), "write to nonexistent dir should fail");

        // Original untouched
        assert_eq!(fs::read_to_string(&good_path).unwrap(), "original");
    }
}
