use super::super::errors::{HttpError, HttpErrorKind};

pub const HTTP2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Data,
    Headers,
    Priority,
    RstStream,
    Settings,
    PushPromise,
    Ping,
    GoAway,
    WindowUpdate,
    Continuation,
    Unknown(u8),
}

impl FrameType {
    pub fn byte(&self) -> u8 {
        match self {
            FrameType::Data => 0x0,
            FrameType::Headers => 0x1,
            FrameType::Priority => 0x2,
            FrameType::RstStream => 0x3,
            FrameType::Settings => 0x4,
            FrameType::PushPromise => 0x5,
            FrameType::Ping => 0x6,
            FrameType::GoAway => 0x7,
            FrameType::WindowUpdate => 0x8,
            FrameType::Continuation => 0x9,
            FrameType::Unknown(u) => *u,
        }
    }
}

impl From<u8> for FrameType {
    fn from(byte: u8) -> Self {
        match byte {
            0x0 => FrameType::Data,
            0x1 => FrameType::Headers,
            0x2 => FrameType::Priority,
            0x3 => FrameType::RstStream,
            0x4 => FrameType::Settings,
            0x5 => FrameType::PushPromise,
            0x6 => FrameType::Ping,
            0x7 => FrameType::GoAway,
            0x8 => FrameType::WindowUpdate,
            0x9 => FrameType::Continuation,
            other => FrameType::Unknown(other),
        }
    }
}

pub mod flags {
    pub const END_STREAM: u8 = 0x1;
    pub const END_HEADERS: u8 = 0x4;
    pub const PADDED: u8 = 0x8;
    pub const PRIORITY: u8 = 0x20;
    pub const ACK: u8 = 0x1;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameHeader {
    pub length: u32,
    pub frame_type: FrameType,
    pub flags: u8,
    pub stream_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Http2Frame {
    Data {
        stream_id: u32,
        end_stream: bool,
        data: Vec<u8>,
    },
    Headers {
        stream_id: u32,
        end_stream: bool,
        end_headers: bool,
        header_block_fragment: Vec<u8>,
    },
    Priority {
        stream_id: u32,
        exclusive: bool,
        stream_dependency: u32,
        weight: u8,
    },
    RstStream {
        stream_id: u32,
        error_code: u32,
    },
    Settings {
        ack: bool,
        settings: Vec<(u16, u32)>,
    },
    PushPromise {
        stream_id: u32,
        promised_stream_id: u32,
        end_headers: bool,
        header_block_fragment: Vec<u8>,
    },
    Ping {
        ack: bool,
        opaque_data: [u8; 8],
    },
    GoAway {
        last_stream_id: u32,
        error_code: u32,
        debug_data: Vec<u8>,
    },
    WindowUpdate {
        stream_id: u32,
        window_size_increment: u32,
    },
    Continuation {
        stream_id: u32,
        end_headers: bool,
        header_block_fragment: Vec<u8>,
    },
}

impl Http2Frame {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Http2Frame::Data {
                stream_id,
                end_stream,
                data,
            } => {
                let flags = if *end_stream { flags::END_STREAM } else { 0 };
                encode_header(&mut out, data.len() as u32, 0x0, flags, *stream_id);
                out.extend_from_slice(data);
            }
            Http2Frame::Headers {
                stream_id,
                end_stream,
                end_headers,
                header_block_fragment,
            } => {
                let mut flags = 0;
                if *end_stream {
                    flags |= flags::END_STREAM;
                }
                if *end_headers {
                    flags |= flags::END_HEADERS;
                }
                encode_header(
                    &mut out,
                    header_block_fragment.len() as u32,
                    0x1,
                    flags,
                    *stream_id,
                );
                out.extend_from_slice(header_block_fragment);
            }
            Http2Frame::Settings { ack, settings } => {
                let flags = if *ack { flags::ACK } else { 0 };
                let len = (settings.len() * 6) as u32;
                encode_header(&mut out, len, 0x4, flags, 0);
                for (id, val) in settings {
                    out.extend_from_slice(&id.to_be_bytes());
                    out.extend_from_slice(&val.to_be_bytes());
                }
            }
            Http2Frame::WindowUpdate {
                stream_id,
                window_size_increment,
            } => {
                encode_header(&mut out, 4, 0x8, 0, *stream_id);
                out.extend_from_slice(&(window_size_increment & 0x7FFFFFFF).to_be_bytes());
            }
            Http2Frame::Ping { ack, opaque_data } => {
                let flags = if *ack { flags::ACK } else { 0 };
                encode_header(&mut out, 8, 0x6, flags, 0);
                out.extend_from_slice(opaque_data);
            }
            Http2Frame::RstStream {
                stream_id,
                error_code,
            } => {
                encode_header(&mut out, 4, 0x3, 0, *stream_id);
                out.extend_from_slice(&error_code.to_be_bytes());
            }
            Http2Frame::GoAway {
                last_stream_id,
                error_code,
                debug_data,
            } => {
                let len = (8 + debug_data.len()) as u32;
                encode_header(&mut out, len, 0x7, 0, 0);
                out.extend_from_slice(&(last_stream_id & 0x7FFFFFFF).to_be_bytes());
                out.extend_from_slice(&error_code.to_be_bytes());
                out.extend_from_slice(debug_data);
            }
            Http2Frame::Continuation {
                stream_id,
                end_headers,
                header_block_fragment,
            } => {
                let flags = if *end_headers { flags::END_HEADERS } else { 0 };
                encode_header(
                    &mut out,
                    header_block_fragment.len() as u32,
                    0x9,
                    flags,
                    *stream_id,
                );
                out.extend_from_slice(header_block_fragment);
            }
            Http2Frame::PushPromise {
                stream_id,
                promised_stream_id,
                end_headers,
                header_block_fragment,
            } => {
                let flags = if *end_headers { flags::END_HEADERS } else { 0 };
                let len = (4 + header_block_fragment.len()) as u32;
                encode_header(&mut out, len, 0x5, flags, *stream_id);
                out.extend_from_slice(&(promised_stream_id & 0x7FFFFFFF).to_be_bytes());
                out.extend_from_slice(header_block_fragment);
            }
            Http2Frame::Priority {
                stream_id,
                exclusive,
                stream_dependency,
                weight,
            } => {
                encode_header(&mut out, 5, 0x2, 0, *stream_id);
                let dep = if *exclusive {
                    stream_dependency | 0x80000000
                } else {
                    stream_dependency & 0x7FFFFFFF
                };
                out.extend_from_slice(&dep.to_be_bytes());
                out.push(*weight);
            }
        }
        out
    }

    pub fn decode(buffer: &[u8]) -> Result<Option<(Http2Frame, usize)>, HttpError> {
        if buffer.len() < 9 {
            return Ok(None);
        }

        let length = ((buffer[0] as u32) << 16) | ((buffer[1] as u32) << 8) | (buffer[2] as u32);
        let frame_type = FrameType::from(buffer[3]);
        let flags = buffer[4];
        let stream_id = u32::from_be_bytes([buffer[5] & 0x7F, buffer[6], buffer[7], buffer[8]]);

        let total_frame_len = 9 + length as usize;
        if buffer.len() < total_frame_len {
            return Ok(None);
        }

        let payload = &buffer[9..total_frame_len];

        let frame = match frame_type {
            FrameType::Data => {
                let end_stream = (flags & flags::END_STREAM) != 0;
                Http2Frame::Data {
                    stream_id,
                    end_stream,
                    data: payload.to_vec(),
                }
            }
            FrameType::Headers => {
                let end_stream = (flags & flags::END_STREAM) != 0;
                let end_headers = (flags & flags::END_HEADERS) != 0;
                Http2Frame::Headers {
                    stream_id,
                    end_stream,
                    end_headers,
                    header_block_fragment: payload.to_vec(),
                }
            }
            FrameType::Settings => {
                let ack = (flags & flags::ACK) != 0;
                if length % 6 != 0 {
                    return Err(HttpError::new(
                        HttpErrorKind::Http2Error,
                        "Malformed SETTINGS frame length",
                    ));
                }
                let mut settings = Vec::new();
                for chunk in payload.chunks_exact(6) {
                    let id = u16::from_be_bytes([chunk[0], chunk[1]]);
                    let val = u32::from_be_bytes([chunk[2], chunk[3], chunk[4], chunk[5]]);
                    settings.push((id, val));
                }
                Http2Frame::Settings { ack, settings }
            }
            FrameType::WindowUpdate => {
                if length != 4 {
                    return Err(HttpError::new(
                        HttpErrorKind::Http2Error,
                        "Invalid WINDOW_UPDATE frame size",
                    ));
                }
                let inc =
                    u32::from_be_bytes([payload[0] & 0x7F, payload[1], payload[2], payload[3]]);
                Http2Frame::WindowUpdate {
                    stream_id,
                    window_size_increment: inc,
                }
            }
            FrameType::Ping => {
                if length != 8 {
                    return Err(HttpError::new(
                        HttpErrorKind::Http2Error,
                        "Invalid PING frame size",
                    ));
                }
                let mut opaque = [0u8; 8];
                opaque.copy_from_slice(payload);
                Http2Frame::Ping {
                    ack: (flags & flags::ACK) != 0,
                    opaque_data: opaque,
                }
            }
            FrameType::RstStream => {
                if length != 4 {
                    return Err(HttpError::new(
                        HttpErrorKind::Http2Error,
                        "Invalid RST_STREAM frame size",
                    ));
                }
                let err = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                Http2Frame::RstStream {
                    stream_id,
                    error_code: err,
                }
            }
            FrameType::GoAway => {
                if length < 8 {
                    return Err(HttpError::new(
                        HttpErrorKind::Http2Error,
                        "Invalid GOAWAY frame size",
                    ));
                }
                let last_id =
                    u32::from_be_bytes([payload[0] & 0x7F, payload[1], payload[2], payload[3]]);
                let err = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                Http2Frame::GoAway {
                    last_stream_id: last_id,
                    error_code: err,
                    debug_data: payload[8..].to_vec(),
                }
            }
            FrameType::Continuation => {
                let end_headers = (flags & flags::END_HEADERS) != 0;
                Http2Frame::Continuation {
                    stream_id,
                    end_headers,
                    header_block_fragment: payload.to_vec(),
                }
            }
            _ => {
                // Ignore unknown frames per RFC 9113
                return Ok(Some((
                    Http2Frame::Data {
                        stream_id,
                        end_stream: false,
                        data: Vec::new(),
                    },
                    total_frame_len,
                )));
            }
        };

        Ok(Some((frame, total_frame_len)))
    }
}

fn encode_header(out: &mut Vec<u8>, length: u32, frame_type: u8, flags: u8, stream_id: u32) {
    out.push(((length >> 16) & 0xFF) as u8);
    out.push(((length >> 8) & 0xFF) as u8);
    out.push((length & 0xFF) as u8);
    out.push(frame_type);
    out.push(flags);
    out.extend_from_slice(&(stream_id & 0x7FFFFFFF).to_be_bytes());
}
