//! Implementation modules for the tiered JIT compiler.
//!
//! This module contains the internal implementation details of the tiered JIT,
//! organized into focused submodules for maintainability.

pub mod api;
pub mod cache;
pub mod execution;
pub mod execution_tiers;
pub mod promises;
pub mod types;

// Re-export commonly used types
pub use api::{tiered_jit_run, tiered_jit_run_with_opt};
pub use execution_tiers::{ControlFlow, JitFrame};
pub use types::{CachedModule, FunctionProfile, OptLevel, Tier, TierThresholds, TieredJitStats};
