//! Native Function Callbacks & Crypto Module Registration for AdeshLang.

use crate::parsing::ast::{BuiltinEnv, Value};
use crate::runtime::stdlib_src::crypto::aead::{
    AeadAlgo, decrypt_aead, encrypt_aead, seal_random_nonce,
};
use crate::runtime::stdlib_src::crypto::hash::{
    HashAlgo, constant_time_compare, hash_bytes, hash_file,
};
use crate::runtime::stdlib_src::crypto::hmac_hkdf::{hkdf_sha256_derive, hmac_sha256, hmac_verify};
use crate::runtime::stdlib_src::crypto::jwt_jwk::{
    compute_jwk_thumbprint, sign_jwt_hs256, verify_jwt_hs256,
};
use crate::runtime::stdlib_src::crypto::key_exchange::{x25519_exchange, x25519_generate};
use crate::runtime::stdlib_src::crypto::merkle_file::{
    content_id, decrypt_file_payload, encrypt_file_payload,
};
use crate::runtime::stdlib_src::crypto::password::{
    hash_password_argon2id, verify_password_argon2id,
};
use crate::runtime::stdlib_src::crypto::pem_der_cert::parse_x509_pem;
use crate::runtime::stdlib_src::crypto::signatures::{
    ed25519_generate, ed25519_sign, ed25519_verify,
};
use crate::runtime::stdlib_src::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::sync::Arc;

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "Crypto.hash",
        "crypto",
        "Compute cryptographic hash digest",
        builtin_hash,
    );
    registry.register(
        "Crypto.sha256",
        "crypto",
        "Compute SHA-256 digest",
        builtin_sha256,
    );
    registry.register(
        "Crypto.sha512",
        "crypto",
        "Compute SHA-512 digest",
        builtin_sha512,
    );
    registry.register(
        "Crypto.blake3",
        "crypto",
        "Compute BLAKE3 digest",
        builtin_blake3,
    );
    registry.register(
        "Crypto.hashFile",
        "crypto",
        "Compute hash digest of file",
        builtin_hash_file,
    );
    registry.register(
        "Crypto.hmac",
        "crypto",
        "Compute HMAC authentication tag",
        builtin_hmac,
    );
    registry.register(
        "Crypto.hmacVerify",
        "crypto",
        "Constant-time verify HMAC tag",
        builtin_hmac_verify,
    );
    registry.register(
        "Crypto.hkdfDerive",
        "crypto",
        "Derive key using HKDF",
        builtin_hkdf_derive,
    );
    registry.register(
        "Crypto.passwordHash",
        "crypto",
        "Hash password using Argon2id",
        builtin_password_hash,
    );
    registry.register(
        "Crypto.passwordVerify",
        "crypto",
        "Verify Argon2id password hash",
        builtin_password_verify,
    );
    registry.register(
        "Crypto.encryptAead",
        "crypto",
        "Authenticated encryption (AEAD)",
        builtin_encrypt_aead,
    );
    registry.register(
        "Crypto.decryptAead",
        "crypto",
        "Authenticated decryption (AEAD)",
        builtin_decrypt_aead,
    );
    registry.register(
        "Crypto.seal",
        "crypto",
        "Seal plaintext with random nonce",
        builtin_seal,
    );
    registry.register(
        "Crypto.ed25519Generate",
        "crypto",
        "Generate Ed25519 keypair",
        builtin_ed25519_generate,
    );
    registry.register(
        "Crypto.ed25519Sign",
        "crypto",
        "Sign message with Ed25519",
        builtin_ed25519_sign,
    );
    registry.register(
        "Crypto.ed25519Verify",
        "crypto",
        "Verify Ed25519 signature",
        builtin_ed25519_verify,
    );
    registry.register(
        "Crypto.x25519Generate",
        "crypto",
        "Generate X25519 keypair",
        builtin_x25519_generate,
    );
    registry.register(
        "Crypto.x25519Exchange",
        "crypto",
        "Perform X25519 Diffie-Hellman",
        builtin_x25519_exchange,
    );
    registry.register(
        "Crypto.constantTimeEquals",
        "crypto",
        "Constant-time equality comparison",
        builtin_constant_time_equals,
    );
    registry.register(
        "Crypto.encodeHex",
        "crypto",
        "Encode bytes to hex string",
        builtin_encode_hex,
    );
    registry.register(
        "Crypto.decodeHex",
        "crypto",
        "Decode hex string to bytes",
        builtin_decode_hex,
    );
    registry.register(
        "Crypto.encodeBase64",
        "crypto",
        "Encode bytes to base64 string",
        builtin_encode_base64,
    );
    registry.register(
        "Crypto.decodeBase64",
        "crypto",
        "Decode base64 string to bytes",
        builtin_decode_base64,
    );
    registry.register(
        "Crypto.parseCert",
        "crypto",
        "Parse X.509 PEM Certificate",
        builtin_parse_cert,
    );
    registry.register(
        "Crypto.signJwt",
        "crypto",
        "Sign JWT with HS256",
        builtin_sign_jwt,
    );
    registry.register(
        "Crypto.verifyJwt",
        "crypto",
        "Safely verify JWT with algorithm whitelist",
        builtin_verify_jwt,
    );
    registry.register(
        "Crypto.jwkThumbprint",
        "crypto",
        "Compute RFC 7638 JWK thumbprint",
        builtin_jwk_thumbprint,
    );
    registry.register(
        "Crypto.contentId",
        "crypto",
        "Compute content-addressable identifier",
        builtin_content_id,
    );
    registry.register(
        "Crypto.encryptFile",
        "crypto",
        "Encrypt file payload",
        builtin_encrypt_file,
    );
    registry.register(
        "Crypto.decryptFile",
        "crypto",
        "Decrypt file payload",
        builtin_decrypt_file,
    );
}

fn get_str(val: &Value) -> Option<&str> {
    match val {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

// Helper to extract bytes from Value::Array or Value::Str
fn value_to_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Array(arr) => {
            let mut bytes = Vec::with_capacity(arr.len());
            for elem in arr.iter() {
                if let Value::Number(f) = elem {
                    bytes.push(*f as u8);
                } else if let Value::U8(n) = elem {
                    bytes.push(*n);
                } else if let Value::I64(n) = elem {
                    bytes.push(*n as u8);
                } else {
                    return Err("Expected array of byte numbers".to_string());
                }
            }
            Ok(bytes)
        }
        _ => Err("Expected String or Byte Array".to_string()),
    }
}

fn bytes_to_value(bytes: &[u8]) -> Value {
    Value::Array(bytes.iter().map(|&b| Value::Number(b as f64)).collect())
}

fn builtin_hash(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let algo_str = args.get(0).and_then(|v| get_str(v)).unwrap_or("sha256");
    let data = args.get(1).ok_or("Crypto.hash requires (algo, data)")?;
    let bytes = value_to_bytes(data)?;
    let algo = HashAlgo::from_str(algo_str)?;
    let digest = hash_bytes(&algo, &bytes);
    Ok(bytes_to_value(&digest))
}

fn builtin_sha256(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = args.get(0).ok_or("Crypto.sha256 requires (data)")?;
    let bytes = value_to_bytes(data)?;
    let digest = hash_bytes(&HashAlgo::Sha256, &bytes);
    Ok(Value::Str(hex::encode(digest)))
}

fn builtin_sha512(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = args.get(0).ok_or("Crypto.sha512 requires (data)")?;
    let bytes = value_to_bytes(data)?;
    let digest = hash_bytes(&HashAlgo::Sha512, &bytes);
    Ok(Value::Str(hex::encode(digest)))
}

fn builtin_blake3(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = args.get(0).ok_or("Crypto.blake3 requires (data)")?;
    let bytes = value_to_bytes(data)?;
    let digest = hash_bytes(&HashAlgo::Blake3, &bytes);
    Ok(Value::Str(hex::encode(digest)))
}

fn builtin_hash_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let path = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.hashFile requires (path)")?;
    let algo_str = args.get(1).and_then(|v| get_str(v)).unwrap_or("sha256");
    let algo = HashAlgo::from_str(algo_str)?;
    let digest = hash_file(&algo, path)?;
    Ok(Value::Str(hex::encode(digest)))
}

fn builtin_hmac(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let key = value_to_bytes(args.get(0).ok_or("Crypto.hmac requires (key, message)")?)?;
    let message = value_to_bytes(args.get(1).ok_or("Crypto.hmac requires (key, message)")?)?;
    let tag = hmac_sha256(&key, &message)?;
    Ok(bytes_to_value(&tag))
}

fn builtin_hmac_verify(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let key = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.hmacVerify requires (key, message, tag)")?,
    )?;
    let message = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.hmacVerify requires (key, message, tag)")?,
    )?;
    let tag = value_to_bytes(
        args.get(2)
            .ok_or("Crypto.hmacVerify requires (key, message, tag)")?,
    )?;
    let valid = hmac_verify(&key, &message, &tag, "sha256");
    Ok(Value::Bool(valid))
}

fn builtin_hkdf_derive(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let ikm = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.hkdfDerive requires (ikm, salt, info, length)")?,
    )?;
    let salt = if let Some(s) = args.get(1) {
        if matches!(s, Value::Null) {
            None
        } else {
            Some(value_to_bytes(s)?)
        }
    } else {
        None
    };
    let info = args
        .get(2)
        .and_then(|v| get_str(v))
        .unwrap_or("")
        .as_bytes()
        .to_vec();
    let len = args
        .get(3)
        .and_then(|v| match v {
            Value::Number(n) => Some(*n as usize),
            Value::I64(n) => Some(*n as usize),
            _ => None,
        })
        .unwrap_or(32);
    let derived = hkdf_sha256_derive(salt.as_deref(), &ikm, &info, len)?;
    Ok(bytes_to_value(&derived))
}

fn builtin_password_hash(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let password = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.passwordHash requires (password)")?;
    let hash_str = hash_password_argon2id(password)?;
    Ok(Value::Str(hash_str))
}

fn builtin_password_verify(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let password = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.passwordVerify requires (password, hash)")?;
    let hash_str = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.passwordVerify requires (password, hash)")?;
    let valid = verify_password_argon2id(password, hash_str);
    Ok(Value::Bool(valid))
}

fn builtin_encrypt_aead(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let key = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.encryptAead requires (key, nonce, plaintext)")?,
    )?;
    let nonce = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.encryptAead requires (key, nonce, plaintext)")?,
    )?;
    let plaintext = value_to_bytes(
        args.get(2)
            .ok_or("Crypto.encryptAead requires (key, nonce, plaintext)")?,
    )?;
    let algo = AeadAlgo::Aes256Gcm;
    let ciphertext = encrypt_aead(&algo, &key, &nonce, &plaintext, None)?;
    Ok(bytes_to_value(&ciphertext))
}

fn builtin_decrypt_aead(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let key = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.decryptAead requires (key, nonce, ciphertext)")?,
    )?;
    let nonce = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.decryptAead requires (key, nonce, ciphertext)")?,
    )?;
    let ciphertext = value_to_bytes(
        args.get(2)
            .ok_or("Crypto.decryptAead requires (key, nonce, ciphertext)")?,
    )?;
    let algo = AeadAlgo::Aes256Gcm;
    let plaintext = decrypt_aead(&algo, &key, &nonce, &ciphertext, None)?;
    Ok(bytes_to_value(&plaintext))
}

fn builtin_seal(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let key = value_to_bytes(args.get(0).ok_or("Crypto.seal requires (key, plaintext)")?)?;
    let plaintext = value_to_bytes(args.get(1).ok_or("Crypto.seal requires (key, plaintext)")?)?;
    let algo = AeadAlgo::Aes256Gcm;
    let (nonce, ciphertext) = seal_random_nonce(&algo, &key, &plaintext, None)?;

    let mut obj = FastMap::default();
    obj.insert("nonce".to_string(), bytes_to_value(&nonce));
    obj.insert("ciphertext".to_string(), bytes_to_value(&ciphertext));
    Ok(Value::Object(Arc::new(obj)))
}

fn builtin_ed25519_generate(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let kp = ed25519_generate();
    let mut obj = FastMap::default();
    obj.insert("privateKey".to_string(), bytes_to_value(&kp.private_bytes));
    obj.insert("publicKey".to_string(), bytes_to_value(&kp.public_bytes));
    Ok(Value::Object(Arc::new(obj)))
}

fn builtin_ed25519_sign(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let priv_key = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.ed25519Sign requires (privateKey, message)")?,
    )?;
    let message = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.ed25519Sign requires (privateKey, message)")?,
    )?;
    let sig = ed25519_sign(&priv_key, &message)?;
    Ok(bytes_to_value(&sig))
}

fn builtin_ed25519_verify(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let pub_key = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.ed25519Verify requires (publicKey, message, signature)")?,
    )?;
    let message = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.ed25519Verify requires (publicKey, message, signature)")?,
    )?;
    let sig = value_to_bytes(
        args.get(2)
            .ok_or("Crypto.ed25519Verify requires (publicKey, message, signature)")?,
    )?;
    let valid = ed25519_verify(&pub_key, &message, &sig);
    Ok(Value::Bool(valid))
}

fn builtin_x25519_generate(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let kp = x25519_generate();
    let mut obj = FastMap::default();
    obj.insert("secretKey".to_string(), bytes_to_value(&kp.secret_bytes));
    obj.insert("publicKey".to_string(), bytes_to_value(&kp.public_bytes));
    Ok(Value::Object(Arc::new(obj)))
}

fn builtin_x25519_exchange(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let my_secret = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.x25519Exchange requires (secretKey, peerPublicKey)")?,
    )?;
    let peer_pub = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.x25519Exchange requires (secretKey, peerPublicKey)")?,
    )?;
    let shared = x25519_exchange(&my_secret, &peer_pub)?;
    Ok(bytes_to_value(&shared))
}

fn builtin_constant_time_equals(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let a = value_to_bytes(
        args.get(0)
            .ok_or("Crypto.constantTimeEquals requires (a, b)")?,
    )?;
    let b = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.constantTimeEquals requires (a, b)")?,
    )?;
    Ok(Value::Bool(constant_time_compare(&a, &b)))
}

fn builtin_encode_hex(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("Crypto.encodeHex requires (bytes)")?)?;
    Ok(Value::Str(hex::encode(bytes)))
}

fn builtin_decode_hex(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.decodeHex requires (hexString)")?;
    let bytes = hex::decode(s).map_err(|e| format!("Invalid hex string: {}", e))?;
    Ok(bytes_to_value(&bytes))
}

fn builtin_encode_base64(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("Crypto.encodeBase64 requires (bytes)")?)?;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
    Ok(Value::Str(b64))
}

fn builtin_decode_base64(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.decodeBase64 requires (base64String)")?;
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .map_err(|e| format!("Invalid base64 string: {}", e))?;
    Ok(bytes_to_value(&bytes))
}

fn builtin_generate_self_signed_cert(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (cert_pem, key_pem) = crate::runtime::stdlib_src::tls::cert::generate_self_signed_cert()
            .map_err(|e| e.to_string())?;

        // Automatically write cert.pem and key.pem to the root directory
        let _ = std::fs::write("cert.pem", &cert_pem);
        let _ = std::fs::write("key.pem", &key_pem);

        let mut obj = FastMap::default();
        obj.insert("certPem".to_string(), Value::Str(cert_pem.clone()));
        obj.insert("keyPem".to_string(), Value::Str(key_pem.clone()));
        obj.insert("cert".to_string(), Value::Str(cert_pem));
        obj.insert("key".to_string(), Value::Str(key_pem));
        obj.insert("certPath".to_string(), Value::Str("cert.pem".to_string()));
        obj.insert("keyPath".to_string(), Value::Str("key.pem".to_string()));
        Ok(Value::Object(Arc::new(obj)))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err("TLS certificate generation is not supported on WebAssembly".to_string())
    }
}

fn builtin_parse_cert(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let pem = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.parseCert requires (pemString)")?;
    let cert = parse_x509_pem(pem)?;
    let mut obj = FastMap::default();
    obj.insert("subject".to_string(), Value::Str(cert.subject));
    obj.insert("issuer".to_string(), Value::Str(cert.issuer));
    obj.insert("serialNumber".to_string(), Value::Str(cert.serial_hex));
    obj.insert("notBefore".to_string(), Value::Str(cert.not_before));
    obj.insert("notAfter".to_string(), Value::Str(cert.not_after));
    obj.insert(
        "fingerprintSha256".to_string(),
        Value::Str(cert.fingerprint_sha256),
    );
    obj.insert("isCa".to_string(), Value::Bool(cert.is_ca));
    Ok(Value::Object(Arc::new(obj)))
}

fn builtin_sign_jwt(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let claims_str = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.signJwt requires (claimsJsonStr, secret)")?;
    let secret = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.signJwt requires (claimsJsonStr, secret)")?,
    )?;
    let claims: serde_json::Value =
        serde_json::from_str(claims_str).map_err(|e| format!("Invalid JSON claims: {}", e))?;
    let token = sign_jwt_hs256(&claims, &secret)?;
    Ok(Value::Str(token))
}

fn builtin_verify_jwt(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let token = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.verifyJwt requires (token, secret, allowedAlgos)")?;
    let secret = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.verifyJwt requires (token, secret, allowedAlgos)")?,
    )?;
    let claims = verify_jwt_hs256(token, &secret, &["HS256"])?;
    Ok(Value::Str(claims.to_string()))
}

fn builtin_jwk_thumbprint(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let jwk_str = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.jwkThumbprint requires (jwkJsonStr)")?;
    let jwk: serde_json::Value =
        serde_json::from_str(jwk_str).map_err(|e| format!("Invalid JWK JSON: {}", e))?;
    let thumbprint = compute_jwk_thumbprint(&jwk)?;
    Ok(Value::Str(thumbprint))
}

fn builtin_content_id(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Crypto.contentId requires (data)")?)?;
    Ok(Value::Str(content_id(&data)))
}

fn builtin_encrypt_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let passphrase = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.encryptFile requires (passphrase, plaintextBytes)")?;
    let plaintext = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.encryptFile requires (passphrase, plaintextBytes)")?,
    )?;
    let payload = encrypt_file_payload(passphrase, &plaintext)?;
    Ok(bytes_to_value(&payload))
}

fn builtin_decrypt_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let passphrase = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Crypto.decryptFile requires (passphrase, payloadBytes)")?;
    let payload = value_to_bytes(
        args.get(1)
            .ok_or("Crypto.decryptFile requires (passphrase, payloadBytes)")?,
    )?;
    let plaintext = decrypt_file_payload(passphrase, &payload)?;
    Ok(bytes_to_value(&plaintext))
}

fn builtin_crypto_uuid(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = crate::runtime::stdlib_src::random::api::DEFAULT_GLOBAL_PRNG
        .lock()
        .unwrap();
    Ok(Value::Str(
        crate::runtime::stdlib_src::random::strings::random_uuid4(&mut g),
    ))
}

pub fn build_crypto_module_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "uuid".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_crypto_uuid))),
    );
    map.insert(
        "randomUuid".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_crypto_uuid))),
    );
    map.insert(
        "randomInt".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(|_env, args| {
            let min = args.first().and_then(|v| match v { Value::Number(n) => Some(*n as i64), Value::I64(n) => Some(*n), _ => None }).unwrap_or(0);
            let max = args.get(1).and_then(|v| match v { Value::Number(n) => Some(*n as i64), Value::I64(n) => Some(*n), _ => None }).unwrap_or(255);
            let mut g = crate::runtime::stdlib_src::random::api::DEFAULT_GLOBAL_PRNG.lock().unwrap();
            let res = crate::runtime::stdlib_src::random::bounded::next_i64_range_inclusive(&mut g, min, max).unwrap_or(min);
            Ok(Value::Number(res as f64))
        }))),
    );
    map.insert(
        "randomBytes".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(|_env, args| {
            let len = args.first().and_then(|v| match v { Value::Number(n) => Some(*n as usize), Value::I64(n) => Some(*n as usize), _ => None }).unwrap_or(16);
            let mut g = crate::runtime::stdlib_src::random::api::DEFAULT_GLOBAL_PRNG.lock().unwrap();
            let mut vals: Vec<Value> = Vec::with_capacity(len);
            for _ in 0..len {
                let b = crate::runtime::stdlib_src::random::bounded::next_i64_range_inclusive(&mut g, 0, 255).unwrap_or(0) as u8;
                vals.push(Value::Number(b as f64));
            }
            Ok(Value::Array(vals))
        }))),
    );
    map.insert(
        "hash".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_hash))),
    );
    map.insert(
        "sha1".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(|_env, args| {
            let input = match args.first() {
                Some(Value::Str(s)) => s.as_bytes().to_vec(),
                Some(Value::Array(arr)) => arr.iter().filter_map(|v| match v { Value::Number(n) => Some(*n as u8), _ => None }).collect(),
                _ => Vec::new(),
            };
            let digest = hash_bytes(&HashAlgo::Sha1, &input);
            let vals: Vec<Value> = digest.into_iter().map(|b| Value::Number(b as f64)).collect();
            Ok(Value::Array(vals))
        }))),
    );
    map.insert(
        "sha256".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_sha256))),
    );
    map.insert(
        "sha512".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_sha512))),
    );
    map.insert(
        "blake3".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_blake3))),
    );
    map.insert(
        "hashFile".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_hash_file))),
    );
    map.insert(
        "hmac".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_hmac))),
    );
    map.insert(
        "hmacVerify".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_hmac_verify))),
    );
    map.insert(
        "hkdfDerive".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_hkdf_derive))),
    );
    map.insert(
        "passwordHash".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_password_hash,
        ))),
    );
    map.insert(
        "passwordVerify".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_password_verify,
        ))),
    );
    map.insert(
        "encryptAead".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_encrypt_aead,
        ))),
    );
    map.insert(
        "decryptAead".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_decrypt_aead,
        ))),
    );
    map.insert(
        "seal".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_seal))),
    );
    map.insert(
        "ed25519Generate".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_ed25519_generate,
        ))),
    );
    map.insert(
        "ed25519Sign".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_ed25519_sign,
        ))),
    );
    map.insert(
        "ed25519Verify".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_ed25519_verify,
        ))),
    );
    map.insert(
        "x25519Generate".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_x25519_generate,
        ))),
    );
    map.insert(
        "x25519Exchange".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_x25519_exchange,
        ))),
    );
    map.insert(
        "constantTimeEquals".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_constant_time_equals,
        ))),
    );
    map.insert(
        "encodeHex".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_encode_hex))),
    );
    map.insert(
        "decodeHex".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_decode_hex))),
    );
    map.insert(
        "encodeBase64".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_encode_base64,
        ))),
    );
    map.insert(
        "decodeBase64".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_decode_base64,
        ))),
    );
    map.insert(
        "parseCert".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_parse_cert))),
    );
    map.insert(
        "generateSelfSignedCert".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_generate_self_signed_cert,
        ))),
    );
    map.insert(
        "signJwt".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_sign_jwt))),
    );
    map.insert(
        "verifyJwt".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_verify_jwt))),
    );
    map.insert(
        "jwkThumbprint".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_jwk_thumbprint,
        ))),
    );
    map.insert(
        "contentId".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(builtin_content_id))),
    );
    map.insert(
        "encryptFile".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_encrypt_file,
        ))),
    );
    map.insert(
        "decryptFile".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(
            builtin_decrypt_file,
        ))),
    );
    Value::Object(Arc::new(map))
}
