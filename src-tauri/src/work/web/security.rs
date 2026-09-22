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
    if ips
        .iter()
        .any(|&ip| crate::work::browser_operator::security::is_dangerous_metadata_or_special(ip))
    {
        return Err("网络访问 URL 指向本机、内网或云 metadata 地址，已拒绝".into());
    }

    // Check allowlist
    if crate::work::browser_operator::security::is_host_or_ip_allowed(host_str, &ips, allowed_hosts)
    {
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
