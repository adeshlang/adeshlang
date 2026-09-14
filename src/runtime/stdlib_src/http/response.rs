use super::body::Body;
use super::digest::{
    ContentDigest, DigestAlgorithm, DigestPreference, RepresentationDigest,
    apply_content_digest_trailer, apply_repr_digest_header, verify_content_digest,
    verify_representation_digest, wrap_response_stream_for_digest_verification,
};
use super::errors::HttpError;
use super::headers::Headers;
use super::priority::Priority;
use super::problem::ProblemDetails;
use super::status::HttpStatus;
use super::version::HttpVersion;

#[derive(Debug, Clone)]
pub struct Response {
    pub status: HttpStatus,
    pub version: HttpVersion,
    pub headers: Headers,
    pub body: Body,
    pub trailers: Option<Headers>,
}

impl Response {
    pub fn new(status: HttpStatus) -> Self {
        Self {
            status,
            version: HttpVersion::Http11,
            headers: Headers::new(),
            body: Body::Empty,
            trailers: None,
        }
    }

    pub fn ok() -> Self {
        Self::new(HttpStatus::OK)
    }

    pub fn created() -> Self {
        Self::new(HttpStatus::CREATED)
    }

    pub fn no_content() -> Self {
        Self::new(HttpStatus::NO_CONTENT)
    }

    pub fn not_found() -> Self {
        Self::new(HttpStatus::NOT_FOUND)
    }

    pub fn bad_request() -> Self {
        Self::new(HttpStatus::BAD_REQUEST)
    }

    pub fn server_error() -> Self {
        Self::new(HttpStatus::INTERNAL_SERVER_ERROR)
    }

    pub fn redirect(location: &str, permanent: bool) -> Result<Self, HttpError> {
        let status = if permanent {
            HttpStatus::PERMANENT_REDIRECT
        } else {
            HttpStatus::TEMPORARY_REDIRECT
        };
        let mut resp = Self::new(status);
        resp.headers.insert("location", location)?;
        Ok(resp)
    }

    pub fn text(content: impl Into<String>) -> Result<Self, HttpError> {
        let s = content.into();
        let mut resp = Self::ok();
        resp.headers.set_content_type("text/plain; charset=utf-8")?;
        resp.headers.set_content_length(s.len() as u64)?;
        resp.body = Body::from_string(s);
        Ok(resp)
    }

    pub fn html(content: impl Into<String>) -> Result<Self, HttpError> {
        let s = content.into();
        let mut resp = Self::ok();
        resp.headers.set_content_type("text/html; charset=utf-8")?;
        resp.headers.set_content_length(s.len() as u64)?;
        resp.body = Body::from_string(s);
        Ok(resp)
    }

    pub fn json(val: &serde_json::Value) -> Result<Self, HttpError> {
        let body = Body::from_json(val)?;
        let mut resp = Self::ok();
        resp.headers.set_content_type("application/json")?;
        if let Some(len) = body.len() {
            resp.headers.set_content_length(len as u64)?;
        }
        resp.body = body;
        Ok(resp)
    }

    pub fn problem(prob: &ProblemDetails) -> Result<Self, HttpError> {
        let mut resp = Self::new(HttpStatus(prob.status));
        resp.headers.set_content_type("application/problem+json")?;
        let json_str = prob.to_json();
        resp.headers.set_content_length(json_str.len() as u64)?;
        resp.body = Body::from_string(json_str);
        Ok(resp)
    }

    pub fn header(mut self, name: &str, value: &str) -> Result<Self, HttpError> {
        self.headers.insert(name, value)?;
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

    pub fn content_digest_value(&self) -> Result<Option<ContentDigest>, HttpError> {
        if let Some(raw) = self.headers.get("content-digest") {
            return Ok(Some(ContentDigest::parse(raw)?));
        }
        if let Some(trailers) = &self.trailers {
            if let Some(raw) = trailers.get("content-digest") {
                return Ok(Some(ContentDigest::parse(raw)?));
            }
        }
        Ok(None)
    }

    pub fn repr_digest_value(&self) -> Result<Option<RepresentationDigest>, HttpError> {
        if let Some(raw) = self.headers.get("repr-digest") {
            return Ok(Some(RepresentationDigest::parse(raw)?));
        }
        if let Some(trailers) = &self.trailers {
            if let Some(raw) = trailers.get("repr-digest") {
                return Ok(Some(RepresentationDigest::parse(raw)?));
            }
        }
        Ok(None)
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

    pub fn repr_digest(mut self, algorithm: DigestAlgorithm) -> Result<Self, HttpError> {
        apply_repr_digest_header(&mut self, algorithm)?;
        Ok(self)
    }

    pub fn set_repr_digest(&mut self, algorithm: DigestAlgorithm) -> Result<(), HttpError> {
        apply_repr_digest_header(self, algorithm)
    }

    pub fn content_digest_trailer(mut self, algorithm: DigestAlgorithm) -> Result<Self, HttpError> {
        apply_content_digest_trailer(&mut self, algorithm)?;
        Ok(self)
    }

    pub fn set_content_digest_trailer(
        &mut self,
        algorithm: DigestAlgorithm,
    ) -> Result<(), HttpError> {
        apply_content_digest_trailer(self, algorithm)
    }

    pub fn verify_content_digest(&self, algorithm: DigestAlgorithm) -> Result<(), HttpError> {
        verify_content_digest(self, algorithm)
    }

    pub fn verify_representation_digest(
        &self,
        algorithm: DigestAlgorithm,
    ) -> Result<(), HttpError> {
        verify_representation_digest(self, algorithm)
    }

    pub fn verify_digest_streaming(
        mut self,
        algorithm: DigestAlgorithm,
    ) -> Result<Self, HttpError> {
        wrap_response_stream_for_digest_verification(&mut self, algorithm)?;
        Ok(self)
    }
}
