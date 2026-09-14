//! Error types for the interpreter runtime.
//!
//! Defines error types used throughout the interpreter execution,
//! including language errors and runtime messages.

use crate::parsing::error::LangError;

/// Runtime error type for interpreter execution.
///
/// This enum represents errors that can occur during interpretation:
/// - `Msg`: Simple string error messages
/// - `Lang`: Structured language errors with spans and context
#[derive(Debug)]
pub enum RunErr {
    Msg(String),
    Lang(LangError),
}

impl std::fmt::Display for RunErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunErr::Msg(s) => write!(f, "{}", s),
            RunErr::Lang(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for RunErr {}

impl From<String> for RunErr {
    fn from(s: String) -> Self {
        RunErr::Msg(s)
    }
}

/// Alias for LangError - kept for backward compatibility
pub type IndiaError = LangError;
