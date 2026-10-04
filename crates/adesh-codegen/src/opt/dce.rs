//! Dead Code Elimination (DCE) Pass for Machine IR.
//!
//! Eliminates unreachable basic blocks and dead assignments to virtual registers
//! that have no downstream uses.

use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, MoveLocation,
    VirtualRegister,
};
use std::collections::{HashMap, HashSet};

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
        total_changes += self.eliminate_dead_instructions_after_terminators(func);
        total_changes += self.eliminate_dead_blocks(func);
        total_changes += self.eliminate_unused_vregs(func);
        total_changes
    }

    /// Truncate any instructions following a terminator (Return or unconditional Branch) in each block.
    fn eliminate_dead_instructions_after_terminators(&self, func: &mut MachineFunction) -> usize {
        let mut removed = 0;
        for block in &mut func.blocks {
            if let Some(pos) = block.instructions.iter().position(|inst| {
                matches!(
                    inst,
                    MachineInstruction::Return | MachineInstruction::Branch { .. }
                )
            }) {
                if pos + 1 < block.instructions.len() {
                    removed += block.instructions.len() - (pos + 1);
                    block.instructions.truncate(pos + 1);
                }
            }
        }
        removed
    }

    /// Eliminate unreachable blocks.
    ///
    /// Successors are the `Branch`/`BranchCc` targets plus the *implicit
    /// fallthrough* to the following block (unless the block ends in an
    /// unconditional `Branch` or `Return`). Ignoring fallthrough would delete
    /// blocks that are still reachable.
    fn eliminate_dead_blocks(&self, func: &mut MachineFunction) -> usize {
        if func.blocks.is_empty() {
            return 0;
        }

        // label -> successor labels, following real control flow.
        let mut successors: HashMap<String, Vec<String>> = HashMap::new();
        for (idx, block) in func.blocks.iter().enumerate() {
            let mut succ = Vec::new();
            let mut unconditional = false;
            let mut returns = false;

            for inst in &block.instructions {
                if unconditional || returns {
                    // Unreachable tail of the block.
                    continue;
                }
                match inst {
                    MachineInstruction::Branch { target } => {
                        succ.push(target.clone());
                        unconditional = true;
                    }
                    MachineInstruction::BranchCc { target, .. } => {
                        succ.push(target.clone());
                    }
                    MachineInstruction::Return => {
                        returns = true;
                    }
                    _ => {}
                }
            }

            if !unconditional && !returns && idx + 1 < func.blocks.len() {
                succ.push(func.blocks[idx + 1].label.clone());
            }

            successors.insert(block.label.clone(), succ);
        }

        let mut reachable_labels = HashSet::new();
        reachable_labels.insert(func.blocks[0].label.clone());

        let mut worklist = vec![func.blocks[0].label.clone()];
        while let Some(current_label) = worklist.pop() {
            if let Some(succ) = successors.get(&current_label) {
                for target in succ {
                    if reachable_labels.insert(target.clone()) {
                        worklist.push(target.clone());
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

    /// Collect every virtual register *read* by `inst`.
    ///
    /// Every instruction form that can read a virtual register must be covered
    /// here: a vreg whose only read is missed would have its defining `Move`
    /// deleted, silently changing program behaviour. Forms that can also write
    /// through memory (SetCc/Load/Store to a memory operand) contribute their
    /// base/index registers.
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
            MachineInstruction::Move { dst, src } => {
                check_operand(src);
                // A move into memory reads the address registers.
                if matches!(dst, MachineOperand::Memory { .. }) {
                    check_operand(dst);
                }
            }
            MachineInstruction::Load { dst, src, .. } => {
                check_operand(src);
                if matches!(dst, MachineOperand::Memory { .. }) {
                    check_operand(dst);
                }
            }
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
            | MachineInstruction::Sar { dst, src }
            | MachineInstruction::FAdd { dst, src, .. }
            | MachineInstruction::FSub { dst, src, .. }
            | MachineInstruction::FMul { dst, src, .. }
            | MachineInstruction::FDiv { dst, src, .. }
            | MachineInstruction::VectorAdd { dst, src, .. }
            | MachineInstruction::VectorSub { dst, src, .. }
            | MachineInstruction::VectorMul { dst, src, .. }
            | MachineInstruction::VectorDiv { dst, src, .. }
            | MachineInstruction::VectorAnd { dst, src, .. }
            | MachineInstruction::VectorOr { dst, src, .. }
            | MachineInstruction::VectorXor { dst, src, .. }
            | MachineInstruction::VectorBroadcast { dst, src, .. }
            | MachineInstruction::VectorShuffle { dst, src, .. }
            | MachineInstruction::VectorReduceAdd { dst, src, .. }
            | MachineInstruction::VectorMin { dst, src, .. }
            | MachineInstruction::VectorMax { dst, src, .. }
            | MachineInstruction::VectorCmp { dst, src, .. }
            | MachineInstruction::VectorBlend { dst, src, .. }
            | MachineInstruction::VectorShiftLeft { dst, src, .. }
            | MachineInstruction::VectorShiftRight { dst, src, .. }
            | MachineInstruction::VectorStore { dst, src, .. }
            | MachineInstruction::AtomicStore { dst, src, .. }
            | MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
                check_operand(dst);
                check_operand(src);
            }
            MachineInstruction::VectorLoad { dst, src, .. }
            | MachineInstruction::AtomicLoad { dst, src, .. } => {
                check_operand(src);
                if matches!(dst, MachineOperand::Memory { .. }) {
                    check_operand(dst);
                }
            }
            MachineInstruction::AtomicCompareExchange {
                dst,
                expected,
                desired,
                ..
            } => {
                check_operand(dst);
                check_operand(expected);
                check_operand(desired);
            }
            MachineInstruction::FCmp { lhs, rhs, .. } => {
                check_operand(lhs);
                check_operand(rhs);
            }
            MachineInstruction::FCvtIntToFloat { dst, src, .. }
            | MachineInstruction::FCvtFloatToInt { dst, src, .. }
            | MachineInstruction::FCvtFloatToFloat { dst, src, .. } => {
                check_operand(src);
                if matches!(dst, MachineOperand::Memory { .. }) {
                    check_operand(dst);
                }
            }
            MachineInstruction::Compare { lhs, rhs } | MachineInstruction::Test { lhs, rhs } => {
                check_operand(lhs);
                check_operand(rhs);
            }
            MachineInstruction::Neg { dst }
            | MachineInstruction::Not { dst }
            | MachineInstruction::FNeg { dst, .. }
            | MachineInstruction::Push { src: dst } => {
                check_operand(dst);
            }
            MachineInstruction::SetCc { dst, .. } => {
                // The condition code itself reads EFLAGS; only a memory
                // destination contributes vregs.
                if matches!(dst, MachineOperand::Memory { .. }) {
                    check_operand(dst);
                }
            }
            MachineInstruction::Pop { dst } => {
                if matches!(dst, MachineOperand::Memory { .. }) {
                    check_operand(dst);
                }
            }
            MachineInstruction::Call { target, .. } => check_operand(target),
            MachineInstruction::ParallelMove { moves } => {
                for m in moves {
                    if let MoveLocation::VirtualRegister(vreg) = m.src {
                        used.insert(vreg);
                    }
                    for loc in [&m.src, &m.dst] {
                        if let MoveLocation::Memory { base, index, .. } = loc {
                            if let MachineRegister::Virtual(vreg) = base {
                                used.insert(*vreg);
                            }
                            if let Some((MachineRegister::Virtual(vreg), _)) = index {
                                used.insert(*vreg);
                            }
                        }
                    }
                }
            }
            MachineInstruction::Custom { operands, .. } => {
                // Custom instructions are opaque: conservatively treat every
                // operand as a use.
                for op in operands {
                    check_operand(op);
                }
            }
            MachineInstruction::Nop
            | MachineInstruction::Return
            | MachineInstruction::Branch { .. }
            | MachineInstruction::BranchCc { .. }
            | MachineInstruction::Barrier => {}
        }
    }
}
