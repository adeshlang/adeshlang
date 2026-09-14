//! Advanced Security policies and builder options for AdeshLang TLS.

use rustls::SupportedProtocolVersion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsVersion {
    Tls12,
    Tls13,
}

#[derive(Debug, Clone)]
pub struct TlsSecurityPolicy {
    pub min_version: TlsVersion,
    pub max_version: TlsVersion,
    pub verify_certificates: bool,
    pub verify_hostname: bool,
    pub allow_0rtt: bool,
    pub alpn_protocols: Vec<Vec<u8>>,
    pub enforce_system_trust: bool,
    pub custom_ca_pems: Vec<Vec<u8>>,
    pub spki_pins: Vec<String>,
    pub enable_resumption: bool,
}

impl Default for TlsSecurityPolicy {
    fn default() -> Self {
        Self {
            min_version: TlsVersion::Tls12,
            max_version: TlsVersion::Tls13,
            verify_certificates: true,
            verify_hostname: true,
            allow_0rtt: false,
            alpn_protocols: Vec::new(),
            enforce_system_trust: true,
            custom_ca_pems: Vec::new(),
            spki_pins: Vec::new(),
            enable_resumption: true,
        }
    }
}

impl TlsSecurityPolicy {
    pub fn strict() -> Self {
        Self {
            min_version: TlsVersion::Tls13,
            max_version: TlsVersion::Tls13,
            verify_certificates: true,
            verify_hostname: true,
            allow_0rtt: false,
            alpn_protocols: Vec::new(),
            enforce_system_trust: true,
            custom_ca_pems: Vec::new(),
            spki_pins: Vec::new(),
            enable_resumption: true,
        }
    }

    pub fn insecure_dev() -> Self {
        Self {
            min_version: TlsVersion::Tls12,
            max_version: TlsVersion::Tls13,
            verify_certificates: false,
            verify_hostname: false,
            allow_0rtt: false,
            alpn_protocols: Vec::new(),
            enforce_system_trust: false,
            custom_ca_pems: Vec::new(),
            spki_pins: Vec::new(),
            enable_resumption: false,
        }
    }

    pub fn to_rustls_versions(&self) -> Vec<&'static SupportedProtocolVersion> {
        let mut versions = Vec::new();
        if self.max_version == TlsVersion::Tls13 {
            versions.push(&rustls::version::TLS13);
        }
        if self.min_version == TlsVersion::Tls12 {
            versions.push(&rustls::version::TLS12);
        }
        versions
    }
}
