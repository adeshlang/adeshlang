//! ATP integration and property tests.

#[cfg(test)]
mod integration {
    use super::super::engine::{AtpEngine, Delivery, SendRequest};
    use super::super::stream::Priority;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::thread;
    use std::time::Duration;

    fn localhost_port() -> u16 {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    }

    #[test]
    fn test_client_server_handshake_with_retry() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let server = AtpEngine::new_server(listen).unwrap();
        server.start();

        let client = AtpEngine::new_client(listen).unwrap();
        let client_conn = client.ensure_client_handshake().unwrap();
        client.start();

        let mut connected = false;
        for _ in 0..150 {
            thread::sleep(Duration::from_millis(20));
            for d in server.poll_deliveries() {
                if let Delivery::Connected { .. } = d {
                    connected = true;
                }
            }
            for d in client.poll_deliveries() {
                if let Delivery::Connected { connection_id } = d {
                    assert_eq!(connection_id, client_conn);
                    connected = true;
                }
            }
            if connected {
                break;
            }
        }

        server.stop();
        client.stop();
        assert!(connected, "handshake with retry token should complete");
    }

    #[test]
    fn test_message_delivery_after_handshake() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let server = AtpEngine::new_server(listen).unwrap();
        server.start();

        let client = AtpEngine::new_client(listen).unwrap();
        let client_conn = client.ensure_client_handshake().unwrap();
        client.start();

        let mut established = false;
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(20));
            for d in client.poll_deliveries() {
                if let Delivery::Connected { .. } = d {
                    established = true;
                }
            }
            if established {
                break;
            }
        }
        assert!(established, "connection should establish");

        client.submit_request(SendRequest::Message {
            connection_id: client_conn,
            stream_id: 1,
            data: b"hello-atp".to_vec(),
            reliable: true,
            ordered: true,
        });

        let mut received = false;
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(20));
            for d in server.poll_deliveries() {
                if let Delivery::Message { data, .. } = d {
                    assert_eq!(data, b"hello-atp");
                    received = true;
                }
            }
            if received {
                break;
            }
        }

        server.stop();
        client.stop();
        assert!(received, "server should receive client message");
    }

    #[test]
    fn test_multiple_connections_isolated() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let server = AtpEngine::new_server(listen).unwrap();
        server.start();

        let c1 = AtpEngine::new_client(listen).unwrap();
        let _ = c1.ensure_client_handshake().unwrap();
        c1.start();
        let c2 = AtpEngine::new_client(listen).unwrap();
        let _ = c2.ensure_client_handshake().unwrap();
        c2.start();

        thread::sleep(Duration::from_millis(500));

        let id1 = c1.connection_id_for_client().unwrap();
        let id2 = c2.connection_id_for_client().unwrap();
        assert_ne!(id1, id2);

        c1.submit_request(SendRequest::OpenStream {
            connection_id: id1,
            priority: Priority::Medium,
        });
        c2.submit_request(SendRequest::Message {
            connection_id: id2,
            stream_id: 1,
            data: b"only-c2".to_vec(),
            reliable: true,
            ordered: true,
        });

        thread::sleep(Duration::from_millis(200));
        server.stop();
        c1.stop();
        c2.stop();
    }
}

#[cfg(test)]
mod identity_integration {
    use super::super::config::AtpConfig;
    use super::super::engine::{AtpEngine, Delivery};
    use super::super::identity::IdentityKeyPair;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    fn localhost_port() -> u16 {
        static NEXT_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(21100);
        NEXT_PORT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    #[test]
    fn test_handshake_with_identity_proofs() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let server_id = Arc::new(IdentityKeyPair::generate());
        let client_id = Arc::new(IdentityKeyPair::generate());

        let mut server_cfg = AtpConfig::default();
        server_cfg.local_identity = Some(server_id.clone());
        server_cfg.trusted_peer_keys = vec![client_id.public_bytes()];
        server_cfg.require_peer_identity = true;

        let mut client_cfg = AtpConfig::default();
        client_cfg.local_identity = Some(client_id.clone());
        client_cfg.trusted_peer_keys = vec![server_id.public_bytes()];
        client_cfg.require_peer_identity = true;

        let server = AtpEngine::new_server_with_config(listen, server_cfg).unwrap();
        server.start();

        let client = AtpEngine::new_client_with_config(listen, client_cfg).unwrap();
        let client_conn = client.ensure_client_handshake().unwrap();
        client.start();

        let mut connected = false;
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(20));
            for d in client.poll_deliveries() {
                if let Delivery::Connected { connection_id } = d {
                    assert_eq!(connection_id, client_conn);
                    connected = true;
                }
            }
            if connected {
                break;
            }
        }
        server.stop();
        client.stop();
        assert!(
            connected,
            "identity-authenticated handshake should complete"
        );
    }

    #[test]
    fn test_identity_e2e_with_retry_and_message_delivery() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let server_id = Arc::new(IdentityKeyPair::generate());
        let client_id = Arc::new(IdentityKeyPair::generate());

        let mut server_cfg = AtpConfig::default();
        server_cfg.local_identity = Some(server_id.clone());
        server_cfg.trusted_peer_keys = vec![client_id.public_bytes()];
        server_cfg.require_peer_identity = true;

        let mut client_cfg = AtpConfig::default();
        client_cfg.local_identity = Some(client_id.clone());
        client_cfg.trusted_peer_keys = vec![server_id.public_bytes()];
        client_cfg.require_peer_identity = true;

        let server = AtpEngine::new_server_with_config(listen, server_cfg).unwrap();
        server.start();

        let client = AtpEngine::new_client_with_config(listen, client_cfg).unwrap();
        let client_conn = client.ensure_client_handshake().unwrap();
        client.start();

        let mut connected = false;
        for _ in 0..150 {
            thread::sleep(Duration::from_millis(20));
            for d in client.poll_deliveries() {
                if let Delivery::Connected { connection_id } = d {
                    assert_eq!(connection_id, client_conn);
                    connected = true;
                }
            }
            if connected {
                break;
            }
        }
        assert!(connected, "identity handshake with retry should complete");

        client.submit_request(super::super::engine::SendRequest::Message {
            connection_id: client_conn,
            stream_id: 1,
            data: b"identity-e2e".to_vec(),
            reliable: true,
            ordered: true,
        });

        let mut received = false;
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(20));
            for d in server.poll_deliveries() {
                if let Delivery::Message { data, .. } = d {
                    if data == b"identity-e2e" {
                        received = true;
                    }
                }
            }
            if received {
                break;
            }
        }

        server.stop();
        client.stop();
        assert!(
            received,
            "application data should flow after identity handshake"
        );
    }

    #[test]
    fn test_identity_e2e_rejects_wrong_server_key() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let server_id = Arc::new(IdentityKeyPair::generate());
        let client_id = Arc::new(IdentityKeyPair::generate());
        let wrong_server = Arc::new(IdentityKeyPair::generate());

        let mut server_cfg = AtpConfig::default();
        server_cfg.local_identity = Some(server_id.clone());
        server_cfg.trusted_peer_keys = vec![client_id.public_bytes()];
        server_cfg.require_peer_identity = true;

        let mut client_cfg = AtpConfig::default();
        client_cfg.local_identity = Some(client_id.clone());
        client_cfg.trusted_peer_keys = vec![wrong_server.public_bytes()];
        client_cfg.require_peer_identity = true;

        let server = AtpEngine::new_server_with_config(listen, server_cfg).unwrap();
        server.start();

        let client = AtpEngine::new_client_with_config(listen, client_cfg).unwrap();
        let _ = client.ensure_client_handshake().unwrap();
        client.start();

        let mut connected = false;
        for _ in 0..80 {
            thread::sleep(Duration::from_millis(20));
            for d in client.poll_deliveries() {
                if matches!(d, Delivery::Connected { .. }) {
                    connected = true;
                }
            }
            if connected {
                break;
            }
        }
        server.stop();
        client.stop();
        assert!(
            !connected,
            "client trusting wrong server key must not complete handshake"
        );
    }
}

#[cfg(test)]
mod loss_engine_tests {
    use super::super::engine::{AtpEngine, Delivery, SendRequest};
    use super::super::loss_sim::LossSimulator;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    fn localhost_port() -> u16 {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    }

    #[test]
    fn test_loss_simulator_hooks_without_drop() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let server = AtpEngine::new_server(listen).unwrap();
        server.start();

        let client = AtpEngine::new_client(listen).unwrap();
        let client_conn = client.ensure_client_handshake().unwrap();
        client.start();

        let mut connected = false;
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(20));
            for d in client.poll_deliveries() {
                if let Delivery::Connected { .. } = d {
                    connected = true;
                }
            }
            if connected {
                break;
            }
        }
        assert!(connected);

        let sim = Arc::new(Mutex::new(LossSimulator::new()));
        client.set_loss_simulator(sim.clone());
        sim.lock().unwrap().deliver_next(10);

        client.submit_request(SendRequest::Message {
            connection_id: client_conn,
            stream_id: 1,
            data: b"loss-test".to_vec(),
            reliable: true,
            ordered: true,
        });

        let mut received = false;
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(20));
            for d in server.poll_deliveries() {
                if let Delivery::Message { data, .. } = d {
                    if data == b"loss-test" {
                        received = true;
                    }
                }
            }
            if received {
                break;
            }
        }

        server.stop();
        client.stop();
        assert!(received, "message should deliver through loss sim hook");
    }
}

#[cfg(test)]
mod fuzz_wire {
    use super::super::wire::{Frame, PacketHeader};

    /// Arbitrary bytes must never panic during parse.
    #[test]
    fn test_packet_header_never_panics() {
        for seed in 0u8..=255 {
            let data = vec![seed; (seed as usize % 64) + 1];
            let _ = PacketHeader::decode(&data);
        }
    }

    #[test]
    fn test_frame_decode_never_panics() {
        for seed in 0u8..=255 {
            let data = vec![seed; (seed as usize % 128) + 1];
            let _ = Frame::decode(&data, 0);
            let _ = Frame::decode_all(&data);
        }
    }
}

#[cfg(test)]
mod adversarial {
    use super::super::config::AtpConfig;
    use super::super::engine::{AtpEngine, Delivery};
    use super::super::identity::IdentityKeyPair;
    use super::super::path::PathManager;
    use super::super::token::{HandshakeRateLimiter, RetryTokenIssuer};
    use super::super::wire::Frame;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    fn localhost_port() -> u16 {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    }

    #[test]
    fn test_untrusted_identity_rejected() {
        let port = localhost_port();
        let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let server_id = Arc::new(IdentityKeyPair::generate());
        let client_id = Arc::new(IdentityKeyPair::generate());
        let impostor = Arc::new(IdentityKeyPair::generate());

        let mut server_cfg = AtpConfig::default();
        server_cfg.local_identity = Some(server_id.clone());
        server_cfg.trusted_peer_keys = vec![client_id.public_bytes()];
        server_cfg.require_peer_identity = true;

        let mut client_cfg = AtpConfig::default();
        client_cfg.local_identity = Some(impostor.clone());
        client_cfg.trusted_peer_keys = vec![server_id.public_bytes()];
        client_cfg.require_peer_identity = true;

        let server = AtpEngine::new_server_with_config(listen, server_cfg).unwrap();
        server.start();

        let client = AtpEngine::new_client_with_config(listen, client_cfg).unwrap();
        let _ = client.ensure_client_handshake().unwrap();
        client.start();

        let mut connected = false;
        for _ in 0..80 {
            thread::sleep(Duration::from_millis(20));
            for d in client.poll_deliveries() {
                if matches!(d, Delivery::Connected { .. }) {
                    connected = true;
                }
            }
            if connected {
                break;
            }
        }
        server.stop();
        client.stop();
        assert!(
            !connected,
            "handshake with untrusted client identity must not complete"
        );
    }

    #[test]
    fn test_rate_limiter_eviction_under_many_ips() {
        let mut limiter =
            HandshakeRateLimiter::new(100, Duration::from_secs(60)).with_max_entries(32);
        for i in 0..64u16 {
            let ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, (i % 256) as u8));
            limiter.allow(&ip);
        }
        assert!(limiter.len() <= 32);
    }

    #[test]
    fn test_migration_survives_bad_path_response() {
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9100);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9101);
        let now = Instant::now();
        let mut pm = PathManager::new(addr1, now);
        let challenge = pm.begin_validation(addr2, now).unwrap();
        assert!(pm.on_path_response([0u8; 8], addr2, now).unwrap().is_none());
        assert_eq!(
            pm.migration_state,
            super::super::path::MigrationState::Validating
        );
        let migrated = pm.on_path_response(challenge, addr2, now).unwrap();
        assert_eq!(migrated, Some(addr2));
    }

    #[test]
    fn test_retry_token_rejects_string_binding_mismatch() {
        let issuer = RetryTokenIssuer::new(Duration::from_secs(60));
        let token = issuer.issue(&IpAddr::V4(Ipv4Addr::LOCALHOST));
        let other = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        assert!(issuer.validate(&token, &other).is_err());
    }

    #[test]
    fn test_oversized_cid_rejected() {
        let mut data = vec![super::super::wire::FrameType::ConnectionInit as u8];
        data.extend_from_slice(&super::super::config::ATP_VERSION.to_be_bytes());
        data.push((super::super::config::MAX_CID_LEN + 1) as u8);
        assert!(Frame::decode(&data, 0).is_err());
    }

    #[test]
    fn test_truncated_ack_range_rejected() {
        let mut data = vec![super::super::wire::FrameType::Ack as u8];
        data.extend_from_slice(&[0x01]); // one range
        data.extend_from_slice(&[0x10]); // truncated range payload
        assert!(Frame::decode(&data, 0).is_err());
    }

    #[test]
    fn test_oversized_frame_length_rejected() {
        let mut data = vec![super::super::wire::FrameType::Data as u8];
        data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // huge varint
        assert!(Frame::decode(&data, 0).is_err());
    }
}

#[cfg(test)]
mod loss_sim_tests {
    use super::super::loss_sim::LossSimulator;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn test_loss_simulator_drop_retransmit_scenario() {
        let mut sim = LossSimulator::new();
        sim.deliver_next(1);
        sim.drop_next(1);
        sim.deliver_next(1);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
        sim.send(b"pkt1".to_vec(), addr);
        sim.send(b"pkt2".to_vec(), addr);
        sim.send(b"pkt3".to_vec(), addr);
        assert_eq!(sim.dropped_count(), 1);
        let delivered = sim.drain_delivered();
        assert_eq!(delivered.len(), 2);
        assert_eq!(delivered[0].0, b"pkt1");
        assert_eq!(delivered[1].0, b"pkt3");
    }

    #[test]
    fn test_loss_simulator_reorder() {
        let mut sim = LossSimulator::new();
        sim.reorder_next(1);
        sim.deliver_next(1);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
        sim.send(b"a".to_vec(), addr);
        sim.send(b"b".to_vec(), addr);
        let d = sim.drain_delivered();
        assert_eq!(d[0].0, b"b");
        assert_eq!(d[1].0, b"a");
    }
}
