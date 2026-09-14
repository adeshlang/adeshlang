//! Dead Code Elimination
//!
//! Removes unused instructions, blocks, and values.

use super::{OptLevel, OptResult, VirOptimization};
use crate::ir::vir::{ValueId, VirFunction, VirModule};
use std::collections::HashSet;

pub struct DeadCodeElimination;

impl DeadCodeElimination {
    pub fn new() -> Self {
        Self
    }

    /// Mark live values starting from function return
    fn mark_live_values(func: &VirFunction) -> HashSet<ValueId> {
        let mut live = HashSet::new();

        // Start with values used in terminators and side-effecting instructions
        for block in &func.blocks {
            // Mark terminator values as live
            match &block.terminator {
                crate::ir::vir::VirTerminator::Return { value: Some(v) } => {
                    live.insert(*v);
                }
                crate::ir::vir::VirTerminator::Branch { cond, .. } => {
                    live.insert(*cond);
                }
                crate::ir::vir::VirTerminator::Switch { value, .. } => {
                    live.insert(*value);
                }
                _ => {}
            }

            // Mark values from side-effecting instructions
            for inst in &block.instructions {
                if has_side_effect(inst) {
                    mark_instruction_inputs(inst, &mut live);
                }
            }
        }

        // Propagate liveness backwards
        let mut changed = true;
        while changed {
            changed = false;

            for block in &func.blocks {
                for inst in &block.instructions {
                    if let Some(dest) = get_dest(inst) {
                        if live.contains(&dest) {
                            // This value is live, mark inputs as live
                            let old_len = live.len();
                            mark_instruction_inputs(inst, &mut live);
                            changed |= live.len() > old_len;
                        }
                    }
                }
            }
        }

        live
    }
}

impl Default for DeadCodeElimination {
    fn default() -> Self {
        Self::new()
    }
}

impl VirOptimization for DeadCodeElimination {
    fn name(&self) -> &str {
        "dead-code-elimination"
    }

    fn apply(&self, module: &mut VirModule) -> OptResult<bool> {
        let mut changed = false;

        for func in &mut module.functions {
            let live = Self::mark_live_values(func);

            // Remove dead instructions
            for block in &mut func.blocks {
                let original_len = block.instructions.len();

                block.instructions.retain(|inst| {
                    if has_side_effect(inst) {
                        true // Always keep side-effecting instructions
                    } else if let Some(dest) = get_dest(inst) {
                        live.contains(&dest) // Keep if destination is live
                    } else {
                        false // Remove if no dest and no side effects
                    }
                });

                changed |= block.instructions.len() < original_len;
            }
        }

        Ok(changed)
    }

    fn enabled_at(&self, level: OptLevel) -> bool {
        level >= OptLevel::Basic
    }
}

fn has_side_effect(inst: &crate::ir::vir::VirInstruction) -> bool {
    use crate::ir::vir::VirInstruction;

    matches!(
        inst,
        VirInstruction::Store { .. }
            | VirInstruction::StoreLocal { .. }
            | VirInstruction::Free { .. }
            | VirInstruction::ArcIncrement { .. }
            | VirInstruction::ArcDecrement { .. }
            | VirInstruction::ArcDrop { .. }
            | VirInstruction::Drop { .. }
            | VirInstruction::Call { .. }
            | VirInstruction::Intrinsic { .. }
    )
}

fn get_dest(inst: &crate::ir::vir::VirInstruction) -> Option<ValueId> {
    use crate::ir::vir::VirInstruction;

    match inst {
        VirInstruction::ConstInt { dest, .. }
        | VirInstruction::ConstFloat { dest, .. }
        | VirInstruction::ConstBool { dest, .. }
        | VirInstruction::ConstString { dest, .. }
        | VirInstruction::ConstNull { dest }
        | VirInstruction::Alloc { dest, .. }
        | VirInstruction::Load { dest, .. }
        | VirInstruction::LoadLocal { dest, .. }
        | VirInstruction::ArcClone { dest, .. }
        | VirInstruction::IntBinOp { dest, .. }
        | VirInstruction::FloatBinOp { dest, .. }
        | VirInstruction::IntUnOp { dest, .. }
        | VirInstruction::FloatUnOp { dest, .. }
        | VirInstruction::IntCmp { dest, .. }
        | VirInstruction::FloatCmp { dest, .. }
        | VirInstruction::Cast { dest, .. }
        | VirInstruction::Bitcast { dest, .. }
        | VirInstruction::BuildStruct { dest, .. }
        | VirInstruction::ExtractField { dest, .. }
        | VirInstruction::InsertField { dest, .. }
        | VirInstruction::BuildArray { dest, .. }
        | VirInstruction::ArrayIndex { dest, .. }
        | VirInstruction::BuildTuple { dest, .. }
        | VirInstruction::ExtractTuple { dest, .. }
        | VirInstruction::BuildObject { dest, .. }
        | VirInstruction::BuildEnum { dest, .. }
        | VirInstruction::GetDiscriminant { dest, .. }
        | VirInstruction::ExtractPayload { dest, .. }
        | VirInstruction::Copy { dest, .. }
        | VirInstruction::Move { dest, .. } => Some(*dest),
        VirInstruction::Call { dest: Some(d), .. }
        | VirInstruction::Intrinsic { dest: Some(d), .. } => Some(*d),
        _ => None,
    }
}

fn mark_instruction_inputs(inst: &crate::ir::vir::VirInstruction, live: &mut HashSet<ValueId>) {
    use crate::ir::vir::VirInstruction;

    match inst {
        VirInstruction::Load { ptr, .. } => {
            live.insert(*ptr);
        }
        VirInstruction::Store { ptr, value, .. } => {
            live.insert(*ptr);
            live.insert(*value);
        }
        VirInstruction::IntBinOp { lhs, rhs, .. }
        | VirInstruction::FloatBinOp { lhs, rhs, .. }
        | VirInstruction::IntCmp { lhs, rhs, .. }
        | VirInstruction::FloatCmp { lhs, rhs, .. } => {
            live.insert(*lhs);
            live.insert(*rhs);
        }
        VirInstruction::IntUnOp { operand, .. } | VirInstruction::FloatUnOp { operand, .. } => {
            live.insert(*operand);
        }
        VirInstruction::Call { func, args, .. } => {
            live.insert(*func);
            for arg in args {
                live.insert(*arg);
            }
        }
        VirInstruction::Copy { src, .. } | VirInstruction::Move { src, .. } => {
            live.insert(*src);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dce_enabled() {
        let dce = DeadCodeElimination::new();
        assert!(!dce.enabled_at(OptLevel::None));
        assert!(dce.enabled_at(OptLevel::Basic));
        assert!(dce.enabled_at(OptLevel::Aggressive));
    }
}
