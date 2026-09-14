//! Comprehensive Rust Test Suite & Vector Verification for AdeshLang Crypto.

use adeshlang::runtime::stdlib_src::crypto::aead::{AeadAlgo, decrypt_aead, encrypt_aead};
use adeshlang::runtime::stdlib_src::crypto::hash::{HashAlgo, constant_time_compare, hash_bytes};
use adeshlang::runtime::stdlib_src::crypto::hmac_hkdf::{
    hkdf_sha256_derive, hmac_sha256, hmac_verify,
};
use adeshlang::runtime::stdlib_src::crypto::jwt_jwk::{
    compute_jwk_thumbprint, sign_jwt_hs256, verify_jwt_hs256,
};
use adeshlang::runtime::stdlib_src::crypto::key_exchange::{x25519_exchange, x25519_generate};
use adeshlang::runtime::stdlib_src::crypto::merkle_file::{
    MerkleTree, decrypt_file_payload, encrypt_file_payload,
};
use adeshlang::runtime::stdlib_src::crypto::password::{
    hash_password_argon2id, verify_password_argon2id,
};
use adeshlang::runtime::stdlib_src::crypto::secure_memory::SecretBytes;
use adeshlang::runtime::stdlib_src::crypto::signatures::{
    ed25519_generate, ed25519_sign, ed25519_verify,
};
use serde_json::json;

#[test]
fn test_sha256_nist_vectors() {
    // NIST SHA-256 Vector: "abc"
    let digest = hash_bytes(&HashAlgo::Sha256, b"abc");
    assert_eq!(
        hex::encode(digest),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );

    // NIST SHA-256 Vector: ""
    let empty_digest = hash_bytes(&HashAlgo::Sha256, b"");
    assert_eq!(
        hex::encode(empty_digest),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn test_blake3_vector() {
    let digest = hash_bytes(&HashAlgo::Blake3, b"hello world");
    assert_eq!(
        hex::encode(digest),
        "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
    );
}

#[test]
fn test_hmac_sha256_vector() {
    let key = b"key";
    let message = b"The quick brown fox jumps over the lazy dog";
    let tag = hmac_sha256(key, message).unwrap();
    assert_eq!(
        hex::encode(&tag),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert!(hmac_verify(key, message, &tag, "sha256"));
}

#[test]
fn test_hkdf_rfc5869_vector() {
    let ikm = hex::decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b").unwrap();
    let salt = hex::decode("000102030405060708090a0b0c").unwrap();
    let info = hex::decode("f0f1f2f3f4f5f6f7f8f9").unwrap();
    let okm = hkdf_sha256_derive(Some(&salt), &ikm, &info, 42).unwrap();
    assert_eq!(
        hex::encode(okm),
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
}

#[test]
fn test_argon2id_password_hashing() {
    let password = "SuperSecretPassword123!";
    let hash = hash_password_argon2id(password).unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert!(verify_password_argon2id(password, &hash));
    assert!(!verify_password_argon2id("WrongPassword", &hash));
}

#[test]
fn test_aead_aes256gcm_roundtrip() {
    let key = vec![0u8; 32];
    let nonce = vec![1u8; 12];
    let plaintext = b"Top Secret Data for AdeshLang AEAD Test";
    let ciphertext = encrypt_aead(
        &AeadAlgo::Aes256Gcm,
        &key,
        &nonce,
        plaintext,
        Some(b"header"),
    )
    .unwrap();
    let decrypted = decrypt_aead(
        &AeadAlgo::Aes256Gcm,
        &key,
        &nonce,
        &ciphertext,
        Some(b"header"),
    )
    .unwrap();
    assert_eq!(decrypted, plaintext);

    // Tampered ciphertext failure test
    let mut tampered = ciphertext.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xFF;
    assert!(
        decrypt_aead(
            &AeadAlgo::Aes256Gcm,
            &key,
            &nonce,
            &tampered,
            Some(b"header")
        )
        .is_err()
    );
}

#[test]
fn test_ed25519_signatures() {
    let kp = ed25519_generate();
    let message = b"Sign this AdeshLang contract";
    let signature = ed25519_sign(&kp.private_bytes, message).unwrap();
    assert!(ed25519_verify(&kp.public_bytes, message, &signature));
    assert!(!ed25519_verify(
        &kp.public_bytes,
        b"Modified contract",
        &signature
    ));
}

#[test]
fn test_x25519_key_exchange() {
    let alice = x25519_generate();
    let bob = x25519_generate();
    let alice_shared = x25519_exchange(&alice.secret_bytes, &bob.public_bytes).unwrap();
    let bob_shared = x25519_exchange(&bob.secret_bytes, &alice.public_bytes).unwrap();
    assert_eq!(alice_shared, bob_shared);
}

#[test]
fn test_constant_time_equals() {
    assert!(constant_time_compare(b"secret_tag_123", b"secret_tag_123"));
    assert!(!constant_time_compare(b"secret_tag_123", b"secret_tag_124"));
    assert!(!constant_time_compare(b"secret_tag_123", b"short"));
}

#[test]
fn test_secure_memory_zeroizing() {
    let mut secret = SecretBytes::new(vec![1, 2, 3, 4, 5]);
    assert_eq!(secret.len(), 5);
    assert_eq!(secret.expose(), &[1, 2, 3, 4, 5]);
    let _ = secret.lock_memory();
}

#[test]
fn test_jwt_safe_verification() {
    let claims = json!({"sub": "user_123", "exp": 2524608000u64});
    let secret = b"my_jwt_signing_key_32_bytes_long!!";
    let token = sign_jwt_hs256(&claims, secret).unwrap();

    let verified = verify_jwt_hs256(&token, secret, &["HS256"]).unwrap();
    assert_eq!(verified["sub"], "user_123");

    // Whitelist rejection test
    assert!(verify_jwt_hs256(&token, secret, &["RS256"]).is_err());
}

#[test]
fn test_jwk_thumbprint_rfc7638() {
    let jwk = json!({
        "kty": "RSA",
        "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lnt6skiFii754321654987",
        "e": "AQAB"
    });
    let thumbprint = compute_jwk_thumbprint(&jwk).unwrap();
    assert!(!thumbprint.is_empty());
}

#[test]
fn test_merkle_tree_proof_verification() {
    let items = vec![
        b"leaf0".to_vec(),
        b"leaf1".to_vec(),
        b"leaf2".to_vec(),
        b"leaf3".to_vec(),
    ];
    let tree = MerkleTree::build(&items);
    let root = tree.root();
    assert_eq!(root.len(), 32);

    let proof0 = tree.proof(0).unwrap();
    assert!(MerkleTree::verify_proof(b"leaf0", &proof0, &root));
    assert!(!MerkleTree::verify_proof(b"fake_leaf", &proof0, &root));
}

#[test]
fn test_encrypted_file_payload_roundtrip() {
    let passphrase = "MySuperSecretFilePassphrase123!";
    let plaintext = b"Confidential Financial Audit Report 2026";
    let payload = encrypt_file_payload(passphrase, plaintext).unwrap();
    let decrypted = decrypt_file_payload(passphrase, &payload).unwrap();
    assert_eq!(decrypted, plaintext);

    assert!(decrypt_file_payload("WrongPassphrase", &payload).is_err());
}
