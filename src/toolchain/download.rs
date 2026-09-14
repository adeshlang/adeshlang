//! Minimal HTTPS downloader used by `adesh toolchain install`.
//!
//! Streams directly from rustls (already a dependency) to disk while hashing,
//! with redirect handling and chunked transfer-encoding support. No new
//! dependencies are introduced.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use rustls_pki_types::ServerName;
use sha2::{Digest, Sha256};

const MAX_REDIRECTS: usize = 8;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const READ_TIMEOUT: Duration = Duration::from_secs(180);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct DownloadResult {
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
struct Url {
    host: String,
    port: u16,
    path_and_query: String,
}

fn parse_url(input: &str) -> Result<Url, String> {
    let rest = input
        .strip_prefix("https://")
        .ok_or_else(|| format!("Toolchain downloads require https:// URLs (got `{input}`)"))?;
    let (authority, path_and_query) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    // Strip userinfo.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        // IPv6 literal: [::1]:443
        let end = stripped
            .find(']')
            .ok_or_else(|| format!("Malformed host in URL `{input}`"))?;
        let host = stripped[..end].to_string();
        let port = stripped[end + 1..]
            .strip_prefix(':')
            .map(|p| p.parse::<u16>())
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(443);
        (host, port)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (
            host.to_string(),
            port.parse::<u16>()
                .map_err(|e| format!("Invalid port in URL `{input}`: {e}"))?,
        )
    } else {
        (authority.to_string(), 443)
    };
    if host.is_empty() {
        return Err(format!("Malformed URL `{input}`"));
    }
    Ok(Url {
        host,
        port,
        path_and_query: path_and_query.to_string(),
    })
}

fn root_store() -> &'static RootCertStore {
    static ROOTS: std::sync::OnceLock<RootCertStore> = std::sync::OnceLock::new();
    ROOTS.get_or_init(|| {
        let mut roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        // Augment with the platform trust store (enterprise CAs etc.).
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(certs) = rustls_native_certs::load_native_certs() {
            for cert in certs {
                let _ = roots.add(cert);
            }
        }
        roots
    })
}

fn tls_config() -> Arc<ClientConfig> {
    static CONFIG: std::sync::OnceLock<Arc<ClientConfig>> = std::sync::OnceLock::new();
    CONFIG
        .get_or_init(|| {
            Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(root_store().clone())
                    .with_no_client_auth(),
            )
        })
        .clone()
}

fn connect(url: &Url) -> Result<StreamOwned<ClientConnection, TcpStream>, String> {
    let addrs = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| format!("Failed to resolve {}: {e}", url.host))?;
    let mut last_err = String::from("no addresses");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
                let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                let server_name = ServerName::try_from(url.host.clone())
                    .map_err(|e| format!("Invalid server name `{}`: {e}", url.host))?;
                let conn =
                    ClientConnection::new(tls_config(), server_name).map_err(|e| e.to_string())?;
                return Ok(StreamOwned::new(conn, stream));
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(format!(
        "Failed to connect to {}:{}: {last_err}",
        url.host, url.port
    ))
}

struct Response {
    status: u16,
    headers: HashMap<String, String>,
}

fn read_line(stream: &mut StreamOwned<ClientConnection, TcpStream>) -> Result<String, String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] != b'\r' {
            line.push(byte[0]);
        }
    }
    Ok(String::from_utf8_lossy(&line).trim().to_string())
}

fn read_headers(stream: &mut StreamOwned<ClientConnection, TcpStream>) -> Result<Response, String> {
    let status_line = read_line(stream)?;
    let status = status_line
        .split_ascii_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("Malformed HTTP status line `{status_line}`"))?;
    let mut headers = HashMap::new();
    loop {
        let line = read_line(stream)?;
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    Ok(Response { status, headers })
}

fn fmt_mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// Download `url` to `dest`, hashing while streaming. Prints milestone progress
/// lines (25/50/75/100%) so output stays usable in both TTYs and logs.
pub fn download_to(url: &str, dest: &Path) -> Result<DownloadResult, String> {
    let mut current = parse_url(url)?;
    for _ in 0..=MAX_REDIRECTS {
        let mut stream = connect(&current)?;
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: adeshlang/{} (+adesh toolchain install)\r\nAccept: */*\r\nConnection: close\r\n\r\n",
            current.path_and_query,
            current.host,
            env!("CARGO_PKG_VERSION")
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("Failed to send request: {e}"))?;
        stream.flush().ok();

        let response = read_headers(&mut stream)?;
        if [301, 302, 303, 307, 308].contains(&response.status) {
            let location = response
                .headers
                .get("location")
                .ok_or_else(|| "Redirect without Location header".to_string())?;
            let next = resolve_redirect(&current, location)?;
            current = next;
            continue;
        }
        if response.status != 200 {
            return Err(format!("HTTP {} fetching {url}", response.status));
        }

        let chunked = response
            .headers
            .get("transfer-encoding")
            .map(|v| v.eq_ignore_ascii_case("chunked"))
            .unwrap_or(false);
        let content_length: Option<u64> = response
            .headers
            .get("content-length")
            .and_then(|v| v.parse().ok());
        if let Some(len) = content_length {
            println!("  Downloading {url} ({})", fmt_mb(len));
        } else {
            println!("  Downloading {url} (size unknown)");
        }

        let mut hasher = Sha256::new();
        let mut file = std::fs::File::create(dest)
            .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
        let mut total: u64 = 0;
        let mut next_milestone: u64 = 25;

        let mut body_chunk = |buf: &[u8]| -> Result<(), String> {
            if buf.is_empty() {
                return Ok(());
            }
            hasher.update(buf);
            file.write_all(buf)
                .map_err(|e| format!("Failed writing {}: {e}", dest.display()))?;
            total += buf.len() as u64;
            if let Some(len) = content_length {
                let pct = total * 100 / len.max(1);
                if pct >= next_milestone {
                    println!("    [{}%] {} / {}", pct, fmt_mb(total), fmt_mb(len));
                    next_milestone = pct + 25;
                }
            }
            Ok(())
        };

        if chunked {
            loop {
                let size_line = read_line(&mut stream)?;
                if size_line.is_empty() {
                    continue;
                }
                let size =
                    usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
                        .map_err(|_| format!("Malformed chunk size `{size_line}`"))?;
                if size == 0 {
                    // Trailers until blank line.
                    while !read_line(&mut stream)?.is_empty() {}
                    break;
                }
                let mut remaining = size;
                let mut buf = [0u8; 64 * 1024];
                while remaining > 0 {
                    let want = buf.len().min(remaining);
                    let n = stream
                        .read(&mut buf[..want])
                        .map_err(|e| format!("read chunk: {e}"))?;
                    if n == 0 {
                        return Err("Connection closed mid-chunk".to_string());
                    }
                    body_chunk(&buf[..n])?;
                    remaining -= n;
                }
                // Consume chunk terminator CRLF.
                read_line(&mut stream)?;
            }
        } else if let Some(len) = content_length {
            let mut remaining = len;
            let mut buf = [0u8; 64 * 1024];
            while remaining > 0 {
                let want = buf.len().min(remaining as usize);
                let n = stream
                    .read(&mut buf[..want])
                    .map_err(|e| format!("read: {e}"))?;
                if n == 0 {
                    return Err(format!("Connection closed early at {total} of {len} bytes"));
                }
                body_chunk(&buf[..n])?;
                remaining -= n as u64;
            }
        } else {
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) => return Err(format!("read: {e}")),
                };
                body_chunk(&buf[..n])?;
            }
        }

        file.flush().map_err(|e| format!("flush: {e}"))?;
        let digest = hasher.finalize();
        let mut hex = String::with_capacity(64);
        for b in digest {
            hex.push_str(&format!("{b:02x}"));
        }
        return Ok(DownloadResult {
            sha256: hex,
            bytes: total,
        });
    }
    Err(format!("Too many redirects fetching {url}"))
}

fn resolve_redirect(base: &Url, location: &str) -> Result<Url, String> {
    if location.starts_with("https://") {
        parse_url(location)
    } else if location.starts_with('/') {
        Ok(Url {
            host: base.host.clone(),
            port: base.port,
            path_and_query: location.to_string(),
        })
    } else {
        // Relative path: merge with the base path's directory.
        let dir = match base.path_and_query.rfind('/') {
            Some(i) => &base.path_and_query[..i],
            None => "",
        };
        Ok(Url {
            host: base.host.clone(),
            port: base.port,
            path_and_query: format!("{dir}/{location}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_urls() {
        let u = parse_url("https://github.com/a/b/releases/download/v1/x.zip?token=1").unwrap();
        assert_eq!(u.host, "github.com");
        assert_eq!(u.port, 443);
        assert_eq!(u.path_and_query, "/a/b/releases/download/v1/x.zip?token=1");
    }

    #[test]
    fn parses_urls_with_ports() {
        let u = parse_url("https://example.com:8443/path").unwrap();
        assert_eq!(u.port, 8443);
    }

    #[test]
    fn rejects_plain_http() {
        assert!(parse_url("http://example.com/x").is_err());
    }

    #[test]
    fn resolves_absolute_and_relative_redirects() {
        let base = parse_url("https://example.com/a/b").unwrap();
        assert_eq!(
            resolve_redirect(&base, "/c/d").unwrap().path_and_query,
            "/c/d"
        );
        assert_eq!(
            resolve_redirect(&base, "https://other.com/e").unwrap().host,
            "other.com"
        );
    }
}
