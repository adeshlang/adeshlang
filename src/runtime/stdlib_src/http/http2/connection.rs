use super::super::body::Body;
use super::super::errors::{HttpError, HttpErrorKind};
use super::super::headers::Headers;
use super::super::http1::connection::ReadWriteSendSync;
use super::super::request::Request;
use super::super::response::Response;
use super::super::status::HttpStatus;
use super::super::version::HttpVersion;
use super::flow_control::FlowControl;
use super::frames::{HTTP2_PREFACE, Http2Frame};
use super::hpack::{HpackDecoder, HpackEncoder};
use super::streams::Http2Stream;
use std::collections::HashMap;
use std::io::{Read, Write};

pub struct Http2Connection {
    pub stream: Box<dyn ReadWriteSendSync>,
    pub is_client: bool,
    pub next_stream_id: u32,
    pub active_streams: HashMap<u32, Http2Stream>,
    pub flow_control: FlowControl,
    pub encoder: HpackEncoder,
    pub decoder: HpackDecoder,
    pub read_buffer: Vec<u8>,
    pub handshake_done: bool,
}

impl Http2Connection {
    pub fn new(stream: Box<dyn ReadWriteSendSync>, is_client: bool) -> Self {
        Self {
            stream,
            is_client,
            next_stream_id: if is_client { 1 } else { 2 },
            active_streams: HashMap::new(),
            flow_control: FlowControl::default(),
            encoder: HpackEncoder::new(4096),
            decoder: HpackDecoder::new(4096),
            read_buffer: Vec::with_capacity(16384),
            handshake_done: false,
        }
    }

    pub fn client_handshake(&mut self) -> Result<(), HttpError> {
        if self.handshake_done {
            return Ok(());
        }

        // 1. Send client connection preface
        self.stream.write_all(HTTP2_PREFACE).map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to send H2 preface: {}", e),
            )
        })?;

        // 2. Send initial SETTINGS frame
        let init_settings = Http2Frame::Settings {
            ack: false,
            settings: vec![
                (0x1, 4096),  // HEADER_TABLE_SIZE
                (0x2, 0),     // ENABLE_PUSH = 0
                (0x3, 100),   // MAX_CONCURRENT_STREAMS = 100
                (0x4, 65535), // INITIAL_WINDOW_SIZE
            ],
        };
        self.stream
            .write_all(&init_settings.encode())
            .map_err(|e| {
                HttpError::new(
                    HttpErrorKind::IoError,
                    format!("Failed to send initial SETTINGS: {}", e),
                )
            })?;
        self.stream.flush().map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to flush stream: {}", e),
            )
        })?;

        self.handshake_done = true;
        Ok(())
    }

    pub fn server_handshake(&mut self) -> Result<(), HttpError> {
        if self.handshake_done {
            return Ok(());
        }

        // Send server initial SETTINGS frame to complete H2 handshake
        let init_settings = Http2Frame::Settings {
            ack: false,
            settings: vec![
                (0x1, 4096),  // HEADER_TABLE_SIZE
                (0x3, 100),   // MAX_CONCURRENT_STREAMS = 100
                (0x4, 65535), // INITIAL_WINDOW_SIZE
            ],
        };
        self.stream
            .write_all(&init_settings.encode())
            .map_err(|e| {
                HttpError::new(
                    HttpErrorKind::IoError,
                    format!("Failed to send server SETTINGS: {}", e),
                )
            })?;
        self.stream.flush().map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to flush stream: {}", e),
            )
        })?;

        self.handshake_done = true;
        Ok(())
    }

    pub fn send_request(&mut self, req: &Request) -> Result<Response, HttpError> {
        self.client_handshake()?;

        let stream_id = self.next_stream_id;
        self.next_stream_id += 2;

        let path = req.uri.path_and_query();
        let scheme = if req.uri.is_https() { "https" } else { "http" };
        let authority = req.uri.host.clone().unwrap_or_default();

        let mut hpack_headers = vec![
            (":method", req.method.as_str()),
            (":scheme", scheme),
            (":path", path.as_str()),
            (":authority", authority.as_str()),
        ];

        let req_headers_map = req.headers.to_map();
        for (k, v) in &req_headers_map {
            if k != "host" && k != "connection" && k != "transfer-encoding" {
                hpack_headers.push((k.as_str(), v.as_str()));
            }
        }

        let header_block = self.encoder.encode(&hpack_headers);
        let body_bytes = req.body.to_bytes()?;
        let has_body = !body_bytes.is_empty();

        let headers_frame = Http2Frame::Headers {
            stream_id,
            end_stream: !has_body,
            end_headers: true,
            header_block_fragment: header_block,
        };

        self.stream
            .write_all(&headers_frame.encode())
            .map_err(|e| {
                HttpError::new(
                    HttpErrorKind::IoError,
                    format!("Failed to write HEADERS: {}", e),
                )
            })?;

        if has_body {
            let data_frame = Http2Frame::Data {
                stream_id,
                end_stream: true,
                data: body_bytes,
            };
            self.stream.write_all(&data_frame.encode()).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::IoError,
                    format!("Failed to write DATA: {}", e),
                )
            })?;
        }

        self.stream.flush().map_err(|e| {
            HttpError::new(
                HttpErrorKind::IoError,
                format!("Failed to flush stream: {}", e),
            )
        })?;

        let stream_obj = Http2Stream::new(stream_id, self.flow_control.initial_stream_window);
        self.active_streams.insert(stream_id, stream_obj);

        // Read frames until response for this stream is complete
        self.wait_for_response(stream_id)
    }

    fn wait_for_response(&mut self, target_stream_id: u32) -> Result<Response, HttpError> {
        let mut temp_buf = [0u8; 8192];
        loop {
            // Process any frame in buffer
            if let Some((frame, consumed)) = Http2Frame::decode(&self.read_buffer)? {
                self.read_buffer.drain(..consumed);
                self.handle_frame(frame)?;

                if let Some(stream) = self.active_streams.get(&target_stream_id) {
                    if stream.end_stream_received {
                        return self.construct_response(target_stream_id);
                    }
                }
                continue;
            }

            // Read more bytes
            let n = self.stream.read(&mut temp_buf).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::IoError,
                    format!("H2 socket read error: {}", e),
                )
            })?;
            if n == 0 {
                return Err(HttpError::new(
                    HttpErrorKind::ConnectError,
                    "HTTP/2 connection closed prematurely by peer",
                ));
            }
            self.read_buffer.extend_from_slice(&temp_buf[..n]);
        }
    }

    pub(crate) fn handle_frame(&mut self, frame: Http2Frame) -> Result<(), HttpError> {
        match frame {
            Http2Frame::Settings { ack, .. } => {
                if !ack {
                    // Send SETTINGS ACK
                    let ack_frame = Http2Frame::Settings {
                        ack: true,
                        settings: Vec::new(),
                    };
                    let _ = self.stream.write_all(&ack_frame.encode());
                    let _ = self.stream.flush();
                }
            }
            Http2Frame::Ping { ack, opaque_data } => {
                if !ack {
                    // Reply with PING ACK
                    let pong = Http2Frame::Ping {
                        ack: true,
                        opaque_data,
                    };
                    let _ = self.stream.write_all(&pong.encode());
                    let _ = self.stream.flush();
                }
            }
            Http2Frame::Headers {
                stream_id,
                end_stream,
                header_block_fragment,
                ..
            } => {
                let decoded = self.decoder.decode(&header_block_fragment)?;
                let stream = self.active_streams.entry(stream_id).or_insert_with(|| {
                    Http2Stream::new(stream_id, self.flow_control.initial_stream_window)
                });

                let is_trailers = !stream.incoming_headers.is_empty();
                if is_trailers {
                    let trailers = stream.trailers.get_or_insert_with(Headers::new);
                    for (name, val) in decoded {
                        if !name.starts_with(':') {
                            let _ = trailers.append(&name, &val);
                        }
                    }
                } else {
                    stream.incoming_headers.extend(decoded);
                }

                if end_stream {
                    stream.end_stream_received = true;
                }
            }
            Http2Frame::Data {
                stream_id,
                end_stream,
                data,
            } => {
                let stream = self.active_streams.entry(stream_id).or_insert_with(|| {
                    Http2Stream::new(stream_id, self.flow_control.initial_stream_window)
                });
                stream.incoming_data.extend_from_slice(&data);
                if end_stream {
                    stream.end_stream_received = true;
                }
            }
            Http2Frame::GoAway {
                last_stream_id,
                error_code,
                ..
            } => {
                return Err(HttpError::new(
                    HttpErrorKind::Http2Error,
                    format!(
                        "Received GOAWAY (last_stream={}, error={})",
                        last_stream_id, error_code
                    ),
                ));
            }
            Http2Frame::RstStream {
                stream_id,
                error_code,
            } => {
                return Err(HttpError::new(
                    HttpErrorKind::Http2Error,
                    format!("Stream {} reset with error code {}", stream_id, error_code),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn construct_response(&mut self, stream_id: u32) -> Result<Response, HttpError> {
        let stream = self
            .active_streams
            .remove(&stream_id)
            .ok_or_else(|| HttpError::new(HttpErrorKind::Http2Error, "Stream data not found"))?;

        let mut status = HttpStatus::OK;
        let mut headers = Headers::new();

        for (name, val) in stream.incoming_headers {
            if name == ":status" {
                let code = val.parse::<u16>().unwrap_or(200);
                status = HttpStatus(code);
            } else if !name.starts_with(':') {
                let _ = headers.append(&name, &val);
            }
        }

        let resp = Response {
            status,
            version: HttpVersion::Http20,
            headers,
            body: Body::from_bytes(stream.incoming_data),
            trailers: stream.trailers,
        };

        Ok(resp)
    }
}
