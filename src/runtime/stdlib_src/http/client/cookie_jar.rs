use super::super::errors::{HttpError, HttpErrorKind};
use super::super::uri::Uri;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SameSitePolicy {
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: String,
    pub expires_timestamp: Option<u64>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSitePolicy,
}

impl Cookie {
    pub fn parse(header_val: &str, default_domain: &str) -> Result<Self, HttpError> {
        let parts: Vec<&str> = header_val.split(';').map(|s| s.trim()).collect();
        if parts.is_empty() {
            return Err(HttpError::new(
                HttpErrorKind::InvalidHeader,
                "Empty Set-Cookie header",
            ));
        }

        let mut name_val = parts[0].splitn(2, '=');
        let name = name_val
            .next()
            .ok_or_else(|| HttpError::new(HttpErrorKind::InvalidHeader, "Missing cookie name"))?
            .trim()
            .to_string();
        let value = name_val.next().unwrap_or("").trim().to_string();

        let mut domain = None;
        let mut path = "/".to_string();
        let mut expires_timestamp = None;
        let mut secure = false;
        let mut http_only = false;
        let mut same_site = SameSitePolicy::Lax;

        for part in &parts[1..] {
            let lower = part.to_ascii_lowercase();
            if lower.starts_with("domain=") {
                domain = Some(
                    part[7..]
                        .trim()
                        .trim_start_matches('.')
                        .to_ascii_lowercase(),
                );
            } else if lower.starts_with("path=") {
                path = part[5..].trim().to_string();
            } else if lower.starts_with("max-age=") {
                if let Ok(secs) = part[8..].trim().parse::<u64>() {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    expires_timestamp = Some(now + secs);
                }
            } else if lower == "secure" {
                secure = true;
            } else if lower == "httponly" {
                http_only = true;
            } else if lower.starts_with("samesite=") {
                match lower[9..].trim() {
                    "strict" => same_site = SameSitePolicy::Strict,
                    "none" => same_site = SameSitePolicy::None,
                    _ => same_site = SameSitePolicy::Lax,
                }
            }
        }

        let eff_domain = domain.unwrap_or_else(|| default_domain.to_ascii_lowercase());

        Ok(Cookie {
            name,
            value,
            domain: Some(eff_domain),
            path,
            expires_timestamp,
            secure,
            http_only,
            same_site,
        })
    }

    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_timestamp {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > exp
        } else {
            false
        }
    }

    pub fn matches_uri(&self, uri: &Uri) -> bool {
        if self.is_expired() {
            return false;
        }

        if let Some(ref host) = uri.host {
            let lower_host = host.to_ascii_lowercase();
            if let Some(ref d) = self.domain {
                if !lower_host.ends_with(d) {
                    return false;
                }
            }
        }

        if !uri.path.starts_with(&self.path) {
            return false;
        }

        if self.secure && !uri.is_https() {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct CookieJar {
    cookies: Arc<Mutex<HashMap<String, Cookie>>>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self {
            cookies: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add(&self, cookie: Cookie) {
        let key = format!(
            "{}:{}:{}",
            cookie.domain.as_deref().unwrap_or(""),
            cookie.path,
            cookie.name
        );
        if let Ok(mut lock) = self.cookies.lock() {
            lock.insert(key, cookie);
        }
    }

    pub fn get_header_for_uri(&self, uri: &Uri) -> Option<String> {
        let lock = self.cookies.lock().ok()?;
        let mut matched = Vec::new();
        for cookie in lock.values() {
            if cookie.matches_uri(uri) {
                matched.push(format!("{}={}", cookie.name, cookie.value));
            }
        }
        if matched.is_empty() {
            None
        } else {
            Some(matched.join("; "))
        }
    }
}
