//! Execution engine module for the language interpreter.
//!
//! This module provides the core execution functionality, organized into focused submodules:
//! - `core`: Main Exec struct and scope management
//! - `stmt`: Statement execution logic
//! - `expression_eval`: Modular expression evaluation organized by expression type
//!
//! Additional modules for future extraction:
//! - `helpers`: Utility methods (stub)

pub mod core;
pub mod expression_eval;
pub mod helpers;
pub mod iterative_eval;
pub mod stmt;

// Re-export the main struct and commonly used items
pub use core::Exec;
pub use stmt::is_copy_value;
