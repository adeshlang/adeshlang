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
            // Find a dedicated loop preheader: the unique predecessor of the
            // header outside the loop, dominating the header, with a single
            // unconditional exit (so hoisted instructions never execute on
            // unrelated branches, sit between BranchCc and Branch, or clobber
            // flags between Compare and BranchCc).
            let header_block = match func.blocks.iter().find(|b| b.id == nat_loop.header_id) {
                Some(b) => b,
                None => continue,
            };

            let outside_preds: Vec<u32> = header_block
                .predecessors
                .iter()
                .copied()
                .filter(|p| !nat_loop.block_ids.contains(p))
                .collect();

            if outside_preds.len() != 1 {
                continue;
            }
            let preheader_id = outside_preds[0];
            if !dom.dominates(preheader_id, nat_loop.header_id) {
                continue;
            }
            let Some(preheader_ref) = func.blocks.iter().find(|b| b.id == preheader_id) else {
                continue;
            };
            if preheader_ref.successors.len() > 1
                || preheader_ref.instructions.iter().any(|inst| {
                    matches!(
                        inst,
                        MachineInstruction::BranchCc { .. } | MachineInstruction::Return
                    )
                })
            {
                continue;
            }

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
                let mut insert_pos = preheader.instructions.len();
                while insert_pos > 0
                    && matches!(
                        preheader.instructions[insert_pos - 1],
                        MachineInstruction::Branch { .. }
                            | MachineInstruction::BranchCc { .. }
                            | MachineInstruction::Return
                    )
                {
                    insert_pos -= 1;
                }

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
            func.blocks[0].instructions.iter().any(|inst| matches!(
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

    #[test]
    fn test_does_not_hoist_into_multi_successor_branch_cc_preheader() {
        let mut func = MachineFunction::new("licm_branch_cc_preheader");
        let flag = func.alloc_vreg();
        let size = func.alloc_vreg();
        let acc = func.alloc_vreg();

        func.entry_block_mut().push(MachineInstruction::Compare {
            lhs: MachineOperand::Register(MachineRegister::Virtual(flag)),
            rhs: MachineOperand::Immediate(0),
        });
        func.entry_block_mut().push(MachineInstruction::BranchCc {
            cc: crate::machine_ir::ConditionCode::Equal,
            target: "loop".to_string(),
        });
        func.entry_block_mut().push(MachineInstruction::Branch {
            target: "unwind".to_string(),
        });

        let unwind_id = func.create_block("unwind");
        func.blocks[unwind_id as usize].push(MachineInstruction::Return);

        let loop_id = func.create_block("loop");
        func.blocks[loop_id as usize].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(size)),
            src: MachineOperand::Immediate(32),
        });
        func.blocks[loop_id as usize].push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(acc)),
            src: MachineOperand::Immediate(1),
        });
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

        let mut pass = LICMPass::new();
        let changed = pass.run_on_function(&mut func).expect("licm runs");
        assert!(
            !changed,
            "must not hoist into a multi-successor preheader ending in BranchCc + Branch"
        );
        assert_eq!(
            func.blocks[0].instructions.len(),
            3,
            "preheader must remain untouched (Compare, BranchCc, Branch)"
        );
    }
}
