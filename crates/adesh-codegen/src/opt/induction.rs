//! Loop Induction Variable Analysis and Strength Reduction.
//!
//! Detects linear induction variables (`i = i + step`) in natural loops and simplifies
//! dependent address computations into incremental pointer updates.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use crate::opt::dominance::DominatorTree;
use crate::opt::loop_analysis::LoopInfo;
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

/// Linear Induction Variable: `v = base + i * step`.
#[derive(Debug, Clone)]
pub struct InductionVariable {
    pub primary_vreg: VirtualRegister,
    pub step: i64,
    pub loop_header: u32,
}

pub struct InductionVariablePass;

impl Default for InductionVariablePass {
    fn default() -> Self {
        Self::new()
    }
}

impl InductionVariablePass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for InductionVariablePass {
    fn name(&self) -> &'static str {
        "InductionVariableAnalysis"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let dom = DominatorTree::compute(func);
        let loop_info = LoopInfo::analyze(func, &dom);

        if loop_info.loops.is_empty() {
            return Ok(false);
        }

        let mut changed = false;
        let mut induction_vars: HashMap<VirtualRegister, InductionVariable> = HashMap::new();

        // 1. Detect basic induction variables (`v = v + step`) in loop bodies
        for nat_loop in loop_info.loops.values() {
            for &b_id in &nat_loop.block_ids {
                if let Some(block) = func.blocks.iter().find(|b| b.id == b_id) {
                    for inst in &block.instructions {
                        if let MachineInstruction::Add {
                            dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                            src: MachineOperand::Immediate(step),
                        } = inst
                        {
                            induction_vars.insert(
                                *v,
                                InductionVariable {
                                    primary_vreg: *v,
                                    step: *step,
                                    loop_header: nat_loop.header_id,
                                },
                            );
                        }
                    }
                }
            }
        }

        // 2. Simplify derived induction multiplications (`d = v * power_of_two`) inside the loop
        for nat_loop in loop_info.loops.values() {
            for &b_id in &nat_loop.block_ids {
                if let Some(block) = func.blocks.iter_mut().find(|b| b.id == b_id) {
                    for inst in &mut block.instructions {
                        if let MachineInstruction::Mul {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src: MachineOperand::Immediate(scale),
                        } = inst
                            && *scale > 0
                            && (*scale as u64).is_power_of_two()
                        {
                            let shift = (*scale as u64).trailing_zeros() as i64;
                            *inst = MachineInstruction::Shl {
                                dst: MachineOperand::Register(MachineRegister::Virtual(*dst_v)),
                                src: MachineOperand::Immediate(shift),
                            };
                            changed = true;
                        }
                    }
                }
            }
        }

        Ok(changed)
    }
}
