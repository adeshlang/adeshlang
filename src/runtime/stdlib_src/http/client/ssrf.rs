//! Post-DNS SSRF guards for outbound HTTP.
//!
//! Blocks connections to loopback, RFC1918 private ranges, link-local /
//! cloud-metadata addresses, CGNAT, and the IPv6 equivalents (including
//! IPv4-mapped IPv6 forms of the blocked IPv4 ranges).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Return `true` when `ip` must not be contacted by the default HttpClient.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    // 0.0.0.0/8 — "this" network / unspecified
    if octets[0] == 0 {
        return true;
    }
    // Loopback 127.0.0.0/8
    if ip.is_loopback() {
        return true;
    }
    // RFC1918 private
    if ip.is_private() {
        return true;
    }
    // Link-local 169.254.0.0/16 (includes cloud metadata 169.254.169.254)
    if ip.is_link_local() {
        return true;
    }
    // CGNAT 100.64.0.0/10
    if octets[0] == 100 && (octets[1] & 0xc0) == 64 {
        return true;
    }
    // Benchmarking 198.18.0.0/15
    if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
        return true;
    }
    // Documentation 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
    if (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
    {
        return true;
    }
    // Multicast / broadcast / reserved
    if ip.is_multicast() || ip.is_broadcast() {
        return true;
    }
    false
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    // Unwrap IPv4-mapped (::ffff:x.x.x.x) and check the embedded v4.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_ipv4(v4);
    }
    // Also handle SIIT / deprecated forms that `to_ipv4()` may surface.
    if let Some(v4) = ip.to_ipv4() {
        if is_blocked_ipv4(v4) {
            return true;
        }
    }
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }
    // Unique-local fc00::/7
    let segments = ip.segments();
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // Link-local fe80::/10
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // Multicast ff00::/8
    if ip.is_multicast() {
        return true;
    }
    // Discard-only 100::/64
    if segments[0] == 0x0100
        && segments[1] == 0
        && segments[2] == 0
        && segments[3] == 0
    {
        return true;
    }
    false
}

/// Filter a resolved address list, returning only publicly routable targets.
pub fn filter_public_addrs(addrs: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, String> {
    let public: Vec<SocketAddr> = addrs
        .into_iter()
        .filter(|a| !is_blocked_ip(a.ip()))
        .collect();
    if public.is_empty() {
        Err(
            "SSRF protection: all resolved addresses are in blocked private/loopback/link-local ranges"
                .to_string(),
        )
    } else {
        Ok(public)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn blocks_loopback_and_rfc1918() {
        assert!(is_blocked_ip(IpAddr::from_str("127.0.0.1").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("127.0.0.2").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("10.1.2.3").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("172.16.5.6").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("172.31.255.255").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("192.168.0.1").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("169.254.169.254").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("100.64.1.1").unwrap()));
    }

    #[test]
    fn allows_public_v4() {
        assert!(!is_blocked_ip(IpAddr::from_str("8.8.8.8").unwrap()));
        assert!(!is_blocked_ip(IpAddr::from_str("1.1.1.1").unwrap()));
        // 172.32 is outside 172.16/12
        assert!(!is_blocked_ip(IpAddr::from_str("172.32.0.1").unwrap()));
    }

    #[test]
    fn blocks_v6_private_and_mapped() {
        assert!(is_blocked_ip(IpAddr::from_str("::1").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("fc00::1").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("fe80::1").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("::ffff:127.0.0.1").unwrap()));
        assert!(is_blocked_ip(IpAddr::from_str("::ffff:10.0.0.1").unwrap()));
    }
}
