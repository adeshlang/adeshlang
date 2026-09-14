//! Loop analysis for automatic SIMD vectorization

use super::types::{Alignment, SimdType};

/// Result of analyzing a loop for vectorization eligibility
#[derive(Debug, Clone)]
pub struct VectorizationCandidate {
    pub loop_id: u32,
    pub simd_type: SimdType,
    pub iteration_count: Option<u64>,
    pub stride: i64,
    pub has_reduction: bool,
    pub reduction_op: Option<ReductionOp>,
    pub alias_safe: bool,
    pub alignment: Alignment,
    pub reason_rejected: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReductionOp {
    Add,
    Mul,
    Min,
    Max,
    And,
    Or,
    Xor,
}

/// Loop dependency between memory accesses
#[derive(Debug, Clone)]
pub struct MemoryDependency {
    pub read_index: String,
    pub write_index: String,
    pub distance: i64,
    pub is_loop_carried: bool,
}

/// Analyze whether a loop can be safely vectorized
pub struct LoopVectorizer;

impl LoopVectorizer {
    /// Check if two array accesses may alias
    pub fn may_alias(a_var: &str, b_var: &str, ownership_disjoint: bool) -> bool {
        if a_var == b_var {
            return true;
        }
        // Ownership/borrow guarantees can prove disjointness
        !ownership_disjoint
    }

    /// Determine if loop is a vectorization candidate
    pub fn analyze_candidate(
        iteration_count: u64,
        element_type: &str,
        stride: i64,
        has_side_effects: bool,
        alias_safe: bool,
        min_iterations: u64,
    ) -> Option<VectorizationCandidate> {
        if has_side_effects || !alias_safe {
            return None;
        }
        if iteration_count < min_iterations {
            return None;
        }
        if stride != 1 {
            return None;
        }
        let elem = super::types::SimdElement::from_type_name(element_type)?;
        let lanes = 4u32; // default; cost model refines this
        Some(VectorizationCandidate {
            loop_id: 0,
            simd_type: SimdType::new(elem, lanes),
            iteration_count: Some(iteration_count),
            stride,
            has_reduction: false,
            reduction_op: None,
            alias_safe,
            alignment: Alignment::Unknown,
            reason_rejected: None,
        })
    }
}
