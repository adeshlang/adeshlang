//! Speculative optimization and assumption tracking
//!
//! This module implements speculative optimizations based on runtime observations.
//! It tracks assumptions made during optimization and handles deoptimization when
//! assumptions are violated.

use super::profiling::ObservedType;
use crate::backends::jit::lir::ValueId;
use crate::utils::collections::FastMap;

/// A speculation assumption that can be invalidated
#[derive(Debug, Clone)]
pub struct Assumption {
    /// Unique ID for this assumption
    pub id: u64,
    /// What kind of assumption this is
    pub kind: AssumptionKind,
    /// Whether this assumption is still valid
    pub valid: bool,
}

/// Types of assumptions we can make
#[derive(Debug, Clone)]
pub enum AssumptionKind {
    /// Type of a variable is constant
    TypeStable { var: String, expected: ObservedType },
    /// Property access always hits same offset
    PropertyStable {
        class_id: u64,
        property: String,
        offset: u32,
    },
    /// Function is not redefined
    FunctionStable { name: String },
    /// Array bounds are within range
    ArrayBoundsCheck { max_size: usize },
    /// No new properties added to object
    ShapeStable { class_id: u64 },
}

/// Deoptimization point information
#[derive(Debug, Clone)]
pub struct DeoptPoint {
    /// Location in optimized code
    pub opt_pc: u32,
    /// Corresponding location in unoptimized code
    pub unopt_pc: u32,
    /// Values that need to be reconstructed
    pub live_values: Vec<ValueId>,
    /// Assumptions that must hold
    pub assumptions: Vec<u64>,
}

/// Speculative optimizer that tracks and validates assumptions
pub struct SpeculativeOptimizer {
    /// All active assumptions
    assumptions: FastMap<u64, Assumption>,
    /// Next assumption ID
    next_id: u64,
    /// Deoptimization points for each function
    deopt_points: FastMap<String, Vec<DeoptPoint>>,
    /// Statistics
    total_speculations: u64,
    successful_speculations: u64,
    failed_speculations: u64,
}

impl SpeculativeOptimizer {
    pub fn new() -> Self {
        SpeculativeOptimizer {
            assumptions: FastMap::default(),
            next_id: 1,
            deopt_points: FastMap::default(),
            total_speculations: 0,
            successful_speculations: 0,
            failed_speculations: 0,
        }
    }

    /// Create a new assumption
    pub fn assume(&mut self, kind: AssumptionKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.total_speculations += 1;

        self.assumptions.insert(
            id,
            Assumption {
                id,
                kind,
                valid: true,
            },
        );

        id
    }

    /// Check if an assumption is still valid
    pub fn is_valid(&self, id: u64) -> bool {
        self.assumptions.get(&id).map(|a| a.valid).unwrap_or(false)
    }

    /// Invalidate an assumption
    pub fn invalidate(&mut self, id: u64) {
        if let Some(assumption) = self.assumptions.get_mut(&id) {
            if assumption.valid {
                assumption.valid = false;
                self.failed_speculations += 1;
            }
        }
    }

    /// Check all assumptions for a type change
    pub fn check_type_change(&mut self, var: &str, actual: ObservedType) -> Vec<u64> {
        let mut invalidated = Vec::new();

        for (id, assumption) in &mut self.assumptions {
            if assumption.valid {
                if let AssumptionKind::TypeStable { var: v, expected } = &assumption.kind {
                    if v == var && *expected != actual {
                        assumption.valid = false;
                        self.failed_speculations += 1;
                        invalidated.push(*id);
                    }
                }
            }
        }

        invalidated
    }

    /// Register a deoptimization point
    pub fn register_deopt_point(&mut self, func: &str, point: DeoptPoint) {
        self.deopt_points
            .entry(func.to_string())
            .or_default()
            .push(point);
    }

    /// Get statistics
    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.total_speculations,
            self.successful_speculations,
            self.failed_speculations,
        )
    }
}

impl Default for SpeculativeOptimizer {
    fn default() -> Self {
        Self::new()
    }
}
