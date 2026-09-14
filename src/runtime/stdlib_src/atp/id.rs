//! Connection identity and handle types.
//!
//! ATP uses wire connection IDs for routing and internal connection IDs for
//! API/engine bookkeeping. Handles include a generation counter to detect
//! stale references after connection reuse.

use super::config::CONNECTION_ID_LEN;
use super::errors::{AtpError, AtpResult};
use std::fmt;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Internal engine connection identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

impl ConnectionId {
    pub fn new() -> Self {
        ConnectionId(NEXT_CONNECTION_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "conn:{}", self.0)
    }
}

/// 8-byte wire connection ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WireConnectionId(pub [u8; CONNECTION_ID_LEN]);

impl WireConnectionId {
    pub fn random() -> Self {
        let mut id = [0u8; CONNECTION_ID_LEN];
        for b in &mut id {
            *b = rand::random();
        }
        WireConnectionId(id)
    }

    pub fn from_slice(slice: &[u8]) -> AtpResult<Self> {
        if slice.len() != CONNECTION_ID_LEN {
            return Err(AtpError::protocol(format!(
                "connection ID must be {} bytes, got {}",
                CONNECTION_ID_LEN,
                slice.len()
            )));
        }
        let mut id = [0u8; CONNECTION_ID_LEN];
        id.copy_from_slice(slice);
        Ok(WireConnectionId(id))
    }

    pub fn as_bytes(&self) -> &[u8; CONNECTION_ID_LEN] {
        &self.0
    }
}

/// Generation counter for connection handle safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Generation(pub u64);

impl Generation {
    pub fn new() -> Self {
        Generation(NEXT_GENERATION.fetch_add(1, Ordering::SeqCst))
    }
}

/// API-visible connection handle with generation for stale detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionHandle {
    pub connection_id: ConnectionId,
    pub generation: Generation,
}

impl ConnectionHandle {
    pub fn new(connection_id: ConnectionId) -> Self {
        ConnectionHandle {
            connection_id,
            generation: Generation::new(),
        }
    }
}

/// Validated network path to a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub address: SocketAddr,
    pub validated: bool,
}

impl Path {
    pub fn new(address: SocketAddr) -> Self {
        Path {
            address,
            validated: true, // Initial path from handshake is validated by retry token / init
        }
    }

    pub fn unvalidated(address: SocketAddr) -> Self {
        Path {
            address,
            validated: false,
        }
    }
}
