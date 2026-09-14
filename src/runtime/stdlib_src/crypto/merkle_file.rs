//! Merkle Trees, Content Identifiers, and Encrypted File Tools for AdeshLang Crypto.

use crate::runtime::stdlib_src::crypto::aead::{AeadAlgo, decrypt_aead, encrypt_aead};
use crate::runtime::stdlib_src::crypto::password::hash_pbkdf2_sha256;
use rand::RngCore;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct MerkleProofStep {
    pub is_right: bool,
    pub hash: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MerkleTree {
    pub leaves: Vec<Vec<u8>>,
    pub layers: Vec<Vec<Vec<u8>>>,
}

impl MerkleTree {
    pub fn build(items: &[Vec<u8>]) -> Self {
        if items.is_empty() {
            let empty_root = Sha256::digest(b"").to_vec();
            return MerkleTree {
                leaves: vec![],
                layers: vec![vec![empty_root]],
            };
        }

        let mut current_layer: Vec<Vec<u8>> = items
            .iter()
            .map(|item| {
                let mut hasher = Sha256::new();
                hasher.update(&[0x00]); // Leaf prefix for domain separation
                hasher.update(item);
                hasher.finalize().to_vec()
            })
            .collect();

        let leaves = current_layer.clone();
        let mut layers = vec![current_layer.clone()];

        while current_layer.len() > 1 {
            let mut next_layer = Vec::new();
            for chunk in current_layer.chunks(2) {
                let mut hasher = Sha256::new();
                hasher.update(&[0x01]); // Internal node prefix for domain separation
                if chunk.len() == 2 {
                    hasher.update(&chunk[0]);
                    hasher.update(&chunk[1]);
                } else {
                    hasher.update(&chunk[0]);
                    hasher.update(&chunk[0]);
                }
                next_layer.push(hasher.finalize().to_vec());
            }
            layers.push(next_layer.clone());
            current_layer = next_layer;
        }

        MerkleTree { leaves, layers }
    }

    pub fn root(&self) -> Vec<u8> {
        self.layers
            .last()
            .and_then(|l| l.first())
            .cloned()
            .unwrap_or_default()
    }

    pub fn proof(&self, leaf_index: usize) -> Result<Vec<MerkleProofStep>, String> {
        if leaf_index >= self.leaves.len() {
            return Err(format!("Leaf index {} out of bounds", leaf_index));
        }

        let mut proof = Vec::new();
        let mut idx = leaf_index;

        for layer in &self.layers[..self.layers.len() - 1] {
            let is_right = idx % 2 == 0;
            let sibling_idx = if is_right {
                if idx + 1 < layer.len() { idx + 1 } else { idx }
            } else {
                idx - 1
            };

            proof.push(MerkleProofStep {
                is_right,
                hash: layer[sibling_idx].clone(),
            });

            idx /= 2;
        }

        Ok(proof)
    }

    pub fn verify_proof(item: &[u8], proof: &[MerkleProofStep], root: &[u8]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(&[0x00]);
        hasher.update(item);
        let mut current_hash = hasher.finalize().to_vec();

        for step in proof {
            let mut hasher = Sha256::new();
            hasher.update(&[0x01]);
            if step.is_right {
                hasher.update(&current_hash);
                hasher.update(&step.hash);
            } else {
                hasher.update(&step.hash);
                hasher.update(&current_hash);
            }
            current_hash = hasher.finalize().to_vec();
        }

        current_hash == root
    }
}

pub fn content_id(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    format!("sha256-{}", hex::encode(digest))
}

pub fn encrypt_file_payload(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let derived_key = hash_pbkdf2_sha256(passphrase.as_bytes(), &salt, 100_000, 32);

    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);

    let algo = AeadAlgo::Aes256Gcm;
    let ciphertext_and_tag = encrypt_aead(
        &algo,
        &derived_key,
        &nonce,
        plaintext,
        Some(b"AdeshCryptFile-v1"),
    )?;

    let mut output = Vec::new();
    output.extend_from_slice(b"ADCRYPT1"); // Magic header
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext_and_tag);
    Ok(output)
}

pub fn decrypt_file_payload(passphrase: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() < 8 + 16 + 12 + 16 {
        return Err("Encrypted file payload too short or corrupted".to_string());
    }
    if &payload[..8] != b"ADCRYPT1" {
        return Err("Invalid file magic header, expected 'ADCRYPT1'".to_string());
    }

    let salt = &payload[8..24];
    let nonce = &payload[24..36];
    let ciphertext_and_tag = &payload[36..];

    let derived_key = hash_pbkdf2_sha256(passphrase.as_bytes(), salt, 100_000, 32);
    let algo = AeadAlgo::Aes256Gcm;

    decrypt_aead(
        &algo,
        &derived_key,
        nonce,
        ciphertext_and_tag,
        Some(b"AdeshCryptFile-v1"),
    )
}
