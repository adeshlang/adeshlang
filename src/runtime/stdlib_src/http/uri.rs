use super::errors::{HttpError, HttpErrorKind};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    pub scheme: Option<String>,
    pub authority: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

impl Uri {
    pub fn parse(s: &str) -> Result<Self, HttpError> {
        let input = s.trim();
        if input.is_empty() {
            return Err(HttpError::new(
                HttpErrorKind::InvalidUri,
                "URI cannot be empty",
            ));
        }

        // Handle asterisk form "*" (for OPTIONS)
        if input == "*" {
            return Ok(Uri {
                scheme: None,
                authority: None,
                host: None,
                port: None,
                path: "*".to_string(),
                query: None,
                fragment: None,
            });
        }

        let mut scheme = None;
        let mut rest = input;

        // Extract scheme if present (e.g. "https://...")
        if let Some(pos) = rest.find("://") {
            let sc = &rest[..pos];
            if !sc
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            {
                return Err(HttpError::new(
                    HttpErrorKind::InvalidUri,
                    "Invalid scheme in URI",
                ));
            }
            scheme = Some(sc.to_ascii_lowercase());
            rest = &rest[pos + 3..];
        }

        let mut authority = None;
        let mut host = None;
        let mut port = None;

        // If scheme was present or begins with authority "//"
        if scheme.is_some() || rest.starts_with("//") {
            if rest.starts_with("//") {
                rest = &rest[2..];
            }
            let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
            let auth_str = &rest[..auth_end];
            authority = Some(auth_str.to_string());
            rest = &rest[auth_end..];

            // Parse host and port from authority
            let (h, p) = parse_host_port(auth_str, scheme.as_deref())?;
            host = Some(h);
            port = p;
        }

        // Fragment
        let mut fragment = None;
        if let Some(frag_pos) = rest.find('#') {
            fragment = Some(rest[frag_pos + 1..].to_string());
            rest = &rest[..frag_pos];
        }

        // Query
        let mut query = None;
        if let Some(q_pos) = rest.find('?') {
            query = Some(rest[q_pos + 1..].to_string());
            rest = &rest[..q_pos];
        }

        let path = if rest.is_empty() {
            if scheme.is_some() {
                "/".to_string()
            } else {
                "".to_string()
            }
        } else {
            rest.to_string()
        };

        Ok(Uri {
            scheme,
            authority,
            host,
            port,
            path,
            query,
            fragment,
        })
    }

    pub fn is_https(&self) -> bool {
        self.scheme.as_deref() == Some("https")
    }

    pub fn default_port(&self) -> u16 {
        match self.scheme.as_deref() {
            Some("https") => 443,
            Some("http") => 80,
            _ => 80,
        }
    }

    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| self.default_port())
    }

    pub fn path_and_query(&self) -> String {
        let mut res = if self.path.is_empty() {
            "/".to_string()
        } else {
            self.path.clone()
        };
        if let Some(ref q) = self.query {
            res.push('?');
            res.push_str(q);
        }
        res
    }

    pub fn query_params(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        if let Some(ref q) = self.query {
            for pair in q.split('&') {
                if pair.is_empty() {
                    continue;
                }
                let mut parts = pair.splitn(2, '=');
                let k = parts.next().unwrap_or("");
                let v = parts.next().unwrap_or("");
                map.insert(percent_decode(k), percent_decode(v));
            }
        }
        map
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref s) = self.scheme {
            write!(f, "{}://", s)?;
        }
        if let Some(ref a) = self.authority {
            write!(f, "{}", a)?;
        }
        write!(f, "{}", self.path)?;
        if let Some(ref q) = self.query {
            write!(f, "?{}", q)?;
        }
        if let Some(ref fr) = self.fragment {
            write!(f, "#{}", fr)?;
        }
        Ok(())
    }
}

fn parse_host_port(auth: &str, scheme: Option<&str>) -> Result<(String, Option<u16>), HttpError> {
    // Strip userinfo if present ("user:pass@host")
    let host_part = if let Some(at) = auth.find('@') {
        &auth[at + 1..]
    } else {
        auth
    };

    if host_part.starts_with('[') {
        // IPv6 literal, e.g. "[::1]:8080"
        let close = host_part.find(']').ok_or_else(|| {
            HttpError::new(HttpErrorKind::InvalidUri, "Malformed IPv6 address in URI")
        })?;
        let ip6 = &host_part[1..close];
        let rest = &host_part[close + 1..];
        let port =
            if let Some(colon) = rest.find(':') {
                let p_str = &rest[colon + 1..];
                Some(p_str.parse::<u16>().map_err(|_| {
                    HttpError::new(HttpErrorKind::InvalidUri, "Invalid port in URI")
                })?)
            } else {
                None
            };
        Ok((format!("[{}]", ip6), port))
    } else if let Some(colon) = host_part.rfind(':') {
        let host = &host_part[..colon];
        let p_str = &host_part[colon + 1..];
        let port = p_str
            .parse::<u16>()
            .map_err(|_| HttpError::new(HttpErrorKind::InvalidUri, "Invalid port in URI"))?;
        Ok((host.to_ascii_lowercase(), Some(port)))
    } else {
        let def_port = match scheme {
            Some("https") => Some(443),
            Some("http") => Some(80),
            _ => None,
        };
        Ok((host_part.to_ascii_lowercase(), def_port))
    }
}

pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(val);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
