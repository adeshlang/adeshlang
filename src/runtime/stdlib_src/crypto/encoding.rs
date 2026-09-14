//! Hex and Base64 Encodings for AdeshLang Crypto.

use base64::Engine;

pub fn encode_hex(data: &[u8]) -> String {
    hex::encode(data)
}

pub fn decode_hex(hex_str: &str) -> Result<Vec<u8>, String> {
    hex::decode(hex_str).map_err(|e| format!("Hex decoding failed: {}", e))
}

pub fn encode_base64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

pub fn decode_base64(b64_str: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(b64_str)
        .map_err(|e| format!("Base64 decoding failed: {}", e))
}

pub fn encode_base64url(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

pub fn decode_base64url(b64_url_str: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(b64_url_str)
        .map_err(|e| format!("Base64Url decoding failed: {}", e))
}
