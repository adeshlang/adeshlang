//! Fluent URL Builder for AdeshLang URL.

use super::query::QueryParams;
use super::url_object::URL;

#[derive(Debug, Clone, Default)]
pub struct URLBuilder {
    scheme: Option<String>,
    username: Option<String>,
    password: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    path: Option<String>,
    path_segments: Vec<String>,
    query: QueryParams,
    fragment: Option<String>,
}

impl URLBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scheme(mut self, scheme: &str) -> Self {
        self.scheme = Some(scheme.to_lowercase());
        self
    }

    pub fn username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }

    pub fn password(mut self, password: &str) -> Self {
        self.password = Some(password.to_string());
        self
    }

    pub fn host(mut self, host: &str) -> Self {
        self.host = Some(host.to_string());
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    pub fn append_path_segment(mut self, segment: &str) -> Self {
        self.path_segments.push(segment.to_string());
        self
    }

    pub fn query_param(mut self, key: &str, value: &str) -> Self {
        self.query.append(key, value);
        self
    }

    pub fn fragment(mut self, fragment: &str) -> Self {
        self.fragment = Some(fragment.to_string());
        self
    }

    pub fn build(self) -> Result<URL, String> {
        let scheme = self
            .scheme
            .ok_or_else(|| "URLBuilder requires a scheme (e.g. 'https')".to_string())?;

        let is_opaque = scheme == "data" || scheme == "mailto" || scheme == "urn";

        let mut final_path = self.path.unwrap_or_default();
        if !self.path_segments.is_empty() {
            if !final_path.ends_with('/') && !final_path.is_empty() {
                final_path.push('/');
            }
            final_path.push_str(&self.path_segments.join("/"));
        }

        if final_path.is_empty() && !is_opaque {
            final_path = "/".to_string();
        }

        let mut url_str = String::new();
        url_str.push_str(&scheme);
        url_str.push(':');

        if !is_opaque {
            url_str.push_str("//");
            if let Some(ref u) = self.username {
                url_str.push_str(u);
                if let Some(ref p) = self.password {
                    url_str.push(':');
                    url_str.push_str(p);
                }
                url_str.push('@');
            }

            if let Some(ref h) = self.host {
                url_str.push_str(h);
            } else if !is_opaque {
                return Err("URLBuilder requires a host for hierarchical schemes".to_string());
            }

            if let Some(p) = self.port {
                url_str.push(':');
                url_str.push_str(&p.to_string());
            }
        }

        url_str.push_str(&final_path);

        let q_str = self.query.to_string();
        if !q_str.is_empty() {
            url_str.push('?');
            url_str.push_str(&q_str);
        }

        if let Some(ref frag) = self.fragment {
            url_str.push('#');
            url_str.push_str(frag);
        }

        URL::parse(&url_str)
    }
}
