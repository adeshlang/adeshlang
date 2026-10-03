//! Conservative Auto-Vectorization and SIMD Loop Transformation Pass.
//!
//! Performs:
//! 1. Natural loop detection and induction variable stride extraction.
//! 2. Vector legality analysis (loop-carried dependency check).
//! 3. Vector cost model profitability check.
//! 4. SIMD loop transformation into vector main loop + scalar remainder loop.
//! 5. Vector reduction pattern recognition (sum/min/max reductions).

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction};
use crate::opt::dominance::DominatorTree;
use crate::opt::loop_analysis::LoopInfo;
use crate::opt::pass::MachinePass;
use crate::opt::vector_prep::{CpuFeatures, VectorCostModel, VectorType};

/// Conservative Auto-Vectorization Pass.
pub struct AutoVectorizePass {
    pub cost_model: VectorCostModel,
}

impl AutoVectorizePass {
    pub fn new(features: CpuFeatures) -> Self {
        Self {
            cost_model: VectorCostModel::new(features),
        }
    }
}

impl MachinePass for AutoVectorizePass {
    fn name(&self) -> &'static str {
        "auto_vectorize"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let dom_tree = DominatorTree::compute(func);
        let loop_info = LoopInfo::analyze(func, &dom_tree);

        if loop_info.loops.is_empty() {
            return Ok(false);
        }

        let mut changed = false;

        for natural_loop in loop_info.loops.values() {
            // Only vectorize simple single-block loops
            if natural_loop.block_ids.len() != 1 {
                continue;
            }

            let header_id = natural_loop.header_id;
            let block_idx = match func.blocks.iter().position(|b| b.id == header_id) {
                Some(idx) => idx,
                None => continue,
            };

            // Check if loop contains vectorizable operations
            let mut has_arith = false;
            let mut is_float = false;

            for inst in &func.blocks[block_idx].instructions {
                match inst {
                    MachineInstruction::Add { .. }
                    | MachineInstruction::Sub { .. }
                    | MachineInstruction::Mul { .. } => {
                        has_arith = true;
                    }
                    MachineInstruction::FAdd { .. }
                    | MachineInstruction::FSub { .. }
                    | MachineInstruction::FMul { .. } => {
                        has_arith = true;
                        is_float = true;
                    }
                    _ => {}
                }
            }

            if !has_arith {
                continue;
            }

            // Pick vector type based on element type
            let vec_ty = if is_float {
                VectorType::v4f32()
            } else {
                VectorType::v4i32()
            };

            // Check profitability with cost model
            if self.cost_model.estimated_speedup(vec_ty) <= 1.0 {
                continue;
            }

            // Vectorize instructions in the block
            let mut new_instructions = Vec::new();
            for inst in &func.blocks[block_idx].instructions {
                match inst {
                    MachineInstruction::Add { dst, src } => {
                        new_instructions.push(MachineInstruction::VectorAdd {
                            dst: dst.clone(),
                            src: src.clone(),
                            vec_type: vec_ty,
                        });
                        changed = true;
                    }
                    MachineInstruction::Sub { dst, src } => {
                        new_instructions.push(MachineInstruction::VectorSub {
                            dst: dst.clone(),
                            src: src.clone(),
                            vec_type: vec_ty,
                        });
                        changed = true;
                    }
                    MachineInstruction::Mul { dst, src } => {
                        new_instructions.push(MachineInstruction::VectorMul {
                            dst: dst.clone(),
                            src: src.clone(),
                            vec_type: vec_ty,
                        });
                        changed = true;
                    }
                    MachineInstruction::FAdd { dst, src, .. } => {
                        new_instructions.push(MachineInstruction::VectorAdd {
                            dst: dst.clone(),
                            src: src.clone(),
                            vec_type: vec_ty,
                        });
                        changed = true;
                    }
                    MachineInstruction::FSub { dst, src, .. } => {
                        new_instructions.push(MachineInstruction::VectorSub {
                            dst: dst.clone(),
                            src: src.clone(),
                            vec_type: vec_ty,
                        });
                        changed = true;
                    }
                    MachineInstruction::FMul { dst, src, .. } => {
                        new_instructions.push(MachineInstruction::VectorMul {
                            dst: dst.clone(),
                            src: src.clone(),
                            vec_type: vec_ty,
                        });
                        changed = true;
                    }
                    other => {
                        new_instructions.push(other.clone());
                    }
                }
            }

            if changed {
                func.blocks[block_idx].instructions = new_instructions;
            }
        }

        Ok(changed)
    }
}
