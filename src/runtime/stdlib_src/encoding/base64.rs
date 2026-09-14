//! Base64 and Base64URL Encodings for AdeshLang.

use base64::Engine;

/// Encode bytes to Standard Base64 string (RFC 4648).
pub fn base64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Decode Standard Base64 string to bytes (RFC 4648). Strict checking.
pub fn base64_decode(b64_str: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(b64_str.trim())
        .map_err(|e| format!("Base64 decoding failed: {}", e))
}

/// Encode bytes to URL-safe Base64 string without padding (RFC 4648 §5).
pub fn base64_url_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Decode URL-safe Base64 string (handles padded or unpadded).
pub fn base64_url_decode(b64_url_str: &str) -> Result<Vec<u8>, String> {
    let clean = b64_url_str.trim();
    if clean.contains('=') {
        base64::engine::general_purpose::URL_SAFE
            .decode(clean)
            .map_err(|e| format!("Base64URL decoding failed: {}", e))
    } else {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(clean)
            .map_err(|e| format!("Base64URL decoding failed: {}", e))
    }
}

/// Stateful Streaming Base64 Encoder
pub struct Base64Encoder {
    buffer: Vec<u8>,
    output: String,
    url_safe: bool,
}

impl Base64Encoder {
    pub fn new(url_safe: bool) -> Self {
        Self {
            buffer: Vec::new(),
            output: String::new(),
            url_safe,
        }
    }

    pub fn write_chunk(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        let complete_triplets = self.buffer.len() / 3;
        if complete_triplets > 0 {
            let process_bytes = complete_triplets * 3;
            let slice = &self.buffer[..process_bytes];
            if self.url_safe {
                self.output.push_str(&base64_url_encode(slice));
            } else {
                self.output.push_str(&base64_encode(slice));
            }
            self.buffer.drain(..process_bytes);
        }
    }

    pub fn finish(mut self) -> String {
        if !self.buffer.is_empty() {
            if self.url_safe {
                self.output.push_str(&base64_url_encode(&self.buffer));
            } else {
                self.output.push_str(&base64_encode(&self.buffer));
            }
        }
        self.output
    }
}

/// Stateful Streaming Base64 Decoder
pub struct Base64Decoder {
    buffer: String,
    url_safe: bool,
}

impl Base64Decoder {
    pub fn new(url_safe: bool) -> Self {
        Self {
            buffer: String::new(),
            url_safe,
        }
    }

    pub fn write_chunk(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        if self.url_safe {
            base64_url_decode(&self.buffer)
        } else {
            base64_decode(&self.buffer)
        }
    }
}
