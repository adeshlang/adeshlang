use super::super::body::Body;
use super::super::errors::{HttpError, HttpErrorKind};
use super::super::response::Response;
use super::super::status::HttpStatus;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub fn serve_static_file(
    base_dir: &Path,
    req_path: &str,
    if_none_match: Option<&str>,
    range_hdr: Option<&str>,
) -> Result<Response, HttpError> {
    // 1. Path traversal defense
    let sanitized_rel = req_path.trim_start_matches('/');
    if sanitized_rel.contains("..") || sanitized_rel.contains('\\') {
        return Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            "Directory traversal detected and rejected",
        ));
    }

    let full_path = base_dir.join(sanitized_rel);
    let target = if full_path.is_dir() {
        full_path.join("index.html")
    } else {
        full_path
    };

    if !target.exists() || !target.is_file() {
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
            let mut buf = vec![0u8; chunk_len as usize];
            use std::io::Seek;
            let _ = file.seek(std::io::SeekFrom::Start(start));
            let _ = file.read_exact(&mut buf);

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

    let mut content = Vec::with_capacity(file_len as usize);
    file.read_to_end(&mut content)
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;

    let mut resp = Response::ok();
    let _ = resp.headers.set_content_type(mime);
    let _ = resp.headers.set_content_length(file_len);
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
    let lower = range_str.trim().to_ascii_lowercase();
    if !lower.starts_with("bytes=") {
        return None;
    }
    let val = &lower[6..].trim();
    let mut parts = val.split('-');
    let start_str = parts.next()?;
    let end_str = parts.next()?;

    if start_str.is_empty() {
        let suffix: u64 = end_str.parse().ok()?;
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
