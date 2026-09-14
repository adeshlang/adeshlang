//! Text Encodings for AdeshLang (UTF-8, UTF-16, UTF-32, ASCII).

/// Encode a Rust String to UTF-8 bytes.
pub fn utf8_encode(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

/// Decode UTF-8 bytes to a String (Strict mode). Returns error if invalid UTF-8.
pub fn utf8_decode(bytes: &[u8]) -> Result<String, String> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(e) => Err(format!(
            "Invalid UTF-8 at byte offset {}: {}",
            e.valid_up_to(),
            e
        )),
    }
}

/// Decode UTF-8 bytes to a String (Lossy mode). Replaces invalid sequences with U+FFFD.
pub fn utf8_decode_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Validate if bytes form valid UTF-8.
pub fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// Detailed UTF-8 validation error details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf8ValidationError {
    pub offset: usize,
    pub error_len: Option<usize>,
    pub message: String,
}

pub fn validate_utf8(bytes: &[u8]) -> Result<(), Utf8ValidationError> {
    match std::str::from_utf8(bytes) {
        Ok(_) => Ok(()),
        Err(e) => Err(Utf8ValidationError {
            offset: e.valid_up_to(),
            error_len: e.error_len(),
            message: format!("Invalid UTF-8 sequence at byte offset {}", e.valid_up_to()),
        }),
    }
}

/// UTF-16 LE Encode
pub fn utf16_le_encode(s: &str) -> Vec<u8> {
    let u16s: Vec<u16> = s.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(u16s.len() * 2);
    for word in u16s {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

/// UTF-16 BE Encode
pub fn utf16_be_encode(s: &str) -> Vec<u8> {
    let u16s: Vec<u16> = s.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(u16s.len() * 2);
    for word in u16s {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    bytes
}

/// UTF-16 LE Decode
pub fn utf16_le_decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("UTF-16 LE decoding requires an even number of bytes".to_string());
    }
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    String::from_utf16(&words).map_err(|e| format!("Invalid UTF-16 LE sequence: {}", e))
}

/// UTF-16 BE Decode
pub fn utf16_be_decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("UTF-16 BE decoding requires an even number of bytes".to_string());
    }
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();
    String::from_utf16(&words).map_err(|e| format!("Invalid UTF-16 BE sequence: {}", e))
}

/// UTF-16 Decode with optional BOM detection. Default is LE if no BOM present.
pub fn utf16_decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() >= 2 {
        if bytes[0] == 0xFF && bytes[1] == 0xFE {
            return utf16_le_decode(&bytes[2..]);
        } else if bytes[0] == 0xFE && bytes[1] == 0xFF {
            return utf16_be_decode(&bytes[2..]);
        }
    }
    utf16_le_decode(bytes)
}

/// UTF-32 LE Encode
pub fn utf32_le_encode(s: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(s.chars().count() * 4);
    for ch in s.chars() {
        bytes.extend_from_slice(&(ch as u32).to_le_bytes());
    }
    bytes
}

/// UTF-32 BE Encode
pub fn utf32_be_encode(s: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(s.chars().count() * 4);
    for ch in s.chars() {
        bytes.extend_from_slice(&(ch as u32).to_be_bytes());
    }
    bytes
}

/// UTF-32 LE Decode
pub fn utf32_le_decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() % 4 != 0 {
        return Err("UTF-32 LE decoding requires byte length multiple of 4".to_string());
    }
    let mut s = String::with_capacity(bytes.len() / 4);
    for (idx, chunk) in bytes.chunks_exact(4).enumerate() {
        let code = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let ch = char::from_u32(code).ok_or_else(|| {
            format!(
                "Invalid Unicode scalar value 0x{:08X} at char index {}",
                code, idx
            )
        })?;
        s.push(ch);
    }
    Ok(s)
}

/// UTF-32 BE Decode
pub fn utf32_be_decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() % 4 != 0 {
        return Err("UTF-32 BE decoding requires byte length multiple of 4".to_string());
    }
    let mut s = String::with_capacity(bytes.len() / 4);
    for (idx, chunk) in bytes.chunks_exact(4).enumerate() {
        let code = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let ch = char::from_u32(code).ok_or_else(|| {
            format!(
                "Invalid Unicode scalar value 0x{:08X} at char index {}",
                code, idx
            )
        })?;
        s.push(ch);
    }
    Ok(s)
}

/// UTF-32 Decode with optional BOM detection. Default is LE if no BOM present.
pub fn utf32_decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() >= 4 {
        if bytes[0..4] == [0xFF, 0xFE, 0x00, 0x00] {
            return utf32_le_decode(&bytes[4..]);
        } else if bytes[0..4] == [0x00, 0x00, 0xFE, 0xFF] {
            return utf32_be_decode(&bytes[4..]);
        }
    }
    utf32_le_decode(bytes)
}

/// ASCII Encode (strict mode: errors if string contains non-ASCII characters).
pub fn ascii_encode(s: &str) -> Result<Vec<u8>, String> {
    if !s.is_ascii() {
        return Err("String contains non-ASCII characters".to_string());
    }
    Ok(s.as_bytes().to_vec())
}

/// ASCII Decode (strict mode: errors if bytes contain non-ASCII byte values).
pub fn ascii_decode(bytes: &[u8]) -> Result<String, String> {
    if !bytes.is_ascii() {
        return Err("Byte slice contains non-ASCII byte values (> 127)".to_string());
    }
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// Check if byte slice or string is ASCII
pub fn is_ascii(bytes: &[u8]) -> bool {
    bytes.is_ascii()
}
