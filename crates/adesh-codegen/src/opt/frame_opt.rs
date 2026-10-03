//! Stack Frame Optimization & Allocation Compaction.
//!
//! Compacts stack frame sizes, eliminates unused spill slots, and ensures 16-byte ABI alignment.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand};
use crate::opt::pass::MachinePass;
use std::collections::HashSet;

pub struct StackFrameOptimizationPass;

impl Default for StackFrameOptimizationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl StackFrameOptimizationPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for StackFrameOptimizationPass {
    fn name(&self) -> &'static str {
        "StackFrameOptimization"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut used_slots = HashSet::new();

        for block in &func.blocks {
            for inst in &block.instructions {
                let mut check_op = |op: &MachineOperand| {
                    if let MachineOperand::StackSlot(slot) = op {
                        used_slots.insert(*slot);
                    }
                };

                match inst {
                    MachineInstruction::Move { dst, src }
                    | MachineInstruction::Load { dst, src, .. }
                    | MachineInstruction::Store { dst, src, .. }
                    | MachineInstruction::Add { dst, src }
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
                        check_op(dst);
                        check_op(src);
                    }
                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::Test { lhs, rhs }
                    | MachineInstruction::FCmp { lhs, rhs, .. } => {
                        check_op(lhs);
                        check_op(rhs);
                    }
                    _ => {}
                }
            }
        }

        // If no stack slots are touched, we can minimize stack size
        let initial_size = func.stack_size;
        if used_slots.is_empty() && func.stack_size > 0 {
            // Keep required frame size or 0
            func.stack_size = 0;
            return Ok(initial_size != 0);
        }

        if let Some(min_slot) = used_slots.iter().min() {
            let required_bytes = ((-min_slot) as u64 + 15) & !15;
            if required_bytes < func.stack_size {
                func.stack_size = required_bytes;
                return Ok(true);
            }
        }

        Ok(false)
    }
}
