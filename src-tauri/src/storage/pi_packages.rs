use crate::models::PiPackageItem;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::work::system_packages;
use tokio::sync::Mutex;

const PI_PACKAGES_URL: &str = "https://pi.dev/packages";
const CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("AgentCabin/0.1 (Pi Package Explorer)")
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .unwrap_or_default()
});

type PackageCache = HashMap<String, (Instant, Vec<PiPackageItem>)>;
static CACHE: LazyLock<Mutex<PackageCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn fallback_extensions() -> Vec<PiPackageItem> {
    vec![
        PiPackageItem {
            name: "@vigolium/piolium".to_string(),
            description: "Multi-phase security audits with specialist sub-agents, isolated context windows, capped concurrency, and resumable state — packaged as a Pi extension.".to_string(),
            author: "j3ssie".to_string(),
            downloads: 231162,
            downloads_formatted: "231.2K/mo".to_string(),
            time_ago: "25d ago".to_string(),
            date: 1784623367738,
            npm_url: "https://www.npmjs.com/package/@vigolium/piolium".to_string(),
            repo_url: "https://github.com/vigolium/piolium".to_string(),
            package_path: "/packages/@vigolium/piolium?type=extension".to_string(),
            install_source: "npm:@vigolium/piolium".to_string(),
            types: vec!["extension".to_string()],
            is_recent: false,
        },
        PiPackageItem {
            name: "@juicesharp/rpiv-ask-user-question".to_string(),
            description: "Pi extension. A structured questionnaire the model can put to you when it would otherwise guess, with typed options instead of free-form replies.".to_string(),
            author: "juicesharp".to_string(),
            downloads: 51649,
            downloads_formatted: "51.6K/mo".to_string(),
            time_ago: "1d ago".to_string(),
            date: 1786720852792,
            npm_url: "https://www.npmjs.com/package/@juicesharp/rpiv-ask-user-question".to_string(),
            repo_url: "https://github.com/juicesharp/rpiv-mono".to_string(),
            package_path: "/packages/@juicesharp/rpiv-ask-user-question?type=extension".to_string(),
            install_source: "npm:@juicesharp/rpiv-ask-user-question".to_string(),
            types: vec!["extension".to_string()],
            is_recent: false,
        },
        PiPackageItem {
            name: "@juicesharp/rpiv-todo".to_string(),
            description: "Pi extension. A todo list for the model, rendered as a live overlay that survives /reload and conversation compaction.".to_string(),
            author: "juicesharp".to_string(),
            downloads: 43055,
            downloads_formatted: "43.1K/mo".to_string(),
            time_ago: "1d ago".to_string(),
            date: 1786720865096,
            npm_url: "https://www.npmjs.com/package/@juicesharp/rpiv-todo".to_string(),
            repo_url: "https://github.com/juicesharp/rpiv-mono".to_string(),
            package_path: "/packages/@juicesharp/rpiv-todo?type=extension".to_string(),
            install_source: "npm:@juicesharp/rpiv-todo".to_string(),
            types: vec!["extension".to_string()],
            is_recent: false,
        },
        PiPackageItem {
            name: "pi-lens".to_string(),
            description: "Real-time code feedback for pi — LSP, linters, formatters, type-checking, structural analysis & booboo".to_string(),
            author: "apmantza".to_string(),
            downloads: 40911,
            downloads_formatted: "40.9K/mo".to_string(),
            time_ago: "1d ago".to_string(),
            date: 1786716595640,
            npm_url: "https://www.npmjs.com/package/pi-lens".to_string(),
            repo_url: "https://github.com/apmantza/pi-lens".to_string(),
            package_path: "/packages/pi-lens?type=extension".to_string(),
            install_source: "npm:pi-lens".to_string(),
            types: vec!["extension".to_string()],
            is_recent: false,
        },
        PiPackageItem {
            name: "@tintinweb/pi-subagents".to_string(),
            description: "A pi extension extension that brings smart Claude Code-style autonomous sub-agents to pi.".to_string(),
            author: "tintinweb".to_string(),
            downloads: 40722,
            downloads_formatted: "40.7K/mo".to_string(),
            time_ago: "17h ago".to_string(),
            date: 1786750995550,
            npm_url: "https://www.npmjs.com/package/@tintinweb/pi-subagents".to_string(),
            repo_url: "https://github.com/tintinweb/pi-subagents".to_string(),
            package_path: "/packages/@tintinweb/pi-subagents?type=extension".to_string(),
            install_source: "npm:@tintinweb/pi-subagents".to_string(),
            types: vec!["extension".to_string()],
            is_recent: false,
        },
        PiPackageItem {
            name: "pi-pair".to_string(),
            description: "Pair decision audit for pi coding agent: fresh-spawn pair auditor, real-artifact gating, delivery gate, decision chain capture.".to_string(),
            author: "community".to_string(),
            downloads: 32000,
            downloads_formatted: "32K/mo".to_string(),
            time_ago: "26m ago".to_string(),
            date: 1786760000000,
            npm_url: "https://www.npmjs.com/package/pi-pair".to_string(),
            repo_url: "https://github.com/earendil-works/pi".to_string(),
            package_path: "/packages/pi-pair?type=extension".to_string(),
            install_source: "npm:pi-pair".to_string(),
            types: vec!["extension".to_string()],
            is_recent: true,
        },
    ]
}

fn unescape_html(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

pub fn parse_pi_packages_html(html: &str) -> Vec<PiPackageItem> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. First parse recently published items
    if let Some(recent_start) = html.find("packages-recent-list") {
        if let Some(recent_slice) = html.get(recent_start..) {
            let end_pos = recent_slice
                .find("</section>")
                .unwrap_or(recent_slice.len());
            let recent_block = &recent_slice[..end_pos];

            let mut start_idx = 0;
            while let Some(item_idx) = recent_block[start_idx..].find("<a href=\"/packages/") {
                let actual_idx = start_idx + item_idx;
                let Some(sub) = recent_block.get(actual_idx..) else {
                    break;
                };
                let item_end = sub.find("</a>").unwrap_or(0);
                if item_end == 0 {
                    break;
                };
                let link_content = &sub[..item_end + 4];
                start_idx = actual_idx + item_end + 4;

                // Extract name
                let name = if let (Some(s), Some(e)) = (
                    link_content.find("<strong>"),
                    link_content.find("</strong>"),
                ) {
                    unescape_html(&link_content[s + 8..e])
                } else {
                    continue;
                };

                let desc = if let (Some(s), Some(e)) =
                    (link_content.find("<span>"), link_content.find("</span>"))
                {
                    unescape_html(&link_content[s + 6..e])
                } else {
                    String::new()
                };

                let time_ago = if let (Some(s), Some(e)) =
                    (link_content.find("<small>"), link_content.find("</small>"))
                {
                    unescape_html(&link_content[s + 7..e])
                } else {
                    String::new()
                };

                let npm_url = format!("https://www.npmjs.com/package/{name}");
                let install_source = format!("npm:{name}");
                let package_path = format!("/packages/{name}?type=extension");

                seen.insert(name.clone());
                items.push(PiPackageItem {
                    name,
                    description: desc,
                    author: String::new(),
                    downloads: 0,
                    downloads_formatted: String::new(),
                    time_ago,
                    date: 0,
                    npm_url,
                    repo_url: String::new(),
                    package_path,
                    install_source,
                    types: vec!["extension".to_string()],
                    is_recent: true,
                });
            }
        }
    }

    // 2. Parse main package cards: `<article ... data-package-card="true" ...>`
    let mut search_idx = 0;
    while let Some(card_idx) = html[search_idx..].find("<article") {
        let actual_start = search_idx + card_idx;
        let Some(slice) = html.get(actual_start..) else {
            break;
        };
        let card_end = slice.find("</article>").unwrap_or(0);
        if card_end == 0 {
            break;
        };
        let article_html = &slice[..card_end + 10];
        search_idx = actual_start + card_end + 10;

        if !article_html.contains("data-package-card=\"true\"") {
            continue;
        }

        // Extract attributes
        let name = extract_attr(article_html, "data-package-name").unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let downloads: u64 = extract_attr(article_html, "data-package-downloads")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let date: u64 = extract_attr(article_html, "data-package-date")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let types = extract_attr(article_html, "data-package-types")
            .map(|t| t.split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_else(|| vec!["extension".to_string()]);

        // Description
        let desc = if let (Some(s), Some(e)) = (
            article_html.find("class=\"packages-desc\">"),
            article_html.find("</p>"),
        ) {
            unescape_html(&article_html[s + 22..e])
        } else {
            String::new()
        };

        // Meta: author, downloads_formatted, time_ago
        let (author, downloads_formatted, time_ago) =
            if let Some(meta_start) = article_html.find("class=\"packages-meta\">") {
                let meta_slice = &article_html[meta_start + 22..];
                let spans = extract_all_spans(meta_slice);
                let a = spans.first().cloned().unwrap_or_default();
                let d = spans.get(1).cloned().unwrap_or_default();
                let t = spans.get(2).cloned().unwrap_or_default();
                (a, d, t)
            } else {
                (String::new(), String::new(), String::new())
            };

        // Links
        let npm_url = extract_href_matching(article_html, "npmjs.com/package")
            .unwrap_or_else(|| format!("https://www.npmjs.com/package/{name}"));
        let repo_url = extract_href_matching(article_html, "github.com/").unwrap_or_default();

        let install_source = format!("npm:{name}");
        let package_path = format!("/packages/{name}?type=extension");

        if let Some(existing) = items.iter_mut().find(|item| item.name == name) {
            existing.description = desc;
            existing.author = author;
            existing.downloads = downloads;
            existing.downloads_formatted = downloads_formatted;
            if existing.time_ago.is_empty() {
                existing.time_ago = time_ago;
            }
            existing.date = date;
            existing.npm_url = npm_url;
            existing.repo_url = repo_url;
            existing.types = types;
        } else {
            items.push(PiPackageItem {
                name,
                description: desc,
                author,
                downloads,
                downloads_formatted,
                time_ago,
                date,
                npm_url,
                repo_url,
                package_path,
                install_source,
                types,
                is_recent: false,
            });
        }
    }

    let items = if items.is_empty() {
        fallback_extensions()
    } else {
        items
    };
    items
        .into_iter()
        .filter(|item| !system_packages::is_system_managed_package_name(&item.name))
        .collect()
}

fn extract_attr(html: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    let start = html.find(&pattern)?;
    let rest = &html[start + pattern.len()..];
    let end = rest.find('"')?;
    Some(unescape_html(&rest[..end]))
}

fn extract_href_matching(html: &str, contains: &str) -> Option<String> {
    let mut search = 0;
    while let Some(href_idx) = html[search..].find("href=\"") {
        let actual = search + href_idx + 6;
        let rest = &html[actual..];
        let end = rest.find('"')?;
        let url = &rest[..end];
        if url.contains(contains) {
            return Some(url.to_string());
        }
        search = actual + end;
    }
    None
}

fn extract_all_spans(html: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut search = 0;
    while let Some(start) = html[search..].find("<span>") {
        let actual_start = search + start + 6;
        let rest = &html[actual_start..];
        let Some(end) = rest.find("</span>") else {
            break;
        };
        spans.push(unescape_html(&rest[..end]));
        search = actual_start + end + 7;
        if spans.len() >= 3 {
            break;
        }
    }
    spans
}

pub async fn fetch_packages(
    query: Option<&str>,
    sort: Option<&str>,
    package_type: Option<&str>,
) -> Result<Vec<PiPackageItem>, String> {
    let type_param = package_type.unwrap_or("extension");
    let sort_param = sort.unwrap_or("downloads");
    let query_param = query.unwrap_or("").trim();

    let cache_key = format!("type={type_param}&sort={sort_param}&name={query_param}");

    // Check cache
    {
        let cache = CACHE.lock().await;
        if let Some((instant, items)) = cache.get(&cache_key) {
            if instant.elapsed() < CACHE_TTL {
                return Ok(items.clone());
            }
        }
    }

    let mut url = format!("{PI_PACKAGES_URL}?type={type_param}&sort={sort_param}");
    if !query_param.is_empty() {
        url.push_str("&name=");
        url.push_str(&urlencoding::encode(query_param));
    }

    log::debug!("[pi_packages] fetching package catalog from {}", url);

    let resp_result = CLIENT.get(&url).send().await;

    let items = match resp_result {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await.unwrap_or_default();
            parse_pi_packages_html(&text)
        }
        Ok(resp) => {
            log::warn!(
                "[pi_packages] request to {} returned status {}",
                url,
                resp.status()
            );
            let cache = CACHE.lock().await;
            if let Some((_, items)) = cache.get(&cache_key) {
                items.clone()
            } else {
                fallback_extensions()
            }
        }
        Err(err) => {
            log::warn!("[pi_packages] request to {} failed: {}", url, err);
            let cache = CACHE.lock().await;
            if let Some((_, items)) = cache.get(&cache_key) {
                items.clone()
            } else {
                fallback_extensions()
            }
        }
    };

    let items = items
        .into_iter()
        .filter(|item| !system_packages::is_system_managed_package_name(&item.name))
        .collect::<Vec<_>>();

    // Store in cache
    {
        let mut cache = CACHE.lock().await;
        cache.insert(cache_key, (Instant::now(), items.clone()));
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pi_packages_html_correctly() {
        let sample_html = r#"
        <div class="packages-recent-list">
          <a href="/packages/pi-pair?type=extension">
            <strong>pi-pair</strong>
            <span>Pair decision auditor</span>
            <small>26m ago</small>
          </a>
        </div>
        <article class="surface-panel content-card" data-package-card="true" data-package-name="pi-mcp-adapter" data-package-search="pi-mcp-adapter" data-package-types="extension" data-package-downloads="354380" data-package-date="1786750145753">
          <div class="packages-card-body">
            <h3 class="packages-name"><a href="/packages/pi-mcp-adapter">pi-mcp-adapter</a></h3>
            <p class="packages-desc">MCP adapter for Pi</p>
            <div class="packages-meta"><span>nicopreme</span><span>354.4K/mo</span><span>17h ago</span></div>
          </div>
        </article>
        "#;
        let items = parse_pi_packages_html(sample_html);
        assert!(!items.is_empty());
        let pair = items.iter().find(|i| i.name == "pi-pair").unwrap();
        assert!(pair.is_recent);
        assert_eq!(pair.install_source, "npm:pi-pair");

        assert!(items
            .iter()
            .all(|item| !system_packages::is_system_managed_package_name(&item.name)));
    }
}
