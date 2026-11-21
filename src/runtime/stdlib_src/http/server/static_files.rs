use super::super::body::Body;
use super::super::errors::{HttpError, HttpErrorKind};
use super::super::response::Response;
use super::super::status::HttpStatus;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Maximum file/range size buffered into memory by `serve_static_file` (64 MiB).
const MAX_STATIC_FILE_BYTES: u64 = 64 * 1024 * 1024;

pub fn serve_static_file(
    base_dir: &Path,
    req_path: &str,
    if_none_match: Option<&str>,
    range_hdr: Option<&str>,
) -> Result<Response, HttpError> {
    // 1. Path traversal defense (reject null bytes, traversal segments, backslashes, prefixes)
    if req_path.contains('\0') || req_path.contains('\\') {
        return Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            "Directory traversal detected and rejected",
        ));
    }

    let sanitized_rel = req_path.trim_start_matches('/');
    let rel_path = Path::new(sanitized_rel);
    for comp in rel_path.components() {
        match comp {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    "Directory traversal detected and rejected",
                ));
            }
            _ => {}
        }
    }

    let canonical_base = base_dir
        .canonicalize()
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;

    let full_path = canonical_base.join(rel_path);
    let candidate = if full_path.is_dir() {
        full_path.join("index.html")
    } else {
        full_path
    };

    if !candidate.exists() {
        return Ok(Response::not_found());
    }

    // Canonicalize the target and verify it stays strictly within canonical_base
    // (blocks symlink escapes, URL-normalized segments, and drive-relative tricks).
    let target = candidate
        .canonicalize()
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;
    if !target.starts_with(&canonical_base) {
        return Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            "Resolved path escapes static root directory",
        ));
    }

    if !target.is_file() {
        return Ok(Response::not_found());
    }

    let metadata = std::fs::metadata(&target)
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;

    let file_len = metadata.len();
    let mod_time = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let etag = format!("\"{:x}-{:x}\"", file_len, mod_time);

    // 2. Conditional check (304 Not Modified)
    if let Some(inm) = if_none_match {
        if inm.trim() == etag || inm.trim() == "*" {
            let mut resp = Response::new(HttpStatus::NOT_MODIFIED);
            let _ = resp.headers.insert("etag", &etag);
            return Ok(resp);
        }
    }

    let mime = mime_guess_from_path(&target);
    let mut file =
        File::open(&target).map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;

    // 3. Range request handling (206 Partial Content)
    if let Some(range) = range_hdr {
        if let Some((start, end)) = parse_byte_range(range, file_len) {
            let chunk_len = end - start + 1;
            if chunk_len > MAX_STATIC_FILE_BYTES {
                return Err(HttpError::new(
                    HttpErrorKind::BodyTooLarge,
                    format!(
                        "Requested byte range ({} bytes) exceeds maximum buffered size ({} bytes)",
                        chunk_len, MAX_STATIC_FILE_BYTES
                    ),
                ));
            }
            let mut buf = vec![0u8; chunk_len as usize];
            use std::io::Seek;
            file.seek(std::io::SeekFrom::Start(start))
                .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;
            file.read_exact(&mut buf)
                .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;

            let mut resp = Response::new(HttpStatus::PARTIAL_CONTENT);
            let _ = resp.headers.set_content_type(mime);
            let _ = resp.headers.set_content_length(chunk_len);
            let _ = resp.headers.insert("etag", &etag);
            let _ = resp.headers.insert("accept-ranges", "bytes");
            let _ = resp.headers.insert(
                "content-range",
                &format!("bytes {}-{}/{}", start, end, file_len),
            );
            resp.body = Body::from_bytes(buf);
            return Ok(resp);
        }
    }

    if file_len > MAX_STATIC_FILE_BYTES {
        return Err(HttpError::new(
            HttpErrorKind::BodyTooLarge,
            format!(
                "Static file size ({} bytes) exceeds maximum buffered size ({} bytes)",
                file_len, MAX_STATIC_FILE_BYTES
            ),
        ));
    }

    let mut content = Vec::with_capacity(file_len as usize);
    file.take(MAX_STATIC_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut content)
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;
    if content.len() as u64 > MAX_STATIC_FILE_BYTES {
        return Err(HttpError::new(
            HttpErrorKind::BodyTooLarge,
            "Static file grew past maximum buffered size while reading",
        ));
    }

    let mut resp = Response::ok();
    let _ = resp.headers.set_content_type(mime);
    let _ = resp.headers.set_content_length(content.len() as u64);
    let _ = resp.headers.insert("etag", &etag);
    let _ = resp.headers.insert("accept-ranges", "bytes");
    resp.body = Body::from_bytes(content);

    Ok(resp)
}

fn mime_guess_from_path(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn parse_byte_range(range_str: &str, file_len: u64) -> Option<(u64, u64)> {
    // An empty file has no satisfiable byte range (prevents file_len - 1 underflow).
    if file_len == 0 {
        return None;
    }
    let lower = range_str.trim().to_ascii_lowercase();
    if !lower.starts_with("bytes=") {
        return None;
    }
    let val = lower[6..].trim();
    let mut parts = val.split('-');
    let start_str = parts.next()?;
    let end_str = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    if start_str.is_empty() {
        let suffix: u64 = end_str.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = file_len.saturating_sub(suffix);
        Some((start, file_len - 1))
    } else if end_str.is_empty() {
        let start: u64 = start_str.parse().ok()?;
        if start >= file_len {
            return None;
        }
        Some((start, file_len - 1))
    } else {
        let start: u64 = start_str.parse().ok()?;
        let end: u64 = end_str.parse().ok()?;
        if start > end || start >= file_len {
            return None;
        }
        Some((start, end.min(file_len - 1)))
    }
}
