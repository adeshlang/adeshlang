//! Adesh Diagnostics Subsystem
//!
//! Complete, production-quality, self-contained compiler diagnostics system for AdeshLang.
//! Inspired by the clarity and usability of Rust compiler errors, with stable error codes,
//! accurate source error tables, machine-readable JSON, and canonical explanations.

pub mod json;
pub mod model;
pub mod registry;
pub mod source_map;
pub mod terminal;

pub use json::{DIAGNOSTIC_SCHEMA_VERSION, DiagnosticJsonReport};
pub use model::{Diagnostic, Label, SourceId, Span, Suggestion};
pub use registry::{
    DIAGNOSTIC_REGISTRY, DiagnosticInfo, DiagnosticPhase, DiagnosticSeverity, RustCompatibility,
    explain_code, list_diagnostics, lookup_diagnostic,
};
pub use source_map::SourceMap;
pub use terminal::TerminalRenderer;
