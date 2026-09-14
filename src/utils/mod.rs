//! Utilities
//!
//! Shared utility modules used throughout the language:
//! - `timer`: thread-based timer management for async runtime (moved to toolchain)
//! - `docgen`: documentation generator for the language source files (moved to toolchain)
//! - `env`: environment variable helpers and configuration (moved to toolchain)
//! - `formatter`: code formatter for the language source (moved to toolchain)
//! - `collections`: optimized collection types (FastMap, FastSet)
//! - `interner`: string interning for efficient symbol storage
//! - `memory`: memory model infrastructure (smart pointers, allocators, SSO/SAO)

// Modules that remain in utils
pub mod collections;
pub mod interner;
pub mod memory;

// Re-exports from toolchain for backward compatibility
pub use crate::toolchain::docgen;
pub use crate::toolchain::env;
pub use crate::toolchain::formatter;
pub use crate::toolchain::timer;
