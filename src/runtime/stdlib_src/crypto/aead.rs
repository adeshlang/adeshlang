//! Authenticated Encryption with Associated Data (AEAD) for AdeshLang Crypto.

use aes_gcm::{
    Aes128Gcm, Aes256Gcm, Nonce as AesNonce,
    aead::{Aead, KeyInit, Payload},
};
use chacha20poly1305::{ChaCha20Poly1305, XChaCha20Poly1305, XNonce};
use rand::RngCore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AeadAlgo {
    Aes256Gcm,
    Aes128Gcm,
    ChaCha20Poly1305,
    XChaCha20Poly1305,
}

impl AeadAlgo {
    pub fn from_str(name: &str) -> Result<Self, String> {
        match name
            .to_lowercase()
            .replace("-", "_")
            .replace(" ", "_")
            .as_str()
        {
            "aes256gcm" | "aes_256_gcm" => Ok(AeadAlgo::Aes256Gcm),
            "aes128gcm" | "aes_128_gcm" => Ok(AeadAlgo::Aes128Gcm),
            "chacha20poly1305" | "chacha20_poly1305" => Ok(AeadAlgo::ChaCha20Poly1305),
            "xchacha20poly1305" | "xchacha20_poly1305" => Ok(AeadAlgo::XChaCha20Poly1305),
            _ => Err(format!("Unsupported AEAD algorithm: '{}'", name)),
        }
    }

    pub fn key_len(&self) -> usize {
        match self {
            AeadAlgo::Aes256Gcm => 32,
            AeadAlgo::Aes128Gcm => 16,
            AeadAlgo::ChaCha20Poly1305 => 32,
            AeadAlgo::XChaCha20Poly1305 => 32,
        }
    }

    pub fn nonce_len(&self) -> usize {
        match self {
            AeadAlgo::Aes256Gcm => 12,
            AeadAlgo::Aes128Gcm => 12,
            AeadAlgo::ChaCha20Poly1305 => 12,
            AeadAlgo::XChaCha20Poly1305 => 24,
        }
    }
}

pub struct SealedMessage {
    pub algo: AeadAlgo,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub tag: Vec<u8>,
}

pub fn encrypt_aead(
    algo: &AeadAlgo,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if key.len() != algo.key_len() {
        return Err(format!(
            "Invalid key length for {:?}: expected {} bytes, got {}",
            algo,
            algo.key_len(),
            key.len()
        ));
    }
    if nonce.len() != algo.nonce_len() {
        return Err(format!(
            "Invalid nonce length for {:?}: expected {} bytes, got {}",
            algo,
            algo.nonce_len(),
            nonce.len()
        ));
    }

    let payload = Payload {
        msg: plaintext,
        aad: aad.unwrap_or(&[]),
    };

    match algo {
        AeadAlgo::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = AesNonce::from_slice(nonce);
            cipher
                .encrypt(nonce_obj, payload)
                .map_err(|_| "Encryption failed (Authentication tag failure)".to_string())
        }
        AeadAlgo::Aes128Gcm => {
            let cipher = Aes128Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = AesNonce::from_slice(nonce);
            cipher
                .encrypt(nonce_obj, payload)
                .map_err(|_| "Encryption failed (Authentication tag failure)".to_string())
        }
        AeadAlgo::ChaCha20Poly1305 => {
            let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = chacha20poly1305::Nonce::from_slice(nonce);
            cipher
                .encrypt(nonce_obj, payload)
                .map_err(|_| "Encryption failed (Authentication tag failure)".to_string())
        }
        AeadAlgo::XChaCha20Poly1305 => {
            let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = XNonce::from_slice(nonce);
            cipher
                .encrypt(nonce_obj, payload)
                .map_err(|_| "Encryption failed (Authentication tag failure)".to_string())
        }
    }
}

pub fn decrypt_aead(
    algo: &AeadAlgo,
    key: &[u8],
    nonce: &[u8],
    ciphertext_and_tag: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if key.len() != algo.key_len() {
        return Err(format!("Invalid key length for {:?}", algo));
    }
    if nonce.len() != algo.nonce_len() {
        return Err(format!("Invalid nonce length for {:?}", algo));
    }

    let payload = Payload {
        msg: ciphertext_and_tag,
        aad: aad.unwrap_or(&[]),
    };

    match algo {
        AeadAlgo::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = AesNonce::from_slice(nonce);
            cipher
                .decrypt(nonce_obj, payload)
                .map_err(|_| "AuthenticationFailed".to_string())
        }
        AeadAlgo::Aes128Gcm => {
            let cipher = Aes128Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = AesNonce::from_slice(nonce);
            cipher
                .decrypt(nonce_obj, payload)
                .map_err(|_| "AuthenticationFailed".to_string())
        }
        AeadAlgo::ChaCha20Poly1305 => {
            let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = chacha20poly1305::Nonce::from_slice(nonce);
            cipher
                .decrypt(nonce_obj, payload)
                .map_err(|_| "AuthenticationFailed".to_string())
        }
        AeadAlgo::XChaCha20Poly1305 => {
            let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce_obj = XNonce::from_slice(nonce);
            cipher
                .decrypt(nonce_obj, payload)
                .map_err(|_| "AuthenticationFailed".to_string())
        }
    }
}

pub fn seal_random_nonce(
    algo: &AeadAlgo,
    key: &[u8],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut nonce = vec![0u8; algo.nonce_len()];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = encrypt_aead(algo, key, &nonce, plaintext, aad)?;
    Ok((nonce, ciphertext))
}
