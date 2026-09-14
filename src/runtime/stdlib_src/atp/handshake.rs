//! ATP handshake state machine with authenticated transcript.
//!
//! Handshake flow:
//!   Client → CONNECTION_INIT (version, client CID, ephemeral pubkey, transport params)
//!   Server → CONNECTION_INIT_ACK (server CID, ephemeral pubkey, transport params)
//!   Both derive keys from shared secret + transcript
//!   Client → HANDSHAKE (Finished)
//!   Server → HANDSHAKE (Finished)
//!   Established

use super::config::{ATP_VERSION, MAX_HANDSHAKE_SIZE};
use super::errors::{AtpError, AtpResult};
use super::id::WireConnectionId;
use super::identity::{
    ED25519_PUBKEY_LEN, IdentityKeyPair, IdentityProofContext, IdentityRole, verify_identity_proof,
};
use super::security::{
    EphemeralKeyPair, FINISHED_LEN, HandshakeSecrets, derive_application_keys, derive_finished_mac,
    derive_handshake_keys,
};
use super::wire::Frame;
use sha2::{Digest, Sha256};
use std::time::Instant;

/// Handshake role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeRole {
    Client,
    Server,
}

/// Handshake state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeState {
    Initial,
    InitSent,
    InitReceived,
    InitAckSent,
    InitAckReceived,
    FinishedSent,
    FinishedReceived,
    Established,
    Failed,
}

/// Rolling transcript of handshake messages.
#[derive(Debug, Clone, Default)]
pub struct HandshakeTranscript {
    hasher: Sha256,
    bytes: Vec<u8>,
}

impl HandshakeTranscript {
    pub fn new() -> Self {
        HandshakeTranscript {
            hasher: Sha256::new(),
            bytes: Vec::new(),
        }
    }

    /// Append a handshake message to the transcript.
    pub fn append(&mut self, data: &[u8]) {
        if self.bytes.len() + data.len() > MAX_HANDSHAKE_SIZE {
            return; // Caller should check limits before append.
        }
        self.hasher.update(data);
        self.bytes.extend_from_slice(data);
    }

    /// Final transcript hash.
    pub fn hash(&self) -> [u8; 32] {
        let result = self.hasher.clone().finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Client-side handshake context.
pub struct ClientHandshake {
    pub state: HandshakeState,
    pub local_cid: WireConnectionId,
    pub remote_cid: Option<WireConnectionId>,
    pub ephemeral: EphemeralKeyPair,
    pub transcript: HandshakeTranscript,
    pub secrets: Option<HandshakeSecrets>,
    pub started_at: Instant,
}

impl ClientHandshake {
    pub fn new(
        ephemeral: EphemeralKeyPair,
        local_cid: WireConnectionId,
        started_at: Instant,
    ) -> Self {
        ClientHandshake {
            state: HandshakeState::Initial,
            local_cid,
            remote_cid: None,
            ephemeral,
            transcript: HandshakeTranscript::new(),
            secrets: None,
            started_at,
        }
    }

    /// Build CONNECTION_INIT frame and update transcript.
    pub fn build_init(
        &mut self,
        transport_params: &[u8],
        retry_token: &[u8],
        identity: Option<&IdentityKeyPair>,
    ) -> AtpResult<Frame> {
        let transcript_hash = self.transcript.hash();
        let ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: self.local_cid.as_bytes(),
            server_cid: &[],
            local_ephemeral: self.ephemeral.public_bytes(),
            peer_ephemeral: &[],
            transport_params,
            retry_token,
            transcript_hash,
        };
        let identity_proof = identity.map(|id| id.sign_context(&ctx)).unwrap_or_default();
        let frame = Frame::ConnectionInit {
            version: ATP_VERSION,
            src_conn_id: self.local_cid.as_bytes().to_vec(),
            ephemeral_pubkey: self.ephemeral.public_bytes().to_vec(),
            transport_params: transport_params.to_vec(),
            retry_token: retry_token.to_vec(),
            identity_proof,
        };
        let encoded = frame.encode();
        if encoded.len() > MAX_HANDSHAKE_SIZE {
            return Err(AtpError::protocol("handshake message too large"));
        }
        self.transcript.append(&encoded);
        self.state = HandshakeState::InitSent;
        Ok(frame)
    }

    /// Re-send INIT after Retry (reset transcript so server/client transcripts match).
    pub fn resend_init(
        &mut self,
        transport_params: &[u8],
        retry_token: &[u8],
        identity: Option<&IdentityKeyPair>,
    ) -> AtpResult<Frame> {
        self.transcript = HandshakeTranscript::new();
        self.secrets = None;
        self.remote_cid = None;
        self.state = HandshakeState::Initial;
        self.build_init(transport_params, retry_token, identity)
    }

    /// Process CONNECTION_INIT_ACK and derive keys.
    pub fn process_init_ack(
        &mut self,
        server_cid: &[u8],
        server_pubkey: &[u8],
        transport_params: &[u8],
        identity_proof: &[u8],
        frame_bytes: &[u8],
        trusted: &[[u8; ED25519_PUBKEY_LEN]],
        require_identity: bool,
    ) -> AtpResult<()> {
        if self.state != HandshakeState::InitSent {
            return Err(AtpError::protocol("unexpected INIT_ACK"));
        }
        let transcript_hash = self.transcript.hash();
        let ctx = IdentityProofContext {
            role: IdentityRole::Server,
            version: ATP_VERSION,
            client_cid: self.local_cid.as_bytes(),
            server_cid,
            local_ephemeral: server_pubkey,
            peer_ephemeral: self.ephemeral.public_bytes(),
            transport_params,
            retry_token: &[],
            transcript_hash,
        };
        verify_identity_proof(
            identity_proof,
            &ctx.signing_material(),
            trusted,
            require_identity,
        )?;

        self.transcript.append(frame_bytes);
        self.remote_cid = Some(WireConnectionId::from_slice(server_cid)?);

        let shared = super::security::dh_exchange(&self.ephemeral.secret, server_pubkey)?;
        let transcript_hash = self.transcript.hash();
        let secrets = derive_handshake_keys(&shared, &transcript_hash, true)?;
        self.secrets = Some(secrets);
        self.state = HandshakeState::InitAckReceived;
        Ok(())
    }

    /// Build client Finished frame.
    pub fn build_finished(&mut self) -> AtpResult<Frame> {
        let secrets = self
            .secrets
            .as_ref()
            .ok_or_else(|| AtpError::security("no handshake secrets"))?;
        let mac = derive_finished_mac(&secrets.finished_key, &self.transcript.hash())?;
        let frame = Frame::Handshake {
            confirm: mac.to_vec(),
        };
        let encoded = frame.encode();
        self.transcript.append(&encoded);
        self.state = HandshakeState::FinishedSent;
        Ok(frame)
    }

    /// Verify server Finished and transition to established.
    pub fn verify_server_finished(&mut self, confirm: &[u8], frame_bytes: &[u8]) -> AtpResult<()> {
        if self.state != HandshakeState::FinishedSent {
            return Err(AtpError::protocol("unexpected server Finished"));
        }
        let secrets = self
            .secrets
            .as_ref()
            .ok_or_else(|| AtpError::security("no handshake secrets"))?;
        let expected = derive_finished_mac(&secrets.finished_key, &self.transcript.hash())?;
        if confirm.len() != FINISHED_LEN || !constant_time_eq(confirm, &expected) {
            self.state = HandshakeState::Failed;
            return Err(AtpError::security("invalid server Finished"));
        }
        self.transcript.append(frame_bytes);
        self.state = HandshakeState::Established;
        Ok(())
    }

    /// Derive application keys after handshake completes.
    pub fn application_keys(&self) -> AtpResult<HandshakeSecrets> {
        let secrets = self
            .secrets
            .as_ref()
            .ok_or_else(|| AtpError::security("handshake incomplete"))?;
        derive_application_keys(secrets)
    }
}

/// Server-side handshake context.
pub struct ServerHandshake {
    pub state: HandshakeState,
    pub local_cid: WireConnectionId,
    pub remote_cid: Option<WireConnectionId>,
    pub ephemeral: Option<EphemeralKeyPair>,
    pub shared_secret: Option<[u8; 32]>,
    pub peer_ephemeral: Vec<u8>,
    pub transcript: HandshakeTranscript,
    pub secrets: Option<HandshakeSecrets>,
    pub started_at: Instant,
}

impl ServerHandshake {
    pub fn new(local_cid: WireConnectionId, started_at: Instant) -> Self {
        ServerHandshake {
            state: HandshakeState::Initial,
            local_cid,
            remote_cid: None,
            ephemeral: None,
            shared_secret: None,
            peer_ephemeral: Vec::new(),
            transcript: HandshakeTranscript::new(),
            secrets: None,
            started_at,
        }
    }

    /// Process CONNECTION_INIT from client.
    pub fn process_init(
        &mut self,
        version: u32,
        client_cid: &[u8],
        client_pubkey: &[u8],
        identity_proof: &[u8],
        frame_bytes: &[u8],
        trusted: &[[u8; ED25519_PUBKEY_LEN]],
        require_identity: bool,
    ) -> AtpResult<()> {
        if version != ATP_VERSION {
            return Err(AtpError::protocol(format!(
                "unsupported version {}",
                version
            )));
        }
        if self.state != HandshakeState::Initial {
            return Err(AtpError::protocol("duplicate CONNECTION_INIT"));
        }

        let (frame, _) = Frame::decode(frame_bytes, 0)?;
        let Frame::ConnectionInit {
            transport_params: tp,
            retry_token,
            ..
        } = frame
        else {
            return Err(AtpError::protocol("expected CONNECTION_INIT"));
        };
        let transcript_hash = self.transcript.hash();
        let ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version,
            client_cid,
            server_cid: &[],
            local_ephemeral: client_pubkey,
            peer_ephemeral: &[],
            transport_params: &tp,
            retry_token: &retry_token,
            transcript_hash,
        };
        verify_identity_proof(
            identity_proof,
            &ctx.signing_material(),
            trusted,
            require_identity,
        )?;

        self.transcript.append(frame_bytes);
        self.remote_cid = Some(WireConnectionId::from_slice(client_cid)?);
        self.peer_ephemeral = client_pubkey.to_vec();

        let (eph, shared) = super::security::server_handshake(client_pubkey)?;
        self.ephemeral = Some(eph);
        self.shared_secret = Some(shared);
        self.state = HandshakeState::InitReceived;
        Ok(())
    }

    /// Build CONNECTION_INIT_ACK.
    pub fn build_init_ack(
        &mut self,
        transport_params: &[u8],
        identity: Option<&IdentityKeyPair>,
    ) -> AtpResult<Frame> {
        let eph = self
            .ephemeral
            .as_ref()
            .ok_or_else(|| AtpError::security("no ephemeral key"))?;
        let remote_cid = self
            .remote_cid
            .ok_or_else(|| AtpError::protocol("no remote CID"))?;
        let shared = self
            .shared_secret
            .as_ref()
            .ok_or_else(|| AtpError::security("no shared secret"))?;

        let base = Frame::ConnectionInitAck {
            version: ATP_VERSION,
            src_conn_id: self.local_cid.as_bytes().to_vec(),
            dst_conn_id: remote_cid.as_bytes().to_vec(),
            ephemeral_pubkey: eph.public_bytes().to_vec(),
            transport_params: transport_params.to_vec(),
            identity_proof: Vec::new(),
        };
        let _ = base;
        let transcript_hash = self.transcript.hash();
        let ctx = IdentityProofContext {
            role: IdentityRole::Server,
            version: ATP_VERSION,
            client_cid: remote_cid.as_bytes(),
            server_cid: self.local_cid.as_bytes(),
            local_ephemeral: eph.public_bytes(),
            peer_ephemeral: &self.peer_ephemeral,
            transport_params,
            retry_token: &[],
            transcript_hash,
        };
        let identity_proof = identity.map(|id| id.sign_context(&ctx)).unwrap_or_default();
        let frame = Frame::ConnectionInitAck {
            version: ATP_VERSION,
            src_conn_id: self.local_cid.as_bytes().to_vec(),
            dst_conn_id: remote_cid.as_bytes().to_vec(),
            ephemeral_pubkey: eph.public_bytes().to_vec(),
            transport_params: transport_params.to_vec(),
            identity_proof,
        };
        let encoded = frame.encode();
        self.transcript.append(&encoded);

        let transcript_hash = self.transcript.hash();
        let secrets = derive_handshake_keys(shared, &transcript_hash, false)?;
        self.secrets = Some(secrets);
        self.state = HandshakeState::InitAckSent;
        Ok(frame)
    }

    /// Verify client Finished.
    pub fn verify_client_finished(&mut self, confirm: &[u8], frame_bytes: &[u8]) -> AtpResult<()> {
        if self.state != HandshakeState::InitAckSent {
            return Err(AtpError::protocol("unexpected client Finished"));
        }
        let secrets = self
            .secrets
            .as_ref()
            .ok_or_else(|| AtpError::security("no handshake secrets"))?;
        let expected = derive_finished_mac(&secrets.finished_key, &self.transcript.hash())?;
        if confirm.len() != FINISHED_LEN || !constant_time_eq(confirm, &expected) {
            self.state = HandshakeState::Failed;
            return Err(AtpError::security("invalid client Finished"));
        }
        self.transcript.append(frame_bytes);
        self.state = HandshakeState::FinishedReceived;
        Ok(())
    }

    /// Build server Finished response.
    pub fn build_finished(&mut self) -> AtpResult<Frame> {
        let secrets = self
            .secrets
            .as_ref()
            .ok_or_else(|| AtpError::security("no handshake secrets"))?;
        let mac = derive_finished_mac(&secrets.finished_key, &self.transcript.hash())?;
        let frame = Frame::Handshake {
            confirm: mac.to_vec(),
        };
        let encoded = frame.encode();
        self.transcript.append(&encoded);
        self.state = HandshakeState::Established;
        Ok(frame)
    }

    pub fn application_keys(&self) -> AtpResult<HandshakeSecrets> {
        let secrets = self
            .secrets
            .as_ref()
            .ok_or_else(|| AtpError::security("handshake incomplete"))?;
        derive_application_keys(secrets)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::super::security::EphemeralKeyPair;
    use super::*;

    #[test]
    fn test_transcript_hash_deterministic() {
        let mut t = HandshakeTranscript::new();
        t.append(b"msg1");
        t.append(b"msg2");
        let h1 = t.hash();
        let mut t2 = HandshakeTranscript::new();
        t2.append(b"msg1");
        t2.append(b"msg2");
        assert_eq!(h1, t2.hash());
    }

    #[test]
    fn test_full_handshake_roundtrip() {
        let now = Instant::now();
        let tp = b"transport-params";
        let client_eph = EphemeralKeyPair::generate();
        let client_cid = WireConnectionId::random();
        let server_cid = WireConnectionId::random();

        let mut client = ClientHandshake::new(client_eph, client_cid, now);
        let init = client.build_init(tp, &[], None).unwrap();
        let init_bytes = init.encode();

        let Frame::ConnectionInit {
            ephemeral_pubkey, ..
        } = init
        else {
            panic!("expected init");
        };

        let mut server = ServerHandshake::new(server_cid, now);
        server
            .process_init(
                ATP_VERSION,
                client_cid.as_bytes(),
                &ephemeral_pubkey,
                &[],
                &init_bytes,
                &[],
                false,
            )
            .unwrap();

        let ack = server.build_init_ack(tp, None).unwrap();
        let ack_bytes = ack.encode();
        let Frame::ConnectionInitAck {
            src_conn_id,
            ephemeral_pubkey: server_pubkey,
            transport_params,
            ..
        } = ack
        else {
            panic!("expected ack");
        };

        client
            .process_init_ack(
                &src_conn_id,
                &server_pubkey,
                &transport_params,
                &[],
                &ack_bytes,
                &[],
                false,
            )
            .unwrap();
        let client_finished = client.build_finished().unwrap();
        let client_finished_bytes = client_finished.encode();
        let Frame::Handshake {
            confirm: client_confirm,
        } = client_finished
        else {
            panic!("expected finished");
        };

        server
            .verify_client_finished(&client_confirm, &client_finished_bytes)
            .unwrap();
        let server_finished = server.build_finished().unwrap();
        let server_finished_bytes = server_finished.encode();
        let Frame::Handshake {
            confirm: server_confirm,
        } = server_finished
        else {
            panic!("expected finished");
        };

        client
            .verify_server_finished(&server_confirm, &server_finished_bytes)
            .unwrap();

        assert_eq!(client.state, HandshakeState::Established);
        assert_eq!(server.state, HandshakeState::Established);
        assert!(client.application_keys().is_ok());
        assert!(server.application_keys().is_ok());
    }

    fn complete_identity_handshake(
        client_id: &IdentityKeyPair,
        server_id: &IdentityKeyPair,
        retry_token: &[u8],
    ) -> (ClientHandshake, ServerHandshake) {
        let now = Instant::now();
        let tp = b"transport-params-identity";
        let client_eph = EphemeralKeyPair::generate();
        let client_cid = WireConnectionId::random();
        let server_cid = WireConnectionId::random();

        let mut client = ClientHandshake::new(client_eph, client_cid, now);
        let init = client.build_init(tp, retry_token, Some(client_id)).unwrap();
        let init_bytes = init.encode();
        let Frame::ConnectionInit {
            ephemeral_pubkey,
            identity_proof: client_proof,
            ..
        } = init
        else {
            panic!("expected init");
        };

        let mut server = ServerHandshake::new(server_cid, now);
        server
            .process_init(
                ATP_VERSION,
                client_cid.as_bytes(),
                &ephemeral_pubkey,
                &client_proof,
                &init_bytes,
                &[client_id.public_bytes()],
                true,
            )
            .unwrap();

        let ack = server.build_init_ack(tp, Some(server_id)).unwrap();
        let ack_bytes = ack.encode();
        let Frame::ConnectionInitAck {
            src_conn_id,
            ephemeral_pubkey: server_pubkey,
            transport_params,
            identity_proof: server_proof,
            ..
        } = ack
        else {
            panic!("expected ack");
        };

        client
            .process_init_ack(
                &src_conn_id,
                &server_pubkey,
                &transport_params,
                &server_proof,
                &ack_bytes,
                &[server_id.public_bytes()],
                true,
            )
            .unwrap();

        let client_finished = client.build_finished().unwrap();
        let client_finished_bytes = client_finished.encode();
        let Frame::Handshake {
            confirm: client_confirm,
        } = client_finished
        else {
            panic!("expected finished");
        };
        server
            .verify_client_finished(&client_confirm, &client_finished_bytes)
            .unwrap();

        let server_finished = server.build_finished().unwrap();
        let server_finished_bytes = server_finished.encode();
        let Frame::Handshake {
            confirm: server_confirm,
        } = server_finished
        else {
            panic!("expected finished");
        };
        client
            .verify_server_finished(&server_confirm, &server_finished_bytes)
            .unwrap();

        assert_eq!(client.state, HandshakeState::Established);
        assert_eq!(server.state, HandshakeState::Established);

        let client_keys = client.application_keys().unwrap();
        let server_keys = server.application_keys().unwrap();
        assert_eq!(
            client_keys.client_send_key, server_keys.client_send_key,
            "client send keys must match"
        );
        assert_eq!(
            client_keys.server_send_key, server_keys.server_send_key,
            "server send keys must match"
        );

        (client, server)
    }

    #[test]
    fn test_full_identity_handshake_roundtrip_with_retry() {
        let client_id = IdentityKeyPair::generate();
        let server_id = IdentityKeyPair::generate();
        let retry_token = b"retry-token-bytes";
        complete_identity_handshake(&client_id, &server_id, retry_token);
    }

    #[test]
    fn test_identity_handshake_rejects_untrusted_server_key() {
        let client_id = IdentityKeyPair::generate();
        let server_id = IdentityKeyPair::generate();
        let impostor = IdentityKeyPair::generate();
        let now = Instant::now();
        let tp = b"tp";
        let retry_token = b"retry";

        let mut client = ClientHandshake::new(
            EphemeralKeyPair::generate(),
            WireConnectionId::random(),
            now,
        );
        let init = client
            .build_init(tp, retry_token, Some(&client_id))
            .unwrap();
        let init_bytes = init.encode();
        let Frame::ConnectionInit {
            ephemeral_pubkey,
            identity_proof,
            ..
        } = init
        else {
            panic!();
        };

        let mut server = ServerHandshake::new(WireConnectionId::random(), now);
        server
            .process_init(
                ATP_VERSION,
                client.local_cid.as_bytes(),
                &ephemeral_pubkey,
                &identity_proof,
                &init_bytes,
                &[client_id.public_bytes()],
                true,
            )
            .unwrap();

        let ack = server.build_init_ack(tp, Some(&impostor)).unwrap();
        let ack_bytes = ack.encode();
        let Frame::ConnectionInitAck {
            src_conn_id,
            ephemeral_pubkey: server_pubkey,
            transport_params,
            identity_proof: server_proof,
            ..
        } = ack
        else {
            panic!();
        };

        assert!(
            client
                .process_init_ack(
                    &src_conn_id,
                    &server_pubkey,
                    &transport_params,
                    &server_proof,
                    &ack_bytes,
                    &[server_id.public_bytes()],
                    true,
                )
                .is_err()
        );
    }

    #[test]
    fn test_identity_tamper_transport_params_rejected() {
        let client_id = IdentityKeyPair::generate();
        let now = Instant::now();
        let tp = b"transport-params";
        let retry_token = b"retry";

        let mut client = ClientHandshake::new(
            EphemeralKeyPair::generate(),
            WireConnectionId::random(),
            now,
        );
        let init = client
            .build_init(tp, retry_token, Some(&client_id))
            .unwrap();
        let init_bytes = init.encode();
        let Frame::ConnectionInit {
            ephemeral_pubkey,
            identity_proof,
            ..
        } = init
        else {
            panic!();
        };

        let mut server = ServerHandshake::new(WireConnectionId::random(), now);
        let mut tampered_tp = tp.to_vec();
        tampered_tp.push(0xFF);
        let transcript_hash = server.transcript.hash();
        let bad_ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: client.local_cid.as_bytes(),
            server_cid: &[],
            local_ephemeral: &ephemeral_pubkey,
            peer_ephemeral: &[],
            transport_params: &tampered_tp,
            retry_token,
            transcript_hash,
        };
        assert!(
            verify_identity_proof(
                &identity_proof,
                &bad_ctx.signing_material(),
                &[client_id.public_bytes()],
                true
            )
            .is_err()
        );

        server
            .process_init(
                ATP_VERSION,
                client.local_cid.as_bytes(),
                &ephemeral_pubkey,
                &identity_proof,
                &init_bytes,
                &[client_id.public_bytes()],
                true,
            )
            .unwrap();
    }

    #[test]
    fn test_identity_tamper_client_cid_rejected() {
        let client_id = IdentityKeyPair::generate();
        let now = Instant::now();
        let tp = b"tp";
        let retry_token = &[];

        let client_cid = WireConnectionId::random();
        let mut client = ClientHandshake::new(EphemeralKeyPair::generate(), client_cid, now);
        let init = client
            .build_init(tp, retry_token, Some(&client_id))
            .unwrap();
        let _init_bytes = init.encode();
        let Frame::ConnectionInit {
            ephemeral_pubkey,
            identity_proof,
            ..
        } = init
        else {
            panic!();
        };

        let fake_cid = WireConnectionId::random();
        let bad_ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: fake_cid.as_bytes(),
            server_cid: &[],
            local_ephemeral: &ephemeral_pubkey,
            peer_ephemeral: &[],
            transport_params: tp,
            retry_token,
            transcript_hash: HandshakeTranscript::new().hash(),
        };
        assert!(
            verify_identity_proof(
                &identity_proof,
                &bad_ctx.signing_material(),
                &[client_id.public_bytes()],
                true
            )
            .is_err()
        );
    }

    #[test]
    fn test_identity_tamper_ephemeral_key_rejected() {
        let client_id = IdentityKeyPair::generate();
        let now = Instant::now();
        let tp = b"tp";
        let retry_token = &[];

        let mut client = ClientHandshake::new(
            EphemeralKeyPair::generate(),
            WireConnectionId::random(),
            now,
        );
        let init = client
            .build_init(tp, retry_token, Some(&client_id))
            .unwrap();
        let Frame::ConnectionInit {
            ephemeral_pubkey,
            identity_proof,
            ..
        } = init
        else {
            panic!();
        };

        let mut fake_eph = ephemeral_pubkey.clone();
        fake_eph[0] ^= 0xFF;
        let bad_ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: client.local_cid.as_bytes(),
            server_cid: &[],
            local_ephemeral: &fake_eph,
            peer_ephemeral: &[],
            transport_params: tp,
            retry_token,
            transcript_hash: HandshakeTranscript::new().hash(),
        };
        assert!(
            verify_identity_proof(
                &identity_proof,
                &bad_ctx.signing_material(),
                &[client_id.public_bytes()],
                true
            )
            .is_err()
        );
    }

    #[test]
    fn test_identity_tamper_retry_token_rejected() {
        let client_id = IdentityKeyPair::generate();
        let now = Instant::now();
        let tp = b"tp";
        let retry_token = b"valid-retry-token";

        let mut client = ClientHandshake::new(
            EphemeralKeyPair::generate(),
            WireConnectionId::random(),
            now,
        );
        let init = client
            .build_init(tp, retry_token, Some(&client_id))
            .unwrap();
        let Frame::ConnectionInit {
            ephemeral_pubkey,
            identity_proof,
            ..
        } = init
        else {
            panic!();
        };

        let bad_ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: client.local_cid.as_bytes(),
            server_cid: &[],
            local_ephemeral: &ephemeral_pubkey,
            peer_ephemeral: &[],
            transport_params: tp,
            retry_token: b"tampered-retry-token",
            transcript_hash: HandshakeTranscript::new().hash(),
        };
        assert!(
            verify_identity_proof(
                &identity_proof,
                &bad_ctx.signing_material(),
                &[client_id.public_bytes()],
                true
            )
            .is_err()
        );
    }
}
