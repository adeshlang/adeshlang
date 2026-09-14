//! Archive verification and extraction for toolchain downloads.
//!
//! Supports the formats the curated toolchain artifacts ship in:
//! `.zip` (Windows), `.tar.xz`, `.tar.gz`, and `.tar.zst` (Unix).
//! All entry paths are checked against traversal before writing.

use std::fs;
use std::io;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// Compute the hex SHA-256 of a file, streaming in bounded chunks.
pub fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    Ok(hex)
}

/// Constant-time-ish comparison of hex digests (case-insensitive).
pub fn sha256_matches(actual: &str, expected: &str) -> bool {
    if actual.len() != expected.len() {
        return false;
    }
    actual
        .bytes()
        .zip(expected.bytes())
        .fold(true, |acc, (a, b)| {
            acc & (a.to_ascii_lowercase() == b.to_ascii_lowercase())
        })
}

/// Reject absolute paths and `..` traversal inside archives.
fn safe_entry_path(entry: &str) -> Option<PathBuf> {
    let mut path = PathBuf::new();
    for component in Path::new(entry).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => return None, // ParentDir, RootDir, Prefix
        }
    }
    Some(path)
}

/// Guess the archive format from a URL or filename extension.
pub fn format_from_name(name: &str) -> Option<&'static str> {
    if name.ends_with(".zip") {
        Some("zip")
    } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        Some("tar.xz")
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some("tar.gz")
    } else if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        Some("tar.zst")
    } else if name.ends_with(".tar") {
        Some("tar")
    } else {
        None
    }
}

/// Convert an io::Error into the crate's string error type.
fn ioerr(e: std::io::Error) -> String {
    e.to_string()
}

/// Extract `archive_path` (of the given format) into `dest`, returning the
/// number of entries written. Refuses to overwrite files outside `dest`.
pub fn extract(archive_path: &Path, format: &str, dest: &Path) -> Result<u64, String> {
    fs::create_dir_all(dest).map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
    match format {
        "zip" => extract_zip(archive_path, dest),
        "tar" => extract_tar_stream(Box::new(fs::File::open(archive_path).map_err(ioerr)?), dest),
        "tar.xz" => {
            let file = fs::File::open(archive_path).map_err(ioerr)?;
            let temp = tempfile_path(archive_path, "xz-out")?;
            lzma_rs::xz_decompress(
                &mut io::BufReader::new(file),
                &mut fs::File::create(&temp).map_err(ioerr)?,
            )
            .map_err(|e| format!("Failed to decompress xz: {e}"))?;
            let result = extract_tar_stream(Box::new(fs::File::open(&temp).map_err(ioerr)?), dest);
            let _ = fs::remove_file(&temp);
            result
        }
        "tar.gz" => {
            let file = fs::File::open(archive_path).map_err(ioerr)?;
            let gz = flate2::read::GzDecoder::new(file);
            extract_tar_stream(Box::new(gz), dest)
        }
        "tar.zst" => {
            let file = fs::File::open(archive_path).map_err(ioerr)?;
            let zr = zstd::stream::read::Decoder::new(file)
                .map_err(|e| format!("Failed to open zstd stream: {e}"))?;
            extract_tar_stream(Box::new(zr), dest)
        }
        other => Err(format!("Unsupported archive format `{other}`")),
    }
}

fn tempfile_path(archive: &Path, suffix: &str) -> Result<PathBuf, String> {
    let dir = archive.parent().unwrap_or_else(|| Path::new("."));
    let base = archive
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "archive".to_string());
    let path = dir.join(format!(".adesh-{base}-{suffix}.tmp"));
    // Fail fast if a stale temp file exists.
    let _ = fs::remove_file(&path);
    Ok(path)
}

fn extract_zip(archive_path: &Path, dest: &Path) -> Result<u64, String> {
    let file = fs::File::open(archive_path).map_err(ioerr)?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| format!("Failed to open zip: {e}"))?;
    let mut count = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| format!("Failed to read zip entry {index}: {e}"))?;
        let Some(path) = safe_entry_path(entry.name()) else {
            return Err(format!("Refusing unsafe zip entry `{}`", entry.name()));
        };
        let target = dest.join(path);
        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| format!("Failed to create {}: {e}", target.display()))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        let mut out = fs::File::create(&target)
            .map_err(|e| format!("Failed to create {}: {e}", target.display()))?;
        io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Failed to write {}: {e}", target.display()))?;
        count += 1;
    }
    Ok(count)
}

fn extract_tar_stream<R: io::Read + ?Sized>(reader: Box<R>, dest: &Path) -> Result<u64, String> {
    let mut archive = tar::Archive::new(reader);
    let mut count = 0u64;
    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {e}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("Failed to read tar entry: {e}"))?;
        let name = entry
            .path()
            .map_err(|e| format!("Failed to read tar entry path: {e}"))?
            .to_string_lossy()
            .replace('\\', "/");
        let Some(path) = safe_entry_path(&name) else {
            return Err(format!("Refusing unsafe tar entry `{name}`"));
        };
        let target = dest.join(path);
        if target.is_dir() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        entry
            .unpack(&target)
            .map_err(|e| format!("Failed to write {}: {e}", target.display()))?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_digests_compare_case_insensitively() {
        assert!(sha256_matches("AbCd", "aBcD"));
        assert!(!sha256_matches("abcd", "abce"));
        assert!(!sha256_matches("abcd", "abc"));
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        assert!(safe_entry_path("ok/file.txt").is_some());
        assert!(safe_entry_path("../escape").is_none());
        assert!(safe_entry_path("/absolute").is_none());
        assert!(safe_entry_path("a/../../escape").is_none());
        assert!(safe_entry_path("./ok").is_some());
    }

    #[test]
    fn formats_are_inferred_from_names() {
        assert_eq!(format_from_name("x.zip"), Some("zip"));
        assert_eq!(format_from_name("x.tar.xz"), Some("tar.xz"));
        assert_eq!(format_from_name("x.tgz"), Some("tar.gz"));
        assert_eq!(format_from_name("x.tar.zst"), Some("tar.zst"));
        assert_eq!(format_from_name("x.exe"), None);
    }

    #[test]
    fn sha256_of_known_bytes() {
        let dir = std::env::temp_dir().join("adesh-archive-test");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("known.txt");
        fs::write(&file, b"abc").unwrap();
        let digest = file_sha256(&file).unwrap();
        // SHA-256("abc")
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
