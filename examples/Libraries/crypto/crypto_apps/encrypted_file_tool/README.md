# Encrypted File Tool Mini-Project

A command-line tool built in AdeshLang that encrypts and decrypts files using PBKDF2-SHA256 key derivation and AES-256-GCM authenticated encryption.

## Features
- Salted key derivation (100,000 rounds of PBKDF2)
- Random 96-bit nonce generation per encryption
- Sealed magic header (`ADCRYPT1`) verification

## Run
```bash
cargo run -- run examples/crypto_apps/encrypted_file_tool/main.adl
```
