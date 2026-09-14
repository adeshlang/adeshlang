//! Runtime SIMD operations — high-performance vectorized array math
//!
//! Provides:
//! - Element-wise array operations (a + b, a * scalar, etc.)
//! - Expression fusion (sqrt((a * b) + c) in one pass)
//! - Broadcasting (array + scalar)
//! - SIMD-optimized paths with scalar fallback
//! - Reductions (sum, min, max, mean)

pub mod broadcast;
pub mod cpu_features;
pub mod fusion;
pub mod ops;
pub mod value;

pub use broadcast::*;
pub use cpu_features::*;
pub use fusion::*;
pub use ops::*;
pub use value::*;
