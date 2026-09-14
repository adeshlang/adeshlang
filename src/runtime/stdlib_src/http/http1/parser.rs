use super::super::body::Body;
use super::super::errors::{HttpError, HttpErrorKind};
use super::super::headers::Headers;
use super::super::method::HttpMethod;
use super::super::request::Request;
use super::super::response::Response;
use super::super::status::HttpStatus;
use super::super::uri::Uri;
use super::super::version::HttpVersion;
use super::chunked::decode_chunked;
use super::smuggle::validate_framing_security;

#[derive(Debug, Clone)]
pub struct Http1Limits {
    pub max_request_line: usize,
    pub max_header_count: usize,
    pub max_header_bytes: usize,
    pub max_body_size: usize,
}

impl Default for Http1Limits {
    fn default() -> Self {
        Self {
            max_request_line: 8 * 1024,
            max_header_count: 128,
            max_header_bytes: 64 * 1024,
            max_body_size: 64 * 1024 * 1024, // 64 MB
        }
    }
}

pub struct Http1Parser {
    pub limits: Http1Limits,
}

impl Http1Parser {
    pub fn new() -> Self {
        Self {
            limits: Http1Limits::default(),
        }
    }

    pub fn with_limits(limits: Http1Limits) -> Self {
        Self { limits }
    }

    pub fn parse_request(&self, buffer: &[u8]) -> Result<(Request, usize), HttpError> {
        let (header_bytes, header_len) = find_header_end(buffer, self.limits.max_header_bytes)?;

        let header_str = std::str::from_utf8(header_bytes).map_err(|_| {
            HttpError::new(
                HttpErrorKind::Http1Error,
                "Invalid UTF-8 in request headers",
            )
        })?;

        let mut lines = header_str.split("\r\n");
        let req_line = lines
            .next()
            .ok_or_else(|| HttpError::new(HttpErrorKind::Http1Error, "Empty HTTP request"))?;

        if req_line.len() > self.limits.max_request_line {
            return Err(HttpError::new(
                HttpErrorKind::HeaderTooLarge,
                "Request line exceeds maximum allowed size",
            ));
        }

        let mut parts = req_line.split_whitespace();
        let method_str = parts
            .next()
            .ok_or_else(|| HttpError::new(HttpErrorKind::Http1Error, "Missing HTTP method"))?;
        let target_str = parts.next().ok_or_else(|| {
            HttpError::new(HttpErrorKind::Http1Error, "Missing request target URI")
        })?;
        let version_str = parts.next().unwrap_or("HTTP/1.1");

        let method = HttpMethod::parse(method_str)?;
        let uri = Uri::parse(target_str)?;
        let version = HttpVersion::parse(version_str)?;

        let mut headers = Headers::new();
        let mut header_count = 0;
        for line in lines {
            if line.is_empty() {
                break;
            }
            header_count += 1;
            if header_count > self.limits.max_header_count {
                return Err(HttpError::new(
                    HttpErrorKind::HeaderTooLarge,
                    "Too many header fields",
                ));
            }
            if let Some(colon) = line.find(':') {
                let name = &line[..colon];
                let value = &line[colon + 1..];
                headers.append(name, value)?;
            } else {
                return Err(HttpError::new(
                    HttpErrorKind::InvalidHeader,
                    format!("Malformed header line: {:?}", line),
                ));
            }
        }

        validate_framing_security(&headers)?;

        // Parse Body
        let body_start = header_len;
        let remaining = &buffer[body_start..];
        let (body, total_consumed) = if headers.is_chunked() {
            let (chunked_body, _trailers, consumed) = decode_chunked(remaining)?;
            let req = Request {
                method,
                uri,
                version,
                headers,
                body: Body::from_bytes(chunked_body),
                timeout: None,
                extensions: std::collections::HashMap::new(),
            };
            return Ok((req, body_start + consumed));
        } else if let Some(cl) = headers.content_length() {
            let cl_usize = cl as usize;
            if cl_usize > self.limits.max_body_size {
                return Err(HttpError::new(
                    HttpErrorKind::BodyTooLarge,
                    "Request body exceeds maximum size limit",
                ));
            }
            if remaining.len() < cl_usize {
                return Err(HttpError::new(
                    HttpErrorKind::Http1Error,
                    "Incomplete request body for specified Content-Length",
                ));
            }
            (
                Body::from_bytes(remaining[..cl_usize].to_vec()),
                body_start + cl_usize,
            )
        } else {
            (Body::Empty, body_start)
        };

        let req = Request {
            method,
            uri,
            version,
            headers,
            body,
            timeout: None,
            extensions: std::collections::HashMap::new(),
        };

        Ok((req, total_consumed))
    }

    pub fn parse_response(&self, buffer: &[u8]) -> Result<(Response, usize), HttpError> {
        let (header_bytes, header_len) = find_header_end(buffer, self.limits.max_header_bytes)?;

        let header_str = std::str::from_utf8(header_bytes).map_err(|_| {
            HttpError::new(
                HttpErrorKind::Http1Error,
                "Invalid UTF-8 in response headers",
            )
        })?;

        let mut lines = header_str.split("\r\n");
        let status_line = lines
            .next()
            .ok_or_else(|| HttpError::new(HttpErrorKind::Http1Error, "Empty HTTP response"))?;

        let mut parts = status_line.split_whitespace();
        let version_str = parts.next().ok_or_else(|| {
            HttpError::new(
                HttpErrorKind::Http1Error,
                "Missing HTTP version in status line",
            )
        })?;
        let status_code_str = parts.next().ok_or_else(|| {
            HttpError::new(
                HttpErrorKind::Http1Error,
                "Missing status code in status line",
            )
        })?;

        let version = HttpVersion::parse(version_str)?;
        let status_code = status_code_str
            .parse::<u16>()
            .map_err(|_| HttpError::new(HttpErrorKind::Http1Error, "Invalid status code number"))?;
        let status = HttpStatus(status_code);

        let mut headers = Headers::new();
        let mut header_count = 0;
        for line in lines {
            if line.is_empty() {
                break;
            }
            header_count += 1;
            if header_count > self.limits.max_header_count {
                return Err(HttpError::new(
                    HttpErrorKind::HeaderTooLarge,
                    "Too many header fields in response",
                ));
            }
            if let Some(colon) = line.find(':') {
                let name = &line[..colon];
                let value = &line[colon + 1..];
                headers.append(name, value)?;
            }
        }

        validate_framing_security(&headers)?;

        // Body parsing
        let body_start = header_len;
        let remaining = &buffer[body_start..];

        // 1xx, 204, and 304 responses MUST NOT contain a message body
        if status.is_informational() || status.code() == 204 || status.code() == 304 {
            let resp = Response {
                status,
                version,
                headers,
                body: Body::Empty,
                trailers: None,
            };
            return Ok((resp, body_start));
        }

        if headers.is_chunked() {
            let (chunked_body, trailers, consumed) = decode_chunked(remaining)?;
            let resp = Response {
                status,
                version,
                headers,
                body: Body::from_bytes(chunked_body),
                trailers,
            };
            return Ok((resp, body_start + consumed));
        } else if let Some(cl) = headers.content_length() {
            let cl_usize = cl as usize;
            if remaining.len() < cl_usize {
                return Err(HttpError::new(
                    HttpErrorKind::Http1Error,
                    "Incomplete response body for specified Content-Length",
                ));
            }
            let resp = Response {
                status,
                version,
                headers,
                body: Body::from_bytes(remaining[..cl_usize].to_vec()),
                trailers: None,
            };
            Ok((resp, body_start + cl_usize))
        } else {
            // Read until end of connection
            let resp = Response {
                status,
                version,
                headers,
                body: Body::from_bytes(remaining.to_vec()),
                trailers: None,
            };
            Ok((resp, buffer.len()))
        }
    }
}

fn find_header_end(data: &[u8], max_bytes: usize) -> Result<(&[u8], usize), HttpError> {
    let limit = data.len().min(max_bytes);
    for i in 0..limit.saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return Ok((&data[..i], i + 4));
        }
    }
    if data.len() > max_bytes {
        return Err(HttpError::new(
            HttpErrorKind::HeaderTooLarge,
            "Headers exceeded maximum permitted size",
        ));
    }
    Err(HttpError::new(
        HttpErrorKind::Http1Error,
        "Incomplete HTTP headers (missing \\r\\n\\r\\n)",
    ))
}
