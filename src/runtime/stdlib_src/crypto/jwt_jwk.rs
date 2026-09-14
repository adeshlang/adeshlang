//! Safe JWT, JWS, JWK & JWK Thumbprint (RFC 7638) for AdeshLang Crypto.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct GenericClaims {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    #[serde(flatten)]
    pub extra: JsonMap<String, JsonValue>,
}

pub fn sign_jwt_hs256(claims: &JsonValue, secret: &[u8]) -> Result<String, String> {
    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(secret);
    encode(&header, claims, &key).map_err(|e| format!("JWT HS256 sign error: {}", e))
}

pub fn sign_jwt_eddsa(claims: &JsonValue, ed25519_pem: &str) -> Result<String, String> {
    let header = Header::new(Algorithm::EdDSA);
    let key = EncodingKey::from_ed_pem(ed25519_pem.as_bytes())
        .map_err(|e| format!("Invalid Ed25519 PEM: {}", e))?;
    encode(&header, claims, &key).map_err(|e| format!("JWT EdDSA sign error: {}", e))
}

pub fn verify_jwt_hs256(
    token: &str,
    secret: &[u8],
    allowed_algos: &[&str],
) -> Result<JsonValue, String> {
    if !allowed_algos
        .iter()
        .any(|&a| a.eq_ignore_ascii_case("HS256"))
    {
        return Err(
            "Algorithm HS256 not in allowed algorithm whitelist for verification".to_string(),
        );
    }

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let key = DecodingKey::from_secret(secret);

    let token_data = decode::<JsonValue>(token, &key, &validation)
        .map_err(|e| format!("JWT verification failed: {}", e))?;
    Ok(token_data.claims)
}

pub fn verify_jwt_eddsa(
    token: &str,
    ed25519_pub_pem: &str,
    allowed_algos: &[&str],
) -> Result<JsonValue, String> {
    if !allowed_algos
        .iter()
        .any(|&a| a.eq_ignore_ascii_case("EdDSA"))
    {
        return Err(
            "Algorithm EdDSA not in allowed algorithm whitelist for verification".to_string(),
        );
    }

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;
    let key = DecodingKey::from_ed_pem(ed25519_pub_pem.as_bytes())
        .map_err(|e| format!("Invalid Ed25519 public PEM: {}", e))?;

    let token_data = decode::<JsonValue>(token, &key, &validation)
        .map_err(|e| format!("JWT verification failed: {}", e))?;
    Ok(token_data.claims)
}

pub fn compute_jwk_thumbprint(jwk_json: &JsonValue) -> Result<String, String> {
    let obj = jwk_json
        .as_object()
        .ok_or_else(|| "JWK must be a JSON object".to_string())?;
    let kty = obj
        .get("kty")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "JWK missing 'kty'".to_string())?;

    let mut canonical_map = BTreeMap::new();
    canonical_map.insert("kty".to_string(), JsonValue::String(kty.to_string()));

    match kty {
        "RSA" => {
            let e = obj
                .get("e")
                .cloned()
                .ok_or_else(|| "RSA JWK missing 'e'".to_string())?;
            let n = obj
                .get("n")
                .cloned()
                .ok_or_else(|| "RSA JWK missing 'n'".to_string())?;
            canonical_map.insert("e".to_string(), e);
            canonical_map.insert("n".to_string(), n);
        }
        "EC" => {
            let crv = obj
                .get("crv")
                .cloned()
                .ok_or_else(|| "EC JWK missing 'crv'".to_string())?;
            let x = obj
                .get("x")
                .cloned()
                .ok_or_else(|| "EC JWK missing 'x'".to_string())?;
            let y = obj
                .get("y")
                .cloned()
                .ok_or_else(|| "EC JWK missing 'y'".to_string())?;
            canonical_map.insert("crv".to_string(), crv);
            canonical_map.insert("x".to_string(), x);
            canonical_map.insert("y".to_string(), y);
        }
        "OKP" => {
            let crv = obj
                .get("crv")
                .cloned()
                .ok_or_else(|| "OKP JWK missing 'crv'".to_string())?;
            let x = obj
                .get("x")
                .cloned()
                .ok_or_else(|| "OKP JWK missing 'x'".to_string())?;
            canonical_map.insert("crv".to_string(), crv);
            canonical_map.insert("x".to_string(), x);
        }
        "oct" => {
            let k = obj
                .get("k")
                .cloned()
                .ok_or_else(|| "oct JWK missing 'k'".to_string())?;
            canonical_map.insert("k".to_string(), k);
        }
        _ => return Err(format!("Unsupported JWK kty: '{}'", kty)),
    }

    let canonical_json = serde_json::to_string(&canonical_map).map_err(|e| e.to_string())?;
    let hash = Sha256::digest(canonical_json.as_bytes());
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        hash,
    ))
}
