use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpErrorKind {
    InvalidUri,
    InvalidMethod,
    InvalidHeader,
    HeaderTooLarge,
    BodyTooLarge,
    Timeout,
    Cancelled,
    DnsError,
    ConnectError,
    TlsError,
    Http1Error,
    Http2Error,
    Http3Error,
    QuicError,
    ProtocolError,
    RedirectError,
    ProxyError,
    CacheError,
    CompressionError,
    MultipartError,
    AuthenticationError,
    IoError,
    SecurityViolation,
    RateLimited,
    CircuitBroken,
    ParseError,
}

#[derive(Debug, Clone)]
pub struct HttpError {
    pub kind: HttpErrorKind,
    pub message: String,
    pub operation: Option<String>,
    pub url: Option<String>,
    pub method: Option<String>,
    pub protocol: Option<String>,
    pub status_code: Option<u16>,
    pub stream_id: Option<u32>,
    pub retryable: bool,
}

impl HttpError {
    pub fn new(kind: HttpErrorKind, message: impl Into<String>) -> Self {
        let msg = message.into();
        let retryable = matches!(
            kind,
            HttpErrorKind::Timeout
                | HttpErrorKind::ConnectError
                | HttpErrorKind::DnsError
                | HttpErrorKind::RateLimited
        );
        Self {
            kind,
            message: msg,
            operation: None,
            url: None,
            method: None,
            protocol: None,
            status_code: None,
            stream_id: None,
            retryable,
        }
    }

    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    pub fn with_protocol(mut self, proto: impl Into<String>) -> Self {
        self.protocol = Some(proto.into());
        self
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status_code = Some(status);
        self
    }

    pub fn with_stream_id(mut self, stream_id: u32) -> Self {
        self.stream_id = Some(stream_id);
        self
    }

    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn is_timeout(&self) -> bool {
        self.kind == HttpErrorKind::Timeout
    }

    pub fn is_cancelled(&self) -> bool {
        self.kind == HttpErrorKind::Cancelled
    }

    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    pub fn kind_str(&self) -> &'static str {
        match self.kind {
            HttpErrorKind::InvalidUri => "InvalidUri",
            HttpErrorKind::InvalidMethod => "InvalidMethod",
            HttpErrorKind::InvalidHeader => "InvalidHeader",
            HttpErrorKind::HeaderTooLarge => "HeaderTooLarge",
            HttpErrorKind::BodyTooLarge => "BodyTooLarge",
            HttpErrorKind::Timeout => "Timeout",
            HttpErrorKind::Cancelled => "Cancelled",
            HttpErrorKind::DnsError => "DnsError",
            HttpErrorKind::ConnectError => "ConnectError",
            HttpErrorKind::TlsError => "TlsError",
            HttpErrorKind::Http1Error => "Http1Error",
            HttpErrorKind::Http2Error => "Http2Error",
            HttpErrorKind::Http3Error => "Http3Error",
            HttpErrorKind::QuicError => "QuicError",
            HttpErrorKind::ProtocolError => "ProtocolError",
            HttpErrorKind::RedirectError => "RedirectError",
            HttpErrorKind::ProxyError => "ProxyError",
            HttpErrorKind::CacheError => "CacheError",
            HttpErrorKind::CompressionError => "CompressionError",
            HttpErrorKind::MultipartError => "MultipartError",
            HttpErrorKind::AuthenticationError => "AuthenticationError",
            HttpErrorKind::IoError => "IoError",
            HttpErrorKind::SecurityViolation => "SecurityViolation",
            HttpErrorKind::RateLimited => "RateLimited",
            HttpErrorKind::CircuitBroken => "CircuitBroken",
            HttpErrorKind::ParseError => "ParseError",
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HttpError({}): {}", self.kind_str(), self.message)?;
        if let Some(ref op) = self.operation {
            write!(f, " [op={}]", op)?;
        }
        if let Some(ref u) = self.url {
            write!(f, " [url={}]", u)?;
        }
        if let Some(st) = self.status_code {
            write!(f, " [status={}]", st)?;
        }
        Ok(())
    }
}

impl std::error::Error for HttpError {}

impl From<std::io::Error> for HttpError {
    fn from(err: std::io::Error) -> Self {
        let is_timeout = err.kind() == std::io::ErrorKind::TimedOut;
        let is_interrupted = err.kind() == std::io::ErrorKind::Interrupted;
        let is_connection_refused = err.kind() == std::io::ErrorKind::ConnectionRefused;
        let is_connection_reset = err.kind() == std::io::ErrorKind::ConnectionReset;

        let kind = if is_timeout {
            HttpErrorKind::Timeout
        } else if is_interrupted {
            HttpErrorKind::Cancelled
        } else if is_connection_refused || is_connection_reset {
            HttpErrorKind::ConnectError
        } else {
            HttpErrorKind::IoError
        };

        HttpError::new(kind, err.to_string())
    }
}

impl From<String> for HttpError {
    fn from(s: String) -> Self {
        HttpError::new(HttpErrorKind::ProtocolError, s)
    }
}

impl From<&str> for HttpError {
    fn from(s: &str) -> Self {
        HttpError::new(HttpErrorKind::ProtocolError, s.to_string())
    }
}
