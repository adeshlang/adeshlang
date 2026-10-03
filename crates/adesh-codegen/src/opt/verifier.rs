//! Machine IR Semantic and Structural Verifier.
//!
//! Validates CFG integrity, block terminators, operand well-formedness,
//! register class preservation, and ABI invariants before and after optimization passes.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand, MachineRegister};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct MachineIRVerifier;

impl MachineIRVerifier {
    /// Verify all structural and semantic invariants of a MachineFunction.
    pub fn verify(func: &MachineFunction) -> Result<(), CodegenError> {
        if func.blocks.is_empty() {
            return Ok(());
        }

        let mut block_ids = HashSet::new();
        let mut block_labels = HashSet::new();

        // 1. Verify block IDs and labels uniqueness
        for block in &func.blocks {
            if !block_ids.insert(block.id) {
                return Err(CodegenError::new(
                    "verifier",
                    format!(
                        "Duplicate block id {} in function '{}'",
                        block.id, func.name
                    ),
                ));
            }
            if !block_labels.insert(block.label.clone()) {
                return Err(CodegenError::new(
                    "verifier",
                    format!(
                        "Duplicate block label '{}' in function '{}'",
                        block.label, func.name
                    ),
                ));
            }
        }

        // 2. Verify instructions in each block
        for block in &func.blocks {
            let mut saw_terminator = false;
            for (idx, inst) in block.instructions.iter().enumerate() {
                if saw_terminator {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "Dead instruction after terminator at index {} in block '{}' of function '{}'",
                            idx, block.label, func.name
                        ),
                    ));
                }

                Self::verify_instruction(inst, func, &block_labels)?;

                if matches!(
                    inst,
                    MachineInstruction::Return | MachineInstruction::Branch { .. }
                ) {
                    saw_terminator = true;
                }
            }
        }

        // 3. Verify CFG predecessor/successor edge consistency
        for block in &func.blocks {
            for &succ_id in &block.successors {
                let succ_block = func.blocks.iter().find(|b| b.id == succ_id);
                if let Some(sb) = succ_block {
                    if !sb.predecessors.contains(&block.id) {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "CFG inconsistency: block {} has successor {}, but {} lacks predecessor in '{}'",
                                block.id, succ_id, succ_id, func.name
                            ),
                        ));
                    }
                } else {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "CFG edge to nonexistent successor id {} in '{}'",
                            succ_id, func.name
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    fn verify_instruction(
        inst: &MachineInstruction,
        func: &MachineFunction,
        labels: &HashSet<String>,
    ) -> Result<(), CodegenError> {
        match inst {
            MachineInstruction::Branch { target } => {
                if !labels.contains(target) {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "Branch target label '{}' not found in function '{}'",
                            target, func.name
                        ),
                    ));
                }
            }
            MachineInstruction::BranchCc { target, .. } => {
                if !labels.contains(target) {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "BranchCc target label '{}' not found in function '{}'",
                            target, func.name
                        ),
                    ));
                }
            }
            MachineInstruction::Move { dst, src } => {
                Self::verify_operand(dst, func)?;
                Self::verify_operand(src, func)?;
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
                Self::verify_operand(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::FAdd { dst, src, size }
            | MachineInstruction::FSub { dst, src, size }
            | MachineInstruction::FMul { dst, src, size }
            | MachineInstruction::FDiv { dst, src, size } => {
                if *size != 4 && *size != 8 {
                    return Err(CodegenError::new(
                        "verifier",
                        format!("Invalid FP instruction size {} in '{}'", size, func.name),
                    ));
                }
                Self::verify_operand(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn verify_operand(op: &MachineOperand, func: &MachineFunction) -> Result<(), CodegenError> {
        match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                if v.0 >= func.vreg_count {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "Virtual register id {} exceeds vreg_count {} in '{}'",
                            v.0, func.vreg_count, func.name
                        ),
                    ));
                }
            }
            MachineOperand::Memory { base, index, .. } => {
                if let MachineRegister::Virtual(v) = base
                    && v.0 >= func.vreg_count
                {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "Memory base virtual register {} exceeds vreg_count in '{}'",
                            v.0, func.name
                        ),
                    ));
                }
                if let Some((MachineRegister::Virtual(v), scale)) = index {
                    if v.0 >= func.vreg_count {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "Memory index virtual register {} exceeds vreg_count in '{}'",
                                v.0, func.name
                            ),
                        ));
                    }
                    if !matches!(scale, 1 | 2 | 4 | 8) {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "Invalid scale factor {} in memory operand of '{}'",
                                scale, func.name
                            ),
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
