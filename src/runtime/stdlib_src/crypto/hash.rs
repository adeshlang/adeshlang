//! Hashing and Digest implementation for AdeshLang Crypto standard library.

use blake2::{Blake2b512, Blake2s256};
use blake3;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
use sha3::{
    Sha3_224, Sha3_256, Sha3_384, Sha3_512, Shake128, Shake256, digest::ExtendableOutput,
    digest::Update, digest::XofReader,
};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use subtle::ConstantTimeEq;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashAlgo {
    Sha1,
    Sha256,
    Sha224,
    Sha384,
    Sha512,
    Sha512_224,
    Sha512_256,
    Sha3_224,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Shake128(usize),
    Shake256(usize),
    Blake2b,
    Blake2s,
    Blake3,
}

impl HashAlgo {
    pub fn from_str(name: &str) -> Result<Self, String> {
        match name
            .to_lowercase()
            .replace("-", "_")
            .replace(" ", "_")
            .as_str()
        {
            "sha256" | "sha_256" => Ok(HashAlgo::Sha256),
            "sha224" | "sha_224" => Ok(HashAlgo::Sha224),
            "sha384" | "sha_384" => Ok(HashAlgo::Sha384),
            "sha512" | "sha_512" => Ok(HashAlgo::Sha512),
            "sha512_224" | "sha_512_224" => Ok(HashAlgo::Sha512_224),
            "sha512_256" | "sha_512_256" => Ok(HashAlgo::Sha512_256),
            "sha3_224" => Ok(HashAlgo::Sha3_224),
            "sha3_256" => Ok(HashAlgo::Sha3_256),
            "sha3_384" => Ok(HashAlgo::Sha3_384),
            "sha3_512" => Ok(HashAlgo::Sha3_512),
            "shake128" => Ok(HashAlgo::Shake128(32)),
            "shake256" => Ok(HashAlgo::Shake256(64)),
            "blake2b" | "blake2b_512" => Ok(HashAlgo::Blake2b),
            "blake2s" | "blake2s_256" => Ok(HashAlgo::Blake2s),
            "blake3" => Ok(HashAlgo::Blake3),
            _ => Err(format!("Unsupported hash algorithm: '{}'", name)),
        }
    }
}

pub fn hash_bytes(algo: &HashAlgo, data: &[u8]) -> Vec<u8> {
    match algo {
        HashAlgo::Sha1 => Sha1::digest(data).to_vec(),
        HashAlgo::Sha256 => Sha256::digest(data).to_vec(),
        HashAlgo::Sha224 => Sha224::digest(data).to_vec(),
        HashAlgo::Sha384 => Sha384::digest(data).to_vec(),
        HashAlgo::Sha512 => Sha512::digest(data).to_vec(),
        HashAlgo::Sha512_224 => Sha512_224::digest(data).to_vec(),
        HashAlgo::Sha512_256 => Sha512_256::digest(data).to_vec(),
        HashAlgo::Sha3_224 => Sha3_224::digest(data).to_vec(),
        HashAlgo::Sha3_256 => Sha3_256::digest(data).to_vec(),
        HashAlgo::Sha3_384 => Sha3_384::digest(data).to_vec(),
        HashAlgo::Sha3_512 => Sha3_512::digest(data).to_vec(),
        HashAlgo::Shake128(out_len) => {
            let mut hasher = Shake128::default();
            Update::update(&mut hasher, data);
            let mut reader = hasher.finalize_xof();
            let mut buf = vec![0u8; *out_len];
            XofReader::read(&mut reader, &mut buf);
            buf
        }
        HashAlgo::Shake256(out_len) => {
            let mut hasher = Shake256::default();
            Update::update(&mut hasher, data);
            let mut reader = hasher.finalize_xof();
            let mut buf = vec![0u8; *out_len];
            XofReader::read(&mut reader, &mut buf);
            buf
        }
        HashAlgo::Blake2b => Blake2b512::digest(data).to_vec(),
        HashAlgo::Blake2s => Blake2s256::digest(data).to_vec(),
        HashAlgo::Blake3 => blake3::hash(data).as_bytes().to_vec(),
    }
}

pub fn hash_file<P: AsRef<Path>>(algo: &HashAlgo, path: P) -> Result<Vec<u8>, String> {
    let file =
        File::open(path.as_ref()).map_err(|e| format!("Failed to open file for hashing: {}", e))?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; 65536];

    match algo {
        HashAlgo::Sha256 => {
            let mut hasher = Sha256::new();
            loop {
                let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                Digest::update(&mut hasher, &buffer[..count]);
            }
            Ok(hasher.finalize().to_vec())
        }
        HashAlgo::Sha512 => {
            let mut hasher = Sha512::new();
            loop {
                let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                Digest::update(&mut hasher, &buffer[..count]);
            }
            Ok(hasher.finalize().to_vec())
        }
        HashAlgo::Blake3 => {
            let mut hasher = blake3::Hasher::new();
            loop {
                let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
            Ok(hasher.finalize().as_bytes().to_vec())
        }
        _ => {
            // General fallback streaming reader for other algorithms
            let mut all_bytes = Vec::new();
            reader
                .read_to_end(&mut all_bytes)
                .map_err(|e| e.to_string())?;
            Ok(hash_bytes(algo, &all_bytes))
        }
    }
}

pub fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}
