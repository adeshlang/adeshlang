//! Adesh Standard Library
//!
//! This is Layer 3 of the Adesh standard library.
//!
//! ## Principles
//! - Full standard library with OS dependencies
//! - Builds on adesh_core and adesh_alloc
//! - Provides platform abstraction
//! - Stable ABI for cross-version compatibility
//!
//! ## Contents
//! - File I/O
//! - Networking
//! - Threading and concurrency
//! - Time and date operations
//! - Async runtime
//! - Process management
//! - OS bindings abstraction

pub mod async_rt;
pub mod io;
#[cfg(not(target_arch = "wasm32"))]
pub mod net;
pub mod os;
pub mod process;
pub mod thread;
pub mod time;

// Re-export commonly used items
pub use io::{File, Read, Write};
pub use thread::Thread;
pub use time::Instant;
