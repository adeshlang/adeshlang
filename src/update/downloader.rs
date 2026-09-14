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

/// Fetch a text resource (such as a JSON release manifest) from a URL or local file
pub fn fetch_text(url: &str) -> Result<String, String> {
    if let Some(file_path) = url.strip_prefix("file://") {
        return fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read local manifest file '{file_path}': {e}"));
    }

    if url.starts_with("http://") {
        return fetch_http_text(url);
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

/// Download a binary file from a URL to a local destination file
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
        return download_http_file(url, dest_path, expected_size);
    }

    if url.starts_with("https://") {
        return download_https_file(url, dest_path, expected_size);
    }

    Err(format!("Unsupported download URL scheme: {url}"))
}

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

fn fetch_https_text(url: &str) -> Result<String, String> {
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
    let mut tls_stream = StreamOwned::new(conn, sock);

    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: AdeshLang-Updater/{}\r\n\
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

    extract_http_body(&response_bytes)
}

fn fetch_http_text(url: &str) -> Result<String, String> {
    let (host, port, path) = parse_url(url)?;
    let addr = format!("{host}:{port}");
    let mut sock = TcpStream::connect_timeout(
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

    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: AdeshLang-Updater/{}\r\n\
         Accept: application/json, text/plain\r\n\
         Connection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    );

    sock.write_all(req.as_bytes())
        .map_err(|e| format!("Failed writing HTTP request: {e}"))?;

    let mut response_bytes = Vec::new();
    sock.read_to_end(&mut response_bytes)
        .map_err(|e| format!("Failed reading response: {e}"))?;

    extract_http_body(&response_bytes)
}

fn extract_http_body(response: &[u8]) -> Result<String, String> {
    let sep = b"\r\n\r\n";
    if let Some(pos) = response.windows(sep.len()).position(|w| w == sep) {
        let headers_str = String::from_utf8_lossy(&response[..pos]);
        let body_bytes = &response[pos + sep.len()..];

        if !headers_str.contains(" 200 ")
            && !headers_str.starts_with("HTTP/1.1 200")
            && !headers_str.starts_with("HTTP/1.0 200")
        {
            return Err(format!(
                "HTTP request failed with status: {}",
                headers_str.lines().next().unwrap_or("Unknown")
            ));
        }

        Ok(String::from_utf8_lossy(body_bytes).to_string())
    } else {
        Err("Malformed HTTP response header".to_string())
    }
}

fn download_https_file(url: &str, dest: &Path, expected_size: Option<u64>) -> Result<(), String> {
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
    let mut stream = StreamOwned::new(conn, sock);

    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: AdeshLang-Updater/{}\r\n\
         Connection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    );

    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("Failed sending request: {e}"))?;

    stream_response_to_file(&mut stream, dest, expected_size)
}

fn download_http_file(url: &str, dest: &Path, expected_size: Option<u64>) -> Result<(), String> {
    let (host, port, path) = parse_url(url)?;
    let addr = format!("{host}:{port}");
    let mut sock = TcpStream::connect_timeout(
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

    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: AdeshLang-Updater/{}\r\n\
         Connection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    );

    sock.write_all(req.as_bytes())
        .map_err(|e| format!("Failed sending request: {e}"))?;

    stream_response_to_file(&mut sock, dest, expected_size)
}

fn stream_response_to_file<R: Read>(
    reader: &mut R,
    dest: &Path,
    expected_size: Option<u64>,
) -> Result<(), String> {
    let mut header_buf = Vec::new();
    let mut byte = [0u8; 1];
    let sep = b"\r\n\r\n";

    // Read headers byte by byte to find end of headers
    loop {
        if reader.read_exact(&mut byte).is_err() {
            return Err("Unexpected EOF reading HTTP response headers".to_string());
        }
        header_buf.push(byte[0]);
        if header_buf.len() >= 4 && header_buf.ends_with(sep) {
            break;
        }
    }

    let headers_str = String::from_utf8_lossy(&header_buf);
    if !headers_str.contains(" 200 ")
        && !headers_str.starts_with("HTTP/1.1 200")
        && !headers_str.starts_with("HTTP/1.0 200")
    {
        return Err(format!(
            "Download failed with status: {}",
            headers_str.lines().next().unwrap_or("Unknown")
        ));
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

    Ok(())
}
