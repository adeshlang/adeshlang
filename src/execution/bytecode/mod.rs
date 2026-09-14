//! Bytecode Module
//!
//! Contains bytecode definitions and array operation optimizations

pub mod array_ops;
pub mod bytecode_old;

// Re-export bytecode types for backwards compatibility
pub use bytecode_old::*;
