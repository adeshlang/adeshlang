//! Stateful Streaming Compression and Decompression for AdeshLang.

use super::checksums::crc32;
use super::codecs::{CodecType, compress_bytes, decompress_bytes};

#[derive(Debug, Clone)]
pub struct StreamStats {
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub blocks_processed: u64,
    pub crc32_checksum: u32,
}

pub struct StreamEncoder {
    codec: CodecType,
    level: i32,
    buffer: Vec<u8>,
    chunk_size: usize,
    finished: bool,
    stats: StreamStats,
}

impl StreamEncoder {
    pub fn new(codec: CodecType, level: i32, chunk_size: usize) -> Self {
        Self {
            codec,
            level,
            buffer: Vec::new(),
            chunk_size: if chunk_size == 0 { 65536 } else { chunk_size },
            finished: false,
            stats: StreamStats {
                bytes_read: 0,
                bytes_written: 0,
                blocks_processed: 0,
                crc32_checksum: 0,
            },
        }
    }

    pub fn write(&mut self, chunk: &[u8]) -> Result<Vec<u8>, String> {
        if self.finished {
            return Err("StreamEncoder has already finished".to_string());
        }

        self.buffer.extend_from_slice(chunk);
        self.stats.bytes_read += chunk.len() as u64;

        let mut output = Vec::new();
        while self.buffer.len() >= self.chunk_size {
            let block = self.buffer.drain(..self.chunk_size).collect::<Vec<u8>>();
            let compressed = compress_bytes(self.codec, &block, self.level)?;
            output.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            output.extend_from_slice(&compressed);
            self.stats.bytes_written += (4 + compressed.len()) as u64;
            self.stats.blocks_processed += 1;
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, String> {
        if self.finished {
            return Ok(Vec::new());
        }

        self.finished = true;
        let mut output = Vec::new();
        if !self.buffer.is_empty() {
            let block = std::mem::take(&mut self.buffer);
            let compressed = compress_bytes(self.codec, &block, self.level)?;
            output.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            output.extend_from_slice(&compressed);
            self.stats.bytes_written += (4 + compressed.len()) as u64;
            self.stats.blocks_processed += 1;
        }

        // Final footer: 0 len block indicates EOF
        output.extend_from_slice(&0u32.to_le_bytes());
        self.stats.bytes_written += 4;
        Ok(output)
    }

    pub fn stats(&self) -> StreamStats {
        self.stats.clone()
    }
}

pub struct StreamDecoder {
    codec: CodecType,
    max_output_size: Option<usize>,
    pending_bytes: Vec<u8>,
    stats: StreamStats,
}

impl StreamDecoder {
    pub fn new(codec: CodecType, max_output_size: Option<usize>) -> Self {
        Self {
            codec,
            max_output_size,
            pending_bytes: Vec::new(),
            stats: StreamStats {
                bytes_read: 0,
                bytes_written: 0,
                blocks_processed: 0,
                crc32_checksum: 0,
            },
        }
    }

    pub fn read_chunk(&mut self, chunk: &[u8]) -> Result<Vec<u8>, String> {
        self.pending_bytes.extend_from_slice(chunk);
        self.stats.bytes_read += chunk.len() as u64;

        let mut output = Vec::new();
        while self.pending_bytes.len() >= 4 {
            let len_bytes: [u8; 4] = self.pending_bytes[..4].try_into().unwrap();
            let block_len = u32::from_le_bytes(len_bytes) as usize;

            if block_len == 0 {
                // End of stream signal
                self.pending_bytes.drain(..4);
                break;
            }

            if self.pending_bytes.len() < 4 + block_len {
                // Incomplete block, wait for more bytes
                break;
            }

            let block = self
                .pending_bytes
                .drain(4..4 + block_len)
                .collect::<Vec<u8>>();
            self.pending_bytes.drain(..4); // remove length header

            let decompressed = decompress_bytes(self.codec, &block, self.max_output_size)?;
            output.extend_from_slice(&decompressed);

            self.stats.bytes_written += decompressed.len() as u64;
            self.stats.blocks_processed += 1;
        }

        self.stats.crc32_checksum = crc32(&output);
        Ok(output)
    }

    pub fn stats(&self) -> StreamStats {
        self.stats.clone()
    }
}
