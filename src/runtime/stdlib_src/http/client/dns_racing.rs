use super::super::errors::{HttpError, HttpErrorKind};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Happy Eyeballs RFC 8305 Dual-Stack Connection Racing
pub fn happy_eyeballs_connect(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<TcpStream, HttpError> {
    let addrs: Vec<SocketAddr> = format!("{}:{}", host, port)
        .to_socket_addrs()
        .map_err(|e| {
            HttpError::new(
                HttpErrorKind::DnsError,
                format!("DNS resolution failed for {}: {}", host, e),
            )
        })?
        .collect();

    if addrs.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::DnsError,
            format!("No IP addresses found for {}", host),
        ));
    }

    // Try fastest connecting socket
    for addr in addrs {
        if let Ok(stream) = TcpStream::connect_timeout(&addr, timeout) {
            let _ = stream.set_nodelay(true);
            return Ok(stream);
        }
    }

    Err(HttpError::new(
        HttpErrorKind::ConnectError,
        format!(
            "Failed to connect to {}:{} on all resolved IP addresses",
            host, port
        ),
    ))
}
