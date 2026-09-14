//! AdeshLang Socket Address Abstraction
//!
//! Pairs IPAddress with a validated port number (0..=65535).

use super::ip::IPAddress;
use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SocketAddress {
    pub ip: IPAddress,
    pub port: u16,
}

impl SocketAddress {
    pub fn new(ip: IPAddress, port: u16) -> Self {
        Self { ip, port }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("Empty socket address string".to_string());
        }

        // Bracketed IPv6 format: [::1]:8080 or [2001:db8::1]:443
        if trimmed.starts_with('[') {
            if let Some(close_bracket) = trimmed.find(']') {
                let ip_str = &trimmed[1..close_bracket];
                let rest = &trimmed[close_bracket + 1..];
                if !rest.starts_with(':') {
                    return Err(format!(
                        "Missing colon port separator in bracketed address '{}'",
                        s
                    ));
                }
                let port_str = &rest[1..];
                let port = port_str
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid port '{}' in socket address", port_str))?;
                let ip = IPAddress::parse(ip_str)?;
                return Ok(SocketAddress::new(ip, port));
            } else {
                return Err("Unclosed bracket in IPv6 socket address".to_string());
            }
        }

        // Check for single colon separating IPv4 or hostname and port: 127.0.0.1:8080
        let colon_count = trimmed.chars().filter(|&c| c == ':').count();
        if colon_count > 1 {
            return Err(format!(
                "Ambiguous unbracketed IPv6 address with port '{}'. Use bracketed format '[address]:port'.",
                s
            ));
        }

        if let Some(colon_pos) = trimmed.rfind(':') {
            let ip_str = &trimmed[..colon_pos];
            let port_str = &trimmed[colon_pos + 1..];
            let port = port_str
                .parse::<u16>()
                .map_err(|_| format!("Invalid port '{}' in socket address", port_str))?;
            let ip = IPAddress::parse(ip_str)?;
            return Ok(SocketAddress::new(ip, port));
        }

        Err(format!("Socket address missing port: '{}'", s))
    }

    pub fn to_canonical_string(&self) -> String {
        match &self.ip {
            IPAddress::V4(_) => format!("{}:{}", self.ip.to_canonical_string(), self.port),
            IPAddress::V6(_, _) => format!("[{}]:{}", self.ip.to_canonical_string(), self.port),
        }
    }

    pub fn to_std_socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip.to_std_ip(), self.port)
    }

    pub fn to_value(&self) -> Value {
        let mut map = FastMap::default();
        map.insert("ip".to_string(), self.ip.to_value());
        map.insert("port".to_string(), Value::U16(self.port));
        map.insert(
            "address".to_string(),
            Value::Str(self.to_canonical_string()),
        );
        Value::Object(Arc::new(map))
    }
}
