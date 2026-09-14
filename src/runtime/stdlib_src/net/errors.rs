//! AdeshLang Net Errors
//!
//! Categorized networking errors with retryability flags and native OS details.

use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkErrorKind {
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    ConnectionTimedOut,
    HostUnreachable,
    NetworkUnreachable,
    AddressInUse,
    AddressNotAvailable,
    BrokenPipe,
    UnexpectedEOF,
    InvalidAddress,
    InvalidPort,
    PermissionDenied,
    Unsupported,
    WouldBlock,
    Interrupted,
    SecurityPolicyViolation,
    ResourceLimitExceeded,
    Unknown,
}

impl NetworkErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConnectionRefused => "ConnectionRefused",
            Self::ConnectionReset => "ConnectionReset",
            Self::ConnectionAborted => "ConnectionAborted",
            Self::ConnectionTimedOut => "ConnectionTimedOut",
            Self::HostUnreachable => "HostUnreachable",
            Self::NetworkUnreachable => "NetworkUnreachable",
            Self::AddressInUse => "AddressInUse",
            Self::AddressNotAvailable => "AddressNotAvailable",
            Self::BrokenPipe => "BrokenPipe",
            Self::UnexpectedEOF => "UnexpectedEOF",
            Self::InvalidAddress => "InvalidAddress",
            Self::InvalidPort => "InvalidPort",
            Self::PermissionDenied => "PermissionDenied",
            Self::Unsupported => "Unsupported",
            Self::WouldBlock => "WouldBlock",
            Self::Interrupted => "Interrupted",
            Self::SecurityPolicyViolation => "SecurityPolicyViolation",
            Self::ResourceLimitExceeded => "ResourceLimitExceeded",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionTimedOut | Self::WouldBlock | Self::Interrupted | Self::ConnectionReset
        )
    }

    pub fn from_io_error(err: &std::io::Error) -> Self {
        use std::io::ErrorKind;
        match err.kind() {
            ErrorKind::NotFound => Self::InvalidAddress,
            ErrorKind::PermissionDenied => Self::PermissionDenied,
            ErrorKind::ConnectionRefused => Self::ConnectionRefused,
            ErrorKind::ConnectionReset => Self::ConnectionReset,
            ErrorKind::ConnectionAborted => Self::ConnectionAborted,
            ErrorKind::NotConnected => Self::HostUnreachable,
            ErrorKind::AddrInUse => Self::AddressInUse,
            ErrorKind::AddrNotAvailable => Self::AddressNotAvailable,
            ErrorKind::BrokenPipe => Self::BrokenPipe,
            ErrorKind::AlreadyExists => Self::AddressInUse,
            ErrorKind::WouldBlock => Self::WouldBlock,
            ErrorKind::InvalidInput => Self::InvalidAddress,
            ErrorKind::InvalidData => Self::InvalidAddress,
            ErrorKind::TimedOut => Self::ConnectionTimedOut,
            ErrorKind::Interrupted => Self::Interrupted,
            ErrorKind::UnexpectedEof => Self::UnexpectedEOF,
            ErrorKind::Unsupported => Self::Unsupported,
            _ => Self::Unknown,
        }
    }
}

pub fn make_net_error(
    kind: NetworkErrorKind,
    message: impl Into<String>,
    op: Option<&str>,
    target: Option<&str>,
) -> Value {
    let mut map = FastMap::default();
    map.insert("kind".to_string(), Value::Str(kind.as_str().to_string()));
    map.insert("message".to_string(), Value::Str(message.into()));
    map.insert("retryable".to_string(), Value::Bool(kind.is_retryable()));
    map.insert(
        "operation".to_string(),
        Value::Str(op.unwrap_or("none").to_string()),
    );
    map.insert(
        "target".to_string(),
        Value::Str(target.unwrap_or("").to_string()),
    );
    Value::Object(Arc::new(map))
}
