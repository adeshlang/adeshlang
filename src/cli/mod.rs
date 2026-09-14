//! CLI module - Command-line interface components
//!
//! This module provides modularized CLI functionality including:
//! - Memory statistics reporting
//! - AST validation and heap allocation checking
//! - Source code parsing with safety checks
//! - Path resolution utilities
//! - AOT compilation utilities
//! - Backend execution runners
//! - IR dumping utilities
//! - Compile directive handling
//! - Command implementations
//! - FFI configuration

// Re-export toolchain CLI for backward compatibility
pub use crate::toolchain::cli::*;

// Submodules
pub mod aot_utils;
pub mod backends;
pub mod build;
pub mod build_args;
pub mod commands;
pub mod config;
pub mod directives;
pub mod editor_launcher;
pub mod ir_utils;
pub mod memory_stats;
pub mod native_engine;
pub mod parsing;
pub mod path_utils;
pub mod ui;
pub mod validation;

// Re-export key types and functions for convenience
pub use build_args::BuildArgParser;
pub use memory_stats::MemoryStats;
