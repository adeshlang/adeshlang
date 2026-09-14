use super::super::errors::{HttpError, HttpErrorKind};

// RFC 9000 Variable-Length Integer Encoding
pub fn encode_varint(val: u64, out: &mut Vec<u8>) {
    if val < 64 {
        out.push(val as u8);
    } else if val < 16384 {
        let v = (val as u16) | 0x4000;
        out.extend_from_slice(&v.to_be_bytes());
    } else if val < 1073741824 {
        let v = (val as u32) | 0x80000000;
        out.extend_from_slice(&v.to_be_bytes());
    } else {
        let v = val | 0xC000000000000000;
        out.extend_from_slice(&v.to_be_bytes());
    }
}

pub fn decode_varint(data: &[u8]) -> Result<(u64, usize), HttpError> {
    if data.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::Http3Error,
            "Truncated varint",
        ));
    }
    let first = data[0];
    let len = 1 << ((first & 0xC0) >> 6);
    if data.len() < len {
        return Err(HttpError::new(
            HttpErrorKind::Http3Error,
            "Truncated multi-byte varint",
        ));
    }

    let mut val = (first & 0x3F) as u64;
    for &b in &data[1..len] {
        val = (val << 8) | (b as u64);
    }
    Ok((val, len))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Http3Frame {
    Data {
        data: Vec<u8>,
    },
    Headers {
        encoded_fields: Vec<u8>,
    },
    CancelPush {
        push_id: u64,
    },
    Settings {
        settings: Vec<(u64, u64)>,
    },
    PushPromise {
        push_id: u64,
        encoded_fields: Vec<u8>,
    },
    GoAway {
        id: u64,
    },
    MaxPushId {
        push_id: u64,
    },
}

impl Http3Frame {
    pub fn encode(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        let frame_type = match self {
            Http3Frame::Data { data } => {
                payload.extend_from_slice(data);
                0x00
            }
            Http3Frame::Headers { encoded_fields } => {
                payload.extend_from_slice(encoded_fields);
                0x01
            }
            Http3Frame::CancelPush { push_id } => {
                encode_varint(*push_id, &mut payload);
                0x03
            }
            Http3Frame::Settings { settings } => {
                for (id, val) in settings {
                    encode_varint(*id, &mut payload);
                    encode_varint(*val, &mut payload);
                }
                0x04
            }
            Http3Frame::PushPromise {
                push_id,
                encoded_fields,
            } => {
                encode_varint(*push_id, &mut payload);
                payload.extend_from_slice(encoded_fields);
                0x05
            }
            Http3Frame::GoAway { id } => {
                encode_varint(*id, &mut payload);
                0x07
            }
            Http3Frame::MaxPushId { push_id } => {
                encode_varint(*push_id, &mut payload);
                0x0D
            }
        };

        let mut out = Vec::new();
        encode_varint(frame_type, &mut out);
        encode_varint(payload.len() as u64, &mut out);
        out.extend_from_slice(&payload);
        out
    }

    pub fn decode(buffer: &[u8]) -> Result<Option<(Http3Frame, usize)>, HttpError> {
        if buffer.is_empty() {
            return Ok(None);
        }

        let (frame_type, type_len) = match decode_varint(buffer) {
            Ok(res) => res,
            Err(_) => return Ok(None),
        };

        let remaining = &buffer[type_len..];
        let (payload_len, len_len) = match decode_varint(remaining) {
            Ok(res) => res,
            Err(_) => return Ok(None),
        };

        let total_header_len = type_len + len_len;
        let total_frame_len = total_header_len + payload_len as usize;

        if buffer.len() < total_frame_len {
            return Ok(None);
        }

        let payload = &buffer[total_header_len..total_frame_len];

        let frame = match frame_type {
            0x00 => Http3Frame::Data {
                data: payload.to_vec(),
            },
            0x01 => Http3Frame::Headers {
                encoded_fields: payload.to_vec(),
            },
            0x04 => {
                let mut settings = Vec::new();
                let mut offset = 0;
                while offset < payload.len() {
                    let (id, id_len) = decode_varint(&payload[offset..])?;
                    offset += id_len;
                    let (val, val_len) = decode_varint(&payload[offset..])?;
                    offset += val_len;
                    settings.push((id, val));
                }
                Http3Frame::Settings { settings }
            }
            0x07 => {
                let (id, _) = decode_varint(payload)?;
                Http3Frame::GoAway { id }
            }
            _ => {
                // Unknown frame per RFC 9114 Section 7.2.8 is ignored
                Http3Frame::Data { data: Vec::new() }
            }
        };

        Ok(Some((frame, total_frame_len)))
    }
}
