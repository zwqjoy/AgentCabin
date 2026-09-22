use crate::agent::claude_stream::{augmented_path, resolve_pi_path, which_binary};
use crate::agent::codex_control::{self, CodexInfoCache};
use crate::agent::control::{self, CliInfoCache};
use crate::agent::grok_session_actor::process::resolve_grok_path;
use crate::models::{
    CliInfo, CliModelInfo, CodexModelList, GlobalProviderModel, PiProviderCredential,
};
use crate::process_ext::HideConsole;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::State;

const PI_MODEL_LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const GROK_STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

fn parse_pi_model_list(output: &str) -> Vec<CliModelInfo> {
    let mut in_table = false;
    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();

    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.first() == Some(&"provider") && fields.get(1) == Some(&"model") {
            in_table = true;
            continue;
        }
        if !in_table || fields.len() < 5 {
            continue;
        }

        let provider = fields[0].trim();
        let model = fields[1].trim();
        if provider.is_empty() || model.is_empty() || provider == "-" || model == "-" {
            continue;
        }

        let value = format!("{provider}/{model}");
        if !seen.insert(value.clone()) {
            continue;
        }

        let supports_effort = fields.get(4).is_some_and(|value| *value == "yes");
        models.push(CliModelInfo {
            value,
            display_name: model.to_string(),
            description: provider.to_string(),
            context_window: fields
                .get(2)
                .and_then(|value| parse_compact_token_count(value)),
            supports_effort: Some(supports_effort),
            supported_effort_levels: supports_effort.then(|| {
                ["off", "minimal", "low", "medium", "high", "xhigh"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            }),
            supports_adaptive_thinking: None,
        });
    }

    models
}

fn parse_compact_token_count(value: &str) -> Option<u64> {
    let normalized = value.trim().replace(',', "").to_ascii_lowercase();
    if normalized.is_empty() || normalized == "-" {
        return None;
    }
    let (number, multiplier) = match normalized.chars().last()? {
        'k' => (&normalized[..normalized.len() - 1], 1_000_f64),
        'm' => (&normalized[..normalized.len() - 1], 1_000_000_f64),
        _ => (normalized.as_str(), 1_f64),
    };
    let parsed = number.parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some((parsed * multiplier).round() as u64)
}

fn managed_pi_effort_levels(model: &GlobalProviderModel) -> Vec<String> {
    let configured_levels = model
        .supported_effort_levels
        .as_deref()
        .filter(|levels| !levels.is_empty());
    if let Some(levels) = configured_levels {
        let mut result = Vec::new();
        for level in levels {
            if ["off", "low", "medium", "high", "xhigh", "max"].contains(&level.as_str())
                && !result.iter().any(|item| item == level)
            {
                result.push(level.to_string());
            }
        }
        if !result.is_empty() {
            return result;
        }
    }
    let mut levels = vec!["low".to_string(), "medium".to_string(), "high".to_string()];
    if model.supports_xhigh == Some(true) {
        levels.push("xhigh".to_string());
    }
    levels
}

/// Build the pre-session Pi catalog from AgentCabin's selected Provider when Pi is
/// configured in managed mode. `pi --list-models` only knows about Pi's native
/// providers, so until the session actor boots and injects `pi-provider-bridge.mjs`
/// via `--extension`, the picker would only show native models.
pub fn managed_pi_model_list(
    provider: &PiProviderCredential,
    allowed_models: Option<&[String]>,
) -> Vec<CliModelInfo> {
    let allowed = allowed_models
        .filter(|models| !models.is_empty())
        .map(|models| models.iter().map(String::as_str).collect::<HashSet<_>>());

    let mut configured = provider.models.clone();
    if configured.is_empty() && !provider.model.trim().is_empty() {
        configured.push(crate::models::GlobalProviderModel {
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
        .filter(|model| {
            !model.id.trim().is_empty()
                && allowed
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(model.id.as_str()))
        })
        .map(|model| {
            let supports_effort = model.supports_reasoning.unwrap_or(true);
            let supported_effort_levels = supports_effort.then(|| managed_pi_effort_levels(&model));
            CliModelInfo {
                value: model.id.clone(),
                display_name: model.name.unwrap_or_else(|| model.id.clone()),
                description: provider.name.clone(),
                context_window: model
                    .context_window
                    .or(provider.context_window)
                    .or_else(|| {
                        crate::agent::codex_subscription_bridge::is_codex_subscription_id(
                            &provider.id,
                        )
                        .then(|| {
                            crate::agent::codex_control::known_effective_context_window(&model.id)
                        })
                        .flatten()
                    }),
                supports_effort: Some(supports_effort),
                supported_effort_levels,
                supports_adaptive_thinking: None,
            }
        })
        .collect()
}

fn grok_home() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("GROK_HOME") {
        let value = value.trim();
        if !value.is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    crate::storage::home_dir().map(|home| PathBuf::from(home).join(".grok"))
}

fn grok_config_path() -> Option<PathBuf> {
    grok_home().map(|home| home.join("config.toml"))
}

fn grok_auth_path() -> Option<PathBuf> {
    grok_home().map(|home| home.join("auth.json"))
}

fn read_grok_config() -> Option<toml::Value> {
    let path = grok_config_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    toml::from_str(&raw).ok()
}

fn env_has_value(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn grok_model_has_credentials(model: &toml::Value) -> bool {
    let inline_key = model
        .get("api_key")
        .and_then(toml::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let env_key = model
        .get("env_key")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(env_has_value);
    inline_key || env_key
}

/// Detect whether Grok Build has a usable credential source without exposing or copying secrets.
/// This is a readiness signal, not a remote token validation: the live ACP handshake remains
/// authoritative if a cached credential has expired.
fn grok_credentials_available(config: Option<&toml::Value>) -> bool {
    if env_has_value("XAI_API_KEY") {
        return true;
    }
    if grok_auth_path().is_some_and(|path| path.is_file()) {
        return true;
    }

    let Some(config) = config else {
        return false;
    };
    let auth_provider_command = config
        .get("auth")
        .and_then(|auth| auth.get("auth_provider_command"))
        .and_then(toml::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if auth_provider_command {
        return true;
    }

    let model_table = config.get("model").and_then(toml::Value::as_table);
    let default_model = config
        .get("models")
        .and_then(|models| models.get("default"))
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let (Some(table), Some(default_id)) = (model_table, default_model) {
        if table
            .get(default_id)
            .is_some_and(grok_model_has_credentials)
        {
            return true;
        }
    }

    model_table.is_some_and(|table| table.values().any(grok_model_has_credentials))
}

fn grok_default_model_from_config() -> Option<String> {
    read_grok_config()?
        .get("models")?
        .get("default")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn grok_config_models() -> Vec<CliModelInfo> {
    let Some(config) = read_grok_config() else {
        return Vec::new();
    };
    let Some(table) = config.get("model").and_then(toml::Value::as_table) else {
        return Vec::new();
    };

    table
        .iter()
        .map(|(id, value)| {
            let display_name = value
                .get("name")
                .and_then(toml::Value::as_str)
                .unwrap_or(id)
                .to_string();
            let description = value
                .get("description")
                .and_then(toml::Value::as_str)
                .unwrap_or("Custom Grok Build model")
                .to_string();
            CliModelInfo {
                value: id.clone(),
                display_name,
                description,
                context_window: value
                    .get("context_window")
                    .or_else(|| value.get("contextWindow"))
                    .and_then(toml::Value::as_integer)
                    .and_then(|window| u64::try_from(window).ok())
                    .filter(|window| *window > 0),
                supports_effort: None,
                supported_effort_levels: None,
                supports_adaptive_thinking: None,
            }
        })
        .collect()
}

fn looks_like_grok_model_id(candidate: &str) -> bool {
    if candidate.is_empty() {
        return false;
    }
    let lower = candidate.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "model"
            | "models"
            | "name"
            | "id"
            | "default"
            | "available"
            | "current"
            | "warn"
            | "warning"
            | "error"
            | "failed"
            | "auth"
    ) {
        return false;
    }
    // `grok models` may include timestamped stderr diagnostics. They are not
    // model IDs even though the punctuation happens to be permissive above.
    if candidate.len() > 80 || (candidate.contains('T') && candidate.ends_with('Z')) {
        return false;
    }
    candidate
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '[' | ']'))
        && candidate.chars().any(|c| c.is_ascii_alphanumeric())
}

/// `grok models` is deliberately treated as a human-facing command: upstream may add columns or
/// status markers without a JSON compatibility promise. Keep the parser permissive and merge it
/// with explicit `[model.<id>]` entries from config.toml so custom models remain visible.
fn parse_grok_model_list(output: &str) -> Vec<CliModelInfo> {
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let mut default_model_id: Option<String> = None;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let lower_line = line.to_ascii_lowercase();
        if let Some(value) = line.strip_prefix("Default model:") {
            let value = value.trim();
            if looks_like_grok_model_id(value) {
                default_model_id = Some(value.to_string());
            }
            continue;
        }
        if lower_line == "models"
            || lower_line.starts_with("available models")
            || lower_line.starts_with("model ") && lower_line.contains("name")
        {
            continue;
        }

        let mut fields = line.split_whitespace();
        let mut first = fields
            .next()
            .unwrap_or_default()
            .trim_matches(|c: char| matches!(c, '*' | '>' | '•' | '✓' | '✔' | '│' | '└' | '├'));
        if first.is_empty() {
            first = fields.next().unwrap_or_default();
        }
        first = first.trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | ','));

        if !looks_like_grok_model_id(first) {
            continue;
        }
        if !seen.insert(first.to_string()) {
            continue;
        }

        let description = line
            .strip_prefix(first)
            .map(str::trim)
            .unwrap_or_default()
            .trim_start_matches(&['-', '—', ':', '|'][..])
            .trim()
            .to_string();
        let display_name = if description.is_empty() || lower_line.contains("(default)") {
            first.to_string()
        } else {
            description
                .split("  ")
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(first)
                .to_string()
        };

        models.push(CliModelInfo {
            value: first.to_string(),
            display_name,
            description: if description.is_empty() {
                "Grok Build".to_string()
            } else {
                description
            },
            context_window: None,
            supports_effort: None,
            supported_effort_levels: None,
            supports_adaptive_thinking: None,
        });

        if lower_line.contains("(default)") || line.starts_with('*') {
            default_model_id.get_or_insert_with(|| first.to_string());
        }
    }

    for model in grok_config_models() {
        if seen.insert(model.value.clone()) {
            models.push(model);
        }
    }
    if let Some(default_id) = default_model_id {
        if let Some(index) = models.iter().position(|model| model.value == default_id) {
            let default_model = models.remove(index);
            models.insert(0, default_model);
        }
    }
    models
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokStatus {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub authenticated: bool,
    pub auth_error: Option<String>,
    pub current_model: Option<String>,
    pub config_path: Option<String>,
    pub models: Vec<CliModelInfo>,
}

/// Safe, non-secret Grok Build settings surfaced by AgentCabin.
///
/// Authentication, model API keys, headers and provider endpoints are deliberately
/// excluded so the CLI settings page never reads secrets out of config.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokCliConfig {
    pub auto_update: Option<bool>,
    pub permission_mode: Option<String>,
    pub screen_mode: Option<String>,
    pub simple_mode: Option<bool>,
    pub vim_mode: Option<bool>,
    pub show_thinking_blocks: Option<bool>,
    pub group_tool_verbs: Option<bool>,
    pub remember_tool_approvals: Option<bool>,
    pub telemetry: Option<bool>,
    pub codebase_indexing: Option<bool>,
    pub remote_fetch: Option<bool>,
    pub auto_compact_threshold_percent: Option<u64>,
    pub load_envrc: Option<bool>,
    pub respect_gitignore: Option<bool>,
}

fn grok_config_bool(config: Option<&toml::Value>, section: &str, key: &str) -> Option<bool> {
    config
        .and_then(|value| value.get(section))
        .and_then(|value| value.get(key))
        .and_then(toml::Value::as_bool)
}

fn grok_config_string(config: Option<&toml::Value>, section: &str, key: &str) -> Option<String> {
    config
        .and_then(|value| value.get(section))
        .and_then(|value| value.get(key))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
}

fn grok_config_integer(config: Option<&toml::Value>, section: &str, key: &str) -> Option<u64> {
    config
        .and_then(|value| value.get(section))
        .and_then(|value| value.get(key))
        .and_then(toml::Value::as_integer)
        .and_then(|value| u64::try_from(value).ok())
}

fn grok_cli_config_from_toml(config: Option<&toml::Value>) -> GrokCliConfig {
    GrokCliConfig {
        auto_update: grok_config_bool(config, "cli", "auto_update"),
        permission_mode: grok_config_string(config, "ui", "permission_mode"),
        screen_mode: grok_config_string(config, "ui", "screen_mode"),
        simple_mode: grok_config_bool(config, "ui", "simple_mode"),
        vim_mode: grok_config_bool(config, "ui", "vim_mode"),
        show_thinking_blocks: grok_config_bool(config, "ui", "show_thinking_blocks"),
        group_tool_verbs: grok_config_bool(config, "ui", "group_tool_verbs"),
        remember_tool_approvals: grok_config_bool(config, "ui", "remember_tool_approvals"),
        telemetry: grok_config_bool(config, "features", "telemetry"),
        codebase_indexing: grok_config_bool(config, "features", "codebase_indexing"),
        remote_fetch: grok_config_bool(config, "features", "remote_fetch"),
        auto_compact_threshold_percent: grok_config_integer(
            config,
            "session",
            "auto_compact_threshold_percent",
        ),
        load_envrc: grok_config_bool(config, "session", "load_envrc"),
        respect_gitignore: grok_config_bool(config, "tools", "respect_gitignore"),
    }
}

fn set_grok_config_value(
    config: &mut toml::Value,
    section: &str,
    key: &str,
    value: toml::Value,
) -> Result<(), String> {
    let root = config
        .as_table_mut()
        .ok_or_else(|| "Grok config root must be a TOML table".to_string())?;
    let section_value = root
        .entry(section.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let section_table = section_value
        .as_table_mut()
        .ok_or_else(|| format!("Grok config section [{section}] must be a TOML table"))?;
    section_table.insert(key.to_string(), value);
    Ok(())
}

fn update_grok_cli_config_value(
    config: &mut toml::Value,
    patch: &GrokCliConfig,
) -> Result<(), String> {
    if let Some(value) = patch.auto_update {
        set_grok_config_value(config, "cli", "auto_update", toml::Value::Boolean(value))?;
    }
    if let Some(value) = patch.permission_mode.as_deref() {
        if !matches!(
            value,
            "ask" | "auto" | "always-approve" | "default" | "plan"
        ) {
            return Err(format!("Unsupported Grok permission mode: {value}"));
        }
        set_grok_config_value(
            config,
            "ui",
            "permission_mode",
            toml::Value::String(value.to_string()),
        )?;
    }
    if let Some(value) = patch.screen_mode.as_deref() {
        if !matches!(value, "fullscreen" | "minimal") {
            return Err(format!("Unsupported Grok screen mode: {value}"));
        }
        set_grok_config_value(
            config,
            "ui",
            "screen_mode",
            toml::Value::String(value.to_string()),
        )?;
    }
    for (section, key, value) in [
        ("ui", "simple_mode", patch.simple_mode),
        ("ui", "vim_mode", patch.vim_mode),
        ("ui", "show_thinking_blocks", patch.show_thinking_blocks),
        ("ui", "group_tool_verbs", patch.group_tool_verbs),
        (
            "ui",
            "remember_tool_approvals",
            patch.remember_tool_approvals,
        ),
        ("features", "telemetry", patch.telemetry),
        ("features", "codebase_indexing", patch.codebase_indexing),
        ("features", "remote_fetch", patch.remote_fetch),
        ("session", "load_envrc", patch.load_envrc),
        ("tools", "respect_gitignore", patch.respect_gitignore),
    ] {
        if let Some(value) = value {
            set_grok_config_value(config, section, key, toml::Value::Boolean(value))?;
        }
    }
    if let Some(value) = patch.auto_compact_threshold_percent {
        if !(50..=99).contains(&value) {
            return Err("Grok auto-compact threshold must be between 50 and 99".to_string());
        }
        set_grok_config_value(
            config,
            "session",
            "auto_compact_threshold_percent",
            toml::Value::Integer(value as i64),
        )?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_grok_cli_config() -> Result<GrokCliConfig, String> {
    Ok(grok_cli_config_from_toml(read_grok_config().as_ref()))
}

#[tauri::command]
pub fn update_grok_cli_config(patch: GrokCliConfig) -> Result<GrokCliConfig, String> {
    let path =
        grok_config_path().ok_or_else(|| "Could not resolve Grok config path".to_string())?;
    let mut config = if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read Grok config: {error}"))?;
        toml::from_str::<toml::Value>(&raw)
            .map_err(|error| format!("Failed to parse Grok config: {error}"))?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    update_grok_cli_config_value(&mut config, &patch)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create Grok config directory: {error}"))?;
    }
    let serialized = toml::to_string_pretty(&config)
        .map_err(|error| format!("Failed to serialize Grok config: {error}"))?;
    std::fs::write(&path, serialized)
        .map_err(|error| format!("Failed to write Grok config: {error}"))?;
    Ok(grok_cli_config_from_toml(Some(&config)))
}

async fn run_grok_models(binary: &str) -> Result<std::process::Output, String> {
    let mut command = tokio::process::Command::new(binary);
    command
        .args(["--no-auto-update", "models"])
        .env("PATH", augmented_path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .hide_console()
        .kill_on_drop(true);

    match tokio::time::timeout(GROK_STATUS_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(format!("Failed to execute Grok Build: {error}")),
        Err(_) => Err(format!(
            "Timed out after {}s waiting for `grok models`",
            GROK_STATUS_TIMEOUT.as_secs()
        )),
    }
}

#[tauri::command]
pub async fn get_cli_info(
    cache: State<'_, CliInfoCache>,
    force_refresh: Option<bool>,
) -> Result<CliInfo, String> {
    log::debug!(
        "[control] get_cli_info IPC, force={}",
        force_refresh.unwrap_or(false)
    );
    match control::get_cli_info(&cache, force_refresh.unwrap_or(false)).await {
        Ok(info) => Ok(info),
        Err(e) => {
            log::warn!(
                "[control] CLI info failed ({}): {}, using fallback",
                e.code,
                e.message
            );
            Ok(control::fallback_cli_info())
        }
    }
}

#[tauri::command]
pub async fn get_codex_models(
    cache: State<'_, CodexInfoCache>,
    force_refresh: Option<bool>,
) -> Result<CodexModelList, String> {
    log::debug!(
        "[control] get_codex_models IPC, force={}",
        force_refresh.unwrap_or(false)
    );
    match codex_control::get_codex_models(&cache, force_refresh.unwrap_or(false)).await {
        Ok(list) => Ok(list),
        Err(e) => {
            log::warn!(
                "[control] codex models failed ({}): {}, using fallback",
                e.code,
                e.message
            );
            Ok(codex_control::fallback_models())
        }
    }
}

/// Fetch Pi's authoritative provider/model catalog before a session exists.
/// `get_available_models` is only available through a live RPC actor, while the
/// chat picker is also rendered before the first prompt. Pi's one-shot
/// `--list-models` command fills that gap and uses the same authenticated Pi
/// installation as the session actor.
#[tauri::command]
pub async fn get_pi_models() -> Result<Vec<CliModelInfo>, String> {
    let settings = crate::storage::settings::get_user_settings();
    if let Some(provider) = settings.pi_provider.as_ref() {
        let allowed_models = settings
            .agent_provider_bindings
            .as_ref()
            .and_then(|bindings| (bindings.pi.mode == "custom").then_some(&bindings.pi.models))
            .and_then(Option::as_deref);
        let models = managed_pi_model_list(provider, allowed_models);
        if !models.is_empty() {
            log::debug!(
                "[control] get_pi_models: using AgentCabin Provider '{}' with {} models",
                provider.id,
                models.len()
            );
            return Ok(models);
        }
        log::debug!(
            "[control] get_pi_models: managed Provider '{}' has no usable models, falling back to Pi CLI",
            provider.id
        );
    }

    let binary = resolve_pi_path();
    let path_env = augmented_path();
    log::debug!("[control] get_pi_models: resolved binary={}", binary);

    let mut command = tokio::process::Command::new(&binary);
    command
        .arg("--list-models")
        .env("PATH", &path_env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .hide_console()
        .kill_on_drop(true);

    let output = match tokio::time::timeout(PI_MODEL_LIST_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            log::debug!("[control] get_pi_models: failed to execute Pi: {}", error);
            return Err(format!("Failed to execute Pi: {}", error));
        }
        Err(_) => {
            log::debug!("[control] get_pi_models: Pi --list-models timed out");
            return Err(format!(
                "Timed out after {}s waiting for Pi model catalog",
                PI_MODEL_LIST_TIMEOUT.as_secs()
            ));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let models = parse_pi_model_list(&format!("{}\n{}", stdout, stderr));
    if !output.status.success() {
        log::debug!(
            "[control] get_pi_models: Pi exited with status {:?}, parsed {} models",
            output.status.code(),
            models.len()
        );
        return Err(format!(
            "Pi --list-models exited with status {}",
            output.status
        ));
    }
    if models.is_empty() {
        log::debug!("[control] get_pi_models: Pi returned an empty model catalog");
        return Err("Pi returned an empty model catalog".to_string());
    }

    log::debug!("[control] get_pi_models: loaded {} models", models.len());
    Ok(models)
}

/// Grok Build readiness and model catalog for settings/onboarding. Credential readiness is
/// derived from configured credential sources without reading secret values; `grok models` is
/// used only for catalog discovery because it is not a reliable login-status API.
#[tauri::command]
pub async fn get_grok_status() -> Result<GrokStatus, String> {
    let config = read_grok_config();
    let credentials_available = grok_credentials_available(config.as_ref());
    let binary = resolve_grok_path();
    let resolved = if binary.contains('/') || binary.contains('\\') {
        Path::new(&binary).is_file().then(|| binary.clone())
    } else {
        which_binary(&binary)
    };
    let Some(path) = resolved else {
        return Ok(GrokStatus {
            found: false,
            path: None,
            version: None,
            authenticated: false,
            auth_error: None,
            current_model: grok_default_model_from_config(),
            config_path: grok_config_path().map(|path| path.display().to_string()),
            models: grok_config_models(),
        });
    };

    let version = {
        let mut command = tokio::process::Command::new(&path);
        command
            .arg("--version")
            .env("PATH", augmented_path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .hide_console()
            .kill_on_drop(true);
        match tokio::time::timeout(GROK_STATUS_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let raw = if stdout.is_empty() { stderr } else { stdout };
                (!raw.is_empty()).then_some(raw)
            }
            _ => None,
        }
    };

    let models_output = run_grok_models(&path).await;
    let (model_error, models) = match models_output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            let models = parse_grok_model_list(&combined);
            if output.status.success() {
                (None, models)
            } else {
                let detail = if stderr.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    stderr.trim().to_string()
                };
                (
                    Some(if detail.is_empty() {
                        format!("`grok models` exited with status {}", output.status)
                    } else {
                        detail
                    }),
                    models,
                )
            }
        }
        Err(error) => (Some(error), grok_config_models()),
    };
    let auth_error = if credentials_available {
        model_error
    } else {
        Some(
            "No Grok Build credentials detected. Run `grok login`, set XAI_API_KEY, or configure a model api_key/env_key."
                .to_string(),
        )
    };

    Ok(GrokStatus {
        found: true,
        path: Some(path),
        version,
        authenticated: credentials_available,
        auth_error,
        current_model: grok_default_model_from_config(),
        config_path: grok_config_path().map(|path| path.display().to_string()),
        models,
    })
}

#[tauri::command]
pub async fn get_grok_models() -> Result<Vec<CliModelInfo>, String> {
    let status = get_grok_status().await?;
    if !status.found {
        return Err("Grok Build CLI is not installed or not discoverable".to_string());
    }
    // Model discovery is useful before login (and powers the composer default).
    // Only fail when the CLI could not provide any catalog at all; authentication
    // remains a separate readiness concern surfaced by GrokStatus.
    if status.models.is_empty() && !status.authenticated {
        return Err(status
            .auth_error
            .unwrap_or_else(|| "Grok Build is not authenticated; run `grok login`".to_string()));
    }
    Ok(status.models)
}

#[cfg(test)]
mod tests {
    use super::{
        managed_pi_effort_levels, managed_pi_model_list, parse_grok_model_list, parse_pi_model_list,
    };
    use crate::models::{GlobalProviderModel, PiProviderCredential};

    #[test]
    fn parse_pi_model_table_maps_provider_and_reasoning_metadata() {
        let output = r#"
provider      model                context  max-out  thinking  images
openai-codex  gpt-5.5              272K     128K     yes       yes
openai-codex  gpt-5.6-sol          272K     128K     yes       yes
openai-codex  gpt-5.4-mini         272K     128K     no        yes
"#;

        let models = parse_pi_model_list(output);
        assert_eq!(
            models
                .iter()
                .map(|model| model.value.as_str())
                .collect::<Vec<_>>(),
            vec![
                "openai-codex/gpt-5.5",
                "openai-codex/gpt-5.6-sol",
                "openai-codex/gpt-5.4-mini",
            ]
        );
        assert_eq!(models[0].supports_effort, Some(true));
        assert_eq!(models[2].supports_effort, Some(false));
        assert_eq!(models[0].supported_effort_levels.as_ref().unwrap().len(), 6);
        assert_eq!(
            serde_json::to_value(&models[0]).unwrap()["contextWindow"],
            serde_json::json!(272000)
        );
    }

    #[test]
    fn parse_pi_model_table_ignores_banner_and_duplicate_rows() {
        let output = r#"Pi 0.83.0
provider model context max-out thinking images
openai-codex gpt-5.5 272K 128K yes yes
openai-codex gpt-5.5 272K 128K yes yes
"#;

        let models = parse_pi_model_list(output);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].display_name, "gpt-5.5");
    }

    #[test]
    fn managed_pi_provider_catalog_uses_agentcabin_models() {
        let provider = PiProviderCredential {
            id: "newapi-openai".into(),
            name: "NewAPI-OpenAI".into(),
            base_url: "http://127.0.0.1:3000/v1".into(),
            api: "openai-completions".into(),
            model: "glm-5.2".into(),
            models: vec![
                GlobalProviderModel {
                    id: "deepseek/deepseek-v4-flash".into(),
                    name: None,
                    context_window: None,
                    max_tokens: None,
                    supports_reasoning: Some(true),
                    supports_xhigh: None,
                    supported_effort_levels: None,
                    supports_images: None,
                },
                GlobalProviderModel {
                    id: "glm-5.2".into(),
                    name: Some("GLM 5.2".into()),
                    context_window: Some(131072),
                    max_tokens: None,
                    supports_reasoning: Some(true),
                    supports_xhigh: None,
                    supported_effort_levels: None,
                    supports_images: None,
                },
            ],
            context_window: Some(131072),
            api_key: Some("secret".into()),
        };

        let models = managed_pi_model_list(&provider, None);
        assert_eq!(
            models
                .iter()
                .map(|model| model.value.as_str())
                .collect::<Vec<_>>(),
            vec!["deepseek/deepseek-v4-flash", "glm-5.2"]
        );
        assert_eq!(models[1].display_name, "GLM 5.2");
        assert_eq!(models[1].context_window, Some(131072));
        assert_eq!(models[0].description, "NewAPI-OpenAI");
    }

    #[test]
    fn managed_subscription_catalog_uses_effective_codex_context_when_missing() {
        let provider = PiProviderCredential {
            id: crate::agent::codex_subscription_bridge::CODEX_SUBSCRIPTION_PROVIDER_ID.into(),
            name: "OpenAI / ChatGPT 订阅".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            api: "openai-responses".into(),
            model: "gpt-5.6-luna".into(),
            models: vec![GlobalProviderModel {
                id: "gpt-5.6-luna".into(),
                name: None,
                context_window: None,
                max_tokens: None,
                supports_reasoning: Some(true),
                supports_xhigh: None,
                supported_effort_levels: None,
                supports_images: None,
            }],
            context_window: None,
            api_key: None,
        };

        let models = managed_pi_model_list(&provider, None);

        assert_eq!(models[0].context_window, Some(258400));
    }

    #[test]
    fn managed_pi_effort_levels_are_model_specific() {
        let standard = GlobalProviderModel {
            id: "standard".into(),
            name: None,
            context_window: None,
            max_tokens: None,
            supports_reasoning: Some(true),
            supports_xhigh: None,
            supported_effort_levels: None,
            supports_images: None,
        };
        let xhigh = GlobalProviderModel {
            id: "xhigh".into(),
            name: None,
            context_window: None,
            max_tokens: None,
            supports_reasoning: Some(true),
            supports_xhigh: Some(true),
            supported_effort_levels: None,
            supports_images: None,
        };
        assert_eq!(
            managed_pi_effort_levels(&standard),
            vec!["low", "medium", "high"]
        );
        assert_eq!(
            managed_pi_effort_levels(&xhigh),
            vec!["low", "medium", "high", "xhigh"]
        );

        let configured = GlobalProviderModel {
            supported_effort_levels: Some(vec!["low".into(), "medium".into(), "xhigh".into()]),
            ..standard.clone()
        };
        assert_eq!(
            managed_pi_effort_levels(&configured),
            vec!["low", "medium", "xhigh"]
        );

        let configured_with_off = GlobalProviderModel {
            supported_effort_levels: Some(vec![
                "off".into(),
                "low".into(),
                "high".into(),
                "max".into(),
            ]),
            ..standard
        };
        assert_eq!(
            managed_pi_effort_levels(&configured_with_off),
            vec!["off", "low", "high", "max"]
        );
    }

    #[test]
    fn parse_grok_models_accepts_plain_and_marked_rows() {
        let output = r#"
Available models
* grok-build Grok Build
  grok-4.5 Grok 4.5
> custom-coder Custom Coder
"#;
        let models = parse_grok_model_list(output);
        let ids = models
            .iter()
            .map(|model| model.value.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"grok-build"));
        assert!(ids.contains(&"grok-4.5"));
        assert!(ids.contains(&"custom-coder"));
    }

    #[test]
    fn parse_grok_models_handles_official_default_output() {
        let output = r#"
Default model: grok-4.5

Available models:
  * grok-4.5 (default)
"#;

        let models = parse_grok_model_list(output);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].value, "grok-4.5");
        assert_eq!(models[0].display_name, "grok-4.5");
    }

    #[test]
    fn parse_grok_models_ignores_auth_diagnostics() {
        let output = r#"
2026-08-08T05:26:03.584924Z WARN Failed to fetch models: Auth("No auth credentials")
2026-08-08T05:26:03.586421Z WARN model refresh failed
Default model: grok-4.5
Available models:
  * grok-4.5 (default)
"#;

        let models = parse_grok_model_list(output);
        assert_eq!(
            models
                .iter()
                .map(|model| model.value.as_str())
                .collect::<Vec<_>>(),
            ["grok-4.5"]
        );
    }
}
