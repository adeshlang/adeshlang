use super::body::Body;
use super::digest::{
    ContentDigest, DigestAlgorithm, DigestPreference, apply_content_digest_header,
};
use super::errors::HttpError;
use super::headers::Headers;
use super::method::HttpMethod;
use super::priority::Priority;
use super::uri::Uri;
use super::version::HttpVersion;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Request {
    pub method: HttpMethod,
    pub uri: Uri,
    pub version: HttpVersion,
    pub headers: Headers,
    pub body: Body,
    pub timeout: Option<Duration>,
    pub extensions: HashMap<String, String>,
}

impl Request {
    pub fn new(method: HttpMethod, uri: Uri) -> Self {
        let mut headers = Headers::new();
        if let Some(ref host) = uri.host {
            let host_hdr = if let Some(port) = uri.port {
                if port == uri.default_port() {
                    host.clone()
                } else {
                    format!("{}:{}", host, port)
                }
            } else {
                host.clone()
            };
            let _ = headers.insert("host", &host_hdr);
        }

        Self {
            method,
            uri,
            version: HttpVersion::Http11,
            headers,
            body: Body::Empty,
            timeout: None,
            extensions: HashMap::new(),
        }
    }

    pub fn get(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Get, parsed_uri))
    }

    pub fn post(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Post, parsed_uri))
    }

    pub fn put(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Put, parsed_uri))
    }

    pub fn query(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Query, parsed_uri))
    }

    pub fn query_with_body(
        uri: impl AsRef<str>,
        body: impl Into<String>,
    ) -> Result<Self, HttpError> {
        let mut req = Self::query(uri)?;
        let body_str = body.into();
        req.body = Body::from_string(&body_str);
        if !body_str.is_empty() {
            let _ = req.headers.set_content_length(body_str.len() as u64);
        }
        Ok(req)
    }

    pub fn query_json(uri: impl AsRef<str>, value: &serde_json::Value) -> Result<Self, HttpError> {
        let mut req = Self::query(uri)?;
        req.headers.set_content_type("application/json")?;
        let body = Body::from_json(value)?;
        Ok(req.with_body(body))
    }

    pub fn query_stream<F>(uri: impl AsRef<str>, stream: F) -> Result<Self, HttpError>
    where
        F: Fn() -> Option<Result<Vec<u8>, HttpError>> + Send + Sync + 'static,
    {
        let mut req = Self::query(uri)?;
        req.body = Body::from_stream(stream);
        Ok(req)
    }

    pub fn delete(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Delete, parsed_uri))
    }

    pub fn patch(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Patch, parsed_uri))
    }

    pub fn head(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Head, parsed_uri))
    }

    pub fn options(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Options, parsed_uri))
    }

    pub fn trace(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Trace, parsed_uri))
    }

    pub fn connect(uri: impl AsRef<str>) -> Result<Self, HttpError> {
        let parsed_uri = Uri::parse(uri.as_ref())?;
        Ok(Self::new(HttpMethod::Connect, parsed_uri))
    }

    pub fn header(mut self, key: &str, value: &str) -> Result<Self, HttpError> {
        self.headers.insert(key, value)?;
        Ok(self)
    }

    pub fn with_body(mut self, body: Body) -> Self {
        if let Some(len) = body.len() {
            if self.headers.get("content-length").is_none() && !body.is_empty() {
                let _ = self.headers.set_content_length(len as u64);
            }
        }
        self.body = body;
        self
    }

    pub fn with_json(mut self, val: &serde_json::Value) -> Result<Self, HttpError> {
        self.headers.set_content_type("application/json")?;
        let body = Body::from_json(val)?;
        Ok(self.with_body(body))
    }

    pub fn with_timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }

    pub fn priority(mut self, priority: Priority) -> Result<Self, HttpError> {
        self.headers.insert("priority", &priority.encode())?;
        Ok(self)
    }

    pub fn set_priority(&mut self, priority: Priority) -> Result<(), HttpError> {
        self.headers.insert("priority", &priority.encode())
    }

    pub fn priority_value(&self) -> Result<Option<Priority>, HttpError> {
        if let Some(raw) = self.headers.get("priority") {
            return Ok(Some(Priority::parse(raw)?));
        }
        Ok(None)
    }

    pub fn content_digest(mut self, algorithm: DigestAlgorithm) -> Result<Self, HttpError> {
        apply_content_digest_header(&mut self, algorithm)?;
        Ok(self)
    }

    pub fn set_content_digest(&mut self, algorithm: DigestAlgorithm) -> Result<(), HttpError> {
        apply_content_digest_header(self, algorithm)
    }

    pub fn content_digest_value(&self) -> Result<Option<ContentDigest>, HttpError> {
        if let Some(raw) = self.headers.get("content-digest") {
            return Ok(Some(ContentDigest::parse(raw)?));
        }
        Ok(None)
    }

    pub fn want_content_digest(mut self, pref: DigestPreference) -> Result<Self, HttpError> {
        self.headers.insert("want-content-digest", &pref.encode())?;
        Ok(self)
    }

    pub fn want_repr_digest(mut self, pref: DigestPreference) -> Result<Self, HttpError> {
        self.headers.insert("want-repr-digest", &pref.encode())?;
        Ok(self)
    }

    pub fn want_content_digest_value(&self) -> Result<Option<DigestPreference>, HttpError> {
        if let Some(raw) = self.headers.get("want-content-digest") {
            return Ok(Some(DigestPreference::parse(raw)?));
        }
        Ok(None)
    }

    pub fn want_repr_digest_value(&self) -> Result<Option<DigestPreference>, HttpError> {
        if let Some(raw) = self.headers.get("want-repr-digest") {
            return Ok(Some(DigestPreference::parse(raw)?));
        }
        Ok(None)
    }
}
