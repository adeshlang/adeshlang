//! ATP configuration and protocol constants.
//!
//! All limits and magic values are centralized here.

use std::time::Duration;

/// ATP protocol version.
pub const ATP_VERSION: u32 = 1;

/// Conservative maximum UDP payload (no IP fragmentation).
pub const MAX_PACKET_PAYLOAD: usize = 1200;

/// AEAD authentication tag length (ChaCha20-Poly1305).
pub const AEAD_TAG_LEN: usize = 16;

/// Wire connection ID length in bytes.
pub const CONNECTION_ID_LEN: usize = 8;

/// Maximum connection ID length on the wire.
pub const MAX_CID_LEN: usize = 20;

/// Maximum varint value (62-bit).
pub const MAX_VARINT: u64 = 0x3FFF_FFFF_FFFF_FFFF;

/// Maximum frame payload size.
pub const MAX_FRAME_PAYLOAD: usize = MAX_PACKET_PAYLOAD - 64;

/// Maximum message size (64 MiB).
pub const MAX_MESSAGE_SIZE: u64 = 64 * 1024 * 1024;

/// Maximum fragments per message.
pub const MAX_FRAGMENT_COUNT: u64 = 65_536;

/// Maximum concurrent streams per connection.
pub const MAX_STREAMS: u64 = 256;

/// Maximum connection-level data window.
pub const MAX_CONNECTION_DATA: u64 = 16 * 1024 * 1024;

/// Maximum per-stream data window.
pub const MAX_STREAM_DATA: u64 = 2 * 1024 * 1024;

/// Maximum reassembly memory per message.
pub const MAX_REASSEMBLY_MEMORY: usize = 4 * 1024 * 1024;

/// Maximum reason string length in ConnectionClose.
pub const MAX_REASON_LENGTH: usize = 512;

/// Maximum ACK ranges per frame.
pub const MAX_ACK_RANGES: usize = 64;

/// Maximum handshake message size.
pub const MAX_HANDSHAKE_SIZE: usize = 4096;

/// Maximum datagram size.
pub const MAX_DATAGRAM_SIZE: usize = MAX_PACKET_PAYLOAD - 128;

/// Default idle timeout.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Default handshake timeout.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Replay protection window size (packets).
pub const REPLAY_WINDOW_SIZE: u64 = 64;

/// Gap-based loss detection threshold.
pub const REORDER_THRESHOLD: u64 = 3;

/// RTT estimator bounds.
pub const MIN_RTO_MS: u64 = 25;
pub const MAX_RTO_MS: u64 = 60_000;
pub const INITIAL_RTT_MS: u64 = 100;

/// Engine scheduling limits per cycle.
pub const MAX_PACKETS_PER_CYCLE: usize = 64;
pub const MAX_CONNECTIONS_PER_CYCLE: usize = 16;

/// UDP receive buffer size (max ATP packet + short header + AEAD tag slack).
pub const UDP_RECV_BUFFER_SIZE: usize = MAX_PACKET_PAYLOAD + 128;
/// Default buffer pool capacity (aligned with max packet size).
pub const BUFFER_POOL_CAPACITY: usize = UDP_RECV_BUFFER_SIZE;
pub const BUFFER_POOL_MAX_BUFFERS: usize = 256;

/// Default retry token TTL.
pub const DEFAULT_RETRY_TOKEN_TTL: Duration = Duration::from_secs(30);

/// Default handshake rate limit (attempts per window).
pub const DEFAULT_HANDSHAKE_RATE_LIMIT: u32 = 10;

/// Default handshake rate limit window.
pub const DEFAULT_HANDSHAKE_RATE_WINDOW: Duration = Duration::from_secs(10);

/// ATP configuration used by connections and the engine.
#[derive(Clone)]
pub struct AtpConfig {
    pub max_packet_size: usize,
    pub max_message_size: u64,
    pub max_fragment_count: u64,
    pub max_streams: u64,
    pub max_stream_data: u64,
    pub max_connection_data: u64,
    pub max_reassembly_memory: usize,
    pub max_reason_length: usize,
    pub idle_timeout: Duration,
    pub handshake_timeout: Duration,
    pub max_ack_ranges: usize,
    pub max_handshake_size: usize,
    pub max_datagram_size: usize,
    /// Local Ed25519 identity key for endpoint authentication.
    pub local_identity: Option<std::sync::Arc<super::identity::IdentityKeyPair>>,
    /// Trusted peer Ed25519 public keys (empty = accept any valid proof).
    pub trusted_peer_keys: Vec<[u8; super::identity::ED25519_PUBKEY_LEN]>,
    /// Reject handshakes without a valid identity proof.
    pub require_peer_identity: bool,
    /// Shared secret for retry token HMAC (None = random per engine).
    pub retry_token_secret: Option<[u8; 32]>,
    pub retry_token_ttl: Duration,
    pub handshake_rate_limit: u32,
    pub handshake_rate_window: Duration,
}

impl std::fmt::Debug for AtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AtpConfig")
            .field("max_packet_size", &self.max_packet_size)
            .field("require_peer_identity", &self.require_peer_identity)
            .field("trusted_peer_keys", &self.trusted_peer_keys.len())
            .field("has_local_identity", &self.local_identity.is_some())
            .field("retry_token_ttl", &self.retry_token_ttl)
            .finish_non_exhaustive()
    }
}

impl Default for AtpConfig {
    fn default() -> Self {
        AtpConfig {
            max_packet_size: MAX_PACKET_PAYLOAD,
            max_message_size: MAX_MESSAGE_SIZE,
            max_fragment_count: MAX_FRAGMENT_COUNT,
            max_streams: MAX_STREAMS,
            max_stream_data: MAX_STREAM_DATA,
            max_connection_data: MAX_CONNECTION_DATA,
            max_reassembly_memory: MAX_REASSEMBLY_MEMORY,
            max_reason_length: MAX_REASON_LENGTH,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            max_ack_ranges: MAX_ACK_RANGES,
            max_handshake_size: MAX_HANDSHAKE_SIZE,
            max_datagram_size: MAX_DATAGRAM_SIZE,
            local_identity: None,
            trusted_peer_keys: Vec::new(),
            require_peer_identity: false,
            retry_token_secret: None,
            retry_token_ttl: DEFAULT_RETRY_TOKEN_TTL,
            handshake_rate_limit: DEFAULT_HANDSHAKE_RATE_LIMIT,
            handshake_rate_window: DEFAULT_HANDSHAKE_RATE_WINDOW,
        }
    }
}

impl AtpConfig {
    /// Maximum plaintext fragment payload given header overhead estimate.
    pub fn max_fragment_payload(&self) -> usize {
        self.max_packet_size.saturating_sub(128).max(64)
    }
}
