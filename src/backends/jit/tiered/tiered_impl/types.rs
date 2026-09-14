//! Core types and data structures for the tiered JIT compiler.
//!
//! This module defines the fundamental types used throughout the tiered JIT system,
//! including tier levels, profiling data, optimization levels, and statistics.

use crate::backends::jit::builtins::RuntimeValue;
use crate::backends::jit::lir::LirModule;
use crate::utils::collections::FastMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Tier levels for function compilation.
///
/// Functions progress through tiers as they become "hot" (frequently executed):
/// - Interpreter: Cold code, no compilation overhead
/// - Baseline: Warm code, fast compilation with minimal optimization
/// - Optimizing: Hot code, aggressive optimization for maximum performance
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Tier 0: Interpreter execution
    Interpreter = 0,
    /// Tier 1: Baseline JIT with minimal optimization
    Baseline = 1,
    /// Tier 2: Optimizing JIT with aggressive optimization
    Optimizing = 2,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Interpreter => write!(f, "T0-Interpreter"),
            Tier::Baseline => write!(f, "T1-Baseline"),
            Tier::Optimizing => write!(f, "T2-Optimizing"),
        }
    }
}

/// Thresholds for tier promotion decisions.
///
/// Controls when functions are promoted from one tier to another based on
/// execution frequency and profiling data.
#[derive(Debug, Clone)]
pub struct TierThresholds {
    /// Calls needed to promote from Interpreter to Baseline JIT
    pub baseline_threshold: u64,
    /// Calls needed to promote from Baseline to Optimizing JIT
    pub optimizing_threshold: u64,
    /// Enable profiling for tier decisions
    pub enable_profiling: bool,
}

impl Default for TierThresholds {
    fn default() -> Self {
        TierThresholds {
            baseline_threshold: 10,    // Promote to baseline after 10 calls
            optimizing_threshold: 100, // Promote to optimizing after 100 calls
            enable_profiling: true,
        }
    }
}

/// Function execution metadata for tier decisions.
///
/// Tracks execution statistics and characteristics of each function to make
/// informed decisions about tier promotion.
#[derive(Debug)]
pub struct FunctionProfile {
    /// Number of times the function was called
    pub call_count: AtomicU64,
    /// Current compilation tier
    pub current_tier: Tier,
    /// Estimated cost (instruction count)
    pub instruction_count: usize,
    /// Is this function a loop body (promotes faster)
    pub is_loop_body: bool,
    /// Is this function recursive (benefits from optimization)
    pub is_recursive: bool,
}

impl FunctionProfile {
    /// Create a new function profile with the given instruction count.
    pub fn new(instruction_count: usize) -> Self {
        FunctionProfile {
            call_count: AtomicU64::new(0),
            current_tier: Tier::Interpreter,
            instruction_count,
            is_loop_body: false,
            is_recursive: false,
        }
    }

    /// Record a call and check if promotion is needed.
    ///
    /// Returns `Some(new_tier)` if the function should be promoted to a higher tier.
    pub fn record_call(&self, thresholds: &TierThresholds) -> Option<Tier> {
        let count = self.call_count.fetch_add(1, Ordering::Relaxed) + 1;

        match self.current_tier {
            Tier::Interpreter if count >= thresholds.baseline_threshold => Some(Tier::Baseline),
            Tier::Baseline if count >= thresholds.optimizing_threshold => Some(Tier::Optimizing),
            _ => None,
        }
    }
}

/// Cached compiled module with Arc for efficient sharing.
///
/// Modules are compiled once and cached for reuse across multiple imports.
#[derive(Clone)]
pub struct CachedModule {
    /// The compiled LIR module
    pub lir: Arc<LirModule>,
    /// Namespace object containing module exports
    pub namespace: Arc<FastMap<String, RuntimeValue>>,
}

/// Optimization level for the tiered JIT.
///
/// Controls the aggressiveness of optimizations applied at all tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    /// No optimization (debug mode)
    O0,
    /// Basic optimizations
    O1,
    /// Moderate optimizations
    O2,
    /// Aggressive optimizations
    O3,
}

impl Default for OptLevel {
    fn default() -> Self {
        OptLevel::O2
    }
}

/// Statistics for tiered JIT execution.
///
/// Tracks execution metrics across all tiers for performance analysis and tuning.
#[derive(Debug, Default)]
pub struct TieredJitStats {
    /// Total function calls
    pub total_calls: u64,
    /// Calls executed in interpreter
    pub interpreter_calls: u64,
    /// Calls executed in baseline JIT
    pub baseline_calls: u64,
    /// Calls executed in optimizing JIT
    pub optimizing_calls: u64,
    /// Number of tier promotions
    pub promotions: u64,
    /// Total compilation time (microseconds)
    pub compile_time_us: u64,
}

impl TieredJitStats {
    /// Print a summary of execution statistics.
    pub fn print_summary(&self) {
        println!("=== Tiered JIT Statistics ===");
        println!("Total calls: {}", self.total_calls);
        println!(
            "  Interpreter: {} ({:.1}%)",
            self.interpreter_calls,
            100.0 * self.interpreter_calls as f64 / self.total_calls.max(1) as f64
        );
        println!(
            "  Baseline JIT: {} ({:.1}%)",
            self.baseline_calls,
            100.0 * self.baseline_calls as f64 / self.total_calls.max(1) as f64
        );
        println!(
            "  Optimizing JIT: {} ({:.1}%)",
            self.optimizing_calls,
            100.0 * self.optimizing_calls as f64 / self.total_calls.max(1) as f64
        );
        println!("Tier promotions: {}", self.promotions);
        println!("Compile time: {} μs", self.compile_time_us);
    }
}
