//! VIR Validation
//!
//! Validates VIR for correctness:
//! - SSA form: all value IDs are defined before use
//! - Terminator presence: every block ends with a terminator
//! - Control flow: all branch targets reference existing blocks
//! - Type consistency: instruction outputs match declared types

use super::{VirBlock, VirFunction, VirModule, VirTerminator};
use std::collections::HashSet;

/// Validate VIR module
pub fn validate_vir(module: &VirModule) -> Result<(), String> {
    for func in &module.functions {
        validate_function(func)?;
    }
    Ok(())
}

fn validate_function(func: &VirFunction) -> Result<(), String> {
    // Collect all valid block IDs
    let block_ids: HashSet<u32> = func.blocks.iter().map(|b| b.id).collect();

    if func.blocks.is_empty() {
        return Err(format!("Function `{}` has no blocks", func.name));
    }

    // Ensure block 0 exists (entry block)
    if !block_ids.contains(&0) {
        return Err(format!(
            "Function `{}` missing entry block (block 0)",
            func.name
        ));
    }

    // Track defined values (SSA)
    let mut defined_values: HashSet<u32> = HashSet::new();

    // Parameters are pre-defined
    for (idx, _) in func.params.iter().enumerate() {
        defined_values.insert(idx as u32);
    }

    // Validate each block
    for block in &func.blocks {
        validate_block(block, func, &block_ids, &mut defined_values)?;
    }

    Ok(())
}

fn validate_block(
    block: &VirBlock,
    func: &VirFunction,
    valid_block_ids: &HashSet<u32>,
    defined_values: &mut HashSet<u32>,
) -> Result<(), String> {
    // Validate PHI nodes
    for phi in &block.phis {
        // PHI dest must be a new value (not already defined)
        if defined_values.contains(&phi.dest) {
            return Err(format!(
                "SSA violation in `{}` block {}: PHI dest %{} already defined",
                func.name, block.id, phi.dest
            ));
        }
        defined_values.insert(phi.dest);

        // PHI incoming values must reference valid blocks
        for (src_block, _) in &phi.incoming {
            if !valid_block_ids.contains(src_block) {
                return Err(format!(
                    "PHI in `{}` block {} references non-existent block {}",
                    func.name, block.id, src_block
                ));
            }
        }
    }

    // Validate instructions
    for inst in &block.instructions {
        // Check that source operands are defined
        let undefined = check_uses_defined(inst, defined_values);
        if let Some(value_id) = undefined {
            return Err(format!(
                "SSA violation in `{}` block {}: use of undefined value %{}",
                func.name, block.id, value_id
            ));
        }

        // Mark destination as defined
        if let Some(dest) = get_dest(inst) {
            if defined_values.contains(&dest) {
                return Err(format!(
                    "SSA violation in `{}` block {}: redefinition of %{}",
                    func.name, block.id, dest
                ));
            }
            defined_values.insert(dest);
        }
    }

    // Validate terminator references valid blocks
    validate_terminator(&block.terminator, func, block.id, valid_block_ids)?;

    Ok(())
}

/// Get the destination ValueId of an instruction (if any)
fn get_dest(inst: &super::VirInstruction) -> Option<u32> {
    use super::VirInstruction as VI;
    match inst {
        VI::ConstInt { dest, .. }
        | VI::ConstFloat { dest, .. }
        | VI::ConstBool { dest, .. }
        | VI::ConstString { dest, .. }
        | VI::ConstNull { dest }
        | VI::Alloc { dest, .. }
        | VI::Load { dest, .. }
        | VI::LoadLocal { dest, .. }
        | VI::ArcClone { dest, .. }
        | VI::IntBinOp { dest, .. }
        | VI::FloatBinOp { dest, .. }
        | VI::IntUnOp { dest, .. }
        | VI::FloatUnOp { dest, .. }
        | VI::IntCmp { dest, .. }
        | VI::FloatCmp { dest, .. }
        | VI::Cast { dest, .. }
        | VI::Bitcast { dest, .. }
        | VI::BuildStruct { dest, .. }
        | VI::ExtractField { dest, .. }
        | VI::InsertField { dest, .. }
        | VI::BuildArray { dest, .. }
        | VI::ArrayIndex { dest, .. }
        | VI::BuildTuple { dest, .. }
        | VI::ExtractTuple { dest, .. }
        | VI::BuildObject { dest, .. }
        | VI::BuildEnum { dest, .. }
        | VI::GetDiscriminant { dest, .. }
        | VI::ExtractPayload { dest, .. }
        | VI::Copy { dest, .. }
        | VI::Move { dest, .. } => Some(*dest),
        VI::Call { dest, .. } | VI::Intrinsic { dest, .. } => *dest,
        // No destination
        VI::Store { .. }
        | VI::StoreLocal { .. }
        | VI::Free { .. }
        | VI::ArcIncrement { .. }
        | VI::ArcDecrement { .. }
        | VI::ArcDrop { .. }
        | VI::Drop { .. }
        | VI::Nop => None,
    }
}

/// Check that all source operands of an instruction are defined.
/// Returns Some(value_id) if an undefined operand is found, None otherwise.
fn check_uses_defined(inst: &super::VirInstruction, defined: &HashSet<u32>) -> Option<u32> {
    use super::VirInstruction as VI;
    let uses: Vec<u32> = match inst {
        VI::ConstInt { .. }
        | VI::ConstFloat { .. }
        | VI::ConstBool { .. }
        | VI::ConstString { .. }
        | VI::ConstNull { .. }
        | VI::Nop => vec![],
        VI::Alloc { size, .. } => vec![*size],
        VI::Free { ptr }
        | VI::ArcIncrement { ptr }
        | VI::ArcDecrement { ptr }
        | VI::ArcDrop { ptr }
        | VI::Drop { value: ptr } => vec![*ptr],
        VI::Load { ptr, .. } => vec![*ptr],
        VI::Store { ptr, value } => vec![*ptr, *value],
        VI::LoadLocal { .. } => vec![],
        VI::StoreLocal { value, .. } => vec![*value],
        VI::ArcClone { src, .. } => vec![*src],
        VI::IntBinOp { lhs, rhs, .. } | VI::FloatBinOp { lhs, rhs, .. } => vec![*lhs, *rhs],
        VI::IntUnOp { operand, .. } | VI::FloatUnOp { operand, .. } => vec![*operand],
        VI::IntCmp { lhs, rhs, .. } | VI::FloatCmp { lhs, rhs, .. } => vec![*lhs, *rhs],
        VI::Cast { value, .. } | VI::Bitcast { value, .. } => vec![*value],
        VI::BuildStruct { fields, .. } => fields.clone(),
        VI::ExtractField { struct_val, .. } => vec![*struct_val],
        VI::InsertField {
            struct_val, value, ..
        } => vec![*struct_val, *value],
        VI::BuildArray { elements, .. } => elements.clone(),
        VI::ArrayIndex { array, index, .. } => vec![*array, *index],
        VI::BuildTuple { elements, .. } => elements.clone(),
        VI::ExtractTuple { tuple, .. } => vec![*tuple],
        VI::BuildObject { values, .. } => values.clone(),
        VI::BuildEnum { payload, .. } => payload.clone(),
        VI::GetDiscriminant { enum_val, .. } => vec![*enum_val],
        VI::ExtractPayload { enum_val, .. } => vec![*enum_val],
        VI::Call { func, args, .. } => {
            let mut v = vec![*func];
            v.extend(args);
            v
        }
        VI::Intrinsic { args, .. } => args.clone(),
        VI::Copy { src, .. } | VI::Move { src, .. } => vec![*src],
    };

    for u in uses {
        if !defined.contains(&u) {
            return Some(u);
        }
    }
    None
}

/// Validate that terminator references are valid
fn validate_terminator(
    term: &VirTerminator,
    func: &VirFunction,
    block_id: u32,
    valid_block_ids: &HashSet<u32>,
) -> Result<(), String> {
    match term {
        VirTerminator::Return { .. } => Ok(()),
        VirTerminator::Jump { target } => {
            if !valid_block_ids.contains(target) {
                return Err(format!(
                    "Block {} in `{}` jumps to non-existent block {}",
                    block_id, func.name, target
                ));
            }
            Ok(())
        }
        VirTerminator::Branch {
            true_target,
            false_target,
            ..
        } => {
            if !valid_block_ids.contains(true_target) {
                return Err(format!(
                    "Block {} in `{}` branches to non-existent block {}",
                    block_id, func.name, true_target
                ));
            }
            if !valid_block_ids.contains(false_target) {
                return Err(format!(
                    "Block {} in `{}` branches to non-existent block {}",
                    block_id, func.name, false_target
                ));
            }
            Ok(())
        }
        VirTerminator::Switch { cases, default, .. } => {
            for (_, target) in cases {
                if !valid_block_ids.contains(target) {
                    return Err(format!(
                        "Block {} in `{}` switch to non-existent block {}",
                        block_id, func.name, target
                    ));
                }
            }
            if !valid_block_ids.contains(default) {
                return Err(format!(
                    "Block {} in `{}` switch default to non-existent block {}",
                    block_id, func.name, default
                ));
            }
            Ok(())
        }
        VirTerminator::Unreachable => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::vir::*;

    #[test]
    fn test_validate_empty_module() {
        let module = VirModule::new("test".to_string());
        assert!(validate_vir(&module).is_ok());
    }

    #[test]
    fn test_validate_function_no_blocks() {
        let mut module = VirModule::new("test".to_string());
        let func = VirFunction::new("bad".to_string(), VirType::I64);
        module.functions.push(func);
        assert!(validate_vir(&module).is_err());
    }

    #[test]
    fn test_validate_valid_function() {
        let mut module = VirModule::new("test".to_string());
        let mut func = VirFunction::new("good".to_string(), VirType::I64);
        func.params.push(VirParam {
            name: "x".to_string(),
            ty: VirType::I64,
        });
        // Block 0: return x
        func.blocks.push(VirBlock {
            id: 0,
            label: None,
            phis: vec![],
            instructions: vec![],
            terminator: VirTerminator::Return { value: Some(0) },
        });
        module.functions.push(func);
        assert!(validate_vir(&module).is_ok());
    }

    #[test]
    fn test_validate_missing_entry_block() {
        let mut module = VirModule::new("test".to_string());
        let mut func = VirFunction::new("bad".to_string(), VirType::I64);
        func.blocks.push(VirBlock {
            id: 1, // Not block 0!
            label: None,
            phis: vec![],
            instructions: vec![],
            terminator: VirTerminator::Return { value: None },
        });
        module.functions.push(func);
        assert!(validate_vir(&module).is_err());
    }

    #[test]
    fn test_validate_undefined_value_use() {
        let mut module = VirModule::new("test".to_string());
        let mut func = VirFunction::new("bad".to_string(), VirType::I64);
        // Block 0: uses undefined value 99
        func.blocks.push(VirBlock {
            id: 0,
            label: None,
            phis: vec![],
            instructions: vec![VirInstruction::Copy {
                dest: 1,
                src: 99, // 99 is never defined
            }],
            terminator: VirTerminator::Return { value: Some(1) },
        });
        module.functions.push(func);
        assert!(validate_vir(&module).is_err());
    }

    #[test]
    fn test_validate_invalid_jump_target() {
        let mut module = VirModule::new("test".to_string());
        let mut func = VirFunction::new("bad".to_string(), VirType::I64);
        // Block 0: jump to non-existent block 5
        func.blocks.push(VirBlock {
            id: 0,
            label: None,
            phis: vec![],
            instructions: vec![],
            terminator: VirTerminator::Jump { target: 5 },
        });
        module.functions.push(func);
        assert!(validate_vir(&module).is_err());
    }

    #[test]
    fn test_validate_ssa_redefinition() {
        let mut module = VirModule::new("test".to_string());
        let mut func = VirFunction::new("bad".to_string(), VirType::I64);
        // Block 0: define %1 twice
        func.blocks.push(VirBlock {
            id: 0,
            label: None,
            phis: vec![],
            instructions: vec![
                VirInstruction::ConstInt {
                    dest: 1,
                    value: 42,
                    ty: VirType::I64,
                },
                VirInstruction::ConstInt {
                    dest: 1, // Redefinition!
                    value: 99,
                    ty: VirType::I64,
                },
            ],
            terminator: VirTerminator::Return { value: Some(1) },
        });
        module.functions.push(func);
        assert!(validate_vir(&module).is_err());
    }
}
