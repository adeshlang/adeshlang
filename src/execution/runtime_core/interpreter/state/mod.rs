//! State Management Module
//!
//! This module provides focused state management components for the interpreter.
//! It extracts state concerns from the monolithic Interpreter struct into
//! smaller, testable, and maintainable modules.
//!
//! ## Modules
//!
//! - `scope_stack`: Manages scope hierarchy and variable storage
//! - `call_stack`: Tracks function call stack for debugging and error reporting
//! - `variables`: Provides variable lookup and declaration APIs
//! - `closures`: Handles closure capture and management
//!
//! ## Architecture Benefits
//!
//! 1. **Separation of Concerns**: Each module handles one aspect of state
//! 2. **Enforced Invariants**: Clear APIs prevent invalid state transitions
//! 3. **Testability**: Each component can be tested independently
//! 4. **Maintainability**: Easier to understand and modify individual concerns
//! 5. **Foundation for Optimization**: Enables inline caching and other optimizations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::execution::runtime_core::interpreter::state::{
//!     ScopeStack, CallStack, VariableManager, ClosureManager
//! };
//!
//! // Create state components
//! let mut scopes = ScopeStack::new();
//! let mut call_stack = CallStack::new(1000); // max depth
//!
//! // Push a scope
//! let scope_id = scopes.push(None)?;
//!
//! // Declare variable
//! scopes.declare(scope_id, "x".to_string(), Value::Number(42.0))?;
//!
//! // Push call frame
//! call_stack.push("my_function".to_string())?;
//! ```

pub mod call_stack;
pub mod closures;
pub mod scope_stack;
pub mod variables;

// Re-export main types for convenience
pub use call_stack::CallStack;
pub use closures::ClosureManager;
pub use scope_stack::ScopeStack;
pub use variables::VariableManager;
