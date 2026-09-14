//! Toolchain module - CLI, configuration, formatting, and documentation tools
//!
//! This module contains developer tools and command-line interface components:
//! - CLI: Command-line argument parsing and execution control
//! - Config: Runtime configuration management
//! - Formatter: Code formatting utilities
//! - Docgen: Documentation generation
//! - Env: Environment variable handling
//! - Timer: Performance timing utilities

pub mod archive;
pub mod cli;
pub mod config;
pub mod docgen;
pub mod download;
pub mod env;
pub mod expose;
pub mod formatter;
pub mod manifest;
pub mod resolver;
pub mod source_build;
pub mod timer;

// Re-export commonly used types
pub use cli::{ExecutionBackend, OptLevel, ParsedArgs, RuntimeConfig};
pub use config::*;
pub use docgen::*;
pub use env::*;
pub use formatter::*;
pub use resolver::*;
pub use timer::*;
