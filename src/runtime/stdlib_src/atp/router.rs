//! Connection router — routes incoming packets by connection ID.
//!
//! Packets are never associated with a connection merely because they share
//! a source address. Routing uses the destination (local) connection ID.

use super::connection::AtpConnection;
use super::errors::{AtpError, AtpResult};
use super::id::{ConnectionId, WireConnectionId};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Routes packets to connections by wire connection ID.
pub struct ConnectionRouter {
    /// Established connections by internal ID.
    connections: HashMap<ConnectionId, AtpConnection>,
    /// Local CID → internal connection ID.
    by_local_cid: HashMap<[u8; 8], ConnectionId>,
    /// Pending handshakes by client source CID (server side).
    pending_by_client_cid: HashMap<[u8; 8], ConnectionId>,
}

impl ConnectionRouter {
    pub fn new() -> Self {
        ConnectionRouter {
            connections: HashMap::new(),
            by_local_cid: HashMap::new(),
            pending_by_client_cid: HashMap::new(),
        }
    }

    pub fn insert(&mut self, conn: AtpConnection) -> ConnectionId {
        let id = conn.internal_id;
        let local_cid = *conn.local_cid.as_bytes();
        self.by_local_cid.insert(local_cid, id);
        self.connections.insert(id, conn);
        id
    }

    pub fn register_pending(&mut self, client_cid: WireConnectionId, id: ConnectionId) {
        self.pending_by_client_cid
            .insert(*client_cid.as_bytes(), id);
    }

    pub fn get(&self, id: ConnectionId) -> Option<&AtpConnection> {
        self.connections.get(&id)
    }

    pub fn get_mut(&mut self, id: ConnectionId) -> Option<&mut AtpConnection> {
        self.connections.get_mut(&id)
    }

    pub fn remove(&mut self, id: ConnectionId) -> Option<AtpConnection> {
        if let Some(conn) = self.connections.remove(&id) {
            self.by_local_cid.remove(conn.local_cid.as_bytes());
            if let Some(remote) = &conn.remote_cid {
                self.pending_by_client_cid.remove(remote.as_bytes());
            }
            Some(conn)
        } else {
            None
        }
    }

    /// Find connection by local (destination) wire CID.
    pub fn find_by_local_cid(&self, cid: &[u8]) -> Option<ConnectionId> {
        if cid.len() != 8 {
            return None;
        }
        let mut key = [0u8; 8];
        key.copy_from_slice(cid);
        self.by_local_cid.get(&key).copied()
    }

    /// Find pending handshake by client CID.
    pub fn find_pending(&self, client_cid: &[u8]) -> Option<ConnectionId> {
        if client_cid.len() != 8 {
            return None;
        }
        let mut key = [0u8; 8];
        key.copy_from_slice(client_cid);
        self.pending_by_client_cid.get(&key).copied()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&ConnectionId, &mut AtpConnection)> {
        self.connections.iter_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ConnectionId, &AtpConnection)> {
        self.connections.iter()
    }

    pub fn len(&self) -> usize {
        self.connections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Update peer address for a connection (after path validation).
    pub fn update_peer_addr(
        &mut self,
        id: ConnectionId,
        addr: SocketAddr,
    ) -> AtpResult<()> {
        let conn = self
            .connections
            .get_mut(&id)
            .ok_or_else(|| AtpError::connection("connection not found"))?;
        conn.peer_addr = addr;
        Ok(())
    }
}

impl Default for ConnectionRouter {
    fn default() -> Self {
        Self::new()
    }
}
