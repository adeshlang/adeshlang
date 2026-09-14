//! Automatic loop vectorization pass

use super::analysis::LoopVectorizer;
use super::cost_model::{ExecutionStrategy, VectorizationCostModel};
use super::instructions::{SimdBlock, SimdInst};
use super::lowering::{SimdLoweringTarget, detect_host_isa};
use super::types::{SimdElement, SimdType};

/// Result of the vectorization pass
#[derive(Debug)]
pub struct VectorizationResult {
    pub strategy: ExecutionStrategy,
    pub simd_block: Option<SimdBlock>,
    pub remainder_iterations: u64,
    pub diagnostic: Option<String>,
}

/// Auto-vectorization pass
pub struct AutoVectorizer {
    cost_model: VectorizationCostModel,
    lowering_target: SimdLoweringTarget,
}

impl Default for AutoVectorizer {
    fn default() -> Self {
        AutoVectorizer {
            cost_model: VectorizationCostModel::default(),
            lowering_target: SimdLoweringTarget::default(),
        }
    }
}

impl AutoVectorizer {
    pub fn vectorize_loop(
        &self,
        iterations: u64,
        element_type: &str,
        op: VectorOp,
        alias_safe: bool,
    ) -> VectorizationResult {
        let elem = match SimdElement::from_type_name(element_type) {
            Some(e) => e,
            None => {
                return VectorizationResult {
                    strategy: ExecutionStrategy::Scalar,
                    simd_block: None,
                    remainder_iterations: iterations,
                    diagnostic: Some(format!(
                        "loop not vectorized: unsupported element type '{}'",
                        element_type
                    )),
                };
            }
        };

        let candidate = match LoopVectorizer::analyze_candidate(
            iterations,
            element_type,
            1,
            false,
            alias_safe,
            self.cost_model.min_simd_iterations,
        ) {
            Some(c) => c,
            None => {
                return VectorizationResult {
                    strategy: ExecutionStrategy::Scalar,
                    simd_block: None,
                    remainder_iterations: iterations,
                    diagnostic: Some(
                        "loop not vectorized: insufficient iterations or unsafe aliasing"
                            .to_string(),
                    ),
                };
            }
        };

        let num_cores = crate::runtime::thread::logical_cpu_count();
        let strategy = self.cost_model.select_strategy(
            iterations,
            1.0,
            num_cores,
            self.lowering_target.isa,
            elem,
        );

        if strategy == ExecutionStrategy::Scalar {
            return VectorizationResult {
                strategy,
                simd_block: None,
                remainder_iterations: iterations,
                diagnostic: candidate.reason_rejected,
            };
        }

        let simd_type = self.cost_model.optimal_simd_type(elem, detect_host_isa());
        let lanes = simd_type.lanes as u64;
        let vectorized_iters = (iterations / lanes) * lanes;
        let remainder = iterations - vectorized_iters;

        let simd_block = self.generate_simd_block(&simd_type, op);

        VectorizationResult {
            strategy,
            simd_block: Some(simd_block),
            remainder_iterations: remainder,
            diagnostic: None,
        }
    }

    fn generate_simd_block(&self, simd_type: &SimdType, op: VectorOp) -> SimdBlock {
        let mut block = SimdBlock::default();
        // Conceptual SIMD loop body: load a, load b, op, store c
        let _ = simd_type;
        match op {
            VectorOp::Add => {
                block.instructions.push(SimdInst::Load(0, *simd_type, 0));
                block.instructions.push(SimdInst::Load(1, *simd_type, 1));
                block.instructions.push(SimdInst::Add(2, 0, 1));
                block.instructions.push(SimdInst::Store(2, 2));
            }
            VectorOp::Mul => {
                block.instructions.push(SimdInst::Load(0, *simd_type, 0));
                block.instructions.push(SimdInst::Load(1, *simd_type, 1));
                block.instructions.push(SimdInst::Mul(2, 0, 1));
                block.instructions.push(SimdInst::Store(2, 2));
            }
            VectorOp::Sub => {
                block.instructions.push(SimdInst::Load(0, *simd_type, 0));
                block.instructions.push(SimdInst::Load(1, *simd_type, 1));
                block.instructions.push(SimdInst::Sub(2, 0, 1));
                block.instructions.push(SimdInst::Store(2, 2));
            }
            VectorOp::Div => {
                block.instructions.push(SimdInst::Load(0, *simd_type, 0));
                block.instructions.push(SimdInst::Load(1, *simd_type, 1));
                block.instructions.push(SimdInst::Div(2, 0, 1));
                block.instructions.push(SimdInst::Store(2, 2));
            }
            VectorOp::FusedMulAdd => {
                block.instructions.push(SimdInst::Load(0, *simd_type, 0));
                block.instructions.push(SimdInst::Load(1, *simd_type, 1));
                block.instructions.push(SimdInst::Load(2, *simd_type, 2));
                block.instructions.push(SimdInst::Fma(3, 0, 1, 2));
                block.instructions.push(SimdInst::Store(3, 3));
            }
        }
        block
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VectorOp {
    Add,
    Sub,
    Mul,
    Div,
    FusedMulAdd,
}
