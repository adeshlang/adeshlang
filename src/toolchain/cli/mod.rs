//! CLI Configuration Module
//!
//! This module provides comprehensive configuration options for the Adesh runtime,
//! including execution backends, optimization levels, memory models, and IR dumping.

pub mod args;
pub mod crypto;
pub mod doctor;
pub mod env;
pub mod help;
// `adesh toolchain install` / `adesh repair`: native-only installers
// (TLS downloads, archive extraction, OS process replacement).
#[cfg(not(target_arch = "wasm32"))]
pub mod install;
pub mod pkg;
#[cfg(not(target_arch = "wasm32"))]
pub mod repair;
pub mod toolchain;

pub use args::*;
pub use crypto::*;
pub use doctor::*;
pub use env::*;
pub use help::*;
#[cfg(not(target_arch = "wasm32"))]
pub use install::*;
pub use pkg::*;
#[cfg(not(target_arch = "wasm32"))]
pub use repair::*;
pub use toolchain::*;

// Re-export config types from parent toolchain module
pub use super::config::*;
