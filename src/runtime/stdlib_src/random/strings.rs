//! String, Token, Char, and UUID Randomness Generators
//!
//! Handles alphanumeric, ASCII, hex, custom charset sampling, Unicode scalar values, and RFC 4122 v4 UUID generation.

use super::bounded::next_u64_bounded;
use super::prng::Xoshiro256PlusPlus;

const ALPHANUMERIC_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const HEX_CHARS: &[u8] = b"0123456789abcdef";

/// Generate a random string of length `len` using alphanumeric characters.
pub fn random_alphanumeric(rng: &mut Xoshiro256PlusPlus, len: usize) -> String {
    let mut res = String::with_capacity(len);
    let charset_len = ALPHANUMERIC_CHARS.len() as u64;
    for _ in 0..len {
        let idx = next_u64_bounded(rng, charset_len) as usize;
        res.push(ALPHANUMERIC_CHARS[idx] as char);
    }
    res
}

/// Generate a random string of length `len` using printable ASCII characters (32..126).
pub fn random_ascii(rng: &mut Xoshiro256PlusPlus, len: usize) -> String {
    let mut res = String::with_capacity(len);
    for _ in 0..len {
        let code = 32 + next_u64_bounded(rng, 95) as u8;
        res.push(code as char);
    }
    res
}

/// Generate a random hexadecimal string of length `len`.
pub fn random_hex(rng: &mut Xoshiro256PlusPlus, len: usize) -> String {
    let mut res = String::with_capacity(len);
    for _ in 0..len {
        let idx = next_u64_bounded(rng, 16) as usize;
        res.push(HEX_CHARS[idx] as char);
    }
    res
}

/// Generate a random string of length `len` from a custom charset string.
pub fn random_string_from(
    rng: &mut Xoshiro256PlusPlus,
    charset: &str,
    len: usize,
) -> Result<String, String> {
    if charset.is_empty() {
        return Err("Charset string cannot be empty".to_string());
    }
    let chars: Vec<char> = charset.chars().collect();
    let chars_len = chars.len() as u64;
    let mut res = String::with_capacity(len);
    for _ in 0..len {
        let idx = next_u64_bounded(rng, chars_len) as usize;
        res.push(chars[idx]);
    }
    Ok(res)
}

/// Generate a random valid Unicode scalar value (avoiding surrogate range 0xD800..=0xDFFF).
pub fn random_char(rng: &mut Xoshiro256PlusPlus) -> char {
    loop {
        // Unicode scalar range 0..=0x10FFFF
        let val = next_u64_bounded(rng, 0x110000) as u32;
        if !(0xD800..=0xDFFF).contains(&val) {
            if let Some(c) = char::from_u32(val) {
                return c;
            }
        }
    }
}

/// Generate RFC 4122 version 4 compliant UUID string: `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`.
pub fn random_uuid4(rng: &mut Xoshiro256PlusPlus) -> String {
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);

    // Set version to 4 (0100)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    // Set variant to RFC 4122 (10xx)
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

/// Generate random URL-safe base64 token.
pub fn random_base64_token(rng: &mut Xoshiro256PlusPlus, len: usize) -> String {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut res = String::with_capacity(len);
    for _ in 0..len {
        let idx = next_u64_bounded(rng, 64) as usize;
        res.push(BASE64_CHARS[idx] as char);
    }
    res
}
