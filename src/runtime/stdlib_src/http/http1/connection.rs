use super::super::errors::{HttpError, HttpErrorKind};
use super::super::request::Request;
use super::super::response::Response;
use super::encoder::encode_request;
use super::parser::Http1Parser;
use std::io::{Read, Write};
use std::net::TcpStream;

pub enum Http1Stream {
    Plain(TcpStream),
    #[cfg(feature = "default")]
    Tls(Box<dyn TlsStreamReadWrite>),
    Custom(Box<dyn ReadWriteSendSync>),
}

pub trait ReadWriteSendSync: Read + Write + Send + Sync {}
impl<T: Read + Write + Send + Sync> ReadWriteSendSync for T {}

pub trait TlsStreamReadWrite: Read + Write + Send + Sync {}
impl<T: Read + Write + Send + Sync> TlsStreamReadWrite for T {}

pub struct Http1Connection {
    pub stream: Box<dyn ReadWriteSendSync>,
    pub parser: Http1Parser,
    pub buffer: Vec<u8>,
}

impl Http1Connection {
    pub fn new(stream: Box<dyn ReadWriteSendSync>) -> Self {
        Self {
            stream,
            parser: Http1Parser::new(),
            buffer: Vec::with_capacity(8192),
        }
    }

    pub fn send_request(&mut self, req: &Request) -> Result<Response, HttpError> {
        let wire = encode_request(req)?;
        self.stream.write_all(&wire).map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to write request: {}", e),
            )
        })?;
        self.stream.flush().map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to flush stream: {}", e),
            )
        })?;

        self.read_response()
    }

    pub fn read_response(&mut self) -> Result<Response, HttpError> {
        let mut temp = [0u8; 8192];
        loop {
            // Try parsing what we currently have in self.buffer
            match self.parser.parse_response(&self.buffer) {
                Ok((resp, consumed)) => {
                    self.buffer.drain(..consumed);
                    return Ok(resp);
                }
                Err(e)
                    if e.kind == HttpErrorKind::Http1Error && e.message.contains("Incomplete") =>
                {
                    // Need more bytes from socket
                    let n = self.stream.read(&mut temp).map_err(|e| {
                        HttpError::new(HttpErrorKind::IoError, format!("Socket read error: {}", e))
                    })?;
                    if n == 0 {
                        // EOF reached; try parsing whatever is left
                        if self.buffer.is_empty() {
                            return Err(HttpError::new(
                                HttpErrorKind::ConnectError,
                                "Connection closed by remote peer before response received",
                            ));
                        }
                        let (resp, _) = self.parser.parse_response(&self.buffer)?;
                        self.buffer.clear();
                        return Ok(resp);
                    }
                    self.buffer.extend_from_slice(&temp[..n]);
                }
                Err(e) => return Err(e),
            }
        }
    }
}
