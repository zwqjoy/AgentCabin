use crate::agent::claude_stream::{augmented_path, resolve_claude_path};
use crate::models::{
    ConfiguredMcpServer, McpRegistrySearchResult, McpRegistryServer, PluginOperationResult,
    ProviderHealth,
};
use crate::process_ext::HideConsole;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;

// ── Constants ──

const REGISTRY_BASE: &str = "https://registry.modelcontextprotocol.io/v0";
const CACHE_TTL: Duration = Duration::from_secs(120);
const HEALTH_TTL: Duration = Duration::from_secs(300);
const CMD_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CACHE_ENTRIES: usize = 100;

// Patterns to redact in args
const SENSITIVE_PATTERNS: &[&str] = &["token", "key", "secret", "bearer", "password", "auth"];

// ── HTTP client (reuse across requests) ──

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .user_agent("AgentCabin/0.1")
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(2);

    if let Some(proxy_url) = crate::work::proxy::work_proxy_url() {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }

    builder.build().unwrap_or_default()
});

// ── Search cache: key → (timestamp, result) ──

type SearchCache = HashMap<String, (Instant, McpRegistrySearchResult)>;
static SEARCH_CACHE: LazyLock<Mutex<SearchCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// ── Health cache ──

static HEALTH_CACHE: LazyLock<Mutex<Option<(Instant, ProviderHealth)>>> =
    LazyLock::new(|| Mutex::new(None));

// ── Install mutex (serialize add/remove) ──

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// ── Intermediate deserialization structs ──
// The registry API wraps each server entry in a `server` object with optional `_meta`.

#[derive(Deserialize)]
struct RegistryApiResponse {
    #[serde(default)]
    servers: Vec<RegistryApiEntry>,
    #[serde(default)]
    metadata: Option<RegistryApiMetadata>,
}

#[derive(Deserialize)]
struct RegistryApiEntry {
    #[serde(default)]
    server: serde_json::Value,
    #[serde(default, rename = "_meta")]
    meta: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryApiMetadata {
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    count: Option<u32>,
}

// ── Public API ──

pub async fn health_check() -> ProviderHealth {
    // Check cache first
    {
        let cache = HEALTH_CACHE.lock().await;
        if let Some((ts, ref health)) = *cache {
            if ts.elapsed() < HEALTH_TTL {
                log::debug!(
                    "[mcp_registry] health_check: cached result={}",
                    health.available
                );
                return health.clone();
            }
        }
    }

    log::debug!("[mcp_registry] health_check: fetching from registry");
    let url = format!("{}/servers?search=test&limit=1", REGISTRY_BASE);
    let result = CLIENT.get(&url).send().await;

    let health = match result {
        Ok(resp) if resp.status().is_success() => ProviderHealth {
            available: true,
            reason: None,
        },
        Ok(resp) => ProviderHealth {
            available: false,
            reason: Some(format!("HTTP {}", resp.status())),
        },
        Err(e) => ProviderHealth {
            available: false,
            reason: Some(format!("{e}")),
        },
    };

    log::debug!(
        "[mcp_registry] health_check: available={}, reason={:?}",
        health.available,
        health.reason
    );

    let mut cache = HEALTH_CACHE.lock().await;
    *cache = Some((Instant::now(), health.clone()));
    health
}

pub async fn search(
    query: &str,
    limit: u32,
    cursor: Option<&str>,
) -> Result<McpRegistrySearchResult, String> {
    let cache_key = format!(
        "{}:{}:{}",
        query.to_lowercase(),
        limit,
        cursor.unwrap_or("")
    );

    // Check cache
    {
        let cache = SEARCH_CACHE.lock().await;
        if let Some((ts, ref results)) = cache.get(&cache_key) {
            if ts.elapsed() < CACHE_TTL {
                log::debug!(
                    "[mcp_registry] search: cache hit for '{}', {} servers",
                    query,
                    results.servers.len()
                );
                return Ok(results.clone());
            }
        }
    }

    log::debug!(
        "[mcp_registry] search: query='{}', limit={}, cursor={:?}",
        query,
        limit,
        cursor
    );

    let mut url = format!("{}/servers?search={}&limit={}", REGISTRY_BASE, query, limit);
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let resp = CLIENT
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Search request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Registry API returned HTTP {}", resp.status()));
    }

    let body: RegistryApiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse registry response: {e}"))?;

    // Lenient deserialization: parse each entry individually, skip failures
    let mut servers = Vec::new();
    for entry in &body.servers {
        // Only include latest versions
        if let Some(ref meta) = entry.meta {
            if let Some(is_latest) = meta.get("isLatest") {
                if is_latest == &serde_json::Value::Bool(false) {
                    continue;
                }
            }
        }

        match serde_json::from_value::<McpRegistryServer>(entry.server.clone()) {
            Ok(s) => servers.push(s),
            Err(e) => {
                log::debug!("[mcp_registry] skipping entry: parse error: {}", e);
            }
        }
    }

    // Deduplicate by name — keep the first occurrence (registry returns latest-first)
    let mut seen_names = std::collections::HashSet::new();
    servers.retain(|s| seen_names.insert(s.name.clone()));

    let next_cursor = body.metadata.as_ref().and_then(|m| m.next_cursor.clone());
    let count = body
        .metadata
        .as_ref()
        .and_then(|m| m.count)
        .unwrap_or(servers.len() as u32);

    let result = McpRegistrySearchResult {
        servers,
        next_cursor,
        count,
    };

    log::debug!(
        "[mcp_registry] search: '{}' returned {} servers, next_cursor={:?}",
        query,
        result.servers.len(),
        result.next_cursor
    );

    // Store in cache (with eviction)
    {
        let mut cache = SEARCH_CACHE.lock().await;
        if cache.len() >= MAX_CACHE_ENTRIES {
            let now = Instant::now();
            cache.retain(|_, (ts, _)| now.duration_since(*ts) < CACHE_TTL);
            if cache.len() >= MAX_CACHE_ENTRIES {
                cache.clear();
            }
        }
        cache.insert(cache_key, (Instant::now(), result.clone()));
    }

    Ok(result)
}

/// List configured MCP servers from all config file locations.
///
/// Reads from:
/// - `~/.claude.json` → `projects[cwd].mcpServers` → scope="local"
/// - `~/.claude.json` → top-level `mcpServers` → scope="user" (CLI primary location)
/// - `~/.claude/settings.json` → `mcpServers` → scope="user" (fallback)
/// - `{cwd}/.mcp.json` → `mcpServers` → scope="project"
pub fn list_configured(cwd: Option<&str>) -> Vec<ConfiguredMcpServer> {
    let mut servers = Vec::new();
    let home = match crate::storage::dirs_next() {
        Some(h) => h,
        None => {
            log::warn!("[mcp_registry] could not determine home directory");
            return servers;
        }
    };

    // 1. ~/.claude.json → projects[cwd].mcpServers (scope="local")
    if let Some(cwd_str) = cwd {
        if !cwd_str.is_empty() {
            let claude_json = home.join(".claude.json");
            if let Ok(content) = std::fs::read_to_string(&claude_json) {
                if let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(project_servers) = root
                        .get("projects")
                        .and_then(|p| p.get(cwd_str))
                        .and_then(|p| p.get("mcpServers"))
                        .and_then(|v| v.as_object())
                    {
                        for (name, config) in project_servers {
                            servers.push(parse_mcp_entry(
                                name,
                                config,
                                "local",
                                claude_json.to_str(),
                            ));
                        }
                        log::debug!(
                            "[mcp_registry] local servers from ~/.claude.json: {}",
                            project_servers.len()
                        );
                    }
                }
            }
        }
    }

    // 2a. ~/.claude.json → top-level mcpServers (scope="user")
    //     CLI stores user-scope servers here via `claude mcp add --scope user`
    {
        let claude_json = home.join(".claude.json");
        if let Ok(content) = std::fs::read_to_string(&claude_json) {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(mcp_servers) = root.get("mcpServers").and_then(|v| v.as_object()) {
                    for (name, config) in mcp_servers {
                        servers.push(parse_mcp_entry(name, config, "user", claude_json.to_str()));
                    }
                    log::debug!(
                        "[mcp_registry] user servers from ~/.claude.json: {}",
                        mcp_servers.len()
                    );
                }
            }
        }
    }

    // 2b. ~/.claude/settings.json → mcpServers (scope="user")
    let settings_path = home.join(".claude").join("settings.json");
    if let Ok(content) = std::fs::read_to_string(&settings_path) {
        if let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(mcp_servers) = root.get("mcpServers").and_then(|v| v.as_object()) {
                for (name, config) in mcp_servers {
                    // Avoid duplicates if same name already found in ~/.claude.json
                    if !servers.iter().any(|s| s.name == *name && s.scope == "user") {
                        servers.push(parse_mcp_entry(
                            name,
                            config,
                            "user",
                            settings_path.to_str(),
                        ));
                    }
                }
                log::debug!(
                    "[mcp_registry] user servers from settings.json: {}",
                    mcp_servers.len()
                );
            }
        }
    }

    // 3. {cwd}/.mcp.json → mcpServers (scope="project")
    if let Some(cwd_str) = cwd {
        if !cwd_str.is_empty() {
            let mcp_json = std::path::PathBuf::from(cwd_str).join(".mcp.json");
            if let Ok(content) = std::fs::read_to_string(&mcp_json) {
                if let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Support both flat format and wrapped { mcpServers: {...} }
                    let mcp_obj = root
                        .get("mcpServers")
                        .and_then(|v| v.as_object())
                        .or_else(|| root.as_object());

                    if let Some(entries) = mcp_obj {
                        // Skip if entries look like the wrapper itself (has "mcpServers" key only)
                        let is_wrapper = entries.len() == 1 && entries.contains_key("mcpServers");
                        if !is_wrapper {
                            for (name, config) in entries {
                                servers.push(parse_mcp_entry(
                                    name,
                                    config,
                                    "project",
                                    mcp_json.to_str(),
                                ));
                            }
                            log::debug!(
                                "[mcp_registry] project servers from .mcp.json: {}",
                                entries.len()
                            );
                        }
                    }
                }
            }
        }
    }

    log::debug!(
        "[mcp_registry] list_configured: {} total servers",
        servers.len()
    );
    servers
}

/// Parse a single MCP server entry from JSON config into ConfiguredMcpServer.
fn parse_mcp_entry(
    name: &str,
    config: &serde_json::Value,
    scope: &str,
    source_path: Option<&str>,
) -> ConfiguredMcpServer {
    let server_type = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("stdio")
        .to_string();

    let command = config
        .get("command")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let args = config
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| {
                    let s = v.as_str().unwrap_or("").to_string();
                    redact_sensitive_arg(&s)
                })
                .collect()
        })
        .unwrap_or_default();

    let url = config
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Only expose env keys, not values
    let env_keys = config
        .get("env")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    // Only expose header names
    let header_keys = config
        .get("headers")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    ConfiguredMcpServer {
        name: name.to_string(),
        server_type,
        scope: scope.to_string(),
        command,
        args,
        url,
        env_keys,
        header_keys,
        source_path: source_path.map(|s| s.to_string()),
        ..Default::default()
    }
}

/// Redact arg values that match sensitive patterns.
fn redact_sensitive_arg(arg: &str) -> String {
    let lower = arg.to_lowercase();
    for pattern in SENSITIVE_PATTERNS {
        if lower.contains(pattern) {
            return "***".to_string();
        }
    }
    arg.to_string()
}

/// Add an MCP server via Claude CLI.
#[allow(clippy::too_many_arguments)]
pub async fn add_server(
    name: &str,
    transport: &str,
    scope: &str,
    cwd: Option<&str>,
    config_json: Option<&str>,
    url: Option<&str>,
    env_vars: Option<&HashMap<String, String>>,
    headers: Option<&HashMap<String, String>>,
) -> Result<PluginOperationResult, String> {
    validate_name(name)?;
    validate_scope(scope)?;

    // scope=local or project requires cwd
    if (scope == "local" || scope == "project") && cwd.map(|s| s.is_empty()).unwrap_or(true) {
        return Err(format!(
            "Scope '{}' requires a working directory (cwd)",
            scope
        ));
    }

    // CLI only accepts [a-zA-Z0-9_-] in names — derive local name from registry format
    // e.g. "ai.kubit/mcp-server" → "mcp-server", "com.letta/memory-mcp" → "memory-mcp"
    let local_name = to_cli_name(name);

    let _lock = INSTALL_LOCK.lock().await;

    let claude_bin = resolve_claude_path();
    let path_env = augmented_path();

    let mut cmd = Command::new(&claude_bin);

    match transport {
        "stdio" | "sse" => {
            // Use add-json: `claude mcp add-json --scope {scope} {name} '{json}'`
            let json_str = match config_json {
                Some(j) => j.to_string(),
                None => {
                    return Err("config_json is required for stdio/sse transport".to_string());
                }
            };
            cmd.args(["mcp", "add-json", "--scope", scope, &local_name, &json_str]);
        }
        "http" => {
            // Use add: `claude mcp add --transport http --scope {scope} [-H "K: V"]... {name} {url}`
            let server_url = match url {
                Some(u) if !u.is_empty() => u,
                _ => return Err("url is required for http transport".to_string()),
            };
            cmd.args(["mcp", "add", "--transport", "http", "--scope", scope]);
            // Add headers
            if let Some(hdrs) = headers {
                for (k, v) in hdrs {
                    cmd.args(["-H", &format!("{}: {}", k, v)]);
                }
            }
            cmd.args([&local_name, server_url]);
        }
        _ => {
            return Err(format!("Unsupported transport: {}", transport));
        }
    }

    // Set env vars for stdio servers
    if let Some(env) = env_vars {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }

    cmd.env("PATH", &path_env)
        .env_remove("CLAUDECODE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Set cwd for local/project scope
    if let Some(cwd_str) = cwd {
        if !cwd_str.is_empty() {
            cmd.current_dir(cwd_str);
        }
    }

    log::debug!(
        "[mcp_registry] add_server: name={} → local_name={}, transport={}, scope={}",
        name,
        local_name,
        transport,
        scope
    );

    cmd.hide_console().kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| {
        log::error!("[mcp_registry] failed to spawn claude: {}", e);
        format!("Failed to spawn claude: {e}")
    })?;

    let result = timeout(CMD_TIMEOUT, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let success = output.status.success();

            log::debug!(
                "[mcp_registry] add_server completed: success={}, stdout_len={}, stderr_len={}",
                success,
                stdout.len(),
                stderr.len()
            );
            if !success {
                log::debug!(
                    "[mcp_registry] add stderr: {}",
                    &stderr[..stderr.len().min(500)]
                );
            }

            Ok(PluginOperationResult {
                success,
                message: if success {
                    let msg = stdout.trim().to_string();
                    if msg.is_empty() {
                        format!("Added MCP server '{}'", name)
                    } else {
                        msg
                    }
                } else {
                    stderr.trim().to_string()
                },
            })
        }
        Ok(Err(e)) => {
            log::error!("[mcp_registry] process error: {}", e);
            Err(format!("Process error: {e}"))
        }
        Err(_) => {
            log::error!(
                "[mcp_registry] command timed out after {}s",
                CMD_TIMEOUT.as_secs()
            );
            Err(format!(
                "Command timed out after {}s",
                CMD_TIMEOUT.as_secs()
            ))
        }
    }
}

/// Remove an MCP server via Claude CLI.
pub async fn remove_server(
    name: &str,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    validate_name(name)?;
    validate_scope(scope)?;

    // scope=local or project requires cwd
    if (scope == "local" || scope == "project") && cwd.map(|s| s.is_empty()).unwrap_or(true) {
        return Err(format!(
            "Scope '{}' requires a working directory (cwd)",
            scope
        ));
    }

    let _lock = INSTALL_LOCK.lock().await;

    let claude_bin = resolve_claude_path();
    let path_env = augmented_path();

    let mut cmd = Command::new(&claude_bin);
    cmd.args(["mcp", "remove", "--scope", scope, name]);
    cmd.env("PATH", &path_env)
        .env_remove("CLAUDECODE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Set cwd for local/project scope
    if let Some(cwd_str) = cwd {
        if !cwd_str.is_empty() {
            cmd.current_dir(cwd_str);
        }
    }

    log::debug!(
        "[mcp_registry] remove_server: name={}, scope={}",
        name,
        scope
    );

    cmd.hide_console().kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| {
        log::error!("[mcp_registry] failed to spawn claude: {}", e);
        format!("Failed to spawn claude: {e}")
    })?;

    let result = timeout(CMD_TIMEOUT, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let success = output.status.success();

            log::debug!(
                "[mcp_registry] remove_server completed: success={}, stdout_len={}, stderr_len={}",
                success,
                stdout.len(),
                stderr.len()
            );

            Ok(PluginOperationResult {
                success,
                message: if success {
                    let msg = stdout.trim().to_string();
                    if msg.is_empty() {
                        format!("Removed MCP server '{}'", name)
                    } else {
                        msg
                    }
                } else {
                    stderr.trim().to_string()
                },
            })
        }
        Ok(Err(e)) => {
            log::error!("[mcp_registry] process error: {}", e);
            Err(format!("Process error: {e}"))
        }
        Err(_) => {
            log::error!(
                "[mcp_registry] command timed out after {}s",
                CMD_TIMEOUT.as_secs()
            );
            Err(format!(
                "Command timed out after {}s",
                CMD_TIMEOUT.as_secs()
            ))
        }
    }
}

/// Toggle an MCP server's disabled state by modifying the config file directly.
/// Claude CLI does not support toggle via the stream-json control protocol,
/// so we set/remove `"disabled": true` in the config JSON.
/// Atomically replace an existing config file (write tmp in the same dir → rename),
/// preserving its current permissions. Prevents a crash/interrupt mid-write from
/// truncating shared, high-value files like `~/.claude.json` (which holds all of the
/// CLI's project config + secrets). The file is expected to exist. (audit #6)
fn write_config_atomic(path: &std::path::Path, content: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| "config path has no parent".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| "config path has no filename".to_string())?;
    let tmp = dir.join(format!(
        ".{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::write(&tmp, content).map_err(|e| format!("write tmp: {e}"))?;
    // Preserve the original file's permissions on the replacement (don't widen or
    // narrow perms on a project .mcp.json or a 0600 ~/.claude.json).
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename: {e}")
    })?;
    Ok(())
}

pub fn toggle_server_config(
    name: &str,
    enabled: bool,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    let home = crate::storage::dirs_next()
        .ok_or_else(|| "Could not determine home directory".to_string())?;

    // Determine which config file and JSON path to modify
    let (config_path, json_path) = match scope {
        "local" => {
            let cwd_str = cwd
                .filter(|s| !s.is_empty())
                .ok_or("Local scope requires a working directory")?;
            (home.join(".claude.json"), Some(cwd_str.to_string()))
        }
        "user" => (home.join(".claude.json"), None),
        "project" => {
            let cwd_str = cwd
                .filter(|s| !s.is_empty())
                .ok_or("Project scope requires a working directory")?;
            (std::path::PathBuf::from(cwd_str).join(".mcp.json"), None)
        }
        _ => return Err(format!("Unknown scope: {}", scope)),
    };

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;
    let mut root: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", config_path.display(), e))?;

    // Navigate to the correct mcpServers object
    let servers = if let Some(ref cwd_str) = json_path {
        // local scope: projects[cwd].mcpServers
        root.pointer_mut(&format!(
            "/projects/{}/mcpServers",
            cwd_str.replace('~', "~0").replace('/', "~1")
        ))
    } else if scope == "project" {
        // project scope: mcpServers in .mcp.json (may be top-level or nested)
        if root.get("mcpServers").is_some() {
            root.get_mut("mcpServers")
        } else {
            Some(&mut root)
        }
    } else {
        // user scope: top-level mcpServers
        root.get_mut("mcpServers")
    };

    let servers = servers
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| format!("mcpServers not found in {}", config_path.display()))?;

    let server = servers
        .get_mut(name)
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| format!("MCP server '{}' not found", name))?;

    if enabled {
        server.remove("disabled");
    } else {
        server.insert("disabled".to_string(), serde_json::Value::Bool(true));
    }

    let output = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    write_config_atomic(&config_path, &output)
        .map_err(|e| format!("Failed to write {}: {}", config_path.display(), e))?;

    let action = if enabled { "Enabled" } else { "Disabled" };
    log::debug!(
        "[mcp_registry] toggle_server_config: {} '{}' in {}",
        action,
        name,
        config_path.display()
    );

    Ok(PluginOperationResult {
        success: true,
        message: format!("{} MCP server '{}'", action, name),
    })
}

/// Return names of all MCP servers that have `"disabled": true` in user-scope config.
pub fn get_disabled_server_names() -> Vec<String> {
    let home = match crate::storage::dirs_next() {
        Some(h) => h,
        None => return vec![],
    };
    let config_path = home.join(".claude.json");
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let mut disabled = Vec::new();
    if let Some(servers) = root.get("mcpServers").and_then(|v| v.as_object()) {
        for (name, cfg) in servers {
            if cfg
                .get("disabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                disabled.push(name.clone());
            }
        }
    }
    disabled
}

// ── Validators ──

/// Convert a registry name to a CLI-friendly local name.
/// Registry uses reverse-domain format: "ai.kubit/mcp-server" → "mcp-server"
/// Falls back to replacing dots with hyphens if no slash present.
fn to_cli_name(name: &str) -> String {
    // Take the part after the last '/'
    let base = name.rsplit('/').next().unwrap_or(name);
    // Replace any remaining dots with hyphens, filter to [a-zA-Z0-9_-]
    let slug: String = base
        .chars()
        .map(|c| if c == '.' { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if slug.is_empty() {
        // Fallback: sanitize the whole name
        name.chars()
            .map(|c| if c == '.' || c == '/' { '-' } else { c })
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    } else {
        slug
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Server name cannot be empty".into());
    }
    if name.len() > 128 {
        return Err("Server name too long (max 128 characters)".into());
    }
    if name.chars().any(|c| c.is_control()) {
        return Err("Server name contains invalid characters".into());
    }
    Ok(())
}

fn validate_scope(scope: &str) -> Result<(), String> {
    match scope {
        "local" | "user" | "project" => Ok(()),
        _ => Err(format!(
            "Invalid scope: {scope}. Must be \"local\", \"user\", or \"project\""
        )),
    }
}

// ── Codex MCP support ──

/// Validate a Codex MCP server name: only [a-zA-Z0-9_-], no dots.
fn validate_codex_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Server name cannot be empty".into());
    }
    if name.contains('.') {
        return Err(format!(
            "Server name '{}' contains '.'; only [a-zA-Z0-9_-] allowed",
            name
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "Server name '{}' contains invalid characters; only [a-zA-Z0-9_-] allowed",
            name
        ));
    }
    Ok(())
}

/// Allowed top-level fields shared by both stdio and streamable-http Codex MCP configs.
const CODEX_MCP_SHARED_FIELDS: &[&str] = &[
    "enabled",
    "required",
    "startup_timeout_sec",
    "startup_timeout_ms",
    "tool_timeout_sec",
    "enabled_tools",
    "disabled_tools",
    "tools",
    "scopes",
    "name",
];

/// Additional fields allowed only for stdio transport (has `command`).
const CODEX_MCP_STDIO_FIELDS: &[&str] = &["command", "args", "env", "env_vars", "cwd"];

/// Additional fields allowed only for streamable-http transport (has `url`).
const CODEX_MCP_HTTP_FIELDS: &[&str] = &[
    "url",
    "bearer_token_env_var",
    "http_headers",
    "env_http_headers",
    "oauth_resource",
];

/// Validate a Codex MCP server config JSON before writing.
fn validate_codex_config(config: &serde_json::Value) -> Result<(), String> {
    let obj = config
        .as_object()
        .ok_or_else(|| "Config must be a JSON object".to_string())?;

    let has_command = obj.contains_key("command");
    let has_url = obj.contains_key("url");

    if has_command && has_url {
        return Err("Config must have 'command' OR 'url', not both".into());
    }
    if !has_command && !has_url {
        return Err("Config must have 'command' (stdio) or 'url' (streamable-http)".into());
    }

    // Reject plaintext bearer_token
    if obj.contains_key("bearer_token") {
        return Err(
            "Plaintext 'bearer_token' is not allowed; use 'bearer_token_env_var' instead".into(),
        );
    }

    // Build allowed set based on transport
    let mut allowed: std::collections::HashSet<&str> =
        CODEX_MCP_SHARED_FIELDS.iter().copied().collect();
    if has_command {
        allowed.extend(CODEX_MCP_STDIO_FIELDS.iter());
        // Reject http-only fields on stdio
        for field in CODEX_MCP_HTTP_FIELDS {
            if *field != "url" && obj.contains_key(*field) {
                return Err(format!(
                    "Field '{}' is only valid for streamable-http (url) transport",
                    field
                ));
            }
        }
    } else {
        allowed.extend(CODEX_MCP_HTTP_FIELDS.iter());
        // Reject stdio-only fields on http
        for field in CODEX_MCP_STDIO_FIELDS {
            if *field != "command" && obj.contains_key(*field) {
                return Err(format!(
                    "Field '{}' is only valid for stdio (command) transport",
                    field
                ));
            }
        }
    }

    // Reject unknown top-level fields
    for key in obj.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(format!("Unknown field '{}' in Codex MCP config", key));
        }
    }

    Ok(())
}

/// Recursively convert a serde_json::Value to a toml_edit::Item.
fn json_to_toml_edit_item(jv: &serde_json::Value) -> toml_edit::Item {
    match jv {
        serde_json::Value::String(s) => toml_edit::value(s.as_str()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml_edit::value(i)
            } else if let Some(f) = n.as_f64() {
                toml_edit::value(f)
            } else {
                toml_edit::value(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => toml_edit::value(*b),
        serde_json::Value::Array(arr) => {
            // Check if any element is an object — if so, use array of tables
            let has_objects = arr.iter().any(|v| v.is_object());
            if has_objects {
                let mut aot = toml_edit::ArrayOfTables::new();
                for item in arr {
                    if let serde_json::Value::Object(obj) = item {
                        let mut tbl = toml_edit::Table::new();
                        for (k, v) in obj {
                            tbl[k] = json_to_toml_edit_item(v);
                        }
                        aot.push(tbl);
                    }
                }
                toml_edit::Item::ArrayOfTables(aot)
            } else {
                let mut a = toml_edit::Array::new();
                for item in arr {
                    match item {
                        serde_json::Value::String(s) => a.push(s.as_str()),
                        serde_json::Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                a.push(i);
                            } else if let Some(f) = n.as_f64() {
                                a.push(f);
                            }
                        }
                        serde_json::Value::Bool(b) => a.push(*b),
                        _ => {}
                    }
                }
                toml_edit::value(a)
            }
        }
        serde_json::Value::Object(obj) => {
            let mut tbl = toml_edit::Table::new();
            for (k, v) in obj {
                tbl[k] = json_to_toml_edit_item(v);
            }
            toml_edit::Item::Table(tbl)
        }
        serde_json::Value::Null => toml_edit::Item::None,
    }
}

/// Parse a Codex TOML `[mcp_servers.X]` table into a ConfiguredMcpServer.
fn parse_codex_mcp_entry(
    name: &str,
    table: &toml::Value,
    scope: &str,
    source_path: Option<&str>,
) -> ConfiguredMcpServer {
    let obj = match table.as_table() {
        Some(t) => t,
        None => {
            return ConfiguredMcpServer {
                name: name.to_string(),
                scope: scope.to_string(),
                agent: "codex".into(),
                source_path: source_path.map(|s| s.to_string()),
                ..Default::default()
            }
        }
    };

    let command = obj
        .get("command")
        .and_then(|v| v.as_str())
        .map(String::from);
    let url = obj.get("url").and_then(|v| v.as_str()).map(String::from);

    let server_type = if command.is_some() {
        "stdio".to_string()
    } else if url.is_some() {
        "streamable-http".to_string()
    } else {
        "unknown".to_string()
    };

    let args = obj
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| {
                    let s = v.as_str().unwrap_or("").to_string();
                    redact_sensitive_arg(&s)
                })
                .collect()
        })
        .unwrap_or_default();

    // Collect env keys from `env` table and `env_vars` entries
    let mut env_keys: Vec<String> = Vec::new();
    if let Some(env_table) = obj.get("env").and_then(|v| v.as_table()) {
        env_keys.extend(env_table.keys().map(String::from));
    }
    if let Some(env_vars) = obj.get("env_vars").and_then(|v| v.as_table()) {
        env_keys.extend(env_vars.keys().map(String::from));
    }
    if let Some(bearer_env) = obj.get("bearer_token_env_var").and_then(|v| v.as_str()) {
        env_keys.push(bearer_env.to_string());
    }

    // Collect header keys from `http_headers` and `env_http_headers`
    let mut header_keys: Vec<String> = Vec::new();
    if let Some(hdrs) = obj.get("http_headers").and_then(|v| v.as_table()) {
        header_keys.extend(hdrs.keys().map(String::from));
    }
    if let Some(env_hdrs) = obj.get("env_http_headers").and_then(|v| v.as_table()) {
        header_keys.extend(env_hdrs.keys().map(String::from));
    }

    ConfiguredMcpServer {
        name: name.to_string(),
        server_type,
        scope: scope.to_string(),
        command,
        args,
        url,
        env_keys,
        header_keys,
        agent: "codex".into(),
        source_path: source_path.map(|s| s.to_string()),
    }
}

/// Find the git project root by walking up from `cwd`.
fn find_git_root(cwd: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut dir = cwd;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Collect project-scope MCP servers by walking .codex/config.toml from root→cwd.
/// Later layers override same-name servers within the project scope.
fn collect_project_codex_servers(project_roots: &[std::path::PathBuf]) -> Vec<ConfiguredMcpServer> {
    let mut by_name: std::collections::HashMap<String, ConfiguredMcpServer> =
        std::collections::HashMap::new();

    for config_path in project_roots {
        if !config_path.is_file() {
            continue;
        }
        let content = match std::fs::read_to_string(config_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let tv: toml::Value = match toml::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                log::debug!(
                    "[codex_mcp] skipping {}: parse error: {}",
                    config_path.display(),
                    e
                );
                continue;
            }
        };
        if let Some(mcp_table) = tv.get("mcp_servers").and_then(|v| v.as_table()) {
            for (name, entry) in mcp_table {
                let server = parse_codex_mcp_entry(name, entry, "project", config_path.to_str());
                // Later layers override same-name within project scope
                by_name.insert(name.clone(), server);
            }
            log::debug!(
                "[codex_mcp] project layer {}: {} servers",
                config_path.display(),
                mcp_table.len()
            );
        }
    }

    by_name.into_values().collect()
}

/// Build the list of project .codex/config.toml paths from root→cwd.
/// Stops BEFORE reading cwd's own layer — consistent with
/// `load_project_codex_config` in cli_config.rs.
fn project_codex_config_paths(cwd: &str) -> Vec<std::path::PathBuf> {
    let cwd_path = std::path::PathBuf::from(cwd);
    let project_root = match find_git_root(&cwd_path) {
        Some(r) => r,
        None => return vec![],
    };

    let mut paths = Vec::new();
    let mut current = project_root.clone();
    loop {
        // Stop before reading cwd's layer (matches load_project_codex_config)
        if current == cwd_path {
            break;
        }

        let config_path = current.join(".codex").join("config.toml");
        paths.push(config_path);

        let relative = match cwd_path.strip_prefix(&current) {
            Ok(r) => r,
            Err(_) => break,
        };
        match relative.components().next() {
            Some(component) => current = current.join(component),
            None => break,
        }
    }

    paths
}

/// Internal: list Codex configured MCP servers from explicit paths.
fn list_codex_configured_with_paths(
    user_config: &std::path::Path,
    project_roots: &[std::path::PathBuf],
) -> Vec<ConfiguredMcpServer> {
    let mut servers = Vec::new();

    // 1. User-scope from $CODEX_HOME/config.toml
    if user_config.is_file() {
        if let Ok(content) = std::fs::read_to_string(user_config) {
            if let Ok(tv) = toml::from_str::<toml::Value>(&content) {
                if let Some(mcp_table) = tv.get("mcp_servers").and_then(|v| v.as_table()) {
                    for (name, entry) in mcp_table {
                        servers.push(parse_codex_mcp_entry(
                            name,
                            entry,
                            "user",
                            user_config.to_str(),
                        ));
                    }
                    log::debug!(
                        "[codex_mcp] user servers from {}: {}",
                        user_config.display(),
                        mcp_table.len()
                    );
                }
            }
        }
    }

    // 2. Project-scope from ancestor .codex/config.toml chain
    let project_servers = collect_project_codex_servers(project_roots);
    log::debug!(
        "[codex_mcp] project servers (effective): {}",
        project_servers.len()
    );
    servers.extend(project_servers);

    log::debug!("[codex_mcp] list_codex_configured: {} total", servers.len());
    servers
}

/// List configured Codex MCP servers from user and project config files.
///
/// - User scope: `$CODEX_HOME/config.toml` → `[mcp_servers]`
/// - Project scope: walk from .git root → cwd, reading `.codex/config.toml` at each layer
///
/// Same-name servers across scopes both appear (no cross-scope dedup).
/// Within project scope, later layers override earlier same-name servers.
pub fn list_codex_configured(cwd: Option<&str>) -> Vec<ConfiguredMcpServer> {
    let user_config = match crate::storage::cli_config::codex_config_path() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[codex_mcp] codex_config_path error: {}", e);
            // Still try project configs
            let project_roots = cwd.map(project_codex_config_paths).unwrap_or_default();
            return list_codex_configured_with_paths(std::path::Path::new(""), &project_roots);
        }
    };

    let project_roots = cwd.map(project_codex_config_paths).unwrap_or_default();

    list_codex_configured_with_paths(&user_config, &project_roots)
}

/// Internal: add a Codex MCP server to a specific config file path.
fn add_codex_server_to_path(
    name: &str,
    config: &serde_json::Value,
    config_path: &std::path::Path,
) -> Result<PluginOperationResult, String> {
    validate_codex_name(name)?;
    validate_codex_config(config)?;

    // Read or create the TOML document
    let mut doc: toml_edit::DocumentMut = match std::fs::read_to_string(config_path) {
        Ok(s) => s
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("TOML parse error in {}: {}", config_path.display(), e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => toml_edit::DocumentMut::new(),
        Err(e) => return Err(format!("Failed to read {}: {}", config_path.display(), e)),
    };

    // Ensure [mcp_servers] table exists
    if !doc.contains_table("mcp_servers") {
        doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    // Insert [mcp_servers.{name}] sub-table
    let mcp_table = doc["mcp_servers"]
        .as_table_mut()
        .ok_or_else(|| "mcp_servers is not a table".to_string())?;

    let obj = config
        .as_object()
        .ok_or_else(|| "Config must be a JSON object".to_string())?;

    let mut server_table = toml_edit::Table::new();
    for (k, v) in obj {
        server_table[k] = json_to_toml_edit_item(v);
    }

    mcp_table[name] = toml_edit::Item::Table(server_table);

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let content = doc.to_string();
    std::fs::write(config_path, &content)
        .map_err(|e| format!("Failed to write {}: {}", config_path.display(), e))?;

    // Match cli_config.rs: set 0600 permissions (config can contain env/header references)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(config_path, std::fs::Permissions::from_mode(0o600));
    }

    log::debug!(
        "[codex_mcp] added server '{}' to {}",
        name,
        config_path.display()
    );

    Ok(PluginOperationResult {
        success: true,
        message: format!("Added Codex MCP server '{}'", name),
    })
}

/// Add a Codex MCP server to the user-level config ($CODEX_HOME/config.toml).
pub fn add_codex_server(
    name: &str,
    config: &serde_json::Value,
) -> Result<PluginOperationResult, String> {
    let config_path = crate::storage::cli_config::codex_config_path()?;
    log::debug!(
        "[codex_mcp] add_codex_server: name={}, path={}",
        name,
        config_path.display()
    );
    add_codex_server_to_path(name, config, &config_path)
}

/// Internal: remove a Codex MCP server from a specific config file path.
fn remove_codex_server_from_path(
    name: &str,
    config_path: &std::path::Path,
) -> Result<PluginOperationResult, String> {
    let content = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;

    let mut doc: toml_edit::DocumentMut = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("TOML parse error in {}: {}", config_path.display(), e))?;

    let mcp_table = doc
        .get_mut("mcp_servers")
        .and_then(|item| item.as_table_mut())
        .ok_or_else(|| format!("No [mcp_servers] table found in {}", config_path.display()))?;

    if mcp_table.remove(name).is_none() {
        return Err(format!("Server '{}' not found in [mcp_servers]", name));
    }

    // If [mcp_servers] is now empty, remove it entirely
    if mcp_table.is_empty() {
        doc.remove("mcp_servers");
    }

    let output = doc.to_string();
    std::fs::write(config_path, &output)
        .map_err(|e| format!("Failed to write {}: {}", config_path.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(config_path, std::fs::Permissions::from_mode(0o600));
    }

    log::debug!(
        "[codex_mcp] removed server '{}' from {}",
        name,
        config_path.display()
    );

    Ok(PluginOperationResult {
        success: true,
        message: format!("Removed Codex MCP server '{}'", name),
    })
}

/// Remove a Codex MCP server from the specified scope config.
///
/// - scope="user" → remove from $CODEX_HOME/config.toml
/// - scope="project" → not supported yet
pub fn remove_codex_server(
    name: &str,
    scope: &str,
    _cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    match scope {
        "project" => Err("Project-scope MCP removal is not supported yet".into()),
        "user" => {
            let config_path = crate::storage::cli_config::codex_config_path()?;
            log::debug!(
                "[codex_mcp] remove_codex_server: name={}, path={}",
                name,
                config_path.display()
            );
            remove_codex_server_from_path(name, &config_path)
        }
        _ => Err(format!("Unknown scope: {}", scope)),
    }
}

// ── Grok MCP support ──

fn grok_home_path() -> Result<std::path::PathBuf, String> {
    if let Some(path) = std::env::var_os("GROK_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Ok(path);
    }
    crate::storage::home_dir()
        .map(|home| std::path::PathBuf::from(home).join(".grok"))
        .ok_or_else(|| "Home directory is unavailable".to_string())
}

fn grok_mcp_config_path(scope: &str, cwd: Option<&str>) -> Result<std::path::PathBuf, String> {
    match scope {
        "user" => Ok(grok_home_path()?.join("config.toml")),
        "project" | "local" => {
            let cwd = cwd
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Project scope requires cwd".to_string())?;
            Ok(std::path::Path::new(cwd).join(".grok").join("config.toml"))
        }
        _ => Err(format!("Unsupported Grok MCP scope: {scope}")),
    }
}

fn parse_grok_mcp_entry(
    name: &str,
    table: &toml::Value,
    scope: &str,
    source_path: Option<&str>,
) -> ConfiguredMcpServer {
    let obj = table.as_table();
    let command = obj
        .and_then(|value| value.get("command"))
        .and_then(toml::Value::as_str)
        .map(String::from);
    let url = obj
        .and_then(|value| value.get("url"))
        .and_then(toml::Value::as_str)
        .map(String::from);
    let server_type = if command.is_some() {
        "stdio"
    } else if url.is_some() {
        "streamable-http"
    } else {
        "unknown"
    }
    .to_string();
    let args = obj
        .and_then(|value| value.get("args"))
        .and_then(toml::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(toml::Value::as_str)
                .map(redact_sensitive_arg)
                .collect()
        })
        .unwrap_or_default();
    let env_keys = obj
        .and_then(|value| value.get("env"))
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect())
        .unwrap_or_default();
    let header_keys = obj
        .and_then(|value| value.get("headers"))
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect())
        .unwrap_or_default();

    ConfiguredMcpServer {
        name: name.to_string(),
        server_type,
        scope: scope.to_string(),
        command,
        args,
        url,
        env_keys,
        header_keys,
        agent: "grok".into(),
        source_path: source_path.map(String::from),
    }
}

fn read_grok_mcp_servers(config_path: &std::path::Path, scope: &str) -> Vec<ConfiguredMcpServer> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let value = match toml::from_str::<toml::Value>(&content) {
        Ok(value) => value,
        Err(error) => {
            log::debug!(
                "[grok_mcp] skipping {}: parse error: {}",
                config_path.display(),
                error
            );
            return Vec::new();
        }
    };
    value
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .iter()
                .map(|(name, entry)| parse_grok_mcp_entry(name, entry, scope, config_path.to_str()))
                .collect()
        })
        .unwrap_or_default()
}

fn project_grok_config_paths(cwd: &str) -> Vec<std::path::PathBuf> {
    let cwd_path = std::path::PathBuf::from(cwd);
    let root = find_git_root(&cwd_path);
    let mut dirs = Vec::new();
    let mut current = cwd_path.as_path();
    loop {
        dirs.push(current.to_path_buf());
        if root.as_deref() == Some(current) {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    dirs.reverse();
    dirs.into_iter()
        .map(|dir| dir.join(".grok").join("config.toml"))
        .collect()
}

/// List Grok's native MCP servers from ~/.grok/config.toml and the effective
/// project .grok/config.toml chain. Project definitions override same-name
/// user definitions, matching Grok's loader precedence.
pub fn list_grok_configured(cwd: Option<&str>) -> Vec<ConfiguredMcpServer> {
    let mut effective = std::collections::HashMap::<String, ConfiguredMcpServer>::new();
    if let Ok(user_path) = grok_mcp_config_path("user", None) {
        for server in read_grok_mcp_servers(&user_path, "user") {
            effective.insert(server.name.clone(), server);
        }
    }
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        for path in project_grok_config_paths(cwd) {
            for server in read_grok_mcp_servers(&path, "project") {
                effective.insert(server.name.clone(), server);
            }
        }
    }
    let mut servers: Vec<_> = effective.into_values().collect();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    log::debug!("[grok_mcp] list_grok_configured: {} total", servers.len());
    servers
}

fn validate_grok_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Server name cannot be empty".into());
    }
    if name.len() > 128
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        return Err(format!(
            "Server name '{}' contains invalid characters; only [a-zA-Z0-9_-] allowed",
            name
        ));
    }
    Ok(())
}

fn add_grok_server_to_path(
    name: &str,
    config: &serde_json::Value,
    config_path: &std::path::Path,
) -> Result<PluginOperationResult, String> {
    validate_grok_name(name)?;
    if !config.is_object() {
        return Err("Grok MCP config must be a JSON object".into());
    }
    let mut doc = match std::fs::read_to_string(config_path) {
        Ok(content) => content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| format!("TOML parse error in {}: {error}", config_path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => toml_edit::DocumentMut::new(),
        Err(error) => return Err(format!("Failed to read {}: {error}", config_path.display())),
    };
    if !doc.get("mcp_servers").is_some_and(|item| item.is_table()) {
        doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let mcp_table = doc["mcp_servers"]
        .as_table_mut()
        .ok_or_else(|| "Grok mcp_servers must be a table".to_string())?;
    mcp_table[name] = json_to_toml_edit_item(config);
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    }
    std::fs::write(config_path, doc.to_string())
        .map_err(|error| format!("Failed to write {}: {error}", config_path.display()))?;
    Ok(PluginOperationResult {
        success: true,
        message: format!("Added Grok MCP server '{name}'"),
    })
}

pub fn add_grok_server(
    name: &str,
    config: &serde_json::Value,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    let path = grok_mcp_config_path(scope, cwd)?;
    add_grok_server_to_path(name, config, &path)
}

pub fn remove_grok_server(
    name: &str,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    validate_grok_name(name)?;
    let path = grok_mcp_config_path(scope, cwd)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("TOML parse error in {}: {error}", path.display()))?;
    let mcp_table = doc
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_mut)
        .ok_or_else(|| format!("No [mcp_servers] table found in {}", path.display()))?;
    if mcp_table.remove(name).is_none() {
        return Err(format!("Server '{name}' not found in {}", path.display()));
    }
    if mcp_table.is_empty() {
        doc.remove("mcp_servers");
    }
    std::fs::write(&path, doc.to_string())
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    Ok(PluginOperationResult {
        success: true,
        message: format!("Removed Grok MCP server '{name}'"),
    })
}

// ── Pi MCP Registry Management ──

fn pi_mcp_config_path(scope: &str, cwd: Option<&str>) -> Result<std::path::PathBuf, String> {
    match scope {
        "user" => crate::storage::home_dir()
            .map(std::path::PathBuf::from)
            .map(|home| home.join(".pi").join("agent").join("mcp.json"))
            .ok_or_else(|| "Home directory is unavailable".to_string()),
        "project" | "local" => {
            let cwd_str = cwd
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| "Project scope requires cwd".to_string())?;
            Ok(std::path::Path::new(cwd_str).join(".pi").join("mcp.json"))
        }
        _ => Err(format!("Unsupported Pi MCP scope: {}", scope)),
    }
}

pub fn list_pi_configured(cwd: Option<&str>) -> Vec<ConfiguredMcpServer> {
    let mut servers = Vec::new();

    // 1. User scope: ~/.pi/agent/mcp.json
    if let Ok(user_path) = pi_mcp_config_path("user", None) {
        if let Ok(content) = std::fs::read_to_string(&user_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(mcp_map) = json
                    .get("mcpServers")
                    .or_else(|| json.get("mcp_servers"))
                    .and_then(|v| v.as_object())
                {
                    for (name, config) in mcp_map {
                        let mut entry = parse_mcp_entry(name, config, "user", user_path.to_str());
                        entry.agent = "pi".into();
                        servers.push(entry);
                    }
                }
            }
        }
    }

    // 2. Project scope: <cwd>/.pi/mcp.json
    if let Some(cwd_str) = cwd.filter(|s| !s.trim().is_empty()) {
        if let Ok(proj_path) = pi_mcp_config_path("project", Some(cwd_str)) {
            if let Ok(content) = std::fs::read_to_string(&proj_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(mcp_map) = json
                        .get("mcpServers")
                        .or_else(|| json.get("mcp_servers"))
                        .and_then(|v| v.as_object())
                    {
                        for (name, config) in mcp_map {
                            let mut entry =
                                parse_mcp_entry(name, config, "project", proj_path.to_str());
                            entry.agent = "pi".into();
                            servers.push(entry);
                        }
                    }
                }
            }
        }
    }

    // 3. Shared global: ~/.config/mcp/mcp.json (readable by pi-mcp-adapter)
    if let Some(home) = crate::storage::home_dir() {
        let shared_global = std::path::PathBuf::from(home)
            .join(".config")
            .join("mcp")
            .join("mcp.json");
        if let Ok(content) = std::fs::read_to_string(&shared_global) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(mcp_map) = json
                    .get("mcpServers")
                    .or_else(|| json.get("mcp_servers"))
                    .and_then(|v| v.as_object())
                {
                    for (name, config) in mcp_map {
                        let mut entry =
                            parse_mcp_entry(name, config, "shared", shared_global.to_str());
                        entry.agent = "pi".into();
                        servers.push(entry);
                    }
                    log::debug!(
                        "[pi_mcp] shared global servers from {}: {}",
                        shared_global.display(),
                        json.get("mcpServers")
                            .or_else(|| json.get("mcp_servers"))
                            .and_then(|v| v.as_object())
                            .map(|m| m.len())
                            .unwrap_or(0)
                    );
                }
            }
        }
    }

    // 4. Shared project: <cwd>/.mcp.json (readable by both Claude and pi-mcp-adapter)
    if let Some(cwd_str) = cwd.filter(|s| !s.trim().is_empty()) {
        let shared_project = std::path::PathBuf::from(cwd_str).join(".mcp.json");
        if let Ok(content) = std::fs::read_to_string(&shared_project) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let mcp_obj = json
                    .get("mcpServers")
                    .and_then(|v| v.as_object())
                    .or_else(|| json.as_object());
                if let Some(entries) = mcp_obj {
                    let is_wrapper = entries.len() == 1 && entries.contains_key("mcpServers");
                    if !is_wrapper {
                        for (name, config) in entries {
                            let mut entry =
                                parse_mcp_entry(name, config, "shared", shared_project.to_str());
                            entry.agent = "pi".into();
                            servers.push(entry);
                        }
                    }
                }
            }
        }
    }

    servers
}

pub fn add_pi_server(
    name: &str,
    config: &serde_json::Value,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Server name cannot be empty".into());
    }

    let config_path = pi_mcp_config_path(scope, cwd)?;
    let mut root: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !root.is_object() {
        root = serde_json::json!({});
    }

    let mcp_obj = root
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    if !mcp_obj.is_object() {
        *mcp_obj = serde_json::json!({});
    }

    mcp_obj
        .as_object_mut()
        .unwrap()
        .insert(name.to_string(), config.clone());

    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let content =
        serde_json::to_string_pretty(&root).map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&config_path, content)
        .map_err(|e| format!("Failed to write {}: {}", config_path.display(), e))?;

    Ok(PluginOperationResult {
        success: true,
        message: format!("Added Pi MCP server '{}' ({})", name, scope),
    })
}

pub fn remove_pi_server(
    name: &str,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    let name = name.trim();
    let config_path = pi_mcp_config_path(scope, cwd)?;
    if !config_path.exists() {
        return Err(format!(
            "Pi MCP config file not found: {}",
            config_path.display()
        ));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;
    let mut root: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON in {}: {}", config_path.display(), e))?;

    let mcp_map = if root.get("mcpServers").is_some() {
        root.get_mut("mcpServers")
            .and_then(serde_json::Value::as_object_mut)
    } else {
        root.get_mut("mcp_servers")
            .and_then(serde_json::Value::as_object_mut)
    };

    let removed = mcp_map
        .map(|map| map.remove(name).is_some())
        .unwrap_or(false);

    if !removed {
        return Err(format!(
            "Server '{}' not found in {}",
            name,
            config_path.display()
        ));
    }

    let output =
        serde_json::to_string_pretty(&root).map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&config_path, output)
        .map_err(|e| format!("Failed to write {}: {}", config_path.display(), e))?;

    Ok(PluginOperationResult {
        success: true,
        message: format!("Removed Pi MCP server '{}' ({})", name, scope),
    })
}

pub fn toggle_pi_server_config(
    name: &str,
    enabled: bool,
    scope: &str,
    cwd: Option<&str>,
) -> Result<PluginOperationResult, String> {
    let name = name.trim();
    let config_path = pi_mcp_config_path(scope, cwd)?;
    if !config_path.exists() {
        return Err(format!(
            "Pi MCP config file not found: {}",
            config_path.display()
        ));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;
    let mut root: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON in {}: {}", config_path.display(), e))?;

    let mcp_map = if root.get("mcpServers").is_some() {
        root.get_mut("mcpServers")
            .and_then(serde_json::Value::as_object_mut)
    } else {
        root.get_mut("mcp_servers")
            .and_then(serde_json::Value::as_object_mut)
    };

    let found = mcp_map
        .and_then(|map| map.get_mut(name))
        .and_then(|server| server.as_object_mut());

    let server_obj = match found {
        Some(obj) => obj,
        None => {
            return Err(format!(
                "Server '{}' not found in {}",
                name,
                config_path.display()
            ))
        }
    };

    server_obj.insert("disabled".to_string(), serde_json::json!(!enabled));

    let output =
        serde_json::to_string_pretty(&root).map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&config_path, output)
        .map_err(|e| format!("Failed to write {}: {}", config_path.display(), e))?;

    Ok(PluginOperationResult {
        success: true,
        message: format!("Toggled Pi MCP server '{}' to enabled={}", name, enabled),
    })
}

/// Check whether `pi-mcp-adapter` is installed as a Pi extension.
///
/// Reads `~/.pi/agent/settings.json` and `<cwd>/.pi/settings.json` for the
/// `packages` array and looks for an entry whose source matches
/// `pi-mcp-adapter` or an npm reference for that package.
pub fn is_pi_mcp_adapter_installed(cwd: Option<&str>) -> bool {
    let home = match crate::storage::home_dir() {
        Some(h) => std::path::PathBuf::from(h),
        None => return false,
    };

    let paths = [
        Some(home.join(".pi").join("agent").join("settings.json")),
        cwd.filter(|s| !s.trim().is_empty()).map(|c| {
            std::path::PathBuf::from(c)
                .join(".pi")
                .join("settings.json")
        }),
    ];

    for path in paths.into_iter().flatten() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(packages) = root.get("packages").and_then(|v| v.as_array()) {
                    for entry in packages {
                        let source = match entry {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Object(obj) => obj
                                .get("source")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            _ => continue,
                        };
                        if source == "pi-mcp-adapter"
                            || source == "npm:pi-mcp-adapter"
                            || source.starts_with("npm:pi-mcp-adapter@")
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name() {
        assert!(validate_name("my-server").is_ok());
        assert!(validate_name("test_server_123").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("test\x00").is_err());
        assert!(validate_name(&"a".repeat(129)).is_err());
    }

    #[test]
    fn test_validate_scope() {
        assert!(validate_scope("local").is_ok());
        assert!(validate_scope("user").is_ok());
        assert!(validate_scope("project").is_ok());
        assert!(validate_scope("global").is_err());
        assert!(validate_scope("").is_err());
    }

    #[test]
    fn test_redact_sensitive_arg() {
        assert_eq!(redact_sensitive_arg("hello"), "hello");
        assert_eq!(redact_sensitive_arg("my-api-token-123"), "***");
        assert_eq!(redact_sensitive_arg("GITHUB_KEY"), "***");
        assert_eq!(redact_sensitive_arg("Bearer xyz"), "***");
        assert_eq!(redact_sensitive_arg("some-secret-value"), "***");
        assert_eq!(redact_sensitive_arg("password=abc"), "***");
        assert_eq!(redact_sensitive_arg("normal-arg"), "normal-arg");
    }

    #[test]
    fn test_parse_mcp_entry_stdio() {
        let config = serde_json::json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"],
            "env": {
                "NODE_ENV": "production",
                "API_TOKEN": "secret123"
            }
        });

        let entry = parse_mcp_entry("filesystem", &config, "user", None);
        assert_eq!(entry.name, "filesystem");
        assert_eq!(entry.server_type, "stdio");
        assert_eq!(entry.scope, "user");
        assert_eq!(entry.command, Some("npx".to_string()));
        assert_eq!(entry.args.len(), 3);
        assert_eq!(entry.env_keys.len(), 2);
        assert!(entry.env_keys.contains(&"NODE_ENV".to_string()));
        assert!(entry.env_keys.contains(&"API_TOKEN".to_string()));
    }

    #[test]
    fn test_parse_mcp_entry_http() {
        let config = serde_json::json!({
            "type": "http",
            "url": "https://example.com/mcp",
            "headers": {
                "Authorization": "Bearer xyz",
                "X-Custom": "val"
            }
        });

        let entry = parse_mcp_entry("remote-server", &config, "project", None);
        assert_eq!(entry.name, "remote-server");
        assert_eq!(entry.server_type, "http");
        assert_eq!(entry.scope, "project");
        assert_eq!(entry.url, Some("https://example.com/mcp".to_string()));
        assert_eq!(entry.header_keys.len(), 2);
        assert!(entry.command.is_none());
    }

    #[test]
    fn test_parse_mcp_entry_default_type() {
        let config = serde_json::json!({
            "command": "my-server"
        });

        let entry = parse_mcp_entry("test", &config, "local", None);
        assert_eq!(entry.server_type, "stdio"); // default
    }

    #[test]
    fn test_redact_args() {
        let config = serde_json::json!({
            "type": "stdio",
            "command": "server",
            "args": ["--port", "8080", "--api-token", "secret123"]
        });

        let entry = parse_mcp_entry("test", &config, "user", None);
        assert_eq!(entry.args[0], "--port");
        assert_eq!(entry.args[1], "8080");
        assert_eq!(entry.args[2], "***"); // contains "token"
        assert_eq!(entry.args[3], "***"); // the actual secret value doesn't match but "secret123" contains "secret"
    }

    // ── Codex MCP tests ──

    #[test]
    fn test_codex_mcp_add_preserves_comments() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        // Write initial config with comments
        std::fs::write(
            &config_path,
            "# Main codex config\nmodel = \"o4-mini\"\n\n# My servers\n[mcp_servers.existing]\ncommand = \"old-server\"\n",
        )
        .unwrap();

        let result = add_codex_server_to_path(
            "new-server",
            &serde_json::json!({"command": "my-cmd", "args": ["--flag"]}),
            &config_path,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().success);

        let content = std::fs::read_to_string(&config_path).unwrap();
        // Comments should be preserved
        assert!(content.contains("# Main codex config"));
        assert!(content.contains("# My servers"));
        // Both servers should exist
        assert!(content.contains("[mcp_servers.existing]"));
        assert!(content.contains("[mcp_servers.new-server]"));
        assert!(content.contains("my-cmd"));
    }

    #[test]
    fn test_codex_mcp_add_nested_headers() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let result = add_codex_server_to_path(
            "remote",
            &serde_json::json!({
                "url": "https://example.com/mcp",
                "http_headers": {"X-Custom": "val1"},
                "env_http_headers": {"Authorization": "AUTH_TOKEN_VAR"},
                "bearer_token_env_var": "MY_BEARER"
            }),
            &config_path,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().success);

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("https://example.com/mcp"));
        assert!(content.contains("X-Custom"));
        assert!(content.contains("AUTH_TOKEN_VAR"));
        assert!(content.contains("MY_BEARER"));
    }

    #[test]
    fn test_codex_mcp_remove_user() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        // Add two servers
        std::fs::write(
            &config_path,
            "[mcp_servers.alpha]\ncommand = \"alpha-cmd\"\n\n[mcp_servers.beta]\ncommand = \"beta-cmd\"\n",
        )
        .unwrap();

        let result = remove_codex_server_from_path("alpha", &config_path);
        assert!(result.is_ok());
        assert!(result.unwrap().success);

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("alpha"));
        assert!(content.contains("beta"));
    }

    #[test]
    fn test_codex_mcp_remove_project_unsupported() {
        let result = remove_codex_server("test", "project", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported"));
    }

    #[test]
    fn test_codex_mcp_list_user_and_project() {
        let tmp = tempfile::TempDir::new().unwrap();

        // User config
        let user_config = tmp.path().join("user_config.toml");
        std::fs::write(
            &user_config,
            "[mcp_servers.user-srv]\ncommand = \"user-cmd\"\n",
        )
        .unwrap();

        // Project config (simulating a project layer)
        let project_dir = tmp.path().join("project");
        std::fs::create_dir_all(project_dir.join(".codex")).unwrap();
        let project_config = project_dir.join(".codex").join("config.toml");
        std::fs::write(
            &project_config,
            "[mcp_servers.proj-srv]\nurl = \"https://proj.example.com\"\n",
        )
        .unwrap();

        let servers = list_codex_configured_with_paths(&user_config, &[project_config]);

        assert_eq!(servers.len(), 2);

        let user_srv = servers.iter().find(|s| s.name == "user-srv").unwrap();
        assert_eq!(user_srv.scope, "user");
        assert_eq!(user_srv.agent, "codex");
        assert_eq!(user_srv.server_type, "stdio");

        let proj_srv = servers.iter().find(|s| s.name == "proj-srv").unwrap();
        assert_eq!(proj_srv.scope, "project");
        assert_eq!(proj_srv.agent, "codex");
        assert_eq!(proj_srv.server_type, "streamable-http");
    }

    #[test]
    fn test_codex_mcp_add_invalid_stdio_with_url() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        // Has command (stdio) but also has url — invalid
        let result = add_codex_server_to_path(
            "bad",
            &serde_json::json!({"command": "cmd", "url": "https://x.com"}),
            &config_path,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not both"));
    }

    #[test]
    fn test_codex_mcp_add_invalid_http_with_command() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        // Has url (http) but also tries to use stdio-only field 'args'
        let result = add_codex_server_to_path(
            "bad",
            &serde_json::json!({"url": "https://x.com", "args": ["--flag"]}),
            &config_path,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("only valid for stdio"));
    }

    #[test]
    fn test_codex_mcp_project_effective_merge() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Root layer: defines "shared" with command=old
        let root_config = tmp.path().join("root.toml");
        std::fs::write(
            &root_config,
            "[mcp_servers.shared]\ncommand = \"old-cmd\"\n",
        )
        .unwrap();

        // CWD layer: defines "shared" with command=new (overrides)
        let cwd_config = tmp.path().join("cwd.toml");
        std::fs::write(&cwd_config, "[mcp_servers.shared]\ncommand = \"new-cmd\"\n").unwrap();

        let project_servers = collect_project_codex_servers(&[root_config, cwd_config]);

        assert_eq!(project_servers.len(), 1);
        let srv = &project_servers[0];
        assert_eq!(srv.name, "shared");
        assert_eq!(srv.command, Some("new-cmd".to_string()));
    }

    #[test]
    fn test_codex_mcp_user_and_project_same_name() {
        let tmp = tempfile::TempDir::new().unwrap();

        // User config with "overlap"
        let user_config = tmp.path().join("user.toml");
        std::fs::write(
            &user_config,
            "[mcp_servers.overlap]\ncommand = \"user-ver\"\n",
        )
        .unwrap();

        // Project config with "overlap"
        let proj_config = tmp.path().join("proj.toml");
        std::fs::write(
            &proj_config,
            "[mcp_servers.overlap]\ncommand = \"proj-ver\"\n",
        )
        .unwrap();

        let servers = list_codex_configured_with_paths(&user_config, &[proj_config]);

        // Both should appear (no cross-scope dedup)
        assert_eq!(servers.len(), 2);
        let scopes: Vec<&str> = servers.iter().map(|s| s.scope.as_str()).collect();
        assert!(scopes.contains(&"user"));
        assert!(scopes.contains(&"project"));
    }

    #[test]
    fn test_codex_mcp_add_allows_timeout_fields() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let result = add_codex_server_to_path(
            "with-timeout",
            &serde_json::json!({
                "command": "my-cmd",
                "startup_timeout_sec": 30,
                "tool_timeout_sec": 60
            }),
            &config_path,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().success);

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("startup_timeout_sec"));
        assert!(content.contains("tool_timeout_sec"));
    }

    #[test]
    fn test_codex_mcp_add_allows_oauth_resource_on_http() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let result = add_codex_server_to_path(
            "oauth-srv",
            &serde_json::json!({
                "url": "https://example.com/mcp",
                "oauth_resource": "https://auth.example.com"
            }),
            &config_path,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().success);

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("oauth_resource"));
    }

    #[test]
    fn test_codex_mcp_add_rejects_bearer_token_plaintext() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let result = add_codex_server_to_path(
            "bad",
            &serde_json::json!({
                "url": "https://example.com",
                "bearer_token": "sk-secret"
            }),
            &config_path,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bearer_token_env_var"));
    }

    #[test]
    fn test_codex_mcp_add_name_with_dot() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let result = add_codex_server_to_path(
            "my.server",
            &serde_json::json!({"command": "cmd"}),
            &config_path,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'.'"));
    }

    #[test]
    fn test_codex_mcp_add_name_validation() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        // Space in name
        let result = add_codex_server_to_path(
            "my server",
            &serde_json::json!({"command": "cmd"}),
            &config_path,
        );
        assert!(result.is_err());

        // Slash in name
        let result = add_codex_server_to_path(
            "org/server",
            &serde_json::json!({"command": "cmd"}),
            &config_path,
        );
        assert!(result.is_err());

        // Empty name
        let result =
            add_codex_server_to_path("", &serde_json::json!({"command": "cmd"}), &config_path);
        assert!(result.is_err());

        // Valid names
        let result = add_codex_server_to_path(
            "my-server_v2",
            &serde_json::json!({"command": "cmd"}),
            &config_path,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_codex_mcp_add_tools_passthrough() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let tools_value = serde_json::json!([
            {"name": "read_file", "description": "Read a file"},
            {"name": "write_file", "description": "Write a file"}
        ]);

        let result = add_codex_server_to_path(
            "with-tools",
            &serde_json::json!({
                "command": "my-cmd",
                "tools": tools_value
            }),
            &config_path,
        );
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("tools"));
        // The tools field should have been written (passthrough)
        assert!(content.contains("read_file"));
        assert!(content.contains("write_file"));
    }

    #[test]
    fn test_codex_mcp_remove_existing_quoted_key() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        // Write a config with a quoted key (e.g., hand-written with special chars)
        std::fs::write(
            &config_path,
            "[mcp_servers]\n\n[mcp_servers.\"my-special-server\"]\ncommand = \"special\"\n\n[mcp_servers.normal]\ncommand = \"norm\"\n",
        )
        .unwrap();

        // Remove should work for the quoted key name (just the bare name, not quotes)
        let result = remove_codex_server_from_path("my-special-server", &config_path);
        assert!(result.is_ok());
        assert!(result.unwrap().success);

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("special"));
        assert!(content.contains("normal"));
    }

    #[test]
    fn test_pi_mcp_crud_and_toggle() {
        let tmp = tempfile::TempDir::new().unwrap();
        let proj_dir = tmp.path().to_str().unwrap();

        // 1. Add Pi MCP server to project scope
        let server_name = "test-pi-server";
        let config = serde_json::json!({
            "command": "node",
            "args": ["server.js"],
            "env": {
                "TEST_ENV": "1"
            }
        });

        let add_res = add_pi_server(server_name, &config, "project", Some(proj_dir));
        assert!(add_res.is_ok());

        // 2. List configured Pi MCP servers
        let configured = list_pi_configured(Some(proj_dir));
        let found = configured.iter().find(|s| s.name == server_name);
        assert!(found.is_some());
        let s = found.unwrap();
        assert_eq!(s.agent, "pi");
        assert_eq!(s.scope, "project");
        assert_eq!(s.command.as_deref(), Some("node"));

        // 3. Toggle disabled
        let toggle_res = toggle_pi_server_config(server_name, false, "project", Some(proj_dir));
        assert!(toggle_res.is_ok());

        let settings_path = tmp.path().join(".pi").join("mcp.json");
        let content = std::fs::read_to_string(&settings_path).unwrap();
        assert!(content.contains("\"disabled\": true"));

        // 4. Remove Pi server
        let remove_res = remove_pi_server(server_name, "project", Some(proj_dir));
        assert!(remove_res.is_ok());

        let configured_after = list_pi_configured(Some(proj_dir));
        assert!(configured_after.iter().all(|s| s.name != server_name));
    }
}
