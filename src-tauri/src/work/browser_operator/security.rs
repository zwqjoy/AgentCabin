//! SSRF Prevention and URL Security Guard for Browser Operator.
//!
//! Validates target URLs before passing them to the Browser Worker:
//! - Only allows `http` and `https` schemes, plus `file:` for one exact
//!   WorkRun `output/` directory.
//! - Rejects arbitrary `file:`, `javascript:`, `chrome:`, `data:`, `about:`.
//! - Resolves hostnames to IP addresses and blocks:
//!   * Loopback (127.0.0.0/8, ::1)
//!   * Private IPv4 (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
//!   * Link-local (169.254.0.0/16, fe80::/10)
//!   * Cloud metadata endpoints (e.g. 169.254.169.254)
//!   * Localhost domain names (*.localhost, *.local, *.internal)

use std::net::{IpAddr, ToSocketAddrs};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlValidationError {
    InvalidUrl(String),
    DisallowedScheme(String),
    BlockedHost(String),
    BlockedIp(String),
    DnsResolutionFailed(String),
    LocalFileOutsideOutput(String),
}

impl std::fmt::Display for UrlValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl(err) => write!(f, "Invalid URL format: {err}"),
            Self::DisallowedScheme(scheme) => {
                write!(
                    f,
                    "Disallowed URL scheme '{scheme}'; only http/https and workspace output file:// are allowed"
                )
            }
            Self::BlockedHost(host) => {
                write!(f, "Access to host '{host}' is blocked (SSRF policy)")
            }
            Self::BlockedIp(ip) => {
                write!(f, "Access to IP '{ip}' is blocked (SSRF policy: private/loopback/metadata restricted)")
            }
            Self::DnsResolutionFailed(err) => write!(f, "DNS resolution failed: {err}"),
            Self::LocalFileOutsideOutput(path) => {
                write!(f, "Local file access is strictly restricted to workspace output/ directory: {path}")
            }
        }
    }
}

impl std::error::Error for UrlValidationError {}

/// Check if an IP address is considered private, loopback, link-local, or cloud metadata.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            // Loopback 127.0.0.0/8
            if v4.is_loopback() {
                return true;
            }
            // Broadcast / Unspecified
            if v4.is_unspecified() || v4.is_broadcast() {
                return true;
            }
            // Private ranges: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
            if v4.is_private() {
                return true;
            }
            // Link-local: 169.254.0.0/16 (includes AWS/GCP metadata 169.254.169.254)
            if v4.is_link_local() {
                return true;
            }
            // Carrier-grade NAT 100.64.0.0/10
            let octets = v4.octets();
            if octets[0] == 100 && (octets[1] >= 64 && octets[1] <= 127) {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            // Check for IPv4-mapped IPv6 (::ffff:x.x.x.x)
            if let Some(v4_mapped) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(v4_mapped));
            }
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            // Unique local address (fc00::/7)
            let seg0 = v6.segments()[0];
            if (seg0 & 0xfe00) == 0xfc00 {
                return true;
            }
            // Link-local unicast (fe80::/10)
            if (seg0 & 0xffc0) == 0xfe80 {
                return true;
            }
            false
        }
    }
}

/// Check if a hostname string is blocked.
pub fn is_blocked_hostname(host: &str) -> bool {
    let lower = host.trim().to_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower == "instance-data"
        || lower == "metadata.google.internal"
    {
        return true;
    }
    false
}

/// Check if an IP address is cloud metadata or dangerous unspecified/broadcast address.
/// These addresses are NEVER allowed, even if they match an allowlist rule.
pub fn is_dangerous_metadata_or_special(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            // 169.254.169.254 (Cloud metadata)
            if v4.octets() == [169, 254, 169, 254] {
                return true;
            }
            if v4.is_unspecified() || v4.is_broadcast() {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_dangerous_metadata_or_special(IpAddr::V4(v4));
            }
            v6.is_unspecified()
        }
    }
}

/// Helper to match an IPv4 address against an IPv4 CIDR string (e.g. "10.73.0.0/16").
fn matches_ipv4_cidr(ip: std::net::Ipv4Addr, cidr: &str) -> bool {
    if let Some((net_str, prefix_str)) = cidr.split_once('/') {
        if let (Ok(net_ip), Ok(prefix)) = (
            net_str.parse::<std::net::Ipv4Addr>(),
            prefix_str.parse::<u32>(),
        ) {
            if prefix <= 32 {
                let mask = if prefix == 0 {
                    0u32
                } else {
                    !0u32 << (32 - prefix)
                };
                let ip_u32 = u32::from(ip);
                let net_u32 = u32::from(net_ip);
                return (ip_u32 & mask) == (net_u32 & mask);
            }
        }
    }
    false
}

/// Check if a target host string or resolved IPs match any of the allowed rules.
pub fn is_host_or_ip_allowed(
    host_str: &str,
    resolved_ips: &[IpAddr],
    allowed_hosts: &[String],
) -> bool {
    if allowed_hosts.is_empty() {
        return false;
    }
    let lower_host = host_str.trim().to_ascii_lowercase();

    for pattern in allowed_hosts {
        let pat = pattern.trim().to_ascii_lowercase();
        if pat.is_empty() {
            continue;
        }

        // 1. Exact hostname match
        if pat == lower_host {
            return true;
        }

        // 2. Wildcard domain match (*.domain.com or .domain.com)
        if let Some(suffix) = pat.strip_prefix("*.") {
            if lower_host == suffix || lower_host.ends_with(&format!(".{suffix}")) {
                return true;
            }
        } else if let Some(suffix) = pat.strip_prefix('.') {
            if lower_host == suffix || lower_host.ends_with(&format!(".{suffix}")) {
                return true;
            }
        }

        // 3. Pattern is an exact IP address
        if let Ok(pat_ip) = pat.parse::<IpAddr>() {
            if resolved_ips.contains(&pat_ip) {
                return true;
            }
        }

        // 4. Pattern is an IPv4 CIDR (e.g. 10.73.0.0/16)
        if pat.contains('/') {
            for &ip in resolved_ips {
                if let IpAddr::V4(v4) = ip {
                    if matches_ipv4_cidr(v4, &pat) {
                        return true;
                    }
                }
            }
            if let Ok(host_v4) = lower_host.parse::<std::net::Ipv4Addr>() {
                if matches_ipv4_cidr(host_v4, &pat) {
                    return true;
                }
            }
        }
    }

    false
}

/// Validate a target URL against SSRF policy with allowed host rules.
pub fn validate_browser_url_with_allowed_hosts(
    raw_url: &str,
    allowed_hosts: &[String],
    allowed_output_root: Option<&std::path::Path>,
) -> Result<Url, UrlValidationError> {
    let parsed = Url::parse(raw_url).map_err(|e| UrlValidationError::InvalidUrl(e.to_string()))?;

    // A generated HTML deliverable may be opened directly, but only when the
    // caller supplies the exact managed output directory for this WorkRun.
    if parsed.scheme() == "file" {
        let root = allowed_output_root
            .ok_or_else(|| UrlValidationError::DisallowedScheme(parsed.scheme().to_string()))?;
        let file_path = parsed
            .to_file_path()
            .map_err(|_| UrlValidationError::InvalidUrl("Invalid file URL".into()))?;
        let canonical_root = std::fs::canonicalize(root).map_err(|_| {
            UrlValidationError::LocalFileOutsideOutput(file_path.display().to_string())
        })?;
        let canonical_file = std::fs::canonicalize(&file_path).map_err(|_| {
            UrlValidationError::LocalFileOutsideOutput(file_path.display().to_string())
        })?;
        if !canonical_file.starts_with(&canonical_root) || !canonical_file.is_file() {
            return Err(UrlValidationError::LocalFileOutsideOutput(
                file_path.display().to_string(),
            ));
        }
        return Ok(parsed);
    }

    // Only http and https are allowed for network navigation.
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(UrlValidationError::DisallowedScheme(scheme.to_string()));
    }

    // 2. Check host
    let host_str = match parsed.host_str() {
        Some(h) => h,
        None => return Err(UrlValidationError::InvalidUrl("Missing host in URL".into())),
    };

    // Metadata endpoints are NEVER allowed
    let lower_host = host_str.trim().to_ascii_lowercase();
    if lower_host == "metadata.google.internal" || lower_host == "instance-data" {
        return Err(UrlValidationError::BlockedHost(host_str.to_string()));
    }

    if is_blocked_hostname(host_str) && !is_host_or_ip_allowed(host_str, &[], allowed_hosts) {
        return Err(UrlValidationError::BlockedHost(host_str.to_string()));
    }

    // 3. Direct IP or DNS Resolution
    let port = parsed.port_or_known_default().unwrap_or(80);
    let socket_addr_str = format!("{host_str}:{port}");

    let resolved_ips = if let Ok(ip) = host_str.parse::<IpAddr>() {
        vec![ip]
    } else {
        match socket_addr_str.to_socket_addrs() {
            Ok(addrs) => {
                let ips: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
                if ips.is_empty() {
                    return Err(UrlValidationError::DnsResolutionFailed(
                        "No IP addresses found".into(),
                    ));
                }
                ips
            }
            Err(e) => {
                return Err(UrlValidationError::DnsResolutionFailed(e.to_string()));
            }
        }
    };

    // 4. Any dangerous metadata IP is strictly blocked
    for &ip in &resolved_ips {
        if is_dangerous_metadata_or_special(ip) {
            return Err(UrlValidationError::BlockedIp(ip.to_string()));
        }
    }

    // 5. Check allowlist
    if is_host_or_ip_allowed(host_str, &resolved_ips, allowed_hosts) {
        return Ok(parsed);
    }

    // 6. Default blocked host and blocked IP checks
    if is_blocked_hostname(host_str) {
        return Err(UrlValidationError::BlockedHost(host_str.to_string()));
    }

    for &ip in &resolved_ips {
        if is_blocked_ip(ip) {
            return Err(UrlValidationError::BlockedIp(ip.to_string()));
        }
    }

    Ok(parsed)
}

/// Validate a target URL against SSRF policy with an optional allowed output directory root.
pub fn validate_browser_url_with_allowed_root(
    raw_url: &str,
    allowed_output_root: Option<&std::path::Path>,
) -> Result<Url, UrlValidationError> {
    validate_browser_url_with_allowed_hosts(raw_url, &[], allowed_output_root)
}

/// Validate a target URL against SSRF policy (disallowing file:// by default).
pub fn validate_browser_url(raw_url: &str) -> Result<Url, UrlValidationError> {
    validate_browser_url_with_allowed_hosts(raw_url, &[], None)
}

/// Validate an evaluation-only fixture URL against one exact loopback origin.
/// This intentionally does not broaden the normal browser allowlist: callers
/// must provide the temporary origin they just created, and the target URL
/// must use the same scheme, host, and port.
pub fn validate_browser_eval_fixture_url(
    raw_url: &str,
    allowed_origin: &str,
) -> Result<Url, UrlValidationError> {
    let origin = Url::parse(allowed_origin)
        .map_err(|error| UrlValidationError::InvalidUrl(error.to_string()))?;
    let target =
        Url::parse(raw_url).map_err(|error| UrlValidationError::InvalidUrl(error.to_string()))?;

    let origin_host = origin
        .host_str()
        .ok_or_else(|| UrlValidationError::InvalidUrl("Eval fixture origin has no host".into()))?;
    let origin_port = origin.port().ok_or_else(|| {
        UrlValidationError::InvalidUrl("Eval fixture origin must include a port".into())
    })?;
    if origin.scheme() != "http"
        || !matches!(
            origin_host.to_ascii_lowercase().as_str(),
            "127.0.0.1" | "localhost"
        )
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
        || !origin.username().is_empty()
        || origin.password().is_some()
    {
        return Err(UrlValidationError::BlockedHost(
            "Eval fixture origin must be an HTTP loopback origin with an explicit port".into(),
        ));
    }

    let target_host = target
        .host_str()
        .ok_or_else(|| UrlValidationError::InvalidUrl("Eval fixture URL has no host".into()))?;
    if target.scheme() != origin.scheme()
        || !target_host.eq_ignore_ascii_case(origin_host)
        || target.port() != Some(origin_port)
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return Err(UrlValidationError::BlockedHost(
            "Eval fixture URL is outside the explicitly allowlisted loopback origin".into(),
        ));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disallows_bad_schemes() {
        assert!(matches!(
            validate_browser_url("file:///etc/passwd"),
            Err(UrlValidationError::DisallowedScheme(_))
        ));
        assert!(matches!(
            validate_browser_url("javascript:alert(1)"),
            Err(UrlValidationError::DisallowedScheme(_))
        ));
        assert!(matches!(
            validate_browser_url("data:text/html,<b>hi</b>"),
            Err(UrlValidationError::DisallowedScheme(_))
        ));
    }

    #[test]
    fn test_blocks_localhost_and_loopback() {
        assert!(matches!(
            validate_browser_url("http://localhost:3000"),
            Err(UrlValidationError::BlockedHost(_))
        ));
        assert!(matches!(
            validate_browser_url("http://app.localhost:8080"),
            Err(UrlValidationError::BlockedHost(_))
        ));
        assert!(matches!(
            validate_browser_url("http://127.0.0.1:8080"),
            Err(UrlValidationError::BlockedIp(_))
        ));
        assert!(matches!(
            validate_browser_url("http://127.0.1.1/test"),
            Err(UrlValidationError::BlockedIp(_))
        ));
        assert!(matches!(
            validate_browser_url("http://[::1]:8080"),
            Err(UrlValidationError::BlockedIp(_))
        ));
        assert!(matches!(
            validate_browser_url("http://[::ffff:127.0.0.1]:8080"),
            Err(UrlValidationError::BlockedIp(_))
        ));
    }

    #[test]
    fn test_blocks_private_ranges() {
        assert!(matches!(
            validate_browser_url("http://192.168.1.1"),
            Err(UrlValidationError::BlockedIp(_))
        ));
        assert!(matches!(
            validate_browser_url("http://10.0.0.1"),
            Err(UrlValidationError::BlockedIp(_))
        ));
        assert!(matches!(
            validate_browser_url("http://172.16.0.1"),
            Err(UrlValidationError::BlockedIp(_))
        ));
        assert!(matches!(
            validate_browser_url("http://169.254.169.254"),
            Err(UrlValidationError::BlockedIp(_))
        ));
    }

    #[test]
    fn test_disallows_file_scheme() {
        assert!(matches!(
            validate_browser_url("file:///tmp/test.html"),
            Err(UrlValidationError::DisallowedScheme(_))
        ));
    }

    #[test]
    fn test_allows_file_only_inside_output_root() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let html = output.join("chart.html");
        std::fs::write(&html, "<html></html>").unwrap();

        let file_url = Url::from_file_path(&html).unwrap().to_string();
        assert!(validate_browser_url_with_allowed_hosts(&file_url, &[], Some(&output),).is_ok());

        let outside = temp.path().join("secret.html");
        std::fs::write(&outside, "secret").unwrap();
        let outside_url = Url::from_file_path(&outside).unwrap().to_string();
        assert!(matches!(
            validate_browser_url_with_allowed_hosts(&outside_url, &[], Some(&output)),
            Err(UrlValidationError::LocalFileOutsideOutput(_))
        ));
    }

    #[test]
    fn test_is_host_or_ip_allowed_matching() {
        let ip: IpAddr = "10.73.10.234".parse().unwrap();
        // Exact hostname
        assert!(is_host_or_ip_allowed(
            "jira.virtueit.net",
            &[ip],
            &["jira.virtueit.net".into()]
        ));
        // Wildcard *.domain
        assert!(is_host_or_ip_allowed(
            "jira.virtueit.net",
            &[ip],
            &["*.virtueit.net".into()]
        ));
        // Suffix .domain
        assert!(is_host_or_ip_allowed(
            "jira.virtueit.net",
            &[ip],
            &[".virtueit.net".into()]
        ));
        // Exact IP
        assert!(is_host_or_ip_allowed(
            "jira.virtueit.net",
            &[ip],
            &["10.73.10.234".into()]
        ));
        // CIDR match
        assert!(is_host_or_ip_allowed(
            "jira.virtueit.net",
            &[ip],
            &["10.73.0.0/16".into()]
        ));
        // Non-matching CIDR
        assert!(!is_host_or_ip_allowed(
            "jira.virtueit.net",
            &[ip],
            &["10.74.0.0/16".into()]
        ));
    }

    #[test]
    fn test_validate_browser_url_with_allowed_hosts() {
        // Direct private IP allowed via allowlist
        let allowed = vec!["10.73.10.234".to_string()];
        let res = validate_browser_url_with_allowed_hosts(
            "http://10.73.10.234:8080/dashboard",
            &allowed,
            None,
        );
        assert!(res.is_ok());

        // Direct private IP allowed via CIDR
        let allowed_cidr = vec!["10.73.0.0/16".to_string()];
        let res_cidr = validate_browser_url_with_allowed_hosts(
            "http://10.73.10.234:8080/",
            &allowed_cidr,
            None,
        );
        assert!(res_cidr.is_ok());

        // Other private IP not in allowlist is still blocked
        let res_blocked =
            validate_browser_url_with_allowed_hosts("http://192.168.1.1/", &allowed, None);
        assert!(matches!(res_blocked, Err(UrlValidationError::BlockedIp(_))));
    }

    #[test]
    fn test_cloud_metadata_strictly_blocked_even_in_allowed_hosts() {
        let allowed = vec![
            "169.254.169.254".to_string(),
            "metadata.google.internal".to_string(),
            "169.254.0.0/16".to_string(),
        ];
        assert!(matches!(
            validate_browser_url_with_allowed_hosts(
                "http://169.254.169.254/latest/meta-data/",
                &allowed,
                None
            ),
            Err(UrlValidationError::BlockedIp(_))
        ));
        assert!(matches!(
            validate_browser_url_with_allowed_hosts(
                "http://metadata.google.internal/computeMetadata/v1/",
                &allowed,
                None
            ),
            Err(UrlValidationError::BlockedHost(_))
        ));
    }

    #[test]
    fn test_eval_fixture_url_requires_the_exact_loopback_origin() {
        let valid = validate_browser_eval_fixture_url(
            "http://127.0.0.1:43127/fixtures/computer-use/form.html",
            "http://127.0.0.1:43127",
        )
        .unwrap();
        assert_eq!(valid.path(), "/fixtures/computer-use/form.html");

        assert!(validate_browser_eval_fixture_url(
            "http://127.0.0.1:43128/fixtures/computer-use/form.html",
            "http://127.0.0.1:43127",
        )
        .is_err());
        assert!(validate_browser_eval_fixture_url(
            "http://192.168.1.1:43127/fixtures/computer-use/form.html",
            "http://127.0.0.1:43127",
        )
        .is_err());
    }
}
