use super::errors::{HttpError, HttpErrorKind};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
    Query,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodProperties {
    pub safe: bool,
    pub idempotent: bool,
    pub cacheable: bool,
    pub body_allowed: bool,
}

impl MethodProperties {
    pub const fn new(safe: bool, idempotent: bool, cacheable: bool, body_allowed: bool) -> Self {
        Self {
            safe,
            idempotent,
            cacheable,
            body_allowed,
        }
    }

    pub fn for_standard(method: &HttpMethod) -> Self {
        match method {
            HttpMethod::Get => MethodProperties::new(true, true, true, true),
            HttpMethod::Head => MethodProperties::new(true, true, true, false),
            HttpMethod::Post => MethodProperties::new(false, false, true, true),
            HttpMethod::Put => MethodProperties::new(false, true, false, true),
            HttpMethod::Delete => MethodProperties::new(false, true, false, true),
            HttpMethod::Connect => MethodProperties::new(false, false, false, true),
            HttpMethod::Options => MethodProperties::new(true, true, false, true),
            HttpMethod::Trace => MethodProperties::new(true, true, false, false),
            HttpMethod::Patch => MethodProperties::new(false, false, false, true),
            HttpMethod::Query => MethodProperties::new(true, true, false, true),
            HttpMethod::Custom(_) => MethodProperties::new(false, false, false, true),
        }
    }
}

#[derive(Clone, Default)]
pub struct HttpMethodRegistry {
    methods: Arc<RwLock<HashMap<String, MethodProperties>>>,
}

static GLOBAL_METHOD_REGISTRY: Lazy<HttpMethodRegistry> = Lazy::new(HttpMethodRegistry::new);

impl HttpMethodRegistry {
    pub fn new() -> Self {
        let registry = Self {
            methods: Arc::new(RwLock::new(HashMap::new())),
        };
        registry.register_builtin_defaults();
        registry
    }

    pub fn global() -> &'static HttpMethodRegistry {
        &GLOBAL_METHOD_REGISTRY
    }

    pub fn register(
        &self,
        name: &str,
        safe: bool,
        idempotent: bool,
        cacheable: bool,
        body_allowed: bool,
    ) -> Result<(), HttpError> {
        let normalized = normalize_method_name(name)?;
        let parsed = HttpMethod::parse(&normalized)?;

        if parsed.is_standard() {
            let expected = MethodProperties::for_standard(&parsed);
            let provided = MethodProperties::new(safe, idempotent, cacheable, body_allowed);
            if expected != provided {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!(
                        "Cannot override standard method semantics for {}",
                        parsed.as_str()
                    ),
                ));
            }
        }

        let mut lock = self.methods.write().map_err(|_| {
            HttpError::new(
                HttpErrorKind::ProtocolError,
                "Method registry lock poisoned",
            )
        })?;
        lock.insert(
            normalized,
            MethodProperties::new(safe, idempotent, cacheable, body_allowed),
        );
        Ok(())
    }

    pub fn lookup(&self, name: &str) -> Option<MethodProperties> {
        let normalized = normalize_method_name(name).ok()?;
        let lock = self.methods.read().ok()?;
        lock.get(&normalized).copied()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    fn register_builtin_defaults(&self) {
        let defaults = [
            HttpMethod::Get,
            HttpMethod::Head,
            HttpMethod::Post,
            HttpMethod::Put,
            HttpMethod::Delete,
            HttpMethod::Connect,
            HttpMethod::Options,
            HttpMethod::Trace,
            HttpMethod::Patch,
            HttpMethod::Query,
        ];
        if let Ok(mut lock) = self.methods.write() {
            for method in defaults {
                lock.insert(
                    method.as_str().to_string(),
                    MethodProperties::for_standard(&method),
                );
            }
        }
    }
}

impl HttpMethod {
    pub fn parse(s: &str) -> Result<Self, HttpError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(HttpError::new(
                HttpErrorKind::InvalidMethod,
                "Method cannot be empty",
            ));
        }
        if trimmed.len() > 64 {
            return Err(HttpError::new(
                HttpErrorKind::InvalidMethod,
                "Method exceeds maximum length of 64",
            ));
        }

        // Validate token syntax according to RFC 9110 Section 5.6.2
        for b in trimmed.bytes() {
            if !is_tchar(b) {
                return Err(HttpError::new(
                    HttpErrorKind::InvalidMethod,
                    format!("Invalid character in HTTP method: {:?}", b as char),
                ));
            }
        }

        match trimmed.to_ascii_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "HEAD" => Ok(HttpMethod::Head),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "DELETE" => Ok(HttpMethod::Delete),
            "CONNECT" => Ok(HttpMethod::Connect),
            "OPTIONS" => Ok(HttpMethod::Options),
            "TRACE" => Ok(HttpMethod::Trace),
            "PATCH" => Ok(HttpMethod::Patch),
            "QUERY" => Ok(HttpMethod::Query),
            other => Ok(HttpMethod::Custom(other.to_string())),
        }
    }

    pub fn custom(name: &str) -> Result<Self, HttpError> {
        Self::parse(name)
    }

    pub fn as_str(&self) -> &str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Head => "HEAD",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Connect => "CONNECT",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Trace => "TRACE",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Query => "QUERY",
            HttpMethod::Custom(s) => s.as_str(),
        }
    }

    pub fn is_safe(&self) -> bool {
        self.properties().safe
    }

    pub fn is_idempotent(&self) -> bool {
        self.properties().idempotent
    }

    pub fn is_cacheable(&self) -> bool {
        self.properties().cacheable
    }

    pub fn requires_body(&self) -> bool {
        false
    }

    pub fn allows_body(&self) -> bool {
        self.properties().body_allowed
    }

    pub fn is_standard(&self) -> bool {
        !matches!(self, HttpMethod::Custom(_))
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, HttpMethod::Custom(_))
    }

    pub fn properties(&self) -> MethodProperties {
        match self {
            HttpMethod::Custom(name) => HttpMethodRegistry::global()
                .lookup(name)
                .unwrap_or_else(|| MethodProperties::for_standard(self)),
            _ => MethodProperties::for_standard(self),
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[inline]
pub fn is_tchar(b: u8) -> bool {
    matches!(
        b,
        b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    )
}

fn normalize_method_name(name: &str) -> Result<String, HttpError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::InvalidMethod,
            "Method cannot be empty",
        ));
    }
    if trimmed.len() > 64 {
        return Err(HttpError::new(
            HttpErrorKind::InvalidMethod,
            "Method exceeds maximum length of 64",
        ));
    }
    for b in trimmed.bytes() {
        if !is_tchar(b) {
            return Err(HttpError::new(
                HttpErrorKind::InvalidMethod,
                format!("Invalid character in HTTP method: {:?}", b as char),
            ));
        }
    }
    Ok(trimmed.to_ascii_uppercase())
}
