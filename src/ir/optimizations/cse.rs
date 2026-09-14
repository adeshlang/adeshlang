//! Common Subexpression Elimination (CSE)
//!
//! Eliminates redundant computations of pure operations within basic blocks.
//! For example:
//!   t1 = a + b
//!   t2 = a + b  --> replaced with t1, removing redundant evaluation.

use super::{OptLevel, OptResult, VirOptimization};
use crate::ir::vir::{ValueId, VirFunction, VirInstruction, VirModule, VirTerminator};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PureExpr {
    IntBin { op: u8, lhs: ValueId, rhs: ValueId },
    FloatBin { op: u8, lhs: ValueId, rhs: ValueId },
    IntUn { op: u8, operand: ValueId },
    FloatUn { op: u8, operand: ValueId },
    IntCmp { op: u8, lhs: ValueId, rhs: ValueId },
    FloatCmp { op: u8, lhs: ValueId, rhs: ValueId },
}

pub struct CommonSubexpressionElimination;

impl CommonSubexpressionElimination {
    pub fn new() -> Self {
        Self
    }

    fn eliminate_in_function(func: &mut VirFunction) -> bool {
        let mut changed = false;
        let mut alias_map: HashMap<ValueId, ValueId> = HashMap::new();

        for block in &mut func.blocks {
            let mut expr_map: HashMap<PureExpr, ValueId> = HashMap::new();
            let mut new_instructions = Vec::with_capacity(block.instructions.len());

            for mut inst in block.instructions.drain(..) {
                // First, replace any operands using existing aliases
                Self::remap_instruction_operands(&mut inst, &alias_map);

                let pure_expr = match &inst {
                    VirInstruction::IntBinOp { op, lhs, rhs, .. } => Some(PureExpr::IntBin {
                        op: *op as u8,
                        lhs: *lhs,
                        rhs: *rhs,
                    }),
                    VirInstruction::FloatBinOp { op, lhs, rhs, .. } => Some(PureExpr::FloatBin {
                        op: *op as u8,
                        lhs: *lhs,
                        rhs: *rhs,
                    }),
                    VirInstruction::IntUnOp { op, operand, .. } => Some(PureExpr::IntUn {
                        op: *op as u8,
                        operand: *operand,
                    }),
                    VirInstruction::FloatUnOp { op, operand, .. } => Some(PureExpr::FloatUn {
                        op: *op as u8,
                        operand: *operand,
                    }),
                    VirInstruction::IntCmp { op, lhs, rhs, .. } => Some(PureExpr::IntCmp {
                        op: *op as u8,
                        lhs: *lhs,
                        rhs: *rhs,
                    }),
                    VirInstruction::FloatCmp { op, lhs, rhs, .. } => Some(PureExpr::FloatCmp {
                        op: *op as u8,
                        lhs: *lhs,
                        rhs: *rhs,
                    }),
                    _ => None,
                };

                if let Some(expr) = pure_expr {
                    let dest = match &inst {
                        VirInstruction::IntBinOp { dest, .. } => *dest,
                        VirInstruction::FloatBinOp { dest, .. } => *dest,
                        VirInstruction::IntUnOp { dest, .. } => *dest,
                        VirInstruction::FloatUnOp { dest, .. } => *dest,
                        VirInstruction::IntCmp { dest, .. } => *dest,
                        VirInstruction::FloatCmp { dest, .. } => *dest,
                        _ => unreachable!(),
                    };

                    if let Some(&existing_dest) = expr_map.get(&expr) {
                        // Found common subexpression! Alias this dest to the existing one.
                        alias_map.insert(dest, existing_dest);
                        changed = true;
                        // Drop redundant instruction
                        continue;
                    } else {
                        expr_map.insert(expr, dest);
                    }
                }

                new_instructions.push(inst);
            }

            block.instructions = new_instructions;
        }

        // Remap terminator operands
        for block in &mut func.blocks {
            Self::remap_terminator(&mut block.terminator, &alias_map);
        }

        changed
    }

    fn remap_instruction_operands(inst: &mut VirInstruction, aliases: &HashMap<ValueId, ValueId>) {
        if aliases.is_empty() {
            return;
        }
        let remap = |v: &mut ValueId| {
            let mut curr = *v;
            while let Some(&target) = aliases.get(&curr) {
                curr = target;
            }
            *v = curr;
        };

        match inst {
            VirInstruction::IntBinOp { lhs, rhs, .. } => {
                remap(lhs);
                remap(rhs);
            }
            VirInstruction::FloatBinOp { lhs, rhs, .. } => {
                remap(lhs);
                remap(rhs);
            }
            VirInstruction::IntUnOp { operand, .. } => {
                remap(operand);
            }
            VirInstruction::FloatUnOp { operand, .. } => {
                remap(operand);
            }
            VirInstruction::IntCmp { lhs, rhs, .. } => {
                remap(lhs);
                remap(rhs);
            }
            VirInstruction::FloatCmp { lhs, rhs, .. } => {
                remap(lhs);
                remap(rhs);
            }
            VirInstruction::Store { ptr, value } => {
                remap(ptr);
                remap(value);
            }
            VirInstruction::Load { ptr, .. } => {
                remap(ptr);
            }
            VirInstruction::StoreLocal { value, .. } => {
                remap(value);
            }
            VirInstruction::Call { args, .. } => {
                for arg in args {
                    remap(arg);
                }
            }
            VirInstruction::ArrayIndex { array, index, .. } => {
                remap(array);
                remap(index);
            }
            VirInstruction::ExtractField { struct_val, .. } => {
                remap(struct_val);
            }
            VirInstruction::InsertField {
                struct_val, value, ..
            } => {
                remap(struct_val);
                remap(value);
            }
            _ => {}
        }
    }

    fn remap_terminator(term: &mut VirTerminator, aliases: &HashMap<ValueId, ValueId>) {
        if aliases.is_empty() {
            return;
        }
        let remap = |v: &mut ValueId| {
            let mut curr = *v;
            while let Some(&target) = aliases.get(&curr) {
                curr = target;
            }
            *v = curr;
        };

        match term {
            VirTerminator::Return { value: Some(val) } => {
                remap(val);
            }
            VirTerminator::Branch { cond, .. } => {
                remap(cond);
            }
            VirTerminator::Switch { value, .. } => {
                remap(value);
            }
            _ => {}
        }
    }
}

impl VirOptimization for CommonSubexpressionElimination {
    fn name(&self) -> &str {
        "Common Subexpression Elimination"
    }

    fn apply(&self, module: &mut VirModule) -> OptResult<bool> {
        let mut changed = false;
        for func in &mut module.functions {
            changed |= Self::eliminate_in_function(func);
        }
        Ok(changed)
    }

    fn enabled_at(&self, level: OptLevel) -> bool {
        level >= OptLevel::Basic
    }
}
