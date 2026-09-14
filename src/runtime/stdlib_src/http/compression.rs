use super::errors::{HttpError, HttpErrorKind};
use std::io::{Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentEncoding {
    Gzip,
    Deflate,
    Brotli,
    Zstd,
    Identity,
}

impl ContentEncoding {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "gzip" => ContentEncoding::Gzip,
            "deflate" => ContentEncoding::Deflate,
            "br" => ContentEncoding::Brotli,
            "zstd" => ContentEncoding::Zstd,
            _ => ContentEncoding::Identity,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ContentEncoding::Gzip => "gzip",
            ContentEncoding::Deflate => "deflate",
            ContentEncoding::Brotli => "br",
            ContentEncoding::Zstd => "zstd",
            ContentEncoding::Identity => "identity",
        }
    }
}

pub fn decompress_body(
    data: &[u8],
    encoding: ContentEncoding,
    max_decompressed: usize,
) -> Result<Vec<u8>, HttpError> {
    match encoding {
        ContentEncoding::Identity => {
            if data.len() > max_decompressed {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!(
                        "Body exceeds maximum decompressed size of {} bytes",
                        max_decompressed
                    ),
                ));
            }
            Ok(data.to_vec())
        }
        ContentEncoding::Gzip => {
            let decoder = flate2::read::GzDecoder::new(data);
            let mut out = Vec::new();
            let mut take = decoder.take((max_decompressed + 1) as u64);
            take.read_to_end(&mut out).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::CompressionError,
                    format!("Gzip decompression failed: {}", e),
                )
            })?;
            if out.len() > max_decompressed {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!(
                        "Decompressed body exceeds size limit of {} bytes (Decompression bomb rejected)",
                        max_decompressed
                    ),
                ));
            }
            Ok(out)
        }
        ContentEncoding::Deflate => {
            let decoder = flate2::read::ZlibDecoder::new(data);
            let mut out = Vec::new();
            let mut take = decoder.take((max_decompressed + 1) as u64);
            take.read_to_end(&mut out).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::CompressionError,
                    format!("Deflate decompression failed: {}", e),
                )
            })?;
            if out.len() > max_decompressed {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!(
                        "Decompressed body exceeds size limit of {} bytes (Decompression bomb rejected)",
                        max_decompressed
                    ),
                ));
            }
            Ok(out)
        }
        ContentEncoding::Brotli => {
            let decoder = brotli::Decompressor::new(data, 4096);
            let mut out = Vec::new();
            let mut take = decoder.take((max_decompressed + 1) as u64);
            take.read_to_end(&mut out).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::CompressionError,
                    format!("Brotli decompression failed: {}", e),
                )
            })?;
            if out.len() > max_decompressed {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!(
                        "Decompressed body exceeds size limit of {} bytes (Decompression bomb rejected)",
                        max_decompressed
                    ),
                ));
            }
            Ok(out)
        }
        ContentEncoding::Zstd => {
            let decoder = zstd::stream::Decoder::new(data).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::CompressionError,
                    format!("Zstd init failed: {}", e),
                )
            })?;
            let mut out = Vec::new();
            let mut take = decoder.take((max_decompressed + 1) as u64);
            take.read_to_end(&mut out).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::CompressionError,
                    format!("Zstd decompression failed: {}", e),
                )
            })?;
            if out.len() > max_decompressed {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    format!(
                        "Decompressed body exceeds size limit of {} bytes (Decompression bomb rejected)",
                        max_decompressed
                    ),
                ));
            }
            Ok(out)
        }
    }
}

pub fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| HttpError::new(HttpErrorKind::CompressionError, e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| HttpError::new(HttpErrorKind::CompressionError, e.to_string()))
}
