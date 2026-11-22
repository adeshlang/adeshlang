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

/// Fetch a text resource (such as a JSON release manifest) from a URL.
///
/// Only `https://` is accepted. Plaintext `http://`, `file://`, and bare
/// filesystem paths are rejected to prevent MITM / local-file plant attacks.
pub fn fetch_text(url: &str) -> Result<String, String> {
    require_https_url(url)?;
    fetch_https_text_follow(url, 0)
}

/// Download a binary file from a URL to a local destination file.
/// Only `https://` is accepted (see [`fetch_text`]).
pub fn download_file(
    url: &str,
    dest_path: &Path,
    expected_size: Option<u64>,
) -> Result<(), String> {
    require_https_url(url)?;
    if let Some(parent) = dest_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    download_https_file_follow(url, dest_path, expected_size, 0)
}

fn require_https_url(url: &str) -> Result<(), String> {
    if url.starts_with("https://") {
        Ok(())
    } else if url.starts_with("http://") {
        Err("Insecure http:// update URLs are rejected; use https://".to_string())
    } else if url.starts_with("file://") || Path::new(url).exists() {
        Err("Local file:// / bare-path update sources are rejected; use https://".to_string())
    } else {
        Err(format!("Unsupported update URL scheme (https:// required): {url}"))
    }
}

const MAX_REDIRECTS: u8 = 5;
const MAX_HEADER_BYTES: usize = 64 * 1024;

// -----------------------------------------------------------------------------
// HTTPS Implementation using native rustls
// -----------------------------------------------------------------------------

fn build_tls_config() -> Result<Arc<ClientConfig>, String> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

fn parse_url(url_str: &str) -> Result<(String, u16, String), String> {
    let rest = if let Some(s) = url_str.strip_prefix("https://") {
        s
    } else if let Some(s) = url_str.strip_prefix("http://") {
        s
    } else {
        return Err(format!("Invalid URL: {url_str}"));
    };

    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };

    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let port_num: u16 = p.parse().map_err(|e| format!("Invalid port in URL: {e}"))?;
        (h.to_string(), port_num)
    } else {
        let default_port = if url_str.starts_with("https://") {
            443
        } else {
            80
        };
        (host_port.to_string(), default_port)
    };

    Ok((host, port, path.to_string()))
}

fn connect_tls(url: &str) -> Result<(String, StreamOwned<ClientConnection, TcpStream>), String> {
    let (host, port, path) = parse_url(url)?;
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
    Ok((path, StreamOwned::new(conn, sock)))
}

fn fetch_https_text_follow(url: &str, depth: u8) -> Result<String, String> {
    if depth > MAX_REDIRECTS {
        return Err(format!("Exceeded max HTTPS redirects ({MAX_REDIRECTS})"));
    }
    require_https_url(url)?;
    let (host, _port, _) = parse_url(url)?;
    let (path, mut tls_stream) = connect_tls(url)?;

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

    let mut response_bytes = Vec::new();
    tls_stream
        .read_to_end(&mut response_bytes)
        .map_err(|e| format!("Failed reading response: {e}"))?;

    match parse_http_response(&response_bytes)? {
        HttpOutcome::Ok(body) => Ok(String::from_utf8_lossy(&body).to_string()),
        HttpOutcome::Redirect(loc) => {
            let next = resolve_redirect_url(url, &loc)?;
            fetch_https_text_follow(&next, depth + 1)
        }
    }
}

enum HttpOutcome {
    Ok(Vec<u8>),
    Redirect(String),
}

fn parse_http_response(response: &[u8]) -> Result<HttpOutcome, String> {
    let sep = b"\r\n\r\n";
    let pos = response
        .windows(sep.len())
        .position(|w| w == sep)
        .ok_or_else(|| "Malformed HTTP response header".to_string())?;
    if pos > MAX_HEADER_BYTES {
        return Err(format!(
            "HTTP response headers exceed {MAX_HEADER_BYTES} byte limit"
        ));
    }
    let headers_str = String::from_utf8_lossy(&response[..pos]);
    let status_line = headers_str.lines().next().unwrap_or("");
    let body_bytes = response[pos + sep.len()..].to_vec();

    if status_line.contains(" 301 ")
        || status_line.contains(" 302 ")
        || status_line.contains(" 303 ")
        || status_line.contains(" 307 ")
        || status_line.contains(" 308 ")
    {
        let location = headers_str
            .lines()
            .find_map(|line| {
                let lower = line.to_ascii_lowercase();
                lower
                    .strip_prefix("location:")
                    .map(|v| v.trim().to_string())
            })
            .ok_or_else(|| "Redirect response missing Location header".to_string())?;
        return Ok(HttpOutcome::Redirect(location));
    }

    if !status_line.contains(" 200 ")
        && !status_line.starts_with("HTTP/1.1 200")
        && !status_line.starts_with("HTTP/1.0 200")
    {
        return Err(format!("HTTP request failed with status: {status_line}"));
    }

    Ok(HttpOutcome::Ok(body_bytes))
}

fn resolve_redirect_url(current: &str, location: &str) -> Result<String, String> {
    if location.starts_with("https://") {
        return Ok(location.to_string());
    }
    if location.starts_with("http://") {
        return Err("Insecure http:// redirect rejected during update download".to_string());
    }
    // Relative redirect — resolve against current HTTPS origin.
    let (host, port, path) = parse_url(current)?;
    let base = if location.starts_with('/') {
        location.to_string()
    } else {
        let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        format!("{parent}/{location}")
    };
    let port_suffix = if port == 443 {
        String::new()
    } else {
        format!(":{port}")
    };
    Ok(format!("https://{host}{port_suffix}{base}"))
}

fn download_https_file_follow(
    url: &str,
    dest: &Path,
    expected_size: Option<u64>,
    depth: u8,
) -> Result<(), String> {
    if depth > MAX_REDIRECTS {
        return Err(format!("Exceeded max HTTPS redirects ({MAX_REDIRECTS})"));
    }
    require_https_url(url)?;
    let (host, _port, _) = parse_url(url)?;
    let (path, mut stream) = connect_tls(url)?;

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

    match stream_response_to_file(&mut stream, dest, expected_size)? {
        None => Ok(()),
        Some(loc) => {
            let next = resolve_redirect_url(url, &loc)?;
            download_https_file_follow(&next, dest, expected_size, depth + 1)
        }
    }
}

/// Returns `Ok(None)` on success, `Ok(Some(location))` when a redirect must be followed.
fn stream_response_to_file<R: Read>(
    reader: &mut R,
    dest: &Path,
    expected_size: Option<u64>,
) -> Result<Option<String>, String> {
    let mut header_buf = Vec::new();
    let mut byte = [0u8; 1];
    let sep = b"\r\n\r\n";

    // Read headers byte by byte to find end of headers (capped).
    loop {
        if header_buf.len() > MAX_HEADER_BYTES {
            return Err(format!(
                "HTTP response headers exceed {MAX_HEADER_BYTES} byte limit"
            ));
        }
        if reader.read_exact(&mut byte).is_err() {
            return Err("Unexpected EOF reading HTTP response headers".to_string());
        }
        header_buf.push(byte[0]);
        if header_buf.len() >= 4 && header_buf.ends_with(sep) {
            break;
        }
    }

    let headers_str = String::from_utf8_lossy(&header_buf);
    let status_line = headers_str.lines().next().unwrap_or("");

    if status_line.contains(" 301 ")
        || status_line.contains(" 302 ")
        || status_line.contains(" 303 ")
        || status_line.contains(" 307 ")
        || status_line.contains(" 308 ")
    {
        let location = headers_str
            .lines()
            .find_map(|line| {
                let lower = line.to_ascii_lowercase();
                lower
                    .strip_prefix("location:")
                    .map(|v| v.trim().to_string())
            })
            .ok_or_else(|| "Redirect response missing Location header".to_string())?;
        return Ok(Some(location));
    }

    if !status_line.contains(" 200 ")
        && !status_line.starts_with("HTTP/1.1 200")
        && !status_line.starts_with("HTTP/1.0 200")
    {
        return Err(format!("Download failed with status: {status_line}"));
    }

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
            return Err(format!(
                "Download incomplete: expected {exp} bytes, received {total_read} bytes"
            ));
        }
    }

    Ok(None)
}
