//! Expression Evaluation Helpers (Phase 3: PR3)
//!
//! This module provides focused helper functions for expression evaluation,
//! reducing code duplication and providing optimization opportunities.
//!
//! ## Architecture
//!
//! The eval module is organized to support focused, testable evaluation logic:
//!
//! - `helpers.rs`: Common utilities (error messages, type checks, value conversion)
//!
//! ## Future Modules (PR3 continuation)
//!
//! Additional modules planned for full expression splitting:
//!
//! - `binary.rs`: Binary operations (+, -, *, /, etc.)
//! - `unary.rs`: Unary operations (!, -, typeof, etc.)
//! - `calls.rs`: Function and method calls
//! - `construction.rs`: Array, object, tuple literal construction
//! - `access.rs`: Property and index access
//! - `assignment.rs`: Variable assignment operations
//! - `control.rs`: Control flow (match, ternary, etc.)
//!
//! ## Usage
//!
//! These helpers are used throughout the interpreter to:
//!
//! 1. **Reduce allocations**: Optimized string operations and type checks
//! 2. **Improve error messages**: Consistent, clear error formatting
//! 3. **Centralize common logic**: Single place to optimize patterns
//!
//! ## Example
//!
//! ```rust,ignore
//! use crate::execution::runtime_core::interpreter::eval::helpers::*;
//!
//! // Type checking
//! if !is_numeric(&value) {
//!     return Err(type_mismatch_error("addition", "number", &value));
//! }
//!
//! // Truthiness
//! if is_truthy(&condition) {
//!     // execute then branch
//! }
//! ```

pub mod helpers;

pub use helpers::{
    arity_error, err, err_with_context, format_value_type, is_callable, is_numeric, is_string,
    is_truthy, type_mismatch_error, value_to_display_string, values_equal,
};
