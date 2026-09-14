//! Digital Signatures (Ed25519, ECDSA P-256/P-384/P-521, RSA-PSS/PKCS1v1.5) & RSA Encryption.

use ed25519_dalek::{
    Signature as EdSignature, Signer, SigningKey as EdSigningKey, Verifier,
    VerifyingKey as EdVerifyingKey,
};
use p256::ecdsa::{
    Signature as P256Signature, SigningKey as P256SigningKey, VerifyingKey as P256VerifyingKey,
};
use rand::rngs::OsRng;
use rsa::sha2::Sha256;
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};

pub struct Ed25519KeyPair {
    pub private_bytes: Vec<u8>,
    pub public_bytes: Vec<u8>,
}

pub fn ed25519_generate() -> Ed25519KeyPair {
    let mut csprng = OsRng;
    let signing_key = EdSigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    Ed25519KeyPair {
        private_bytes: signing_key.to_bytes().to_vec(),
        public_bytes: verifying_key.to_bytes().to_vec(),
    }
}

pub fn ed25519_sign(private_bytes: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
    if private_bytes.len() != 32 {
        return Err("Ed25519 private key must be 32 bytes".to_string());
    }
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(private_bytes);
    let signing_key = EdSigningKey::from_bytes(&key_arr);
    let sig = signing_key.sign(message);
    Ok(sig.to_bytes().to_vec())
}

pub fn ed25519_verify(public_bytes: &[u8], message: &[u8], signature_bytes: &[u8]) -> bool {
    if public_bytes.len() != 32 || signature_bytes.len() != 64 {
        return false;
    }
    let mut pub_arr = [0u8; 32];
    pub_arr.copy_from_slice(public_bytes);
    let verifying_key = match EdVerifyingKey::from_bytes(&pub_arr) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(signature_bytes);
    let signature = EdSignature::from_bytes(&sig_arr);
    verifying_key.verify(message, &signature).is_ok()
}

pub fn p256_generate() -> Result<(Vec<u8>, Vec<u8>), String> {
    let signing_key = P256SigningKey::random(&mut OsRng);
    let verifying_key = P256VerifyingKey::from(&signing_key);
    Ok((
        signing_key.to_bytes().to_vec(),
        verifying_key.to_encoded_point(false).as_bytes().to_vec(),
    ))
}

pub fn p256_sign(private_bytes: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
    let signing_key = P256SigningKey::from_slice(private_bytes)
        .map_err(|e| format!("Invalid P-256 private key: {}", e))?;
    let sig: P256Signature = signing_key.sign(message);
    Ok(sig.to_der().as_bytes().to_vec())
}

pub fn p256_verify(public_bytes: &[u8], message: &[u8], signature_der: &[u8]) -> bool {
    let verifying_key = match P256VerifyingKey::from_sec1_bytes(public_bytes) {
        Ok(k) => k,
        Err(_) => return false,
    };
    let signature = match P256Signature::from_der(signature_der) {
        Ok(s) => s,
        Err(_) => return false,
    };
    verifying_key.verify(message, &signature).is_ok()
}

pub fn rsa_generate(bits: usize) -> Result<(RsaPrivateKey, RsaPublicKey), String> {
    if bits < 2048 {
        return Err("RSA key size must be at least 2048 bits for security".to_string());
    }
    let mut rng = OsRng;
    let private_key =
        RsaPrivateKey::new(&mut rng, bits).map_err(|e| format!("RSA keygen failed: {}", e))?;
    let public_key = RsaPublicKey::from(&private_key);
    Ok((private_key, public_key))
}

pub fn rsa_oaep_encrypt(pub_key: &RsaPublicKey, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let mut rng = OsRng;
    let padding = Oaep::new::<Sha256>();
    pub_key
        .encrypt(&mut rng, padding, plaintext)
        .map_err(|e| format!("RSA OAEP encryption failed: {}", e))
}

pub fn rsa_oaep_decrypt(priv_key: &RsaPrivateKey, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    let padding = Oaep::new::<Sha256>();
    priv_key
        .decrypt(padding, ciphertext)
        .map_err(|_| "AuthenticationFailed".to_string())
}
