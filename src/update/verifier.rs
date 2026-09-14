//! Verification utilities for downloaded update archives and files.

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
