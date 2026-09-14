//! Tests for AdeshLang TLS standard library.

#[cfg(test)]
mod tests {
    use adeshlang::runtime::stdlib_src::tls::cert::TlsTrustStore;
    use adeshlang::runtime::stdlib_src::tls::policy::{TlsSecurityPolicy, TlsVersion};

    #[test]
    fn test_default_security_policy() {
        let policy = TlsSecurityPolicy::default();
        assert_eq!(policy.min_version, TlsVersion::Tls12);
        assert_eq!(policy.max_version, TlsVersion::Tls13);
        assert!(policy.verify_certificates);
        assert!(policy.verify_hostname);
        assert!(!policy.allow_0rtt);
        assert!(policy.enable_resumption);
        assert!(policy.enforce_system_trust);
    }

    #[test]
    fn test_strict_security_policy() {
        let policy = TlsSecurityPolicy::strict();
        assert_eq!(policy.min_version, TlsVersion::Tls13);
        assert_eq!(policy.max_version, TlsVersion::Tls13);
        assert!(policy.verify_certificates);
        assert!(policy.verify_hostname);
        assert!(!policy.allow_0rtt);
    }

    #[test]
    fn test_insecure_dev_policy() {
        let policy = TlsSecurityPolicy::insecure_dev();
        assert_eq!(policy.min_version, TlsVersion::Tls12);
        assert_eq!(policy.max_version, TlsVersion::Tls13);
        assert!(!policy.verify_certificates);
        assert!(!policy.verify_hostname);
        assert!(!policy.allow_0rtt);
    }

    #[test]
    fn test_protocol_versions_conversion() {
        let default_policy = TlsSecurityPolicy::default();
        let versions = default_policy.to_rustls_versions();
        assert_eq!(versions.len(), 2);

        let strict_policy = TlsSecurityPolicy::strict();
        let strict_versions = strict_policy.to_rustls_versions();
        assert_eq!(strict_versions.len(), 1);
    }

    #[test]
    fn test_trust_store_creation() {
        let store = TlsTrustStore::system_and_webpki();
        assert!(!store.root_store.is_empty());
    }

    #[test]
    fn test_empty_trust_store() {
        let store = TlsTrustStore::empty();
        assert!(store.root_store.is_empty());
    }

    #[test]
    fn test_resolve_pem_from_file_and_memory() {
        use adeshlang::runtime::stdlib_src::tls::cert::resolve_pem_bytes;
        // Test raw PEM memory string
        let pem_str = "-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----";
        let resolved = resolve_pem_bytes(pem_str.as_bytes()).unwrap();
        assert_eq!(resolved, pem_str.as_bytes());

        // Test non-existent file path fallback gracefully
        let raw_bytes = vec![0x30, 0x82, 0x01];
        let res = resolve_pem_bytes(&raw_bytes).unwrap();
        assert_eq!(res, raw_bytes);
    }

    #[test]
    fn test_utf8_strict_decoding() {
        let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
        let res = String::from_utf8(invalid_utf8);
        assert!(res.is_err());
    }

    #[test]
    fn test_structured_tls_errors() {
        use adeshlang::runtime::stdlib_src::tls::errors::TlsError;
        let err = TlsError::CertificateError("Expired certificate".into());
        assert_eq!(
            err.to_string(),
            "TLS Certificate Error: Expired certificate"
        );

        let sni_err = TlsError::SniError("Invalid SNI host".into());
        assert_eq!(sni_err.to_string(), "TLS SNI Error: Invalid SNI host");

        let alpn_err = TlsError::AlpnError("Negotiation mismatch".into());
        assert_eq!(alpn_err.to_string(), "TLS ALPN Error: Negotiation mismatch");
    }

    #[test]
    fn test_tls_module_object_builder() {
        use adeshlang::parsing::ast::Value;
        let mod_obj = adeshlang::runtime::stdlib_src::tls::api::build_tls_module_object();
        if let Value::Object(map) = mod_obj {
            assert!(map.contains_key("connect"));
            assert!(map.contains_key("connectWithOptions"));
            assert!(map.contains_key("bindServer"));
            assert!(map.contains_key("Server"));
            assert!(map.contains_key("wrap"));
            assert!(map.contains_key("write"));
            assert!(map.contains_key("writeBytes"));
            assert!(map.contains_key("writeText"));
            assert!(map.contains_key("read"));
            assert!(map.contains_key("readBytes"));
            assert!(map.contains_key("readText"));
            assert!(map.contains_key("version"));
            assert!(map.contains_key("alpn"));
            assert!(map.contains_key("peerCertificates"));
            assert!(map.contains_key("trace"));
            assert!(map.contains_key("close"));
            assert!(map.contains_key("SecurityPolicy"));
            assert!(map.contains_key("TrustStore"));
        } else {
            panic!("Expected Value::Object for TLS module");
        }
    }
}
