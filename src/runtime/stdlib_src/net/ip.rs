//! AdeshLang IP Address Abstraction
//!
//! Complete IPv4 and IPv6 support including scope zone indices, classification,
//! parsing, canonical formatting, and conversion.

use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IPAddress {
    V4(Ipv4Addr),
    V6(Ipv6Addr, Option<String>),
}

impl IPAddress {
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("Empty IP address string".to_string());
        }

        // Check for IPv6 zone identifier, e.g. fe80::1%eth0
        if let Some(percent_pos) = trimmed.find('%') {
            let addr_part = &trimmed[..percent_pos];
            let zone_part = &trimmed[percent_pos + 1..];
            if zone_part.is_empty() {
                return Err("Empty zone identifier in IPv6 address".to_string());
            }
            let ipv6 = Ipv6Addr::from_str(addr_part)
                .map_err(|e| format!("Invalid IPv6 address '{}': {}", addr_part, e))?;
            return Ok(IPAddress::V6(ipv6, Some(zone_part.to_string())));
        }

        // Try direct parsing as IpAddr
        if let Ok(ip) = IpAddr::from_str(trimmed) {
            return match ip {
                IpAddr::V4(v4) => Ok(IPAddress::V4(v4)),
                IpAddr::V6(v6) => Ok(IPAddress::V6(v6, None)),
            };
        }

        Err(format!("Invalid IP address: '{}'", s))
    }

    pub fn is_v4(&self) -> bool {
        matches!(self, IPAddress::V4(_))
    }

    pub fn is_v6(&self) -> bool {
        matches!(self, IPAddress::V6(_, _))
    }

    pub fn is_loopback(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_loopback(),
            IPAddress::V6(addr, _) => addr.is_loopback(),
        }
    }

    pub fn is_private(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_private(),
            IPAddress::V6(addr, _) => {
                // Unique Local Address (fc00::/7)
                (addr.segments()[0] & 0xfe00) == 0xfc00
            }
        }
    }

    pub fn is_link_local(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_link_local(),
            IPAddress::V6(addr, _) => (addr.segments()[0] & 0xffc0) == 0xfe80,
        }
    }

    pub fn is_multicast(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_multicast(),
            IPAddress::V6(addr, _) => addr.is_multicast(),
        }
    }

    pub fn is_broadcast(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_broadcast(),
            IPAddress::V6(_, _) => false,
        }
    }

    pub fn is_unspecified(&self) -> bool {
        match self {
            IPAddress::V4(addr) => addr.is_unspecified(),
            IPAddress::V6(addr, _) => addr.is_unspecified(),
        }
    }

    pub fn is_global(&self) -> bool {
        !self.is_private()
            && !self.is_loopback()
            && !self.is_link_local()
            && !self.is_unspecified()
            && !self.is_multicast()
            && !self.is_documentation()
            && !self.is_benchmark()
    }

    pub fn is_documentation(&self) -> bool {
        match self {
            IPAddress::V4(addr) => {
                let octets = addr.octets();
                // 192.0.2.0/24 (TEST-NET-1), 198.51.100.0/24 (TEST-NET-2), 203.0.113.0/24 (TEST-NET-3)
                (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                    || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                    || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
            }
            IPAddress::V6(addr, _) => {
                // 2001:db8::/32
                let segs = addr.segments();
                segs[0] == 0x2001 && segs[1] == 0x0db8
            }
        }
    }

    pub fn is_benchmark(&self) -> bool {
        match self {
            IPAddress::V4(addr) => {
                let octets = addr.octets();
                // 198.18.0.0/15
                octets[0] == 198 && (octets[1] & 0xfe) == 18
            }
            IPAddress::V6(addr, _) => {
                let segs = addr.segments();
                // 2001:2::/48
                segs[0] == 0x2001 && segs[1] == 0x0002
            }
        }
    }

    pub fn to_canonical_string(&self) -> String {
        match self {
            IPAddress::V4(addr) => addr.to_string(),
            IPAddress::V6(addr, None) => addr.to_string(),
            IPAddress::V6(addr, Some(zone)) => format!("{}%{}", addr, zone),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            IPAddress::V4(addr) => addr.octets().to_vec(),
            IPAddress::V6(addr, _) => addr.octets().to_vec(),
        }
    }

    pub fn to_ipv4(&self) -> Option<IPAddress> {
        match self {
            IPAddress::V4(_) => Some(self.clone()),
            IPAddress::V6(v6, _) => v6.to_ipv4_mapped().map(IPAddress::V4),
        }
    }

    pub fn to_ipv6(&self) -> IPAddress {
        match self {
            IPAddress::V4(v4) => IPAddress::V6(v4.to_ipv6_mapped(), None),
            IPAddress::V6(_, _) => self.clone(),
        }
    }

    pub fn to_std_ip(&self) -> IpAddr {
        match self {
            IPAddress::V4(v4) => IpAddr::V4(*v4),
            IPAddress::V6(v6, _) => IpAddr::V6(*v6),
        }
    }

    pub fn to_value(&self) -> Value {
        let mut map = FastMap::default();
        map.insert(
            "version".to_string(),
            Value::Str(if self.is_v4() {
                "v4".to_string()
            } else {
                "v6".to_string()
            }),
        );
        map.insert(
            "address".to_string(),
            Value::Str(self.to_canonical_string()),
        );
        map.insert("isLoopback".to_string(), Value::Bool(self.is_loopback()));
        map.insert("isPrivate".to_string(), Value::Bool(self.is_private()));
        map.insert("isLinkLocal".to_string(), Value::Bool(self.is_link_local()));
        map.insert("isMulticast".to_string(), Value::Bool(self.is_multicast()));
        map.insert("isBroadcast".to_string(), Value::Bool(self.is_broadcast()));
        map.insert(
            "isUnspecified".to_string(),
            Value::Bool(self.is_unspecified()),
        );
        map.insert("isGlobal".to_string(), Value::Bool(self.is_global()));
        map.insert(
            "isDocumentation".to_string(),
            Value::Bool(self.is_documentation()),
        );
        map.insert("isBenchmark".to_string(), Value::Bool(self.is_benchmark()));

        if let IPAddress::V6(_, Some(zone)) = self {
            map.insert("zone".to_string(), Value::Str(zone.clone()));
        }

        Value::Object(Arc::new(map))
    }
}
