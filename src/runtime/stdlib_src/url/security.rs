//! URL Security Policy, SSRF Protection & Target Validation for AdeshLang.

use super::idna_punycode;
use super::url_object::URL;

#[derive(Debug, Clone, Default)]
pub struct URLSecurityPolicy {
    allowed_schemes: Vec<String>,
    denied_schemes: Vec<String>,
    allowed_hosts: Vec<String>,
    denied_hosts: Vec<String>,
    deny_private_networks: bool,
    deny_credentials: bool,
    deny_non_default_ports: bool,
}

impl URLSecurityPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_scheme(mut self, scheme: &str) -> Self {
        self.allowed_schemes.push(scheme.to_lowercase());
        self
    }

    pub fn deny_scheme(mut self, scheme: &str) -> Self {
        self.denied_schemes.push(scheme.to_lowercase());
        self
    }

    pub fn allow_host(mut self, host: &str) -> Self {
        self.allowed_hosts.push(host.to_lowercase());
        self
    }

    pub fn deny_host(mut self, host: &str) -> Self {
        self.denied_hosts.push(host.to_lowercase());
        self
    }

    pub fn deny_private_networks(mut self) -> Self {
        self.deny_private_networks = true;
        self
    }

    pub fn deny_credentials(mut self) -> Self {
        self.deny_credentials = true;
        self
    }

    pub fn deny_non_default_ports(mut self) -> Self {
        self.deny_non_default_ports = true;
        self
    }

    pub fn allow_domain(mut self, domain: &str) -> Self {
        self.allowed_hosts.push(domain.to_lowercase());
        self
    }

    pub fn deny_domain(mut self, domain: &str) -> Self {
        self.denied_hosts.push(domain.to_lowercase());
        self
    }

    fn matches_domain_rule(rule: &str, host: &str) -> bool {
        let rule = rule.to_lowercase();
        let host = host.to_lowercase();
        if rule == host {
            return true;
        }
        if let Some(suffix) = rule.strip_prefix("*.") {
            if host.ends_with(suffix)
                && host.len() > suffix.len()
                && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
            {
                return true;
            }
        }
        false
    }

    pub fn validate(&self, url: &URL) -> Result<(), String> {
        let scheme = url.scheme();

        if !self.allowed_schemes.is_empty() && !self.allowed_schemes.contains(&scheme.to_string()) {
            return Err(format!("Scheme '{}' is not allowed by policy", scheme));
        }

        if self.denied_schemes.contains(&scheme.to_string()) {
            return Err(format!(
                "Scheme '{}' is explicitly denied by policy",
                scheme
            ));
        }

        if self.deny_credentials && url.has_credentials() {
            return Err("Embedded credentials in URL are denied by security policy".to_string());
        }

        if let Some(h) = url.host() {
            if !self.allowed_hosts.is_empty() {
                let is_allowed = self
                    .allowed_hosts
                    .iter()
                    .any(|rule| Self::matches_domain_rule(rule, h));
                if !is_allowed {
                    return Err(format!("Host '{}' is not in allowed hosts policy", h));
                }
            }

            let is_denied = self
                .denied_hosts
                .iter()
                .any(|rule| Self::matches_domain_rule(rule, h));
            if is_denied {
                return Err(format!("Host '{}' is explicitly denied by policy", h));
            }
        }

        if self.deny_non_default_ports {
            if let Some(port) = url.port() {
                if Some(port) != url.default_port() {
                    return Err(format!(
                        "Non-default port {} is denied by security policy",
                        port
                    ));
                }
            }
        }

        if self.deny_private_networks {
            self.validate_network_target(url)?;
        }

        Ok(())
    }

    pub fn validate_network_target(&self, url: &URL) -> Result<(), String> {
        if let Some(ip) = url.ip_address() {
            if ip.is_loopback() {
                return Err(format!(
                    "Destination IP {} is a loopback address (SSRF risk)",
                    ip.to_canonical_string()
                ));
            }
            if ip.is_private() {
                return Err(format!(
                    "Destination IP {} is a private network address (SSRF risk)",
                    ip.to_canonical_string()
                ));
            }
            if ip.is_link_local() {
                return Err(format!(
                    "Destination IP {} is a link-local address (SSRF risk)",
                    ip.to_canonical_string()
                ));
            }
            if ip.is_unspecified() {
                return Err(format!(
                    "Destination IP {} is an unspecified address (SSRF risk)",
                    ip.to_canonical_string()
                ));
            }
        }

        if let Some(h) = url.host() {
            let lower = h.to_lowercase();
            if lower == "localhost" || lower.ends_with(".localhost") || lower.ends_with(".local") {
                return Err(format!("Host '{}' points to local target (SSRF risk)", h));
            }
            // Check cloud metadata addresses
            if lower == "169.254.169.254" || lower == "metadata.google.internal" {
                return Err(format!("Host '{}' is a cloud metadata service endpoint", h));
            }
        }

        Ok(())
    }

    pub fn validate_redirect(&self, old_url: &URL, new_url: &URL) -> Result<(), String> {
        self.validate(new_url)?;

        // Check scheme downgrade (HTTPS -> HTTP)
        if old_url.is_secure() && !new_url.is_secure() {
            return Err(format!(
                "Insecure redirect downgrade from {} to {}",
                old_url.scheme(),
                new_url.scheme()
            ));
        }

        Ok(())
    }
}

pub fn url_diff(a: &URL, b: &URL) -> Vec<String> {
    let mut diffs = Vec::new();
    if a.scheme() != b.scheme() {
        diffs.push(format!("scheme: '{}' vs '{}'", a.scheme(), b.scheme()));
    }
    if a.host() != b.host() {
        diffs.push(format!("host: '{:?}' vs '{:?}'", a.host(), b.host()));
    }
    if a.port() != b.port() {
        diffs.push(format!("port: '{:?}' vs '{:?}'", a.port(), b.port()));
    }
    if a.path() != b.path() {
        diffs.push(format!("path: '{}' vs '{}'", a.path(), b.path()));
    }
    if a.query() != b.query() {
        diffs.push(format!("query: '{}' vs '{}'", a.query(), b.query()));
    }
    if a.fragment() != b.fragment() {
        diffs.push(format!(
            "fragment: '{:?}' vs '{:?}'",
            a.fragment(),
            b.fragment()
        ));
    }
    diffs
}

pub fn security_report(url: &URL) -> Vec<String> {
    let mut report = Vec::new();
    if url.has_credentials() {
        report.push("URL contains embedded plaintext user credentials".to_string());
    }
    if let Some(h) = url.host() {
        if idna_punycode::is_suspicious_domain(h) {
            report.push(format!(
                "Host '{}' contains suspicious homographs or Punycode script mixing",
                h
            ));
        }
    }
    if let Some(ip) = url.ip_address() {
        if ip.is_private() || ip.is_loopback() {
            report.push(format!(
                "Host resolves to private or loopback IP address {}",
                ip.to_canonical_string()
            ));
        }
    }
    report
}
