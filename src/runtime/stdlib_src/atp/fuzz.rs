//! Fuzz/property tests for ATP wire format and handshake parsing.

#[cfg(test)]
mod targets {
    use super::super::config::ATP_VERSION;
    use super::super::handshake::{ClientHandshake, ServerHandshake};
    use super::super::identity::{
        empty_transcript_hash, verify_identity_proof, IdentityKeyPair, IdentityProofContext,
        IdentityRole,
    };
    use super::super::id::WireConnectionId;
    use super::super::security::EphemeralKeyPair;
    use super::super::wire::{Frame, PacketHeader};
    use std::time::Instant;

    #[test]
    fn fuzz_packet_header_all_byte_seeds() {
        for seed in 0u8..=255u8 {
            let len = (seed as usize % 128) + 1;
            let data = vec![seed; len];
            let _ = PacketHeader::decode(&data);
        }
    }

    #[test]
    fn fuzz_frame_decode_all_byte_seeds() {
        for seed in 0u8..=255u8 {
            let len = (seed as usize % 256) + 1;
            let data = vec![seed.wrapping_add(1); len];
            let _ = Frame::decode(&data, 0);
            let _ = Frame::decode_all(&data);
        }
    }

    #[test]
    fn fuzz_frame_type_byte_sweep() {
        for ft in 0u8..=255u8 {
            for extra in 0usize..32 {
                let mut data = vec![ft];
                data.extend(std::iter::repeat(0xAB).take(extra));
                let _ = Frame::decode_all(&data);
            }
        }
    }

    #[test]
    fn fuzz_identity_proof_corruption() {
        let id = IdentityKeyPair::generate();
        let ctx = IdentityProofContext {
            role: IdentityRole::Client,
            version: ATP_VERSION,
            client_cid: &[0; 8],
            server_cid: &[],
            local_ephemeral: &[0; 32],
            peer_ephemeral: &[],
            transport_params: &[],
            retry_token: &[],
            transcript_hash: empty_transcript_hash(),
        };
        let msg = ctx.signing_material();
        let proof = id.sign_context(&ctx);
        for i in 0..proof.len() {
            let mut corrupted = proof.clone();
            corrupted[i] ^= 0xFF;
            assert!(verify_identity_proof(&corrupted, &msg, &[], false).is_err());
        }
        for len in 0..96 {
            let _ = verify_identity_proof(&proof[..len], &msg, &[], false);
        }
    }

    #[test]
    fn fuzz_server_process_init_garbage() {
        let now = Instant::now();
        let mut server = ServerHandshake::new(WireConnectionId::random(), now);
        for seed in 0u8..=255u8 {
            let garbage = vec![seed; (seed as usize % 64) + 1];
            let _ = server.process_init(
                ATP_VERSION,
                &[seed; 8],
                &[seed; 32],
                &[],
                &garbage,
                &[],
                false,
            );
        }
    }

    #[test]
    fn fuzz_handshake_identity_roundtrip_seeds() {
        for seed in 0u16..64 {
            let now = Instant::now();
            let client_id = IdentityKeyPair::from_seed([(seed >> 8) as u8; 32]);
            let server_id = IdentityKeyPair::from_seed([seed as u8; 32]);
            let client_cid = WireConnectionId::random();
            let server_cid = WireConnectionId::random();
            let tp = format!("tp-{}", seed).into_bytes();

            let mut client = ClientHandshake::new(EphemeralKeyPair::generate(), client_cid, now);
            let init = client.build_init(&tp, &[], Some(&client_id)).unwrap();
            let init_bytes = init.encode();
            let Frame::ConnectionInit {
                ephemeral_pubkey,
                identity_proof,
                ..
            } = init
            else {
                continue;
            };

            let mut server = ServerHandshake::new(server_cid, now);
            if server
                .process_init(
                    ATP_VERSION,
                    client_cid.as_bytes(),
                    &ephemeral_pubkey,
                    &identity_proof,
                    &init_bytes,
                    &[client_id.public_bytes()],
                    true,
                )
                .is_err()
            {
                continue;
            }
            let ack = server.build_init_ack(&tp, Some(&server_id)).unwrap();
            let ack_bytes = ack.encode();
            let Frame::ConnectionInitAck {
                src_conn_id,
                ephemeral_pubkey: spk,
                transport_params,
                identity_proof: server_proof,
                ..
            } = ack
            else {
                continue;
            };

            let _ = client.process_init_ack(
                &src_conn_id,
                &spk,
                &transport_params,
                &server_proof,
                &ack_bytes,
                &[server_id.public_bytes()],
                true,
            );
        }
    }

    #[test]
    fn fuzz_malformed_cid_lengths() {
        for cid_len in 0u8..=255u8 {
            let mut data = vec![0x01]; // ConnectionInit
            data.extend_from_slice(&ATP_VERSION.to_be_bytes());
            data.push(cid_len);
            data.extend(vec![0xAA; cid_len as usize]);
            let _ = Frame::decode_all(&data);
        }
    }

    #[test]
    fn fuzz_truncated_ack_ranges() {
        let mut data = vec![0x05]; // Ack frame type
        data.extend_from_slice(&[0x01, 0x02, 0x03]); // truncated
        let _ = Frame::decode_all(&data);
    }
}
