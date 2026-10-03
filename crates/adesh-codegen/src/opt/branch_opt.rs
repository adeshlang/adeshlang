//! Branch Optimization and Basic Block Layout Simplification.
//!
//! Performs jump-to-jump threading, empty block elimination, branch inversion,
//! and fallthrough layout optimization.

use crate::error::CodegenError;
use crate::machine_ir::{ConditionCode, MachineFunction, MachineInstruction};
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

pub struct BranchOptimizationPass;

impl Default for BranchOptimizationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl BranchOptimizationPass {
    pub fn new() -> Self {
        Self
    }

    fn invert_cc(cc: ConditionCode) -> ConditionCode {
        match cc {
            ConditionCode::Equal => ConditionCode::NotEqual,
            ConditionCode::NotEqual => ConditionCode::Equal,
            ConditionCode::LessThan => ConditionCode::GreaterOrEqual,
            ConditionCode::LessOrEqual => ConditionCode::GreaterThan,
            ConditionCode::GreaterThan => ConditionCode::LessOrEqual,
            ConditionCode::GreaterOrEqual => ConditionCode::LessThan,
            ConditionCode::Below => ConditionCode::AboveOrEqual,
            ConditionCode::BelowOrEqual => ConditionCode::Above,
            ConditionCode::Above => ConditionCode::BelowOrEqual,
            ConditionCode::AboveOrEqual => ConditionCode::Below,
            ConditionCode::Zero => ConditionCode::NotZero,
            ConditionCode::NotZero => ConditionCode::Zero,
            ConditionCode::Parity => ConditionCode::NotParity,
            ConditionCode::NotParity => ConditionCode::Parity,
        }
    }
}

impl MachinePass for BranchOptimizationPass {
    fn name(&self) -> &'static str {
        "BranchOptimization"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;

        // 1. Thread jumps to jumps
        // Map: label -> target if block contains only `Branch { target }`
        let mut jump_targets: HashMap<String, String> = HashMap::new();
        for block in &func.blocks {
            if block.instructions.len() == 1
                && let MachineInstruction::Branch { target } = &block.instructions[0]
                && target != &block.label
            {
                jump_targets.insert(block.label.clone(), target.clone());
            }
        }

        if !jump_targets.is_empty() {
            for block in &mut func.blocks {
                for inst in &mut block.instructions {
                    match inst {
                        MachineInstruction::Branch { target } => {
                            if let Some(new_target) = jump_targets.get(target) {
                                *target = new_target.clone();
                                changed = true;
                            }
                        }
                        MachineInstruction::BranchCc { target, .. } => {
                            if let Some(new_target) = jump_targets.get(target) {
                                *target = new_target.clone();
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // 2. Branch inversion & fallthrough optimization
        // Pattern:
        //   BranchCc cc, L_next
        //   Branch L_target
        //   L_next:
        // -> Invert into:
        //   BranchCc (invert_cc), L_target
        //   L_next:
        for i in 0..func.blocks.len() {
            let next_label = if i + 1 < func.blocks.len() {
                Some(func.blocks[i + 1].label.clone())
            } else {
                None
            };

            let block = &mut func.blocks[i];
            let len = block.instructions.len();
            if len >= 2
                && let (
                    MachineInstruction::BranchCc {
                        cc,
                        target: cc_target,
                    },
                    MachineInstruction::Branch { target: jmp_target },
                ) = (&block.instructions[len - 2], &block.instructions[len - 1])
                && let Some(next_lbl) = &next_label
                && cc_target == next_lbl
                && jmp_target != next_lbl
            {
                let inv = Self::invert_cc(*cc);
                let target = jmp_target.clone();
                block.instructions[len - 2] = MachineInstruction::BranchCc { cc: inv, target };
                block.instructions.pop(); // eliminate the unconditional branch!
                changed = true;
            }
        }

        if changed {
            func.rebuild_cfg();
        }

        Ok(changed)
    }
}
