use super::errors::{HttpError, HttpErrorKind};
use super::method::HttpMethod;
use super::request::Request;

#[derive(Debug, Clone)]
pub struct HttpPolicy {
    pub require_tls: bool,
    pub allow_trace: bool,
    pub max_body_bytes: Option<usize>,
    pub allowed_methods: Option<Vec<HttpMethod>>,
    pub required_headers: Vec<String>,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            require_tls: false,
            allow_trace: false,
            max_body_bytes: Some(32 * 1024 * 1024),
            allowed_methods: None,
            required_headers: Vec::new(),
        }
    }
}

impl HttpPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enforce_tls(mut self, require: bool) -> Self {
        self.require_tls = require;
        self
    }

    pub fn max_body(mut self, bytes: usize) -> Self {
        self.max_body_bytes = Some(bytes);
        self
    }

    pub fn allow_methods(mut self, methods: Vec<HttpMethod>) -> Self {
        self.allowed_methods = Some(methods);
        self
    }

    pub fn validate_request(&self, req: &Request) -> Result<(), HttpError> {
        if self.require_tls && !req.uri.is_https() {
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                "Policy violation: TLS required for request",
            ));
        }

        if !self.allow_trace && req.method == HttpMethod::Trace {
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                "Policy violation: TRACE method disabled by security policy",
            ));
        }

        if let Some(ref allowed) = self.allowed_methods {
            if !allowed.contains(&req.method) {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!("Policy violation: method {} not permitted", req.method),
                ));
            }
        }

        if let Some(max_b) = self.max_body_bytes {
            if let Some(len) = req.body.len() {
                if len > max_b {
                    return Err(HttpError::new(
                        HttpErrorKind::BodyTooLarge,
                        format!(
                            "Policy violation: request body size ({} bytes) exceeds limit ({} bytes)",
                            len, max_b
                        ),
                    ));
                }
            }
        }

        Ok(())
    }
}
