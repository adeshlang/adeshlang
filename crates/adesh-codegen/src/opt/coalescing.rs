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
            for inst in &block.instructions {
                if let MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                    src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                } = inst
                    && dst_v != src_v
                {
                    let r1 = interval_map.get(dst_v);
                    let r2 = interval_map.get(src_v);

                    if let (Some(iv1), Some(iv2)) = (r1, r2)
                        && iv1.class == iv2.class
                        && !iv1.overlaps(iv2)
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
