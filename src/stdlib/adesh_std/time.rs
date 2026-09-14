//! Time and Date Operations
//!
//! Timing, duration, and instant measurement

/// Re-export from runtime stdlib
pub use crate::runtime::stdlib_src::system::time::*;

/// An instant in time
#[derive(Clone, Copy, Debug)]
pub struct Instant {
    inner: std::time::Instant,
}

impl Instant {
    /// Returns the current instant
    pub fn now() -> Self {
        Instant {
            inner: std::time::Instant::now(),
        }
    }

    /// Returns the duration since this instant
    pub fn elapsed(&self) -> Duration {
        Duration {
            inner: self.inner.elapsed(),
        }
    }
}

/// A duration of time
#[derive(Clone, Copy, Debug)]
pub struct Duration {
    inner: std::time::Duration,
}

impl Duration {
    /// Creates a duration from seconds
    pub fn from_secs(secs: u64) -> Self {
        Duration {
            inner: std::time::Duration::from_secs(secs),
        }
    }

    /// Creates a duration from milliseconds
    pub fn from_millis(millis: u64) -> Self {
        Duration {
            inner: std::time::Duration::from_millis(millis),
        }
    }

    /// Returns the number of seconds
    pub fn as_secs(&self) -> u64 {
        self.inner.as_secs()
    }

    /// Returns the number of milliseconds
    pub fn as_millis(&self) -> u128 {
        self.inner.as_millis()
    }
}
