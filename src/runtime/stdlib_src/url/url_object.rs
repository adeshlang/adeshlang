//! Main URL Object Representation & API Methods for AdeshLang.

use super::idna_punycode;
use super::ipv4_ipv6::IPAddress;
use super::parser::{ParsedURLComponents, default_port_for_scheme, parse_url, resolve_relative};
use super::query::QueryParams;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct URL {
    pub(crate) components: ParsedURLComponents,
}

impl URL {
    pub fn parse(input: &str) -> Result<Self, String> {
        let components = parse_url(input)?;
        Ok(Self { components })
    }

    pub fn parse_relative(input: &str, base: &URL) -> Result<Self, String> {
        let components = resolve_relative(&base.components, input)?;
        Ok(Self { components })
    }

    pub fn scheme(&self) -> &str {
        &self.components.scheme
    }

    pub fn username(&self) -> Option<&str> {
        self.components.username.as_deref()
    }

    pub fn password(&self) -> Option<&str> {
        self.components.password.as_deref()
    }

    pub fn has_credentials(&self) -> bool {
        self.components.username.is_some() || self.components.password.is_some()
    }

    pub fn host(&self) -> Option<&str> {
        self.components.host.as_deref()
    }

    pub fn hostname(&self) -> Option<&str> {
        self.components.host.as_deref()
    }

    pub fn ip_address(&self) -> Option<&IPAddress> {
        self.components.ip_address.as_ref()
    }

    pub fn port(&self) -> Option<u16> {
        self.components.port
    }

    pub fn default_port(&self) -> Option<u16> {
        default_port_for_scheme(self.scheme())
    }

    pub fn effective_port(&self) -> Option<u16> {
        self.port().or_else(|| self.default_port())
    }

    pub fn authority(&self) -> String {
        let mut auth = String::new();
        if let Some(user) = self.username() {
            auth.push_str(user);
            if let Some(pass) = self.password() {
                auth.push(':');
                auth.push_str(pass);
            }
            auth.push('@');
        }

        if let Some(h) = self.host() {
            auth.push_str(h);
        }

        if let Some(p) = self.port() {
            auth.push(':');
            auth.push_str(&p.to_string());
        }

        auth
    }

    pub fn path(&self) -> &str {
        &self.components.path
    }

    pub fn path_segments(&self) -> Vec<String> {
        self.path()
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    pub fn query(&self) -> String {
        self.components.query.to_string()
    }

    pub fn query_params(&self) -> &QueryParams {
        &self.components.query
    }

    pub fn query_params_mut(&mut self) -> &mut QueryParams {
        &mut self.components.query
    }

    pub fn fragment(&self) -> Option<&str> {
        self.components.fragment.as_deref()
    }

    pub fn is_opaque(&self) -> bool {
        self.components.is_opaque
    }

    pub fn origin(&self) -> String {
        if self.is_opaque() || self.host().is_none() {
            return "null".to_string();
        }

        let mut orig = format!("{}://{}", self.scheme(), self.host().unwrap());
        if let Some(p) = self.port() {
            if Some(p) != self.default_port() {
                orig.push(':');
                orig.push_str(&p.to_string());
            }
        }
        orig
    }

    pub fn same_origin(&self, other: &URL) -> bool {
        self.origin() == other.origin() && self.origin() != "null"
    }

    pub fn is_http(&self) -> bool {
        self.scheme() == "http"
    }

    pub fn is_https(&self) -> bool {
        self.scheme() == "https"
    }

    pub fn is_web_socket(&self) -> bool {
        self.scheme() == "ws" || self.scheme() == "wss"
    }

    pub fn is_secure(&self) -> bool {
        self.scheme() == "https" || self.scheme() == "wss"
    }

    pub fn requires_tls(&self) -> bool {
        self.is_secure()
    }

    pub fn resolve(&self, relative_input: &str) -> Result<URL, String> {
        URL::parse_relative(relative_input, self)
    }

    pub fn href(&self) -> String {
        self.to_string()
    }

    pub fn redacted(&self) -> String {
        let mut copy = self.clone();
        if copy.components.password.is_some() {
            copy.components.password = Some("***".to_string());
        }
        copy.to_string()
    }

    pub fn safe_string(&self) -> String {
        self.redacted()
    }

    pub fn strip_credentials(&self) -> URL {
        let mut copy = self.clone();
        copy.components.username = None;
        copy.components.password = None;
        copy
    }

    pub fn strip_fragment(&self) -> URL {
        let mut copy = self.clone();
        copy.components.fragment = None;
        copy
    }

    pub fn strip_tracking_parameters(&self) -> URL {
        let tracking_keys = [
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "utm_term",
            "utm_content",
            "fbclid",
            "gclid",
            "msclkid",
        ];
        let mut copy = self.clone();
        for key in &tracking_keys {
            copy.components.query.remove(key);
        }
        copy
    }

    pub fn canonicalize(&self) -> Result<URL, String> {
        let mut copy = self.clone();
        // Lowercase scheme & host
        copy.components.scheme = copy.components.scheme.to_lowercase();
        if let Some(ref h) = copy.components.host {
            let ascii_host = idna_punycode::domain_to_ascii(h)?;
            copy.components.host = Some(ascii_host);
        }
        // Normalize default port
        if let Some(p) = copy.components.port {
            if Some(p) == copy.default_port() {
                copy.components.port = None;
            }
        }
        // Sort query parameters canonically
        copy.components.query.sort();
        Ok(copy)
    }

    pub fn with_scheme(&self, scheme: &str) -> Result<URL, String> {
        let mut copy = self.clone();
        copy.components.scheme = scheme.to_lowercase();
        Ok(copy)
    }

    pub fn with_host(&self, host: &str) -> Result<URL, String> {
        let mut copy = self.clone();
        copy.components.host = Some(host.to_string());
        copy.components.ip_address = IPAddress::parse(host).ok();
        Ok(copy)
    }

    pub fn with_port(&self, port: Option<u16>) -> URL {
        let mut copy = self.clone();
        copy.components.port = port;
        copy
    }

    pub fn with_path(&self, path: &str) -> URL {
        let mut copy = self.clone();
        copy.components.path = super::parser::normalize_dot_segments(path);
        copy
    }

    pub fn with_fragment(&self, fragment: Option<&str>) -> URL {
        let mut copy = self.clone();
        copy.components.fragment = fragment.map(|s| s.to_string());
        copy
    }

    pub fn with_query_param(&self, key: &str, value: &str) -> URL {
        let mut copy = self.clone();
        copy.components.query.append(key, value);
        copy
    }

    pub fn remove_query_param(&self, key: &str) -> URL {
        let mut copy = self.clone();
        copy.components.query.remove(key);
        copy
    }

    pub fn append_path_segment(&self, segment: &str) -> URL {
        let mut copy = self.clone();
        let mut current = copy.components.path;
        if !current.ends_with('/') && !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        copy.components.path = super::parser::normalize_dot_segments(&current);
        copy
    }

    pub fn edit(&self) -> super::builder::URLBuilder {
        let mut builder = super::builder::URLBuilder::new()
            .scheme(self.scheme())
            .path(self.path());

        if let Some(u) = self.username() {
            builder = builder.username(u);
        }
        if let Some(p) = self.password() {
            builder = builder.password(p);
        }
        if let Some(h) = self.host() {
            builder = builder.host(h);
        }
        if let Some(p) = self.port() {
            builder = builder.port(p);
        }
        if let Some(f) = self.fragment() {
            builder = builder.fragment(f);
        }
        for (k, v) in self.query_params().entries() {
            builder = builder.query_param(k, v);
        }
        builder
    }

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        out.push_str(self.scheme());
        out.push(':');

        if !self.is_opaque() {
            out.push_str("//");
            if let Some(user) = self.username() {
                out.push_str(user);
                if let Some(pass) = self.password() {
                    out.push(':');
                    out.push_str(pass);
                }
                out.push('@');
            }

            if let Some(h) = self.host() {
                out.push_str(h);
            }

            if let Some(p) = self.port() {
                out.push(':');
                out.push_str(&p.to_string());
            }
        }

        out.push_str(self.path());

        let q_str = self.components.query.to_string();
        if !q_str.is_empty() {
            out.push('?');
            out.push_str(&q_str);
        }

        if let Some(ref frag) = self.components.fragment {
            out.push('#');
            out.push_str(frag);
        }

        out
    }
}
