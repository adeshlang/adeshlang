//! ATP Wire Format
//!
//! Defines the binary encoding for ATP/1 packets and frames.

use super::config::{
    CONNECTION_ID_LEN, MAX_ACK_RANGES, MAX_CID_LEN, MAX_DATAGRAM_SIZE, MAX_FRAME_PAYLOAD,
    MAX_HANDSHAKE_SIZE, MAX_REASON_LENGTH, MAX_VARINT,
};
use super::errors::{AtpError, AtpResult};

// Re-export protocol constants for compatibility.
pub use super::config::MAX_PACKET_PAYLOAD;

/// Long-header flag bit (bit 7 = 1 means long header).
pub const FLAG_LONG_HEADER: u8 = 0x80;
/// Fixed bit (bit 6) — must be set on all valid ATP packets.
pub const FLAG_FIXED_BIT: u8 = 0x40;

// ---------------------------------------------------------------------------
// Frame types
// ---------------------------------------------------------------------------

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    ConnectionInit = 0x01,
    ConnectionInitAck = 0x02,
    Handshake = 0x03,
    Data = 0x04,
    Ack = 0x05,
    StreamOpen = 0x06,
    StreamClose = 0x07,
    StreamReset = 0x08,
    MaxData = 0x09,
    MaxStreamData = 0x0A,
    Ping = 0x0B,
    Pong = 0x0C,
    PathChallenge = 0x0D,
    PathResponse = 0x0E,
    ConnectionClose = 0x0F,
    KeyUpdate = 0x10,
    Cancel = 0x11,
    Datagram = 0x12,
    Retry = 0x13,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::ConnectionInit),
            0x02 => Some(Self::ConnectionInitAck),
            0x03 => Some(Self::Handshake),
            0x04 => Some(Self::Data),
            0x05 => Some(Self::Ack),
            0x06 => Some(Self::StreamOpen),
            0x07 => Some(Self::StreamClose),
            0x08 => Some(Self::StreamReset),
            0x09 => Some(Self::MaxData),
            0x0A => Some(Self::MaxStreamData),
            0x0B => Some(Self::Ping),
            0x0C => Some(Self::Pong),
            0x0D => Some(Self::PathChallenge),
            0x0E => Some(Self::PathResponse),
            0x0F => Some(Self::ConnectionClose),
            0x10 => Some(Self::KeyUpdate),
            0x11 => Some(Self::Cancel),
            0x12 => Some(Self::Datagram),
            0x13 => Some(Self::Retry),
            _ => None,
        }
    }
}

pub const DATA_FLAG_RELIABLE: u8 = 0x01;
pub const DATA_FLAG_ORDERED: u8 = 0x02;
pub const DATA_FLAG_FIN: u8 = 0x04;
pub const DATA_FLAG_FIRST: u8 = 0x08;

// ---------------------------------------------------------------------------
// Varint codec (QUIC-style, canonical)
// ---------------------------------------------------------------------------

/// Encode a value as a canonical QUIC-style varint.
pub fn encode_varint(value: u64) -> Vec<u8> {
    assert!(value <= MAX_VARINT, "varint overflow");
    if value <= 63 {
        vec![value as u8]
    } else if value <= 16383 {
        let v = value | 0x4000;
        (v as u16).to_be_bytes().to_vec()
    } else if value <= 1_073_741_823 {
        let v = value | 0x8000_0000;
        (v as u32).to_be_bytes().to_vec()
    } else {
        let v = value | 0xC000_0000_0000_0000;
        v.to_be_bytes().to_vec()
    }
}

/// Decode a varint, rejecting non-canonical encodings.
pub fn decode_varint(buf: &[u8], offset: usize) -> AtpResult<(u64, usize)> {
    if offset >= buf.len() {
        return Err(AtpError::protocol("varint: buffer too short"));
    }
    let first = buf[offset];
    let tag = first >> 6;
    let (value, len): (u64, usize) = match tag {
        0 => (first as u64, 1usize),
        1 => {
            if offset + 2 > buf.len() {
                return Err(AtpError::protocol("varint(2): truncated"));
            }
            let raw = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
            ((raw & 0x3FFF) as u64, 2usize)
        }
        2 => {
            if offset + 4 > buf.len() {
                return Err(AtpError::protocol("varint(4): truncated"));
            }
            let raw = u32::from_be_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            (raw as u64 & 0x3FFF_FFFF, 4usize)
        }
        3 => {
            if offset + 8 > buf.len() {
                return Err(AtpError::protocol("varint(8): truncated"));
            }
            let raw = u64::from_be_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
                buf[offset + 4],
                buf[offset + 5],
                buf[offset + 6],
                buf[offset + 7],
            ]);
            (raw & 0x3FFF_FFFF_FFFF_FFFF, 8usize)
        }
        _ => unreachable!(),
    };

    // Reject non-canonical encodings.
    let canonical = encode_varint(value);
    if canonical.len() != len || buf[offset..offset + len] != canonical[..] {
        return Err(AtpError::protocol("varint: non-canonical encoding"));
    }

    Ok((value, len))
}

pub fn push_varint(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&encode_varint(value));
}

// ---------------------------------------------------------------------------
// Packet headers
// ---------------------------------------------------------------------------

/// Long header — handshake and control during establishment.
#[derive(Debug, Clone)]
pub struct LongHeader {
    pub version: u32,
    pub dst_conn_id: Vec<u8>,
    pub src_conn_id: Vec<u8>,
    pub packet_number: u64,
    pub payload: Vec<u8>,
}

/// Short header — established connections (destination CID, no context compression).
#[derive(Debug, Clone)]
pub struct ShortHeader {
    pub dst_conn_id: [u8; CONNECTION_ID_LEN],
    pub packet_number: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum PacketHeader {
    Long(LongHeader),
    Short(ShortHeader),
}

impl PacketHeader {
    pub fn decode(buf: &[u8]) -> AtpResult<(PacketHeader, usize)> {
        if buf.is_empty() {
            return Err(AtpError::protocol("empty packet"));
        }
        if buf.len() > MAX_PACKET_PAYLOAD {
            return Err(AtpError::protocol("packet exceeds max size"));
        }
        let first = buf[0];

        if first & FLAG_LONG_HEADER != 0 {
            if first & FLAG_FIXED_BIT == 0 {
                return Err(AtpError::protocol("long header: fixed bit not set"));
            }
            let mut off = 1;
            if off + 4 > buf.len() {
                return Err(AtpError::protocol("long header: version truncated"));
            }
            let version = u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
            off += 4;

            if off >= buf.len() {
                return Err(AtpError::protocol("long header: dst cid len truncated"));
            }
            let dst_cid_len = buf[off] as usize;
            off += 1;
            if dst_cid_len > MAX_CID_LEN {
                return Err(AtpError::protocol("dst CID too long"));
            }
            if off + dst_cid_len > buf.len() {
                return Err(AtpError::protocol("long header: dst cid truncated"));
            }
            let dst_conn_id = buf[off..off + dst_cid_len].to_vec();
            off += dst_cid_len;

            if off >= buf.len() {
                return Err(AtpError::protocol("long header: src cid len truncated"));
            }
            let src_cid_len = buf[off] as usize;
            off += 1;
            if src_cid_len > MAX_CID_LEN {
                return Err(AtpError::protocol("src CID too long"));
            }
            if off + src_cid_len > buf.len() {
                return Err(AtpError::protocol("long header: src cid truncated"));
            }
            let src_conn_id = buf[off..off + src_cid_len].to_vec();
            off += src_cid_len;

            let (packet_number, pn_len) = decode_varint(buf, off)?;
            off += pn_len;
            let payload = buf[off..].to_vec();

            Ok((
                PacketHeader::Long(LongHeader {
                    version,
                    dst_conn_id,
                    src_conn_id,
                    packet_number,
                    payload,
                }),
                off,
            ))
        } else {
            if first & FLAG_FIXED_BIT == 0 {
                return Err(AtpError::protocol("short header: fixed bit not set"));
            }
            let mut off = 1;
            if off + CONNECTION_ID_LEN > buf.len() {
                return Err(AtpError::protocol("short header: CID truncated"));
            }
            let mut dst_conn_id = [0u8; CONNECTION_ID_LEN];
            dst_conn_id.copy_from_slice(&buf[off..off + CONNECTION_ID_LEN]);
            off += CONNECTION_ID_LEN;

            let (packet_number, pn_len) = decode_varint(buf, off)?;
            off += pn_len;
            let payload = buf[off..].to_vec();

            Ok((
                PacketHeader::Short(ShortHeader {
                    dst_conn_id,
                    packet_number,
                    payload,
                }),
                off,
            ))
        }
    }

    pub fn encode_long(h: &LongHeader) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + h.payload.len());
        out.push(FLAG_LONG_HEADER | FLAG_FIXED_BIT);
        out.extend_from_slice(&h.version.to_be_bytes());
        out.push(h.dst_conn_id.len() as u8);
        out.extend_from_slice(&h.dst_conn_id);
        out.push(h.src_conn_id.len() as u8);
        out.extend_from_slice(&h.src_conn_id);
        push_varint(&mut out, h.packet_number);
        out.extend_from_slice(&h.payload);
        out
    }

    pub fn encode_short(h: &ShortHeader) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + h.payload.len());
        out.push(FLAG_FIXED_BIT);
        out.extend_from_slice(&h.dst_conn_id);
        push_varint(&mut out, h.packet_number);
        out.extend_from_slice(&h.payload);
        out
    }

    pub fn packet_number(&self) -> u64 {
        match self {
            PacketHeader::Long(h) => h.packet_number,
            PacketHeader::Short(h) => h.packet_number,
        }
    }

    pub fn payload(&self) -> &[u8] {
        match self {
            PacketHeader::Long(h) => &h.payload,
            PacketHeader::Short(h) => &h.payload,
        }
    }
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    ConnectionInit {
        version: u32,
        src_conn_id: Vec<u8>,
        ephemeral_pubkey: Vec<u8>,
        transport_params: Vec<u8>,
        retry_token: Vec<u8>,
        identity_proof: Vec<u8>,
    },
    ConnectionInitAck {
        version: u32,
        src_conn_id: Vec<u8>,
        dst_conn_id: Vec<u8>,
        ephemeral_pubkey: Vec<u8>,
        transport_params: Vec<u8>,
        identity_proof: Vec<u8>,
    },
    Handshake {
        confirm: Vec<u8>,
    },
    Data {
        stream_id: u64,
        message_id: u64,
        fragment_num: u64,
        total_fragments: u64,
        flags: u8,
        payload: Vec<u8>,
    },
    Ack {
        largest_acked: u64,
        ack_delay: u64,
        ranges: Vec<(u64, u64)>,
    },
    StreamOpen {
        stream_id: u64,
    },
    StreamClose {
        stream_id: u64,
    },
    StreamReset {
        stream_id: u64,
        reason: u64,
    },
    MaxData {
        max_data: u64,
    },
    MaxStreamData {
        stream_id: u64,
        max_data: u64,
    },
    Ping,
    Pong,
    PathChallenge {
        data: [u8; 8],
    },
    PathResponse {
        data: [u8; 8],
    },
    ConnectionClose {
        error_code: u64,
        reason: String,
    },
    KeyUpdate {
        epoch: u64,
    },
    Cancel {
        stream_id: u64,
        message_id: u64,
    },
    Datagram {
        payload: Vec<u8>,
    },
    /// Server-issued retry token request (stateless handshake validation).
    Retry {
        token: Vec<u8>,
    },
}

impl Frame {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Frame::ConnectionInit {
                version,
                src_conn_id,
                ephemeral_pubkey,
                transport_params,
                retry_token,
                identity_proof,
            } => {
                out.push(FrameType::ConnectionInit as u8);
                out.extend_from_slice(&version.to_be_bytes());
                out.push(src_conn_id.len() as u8);
                out.extend_from_slice(src_conn_id);
                out.push(ephemeral_pubkey.len() as u8);
                out.extend_from_slice(ephemeral_pubkey);
                push_varint(&mut out, transport_params.len() as u64);
                out.extend_from_slice(transport_params);
                push_varint(&mut out, retry_token.len() as u64);
                out.extend_from_slice(retry_token);
                push_varint(&mut out, identity_proof.len() as u64);
                out.extend_from_slice(identity_proof);
            }
            Frame::ConnectionInitAck {
                version,
                src_conn_id,
                dst_conn_id,
                ephemeral_pubkey,
                transport_params,
                identity_proof,
            } => {
                out.push(FrameType::ConnectionInitAck as u8);
                out.extend_from_slice(&version.to_be_bytes());
                out.push(src_conn_id.len() as u8);
                out.extend_from_slice(src_conn_id);
                out.push(dst_conn_id.len() as u8);
                out.extend_from_slice(dst_conn_id);
                out.push(ephemeral_pubkey.len() as u8);
                out.extend_from_slice(ephemeral_pubkey);
                push_varint(&mut out, transport_params.len() as u64);
                out.extend_from_slice(transport_params);
                push_varint(&mut out, identity_proof.len() as u64);
                out.extend_from_slice(identity_proof);
            }
            Frame::Handshake { confirm } => {
                out.push(FrameType::Handshake as u8);
                push_varint(&mut out, confirm.len() as u64);
                out.extend_from_slice(confirm);
            }
            Frame::Data {
                stream_id,
                message_id,
                fragment_num,
                total_fragments,
                flags,
                payload,
            } => {
                out.push(FrameType::Data as u8);
                push_varint(&mut out, *stream_id);
                push_varint(&mut out, *message_id);
                push_varint(&mut out, *fragment_num);
                push_varint(&mut out, *total_fragments);
                out.push(*flags);
                push_varint(&mut out, payload.len() as u64);
                out.extend_from_slice(payload);
            }
            Frame::Ack {
                largest_acked,
                ack_delay,
                ranges,
            } => {
                out.push(FrameType::Ack as u8);
                push_varint(&mut out, *largest_acked);
                push_varint(&mut out, *ack_delay);
                push_varint(&mut out, ranges.len() as u64);
                for (gap, range_len) in ranges {
                    push_varint(&mut out, *gap);
                    push_varint(&mut out, *range_len);
                }
            }
            Frame::StreamOpen { stream_id } => {
                out.push(FrameType::StreamOpen as u8);
                push_varint(&mut out, *stream_id);
            }
            Frame::StreamClose { stream_id } => {
                out.push(FrameType::StreamClose as u8);
                push_varint(&mut out, *stream_id);
            }
            Frame::StreamReset { stream_id, reason } => {
                out.push(FrameType::StreamReset as u8);
                push_varint(&mut out, *stream_id);
                push_varint(&mut out, *reason);
            }
            Frame::MaxData { max_data } => {
                out.push(FrameType::MaxData as u8);
                push_varint(&mut out, *max_data);
            }
            Frame::MaxStreamData { stream_id, max_data } => {
                out.push(FrameType::MaxStreamData as u8);
                push_varint(&mut out, *stream_id);
                push_varint(&mut out, *max_data);
            }
            Frame::Ping => out.push(FrameType::Ping as u8),
            Frame::Pong => out.push(FrameType::Pong as u8),
            Frame::PathChallenge { data } => {
                out.push(FrameType::PathChallenge as u8);
                out.extend_from_slice(data);
            }
            Frame::PathResponse { data } => {
                out.push(FrameType::PathResponse as u8);
                out.extend_from_slice(data);
            }
            Frame::ConnectionClose { error_code, reason } => {
                out.push(FrameType::ConnectionClose as u8);
                push_varint(&mut out, *error_code);
                push_varint(&mut out, reason.len() as u64);
                out.extend_from_slice(reason.as_bytes());
            }
            Frame::KeyUpdate { epoch } => {
                out.push(FrameType::KeyUpdate as u8);
                push_varint(&mut out, *epoch);
            }
            Frame::Cancel {
                stream_id,
                message_id,
            } => {
                out.push(FrameType::Cancel as u8);
                push_varint(&mut out, *stream_id);
                push_varint(&mut out, *message_id);
            }
            Frame::Datagram { payload } => {
                out.push(FrameType::Datagram as u8);
                push_varint(&mut out, payload.len() as u64);
                out.extend_from_slice(payload);
            }
            Frame::Retry { token } => {
                out.push(FrameType::Retry as u8);
                push_varint(&mut out, token.len() as u64);
                out.extend_from_slice(token);
            }
        }
        out
    }

    pub fn decode(buf: &[u8], offset: usize) -> AtpResult<(Frame, usize)> {
        if offset >= buf.len() {
            return Err(AtpError::protocol("frame: buffer too short"));
        }
        let ft = buf[offset];
        let frame_type = FrameType::from_u8(ft)
            .ok_or_else(|| AtpError::protocol(format!("unknown frame type 0x{:02X}", ft)))?;
        let mut off = offset + 1;

        macro_rules! read_varint {
            () => {{
                let (v, l) = decode_varint(buf, off)?;
                off += l;
                v
            }};
        }
        macro_rules! read_bytes {
            ($len:expr) => {{
                let len = $len as usize;
                if len > MAX_FRAME_PAYLOAD {
                    return Err(AtpError::protocol("frame field too large"));
                }
                if off + len > buf.len() {
                    return Err(AtpError::protocol("frame: field truncated"));
                }
                let data = buf[off..off + len].to_vec();
                off += len;
                data
            }};
        }

        let frame = match frame_type {
            FrameType::ConnectionInit => {
                if off + 4 > buf.len() {
                    return Err(AtpError::protocol("ConnectionInit: version truncated"));
                }
                let version = u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                off += 4;
                if off >= buf.len() {
                    return Err(AtpError::protocol("ConnectionInit: CID length truncated"));
                }
                let src_cid_len = buf[off] as usize;
                off += 1;
                if src_cid_len > MAX_CID_LEN {
                    return Err(AtpError::protocol("CID too long"));
                }
                let src_conn_id = read_bytes!(src_cid_len);
                if off >= buf.len() {
                    return Err(AtpError::protocol("ConnectionInit: eph len truncated"));
                }
                let eph_len = buf[off] as usize;
                off += 1;
                if eph_len != 32 {
                    return Err(AtpError::protocol("invalid ephemeral key length"));
                }
                let ephemeral_pubkey = read_bytes!(eph_len);
                let tp_len = read_varint!() as usize;
                if tp_len > MAX_HANDSHAKE_SIZE {
                    return Err(AtpError::protocol("transport params too large"));
                }
                let transport_params = read_bytes!(tp_len);
                let retry_token = if off < buf.len() {
                    let rt_len = read_varint!() as usize;
                    if rt_len > 128 {
                        return Err(AtpError::protocol("retry token too large"));
                    }
                    read_bytes!(rt_len)
                } else {
                    Vec::new()
                };
                let identity_proof = if off < buf.len() {
                    let id_len = read_varint!() as usize;
                    if id_len > super::identity::IDENTITY_PROOF_LEN {
                        return Err(AtpError::protocol("identity proof too large"));
                    }
                    read_bytes!(id_len)
                } else {
                    Vec::new()
                };
                Frame::ConnectionInit {
                    version,
                    src_conn_id,
                    ephemeral_pubkey,
                    transport_params,
                    retry_token,
                    identity_proof,
                }
            }
            FrameType::ConnectionInitAck => {
                if off + 4 > buf.len() {
                    return Err(AtpError::protocol("ConnectionInitAck: version truncated"));
                }
                let version = u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                off += 4;
                if off >= buf.len() {
                    return Err(AtpError::protocol("ConnectionInitAck: src CID len truncated"));
                }
                let src_cid_len = buf[off] as usize;
                off += 1;
                let src_conn_id = read_bytes!(src_cid_len);
                if off >= buf.len() {
                    return Err(AtpError::protocol("ConnectionInitAck: dst CID len truncated"));
                }
                let dst_cid_len = buf[off] as usize;
                off += 1;
                let dst_conn_id = read_bytes!(dst_cid_len);
                if off >= buf.len() {
                    return Err(AtpError::protocol("ConnectionInitAck: eph len truncated"));
                }
                let eph_len = buf[off] as usize;
                off += 1;
                if eph_len != 32 {
                    return Err(AtpError::protocol("invalid ephemeral key length"));
                }
                let ephemeral_pubkey = read_bytes!(eph_len);
                let tp_len = read_varint!() as usize;
                let transport_params = read_bytes!(tp_len);
                let identity_proof = if off < buf.len() {
                    let id_len = read_varint!() as usize;
                    if id_len > super::identity::IDENTITY_PROOF_LEN {
                        return Err(AtpError::protocol("identity proof too large"));
                    }
                    read_bytes!(id_len)
                } else {
                    Vec::new()
                };
                Frame::ConnectionInitAck {
                    version,
                    src_conn_id,
                    dst_conn_id,
                    ephemeral_pubkey,
                    transport_params,
                    identity_proof,
                }
            }
            FrameType::Handshake => {
                let len = read_varint!() as usize;
                if len != 32 {
                    return Err(AtpError::protocol("invalid Finished length"));
                }
                let confirm = read_bytes!(len);
                Frame::Handshake { confirm }
            }
            FrameType::Data => {
                let stream_id = read_varint!();
                let message_id = read_varint!();
                let fragment_num = read_varint!();
                let total_fragments = read_varint!();
                if off >= buf.len() {
                    return Err(AtpError::protocol("Data: flags truncated"));
                }
                let flags = buf[off];
                off += 1;
                let payload_len = read_varint!() as usize;
                if payload_len > MAX_FRAME_PAYLOAD {
                    return Err(AtpError::protocol("data payload too large"));
                }
                let payload = read_bytes!(payload_len);
                Frame::Data {
                    stream_id,
                    message_id,
                    fragment_num,
                    total_fragments,
                    flags,
                    payload,
                }
            }
            FrameType::Ack => {
                let largest_acked = read_varint!();
                let ack_delay = read_varint!();
                let range_count = read_varint!() as usize;
                if range_count > MAX_ACK_RANGES {
                    return Err(AtpError::protocol("too many ACK ranges"));
                }
                let mut ranges = Vec::with_capacity(range_count);
                for _ in 0..range_count {
                    let gap = read_varint!();
                    let range_len = read_varint!();
                    ranges.push((gap, range_len));
                }
                Frame::Ack {
                    largest_acked,
                    ack_delay,
                    ranges,
                }
            }
            FrameType::StreamOpen => Frame::StreamOpen {
                stream_id: read_varint!(),
            },
            FrameType::StreamClose => Frame::StreamClose {
                stream_id: read_varint!(),
            },
            FrameType::StreamReset => Frame::StreamReset {
                stream_id: read_varint!(),
                reason: read_varint!(),
            },
            FrameType::MaxData => Frame::MaxData {
                max_data: read_varint!(),
            },
            FrameType::MaxStreamData => Frame::MaxStreamData {
                stream_id: read_varint!(),
                max_data: read_varint!(),
            },
            FrameType::Ping => Frame::Ping,
            FrameType::Pong => Frame::Pong,
            FrameType::PathChallenge => {
                if off + 8 > buf.len() {
                    return Err(AtpError::protocol("PathChallenge: truncated"));
                }
                let mut data = [0u8; 8];
                data.copy_from_slice(&buf[off..off + 8]);
                off += 8;
                Frame::PathChallenge { data }
            }
            FrameType::PathResponse => {
                if off + 8 > buf.len() {
                    return Err(AtpError::protocol("PathResponse: truncated"));
                }
                let mut data = [0u8; 8];
                data.copy_from_slice(&buf[off..off + 8]);
                off += 8;
                Frame::PathResponse { data }
            }
            FrameType::ConnectionClose => {
                let error_code = read_varint!();
                let reason_len = read_varint!() as usize;
                if reason_len > MAX_REASON_LENGTH {
                    return Err(AtpError::protocol("reason too long"));
                }
                let reason_bytes = read_bytes!(reason_len);
                let reason = String::from_utf8_lossy(&reason_bytes).into_owned();
                Frame::ConnectionClose { error_code, reason }
            }
            FrameType::KeyUpdate => Frame::KeyUpdate {
                epoch: read_varint!(),
            },
            FrameType::Cancel => Frame::Cancel {
                stream_id: read_varint!(),
                message_id: read_varint!(),
            },
            FrameType::Datagram => {
                let payload_len = read_varint!() as usize;
                if payload_len > MAX_DATAGRAM_SIZE {
                    return Err(AtpError::protocol("datagram too large"));
                }
                let payload = read_bytes!(payload_len);
                Frame::Datagram { payload }
            }
            FrameType::Retry => {
                let token_len = read_varint!() as usize;
                if token_len > 128 {
                    return Err(AtpError::protocol("retry token too large"));
                }
                let token = read_bytes!(token_len);
                Frame::Retry { token }
            }
        };

        Ok((frame, off - offset))
    }

    pub fn decode_all(buf: &[u8]) -> AtpResult<Vec<Frame>> {
        let mut frames = Vec::new();
        let mut off = 0;
        while off < buf.len() {
            let (frame, consumed) = Self::decode(buf, off)?;
            off += consumed;
            frames.push(frame);
        }
        Ok(frames)
    }

    pub fn encode_all(frames: &[Frame]) -> Vec<u8> {
        let mut out = Vec::new();
        for f in frames {
            out.extend_from_slice(&f.encode());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        let values = [
            0u64, 1, 63, 64, 16383, 16384, 1_073_741_823, 1_073_741_824,
            (1u64 << 30) - 1, 1u64 << 30, (1u64 << 62) - 1,
        ];
        for &v in &values {
            let enc = encode_varint(v);
            let (dec, len) = decode_varint(&enc, 0).unwrap();
            assert_eq!(dec, v, "varint roundtrip failed for {}", v);
            assert_eq!(len, enc.len());
        }
    }

    #[test]
    fn test_varint_non_canonical_rejected() {
        // Value 0 encoded as 2-byte form is non-canonical.
        let non_canonical = [0x40, 0x00];
        assert!(decode_varint(&non_canonical, 0).is_err());
    }

    #[test]
    fn test_varint_truncated_rejected() {
        assert!(decode_varint(&[0x40], 0).is_err());
    }

    #[test]
    fn test_short_header_roundtrip() {
        let h = ShortHeader {
            dst_conn_id: [0xAA; CONNECTION_ID_LEN],
            packet_number: 999,
            payload: vec![1, 2, 3],
        };
        let enc = PacketHeader::encode_short(&h);
        let (decoded, _) = PacketHeader::decode(&enc).unwrap();
        match decoded {
            PacketHeader::Short(s) => {
                assert_eq!(s.dst_conn_id, [0xAA; CONNECTION_ID_LEN]);
                assert_eq!(s.packet_number, 999);
                assert_eq!(s.payload, vec![1, 2, 3]);
            }
            _ => panic!("expected short header"),
        }
    }

    #[test]
    fn test_all_frames_roundtrip() {
        let frames = vec![
            Frame::Ping,
            Frame::Pong,
            Frame::StreamOpen { stream_id: 5 },
            Frame::StreamClose { stream_id: 5 },
            Frame::StreamReset { stream_id: 5, reason: 1 },
            Frame::MaxData { max_data: 1_000_000 },
            Frame::MaxStreamData { stream_id: 1, max_data: 500_000 },
            Frame::PathChallenge { data: [1; 8] },
            Frame::PathResponse { data: [2; 8] },
            Frame::KeyUpdate { epoch: 1 },
            Frame::Cancel { stream_id: 1, message_id: 2 },
            Frame::Datagram { payload: b"dg".to_vec() },
            Frame::ConnectionClose { error_code: 0, reason: "bye".to_string() },
            Frame::Handshake { confirm: vec![0; 32] },
            Frame::Data {
                stream_id: 1,
                message_id: 1,
                fragment_num: 0,
                total_fragments: 1,
                flags: DATA_FLAG_RELIABLE | DATA_FLAG_FIRST | DATA_FLAG_FIN,
                payload: b"hi".to_vec(),
            },
            Frame::Ack {
                largest_acked: 10,
                ack_delay: 5,
                ranges: vec![(0, 2)],
            },
        ];
        for f in &frames {
            let enc = f.encode();
            let (decoded, _) = Frame::decode(&enc, 0).unwrap();
            assert_eq!(&decoded, f);
        }
    }
}
