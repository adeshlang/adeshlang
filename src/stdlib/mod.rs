//! Standard Library Module
//!
//! The Adesh standard library is organized into three layers:
//!
//! ## Layer 1: adesh_core
//! - No allocator, no OS, no heap
//! - Primitive traits, Option, Result, iterators
//! - Usable on GPU and in no_std environments
//!
//! ## Layer 2: adesh_alloc
//! - Heap-dependent but OS-independent
//! - ARC, Vec, String, HashMap, Box
//! - No garbage collection
//!
//! ## Layer 3: adesh_std
//! - Full standard library with OS dependencies
//! - I/O, networking, threading, async, process management
//!
//! ## Backward Compatibility
//! The old runtime stdlib is still available and re-exported.

pub mod adesh_alloc;
pub mod adesh_core;
pub mod adesh_std;
pub mod integration;

// Backward compatibility: re-export runtime stdlib
pub use crate::runtime::stdlib::*;

// Re-export integration utilities
pub use integration::{
    EcosystemMetadata, validate_before_execution, validate_ecosystem_integration,
};
