//! Indexed Chunked Compression & Seekable Random-Access Compressed Format.

use super::checksums::crc32;
use super::codecs::{CodecType, compress_bytes, decompress_bytes};

const MAGIC: &[u8; 4] = b"ADSH";

#[derive(Debug, Clone)]
pub struct ChunkMeta {
    pub index: u32,
    pub offset: u64,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub checksum: u32,
}

pub struct ChunkedArchive {
    pub codec: CodecType,
    pub chunk_size: usize,
    pub chunks: Vec<ChunkMeta>,
    pub data: Vec<u8>,
}

impl ChunkedArchive {
    pub fn create(
        input: &[u8],
        codec: CodecType,
        chunk_size: usize,
        level: i32,
    ) -> Result<Vec<u8>, String> {
        let actual_chunk_size = if chunk_size == 0 { 65536 } else { chunk_size };
        let mut archive_bytes = Vec::new();
        archive_bytes.extend_from_slice(MAGIC);

        // Codec ID: Deflate=1, Gzip=2, Zlib=3, Brotli=4, Zstd=5, LZ4=6, XZ=7
        let codec_id: u16 = match codec {
            CodecType::Deflate => 1,
            CodecType::Gzip => 2,
            CodecType::Zlib => 3,
            CodecType::Brotli => 4,
            CodecType::Zstd => 5,
            CodecType::Lz4 => 6,
            CodecType::Xz => 7,
        };
        archive_bytes.extend_from_slice(&codec_id.to_le_bytes());

        let chunks_input: Vec<&[u8]> = input.chunks(actual_chunk_size).collect();
        let chunk_count = chunks_input.len() as u32;
        archive_bytes.extend_from_slice(&chunk_count.to_le_bytes());

        let mut index_entries: Vec<(u64, u32, u32, u32)> = Vec::new();

        for (_idx, chunk_bytes) in chunks_input.iter().enumerate() {
            let offset = archive_bytes.len() as u64;
            let compressed = compress_bytes(codec, chunk_bytes, level)?;
            let checksum = crc32(chunk_bytes);
            let uncomp_size = chunk_bytes.len() as u32;
            let comp_size = compressed.len() as u32;

            archive_bytes.extend_from_slice(&comp_size.to_le_bytes());
            archive_bytes.extend_from_slice(&uncomp_size.to_le_bytes());
            archive_bytes.extend_from_slice(&checksum.to_le_bytes());
            archive_bytes.extend_from_slice(&compressed);

            index_entries.push((offset, comp_size, uncomp_size, checksum));
        }

        let index_offset = archive_bytes.len() as u64;
        for (off, c_sz, u_sz, chk) in index_entries {
            archive_bytes.extend_from_slice(&off.to_le_bytes());
            archive_bytes.extend_from_slice(&c_sz.to_le_bytes());
            archive_bytes.extend_from_slice(&u_sz.to_le_bytes());
            archive_bytes.extend_from_slice(&chk.to_le_bytes());
        }

        archive_bytes.extend_from_slice(&index_offset.to_le_bytes());
        Ok(archive_bytes)
    }

    pub fn read_chunk(archive: &[u8], chunk_index: usize) -> Result<Vec<u8>, String> {
        if archive.len() < 18 || !archive.starts_with(MAGIC) {
            return Err("Invalid seekable compressed archive header".to_string());
        }

        let codec_id = u16::from_le_bytes(archive[4..6].try_into().unwrap());
        let codec = match codec_id {
            1 => CodecType::Deflate,
            2 => CodecType::Gzip,
            3 => CodecType::Zlib,
            4 => CodecType::Brotli,
            5 => CodecType::Zstd,
            6 => CodecType::Lz4,
            7 => CodecType::Xz,
            _ => return Err(format!("Unknown codec ID in archive: {}", codec_id)),
        };

        let chunk_count = u32::from_le_bytes(archive[6..10].try_into().unwrap()) as usize;
        if chunk_index >= chunk_count {
            return Err(format!(
                "Chunk index out of bounds: requested {}, total {}",
                chunk_index, chunk_count
            ));
        }

        let index_offset_pos = archive.len() - 8;
        let index_offset =
            u64::from_le_bytes(archive[index_offset_pos..].try_into().unwrap()) as usize;

        let entry_size = 8 + 4 + 4 + 4; // off(8) + comp_sz(4) + uncomp_sz(4) + chk(4)
        let entry_pos = index_offset + (chunk_index * entry_size);

        if entry_pos + entry_size > index_offset_pos {
            return Err("Corrupted archive index table".to_string());
        }

        let offset =
            u64::from_le_bytes(archive[entry_pos..entry_pos + 8].try_into().unwrap()) as usize;
        let comp_sz =
            u32::from_le_bytes(archive[entry_pos + 8..entry_pos + 12].try_into().unwrap()) as usize;
        let expected_crc =
            u32::from_le_bytes(archive[entry_pos + 16..entry_pos + 20].try_into().unwrap());

        let payload_pos = offset + 12; // 4+4+4 chunk header
        if payload_pos + comp_sz > archive.len() {
            return Err("Corrupted chunk data payload".to_string());
        }

        let compressed_payload = &archive[payload_pos..payload_pos + comp_sz];
        let decompressed = decompress_bytes(codec, compressed_payload, None)?;

        let actual_crc = crc32(&decompressed);
        if actual_crc != expected_crc {
            return Err(format!(
                "Chunk CRC32 checksum mismatch: expected 0x{:08X}, got 0x{:08X}",
                expected_crc, actual_crc
            ));
        }

        Ok(decompressed)
    }
}
