//! Variadic calling convention and argument retrieval infrastructure.
//!
//! Provides:
//! 1. Windows x64 homing space (shadow store) spill sequence:
//!    spilling RCX, RDX, R8, R9 to [rbp + 16..40] for contiguous stack argument access.
//! 2. System V AMD64 Register Save Area (176 bytes for GPR + SSE) and va_list initialization:
//!    gp_offset, fp_offset, overflow_arg_area, reg_save_area.
//! 3. Caller-side SysV `%al` vector count and Win64 variadic float/GPR slot handling.

use crate::abi::sysv64::{SYSV_FPR_ARGS, SYSV_GPR_ARGS};
use crate::abi::win64::WIN64_GPR_ARGS;
use crate::machine_ir::{
    ConditionCode, MachineInstruction, MachineOperand, MachineRegister, PhysicalRegister,
    RegisterClass, VirtualRegister,
};

/// Win64 variadic helper functions.
pub struct Win64Variadics;

impl Win64Variadics {
    /// Emit the Win64 callee spill sequence into the caller's shadow space.
    ///
    /// On Windows x64, the caller allocates 32 bytes of shadow space immediately
    /// above the return address ([rbp + 16]). A variadic callee stores the four
    /// register parameters (RCX, RDX, R8, R9) into this space so that all arguments
    /// form a single, contiguous array of 8-byte slots on the stack.
    pub fn emit_callee_shadow_spill(
        instructions: &mut Vec<MachineInstruction>,
        frame_pointer: PhysicalRegister,
    ) {
        for (i, &reg) in WIN64_GPR_ARGS.iter().enumerate() {
            let offset = 16 + (i as i32 * 8);
            instructions.push(MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Physical(frame_pointer),
                    offset,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Physical(reg)),
                size: 8,
            });
        }
    }

    /// Computes the address of the first variadic argument on Windows x64.
    ///
    /// `named_args_count` is the number of declared fixed parameters.
    /// Returns the stack offset relative to the frame pointer where variadics begin.
    pub fn first_variadic_offset(named_args_count: usize) -> i32 {
        16 + (named_args_count as i32 * 8)
    }
}

/// System V AMD64 variadic helper functions.
pub struct SysVVariadics;

impl SysVVariadics {
    /// Total size of the System V Register Save Area:
    /// 48 bytes (6 GPRs * 8) + 128 bytes (8 XMMs * 16) = 176 bytes.
    pub const REG_SAVE_AREA_SIZE: usize = 176;

    /// Size of the System V `va_list` structure:
    /// - gp_offset: 4 bytes (offset in reg_save_area)
    /// - fp_offset: 4 bytes (offset in reg_save_area)
    /// - overflow_arg_area: 8 bytes (pointer to next stack argument)
    /// - reg_save_area: 8 bytes (pointer to base of reg_save_area)
    ///
    /// Total: 24 bytes.
    pub const VA_LIST_SIZE: usize = 24;

    /// Emits the System V callee register save area spill sequence.
    ///
    /// Spills the 6 argument GPRs to [rbp + rsa_offset + 0..40]
    /// and the 8 argument FPRs (XMM0..XMM7) to [rbp + rsa_offset + 48..160].
    pub fn emit_callee_rsa_spill(
        instructions: &mut Vec<MachineInstruction>,
        frame_pointer: PhysicalRegister,
        rsa_stack_offset: i32,
    ) {
        // 1. Spill general-purpose registers: RDI, RSI, RDX, RCX, R8, R9
        for (i, &reg) in SYSV_GPR_ARGS.iter().enumerate() {
            let offset = rsa_stack_offset + (i as i32 * 8);
            instructions.push(MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Physical(frame_pointer),
                    offset,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Physical(reg)),
                size: 8,
            });
        }

        // 2. Spill floating-point / SSE registers: XMM0..XMM7
        for (i, &reg) in SYSV_FPR_ARGS.iter().enumerate() {
            let offset = rsa_stack_offset + 48 + (i as i32 * 16);
            instructions.push(MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Physical(frame_pointer),
                    offset,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Physical(reg)),
                size: 8, // 64-bit scalar or lower half of 128-bit
            });
        }
    }

    /// Emits initialization of the System V `va_list` structure on the stack.
    ///
    /// - `va_list_offset`: stack offset of the 24-byte `va_list` structure.
    /// - `rsa_offset`: stack offset of the 176-byte Register Save Area.
    /// - `fixed_gpr_count`: number of fixed parameters passed in GPRs.
    /// - `fixed_fpr_count`: number of fixed parameters passed in FPRs.
    pub fn emit_va_start(
        instructions: &mut Vec<MachineInstruction>,
        frame_pointer: PhysicalRegister,
        va_list_offset: i32,
        rsa_offset: i32,
        fixed_gpr_count: usize,
        fixed_fpr_count: usize,
        temp_vreg: VirtualRegister,
    ) {
        let gp_offset = (fixed_gpr_count.min(6) * 8) as i64;
        let fp_offset = (48 + fixed_fpr_count.min(8) * 16) as i64;

        // Store gp_offset (u32 at va_list + 0)
        instructions.push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(frame_pointer),
                offset: va_list_offset,
                index: None,
            },
            src: MachineOperand::Immediate(gp_offset),
            size: 4,
        });

        // Store fp_offset (u32 at va_list + 4)
        instructions.push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(frame_pointer),
                offset: va_list_offset + 4,
                index: None,
            },
            src: MachineOperand::Immediate(fp_offset),
            size: 4,
        });

        // overflow_arg_area = frame_pointer + 16 (first stack argument slot)
        instructions.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(temp_vreg)),
            src: MachineOperand::Register(MachineRegister::Physical(frame_pointer)),
        });
        instructions.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(temp_vreg)),
            src: MachineOperand::Immediate(16),
        });
        instructions.push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(frame_pointer),
                offset: va_list_offset + 8,
                index: None,
            },
            src: MachineOperand::Register(MachineRegister::Virtual(temp_vreg)),
            size: 8,
        });

        // reg_save_area = frame_pointer + rsa_offset
        instructions.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(temp_vreg)),
            src: MachineOperand::Register(MachineRegister::Physical(frame_pointer)),
        });
        if rsa_offset < 0 {
            instructions.push(MachineInstruction::Sub {
                dst: MachineOperand::Register(MachineRegister::Virtual(temp_vreg)),
                src: MachineOperand::Immediate((-rsa_offset) as i64),
            });
        } else {
            instructions.push(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Virtual(temp_vreg)),
                src: MachineOperand::Immediate(rsa_offset as i64),
            });
        }
        instructions.push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(frame_pointer),
                offset: va_list_offset + 16,
                index: None,
            },
            src: MachineOperand::Register(MachineRegister::Virtual(temp_vreg)),
            size: 8,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_win64_shadow_spill_emits_four_stores() {
        let mut insts = Vec::new();
        Win64Variadics::emit_callee_shadow_spill(&mut insts, PhysicalRegister(5)); // RBP
        assert_eq!(insts.len(), 4);
        assert_eq!(
            insts[0],
            MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Physical(PhysicalRegister(5)),
                    offset: 16,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))), // RCX
                size: 8,
            }
        );
        assert_eq!(
            insts[3],
            MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Physical(PhysicalRegister(5)),
                    offset: 40,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(9))), // R9
                size: 8,
            }
        );
    }

    #[test]
    fn test_sysv_rsa_spill_emits_14_stores() {
        let mut insts = Vec::new();
        SysVVariadics::emit_callee_rsa_spill(&mut insts, PhysicalRegister(5), -176);
        // 6 GPR stores + 8 FPR stores = 14 stores
        assert_eq!(insts.len(), 14);
    }

    #[test]
    fn test_sysv_va_start_initialization() {
        let mut insts = Vec::new();
        SysVVariadics::emit_va_start(
            &mut insts,
            PhysicalRegister(5),
            -24,
            -200,
            2, // 2 fixed GPRs
            1, // 1 fixed FPR
            VirtualRegister(10),
        );
        // Stores for gp_offset, fp_offset, overflow_arg_area, reg_save_area
        assert!(insts.len() >= 6);
    }
}
