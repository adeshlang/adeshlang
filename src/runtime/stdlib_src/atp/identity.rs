//! Endpoint identity authentication via Ed25519 signatures.
//!
//! Identity proofs are signed over an explicit domain-separated context:
//! `ATP/1 identity proof` || role || version || CIDs || ephemeral keys ||
//! transport params || retry token || transcript hash.

use super::errors::{AtpError, AtpResult};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

pub const ED25519_PUBKEY_LEN: usize = 32;
pub const ED25519_SIG_LEN: usize = 64;
pub const IDENTITY_PROOF_LEN: usize = ED25519_PUBKEY_LEN + ED25519_SIG_LEN;

const DOMAIN_LABEL: &[u8] = b"ATP/1 identity proof";

/// Handshake role for identity proof domain separation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityRole {
    Client,
    Server,
}

/// Explicit signing context bound to the handshake.
#[derive(Debug, Clone)]
pub struct IdentityProofContext<'a> {
    pub role: IdentityRole,
    pub version: u32,
    pub client_cid: &'a [u8],
    pub server_cid: &'a [u8],
    pub local_ephemeral: &'a [u8],
    pub peer_ephemeral: &'a [u8],
    pub transport_params: &'a [u8],
    pub retry_token: &'a [u8],
    /// Transcript hash immediately before the signed message is appended.
    pub transcript_hash: [u8; 32],
}

impl<'a> IdentityProofContext<'a> {
    /// Build canonical domain-separated signing material.
    pub fn signing_material(&self) -> Vec<u8> {
        let role_byte = match self.role {
            IdentityRole::Client => 0u8,
            IdentityRole::Server => 1u8,
        };
        let mut material = Vec::new();
        material.extend_from_slice(DOMAIN_LABEL);
        material.push(role_byte);
        material.extend_from_slice(&self.version.to_be_bytes());
        push_len_prefixed(&mut material, self.client_cid);
        push_len_prefixed(&mut material, self.server_cid);
        push_len_prefixed(&mut material, self.local_ephemeral);
        push_len_prefixed(&mut material, self.peer_ephemeral);
        push_len_prefixed(&mut material, self.transport_params);
        push_len_prefixed(&mut material, self.retry_token);
        material.extend_from_slice(&self.transcript_hash);
        material
    }
}

fn push_len_prefixed(out: &mut Vec<u8>, data: &[u8]) {
    let len = data.len().min(u32::MAX as usize) as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&data[..len as usize]);
}

/// Long-term Ed25519 identity key for endpoint authentication.
#[derive(Clone)]
pub struct IdentityKeyPair {
    signing_key: SigningKey,
}

impl IdentityKeyPair {
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        IdentityKeyPair {
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn from_seed(seed: [u8; 32]) -> Self {
        IdentityKeyPair {
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn public_bytes(&self) -> [u8; ED25519_PUBKEY_LEN] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub fn seed_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Sign an identity proof context; returns pubkey || signature.
    pub fn sign_context(&self, ctx: &IdentityProofContext<'_>) -> Vec<u8> {
        let material = ctx.signing_material();
        self.sign_material(&material)
    }

    /// Sign raw material; returns pubkey || signature.
    pub fn sign_material(&self, material: &[u8]) -> Vec<u8> {
        let sig = self.signing_key.sign(material);
        let mut proof = Vec::with_capacity(IDENTITY_PROOF_LEN);
        proof.extend_from_slice(&self.public_bytes());
        proof.extend_from_slice(&sig.to_bytes());
        proof
    }
}

/// Verify an identity proof over explicit signing material.
pub fn verify_identity_proof(
    proof: &[u8],
    material: &[u8],
    trusted_keys: &[[u8; ED25519_PUBKEY_LEN]],
    require_proof: bool,
) -> AtpResult<[u8; ED25519_PUBKEY_LEN]> {
    if proof.is_empty() {
        if require_proof {
            return Err(AtpError::security("missing endpoint identity proof"));
        }
        if !trusted_keys.is_empty() {
            return Err(AtpError::security(
                "identity proof required when trusted peer keys are configured",
            ));
        }
        return Ok([0u8; ED25519_PUBKEY_LEN]);
    }
    if proof.len() != IDENTITY_PROOF_LEN {
        return Err(AtpError::security("invalid identity proof length"));
    }

    let mut pubkey = [0u8; ED25519_PUBKEY_LEN];
    pubkey.copy_from_slice(&proof[..ED25519_PUBKEY_LEN]);
    let sig_bytes: [u8; ED25519_SIG_LEN] = proof[ED25519_PUBKEY_LEN..]
        .try_into()
        .map_err(|_| AtpError::security("invalid signature length"))?;

    let verifying_key = VerifyingKey::from_bytes(&pubkey)
        .map_err(|_| AtpError::security("invalid identity public key"))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    verifying_key
        .verify(material, &signature)
        .map_err(|_| AtpError::security("identity signature verification failed"))?;

    if !trusted_keys.is_empty() && !trusted_keys.contains(&pubkey) {
        return Err(AtpError::security("untrusted endpoint identity"));
    }

    Ok(pubkey)
}

/// Hash an empty transcript (used for first client INIT).
pub fn empty_transcript_hash() -> [u8; 32] {
    Sha256::digest([]).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::ATP_VERSION;

    #[test]
    fn test_domain_separation_differs_by_role() {
        let ctx_client = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: &[1; 8],
            server_cid: &[],
            local_ephemeral: &[2; 32],
            peer_ephemeral: &[],
            transport_params: b"tp",
            retry_token: &[],
            transcript_hash: empty_transcript_hash(),
        };
        let ctx_server = IdentityProofContext {
            role: IdentityRole::Server,
            ..ctx_client.clone()
        };
        assert_ne!(
            ctx_client.signing_material(),
            ctx_server.signing_material()
        );
    }

    #[test]
    fn test_identity_sign_verify_context() {
        let id = IdentityKeyPair::generate();
        let ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: &[1; 8],
            server_cid: &[],
            local_ephemeral: &[2; 32],
            peer_ephemeral: &[],
            transport_params: b"tp",
            retry_token: &[],
            transcript_hash: empty_transcript_hash(),
        };
        let proof = id.sign_context(&ctx);
        let material = ctx.signing_material();
        let pk = verify_identity_proof(&proof, &material, &[], false).unwrap();
        assert_eq!(pk, id.public_bytes());
    }

    #[test]
    fn test_trusted_key_required_when_configured() {
        let id = IdentityKeyPair::generate();
        let other = IdentityKeyPair::generate();
        let ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: &[1; 8],
            server_cid: &[],
            local_ephemeral: &[2; 32],
            peer_ephemeral: &[],
            transport_params: &[],
            retry_token: &[],
            transcript_hash: empty_transcript_hash(),
        };
        let proof = id.sign_context(&ctx);
        let material = ctx.signing_material();
        assert!(verify_identity_proof(&proof, &material, &[other.public_bytes()], false).is_err());
        assert!(verify_identity_proof(&proof, &material, &[id.public_bytes()], false).is_ok());
        assert!(verify_identity_proof(&[], &material, &[id.public_bytes()], false).is_err());
    }
}
