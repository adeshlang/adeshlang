//! Percent Encoding and Web/Form Encoding for AdeshLang.

/// Check if character is unreserved in RFC 3986 (A-Z, a-z, 0-9, '-', '_', '.', '~').
pub fn is_unreserved(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
}

/// Percent encode string (RFC 3986 URI Component rules).
pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        if is_unreserved(b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

/// Percent decode string (RFC 3986).
pub fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("Truncated percent escape at position {}", i));
            }
            let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3])
                .map_err(|_| format!("Invalid UTF-8 in percent escape at position {}", i))?;
            let val = u8::from_str_radix(hex_str, 16).map_err(|_| {
                format!(
                    "Invalid hex digit '{}' in percent escape at position {}",
                    hex_str, i
                )
            })?;
            decoded.push(val);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(decoded).map_err(|e| format!("Invalid UTF-8 after percent decoding: {}", e))
}

/// Encode component for URLs.
pub fn url_encode_component(input: &str) -> String {
    percent_encode(input)
}

/// Decode component for URLs.
pub fn url_decode_component(input: &str) -> Result<String, String> {
    percent_decode(input)
}

/// Form URL encode (`application/x-www-form-urlencoded`), spaces become `+`.
pub fn form_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        if b == b' ' {
            encoded.push('+');
        } else if is_unreserved(b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

/// Form URL decode (`application/x-www-form-urlencoded`), `+` becomes space.
pub fn form_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'+' {
            decoded.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("Truncated percent escape at position {}", i));
            }
            let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3])
                .map_err(|_| format!("Invalid UTF-8 in percent escape at position {}", i))?;
            let val = u8::from_str_radix(hex_str, 16).map_err(|_| {
                format!(
                    "Invalid hex digit '{}' in percent escape at position {}",
                    hex_str, i
                )
            })?;
            decoded.push(val);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(decoded).map_err(|e| format!("Invalid UTF-8 after form decoding: {}", e))
}
