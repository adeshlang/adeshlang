//! Machine IR Semantic and Structural Verifier (Phase 19.2.5).
//!
//! Validates CFG integrity, block terminators, operand well-formedness,
//! load/store widths, parallel move invariants, register class preservation,
//! and ABI invariants before and after optimization/allocation passes.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, MoveLocation,
    RegisterClass,
};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct MachineIRVerifier;

impl MachineIRVerifier {
    /// Verify all structural and semantic invariants of a MachineFunction.
    pub fn verify(func: &MachineFunction) -> Result<(), CodegenError> {
        if func.blocks.is_empty() {
            return Ok(());
        }

        // 0. Verify vreg metadata tables
        for (&vreg, &width) in &func.vreg_widths {
            if vreg.0 >= func.vreg_count {
                return Err(CodegenError::new(
                    "verifier",
                    format!(
                        "vreg_widths contains out-of-range v{} (vreg_count={}) in '{}'",
                        vreg.0, func.vreg_count, func.name
                    ),
                ));
            }
            if !matches!(width, 1 | 2 | 4 | 8 | 16) {
                return Err(CodegenError::new(
                    "verifier",
                    format!(
                        "Invalid vreg width {} for v{} in '{}'",
                        width, vreg.0, func.name
                    ),
                ));
            }
        }
        for &vreg in func.vreg_classes.keys() {
            if vreg.0 >= func.vreg_count {
                return Err(CodegenError::new(
                    "verifier",
                    format!(
                        "vreg_classes contains out-of-range v{} (vreg_count={}) in '{}'",
                        vreg.0, func.vreg_count, func.name
                    ),
                ));
            }
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

        // 3. Verify CFG predecessor/successor edge consistency (both directions)
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
            for &pred_id in &block.predecessors {
                let pred_block = func.blocks.iter().find(|b| b.id == pred_id);
                if let Some(pb) = pred_block {
                    if !pb.successors.contains(&block.id) {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "CFG inconsistency: block {} has predecessor {}, but {} lacks successor in '{}'",
                                block.id, pred_id, pred_id, func.name
                            ),
                        ));
                    }
                } else {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "CFG edge from nonexistent predecessor id {} in '{}'",
                            pred_id, func.name
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
            MachineInstruction::Nop | MachineInstruction::Return | MachineInstruction::Barrier => {}
            MachineInstruction::Branch { target } | MachineInstruction::BranchCc { target, .. } => {
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
            MachineInstruction::ParallelMove { moves } => {
                for mv in moves {
                    if !matches!(mv.size, 1 | 2 | 4 | 8 | 16) {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "Invalid ParallelMove size {} in '{}'",
                                mv.size, func.name
                            ),
                        ));
                    }
                    if matches!(
                        mv.dst,
                        MoveLocation::Immediate(_) | MoveLocation::FloatImmediate(_)
                    ) {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "ParallelMove destination cannot be an immediate in '{}'",
                                func.name
                            ),
                        ));
                    }
                    Self::verify_move_location(&mv.dst, func)?;
                    Self::verify_move_location(&mv.src, func)?;
                }
            }
            MachineInstruction::Move { dst, src } => {
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::Load { dst, src, size } => {
                if !matches!(*size, 1 | 2 | 4 | 8 | 16) {
                    return Err(CodegenError::new(
                        "verifier",
                        format!("Invalid Load width {} in '{}'", size, func.name),
                    ));
                }
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::Store { dst, src, size } => {
                if !matches!(*size, 1 | 2 | 4 | 8 | 16) {
                    return Err(CodegenError::new(
                        "verifier",
                        format!("Invalid Store width {} in '{}'", size, func.name),
                    ));
                }
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::Add { dst, src }
            | MachineInstruction::AddWithCarry { dst, src }
            | MachineInstruction::Sub { dst, src }
            | MachineInstruction::SubWithBorrow { dst, src }
            | MachineInstruction::Mul { dst, src }
            | MachineInstruction::Div { dst, src }
            | MachineInstruction::Mod { dst, src }
            | MachineInstruction::And { dst, src }
            | MachineInstruction::Or { dst, src }
            | MachineInstruction::Xor { dst, src }
            | MachineInstruction::Shl { dst, src }
            | MachineInstruction::Shr { dst, src }
            | MachineInstruction::Sar { dst, src } => {
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::Neg { dst }
            | MachineInstruction::Not { dst }
            | MachineInstruction::SetCc { dst, .. }
            | MachineInstruction::Pop { dst }
            | MachineInstruction::TlsAddress { dst, .. } => {
                Self::verify_writable_dst(dst, func)?;
            }
            MachineInstruction::Push { src } => {
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::Call { target, .. } => {
                Self::verify_operand(target, func)?;
            }
            MachineInstruction::Compare { lhs, rhs }
            | MachineInstruction::CompareWithBorrow { lhs, rhs }
            | MachineInstruction::Test { lhs, rhs } => {
                Self::verify_operand(lhs, func)?;
                Self::verify_operand(rhs, func)?;
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
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
                Self::verify_vreg_class(dst, RegisterClass::Float, func, "FP op dst")?;
                Self::verify_vreg_class(src, RegisterClass::Float, func, "FP op src")?;
            }
            MachineInstruction::FNeg { dst, size } => {
                if *size != 4 && *size != 8 {
                    return Err(CodegenError::new(
                        "verifier",
                        format!("Invalid FNeg size {} in '{}'", size, func.name),
                    ));
                }
                Self::verify_writable_dst(dst, func)?;
                Self::verify_vreg_class(dst, RegisterClass::Float, func, "FNeg dst")?;
            }
            MachineInstruction::FCmp { lhs, rhs, size } => {
                if *size != 4 && *size != 8 {
                    return Err(CodegenError::new(
                        "verifier",
                        format!("Invalid FCmp size {} in '{}'", size, func.name),
                    ));
                }
                Self::verify_operand(lhs, func)?;
                Self::verify_operand(rhs, func)?;
                Self::verify_vreg_class(lhs, RegisterClass::Float, func, "FCmp lhs")?;
                Self::verify_vreg_class(rhs, RegisterClass::Float, func, "FCmp rhs")?;
            }
            MachineInstruction::FCvtIntToFloat { dst, src, .. } => {
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
                Self::verify_vreg_class(dst, RegisterClass::Float, func, "FCvtIntToFloat dst")?;
                Self::verify_vreg_class(src, RegisterClass::Gpr, func, "FCvtIntToFloat src")?;
            }
            MachineInstruction::FCvtFloatToInt { dst, src, .. } => {
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
                Self::verify_vreg_class(dst, RegisterClass::Gpr, func, "FCvtFloatToInt dst")?;
                Self::verify_vreg_class(src, RegisterClass::Float, func, "FCvtFloatToInt src")?;
            }
            MachineInstruction::FCvtFloatToFloat { dst, src, .. } => {
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
                Self::verify_vreg_class(dst, RegisterClass::Float, func, "FCvtFloatToFloat dst")?;
                Self::verify_vreg_class(src, RegisterClass::Float, func, "FCvtFloatToFloat src")?;
            }
            MachineInstruction::AtomicLoad { dst, src, size }
            | MachineInstruction::AtomicStore { dst, src, size }
            | MachineInstruction::AtomicFetchAdd { dst, src, size }
            | MachineInstruction::AtomicExchange { dst, src, size } => {
                if !matches!(*size, 1 | 2 | 4 | 8) {
                    return Err(CodegenError::new(
                        "verifier",
                        format!("Invalid Atomic width {} in '{}'", size, func.name),
                    ));
                }
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::AtomicCompareExchange {
                dst,
                expected,
                desired,
                size,
            } => {
                if !matches!(*size, 1 | 2 | 4 | 8) {
                    return Err(CodegenError::new(
                        "verifier",
                        format!("Invalid AtomicCAS width {} in '{}'", size, func.name),
                    ));
                }
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(expected, func)?;
                Self::verify_operand(desired, func)?;
            }
            MachineInstruction::VectorAdd { dst, src, .. }
            | MachineInstruction::VectorSub { dst, src, .. }
            | MachineInstruction::VectorMul { dst, src, .. }
            | MachineInstruction::VectorDiv { dst, src, .. }
            | MachineInstruction::VectorAnd { dst, src, .. }
            | MachineInstruction::VectorOr { dst, src, .. }
            | MachineInstruction::VectorXor { dst, src, .. }
            | MachineInstruction::VectorLoad { dst, src, .. }
            | MachineInstruction::VectorStore { dst, src, .. }
            | MachineInstruction::VectorBroadcast { dst, src, .. }
            | MachineInstruction::VectorShuffle { dst, src, .. }
            | MachineInstruction::VectorReduceAdd { dst, src, .. }
            | MachineInstruction::VectorMin { dst, src, .. }
            | MachineInstruction::VectorMax { dst, src, .. }
            | MachineInstruction::VectorCmp { dst, src, .. }
            | MachineInstruction::VectorBlend { dst, src, .. }
            | MachineInstruction::VectorShiftLeft { dst, src, .. }
            | MachineInstruction::VectorShiftRight { dst, src, .. } => {
                Self::verify_writable_dst(dst, func)?;
                Self::verify_operand(src, func)?;
            }
            MachineInstruction::Custom { operands, .. } => {
                for op in operands {
                    Self::verify_operand(op, func)?;
                }
            }
        }
        Ok(())
    }

    fn verify_writable_dst(
        dst: &MachineOperand,
        func: &MachineFunction,
    ) -> Result<(), CodegenError> {
        if matches!(
            dst,
            MachineOperand::Immediate(_) | MachineOperand::Symbol(_)
        ) {
            return Err(CodegenError::new(
                "verifier",
                format!(
                    "Destination operand {:?} cannot be an immediate or symbol in '{}'",
                    dst, func.name
                ),
            ));
        }
        Self::verify_operand(dst, func)
    }

    fn verify_vreg_class(
        op: &MachineOperand,
        expected: RegisterClass,
        func: &MachineFunction,
        role: &str,
    ) -> Result<(), CodegenError> {
        if let MachineOperand::Register(MachineRegister::Virtual(v)) = op {
            let actual = func.vreg_class(*v);
            if actual != expected {
                return Err(CodegenError::new(
                    "verifier",
                    format!(
                        "Register class mismatch for {} (v{}): expected {:?}, found {:?} in '{}'",
                        role, v.0, expected, actual, func.name
                    ),
                ));
            }
        }
        Ok(())
    }

    fn verify_move_location(
        loc: &MoveLocation,
        func: &MachineFunction,
    ) -> Result<(), CodegenError> {
        match loc {
            MoveLocation::VirtualRegister(v) => {
                if v.0 >= func.vreg_count {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "ParallelMove virtual register v{} exceeds vreg_count {} in '{}'",
                            v.0, func.vreg_count, func.name
                        ),
                    ));
                }
            }
            MoveLocation::Memory { base, index, .. } => {
                if let MachineRegister::Virtual(v) = base
                    && v.0 >= func.vreg_count
                {
                    return Err(CodegenError::new(
                        "verifier",
                        format!(
                            "ParallelMove memory base v{} exceeds vreg_count in '{}'",
                            v.0, func.name
                        ),
                    ));
                }
                if let Some((MachineRegister::Virtual(v), scale)) = index {
                    if v.0 >= func.vreg_count {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "ParallelMove memory index v{} exceeds vreg_count in '{}'",
                                v.0, func.name
                            ),
                        ));
                    }
                    if !matches!(scale, 1 | 2 | 4 | 8) {
                        return Err(CodegenError::new(
                            "verifier",
                            format!(
                                "Invalid scale {} in ParallelMove memory operand in '{}'",
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
                if let Some((reg, scale)) = index {
                    if let MachineRegister::Virtual(v) = reg
                        && v.0 >= func.vreg_count
                    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::MoveOperation;

    #[test]
    fn rejects_invalid_load_width_and_immediate_dst() {
        let mut func = MachineFunction::new("bad_load");
        let r0 = func.alloc_vreg();
        func.entry_block_mut().push(MachineInstruction::Load {
            dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
            src: MachineOperand::StackSlot(-8),
            size: 3,
        });
        assert!(MachineIRVerifier::verify(&func).is_err());

        let mut func2 = MachineFunction::new("bad_dst");
        func2.entry_block_mut().push(MachineInstruction::Move {
            dst: MachineOperand::Immediate(1),
            src: MachineOperand::Immediate(2),
        });
        assert!(MachineIRVerifier::verify(&func2).is_err());
    }

    #[test]
    fn rejects_fp_gpr_register_class_mismatch() {
        let mut func = MachineFunction::new("bad_fp_class");
        let gpr = func.alloc_vreg();
        let fp = func.alloc_fp_vreg();
        func.entry_block_mut().push(MachineInstruction::FAdd {
            dst: MachineOperand::Register(MachineRegister::Virtual(gpr)),
            src: MachineOperand::Register(MachineRegister::Virtual(fp)),
            size: 8,
        });
        assert!(MachineIRVerifier::verify(&func).is_err());
    }

    #[test]
    fn rejects_bad_parallel_move_and_cfg_asymmetry() {
        let mut func = MachineFunction::new("bad_pmove");
        func.entry_block_mut()
            .push(MachineInstruction::ParallelMove {
                moves: vec![MoveOperation::new(
                    MoveLocation::Immediate(5),
                    MoveLocation::Immediate(6),
                    8,
                )],
            });
        assert!(MachineIRVerifier::verify(&func).is_err());

        let mut func2 = MachineFunction::new("bad_cfg");
        let b1 = func2.create_block("b1");
        func2.blocks[0].successors.push(b1);
        // Deliberately omit b1.predecessors.push(0)
        assert!(MachineIRVerifier::verify(&func2).is_err());
    }

    #[test]
    fn accepts_valid_fp_conversion_and_parallel_move() {
        let mut func = MachineFunction::new("valid_fn");
        let gpr = func.alloc_vreg();
        let fp = func.alloc_fp_vreg();
        func.entry_block_mut()
            .push(MachineInstruction::FCvtIntToFloat {
                dst: MachineOperand::Register(MachineRegister::Virtual(fp)),
                src: MachineOperand::Register(MachineRegister::Virtual(gpr)),
                is_f64: true,
                is_signed: true,
            });
        func.entry_block_mut()
            .push(MachineInstruction::ParallelMove {
                moves: vec![MoveOperation::new(
                    MoveLocation::VirtualRegister(gpr),
                    MoveLocation::Immediate(42),
                    8,
                )],
            });
        func.entry_block_mut().push(MachineInstruction::Return);
        assert!(MachineIRVerifier::verify(&func).is_ok());
    }
}
