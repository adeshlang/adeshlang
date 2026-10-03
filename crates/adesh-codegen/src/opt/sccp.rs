//! Sparse Conditional Constant Propagation (SCCP).
//!
//! Simultaneously tracks constant values across basic blocks and evaluates branch conditions,
//! resolving branches with known conditions and eliminating dead/unreachable blocks.

use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    VirtualRegister,
};
use crate::opt::pass::MachinePass;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatticeValue {
    Top, // Uninitialized / unknown
    Constant(i64),
    Bottom, // Variable / multiple values
}

pub struct SCCPPass;

impl Default for SCCPPass {
    fn default() -> Self {
        Self::new()
    }
}

impl SCCPPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for SCCPPass {
    fn name(&self) -> &'static str {
        "SCCP"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        if func.blocks.is_empty() {
            return Ok(false);
        }

        let mut changed = false;
        let mut lat_values: HashMap<VirtualRegister, LatticeValue> = HashMap::new();
        let mut executable_blocks: HashSet<u32> = HashSet::new();

        executable_blocks.insert(func.blocks[0].id);

        // Track constants and fold branches
        for block in &mut func.blocks {
            if !executable_blocks.contains(&block.id) {
                continue;
            }

            for inst in &mut block.instructions {
                match inst {
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src: MachineOperand::Immediate(val),
                    } => {
                        lat_values.insert(*dst_v, LatticeValue::Constant(*val));
                    }

                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                    } => {
                        if let Some(&lat) = lat_values.get(src_v) {
                            lat_values.insert(*dst_v, lat);
                        }
                    }

                    _ => {}
                }
            }

            // If block ends with BranchCc where flags/condition are statically known:
            let len = block.instructions.len();
            if len >= 2
                && let (
                    MachineInstruction::Compare {
                        lhs: MachineOperand::Register(MachineRegister::Virtual(lv)),
                        rhs: MachineOperand::Immediate(rv),
                    },
                    MachineInstruction::BranchCc { cc, target },
                ) = (&block.instructions[len - 2], &block.instructions[len - 1])
                && let Some(LatticeValue::Constant(left_const)) = lat_values.get(lv)
            {
                let is_taken = match cc {
                    ConditionCode::Equal => *left_const == *rv,
                    ConditionCode::NotEqual => *left_const != *rv,
                    ConditionCode::LessThan => *left_const < *rv,
                    ConditionCode::LessOrEqual => *left_const <= *rv,
                    ConditionCode::GreaterThan => *left_const > *rv,
                    ConditionCode::GreaterOrEqual => *left_const >= *rv,
                    _ => false,
                };

                let branch_target = target.clone();
                if is_taken {
                    // Convert conditional branch to unconditional branch
                    block.instructions[len - 1] = MachineInstruction::Branch {
                        target: branch_target,
                    };
                    changed = true;
                } else {
                    // Eliminate non-taken conditional branch
                    block.instructions.remove(len - 1);
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
