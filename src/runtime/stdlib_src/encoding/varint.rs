//! Variable-length Integers (VarInt, ZigZag, LEB128) for AdeshLang.

// ----------------------------------------------------------------------------
// Unsigned VarInt (Base-128 VLQ)
// ----------------------------------------------------------------------------

/// Encode a u64 integer as an unsigned VarInt (Base-128 LEB128 / VLQ).
pub fn varint_encode(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 0x80 {
        bytes.push(((value as u8) & 0x7F) | 0x80);
        value >>= 7;
    }
    bytes.push((value as u8) & 0x7F);
    bytes
}

/// Decode an unsigned VarInt from bytes. Returns (decoded_value, bytes_read).
pub fn varint_decode(bytes: &[u8]) -> Result<(u64, usize), String> {
    let mut result: u64 = 0;
    let mut shift = 0;
    for (idx, &byte) in bytes.iter().enumerate() {
        if shift >= 64 {
            return Err("VarInt overflow: more than 10 bytes read for u64".to_string());
        }
        let val = (byte & 0x7F) as u64;
        result |= val << shift;
        if (byte & 0x80) == 0 {
            return Ok((result, idx + 1));
        }
        shift += 7;
    }
    Err("Truncated VarInt sequence (missing terminating byte with MSB=0)".to_string())
}

// ----------------------------------------------------------------------------
// ZigZag Encoding for Signed Integers
// ----------------------------------------------------------------------------

/// Convert i64 to ZigZag u64 representation (0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, ...).
pub fn zigzag_encode(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

/// Convert ZigZag u64 representation back to i64.
pub fn zigzag_decode(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

/// Encode signed i64 as signed VarInt using ZigZag encoding.
pub fn signed_varint_encode(value: i64) -> Vec<u8> {
    varint_encode(zigzag_encode(value))
}

/// Decode signed VarInt using ZigZag decoding. Returns (decoded_i64, bytes_read).
pub fn signed_varint_decode(bytes: &[u8]) -> Result<(i64, usize), String> {
    let (u_val, read) = varint_decode(bytes)?;
    Ok((zigzag_decode(u_val), read))
}

// ----------------------------------------------------------------------------
// ULEB128 and SLEB128 (WASM Binary Specification compliant)
// ----------------------------------------------------------------------------

/// Encode u64 to ULEB128.
pub fn uleb128_encode(value: u64) -> Vec<u8> {
    varint_encode(value)
}

/// Decode ULEB128. Returns (value, bytes_read).
pub fn uleb128_decode(bytes: &[u8]) -> Result<(u64, usize), String> {
    varint_decode(bytes)
}

/// Encode i64 to SLEB128.
pub fn sleb128_encode(mut value: i64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (value == 0 && !sign_bit) || (value == -1 && sign_bit) {
            bytes.push(byte);
            break;
        } else {
            bytes.push(byte | 0x80);
        }
    }
    bytes
}

/// Decode SLEB128. Returns (value, bytes_read).
pub fn sleb128_decode(bytes: &[u8]) -> Result<(i64, usize), String> {
    let mut result: i64 = 0;
    let mut shift = 0;
    let mut byte: u8;
    for (idx, &b) in bytes.iter().enumerate() {
        byte = b;
        if shift >= 64 {
            return Err("SLEB128 overflow: exceeds i64 capacity".to_string());
        }
        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            if shift < 64 && (byte & 0x40) != 0 {
                result |= !0 << shift;
            }
            return Ok((result, idx + 1));
        }
    }
    Err("Truncated SLEB128 sequence".to_string())
}
