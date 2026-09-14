//! Threading and Concurrency
//!
//! Language-facing APIs live in `runtime::stdlib_src::concurrency`.
//! This module re-exports the native core plus a thin RAII thread handle
//! for Rust-side embedding.

pub use crate::runtime::stdlib_src::concurrency::*;
pub use crate::runtime::thread::{
    ThreadId, hardware_concurrency, park, park_timeout, sleep, yield_now,
};

/// Rust-embedding thread handle (not the AdeshLang `thread` namespace).
pub struct Thread {
    inner: std::thread::JoinHandle<()>,
}

impl Thread {
    pub fn spawn<F>(f: F) -> Thread
    where
        F: FnOnce() + Send + 'static,
    {
        Thread {
            inner: std::thread::spawn(f),
        }
    }

    pub fn join(self) {
        let _ = self.inner.join();
    }
}
