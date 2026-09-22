//! AgentCabin Native Web Capability.
//!
//! Provides backend-owned, runtime-agnostic public web search (Tavily) and safe
//! web fetch (SSRF-guarded, multi-hop redirect checked, HTML-to-readable text).

pub mod fetch;
pub mod search;
pub mod security;

pub(crate) fn with_proxy(
    builder: reqwest::ClientBuilder,
    proxy_url: Option<&str>,
) -> Result<reqwest::ClientBuilder, String> {
    let Some(proxy_url) = proxy_url.map(str::trim).filter(|url| !url.is_empty()) else {
        // Work owns proxy selection. Do not let reqwest silently inherit an
        // unrelated HTTP(S)_PROXY from the desktop launch environment.
        return Ok(builder.no_proxy());
    };
    let proxy = reqwest::Proxy::all(proxy_url)
        .map_err(|error| format!("Work 网络代理配置无效: {error}"))?;
    Ok(builder.proxy(proxy))
}

pub use fetch::{execute_web_fetch, WebFetchResult};
pub use search::{execute_web_search, WebSearchResult};
pub use security::{assert_first_stage_url, assert_public_url, is_private_ip};

#[cfg(test)]
mod tests {
    use super::with_proxy;

    #[test]
    fn native_web_client_accepts_explicit_work_proxy() {
        let builder =
            with_proxy(reqwest::Client::builder(), Some("http://127.0.0.1:7897")).unwrap();
        assert!(builder.build().is_ok());
    }

    #[test]
    fn native_web_client_rejects_invalid_proxy_configuration() {
        let error = with_proxy(reqwest::Client::builder(), Some("not a proxy")).unwrap_err();
        assert!(error.contains("代理"));
    }
}
