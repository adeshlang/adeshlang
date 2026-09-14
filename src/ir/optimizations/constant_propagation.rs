//! Constant Propagation
//!
//! Propagates constant values through the program.
//! If we know %1 = 5, replace all uses of %1 with 5.

use super::{OptLevel, OptResult, VirOptimization};
use crate::ir::vir::{ValueId, VirInstruction, VirModule};
use std::collections::HashMap;

pub struct ConstantPropagation;

impl ConstantPropagation {
    pub fn new() -> Self {
        Self
    }

    /// Replace value ID uses in an instruction
    fn replace_uses(inst: &mut VirInstruction, replacements: &HashMap<ValueId, ValueId>) {
        match inst {
            VirInstruction::Copy { src, .. } | VirInstruction::Move { src, .. } => {
                if let Some(&new_src) = replacements.get(src) {
                    *src = new_src;
                }
            }

            VirInstruction::Load { ptr, .. }
            | VirInstruction::Free { ptr }
            | VirInstruction::ArcIncrement { ptr }
            | VirInstruction::ArcDecrement { ptr }
            | VirInstruction::ArcDrop { ptr }
            | VirInstruction::Drop { value: ptr } => {
                if let Some(&new_ptr) = replacements.get(ptr) {
                    *ptr = new_ptr;
                }
            }

            VirInstruction::Store { ptr, value } => {
                if let Some(&new_ptr) = replacements.get(ptr) {
                    *ptr = new_ptr;
                }
                if let Some(&new_value) = replacements.get(value) {
                    *value = new_value;
                }
            }

            VirInstruction::IntBinOp { lhs, rhs, .. }
            | VirInstruction::FloatBinOp { lhs, rhs, .. }
            | VirInstruction::IntCmp { lhs, rhs, .. }
            | VirInstruction::FloatCmp { lhs, rhs, .. } => {
                if let Some(&new_lhs) = replacements.get(lhs) {
                    *lhs = new_lhs;
                }
                if let Some(&new_rhs) = replacements.get(rhs) {
                    *rhs = new_rhs;
                }
            }

            VirInstruction::IntUnOp { operand, .. } | VirInstruction::FloatUnOp { operand, .. } => {
                if let Some(&new_operand) = replacements.get(operand) {
                    *operand = new_operand;
                }
            }

            VirInstruction::Cast { value, .. }
            | VirInstruction::Bitcast { value, .. }
            | VirInstruction::ArcClone { src: value, .. } => {
                if let Some(&new_value) = replacements.get(value) {
                    *value = new_value;
                }
            }

            VirInstruction::BuildStruct { fields, .. }
            | VirInstruction::BuildArray {
                elements: fields, ..
            }
            | VirInstruction::BuildTuple {
                elements: fields, ..
            } => {
                for field in fields.iter_mut() {
                    if let Some(&new_field) = replacements.get(field) {
                        *field = new_field;
                    }
                }
            }

            VirInstruction::BuildObject { values, .. } => {
                for value in values.iter_mut() {
                    if let Some(&new_value) = replacements.get(value) {
                        *value = new_value;
                    }
                }
            }

            VirInstruction::ExtractField { struct_val, .. }
            | VirInstruction::GetDiscriminant {
                enum_val: struct_val,
                ..
            }
            | VirInstruction::ExtractTuple {
                tuple: struct_val, ..
            } => {
                if let Some(&new_val) = replacements.get(struct_val) {
                    *struct_val = new_val;
                }
            }

            VirInstruction::InsertField {
                struct_val, value, ..
            } => {
                if let Some(&new_struct) = replacements.get(struct_val) {
                    *struct_val = new_struct;
                }
                if let Some(&new_value) = replacements.get(value) {
                    *value = new_value;
                }
            }

            VirInstruction::ArrayIndex { array, index, .. } => {
                if let Some(&new_array) = replacements.get(array) {
                    *array = new_array;
                }
                if let Some(&new_index) = replacements.get(index) {
                    *index = new_index;
                }
            }

            VirInstruction::BuildEnum { payload, .. } => {
                for val in payload.iter_mut() {
                    if let Some(&new_val) = replacements.get(val) {
                        *val = new_val;
                    }
                }
            }

            VirInstruction::ExtractPayload { enum_val, .. } => {
                if let Some(&new_val) = replacements.get(enum_val) {
                    *enum_val = new_val;
                }
            }

            VirInstruction::Call { func, args, .. } => {
                if let Some(&new_func) = replacements.get(func) {
                    *func = new_func;
                }
                for arg in args.iter_mut() {
                    if let Some(&new_arg) = replacements.get(arg) {
                        *arg = new_arg;
                    }
                }
            }

            VirInstruction::Intrinsic { args, .. } => {
                for arg in args.iter_mut() {
                    if let Some(&new_arg) = replacements.get(arg) {
                        *arg = new_arg;
                    }
                }
            }

            VirInstruction::StoreLocal { value, .. }
            | VirInstruction::Alloc { size: value, .. } => {
                if let Some(&new_value) = replacements.get(value) {
                    *value = new_value;
                }
            }

            // Constants don't have uses to replace
            VirInstruction::ConstInt { .. }
            | VirInstruction::ConstFloat { .. }
            | VirInstruction::ConstBool { .. }
            | VirInstruction::ConstString { .. }
            | VirInstruction::ConstNull { .. }
            | VirInstruction::LoadLocal { .. }
            | VirInstruction::Nop => {}
        }
    }
}

impl Default for ConstantPropagation {
    fn default() -> Self {
        Self::new()
    }
}

impl VirOptimization for ConstantPropagation {
    fn name(&self) -> &str {
        "constant-propagation"
    }

    fn apply(&self, module: &mut VirModule) -> OptResult<bool> {
        let mut changed = false;

        // For each function, find constant definitions and propagate them
        for func in &mut module.functions {
            let mut replacements: HashMap<ValueId, ValueId> = HashMap::new();

            // First pass: find values that are just copies of constants
            for block in &func.blocks {
                for inst in &block.instructions {
                    match inst {
                        VirInstruction::Copy { dest, src } | VirInstruction::Move { dest, src } => {
                            // If src is a constant, remember that dest can be replaced with src
                            if Self::is_constant_def(src, &func.blocks) {
                                replacements.insert(*dest, *src);
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Second pass: apply replacements
            if !replacements.is_empty() {
                for block in &mut func.blocks {
                    for inst in &mut block.instructions {
                        Self::replace_uses(inst, &replacements);
                    }

                    // Also replace in terminators
                    if let crate::ir::vir::VirTerminator::Branch { cond, .. } =
                        &mut block.terminator
                    {
                        if let Some(&new_cond) = replacements.get(cond) {
                            *cond = new_cond;
                        }
                    } else if let crate::ir::vir::VirTerminator::Switch { value, .. } =
                        &mut block.terminator
                    {
                        if let Some(&new_value) = replacements.get(value) {
                            *value = new_value;
                        }
                    } else if let crate::ir::vir::VirTerminator::Return { value: Some(val) } =
                        &mut block.terminator
                    {
                        if let Some(&new_val) = replacements.get(val) {
                            *val = new_val;
                        }
                    }
                }
            }
        }

        Ok(changed)
    }

    fn enabled_at(&self, level: OptLevel) -> bool {
        level >= OptLevel::Basic
    }
}

impl ConstantPropagation {
    /// Check if a value is defined as a constant
    fn is_constant_def(value_id: &ValueId, blocks: &[crate::ir::vir::VirBlock]) -> bool {
        for block in blocks {
            for inst in &block.instructions {
                match inst {
                    VirInstruction::ConstInt { dest, .. }
                    | VirInstruction::ConstFloat { dest, .. }
                    | VirInstruction::ConstBool { dest, .. }
                    | VirInstruction::ConstString { dest, .. }
                    | VirInstruction::ConstNull { dest } => {
                        if dest == value_id {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }
}
