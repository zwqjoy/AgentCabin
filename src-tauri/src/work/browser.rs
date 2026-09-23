//! Work-owned network access configuration and multi-provider health checks.
//!
//! Network credentials intentionally live outside Pi/Code settings. The
//! frontend only receives redacted status; the API key is read at spawn time
//! and injected into the isolated Work network adapter's environment.

use std::fs;
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::work::models::{
    WorkBrowserConfig, WorkBrowserHealth, WorkBrowserHealthStatus, WorkBrowserSummary,
};
use crate::work::paths::WorkPaths;

pub const DUCKDUCKGO_PROVIDER: &str = "duckduckgo";
pub const TAVILY_PROVIDER: &str = "tavily";
pub const EXA_PROVIDER: &str = "exa";
pub const SEARXNG_PROVIDER: &str = "searxng";
pub const ANYSEARCH_PROVIDER: &str = "anysearch";

pub const WORK_BROWSER_ADAPTER_FILENAME: &str = "agentcabin-work-browser.mjs";
const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_SECRETS_BYTES: u64 = 64 * 1024;
const DEFAULT_MAX_RESULTS: u32 = 5;
const EXA_HEALTH_URL: &str = "https://api.exa.ai/search";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserConfigFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_provider")]
    provider: String,
    #[serde(default = "default_max_results")]
    max_results: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    endpoint_url: Option<String>,
    #[serde(default)]
    allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct BrowserSecretsFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    providers: std::collections::HashMap<String, BrowserProviderSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BrowserProviderSecret {
    #[serde(default)]
    api_key: Option<String>,
}

pub fn get_config() -> Result<WorkBrowserSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    get_config_with_paths(&paths)
}

pub fn get_config_with_paths(paths: &WorkPaths) -> Result<WorkBrowserSummary, String> {
    paths.ensure_layout()?;
    let config = read_config(paths)?;
    let configured = is_provider_configured(&config, paths)?;
    let adapter_installed = adapter_installed(paths);
    Ok(summary(&config, configured, adapter_installed, paths))
}

pub fn normalize_allowed_hosts(hosts: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for host in hosts {
        for item in host.split([',', '\n', '，', ' ', ';']) {
            let mut s = item.trim().to_ascii_lowercase();
            if s.is_empty() {
                continue;
            }
            if let Some(rest) = s.strip_prefix("http://") {
                s = rest.to_string();
            } else if let Some(rest) = s.strip_prefix("https://") {
                s = rest.to_string();
            }
            let is_cidr = if let Some((ip_str, mask_str)) = s.split_once('/') {
                ip_str.parse::<std::net::IpAddr>().is_ok() && mask_str.parse::<u32>().is_ok()
            } else {
                false
            };
            if !is_cidr {
                if let Some(pos) = s.find('/') {
                    s.truncate(pos);
                }
                if !s.starts_with('[') {
                    if let Some((h, port)) = s.split_once(':') {
                        if port.parse::<u16>().is_ok() {
                            s = h.to_string();
                        }
                    }
                }
            }
            let s = s.trim().to_string();
            if !s.is_empty() && !normalized.contains(&s) {
                normalized.push(s);
            }
        }
    }
    normalized
}

pub fn save_config(
    provider: &str,
    enabled: bool,
    max_results: Option<u32>,
    api_key: Option<&str>,
    endpoint_url: Option<&str>,
    allowed_hosts: Option<Vec<String>>,
) -> Result<WorkBrowserSummary, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    save_config_with_paths(
        &paths,
        provider,
        enabled,
        max_results,
        api_key,
        endpoint_url,
        allowed_hosts,
    )
}

pub fn save_config_with_paths(
    paths: &WorkPaths,
    provider: &str,
    enabled: bool,
    max_results: Option<u32>,
    api_key: Option<&str>,
    endpoint_url: Option<&str>,
    allowed_hosts: Option<Vec<String>>,
) -> Result<WorkBrowserSummary, String> {
    paths.ensure_layout()?;
    let provider = normalize_provider(provider)?;
    let max_results = normalize_max_results(max_results.unwrap_or(DEFAULT_MAX_RESULTS))?;
    let mut config = read_config(paths)?;
    config.enabled = enabled;
    config.provider = provider.to_string();
    config.max_results = max_results;
    config.endpoint_url = endpoint_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(hosts) = allowed_hosts {
        config.allowed_hosts = normalize_allowed_hosts(&hosts);
    }
    write_config(paths, &config)?;

    crate::work::browser_operator::browser_operator_manager().reload_worker();

    if let Some(api_key) = api_key {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            remove_api_key(paths, provider)?;
        } else {
            validate_secret(api_key)?;
            write_api_key(paths, provider, api_key)?;
        }
    }
    let configured = is_provider_configured(&config, paths)?;
    Ok(summary(
        &config,
        configured,
        adapter_installed(paths),
        paths,
    ))
}

pub async fn test() -> Result<WorkBrowserHealth, String> {
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    test_with_paths(&paths).await
}

pub async fn test_with_paths(paths: &WorkPaths) -> Result<WorkBrowserHealth, String> {
    let config = read_config(paths)?;
    let checked_at = Utc::now().to_rfc3339();
    if !config.enabled {
        return Ok(health(
            WorkBrowserHealthStatus::Disabled,
            &config.provider,
            "网络访问未启用",
            checked_at,
            0,
        ));
    }

    let provider = config.provider.as_str();
    let api_key = read_api_key_for_provider(paths, provider)?;
    let proxy_url = crate::work::proxy::work_proxy_url();

    match provider {
        DUCKDUCKGO_PROVIDER => {
            let started = Instant::now();
            let client_builder = crate::work::web::with_proxy(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(12))
                    .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
                proxy_url.as_deref(),
            )
            .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let client = client_builder
                .build()
                .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let response = client
                .post("https://html.duckduckgo.com/html/")
                .form(&[("q", "AgentCabin")])
                .send()
                .await;
            let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            match response {
                Ok(response) if response.status().is_success() => Ok(health(
                    WorkBrowserHealthStatus::Healthy,
                    provider,
                    "DuckDuckGo（免 Key）连接正常",
                    checked_at,
                    latency_ms,
                )),
                Ok(response) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("DuckDuckGo 返回 HTTP {}", response.status().as_u16()),
                    checked_at,
                    latency_ms,
                )),
                Err(error) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("DuckDuckGo 请求失败: {}", safe_network_error(&error)),
                    checked_at,
                    latency_ms,
                )),
            }
        }
        TAVILY_PROVIDER => {
            let Some(api_key) = api_key else {
                return Ok(health(
                    WorkBrowserHealthStatus::Unconfigured,
                    provider,
                    "未配置 Tavily API key",
                    checked_at,
                    0,
                ));
            };
            let started = Instant::now();
            let response = crate::work::web::search::send_tavily_request_with_retry(
                &json!({
                    "api_key": api_key,
                    "query": "AgentCabin Work network access health check",
                    "max_results": 1,
                    "include_answer": false,
                }),
                proxy_url.as_deref(),
                std::time::Duration::from_secs(15),
            )
            .await;
            let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            match response {
                Ok(response) if response.status().is_success() => Ok(health(
                    WorkBrowserHealthStatus::Healthy,
                    provider,
                    "Native Web（Tavily）连接正常",
                    checked_at,
                    latency_ms,
                )),
                Ok(response) => {
                    let status = response.status();
                    Ok(health(
                        WorkBrowserHealthStatus::Failed,
                        provider,
                        &format!("Tavily 返回 HTTP {}", status.as_u16()),
                        checked_at,
                        latency_ms,
                    ))
                }
                Err(error) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("Tavily 请求失败: {error}"),
                    checked_at,
                    latency_ms,
                )),
            }
        }
        EXA_PROVIDER => {
            let Some(api_key) = api_key else {
                return Ok(health(
                    WorkBrowserHealthStatus::Unconfigured,
                    provider,
                    "未配置 Exa API key",
                    checked_at,
                    0,
                ));
            };
            let started = Instant::now();
            let client_builder = crate::work::web::with_proxy(
                reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)),
                proxy_url.as_deref(),
            )
            .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let client = client_builder
                .build()
                .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let response = client
                .post(EXA_HEALTH_URL)
                .header("x-api-key", &api_key)
                .header("Content-Type", "application/json")
                .json(&json!({
                    "query": "AgentCabin",
                    "numResults": 1
                }))
                .send()
                .await;
            let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            match response {
                Ok(response) if response.status().is_success() => Ok(health(
                    WorkBrowserHealthStatus::Healthy,
                    provider,
                    "Exa 连接正常",
                    checked_at,
                    latency_ms,
                )),
                Ok(response) => {
                    let status = response.status();
                    Ok(health(
                        WorkBrowserHealthStatus::Failed,
                        provider,
                        &format!("Exa 返回 HTTP {}", status.as_u16()),
                        checked_at,
                        latency_ms,
                    ))
                }
                Err(error) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("Exa 请求失败: {}", safe_network_error(&error)),
                    checked_at,
                    latency_ms,
                )),
            }
        }
        SEARXNG_PROVIDER => {
            let endpoint = config.endpoint_url.as_deref().unwrap_or("").trim();
            if endpoint.is_empty() {
                return Ok(health(
                    WorkBrowserHealthStatus::Unconfigured,
                    provider,
                    "未配置 SearXNG 实例地址 (URL)",
                    checked_at,
                    0,
                ));
            }
            let target_url = format!(
                "{}/search?format=json&q=AgentCabin",
                endpoint.trim_end_matches('/')
            );
            let started = Instant::now();
            let client_builder = crate::work::web::with_proxy(
                reqwest::Client::builder().timeout(std::time::Duration::from_secs(12)),
                proxy_url.as_deref(),
            )
            .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let client = client_builder
                .build()
                .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let mut req = client.get(&target_url);
            if let Some(key) = api_key.as_ref() {
                req = req.header("Authorization", format!("Bearer {key}"));
            }
            let response = req.send().await;
            let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            match response {
                Ok(response) if response.status().is_success() => Ok(health(
                    WorkBrowserHealthStatus::Healthy,
                    provider,
                    "SearXNG 实例连接正常",
                    checked_at,
                    latency_ms,
                )),
                Ok(response) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("SearXNG 返回 HTTP {}", response.status().as_u16()),
                    checked_at,
                    latency_ms,
                )),
                Err(error) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("SearXNG 请求失败: {}", safe_network_error(&error)),
                    checked_at,
                    latency_ms,
                )),
            }
        }
        ANYSEARCH_PROVIDER => {
            let started = Instant::now();
            let client_builder = crate::work::web::with_proxy(
                reqwest::Client::builder().timeout(std::time::Duration::from_secs(12)),
                proxy_url.as_deref(),
            )
            .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let client = client_builder
                .build()
                .map_err(|error| format!("创建网络访问健康检查客户端失败: {error}"))?;
            let response = client
                .get("https://html.duckduckgo.com/html/?q=AgentCabin")
                .send()
                .await;
            let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            match response {
                Ok(response) if response.status().is_success() => Ok(health(
                    WorkBrowserHealthStatus::Healthy,
                    provider,
                    "AnySearch 连接正常",
                    checked_at,
                    latency_ms,
                )),
                Ok(response) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("AnySearch 返回 HTTP {}", response.status().as_u16()),
                    checked_at,
                    latency_ms,
                )),
                Err(error) => Ok(health(
                    WorkBrowserHealthStatus::Failed,
                    provider,
                    &format!("AnySearch 请求失败: {}", safe_network_error(&error)),
                    checked_at,
                    latency_ms,
                )),
            }
        }
        _ => Ok(health(
            WorkBrowserHealthStatus::Failed,
            provider,
            "不支持的网络访问 provider",
            checked_at,
            0,
        )),
    }
}

/// Return the in-memory runtime tuple used by the Pi launch context. Never
/// serialize the second element or pass it to the frontend.
pub fn runtime(paths: &WorkPaths) -> Result<(WorkBrowserConfig, Option<String>), String> {
    paths.ensure_layout()?;
    let config = read_config(paths)?;
    let api_key = if config.enabled {
        read_api_key_for_provider(paths, &config.provider)?
    } else {
        None
    };
    Ok((config.into(), api_key))
}

pub fn adapter_installed(paths: &WorkPaths) -> bool {
    paths
        .work_extensions_dir()
        .join(WORK_BROWSER_ADAPTER_FILENAME)
        .is_file()
}

pub fn auth_kind_for_provider(provider: &str) -> &'static str {
    match provider.trim().to_ascii_lowercase().as_str() {
        DUCKDUCKGO_PROVIDER => "anonymous",
        TAVILY_PROVIDER => "api_key",
        EXA_PROVIDER => "api_key",
        SEARXNG_PROVIDER => "self_hosted_endpoint",
        ANYSEARCH_PROVIDER => "anonymous",
        _ => "api_key",
    }
}

fn is_provider_configured(config: &BrowserConfigFile, paths: &WorkPaths) -> Result<bool, String> {
    match config.provider.as_str() {
        DUCKDUCKGO_PROVIDER | ANYSEARCH_PROVIDER => Ok(true),
        SEARXNG_PROVIDER => Ok(config
            .endpoint_url
            .as_ref()
            .map(|u| !u.trim().is_empty())
            .unwrap_or(false)),
        _ => Ok(read_api_key_for_provider(paths, &config.provider)?.is_some()),
    }
}

fn summary(
    config: &BrowserConfigFile,
    configured: bool,
    adapter_installed: bool,
    paths: &WorkPaths,
) -> WorkBrowserSummary {
    let browser_runtime = crate::work::browser_operator::runtime::status_with_paths(paths);
    let pi_runtime_available = config.enabled && configured && adapter_installed;
    let dsh_runtime_available = false; // Kept for older frontend clients.
    WorkBrowserSummary {
        enabled: config.enabled,
        provider: config.provider.clone(),
        configured,
        adapter_installed,
        pi_runtime_available,
        dsh_runtime_available,
        // Keep the legacy aggregate meaningful for clients that do not yet
        // render the runtime matrix.
        runtime_available: pi_runtime_available || dsh_runtime_available,
        browser_runtime_node_available: browser_runtime.node_available,
        browser_runtime_available: browser_runtime.runtime_available,
        playwright_installed: browser_runtime.playwright_installed,
        chromium_installed: browser_runtime.chromium_installed,
        browser_runtime_managed: browser_runtime.managed,
        browser_runtime_message: browser_runtime.message,
        max_results: config.max_results,
        endpoint_url: config.endpoint_url.clone(),
        auth_kind: auth_kind_for_provider(&config.provider).to_string(),
        allowed_hosts: config.allowed_hosts.clone(),
    }
}

fn health(
    status: WorkBrowserHealthStatus,
    provider: &str,
    message: &str,
    checked_at: String,
    latency_ms: u64,
) -> WorkBrowserHealth {
    WorkBrowserHealth {
        status,
        provider: provider.to_string(),
        message: message.to_string(),
        checked_at,
        latency_ms,
    }
}

fn read_config(paths: &WorkPaths) -> Result<BrowserConfigFile, String> {
    let path = paths.work_browser_config_path();
    if !path.is_file() {
        return Ok(default_config());
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err("Work 网络访问 configuration is too large".into());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut config = serde_json::from_str::<BrowserConfigFile>(&content)
        .map_err(|error| format!("Invalid Work 网络访问 configuration: {error}"))?;
    config.provider = normalize_provider(&config.provider)?.to_string();
    config.max_results = normalize_max_results(config.max_results)?;
    Ok(config)
}

fn read_secrets(paths: &WorkPaths) -> Result<BrowserSecretsFile, String> {
    let path = paths.work_browser_secrets_path();
    if !path.is_file() {
        return Ok(BrowserSecretsFile {
            version: 1,
            providers: Default::default(),
        });
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_SECRETS_BYTES {
        return Err("Work 网络访问 secret store is too large".into());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str::<BrowserSecretsFile>(&content)
        .map_err(|error| format!("Invalid Work 网络访问 secret store: {error}"))
}

pub fn read_api_key_for_provider(
    paths: &WorkPaths,
    provider: &str,
) -> Result<Option<String>, String> {
    Ok(read_secrets(paths)?
        .providers
        .get(provider)
        .and_then(|secret| secret.api_key.clone())
        .filter(|value| !value.trim().is_empty()))
}

fn write_api_key(paths: &WorkPaths, provider: &str, api_key: &str) -> Result<(), String> {
    let mut secrets = read_secrets(paths)?;
    secrets.providers.insert(
        provider.to_string(),
        BrowserProviderSecret {
            api_key: Some(api_key.to_string()),
        },
    );
    write_secrets(paths, &secrets)
}

fn remove_api_key(paths: &WorkPaths, provider: &str) -> Result<(), String> {
    let mut secrets = read_secrets(paths)?;
    secrets.providers.remove(provider);
    if secrets.providers.is_empty() {
        let _ = fs::remove_file(paths.work_browser_secrets_path());
        return Ok(());
    }
    write_secrets(paths, &secrets)
}

fn write_config(paths: &WorkPaths, config: &BrowserConfigFile) -> Result<(), String> {
    atomic_write_json(&paths.work_browser_config_path(), config, false)
}

fn write_secrets(paths: &WorkPaths, secrets: &BrowserSecretsFile) -> Result<(), String> {
    atomic_write_json(&paths.work_browser_secrets_path(), secrets, true)
}

fn atomic_write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
    secret: bool,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Work 网络访问 file has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let serialized = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temporary, format!("{serialized}\n")).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn default_config() -> BrowserConfigFile {
    BrowserConfigFile {
        version: 1,
        enabled: false,
        provider: TAVILY_PROVIDER.to_string(),
        max_results: DEFAULT_MAX_RESULTS,
        endpoint_url: None,
        allowed_hosts: Vec::new(),
    }
}

pub fn normalize_provider(provider: &str) -> Result<&'static str, String> {
    let p = provider.trim().to_ascii_lowercase();
    match p.as_str() {
        TAVILY_PROVIDER => Ok(TAVILY_PROVIDER),
        EXA_PROVIDER => Ok(EXA_PROVIDER),
        SEARXNG_PROVIDER | "searx" => Ok(SEARXNG_PROVIDER),
        _ => Err(format!(
            "不支持的网络访问 provider: {provider}。DuckDuckGo/AnySearch 已移除；支持: tavily, exa, searxng"
        )),
    }
}

fn normalize_max_results(value: u32) -> Result<u32, String> {
    if (1..=10).contains(&value) {
        Ok(value)
    } else {
        Err("网络访问 maxResults 必须在 1 到 10 之间".into())
    }
}

fn validate_secret(value: &str) -> Result<(), String> {
    if value.len() > 4_096 || value.chars().any(char::is_control) {
        return Err("API key 无效或过长".into());
    }
    Ok(())
}

fn safe_network_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "网络超时".into()
    } else if error.is_connect() {
        "无法连接网络搜索服务".into()
    } else {
        "网络请求异常".into()
    }
}

fn default_version() -> u32 {
    1
}

fn default_provider() -> String {
    TAVILY_PROVIDER.to_string()
}

fn default_max_results() -> u32 {
    DEFAULT_MAX_RESULTS
}

impl From<BrowserConfigFile> for WorkBrowserConfig {
    fn from(config: BrowserConfigFile) -> Self {
        Self {
            enabled: config.enabled,
            provider: config.provider,
            max_results: config.max_results,
            endpoint_url: config.endpoint_url,
            allowed_hosts: config.allowed_hosts,
        }
    }
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
    fn defaults_to_disabled_tavily_without_a_secret() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let summary = get_config_with_paths(&paths).unwrap();
        assert!(!summary.enabled);
        assert_eq!(summary.provider, TAVILY_PROVIDER);
        assert!(!summary.configured);
        assert_eq!(summary.auth_kind, "api_key");
        assert!(!summary.runtime_available);
    }

    #[test]
    fn saves_config_without_returning_the_api_key() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let summary = save_config_with_paths(
            &paths,
            "tavily",
            true,
            Some(3),
            Some("tvly-secret"),
            None,
            None,
        )
        .unwrap();
        assert!(summary.enabled);
        assert!(summary.configured);
        assert!(!serde_json::to_string(&summary)
            .unwrap()
            .contains("tvly-secret"));
        let config = fs::read_to_string(paths.work_browser_config_path()).unwrap();
        assert!(!config.contains("tvly-secret"));
    }

    #[test]
    fn rejects_invalid_provider_and_max_results() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        assert!(
            save_config_with_paths(&paths, "unknown_engine", true, Some(3), None, None, None)
                .is_err()
        );
        assert!(save_config_with_paths(&paths, "tavily", true, Some(0), None, None, None).is_err());
        assert!(
            save_config_with_paths(&paths, "tavily", true, Some(11), None, None, None).is_err()
        );
    }

    #[test]
    fn empty_api_key_removes_the_secret() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        save_config_with_paths(
            &paths,
            "tavily",
            true,
            None,
            Some("tvly-secret"),
            None,
            None,
        )
        .unwrap();
        let summary =
            save_config_with_paths(&paths, "tavily", false, None, Some(""), None, None).unwrap();
        assert!(!summary.enabled);
        assert!(!summary.configured);
        assert!(!summary.runtime_available);
        assert!(!paths.work_browser_secrets_path().exists());
    }

    #[test]
    fn normalizes_and_persists_allowed_hosts() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let summary = save_config_with_paths(
            &paths,
            "tavily",
            true,
            Some(3),
            None,
            None,
            Some(vec![
                "http://jira.virtueit.net/".into(),
                "10.73.10.234:8080".into(),
                "*.virtueit.net".into(),
                "10.73.0.0/16".into(),
            ]),
        )
        .unwrap();
        assert_eq!(
            summary.allowed_hosts,
            vec![
                "jira.virtueit.net".to_string(),
                "10.73.10.234".to_string(),
                "*.virtueit.net".to_string(),
                "10.73.0.0/16".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn health_is_unconfigured_without_secret_for_tavily() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        save_config_with_paths(&paths, "tavily", true, Some(3), None, None, None).unwrap();
        let health = test_with_paths(&paths).await.unwrap();
        assert_eq!(health.status, WorkBrowserHealthStatus::Unconfigured);
    }

    #[cfg(unix)]
    #[test]
    fn secret_store_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        save_config_with_paths(
            &paths,
            "tavily",
            true,
            None,
            Some("tvly-secret"),
            None,
            None,
        )
        .unwrap();
        let mode = fs::metadata(paths.work_browser_secrets_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
