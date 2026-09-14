//! Integrity Checksums for AdeshLang Compression.
//! Provides CRC32, CRC32C, Adler32, and XXHash implementations.

pub fn crc32(data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

pub fn crc32c(data: &[u8]) -> u32 {
    // CRC32C Castagnoli polynomial 0x82F63B78
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0x82F6_3B78 & mask);
        }
    }
    !crc
}

pub fn adler32(data: &[u8]) -> u32 {
    adler2::adler32_slice(data)
}

pub fn xxhash64(data: &[u8]) -> u64 {
    // Fast 64-bit non-cryptographic checksum
    const PRIME64_1: u64 = 11400714785074694791;
    const PRIME64_2: u64 = 14029467366897019727;
    const PRIME64_5: u64 = 2870177450012600261;

    let mut hash = (data.len() as u64).wrapping_mul(PRIME64_5);
    for chunk in data.chunks(8) {
        if chunk.len() == 8 {
            let val = u64::from_le_bytes(chunk.try_into().unwrap());
            let k1 = val.wrapping_mul(PRIME64_2);
            let k1 = k1.rotate_left(31).wrapping_mul(PRIME64_1);
            hash = hash ^ k1;
            hash = hash
                .rotate_left(27)
                .wrapping_mul(PRIME64_1)
                .wrapping_add(PRIME64_2);
        } else {
            for &byte in chunk {
                let k1 = (byte as u64).wrapping_mul(PRIME64_5);
                hash = (hash ^ k1).rotate_left(11).wrapping_mul(PRIME64_1);
            }
        }
    }
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(PRIME64_2);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(PRIME64_3_VAL);
    hash ^= hash >> 32;
    hash
}

const PRIME64_3_VAL: u64 = 16097379290577686587;
