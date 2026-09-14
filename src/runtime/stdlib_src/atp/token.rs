//! Handshake retry tokens for stateless DoS protection.
//!
//! Before allocating expensive connection state, the server can issue an
//! authenticated retry token proving the client received a prior packet.

use super::errors::{AtpError, AtpResult};
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const TOKEN_BODY_LEN: usize = 8 + 16; // expiry + address binding
const TOKEN_MAC_LEN: usize = 16;
pub const RETRY_TOKEN_LEN: usize = TOKEN_BODY_LEN + TOKEN_MAC_LEN;

/// Default max tracked IPs in the handshake rate limiter.
pub const DEFAULT_RATE_LIMITER_MAX_ENTRIES: usize = 10_000;

/// Issues and validates authenticated retry tokens with optional key rotation.
pub struct RetryTokenIssuer {
    secret: [u8; 32],
    previous_secret: Option<[u8; 32]>,
    ttl: Duration,
}

impl RetryTokenIssuer {
    pub fn new(ttl: Duration) -> Self {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        RetryTokenIssuer {
            secret,
            previous_secret: None,
            ttl,
        }
    }

    pub fn with_secret(secret: [u8; 32], ttl: Duration) -> Self {
        RetryTokenIssuer {
            secret,
            previous_secret: None,
            ttl,
        }
    }

    /// Rotate to a new secret; the old secret is retained for validation.
    pub fn rotate_secret(&mut self, new_secret: [u8; 32]) {
        self.previous_secret = Some(self.secret);
        self.secret = new_secret;
    }

    /// Issue a token bound to the peer address.
    pub fn issue(&self, peer_ip: &IpAddr) -> Vec<u8> {
        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + self.ttl.as_secs();
        self.sign(expiry, peer_ip, &self.secret)
    }

    /// Validate token for the peer address (current or previous secret).
    pub fn validate(&self, token: &[u8], peer_ip: &IpAddr) -> AtpResult<()> {
        if token.len() != RETRY_TOKEN_LEN {
            return Err(AtpError::security("invalid retry token length"));
        }
        let expiry = u64::from_be_bytes(token[0..8].try_into().unwrap());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now > expiry {
            return Err(AtpError::security("retry token expired"));
        }
        let expected = self.sign(expiry, peer_ip, &self.secret);
        if constant_time_eq(token, &expected) {
            return Ok(());
        }
        if let Some(prev) = &self.previous_secret {
            let expected_prev = self.sign(expiry, peer_ip, prev);
            if constant_time_eq(token, &expected_prev) {
                return Ok(());
            }
        }
        Err(AtpError::security("retry token authentication failed"))
    }

    fn sign(&self, expiry: u64, peer_ip: &IpAddr, secret: &[u8; 32]) -> Vec<u8> {
        let mut body = [0u8; TOKEN_BODY_LEN];
        body[0..8].copy_from_slice(&expiry.to_be_bytes());
        let binding = address_binding(peer_ip);
        body[8..24].copy_from_slice(&binding);

        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret)
            .expect("HMAC key length valid");
        mac.update(&body);
        let tag = mac.finalize().into_bytes();

        let mut token = Vec::with_capacity(RETRY_TOKEN_LEN);
        token.extend_from_slice(&body);
        token.extend_from_slice(&tag[..TOKEN_MAC_LEN]);
        token
    }
}

/// Canonical 16-byte address binding (no string serialization).
fn address_binding(ip: &IpAddr) -> [u8; 16] {
    let mut out = [0u8; 16];
    match ip {
        IpAddr::V4(v4) => {
            out[0] = 4;
            out[1..5].copy_from_slice(&v4.octets());
        }
        IpAddr::V6(v6) => {
            out.copy_from_slice(&v6.octets());
        }
    }
    out
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

#[derive(Debug, Clone)]
struct LimitEntry {
    attempts: u32,
    window_start: Instant,
}

/// Per-server handshake rate limiting with bounded state and TTL eviction.
///
/// Uses `IpAddr` keys (full 128-bit IPv6) and an LRU queue for O(1) eviction.
#[derive(Debug)]
pub struct HandshakeRateLimiter {
    attempts: HashMap<IpAddr, LimitEntry>,
    lru: VecDeque<IpAddr>,
    max_per_window: u32,
    window: Duration,
    max_entries: usize,
}

impl HandshakeRateLimiter {
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        HandshakeRateLimiter {
            attempts: HashMap::new(),
            lru: VecDeque::new(),
            max_per_window,
            window,
            max_entries: DEFAULT_RATE_LIMITER_MAX_ENTRIES,
        }
    }

    pub fn with_max_entries(mut self, max_entries: usize) -> Self {
        self.max_entries = max_entries.max(1);
        self
    }

    pub fn allow(&mut self, peer_ip: &IpAddr) -> bool {
        let now = Instant::now();
        self.evict_expired(now);

        if let Some(pos) = self.lru.iter().position(|ip| ip == peer_ip) {
            self.lru.remove(pos);
        }

        let entry = self
            .attempts
            .entry(*peer_ip)
            .or_insert(LimitEntry {
                attempts: 0,
                window_start: now,
            });
        if now.duration_since(entry.window_start) > self.window {
            *entry = LimitEntry {
                attempts: 0,
                window_start: now,
            };
        }
        if entry.attempts >= self.max_per_window {
            self.lru.push_front(*peer_ip);
            return false;
        }
        entry.attempts += 1;
        self.lru.push_back(*peer_ip);
        self.evict_over_capacity();
        true
    }

    pub fn len(&self) -> usize {
        self.attempts.len()
    }

    fn evict_expired(&mut self, now: Instant) {
        while let Some(front) = self.lru.front().copied() {
            let expired = self
                .attempts
                .get(&front)
                .is_some_and(|e| now.duration_since(e.window_start) > self.window);
            if !expired {
                break;
            }
            self.lru.pop_front();
            self.attempts.remove(&front);
        }
    }

    fn evict_over_capacity(&mut self) {
        while self.attempts.len() > self.max_entries {
            let Some(old) = self.lru.pop_front() else {
                break;
            };
            self.attempts.remove(&old);
        }
    }
}

impl Default for HandshakeRateLimiter {
    fn default() -> Self {
        Self::new(10, Duration::from_secs(10))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_retry_token_roundtrip() {
        let issuer = RetryTokenIssuer::new(Duration::from_secs(60));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let token = issuer.issue(&ip);
        assert_eq!(token.len(), RETRY_TOKEN_LEN);
        assert!(issuer.validate(&token, &ip).is_ok());
    }

    #[test]
    fn test_retry_token_wrong_address() {
        let issuer = RetryTokenIssuer::new(Duration::from_secs(60));
        let token = issuer.issue(&IpAddr::V4(Ipv4Addr::LOCALHOST));
        let other = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        assert!(issuer.validate(&token, &other).is_err());
    }

    #[test]
    fn test_retry_token_rotation() {
        let secret1 = [1u8; 32];
        let secret2 = [2u8; 32];
        let mut issuer = RetryTokenIssuer::with_secret(secret1, Duration::from_secs(60));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let old_token = issuer.issue(&ip);
        issuer.rotate_secret(secret2);
        assert!(issuer.validate(&old_token, &ip).is_ok());
        let new_token = issuer.issue(&ip);
        assert!(issuer.validate(&new_token, &ip).is_ok());
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = HandshakeRateLimiter::new(2, Duration::from_secs(10));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(limiter.allow(&ip));
        assert!(limiter.allow(&ip));
        assert!(!limiter.allow(&ip));
    }

    #[test]
    fn test_rate_limiter_bounded_entries() {
        let mut limiter = HandshakeRateLimiter::new(100, Duration::from_secs(10))
            .with_max_entries(4);
        for i in 0..8u8 {
            let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, i));
            limiter.allow(&ip);
        }
        assert!(limiter.len() <= 4);
    }

    #[test]
    fn test_ipv6_same_prefix64_distinct_buckets() {
        let mut limiter = HandshakeRateLimiter::new(1, Duration::from_secs(10));
        let ip1 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
        let ip2 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2));
        assert!(limiter.allow(&ip1));
        assert!(limiter.allow(&ip2));
        assert!(!limiter.allow(&ip1));
    }
}
