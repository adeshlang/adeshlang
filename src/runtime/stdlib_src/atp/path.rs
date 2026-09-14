//! Path validation and connection migration state machine.

use super::errors::{AtpError, AtpResult};
use rand::rngs::OsRng;
use rand::RngCore;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationState {
    /// Using the validated active path only.
    Stable,
    /// PATH_CHALLENGE sent; awaiting PATH_RESPONSE from candidate.
    Validating,
    /// PATH_RESPONSE verified; active address updated.
    Completed,
}

#[derive(Debug, Clone)]
pub struct ValidatedPath {
    pub address: SocketAddr,
    pub validated_at: Instant,
}

/// Manages active and candidate paths for a connection.
#[derive(Debug)]
pub struct PathManager {
    pub active: ValidatedPath,
    pub migration_state: MigrationState,
    pub pending_challenge: Option<([u8; 8], SocketAddr, Instant)>,
    pub validation_timeout: Duration,
    pub previous_path: Option<ValidatedPath>,
}

impl PathManager {
    pub fn new(initial_addr: SocketAddr, now: Instant) -> Self {
        PathManager {
            active: ValidatedPath {
                address: initial_addr,
                validated_at: now,
            },
            migration_state: MigrationState::Stable,
            pending_challenge: None,
            validation_timeout: Duration::from_secs(5),
            previous_path: None,
        }
    }

    /// Begin validating a new peer address. Returns challenge bytes to send.
    pub fn begin_validation(&mut self, new_addr: SocketAddr, now: Instant) -> AtpResult<[u8; 8]> {
        if self.migration_state == MigrationState::Validating {
            return Err(AtpError::protocol("migration already in progress"));
        }
        if new_addr == self.active.address {
            return Err(AtpError::protocol("cannot migrate to same address"));
        }
        let mut challenge = [0u8; 8];
        OsRng.fill_bytes(&mut challenge);
        self.pending_challenge = Some((challenge, new_addr, now));
        self.migration_state = MigrationState::Validating;
        Ok(challenge)
    }

    /// Process PATH_RESPONSE; returns new active address if validated.
    ///
    /// Invalid responses are ignored (challenge state preserved).
    /// Timeout aborts migration back to Stable.
    pub fn on_path_response(
        &mut self,
        data: [u8; 8],
        from_addr: SocketAddr,
        now: Instant,
    ) -> AtpResult<Option<SocketAddr>> {
        let Some((expected, target, started)) = self.pending_challenge else {
            return Err(AtpError::protocol("unexpected PATH_RESPONSE"));
        };

        if self.migration_state != MigrationState::Validating {
            return Err(AtpError::protocol("not in validating state"));
        }

        if now.duration_since(started) > self.validation_timeout {
            self.abort_migration();
            return Err(AtpError::timeout("path validation timed out"));
        }

        if from_addr != target || data != expected {
            // Transient bad packet — remain in Validating with challenge intact.
            return Ok(None);
        }

        self.pending_challenge = None;
        self.previous_path = Some(self.active.clone());
        self.active = ValidatedPath {
            address: target,
            validated_at: now,
        };
        self.migration_state = MigrationState::Completed;
        Ok(Some(target))
    }

    /// Mark migration stable after successful switch.
    pub fn confirm_migration(&mut self) {
        if self.migration_state == MigrationState::Completed {
            self.migration_state = MigrationState::Stable;
            self.previous_path = None;
        }
    }

    /// Cancel or fail an in-progress migration, returning to Stable.
    pub fn abort_migration(&mut self) {
        self.pending_challenge = None;
        if self.migration_state == MigrationState::Validating {
            self.migration_state = MigrationState::Stable;
        }
    }

    /// Whether application data from `addr` should be accepted.
    pub fn accepts_active(&self, addr: SocketAddr) -> bool {
        addr == self.active.address
    }

    /// Whether `addr` is the pending validation target (PATH_RESPONSE only).
    pub fn is_validation_target(&self, addr: SocketAddr) -> bool {
        self.pending_challenge
            .map(|(_, target, _)| target == addr)
            .unwrap_or(false)
    }

    pub fn active_addr(&self) -> SocketAddr {
        self.active.address
    }

    pub fn is_migrating(&self) -> bool {
        self.migration_state == MigrationState::Validating
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_path_validation_success() {
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9000);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9001);
        let now = Instant::now();
        let mut pm = PathManager::new(addr1, now);
        let challenge = pm.begin_validation(addr2, now).unwrap();
        assert_eq!(pm.migration_state, MigrationState::Validating);
        let migrated = pm.on_path_response(challenge, addr2, now).unwrap();
        assert_eq!(migrated, Some(addr2));
        assert_eq!(pm.active_addr(), addr2);
        pm.confirm_migration();
        assert_eq!(pm.migration_state, MigrationState::Stable);
    }

    #[test]
    fn test_bad_response_stays_validating() {
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9000);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9001);
        let addr3 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9002);
        let now = Instant::now();
        let mut pm = PathManager::new(addr1, now);
        let challenge = pm.begin_validation(addr2, now).unwrap();
        assert!(pm.on_path_response(challenge, addr3, now).unwrap().is_none());
        assert_eq!(pm.migration_state, MigrationState::Validating);
        assert!(pm.pending_challenge.is_some());
        // Valid response still works after bad packet.
        let migrated = pm.on_path_response(challenge, addr2, now).unwrap();
        assert_eq!(migrated, Some(addr2));
    }

    #[test]
    fn test_validation_target_not_active() {
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9000);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9001);
        let now = Instant::now();
        let mut pm = PathManager::new(addr1, now);
        assert!(pm.accepts_active(addr1));
        assert!(!pm.accepts_active(addr2));
        let _ = pm.begin_validation(addr2, now).unwrap();
        assert!(pm.accepts_active(addr1));
        assert!(!pm.accepts_active(addr2));
        assert!(pm.is_validation_target(addr2));
    }
}
