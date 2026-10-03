//! Optimization level definitions and configuration.

use serde::{Deserialize, Serialize};

/// Optimization levels supported by the native code generation pipeline.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum OptLevel {
    /// No optimization: fastest compile times, straightforward debuggable code.
    O0,
    /// Basic optimizations: constant folding, copy propagation, DCE, basic peephole.
    O1,
    /// Production optimization: aggressive instruction selection, move coalescing,
    /// CSE, branch optimization, stack frame minimization, and LTO deduplication.
    #[default]
    O2,
    /// Maximum optimization: aggressive inlining, unrolling hints, loop optimizations.
    O3,
    /// Size optimization: prioritize compact binary size over minor speed gains.
    Os,
    /// Aggressive size optimization: eliminate all optional alignment and inlining.
    Oz,
}

impl OptLevel {
    /// True if any optimizations should be applied.
    pub fn is_optimized(&self) -> bool {
        !matches!(self, Self::O0)
    }

    /// True if aggressive optimizations (CSE, coalescing, loop weighting) are enabled.
    pub fn is_aggressive(&self) -> bool {
        matches!(self, Self::O2 | Self::O3 | Self::Os | Self::Oz)
    }

    /// True if size is prioritized over speed.
    pub fn is_size_optimized(&self) -> bool {
        matches!(self, Self::Os | Self::Oz)
    }

    /// Maximum number of iterative optimization pass cycles.
    pub fn max_pass_iterations(&self) -> usize {
        match self {
            Self::O0 => 0,
            Self::O1 => 2,
            Self::O2 => 4,
            Self::O3 => 8,
            Self::Os | Self::Oz => 3,
        }
    }
}
