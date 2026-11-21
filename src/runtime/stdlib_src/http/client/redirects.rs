use super::super::errors::{HttpError, HttpErrorKind};
use super::super::method::HttpMethod;
use super::super::request::Request;
use super::super::response::Response;
use super::super::uri::Uri;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectPolicy {
    None,
    Follow(usize),
    Strict(usize),
}

impl Default for RedirectPolicy {
    fn default() -> Self {
        RedirectPolicy::Follow(10)
    }
}

pub struct RedirectEngine {
    pub policy: RedirectPolicy,
}

impl RedirectEngine {
    pub fn new(policy: RedirectPolicy) -> Self {
        Self { policy }
    }

    pub fn should_redirect(
        &self,
        req: &Request,
        resp: &Response,
        redirect_count: usize,
    ) -> Result<Option<Request>, HttpError> {
        let max_redirects = match self.policy {
            RedirectPolicy::None => return Ok(None),
            RedirectPolicy::Follow(max) => max,
            RedirectPolicy::Strict(max) => max,
        };

        if !resp.status.is_redirection() {
            return Ok(None);
        }

        if redirect_count >= max_redirects {
            return Err(HttpError::new(
                HttpErrorKind::RedirectError,
                format!("Exceeded maximum redirect limit ({})", max_redirects),
            ));
        }

        let location = match resp.headers.get("location") {
            Some(loc) => loc.trim(),
            None => return Ok(None),
        };

        let new_uri = if location.starts_with("http://") || location.starts_with("https://") {
            Uri::parse(location)?
        } else if location.starts_with("//") {
            // Scheme-relative redirect (e.g., "//example.com/path")
            let scheme = req.uri.scheme.as_deref().unwrap_or("http");
            Uri::parse(&format!("{}:{}", scheme, location))?
        } else if location.contains("://") {
            // Reject non-HTTP(S) schemes such as file://, gopher://, ftp://
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!("Unsupported redirect URL scheme in '{}'", location),
            ));
        } else {
            // Relative URI resolution
            let mut resolved = req.uri.clone();
            resolved.path = if location.starts_with('/') {
                location.to_string()
            } else {
                format!("{}/{}", req.uri.path.trim_end_matches('/'), location)
            };
            resolved
        };

        // Validate redirect target host against SSRF rules (post-DNS check also runs on connect)
        if let Some(target_host) = new_uri.host.as_deref() {
            super::dns_racing::validate_ssrf_host(target_host)?;
        }

        // HTTPS -> HTTP downgrade prevention (RFC 9110 Section 15.4)
        if req.uri.is_https() && !new_uri.is_https() {
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                "Insecure redirect downgrade from HTTPS to HTTP blocked",
            ));
        }

        // Method transformation for 301, 302, 303 (convert POST to GET per standard practice)
        let (new_method, new_body) = match resp.status.code() {
            303 => (HttpMethod::Get, super::super::body::Body::Empty),
            301 | 302 if req.method == HttpMethod::Post => {
                (HttpMethod::Get, super::super::body::Body::Empty)
            }
            _ => (req.method.clone(), req.body.clone()),
        };

        let mut new_req = Request::new(new_method, new_uri);
        new_req.body = new_body;

        // Copy safe headers; strip Authorization and sensitive headers across origin changes
        let same_origin = req.uri.host == new_req.uri.host && req.uri.port == new_req.uri.port;
        for (k, vals) in req.headers.iter() {
            if !same_origin && (k == "authorization" || k == "cookie" || k == "proxy-authorization")
            {
                continue;
            }
            if k != "host" && k != "content-length" {
                for v in vals {
                    let _ = new_req.headers.append(k, v);
                }
            }
        }

        Ok(Some(new_req))
    }
}
