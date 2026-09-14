//! X.509 Certificate and Trust Store management for AdeshLang TLS.

use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use std::sync::Arc;

use super::errors::TlsError;

pub fn generate_self_signed_cert() -> Result<(String, String), TlsError> {
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let cert = rcgen::generate_simple_self_signed(subject_alt_names).map_err(|e| {
        TlsError::CertificateError(format!("Failed to generate self-signed cert: {}", e))
    })?;
    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();
    Ok((cert_pem, key_pem))
}

#[derive(Clone)]
pub struct TlsTrustStore {
    pub root_store: Arc<RootCertStore>,
}

impl Default for TlsTrustStore {
    fn default() -> Self {
        Self::system_and_webpki()
    }
}

impl TlsTrustStore {
    pub fn empty() -> Self {
        Self {
            root_store: Arc::new(RootCertStore::empty()),
        }
    }

    pub fn system_and_webpki() -> Self {
        let mut root_store = RootCertStore::empty();

        // Add webpki roots
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        // Add native system certificates if available
        if let Ok(native_certs) = rustls_native_certs::load_native_certs() {
            for cert in native_certs {
                let _ = root_store.add(cert);
            }
        }

        Self {
            root_store: Arc::new(root_store),
        }
    }

    pub fn add_pem_ca(&mut self, pem_bytes_or_path: &[u8]) -> Result<(), TlsError> {
        let pem_bytes = resolve_pem_bytes(pem_bytes_or_path)?;
        let mut reader = std::io::BufReader::new(pem_bytes.as_slice());
        let certs = certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| TlsError::CertificateError(format!("Failed to parse PEM CA: {}", e)))?;

        let root_store = Arc::make_mut(&mut self.root_store);
        for cert in certs {
            root_store.add(cert).map_err(|e| {
                TlsError::CertificateError(format!(
                    "Failed to add CA certificate to trust store: {}",
                    e
                ))
            })?;
        }

        Ok(())
    }
}

pub fn resolve_pem_bytes(input: &[u8]) -> Result<Vec<u8>, TlsError> {
    if let Ok(s) = std::str::from_utf8(input) {
        let trimmed = s.trim();
        // If it starts with "-----BEGIN", it's raw PEM content
        if trimmed.starts_with("-----BEGIN") {
            return Ok(trimmed.as_bytes().to_vec());
        }
        // Otherwise, test if it's a valid filesystem path
        let path = std::path::Path::new(trimmed);
        if path.exists() && path.is_file() {
            return std::fs::read(path).map_err(|e| {
                TlsError::CertificateError(format!(
                    "Failed to read certificate/key file '{}': {}",
                    trimmed, e
                ))
            });
        }
    }
    Ok(input.to_vec())
}

pub fn load_pem_key_and_chain(
    cert_pem_or_path: &[u8],
    key_pem_or_path: &[u8],
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TlsError> {
    let cert_bytes = resolve_pem_bytes(cert_pem_or_path)?;
    let key_bytes = resolve_pem_bytes(key_pem_or_path)?;

    let mut cert_reader = std::io::BufReader::new(cert_bytes.as_slice());
    let cert_chain = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            TlsError::CertificateError(format!("Failed to parse certificate chain: {}", e))
        })?;

    if cert_chain.is_empty() {
        return Err(TlsError::CertificateError(
            "Certificate chain is empty or invalid".into(),
        ));
    }

    let mut key_reader = std::io::BufReader::new(key_bytes.as_slice());
    if let Ok(Some(key)) = rustls_pemfile::private_key(&mut key_reader) {
        return Ok((cert_chain, key));
    }

    let mut key_reader = std::io::BufReader::new(key_bytes.as_slice());
    let mut pkcs8_keys = pkcs8_private_keys(&mut key_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            TlsError::CertificateError(format!("Failed to parse PKCS8 private key: {}", e))
        })?;

    if let Some(key) = pkcs8_keys.pop() {
        return Ok((cert_chain, PrivateKeyDer::Pkcs8(key)));
    }

    let mut key_reader = std::io::BufReader::new(key_bytes.as_slice());
    let mut rsa_keys = rsa_private_keys(&mut key_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            TlsError::CertificateError(format!("Failed to parse RSA private key: {}", e))
        })?;

    if let Some(key) = rsa_keys.pop() {
        return Ok((cert_chain, PrivateKeyDer::Pkcs1(key)));
    }

    Err(TlsError::CertificateError(
        "No valid private key (EC, PKCS#8, or PKCS#1) found in PEM or key file".into(),
    ))
}
