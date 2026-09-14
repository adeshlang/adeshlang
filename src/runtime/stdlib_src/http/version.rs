use super::errors::{HttpError, HttpErrorKind};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HttpVersion {
    Http10,
    Http11,
    Http20,
    Http30,
}

impl HttpVersion {
    pub fn parse(s: &str) -> Result<Self, HttpError> {
        let trimmed = s.trim();
        match trimmed {
            "HTTP/1.0" | "http/1.0" | "1.0" => Ok(HttpVersion::Http10),
            "HTTP/1.1" | "http/1.1" | "1.1" => Ok(HttpVersion::Http11),
            "HTTP/2.0" | "HTTP/2" | "http/2.0" | "http/2" | "2.0" | "2" | "h2" => {
                Ok(HttpVersion::Http20)
            }
            "HTTP/3.0" | "HTTP/3" | "http/3.0" | "http/3" | "3.0" | "3" | "h3" => {
                Ok(HttpVersion::Http30)
            }
            other => Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                format!("Unsupported or invalid HTTP version: {}", other),
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HttpVersion::Http10 => "HTTP/1.0",
            HttpVersion::Http11 => "HTTP/1.1",
            HttpVersion::Http20 => "HTTP/2.0",
            HttpVersion::Http30 => "HTTP/3.0",
        }
    }

    pub fn alpn_identifier(&self) -> Option<&'static str> {
        match self {
            HttpVersion::Http10 => None,
            HttpVersion::Http11 => Some("http/1.1"),
            HttpVersion::Http20 => Some("h2"),
            HttpVersion::Http30 => Some("h3"),
        }
    }

    pub fn is_multiplexed(&self) -> bool {
        matches!(self, HttpVersion::Http20 | HttpVersion::Http30)
    }

    pub fn supports_server_push(&self) -> bool {
        matches!(self, HttpVersion::Http20 | HttpVersion::Http30)
    }

    pub fn supports_extended_connect(&self) -> bool {
        matches!(self, HttpVersion::Http20 | HttpVersion::Http30)
    }

    pub fn supports_datagrams(&self) -> bool {
        matches!(self, HttpVersion::Http30)
    }

    pub fn supports_trailers(&self) -> bool {
        matches!(
            self,
            HttpVersion::Http11 | HttpVersion::Http20 | HttpVersion::Http30
        )
    }
}

impl fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersionPolicy {
    Auto,
    Http1Only,
    Http2Only,
    Http3Only,
    Http2OrHttp1,
    Http3Preferred,
}

impl Default for HttpVersionPolicy {
    fn default() -> Self {
        HttpVersionPolicy::Auto
    }
}
