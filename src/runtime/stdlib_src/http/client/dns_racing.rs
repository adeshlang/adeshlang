use super::super::errors::{HttpError, HttpErrorKind};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Returns true if an IPv4 address belongs to loopback, private, link-local,
/// CGNAT, multicast, broadcast, unspecified, or reserved internal ranges.
pub fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_documentation()
        // 0.0.0.0/8 ("this network")
        || o[0] == 0
        // 100.64.0.0/10 (CGNAT / Shared Address Space, RFC 6598)
        || (o[0] == 100 && (o[1] & 0b1100_0000) == 64)
        // 192.0.0.0/24 (IETF Protocol Assignments)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        // 198.18.0.0/15 (Benchmarking)
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        // 240.0.0.0/4 (Reserved for future use)
        || o[0] >= 240
}

/// Returns true if an IPv6 address is loopback, unspecified, ULA, link-local,
/// multicast, or embeds a blocked IPv4 address (IPv4-mapped, IPv4-compatible, NAT64).
pub fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_ipv4(v4);
    }
    let segs = ip.segments();
    // Deprecated IPv4-compatible ::a.b.c.d (excluding :: and ::1 handled below)
    if segs[0..6] == [0, 0, 0, 0, 0, 0] && (segs[6] != 0 || segs[7] > 1) {
        let v4 = Ipv4Addr::new(
            (segs[6] >> 8) as u8,
            (segs[6] & 0xff) as u8,
            (segs[7] >> 8) as u8,
            (segs[7] & 0xff) as u8,
        );
        return is_blocked_ipv4(v4);
    }
    // NAT64 well-known prefix 64:ff9b::/96
    if segs[0] == 0x0064 && segs[1] == 0xff9b && segs[2..6] == [0, 0, 0, 0] {
        let v4 = Ipv4Addr::new(
            (segs[6] >> 8) as u8,
            (segs[6] & 0xff) as u8,
            (segs[7] >> 8) as u8,
            (segs[7] & 0xff) as u8,
        );
        return is_blocked_ipv4(v4);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // fc00::/7 Unique Local Addresses (ULA)
        || (segs[0] & 0xfe00) == 0xfc00
        // fe80::/10 Link-Local Unicast
        || (segs[0] & 0xffc0) == 0xfe80
        // fec0::/10 Deprecated Site-Local
        || (segs[0] & 0xffc0) == 0xfec0
}

/// Returns true if an IP address must be blocked by SSRF protections.
pub fn is_blocked_ssrf_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

/// Validate a hostname before DNS resolution for known internal/metadata names.
pub fn validate_ssrf_host(host: &str) -> Result<(), HttpError> {
    #[cfg(feature = "allow_internal_network")]
    {
        let _ = host;
        return Ok(());
    }

    #[cfg(not(feature = "allow_internal_network"))]
    {
        let normalized = host
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches('.')
            .to_ascii_lowercase();

        if normalized.is_empty() {
            return Err(HttpError::new(
                HttpErrorKind::InvalidUri,
                "Target URI has empty host",
            ));
        }

        if normalized == "localhost"
            || normalized.ends_with(".localhost")
            || normalized.ends_with(".local")
            || normalized.ends_with(".internal")
            || normalized == "metadata.google.internal"
            || normalized == "metadata"
        {
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!("SSRF protection blocked internal host '{}'", host),
            ));
        }

        if let Ok(ip) = normalized.parse::<IpAddr>() {
            if is_blocked_ssrf_ip(ip) {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!("SSRF protection blocked private/internal IP '{}'", ip),
                ));
            }
        }

        Ok(())
    }
}

/// Happy Eyeballs RFC 8305 Dual-Stack Connection Racing with post-DNS SSRF validation.
pub fn happy_eyeballs_connect(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<TcpStream, HttpError> {
    happy_eyeballs_connect_with_policy(
        host,
        port,
        timeout,
        cfg!(feature = "allow_internal_network"),
    )
}

/// Connect using Happy Eyeballs with explicit control over internal network access.
pub fn happy_eyeballs_connect_with_policy(
    host: &str,
    port: u16,
    timeout: Duration,
    allow_internal_network: bool,
) -> Result<TcpStream, HttpError> {
    if !allow_internal_network {
        validate_ssrf_host(host)?;
    }

    let lookup_host = if host.contains(':') && !host.starts_with('[') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    };

    let addrs: Vec<SocketAddr> = lookup_host
        .to_socket_addrs()
        .map_err(|e| {
            HttpError::new(
                HttpErrorKind::DnsError,
                format!("DNS resolution failed for {}: {}", host, e),
            )
        })?
        .collect();

    if addrs.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::DnsError,
            format!("No IP addresses found for {}", host),
        ));
    }

    // Post-DNS SSRF check: verify every resolved address before connecting so
    // DNS rebinding, hex/octal/integer IPs, and IPv4-mapped IPv6 cannot bypass checks.
    if !allow_internal_network {
        for addr in &addrs {
            if is_blocked_ssrf_ip(addr.ip()) {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!(
                        "SSRF protection blocked resolved internal IP '{}' for host '{}'",
                        addr.ip(),
                        host
                    ),
                ));
            }
        }
    }

    // Connect directly to the already-validated SocketAddr (no second DNS lookup).
    for addr in addrs {
        if let Ok(stream) = TcpStream::connect_timeout(&addr, timeout) {
            let _ = stream.set_nodelay(true);
            return Ok(stream);
        }
    }

    Err(HttpError::new(
        HttpErrorKind::ConnectError,
        format!(
            "Failed to connect to {}:{} on all resolved IP addresses",
            host, port
        ),
    ))
}
