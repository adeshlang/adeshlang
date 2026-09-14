//! Built-in Validated HTTP & Domain Types for AdeshLang HTTP Standard Library.
//! Provides boundary validation for Email, URL, UUID, IP addresses, Hostnames, MediaTypes, DateTimes, etc.

use std::fmt;

/// Built-in Domain Type Specifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DomainTypeKind {
    Email,
    URL,
    UUID,
    IPv4,
    IPv6,
    IP,
    Hostname,
    HttpMethod,
    MediaType,
    DateTime,
    ETag,
}

impl DomainTypeKind {
    pub fn name(&self) -> &'static str {
        match self {
            DomainTypeKind::Email => "Email",
            DomainTypeKind::URL => "URL",
            DomainTypeKind::UUID => "UUID",
            DomainTypeKind::IPv4 => "IPv4",
            DomainTypeKind::IPv6 => "IPv6",
            DomainTypeKind::IP => "IP",
            DomainTypeKind::Hostname => "Hostname",
            DomainTypeKind::HttpMethod => "HttpMethod",
            DomainTypeKind::MediaType => "MediaType",
            DomainTypeKind::DateTime => "DateTime",
            DomainTypeKind::ETag => "ETag",
        }
    }

    /// Validates if a string value conforms to the domain type rules.
    pub fn validate_str(&self, val: &str) -> Result<(), String> {
        let val = val.trim();
        if val.is_empty() {
            return Err(format!("{} value cannot be empty", self.name()));
        }

        match self {
            DomainTypeKind::Email => {
                if !val.contains('@') || !val.contains('.') || val.len() > 254 {
                    return Err(format!("Invalid email address format: '{}'", val));
                }
                let parts: Vec<&str> = val.split('@').collect();
                if parts.len() != 2
                    || parts[0].is_empty()
                    || parts[1].is_empty()
                    || !parts[1].contains('.')
                {
                    return Err(format!("Invalid email address format: '{}'", val));
                }
            }
            DomainTypeKind::URL => {
                if !(val.starts_with("http://")
                    || val.starts_with("https://")
                    || val.starts_with("ftp://")
                    || val.starts_with("ws://")
                    || val.starts_with("wss://"))
                {
                    return Err(format!(
                        "Invalid URL format (must specify valid scheme): '{}'",
                        val
                    ));
                }
            }
            DomainTypeKind::UUID => {
                let clean = val.replace('-', "");
                if clean.len() != 32 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(format!("Invalid UUID format: '{}'", val));
                }
            }
            DomainTypeKind::IPv4 => {
                if val.parse::<std::net::Ipv4Addr>().is_err() {
                    return Err(format!("Invalid IPv4 address: '{}'", val));
                }
            }
            DomainTypeKind::IPv6 => {
                if val.parse::<std::net::Ipv6Addr>().is_err() {
                    return Err(format!("Invalid IPv6 address: '{}'", val));
                }
            }
            DomainTypeKind::IP => {
                if val.parse::<std::net::IpAddr>().is_err() {
                    return Err(format!("Invalid IP address (IPv4 or IPv6): '{}'", val));
                }
            }
            DomainTypeKind::Hostname => {
                if val.len() > 253 || val.contains('/') || val.contains(':') {
                    return Err(format!("Invalid hostname format: '{}'", val));
                }
                for label in val.split('.') {
                    if label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                    {
                        return Err(format!("Invalid hostname label in '{}'", val));
                    }
                }
            }
            DomainTypeKind::HttpMethod => {
                let m = val.to_uppercase();
                match m.as_str() {
                    "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
                    | "CONNECT" | "TRACE" | "QUERY" => {}
                    _ => return Err(format!("Invalid HTTP method: '{}'", val)),
                }
            }
            DomainTypeKind::MediaType => {
                if !val.contains('/') || val.contains('\r') || val.contains('\n') {
                    return Err(format!("Invalid MediaType/Content-Type format: '{}'", val));
                }
            }
            DomainTypeKind::DateTime => {
                // Accepts ISO-8601 / RFC 3339 formats, e.g. 2026-08-16T22:00:00Z
                if !val.contains('T') && !val.contains(' ') {
                    return Err(format!(
                        "Invalid DateTime format (expected ISO-8601 / RFC-3339): '{}'",
                        val
                    ));
                }
            }
            DomainTypeKind::ETag => {
                if !(val.starts_with('"') && val.ends_with('"'))
                    && !(val.starts_with("W/\"") && val.ends_with('"'))
                {
                    return Err(format!(
                        "Invalid ETag format (must be quoted or weak-quoted): '{}'",
                        val
                    ));
                }
            }
        }
        Ok(())
    }
}

impl fmt::Display for DomainTypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
