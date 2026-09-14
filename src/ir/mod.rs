//! IR Module - Intermediate Representations
//!
//! This module contains the compiler's IR layers:
//! - HIR: High-level IR from parsing (legacy, re-exported from parsing module)
//! - MIR: Memory IR with ownership/borrow analysis (compile-time memory safety)
//! - VIR: Value IR - backend-neutral SSA form (consumed by all backends)
//! - Optimizations: VIR optimization passes

pub mod mir;
pub mod optimizations;
pub mod parallel;
pub mod simd;
pub mod vir;

// Re-export IR components from parsing module (legacy HIR)
pub use crate::parsing::ast_optimizer;
pub use crate::parsing::hir;
pub use crate::parsing::hir_lower;
pub use crate::parsing::hir_passes;

// Re-export commonly used types
pub use crate::parsing::hir::*;

// Export new IR types
pub use mir::MirModule;
pub use optimizations::*;
pub use vir::VirModule;

// SIMD and parallel IR (explicit exports to avoid glob name collisions)
pub use parallel::analysis as parallel_analysis;
pub use parallel::cost_model as parallel_cost_model;
pub use parallel::transform as parallel_transform;
pub use simd::analysis as simd_analysis;
pub use simd::cost_model as simd_cost_model;
pub use simd::instructions as simd_instructions;
pub use simd::lowering as simd_lowering;
pub use simd::types as simd_types;
pub use simd::vectorize as simd_vectorize;
