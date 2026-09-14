//! Elliptic-curve Diffie-Hellman (X25519) for AdeshLang Crypto.

use rand::rngs::OsRng;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

pub struct X25519KeyPair {
    pub secret_bytes: Vec<u8>,
    pub public_bytes: Vec<u8>,
}

pub fn x25519_generate() -> X25519KeyPair {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public_key = X25519PublicKey::from(&secret);
    X25519KeyPair {
        secret_bytes: secret.to_bytes().to_vec(),
        public_bytes: public_key.to_bytes().to_vec(),
    }
}

pub fn x25519_exchange(
    my_secret_bytes: &[u8],
    peer_public_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    if my_secret_bytes.len() != 32 {
        return Err("X25519 secret key must be 32 bytes".to_string());
    }
    if peer_public_bytes.len() != 32 {
        return Err("X25519 public key must be 32 bytes".to_string());
    }

    let mut secret_arr = [0u8; 32];
    secret_arr.copy_from_slice(my_secret_bytes);
    let secret = StaticSecret::from(secret_arr);

    let mut pub_arr = [0u8; 32];
    pub_arr.copy_from_slice(peer_public_bytes);
    let peer_public = X25519PublicKey::from(pub_arr);

    let shared_secret = secret.diffie_hellman(&peer_public);
    Ok(shared_secret.as_bytes().to_vec())
}
