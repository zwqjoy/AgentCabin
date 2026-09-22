use std::collections::HashMap;
use std::fs;

use serde_json::{Map, Value};

use crate::work::models::WorkConnectorSummary;
use crate::work::paths::WorkPaths;

pub use crate::work::system_packages::PI_MCP_ADAPTER_SOURCE;
pub const WORK_MCP_SECRET_PLACEHOLDER: &str = "__AGENTCABIN_WORK_SECRET__";
const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SECRETS_BYTES: u64 = 4 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 80;

struct ConnectorInput<'a> {
    name: &'a str,
    transport: &'a str,
    command: Option<&'a str>,
    args: &'a [String],
    url: Option<&'a str>,
    env_vars: Option<&'a HashMap<String, String>>,
    headers: Option<&'a HashMap<String, String>>,
}

pub fn list() -> Result<Vec<WorkConnectorSummary>, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    list_with_paths(&paths)
}

pub fn upsert(
    name: &str,
    transport: &str,
    command: Option<&str>,
    args: &[String],
    url: Option<&str>,
    env_vars: Option<&HashMap<String, String>>,
    headers: Option<&HashMap<String, String>>,
) -> Result<WorkConnectorSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    upsert_with_paths(
        &paths,
        ConnectorInput {
            name,
            transport,
            command,
            args,
            url,
            env_vars,
            headers,
        },
    )
}

pub fn toggle(name: &str, enabled: bool) -> Result<WorkConnectorSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let mut root = read_root(&paths)?;
    let servers = server_map_mut(&mut root)?;
    let server = servers
        .get_mut(name.trim())
        .ok_or_else(|| format!("Work connector '{}' not found", name.trim()))?;
    let object = server
        .as_object_mut()
        .ok_or_else(|| "Work connector configuration must be an object".to_string())?;
    object.insert("disabled".into(), Value::Bool(!enabled));
    write_root(&paths, &root)?;
    list_with_paths(&paths)?
        .into_iter()
        .find(|connector| connector.name == name.trim())
        .ok_or_else(|| format!("Work connector '{}' disappeared after update", name.trim()))
}

pub fn remove(name: &str) -> Result<(), String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let mut root = read_root(&paths)?;
    let servers = server_map_mut(&mut root)?;
    if servers.remove(name.trim()).is_none() {
        return Err(format!("Work connector '{}' not found", name.trim()));
    }
    write_root(&paths, &root)?;
    remove_secret_connector(&paths, name.trim())
}

/// Return the Work-only runtime configuration with secret placeholders resolved in memory.
/// The returned value must never be sent to the frontend or resource catalog.
pub fn read_runtime_root(paths: &WorkPaths) -> Result<Value, String> {
    let mut root = read_root(paths)?;
    let secrets = read_secrets(paths)?;
    merge_secret_values(&mut root, &secrets)?;
    Ok(root)
}

pub fn list_with_paths(paths: &WorkPaths) -> Result<Vec<WorkConnectorSummary>, String> {
    let root = read_root(paths)?;
    let pi_runtime_available = has_pi_adapter(paths)?;
    // DSH Work registers the connector bridge as a native Cordis plugin, so it
    // does not need Pi's npm adapter package. Keep the two readiness signals
    // separate and retain the aggregate field for older clients.
    let dsh_runtime_available = has_dsh_runtime();
    let runtime_available = pi_runtime_available || dsh_runtime_available;
    let adapter_installed = is_adapter_installed(paths);
    let Some(servers) = root
        .get("mcpServers")
        .or_else(|| root.get("mcp_servers"))
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };

    let mut result = servers
        .iter()
        .map(|(name, config)| {
            summarize(
                name,
                config,
                runtime_available,
                pi_runtime_available,
                dsh_runtime_available,
                adapter_installed,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    result.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(result)
}

pub fn has_enabled(paths: &WorkPaths) -> Result<bool, String> {
    let root = read_root(paths)?;
    Ok(root
        .get("mcpServers")
        .or_else(|| root.get("mcp_servers"))
        .and_then(Value::as_object)
        .is_some_and(|servers| {
            servers.values().any(|config| {
                !config
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
        }))
}

pub fn sync_mcp_from_catalog_and_bindings(paths: &WorkPaths) -> Result<(), String> {
    let (projected_servers, catalog) =
        crate::storage::profile_bindings::project_mcp_bindings_from_catalog(paths.data_root());
    if projected_servers.is_empty() && catalog.is_empty() {
        return Ok(());
    }
    let mut root = read_root(paths).unwrap_or_else(|_| empty_root());
    let mut secrets = read_secrets(paths).unwrap_or_default();

    let servers = server_map_mut(&mut root)?;
    for server in &projected_servers {
        let mut server_obj = Map::new();
        if !server.enabled {
            server_obj.insert("disabled".into(), Value::Bool(true));
        }

        let secret_env_map: Map<String, Value> = server
            .secret_env
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let secret_headers_map: Map<String, Value> = server
            .secret_headers
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();

        if server.transport == "stdio" {
            if let Some(cmd) = &server.command {
                server_obj.insert("command".into(), Value::String(cmd.clone()));
            }
            server_obj.insert(
                "args".into(),
                Value::Array(
                    server
                        .args
                        .iter()
                        .map(|a| Value::String(a.clone()))
                        .collect(),
                ),
            );
            let mut env_map = Map::new();
            for (k, v) in &server.env {
                env_map.insert(k.clone(), Value::String(v.clone()));
            }
            for k in secret_env_map.keys() {
                env_map.insert(k.clone(), Value::String(WORK_MCP_SECRET_PLACEHOLDER.into()));
            }
            if !env_map.is_empty() {
                server_obj.insert("env".into(), Value::Object(env_map));
            }
        } else if server.transport == "sse"
            || server.transport == "http"
            || server.transport == "streamable-http"
        {
            server_obj.insert("transport".into(), Value::String(server.transport.clone()));
            if let Some(url) = &server.url {
                server_obj.insert("url".into(), Value::String(url.clone()));
            }
            let mut headers_map = Map::new();
            for (k, v) in &server.headers {
                headers_map.insert(k.clone(), Value::String(v.clone()));
            }
            for k in secret_headers_map.keys() {
                headers_map.insert(k.clone(), Value::String(WORK_MCP_SECRET_PLACEHOLDER.into()));
            }
            if !headers_map.is_empty() {
                server_obj.insert("headers".into(), Value::Object(headers_map));
            }
        }

        if !secret_env_map.is_empty() || !secret_headers_map.is_empty() {
            let _ = set_secret_connector(
                &mut secrets,
                &server.server_id,
                &secret_env_map,
                &secret_headers_map,
            );
        }

        servers.insert(server.server_id.clone(), Value::Object(server_obj));
    }
    write_root(paths, &root)?;
    write_secrets(paths, &secrets)?;
    Ok(())
}

pub fn ensure_config(paths: &WorkPaths) -> Result<(), String> {
    if !paths.work_mcp_config_path().is_file() {
        write_root(paths, &empty_root())?;
    }
    let catalog = crate::storage::profile_bindings::read_mcp_catalog_with_root(paths.data_root());
    let bindings = crate::storage::profile_bindings::read_mcp_bindings_with_root(paths.data_root());
    if !catalog.is_empty() || !bindings.is_empty() {
        if let Err(error) = sync_mcp_from_catalog_and_bindings(paths) {
            log::warn!(
                "[work/connectors] failed to sync MCP catalog bindings: {}",
                error
            );
        }
    }
    Ok(())
}

pub fn is_adapter_installed(paths: &WorkPaths) -> bool {
    let package_path = crate::work::system_packages::common_system_package_manifest_path(
        paths,
        crate::work::system_packages::PI_MCP_ADAPTER_PACKAGE_NAME,
    );
    let Ok(content) = fs::read_to_string(package_path) else {
        return false;
    };
    let Ok(package) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    package.get("name").and_then(Value::as_str)
        == Some(crate::work::system_packages::PI_MCP_ADAPTER_PACKAGE_NAME)
        && package.get("version").and_then(Value::as_str)
            == Some(crate::work::system_packages::PI_MCP_ADAPTER_VERSION)
}

pub fn sync_pi_settings(
    paths: &WorkPaths,
    settings: &mut Map<String, Value>,
) -> Result<(), String> {
    ensure_config(paths)?;
    let mut packages = settings
        .remove("packages")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| {
            package_source(entry).is_none_or(|source| {
                !crate::work::system_packages::is_pi_mcp_adapter_source(source)
            })
        })
        .collect::<Vec<_>>();
    if has_enabled(paths)? {
        packages.push(Value::String(PI_MCP_ADAPTER_SOURCE.to_string()));
    }
    settings.insert("packages".into(), Value::Array(packages));
    Ok(())
}

fn upsert_with_paths(
    paths: &WorkPaths,
    input: ConnectorInput<'_>,
) -> Result<WorkConnectorSummary, String> {
    let name = normalize_name(input.name)?;
    let transport = normalize_transport(input.transport)?;
    let command = input
        .command
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let url = input.url.map(str::trim).filter(|value| !value.is_empty());
    if transport == "stdio" && command.is_none() {
        return Err("stdio connector requires a command".into());
    }
    if transport != "stdio" && url.is_none() {
        return Err("HTTP connector requires a URL".into());
    }
    if input.args.len() > 100 || input.args.iter().any(|arg| arg.chars().count() > 4_000) {
        return Err("Connector arguments are too large".into());
    }
    let env = normalize_map(input.env_vars, "environment variable")?;
    let headers = normalize_map(input.headers, "header")?;

    let mut config = Map::new();
    if transport != "stdio" {
        config.insert("type".into(), Value::String(transport.to_string()));
    }
    config.insert("lifecycle".into(), Value::String("lazy".into()));
    if let Some(command) = command {
        config.insert("command".into(), Value::String(command.to_string()));
        config.insert(
            "args".into(),
            Value::Array(input.args.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(url) = url {
        config.insert("url".into(), Value::String(url.to_string()));
    }
    if !env.is_empty() {
        config.insert("env".into(), placeholder_map(&env));
    }
    if !headers.is_empty() {
        config.insert("headers".into(), placeholder_map(&headers));
    }
    config.insert("disabled".into(), Value::Bool(false));

    let mut root = read_root(paths)?;
    server_map_mut(&mut root)?.insert(name.clone(), Value::Object(config));
    let mut secrets = read_secrets(paths)?;
    set_secret_connector(&mut secrets, &name, &env, &headers)?;
    write_secrets(paths, &secrets)?;
    write_root(paths, &root)?;
    list_with_paths(paths)?
        .into_iter()
        .find(|connector| connector.name == name)
        .ok_or_else(|| format!("Work connector '{}' disappeared after save", name))
}

fn summarize(
    name: &str,
    config: &Value,
    runtime_available: bool,
    pi_runtime_available: bool,
    dsh_runtime_available: bool,
    adapter_installed: bool,
) -> Result<WorkConnectorSummary, String> {
    let object = config
        .as_object()
        .ok_or_else(|| format!("Work connector '{}' must be an object", name))?;
    let transport = object
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| object.get("url").map(|_| "streamable-http"))
        .unwrap_or("stdio")
        .to_string();
    let args = object
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(redact_argument)
                .collect()
        })
        .unwrap_or_default();
    Ok(WorkConnectorSummary {
        name: name.to_string(),
        transport,
        enabled: !object
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        command: object
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_string),
        args,
        url: object
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string),
        env_keys: object
            .get("env")
            .and_then(Value::as_object)
            .map(|values| values.keys().cloned().collect())
            .unwrap_or_default(),
        header_keys: object
            .get("headers")
            .and_then(Value::as_object)
            .map(|values| values.keys().cloned().collect())
            .unwrap_or_default(),
        runtime_available,
        pi_runtime_available,
        dsh_runtime_available,
        adapter_installed,
    })
}

fn read_root(paths: &WorkPaths) -> Result<Value, String> {
    let path = paths.work_mcp_config_path();
    if !path.is_file() {
        return Ok(empty_root());
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err("Work MCP configuration is too large".into());
    }
    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let root = serde_json::from_str::<Value>(&content)
        .map_err(|error| format!("Invalid Work MCP configuration: {error}"))?;
    if !root.is_object() {
        return Err("Work MCP configuration must be a JSON object".into());
    }
    Ok(root)
}

fn read_secrets(paths: &WorkPaths) -> Result<Value, String> {
    let path = paths.work_mcp_secrets_path();
    if !path.is_file() {
        return Ok(empty_secrets());
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_SECRETS_BYTES {
        return Err("Work MCP secret store is too large".into());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let root = serde_json::from_str::<Value>(&content)
        .map_err(|error| format!("Invalid Work MCP secret store: {error}"))?;
    if !root.is_object() {
        return Err("Work MCP secret store must be a JSON object".into());
    }
    Ok(root)
}

fn write_secrets(paths: &WorkPaths, root: &Value) -> Result<(), String> {
    let path = paths.work_mcp_secrets_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Work MCP secret store has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let serialized = serde_json::to_string_pretty(root).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temporary, format!("{serialized}\n")).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn set_secret_connector(
    root: &mut Value,
    name: &str,
    env: &Map<String, Value>,
    headers: &Map<String, Value>,
) -> Result<(), String> {
    let object = root
        .as_object_mut()
        .ok_or_else(|| "Work MCP secret store must be a JSON object".to_string())?;
    let connectors = object
        .entry("connectors")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Work MCP secret store connectors must be an object".to_string())?;
    let mut entry = Map::new();
    if !env.is_empty() {
        entry.insert("env".into(), Value::Object(env.clone()));
    }
    if !headers.is_empty() {
        entry.insert("headers".into(), Value::Object(headers.clone()));
    }
    if entry.is_empty() {
        connectors.remove(name);
    } else {
        connectors.insert(name.to_string(), Value::Object(entry));
    }
    Ok(())
}

fn remove_secret_connector(paths: &WorkPaths, name: &str) -> Result<(), String> {
    if !paths.work_mcp_secrets_path().is_file() {
        return Ok(());
    }
    let mut root = read_secrets(paths)?;
    if let Some(connectors) = root.get_mut("connectors").and_then(Value::as_object_mut) {
        connectors.remove(name);
    }
    if root
        .get("connectors")
        .and_then(Value::as_object)
        .is_none_or(Map::is_empty)
    {
        let _ = fs::remove_file(paths.work_mcp_secrets_path());
        return Ok(());
    }
    write_secrets(paths, &root)
}

fn merge_secret_values(root: &mut Value, secrets: &Value) -> Result<(), String> {
    let servers_value = if root.get("mcpServers").is_some() {
        root.get_mut("mcpServers")
    } else {
        root.get_mut("mcp_servers")
    };
    let Some(servers) = servers_value.and_then(Value::as_object_mut) else {
        return Ok(());
    };
    let secret_connectors = secrets
        .get("connectors")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (name, server) in servers {
        let Some(server) = server.as_object_mut() else {
            continue;
        };
        let Some(secret) = secret_connectors.get(name).and_then(Value::as_object) else {
            ensure_no_secret_placeholder(server, name)?;
            continue;
        };
        merge_secret_map(server, secret, "env", name)?;
        merge_secret_map(server, secret, "headers", name)?;
    }
    Ok(())
}

fn merge_secret_map(
    server: &mut Map<String, Value>,
    secret: &Map<String, Value>,
    field: &str,
    connector_name: &str,
) -> Result<(), String> {
    let Some(values) = server.get_mut(field).and_then(Value::as_object_mut) else {
        return Ok(());
    };
    let secret_values = secret
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (key, value) in values {
        if value.as_str() != Some(WORK_MCP_SECRET_PLACEHOLDER) {
            continue;
        }
        let secret_value = secret_values
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("Work MCP credential missing: {connector_name}/{field}/{key}")
            })?;
        *value = Value::String(secret_value.to_string());
    }
    Ok(())
}

fn ensure_no_secret_placeholder(
    server: &Map<String, Value>,
    connector_name: &str,
) -> Result<(), String> {
    for field in ["env", "headers"] {
        let Some(values) = server.get(field).and_then(Value::as_object) else {
            continue;
        };
        if values
            .values()
            .any(|value| value.as_str() == Some(WORK_MCP_SECRET_PLACEHOLDER))
        {
            return Err(format!(
                "Work MCP credentials missing for connector '{connector_name}'"
            ));
        }
    }
    Ok(())
}

fn placeholder_map(values: &Map<String, Value>) -> Value {
    Value::Object(
        values
            .keys()
            .map(|key| {
                (
                    key.clone(),
                    Value::String(WORK_MCP_SECRET_PLACEHOLDER.to_string()),
                )
            })
            .collect(),
    )
}

fn write_root(paths: &WorkPaths, root: &Value) -> Result<(), String> {
    let path = paths.work_mcp_config_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Work MCP configuration has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let serialized = serde_json::to_string_pretty(root).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temporary, format!("{serialized}\n")).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn server_map_mut(root: &mut Value) -> Result<&mut Map<String, Value>, String> {
    let object = root
        .as_object_mut()
        .ok_or_else(|| "Work MCP configuration must be a JSON object".to_string())?;
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    servers
        .as_object_mut()
        .ok_or_else(|| "mcpServers must be a JSON object".into())
}

fn empty_root() -> Value {
    serde_json::json!({ "mcpServers": {} })
}

fn empty_secrets() -> Value {
    serde_json::json!({ "version": 1, "connectors": {} })
}

fn normalize_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Connector name must use ASCII letters, numbers, '-' or '_'".into());
    }
    Ok(name.to_string())
}

fn normalize_transport(transport: &str) -> Result<&str, String> {
    let transport = transport.trim();
    if transport.eq_ignore_ascii_case("stdio") {
        Ok("stdio")
    } else if transport.eq_ignore_ascii_case("http")
        || transport.eq_ignore_ascii_case("streamable-http")
    {
        Ok("streamable-http")
    } else if transport.eq_ignore_ascii_case("sse") {
        Ok("sse")
    } else {
        Err("Unsupported Work connector transport".into())
    }
}

fn normalize_map(
    values: Option<&HashMap<String, String>>,
    label: &str,
) -> Result<Map<String, Value>, String> {
    let mut normalized = Map::new();
    if let Some(values) = values {
        for (key, value) in values {
            let key = key.trim();
            if key.is_empty() || key.chars().count() > 200 || key.chars().any(char::is_control) {
                return Err(format!("Invalid {label} name"));
            }
            if value.len() > 16_000 || value.chars().any(char::is_control) {
                return Err(format!("Invalid {label} value"));
            }
            normalized.insert(key.to_string(), Value::String(value.clone()));
        }
    }
    Ok(normalized)
}

fn package_source(entry: &Value) -> Option<&str> {
    match entry {
        Value::String(source) => Some(source.as_str()),
        Value::Object(object) => object.get("source").and_then(Value::as_str),
        _ => None,
    }
}

fn redact_argument(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if ["token", "key", "secret", "password", "bearer", "auth"]
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        "••••".into()
    } else {
        value.to_string()
    }
}

fn has_pi_adapter(paths: &WorkPaths) -> Result<bool, String> {
    let path = paths.work_profile_dir().join("settings.json");
    if !path.is_file() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let root = serde_json::from_str::<Value>(&content).unwrap_or_else(|_| serde_json::json!({}));
    Ok(root
        .get("packages")
        .and_then(Value::as_array)
        .is_some_and(|packages| {
            packages.iter().any(|entry| {
                package_source(entry).is_some_and(|source| {
                    crate::work::system_packages::is_pi_mcp_adapter_source(source)
                })
            })
        }))
}

fn has_dsh_runtime() -> bool {
    let resolved = crate::agent::claude_stream::resolve_dsh_path();
    std::path::Path::new(&resolved).is_file()
        || crate::agent::claude_stream::which_binary("dsh").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths(temp: &TempDir) -> WorkPaths {
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        paths
    }

    #[test]
    fn stores_work_mcp_config_without_exposing_secret_values() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let mut env = HashMap::new();
        env.insert("API_TOKEN".into(), "secret-value".into());
        let summary = upsert_with_paths(
            &paths,
            ConnectorInput {
                transport: "stdio",
                name: "research",
                command: Some("npx"),
                args: &["-y".into(), "mcp-server".into()],
                url: None,
                env_vars: Some(&env),
                headers: None,
            },
        )
        .unwrap();
        assert_eq!(summary.name, "research");
        assert_eq!(summary.env_keys, vec!["API_TOKEN"]);
        let content = fs::read_to_string(paths.work_mcp_config_path()).unwrap();
        assert!(content.contains(WORK_MCP_SECRET_PLACEHOLDER));
        assert!(!content.contains("secret-value"));
        let secret_content = fs::read_to_string(paths.work_mcp_secrets_path()).unwrap();
        assert!(secret_content.contains("secret-value"));
        assert!(!serde_json::to_string(&summary)
            .unwrap()
            .contains("secret-value"));
        let runtime = read_runtime_root(&paths).unwrap();
        assert_eq!(
            runtime["mcpServers"]["research"]["env"]["API_TOKEN"],
            "secret-value"
        );
    }

    #[test]
    fn removes_work_connector_secret_when_connector_is_removed() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let mut env = HashMap::new();
        env.insert("API_TOKEN".into(), "secret-value".into());
        upsert_with_paths(
            &paths,
            ConnectorInput {
                name: "research",
                transport: "stdio",
                command: Some("npx"),
                args: &[],
                url: None,
                env_vars: Some(&env),
                headers: None,
            },
        )
        .unwrap();

        let mut root = read_root(&paths).unwrap();
        server_map_mut(&mut root).unwrap().remove("research");
        write_root(&paths, &root).unwrap();
        remove_secret_connector(&paths, "research").unwrap();

        assert!(!paths.work_mcp_secrets_path().exists());
    }

    #[test]
    fn rejects_work_connector_with_missing_secret() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        fs::write(
            paths.work_mcp_config_path(),
            serde_json::json!({
                "mcpServers": {
                    "research": {
                        "command": "npx",
                        "env": {
                            "API_TOKEN": WORK_MCP_SECRET_PLACEHOLDER
                        }
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let error = read_runtime_root(&paths).unwrap_err();
        assert!(error.contains("credentials missing"));
        assert!(error.contains("research"));
    }

    #[cfg(unix)]
    #[test]
    fn stores_work_mcp_secret_store_with_user_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let mut env = HashMap::new();
        env.insert("API_TOKEN".into(), "secret-value".into());
        upsert_with_paths(
            &paths,
            ConnectorInput {
                name: "research",
                transport: "stdio",
                command: Some("npx"),
                args: &[],
                url: None,
                env_vars: Some(&env),
                headers: None,
            },
        )
        .unwrap();

        let mode = fs::metadata(paths.work_mcp_secrets_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn disabled_connector_is_removed_from_pi_adapter_packages() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        upsert_with_paths(
            &paths,
            ConnectorInput {
                transport: "http",
                name: "research",
                command: None,
                args: &[],
                url: Some("https://example.test/mcp"),
                env_vars: None,
                headers: None,
            },
        )
        .unwrap();
        let mut settings = Map::new();
        sync_pi_settings(&paths, &mut settings).unwrap();
        assert_eq!(settings["packages"][0], PI_MCP_ADAPTER_SOURCE);

        let mut root = read_root(&paths).unwrap();
        root["mcpServers"]["research"]["disabled"] = Value::Bool(true);
        write_root(&paths, &root).unwrap();
        let mut settings = Map::new();
        sync_pi_settings(&paths, &mut settings).unwrap();
        assert!(settings["packages"].as_array().unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_connector_names_and_transports() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        assert!(upsert_with_paths(
            &paths,
            ConnectorInput {
                transport: "stdio",
                name: "bad name",
                command: Some("npx"),
                args: &[],
                url: None,
                env_vars: None,
                headers: None,
            },
        )
        .is_err());
        assert!(upsert_with_paths(
            &paths,
            ConnectorInput {
                transport: "socket",
                name: "valid",
                command: Some("npx"),
                args: &[],
                url: None,
                env_vars: None,
                headers: None,
            },
        )
        .is_err());
    }
}
