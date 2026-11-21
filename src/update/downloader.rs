//! Downloader module for release manifests and component zip archives.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use rustls_pki_types::ServerName;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_REDIRECTS: usize = 5;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

/// Fetch a text resource (such as a JSON release manifest) from an HTTPS URL or local file
pub fn fetch_text(url: &str) -> Result<String, String> {
    if let Some(file_path) = url.strip_prefix("file://") {
        return fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read local manifest file '{file_path}': {e}"));
    }

    if url.starts_with("http://") {
        return Err(format!(
            "Security violation: plaintext HTTP update URL rejected (HTTPS required): {url}"
        ));
    }

    if url.starts_with("https://") {
        return fetch_https_text(url);
    }

    // Direct path fallback
    if Path::new(url).exists() {
        return fs::read_to_string(url)
            .map_err(|e| format!("Failed to read manifest file '{url}': {e}"));
    }

    Err(format!("Unsupported manifest URL scheme: {url}"))
}

/// Download a binary file from an HTTPS URL or local file to a local destination file
pub fn download_file(
    url: &str,
    dest_path: &Path,
    expected_size: Option<u64>,
) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Some(file_path) = url.strip_prefix("file://") {
        fs::copy(file_path, dest_path)
            .map_err(|e| format!("Failed to copy local file '{file_path}': {e}"))?;
        return Ok(());
    }

    if Path::new(url).exists() {
        fs::copy(url, dest_path)
            .map_err(|e| format!("Failed to copy local archive '{url}': {e}"))?;
        return Ok(());
    }

    if url.starts_with("http://") {
        return Err(format!(
            "Security violation: plaintext HTTP download URL rejected (HTTPS required): {url}"
        ));
    }

    if url.starts_with("https://") {
        return download_https_file(url, dest_path, expected_size);
    }

    Err(format!("Unsupported download URL scheme: {url}"))
}

// -----------------------------------------------------------------------------
// HTTPS Implementation using native rustls (with redirect & header-size bounds)
// -----------------------------------------------------------------------------

fn build_tls_config() -> Result<Arc<ClientConfig>, String> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

fn parse_https_url(url_str: &str) -> Result<(String, u16, String), String> {
    let rest = url_str
        .strip_prefix("https://")
        .ok_or_else(|| format!("Security violation: HTTPS URL required, got '{url_str}'"))?;

    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };

    if host_port.contains('\r')
        || host_port.contains('\n')
        || path.contains('\r')
        || path.contains('\n')
    {
        return Err("Invalid control characters in URL".to_string());
    }

    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let port_num: u16 = p.parse().map_err(|e| format!("Invalid port in URL: {e}"))?;
        (h.to_string(), port_num)
    } else {
        (host_port.to_string(), 443)
    };

    if host.is_empty() {
        return Err(format!("Missing host in URL: {url_str}"));
    }

    Ok((host, port, path.to_string()))
}

fn connect_https(
    url: &str,
) -> Result<(StreamOwned<ClientConnection, TcpStream>, String, String), String> {
    let (host, port, path) = parse_https_url(url)?;
    let tls_config = build_tls_config()?;
    let server_name = ServerName::try_from(host.as_str())
        .map_err(|e| format!("Invalid DNS name '{host}': {e}"))?
        .to_owned();

    let addr = format!("{host}:{port}");
    let sock = TcpStream::connect_timeout(
        &addr
            .parse()
            .or_else(|_| {
                std::net::ToSocketAddrs::to_socket_addrs(&addr)?
                    .next()
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::NotFound, "Host lookup failed")
                    })
            })
            .map_err(|e| format!("Failed to connect to {addr}: {e}"))?,
        CONNECT_TIMEOUT,
    )
    .map_err(|e| format!("Connect timeout for {addr}: {e}"))?;

    sock.set_read_timeout(Some(READ_TIMEOUT)).ok();

    let conn = ClientConnection::new(tls_config, server_name)
        .map_err(|e| format!("TLS handshake initialization failed: {e}"))?;
    Ok((StreamOwned::new(conn, sock), host, path))
}

fn read_http_headers<R: Read>(reader: &mut R) -> Result<String, String> {
    let mut header_buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    let sep = b"\r\n\r\n";

    loop {
        if reader.read_exact(&mut byte).is_err() {
            return Err("Unexpected EOF reading HTTP response headers".to_string());
        }
        header_buf.push(byte[0]);
        if header_buf.len() > MAX_HEADER_BYTES {
            return Err(format!(
                "HTTP response headers exceeded maximum allowed size ({MAX_HEADER_BYTES} bytes)"
            ));
        }
        if header_buf.len() >= 4 && header_buf.ends_with(sep) {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&header_buf).into_owned())
}

fn parse_status_code(headers_str: &str) -> Result<u16, String> {
    let status_line = headers_str
        .lines()
        .next()
        .ok_or_else(|| "Missing HTTP status line".to_string())?;
    let code_str = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("Malformed HTTP status line: {status_line}"))?;
    code_str
        .parse::<u16>()
        .map_err(|_| format!("Invalid HTTP status code in: {status_line}"))
}

fn extract_redirect_location(headers_str: &str, current_url: &str) -> Result<String, String> {
    for line in headers_str.lines().skip(1) {
        if let Some((name, val)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("location") {
                let loc = val.trim();
                if loc.starts_with("https://") {
                    return Ok(loc.to_string());
                } else if loc.starts_with("http://") {
                    return Err(format!(
                        "Security violation: HTTPS -> HTTP redirect downgrade blocked ({loc})"
                    ));
                } else if loc.starts_with('/') {
                    let (host, port, _) = parse_https_url(current_url)?;
                    if port == 443 {
                        return Ok(format!("https://{host}{loc}"));
                    } else {
                        return Ok(format!("https://{host}:{port}{loc}"));
                    }
                } else {
                    return Err(format!(
                        "Security violation: unsupported redirect target '{loc}'"
                    ));
                }
            }
        }
    }
    Err("HTTP redirect response missing Location header".to_string())
}

fn fetch_https_text(url: &str) -> Result<String, String> {
    let mut current_url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let (mut tls_stream, host, path) = connect_https(&current_url)?;
        let req = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: Adesh-Updater/{}\r\n\
             Accept: application/json, text/plain\r\n\
             Connection: close\r\n\r\n",
            env!("CARGO_PKG_VERSION")
        );

        tls_stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("Failed to write HTTP request: {e}"))?;

        let headers_str = read_http_headers(&mut tls_stream)?;
        let status = parse_status_code(&headers_str)?;

        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            current_url = extract_redirect_location(&headers_str, &current_url)?;
            continue;
        }

        if status != 200 {
            return Err(format!(
                "HTTP request failed with status: {}",
                headers_str.lines().next().unwrap_or("Unknown")
            ));
        }

        let mut body_bytes = Vec::new();
        tls_stream
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut body_bytes)
            .map_err(|e| format!("Failed reading response: {e}"))?;
        if body_bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(format!(
                "Manifest response exceeded maximum allowed size ({MAX_MANIFEST_BYTES} bytes)"
            ));
        }

        return Ok(String::from_utf8_lossy(&body_bytes).to_string());
    }

    Err(format!(
        "Exceeded maximum redirect limit ({MAX_REDIRECTS}) fetching {url}"
    ))
}

fn download_https_file(url: &str, dest: &Path, expected_size: Option<u64>) -> Result<(), String> {
    let mut current_url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let (mut stream, host, path) = connect_https(&current_url)?;
        let req = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: Adesh-Updater/{}\r\n\
             Connection: close\r\n\r\n",
            env!("CARGO_PKG_VERSION")
        );

        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("Failed sending request: {e}"))?;

        let headers_str = read_http_headers(&mut stream)?;
        let status = parse_status_code(&headers_str)?;

        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            current_url = extract_redirect_location(&headers_str, &current_url)?;
            continue;
        }

        if status != 200 {
            return Err(format!(
                "Download failed with status: {}",
                headers_str.lines().next().unwrap_or("Unknown")
            ));
        }

        return stream_body_to_file(&mut stream, dest, expected_size);
    }

    Err(format!(
        "Exceeded maximum redirect limit ({MAX_REDIRECTS}) downloading {url}"
    ))
}

fn stream_body_to_file<R: Read>(
    reader: &mut R,
    dest: &Path,
    expected_size: Option<u64>,
) -> Result<(), String> {
    let mut out_file = File::create(dest)
        .map_err(|e| format!("Failed creating destination file '{}': {e}", dest.display()))?;

    let mut chunk_buf = [0u8; 64 * 1024];
    let mut total_read = 0u64;

    loop {
        let n = reader
            .read(&mut chunk_buf)
            .map_err(|e| format!("Read error during download: {e}"))?;
        if n == 0 {
            break;
        }
        out_file
            .write_all(&chunk_buf[..n])
            .map_err(|e| format!("Write error during download: {e}"))?;
        total_read += n as u64;
    }

    if let Some(exp) = expected_size {
        if exp > 0 && total_read != exp {
            let _ = fs::remove_file(dest);
            return Err(format!(
                "Download incomplete: expected {exp} bytes, received {total_read} bytes"
            ));
        }
    }

    Ok(())
}
