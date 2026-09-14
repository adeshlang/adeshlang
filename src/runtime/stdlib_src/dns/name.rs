#![allow(dead_code)]

//! DNS Name representation and normalization.

use std::fmt;

/// Maximum length of a single label in octets.
pub const MAX_LABEL_LEN: usize = 63;
/// Maximum length of a full domain name in octets.
pub const MAX_DOMAIN_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DNSName {
    labels: Vec<String>,
}

impl DNSName {
    /// Parse a domain name string into a normalized `DNSName`.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("DNS name cannot be empty".to_string());
        }

        if trimmed == "." {
            return Ok(Self { labels: Vec::new() });
        }

        // Strip trailing dot if present for uniform label representation
        let name_str = if trimmed.ends_with('.') {
            &trimmed[..trimmed.len() - 1]
        } else {
            trimmed
        };

        if name_str.is_empty() {
            return Err("Invalid DNS name with empty label".to_string());
        }

        let parts: Vec<&str> = name_str.split('.').collect();
        let mut labels = Vec::with_capacity(parts.len());

        let mut total_wire_len = 1; // root null byte

        for part in parts {
            if part.is_empty() {
                return Err("DNS name contains an empty label".to_string());
            }

            // Convert label to ASCII if it contains non-ASCII characters (IDN / Punycode)
            let ascii_label = if part.is_ascii() {
                part.to_ascii_lowercase()
            } else {
                match idna_to_ascii(part) {
                    Ok(encoded) => encoded.to_ascii_lowercase(),
                    Err(e) => {
                        return Err(format!("IDNA encoding failed for label '{}': {}", part, e));
                    }
                }
            };

            if ascii_label.len() > MAX_LABEL_LEN {
                return Err(format!(
                    "Label '{}' exceeds maximum allowed length of {} octets",
                    ascii_label, MAX_LABEL_LEN
                ));
            }

            total_wire_len += ascii_label.len() + 1; // 1 byte for label length prefix
            labels.push(ascii_label);
        }

        if total_wire_len > MAX_DOMAIN_LEN {
            return Err(format!(
                "DNS domain name exceeds maximum wire format length of {} octets (got {})",
                MAX_DOMAIN_LEN, total_wire_len
            ));
        }

        Ok(Self { labels })
    }

    /// Create root domain name "."
    pub fn root() -> Self {
        Self { labels: Vec::new() }
    }

    pub fn is_root(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Format as canonical string (without trailing dot unless root)
    pub fn to_string_canonical(&self) -> String {
        if self.is_root() {
            ".".to_string()
        } else {
            self.labels.join(".")
        }
    }

    /// Format with explicit trailing dot
    pub fn to_fqdn(&self) -> String {
        if self.is_root() {
            ".".to_string()
        } else {
            format!("{}.", self.labels.join("."))
        }
    }

    /// Return wire-format encoded bytes for the domain name
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for label in &self.labels {
            bytes.push(label.len() as u8);
            bytes.extend_from_slice(label.as_bytes());
        }
        bytes.push(0); // Root label null byte
        bytes
    }
}

impl fmt::Display for DNSName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_canonical())
    }
}

/// Helper for converting non-ASCII label using basic Punycode / IDNA prefix "xn--"
fn idna_to_ascii(label: &str) -> Result<String, String> {
    let mut puny = String::from("xn--");
    let mut num_basic = 0;
    for c in label.chars() {
        if c.is_ascii() {
            puny.push(c.to_ascii_lowercase());
            num_basic += 1;
        }
    }

    if num_basic > 0 {
        puny.push('-');
    }

    for c in label.chars() {
        if !c.is_ascii() {
            puny.push_str(&format!("{:x}", c as u32));
        }
    }

    Ok(puny)
}
