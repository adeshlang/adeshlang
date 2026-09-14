//! SIMD vectorization cost model

use super::analysis::VectorizationCandidate;
use super::types::{SimdElement, SimdIsa, SimdType};

/// Execution strategy selected by the cost model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStrategy {
  Scalar,
  Simd,
  Parallel,
  ParallelSimd,
}

/// Cost model for selecting scalar vs SIMD vs parallel execution
pub struct VectorizationCostModel {
    pub min_simd_iterations: u64,
    pub min_parallel_iterations: u64,
    pub thread_overhead_cost: f64,
    pub simd_setup_cost: f64,
}

impl Default for VectorizationCostModel {
    fn default() -> Self {
        VectorizationCostModel {
            min_simd_iterations: 8,
            min_parallel_iterations: 1024,
            thread_overhead_cost: 1000.0,
            simd_setup_cost: 10.0,
        }
    }
}

impl VectorizationCostModel {
    pub fn select_strategy(
        &self,
        iterations: u64,
        op_cost_per_element: f64,
        num_cores: usize,
        isa: SimdIsa,
        element: SimdElement,
    ) -> ExecutionStrategy {
        let work = iterations as f64 * op_cost_per_element;
        let lanes = isa.max_lanes(element) as f64;

        let scalar_cost = work;
        let simd_cost = self.simd_setup_cost + work / lanes;
        let parallel_cost =
            self.thread_overhead_cost + work / num_cores.max(1) as f64;
        let parallel_simd_cost =
            self.thread_overhead_cost + work / (num_cores.max(1) as f64 * lanes);

        if iterations < self.min_simd_iterations {
            return ExecutionStrategy::Scalar;
        }

        let mut best = ExecutionStrategy::Scalar;
        let mut best_cost = scalar_cost;

        if simd_cost < best_cost {
            best = ExecutionStrategy::Simd;
            best_cost = simd_cost;
        }

        if iterations >= self.min_parallel_iterations && parallel_cost < best_cost {
            best = ExecutionStrategy::Parallel;
            best_cost = parallel_cost;
        }

        if iterations >= self.min_parallel_iterations && parallel_simd_cost < best_cost {
            best = ExecutionStrategy::ParallelSimd;
        }

        best
    }

    pub fn should_vectorize(&self, candidate: &VectorizationCandidate) -> bool {
        if let Some(reason) = &candidate.reason_rejected {
            return reason.is_empty();
        }
        candidate
            .iteration_count
            .map(|n| n >= self.min_simd_iterations)
            .unwrap_or(false)
    }

    pub fn optimal_simd_type(&self, element: SimdElement, isa: SimdIsa) -> SimdType {
        let lanes = isa.max_lanes(element);
        SimdType::new(element, lanes)
    }
}
