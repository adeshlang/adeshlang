//! Conservative Move Coalescing Pass for Machine IR.
//!
//! Identifies non-interfering virtual register copy pairs (`mov v1, v2`)
//! and merges them into a single virtual register to eliminate unnecessary moves
//! and reduce register pressure before allocation.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use crate::opt::pass::MachinePass;
use crate::register_alloc::LivenessAnalysis;
use std::collections::HashMap;

pub struct MoveCoalescingPass;

impl Default for MoveCoalescingPass {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveCoalescingPass {
    pub fn new() -> Self {
        Self
    }

    /// Removes copies whose assigned source and destination are already the
    /// same physical register. Stack-slot self-copies are safe to remove too.
    pub fn eliminate_redundant_allocated_moves(func: &mut MachineFunction) -> usize {
        let mut removed = 0;
        for block in &mut func.blocks {
            block.instructions.retain(|inst| {
                let redundant = match inst {
                    MachineInstruction::Move { dst, src } => match (dst, src) {
                        (
                            MachineOperand::Register(MachineRegister::Physical(dst)),
                            MachineOperand::Register(MachineRegister::Physical(src)),
                        ) => dst == src,
                        (MachineOperand::StackSlot(dst), MachineOperand::StackSlot(src)) => {
                            dst == src
                        }
                        (
                            MachineOperand::Register(MachineRegister::Virtual(dst)),
                            MachineOperand::Register(MachineRegister::Virtual(src)),
                        ) => dst == src,
                        _ => false,
                    },
                    _ => false,
                };
                if redundant {
                    removed += 1;
                }
                !redundant
            });
        }
        removed
    }

    fn has_unsupported_reference(func: &MachineFunction, vreg: VirtualRegister) -> bool {
        func.blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|inst| {
                let references_vreg = inst
                    .uses()
                    .into_iter()
                    .chain(inst.defs())
                    .any(|reg| reg == MachineRegister::Virtual(vreg));
                references_vreg && !Self::rewrites_instruction_operands(inst)
            })
    }

    fn rewrites_instruction_operands(inst: &MachineInstruction) -> bool {
        matches!(
            inst,
            MachineInstruction::Move { .. }
                | MachineInstruction::Add { .. }
                | MachineInstruction::Sub { .. }
                | MachineInstruction::Mul { .. }
                | MachineInstruction::Div { .. }
                | MachineInstruction::Mod { .. }
                | MachineInstruction::And { .. }
                | MachineInstruction::Or { .. }
                | MachineInstruction::Xor { .. }
                | MachineInstruction::Shl { .. }
                | MachineInstruction::Shr { .. }
                | MachineInstruction::Sar { .. }
                | MachineInstruction::FAdd { .. }
                | MachineInstruction::FSub { .. }
                | MachineInstruction::FMul { .. }
                | MachineInstruction::FDiv { .. }
                | MachineInstruction::Compare { .. }
                | MachineInstruction::Test { .. }
                | MachineInstruction::FCmp { .. }
                | MachineInstruction::Neg { .. }
                | MachineInstruction::Not { .. }
                | MachineInstruction::FNeg { .. }
                | MachineInstruction::SetCc { .. }
                | MachineInstruction::Push { .. }
                | MachineInstruction::Pop { .. }
        )
    }

    fn overlaps_outside_copy(
        left: &crate::register_alloc::LiveRange,
        right: &crate::register_alloc::LiveRange,
        copy_position: usize,
    ) -> bool {
        left.segments.iter().any(|left_segment| {
            right.segments.iter().any(|right_segment| {
                let start = left_segment.start.max(right_segment.start);
                let end = left_segment.end.min(right_segment.end);
                start < end && (start < copy_position || end > copy_position + 1)
            })
        })
    }
}

impl MachinePass for MoveCoalescingPass {
    fn name(&self) -> &'static str {
        "MoveCoalescing"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let liveness = LivenessAnalysis::compute(func);
        let intervals = liveness.build_live_ranges(func);

        let mut interval_map = HashMap::new();
        for interval in &intervals {
            interval_map.insert(interval.vreg, interval.clone());
        }

        let mut merge_map: HashMap<VirtualRegister, VirtualRegister> = HashMap::new();
        let mut coalesced_count = 0;

        for block in &func.blocks {
            for (local_idx, inst) in block.instructions.iter().enumerate() {
                if let MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                    src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                } = inst
                    && dst_v != src_v
                {
                    let r1 = interval_map.get(dst_v);
                    let r2 = interval_map.get(src_v);

                    let copy_position = liveness
                        .inst_index_map
                        .get(&(block.id, local_idx))
                        .copied()
                        .unwrap_or(usize::MAX);
                    if let (Some(iv1), Some(iv2)) = (r1, r2)
                        && iv1.class == iv2.class
                        && !Self::overlaps_outside_copy(iv1, iv2, copy_position)
                        && !Self::has_unsupported_reference(func, *dst_v)
                        && !Self::has_unsupported_reference(func, *src_v)
                    {
                        // Safe to coalesce!
                        let target_v = merge_map.get(src_v).copied().unwrap_or(*src_v);
                        merge_map.insert(*dst_v, target_v);
                        coalesced_count += 1;
                    }
                }
            }
        }

        if coalesced_count == 0 {
            return Ok(false);
        }

        // Apply coalesced mappings
        for block in &mut func.blocks {
            block.instructions.retain_mut(|inst| {
                if let MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                    src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                } = inst
                {
                    let d = merge_map.get(dst_v).copied().unwrap_or(*dst_v);
                    let s = merge_map.get(src_v).copied().unwrap_or(*src_v);
                    if d == s {
                        return false; // Eliminate coalesced self-move!
                    }
                }

                // Rewrite virtual registers in operands
                let rewrite_op = |op: &mut MachineOperand| {
                    if let MachineOperand::Register(MachineRegister::Virtual(v)) = op
                        && let Some(&merged) = merge_map.get(v)
                    {
                        *v = merged;
                    }
                };

                match inst {
                    MachineInstruction::Move { dst, src } => {
                        rewrite_op(dst);
                        rewrite_op(src);
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
                    | MachineInstruction::FDiv { dst, src, .. } => {
                        rewrite_op(dst);
                        rewrite_op(src);
                    }
                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::Test { lhs, rhs }
                    | MachineInstruction::FCmp { lhs, rhs, .. } => {
                        rewrite_op(lhs);
                        rewrite_op(rhs);
                    }
                    MachineInstruction::Neg { dst }
                    | MachineInstruction::Not { dst }
                    | MachineInstruction::FNeg { dst, .. }
                    | MachineInstruction::SetCc { dst, .. }
                    | MachineInstruction::Push { src: dst }
                    | MachineInstruction::Pop { dst } => {
                        rewrite_op(dst);
                    }
                    _ => {}
                }

                true
            });
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::{MachineRegister, PhysicalRegister};

    #[test]
    fn removes_only_redundant_allocated_copies() {
        let mut func = MachineFunction::new("allocated_move_elimination");
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::StackSlot(-8),
            src: MachineOperand::StackSlot(-8),
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        });

        assert_eq!(
            MoveCoalescingPass::eliminate_redundant_allocated_moves(&mut func),
            2
        );
        assert_eq!(func.blocks[0].instructions.len(), 1);
    }

    #[test]
    fn coalesces_copy_with_only_copy_position_interference() {
        let mut func = MachineFunction::new("virtual_copy_coalescing");
        let source = func.alloc_vreg();
        let destination = func.alloc_vreg();
        let entry = func.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(source)),
            src: MachineOperand::Immediate(1),
        });
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(destination)),
            src: MachineOperand::Register(MachineRegister::Virtual(source)),
        });
        entry.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(destination)),
            src: MachineOperand::Immediate(2),
        });
        entry.push(MachineInstruction::Return);

        let mut pass = MoveCoalescingPass::new();
        assert!(pass.run_on_function(&mut func).unwrap());
        assert!(!func.blocks[0].instructions.iter().any(|inst| matches!(
            inst,
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(_)),
                src: MachineOperand::Register(MachineRegister::Virtual(_)),
            }
        )));
        assert!(func.blocks[0].instructions.iter().any(|inst| matches!(
            inst,
            MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                ..
            } if *v == source
        )));
    }

    #[test]
    fn refuses_coalescing_when_an_unsupported_instruction_uses_value() {
        let mut func = MachineFunction::new("unsafe_copy_coalescing_guard");
        let source = func.alloc_vreg();
        let destination = func.alloc_vreg();
        let entry = func.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(source)),
            src: MachineOperand::Immediate(1),
        });
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(destination)),
            src: MachineOperand::Register(MachineRegister::Virtual(source)),
        });
        entry.push(MachineInstruction::Load {
            dst: MachineOperand::Register(MachineRegister::Virtual(destination)),
            src: MachineOperand::StackSlot(-8),
            size: 8,
        });
        entry.push(MachineInstruction::Return);

        let mut pass = MoveCoalescingPass::new();
        assert!(!pass.run_on_function(&mut func).unwrap());
        assert!(func.blocks[0].instructions.iter().any(|inst| matches!(
            inst,
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(_)),
                src: MachineOperand::Register(MachineRegister::Virtual(_)),
            }
        )));
    }
}
