//! PEM, DER parsing, X.509 Certificate inspection, and OS trust store validation.

use sha2::Digest;
use x509_parser::prelude::*;

pub struct ParsedCertificate {
    pub subject: String,
    pub issuer: String,
    pub serial_hex: String,
    pub not_before: String,
    pub not_after: String,
    pub fingerprint_sha256: String,
    pub is_ca: bool,
    pub san_dns_names: Vec<String>,
}

pub fn parse_x509_pem(pem_str: &str) -> Result<ParsedCertificate, String> {
    let pem_obj = ::pem::parse(pem_str).map_err(|e| format!("PEM parse error: {}", e))?;
    if pem_obj.tag() != "CERTIFICATE" {
        return Err(format!(
            "Expected CERTIFICATE tag in PEM, got '{}'",
            pem_obj.tag()
        ));
    }
    parse_x509_der(pem_obj.contents())
}

pub fn parse_x509_der(der_bytes: &[u8]) -> Result<ParsedCertificate, String> {
    let (_, cert) = X509Certificate::from_der(der_bytes)
        .map_err(|e| format!("X.509 DER parse error: {}", e))?;

    let subject = cert.subject().to_string();
    let issuer = cert.issuer().to_string();
    let serial_hex = cert.raw_serial_as_string();
    let not_before = cert.validity().not_before.to_string();
    let not_after = cert.validity().not_after.to_string();

    let digest = sha2::Sha256::digest(der_bytes);
    let fingerprint_sha256 = hex::encode(digest);

    let mut is_ca = false;
    let mut san_dns_names = Vec::new();

    if let Ok(Some(basic_constraints)) = cert.basic_constraints() {
        is_ca = basic_constraints.value.ca;
    }

    if let Ok(Some(san)) = cert.subject_alternative_name() {
        for name in &san.value.general_names {
            if let GeneralName::DNSName(dns) = name {
                san_dns_names.push(dns.to_string());
            }
        }
    }

    Ok(ParsedCertificate {
        subject,
        issuer,
        serial_hex,
        not_before,
        not_after,
        fingerprint_sha256,
        is_ca,
        san_dns_names,
    })
}

pub fn encode_to_pem(tag: &str, contents: &[u8]) -> String {
    let pem_obj = ::pem::Pem::new(tag, contents);
    ::pem::encode(&pem_obj)
}

pub fn decode_from_pem(pem_str: &str) -> Result<(String, Vec<u8>), String> {
    let pem_obj = ::pem::parse(pem_str).map_err(|e| format!("PEM decode failed: {}", e))?;
    Ok((pem_obj.tag().to_string(), pem_obj.contents().to_vec()))
}
