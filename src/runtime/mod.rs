//! Runtime Module
//!
//! This module contains runtime support for AdeshLang:
//! - Standard library (stdlib) implementations
//! - C runtime integration
//! - FFI is in backends/common/ffi (shared with codegen)
//! - System-level operations
//! - Unified ABI for consistent operation semantics across backends
//! - NaN-boxing value representation for memory efficiency
//! - ARC (Automatic Reference Counting) runtime for memory management

pub mod abi;
pub mod arc;
#[cfg(not(target_arch = "wasm32"))]
pub mod c_runtime;
pub mod nanvalue;
pub mod scheduler;
pub mod simd;
pub mod stdlib;
pub mod thread;
pub mod stdlib_src;
pub mod system;
pub mod tui_input;

// Re-export stdlib for easy access
pub use stdlib::*;

// Re-export ARC runtime
pub use arc::{
    arc_alloc, arc_alloc_with_drop, arc_clone, arc_get_count, arc_release, arc_retain,
    arc_set_drop_fn, ArcDropFn,
};
