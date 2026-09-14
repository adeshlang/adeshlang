//! Constant Folding
//!
//! Evaluates constant expressions at compile time.
//! Examples: 2+3 -> 5, 10/2 -> 5, true && false -> false

use super::{OptLevel, OptResult, VirOptimization};
use crate::ir::vir::{
    CmpOp, FloatBinOp, FloatUnOp, IntBinOp, IntUnOp, ValueId, VirInstruction, VirModule,
};
use std::collections::HashMap;

pub struct ConstantFolding;

impl ConstantFolding {
    pub fn new() -> Self {
        Self
    }

    /// Track known constant values
    fn build_constant_map(module: &VirModule) -> HashMap<ValueId, ConstantValue> {
        let mut constants = HashMap::new();

        for func in &module.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    match inst {
                        VirInstruction::ConstInt { dest, value, .. } => {
                            constants.insert(*dest, ConstantValue::Int(*value));
                        }
                        VirInstruction::ConstFloat { dest, value, .. } => {
                            constants.insert(*dest, ConstantValue::Float(*value));
                        }
                        VirInstruction::ConstBool { dest, value } => {
                            constants.insert(*dest, ConstantValue::Bool(*value));
                        }
                        _ => {}
                    }
                }
            }
        }

        constants
    }

    /// Fold constant arithmetic operations
    fn fold_instruction(
        inst: &VirInstruction,
        constants: &HashMap<ValueId, ConstantValue>,
    ) -> Option<VirInstruction> {
        match inst {
            // Integer binary operations
            VirInstruction::IntBinOp {
                dest,
                op,
                lhs,
                rhs,
                ty,
            } => {
                if let (Some(ConstantValue::Int(l)), Some(ConstantValue::Int(r))) =
                    (constants.get(lhs), constants.get(rhs))
                {
                    let result = match op {
                        IntBinOp::Add => l.checked_add(*r),
                        IntBinOp::Sub => l.checked_sub(*r),
                        IntBinOp::Mul => l.checked_mul(*r),
                        IntBinOp::Div => {
                            if *r == 0 {
                                None
                            } else {
                                l.checked_div(*r)
                            }
                        }
                        IntBinOp::Rem => {
                            if *r == 0 {
                                None
                            } else {
                                l.checked_rem(*r)
                            }
                        }
                        IntBinOp::And => Some(l & r),
                        IntBinOp::Or => Some(l | r),
                        IntBinOp::Xor => Some(l ^ r),
                        IntBinOp::Shl => {
                            if *r < 0 || *r >= 64 {
                                None
                            } else {
                                l.checked_shl(*r as u32)
                            }
                        }
                        IntBinOp::Shr => {
                            if *r < 0 || *r >= 64 {
                                None
                            } else {
                                l.checked_shr(*r as u32)
                            }
                        }
                    };
                    if let Some(value) = result {
                        return Some(VirInstruction::ConstInt {
                            dest: *dest,
                            value,
                            ty: ty.clone(),
                        });
                    }
                }

                // Algebraic simplifications when one operand is a known constant
                if let Some(ConstantValue::Int(r)) = constants.get(rhs) {
                    if *r == 0 && matches!(op, IntBinOp::Mul | IntBinOp::And) {
                        return Some(VirInstruction::ConstInt {
                            dest: *dest,
                            value: 0,
                            ty: ty.clone(),
                        });
                    }
                }
                if let Some(ConstantValue::Int(l)) = constants.get(lhs) {
                    if *l == 0 && matches!(op, IntBinOp::Mul | IntBinOp::And) {
                        return Some(VirInstruction::ConstInt {
                            dest: *dest,
                            value: 0,
                            ty: ty.clone(),
                        });
                    }
                }
                if lhs == rhs {
                    match op {
                        IntBinOp::Sub | IntBinOp::Xor => {
                            return Some(VirInstruction::ConstInt {
                                dest: *dest,
                                value: 0,
                                ty: ty.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }

            // Float binary operations
            VirInstruction::FloatBinOp {
                dest,
                op,
                lhs,
                rhs,
                ty,
            } => {
                if let (Some(ConstantValue::Float(l)), Some(ConstantValue::Float(r))) =
                    (constants.get(lhs), constants.get(rhs))
                {
                    let result = match op {
                        FloatBinOp::Add => l + r,
                        FloatBinOp::Sub => l - r,
                        FloatBinOp::Mul => l * r,
                        FloatBinOp::Div => l / r,
                    };
                    return Some(VirInstruction::ConstFloat {
                        dest: *dest,
                        value: result,
                        ty: ty.clone(),
                    });
                }
            }

            // Integer unary operations
            VirInstruction::IntUnOp {
                dest,
                op,
                operand,
                ty,
            } => {
                if let Some(ConstantValue::Int(val)) = constants.get(operand) {
                    let result = match op {
                        IntUnOp::Neg => val.checked_neg(),
                        IntUnOp::Not => Some(!val),
                    };
                    if let Some(value) = result {
                        return Some(VirInstruction::ConstInt {
                            dest: *dest,
                            value,
                            ty: ty.clone(),
                        });
                    }
                }
            }

            // Float unary operations
            VirInstruction::FloatUnOp {
                dest,
                op,
                operand,
                ty,
            } => {
                if let Some(ConstantValue::Float(val)) = constants.get(operand) {
                    let result = match op {
                        FloatUnOp::Neg => -val,
                        FloatUnOp::Abs => val.abs(),
                        FloatUnOp::Sqrt => val.sqrt(),
                    };
                    return Some(VirInstruction::ConstFloat {
                        dest: *dest,
                        value: result,
                        ty: ty.clone(),
                    });
                }
            }

            // Integer comparisons
            VirInstruction::IntCmp { dest, op, lhs, rhs } => {
                if let (Some(ConstantValue::Int(l)), Some(ConstantValue::Int(r))) =
                    (constants.get(lhs), constants.get(rhs))
                {
                    let result = match op {
                        CmpOp::Eq => l == r,
                        CmpOp::Ne => l != r,
                        CmpOp::Lt => l < r,
                        CmpOp::Le => l <= r,
                        CmpOp::Gt => l > r,
                        CmpOp::Ge => l >= r,
                    };
                    return Some(VirInstruction::ConstBool {
                        dest: *dest,
                        value: result,
                    });
                }

                if lhs == rhs {
                    let result = match op {
                        CmpOp::Eq | CmpOp::Le | CmpOp::Ge => true,
                        CmpOp::Ne | CmpOp::Lt | CmpOp::Gt => false,
                    };
                    return Some(VirInstruction::ConstBool {
                        dest: *dest,
                        value: result,
                    });
                }
            }

            // Float comparisons
            VirInstruction::FloatCmp { dest, op, lhs, rhs } => {
                if let (Some(ConstantValue::Float(l)), Some(ConstantValue::Float(r))) =
                    (constants.get(lhs), constants.get(rhs))
                {
                    let result = match op {
                        CmpOp::Eq => l == r,
                        CmpOp::Ne => l != r,
                        CmpOp::Lt => l < r,
                        CmpOp::Le => l <= r,
                        CmpOp::Gt => l > r,
                        CmpOp::Ge => l >= r,
                    };
                    return Some(VirInstruction::ConstBool {
                        dest: *dest,
                        value: result,
                    });
                }
            }

            _ => {}
        }

        None
    }
}

impl Default for ConstantFolding {
    fn default() -> Self {
        Self::new()
    }
}

impl VirOptimization for ConstantFolding {
    fn name(&self) -> &str {
        "constant-folding"
    }

    fn apply(&self, module: &mut VirModule) -> OptResult<bool> {
        let mut changed = false;

        // Build map of known constants
        let constants = Self::build_constant_map(module);

        // Try to fold constant operations in each function
        for func in &mut module.functions {
            for block in &mut func.blocks {
                let mut folded_instructions = Vec::new();

                for inst in &block.instructions {
                    if let Some(folded) = Self::fold_instruction(inst, &constants) {
                        folded_instructions.push(folded);
                        changed = true;
                    } else {
                        folded_instructions.push(inst.clone());
                    }
                }

                block.instructions = folded_instructions;
            }
        }

        Ok(changed)
    }

    fn enabled_at(&self, level: OptLevel) -> bool {
        level >= OptLevel::Basic
    }
}

/// Internal representation of constant values
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum ConstantValue {
    Int(i64),
    Float(f64),
    Bool(bool),
}
