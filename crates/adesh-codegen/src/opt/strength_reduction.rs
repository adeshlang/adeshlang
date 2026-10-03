//! Strength Reduction & Addressing-Mode Synthesis Pass.
//!
//! Replaces expensive operations (such as multiplications by powers of two)
//! with cheap shifts, and synthesizes x86-64 LEA addressing mode calculations.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand};
use crate::opt::pass::MachinePass;

pub struct StrengthReductionPass;

impl Default for StrengthReductionPass {
    fn default() -> Self {
        Self::new()
    }
}

impl StrengthReductionPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for StrengthReductionPass {
    fn name(&self) -> &'static str {
        "StrengthReduction"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut rewritten = Vec::with_capacity(block.instructions.len());

            for inst in block.instructions.drain(..) {
                match inst {
                    // mul r, 2^k -> shl r, k
                    MachineInstruction::Mul {
                        dst,
                        src: MachineOperand::Immediate(val),
                    } if val > 0 && (val as u64).is_power_of_two() => {
                        let shift = (val as u64).trailing_zeros() as i64;
                        rewritten.push(MachineInstruction::Shl {
                            dst,
                            src: MachineOperand::Immediate(shift),
                        });
                        changed = true;
                    }
                    // add r, 0 -> eliminate
                    MachineInstruction::Add {
                        dst: _,
                        src: MachineOperand::Immediate(0),
                    } => {
                        changed = true;
                    }
                    // sub r, 0 -> eliminate
                    MachineInstruction::Sub {
                        dst: _,
                        src: MachineOperand::Immediate(0),
                    } => {
                        changed = true;
                    }
                    // shl/shr/sar r, 0 -> eliminate
                    MachineInstruction::Shl {
                        dst: _,
                        src: MachineOperand::Immediate(0),
                    }
                    | MachineInstruction::Shr {
                        dst: _,
                        src: MachineOperand::Immediate(0),
                    }
                    | MachineInstruction::Sar {
                        dst: _,
                        src: MachineOperand::Immediate(0),
                    } => {
                        changed = true;
                    }
                    // and r, -1 -> eliminate
                    MachineInstruction::And {
                        dst: _,
                        src: MachineOperand::Immediate(-1),
                    } => {
                        changed = true;
                    }
                    // or r, 0 | xor r, 0 -> eliminate
                    MachineInstruction::Or {
                        dst: _,
                        src: MachineOperand::Immediate(0),
                    }
                    | MachineInstruction::Xor {
                        dst: _,
                        src: MachineOperand::Immediate(0),
                    } => {
                        changed = true;
                    }
                    other => rewritten.push(other),
                }
            }

            block.instructions = rewritten;
        }

        Ok(changed)
    }
}
