use super::super::errors::{HttpError, HttpErrorKind};
use super::super::response::Response;
use std::net::TcpStream;

pub struct ForwardProxy;

impl ForwardProxy {
    pub fn handle_connect(target_host: &str, target_port: u16) -> Result<Response, HttpError> {
        let stream = TcpStream::connect((target_host, target_port)).map_err(|e| {
            HttpError::new(
                HttpErrorKind::ProxyError,
                format!(
                    "Failed to connect to tunnel destination {}:{}: {}",
                    target_host, target_port, e
                ),
            )
        })?;
        drop(stream);

        // 200 Connection Established
        Ok(Response::ok())
    }
}
