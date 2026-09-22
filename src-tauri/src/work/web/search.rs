//! Native Web Search backend for AgentCabin (DuckDuckGo, Tavily, Exa, SearXNG, AnySearch).

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;
use std::time::Duration;

use crate::work::browser::{
    self, ANYSEARCH_PROVIDER, DUCKDUCKGO_PROVIDER, EXA_PROVIDER, SEARXNG_PROVIDER, TAVILY_PROVIDER,
};
use crate::work::paths::WorkPaths;

const TAVILY_SEARCH_URL: &str = "https://api.tavily.com/search";
const EXA_SEARCH_URL: &str = "https://api.exa.ai/search";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const TAVILY_REQUEST_ATTEMPTS: usize = 3;
const TAVILY_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const TAVILY_RETRY_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResponse {
    #[serde(default)]
    results: Vec<TavilyResultItem>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TavilyResultItem {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    snippet: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExaSearchResponse {
    #[serde(default)]
    results: Vec<ExaResultItem>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExaResultItem {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    highlights: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SearxngSearchResponse {
    #[serde(default)]
    results: Vec<SearxngResultItem>,
}

#[derive(Debug, Deserialize)]
struct SearxngResultItem {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

pub async fn execute_web_search(
    paths: &WorkPaths,
    query: &str,
    max_results: Option<u32>,
    proxy_url: Option<&str>,
) -> Result<Vec<WebSearchResult>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("网络访问 search query 不能为空".into());
    }

    let (config, api_key_opt) = browser::runtime(paths)?;
    if !config.enabled {
        return Err("网络访问未启用".into());
    }

    let limit = max_results.unwrap_or(config.max_results).clamp(1, 10);
    let provider = config.provider.as_str();

    match provider {
        DUCKDUCKGO_PROVIDER => execute_duckduckgo_search(query, limit, proxy_url).await,
        TAVILY_PROVIDER => {
            let api_key = api_key_opt
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "网络访问未配置 Tavily API key".to_string())?;
            execute_tavily_search(query, limit, &api_key, proxy_url).await
        }
        EXA_PROVIDER => {
            let api_key = api_key_opt
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "网络访问未配置 Exa API key".to_string())?;
            execute_exa_search(query, limit, &api_key, proxy_url).await
        }
        SEARXNG_PROVIDER => {
            let endpoint = config.endpoint_url.as_deref().unwrap_or("").trim();
            if endpoint.is_empty() {
                return Err("网络访问未配置 SearXNG 实例地址 (URL)".into());
            }
            execute_searxng_search(query, limit, endpoint, api_key_opt.as_deref(), proxy_url).await
        }
        ANYSEARCH_PROVIDER => {
            // AnySearch: try DuckDuckGo as free/anonymous engine
            execute_duckduckgo_search(query, limit, proxy_url).await
        }
        _ => Err(format!("不支持的网络访问 provider: {provider}")),
    }
}

pub async fn execute_duckduckgo_search(
    query: &str,
    limit: u32,
    proxy_url: Option<&str>,
) -> Result<Vec<WebSearchResult>, String> {
    let client_builder = super::with_proxy(
        reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
        proxy_url,
    )?;
    let client = client_builder
        .build()
        .map_err(|error| format!("创建 DuckDuckGo 搜索客户端失败: {error}"))?;

    let response = client
        .post("https://html.duckduckgo.com/html/")
        .form(&[("q", query)])
        .send()
        .await
        .map_err(|error| format!("DuckDuckGo 搜索请求失败: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "DuckDuckGo 搜索返回 HTTP {}: {}",
            status.as_u16(),
            body
        ));
    }

    let html = response
        .text()
        .await
        .map_err(|error| format!("读取 DuckDuckGo 响应失败: {error}"))?;

    let parsed = parse_duckduckgo_html(&html, limit);
    if parsed.is_empty() {
        // Fallback or empty
        return Ok(Vec::new());
    }
    Ok(parsed)
}

fn parse_duckduckgo_html(html: &str, limit: u32) -> Vec<WebSearchResult> {
    let mut results = Vec::new();
    // Split by result chunks in DuckDuckGo HTML
    let chunks: Vec<&str> = html.split("class=\"result results_links").collect();
    for chunk in chunks.into_iter().skip(1) {
        if results.len() >= limit as usize {
            break;
        }

        // Extract title and URL from <a class="result__url" href="..."> or <a class="result__a" href="...">
        let raw_url = if let Some(href_pos) = chunk.find("class=\"result__snippet\"") {
            // Find link preceding snippet
            let prefix = &chunk[..href_pos];
            extract_ddg_href(prefix)
        } else {
            extract_ddg_href(chunk)
        };

        let Some(mut url) = raw_url else { continue };

        // Normalize uddg URL if redirected
        if url.contains("uddg=") {
            if let Some(pos) = url.find("uddg=") {
                let encoded = &url[pos + 5..];
                let end = encoded.find('&').unwrap_or(encoded.len());
                if let Ok(decoded) = urlencoding::decode(&encoded[..end]) {
                    url = decoded.into_owned();
                }
            }
        }

        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }

        // Extract title
        let title = extract_tag_text(chunk, "result__title")
            .or_else(|| extract_tag_text(chunk, "result__a"))
            .unwrap_or_else(|| url.clone());

        // Extract snippet
        let snippet = extract_tag_text(chunk, "result__snippet").unwrap_or_default();

        results.push(WebSearchResult {
            title: clean_html_text(&title),
            url,
            snippet: clean_html_text(&snippet),
        });
    }
    results
}

fn extract_ddg_href(chunk: &str) -> Option<String> {
    let tag_pos = chunk
        .find("class=\"result__a\"")
        .or_else(|| chunk.find("class=\"result__url\""))?;
    let start_search = &chunk[..tag_pos];
    let href_pos = start_search.rfind("href=\"")?;
    let start = href_pos + 6;
    let end = chunk[start..].find('"')? + start;
    Some(chunk[start..end].trim().to_string())
}

fn extract_tag_text(chunk: &str, class_name: &str) -> Option<String> {
    let class_pos = chunk.find(class_name)?;
    let tag_start = chunk[class_pos..].find('>')? + class_pos + 1;
    let tag_end = chunk[tag_start..]
        .find("</a>")
        .or_else(|| chunk[tag_start..].find("</div>"))?
        + tag_start;
    Some(chunk[tag_start..tag_end].to_string())
}

fn clean_html_text(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            clean.push(ch);
        }
    }
    clean
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn execute_tavily_search(
    query: &str,
    limit: u32,
    api_key: &str,
    proxy_url: Option<&str>,
) -> Result<Vec<WebSearchResult>, String> {
    let payload = json!({
        "api_key": api_key,
        "query": query,
        "max_results": limit,
        "include_answer": false,
    });

    let response = send_tavily_request_with_retry(&payload, proxy_url, DEFAULT_TIMEOUT)
        .await
        .map_err(|error| format!("Tavily 搜索请求失败: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Tavily 搜索返回 HTTP {}: {}",
            status.as_u16(),
            body
        ));
    }

    let parsed: TavilySearchResponse = response
        .json()
        .await
        .map_err(|error| format!("解析 Tavily 搜索结果失败: {error}"))?;

    if let Some(err) = parsed.error {
        return Err(format!("Tavily 搜索错误: {err}"));
    }

    let mut results = Vec::new();
    for item in parsed.results {
        let Some(url) = item
            .url
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
        else {
            continue;
        };
        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }
        let title = item.title.unwrap_or_else(|| url.clone());
        let snippet = item.content.or(item.snippet).unwrap_or_default();
        let snippet = snippet.chars().take(2000).collect();

        results.push(WebSearchResult {
            title,
            url,
            snippet,
        });

        if results.len() >= limit as usize {
            break;
        }
    }

    Ok(results)
}

/// Send a Tavily request with a fresh HTTP client for each transport retry.
///
/// A fresh client gives the resolver another chance to select a healthy
/// address when a load-balanced endpoint has one unreachable address. We only
/// retry transport failures; an HTTP response (including 401/429/5xx) is
/// returned to the caller so the provider error remains visible and bounded.
pub(crate) async fn send_tavily_request_with_retry(
    payload: &serde_json::Value,
    proxy_url: Option<&str>,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let mut last_error = "Tavily 请求失败".to_string();

    for attempt in 1..=TAVILY_REQUEST_ATTEMPTS {
        let client_builder = super::with_proxy(
            reqwest::Client::builder()
                .connect_timeout(timeout.min(TAVILY_CONNECT_TIMEOUT))
                .timeout(timeout),
            proxy_url,
        )?;
        let client = client_builder
            .build()
            .map_err(|error| format!("创建网络访问搜索客户端失败: {error}"))?;

        match client.post(TAVILY_SEARCH_URL).json(payload).send().await {
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = describe_network_error(&error);
                let retryable = error.is_connect() || error.is_timeout() || error.is_request();
                if !retryable || attempt == TAVILY_REQUEST_ATTEMPTS {
                    break;
                }

                log::warn!(
                    "[native_web] Tavily transport attempt {}/{} failed: {}; retrying",
                    attempt,
                    TAVILY_REQUEST_ATTEMPTS,
                    last_error
                );
                tokio::time::sleep(TAVILY_RETRY_DELAY * attempt as u32).await;
            }
        }
    }

    Err(last_error)
}

fn describe_network_error(error: &reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "网络超时"
    } else if error.is_connect() {
        "无法连接网络搜索服务"
    } else if error.is_request() {
        "网络请求传输失败"
    } else {
        "网络请求异常"
    };

    let mut source = error as &dyn Error;
    let mut detail = error.to_string();
    while let Some(next) = source.source() {
        detail = next.to_string();
        source = next;
    }
    format!("{kind}（{detail}）")
}

pub async fn execute_exa_search(
    query: &str,
    limit: u32,
    api_key: &str,
    proxy_url: Option<&str>,
) -> Result<Vec<WebSearchResult>, String> {
    let client_builder = super::with_proxy(
        reqwest::Client::builder().timeout(DEFAULT_TIMEOUT),
        proxy_url,
    )?;
    let client = client_builder
        .build()
        .map_err(|error| format!("创建 Exa 搜索客户端失败: {error}"))?;

    let payload = json!({
        "query": query,
        "numResults": limit,
        "useAutoprompt": true,
    });

    let response = client
        .post(EXA_SEARCH_URL)
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("Exa 搜索请求失败: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Exa 搜索返回 HTTP {}: {}", status.as_u16(), body));
    }

    let parsed: ExaSearchResponse = response
        .json()
        .await
        .map_err(|error| format!("解析 Exa 搜索结果失败: {error}"))?;

    if let Some(err) = parsed.error {
        return Err(format!("Exa 搜索错误: {err}"));
    }

    let mut results = Vec::new();
    for item in parsed.results {
        let Some(url) = item.url.filter(|u| !u.trim().is_empty()) else {
            continue;
        };
        let title = item.title.unwrap_or_else(|| url.clone());
        let snippet = item
            .highlights
            .and_then(|h| h.into_iter().next())
            .or(item.text)
            .unwrap_or_default();
        results.push(WebSearchResult {
            title,
            url,
            snippet: snippet.chars().take(2000).collect(),
        });
    }

    Ok(results)
}

pub async fn execute_searxng_search(
    query: &str,
    limit: u32,
    endpoint: &str,
    api_key: Option<&str>,
    proxy_url: Option<&str>,
) -> Result<Vec<WebSearchResult>, String> {
    let client_builder = super::with_proxy(
        reqwest::Client::builder().timeout(DEFAULT_TIMEOUT),
        proxy_url,
    )?;
    let client = client_builder
        .build()
        .map_err(|error| format!("创建 SearXNG 搜索客户端失败: {error}"))?;

    let base = endpoint.trim_end_matches('/');
    let target_url = format!("{base}/search");

    let mut req = client
        .get(&target_url)
        .query(&[("q", query), ("format", "json")]);

    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    let response = req
        .send()
        .await
        .map_err(|error| format!("SearXNG 搜索请求失败: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "SearXNG 搜索返回 HTTP {}: {}",
            status.as_u16(),
            body
        ));
    }

    let parsed: SearxngSearchResponse = response
        .json()
        .await
        .map_err(|error| format!("解析 SearXNG 搜索结果失败: {error}"))?;

    let mut results = Vec::new();
    for item in parsed.results {
        let Some(url) = item.url.filter(|u| !u.trim().is_empty()) else {
            continue;
        };
        let title = item.title.unwrap_or_else(|| url.clone());
        let snippet = item.content.unwrap_or_default();
        results.push(WebSearchResult {
            title,
            url,
            snippet: snippet.chars().take(2000).collect(),
        });
        if results.len() >= limit as usize {
            break;
        }
    }

    Ok(results)
}
