//! Adaptive Compression & Payload Classification for AdeshLang.

use super::checksums::crc32;
use super::codecs::{CodecType, compress_bytes};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataKind {
    Text,
    Json,
    Binary,
    Repetitive,
    AlreadyCompressed,
    EncryptedOrRandom,
}

pub fn entropy_estimate(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }
    let len_f = data.len() as f64;
    let mut entropy = 0.0f64;
    for &count in counts.iter() {
        if count > 0 {
            let p = count as f64 / len_f;
            entropy -= p * p.log2();
        }
    }
    entropy
}

pub fn classify_data(data: &[u8]) -> DataKind {
    if data.is_empty() {
        return DataKind::Text;
    }

    let entropy = entropy_estimate(data);

    // Check for magic headers of compressed files
    if data.len() >= 4 {
        if data.starts_with(b"\x1f\x8b")
            || data.starts_with(b"\x28\xb5\x2f\xfd")
            || data.starts_with(b"PK\x03\x04")
            || data.starts_with(b"\xfd7zXZ")
        {
            return DataKind::AlreadyCompressed;
        }
    }

    if entropy >= 7.8 {
        return DataKind::EncryptedOrRandom;
    }

    let ascii_count = data
        .iter()
        .filter(|&&b| b.is_ascii_graphic() || b.is_ascii_whitespace())
        .count();
    let ascii_ratio = ascii_count as f64 / data.len() as f64;

    if ascii_ratio > 0.90 {
        if data.trim_ascii_start().starts_with(b"{") || data.trim_ascii_start().starts_with(b"[") {
            return DataKind::Json;
        }
        return DataKind::Text;
    }

    if entropy < 4.0 {
        return DataKind::Repetitive;
    }

    DataKind::Binary
}

#[derive(Debug, Clone)]
pub struct AdaptiveResult {
    pub compressed_data: Vec<u8>,
    pub original_size: usize,
    pub compressed_size: usize,
    pub ratio: f64,
    pub space_saved_percent: f64,
    pub algorithm: String,
    pub was_compressed: bool,
    pub checksum: u32,
}

pub fn compress_adaptive(
    data: &[u8],
    force_codec: Option<CodecType>,
) -> Result<AdaptiveResult, String> {
    let original_size = data.len();
    let original_crc = crc32(data);

    if original_size == 0 {
        return Ok(AdaptiveResult {
            compressed_data: Vec::new(),
            original_size: 0,
            compressed_size: 0,
            ratio: 1.0,
            space_saved_percent: 0.0,
            algorithm: "None".to_string(),
            was_compressed: false,
            checksum: original_crc,
        });
    }

    let kind = classify_data(data);

    // Skip compression if already compressed or high entropy random/encrypted
    if kind == DataKind::AlreadyCompressed || kind == DataKind::EncryptedOrRandom {
        return Ok(AdaptiveResult {
            compressed_data: data.to_vec(),
            original_size,
            compressed_size: original_size,
            ratio: 1.0,
            space_saved_percent: 0.0,
            algorithm: "Passthrough".to_string(),
            was_compressed: false,
            checksum: original_crc,
        });
    }

    let codec = force_codec.unwrap_or(match kind {
        DataKind::Text | DataKind::Json => CodecType::Brotli,
        DataKind::Repetitive => CodecType::Lz4,
        DataKind::Binary | _ => CodecType::Zstd,
    });

    let level = match codec {
        CodecType::Brotli => 6,
        CodecType::Zstd => 5,
        _ => 3,
    };

    let compressed = compress_bytes(codec, data, level)?;

    if compressed.len() >= original_size {
        // Skip compression if output would expand
        Ok(AdaptiveResult {
            compressed_data: data.to_vec(),
            original_size,
            compressed_size: original_size,
            ratio: 1.0,
            space_saved_percent: 0.0,
            algorithm: "Passthrough".to_string(),
            was_compressed: false,
            checksum: original_crc,
        })
    } else {
        let compressed_size = compressed.len();
        let ratio = compressed_size as f64 / original_size as f64;
        let space_saved_percent = (1.0 - ratio) * 100.0;

        Ok(AdaptiveResult {
            compressed_data: compressed,
            original_size,
            compressed_size,
            ratio,
            space_saved_percent,
            algorithm: codec.as_str().to_string(),
            was_compressed: true,
            checksum: original_crc,
        })
    }
}
