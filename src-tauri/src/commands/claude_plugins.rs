//! Claude Code plugin marketplace and lifecycle adapter.
//!
//! Claude Code owns marketplace registration, plugin discovery, and plugin
//! lifecycle state. Keep the UI backed by Claude's JSON CLI contract instead
//! of assuming that `known_marketplaces.json` is always materialized.

use crate::models::{
    InstalledPlugin, MarketplaceInfo, MarketplacePlugin, PluginAuthor, PluginComponents,
    PluginOperationResult,
};
use crate::storage::plugins::PluginCommandResult;
use serde_json::{Deserializer, Value};
use std::collections::HashMap;

const OFFICIAL_MARKETPLACE_NAME: &str = "claude-plugins-official";
const OFFICIAL_MARKETPLACE_SOURCE: &str = "anthropics/claude-plugins-official";

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
}

fn usize_field(value: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .and_then(|count| usize::try_from(count).ok())
}

fn description_field(value: &Value) -> String {
    string_field(value, &["description", "shortDescription"])
        .or_else(|| {
            value
                .get("interface")
                .and_then(|interface| string_field(interface, &["shortDescription", "description"]))
        })
        .unwrap_or_default()
}

fn plugin_id(value: &Value) -> Option<String> {
    string_field(value, &["id", "pluginId", "plugin_id"])
}

fn marketplace_name(value: &Value, id: Option<&str>) -> Option<String> {
    string_field(
        value,
        &[
            "marketplace",
            "marketplaceName",
            "sourceMarketplace",
            "marketplace_name",
        ],
    )
    .or_else(|| {
        id.and_then(|id| {
            id.rsplit_once('@')
                .map(|(_, marketplace)| marketplace.to_string())
        })
    })
}

fn bare_plugin_name(value: &Value, id: Option<&str>) -> Option<String> {
    string_field(value, &["name", "pluginName", "plugin_name"]).or_else(|| {
        id.map(|id| {
            id.split_once('@')
                .map(|(name, _)| name)
                .unwrap_or(id)
                .to_string()
        })
    })
}

fn parse_json_output(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Claude CLI returned empty JSON output".to_string());
    }

    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }

    // Be tolerant of a diagnostic line before the JSON payload.
    for (offset, ch) in raw.char_indices() {
        if ch != '{' && ch != '[' {
            continue;
        }
        let mut stream = Deserializer::from_str(&raw[offset..]).into_iter::<Value>();
        if let Some(Ok(value)) = stream.next() {
            return Ok(value);
        }
    }

    Err("Failed to parse Claude CLI JSON output".to_string())
}

fn payload_array<'a>(payload: &'a Value, key: &str) -> Vec<&'a Value> {
    if let Some(entries) = payload.as_array() {
        return entries.iter().collect();
    }
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|entries| entries.iter().collect())
        .unwrap_or_default()
}

fn command_message(result: &PluginCommandResult) -> String {
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

fn operation_result(result: PluginCommandResult) -> PluginOperationResult {
    let success = result.success;
    let message = command_message(&result);
    PluginOperationResult { success, message }
}

fn author(value: &Value) -> Option<PluginAuthor> {
    let author = value.get("author")?;
    if let Some(name) = author.as_str() {
        return Some(PluginAuthor {
            name: name.to_string(),
            email: None,
        });
    }
    let name = author.get("name").and_then(Value::as_str)?.to_string();
    Some(PluginAuthor {
        name,
        email: author
            .get("email")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn string_array(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn available_plugin(value: &Value) -> Option<MarketplacePlugin> {
    let id = plugin_id(value);
    let name = bare_plugin_name(value, id.as_deref())?;
    let marketplace_name = marketplace_name(value, id.as_deref());

    Some(MarketplacePlugin {
        name,
        description: description_field(value),
        version: string_field(value, &["version"]),
        author: author(value),
        category: string_field(value, &["category"]),
        homepage: string_field(value, &["homepage", "homepageUrl"]),
        source: value.get("source").cloned(),
        tags: string_array(value, &["tags", "keywords"]),
        strict: bool_field(value, &["strict"]),
        lsp_servers: value
            .get("lspServers")
            .or_else(|| value.get("lsp_servers"))
            .cloned(),
        marketplace_name,
        install_count: value
            .get("installCount")
            .or_else(|| value.get("install_count"))
            .and_then(Value::as_u64),
        components: PluginComponents::default(),
    })
}

fn installed_plugin(value: &Value) -> Option<InstalledPlugin> {
    let id = plugin_id(value);
    let name = bare_plugin_name(value, id.as_deref())?;
    let marketplace = marketplace_name(value, id.as_deref());
    let plugin_id = id.or_else(|| {
        marketplace
            .as_ref()
            .map(|marketplace| format!("{}@{}", name, marketplace))
    });

    let mut extra = HashMap::new();
    if let Some(source) = value.get("source").cloned() {
        extra.insert("source".to_string(), source);
    }
    if let Some(error) = value.get("error").cloned() {
        extra.insert("error".to_string(), error);
    }
    // Claude CLI versions have used more than one name for the installed
    // plugin root. Normalize the value so the Grok integration can consume
    // it without coupling the UI to a CLI-version-specific field name.
    if let Some(install_location) = string_field(
        value,
        &[
            "installLocation",
            "install_location",
            "installPath",
            "path",
            "root",
        ],
    ) {
        extra.insert(
            "installLocation".to_string(),
            Value::String(install_location),
        );
    }

    Some(InstalledPlugin {
        name,
        description: description_field(value),
        version: string_field(value, &["version"]),
        scope: string_field(value, &["scope"]),
        enabled: bool_field(value, &["enabled", "isEnabled"]),
        marketplace,
        plugin_id,
        agent: Some("claude".to_string()),
        project_path: string_field(value, &["projectPath", "project_path"]),
        extra: extra.into_iter().collect(),
    })
}

fn marketplace_info(value: &Value) -> Option<MarketplaceInfo> {
    let name = string_field(value, &["name", "marketplaceName"])?;
    let builtin = name == OFFICIAL_MARKETPLACE_NAME;
    let source = value
        .get("source")
        .or_else(|| value.get("marketplaceSource"))
        .cloned()
        .unwrap_or_else(|| {
            if builtin {
                serde_json::json!({
                    "source": "github",
                    "repo": OFFICIAL_MARKETPLACE_SOURCE
                })
            } else {
                Value::Null
            }
        });

    let plugin_count = usize_field(value, &["pluginCount", "plugin_count"]).unwrap_or_else(|| {
        value
            .get("plugins")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    });

    Some(MarketplaceInfo {
        name,
        source,
        install_location: string_field(value, &["installLocation", "install_location", "root"])
            .unwrap_or_default(),
        last_updated: string_field(value, &["lastUpdated", "last_updated"]),
        plugin_count,
        builtin,
        display_name: string_field(value, &["displayName", "display_name"])
            .or_else(|| builtin.then(|| "Anthropic Official".to_string())),
    })
}

fn official_marketplace_info() -> MarketplaceInfo {
    MarketplaceInfo {
        name: OFFICIAL_MARKETPLACE_NAME.to_string(),
        source: serde_json::json!({
            "source": "github",
            "repo": OFFICIAL_MARKETPLACE_SOURCE
        }),
        install_location: String::new(),
        last_updated: None,
        plugin_count: 0,
        builtin: true,
        display_name: Some("Anthropic Official".to_string()),
    }
}

fn merge_marketplaces(cli: Vec<MarketplaceInfo>) -> Vec<MarketplaceInfo> {
    let mut by_name = cli
        .into_iter()
        .map(|marketplace| (marketplace.name.clone(), marketplace))
        .collect::<HashMap<_, _>>();

    // The official marketplace is available in Claude Code even when an older
    // CLI has not materialized known_marketplaces.json yet.
    by_name
        .entry(OFFICIAL_MARKETPLACE_NAME.to_string())
        .or_insert_with(official_marketplace_info);

    let mut marketplaces: Vec<_> = by_name.into_values().collect();
    marketplaces.sort_by(|a, b| b.builtin.cmp(&a.builtin).then_with(|| a.name.cmp(&b.name)));
    marketplaces
}

fn has_official_marketplace(payload: &Value) -> bool {
    payload_array(payload, "marketplaces")
        .into_iter()
        .any(|entry| {
            string_field(entry, &["name", "marketplaceName"]).as_deref()
                == Some(OFFICIAL_MARKETPLACE_NAME)
        })
}

fn plugin_reference(plugin: &InstalledPlugin) -> String {
    if let Some(plugin_id) = plugin.plugin_id.as_deref() {
        return plugin_id.to_string();
    }
    if let Some(marketplace) = plugin.marketplace.as_deref() {
        return format!("{}@{}", plugin.name, marketplace);
    }
    plugin.name.clone()
}

/// List Claude Code plugins installed across user/project/local scopes.
#[tauri::command]
pub async fn list_claude_installed_plugins() -> Result<Vec<InstalledPlugin>, String> {
    let result = crate::storage::plugins::run_plugin_command(&["list", "--json"], None).await?;
    if !result.success {
        return Err(command_message(&result));
    }
    let payload = parse_json_output(&result.stdout)?;
    Ok(payload_array(&payload, "installed")
        .into_iter()
        .filter_map(installed_plugin)
        .collect())
}

/// List plugins available from Claude Code marketplaces.
#[tauri::command]
pub async fn list_claude_available_plugins() -> Result<Vec<MarketplacePlugin>, String> {
    let result =
        crate::storage::plugins::run_plugin_command(&["list", "--available", "--json"], None)
            .await?;
    if !result.success {
        return Err(command_message(&result));
    }
    let payload = parse_json_output(&result.stdout)?;
    Ok(payload_array(&payload, "available")
        .into_iter()
        .filter_map(available_plugin)
        .collect())
}

/// List Claude Code marketplaces, always including the official Anthropic
/// marketplace as a built-in entry.
#[tauri::command]
pub async fn list_claude_marketplaces() -> Result<Vec<MarketplaceInfo>, String> {
    let result =
        crate::storage::plugins::run_plugin_command(&["marketplace", "list", "--json"], None).await;

    let cli_marketplaces = match result {
        Ok(result) if result.success => {
            let payload = parse_json_output(&result.stdout)?;
            payload_array(&payload, "marketplaces")
                .into_iter()
                .filter_map(marketplace_info)
                .collect()
        }
        Ok(result) => {
            log::warn!(
                "[claude_plugins] marketplace list failed: {}",
                command_message(&result)
            );
            Vec::new()
        }
        Err(error) => {
            log::warn!("[claude_plugins] marketplace list unavailable: {}", error);
            Vec::new()
        }
    };

    let mut merged = merge_marketplaces(cli_marketplaces);
    for marketplace in crate::storage::plugins::list_marketplaces() {
        if !merged.iter().any(|item| item.name == marketplace.name) {
            merged.push(marketplace);
        }
    }
    merged.sort_by(|a, b| b.builtin.cmp(&a.builtin).then_with(|| a.name.cmp(&b.name)));
    Ok(merged)
}

async fn list_claude_marketplace_payload() -> Result<Value, String> {
    let listed =
        crate::storage::plugins::run_plugin_command(&["marketplace", "list", "--json"], None)
            .await?;
    if !listed.success {
        return Err(command_message(&listed));
    }
    parse_json_output(&listed.stdout)
}

/// Ensure the official Anthropic marketplace is registered without forcing a
/// network update every time the desktop page is opened.
#[tauri::command]
pub async fn ensure_claude_official_marketplace() -> Result<PluginOperationResult, String> {
    let payload = list_claude_marketplace_payload().await?;
    if has_official_marketplace(&payload) {
        return Ok(PluginOperationResult {
            success: true,
            message: "Claude official marketplace is already registered".to_string(),
        });
    }

    let result = crate::storage::plugins::run_plugin_command(
        &["marketplace", "add", OFFICIAL_MARKETPLACE_SOURCE],
        None,
    )
    .await?;
    Ok(operation_result(result))
}

/// Ensure the official Anthropic marketplace is registered and refresh it.
/// This is used by the explicit Refresh action in the UI.
#[tauri::command]
pub async fn sync_claude_official_marketplace() -> Result<PluginOperationResult, String> {
    let listed =
        crate::storage::plugins::run_plugin_command(&["marketplace", "list", "--json"], None)
            .await?;

    let has_official = listed.success
        && parse_json_output(&listed.stdout)
            .ok()
            .map(|payload| has_official_marketplace(&payload))
            .unwrap_or(false);

    let result = if has_official {
        crate::storage::plugins::run_plugin_command(
            &["marketplace", "update", OFFICIAL_MARKETPLACE_NAME],
            None,
        )
        .await?
    } else {
        crate::storage::plugins::run_plugin_command(
            &["marketplace", "add", OFFICIAL_MARKETPLACE_SOURCE],
            None,
        )
        .await?
    };

    Ok(operation_result(result))
}

/// Use Claude's plugin ID when calling lifecycle commands. Kept as a small
/// public helper for the legacy web dispatch and tests.
pub fn claude_plugin_reference(plugin: &InstalledPlugin) -> String {
    plugin_reference(plugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_available_cli_entry() {
        let value = serde_json::json!({
            "id": "github@claude-plugins-official",
            "name": "github",
            "marketplace": "claude-plugins-official",
            "version": "1.0.0",
            "description": "GitHub integration",
            "author": { "name": "Anthropic" },
            "keywords": ["git", "github"]
        });
        let plugin = available_plugin(&value).expect("maps");
        assert_eq!(plugin.name, "github");
        assert_eq!(
            plugin.marketplace_name.as_deref(),
            Some("claude-plugins-official")
        );
        assert_eq!(plugin.tags, vec!["git", "github"]);
    }

    #[test]
    fn maps_installed_cli_entry() {
        let value = serde_json::json!({
            "id": "github@claude-plugins-official",
            "name": "github",
            "version": "1.0.0",
            "scope": "user",
            "enabled": true,
            "installPath": "/Users/example/.claude/plugins/cache/github/1.0.0"
        });
        let plugin = installed_plugin(&value).expect("maps");
        assert_eq!(plugin.name, "github");
        assert_eq!(
            plugin.plugin_id.as_deref(),
            Some("github@claude-plugins-official")
        );
        assert_eq!(
            plugin.marketplace.as_deref(),
            Some("claude-plugins-official")
        );
        assert_eq!(plugin.agent.as_deref(), Some("claude"));
        assert_eq!(
            plugin.extra.get("installLocation").and_then(Value::as_str),
            Some("/Users/example/.claude/plugins/cache/github/1.0.0")
        );
    }

    #[test]
    fn official_marketplace_is_builtin_and_merged() {
        let marketplaces = merge_marketplaces(Vec::new());
        let official = marketplaces
            .iter()
            .find(|marketplace| marketplace.name == OFFICIAL_MARKETPLACE_NAME)
            .expect("official marketplace");
        assert!(official.builtin);
        assert_eq!(official.display_name.as_deref(), Some("Anthropic Official"));
    }

    #[test]
    fn detects_official_marketplace_in_array_and_object_payloads() {
        let array = serde_json::json!([{"name": OFFICIAL_MARKETPLACE_NAME}]);
        let object = serde_json::json!({
            "marketplaces": [{"name": OFFICIAL_MARKETPLACE_NAME}]
        });
        assert!(has_official_marketplace(&array));
        assert!(has_official_marketplace(&object));
    }

    #[test]
    fn parses_object_and_array_payloads() {
        let object = serde_json::json!({ "available": [{ "id": "one@market" }] });
        let array = serde_json::json!([{ "id": "one@market" }]);
        assert_eq!(payload_array(&object, "available").len(), 1);
        assert_eq!(payload_array(&array, "available").len(), 1);
    }
}
