//! Pre-Shared & Trained Compression Dictionaries for AdeshLang.

use super::checksums::crc32;
use super::codecs::{compress_zstd_dict, decompress_zstd_dict};

#[derive(Debug, Clone)]
pub struct CompressionDictionary {
    pub id: u32,
    pub data: Vec<u8>,
}

impl CompressionDictionary {
    pub fn new(data: Vec<u8>) -> Self {
        let id = crc32(&data);
        Self { id, data }
    }

    pub fn train_from_samples(
        samples: &[Vec<u8>],
        target_dict_size: usize,
    ) -> Result<Self, String> {
        if samples.is_empty() {
            return Err("Cannot train dictionary from empty sample list".to_string());
        }

        let dict_size = if target_dict_size == 0 {
            65536
        } else {
            target_dict_size
        };

        let sample_sizes: Vec<usize> = samples.iter().map(|s| s.len()).collect();
        let mut flat_samples = Vec::new();
        for sample in samples {
            flat_samples.extend_from_slice(sample);
        }

        let trained_bytes = zstd::dict::from_continuous(&flat_samples, &sample_sizes, dict_size)
            .map_err(|e| format!("Failed to train Zstd dictionary: {}", e))?;

        Ok(Self::new(trained_bytes))
    }

    pub fn compress(&self, input: &[u8], level: i32) -> Result<Vec<u8>, String> {
        let mut result = Vec::new();
        // Embed dictionary ID header (4 bytes LE)
        result.extend_from_slice(&self.id.to_le_bytes());
        let compressed = compress_zstd_dict(input, level, &self.data)?;
        result.extend_from_slice(&compressed);
        Ok(result)
    }

    pub fn decompress(
        &self,
        input: &[u8],
        max_output_size: Option<usize>,
    ) -> Result<Vec<u8>, String> {
        if input.len() < 4 {
            return Err("Invalid dictionary compressed payload: missing header".to_string());
        }

        let embedded_id = u32::from_le_bytes(input[..4].try_into().unwrap());
        if embedded_id != self.id {
            return Err(format!(
                "Dictionary ID mismatch: payload requires dictionary ID 0x{:08X}, provided dictionary ID 0x{:08X}",
                embedded_id, self.id
            ));
        }

        decompress_zstd_dict(&input[4..], &self.data, max_output_size)
    }
}
