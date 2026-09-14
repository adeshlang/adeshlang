//! Execution Module - Bytecode VM and Runtime
//!
//! This module contains the execution components:
//! - Bytecode: Bytecode definitions and array optimizations
//! - VM: Virtual machine implementation
//! - Runtime Core: Runtime builtins and execution logic

pub mod bytecode;
pub mod runtime_core;
pub mod vm;

// Re-export commonly used types
pub use bytecode::*;
pub use runtime_core as runtime;
pub use vm::*;

// Legacy compatibility - re-export runtime at top level
pub use runtime_core::*;
