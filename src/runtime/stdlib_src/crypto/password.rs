//! Password Hashing (Argon2id, PBKDF2, scrypt) & Password Policy for AdeshLang Crypto.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use pbkdf2::pbkdf2_hmac;
use scrypt::{Params as ScryptParams, scrypt};
use sha2::{Sha256, Sha512};

pub fn hash_password_argon2id(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("Argon2id hashing failed: {}", e))?;
    Ok(password_hash.to_string())
}

pub fn verify_password_argon2id(password: &str, hash_str: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash_str) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn hash_pbkdf2_sha256(password: &[u8], salt: &[u8], rounds: u32, key_len: usize) -> Vec<u8> {
    let mut key = vec![0u8; key_len];
    pbkdf2_hmac::<Sha256>(password, salt, rounds, &mut key);
    key
}

pub fn hash_pbkdf2_sha512(password: &[u8], salt: &[u8], rounds: u32, key_len: usize) -> Vec<u8> {
    let mut key = vec![0u8; key_len];
    pbkdf2_hmac::<Sha512>(password, salt, rounds, &mut key);
    key
}

pub fn hash_scrypt(
    password: &[u8],
    salt: &[u8],
    log_n: u8,
    r: u32,
    p: u32,
    key_len: usize,
) -> Result<Vec<u8>, String> {
    let params = ScryptParams::new(log_n, r, p, key_len)
        .map_err(|e| format!("Invalid scrypt params: {}", e))?;
    let mut key = vec![0u8; key_len];
    scrypt(password, salt, &params, &mut key).map_err(|e| format!("scrypt failed: {}", e))?;
    Ok(key)
}

#[derive(Debug, Clone)]
pub struct PasswordPolicy {
    pub min_len: usize,
    pub max_len: usize,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_digit: bool,
    pub require_symbol: bool,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_len: 12,
            max_len: 128,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_symbol: false,
        }
    }
}

impl PasswordPolicy {
    pub fn validate(&self, password: &str) -> Result<(), String> {
        let char_count = password.chars().count();
        if char_count < self.min_len {
            return Err(format!(
                "Password length must be at least {} characters",
                self.min_len
            ));
        }
        if char_count > self.max_len {
            return Err(format!(
                "Password length cannot exceed {} characters",
                self.max_len
            ));
        }
        if self.require_uppercase && !password.chars().any(|c| c.is_uppercase()) {
            return Err("Password must contain at least one uppercase letter".to_string());
        }
        if self.require_lowercase && !password.chars().any(|c| c.is_lowercase()) {
            return Err("Password must contain at least one lowercase letter".to_string());
        }
        if self.require_digit && !password.chars().any(|c| c.is_numeric()) {
            return Err("Password must contain at least one numeric digit".to_string());
        }
        if self.require_symbol && !password.chars().any(|c| !c.is_alphanumeric()) {
            return Err("Password must contain at least one special symbol".to_string());
        }
        Ok(())
    }
}
