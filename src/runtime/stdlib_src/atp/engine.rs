//! ATP Engine — event-driven transport I/O loop.

use super::config::{
    ATP_VERSION, AtpConfig, MAX_CONNECTIONS_PER_CYCLE, MAX_PACKETS_PER_CYCLE, UDP_RECV_BUFFER_SIZE,
};
use super::connection::{AtpConnection, ConnectionState, ReceivedData, TransportParams};
use super::errors::{AtpError, AtpResult};
use super::handshake::{ClientHandshake, ServerHandshake};
use super::id::{ConnectionId, WireConnectionId};
use super::memory::{BufferPool, ConnectionMemoryConfig};
use super::message::ReliabilityMode;
use super::reliability::{RetransmittableRef, SentPacket};
use super::router::ConnectionRouter;
use super::security::{EphemeralKeyPair, PacketNumberSpace, SecurityContext};
use super::stream::Priority;
use super::token::{HandshakeRateLimiter, RetryTokenIssuer};
use super::wire::{Frame, LongHeader, PacketHeader, ShortHeader};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static NEXT_ENGINE_ID: AtomicU64 = AtomicU64::new(1);

/// Connection-targeted send request.
#[derive(Debug, Clone)]
pub enum SendRequest {
    Message {
        connection_id: ConnectionId,
        stream_id: u64,
        data: Vec<u8>,
        reliable: bool,
        ordered: bool,
    },
    Datagram {
        connection_id: ConnectionId,
        data: Vec<u8>,
    },
    OpenStream {
        connection_id: ConnectionId,
        priority: Priority,
    },
    CloseStream {
        connection_id: ConnectionId,
        stream_id: u64,
    },
    Cancel {
        connection_id: ConnectionId,
        stream_id: u64,
        message_id: u64,
    },
    Close {
        connection_id: ConnectionId,
    },
    Migrate {
        connection_id: ConnectionId,
        new_addr: SocketAddr,
    },
    Ping {
        connection_id: ConnectionId,
    },
}

#[derive(Debug, Clone)]
pub enum Delivery {
    Message {
        connection_id: ConnectionId,
        stream_id: u64,
        message_id: u64,
        data: Vec<u8>,
    },
    Datagram {
        connection_id: ConnectionId,
        data: Vec<u8>,
    },
    StreamOpen {
        connection_id: ConnectionId,
        stream_id: u64,
    },
    StreamClose {
        connection_id: ConnectionId,
        stream_id: u64,
    },
    Connected {
        connection_id: ConnectionId,
    },
    Closed {
        connection_id: ConnectionId,
        reason: String,
    },
    Error {
        connection_id: Option<ConnectionId>,
        error: AtpError,
    },
}

pub struct AtpEngine {
    pub engine_id: u64,
    pub socket: Arc<Mutex<Option<std::net::UdpSocket>>>,
    pub router: Arc<Mutex<ConnectionRouter>>,
    pub deliveries: Arc<Mutex<VecDeque<Delivery>>>,
    pub send_requests: Arc<Mutex<VecDeque<SendRequest>>>,
    pub running: Arc<AtomicBool>,
    pub is_server: bool,
    pub listen_addr: Option<SocketAddr>,
    pub target_addr: Option<SocketAddr>,
    pub config: AtpConfig,
    pub buffer_pool: Arc<BufferPool>,
    next_wake: Arc<Mutex<Instant>>,
    token_issuer: RetryTokenIssuer,
    rate_limiter: Mutex<HandshakeRateLimiter>,
    #[cfg(test)]
    loss_sim: Mutex<Option<std::sync::Arc<Mutex<super::loss_sim::LossSimulator>>>>,
}

impl AtpEngine {
    pub fn new_server(listen_addr: SocketAddr) -> AtpResult<Arc<Self>> {
        Self::new_server_with_config(listen_addr, AtpConfig::default())
    }

    pub fn new_server_with_config(
        listen_addr: SocketAddr,
        config: AtpConfig,
    ) -> AtpResult<Arc<Self>> {
        let socket = std::net::UdpSocket::bind(listen_addr)
            .map_err(|e| AtpError::transport(format!("bind failed: {}", e)))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| AtpError::transport(format!("set_nonblocking: {}", e)))?;
        Ok(Self::build_engine(
            socket,
            true,
            Some(listen_addr),
            None,
            config,
        ))
    }

    pub fn new_client(target: SocketAddr) -> AtpResult<Arc<Self>> {
        Self::new_client_with_config(target, AtpConfig::default())
    }

    pub fn new_client_with_config(target: SocketAddr, config: AtpConfig) -> AtpResult<Arc<Self>> {
        let bind_addr = match target {
            SocketAddr::V4(_) => "0.0.0.0:0",
            SocketAddr::V6(_) => "[::]:0",
        };
        let socket = std::net::UdpSocket::bind(bind_addr)
            .map_err(|e| AtpError::transport(format!("bind failed: {}", e)))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| AtpError::transport(format!("set_nonblocking: {}", e)))?;
        Ok(Self::build_engine(
            socket,
            false,
            None,
            Some(target),
            config,
        ))
    }

    fn build_engine(
        socket: std::net::UdpSocket,
        is_server: bool,
        listen_addr: Option<SocketAddr>,
        target_addr: Option<SocketAddr>,
        config: AtpConfig,
    ) -> Arc<Self> {
        let id = NEXT_ENGINE_ID.fetch_add(1, Ordering::SeqCst);
        let token_issuer = match config.retry_token_secret {
            Some(secret) => RetryTokenIssuer::with_secret(secret, config.retry_token_ttl),
            None => RetryTokenIssuer::new(config.retry_token_ttl),
        };
        let rate_limiter =
            HandshakeRateLimiter::new(config.handshake_rate_limit, config.handshake_rate_window);
        Arc::new(AtpEngine {
            engine_id: id,
            socket: Arc::new(Mutex::new(Some(socket))),
            router: Arc::new(Mutex::new(ConnectionRouter::new())),
            deliveries: Arc::new(Mutex::new(VecDeque::new())),
            send_requests: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(AtomicBool::new(false)),
            is_server,
            listen_addr,
            target_addr,
            config,
            buffer_pool: Arc::new(BufferPool::new(
                super::config::BUFFER_POOL_CAPACITY,
                super::config::BUFFER_POOL_MAX_BUFFERS,
            )),
            next_wake: Arc::new(Mutex::new(Instant::now())),
            token_issuer,
            rate_limiter: Mutex::new(rate_limiter),
            #[cfg(test)]
            loss_sim: Mutex::new(None),
        })
    }

    /// Attach a loss simulator for integration tests (test-only).
    #[cfg(test)]
    pub fn set_loss_simulator(
        self: &Arc<Self>,
        sim: std::sync::Arc<Mutex<super::loss_sim::LossSimulator>>,
    ) {
        if let Ok(mut guard) = self.loss_sim.lock() {
            *guard = Some(sim);
        }
    }

    fn local_identity(&self) -> Option<std::sync::Arc<super::identity::IdentityKeyPair>> {
        self.config.local_identity.clone()
    }

    pub fn start(self: &Arc<Self>) {
        self.running.store(true, Ordering::SeqCst);
        let engine = self.clone();
        std::thread::spawn(move || engine.run_loop());
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.socket.lock() {
            guard.take();
        }
    }

    fn run_loop(&self) {
        if !self.is_server {
            if self.connection_id_for_client().is_none() {
                if let Err(e) = self.initiate_client_handshake() {
                    self.deliver(Delivery::Error {
                        connection_id: None,
                        error: e,
                    });
                }
            }
        }

        let mut buf = self.buffer_pool.acquire_with_capacity(UDP_RECV_BUFFER_SIZE);
        while self.running.load(Ordering::SeqCst) {
            self.process_send_requests();
            self.process_outgoing();
            self.process_incoming(&mut buf);
            self.process_timers();

            let sleep_dur = self.compute_sleep_duration();
            if sleep_dur > Duration::ZERO {
                std::thread::sleep(sleep_dur);
            }
        }
    }

    fn compute_sleep_duration(&self) -> Duration {
        let wake = self
            .next_wake
            .lock()
            .map(|w| *w)
            .unwrap_or_else(|_| Instant::now());
        let now = Instant::now();
        if wake > now {
            (wake - now).min(Duration::from_millis(50))
        } else {
            Duration::from_millis(5)
        }
    }

    fn initiate_client_handshake(&self) -> AtpResult<ConnectionId> {
        let target = self
            .target_addr
            .ok_or_else(|| AtpError::connection("no target"))?;
        let now = Instant::now();
        let eph = EphemeralKeyPair::generate();
        let local_cid = WireConnectionId::random();

        let mut conn = AtpConnection::new(
            true,
            target,
            ConnectionMemoryConfig::default(),
            self.config.clone(),
            now,
        );
        conn.local_cid = local_cid;
        conn.transition(ConnectionState::Handshaking);
        conn.schedule_handshake_timeout(now);

        let mut hs = ClientHandshake::new(eph, local_cid, now);
        let tp = TransportParams::default().encode();
        let identity = self.local_identity();
        let init_frame = hs.build_init(&tp, &[], identity.as_deref())?;
        conn.client_handshake = Some(hs);

        let payload = Frame::encode_all(&[init_frame]);
        let header = LongHeader {
            version: ATP_VERSION,
            dst_conn_id: vec![0; 8],
            src_conn_id: local_cid.as_bytes().to_vec(),
            packet_number: 0,
            payload,
        };
        let packet = PacketHeader::encode_long(&header);
        self.send_udp(&packet, target)?;

        let id = conn.internal_id;
        if let Ok(mut router) = self.router.lock() {
            router.insert(conn);
        }
        Ok(id)
    }

    fn send_udp(&self, data: &[u8], addr: SocketAddr) -> AtpResult<()> {
        #[cfg(test)]
        {
            if let Ok(guard) = self.loss_sim.lock() {
                if let Some(sim) = guard.as_ref() {
                    if let Ok(mut sim) = sim.lock() {
                        sim.send(data.to_vec(), addr);
                        for (pkt, dest) in sim.drain_delivered() {
                            self.send_udp_direct(&pkt, dest)?;
                        }
                        return Ok(());
                    }
                }
            }
        }
        self.send_udp_direct(data, addr)
    }

    fn send_udp_direct(&self, data: &[u8], addr: SocketAddr) -> AtpResult<()> {
        let guard = self
            .socket
            .lock()
            .map_err(|_| AtpError::transport("mutex"))?;
        let socket = guard
            .as_ref()
            .ok_or_else(|| AtpError::transport("socket closed"))?;
        match socket.send_to(data, addr) {
            Ok(_) => Ok(()),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => Ok(()),
            Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => Ok(()),
            Err(e) => Err(AtpError::transport(format!("send_to: {}", e))),
        }
    }

    fn process_incoming(&self, buf: &mut super::memory::PooledBuffer) {
        for _ in 0..MAX_PACKETS_PER_CYCLE {
            let received = {
                let recv_slice = buf.recv_buf(UDP_RECV_BUFFER_SIZE);
                let guard = match self.socket.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                let socket = match guard.as_ref() {
                    Some(s) => s,
                    None => return,
                };
                match socket.recv_from(recv_slice) {
                    Ok((n, addr)) => {
                        buf.set_recv_len(n);
                        Some((n, addr))
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => None,
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => continue,
                    Err(_) => None,
                }
            };

            let Some((n, src_addr)) = received else {
                break;
            };
            self.handle_packet(buf.as_slice(), src_addr);
            let _ = n;
        }
    }

    fn handle_packet(&self, data: &[u8], src_addr: SocketAddr) {
        let (header, header_end) = match PacketHeader::decode(data) {
            Ok(h) => h,
            Err(_) => return,
        };

        let now = Instant::now();

        match header {
            PacketHeader::Long(ref lh) => {
                self.handle_long_packet(lh, data, header_end, src_addr, now);
            }
            PacketHeader::Short(ref sh) => {
                self.handle_short_packet(sh, data, header_end, src_addr, now);
            }
        }
    }

    fn handle_long_packet(
        &self,
        lh: &LongHeader,
        data: &[u8],
        header_end: usize,
        src_addr: SocketAddr,
        now: Instant,
    ) {
        let frames = match Frame::decode_all(&lh.payload) {
            Ok(f) => f,
            Err(_) => return,
        };

        for frame in &frames {
            let frame_bytes = frame.encode();
            match frame {
                Frame::ConnectionInit {
                    version,
                    src_conn_id,
                    ephemeral_pubkey,
                    transport_params,
                    retry_token,
                    identity_proof,
                } if self.is_server => {
                    self.handle_server_init(
                        *version,
                        src_conn_id,
                        ephemeral_pubkey,
                        transport_params,
                        retry_token,
                        identity_proof,
                        &frame_bytes,
                        src_addr,
                        now,
                    );
                }
                Frame::ConnectionInitAck {
                    src_conn_id,
                    ephemeral_pubkey,
                    transport_params,
                    identity_proof,
                    ..
                } if !self.is_server => {
                    self.handle_client_init_ack(
                        src_conn_id,
                        ephemeral_pubkey,
                        transport_params,
                        identity_proof,
                        &frame_bytes,
                        src_addr,
                        now,
                    );
                }
                Frame::Handshake { confirm } => {
                    self.handle_handshake_frame(confirm, &frame_bytes, src_addr, now);
                }
                Frame::Retry { token } if !self.is_server => {
                    self.resend_client_init_with_token(token.clone(), src_addr);
                }
                _ => {}
            }
        }
        let _ = (data, header_end);
    }

    fn handle_server_init(
        &self,
        version: u32,
        client_cid: &[u8],
        client_pubkey: &[u8],
        transport_params: &[u8],
        retry_token: &[u8],
        identity_proof: &[u8],
        frame_bytes: &[u8],
        src_addr: SocketAddr,
        now: Instant,
    ) {
        if let Ok(mut limiter) = self.rate_limiter.lock() {
            if !limiter.allow(&src_addr.ip()) {
                return;
            }
        }

        if retry_token.is_empty() {
            let token = self.token_issuer.issue(&src_addr.ip());
            let payload = Frame::encode_all(&[Frame::Retry { token }]);
            let header = LongHeader {
                version: ATP_VERSION,
                dst_conn_id: client_cid.to_vec(),
                src_conn_id: vec![0; 8],
                packet_number: 0,
                payload,
            };
            let packet = PacketHeader::encode_long(&header);
            let _ = self.send_udp(&packet, src_addr);
            return;
        }

        if self
            .token_issuer
            .validate(retry_token, &src_addr.ip())
            .is_err()
        {
            let token = self.token_issuer.issue(&src_addr.ip());
            let payload = Frame::encode_all(&[Frame::Retry { token }]);
            let header = LongHeader {
                version: ATP_VERSION,
                dst_conn_id: client_cid.to_vec(),
                src_conn_id: vec![0; 8],
                packet_number: 0,
                payload,
            };
            let packet = PacketHeader::encode_long(&header);
            let _ = self.send_udp(&packet, src_addr);
            return;
        }

        let client_wire = match WireConnectionId::from_slice(client_cid) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        if router.find_pending(client_cid).is_some() {
            return;
        }

        let local_cid = WireConnectionId::random();
        let mut conn = AtpConnection::new(
            false,
            src_addr,
            ConnectionMemoryConfig::default(),
            self.config.clone(),
            now,
        );
        conn.local_cid = local_cid;
        conn.remote_cid = Some(client_wire);
        conn.transition(ConnectionState::Handshaking);
        conn.schedule_handshake_timeout(now);

        let mut hs = ServerHandshake::new(local_cid, now);
        if hs
            .process_init(
                version,
                client_cid,
                client_pubkey,
                identity_proof,
                frame_bytes,
                &self.config.trusted_peer_keys,
                self.config.require_peer_identity,
            )
            .is_err()
        {
            return;
        }

        if let Ok(params) = TransportParams::decode(transport_params) {
            conn.peer_params = params;
        }

        let identity = self.local_identity();
        let init_ack = match hs.build_init_ack(&conn.local_params.encode(), identity.as_deref()) {
            Ok(f) => f,
            Err(_) => return,
        };
        conn.server_handshake = Some(hs);

        let id = conn.internal_id;
        router.insert(conn);
        drop(router);

        self.send_long_handshake(id, src_addr, local_cid, client_wire, vec![init_ack], 0);
    }

    fn handle_client_init_ack(
        &self,
        server_cid: &[u8],
        server_pubkey: &[u8],
        transport_params: &[u8],
        identity_proof: &[u8],
        frame_bytes: &[u8],
        src_addr: SocketAddr,
        now: Instant,
    ) {
        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        let conn_id = router.iter().find_map(|(id, c)| {
            if c.is_client && c.state == ConnectionState::Handshaking {
                Some(*id)
            } else {
                None
            }
        });

        let conn_id = match conn_id {
            Some(id) => id,
            None => return,
        };

        let conn = match router.get_mut(conn_id) {
            Some(c) => c,
            None => return,
        };

        if let Some(hs) = &mut conn.client_handshake {
            if hs
                .process_init_ack(
                    server_cid,
                    server_pubkey,
                    transport_params,
                    identity_proof,
                    frame_bytes,
                    &conn.config.trusted_peer_keys,
                    conn.config.require_peer_identity,
                )
                .is_err()
            {
                return;
            }
            conn.remote_cid = WireConnectionId::from_slice(server_cid).ok();
            if let Ok(params) = TransportParams::decode(transport_params) {
                conn.peer_params = params;
            }

            if let Ok(finished) = hs.build_finished() {
                let local_cid = conn.local_cid;
                let remote_cid = conn.remote_cid.unwrap();
                drop(router);
                self.send_long_handshake(
                    conn_id,
                    src_addr,
                    local_cid,
                    remote_cid,
                    vec![finished],
                    1,
                );
            }
        }
        let _ = now;
    }

    fn handle_handshake_frame(
        &self,
        confirm: &[u8],
        frame_bytes: &[u8],
        src_addr: SocketAddr,
        now: Instant,
    ) {
        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        let conn_id = router.iter().find_map(|(id, c)| {
            if c.peer_addr == src_addr && c.state == ConnectionState::Handshaking {
                Some(*id)
            } else {
                None
            }
        });
        let conn_id = match conn_id {
            Some(id) => id,
            None => return,
        };

        let conn = match router.get_mut(conn_id) {
            Some(c) => c,
            None => return,
        };

        if conn.is_client {
            if let Some(hs) = &mut conn.client_handshake {
                if hs.verify_server_finished(confirm, frame_bytes).is_ok() {
                    if let Ok(app_keys) = hs.application_keys() {
                        let mut sec = SecurityContext::from_handshake_secrets(&app_keys);
                        sec.activate_application(&app_keys);
                        conn.security = Some(sec);
                    }
                    conn.transition(ConnectionState::Established);
                    Self::ensure_default_stream(conn);
                    conn.schedule_keepalive(now);
                    conn.schedule_idle_check(now);
                    drop(router);
                    self.deliver(Delivery::Connected {
                        connection_id: conn_id,
                    });
                }
            }
        } else if let Some(hs) = &mut conn.server_handshake {
            if hs.verify_client_finished(confirm, frame_bytes).is_ok() {
                let local_cid = conn.local_cid;
                let remote_cid = match conn.remote_cid {
                    Some(c) => c,
                    None => return,
                };
                let peer = conn.peer_addr;
                if let Ok(finished) = hs.build_finished() {
                    drop(router);
                    self.send_long_handshake(
                        conn_id,
                        peer,
                        local_cid,
                        remote_cid,
                        vec![finished],
                        1,
                    );
                    if let Ok(mut router) = self.router.lock() {
                        if let Some(conn) = router.get_mut(conn_id) {
                            if let Some(hs) = &conn.server_handshake {
                                if let Ok(app_keys) = hs.application_keys() {
                                    let mut sec =
                                        SecurityContext::from_handshake_secrets(&app_keys);
                                    sec.activate_application(&app_keys);
                                    conn.security = Some(sec);
                                }
                            }
                            conn.transition(ConnectionState::Established);
                            Self::ensure_default_stream(conn);
                            conn.schedule_keepalive(now);
                            conn.schedule_idle_check(now);
                        }
                    }
                    self.deliver(Delivery::Connected {
                        connection_id: conn_id,
                    });
                }
            }
        }
    }

    fn resend_client_init_with_token(&self, token: Vec<u8>, src_addr: SocketAddr) {
        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };
        let conn_id = router.iter().find_map(|(id, c)| {
            if c.is_client && c.state == ConnectionState::Handshaking {
                Some(*id)
            } else {
                None
            }
        });
        let conn_id = match conn_id {
            Some(id) => id,
            None => return,
        };
        let conn = match router.get_mut(conn_id) {
            Some(c) => c,
            None => return,
        };
        conn.retry_token = token.clone();
        if let Some(hs) = &mut conn.client_handshake {
            let tp = conn.local_params.encode();
            let identity = self.local_identity();
            if let Ok(init) = hs.resend_init(&tp, &token, identity.as_deref()) {
                let local_cid = conn.local_cid;
                drop(router);
                let dst = vec![0u8; 8];
                let header = LongHeader {
                    version: ATP_VERSION,
                    dst_conn_id: dst,
                    src_conn_id: local_cid.as_bytes().to_vec(),
                    packet_number: 0,
                    payload: init.encode(),
                };
                let packet = PacketHeader::encode_long(&header);
                let _ = self.send_udp(&packet, src_addr);
            }
        }
    }

    fn send_long_handshake(
        &self,
        _conn_id: ConnectionId,
        addr: SocketAddr,
        src_cid: WireConnectionId,
        dst_cid: WireConnectionId,
        frames: Vec<Frame>,
        pn: u64,
    ) {
        let payload = Frame::encode_all(&frames);
        let header = LongHeader {
            version: ATP_VERSION,
            dst_conn_id: dst_cid.as_bytes().to_vec(),
            src_conn_id: src_cid.as_bytes().to_vec(),
            packet_number: pn,
            payload,
        };
        let packet = PacketHeader::encode_long(&header);
        let _ = self.send_udp(&packet, addr);
    }

    fn handle_short_packet(
        &self,
        sh: &ShortHeader,
        data: &[u8],
        header_end: usize,
        src_addr: SocketAddr,
        now: Instant,
    ) {
        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        let conn_id = match router.find_by_local_cid(&sh.dst_conn_id) {
            Some(id) => id,
            None => return, // Unknown connection — discard safely.
        };

        let conn = match router.get_mut(conn_id) {
            Some(c) => c,
            None => return,
        };

        if !conn.is_established() {
            return;
        }

        let header_aad = &data[..header_end];
        let sec = match conn.security.as_mut() {
            Some(s) => s,
            None => return,
        };

        let plaintext = match sec.decrypt(
            PacketNumberSpace::Application,
            sh.packet_number,
            &sh.payload,
            header_aad,
        ) {
            Ok(p) => p,
            Err(_) => return,
        };

        if !conn.paths.accepts_active(src_addr) {
            if conn.paths.is_validation_target(src_addr) {
                let frames = match Frame::decode_all(&plaintext) {
                    Ok(f) => f,
                    Err(_) => return,
                };
                if !frames
                    .iter()
                    .all(|f| matches!(f, Frame::PathResponse { .. }))
                {
                    return;
                }
            } else {
                return;
            }
        }

        conn.ack_tracker.on_packet_received(sh.packet_number, now);
        conn.metrics.packets_received += 1;
        conn.metrics.bytes_received += data.len() as u64;

        let frames = match Frame::decode_all(&plaintext) {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut conn = match router.remove(conn_id) {
            Some(c) => c,
            None => return,
        };
        drop(router);

        let received = match conn.process_frames(frames, src_addr, now) {
            Ok(r) => r,
            Err(_) => {
                if let Ok(mut r) = self.router.lock() {
                    r.insert(conn);
                }
                return;
            }
        };

        for data in received {
            match data {
                ReceivedData::Message {
                    stream_id,
                    message_id,
                    data,
                } => self.deliver(Delivery::Message {
                    connection_id: conn_id,
                    stream_id,
                    message_id,
                    data,
                }),
                ReceivedData::Datagram { data } => self.deliver(Delivery::Datagram {
                    connection_id: conn_id,
                    data,
                }),
            }
        }

        if let Ok(mut r) = self.router.lock() {
            if !conn.is_closed() {
                r.insert(conn);
            }
        }
    }

    fn process_send_requests(&self) {
        let requests: Vec<SendRequest> = {
            let mut guard = match self.send_requests.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            guard.drain(..).collect()
        };

        let now = Instant::now();
        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        for req in requests {
            let conn_id = match &req {
                SendRequest::Message { connection_id, .. }
                | SendRequest::Datagram { connection_id, .. }
                | SendRequest::OpenStream { connection_id, .. }
                | SendRequest::CloseStream { connection_id, .. }
                | SendRequest::Cancel { connection_id, .. }
                | SendRequest::Close { connection_id }
                | SendRequest::Migrate { connection_id, .. }
                | SendRequest::Ping { connection_id } => *connection_id,
            };

            let conn = match router.get_mut(conn_id) {
                Some(c) => c,
                None => continue,
            };

            match req {
                SendRequest::Message {
                    stream_id,
                    data,
                    reliable,
                    ordered,
                    ..
                } => {
                    let mode = match (reliable, ordered) {
                        (true, true) => ReliabilityMode::ReliableOrdered,
                        (true, false) => ReliabilityMode::ReliableUnordered,
                        (false, true) => ReliabilityMode::OrderedBestEffort,
                        (false, false) => ReliabilityMode::Unreliable,
                    };
                    let _ = conn.send_message(stream_id, &data, mode, now);
                }
                SendRequest::Datagram { data, .. } => {
                    let _ = conn.send_datagram(data);
                }
                SendRequest::OpenStream { priority, .. } => {
                    let _ = conn.open_stream(priority);
                }
                SendRequest::CloseStream { stream_id, .. } => {
                    let _ = conn.close_stream(stream_id);
                }
                SendRequest::Cancel {
                    stream_id,
                    message_id,
                    ..
                } => conn.cancel_message(stream_id, message_id),
                SendRequest::Close { .. } => {
                    conn.push_control_frame(Frame::ConnectionClose {
                        error_code: 0,
                        reason: "closed".to_string(),
                    });
                    conn.transition(ConnectionState::Closing);
                }
                SendRequest::Migrate { new_addr, .. } => {
                    let _ = conn.begin_migration(new_addr, now);
                }
                SendRequest::Ping { .. } => {
                    conn.push_control_frame(Frame::Ping);
                }
            }
        }
    }

    fn process_outgoing(&self) {
        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        let now = Instant::now();
        let conn_ids: Vec<ConnectionId> = router
            .iter()
            .filter(|(_, c)| c.is_established())
            .map(|(id, _)| *id)
            .take(MAX_CONNECTIONS_PER_CYCLE)
            .collect();

        for conn_id in conn_ids {
            let conn = match router.get_mut(conn_id) {
                Some(c) => c,
                None => continue,
            };

            if let Some(ack) = conn.ack_tracker.build_ack_frame(now) {
                conn.push_control_frame(ack);
            }
            if let Some(max_data) = conn.flow.should_send_max_data() {
                conn.push_control_frame(Frame::MaxData { max_data });
            }

            let frames = conn.drain_pending_frames();
            if frames.is_empty() {
                continue;
            }

            let addr = conn.paths.active_addr();
            let local_cid = conn.local_cid;

            for frame in &frames {
                let frame_bytes = frame.encode();
                let mut refs = Vec::new();
                let ack_eliciting = match frame {
                    Frame::Data {
                        stream_id,
                        message_id,
                        fragment_num,
                        flags,
                        ..
                    } => {
                        refs.push(RetransmittableRef {
                            stream_id: *stream_id,
                            message_id: *message_id,
                            fragment_num: *fragment_num,
                        });
                        flags & super::wire::DATA_FLAG_RELIABLE != 0
                    }
                    Frame::Ack { .. } | Frame::Ping | Frame::ConnectionClose { .. } => true,
                    _ => false,
                };

                if self
                    .send_app_packet(conn, &frame_bytes, addr, local_cid, refs, ack_eliciting)
                    .is_err()
                {
                    break;
                }
            }
        }
    }

    fn send_app_packet(
        &self,
        conn: &mut AtpConnection,
        plaintext: &[u8],
        addr: SocketAddr,
        _local_cid: WireConnectionId,
        refs: Vec<RetransmittableRef>,
        ack_eliciting: bool,
    ) -> AtpResult<()> {
        if !conn
            .congestion
            .can_send(conn.app_reliability.bytes_in_flight)
        {
            return Err(AtpError::connection("cwnd full"));
        }

        let remote_cid = conn
            .remote_cid
            .ok_or_else(|| AtpError::connection("no remote CID"))?;

        // Build header with placeholder PN for AAD.
        let pn = conn.app_reliability.allocate_packet_number();
        let mut header = ShortHeader {
            dst_conn_id: *remote_cid.as_bytes(),
            packet_number: pn,
            payload: Vec::new(),
        };
        let header_bytes = PacketHeader::encode_short(&header);

        let sec = conn
            .security
            .as_mut()
            .ok_or_else(|| AtpError::security("no security"))?;

        let ciphertext = {
            let nonce = sec.application.send.nonce_for(pn);
            let key = sec.application.send.key;
            super::security::encrypt_packet(&key, &nonce, plaintext, &header_bytes)?
        };

        header.payload = ciphertext;
        let final_packet = PacketHeader::encode_short(&header);

        let mut buf = self.buffer_pool.acquire_with_capacity(final_packet.len());
        buf.write(&final_packet);
        let packet_bytes = buf.into_vec();

        conn.app_reliability.on_packet_sent(SentPacket {
            packet_number: pn,
            sent_time: Instant::now(),
            bytes_sent: packet_bytes.len(),
            ack_eliciting,
            tx_count: 1,
            refs,
        });

        conn.metrics.packets_sent += 1;
        conn.metrics.bytes_sent += packet_bytes.len() as u64;
        self.send_udp(&packet_bytes, addr)
    }

    fn process_timers(&self) {
        let now = Instant::now();
        let mut router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        let ids: Vec<ConnectionId> = router.iter().map(|(id, _)| *id).collect();
        for conn_id in ids {
            let lost = {
                let conn = match router.get_mut(conn_id) {
                    Some(c) => c,
                    None => continue,
                };
                if !conn.is_established() {
                    continue;
                }
                conn.check_timeouts(now)
            };
            if !lost.is_empty() {
                if let Some(conn) = router.get_mut(conn_id) {
                    conn.congestion.on_timeout();
                    conn.requeue_lost_fragments(lost);
                }
            }
        }

        if let Ok(mut wake) = self.next_wake.lock() {
            *wake = now + Duration::from_millis(10);
        }
    }

    fn deliver(&self, item: Delivery) {
        if let Ok(mut guard) = self.deliveries.lock() {
            guard.push_back(item);
        }
    }

    pub fn poll_deliveries(&self) -> Vec<Delivery> {
        let mut guard = match self.deliveries.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard.drain(..).collect()
    }

    pub fn submit_request(&self, req: SendRequest) {
        if let Ok(mut guard) = self.send_requests.lock() {
            guard.push_back(req);
        }
    }

    pub fn connection_id_for_client(&self) -> Option<ConnectionId> {
        let router = self.router.lock().ok()?;
        router.iter().find(|(_, c)| c.is_client).map(|(id, _)| *id)
    }

    /// Ensure client handshake has been initiated (callable before the event loop runs).
    pub fn ensure_client_handshake(&self) -> AtpResult<ConnectionId> {
        if let Some(id) = self.connection_id_for_client() {
            return Ok(id);
        }
        if !self.is_server {
            self.initiate_client_handshake()
        } else {
            Err(AtpError::connection("not a client engine"))
        }
    }

    fn ensure_default_stream(conn: &mut AtpConnection) {
        if conn.streams.is_empty() {
            let stream_id = if conn.is_client { 1 } else { 2 };
            let mut stream = super::stream::AtpStream::new(
                stream_id,
                conn.peer_params.max_stream_data,
                conn.local_params.max_stream_data,
            );
            stream.open();
            conn.streams.insert(stream_id, stream);
        }
    }

    pub fn metrics(&self) -> EngineMetrics {
        let router = match self.router.lock() {
            Ok(r) => r,
            Err(_) => return EngineMetrics::default(),
        };
        let mut m = EngineMetrics::default();
        for (_, conn) in router.iter() {
            m.connections += 1;
            if conn.is_established() {
                m.established += 1;
            }
            m.streams += conn.stream_count();
            m.memory_used += conn.memory_used();
            m.bytes_sent += conn.metrics.bytes_sent;
            m.bytes_received += conn.metrics.bytes_received;
        }
        m
    }

    pub fn connection_metrics(&self, conn_id: ConnectionId) -> Option<ConnectionMetrics> {
        let router = self.router.lock().ok()?;
        router.get(conn_id).map(|c| c.metrics.clone())
    }
}

use super::connection::ConnectionMetrics;

#[derive(Debug, Clone, Default)]
pub struct EngineMetrics {
    pub connections: usize,
    pub established: usize,
    pub streams: usize,
    pub memory_used: usize,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_engine_server_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let engine = AtpEngine::new_server(addr).unwrap();
        assert!(engine.is_server);
    }
}
