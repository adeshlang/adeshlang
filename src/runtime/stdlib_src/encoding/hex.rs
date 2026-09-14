//! Hexadecimal Encoding for AdeshLang.

/// Encode bytes to a lowercase hex string.
pub fn hex_encode(data: &[u8]) -> String {
    hex::encode(data)
}

/// Encode bytes to an uppercase hex string.
pub fn hex_encode_upper(data: &[u8]) -> String {
    hex::encode_upper(data)
}

/// Decode hex string to bytes. Rejects odd lengths and invalid hex characters.
pub fn hex_decode(hex_str: &str) -> Result<Vec<u8>, String> {
    let clean = hex_str.trim();
    hex::decode(clean).map_err(|e| format!("Hex decoding error: {}", e))
}

/// Stateful Streaming Hex Encoder
pub struct HexEncoder {
    uppercase: bool,
}

impl HexEncoder {
    pub fn new(uppercase: bool) -> Self {
        Self { uppercase }
    }

    pub fn encode_chunk(&self, chunk: &[u8]) -> String {
        if self.uppercase {
            hex_encode_upper(chunk)
        } else {
            hex_encode(chunk)
        }
    }
}

/// Stateful Streaming Hex Decoder
pub struct HexDecoder {
    buffer: String,
}

impl HexDecoder {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    pub fn write_chunk(&mut self, chunk: &str) {
        self.buffer.push_str(chunk.trim());
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        hex_decode(&self.buffer)
    }
}
