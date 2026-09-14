use super::frames::encode_varint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpDatagram {
    pub quarter_stream_id: u64,
    pub payload: Vec<u8>,
}

impl HttpDatagram {
    pub fn new(quarter_stream_id: u64, payload: Vec<u8>) -> Self {
        Self {
            quarter_stream_id,
            payload,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        encode_varint(self.quarter_stream_id, &mut out);
        out.extend_from_slice(&self.payload);
        out
    }
}
