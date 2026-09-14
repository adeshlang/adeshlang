//! ATP Error Model
//!
//! Distinguishes transport, protocol, security, connection, stream,
//! message, timeout, and memory errors as specified in the ATP design.

use std::fmt;

/// Top-level ATP error categories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtpError {
    /// Generic transport-level failure.
    Transport(String),
    /// Protocol violation (malformed frame, invalid state transition, etc.).
    Protocol(String),
    /// Security failure (authentication, decryption, replay, etc.).
    Security(String),
    /// Connection-level failure (closed, handshake failed, migration failed).
    Connection(String),
    /// Stream-level failure (reset, limit exceeded, invalid ID).
    Stream(String),
    /// Message-level failure (too large, cancelled, expired).
    Message(String),
    /// Timeout (handshake, idle, retransmission, path validation).
    Timeout(String),
    /// Memory budget exceeded.
    Memory(String),
}

impl AtpError {
    pub fn transport(msg: impl Into<String>) -> Self { AtpError::Transport(msg.into()) }
    pub fn protocol(msg: impl Into<String>) -> Self { AtpError::Protocol(msg.into()) }
    pub fn security(msg: impl Into<String>) -> Self { AtpError::Security(msg.into()) }
    pub fn connection(msg: impl Into<String>) -> Self { AtpError::Connection(msg.into()) }
    pub fn stream(msg: impl Into<String>) -> Self { AtpError::Stream(msg.into()) }
    pub fn message(msg: impl Into<String>) -> Self { AtpError::Message(msg.into()) }
    pub fn timeout(msg: impl Into<String>) -> Self { AtpError::Timeout(msg.into()) }
    pub fn memory(msg: impl Into<String>) -> Self { AtpError::Memory(msg.into()) }

    /// Human-readable message.
    pub fn message_str(&self) -> &str {
        match self {
            AtpError::Transport(m) | AtpError::Protocol(m) | AtpError::Security(m)
            | AtpError::Connection(m) | AtpError::Stream(m) | AtpError::Message(m)
            | AtpError::Timeout(m) | AtpError::Memory(m) => m,
        }
    }

    /// Short category name for diagnostics.
    pub fn category(&self) -> &'static str {
        match self {
            AtpError::Transport(_) => "TransportError",
            AtpError::Protocol(_) => "ProtocolError",
            AtpError::Security(_) => "SecurityError",
            AtpError::Connection(_) => "ConnectionError",
            AtpError::Stream(_) => "StreamError",
            AtpError::Message(_) => "MessageError",
            AtpError::Timeout(_) => "TimeoutError",
            AtpError::Memory(_) => "MemoryError",
        }
    }
}

impl fmt::Display for AtpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.category(), self.message_str())
    }
}

impl std::error::Error for AtpError {}

impl From<std::io::Error> for AtpError {
    fn from(e: std::io::Error) -> Self {
        AtpError::Transport(e.to_string())
    }
}

/// Result alias used throughout the ATP engine.
pub type AtpResult<T> = Result<T, AtpError>;
