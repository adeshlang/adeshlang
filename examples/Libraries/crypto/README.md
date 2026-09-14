# AdeshLang Cryptography Standard Library Examples & API Reference

This directory contains a complete, numbered suite of 20 `.adesh` example scripts demonstrating the capabilities of AdeshLang's `Crypto` standard library (`import Crypto;`).

---

## 📁 Examples Directory Index

| Number | File | Category / Feature Demonstrated |
| :--- | :--- | :--- |
| **01** | `01_hashing_basic.adesh` | Hashing string data with SHA-256, SHA-512, and BLAKE3 |
| **02** | `02_hashing_streaming_file.adesh` | Streaming large file digests without loading whole files into RAM |
| **03** | `03_hmac_authentication.adesh` | HMAC-SHA256 message authentication tags and constant-time verification |
| **04** | `04_hkdf_key_derivation.adesh` | HKDF extract/expand key expansion for derived subkeys |
| **05** | `05_argon2id_password_hashing.adesh` | Memory-hard Argon2id ($argon2id$) password hashing and verification |
| **06** | `06_password_policy_validation.adesh` | Password complexity, length, and policy enforcement checks |
| **07** | `07_aead_aes_gcm_seal.adesh` | AEAD Authenticated Encryption (AES-256-GCM) with random 96-bit nonces |
| **08** | `08_aead_chacha20_poly1305.adesh` | Software-optimized ChaCha20-Poly1305 AEAD cipher encryption |
| **09** | `09_secure_random_tokens.adesh` | CSPRNG secure random hex/base64 tokens and UUID v4 generation |
| **10** | `10_ed25519_signatures.adesh` | Ed25519 Edwards-curve digital signatures and verification |
| **11** | `11_x25519_key_exchange.adesh` | X25519 Curve25519 Diffie-Hellman key agreement protocol |
| **12** | `12_constant_time_equals.adesh` | Constant-time byte equality comparisons to prevent timing side-channels |
| **13** | `13_hex_base64_encodings.adesh` | Hexadecimal and Base64 byte array encoding and decoding |
| **14** | `14_x509_certificate_inspection.adesh` | X.509 Certificate PEM parsing, SAN, subject, and fingerprint extraction |
| **15** | `15_jwt_safe_verification.adesh` | Safe JWT signing and verification with explicit algorithm whitelist |
| **16** | `16_jwk_rfc7638_thumbprint.adesh` | RFC 7638 compliant canonical JWK thumbprint calculation |
| **17** | `17_merkle_tree_proofs.adesh` | Content-addressable identifiers (`contentId`) (`sha256-<digest>`) |
| **18** | `18_encrypted_file_payloads.adesh` | End-to-end encrypted file container creation and decryption (`AdeshCryptFile`) |
| **19** | `19_edge_cases_and_error_handling.adesh` | Invalid key lengths, wrong key decryption, and security boundaries |
| **20** | `20_real_world_api_signature.adesh` | Real-world API HTTP request HMAC signature verification system |

---

## 📖 Complete API Reference (`import Crypto;`)

### 1. Hashing & File Integrity
- `Crypto.sha256(data)` -> `String` (Hex)
  - Computes SHA-256 digest of input text or byte array.
- `Crypto.sha512(data)` -> `String` (Hex)
  - Computes SHA-512 digest of input text or byte array.
- `Crypto.blake3(data)` -> `String` (Hex)
  - Computes high-speed BLAKE3 digest.
- `Crypto.hash(algo, data)` -> `Array<u8>`
  - Generic hash function (`"sha256"`, `"sha512"`, `"sha3-256"`, `"blake3"`, `"blake2b"`).
- `Crypto.hashFile(filePath, algo)` -> `String` (Hex)
  - Computes file digest using a 64KB buffered stream reader.

### 2. Message Authentication (HMAC) & Key Derivation (HKDF)
- `Crypto.hmac(key, message)` -> `Array<u8>`
  - Computes HMAC-SHA256 authentication tag.
- `Crypto.hmacVerify(key, message, tag)` -> `Bool`
  - Verifies HMAC tag in constant time.
- `Crypto.hkdfDerive(ikm, salt, info, length)` -> `Array<u8>`
  - Derives `length` bytes of key material from input key material (`ikm`).

### 3. Password Hashing (Argon2id)
- `Crypto.hashPassword(password)` -> `String` ($argon2id$)
  - Hashes password using memory-hard Argon2id with a cryptographically secure random salt.
- `Crypto.verifyPassword(password, hash)` -> `Bool`
  - Verifies password against self-describing Argon2id hash string.

### 4. Authenticated Encryption (AEAD)
- `Crypto.seal(key, plaintext)` -> `Object` `{ nonce, ciphertext }`
  - Seals plaintext using AES-256-GCM and automatically generates a fresh random 96-bit nonce.
- `Crypto.encryptAead(key, nonce, plaintext)` -> `Array<u8>`
  - Encrypts plaintext using explicit key and nonce.
- `Crypto.decryptAead(key, nonce, ciphertext)` -> `Array<u8>`
  - Decrypts ciphertext and verifies authentication tag. Returns `null` on tag mismatch or wrong key.

### 5. Secure Randomness & Tokens (CSPRNG)
- `Crypto.randomBytes(count)` -> `Array<u8>`
  - Generates `count` bytes of OS entropy-backed cryptographically secure random numbers.
- `Crypto.randomToken(length, encoding)` -> `String`
  - Generates secure random token in `"hex"` or `"base64"` encoding.
- `Crypto.randomUuid()` -> `String`
  - Generates cryptographically secure UUID v4 string.

### 6. Digital Signatures & Key Exchange
- `Crypto.generateEd25519()` -> `Object` `{ privateKey, publicKey }`
  - Generates 32-byte Ed25519 signing key and 32-byte verifying key.
- `Crypto.signEd25519(privateKey, message)` -> `Array<u8>`
  - Signs message with Ed25519 private key.
- `Crypto.verifyEd25519(publicKey, message, signature)` -> `Bool`
  - Verifies Ed25519 signature.
- `Crypto.generateX25519()` -> `Object` `{ secretKey, publicKey }`
  - Generates Curve25519 Diffie-Hellman keypair.
- `Crypto.exchangeX25519(secretKey, peerPublicKey)` -> `Array<u8>`
  - Performs X25519 key agreement deriving a 32-byte shared secret.

### 7. Constant-Time Comparisons
- `Crypto.constantTimeEquals(a, b)` -> `Bool`
  - Compares two byte arrays or strings in constant time to prevent execution timing leaks.

### 8. Encodings
- `Crypto.encodeHex(bytes)` / `Crypto.decodeHex(hexStr)`
- `Crypto.encodeBase64(bytes)` / `Crypto.decodeBase64(b64Str)`

### 9. Certificates & JWT
- `Crypto.parseCert(pemStr)` -> `Object` `{ subject, issuer, serialNumber, notBefore, notAfter, fingerprintSha256, isCa }`
  - Parses X.509 Certificate PEM metadata.
- `Crypto.signJwt(claimsJsonStr, secret)` -> `String`
  - Signs JWT payload using HS256.
- `Crypto.verifyJwt(token, secret, allowedAlgos)` -> `String` (JSON)
  - Verifies JWT enforcing an explicit algorithm whitelist array (e.g., `["HS256"]`).
- `Crypto.jwkThumbprint(jwkJsonStr)` -> `String` (Base64Url)
  - Computes canonical RFC 7638 JWK thumbprint.

### 10. Encrypted File Container & Content IDs
- `Crypto.contentId(data)` -> `String` (`sha256-<digest>`)
- `Crypto.encryptFile(passphrase, plaintextBytes)` -> `Array<u8>`
- `Crypto.decryptFile(passphrase, payloadBytes)` -> `Array<u8>`

---

## 🏃 Running Examples

To run any example script with the AdeshLang runtime:

```bash
cargo run --bin adeshlang -- run examples/crypto/01_hashing_basic.adesh
cargo run --bin adeshlang -- run examples/crypto/05_argon2id_password_hashing.adesh
cargo run --bin adeshlang -- run examples/crypto/07_aead_aes_gcm_seal.adesh
cargo run --bin adeshlang -- run examples/crypto/10_ed25519_signatures.adesh
cargo run --bin adeshlang -- run examples/crypto/15_jwt_safe_verification.adesh
```
