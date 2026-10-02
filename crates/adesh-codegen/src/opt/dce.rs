//! Dead Code Elimination (DCE) Pass for Machine IR.
//!
//! Eliminates unreachable basic blocks and dead assignments to virtual registers
//! that have no downstream uses.

use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use std::collections::HashSet;

pub struct DeadCodeElimination;

impl Default for DeadCodeElimination {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadCodeElimination {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self, func: &mut MachineFunction) -> usize {
        let mut total_changes = 0;
        total_changes += self.eliminate_dead_blocks(func);
        total_changes += self.eliminate_unused_vregs(func);
        total_changes
    }

    /// Eliminate unreachable blocks via BFS reachability from entry block.
    fn eliminate_dead_blocks(&self, func: &mut MachineFunction) -> usize {
        if func.blocks.is_empty() {
            return 0;
        }

        let mut reachable_labels = HashSet::new();
        reachable_labels.insert(func.blocks[0].label.clone());

        let mut worklist = vec![func.blocks[0].label.clone()];
        let label_to_block: std::collections::HashMap<_, _> =
            func.blocks.iter().map(|b| (b.label.clone(), b)).collect();

        while let Some(current_label) = worklist.pop() {
            if let Some(block) = label_to_block.get(&current_label) {
                for inst in &block.instructions {
                    match inst {
                        MachineInstruction::Branch { target }
                        | MachineInstruction::BranchCc { target, .. }
                            if reachable_labels.insert(target.clone()) =>
                        {
                            worklist.push(target.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        let original_count = func.blocks.len();
        func.blocks.retain(|b| reachable_labels.contains(&b.label));
        original_count - func.blocks.len()
    }

    /// Eliminate pure assignments to virtual registers whose results are never read.
    fn eliminate_unused_vregs(&self, func: &mut MachineFunction) -> usize {
        let mut used_vregs = HashSet::new();

        // 1. Gather all used virtual registers
        for block in &func.blocks {
            for inst in &block.instructions {
                self.collect_uses(inst, &mut used_vregs);
            }
        }

        // 2. Remove dead definitions (moves/constants with unused virtual register destinations)
        let mut removed = 0;
        for block in &mut func.blocks {
            block.instructions.retain(|inst| match inst {
                MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    ..
                } if !used_vregs.contains(vreg) => {
                    removed += 1;
                    false
                }
                _ => true,
            });
        }

        removed
    }

    fn collect_uses(&self, inst: &MachineInstruction, used: &mut HashSet<VirtualRegister>) {
        let mut check_operand = |op: &MachineOperand| match op {
            MachineOperand::Register(MachineRegister::Virtual(vreg)) => {
                used.insert(*vreg);
            }
            MachineOperand::Memory { base, index, .. } => {
                if let MachineRegister::Virtual(vreg) = base {
                    used.insert(*vreg);
                }
                if let Some((MachineRegister::Virtual(vreg), _)) = index {
                    used.insert(*vreg);
                }
            }
            _ => {}
        };

        match inst {
            MachineInstruction::Move { src, .. } => check_operand(src),
            MachineInstruction::Load { src, .. } => check_operand(src),
            MachineInstruction::Store { dst, src, .. } => {
                check_operand(dst);
                check_operand(src);
            }
            MachineInstruction::Add { dst, src }
            | MachineInstruction::Sub { dst, src }
            | MachineInstruction::Mul { dst, src }
            | MachineInstruction::Div { dst, src }
            | MachineInstruction::Mod { dst, src }
            | MachineInstruction::And { dst, src }
            | MachineInstruction::Or { dst, src }
            | MachineInstruction::Xor { dst, src }
            | MachineInstruction::Shl { dst, src }
            | MachineInstruction::Shr { dst, src }
            | MachineInstruction::Sar { dst, src } => {
                check_operand(dst);
                check_operand(src);
            }
            MachineInstruction::Compare { lhs, rhs } | MachineInstruction::Test { lhs, rhs } => {
                check_operand(lhs);
                check_operand(rhs);
            }
            MachineInstruction::Call { target, .. } => check_operand(target),
            MachineInstruction::Push { src } => check_operand(src),
            _ => {}
        }
    }
}
