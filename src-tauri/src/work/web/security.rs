//! Security and SSRF validation for AgentCabin Native Web Capability.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

pub fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    let (a, b) = (octets[0], octets[1]);

    a == 0 // 0.0.0.0/8
        || a == 10 // 10.0.0.0/8
        || a == 127 // 127.0.0.0/8 (Loopback)
        || (a == 169 && b == 254) // 169.254.0.0/16 (Link Local)
        || (a == 172 && (16..=31).contains(&b)) // 172.16.0.0/12
        || (a == 192 && b == 168) // 192.168.0.0/16
        || (a == 100 && (64..=127).contains(&b)) // 100.64.0.0/10 (Carrier-grade NAT)
        || (a == 198 && (b == 18 || b == 19)) // 198.18.0.0/15 (Benchmark / Synthetic)
        || a >= 224 // 224.0.0.0/4 Multicast & Reserved (>=240.0.0.0)
}

pub fn is_proxy_synthetic_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 198 && (octets[1] == 18 || octets[1] == 19)
}

pub fn is_private_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() {
        return true;
    }
    let segments = ip.segments();

    // Link-local: fe80::/10 (first segment fe80..febf)
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // Unique local: fc00::/7 (fc00..fdff)
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // Multicast: ff00::/8
    if (segments[0] & 0xff00) == 0xff00 {
        return true;
    }
    // Documentation: 2001:db8::/32
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return true;
    }

    // IPv4-mapped IPv6: ::ffff:a.b.c.d
    if let Some(mapped_v4) = ip.to_ipv4_mapped() {
        return is_private_ipv4(mapped_v4);
    }

    // NAT64 well-known prefix: 64:ff9b::/96
    if segments[0] == 0x0064
        && segments[1] == 0xff9b
        && segments[2] == 0
        && segments[3] == 0
        && segments[4] == 0
        && segments[5] == 0
    {
        let v4 = Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            (segments[6] & 0xff) as u8,
            (segments[7] >> 8) as u8,
            (segments[7] & 0xff) as u8,
        );
        return is_private_ipv4(v4);
    }

    false
}

pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_ipv4(v4),
        IpAddr::V6(v6) => is_private_ipv6(v6),
    }
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
pub fn matches_ipv4_cidr(ip: std::net::Ipv4Addr, cidr: &str) -> bool {
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

pub fn reject_unsafe_hostname(hostname: &str) -> Result<(), String> {
    let host = hostname
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host == "metadata.google.internal"
    {
        return Err("网络访问 URL 指向本机、内网或云 metadata 地址，已拒绝".into());
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err("网络访问 URL 指向本机、内网或云 metadata 地址，已拒绝".into());
        }
    }

    Ok(())
}

/// Validate URL syntax, schemes, credentials, hostnames, and DNS records against SSRF policy with allowed host rules.
pub async fn assert_public_url_with_allowed_hosts(
    raw_url: &str,
    proxy_active: bool,
    allowed_hosts: &[String],
) -> Result<Url, String> {
    let parsed = Url::parse(raw_url.trim()).map_err(|_| "网络访问 URL 无效".to_string())?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("网络访问仅允许不带凭据的 http/https 公网 URL".into());
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("网络访问仅允许不带凭据的 http/https 公网 URL".into());
    }

    let Some(host_str) = parsed.host_str() else {
        return Err("网络访问 URL 无效".into());
    };

    let lower_host = host_str.trim().to_ascii_lowercase();
    if lower_host == "metadata.google.internal" || lower_host == "instance-data" {
        return Err("网络访问 URL 指向本机、内网或云 metadata 地址，已拒绝".into());
    }

    // Resolve DNS or direct IP
    let port = parsed.port_or_known_default().unwrap_or(80);
    let ips: Vec<IpAddr> = if let Ok(ip) = host_str.parse::<IpAddr>() {
        vec![ip]
    } else {
        let socket_addrs = tokio::net::lookup_host(format!("{}:{}", host_str, port))
            .await
            .map_err(|error| {
                let msg = error.to_string();
                if msg.contains("已拒绝") {
                    msg
                } else {
                    "网络访问 URL 无法完成安全 DNS 校验".to_string()
                }
            })?
            .collect::<Vec<_>>();

        if socket_addrs.is_empty() {
            return Err("网络访问 URL 无法完成安全 DNS 校验".into());
        }
        socket_addrs.into_iter().map(|a| a.ip()).collect()
    };

    // Metadata is NEVER allowed
    if ips.iter().any(|&ip| is_dangerous_metadata_or_special(ip)) {
        return Err("网络访问 URL 指向本机、内网或云 metadata 地址，已拒绝".into());
    }

    // Check allowlist
    if is_host_or_ip_allowed(host_str, &ips, allowed_hosts) {
        return Ok(parsed);
    }

    reject_unsafe_hostname(host_str)?;

    for ip in ips {
        let is_exempt_proxy = match ip {
            IpAddr::V4(v4) => proxy_active && is_proxy_synthetic_ipv4(v4),
            _ => false,
        };

        if is_private_ip(ip) && !is_exempt_proxy {
            return Err("网络访问 URL 的 DNS 解析包含内网地址，已拒绝".into());
        }
    }

    Ok(parsed)
}

/// Validate URL syntax, schemes, credentials, hostnames, and DNS records.
pub async fn assert_public_url(raw_url: &str, proxy_active: bool) -> Result<Url, String> {
    assert_public_url_with_allowed_hosts(raw_url, proxy_active, &[]).await
}

fn is_blocked_first_stage_host(hostname: &str) -> bool {
    let host = hostname.trim().to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    host == "github.com"
        || host.ends_with(".github.com")
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtu.be"
        || host.ends_with(".youtu.be")
}

const BLOCKED_FILE_EXTENSIONS: &[&str] = &[
    ".mp4", ".mov", ".webm", ".avi", ".mpeg", ".mpg", ".wmv", ".flv", ".3gp", ".pdf", ".zip",
];

/// First-stage policy with allowed hosts: HTML/text/JSON URLs only.
pub async fn assert_first_stage_url_with_allowed_hosts(
    raw_url: &str,
    proxy_active: bool,
    allowed_hosts: &[String],
) -> Result<Url, String> {
    let parsed = assert_public_url_with_allowed_hosts(raw_url, proxy_active, allowed_hosts).await?;

    let host = parsed.host_str().unwrap_or_default();
    if is_blocked_first_stage_host(host) {
        return Err("网络访问第一阶段暂不支持 GitHub 克隆或视频 URL".into());
    }

    let path_lower = parsed.path().to_ascii_lowercase();
    for ext in BLOCKED_FILE_EXTENSIONS {
        if path_lower.ends_with(ext) {
            return Err("网络访问第一阶段仅支持 HTML、纯文本或 JSON URL".into());
        }
    }

    Ok(parsed)
}

/// First-stage policy: public HTML/text/JSON URLs only.
pub async fn assert_first_stage_url(raw_url: &str, proxy_active: bool) -> Result<Url, String> {
    assert_first_stage_url_with_allowed_hosts(raw_url, proxy_active, &[]).await
}

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

pub fn is_blocked_hostname(host: &str) -> bool {
    let lower = host.trim().to_lowercase();
    lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower == "instance-data"
        || lower == "metadata.google.internal"
}

/// Validate a target URL against SSRF policy with allowed host rules.
pub fn validate_browser_url_with_allowed_hosts(
    raw_url: &str,
    allowed_hosts: &[String],
    allowed_output_root: Option<&std::path::Path>,
) -> Result<Url, UrlValidationError> {
    use std::net::ToSocketAddrs;
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
        if is_private_ip(ip) {
            return Err(UrlValidationError::BlockedIp(ip.to_string()));
        }
    }

    Ok(parsed)
}

/// Validate a target URL against SSRF policy (disallowing file:// by default).
pub fn validate_browser_url(raw_url: &str) -> Result<Url, UrlValidationError> {
    validate_browser_url_with_allowed_hosts(raw_url, &[], None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_private_ipv4_addresses() {
        assert!(is_private_ipv4("127.0.0.1".parse().unwrap()));
        assert!(is_private_ipv4("10.0.0.1".parse().unwrap()));
        assert!(is_private_ipv4("172.16.0.1".parse().unwrap()));
        assert!(is_private_ipv4("172.31.255.255".parse().unwrap()));
        assert!(is_private_ipv4("192.168.1.1".parse().unwrap()));
        assert!(is_private_ipv4("169.254.169.254".parse().unwrap()));
        assert!(is_private_ipv4("100.64.0.1".parse().unwrap()));
        assert!(is_private_ipv4("198.18.0.1".parse().unwrap()));
        assert!(is_private_ipv4("224.0.0.1".parse().unwrap()));
        assert!(is_private_ipv4("240.0.0.1".parse().unwrap()));
        assert!(is_private_ipv4("255.255.255.255".parse().unwrap()));

        assert!(!is_private_ipv4("8.8.8.8".parse().unwrap()));
        assert!(!is_private_ipv4("93.184.216.34".parse().unwrap()));
        assert!(!is_private_ipv4("172.32.0.1".parse().unwrap()));
    }

    #[test]
    fn rejects_private_ipv6_addresses() {
        assert!(is_private_ipv6("::1".parse().unwrap()));
        assert!(is_private_ipv6("::".parse().unwrap()));
        assert!(is_private_ipv6("fe80::1".parse().unwrap()));
        assert!(is_private_ipv6("fc00::1".parse().unwrap()));
        assert!(is_private_ipv6("fd12:3456:789a::1".parse().unwrap()));
        assert!(is_private_ipv6("ff02::1".parse().unwrap()));
        assert!(is_private_ipv6("2001:db8::1".parse().unwrap()));
        assert!(is_private_ipv6("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_private_ipv6("::ffff:192.168.1.1".parse().unwrap()));

        assert!(!is_private_ipv6("2001:4860:4860::8888".parse().unwrap()));
        assert!(!is_private_ipv6("::ffff:8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn rejects_unsafe_hostnames() {
        assert!(reject_unsafe_hostname("localhost").is_err());
        assert!(reject_unsafe_hostname("foo.localhost").is_err());
        assert!(reject_unsafe_hostname("app.local").is_err());
        assert!(reject_unsafe_hostname("service.internal").is_err());
        assert!(reject_unsafe_hostname("metadata.google.internal").is_err());
        assert!(reject_unsafe_hostname("127.0.0.1").is_err());
        assert!(reject_unsafe_hostname("192.168.1.5").is_err());
        assert!(reject_unsafe_hostname("[::1]").is_err());

        assert!(reject_unsafe_hostname("example.com").is_ok());
        assert!(reject_unsafe_hostname("doc.rust-lang.org").is_ok());
    }

    #[tokio::test]
    async fn url_security_checks() {
        assert!(assert_public_url("http://127.0.0.1:8080/", false)
            .await
            .is_err());
        assert!(assert_public_url("http://10.0.0.4/", false).await.is_err());
        assert!(assert_public_url("http://[::1]/", false).await.is_err());
        assert!(assert_public_url("http://metadata.google.internal/", false)
            .await
            .is_err());
        assert!(assert_public_url("http://user:pass@example.com/", false)
            .await
            .is_err());
        assert!(assert_public_url("ftp://example.com/file", false)
            .await
            .is_err());

        assert!(
            assert_first_stage_url("https://github.com/openai/agents", false)
                .await
                .is_err()
        );
        assert!(
            assert_first_stage_url("https://www.youtube.com/watch?v=123", false)
                .await
                .is_err()
        );
        assert!(
            assert_first_stage_url("https://example.com/file.pdf", false)
                .await
                .is_err()
        );
        assert!(
            assert_first_stage_url("https://example.com/file.zip", false)
                .await
                .is_err()
        );
        assert!(
            assert_first_stage_url("https://example.com/video.mp4", false)
                .await
                .is_err()
        );
    }
}
