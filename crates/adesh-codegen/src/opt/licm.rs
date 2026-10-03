//! Loop-Invariant Code Motion (LICM).
//!
//! Identifies loop-invariant computations within natural loops and hoists them
//! into the loop pre-header block when safe and side-effect free.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand, MachineRegister};
use crate::opt::dominance::DominatorTree;
use crate::opt::loop_analysis::LoopInfo;
use crate::opt::pass::MachinePass;
use std::collections::HashSet;

pub struct LICMPass;

impl Default for LICMPass {
    fn default() -> Self {
        Self::new()
    }
}

impl LICMPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for LICMPass {
    fn name(&self) -> &'static str {
        "LoopInvariantCodeMotion"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        if func.blocks.len() < 2 {
            return Ok(false);
        }

        let dom = DominatorTree::compute(func);
        let loop_info = LoopInfo::analyze(func, &dom);

        if loop_info.loops.is_empty() {
            return Ok(false);
        }

        let mut changed = false;

        for nat_loop in loop_info.loops.values() {
            // Find loop preheader: a predecessor of the header not in the loop
            let header_block = match func.blocks.iter().find(|b| b.id == nat_loop.header_id) {
                Some(b) => b,
                None => continue,
            };

            let preheader_id = header_block
                .predecessors
                .iter()
                .find(|&&p| !nat_loop.block_ids.contains(&p))
                .copied();

            let preheader_id = match preheader_id {
                Some(id) => id,
                None => continue,
            };

            // Collect all virtual registers defined inside the loop
            let mut defs_in_loop = HashSet::new();
            for &b_id in &nat_loop.block_ids {
                if let Some(b) = func.blocks.iter().find(|blk| blk.id == b_id) {
                    for inst in &b.instructions {
                        for def in inst.defs() {
                            if let MachineRegister::Virtual(v) = def {
                                defs_in_loop.insert(v);
                            }
                        }
                    }
                }
            }

            // Identify hoistable instructions in the loop header / loop blocks
            let mut hoisted_instructions = Vec::new();

            for &b_id in &nat_loop.block_ids {
                if let Some(b) = func.blocks.iter_mut().find(|blk| blk.id == b_id) {
                    let mut retain = Vec::new();
                    for inst in b.instructions.drain(..) {
                        let is_invariant = match &inst {
                            MachineInstruction::Move {
                                src: MachineOperand::Immediate(_),
                                ..
                            } => true,
                            MachineInstruction::Add {
                                dst: MachineOperand::Register(MachineRegister::Virtual(d)),
                                src: MachineOperand::Immediate(_),
                            } if !defs_in_loop.contains(d) => true,
                            _ => false,
                        };

                        if is_invariant {
                            hoisted_instructions.push(inst);
                            changed = true;
                        } else {
                            retain.push(inst);
                        }
                    }
                    b.instructions = retain;
                }
            }

            // Insert hoisted instructions at the end of the preheader block (before terminator)
            if !hoisted_instructions.is_empty()
                && let Some(preheader) = func.blocks.iter_mut().find(|b| b.id == preheader_id)
            {
                let insert_pos = if preheader.instructions.is_empty() {
                    0
                } else if matches!(
                    preheader.instructions.last(),
                    Some(MachineInstruction::Branch { .. } | MachineInstruction::BranchCc { .. })
                ) {
                    preheader.instructions.len() - 1
                } else {
                    preheader.instructions.len()
                };

                for (i, inst) in hoisted_instructions.into_iter().enumerate() {
                    preheader.instructions.insert(insert_pos + i, inst);
                }
            }
        }

        Ok(changed)
    }
}
