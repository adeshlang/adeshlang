//! Interpreter implementation modules
//!
//! This package contains extracted modules from the monolithic interpreter_core.rs
//! to improve code organization and maintainability.
//!
//! # Phase 4A & 4B & 4E & 4F & 4G & 4H Refactoring
//!
//! These modules are part of the Phase 4A/4B/4E/4F/4G/4H architectural refactoring to break down
//! the large interpreter_core.rs file (~16,000 lines) into smaller, focused modules.
//!
//! ## Modules
//!
//! - `scope_management`: Scope/environment handling and variable resolution
//! - `builtins`: Builtin method implementations for core types
//! - `utilities`: Standalone utility helper functions (type coercion, path resolution, etc.)
//! - `statement_helpers`: Statement execution helper functions (Phase 4F)
//! - `type_helpers`: Type checking and validation utilities (Phase 4G)
//! - `error_helpers`: Error formatting and context building utilities (Phase 4H)
//!
//! ## Future Modules (Blocked by Circular Dependencies)
//!
//! - `async_runtime`: Promise/microtask/timer management (requires major redesign)
//! - `expression_eval`: Expression evaluation logic (requires trait refactoring)
//! - `statement_exec`: Statement execution logic (requires trait refactoring)

pub mod builtins;
pub mod error_helpers;
pub mod scope_management;
pub mod statement_helpers;
pub mod type_helpers;
pub mod utilities;
