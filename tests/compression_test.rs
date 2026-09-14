//! Unit and Integration Tests for AdeshLang Compression Standard Library.

use adeshlang::runtime::stdlib_src::compression::adaptive::compress_adaptive;
use adeshlang::runtime::stdlib_src::compression::archives::{
    create_tar_archive, create_zip_archive, extract_tar_archive, extract_zip_archive,
    list_zip_entries, read_zip_entry,
};
use adeshlang::runtime::stdlib_src::compression::checksums::{adler32, crc32, crc32c, xxhash64};
use adeshlang::runtime::stdlib_src::compression::chunking::ChunkedArchive;
use adeshlang::runtime::stdlib_src::compression::codecs::{
    CodecType, compress_bytes, compress_gzip, compress_zstd, decompress_bytes,
};
use adeshlang::runtime::stdlib_src::compression::dictionary::CompressionDictionary;
use adeshlang::runtime::stdlib_src::compression::parallel::{
    parallel_compress, parallel_decompress,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_all_codecs_roundtrip() {
    let sample = b"Hello AdeshLang Compression Ecosystem! Testing GZIP, Zstd, Brotli, LZ4, XZ, Deflate, Zlib codecs.";

    let codecs = vec![
        CodecType::Deflate,
        CodecType::Gzip,
        CodecType::Zlib,
        CodecType::Brotli,
        CodecType::Zstd,
        CodecType::Lz4,
        CodecType::Xz,
    ];

    for codec in codecs {
        let compressed = compress_bytes(codec, sample, 5).expect("Compression failed");
        assert!(!compressed.is_empty());

        let decompressed =
            decompress_bytes(codec, &compressed, None).expect("Decompression failed");
        assert_eq!(decompressed, sample, "Codec {:?} roundtrip failed", codec);
    }
}

#[test]
fn test_decompression_bomb_protection() {
    let sample = vec![b'A'; 50_000];
    let compressed = compress_zstd(&sample, 5).unwrap();

    let max_limit = 10_000;
    let result = decompress_bytes(CodecType::Zstd, &compressed, Some(max_limit));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("exceeded"));
}

#[test]
fn test_checksums() {
    let data = b"AdeshLang Compression Checksum Test";
    let c32 = crc32(data);
    let c32c = crc32c(data);
    let adl = adler32(data);
    let xx = xxhash64(data);

    assert_ne!(c32, 0);
    assert_ne!(c32c, 0);
    assert_ne!(adl, 0);
    assert_ne!(xx, 0);

    // Consistency check
    assert_eq!(crc32(data), c32);
    assert_eq!(crc32c(data), c32c);
    assert_eq!(adler32(data), adl);
    assert_eq!(xxhash64(data), xx);
}

#[test]
fn test_adaptive_compression() {
    let text_payload =
        b"Sample JSON text payload: {\"key\": \"value\", \"numbers\": [1, 2, 3, 4, 5]}".repeat(50);
    let res = compress_adaptive(&text_payload, None).unwrap();

    assert!(res.was_compressed);
    assert!(res.compressed_size < res.original_size);
    assert!(res.ratio < 1.0);
    assert!(res.space_saved_percent > 0.0);

    let decompressed = decompress_bytes(
        CodecType::from_str(&res.algorithm).unwrap(),
        &res.compressed_data,
        None,
    )
    .unwrap();
    assert_eq!(decompressed, text_payload);
}

#[test]
fn test_adaptive_skip_already_compressed() {
    let sample = b"Original text data".repeat(20);
    let compressed_gz = compress_gzip(&sample, 6).unwrap();

    let res = compress_adaptive(&compressed_gz, None).unwrap();
    assert!(!res.was_compressed);
    assert_eq!(res.algorithm, "Passthrough");
    assert_eq!(res.compressed_data, compressed_gz);
}

#[test]
fn test_dictionary_compression() {
    let mut samples = Vec::new();
    for i in 0..50 {
        samples.push(format!("{{\"user_id\": {}, \"action\": \"api_request\", \"status\": 200, \"service\": \"auth_v2\"}}", i).into_bytes());
    }

    let dict = CompressionDictionary::train_from_samples(&samples, 1024).unwrap();
    let test_msg = b"{\"user_id\": 999, \"action\": \"api_request\", \"status\": 200, \"service\": \"auth_v2\"}";

    let compressed = dict.compress(test_msg, 5).unwrap();
    let decompressed = dict.decompress(&compressed, None).unwrap();
    assert_eq!(decompressed, test_msg);
}

#[test]
fn test_seekable_chunked_archive() {
    let data =
        b"Chunk 0 Payload data... Chunk 1 Payload data... Chunk 2 Payload data...".repeat(500);
    let archive_bytes = ChunkedArchive::create(&data, CodecType::Zstd, 1024, 5).unwrap();

    // Random access chunk 0 and chunk 2
    let chunk0 = ChunkedArchive::read_chunk(&archive_bytes, 0).unwrap();
    assert!(!chunk0.is_empty());

    let chunk2 = ChunkedArchive::read_chunk(&archive_bytes, 2).unwrap();
    assert!(!chunk2.is_empty());
}

#[test]
fn test_zip_archive_security() {
    let files = vec![
        ("file1.txt", &b"Hello World 1"[..]),
        ("dir/file2.txt", &b"Hello World 2"[..]),
    ];

    let zip_bytes = create_zip_archive(&files).unwrap();
    let entries = list_zip_entries(&zip_bytes).unwrap();
    assert_eq!(entries.len(), 2);

    let content1 = read_zip_entry(&zip_bytes, "file1.txt", None).unwrap();
    assert_eq!(content1, b"Hello World 1");

    let dir = tempdir().unwrap();
    let extracted_count = extract_zip_archive(&zip_bytes, dir.path(), None).unwrap();
    assert_eq!(extracted_count, 2);

    let extracted_content = fs::read(dir.path().join("file1.txt")).unwrap();
    assert_eq!(extracted_content, b"Hello World 1");
}

#[test]
fn test_tar_archive() {
    let files = vec![
        ("doc1.txt", &b"Tar File Content 1"[..]),
        ("doc2.txt", &b"Tar File Content 2"[..]),
    ];

    let tar_bytes = create_tar_archive(&files).unwrap();
    let dir = tempdir().unwrap();
    let extracted_count = extract_tar_archive(&tar_bytes, dir.path(), None).unwrap();
    assert_eq!(extracted_count, 2);

    let extracted_content = fs::read(dir.path().join("doc1.txt")).unwrap();
    assert_eq!(extracted_content, b"Tar File Content 1");
}

#[test]
fn test_parallel_compression() {
    let data = b"Parallel compression testing multi-threaded chunks dataset.".repeat(1000);

    let comp = parallel_compress(&data, CodecType::Zstd, 16384, 5, 2).unwrap();
    let decomp = parallel_decompress(&comp, CodecType::Zstd, None, 2).unwrap();

    assert_eq!(decomp, data);
}

#[test]
fn test_magic_header_format_detection() {
    let raw = b"Plain uncompressed text data payload".to_vec();

    let gz = compress_gzip(&raw, 6).unwrap();
    let zst = compress_zstd(&raw, 5).unwrap();
    let zip_bytes = create_zip_archive(&[("test.txt", &raw[..])]).unwrap();

    // Verify magic byte detection
    assert_eq!(gz[0..2], [0x1f, 0x8b]);
    assert_eq!(zst[0..4], [0x28, 0xb5, 0x2f, 0xfd]);
    assert_eq!(zip_bytes[0..4], [b'P', b'K', 0x03, 0x04]);
}
