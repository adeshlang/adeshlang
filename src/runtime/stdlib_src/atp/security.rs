//! ATP Security Engine
//!
//! Provides:
//! - X25519 ephemeral key agreement (encryption, NOT endpoint identity)
//! - HKDF-SHA256 key derivation with explicit role-separated labels
//! - ChaCha20-Poly1305 AEAD with header bytes as AAD
//! - Per-packet-number-space replay protection
//! - Key rotation with epoch overlap
//!
//! Trust model: ATP provides encrypted transport with forward secrecy via
//! ephemeral keys. It does NOT authenticate endpoint identity unless
//! application-layer credentials are added.

use super::config::{AEAD_TAG_LEN, REPLAY_WINDOW_SIZE};
use super::errors::{AtpError, AtpResult};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::Zeroize;

/// Length of an X25519 key in bytes.
pub const X25519_KEY_LEN: usize = 32;
/// Length of a ChaCha20-Poly1305 key in bytes.
pub const AEAD_KEY_LEN: usize = 32;
/// Length of a ChaCha20-Poly1305 nonce in bytes.
pub const AEAD_NONCE_LEN: usize = 12;
/// Finished MAC length.
pub const FINISHED_LEN: usize = 32;

/// Packet number space — handshake and application are independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketNumberSpace {
    Handshake,
    Application,
}

/// A pair of X25519 keys for key agreement.
pub struct EphemeralKeyPair {
    pub secret: [u8; X25519_KEY_LEN],
    pub public: [u8; X25519_KEY_LEN],
}

impl EphemeralKeyPair {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = X25519PublicKey::from(&secret);
        let mut s = [0u8; X25519_KEY_LEN];
        s.copy_from_slice(secret.as_bytes());
        let mut p = [0u8; X25519_KEY_LEN];
        p.copy_from_slice(public.as_bytes());
        EphemeralKeyPair {
            secret: s,
            public: p,
        }
    }

    pub fn public_bytes(&self) -> &[u8] {
        &self.public
    }
}

impl Drop for EphemeralKeyPair {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

/// Perform X25519 Diffie-Hellman and return the raw shared secret.
pub fn dh_exchange(my_secret: &[u8], peer_public: &[u8]) -> AtpResult<[u8; 32]> {
    if my_secret.len() != X25519_KEY_LEN {
        return Err(AtpError::security("DH: secret key must be 32 bytes"));
    }
    if peer_public.len() != X25519_KEY_LEN {
        return Err(AtpError::security("DH: public key must be 32 bytes"));
    }
    let mut secret_arr = [0u8; 32];
    secret_arr.copy_from_slice(my_secret);
    let secret = StaticSecret::from(secret_arr);
    secret_arr.zeroize();

    let mut pub_arr = [0u8; 32];
    pub_arr.copy_from_slice(peer_public);
    let peer_pub = X25519PublicKey::from(pub_arr);

    let shared = secret.diffie_hellman(&peer_pub);
    let mut out = [0u8; 32];
    out.copy_from_slice(shared.as_bytes());
    Ok(out)
}

/// Directional AEAD keys.
#[derive(Clone)]
pub struct DirectionalKeys {
    pub key: [u8; AEAD_KEY_LEN],
    pub static_iv: [u8; 4],
}

impl DirectionalKeys {
    pub fn new(key: [u8; AEAD_KEY_LEN]) -> Self {
        DirectionalKeys {
            key,
            static_iv: derive_iv(&key),
        }
    }

    pub fn nonce_for(&self, pn: u64) -> [u8; AEAD_NONCE_LEN] {
        build_nonce(&self.static_iv, pn)
    }
}

/// Handshake-derived secrets (before application key derivation).
#[derive(Clone)]
pub struct HandshakeSecrets {
    pub client_send_key: [u8; AEAD_KEY_LEN],
    pub server_send_key: [u8; AEAD_KEY_LEN],
    pub finished_key: [u8; 32],
    pub handshake_secret: [u8; 32],
    pub is_client: bool,
}

/// Derive handshake keys from shared secret and transcript hash.
pub fn derive_handshake_keys(
    shared_secret: &[u8],
    transcript_hash: &[u8; 32],
    is_client: bool,
) -> AtpResult<HandshakeSecrets> {
    let hk = Hkdf::<Sha256>::new(Some(transcript_hash), shared_secret);
    let mut handshake_secret = [0u8; 32];
    hk.expand(b"atp handshake", &mut handshake_secret)
        .map_err(|_| AtpError::security("HKDF handshake secret failed"))?;

    let mut client_send_key = [0u8; AEAD_KEY_LEN];
    let mut server_send_key = [0u8; AEAD_KEY_LEN];
    let mut finished_key = [0u8; 32];

    let hk2 = Hkdf::<Sha256>::new(None, &handshake_secret);
    hk2.expand(b"atp c hs traffic", &mut client_send_key)
        .map_err(|_| AtpError::security("HKDF client hs key failed"))?;
    hk2.expand(b"atp s hs traffic", &mut server_send_key)
        .map_err(|_| AtpError::security("HKDF server hs key failed"))?;
    hk2.expand(b"atp finished", &mut finished_key)
        .map_err(|_| AtpError::security("HKDF finished key failed"))?;

    Ok(HandshakeSecrets {
        client_send_key,
        server_send_key,
        finished_key,
        handshake_secret,
        is_client,
    })
}

/// Derive application keys from handshake secrets.
pub fn derive_application_keys(hs: &HandshakeSecrets) -> AtpResult<HandshakeSecrets> {
    let hk = Hkdf::<Sha256>::new(None, &hs.handshake_secret);
    let mut app_secret = [0u8; 32];
    hk.expand(b"atp application", &mut app_secret)
        .map_err(|_| AtpError::security("HKDF application secret failed"))?;

    let mut client_send_key = [0u8; AEAD_KEY_LEN];
    let mut server_send_key = [0u8; AEAD_KEY_LEN];

    let hk2 = Hkdf::<Sha256>::new(None, &app_secret);
    hk2.expand(b"atp c ap traffic", &mut client_send_key)
        .map_err(|_| AtpError::security("HKDF client ap key failed"))?;
    hk2.expand(b"atp s ap traffic", &mut server_send_key)
        .map_err(|_| AtpError::security("HKDF server ap key failed"))?;

    Ok(HandshakeSecrets {
        client_send_key,
        server_send_key,
        finished_key: hs.finished_key,
        handshake_secret: app_secret,
        is_client: hs.is_client,
    })
}

/// Compute Finished MAC over transcript hash.
pub fn derive_finished_mac(finished_key: &[u8; 32], transcript_hash: &[u8; 32]) -> AtpResult<[u8; FINISHED_LEN]> {
    use hmac::digest::KeyInit;
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(finished_key)
        .map_err(|_| AtpError::security("HMAC init failed"))?;
    mac.update(transcript_hash);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; FINISHED_LEN];
    out.copy_from_slice(&result);
    Ok(out)
}

/// Build a 12-byte nonce: 4-byte static IV XOR'd with 8-byte big-endian packet number.
pub fn build_nonce(static_iv: &[u8; 4], packet_number: u64) -> [u8; AEAD_NONCE_LEN] {
    let mut nonce = [0u8; AEAD_NONCE_LEN];
    nonce[..4].copy_from_slice(static_iv);
    let pn_bytes = packet_number.to_be_bytes();
    nonce[4..].copy_from_slice(&pn_bytes);
    nonce
}

/// Encrypt plaintext with ChaCha20-Poly1305. AAD is the exact cleartext header bytes.
pub fn encrypt_packet(
    key: &[u8; AEAD_KEY_LEN],
    nonce: &[u8; AEAD_NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> AtpResult<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| AtpError::security("invalid AEAD key"))?;
    let nonce_obj = Nonce::from_slice(nonce);
    cipher
        .encrypt(
            nonce_obj,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AtpError::security("AEAD encryption failed"))
}

/// Decrypt and authenticate. Ciphertext includes the 16-byte auth tag.
pub fn decrypt_packet(
    key: &[u8; AEAD_KEY_LEN],
    nonce: &[u8; AEAD_NONCE_LEN],
    ciphertext: &[u8],
    aad: &[u8],
) -> AtpResult<Vec<u8>> {
    if ciphertext.len() < AEAD_TAG_LEN {
        return Err(AtpError::security("ciphertext shorter than auth tag"));
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| AtpError::security("invalid AEAD key"))?;
    let nonce_obj = Nonce::from_slice(nonce);
    cipher
        .decrypt(
            nonce_obj,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AtpError::security("AEAD authentication/decryption failed"))
}

/// Sliding-window replay protection for one direction/epoch/space.
#[derive(Debug, Clone)]
pub struct ReplayWindow {
    largest_pn: u64,
    window: u64,
    initialized: bool,
}

impl ReplayWindow {
    pub fn new() -> Self {
        ReplayWindow {
            largest_pn: 0,
            window: 0,
            initialized: false,
        }
    }

    /// Check and mark a packet number. Returns Err on replay.
    pub fn check_and_mark(&mut self, pn: u64) -> AtpResult<()> {
        if !self.initialized {
            self.largest_pn = pn;
            self.window = 1;
            self.initialized = true;
            return Ok(());
        }

        if pn > self.largest_pn {
            let shift = pn - self.largest_pn;
            if shift >= REPLAY_WINDOW_SIZE {
                self.window = 1;
            } else {
                self.window <<= shift;
                self.window |= 1;
            }
            self.largest_pn = pn;
            Ok(())
        } else {
            let diff = self.largest_pn - pn;
            if diff >= REPLAY_WINDOW_SIZE {
                return Err(AtpError::security("packet below replay window"));
            }
            let bit = 1u64 << diff;
            if self.window & bit != 0 {
                return Err(AtpError::security("duplicate packet (replay)"));
            }
            self.window |= bit;
            Ok(())
        }
    }

    pub fn reset(&mut self) {
        self.largest_pn = 0;
        self.window = 0;
        self.initialized = false;
    }
}

impl Default for ReplayWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Security state for one packet number space.
pub struct PnSpaceSecurity {
    pub send: DirectionalKeys,
    pub recv: DirectionalKeys,
    pub replay: ReplayWindow,
    pub next_pn: u64,
}

impl PnSpaceSecurity {
    pub fn new(send_key: [u8; AEAD_KEY_LEN], recv_key: [u8; AEAD_KEY_LEN]) -> Self {
        PnSpaceSecurity {
            send: DirectionalKeys::new(send_key),
            recv: DirectionalKeys::new(recv_key),
            replay: ReplayWindow::new(),
            next_pn: 0,
        }
    }

    pub fn allocate_pn(&mut self) -> u64 {
        let pn = self.next_pn;
        self.next_pn += 1;
        pn
    }
}

/// Full security context with handshake and application spaces.
pub struct SecurityContext {
    pub is_client: bool,
    pub epoch: u64,
    pub handshake: PnSpaceSecurity,
    pub application: PnSpaceSecurity,
    /// Previous epoch application keys during key update overlap.
    pub prev_application: Option<PnSpaceSecurity>,
}

impl SecurityContext {
    /// Create from derived handshake secrets (still in handshake phase).
    pub fn from_handshake_secrets(secrets: &HandshakeSecrets) -> Self {
        let (client_send, server_send) = (secrets.client_send_key, secrets.server_send_key);
        let (send_hs, recv_hs) = if secrets.is_client {
            (client_send, server_send)
        } else {
            (server_send, client_send)
        };
        SecurityContext {
            is_client: secrets.is_client,
            epoch: 0,
            handshake: PnSpaceSecurity::new(send_hs, recv_hs),
            application: PnSpaceSecurity::new([0; 32], [0; 32]), // Set on establishment.
            prev_application: None,
        }
    }

    /// Activate application keys after handshake completes.
    pub fn activate_application(&mut self, secrets: &HandshakeSecrets) {
        let (client_send, server_send) = (secrets.client_send_key, secrets.server_send_key);
        let (send, recv) = if secrets.is_client {
            (client_send, server_send)
        } else {
            (server_send, client_send)
        };
        self.application = PnSpaceSecurity::new(send, recv);
    }

    fn space_mut(&mut self, space: PacketNumberSpace) -> &mut PnSpaceSecurity {
        match space {
            PacketNumberSpace::Handshake => &mut self.handshake,
            PacketNumberSpace::Application => &mut self.application,
        }
    }

    /// Encrypt in the given packet number space.
    pub fn encrypt(
        &mut self,
        space: PacketNumberSpace,
        plaintext: &[u8],
        header_aad: &[u8],
    ) -> AtpResult<(u64, Vec<u8>)> {
        let pn = self.space_mut(space).allocate_pn();
        let nonce = self.space_mut(space).send.nonce_for(pn);
        let key = self.space_mut(space).send.key;
        let ct = encrypt_packet(&key, &nonce, plaintext, header_aad)?;
        Ok((pn, ct))
    }

    /// Decrypt in the given packet number space. Tries current and previous epoch.
    pub fn decrypt(
        &mut self,
        space: PacketNumberSpace,
        pn: u64,
        ciphertext: &[u8],
        header_aad: &[u8],
    ) -> AtpResult<Vec<u8>> {
        // Try current epoch.
        if let Ok(pt) = self.try_decrypt_space(space, pn, ciphertext, header_aad, false) {
            return Ok(pt);
        }
        // Try previous epoch for application space only.
        if space == PacketNumberSpace::Application {
            if self.prev_application.is_some() {
                if let Ok(pt) = self.try_decrypt_space(space, pn, ciphertext, header_aad, true) {
                    return Ok(pt);
                }
            }
        }
        Err(AtpError::security("decryption failed"))
    }

    fn try_decrypt_space(
        &mut self,
        space: PacketNumberSpace,
        pn: u64,
        ciphertext: &[u8],
        header_aad: &[u8],
        use_prev: bool,
    ) -> AtpResult<Vec<u8>> {
        if use_prev {
            let prev = self
                .prev_application
                .as_mut()
                .ok_or_else(|| AtpError::security("no prev keys"))?;
            let nonce = prev.recv.nonce_for(pn);
            let key = prev.recv.key;
            let pt = decrypt_packet(&key, &nonce, ciphertext, header_aad)?;
            prev.replay.check_and_mark(pn)?;
            return Ok(pt);
        }

        let space_sec = self.space_mut(space);
        let nonce = space_sec.recv.nonce_for(pn);
        let key = space_sec.recv.key;
        let pt = decrypt_packet(&key, &nonce, ciphertext, header_aad)?;
        space_sec.replay.check_and_mark(pn)?;
        Ok(pt)
    }

    /// Rotate application keys to a new epoch.
    pub fn rotate_keys(&mut self) {
        let old = std::mem::replace(
            &mut self.application,
            PnSpaceSecurity::new([0; 32], [0; 32]),
        );
        // Derive next keys from current send/recv keys.
        let new_send = derive_next_key(&old.send.key);
        let new_recv = derive_next_key(&old.recv.key);
        self.prev_application = Some(old);
        self.application = PnSpaceSecurity::new(new_send, new_recv);
        self.epoch += 1;
    }

    pub fn drop_prev_keys(&mut self) {
        self.prev_application = None;
    }
}

fn derive_next_key(prev: &[u8; AEAD_KEY_LEN]) -> [u8; AEAD_KEY_LEN] {
    let hk = Hkdf::<Sha256>::new(Some(prev), b"atp key update");
    let mut new_key = [0u8; AEAD_KEY_LEN];
    let _ = hk.expand(b"next", &mut new_key);
    new_key
}

fn derive_iv(key: &[u8; AEAD_KEY_LEN]) -> [u8; 4] {
    let hash = Sha256::digest(key);
    let mut iv = [0u8; 4];
    iv.copy_from_slice(&hash[..4]);
    iv
}

/// Server handshake helper.
pub fn server_handshake(client_pubkey: &[u8]) -> AtpResult<(EphemeralKeyPair, [u8; 32])> {
    let our_keys = EphemeralKeyPair::generate();
    let shared = dh_exchange(&our_keys.secret, client_pubkey)?;
    Ok((our_keys, shared))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dh_symmetry() {
        let alice = EphemeralKeyPair::generate();
        let bob = EphemeralKeyPair::generate();
        let shared_a = dh_exchange(&alice.secret, &bob.public).unwrap();
        let shared_b = dh_exchange(&bob.secret, &alice.public).unwrap();
        assert_eq!(shared_a, shared_b);
    }

    #[test]
    fn test_key_schedule_symmetry() {
        let shared = [0x42u8; 32];
        let transcript = [0xAAu8; 32];
        let client_hs = derive_handshake_keys(&shared, &transcript, true).unwrap();
        let server_hs = derive_handshake_keys(&shared, &transcript, false).unwrap();
        assert_eq!(client_hs.client_send_key, server_hs.client_send_key);
        assert_eq!(client_hs.server_send_key, server_hs.server_send_key);

        let client_app = derive_application_keys(&client_hs).unwrap();
        let server_app = derive_application_keys(&server_hs).unwrap();
        assert_eq!(client_app.client_send_key, server_app.client_send_key);
        assert_eq!(client_app.server_send_key, server_app.server_send_key);
    }

    #[test]
    fn test_aead_roundtrip_and_aad() {
        let key = [0x42; 32];
        let nonce = build_nonce(&[0u8; 4], 1);
        let aad = b"exact-header-bytes";
        let plaintext = b"hello secure transport";

        let ct = encrypt_packet(&key, &nonce, plaintext, aad).unwrap();
        let pt = decrypt_packet(&key, &nonce, &ct, aad).unwrap();
        assert_eq!(pt, plaintext);

        // AAD mismatch must fail.
        assert!(decrypt_packet(&key, &nonce, &ct, b"wrong-aad").is_err());

        // Tampered ciphertext must fail.
        let mut tampered = ct.clone();
        tampered[0] ^= 0xFF;
        assert!(decrypt_packet(&key, &nonce, &tampered, aad).is_err());

        // Tampered header AAD must fail.
        assert!(decrypt_packet(&key, &nonce, &ct, b"exact-header-byteX").is_err());
    }

    #[test]
    fn test_replay_protection() {
        let mut replay = ReplayWindow::new();
        replay.check_and_mark(5).unwrap();
        assert!(replay.check_and_mark(5).is_err());
        replay.check_and_mark(6).unwrap();
        replay.check_and_mark(4).unwrap(); // within window
        assert!(replay.check_and_mark(4).is_err()); // duplicate
    }

    #[test]
    fn test_nonce_uniqueness() {
        let iv = [0x01, 0x02, 0x03, 0x04];
        let n1 = build_nonce(&iv, 1);
        let n2 = build_nonce(&iv, 2);
        assert_ne!(n1, n2);
    }

    #[test]
    fn test_finished_mac() {
        let key = [0xCC; 32];
        let hash = [0xDD; 32];
        let mac1 = derive_finished_mac(&key, &hash).unwrap();
        let mac2 = derive_finished_mac(&key, &hash).unwrap();
        assert_eq!(mac1, mac2);
        let mut hash2 = hash;
        hash2[0] ^= 1;
        let mac3 = derive_finished_mac(&key, &hash2).unwrap();
        assert_ne!(mac1, mac3);
    }

    #[test]
    fn test_directional_keys_client_server() {
        let shared = [0x55u8; 32];
        let transcript = Sha256::digest(b"transcript").into();
        let client_hs = derive_handshake_keys(&shared, &transcript, true).unwrap();
        let server_hs = derive_handshake_keys(&shared, &transcript, false).unwrap();

        // Client send = server recv for application traffic.
        let client_app = derive_application_keys(&client_hs).unwrap();
        let server_app = derive_application_keys(&server_hs).unwrap();
        assert_eq!(client_app.client_send_key, server_app.client_send_key);
        assert_eq!(client_app.server_send_key, server_app.server_send_key);
    }
}
