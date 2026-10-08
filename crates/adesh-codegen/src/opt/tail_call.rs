//! Tail-call optimization for signature-identical self recursion.
//!
//! Explicitly typed local tail calls are lowered by native HIR lowering,
//! which has the signature and calling-convention information this pass lacks.

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
            Some(block) => block.label.clone(),
            None => return Ok(false),
        };
        for block in &mut func.blocks {
            // A call may have its result copied through RAX/XMM0 before the
            // return. Recognize only that known suffix, with no side effects.
            let len = block.instructions.len();
            if len < 2 || !matches!(block.instructions[len - 1], MachineInstruction::Return) {
                continue;
            }
            let return_start = if len >= 4
                && is_return_register_copy(&block.instructions[len - 2], 0)
                && is_return_value_capture(
                    &block.instructions[len - 3],
                    &block.instructions[len - 2],
                    0,
                ) {
                len - 3
            } else if len >= 4
                && is_return_register_copy(&block.instructions[len - 2], 16)
                && is_return_value_capture(
                    &block.instructions[len - 3],
                    &block.instructions[len - 2],
                    16,
                )
            {
                len - 3
            } else {
                len - 1
            };
            let call_index = if return_start >= 2
                && matches!(
                    block.instructions[return_start - 1],
                    MachineInstruction::Add {
                        dst: MachineOperand::Register(
                            crate::machine_ir::MachineRegister::Physical(
                                crate::machine_ir::PhysicalRegister(4)
                            )
                        ),
                        src: MachineOperand::Immediate(_),
                    }
                ) {
                return_start - 2
            } else {
                return_start - 1
            };
            let MachineInstruction::Call {
                target: MachineOperand::Symbol(symbol),
                num_args,
            } = &block.instructions[call_index]
            else {
                continue;
            };
            // Four integer/register arguments are the common safe subset of
            // both Win64 and SysV x86-64. The caller and callee use the same
            // ABI, so the existing caller shadow area remains valid on Win64.
            if *num_args > 4 {
                continue;
            }
            // The current Call IR carries argument count, but not the return
            // ABI class. A self-call is provably signature-compatible; an
            // arbitrary callee is not, so do not tail-jump across that ABI
            // boundary until Call carries explicit result metadata.
            if symbol != &func.name {
                continue;
            }
            // The argument shuffle has populated the ABI parameter registers;
            // the entry block re-copies those into the parameter slots.
            block.instructions[call_index] = MachineInstruction::Branch {
                target: entry_label.clone(),
            };
            block.instructions.truncate(call_index + 1);
            // On Win64 the call setup reserves 32-byte shadow space plus
            // alignment. Remove it for a loop backedge, otherwise each
            // iteration would leak stack space.
            let setup = (0..call_index).rev().find(|&i| {
                is_win64_shadow_reservation(&block.instructions[i])
                    && block.instructions[i + 1..call_index]
                        .iter()
                        .all(is_argument_shuffle)
            });
            if let Some(setup_index) = setup {
                block.instructions.remove(setup_index);
            }
            changed = true;
        }

        if changed {
            func.rebuild_cfg();
        }

        Ok(changed)
    }
}

fn is_win64_shadow_reservation(inst: &MachineInstruction) -> bool {
    matches!(
        inst,
        MachineInstruction::Sub {
            dst: MachineOperand::Register(crate::machine_ir::MachineRegister::Physical(
                crate::machine_ir::PhysicalRegister(4)
            )),
            src: MachineOperand::Immediate(40),
        }
    )
}

fn is_argument_shuffle(inst: &MachineInstruction) -> bool {
    matches!(
        inst,
        MachineInstruction::Move { .. } | MachineInstruction::ParallelMove { .. }
    )
}

/// `return foo()` currently lowers through RAX/XMM0 -> virtual -> return
/// register copies. Recognize that exact, side-effect-free suffix so the
/// call can be turned into a tail transfer without losing its result.
fn is_return_register_copy(inst: &MachineInstruction, reg: u8) -> bool {
    matches!(
        inst,
        MachineInstruction::Move {
            dst: MachineOperand::Register(crate::machine_ir::MachineRegister::Physical(
                crate::machine_ir::PhysicalRegister(dst)
            )),
            src: MachineOperand::Register(crate::machine_ir::MachineRegister::Virtual(_)),
        } if *dst == reg
    )
}

fn is_return_value_capture(
    capture: &MachineInstruction,
    returned: &MachineInstruction,
    reg: u8,
) -> bool {
    let MachineInstruction::Move {
        dst:
            MachineOperand::Register(crate::machine_ir::MachineRegister::Physical(
                crate::machine_ir::PhysicalRegister(return_reg),
            )),
        src: MachineOperand::Register(crate::machine_ir::MachineRegister::Virtual(return_vreg)),
    } = returned
    else {
        return false;
    };
    if *return_reg != reg {
        return false;
    }
    matches!(
        capture,
        MachineInstruction::Move {
            dst: MachineOperand::Register(crate::machine_ir::MachineRegister::Virtual(capture_vreg)),
            src: MachineOperand::Register(crate::machine_ir::MachineRegister::Physical(
                crate::machine_ir::PhysicalRegister(source_reg)
            )),
        } if capture_vreg == return_vreg && *source_reg == reg
    )
}
