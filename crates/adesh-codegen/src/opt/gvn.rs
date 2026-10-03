//! Global Value Numbering (GVN) and Global CSE.
//!
//! Extends basic-block CSE across basic blocks by taking advantage of dominator relationships
//! and side-effect guarantees.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use crate::opt::dominance::DominatorTree;
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlobalExprKey {
    Add(u32, u32),
    Sub(u32, u32),
    Mul(u32, u32),
    Shl(u32, i64),
    Shr(u32, i64),
}

pub struct GVNPass;

impl Default for GVNPass {
    fn default() -> Self {
        Self::new()
    }
}

impl GVNPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for GVNPass {
    fn name(&self) -> &'static str {
        "GlobalValueNumbering"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        if func.blocks.len() < 2 {
            return Ok(false);
        }

        let dom = DominatorTree::compute(func);
        let mut changed = false;

        // Map computed expression -> (defining_block_id, virtual_register)
        let mut expr_defs: HashMap<GlobalExprKey, (u32, VirtualRegister)> = HashMap::new();

        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                match inst {
                    MachineInstruction::Add {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                    } => {
                        let key = if dst_v.0 <= src_v.0 {
                            GlobalExprKey::Add(dst_v.0, src_v.0)
                        } else {
                            GlobalExprKey::Add(src_v.0, dst_v.0)
                        };

                        if let Some(&(def_block, existing_vreg)) = expr_defs.get(&key) {
                            if dom.dominates(def_block, block.id) && existing_vreg != *dst_v {
                                *inst = MachineInstruction::Move {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(*dst_v)),
                                    src: MachineOperand::Register(MachineRegister::Virtual(
                                        existing_vreg,
                                    )),
                                };
                                changed = true;
                            }
                        } else {
                            expr_defs.insert(key, (block.id, *dst_v));
                        }
                    }

                    MachineInstruction::Mul {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                    } => {
                        let key = if dst_v.0 <= src_v.0 {
                            GlobalExprKey::Mul(dst_v.0, src_v.0)
                        } else {
                            GlobalExprKey::Mul(src_v.0, dst_v.0)
                        };

                        if let Some(&(def_block, existing_vreg)) = expr_defs.get(&key) {
                            if dom.dominates(def_block, block.id) && existing_vreg != *dst_v {
                                *inst = MachineInstruction::Move {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(*dst_v)),
                                    src: MachineOperand::Register(MachineRegister::Virtual(
                                        existing_vreg,
                                    )),
                                };
                                changed = true;
                            }
                        } else {
                            expr_defs.insert(key, (block.id, *dst_v));
                        }
                    }

                    _ => {}
                }
            }
        }

        Ok(changed)
    }
}
