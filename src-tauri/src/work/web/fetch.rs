//! Native Web Fetch for AgentCabin (SSRF-safe, multi-hop redirect validated).

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

use super::security::assert_first_stage_url_with_allowed_hosts;

const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REDIRECT_HOPS: usize = 5;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024; // 2 MB
pub const MAX_PREVIEW_CHARS: usize = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebFetchResult {
    pub final_url: String,
    pub title: String,
    pub content_type: String,
    pub text: String,
}

pub fn decode_html_entities(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut found_semi = false;
            for next_ch in chars.by_ref() {
                if next_ch == ';' {
                    found_semi = true;
                    break;
                }
                if next_ch.is_whitespace() || next_ch == '&' || entity.len() > 10 {
                    entity.push(next_ch);
                    break;
                }
                entity.push(next_ch);
            }

            if found_semi {
                match entity.to_ascii_lowercase().as_str() {
                    "amp" => result.push('&'),
                    "lt" => result.push('<'),
                    "gt" => result.push('>'),
                    "quot" => result.push('"'),
                    "apos" => result.push('\''),
                    "nbsp" => result.push(' '),
                    _ if entity.starts_with('#') => {
                        let num_str = &entity[1..];
                        let parsed_char = if num_str.starts_with('x') || num_str.starts_with('X') {
                            u32::from_str_radix(&num_str[1..], 16)
                                .ok()
                                .and_then(char::from_u32)
                        } else {
                            num_str.parse::<u32>().ok().and_then(char::from_u32)
                        };
                        if let Some(decoded_ch) = parsed_char {
                            result.push(decoded_ch);
                        } else {
                            result.push('&');
                            result.push_str(&entity);
                            result.push(';');
                        }
                    }
                    _ => {
                        result.push('&');
                        result.push_str(&entity);
                        result.push(';');
                    }
                }
            } else {
                result.push('&');
                result.push_str(&entity);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn strip_tag_blocks(html: &str, tag_name: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let open_tag_prefix = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);

    let mut cursor = 0;
    while cursor < html.len() {
        if let Some(start_idx) = lower[cursor..].find(&open_tag_prefix) {
            let abs_start = cursor + start_idx;
            out.push_str(&html[cursor..abs_start]);

            // Find closing '>' of opening tag
            if let Some(open_tag_end) = lower[abs_start..].find('>') {
                let content_start = abs_start + open_tag_end + 1;
                // Find closing tag
                if let Some(close_idx) = lower[content_start..].find(&close_tag) {
                    cursor = content_start + close_idx + close_tag.len();
                } else {
                    // Tag wasn't closed, skip to end
                    cursor = html.len();
                }
            } else {
                cursor = html.len();
            }
        } else {
            out.push_str(&html[cursor..]);
            break;
        }
    }

    out
}

pub fn extract_title(html: &str, fallback_host: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if let Some(start_idx) = lower.find("<title") {
        if let Some(open_end) = lower[start_idx..].find('>') {
            let content_start = start_idx + open_end + 1;
            if let Some(close_idx) = lower[content_start..].find("</title>") {
                let raw_title = &html[content_start..content_start + close_idx];
                let cleaned = decode_html_entities(raw_title).replace(['\r', '\n', '\t'], " ");
                let normalized = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
                if !normalized.is_empty() {
                    return normalized;
                }
            }
        }
    }
    fallback_host.to_string()
}

pub fn extract_text(body: &str, content_type: &str) -> String {
    let ct = content_type.to_ascii_lowercase();
    if (ct.contains("json") || ct.contains("xml") || ct.starts_with("text/"))
        && !ct.contains("html")
    {
        return body.split_whitespace().collect::<Vec<_>>().join(" ");
    }

    // Strip script, style, noscript
    let stripped = strip_tag_blocks(body, "script");
    let stripped = strip_tag_blocks(&stripped, "style");
    let stripped = strip_tag_blocks(&stripped, "noscript");

    let mut out = String::with_capacity(stripped.len());
    let mut in_tag = false;
    let mut tag_buf = String::new();

    for ch in stripped.chars() {
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
        } else if ch == '>' {
            in_tag = false;
            let tag_lower = tag_buf.trim().to_ascii_lowercase();
            if tag_lower.starts_with("br")
                || tag_lower.starts_with("/p")
                || tag_lower.starts_with("/div")
                || tag_lower.starts_with("/li")
                || tag_lower.starts_with("/h1")
                || tag_lower.starts_with("/h2")
                || tag_lower.starts_with("/h3")
                || tag_lower.starts_with("/h4")
                || tag_lower.starts_with("/h5")
                || tag_lower.starts_with("/h6")
                || tag_lower.starts_with("/tr")
                || tag_lower.starts_with("/article")
                || tag_lower.starts_with("/section")
                || tag_lower.starts_with("/header")
                || tag_lower.starts_with("/footer")
            {
                out.push('\n');
            } else {
                out.push(' ');
            }
        } else if in_tag {
            tag_buf.push(ch);
        } else {
            out.push(ch);
        }
    }

    let decoded = decode_html_entities(&out);
    let mut lines = Vec::new();
    for line in decoded.lines() {
        let cleaned = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !cleaned.is_empty() {
            lines.push(cleaned);
        }
    }

    lines.join("\n")
}

fn assert_fetch_content_type(content_type: &str) -> Result<String, String> {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("application/pdf")
        || ct.starts_with("image/")
        || ct.starts_with("audio/")
        || ct.starts_with("video/")
        || ct.contains("application/octet-stream")
        || ct.contains("application/zip")
    {
        return Err("网络访问第一阶段仅支持 HTML、纯文本或 JSON 页面".into());
    }

    if ct.is_empty() {
        Ok("text/html".into())
    } else {
        Ok(ct)
    }
}

pub async fn execute_web_fetch(
    initial_url: &str,
    proxy_url: Option<&str>,
) -> Result<WebFetchResult, String> {
    let proxy_url = proxy_url.map(str::trim).filter(|url| !url.is_empty());
    let proxy_active = proxy_url.is_some();
    let allowed_hosts = crate::work::browser::get_config()
        .map(|c| c.allowed_hosts)
        .unwrap_or_default();
    let mut current_url =
        assert_first_stage_url_with_allowed_hosts(initial_url, proxy_active, &allowed_hosts)
            .await?;
    let mut visited = HashSet::new();
    visited.insert(current_url.to_string());

    let client_builder = super::with_proxy(
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(DEFAULT_FETCH_TIMEOUT)
            .user_agent("AgentCabin/1.0.0 (Native Web)"),
        proxy_url,
    )?;
    let client = client_builder
        .build()
        .map_err(|e| format!("创建网络请求客户端失败: {e}"))?;

    let mut hops = 0;
    loop {
        let response = client
            .get(current_url.as_str())
            .send()
            .await
            .map_err(|e| format!("URL 抓取失败: {e}"))?;

        let status = response.status();
        if status.is_redirection() {
            hops += 1;
            if hops > MAX_REDIRECT_HOPS {
                return Err("网络访问重定向次数过多（已超过限制）".into());
            }

            let Some(loc_header) = response.headers().get(reqwest::header::LOCATION) else {
                return Err("重定向缺少 Location 响应头".into());
            };
            let loc_str = loc_header.to_str().map_err(|_| "Location 响应头格式无效")?;

            let next_url = current_url
                .join(loc_str)
                .map_err(|_| "重定向 Location URL 解析失败")?;

            if !visited.insert(next_url.to_string()) {
                return Err("检测到重定向死循环".into());
            }

            // Re-run full SSRF, private address, DNS check for redirect destination
            current_url = assert_first_stage_url_with_allowed_hosts(
                next_url.as_str(),
                proxy_active,
                &allowed_hosts,
            )
            .await?;
            continue;
        }

        if !status.is_success() {
            return Err(format!("网页抓取失败，HTTP 返回 {}", status.as_u16()));
        }

        let raw_content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|val| val.to_str().ok())
            .unwrap_or("text/html");

        let content_type = assert_fetch_content_type(raw_content_type)?;

        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(format!(
                "网页大小超出安全限制（最大支持 {} 字节）",
                MAX_RESPONSE_BYTES
            ));
        }

        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("读取网页内容失败: {e}"))?;
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(format!(
                    "网页大小超出安全限制（最大支持 {} 字节）",
                    MAX_RESPONSE_BYTES
                ));
            }
            bytes.extend_from_slice(&chunk);
        }

        let body_str = String::from_utf8_lossy(&bytes);
        let title = extract_title(&body_str, current_url.host_str().unwrap_or(""));
        let text = extract_text(&body_str, &content_type);

        if text.trim().is_empty() {
            return Err("未返回可读取的页面内容".into());
        }

        return Ok(WebFetchResult {
            final_url: current_url.to_string(),
            title,
            content_type,
            text,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_readable_text_and_title() {
        let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Test Page &amp; Article</title>
            <style>body { color: red; }</style>
            <script>console.log("secret");</script>
        </head>
        <body>
            <h1>Heading 1</h1>
            <p>First paragraph with <a href="/link">a link</a> and &nbsp; entity.</p>
            <noscript>JavaScript disabled</noscript>
            <div>Second block of text.</div>
        </body>
        </html>
        "#;

        let title = extract_title(html, "fallback.com");
        assert_eq!(title, "Test Page & Article");

        let text = extract_text(html, "text/html");
        assert!(text.contains("Heading 1"));
        assert!(text.contains("First paragraph with a link and entity."));
        assert!(text.contains("Second block of text."));
        assert!(!text.contains("console.log"));
        assert!(!text.contains("color: red"));
        assert!(!text.contains("JavaScript disabled"));
    }

    #[test]
    fn decodes_html_entities() {
        assert_eq!(
            decode_html_entities("&lt;div&gt;&amp;&quot;&#39;&apos;"),
            "<div>&\"''"
        );
        assert_eq!(
            decode_html_entities("Price: &#36;100 &#x26; tax"),
            "Price: $100 & tax"
        );
    }

    #[test]
    fn content_type_filter_rejects_binary_media() {
        assert!(assert_fetch_content_type("application/pdf").is_err());
        assert!(assert_fetch_content_type("image/png").is_err());
        assert!(assert_fetch_content_type("video/mp4").is_err());
        assert!(assert_fetch_content_type("application/zip").is_err());
        assert!(assert_fetch_content_type("application/octet-stream").is_err());

        assert!(assert_fetch_content_type("text/html; charset=utf-8").is_ok());
        assert!(assert_fetch_content_type("text/plain").is_ok());
        assert!(assert_fetch_content_type("application/json").is_ok());
    }
}
