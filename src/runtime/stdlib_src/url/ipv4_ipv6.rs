//! IPv4 and IPv6 Address Parsing, Canonicalization, and Classification for AdeshLang URL.

use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IPAddress {
    V4(Ipv4Addr),
    V6(Ipv6Addr, Option<String>), // Address + optional Zone ID (%25eth0)
}

impl IPAddress {
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len() - 1];
            Self::parse_v6(inner)
        } else if inner_contains_colon(trimmed) {
            Self::parse_v6(trimmed)
        } else {
            Self::parse_v4(trimmed)
        }
    }

    pub fn parse_v4(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if let Ok(addr) = s.parse::<Ipv4Addr>() {
            return Ok(IPAddress::V4(addr));
        }

        // Support hex / octal / integer IPv4 formats if strict parsing allows,
        // but for security standard RFC 3986 requires standard dotted quad or strict parse.
        // Let's parse dotted quad explicitly:
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 4 {
            let mut bytes = [0u8; 4];
            for (i, p) in parts.iter().enumerate() {
                if p.is_empty() || (p.len() > 1 && p.starts_with('0')) {
                    // Leading zeros in IPv4 octets can cause ambiguity (octal confusion / SSRF bypass)
                    return Err(format!("Ambiguous or malformed IPv4 octet '{}'", p));
                }
                match p.parse::<u8>() {
                    Ok(b) => bytes[i] = b,
                    Err(_) => return Err(format!("Invalid IPv4 octet '{}'", p)),
                }
            }
            return Ok(IPAddress::V4(Ipv4Addr::from(bytes)));
        }

        Err(format!("Invalid IPv4 address format: {}", s))
    }

    pub fn parse_v6(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (addr_part, zone_id) = if let Some(pos) = s.find("%25").or_else(|| s.find('%')) {
            let (addr_str, zone_str) = s.split_at(pos);
            let zone = if zone_str.starts_with("%25") {
                &zone_str[3..]
            } else {
                &zone_str[1..]
            };
            (addr_str, Some(zone.to_string()))
        } else {
            (s, None)
        };

        if let Ok(addr) = addr_part.parse::<Ipv6Addr>() {
            return Ok(IPAddress::V6(addr, zone_id));
        }

        Err(format!("Invalid IPv6 address format: {}", s))
    }

    pub fn is_loopback(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_loopback(),
            IPAddress::V6(addr, _) => addr.is_loopback(),
        }
    }

    pub fn is_private(&self) -> bool {
        match self {
            IPAddress::V4(addr) => {
                let octets = addr.octets();
                // 10.0.0.0/8
                octets[0] == 10
                // 172.16.0.0/12
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                // 192.168.0.0/16
                || (octets[0] == 192 && octets[1] == 168)
                // 127.0.0.0/8 (loopback is also private)
                || octets[0] == 127
            }
            IPAddress::V6(addr, _) => {
                let segments = addr.segments();
                // fc00::/7 (Unique Local Address)
                (segments[0] & 0xfe00) == 0xfc00
                // fe80::/10 (Link Local Address)
                || (segments[0] & 0xffc0) == 0xfe80
                // ::1 (Loopback)
                || addr.is_loopback()
                // IPv4-mapped private IPv4
                || if let Some(v4) = addr.to_ipv4_mapped() {
                    IPAddress::V4(v4).is_private()
                } else {
                    false
                }
            }
        }
    }

    pub fn is_link_local(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_link_local(),
            IPAddress::V6(addr, _) => {
                let segments = addr.segments();
                (segments[0] & 0xffc0) == 0xfe80
            }
        }
    }

    pub fn is_multicast(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_multicast(),
            IPAddress::V6(addr, _) => addr.is_multicast(),
        }
    }

    pub fn is_unspecified(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_unspecified(),
            IPAddress::V6(addr, _) => addr.is_unspecified(),
        }
    }

    pub fn is_global(&self) -> bool {
        !self.is_loopback()
            && !self.is_private()
            && !self.is_link_local()
            && !self.is_multicast()
            && !self.is_unspecified()
    }

    pub fn to_canonical_string(&self) -> String {
        match self {
            IPAddress::V4(addr) => addr.to_string(),
            IPAddress::V6(addr, zone) => {
                if let Some(z) = zone {
                    format!("[{}%25{}]", addr, z)
                } else {
                    format!("[{}]", addr)
                }
            }
        }
    }
}

fn inner_contains_colon(s: &str) -> bool {
    s.contains(':')
}
