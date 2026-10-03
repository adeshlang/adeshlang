//! Local Common Subexpression Elimination (CSE) & Value Numbering.
//!
//! Eliminates redundant evaluations of identical expressions within basic blocks.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExpressionKey {
    BinOp {
        op: &'static str,
        lhs: String,
        rhs: String,
    },
    UnOp {
        op: &'static str,
        src: String,
    },
}

pub struct LocalCSEPass;

impl Default for LocalCSEPass {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalCSEPass {
    pub fn new() -> Self {
        Self
    }

    fn op_key(op: &MachineOperand) -> String {
        match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => format!("v{}", v.0),
            MachineOperand::Register(MachineRegister::Physical(p)) => format!("p{}", p.0),
            MachineOperand::Immediate(n) => format!("i{}", n),
            MachineOperand::FloatImmediate(f) => format!("f{}", f.to_bits()),
            MachineOperand::StackSlot(s) => format!("s{}", s),
            MachineOperand::Symbol(sym) => format!("sym:{}", sym),
            _ => "unknown".to_string(),
        }
    }
}

impl MachinePass for LocalCSEPass {
    fn name(&self) -> &'static str {
        "LocalCSE"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut available: HashMap<ExpressionKey, VirtualRegister> = HashMap::new();
            let mut rewritten = Vec::with_capacity(block.instructions.len());

            for inst in block.instructions.drain(..) {
                let mut cse_hit = None;

                match &inst {
                    MachineInstruction::Add {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src,
                    } => {
                        let key = ExpressionKey::BinOp {
                            op: "add",
                            lhs: Self::op_key(&MachineOperand::Register(MachineRegister::Virtual(
                                *dst_v,
                            ))),
                            rhs: Self::op_key(src),
                        };
                        if let Some(&prev_v) = available.get(&key) {
                            cse_hit = Some((*dst_v, prev_v));
                        }
                    }
                    MachineInstruction::Sub {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src,
                    } => {
                        let key = ExpressionKey::BinOp {
                            op: "sub",
                            lhs: Self::op_key(&MachineOperand::Register(MachineRegister::Virtual(
                                *dst_v,
                            ))),
                            rhs: Self::op_key(src),
                        };
                        if let Some(&prev_v) = available.get(&key) {
                            cse_hit = Some((*dst_v, prev_v));
                        }
                    }
                    MachineInstruction::Mul {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src,
                    } => {
                        let key = ExpressionKey::BinOp {
                            op: "mul",
                            lhs: Self::op_key(&MachineOperand::Register(MachineRegister::Virtual(
                                *dst_v,
                            ))),
                            rhs: Self::op_key(src),
                        };
                        if let Some(&prev_v) = available.get(&key) {
                            cse_hit = Some((*dst_v, prev_v));
                        }
                    }
                    _ => {}
                }

                if let Some((dst_v, prev_v)) = cse_hit {
                    changed = true;
                    rewritten.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src: MachineOperand::Register(MachineRegister::Virtual(prev_v)),
                    });
                } else {
                    // Record pure expressions
                    match &inst {
                        MachineInstruction::Add {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src,
                        } => {
                            let key = ExpressionKey::BinOp {
                                op: "add",
                                lhs: Self::op_key(&MachineOperand::Register(
                                    MachineRegister::Virtual(*dst_v),
                                )),
                                rhs: Self::op_key(src),
                            };
                            available.insert(key, *dst_v);
                        }
                        MachineInstruction::Sub {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src,
                        } => {
                            let key = ExpressionKey::BinOp {
                                op: "sub",
                                lhs: Self::op_key(&MachineOperand::Register(
                                    MachineRegister::Virtual(*dst_v),
                                )),
                                rhs: Self::op_key(src),
                            };
                            available.insert(key, *dst_v);
                        }
                        MachineInstruction::Mul {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src,
                        } => {
                            let key = ExpressionKey::BinOp {
                                op: "mul",
                                lhs: Self::op_key(&MachineOperand::Register(
                                    MachineRegister::Virtual(*dst_v),
                                )),
                                rhs: Self::op_key(src),
                            };
                            available.insert(key, *dst_v);
                        }
                        _ => {}
                    }

                    // Invalidate clobbered keys
                    for d in inst.defs() {
                        if let MachineRegister::Virtual(v) = d {
                            let v_str = format!("v{}", v.0);
                            available.retain(|k, &mut res_v| {
                                res_v != v
                                    && match k {
                                        ExpressionKey::BinOp { lhs, rhs, .. } => {
                                            lhs != &v_str && rhs != &v_str
                                        }
                                        ExpressionKey::UnOp { src, .. } => src != &v_str,
                                    }
                            });
                        }
                    }

                    rewritten.push(inst);
                }
            }

            block.instructions = rewritten;
        }

        Ok(changed)
    }
}
