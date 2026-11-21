//! Verification utilities for downloaded update archives and files.

use super::manifest::ReleaseManifest;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

/// Calculate the hex-encoded SHA-256 hash of a file
pub fn calculate_sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in result {
        hex.push_str(&format!("{:02x}", byte));
    }
    Ok(hex)
}

/// Verify that a file's SHA-256 matches the expected checksum
pub fn verify_checksum(path: &Path, expected_sha256: &str) -> Result<bool, String> {
    let actual = calculate_sha256(path)
        .map_err(|e| format!("Failed to compute SHA-256 for '{}': {e}", path.display()))?;

    Ok(actual.eq_ignore_ascii_case(expected_sha256.trim()))
}

fn decode_hex_fixed<const N: usize>(hex_str: &str, label: &str) -> Result<[u8; N], String> {
    let s = hex_str.trim();
    if s.len() != N * 2 {
        return Err(format!(
            "Invalid {label} length: expected {} hex chars, got {}",
            N * 2,
            s.len()
        ));
    }
    let mut out = [0u8; N];
    for (i, slot) in out.iter_mut().enumerate() {
        let byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| format!("Invalid hex character in {label}"))?;
        *slot = byte;
    }
    Ok(out)
}

/// Verify an Ed25519 signature (hex-encoded, 64 bytes) over `payload` using a hex-encoded 32-byte public key.
pub fn verify_ed25519_signature(
    payload: &[u8],
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<bool, String> {
    let pk_bytes = decode_hex_fixed::<32>(public_key_hex, "Ed25519 public key")?;
    let sig_bytes = decode_hex_fixed::<64>(signature_hex, "Ed25519 signature")?;

    let verifying_key = VerifyingKey::from_bytes(&pk_bytes)
        .map_err(|e| format!("Invalid Ed25519 public key: {e}"))?;
    let signature = Signature::from_bytes(&sig_bytes);

    Ok(verifying_key.verify(payload, &signature).is_ok())
}

/// Verify a release manifest's Ed25519 signature against a trusted public key (if configured
/// via `public_key_hex` or `ADESH_UPDATE_PUBLIC_KEY`).
pub fn verify_manifest_signature(
    manifest: &ReleaseManifest,
    public_key_hex: Option<&str>,
) -> Result<(), String> {
    let env_key = std::env::var("ADESH_UPDATE_PUBLIC_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let active_key = public_key_hex
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(env_key.as_deref());

    match (active_key, manifest.signature.as_deref()) {
        (Some(key), Some(sig)) => {
            let payload = manifest.canonical_signing_payload()?;
            let valid = verify_ed25519_signature(&payload, sig, key)?;
            if !valid {
                return Err(
                    "Security verification failed: release manifest Ed25519 signature is invalid"
                        .to_string(),
                );
            }
            Ok(())
        }
        (Some(_), None) => Err(
            "Security verification failed: ADESH_UPDATE_PUBLIC_KEY is configured but release manifest is unsigned"
                .to_string(),
        ),
        (None, Some(sig)) => {
            // Validate signature syntax even when no trusted public key is pinned locally
            let _ = decode_hex_fixed::<64>(sig, "Ed25519 signature")?;
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

/// Validate a version string or component name before joining it into filesystem paths.
/// Rejects path separators, `..`, `.`, null bytes, and characters outside `[A-Za-z0-9._+-]`.
pub fn validate_safe_identifier<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    if value.is_empty() || value.len() > 128 {
        return Err(format!("Invalid {label}: must be 1..=128 characters"));
    }
    if value == "." || value == ".." || value.contains("..") {
        return Err(format!(
            "Security violation: unsafe traversal sequence in {label} '{value}'"
        ));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(format!(
            "Security violation: disallowed characters in {label} '{value}'"
        ));
    }
    Ok(value)
}

/// Sanitize entry paths inside zip archives to prevent path traversal attacks
pub fn sanitize_archive_entry_path(entry_path: &str) -> Option<PathBuf> {
    let mut sanitized = PathBuf::new();
    for comp in Path::new(entry_path).components() {
        match comp {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir => {}
            _ => return None, // Reject ParentDir, RootDir, Prefix
        }
    }
    Some(sanitized)
}
