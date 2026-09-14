//! Frontend Module - Lexing, parsing, AST, and diagnostics
//!
//! This module provides a clean interface to the frontend components.
//! The actual implementations are in the parsing module until full migration is complete.

// Re-export frontend components from parsing module
pub use crate::parsing::ast;
pub use crate::parsing::error;
pub use crate::parsing::lexer;
pub use crate::parsing::parser;

// Re-export commonly used types
pub use crate::parsing::ast::*;
pub use crate::parsing::error::*;
pub use crate::parsing::lexer::*;
pub use crate::parsing::parser::*;
