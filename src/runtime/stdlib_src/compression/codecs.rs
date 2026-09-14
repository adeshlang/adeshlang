//! Lossless Compression Codecs Implementation for AdeshLang.
//! Supports DEFLATE, GZIP, ZLIB, Brotli, Zstandard, LZ4, XZ/LZMA.

use flate2::Compression as FlateCompression;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use flate2::write::{DeflateEncoder, GzEncoder, ZlibEncoder};
use std::io::{Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecType {
    Deflate,
    Gzip,
    Zlib,
    Brotli,
    Zstd,
    Lz4,
    Xz,
}

impl CodecType {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "deflate" => Ok(CodecType::Deflate),
            "gzip" | "gz" => Ok(CodecType::Gzip),
            "zlib" => Ok(CodecType::Zlib),
            "brotli" | "br" => Ok(CodecType::Brotli),
            "zstd" | "zst" => Ok(CodecType::Zstd),
            "lz4" => Ok(CodecType::Lz4),
            "xz" | "lzma" => Ok(CodecType::Xz),
            _ => Err(format!("Unsupported compression codec: {}", s)),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CodecType::Deflate => "Deflate",
            CodecType::Gzip => "Gzip",
            CodecType::Zlib => "Zlib",
            CodecType::Brotli => "Brotli",
            CodecType::Zstd => "Zstd",
            CodecType::Lz4 => "LZ4",
            CodecType::Xz => "XZ",
        }
    }
}

pub fn compress_bound(codec: CodecType, input_len: usize) -> usize {
    match codec {
        CodecType::Deflate | CodecType::Gzip | CodecType::Zlib => input_len + (input_len / 8) + 128,
        CodecType::Brotli => input_len + (input_len / 4) + 1024,
        CodecType::Zstd => zstd::zstd_safe::compress_bound(input_len),
        CodecType::Lz4 => lz4_flex::block::get_maximum_output_size(input_len),
        CodecType::Xz => input_len + (input_len / 5) + 2048,
    }
}

// ----------------------------------------------------------------------------
// DEFLATE / GZIP / ZLIB
// ----------------------------------------------------------------------------

pub fn compress_deflate(input: &[u8], level: u32) -> Result<Vec<u8>, String> {
    let lvl = FlateCompression::new(level.min(9));
    let mut encoder = DeflateEncoder::new(Vec::new(), lvl);
    encoder
        .write_all(input)
        .map_err(|e| format!("DEFLATE compression failed: {}", e))?;
    encoder
        .finish()
        .map_err(|e| format!("DEFLATE finish failed: {}", e))
}

pub fn decompress_deflate(input: &[u8], max_output_size: Option<usize>) -> Result<Vec<u8>, String> {
    let mut decoder = DeflateDecoder::new(input);
    read_with_limit(&mut decoder, max_output_size, "DEFLATE")
}

pub fn compress_gzip(input: &[u8], level: u32) -> Result<Vec<u8>, String> {
    let lvl = FlateCompression::new(level.min(9));
    let mut encoder = GzEncoder::new(Vec::new(), lvl);
    encoder
        .write_all(input)
        .map_err(|e| format!("GZIP compression failed: {}", e))?;
    encoder
        .finish()
        .map_err(|e| format!("GZIP finish failed: {}", e))
}

pub fn decompress_gzip(input: &[u8], max_output_size: Option<usize>) -> Result<Vec<u8>, String> {
    let mut decoder = GzDecoder::new(input);
    read_with_limit(&mut decoder, max_output_size, "GZIP")
}

pub fn compress_zlib(input: &[u8], level: u32) -> Result<Vec<u8>, String> {
    let lvl = FlateCompression::new(level.min(9));
    let mut encoder = ZlibEncoder::new(Vec::new(), lvl);
    encoder
        .write_all(input)
        .map_err(|e| format!("ZLIB compression failed: {}", e))?;
    encoder
        .finish()
        .map_err(|e| format!("ZLIB finish failed: {}", e))
}

pub fn decompress_zlib(input: &[u8], max_output_size: Option<usize>) -> Result<Vec<u8>, String> {
    let mut decoder = ZlibDecoder::new(input);
    read_with_limit(&mut decoder, max_output_size, "ZLIB")
}

// ----------------------------------------------------------------------------
// Brotli
// ----------------------------------------------------------------------------

pub fn compress_brotli(input: &[u8], level: u32) -> Result<Vec<u8>, String> {
    let params = brotli::enc::backward_references::BrotliEncoderParams {
        quality: level.min(11) as i32,
        ..Default::default()
    };
    let mut output = Vec::new();
    brotli::BrotliCompress(&mut &input[..], &mut output, &params)
        .map_err(|e| format!("Brotli compression failed: {}", e))?;
    Ok(output)
}

pub fn decompress_brotli(input: &[u8], max_output_size: Option<usize>) -> Result<Vec<u8>, String> {
    let mut decoder = brotli::Decompressor::new(input, 4096);
    read_with_limit(&mut decoder, max_output_size, "Brotli")
}

// ----------------------------------------------------------------------------
// Zstandard
// ----------------------------------------------------------------------------

pub fn compress_zstd(input: &[u8], level: i32) -> Result<Vec<u8>, String> {
    let lvl = if level <= 0 { 3 } else { level.min(22) };
    zstd::encode_all(input, lvl).map_err(|e| format!("Zstd compression failed: {}", e))
}

pub fn decompress_zstd(input: &[u8], max_output_size: Option<usize>) -> Result<Vec<u8>, String> {
    let mut decoder =
        zstd::Decoder::new(input).map_err(|e| format!("Zstd decoder init failed: {}", e))?;
    read_with_limit(&mut decoder, max_output_size, "Zstd")
}

pub fn compress_zstd_dict(input: &[u8], level: i32, dict: &[u8]) -> Result<Vec<u8>, String> {
    let lvl = if level <= 0 { 3 } else { level.min(22) };
    let mut encoder = zstd::stream::Encoder::with_dictionary(Vec::new(), lvl, dict)
        .map_err(|e| format!("Zstd dict encoder init failed: {}", e))?;
    encoder
        .write_all(input)
        .map_err(|e| format!("Zstd dict write failed: {}", e))?;
    encoder
        .finish()
        .map_err(|e| format!("Zstd dict finish failed: {}", e))
}

pub fn decompress_zstd_dict(
    input: &[u8],
    dict: &[u8],
    max_output_size: Option<usize>,
) -> Result<Vec<u8>, String> {
    let mut decoder = zstd::stream::Decoder::with_dictionary(input, dict)
        .map_err(|e| format!("Zstd dict decoder init failed: {}", e))?;
    read_with_limit(&mut decoder, max_output_size, "Zstd Dict")
}

// ----------------------------------------------------------------------------
// LZ4
// ----------------------------------------------------------------------------

pub fn compress_lz4(input: &[u8]) -> Result<Vec<u8>, String> {
    let compressed = lz4_flex::block::compress_prepend_size(input);
    Ok(compressed)
}

pub fn decompress_lz4(input: &[u8], max_output_size: Option<usize>) -> Result<Vec<u8>, String> {
    let decompressed = lz4_flex::block::decompress_size_prepended(input)
        .map_err(|e| format!("LZ4 decompression failed: {}", e))?;
    if let Some(max_sz) = max_output_size {
        if decompressed.len() > max_sz {
            return Err(format!(
                "Decompression output limit exceeded: max {} bytes, attempted {}",
                max_sz,
                decompressed.len()
            ));
        }
    }
    Ok(decompressed)
}

// ----------------------------------------------------------------------------
// XZ / LZMA
// ----------------------------------------------------------------------------

pub fn compress_xz(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    lzma_rs::xz_compress(&mut &input[..], &mut output)
        .map_err(|e| format!("XZ compression failed: {:?}", e))?;
    Ok(output)
}

pub fn decompress_xz(input: &[u8], max_output_size: Option<usize>) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut cursor = std::io::Cursor::new(input);
    lzma_rs::xz_decompress(&mut cursor, &mut output)
        .map_err(|e| format!("XZ decompression failed: {:?}", e))?;
    if let Some(max_sz) = max_output_size {
        if output.len() > max_sz {
            return Err(format!(
                "Decompression output limit exceeded: max {} bytes, attempted {}",
                max_sz,
                output.len()
            ));
        }
    }
    Ok(output)
}

// ----------------------------------------------------------------------------
// Unified Helper with Decompression Limit Protection
// ----------------------------------------------------------------------------

fn read_with_limit<R: Read>(
    reader: &mut R,
    max_output_size: Option<usize>,
    codec_name: &str,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("{} decompression read error: {}", codec_name, e))?;
        if n == 0 {
            break;
        }
        if let Some(max_sz) = max_output_size {
            if output.len() + n > max_sz {
                return Err(format!(
                    "Decompression bomb protection triggered in {}: maximum output limit of {} bytes exceeded",
                    codec_name, max_sz
                ));
            }
        }
        output.extend_from_slice(&buf[..n]);
    }
    Ok(output)
}

pub fn compress_bytes(codec: CodecType, input: &[u8], level: i32) -> Result<Vec<u8>, String> {
    match codec {
        CodecType::Deflate => compress_deflate(input, level as u32),
        CodecType::Gzip => compress_gzip(input, level as u32),
        CodecType::Zlib => compress_zlib(input, level as u32),
        CodecType::Brotli => compress_brotli(input, level as u32),
        CodecType::Zstd => compress_zstd(input, level),
        CodecType::Lz4 => compress_lz4(input),
        CodecType::Xz => compress_xz(input),
    }
}

pub fn decompress_bytes(
    codec: CodecType,
    input: &[u8],
    max_output_size: Option<usize>,
) -> Result<Vec<u8>, String> {
    match codec {
        CodecType::Deflate => decompress_deflate(input, max_output_size),
        CodecType::Gzip => decompress_gzip(input, max_output_size),
        CodecType::Zlib => decompress_zlib(input, max_output_size),
        CodecType::Brotli => decompress_brotli(input, max_output_size),
        CodecType::Zstd => decompress_zstd(input, max_output_size),
        CodecType::Lz4 => decompress_lz4(input, max_output_size),
        CodecType::Xz => decompress_xz(input, max_output_size),
    }
}
