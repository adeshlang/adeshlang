use super::super::errors::{HttpError, HttpErrorKind};
use super::super::http1::connection::ReadWriteSendSync;
use super::frame::{WebSocketFrame, WebSocketFrameDecodeConfig, WebSocketOpcode};
use std::io::{Read, Write};

use flate2::read::{DeflateDecoder, DeflateEncoder};
use flate2::Compression;

const DEFAULT_MAX_FRAME_SIZE: usize = 4 * 1024 * 1024;
const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close { code: u16, reason: String },
}

pub struct WebSocketStream {
    pub stream: Box<dyn ReadWriteSendSync>,
    pub is_client: bool,
    pub read_buffer: Vec<u8>,
    pub max_frame_size: usize,
    pub max_message_size: usize,
    pub automatic_pong: bool,
    pub compression_enabled: bool,
    close_sent: bool,
    close_received: bool,
    fragmented_opcode: Option<WebSocketOpcode>,
    fragmented_payload: Vec<u8>,
    fragmented_rsv1: bool,
}

impl WebSocketStream {
    pub fn new(stream: Box<dyn ReadWriteSendSync>, is_client: bool) -> Self {
        Self {
            stream,
            is_client,
            read_buffer: Vec::with_capacity(4096),
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
            automatic_pong: true,
            compression_enabled: false,
            close_sent: false,
            close_received: false,
            fragmented_opcode: None,
            fragmented_payload: Vec::new(),
            fragmented_rsv1: false,
        }
    }

    pub fn with_limits(
        stream: Box<dyn ReadWriteSendSync>,
        is_client: bool,
        max_frame_size: usize,
        max_message_size: usize,
    ) -> Self {
        let mut ws = Self::new(stream, is_client);
        ws.max_frame_size = max_frame_size;
        ws.max_message_size = max_message_size.max(max_frame_size);
        ws
    }

    pub fn send_text(&mut self, text: &str) -> Result<(), HttpError> {
        self.send_data(WebSocketOpcode::Text, text.as_bytes())
    }

    pub fn send_binary(&mut self, data: Vec<u8>) -> Result<(), HttpError> {
        self.send_data(WebSocketOpcode::Binary, &data)
    }

    pub fn send_data(&mut self, opcode: WebSocketOpcode, data: &[u8]) -> Result<(), HttpError> {
        if data.len() > self.max_message_size {
            return Err(HttpError::new(
                HttpErrorKind::BodyTooLarge,
                format!(
                    "Outbound WebSocket message size {} exceeds configured limit {}",
                    data.len(),
                    self.max_message_size
                ),
            ));
        }

        let (payload_bytes, is_compressed) = if self.compression_enabled
            && (opcode == WebSocketOpcode::Text || opcode == WebSocketOpcode::Binary)
        {
            (compress_deflate_payload(data)?, true)
        } else {
            (data.to_vec(), false)
        };

        if payload_bytes.len() <= self.max_frame_size {
            let frame = WebSocketFrame {
                fin: true,
                rsv1: is_compressed,
                rsv2: false,
                rsv3: false,
                opcode,
                mask: None,
                payload: payload_bytes,
            };
            return self.send_frame(&frame);
        }

        let chunks: Vec<&[u8]> = payload_bytes.chunks(self.max_frame_size).collect();
        let total_chunks = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == total_chunks - 1;
            let frame_opcode = if is_first { opcode } else { WebSocketOpcode::Continuation };
            let frame = WebSocketFrame {
                fin: is_last,
                rsv1: if is_first { is_compressed } else { false },
                rsv2: false,
                rsv3: false,
                opcode: frame_opcode,
                mask: None,
                payload: chunk.to_vec(),
            };
            self.send_frame(&frame)?;
        }
        Ok(())
    }

    pub fn send_ping(&mut self, payload: Vec<u8>) -> Result<(), HttpError> {
        self.send_frame(&WebSocketFrame::ping(payload))
    }

    pub fn send_pong(&mut self, payload: Vec<u8>) -> Result<(), HttpError> {
        self.send_frame(&WebSocketFrame::pong(payload))
    }

    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), HttpError> {
        validate_close_payload(code, reason.as_bytes())?;
        self.close_sent = true;
        self.send_frame(&WebSocketFrame::close(code, reason))
    }

    pub fn send_frame(&mut self, frame: &WebSocketFrame) -> Result<(), HttpError> {
        validate_outbound_frame(
        frame,
        self.is_client,
        self.max_frame_size,
        self.compression_enabled,
    )?;

        let mut outbound = frame.clone();
        if self.is_client {
            if outbound.mask.is_none() {
                outbound.mask = Some(rand::random());
            }
        } else if outbound.mask.is_some() {
            return Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                "Server WebSocket frames must not be masked",
            ));
        }

        let wire = outbound.encode();
        self.stream.write_all(&wire).map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to send WebSocket frame: {e}"),
            )
        })?;
        self.stream.flush().map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to flush WebSocket frame: {e}"),
            )
        })
    }

    pub fn receive_frame(&mut self) -> Result<WebSocketFrame, HttpError> {
        let mut temp = [0u8; 4096];
        loop {
            if let Some((frame, consumed)) =
                WebSocketFrame::decode_with_config(&self.read_buffer, self.decode_config())?
            {
                self.read_buffer.drain(..consumed);
                return Ok(frame);
            }

            let n = self.stream.read(&mut temp).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::IoError,
                    format!("WebSocket socket read error: {e}"),
                )
            })?;
            if n == 0 {
                return Err(HttpError::new(
                    HttpErrorKind::ConnectError,
                    "WebSocket connection closed by peer",
                ));
            }
            self.read_buffer.extend_from_slice(&temp[..n]);
        }
    }

    pub fn receive_message(&mut self) -> Result<WebSocketMessage, HttpError> {
        loop {
            let frame = self.receive_frame()?;
            match frame.opcode {
                WebSocketOpcode::Ping => {
                    if self.automatic_pong && !self.close_sent {
                        self.send_pong(frame.payload.clone())?;
                    }
                    return Ok(WebSocketMessage::Ping(frame.payload));
                }
                WebSocketOpcode::Pong => {
                    return Ok(WebSocketMessage::Pong(frame.payload));
                }
                WebSocketOpcode::Close => {
                    let (code, reason) = decode_close_payload(&frame.payload)?;
                    self.close_received = true;
                    if !self.close_sent {
                        self.close_sent = true;
                        let reply = if code == 1005 {
                            WebSocketFrame {
                                fin: true,
                                rsv1: false,
                                rsv2: false,
                                rsv3: false,
                                opcode: WebSocketOpcode::Close,
                                mask: None,
                                payload: Vec::new(),
                            }
                        } else {
                            WebSocketFrame::close(code, &reason)
                        };
                        self.send_frame(&reply)?;
                    }
                    return Ok(WebSocketMessage::Close { code, reason });
                }
                WebSocketOpcode::Text | WebSocketOpcode::Binary => {
                    if self.fragmented_opcode.is_some() {
                        return Err(protocol_error(
                            "Received a new data frame while a fragmented message is still in progress",
                        ));
                    }
                    if frame.payload.len() > self.max_message_size {
                        return Err(HttpError::new(
                            HttpErrorKind::BodyTooLarge,
                            "WebSocket message exceeds configured size limit",
                        ));
                    }
                    if frame.fin {
                        return self.into_message(frame.opcode, frame.payload, frame.rsv1);
                    }
                    self.fragmented_opcode = Some(frame.opcode);
                    self.fragmented_payload = frame.payload;
                    self.fragmented_rsv1 = frame.rsv1;
                }
                WebSocketOpcode::Continuation => {
                    let opcode = self.fragmented_opcode.ok_or_else(|| {
                        protocol_error("Received continuation frame without an active fragmented message")
                    })?;

                    let new_len = self
                        .fragmented_payload
                        .len()
                        .checked_add(frame.payload.len())
                        .ok_or_else(|| {
                            HttpError::new(
                                HttpErrorKind::BodyTooLarge,
                                "WebSocket fragmented message length overflow",
                            )
                        })?;
                    if new_len > self.max_message_size {
                        return Err(HttpError::new(
                            HttpErrorKind::BodyTooLarge,
                            "WebSocket fragmented message exceeds configured size limit",
                        ));
                    }

                    self.fragmented_payload.extend_from_slice(&frame.payload);
                    if frame.fin {
                        let payload = std::mem::take(&mut self.fragmented_payload);
                        let is_compressed = self.fragmented_rsv1;
                        self.fragmented_opcode = None;
                        self.fragmented_rsv1 = false;
                        return self.into_message(opcode, payload, is_compressed);
                    }
                }
            }
        }
    }

    fn decode_config(&self) -> WebSocketFrameDecodeConfig {
        WebSocketFrameDecodeConfig {
            expect_masked: Some(!self.is_client),
            max_frame_size: Some(self.max_frame_size),
            allow_reserved_bits: self.compression_enabled,
        }
    }

    fn into_message(
        &self,
        opcode: WebSocketOpcode,
        payload: Vec<u8>,
        is_compressed: bool,
    ) -> Result<WebSocketMessage, HttpError> {
        let raw_payload = if is_compressed {
            decompress_deflate_payload(&payload, self.max_message_size)?
        } else {
            payload
        };

        match opcode {
            WebSocketOpcode::Text => {
                let text = String::from_utf8(raw_payload).map_err(|_| {
                    HttpError::new(
                        HttpErrorKind::ProtocolError,
                        "WebSocket text message contained invalid UTF-8",
                    )
                })?;
                Ok(WebSocketMessage::Text(text))
            }
            WebSocketOpcode::Binary => Ok(WebSocketMessage::Binary(raw_payload)),
            _ => Err(protocol_error("Invalid opcode for assembled WebSocket message")),
        }
    }
}

fn compress_deflate_payload(payload: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut encoder = DeflateEncoder::new(payload, Compression::default());
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed).map_err(|e| {
        HttpError::new(HttpErrorKind::IoError, format!("Deflate compression error: {e}"))
    })?;
    if compressed.ends_with(&[0x00, 0x00, 0xff, 0xff]) {
        compressed.truncate(compressed.len() - 4);
    }
    Ok(compressed)
}

fn decompress_deflate_payload(payload: &[u8], max_size: usize) -> Result<Vec<u8>, HttpError> {
    let mut input = payload.to_vec();
    input.extend_from_slice(&[0x00, 0x00, 0xff, 0xff]);
    let mut decoder = DeflateDecoder::new(&input[..]);
    let mut decompressed = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let n = decoder.read(&mut buffer).map_err(|e| {
            HttpError::new(HttpErrorKind::ProtocolError, format!("Deflate decompression error: {e}"))
        })?;
        if n == 0 {
            break;
        }
        if decompressed.len() + n > max_size {
            return Err(HttpError::new(
                HttpErrorKind::BodyTooLarge,
                format!(
                    "Decompressed WebSocket payload exceeds maximum message limit {} (decompression bomb protection)",
                    max_size
                ),
            ));
        }
        decompressed.extend_from_slice(&buffer[..n]);
    }
    Ok(decompressed)
}

fn validate_outbound_frame(
    frame: &WebSocketFrame,
    is_client: bool,
    max_frame_size: usize,
    compression_enabled: bool,
) -> Result<(), HttpError> {
    if (frame.rsv1 && !compression_enabled) || frame.rsv2 || frame.rsv3 {
        return Err(protocol_error(
            "Reserved WebSocket bits require a negotiated extension",
        ));
    }
    if frame.payload.len() > max_frame_size {
        return Err(HttpError::new(
            HttpErrorKind::BodyTooLarge,
            format!(
                "Outbound WebSocket frame payload {} exceeds configured frame limit {}",
                frame.payload.len(),
                max_frame_size
            ),
        ));
    }
    if frame.opcode.is_control() {
        if !frame.fin {
            return Err(protocol_error("Control frames must not be fragmented"));
        }
        if frame.payload.len() > 125 {
            return Err(protocol_error("Control frames may not exceed 125 bytes"));
        }
    }
    if !is_client && frame.mask.is_some() {
        return Err(protocol_error("Server WebSocket frames must not be masked"));
    }
    Ok(())
}

fn decode_close_payload(payload: &[u8]) -> Result<(u16, String), HttpError> {
    if payload.is_empty() {
        return Ok((1005, String::new()));
    }
    if payload.len() == 1 {
        return Err(protocol_error(
            "WebSocket close frame payload must be empty or at least 2 bytes",
        ));
    }

    let code = u16::from_be_bytes([payload[0], payload[1]]);
    validate_close_code(code)?;
    let reason = std::str::from_utf8(&payload[2..]).map_err(|_| {
        HttpError::new(
            HttpErrorKind::ProtocolError,
            "WebSocket close reason contained invalid UTF-8",
        )
    })?;
    Ok((code, reason.to_string()))
}

fn validate_close_payload(code: u16, reason: &[u8]) -> Result<(), HttpError> {
    validate_close_code(code)?;
    if reason.len() > 123 {
        return Err(protocol_error(
            "WebSocket close reason exceeds the 123-byte RFC limit",
        ));
    }
    Ok(())
}

fn validate_close_code(code: u16) -> Result<(), HttpError> {
    let valid = matches!(
        code,
        1000
            | 1001
            | 1002
            | 1003
            | 1007
            | 1008
            | 1009
            | 1010
            | 1011
            | 1012
            | 1013
            | 1014
            | 3000..=4999
    );
    if !valid {
        return Err(protocol_error(&format!(
            "Invalid or reserved WebSocket close code: {code}"
        )));
    }
    Ok(())
}

fn protocol_error(message: &str) -> HttpError {
    HttpError::new(HttpErrorKind::ProtocolError, message)
}
