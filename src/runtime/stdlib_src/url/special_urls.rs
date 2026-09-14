//! Special Scheme URL Handlers (file://, data:, mailto:, ws://, wss://) for AdeshLang.

use super::url_object::URL;
use crate::runtime::stdlib_src::encoding::base64 as b64;

#[derive(Debug, Clone)]
pub struct DataURL {
    pub media_type: String,
    pub is_base64: bool,
    pub raw_data: String,
}

impl DataURL {
    pub fn parse(input: &str) -> Result<Self, String> {
        if !input.starts_with("data:") {
            return Err("Data URL must start with 'data:' scheme".to_string());
        }

        let payload = &input[5..];
        let (metadata, data) = match payload.find(',') {
            Some(pos) => (&payload[..pos], &payload[pos + 1..]),
            None => return Err("Malformed data URL: missing payload comma ','".to_string()),
        };

        let is_base64 = metadata.ends_with(";base64");
        let media_type = if is_base64 {
            let mt = &metadata[..metadata.len() - 7];
            if mt.is_empty() {
                "text/plain;charset=US-ASCII"
            } else {
                mt
            }
        } else if metadata.is_empty() {
            "text/plain;charset=US-ASCII"
        } else {
            metadata
        };

        Ok(Self {
            media_type: media_type.to_string(),
            is_base64,
            raw_data: data.to_string(),
        })
    }

    pub fn decode_bytes(&self) -> Result<Vec<u8>, String> {
        if self.is_base64 {
            b64::base64_decode(&self.raw_data)
        } else {
            Ok(self.raw_data.as_bytes().to_vec())
        }
    }

    pub fn decode_string(&self) -> Result<String, String> {
        let bytes = self.decode_bytes()?;
        String::from_utf8(bytes).map_err(|e| format!("Data URL payload is invalid UTF-8: {}", e))
    }
}

pub fn from_file_path(path_str: &str) -> Result<URL, String> {
    let normalized = path_str.replace('\\', "/");
    let url_str = if normalized.starts_with('/') {
        format!("file://{}", normalized)
    } else {
        format!("file:///{}", normalized)
    };
    URL::parse(&url_str)
}

pub fn to_file_path(url: &URL) -> Result<String, String> {
    if url.scheme() != "file" {
        return Err(format!(
            "Cannot convert non-file URL scheme '{}' to file path",
            url.scheme()
        ));
    }
    let p = url.path();
    if cfg!(windows) && p.starts_with('/') && p.chars().nth(2) == Some(':') {
        Ok(p[1..].replace('/', "\\"))
    } else {
        Ok(p.to_string())
    }
}

pub fn to_web_socket_url(url: &URL) -> Result<URL, String> {
    match url.scheme() {
        "http" => url.with_scheme("ws"),
        "https" => url.with_scheme("wss"),
        "ws" | "wss" => Ok(url.clone()),
        other => Err(format!(
            "Cannot convert scheme '{}' to WebSocket scheme",
            other
        )),
    }
}

pub fn to_http_url(url: &URL) -> Result<URL, String> {
    match url.scheme() {
        "ws" => url.with_scheme("http"),
        "wss" => url.with_scheme("https"),
        "http" | "https" => Ok(url.clone()),
        other => Err(format!("Cannot convert scheme '{}' to HTTP scheme", other)),
    }
}

pub fn create_data_url(media_type: &str, payload: &str, is_base64: bool) -> String {
    if is_base64 {
        let encoded = b64::base64_encode(payload.as_bytes());
        format!("data:{};base64,{}", media_type, encoded)
    } else {
        format!("data:{},{}", media_type, payload)
    }
}
