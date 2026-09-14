//! Error Formatting Helper Functions
//!
//! This module contains error handling and formatting utilities for the interpreter.
//! Extracted from interpreter_core.rs as part of Phase 4H refactoring.
//!
//! # Current State
//!
//! Error handling in the language is already well-modularized:
//! - `LangError`: Structured error type defined in `parsing::error`
//! - `IndiaError`: Type alias for LangError
//! - `RunErr`: Enum wrapping LangError or simple string messages
//! - Error formatting is handled by Display impl on LangError
//!
//! # Design
//!
//! The language uses structured error handling with:
//! - Line/column information for precise error location
//! - Error kinds (TypeError, RuntimeError, etc.)
//! - File/module context
//! - Colored output for terminal display
//!
//! Most error construction happens inline in interpreter_core.rs using the `err()` macro
//! and utility functions from the `interpreter` module. This module serves as a placeholder
//! for future error formatting utilities if needed.
//!
//! # Future Enhancements
//!
//! Potential additions for Phase 5+:
//! - Error context builders for complex scenarios
//! - Stack trace formatting utilities
//! - Error recovery suggestions
//! - Multi-error aggregation for batch validation

use crate::parsing::error::LangError;

/// Format a simple error message with context
///
/// This is a basic helper that could be expanded in the future.
/// Currently, error formatting is handled by LangError's Display impl.
#[inline]
pub fn format_simple_error(message: impl Into<String>) -> String {
    message.into()
}

/// Build error context string from module and location info
///
/// Helper for constructing consistent error context strings.
#[inline]
pub fn build_error_context(module_id: &str, line: Option<usize>, col: Option<usize>) -> String {
    match (line, col) {
        (Some(l), Some(c)) => format!("[at {}:{}:{}]", module_id, l, c),
        (Some(l), None) => format!("[at {}:{}]", module_id, l),
        _ => format!("[at {}]", module_id),
    }
}

/// Enhance error message with module context
///
/// Appends module/file context to an error message for better debugging.
#[inline]
pub fn with_module_context(error_msg: String, module_id: &str) -> String {
    if error_msg.contains("[at ") {
        // Already has context
        error_msg
    } else {
        format!("{}\n[at {}]", error_msg, module_id)
    }
}

/// Extract error message from LangError for simple display
///
/// Useful when you need just the message without full error formatting.
#[inline]
pub fn extract_error_message(err: &LangError) -> String {
    err.message.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_simple_error() {
        let err = format_simple_error("test error");
        assert_eq!(err, "test error");
    }

    #[test]
    fn test_build_error_context() {
        assert_eq!(
            build_error_context("module.adesh", Some(10), Some(5)),
            "[at module.adesh:10:5]"
        );
        assert_eq!(
            build_error_context("module.adesh", Some(10), None),
            "[at module.adesh:10]"
        );
        assert_eq!(
            build_error_context("module.adesh", None, None),
            "[at module.adesh]"
        );
    }

    #[test]
    fn test_with_module_context() {
        let msg = with_module_context("error occurred".to_string(), "test.adesh");
        assert_eq!(msg, "error occurred\n[at test.adesh]");

        // Already has context - don't add again
        let msg2 = with_module_context("error [at other.adesh]".to_string(), "test.adesh");
        assert_eq!(msg2, "error [at other.adesh]");
    }
}
