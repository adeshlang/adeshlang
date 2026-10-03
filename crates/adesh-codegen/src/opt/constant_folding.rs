//! Constant Folding & Constant Propagation Pass for Machine IR.
//!
//! Tracks known constants across basic blocks and evaluates arithmetic, logical,
//! shift, and comparison operations at compile time, eliminating redundant runtime computations.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnownValue {
    Int(i64),
    Float(f64),
}

pub struct ConstantFoldingPass;

impl Default for ConstantFoldingPass {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstantFoldingPass {
    pub fn new() -> Self {
        Self
    }

    fn fold_int_binop(op: &str, lhs: i64, rhs: i64) -> Option<i64> {
        match op {
            "add" => Some(lhs.wrapping_add(rhs)),
            "sub" => Some(lhs.wrapping_sub(rhs)),
            "mul" => Some(lhs.wrapping_mul(rhs)),
            "div" => {
                if rhs != 0 && !(lhs == i64::MIN && rhs == -1) {
                    Some(lhs.wrapping_div(rhs))
                } else {
                    None
                }
            }
            "mod" => {
                if rhs != 0 && !(lhs == i64::MIN && rhs == -1) {
                    Some(lhs.wrapping_rem(rhs))
                } else {
                    None
                }
            }
            "and" => Some(lhs & rhs),
            "or" => Some(lhs | rhs),
            "xor" => Some(lhs ^ rhs),
            "shl" => {
                if (0..64).contains(&rhs) {
                    Some(lhs.wrapping_shl(rhs as u32))
                } else {
                    None
                }
            }
            "shr" => {
                if (0..64).contains(&rhs) {
                    Some((lhs as u64).wrapping_shr(rhs as u32) as i64)
                } else {
                    None
                }
            }
            "sar" => {
                if (0..64).contains(&rhs) {
                    Some(lhs.wrapping_shr(rhs as u32))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn fold_fp_binop(op: &str, lhs: f64, rhs: f64) -> Option<f64> {
        match op {
            "fadd" => Some(lhs + rhs),
            "fsub" => Some(lhs - rhs),
            "fmul" => Some(lhs * rhs),
            "fdiv" => {
                if rhs != 0.0 {
                    Some(lhs / rhs)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl MachinePass for ConstantFoldingPass {
    fn name(&self) -> &'static str {
        "ConstantFolding"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut const_map: HashMap<VirtualRegister, KnownValue> = HashMap::new();
            let mut rewritten = Vec::with_capacity(block.instructions.len());

            for inst in block.instructions.drain(..) {
                match inst {
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                        src: MachineOperand::Immediate(val),
                    } => {
                        const_map.insert(v, KnownValue::Int(val));
                        rewritten.push(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                            src: MachineOperand::Immediate(val),
                        });
                    }
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                        src: MachineOperand::FloatImmediate(val),
                    } => {
                        const_map.insert(v, KnownValue::Float(val));
                        rewritten.push(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                            src: MachineOperand::FloatImmediate(val),
                        });
                    }
                    MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                    } => {
                        if let Some(val) = const_map.get(&src_v).cloned() {
                            const_map.insert(dst_v, val);
                            match val {
                                KnownValue::Int(n) => {
                                    changed = true;
                                    rewritten.push(MachineInstruction::Move {
                                        dst: MachineOperand::Register(MachineRegister::Virtual(
                                            dst_v,
                                        )),
                                        src: MachineOperand::Immediate(n),
                                    });
                                }
                                KnownValue::Float(f) => {
                                    changed = true;
                                    rewritten.push(MachineInstruction::Move {
                                        dst: MachineOperand::Register(MachineRegister::Virtual(
                                            dst_v,
                                        )),
                                        src: MachineOperand::FloatImmediate(f),
                                    });
                                }
                            }
                        } else {
                            const_map.remove(&dst_v);
                            rewritten.push(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                                src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                            });
                        }
                    }
                    MachineInstruction::Add {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src,
                    } => {
                        let rhs_val = match &src {
                            MachineOperand::Immediate(n) => Some(*n),
                            MachineOperand::Register(MachineRegister::Virtual(src_v)) => {
                                match const_map.get(src_v) {
                                    Some(KnownValue::Int(n)) => Some(*n),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };

                        if let (Some(KnownValue::Int(lhs_n)), Some(rhs_n)) =
                            (const_map.get(&dst_v).copied(), rhs_val)
                            && let Some(folded) = Self::fold_int_binop("add", lhs_n, rhs_n)
                        {
                            const_map.insert(dst_v, KnownValue::Int(folded));
                            rewritten.push(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                                src: MachineOperand::Immediate(folded),
                            });
                            changed = true;
                            continue;
                        }
                        const_map.remove(&dst_v);
                        rewritten.push(MachineInstruction::Add {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src,
                        });
                    }
                    MachineInstruction::Sub {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src,
                    } => {
                        let rhs_val = match &src {
                            MachineOperand::Immediate(n) => Some(*n),
                            MachineOperand::Register(MachineRegister::Virtual(src_v)) => {
                                match const_map.get(src_v) {
                                    Some(KnownValue::Int(n)) => Some(*n),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };

                        if let (Some(KnownValue::Int(lhs_n)), Some(rhs_n)) =
                            (const_map.get(&dst_v).copied(), rhs_val)
                            && let Some(folded) = Self::fold_int_binop("sub", lhs_n, rhs_n)
                        {
                            const_map.insert(dst_v, KnownValue::Int(folded));
                            rewritten.push(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                                src: MachineOperand::Immediate(folded),
                            });
                            changed = true;
                            continue;
                        }
                        const_map.remove(&dst_v);
                        rewritten.push(MachineInstruction::Sub {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src,
                        });
                    }
                    MachineInstruction::Mul {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src,
                    } => {
                        let rhs_val = match &src {
                            MachineOperand::Immediate(n) => Some(*n),
                            MachineOperand::Register(MachineRegister::Virtual(src_v)) => {
                                match const_map.get(src_v) {
                                    Some(KnownValue::Int(n)) => Some(*n),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };

                        if let (Some(KnownValue::Int(lhs_n)), Some(rhs_n)) =
                            (const_map.get(&dst_v).copied(), rhs_val)
                            && let Some(folded) = Self::fold_int_binop("mul", lhs_n, rhs_n)
                        {
                            const_map.insert(dst_v, KnownValue::Int(folded));
                            rewritten.push(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                                src: MachineOperand::Immediate(folded),
                            });
                            changed = true;
                            continue;
                        }
                        const_map.remove(&dst_v);
                        rewritten.push(MachineInstruction::Mul {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src,
                        });
                    }
                    MachineInstruction::FAdd {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                        src,
                        size,
                    } => {
                        let rhs_val = match &src {
                            MachineOperand::FloatImmediate(f) => Some(*f),
                            MachineOperand::Register(MachineRegister::Virtual(src_v)) => {
                                match const_map.get(src_v) {
                                    Some(KnownValue::Float(f)) => Some(*f),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };

                        if let (Some(KnownValue::Float(lhs_f)), Some(rhs_f)) =
                            (const_map.get(&dst_v).copied(), rhs_val)
                            && let Some(folded) = Self::fold_fp_binop("fadd", lhs_f, rhs_f)
                        {
                            const_map.insert(dst_v, KnownValue::Float(folded));
                            rewritten.push(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                                src: MachineOperand::FloatImmediate(folded),
                            });
                            changed = true;
                            continue;
                        }
                        const_map.remove(&dst_v);
                        rewritten.push(MachineInstruction::FAdd {
                            dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                            src,
                            size,
                        });
                    }
                    other => {
                        // Invalidate modified registers
                        for d in other.defs() {
                            if let MachineRegister::Virtual(v) = d {
                                const_map.remove(&v);
                            }
                        }
                        rewritten.push(other);
                    }
                }
            }

            block.instructions = rewritten;
        }

        Ok(changed)
    }
}
