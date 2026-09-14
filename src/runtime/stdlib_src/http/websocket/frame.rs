use super::super::errors::{HttpError, HttpErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSocketOpcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

impl WebSocketOpcode {
    pub fn from_u8(opcode: u8) -> Result<Self, HttpError> {
        match opcode {
            0x0 => Ok(Self::Continuation),
            0x1 => Ok(Self::Text),
            0x2 => Ok(Self::Binary),
            0x8 => Ok(Self::Close),
            0x9 => Ok(Self::Ping),
            0xA => Ok(Self::Pong),
            _ => Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                format!("Unknown WebSocket opcode: 0x{opcode:02X}"),
            )),
        }
    }

    pub fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketFrame {
    pub fin: bool,
    pub rsv1: bool,
    pub rsv2: bool,
    pub rsv3: bool,
    pub opcode: WebSocketOpcode,
    pub mask: Option<[u8; 4]>,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WebSocketFrameDecodeConfig {
    pub expect_masked: Option<bool>,
    pub max_frame_size: Option<usize>,
    pub allow_reserved_bits: bool,
}

impl WebSocketFrame {
    pub fn text(s: &str) -> Self {
        Self {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Text,
            mask: None,
            payload: s.as_bytes().to_vec(),
        }
    }

    pub fn binary(b: Vec<u8>) -> Self {
        Self {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Binary,
            mask: None,
            payload: b,
        }
    }

    pub fn ping(payload: Vec<u8>) -> Self {
        Self {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Ping,
            mask: None,
            payload,
        }
    }

    pub fn pong(payload: Vec<u8>) -> Self {
        Self {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Pong,
            mask: None,
            payload,
        }
    }

    pub fn close(code: u16, reason: &str) -> Self {
        let mut payload = Vec::with_capacity(2 + reason.len());
        payload.extend_from_slice(&code.to_be_bytes());
        payload.extend_from_slice(reason.as_bytes());
        Self {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Close,
            mask: None,
            payload,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mask_overhead = if self.mask.is_some() { 4 } else { 0 };
        let mut out = Vec::with_capacity(2 + self.payload.len() + mask_overhead);
        let mut first = self.opcode as u8;
        if self.fin {
            first |= 0x80;
        }
        if self.rsv1 {
            first |= 0x40;
        }
        if self.rsv2 {
            first |= 0x20;
        }
        if self.rsv3 {
            first |= 0x10;
        }
        out.push(first);

        let len = self.payload.len();
        let is_masked = self.mask.is_some();
        let mask_bit = if is_masked { 0x80 } else { 0x00 };

        if len < 126 {
            out.push((len as u8) | mask_bit);
        } else if len <= u16::MAX as usize {
            out.push(126 | mask_bit);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            out.push(127 | mask_bit);
            out.extend_from_slice(&(len as u64).to_be_bytes());
        }

        if let Some(mask) = self.mask {
            out.extend_from_slice(&mask);
            let mut masked_payload = self.payload.clone();
            apply_mask_in_place(&mut masked_payload, mask);
            out.extend_from_slice(&masked_payload);
        } else {
            out.extend_from_slice(&self.payload);
        }

        out
    }

    pub fn decode(buffer: &[u8]) -> Result<Option<(WebSocketFrame, usize)>, HttpError> {
        Self::decode_with_config(buffer, WebSocketFrameDecodeConfig::default())
    }

    pub fn decode_with_config(
        buffer: &[u8],
        config: WebSocketFrameDecodeConfig,
    ) -> Result<Option<(WebSocketFrame, usize)>, HttpError> {
        if buffer.len() < 2 {
            return Ok(None);
        }

        let first = buffer[0];
        let second = buffer[1];

        let fin = (first & 0x80) != 0;
        let rsv1 = (first & 0x40) != 0;
        let rsv2 = (first & 0x20) != 0;
        let rsv3 = (first & 0x10) != 0;
        if rsv2 || rsv3 {
            return Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                "RSV2 and RSV3 bits are reserved and must be 0",
            ));
        }
        if !config.allow_reserved_bits && rsv1 {
            return Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                "Reserved WebSocket RSV1 bit set without a negotiated extension",
            ));
        }

        let opcode = WebSocketOpcode::from_u8(first & 0x0F)?;
        let masked = (second & 0x80) != 0;
        if let Some(expected_masked) = config.expect_masked {
            if masked != expected_masked {
                return Err(HttpError::new(
                    HttpErrorKind::ProtocolError,
                    if expected_masked {
                        "Expected masked WebSocket frame from peer"
                    } else {
                        "Received masked WebSocket frame from peer"
                    },
                ));
            }
        }

        let mut payload_len = (second & 0x7F) as u64;
        let mut offset = 2usize;

        if payload_len == 126 {
            if buffer.len() < offset + 2 {
                return Ok(None);
            }
            payload_len = u16::from_be_bytes([buffer[offset], buffer[offset + 1]]) as u64;
            offset += 2;
        } else if payload_len == 127 {
            if buffer.len() < offset + 8 {
                return Ok(None);
            }
            payload_len = u64::from_be_bytes([
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
                buffer[offset + 4],
                buffer[offset + 5],
                buffer[offset + 6],
                buffer[offset + 7],
            ]);
            if (payload_len >> 63) != 0 {
                return Err(HttpError::new(
                    HttpErrorKind::ProtocolError,
                    "WebSocket 64-bit payload length uses reserved high bit",
                ));
            }
            offset += 8;
        }

        if opcode.is_control() {
            if !fin {
                return Err(HttpError::new(
                    HttpErrorKind::ProtocolError,
                    "Control frames must not be fragmented",
                ));
            }
            if payload_len > 125 {
                return Err(HttpError::new(
                    HttpErrorKind::ProtocolError,
                    "Control frame payload exceeds 125-byte RFC limit",
                ));
            }
        }

        if let Some(max_frame_size) = config.max_frame_size {
            if payload_len > max_frame_size as u64 {
                return Err(HttpError::new(
                    HttpErrorKind::BodyTooLarge,
                    format!(
                        "WebSocket frame payload length {} exceeds configured limit {}",
                        payload_len, max_frame_size
                    ),
                ));
            }
        }

        let payload_len_usize = usize::try_from(payload_len).map_err(|_| {
            HttpError::new(
                HttpErrorKind::BodyTooLarge,
                "WebSocket frame payload length exceeds platform addressable memory",
            )
        })?;

        let mask = if masked {
            if buffer.len() < offset + 4 {
                return Ok(None);
            }
            let m = [
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            ];
            offset += 4;
            Some(m)
        } else {
            None
        };

        let frame_end = offset.checked_add(payload_len_usize).ok_or_else(|| {
            HttpError::new(
                HttpErrorKind::BodyTooLarge,
                "WebSocket frame length overflow while parsing",
            )
        })?;
        if buffer.len() < frame_end {
            return Ok(None);
        }

        let mut payload = buffer[offset..frame_end].to_vec();
        if let Some(mask_bytes) = mask {
            apply_mask_in_place(&mut payload, mask_bytes);
        }

        Ok(Some((
            WebSocketFrame {
                fin,
                rsv1,
                rsv2,
                rsv3,
                opcode,
                mask,
                payload,
            },
            frame_end,
        )))
    }
}

fn apply_mask_in_place(payload: &mut [u8], mask: [u8; 4]) {
    for (i, b) in payload.iter_mut().enumerate() {
        *b ^= mask[i % 4];
    }
}
