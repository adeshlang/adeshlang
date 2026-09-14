//! Comprehensive Unit Tests for AdeshLang URL Standard Library.

#[cfg(test)]
mod tests {
    use crate::runtime::stdlib_src::url::URL;
    use crate::runtime::stdlib_src::url::idna_punycode;
    use crate::runtime::stdlib_src::url::special_urls;
    use crate::runtime::stdlib_src::url::*;
    use std::collections::HashMap;

    #[test]
    fn test_basic_url_parsing() {
        let url = URL::parse("https://user:pass@example.com:8443/users/10?page=2&limit=50#results")
            .unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.username(), Some("user"));
        assert_eq!(url.password(), Some("pass"));
        assert_eq!(url.host(), Some("example.com"));
        assert_eq!(url.port(), Some(8443));
        assert_eq!(url.path(), "/users/10");
        assert_eq!(url.query_params().get("page"), Some("2"));
        assert_eq!(url.query_params().get("limit"), Some("50"));
        assert_eq!(url.fragment(), Some("results"));
    }

    #[test]
    fn test_credential_redaction() {
        let url = URL::parse("https://admin:secret123@example.com/api").unwrap();
        assert_eq!(url.redacted(), "https://admin:***@example.com/api");
        assert_eq!(
            url.strip_credentials().to_string(),
            "https://example.com/api"
        );
    }

    #[test]
    fn test_ipv4_and_ipv6() {
        let url_v4 = URL::parse("http://127.0.0.1:8080/health").unwrap();
        assert!(url_v4.ip_address().unwrap().is_loopback());

        let url_v6 = URL::parse("http://[::1]:8080/health").unwrap();
        assert!(url_v6.ip_address().unwrap().is_loopback());
    }

    #[test]
    fn test_punycode_idna() {
        let ascii_domain = idna_punycode::domain_to_ascii("münich.example").unwrap();
        assert_eq!(ascii_domain, "xn--mnich-kva.example");

        let unicode_domain = idna_punycode::domain_to_unicode("xn--mnich-kva.example").unwrap();
        assert_eq!(unicode_domain, "münich.example");
    }

    #[test]
    fn test_relative_resolution() {
        let base = URL::parse("https://example.com/docs/index.html").unwrap();
        let resolved = base.resolve("../images/logo.png").unwrap();
        assert_eq!(resolved.to_string(), "https://example.com/images/logo.png");
    }

    #[test]
    fn test_url_builder() {
        let url = URLBuilder::new()
            .scheme("https")
            .host("api.example.com")
            .path("/users")
            .query_param("page", "2")
            .query_param("limit", "20")
            .fragment("active")
            .build()
            .unwrap();

        assert_eq!(
            url.to_string(),
            "https://api.example.com/users?page=2&limit=20#active"
        );
    }

    #[test]
    fn test_url_pattern_matching() {
        let pat = URLPattern::parse("https://example.com/users/:id").unwrap();
        let url = URL::parse("https://example.com/users/42").unwrap();
        let m = pat.match_url(&url).unwrap();
        assert_eq!(m.get("id"), Some("42"));
    }

    #[test]
    fn test_url_template_expansion() {
        let tmpl = URLTemplate::parse("https://api.example.com/users/{id}/posts/{postId}");
        let mut params = HashMap::new();
        params.insert("id".to_string(), "10".to_string());
        params.insert("postId".to_string(), "50".to_string());

        let url = tmpl.expand(&params).unwrap();
        assert_eq!(url.to_string(), "https://api.example.com/users/10/posts/50");
    }

    #[test]
    fn test_security_policy_ssrf() {
        let policy = URLSecurityPolicy::new().deny_private_networks();
        let safe_url = URL::parse("https://example.com/api").unwrap();
        let unsafe_url = URL::parse("http://127.0.0.1/admin").unwrap();

        assert!(policy.validate(&safe_url).is_ok());
        assert!(policy.validate(&unsafe_url).is_err());
    }

    #[test]
    fn test_data_and_file_urls() {
        let data_url = special_urls::DataURL::parse("data:text/plain;base64,SGVsbG8=").unwrap();
        assert_eq!(data_url.decode_string().unwrap(), "Hello");

        let file_url = special_urls::from_file_path("/tmp/test.txt").unwrap();
        assert_eq!(file_url.scheme(), "file");
    }

    #[test]
    fn test_url_view_zero_copy() {
        let input = "https://example.com:8080/path?key=val#frag";
        let view = URLView::parse(input).unwrap();
        assert_eq!(view.scheme(), "https");
        assert_eq!(view.host(), Some("example.com"));
        assert_eq!(view.port_str(), Some("8080"));
        assert_eq!(view.path(), "/path");
        assert_eq!(view.query(), Some("key=val"));
        assert_eq!(view.fragment(), Some("frag"));

        let owned = view.to_owned().unwrap();
        assert_eq!(owned.to_string(), input);
    }
}
