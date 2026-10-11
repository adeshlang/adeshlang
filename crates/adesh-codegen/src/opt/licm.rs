//! Loop-Invariant Code Motion (LICM).
//!
//! Identifies loop-invariant computations within natural loops and hoists them
//! into the loop pre-header block when safe and side-effect free.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use crate::opt::dominance::DominatorTree;
use crate::opt::loop_analysis::LoopInfo;
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

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

            // Count virtual-register definitions inside the loop
            let mut def_counts: HashMap<VirtualRegister, usize> = HashMap::new();
            for &b_id in &nat_loop.block_ids {
                if let Some(b) = func.blocks.iter().find(|blk| blk.id == b_id) {
                    for inst in &b.instructions {
                        for def in inst.defs() {
                            if let MachineRegister::Virtual(v) = def {
                                *def_counts.entry(v).or_insert(0) += 1;
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
                            // Hoisting a definition out of the loop is only
                            // sound when this `Move dst <- imm` is the *only*
                            // definition of `dst` inside the loop. A
                            // destination that the loop also mutates in place
                            // (for example an address computation
                            // `Move scaled <- idx; Mul scaled *= 8; Add scaled += ptr`
                            // sharing one virtual register) must re-execute
                            // every iteration, and a second static definition
                            // would fight the hoisted one. Only virtual
                            // destinations are safe: physical registers can
                            // be clobbered by calls in the loop.
                            MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(d)),
                                src: MachineOperand::Immediate(_),
                            } if def_counts.get(d) == Some(&1) => true,
                            // In-place updates (`dst = dst + imm`) read their
                            // destination, which is redefined every
                            // iteration, so they are never loop-invariant.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::{MachineOperand, MachineRegister, VirtualRegister};

    fn build_loop_func(loop_body: Vec<MachineInstruction>) -> (MachineFunction, u32, u32) {
        let mut func = MachineFunction::new("licm_test");
        let entry = func.entry_block_mut();
        entry.push(MachineInstruction::Branch {
            target: "loop".to_string(),
        });

        let loop_id = func.create_block("loop");
        for inst in loop_body {
            func.blocks[loop_id as usize].push(inst);
        }
        func.blocks[loop_id as usize].push(MachineInstruction::BranchCc {
            cc: crate::machine_ir::ConditionCode::Equal,
            target: "exit".to_string(),
        });
        func.blocks[loop_id as usize].push(MachineInstruction::Branch {
            target: "loop".to_string(),
        });

        let exit_id = func.create_block("exit");
        func.blocks[exit_id as usize].push(MachineInstruction::Return);

        func.rebuild_cfg();
        (func, loop_id, exit_id)
    }

    #[test]
    fn test_single_definition_constant_is_hoisted() {
        let size = VirtualRegister(0);
        let acc = VirtualRegister(1);
        let (mut func, loop_id, _) = build_loop_func(vec![
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(size)),
                src: MachineOperand::Immediate(16),
            },
            MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Virtual(acc)),
                src: MachineOperand::Immediate(1),
            },
        ]);

        let mut pass = LICMPass::new();
        let changed = pass.run_on_function(&mut func).expect("licm runs");
        assert!(changed, "a lone constant Move should be hoisted");
        assert!(
            func.blocks[0]
                .instructions
                .iter()
                .any(|inst| matches!(
                    inst,
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(d)),
                        src: MachineOperand::Immediate(16),
                    } if *d == size
                )),
            "the constant materialization moves to the preheader"
        );
        assert!(
            !func.blocks[loop_id as usize]
                .instructions
                .iter()
                .any(|inst| matches!(
                    inst,
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(d)),
                        src: MachineOperand::Immediate(16),
                    } if *d == size
                )),
            "the loop no longer rematerializes the constant"
        );
    }

    #[test]
    fn test_destination_redefined_in_loop_is_not_hoisted() {
        // Address-computation chain sharing one virtual register:
        // `Move scaled <- 0; Add scaled += ptr`. Hoisting the Move would let
        // the Add keep mutating the hoisted value across iterations.
        let scaled = VirtualRegister(0);
        let ptr = VirtualRegister(1);
        let (mut func, loop_id, _) = build_loop_func(vec![
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(scaled)),
                src: MachineOperand::Immediate(0),
            },
            MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Virtual(scaled)),
                src: MachineOperand::Register(MachineRegister::Virtual(ptr)),
            },
        ]);

        let mut pass = LICMPass::new();
        let changed = pass.run_on_function(&mut func).expect("licm runs");
        assert!(
            !changed,
            "a Move whose destination the loop mutates in place must not be hoisted"
        );
        assert!(
            func.blocks[loop_id as usize]
                .instructions
                .iter()
                .any(|inst| matches!(
                    inst,
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(d)),
                        src: MachineOperand::Immediate(0),
                    } if *d == scaled
                )),
            "the zeroing Move stays inside the loop"
        );
    }

    #[test]
    fn test_in_place_add_is_not_hoisted() {
        // `acc = acc + 1` is never loop-invariant: it reads the value the
        // previous iteration wrote.
        let acc = VirtualRegister(0);
        let (mut func, loop_id, _) = build_loop_func(vec![
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(acc)),
                src: MachineOperand::Immediate(0),
            },
            MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Virtual(acc)),
                src: MachineOperand::Immediate(1),
            },
        ]);

        let mut pass = LICMPass::new();
        let changed = pass.run_on_function(&mut func).expect("licm runs");
        assert!(!changed, "in-place updates must never be hoisted");
        assert!(
            func.blocks[loop_id as usize]
                .instructions
                .iter()
                .filter(|inst| matches!(
                    inst,
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(d)),
                        src: MachineOperand::Immediate(_),
                    } if *d == acc
                ))
                .count()
                == 1,
            "both the init Move and the increment stay in the loop"
        );
    }
}
