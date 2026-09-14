//! Compilation tier definitions and thresholds
//!
//! This module defines the three-tier compilation strategy and the thresholds
//! that control promotion between tiers based on runtime profiling data.

/// Compilation tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Tier 0: Pure interpreter with profiling
    Interpreter = 0,
    /// Tier 1: Baseline JIT with minimal optimization
    Baseline = 1,
    /// Tier 2: Optimizing JIT with speculative optimizations
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

/// Thresholds for tier promotion
#[derive(Debug, Clone)]
pub struct AdaptiveThresholds {
    /// Calls to promote from Interpreter to Baseline
    pub baseline_threshold: u64,
    /// Calls to promote from Baseline to Optimizing
    pub optimizing_threshold: u64,
    /// Loop iterations to trigger OSR
    pub osr_threshold: u64,
    /// Hot call site threshold for inlining consideration
    pub inline_threshold: u64,
    /// Maximum inlining depth
    pub max_inline_depth: u32,
    /// Maximum code size for inlining
    pub max_inline_size: usize,
}

impl Default for AdaptiveThresholds {
    fn default() -> Self {
        AdaptiveThresholds {
            baseline_threshold: 10,
            optimizing_threshold: 100,
            osr_threshold: 1000,
            inline_threshold: 50,
            max_inline_depth: 4,
            max_inline_size: 100,
        }
    }
}
