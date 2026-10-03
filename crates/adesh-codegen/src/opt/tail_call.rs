//! Tail-Call Optimization (TCO) Pass.
//!
//! Recognizes calls followed immediately by returns and transforms them into
//! direct tail jumps where calling convention and frame layout permit.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand};
use crate::opt::pass::MachinePass;

pub struct TailCallOptimizationPass;

impl Default for TailCallOptimizationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl TailCallOptimizationPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for TailCallOptimizationPass {
    fn name(&self) -> &'static str {
        "TailCallOptimization"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;
        let entry_label = match func.blocks.first() {
            Some(b) => b.label.clone(),
            None => return Ok(false),
        };

        for block in &mut func.blocks {
            let len = block.instructions.len();
            if len >= 2
                && let (
                    MachineInstruction::Call {
                        target: MachineOperand::Symbol(sym),
                        num_args,
                    },
                    MachineInstruction::Return,
                ) = (&block.instructions[len - 2], &block.instructions[len - 1])
            {
                // Self-tail-call or direct local tail-call with register arguments (<= 4 on Win64, <= 6 on SysV)
                if *num_args <= 4 && sym == &func.name {
                    block.instructions[len - 2] = MachineInstruction::Branch {
                        target: entry_label.clone(),
                    };
                    block.instructions.pop();
                    changed = true;
                }
            }
        }

        if changed {
            func.rebuild_cfg();
        }

        Ok(changed)
    }
}
