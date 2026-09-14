use super::errors::{HttpError, HttpErrorKind};
use super::method::is_tchar;
use crate::utils::collections::FastMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderValidationMode {
    Strict,
    Compatibility,
    Unsafe,
}

impl Default for HeaderValidationMode {
    fn default() -> Self {
        HeaderValidationMode::Strict
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeaderName(String);

impl HeaderName {
    pub fn new(name: &str) -> Result<Self, HttpError> {
        Self::new_with_mode(name, HeaderValidationMode::Strict)
    }

    pub fn new_with_mode(name: &str, mode: HeaderValidationMode) -> Result<Self, HttpError> {
        if mode == HeaderValidationMode::Strict {
            if name.starts_with(' ') || name.starts_with('\t') || name.ends_with(' ') || name.ends_with('\t') {
                return Err(HttpError::new(
                    HttpErrorKind::InvalidHeader,
                    "Header name contains illegal leading or trailing whitespace",
                ));
            }
        }
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(HttpError::new(
                HttpErrorKind::InvalidHeader,
                "Header name cannot be empty",
            ));
        }

        if mode != HeaderValidationMode::Unsafe {
            for b in trimmed.bytes() {
                if !is_tchar(b) {
                    return Err(HttpError::new(
                        HttpErrorKind::InvalidHeader,
                        format!("Illegal character in header name: {:?}", b as char),
                    ));
                }
            }
        }

        Ok(HeaderName(trimmed.to_ascii_lowercase()))
    }

    pub fn from_static(name: &'static str) -> Self {
        HeaderName(name.to_ascii_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderValue(String);

impl HeaderValue {
    pub fn new(value: &str) -> Result<Self, HttpError> {
        Self::new_with_mode(value, HeaderValidationMode::Strict)
    }

    pub fn new_with_mode(value: &str, mode: HeaderValidationMode) -> Result<Self, HttpError> {
        if mode != HeaderValidationMode::Unsafe {
            // Strictly check for CRLF injection, NUL, and non-printable control characters
            for &b in value.as_bytes() {
                if b == b'\r' || b == b'\n' || b == 0 {
                    return Err(HttpError::new(
                        HttpErrorKind::InvalidHeader,
                        "CRLF or NUL injection detected in header value",
                    ));
                }
                if b < 32 && b != b'\t' {
                    return Err(HttpError::new(
                        HttpErrorKind::InvalidHeader,
                        format!(
                            "Control character {:?} forbidden in header value",
                            b as char
                        ),
                    ));
                }
            }
        }

        Ok(HeaderValue(value.trim().to_string()))
    }

    pub fn from_static(value: &'static str) -> Self {
        HeaderValue(value.trim().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Headers {
    // Stored in lower-case key map, preserves insertion order / duplicates
    entries: FastMap<String, Vec<String>>,
    validation_mode: HeaderValidationMode,
}

impl Headers {
    pub fn new() -> Self {
        Self {
            entries: FastMap::default(),
            validation_mode: HeaderValidationMode::Strict,
        }
    }

    pub fn with_mode(mode: HeaderValidationMode) -> Self {
        Self {
            entries: FastMap::default(),
            validation_mode: mode,
        }
    }

    pub fn insert(&mut self, name: &str, value: &str) -> Result<(), HttpError> {
        let h_name = HeaderName::new_with_mode(name, self.validation_mode)?;
        let h_val = HeaderValue::new_with_mode(value, self.validation_mode)?;
        self.entries.insert(h_name.0, vec![h_val.0]);
        Ok(())
    }

    pub fn append(&mut self, name: &str, value: &str) -> Result<(), HttpError> {
        let h_name = HeaderName::new_with_mode(name, self.validation_mode)?;
        let h_val = HeaderValue::new_with_mode(value, self.validation_mode)?;
        self.entries.entry(h_name.0).or_default().push(h_val.0);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        let key = name.trim().to_ascii_lowercase();
        self.entries
            .get(&key)
            .and_then(|v| v.first().map(|s| s.as_str()))
    }

    pub fn get_all(&self, name: &str) -> Vec<&str> {
        let key = name.trim().to_ascii_lowercase();
        match self.entries.get(&key) {
            Some(vec) => vec.iter().map(|s| s.as_str()).collect(),
            None => Vec::new(),
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        let key = name.trim().to_ascii_lowercase();
        self.entries.contains_key(&key)
    }

    pub fn remove(&mut self, name: &str) -> Option<Vec<String>> {
        let key = name.trim().to_ascii_lowercase();
        self.entries.remove(&key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    // Standard typed helpers
    pub fn content_type(&self) -> Option<&str> {
        self.get("content-type")
    }

    pub fn set_content_type(&mut self, ct: &str) -> Result<(), HttpError> {
        self.insert("content-type", ct)
    }

    pub fn content_length(&self) -> Option<u64> {
        self.get("content-length")
            .and_then(|v| v.parse::<u64>().ok())
    }

    pub fn set_content_length(&mut self, len: u64) -> Result<(), HttpError> {
        self.insert("content-length", &len.to_string())
    }

    pub fn is_chunked(&self) -> bool {
        if let Some(te) = self.get("transfer-encoding") {
            te.to_ascii_lowercase().contains("chunked")
        } else {
            false
        }
    }

    pub fn is_keep_alive(&self) -> bool {
        if let Some(conn) = self.get("connection") {
            let lower = conn.to_ascii_lowercase();
            !lower.contains("close")
        } else {
            true
        }
    }

    pub fn to_map(&self) -> FastMap<String, String> {
        let mut map = FastMap::default();
        for (k, vals) in &self.entries {
            map.insert(k.clone(), vals.join(", "));
        }
        map
    }
}
