//! Peephole Optimizer for Machine IR.
//!
//! Performs local instruction rewrites, algebraic simplifications, strength reductions,
//! redundant move/store elimination, and branch folding to produce compact, high-speed machine code.

use crate::machine_ir::{
    MachineBlock, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    RegisterClass, VirtualRegister,
};
use std::collections::HashMap;

pub struct PeepholeOptimizer {
    pub level: u8,
}

impl PeepholeOptimizer {
    pub fn new(level: u8) -> Self {
        Self { level }
    }

    pub fn optimize_function(&self, func: &mut MachineFunction) -> usize {
        let mut total_changes = 0;
        let classes = func.vreg_classes.clone();
        for block in &mut func.blocks {
            total_changes += self.optimize_block_inner(block, Some(&classes));
        }
        total_changes
    }

    pub fn optimize_block(&self, block: &mut MachineBlock) -> usize {
        // No virtual-register class information is available at this level, so
        // class-sensitive rewrites are skipped.
        self.optimize_block_inner(block, None)
    }

    /// True when `dst` is provably an integer GPR (safe for the `xor r, r`
    /// zero idiom). XMM/float destinations and memory operands must be left
    /// alone: `xor [mem], [mem]` has no encoding and XOR-ing an XMM register
    /// requires XORPS/XORPD.
    fn is_integer_gpr(
        dst: &MachineOperand,
        classes: Option<&HashMap<VirtualRegister, RegisterClass>>,
    ) -> bool {
        match dst {
            MachineOperand::Register(MachineRegister::Physical(p)) => p.0 < 16,
            MachineOperand::Register(MachineRegister::Virtual(v)) => match classes {
                Some(map) => {
                    map.get(v).copied().unwrap_or(RegisterClass::Gpr) == RegisterClass::Gpr
                }
                None => false,
            },
            _ => false,
        }
    }

    fn optimize_block_inner(
        &self,
        block: &mut MachineBlock,
        classes: Option<&HashMap<VirtualRegister, RegisterClass>>,
    ) -> usize {
        if self.level == 0 {
            return 0;
        }

        let mut changes = 0;
        let mut optimized = Vec::with_capacity(block.instructions.len());

        let mut i = 0;
        while i < block.instructions.len() {
            let inst = &block.instructions[i];

            match inst {
                // 1. Redundant Move Elimination: mov rA, rA -> removed
                MachineInstruction::Move { dst, src } if dst == src => {
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 2. Algebraic simplification for Add: add r, 0 -> removed
                MachineInstruction::Add {
                    dst: _,
                    src: MachineOperand::Immediate(0),
                } => {
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 3. Algebraic simplification for Sub: sub r, 0 -> removed
                MachineInstruction::Sub {
                    dst: _,
                    src: MachineOperand::Immediate(0),
                } => {
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 4. Multiplication by 1: mul r, 1 -> removed
                MachineInstruction::Mul {
                    dst: _,
                    src: MachineOperand::Immediate(1),
                } => {
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 5. Multiplication by 0: mul r, 0 -> mov r, 0
                MachineInstruction::Mul {
                    dst,
                    src: MachineOperand::Immediate(0),
                } => {
                    optimized.push(MachineInstruction::Move {
                        dst: dst.clone(),
                        src: MachineOperand::Immediate(0),
                    });
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 6. Strength reduction: mul r, 2^k -> shl r, k
                MachineInstruction::Mul {
                    dst,
                    src: MachineOperand::Immediate(val),
                } if *val > 0 && (*val as u64).is_power_of_two() => {
                    let shift = (*val as u64).trailing_zeros() as i64;
                    optimized.push(MachineInstruction::Shl {
                        dst: dst.clone(),
                        src: MachineOperand::Immediate(shift),
                    });
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 7. Identity shifts: shl/shr/sar r, 0 -> removed
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
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 8. Bitwise identity: and r, -1 -> removed; or r, 0 -> removed; xor r, 0 -> removed
                MachineInstruction::And {
                    dst: _,
                    src: MachineOperand::Immediate(-1),
                }
                | MachineInstruction::Or {
                    dst: _,
                    src: MachineOperand::Immediate(0),
                }
                | MachineInstruction::Xor {
                    dst: _,
                    src: MachineOperand::Immediate(0),
                } => {
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 9. Canonical zeroing: mov r, 0 -> xor r, r
                // Only for provably integer GPR destinations.
                MachineInstruction::Move {
                    dst,
                    src: MachineOperand::Immediate(0),
                } if Self::is_integer_gpr(dst, classes) => {
                    optimized.push(MachineInstruction::Xor {
                        dst: dst.clone(),
                        src: dst.clone(),
                    });
                    changes += 1;
                    i += 1;
                    continue;
                }

                // 10. Unconditional jump after return / branch elimination
                MachineInstruction::Return => {
                    optimized.push(MachineInstruction::Return);
                    // Drop unreachable instructions after return in this block
                    if i + 1 < block.instructions.len() {
                        changes += block.instructions.len() - (i + 1);
                    }
                    break;
                }

                // Default: preserve instruction
                _ => {
                    optimized.push(inst.clone());
                    i += 1;
                }
            }
        }

        block.instructions = optimized;
        changes
    }
}
