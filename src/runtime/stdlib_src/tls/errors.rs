//! Structured TLS errors for AdeshLang.

use std::fmt;

#[derive(Debug, Clone)]
pub enum TlsError {
    HandshakeError(String),
    CertificateError(String),
    HostnameMismatch(String),
    UnknownCA(String),
    ExpiredCertificate(String),
    RevokedCertificate(String),
    InvalidCertificate(String),
    ProtocolError(String),
    RecordError(String),
    DecryptionError(String),
    AlpnError(String),
    SniError(String),
    VersionError(String),
    CipherError(String),
    Timeout(String),
    TransportError(String),
    ClientAuthError(String),
    PolicyViolation(String),
    UnsupportedFeature(String),
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlsError::HandshakeError(msg) => write!(f, "TLS Handshake Error: {}", msg),
            TlsError::CertificateError(msg) => write!(f, "TLS Certificate Error: {}", msg),
            TlsError::HostnameMismatch(msg) => write!(f, "TLS Hostname Mismatch: {}", msg),
            TlsError::UnknownCA(msg) => write!(f, "TLS Unknown CA: {}", msg),
            TlsError::ExpiredCertificate(msg) => write!(f, "TLS Expired Certificate: {}", msg),
            TlsError::RevokedCertificate(msg) => write!(f, "TLS Revoked Certificate: {}", msg),
            TlsError::InvalidCertificate(msg) => write!(f, "TLS Invalid Certificate: {}", msg),
            TlsError::ProtocolError(msg) => write!(f, "TLS Protocol Error: {}", msg),
            TlsError::RecordError(msg) => write!(f, "TLS Record Error: {}", msg),
            TlsError::DecryptionError(msg) => write!(f, "TLS Decryption Error: {}", msg),
            TlsError::AlpnError(msg) => write!(f, "TLS ALPN Error: {}", msg),
            TlsError::SniError(msg) => write!(f, "TLS SNI Error: {}", msg),
            TlsError::VersionError(msg) => write!(f, "TLS Version Error: {}", msg),
            TlsError::CipherError(msg) => write!(f, "TLS Cipher Error: {}", msg),
            TlsError::Timeout(msg) => write!(f, "TLS Timeout: {}", msg),
            TlsError::TransportError(msg) => write!(f, "TLS Transport Error: {}", msg),
            TlsError::ClientAuthError(msg) => write!(f, "TLS Client Auth Error: {}", msg),
            TlsError::PolicyViolation(msg) => write!(f, "TLS Policy Violation: {}", msg),
            TlsError::UnsupportedFeature(msg) => write!(f, "TLS Unsupported Feature: {}", msg),
        }
    }
}

impl std::error::Error for TlsError {}
