//! Array Optimizations & Bounds Check Elimination (BCE) for Adesh.
//!
//! Optimizes array layouts, contiguous slice indexing, constant-index bounds check
//! elimination, and vector loop lowering.

use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
};

/// Array access representation in Machine IR.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayAccess {
    pub base_ptr: MachineOperand,
    pub index: MachineOperand,
    pub elem_size: u8,
    pub known_length: Option<u64>,
}

pub struct ArrayOptimizer;

impl Default for ArrayOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrayOptimizer {
    pub fn new() -> Self {
        Self
    }

    /// Optimize array accesses and eliminate redundant bounds checks when length and index are known.
    pub fn optimize_function(&self, func: &mut MachineFunction) -> usize {
        let mut eliminated_checks = 0;

        for block in &mut func.blocks {
            let mut i = 0;
            while i < block.instructions.len() {
                // Look for pattern:
                // Compare { lhs: Immediate(idx), rhs: Immediate(len) } followed by BranchCc { cc: AboveOrEqual, target: "panic_bounds" }
                if i + 1 < block.instructions.len()
                    && let MachineInstruction::Compare {
                        lhs: MachineOperand::Immediate(idx),
                        rhs: MachineOperand::Immediate(len),
                    } = &block.instructions[i]
                    && *idx >= 0
                    && *idx < *len
                    && let MachineInstruction::BranchCc {
                        cc: ConditionCode::AboveOrEqual | ConditionCode::GreaterOrEqual,
                        ..
                    } = &block.instructions[i + 1]
                {
                    // Statically proven within bounds! Eliminate compare and branch.
                    block.instructions.remove(i + 1);
                    block.instructions.remove(i);
                    eliminated_checks += 1;
                    continue;
                }
                i += 1;
            }
        }

        eliminated_checks
    }

    /// Lower array element address computation with scale factor (1, 2, 4, 8) into indexed memory operand.
    pub fn compute_element_address(
        base: MachineRegister,
        index: MachineRegister,
        elem_size: u8,
        offset: i32,
    ) -> MachineOperand {
        let scale = match elem_size {
            1 => 1,
            2 => 2,
            4 => 4,
            8 => 8,
            _ => 1,
        };

        MachineOperand::Memory {
            base,
            offset,
            index: Some((index, scale)),
        }
    }
}
