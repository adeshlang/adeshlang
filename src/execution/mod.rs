//! Execution Module - Runtime Core
//!
//! This module contains the execution components:
//! - Runtime Core: Runtime builtins and execution logic

pub mod runtime_core;

// Re-export commonly used types
pub use runtime_core as runtime;

// Legacy compatibility - re-export runtime at top level
pub use runtime_core::*;
