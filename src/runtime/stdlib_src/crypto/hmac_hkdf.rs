//! HMAC and HKDF implementations for AdeshLang Crypto standard library.

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha384, Sha512};
use sha3::Sha3_256;
use subtle::ConstantTimeEq;

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|e| format!("HMAC key error: {}", e))?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub fn hmac_sha384(key: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac =
        Hmac::<Sha384>::new_from_slice(key).map_err(|e| format!("HMAC key error: {}", e))?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub fn hmac_sha512(key: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac =
        Hmac::<Sha512>::new_from_slice(key).map_err(|e| format!("HMAC key error: {}", e))?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub fn hmac_sha3_256(key: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac =
        Hmac::<Sha3_256>::new_from_slice(key).map_err(|e| format!("HMAC key error: {}", e))?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub fn hmac_verify(key: &[u8], message: &[u8], expected_tag: &[u8], algo: &str) -> bool {
    let computed = match algo.to_lowercase().as_str() {
        "sha256" | "sha-256" => hmac_sha256(key, message),
        "sha384" | "sha-384" => hmac_sha384(key, message),
        "sha512" | "sha-512" => hmac_sha512(key, message),
        "sha3-256" | "sha3_256" => hmac_sha3_256(key, message),
        _ => return false,
    };

    match computed {
        Ok(tag) => tag.len() == expected_tag.len() && tag.ct_eq(expected_tag).into(),
        Err(_) => false,
    }
}

pub fn hkdf_sha256_extract(salt: Option<&[u8]>, ikm: &[u8]) -> Vec<u8> {
    let (prk, _) = Hkdf::<Sha256>::extract(salt, ikm);
    prk.to_vec()
}

pub fn hkdf_sha256_expand(prk: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>, String> {
    let hk = Hkdf::<Sha256>::from_prk(prk).map_err(|e| format!("Invalid PRK: {}", e))?;
    let mut okm = vec![0u8; length];
    hk.expand(info, &mut okm)
        .map_err(|e| format!("HKDF expand error: {}", e))?;
    Ok(okm)
}

pub fn hkdf_sha256_derive(
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[u8],
    length: usize,
) -> Result<Vec<u8>, String> {
    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut okm = vec![0u8; length];
    hk.expand(info, &mut okm)
        .map_err(|e| format!("HKDF derive error: {}", e))?;
    Ok(okm)
}

pub fn hkdf_sha512_derive(
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[u8],
    length: usize,
) -> Result<Vec<u8>, String> {
    let hk = Hkdf::<Sha512>::new(salt, ikm);
    let mut okm = vec![0u8; length];
    hk.expand(info, &mut okm)
        .map_err(|e| format!("HKDF derive error: {}", e))?;
    Ok(okm)
}
