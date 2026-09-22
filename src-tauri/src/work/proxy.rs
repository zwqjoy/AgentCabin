use std::collections::HashMap;

const PROXY_ENABLED: &str = "AGENTCABIN_WORK_PROXY_ENABLED";
const PROXY_URL: &str = "AGENTCABIN_WORK_PROXY_URL";

/// Convert the macOS system web proxy output into a proxy URL.
///
/// This parser is deliberately kept independent from the process environment
/// so it can be tested with a fixture and so Work does not inherit arbitrary
/// proxy settings from Code-mode.
pub(crate) fn parse_macos_proxy_config(output: &str) -> Option<String> {
    let mut values = HashMap::new();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        values.insert(key.trim().to_string(), value.trim().to_string());
    }

    for prefix in ["HTTPS", "HTTP"] {
        let enabled = values
            .get(&format!("{prefix}Enable"))
            .is_some_and(|value| value == "1");
        if !enabled {
            continue;
        }
        let Some(host) = values.get(&format!("{prefix}Proxy")) else {
            continue;
        };
        let Some(port) = values
            .get(&format!("{prefix}Port"))
            .and_then(|value| value.parse::<u16>().ok())
        else {
            continue;
        };
        if host.is_empty() || host == "<nil>" || host.chars().any(char::is_whitespace) {
            continue;
        }
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        return Some(format!("http://{host}:{port}"));
    }
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn work_proxy_url() -> Option<String> {
    let output = std::process::Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_macos_proxy_config(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn work_proxy_url() -> Option<String> {
    None
}

/// Environment for the isolated Work Pi process.
///
/// Node's built-in fetch only consumes HTTP(S)_PROXY when
/// NODE_USE_ENV_PROXY=1 is set. Keep this opt-in and explicit so ordinary
/// Code-mode Pi sessions are unaffected.
pub(crate) fn work_node_proxy_env() -> HashMap<String, String> {
    let Some(proxy) = work_proxy_url() else {
        return HashMap::new();
    };
    let mut env = HashMap::new();
    env.insert(PROXY_ENABLED.to_string(), "1".to_string());
    env.insert(PROXY_URL.to_string(), proxy.clone());
    env.insert("AGENTCABIN_WEB_PROXY_ENABLED".to_string(), "1".to_string());
    env.insert("AGENTCABIN_WEB_PROXY_URL".to_string(), proxy.clone());
    env.insert("NODE_USE_ENV_PROXY".to_string(), "1".to_string());
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        env.insert(key.to_string(), proxy.clone());
    }
    for key in ["http_proxy", "https_proxy", "all_proxy"] {
        env.insert(key.to_string(), proxy.clone());
    }
    // Do not allow a stale user NO_PROXY value to bypass the Work proxy for
    // public targets. Local addresses remain rejected by the adapter itself.
    for key in ["NO_PROXY", "no_proxy"] {
        env.insert(key.to_string(), "localhost,127.0.0.1,::1".to_string());
    }
    env
}

#[cfg(test)]
mod tests {
    use super::parse_macos_proxy_config;

    #[test]
    fn prefers_https_proxy_from_scutil_output() {
        let output = r#"
HTTPEnable : 1
HTTPProxy : 127.0.0.1
HTTPPort : 8080
HTTPSEnable : 1
HTTPSProxy : 127.0.0.1
HTTPSPort : 7897
"#;
        assert_eq!(
            parse_macos_proxy_config(output).as_deref(),
            Some("http://127.0.0.1:7897")
        );
    }

    #[test]
    fn ignores_disabled_or_invalid_proxy() {
        assert_eq!(parse_macos_proxy_config("HTTPEnable : 0\n"), None);
        assert_eq!(
            parse_macos_proxy_config("HTTPSEnable : 1\nHTTPSProxy : <nil>\nHTTPSPort : 7897\n"),
            None
        );
    }
}
