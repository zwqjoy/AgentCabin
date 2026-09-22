//! Codex plugin marketplace and lifecycle commands.
//!
//! Codex plugins are managed by the Codex CLI rather than by the Claude
//! plugin command surface. Keep the CLI adapter here so the UI can use the
//! same safe, JSON-oriented operations without depending on Codex's cache
//! layout or editing its plugin metadata directly.

use crate::models::{CodexMarketplace, CodexPluginInfo, InstalledPlugin, PluginOperationResult};
use crate::process_ext::HideConsole;
use serde_json::{Deserializer, Value};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const CODEX_PLUGIN_CMD_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug)]
struct CodexCommandResult {
    success: bool,
    stdout: String,
    stderr: String,
}

fn command_message(result: &CodexCommandResult) -> String {
    let stdout = result.stdout.trim();
    let stderr = result.stderr.trim();
    if result.success {
        if !stdout.is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        }
    } else if !stderr.is_empty() {
        stderr.to_string()
    } else {
        stdout.to_string()
    }
}

fn operation_result(result: CodexCommandResult) -> PluginOperationResult {
    let success = result.success;
    let message = command_message(&result);
    PluginOperationResult { success, message }
}

/// Run a non-interactive `codex plugin ...` command.
///
/// JSON mode is requested by each caller. Authentication that cannot be
/// completed non-interactively is returned as a normal CLI error so the UI
/// can show the user the next action instead of hanging on a hidden prompt.
async fn run_codex_plugin_command(args: &[String]) -> Result<CodexCommandResult, String> {
    let binary = crate::agent::claude_stream::resolve_codex_path();
    let path_env = crate::agent::claude_stream::augmented_path();

    log::debug!("[codex_plugins] run: {} {}", binary, args.join(" "));

    let mut command = Command::new(&binary);
    command
        .args(args)
        .env("PATH", &path_env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .hide_console()
        .kill_on_drop(true);

    let child = command.spawn().map_err(|error| {
        log::error!("[codex_plugins] failed to spawn codex: {}", error);
        format!("Failed to start Codex CLI: {}", error)
    })?;

    let output = timeout(CODEX_PLUGIN_CMD_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            format!(
                "Codex plugin command timed out after {} seconds",
                CODEX_PLUGIN_CMD_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| format!("Codex plugin command failed: {}", error))?;

    let result = CodexCommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    log::debug!(
        "[codex_plugins] completed: success={}, stdout_len={}, stderr_len={}",
        result.success,
        result.stdout.len(),
        result.stderr.len()
    );
    if !result.success {
        log::debug!(
            "[codex_plugins] stderr: {}",
            result.stderr.trim().chars().take(500).collect::<String>()
        );
    }

    Ok(result)
}

/// Parse the first JSON value from CLI output. This tolerates a diagnostic
/// line before the JSON payload while still requiring a valid object/array.
fn parse_json_output(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Codex CLI returned empty JSON output".to_string());
    }

    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }

    for (offset, ch) in raw.char_indices() {
        if ch != '{' && ch != '[' {
            continue;
        }
        let mut stream = Deserializer::from_str(&raw[offset..]).into_iter::<Value>();
        if let Some(Ok(value)) = stream.next() {
            return Ok(value);
        }
    }

    Err("Failed to parse Codex CLI JSON output".to_string())
}

fn json_array<'a>(payload: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Codex CLI JSON is missing the '{}' array", key))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn plugin_description(value: &Value) -> String {
    string_field(value, "description")
        .or_else(|| {
            value
                .get("interface")
                .and_then(|interface| string_field(interface, "shortDescription"))
        })
        .unwrap_or_default()
}

fn codex_plugin_info(value: &Value) -> Option<CodexPluginInfo> {
    let name = string_field(value, "name")?;
    let marketplace_name = string_field(value, "marketplaceName");
    let plugin_id = string_field(value, "pluginId").or_else(|| {
        marketplace_name
            .as_ref()
            .map(|marketplace| format!("{}@{}", name, marketplace))
    })?;

    Some(CodexPluginInfo {
        plugin_id,
        name,
        description: plugin_description(value),
        version: string_field(value, "version"),
        marketplace_name,
        installed: value
            .get("installed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        enabled: value.get("enabled").and_then(Value::as_bool),
        install_policy: string_field(value, "installPolicy"),
        auth_policy: string_field(value, "authPolicy"),
        source: value.get("source").cloned(),
        marketplace_source: value.get("marketplaceSource").cloned(),
    })
}

/// Map Codex's installed entry into the shared installed-plugin model used by
/// the existing enable/disable list UI.
fn installed_plugin(value: &Value) -> Option<InstalledPlugin> {
    let info = codex_plugin_info(value)?;
    Some(InstalledPlugin {
        name: info.name,
        description: info.description,
        version: info.version,
        // Codex plugin installation is managed in CODEX_HOME, so it is a
        // user-level entry rather than a Claude project/local scope.
        scope: Some("user".to_string()),
        enabled: info.enabled,
        marketplace: info.marketplace_name,
        plugin_id: Some(info.plugin_id),
        agent: Some("codex".to_string()),
        extra: [
            (
                "installPolicy".to_string(),
                info.install_policy.map_or(Value::Null, Value::String),
            ),
            (
                "authPolicy".to_string(),
                info.auth_policy.map_or(Value::Null, Value::String),
            ),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect(),
        ..Default::default()
    })
}

fn codex_marketplace(value: &Value) -> Option<CodexMarketplace> {
    Some(CodexMarketplace {
        name: string_field(value, "name")?,
        root: string_field(value, "root").unwrap_or_default(),
        marketplace_source: value.get("marketplaceSource").cloned(),
    })
}

fn validate_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty() || plugin_id.len() > 512 || plugin_id.starts_with('-') {
        return Err("Invalid Codex plugin ID".to_string());
    }
    if plugin_id
        .chars()
        .any(|ch| !ch.is_ascii_alphanumeric() && !matches!(ch, '-' | '_' | '.' | '@'))
    {
        return Err("Codex plugin ID contains unsupported characters".to_string());
    }
    if let Some((name, marketplace)) = plugin_id.split_once('@') {
        if name.is_empty() || marketplace.is_empty() || marketplace.contains('@') {
            return Err(
                "Invalid Codex plugin ID: empty or repeated marketplace separator".to_string(),
            );
        }
    }
    Ok(())
}

fn validate_marketplace_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 256 || name.starts_with('-') {
        return Err("Invalid Codex marketplace name".to_string());
    }
    if name
        .chars()
        .any(|ch| !ch.is_ascii_alphanumeric() && !matches!(ch, '-' | '_' | '.'))
    {
        return Err("Codex marketplace name contains unsupported characters".to_string());
    }
    Ok(())
}

fn validate_marketplace_source(source: &str) -> Result<(), String> {
    if source.starts_with('-') {
        return Err("Marketplace source cannot start with '-'".to_string());
    }
    crate::storage::plugins::validate_marketplace_source(source)
}

fn plugin_args(action: &str, plugin_id: &str, marketplace: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "plugin".to_string(),
        action.to_string(),
        plugin_id.to_string(),
    ];
    // Codex expects --marketplace only when the plugin argument does not
    // already carry the `name@marketplace` suffix.
    if !plugin_id.contains('@') {
        if let Some(marketplace) = marketplace {
            args.extend(["--marketplace".to_string(), marketplace.to_string()]);
        }
    }
    args.push("--json".to_string());
    args
}

#[tauri::command]
pub async fn list_codex_installed_plugins() -> Result<Vec<InstalledPlugin>, String> {
    log::debug!("[codex_plugins] list installed");
    let args = vec![
        "plugin".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    match run_codex_plugin_command(&args).await {
        Ok(result) if result.success => match parse_json_output(&result.stdout)
            .and_then(|payload| json_array(&payload, "installed").cloned())
        {
            Ok(entries) => Ok(entries.iter().filter_map(installed_plugin).collect()),
            Err(error) => {
                log::warn!("[codex_plugins] installed list parse failed: {}", error);
                Ok(crate::storage::plugins::list_codex_installed_plugins())
            }
        },
        Ok(result) => {
            log::debug!(
                "[codex_plugins] installed list unavailable: {}",
                command_message(&result)
            );
            Ok(crate::storage::plugins::list_codex_installed_plugins())
        }
        Err(error) => {
            log::debug!("[codex_plugins] installed list unavailable: {}", error);
            Ok(crate::storage::plugins::list_codex_installed_plugins())
        }
    }
}

#[tauri::command]
pub async fn list_codex_available_plugins() -> Result<Vec<CodexPluginInfo>, String> {
    log::debug!("[codex_plugins] list available");
    let args = vec![
        "plugin".to_string(),
        "list".to_string(),
        "--available".to_string(),
        "--json".to_string(),
    ];
    let result = run_codex_plugin_command(&args).await?;
    if !result.success {
        return Err(command_message(&result));
    }
    let payload = parse_json_output(&result.stdout)?;
    Ok(json_array(&payload, "available")?
        .iter()
        .filter_map(codex_plugin_info)
        .collect())
}

#[tauri::command]
pub async fn list_codex_marketplaces() -> Result<Vec<CodexMarketplace>, String> {
    log::debug!("[codex_plugins] list marketplaces");
    let args = vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let result = run_codex_plugin_command(&args).await?;
    if !result.success {
        return Err(command_message(&result));
    }
    let payload = parse_json_output(&result.stdout)?;
    Ok(json_array(&payload, "marketplaces")?
        .iter()
        .filter_map(codex_marketplace)
        .collect())
}

#[tauri::command]
pub async fn add_codex_marketplace(source: String) -> Result<PluginOperationResult, String> {
    let source = source.trim();
    validate_marketplace_source(source)?;
    let args = vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "add".to_string(),
        source.to_string(),
        "--json".to_string(),
    ];
    Ok(operation_result(run_codex_plugin_command(&args).await?))
}

#[tauri::command]
pub async fn remove_codex_marketplace(name: String) -> Result<PluginOperationResult, String> {
    let name = name.trim();
    validate_marketplace_name(name)?;
    let args = vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "remove".to_string(),
        name.to_string(),
        "--json".to_string(),
    ];
    Ok(operation_result(run_codex_plugin_command(&args).await?))
}

#[tauri::command]
pub async fn upgrade_codex_marketplace(
    name: Option<String>,
) -> Result<PluginOperationResult, String> {
    let name = name.map(|value| value.trim().to_string());
    if let Some(name) = name.as_deref() {
        validate_marketplace_name(name)?;
    }
    let mut args = vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "upgrade".to_string(),
    ];
    if let Some(name) = name {
        if !name.is_empty() {
            args.push(name);
        }
    }
    args.push("--json".to_string());
    Ok(operation_result(run_codex_plugin_command(&args).await?))
}

#[tauri::command]
pub async fn install_codex_plugin(
    plugin_id: String,
    marketplace: Option<String>,
) -> Result<PluginOperationResult, String> {
    let plugin_id = plugin_id.trim();
    validate_plugin_id(plugin_id)?;
    let marketplace = marketplace
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if !plugin_id.contains('@') && marketplace.is_none() {
        return Err("Marketplace is required when plugin ID omits @marketplace".to_string());
    }
    if let Some(name) = marketplace.as_deref() {
        validate_marketplace_name(name)?;
    }
    Ok(operation_result(
        run_codex_plugin_command(&plugin_args("add", plugin_id, marketplace.as_deref())).await?,
    ))
}

#[tauri::command]
pub async fn uninstall_codex_plugin(
    plugin_id: String,
    marketplace: Option<String>,
) -> Result<PluginOperationResult, String> {
    let plugin_id = plugin_id.trim();
    validate_plugin_id(plugin_id)?;
    let marketplace = marketplace
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if !plugin_id.contains('@') && marketplace.is_none() {
        return Err("Marketplace is required when plugin ID omits @marketplace".to_string());
    }
    if let Some(name) = marketplace.as_deref() {
        validate_marketplace_name(name)?;
    }
    Ok(operation_result(
        run_codex_plugin_command(&plugin_args("remove", plugin_id, marketplace.as_deref())).await?,
    ))
}

#[tauri::command]
pub fn toggle_codex_plugin(plugin_id: String, enabled: bool) -> Result<(), String> {
    log::debug!("[codex_plugins] toggle: {}={}", plugin_id, enabled);
    crate::storage::plugins::toggle_codex_plugin(&plugin_id, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_json_plugin_maps_cli_entry() {
        let value = serde_json::json!({
            "pluginId": "github@openai-curated",
            "name": "github",
            "marketplaceName": "openai-curated",
            "version": "2abb1c44",
            "installed": true,
            "enabled": true,
            "description": "GitHub tools",
            "source": { "source": "local", "path": "/x/.codex/plugins/github" },
            "installPolicy": "AVAILABLE",
            "authPolicy": "ON_INSTALL"
        });

        let info = codex_plugin_info(&value).expect("maps");
        assert_eq!(info.plugin_id, "github@openai-curated");
        assert_eq!(info.description, "GitHub tools");
        assert!(info.installed);

        let installed = installed_plugin(&value).expect("installed maps");
        assert_eq!(installed.name, "github");
        assert_eq!(
            installed.plugin_id.as_deref(),
            Some("github@openai-curated")
        );
        assert_eq!(installed.marketplace.as_deref(), Some("openai-curated"));
        assert_eq!(installed.scope.as_deref(), Some("user"));
        assert_eq!(installed.agent.as_deref(), Some("codex"));
    }

    #[test]
    fn parses_available_and_marketplace_arrays() {
        let payload = serde_json::json!({
            "available": [{
                "pluginId": "docs@team",
                "name": "docs",
                "marketplaceName": "team",
                "installed": false,
                "interface": { "shortDescription": "Documentation tools" }
            }],
            "marketplaces": [{
                "name": "team",
                "root": "/tmp/team",
                "marketplaceSource": { "source": "local", "value": "/tmp/team" }
            }]
        });
        let available = json_array(&payload, "available").unwrap();
        let info = codex_plugin_info(&available[0]).unwrap();
        assert_eq!(info.description, "Documentation tools");
        assert!(!info.installed);

        let marketplaces = json_array(&payload, "marketplaces").unwrap();
        let marketplace = codex_marketplace(&marketplaces[0]).unwrap();
        assert_eq!(marketplace.name, "team");
        assert_eq!(marketplace.root, "/tmp/team");
    }

    #[test]
    fn validates_plugin_and_marketplace_identifiers() {
        assert!(validate_plugin_id("github@openai-curated").is_ok());
        assert!(validate_plugin_id("github").is_ok());
        assert!(validate_plugin_id("github@openai curated").is_err());
        assert!(validate_marketplace_name("openai-curated").is_ok());
        assert!(validate_marketplace_name("openai curated").is_err());
    }

    #[test]
    fn builds_cli_arguments_without_shell_interpolation() {
        assert_eq!(
            plugin_args("add", "github@openai-curated", Some("openai-curated")),
            vec!["plugin", "add", "github@openai-curated", "--json"]
        );
        assert_eq!(
            plugin_args("add", "github", Some("openai-curated")),
            vec![
                "plugin",
                "add",
                "github",
                "--marketplace",
                "openai-curated",
                "--json"
            ]
        );
    }
}
