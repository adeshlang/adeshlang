//! Runtime Core Module
//!
//! Core runtime functionality for language execution

pub mod arc_dispatch;
pub mod builtins;
pub mod bytecode;
pub mod exec;
pub mod flow;
pub mod format;
pub mod ops;
pub mod pretty_print;

// Interpreter submodules
mod interpreter;
pub use interpreter::*;

// Export arc_bridge for AOT runtime linking
pub use interpreter::arc_bridge;

// Interpreter implementation modules (Phase 4A refactoring)
pub mod interpreter_impl;

// Main interpreter core implementation
mod interpreter_core;
pub use interpreter_core::*;

pub mod fast_print;
pub mod stdio;
