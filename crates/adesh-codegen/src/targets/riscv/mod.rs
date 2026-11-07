//! RISC-V (RV64GC LP64D) Native Codegen Backend.
//!
//! Provides production-grade RV64GC instruction selection, register allocation,
//! calling convention (LP64D ABI), 64-bit immediate synthesis, double/single-precision
//! floating point (RV64D/F), atomics (RV64A), compressed instruction alignment (RVC),
//! vector instructions (RVV 1.0), and ADOB/ELF object generation with full relocations.

use crate::backend::CodegenBackend;
use crate::calling_convention::ParallelMoveResolver;
use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass,
};
use crate::register_alloc::{LinearScanAllocator, RegisterFile};
use adesh_object::relocation::{AdobRelocation, RelocationKind};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility,
    TargetCapabilities, TargetDescriptor, section_flags,
};
use std::collections::HashMap;

/// RISC-V Register File (x0-x31, f0-f31).
pub struct RiscVRegisterFile;

/// Function machine code together with the relocations its instruction pairs
/// need: `RiscV_Call` on a call's `AUIPC`, and `RiscV_PcrelHi20`/`RiscV_PcrelLo12I`
/// on a symbol-address pair. Offsets are relative to the start of the function.
pub type FunctionCodeWithRelocs = (Vec<u8>, Vec<AdobRelocation>);

const RISCV_ALL_REGS: [PhysicalRegister; 64] = [
    // X0..X31 (GPR)
    PhysicalRegister(0),
    PhysicalRegister(1),
    PhysicalRegister(2),
    PhysicalRegister(3),
    PhysicalRegister(4),
    PhysicalRegister(5),
    PhysicalRegister(6),
    PhysicalRegister(7),
    PhysicalRegister(8),
    PhysicalRegister(9),
    PhysicalRegister(10),
    PhysicalRegister(11),
    PhysicalRegister(12),
    PhysicalRegister(13),
    PhysicalRegister(14),
    PhysicalRegister(15),
    PhysicalRegister(16),
    PhysicalRegister(17),
    PhysicalRegister(18),
    PhysicalRegister(19),
    PhysicalRegister(20),
    PhysicalRegister(21),
    PhysicalRegister(22),
    PhysicalRegister(23),
    PhysicalRegister(24),
    PhysicalRegister(25),
    PhysicalRegister(26),
    PhysicalRegister(27),
    PhysicalRegister(28),
    PhysicalRegister(29),
    PhysicalRegister(30),
    PhysicalRegister(31),
    // F0..F31 (Float)
    PhysicalRegister(32),
    PhysicalRegister(33),
    PhysicalRegister(34),
    PhysicalRegister(35),
    PhysicalRegister(36),
    PhysicalRegister(37),
    PhysicalRegister(38),
    PhysicalRegister(39),
    PhysicalRegister(40),
    PhysicalRegister(41),
    PhysicalRegister(42),
    PhysicalRegister(43),
    PhysicalRegister(44),
    PhysicalRegister(45),
    PhysicalRegister(46),
    PhysicalRegister(47),
    PhysicalRegister(48),
    PhysicalRegister(49),
    PhysicalRegister(50),
    PhysicalRegister(51),
    PhysicalRegister(52),
    PhysicalRegister(53),
    PhysicalRegister(54),
    PhysicalRegister(55),
    PhysicalRegister(56),
    PhysicalRegister(57),
    PhysicalRegister(58),
    PhysicalRegister(59),
    PhysicalRegister(60),
    PhysicalRegister(61),
    PhysicalRegister(62),
    PhysicalRegister(63),
];

/// Allocatable GPRs: t0 (5), a0..a7 (10..17), t3..t6 (28..31), s1..s11 (9, 18..27).
const RISCV_ALLOCATABLE_GPR: [PhysicalRegister; 24] = [
    PhysicalRegister(5),  // t0
    PhysicalRegister(10), // a0
    PhysicalRegister(11), // a1
    PhysicalRegister(12), // a2
    PhysicalRegister(13), // a3
    PhysicalRegister(14), // a4
    PhysicalRegister(15), // a5
    PhysicalRegister(16), // a6
    PhysicalRegister(17), // a7
    PhysicalRegister(28), // t3
    PhysicalRegister(29), // t4
    PhysicalRegister(30), // t5
    PhysicalRegister(31), // t6
    PhysicalRegister(9),  // s1
    PhysicalRegister(18), // s2
    PhysicalRegister(19), // s3
    PhysicalRegister(20), // s4
    PhysicalRegister(21), // s5
    PhysicalRegister(22), // s6
    PhysicalRegister(23), // s7
    PhysicalRegister(24), // s8
    PhysicalRegister(25), // s9
    PhysicalRegister(26), // s10
    PhysicalRegister(27), // s11
];

/// Caller-saved GPRs (temporaries & argument registers): t0 (5), a0..a7 (10..17), t3..t6 (28..31).
const RISCV_CALLER_SAVED_GPR: [PhysicalRegister; 13] = [
    PhysicalRegister(5),  // t0
    PhysicalRegister(10), // a0
    PhysicalRegister(11), // a1
    PhysicalRegister(12), // a2
    PhysicalRegister(13), // a3
    PhysicalRegister(14), // a4
    PhysicalRegister(15), // a5
    PhysicalRegister(16), // a6
    PhysicalRegister(17), // a7
    PhysicalRegister(28), // t3
    PhysicalRegister(29), // t4
    PhysicalRegister(30), // t5
    PhysicalRegister(31), // t6
];

/// Callee-saved GPRs: s0/fp (8), s1..s11 (9, 18..27).
const RISCV_CALLEE_SAVED_GPR: [PhysicalRegister; 12] = [
    PhysicalRegister(8),  // s0 / fp
    PhysicalRegister(9),  // s1
    PhysicalRegister(18), // s2
    PhysicalRegister(19), // s3
    PhysicalRegister(20), // s4
    PhysicalRegister(21), // s5
    PhysicalRegister(22), // s6
    PhysicalRegister(23), // s7
    PhysicalRegister(24), // s8
    PhysicalRegister(25), // s9
    PhysicalRegister(26), // s10
    PhysicalRegister(27), // s11
];

/// Allocatable FPRs: ft0..ft7 (32..39), fa0..fa7 (42..49), ft8..ft9 (60..61), fs0..fs11 (40..41, 50..59).
const RISCV_ALLOCATABLE_FP: [PhysicalRegister; 30] = [
    PhysicalRegister(32), // ft0
    PhysicalRegister(33), // ft1
    PhysicalRegister(34), // ft2
    PhysicalRegister(35), // ft3
    PhysicalRegister(36), // ft4
    PhysicalRegister(37), // ft5
    PhysicalRegister(38), // ft6
    PhysicalRegister(39), // ft7
    PhysicalRegister(42), // fa0
    PhysicalRegister(43), // fa1
    PhysicalRegister(44), // fa2
    PhysicalRegister(45), // fa3
    PhysicalRegister(46), // fa4
    PhysicalRegister(47), // fa5
    PhysicalRegister(48), // fa6
    PhysicalRegister(49), // fa7
    PhysicalRegister(60), // ft8
    PhysicalRegister(61), // ft9
    PhysicalRegister(40), // fs0
    PhysicalRegister(41), // fs1
    PhysicalRegister(50), // fs2
    PhysicalRegister(51), // fs3
    PhysicalRegister(52), // fs4
    PhysicalRegister(53), // fs5
    PhysicalRegister(54), // fs6
    PhysicalRegister(55), // fs7
    PhysicalRegister(56), // fs8
    PhysicalRegister(57), // fs9
    PhysicalRegister(58), // fs10
    PhysicalRegister(59), // fs11
];

/// Reserved GPRs: zero (0), ra (1), sp (2), gp (3), tp (4), t1 (6), t2 (7).
const RISCV_RESERVED: [PhysicalRegister; 7] = [
    PhysicalRegister(0), // zero
    PhysicalRegister(1), // ra
    PhysicalRegister(2), // sp
    PhysicalRegister(3), // gp
    PhysicalRegister(4), // tp
    PhysicalRegister(6), // t1 (codegen scratch 1)
    PhysicalRegister(7), // t2 (codegen scratch 2)
];

impl RegisterFile for RiscVRegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &RISCV_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &RISCV_ALLOCATABLE_GPR
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &RISCV_CALLER_SAVED_GPR
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &RISCV_CALLEE_SAVED_GPR
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &RISCV_RESERVED
    }
    fn allocatable_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => &RISCV_ALLOCATABLE_GPR,
            RegisterClass::Float => &RISCV_ALLOCATABLE_FP,
        }
    }
    fn scratch_for_class(&self, class: RegisterClass) -> (PhysicalRegister, PhysicalRegister) {
        match class {
            RegisterClass::Gpr => (PhysicalRegister(6), PhysicalRegister(7)), // t1, t2
            RegisterClass::Float => (PhysicalRegister(62), PhysicalRegister(63)), // ft10, ft11
        }
    }
}

pub struct RiscVBackend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl RiscVBackend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }

    /// The error the encoder returns in place of a placeholder.
    ///
    /// Emitting nothing, or a placeholder register, turns an unencodable operand
    /// form into a silently wrong binary, so every such case names the instruction
    /// and the function that produced it and fails the build instead.
    fn unsupported(
        &self,
        func: &MachineFunction,
        inst: &MachineInstruction,
        detail: impl Into<String>,
    ) -> CodegenError {
        CodegenError::new(
            self.target.triple_string(),
            format!(
                "unsupported machine instruction {:?}: {}",
                inst,
                detail.into()
            ),
        )
        .with_arch("riscv64")
        .with_function(func.name.clone())
    }

    /// Encode a RVV vector arithmetic instruction (`vop.vv vd, vd, vs`), where the
    /// destination also supplies the first source.
    fn vector_arith(
        &self,
        func: &MachineFunction,
        inst: &MachineInstruction,
        dst: &MachineOperand,
        src: &MachineOperand,
        opcode: u32,
        code: &mut Vec<u8>,
    ) -> Result<(), CodegenError> {
        let (Some(d_reg), Some(s_reg)) = (get_fpr(dst), get_fpr(src)) else {
            return Err(self.unsupported(func, inst, "both operands must be V registers"));
        };
        let vd = (d_reg - 32) as u32;
        let vs = (s_reg - 32) as u32;
        let ins = opcode | (vs << 20) | (vd << 15) | (vd << 7);
        code.extend_from_slice(&ins.to_le_bytes());
        Ok(())
    }

    /// Resolve an atomic destination into the register holding its address.
    ///
    /// `AMO`, `LR` and `SC` take a register-only address with no displacement
    /// field, so a frame- or stack-relative offset is materialized here instead of
    /// dropped, and an indexed address is refused rather than encoded without its
    /// index.
    fn atomic_address(
        &self,
        func: &MachineFunction,
        inst: &MachineInstruction,
        dst: &MachineOperand,
        code: &mut Vec<u8>,
    ) -> Result<u32, CodegenError> {
        match dst {
            MachineOperand::Register(_) => get_gpr(dst).map(u32::from).ok_or_else(|| {
                self.unsupported(
                    func,
                    inst,
                    "the destination must name the general-purpose register holding the address",
                )
            }),
            MachineOperand::Memory {
                base,
                offset,
                index,
            } => {
                if index.is_some() {
                    return Err(self.unsupported(
                        func,
                        inst,
                        "an indexed atomic address has no register-only AMO encoding",
                    ));
                }
                let MachineRegister::Physical(base_reg) = base else {
                    return Err(self.unsupported(
                        func,
                        inst,
                        "the atomic address base register is unallocated",
                    ));
                };
                if base_reg.0 >= 32 {
                    return Err(self.unsupported(
                        func,
                        inst,
                        "the atomic address base register must be a general-purpose register",
                    ));
                }
                Ok(self.materialize_address(u32::from(base_reg.0), *offset, code))
            }
            MachineOperand::StackSlot(off) => Ok(self.materialize_address(2, *off, code)),
            other => Err(self.unsupported(
                func,
                inst,
                format!(
                    "an atomic destination must be a register, a memory operand or a stack slot, \
                     not {:?}",
                    other
                ),
            )),
        }
    }

    /// Emit the computation of `base + off` and return the register holding it.
    ///
    /// `base` itself when the offset is zero, otherwise the `t1` scratch register.
    fn materialize_address(&self, base: u32, off: i32, code: &mut Vec<u8>) -> u32 {
        if off == 0 {
            return base;
        }
        if (-2048..=2047).contains(&off) {
            // addi t1, base, off
            let imm12 = (off as u32) & 0xFFF;
            let ins = 0x00000013u32 | (imm12 << 20) | (base << 15) | (6 << 7);
            code.extend_from_slice(&ins.to_le_bytes());
        } else {
            Self::emit_li64(code, 6, off as i64);
            // add t1, base, t1
            let ins = 0x00000033u32 | (6 << 20) | (base << 15) | (6 << 7);
            code.extend_from_slice(&ins.to_le_bytes());
        }
        6
    }

    /// The RV64A width field for an atomic access size.
    ///
    /// RV64A has word and doubleword atomics only; a byte or halfword access needs
    /// an `LR`/`SC` loop, so defaulting the width would silently widen the access.
    fn amo_funct3(
        &self,
        func: &MachineFunction,
        inst: &MachineInstruction,
        size: u8,
    ) -> Result<u32, CodegenError> {
        match size {
            4 => Ok(0x2),
            8 => Ok(0x3),
            other => Err(self.unsupported(
                func,
                inst,
                format!(
                    "atomic access size {other} has no RV64A encoding; the atomics are word (4) \
                     and doubleword (8) wide"
                ),
            )),
        }
    }

    /// The `funct3` width field for an integer load or store.
    ///
    /// RV64 has byte, halfword, word and doubleword memory forms only; anything
    /// else would fall through to the doubleword encoding and move eight bytes
    /// when the program asked for a different width.
    fn mem_funct3(
        &self,
        func: &MachineFunction,
        inst: &MachineInstruction,
        size: u8,
    ) -> Result<u32, CodegenError> {
        match size {
            1 => Ok(0x0),
            2 => Ok(0x1),
            4 => Ok(0x2),
            8 => Ok(0x3),
            other => Err(self.unsupported(
                func,
                inst,
                format!(
                    "access size {other} has no RV64 load or store encoding; the widths are 1, \
                     2, 4 and 8 bytes"
                ),
            )),
        }
    }

    /// The `funct3` width field for a floating-point load or store.
    ///
    /// `flw`/`fld` are the only float memory forms, so a byte or halfword float
    /// access has no encoding to fall back on.
    fn fp_mem_funct3(
        &self,
        func: &MachineFunction,
        inst: &MachineInstruction,
        size: u8,
    ) -> Result<u32, CodegenError> {
        match size {
            4 => Ok(0x2),
            8 => Ok(0x3),
            other => Err(self.unsupported(
                func,
                inst,
                format!(
                    "float access size {other} has no RV64 encoding; a stack float is 4 (flw) or \
                     8 (fld) bytes wide"
                ),
            )),
        }
    }

    /// Emit a doubleword store relative to SP at offset `off` for register `reg`.
    ///
    /// Used only for the frame save area, where every saved GPR and FPR is 8 bytes
    /// under LP64D, so there is no width to choose.
    fn emit_store_sp(code: &mut Vec<u8>, reg: u8, off: i32) {
        let base_reg = if (-2048..=2047).contains(&off) {
            (2u32, off) // sp
        } else {
            Self::emit_li64(code, 6, off as i64); // t1 = off
            let add_ins = 0x00000033u32 | (2 << 20) | (6 << 15) | (6 << 7); // add t1, t1, sp
            code.extend_from_slice(&add_ins.to_le_bytes());
            (6u32, 0)
        };

        let imm = (base_reg.1 as u32) & 0xFFF;
        let imm_hi = (imm >> 5) & 0x7F;
        let imm_lo = imm & 0x1F;

        let (rs2, funct3, opcode) = if reg >= 32 {
            (((reg - 32) as u32) << 20, 0x3u32, 0x27u32) // fsd
        } else {
            ((reg as u32) << 20, 0x3u32, 0x23u32) // sd
        };
        let ins =
            (imm_hi << 25) | rs2 | (base_reg.0 << 15) | (funct3 << 12) | (imm_lo << 7) | opcode;
        code.extend_from_slice(&ins.to_le_bytes());
    }

    /// Emit a doubleword load relative to SP at offset `off` into register `reg`.
    fn emit_load_sp(code: &mut Vec<u8>, reg: u8, off: i32) {
        let base_reg = if (-2048..=2047).contains(&off) {
            (2u32, off) // sp
        } else {
            Self::emit_li64(code, 6, off as i64); // t1 = off
            let add_ins = 0x00000033u32 | (2 << 20) | (6 << 15) | (6 << 7); // add t1, t1, sp
            code.extend_from_slice(&add_ins.to_le_bytes());
            (6u32, 0)
        };

        let imm12 = (base_reg.1 as u32) & 0xFFF;
        let (rd, opcode) = if reg >= 32 {
            ((reg - 32) as u32, 0x7u32) // fld
        } else {
            (reg as u32, 0x3u32) // ld
        };
        let ins = opcode | (imm12 << 20) | (base_reg.0 << 15) | (0x3 << 12) | (rd << 7);
        code.extend_from_slice(&ins.to_le_bytes());
    }

    /// Emit 64-bit immediate loading into register `rd`.
    fn emit_li64(code: &mut Vec<u8>, rd: u8, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            // addi rd, x0, imm12
            let imm12 = (imm as u32) & 0xFFF;
            let ins = 0x00000013u32 | (imm12 << 20) | ((rd as u32) << 7);
            code.extend_from_slice(&ins.to_le_bytes());
        } else if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
            let val32 = imm as i32;
            let hi20 = (((val32.wrapping_add(0x800)) as u32) >> 12) & 0xF_FFFF;
            let lo12 = (val32 as u32) & 0xFFF;
            // lui rd, hi20
            let lui = 0x00000037u32 | (hi20 << 12) | ((rd as u32) << 7);
            code.extend_from_slice(&lui.to_le_bytes());
            if lo12 != 0 {
                // addiw rd, rd, lo12
                let addiw = 0x0000001Bu32 | (lo12 << 20) | ((rd as u32) << 15) | ((rd as u32) << 7);
                code.extend_from_slice(&addiw.to_le_bytes());
            }
        } else {
            // 64-bit decomposition using lui/addiw, slli by 32, and scratch t1 (6)
            let hi32 = ((imm as u64) >> 32) as i32;
            let lo32 = (imm as u64 & 0xFFFF_FFFF) as i32;

            let hi20_hi = (((hi32.wrapping_add(0x800)) as u32) >> 12) & 0xF_FFFF;
            let lo12_hi = (hi32 as u32) & 0xFFF;
            let lui = 0x00000037u32 | (hi20_hi << 12) | ((rd as u32) << 7);
            code.extend_from_slice(&lui.to_le_bytes());
            if lo12_hi != 0 {
                let addiw =
                    0x0000001Bu32 | (lo12_hi << 20) | ((rd as u32) << 15) | ((rd as u32) << 7);
                code.extend_from_slice(&addiw.to_le_bytes());
            }
            // slli rd, rd, 32
            let slli = 0x00001013u32 | (32 << 20) | ((rd as u32) << 15) | ((rd as u32) << 7);
            code.extend_from_slice(&slli.to_le_bytes());

            // load low 32-bits into scratch t1 (6)
            let hi20_lo = (((lo32.wrapping_add(0x800)) as u32) >> 12) & 0xF_FFFF;
            let lo12_lo = (lo32 as u32) & 0xFFF;
            let lui_scratch = 0x00000037u32 | (hi20_lo << 12) | (6 << 7);
            code.extend_from_slice(&lui_scratch.to_le_bytes());
            if lo12_lo != 0 {
                let addiw_scratch = 0x0000001Bu32 | (lo12_lo << 20) | (6 << 15) | (6 << 7);
                code.extend_from_slice(&addiw_scratch.to_le_bytes());
            }
            // zero-extend scratch t1: slli t1, t1, 32; srli t1, t1, 32
            let slli_t1 = 0x00001013u32 | (32 << 20) | (6 << 15) | (6 << 7);
            let srli_t1 = 0x00005013u32 | (32 << 20) | (6 << 15) | (6 << 7);
            code.extend_from_slice(&slli_t1.to_le_bytes());
            code.extend_from_slice(&srli_t1.to_le_bytes());
            // or rd, rd, t1
            let or_ins = 0x00006033u32 | (6 << 20) | ((rd as u32) << 15) | ((rd as u32) << 7);
            code.extend_from_slice(&or_ins.to_le_bytes());
        }
    }

    /// Validate a `slli`/`srli`/`srai` shift amount.
    ///
    /// The 6-bit `shamt` field wraps at 64, so masking an out-of-range constant
    /// would silently shift by a different amount than the program asked for.
    fn shift_amount(
        &self,
        func: &MachineFunction,
        inst: &MachineInstruction,
        val: i64,
        mnemonic: &str,
    ) -> Result<u32, CodegenError> {
        if (0..64).contains(&val) {
            Ok(val as u32)
        } else {
            Err(self.unsupported(
                func,
                inst,
                format!(
                    "{mnemonic} shift amount {val} does not fit the 6-bit shamt field; masking it \
                     to {} would shift by a different amount than the program asked for",
                    (val as u32) & 0x3F
                ),
            ))
        }
    }

    /// Encode a conditional branch, or `None` when the condition has no branch form.
    ///
    /// `Parity` and `NotParity` need a flag RV64 does not have; mapping them onto
    /// `beq` would take a branch on values nothing compared.
    fn encode_branch(cc: ConditionCode, rs1: u32, rs2: u32, disp: i32) -> Option<u32> {
        let (funct3, actual_rs1, actual_rs2) = match cc {
            ConditionCode::Equal | ConditionCode::Zero => (0x0, rs1, rs2),
            ConditionCode::NotEqual | ConditionCode::NotZero => (0x1, rs1, rs2),
            ConditionCode::LessThan => (0x4, rs1, rs2),
            ConditionCode::GreaterOrEqual => (0x5, rs1, rs2),
            ConditionCode::GreaterThan => (0x4, rs2, rs1),
            ConditionCode::LessOrEqual => (0x5, rs2, rs1),
            ConditionCode::Below => (0x6, rs1, rs2),
            ConditionCode::AboveOrEqual => (0x7, rs1, rs2),
            ConditionCode::Above => (0x6, rs2, rs1),
            ConditionCode::BelowOrEqual => (0x7, rs2, rs1),
            ConditionCode::Parity | ConditionCode::NotParity => return None,
        };
        let imm = ((disp >> 1) as u32) & 0xFFF;
        let imm12 = (imm >> 11) & 1;
        let imm10_5 = (imm >> 4) & 0x3F;
        let imm4_1 = imm & 0xF;
        let imm11 = (imm >> 10) & 1;
        Some(
            (imm12 << 31)
                | (imm10_5 << 25)
                | (actual_rs2 << 20)
                | (actual_rs1 << 15)
                | (funct3 << 12)
                | (imm4_1 << 8)
                | (imm11 << 7)
                | 0x63,
        )
    }

    /// Generate function code and collect the relocations its instruction pairs
    /// need, with offsets relative to the start of the function.
    pub fn generate_function_with_relocs(
        &mut self,
        func: &MachineFunction,
    ) -> Result<FunctionCodeWithRelocs, CodegenError> {
        if !self.target.architecture.is_64bit() {
            return Err(CodegenError::new(
                self.target.triple_string(),
                "the RISC-V backend only implements RV64 (doubleword) encodings; refusing to emit RV32-invalid code",
            )
            .with_arch("riscv32")
            .with_function(func.name.clone()));
        }

        let mut expanded = func.clone();
        Self::expand_parallel_moves(&mut expanded)?;
        let func = &expanded;

        if let Some(reg) = first_unallocated_reg(func) {
            return Err(CodegenError::new(
                self.target.triple_string(),
                format!(
                    "unallocated register {:?} reached the RISC-V encoder; \
                     lower_module must run register allocation before emit_object",
                    reg
                ),
            )
            .with_arch("riscv64")
            .with_function(func.name.clone()));
        }

        let mut code = Vec::new();
        let mut label_offsets: HashMap<String, usize> = HashMap::new();
        let mut branch_fixups: Vec<(usize, String, bool, ConditionCode, u32, u32)> = Vec::new();
        let mut relocs: Vec<AdobRelocation> = Vec::new();

        // Find modified callee-saved registers in function body
        let mut used_callee_gprs: Vec<u8> = Vec::new();
        let mut used_callee_fprs: Vec<u8> = Vec::new();

        for block in &func.blocks {
            for inst in &block.instructions {
                for def in inst.defs() {
                    if let Some(p) = def.physical() {
                        if RISCV_CALLEE_SAVED_GPR.contains(&p)
                            && p.0 != 8
                            && !used_callee_gprs.contains(&p.0)
                        {
                            used_callee_gprs.push(p.0);
                        } else if p.0 >= 32 && p.0 < 64 {
                            let callee_fp = [40u8, 41, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59];
                            if callee_fp.contains(&p.0) && !used_callee_fprs.contains(&p.0) {
                                used_callee_fprs.push(p.0);
                            }
                        }
                    }
                }
            }
        }
        used_callee_gprs.sort_unstable();
        used_callee_fprs.sort_unstable();

        // Calculate dynamic stack frame size
        // Fixed saves: ra (8), s0/fp (8) = 16 bytes
        let callee_save_bytes = (used_callee_gprs.len() + used_callee_fprs.len()) * 8;
        let unaligned_frame = 16 + callee_save_bytes + (func.stack_size as usize);
        let frame_size = (unaligned_frame + 15) & !15;

        // Emit Prologue:
        // Adjust SP
        if (-(frame_size as i64)) >= -2048 {
            let imm12 = (-(frame_size as i32) as u32) & 0xFFF;
            let addi_sp = 0x00010113u32 | (imm12 << 20); // addi sp, sp, -frame_size
            code.extend_from_slice(&addi_sp.to_le_bytes());
        } else {
            Self::emit_li64(&mut code, 6, -(frame_size as i64)); // t1 = -frame_size
            let add_sp = 0x00010133u32 | (6 << 20) | (2 << 15) | (2 << 7); // add sp, sp, t1
            code.extend_from_slice(&add_sp.to_le_bytes());
        }

        // Save RA and S0 at top of frame
        let ra_off = frame_size as i32 - 8;
        let s0_off = frame_size as i32 - 16;

        Self::emit_store_sp(&mut code, 1, ra_off); // sd ra, ra_off(sp)
        Self::emit_store_sp(&mut code, 8, s0_off); // sd s0, s0_off(sp)

        // Save additional callee-saved GPRs and FPRs
        let mut save_off = frame_size as i32 - 24;
        for &gpr in &used_callee_gprs {
            Self::emit_store_sp(&mut code, gpr, save_off);
            save_off -= 8;
        }
        for &fpr in &used_callee_fprs {
            Self::emit_store_sp(&mut code, fpr, save_off);
            save_off -= 8;
        }

        // Set frame pointer s0 = sp + frame_size
        if (frame_size as i64) <= 2047 {
            let imm12 = (frame_size as u32) & 0xFFF;
            let addi_s0 = 0x00010413u32 | (imm12 << 20); // addi s0, sp, frame_size
            code.extend_from_slice(&addi_s0.to_le_bytes());
        } else {
            Self::emit_li64(&mut code, 6, frame_size as i64);
            let add_s0 = 0x00010433u32 | (6 << 20) | (2 << 15) | (8 << 7); // add s0, sp, t1
            code.extend_from_slice(&add_s0.to_le_bytes());
        }

        // Track last comparison registers (rs1, rs2) for SetCc and BranchCc.
        //
        // `None` means no `Compare` has produced flags yet. Defaulting to a pair of
        // registers would make a `SetCc`/`BranchCc` that lost its `Compare` encode a
        // real branch on values nothing compared, so the absence is kept explicit and
        // reported.
        let mut last_cmp_regs: Option<(u32, u32)> = None;

        for block in &func.blocks {
            label_offsets.insert(block.label.clone(), code.len());
            // Flags from a previous block do not survive the jump into this one.
            last_cmp_regs = None;

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        // NOP: addi x0, x0, 0
                        code.extend_from_slice(&0x00000013u32.to_le_bytes());
                    }
                    MachineInstruction::Return => {
                        // Restore callee-saved FPRs and GPRs
                        let mut rest_off = frame_size as i32 - 24;
                        for &gpr in &used_callee_gprs {
                            Self::emit_load_sp(&mut code, gpr, rest_off);
                            rest_off -= 8;
                        }
                        for &fpr in &used_callee_fprs {
                            Self::emit_load_sp(&mut code, fpr, rest_off);
                            rest_off -= 8;
                        }

                        // Restore RA and S0
                        Self::emit_load_sp(&mut code, 1, ra_off); // ld ra, ra_off(sp)
                        Self::emit_load_sp(&mut code, 8, s0_off); // ld s0, s0_off(sp)

                        // Deallocate frame
                        if (frame_size as i64) <= 2047 {
                            let imm12 = (frame_size as u32) & 0xFFF;
                            let addi_sp = 0x00010113u32 | (imm12 << 20); // addi sp, sp, frame_size
                            code.extend_from_slice(&addi_sp.to_le_bytes());
                        } else {
                            Self::emit_li64(&mut code, 6, frame_size as i64); // t1 = frame_size
                            let add_sp = 0x00010133u32 | (6 << 20) | (2 << 15) | (2 << 7); // add sp, sp, t1
                            code.extend_from_slice(&add_sp.to_le_bytes());
                        }
                        // ret: jalr x0, 0(ra)
                        code.extend_from_slice(&0x00008067u32.to_le_bytes());
                    }
                    MachineInstruction::Move { dst, src } => {
                        let (d, s) = (get_reg(dst), get_reg(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            if d_reg >= 32 && s_reg >= 32 {
                                // fsgnj.d fd, fs, fs (Float to Float)
                                let fd = (d_reg - 32) as u32;
                                let fs = (s_reg - 32) as u32;
                                let ins = 0x22000053u32 | (fs << 20) | (fs << 15) | (fd << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else if d_reg >= 32 && s_reg < 32 {
                                // fmv.d.x fd, rs (GPR to Float)
                                let fd = (d_reg - 32) as u32;
                                let ins = 0xF2000053u32 | ((s_reg as u32) << 15) | (fd << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else if d_reg < 32 && s_reg >= 32 {
                                // fmv.x.d rd, fs (Float to GPR)
                                let fs = (s_reg - 32) as u32;
                                let ins = 0xE2000053u32 | (fs << 15) | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                // addi rd, rs, 0 (MV rd, rs)
                                let ins =
                                    0x00000013u32 | ((s_reg as u32) << 15) | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) =
                            (get_gpr(dst), src)
                        {
                            Self::emit_li64(&mut code, d_reg, *val);
                        } else if let (Some(d_reg), MachineOperand::Symbol(sym)) =
                            (get_gpr(dst), src)
                        {
                            // auipc rd, %pcrel_hi(sym) followed by addi rd, rd, %pcrel_lo(sym)
                            relocs.push(AdobRelocation::new(
                                code.len() as u64,
                                0,
                                sym.clone(),
                                RelocationKind::RiscV_PcrelHi20,
                                0,
                            ));
                            let auipc = 0x00000017u32 | ((d_reg as u32) << 7);
                            code.extend_from_slice(&auipc.to_le_bytes());

                            // The LO12 half is relative to the AUIPC, not to this
                            // ADDI. The generic relocation engine subtracts each
                            // relocation's own address, so +4 cancels the gap
                            // between the two words. This holds only while the
                            // pair stays adjacent and uncompressed.
                            relocs.push(AdobRelocation::new(
                                code.len() as u64,
                                0,
                                sym.clone(),
                                RelocationKind::RiscV_PcrelLo12I,
                                4,
                            ));
                            let addi =
                                0x00000013u32 | ((d_reg as u32) << 15) | ((d_reg as u32) << 7);
                            code.extend_from_slice(&addi.to_le_bytes());
                        } else {
                            let detail = match (dst, src) {
                                (
                                    MachineOperand::Register(_),
                                    MachineOperand::FloatImmediate(_),
                                ) => "no `fmv` form takes a float immediate; the constant has to \
                                      reach a general-purpose register first"
                                    .to_string(),
                                (MachineOperand::Register(_), other)
                                | (other, MachineOperand::Register(_)) => {
                                    format!("no move form covers a register and {:?}", other)
                                }
                                _ => {
                                    "neither operand is an encodable physical register".to_string()
                                }
                            };
                            return Err(self.unsupported(func, inst, detail));
                        }
                    }
                    MachineInstruction::Add { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // add rd, rd, rs2
                            let ins = 0x00000033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            if *val >= -2048 && *val <= 2047 {
                                let imm12 = (*val as u32) & 0xFFF;
                                let ins = 0x00000013u32
                                    | (imm12 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                Self::emit_li64(&mut code, 6, *val); // t1
                                let ins = 0x00000033u32
                                    | (6 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no add form covers this destination and source; the destination \
                                 must be a general-purpose register and the source a \
                                 general-purpose register or an immediate",
                            ));
                        }
                    }
                    MachineInstruction::Sub { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // sub rd, rd, rs2
                            let ins = 0x40000033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            Self::emit_li64(&mut code, 6, *val); // t1
                            let ins = 0x40000033u32
                                | (6 << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no sub form covers this destination and source; the destination \
                                 must be a general-purpose register and the source a \
                                 general-purpose register or an immediate",
                            ));
                        }
                    }
                    MachineInstruction::Mul { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // mul rd, rd, rs2
                            let ins = 0x02000033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "mul needs two general-purpose register operands; there is no \
                                 multiply-immediate form here",
                            ));
                        }
                    }
                    MachineInstruction::Div { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // div rd, rd, rs2
                            let ins = 0x02004033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "div needs two general-purpose register operands; there is no \
                                 divide-immediate form here",
                            ));
                        }
                    }
                    MachineInstruction::Mod { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // rem rd, rd, rs2
                            let ins = 0x02006033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "rem needs two general-purpose register operands; there is no \
                                 remainder-immediate form here",
                            ));
                        }
                    }
                    MachineInstruction::Neg { dst } => {
                        if let Some(d_reg) = get_gpr(dst) {
                            // neg rd, rd -> sub rd, x0, rd
                            let ins =
                                0x40000033u32 | ((d_reg as u32) << 20) | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "neg destination must be a general-purpose register",
                            ));
                        }
                    }
                    MachineInstruction::Not { dst } => {
                        if let Some(d_reg) = get_gpr(dst) {
                            // not rd, rd -> xori rd, rd, -1
                            let ins =
                                0xFFF04013u32 | ((d_reg as u32) << 15) | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "not destination must be a general-purpose register",
                            ));
                        }
                    }
                    MachineInstruction::And { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let ins = 0x00007033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            if *val >= -2048 && *val <= 2047 {
                                let imm12 = (*val as u32) & 0xFFF;
                                let ins = 0x00007013u32
                                    | (imm12 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                Self::emit_li64(&mut code, 6, *val);
                                let ins = 0x00007033u32
                                    | (6 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no and form covers this destination and source",
                            ));
                        }
                    }
                    MachineInstruction::Or { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let ins = 0x00006033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            if *val >= -2048 && *val <= 2047 {
                                let imm12 = (*val as u32) & 0xFFF;
                                let ins = 0x00006013u32
                                    | (imm12 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                Self::emit_li64(&mut code, 6, *val);
                                let ins = 0x00006033u32
                                    | (6 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no or form covers this destination and source",
                            ));
                        }
                    }
                    MachineInstruction::Xor { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let ins = 0x00004033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            if *val >= -2048 && *val <= 2047 {
                                let imm12 = (*val as u32) & 0xFFF;
                                let ins = 0x00004013u32
                                    | (imm12 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                Self::emit_li64(&mut code, 6, *val);
                                let ins = 0x00004033u32
                                    | (6 << 20)
                                    | ((d_reg as u32) << 15)
                                    | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no xor form covers this destination and source",
                            ));
                        }
                    }
                    MachineInstruction::Shl { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // sll rd, rd, rs2
                            let ins = 0x00001033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            // slli rd, rd, shamt
                            let shamt = self.shift_amount(func, inst, *val, "slli")?;
                            let ins = 0x00001013u32
                                | (shamt << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no shift-left form covers this destination and source",
                            ));
                        }
                    }
                    MachineInstruction::Shr { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // srl rd, rd, rs2
                            let ins = 0x00005033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            // srli rd, rd, shamt
                            let shamt = self.shift_amount(func, inst, *val, "srli")?;
                            let ins = 0x00005013u32
                                | (shamt << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no logical shift-right form covers this destination and source",
                            ));
                        }
                    }
                    MachineInstruction::Sar { dst, src } => {
                        let (d, s) = (get_gpr(dst), get_gpr(src));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // sra rd, rd, rs2
                            let ins = 0x40005033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            // srai rd, rd, shamt
                            let shamt = self.shift_amount(func, inst, *val, "srai")?;
                            let ins = 0x40005013u32
                                | (shamt << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "no arithmetic shift-right form covers this destination and source",
                            ));
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs } => {
                        let (d, s) = (get_gpr(lhs), get_gpr(rhs));
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            last_cmp_regs = Some((d_reg as u32, s_reg as u32));
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, rhs) {
                            if *val == 0 {
                                last_cmp_regs = Some((d_reg as u32, 0)); // x0
                            } else {
                                Self::emit_li64(&mut code, 7, *val); // load into t2 (7)
                                last_cmp_regs = Some((d_reg as u32, 7));
                            }
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "compare needs a general-purpose register operand and a register \
                                 or immediate operand; the flags it produces are what the next \
                                 setcc or branchcc reads, so nothing can stand in for it",
                            ));
                        }
                    }
                    MachineInstruction::SetCc { dst, cc } => {
                        if let Some(d_reg) = get_gpr(dst) {
                            let rd = d_reg as u32;
                            let Some((r1, r2)) = last_cmp_regs else {
                                return Err(self.unsupported(
                                    func,
                                    inst,
                                    "no compare produced the flags this setcc reads; the register \
                                     pair it would branch on is either stale or from another \
                                     block, and encoding a guess would test values nothing \
                                     compared",
                                ));
                            };
                            match cc {
                                ConditionCode::Equal | ConditionCode::Zero => {
                                    // sub t1, r1, r2; sltiu rd, t1, 1 (seqz rd, t1)
                                    let sub_ins =
                                        0x40000033u32 | (r2 << 20) | (r1 << 15) | (6 << 7);
                                    let seqz_ins = 0x00133013u32 | (6 << 15) | (rd << 7);
                                    code.extend_from_slice(&sub_ins.to_le_bytes());
                                    code.extend_from_slice(&seqz_ins.to_le_bytes());
                                }
                                ConditionCode::NotEqual | ConditionCode::NotZero => {
                                    // sub t1, r1, r2; sltu rd, x0, t1 (snez rd, t1)
                                    let sub_ins =
                                        0x40000033u32 | (r2 << 20) | (r1 << 15) | (6 << 7);
                                    let snez_ins = 0x00003033u32 | (6 << 20) | (rd << 7);
                                    code.extend_from_slice(&sub_ins.to_le_bytes());
                                    code.extend_from_slice(&snez_ins.to_le_bytes());
                                }
                                ConditionCode::LessThan => {
                                    // slt rd, r1, r2
                                    let ins = 0x00002033u32 | (r2 << 20) | (r1 << 15) | (rd << 7);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                                ConditionCode::GreaterThan => {
                                    // slt rd, r2, r1
                                    let ins = 0x00002033u32 | (r1 << 20) | (r2 << 15) | (rd << 7);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                                ConditionCode::GreaterOrEqual => {
                                    // slt rd, r1, r2; xori rd, rd, 1
                                    let slt_ins =
                                        0x00002033u32 | (r2 << 20) | (r1 << 15) | (rd << 7);
                                    let xori_ins = 0x00104013u32 | (rd << 15) | (rd << 7);
                                    code.extend_from_slice(&slt_ins.to_le_bytes());
                                    code.extend_from_slice(&xori_ins.to_le_bytes());
                                }
                                ConditionCode::LessOrEqual => {
                                    // slt rd, r2, r1; xori rd, rd, 1
                                    let slt_ins =
                                        0x00002033u32 | (r1 << 20) | (r2 << 15) | (rd << 7);
                                    let xori_ins = 0x00104013u32 | (rd << 15) | (rd << 7);
                                    code.extend_from_slice(&slt_ins.to_le_bytes());
                                    code.extend_from_slice(&xori_ins.to_le_bytes());
                                }
                                ConditionCode::Below => {
                                    // sltu rd, r1, r2
                                    let ins = 0x00003033u32 | (r2 << 20) | (r1 << 15) | (rd << 7);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                                ConditionCode::Above => {
                                    // sltu rd, r2, r1
                                    let ins = 0x00003033u32 | (r1 << 20) | (r2 << 15) | (rd << 7);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                                ConditionCode::AboveOrEqual => {
                                    // sltu rd, r1, r2; xori rd, rd, 1
                                    let sltu_ins =
                                        0x00003033u32 | (r2 << 20) | (r1 << 15) | (rd << 7);
                                    let xori_ins = 0x00104013u32 | (rd << 15) | (rd << 7);
                                    code.extend_from_slice(&sltu_ins.to_le_bytes());
                                    code.extend_from_slice(&xori_ins.to_le_bytes());
                                }
                                ConditionCode::BelowOrEqual => {
                                    // sltu rd, r2, r1; xori rd, rd, 1
                                    let sltu_ins =
                                        0x00003033u32 | (r1 << 20) | (r2 << 15) | (rd << 7);
                                    let xori_ins = 0x00104013u32 | (rd << 15) | (rd << 7);
                                    code.extend_from_slice(&sltu_ins.to_le_bytes());
                                    code.extend_from_slice(&xori_ins.to_le_bytes());
                                }
                                ConditionCode::Parity | ConditionCode::NotParity => {
                                    // RISC-V has no parity or carry flag to read; encoding these
                                    // as a constant, or as another condition, changes what the
                                    // program tests.
                                    return Err(self.unsupported(
                                        func,
                                        inst,
                                        "the RV64 compare/setcc pair has no parity condition to \
                                         encode, and no other condition means the same thing",
                                    ));
                                }
                            }
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "setcc destination must be a general-purpose register",
                            ));
                        }
                    }
                    MachineInstruction::Branch { target } => {
                        let offset = code.len();
                        branch_fixups.push((
                            offset,
                            target.clone(),
                            false,
                            ConditionCode::Equal,
                            0,
                            0,
                        ));
                        code.extend_from_slice(&0x0000006Fu32.to_le_bytes()); // JAL x0, #0
                    }
                    MachineInstruction::BranchCc { cc, target } => {
                        let (r1, r2) = match last_cmp_regs {
                            Some(pair) => pair,
                            None => {
                                return Err(self.unsupported(
                                    func,
                                    inst,
                                    "no compare produced the flags this branch reads",
                                ));
                            }
                        };
                        if matches!(cc, ConditionCode::Parity | ConditionCode::NotParity) {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "the RV64 branch forms have no parity condition to encode",
                            ));
                        }
                        let offset = code.len();
                        branch_fixups.push((offset, target.clone(), true, *cc, r1, r2));
                        code.extend_from_slice(&0x00000063u32.to_le_bytes()); // BEQ placeholder
                    }
                    MachineInstruction::Call { target, .. } => {
                        if let Some(r) = get_gpr(target) {
                            // JALR ra, 0(rs1)
                            let ins = 0x000000E7u32 | ((r as u32) << 15);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let MachineOperand::Symbol(sym) = target {
                            let call_off = code.len();
                            relocs.push(AdobRelocation::new(
                                call_off as u64,
                                0,
                                sym.clone(),
                                RelocationKind::RiscV_Call,
                                0,
                            ));
                            // Emit AUIPC ra, 0; JALR ra, ra, 0 pair (8 bytes)
                            code.extend_from_slice(&0x00000097u32.to_le_bytes()); // auipc ra, 0
                            code.extend_from_slice(&0x000080E7u32.to_le_bytes()); // jalr ra, ra, 0
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "call target must be a general-purpose register or a symbol; a \
                                 JAL with no relocation branches to a displacement nothing \
                                 patches",
                            ));
                        }
                        // A call destroys t1/t2, which is where an immediate operand of the
                        // previous compare may have been materialized.
                        last_cmp_regs = None;
                    }
                    MachineInstruction::Load { dst, src, size } => {
                        if let (Some(d_reg), MachineOperand::StackSlot(slot)) = (get_reg(dst), src)
                        {
                            let off = *slot;
                            let base_reg = if (-2048..=2047).contains(&off) {
                                (2u32, off) // sp
                            } else {
                                Self::emit_li64(&mut code, 6, off as i64); // t1 = off
                                let add_ins = 0x00000033u32 | (2 << 20) | (6 << 15) | (6 << 7); // add t1, t1, sp
                                code.extend_from_slice(&add_ins.to_le_bytes());
                                (6u32, 0)
                            };

                            let imm12 = (base_reg.1 as u32) & 0xFFF;
                            let funct3 = if d_reg >= 32 {
                                self.fp_mem_funct3(func, inst, *size)?
                            } else {
                                self.mem_funct3(func, inst, *size)?
                            };
                            let reg = if d_reg >= 32 {
                                (d_reg - 32) as u32
                            } else {
                                d_reg as u32
                            };
                            let opcode = if d_reg >= 32 { 0x7 } else { 0x3 };
                            let ins = opcode
                                | (imm12 << 20)
                                | (base_reg.0 << 15)
                                | (funct3 << 12)
                                | (reg << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                if matches!(src, MachineOperand::Memory { .. }) {
                                    "indexed register addressing is not implemented; a load needs \
                                     a register destination and a frame-relative stack slot \
                                     source"
                                } else {
                                    "a load needs a physical register destination and a \
                                     frame-relative stack slot source"
                                },
                            ));
                        }
                    }
                    MachineInstruction::Store { dst, src, size } => {
                        if let (MachineOperand::StackSlot(slot), Some(s_reg)) = (dst, get_reg(src))
                        {
                            let off = *slot;
                            let base_reg = if (-2048..=2047).contains(&off) {
                                (2u32, off) // sp
                            } else {
                                Self::emit_li64(&mut code, 6, off as i64); // t1 = off
                                let add_ins = 0x00000033u32 | (2 << 20) | (6 << 15) | (6 << 7); // add t1, t1, sp
                                code.extend_from_slice(&add_ins.to_le_bytes());
                                (6u32, 0)
                            };

                            let imm = (base_reg.1 as u32) & 0xFFF;
                            let imm_hi = (imm >> 5) & 0x7F;
                            let imm_lo = imm & 0x1F;

                            let (rs2, funct3, opcode) = if s_reg >= 32 {
                                (
                                    ((s_reg - 32) as u32) << 20,
                                    self.fp_mem_funct3(func, inst, *size)?,
                                    0x27u32,
                                )
                            } else {
                                (
                                    (s_reg as u32) << 20,
                                    self.mem_funct3(func, inst, *size)?,
                                    0x23u32,
                                )
                            };
                            let ins = (imm_hi << 25)
                                | rs2
                                | (base_reg.0 << 15)
                                | (funct3 << 12)
                                | (imm_lo << 7)
                                | opcode;
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                if matches!(dst, MachineOperand::Memory { .. }) {
                                    "indexed register addressing is not implemented; a store \
                                     needs a frame-relative stack slot destination and a \
                                     register source"
                                } else {
                                    "a store needs a frame-relative stack slot destination and a \
                                     physical register source"
                                },
                            ));
                        }
                    }
                    MachineInstruction::Push { src } => {
                        if let Some(s_reg) = get_gpr(src) {
                            // addi sp, sp, -16
                            code.extend_from_slice(&0xFF010113u32.to_le_bytes());
                            // sd rs, 0(sp)
                            let ins = 0x00013023u32 | ((s_reg as u32) << 20);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "push source must be a general-purpose register; no float or \
                                 vector push form is implemented",
                            ));
                        }
                    }
                    MachineInstruction::Pop { dst } => {
                        if let Some(d_reg) = get_gpr(dst) {
                            // ld rd, 0(sp)
                            let ins = 0x00013003u32 | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                            // addi sp, sp, 16
                            code.extend_from_slice(&0x01010113u32.to_le_bytes());
                        } else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "pop destination must be a general-purpose register; no float or \
                                 vector pop form is implemented",
                            ));
                        }
                    }
                    // Floating-Point (F0..F31)
                    MachineInstruction::FAdd { dst, src, size } => {
                        let (Some(d_reg), Some(s_reg)) = (get_fpr(dst), get_fpr(src)) else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "both operands must be F registers",
                            ));
                        };
                        let fd = (d_reg - 32) as u32;
                        let fs = (s_reg - 32) as u32;
                        let opcode_prefix = if *size == 4 {
                            0x00000053u32
                        } else {
                            0x02000053u32
                        };
                        let ins = opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::FSub { dst, src, size } => {
                        let (Some(d_reg), Some(s_reg)) = (get_fpr(dst), get_fpr(src)) else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "both operands must be F registers",
                            ));
                        };
                        let fd = (d_reg - 32) as u32;
                        let fs = (s_reg - 32) as u32;
                        let opcode_prefix = if *size == 4 {
                            0x08000053u32
                        } else {
                            0x0A000053u32
                        };
                        let ins = opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::FMul { dst, src, size } => {
                        let (Some(d_reg), Some(s_reg)) = (get_fpr(dst), get_fpr(src)) else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "both operands must be F registers",
                            ));
                        };
                        let fd = (d_reg - 32) as u32;
                        let fs = (s_reg - 32) as u32;
                        let opcode_prefix = if *size == 4 {
                            0x10000053u32
                        } else {
                            0x12000053u32
                        };
                        let ins = opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::FDiv { dst, src, size } => {
                        let (Some(d_reg), Some(s_reg)) = (get_fpr(dst), get_fpr(src)) else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "both operands must be F registers",
                            ));
                        };
                        let fd = (d_reg - 32) as u32;
                        let fs = (s_reg - 32) as u32;
                        let opcode_prefix = if *size == 4 {
                            0x18000053u32
                        } else {
                            0x1A000053u32
                        };
                        let ins = opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::FNeg { dst, size } => {
                        let Some(d_reg) = get_fpr(dst) else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "fneg operand must be an F register",
                            ));
                        };
                        let fd = (d_reg - 32) as u32;
                        // fsgnjn.d fd, fd, fd
                        let funct7 = if *size == 4 {
                            0x20000053u32
                        } else {
                            0x22000053u32
                        };
                        let ins = funct7 | (fd << 20) | (fd << 15) | (1 << 12) | (fd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::FCvtIntToFloat { dst, src, .. } => {
                        let (Some(d_reg), Some(s_reg)) = (get_fpr(dst), get_gpr(src)) else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "the destination must be an F register and the source a general-\
                                 purpose register",
                            ));
                        };
                        let fd = (d_reg - 32) as u32;
                        let rs = s_reg as u32;
                        // fcvt.d.l fd, rs
                        let ins = 0xD2200053u32 | (2 << 20) | (rs << 15) | (7 << 12) | (fd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::FCvtFloatToInt { dst, src, .. } => {
                        let (Some(d_reg), Some(s_reg)) = (get_gpr(dst), get_fpr(src)) else {
                            return Err(self.unsupported(
                                func,
                                inst,
                                "the destination must be a general-purpose register and the \
                                 source an F register",
                            ));
                        };
                        let rd = d_reg as u32;
                        let fs = (s_reg - 32) as u32;
                        // fcvt.l.d rd, fs
                        let ins = 0xC2200053u32 | (2 << 20) | (fs << 15) | (7 << 12) | (rd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    // RISC-V Vector Extension (RVV 1.0)
                    MachineInstruction::VectorAdd { dst, src, .. } => {
                        self.vector_arith(func, inst, dst, src, 0x02000057u32, &mut code)?;
                    }
                    MachineInstruction::VectorSub { dst, src, .. } => {
                        self.vector_arith(func, inst, dst, src, 0x0A000057u32, &mut code)?;
                    }
                    MachineInstruction::VectorMul { dst, src, .. } => {
                        self.vector_arith(func, inst, dst, src, 0x92002057u32, &mut code)?;
                    }
                    MachineInstruction::VectorMin { dst, src, .. } => {
                        self.vector_arith(func, inst, dst, src, 0x12000057u32, &mut code)?;
                    }
                    MachineInstruction::VectorMax { dst, src, .. } => {
                        self.vector_arith(func, inst, dst, src, 0x1A000057u32, &mut code)?;
                    }
                    MachineInstruction::VectorLoad { .. }
                    | MachineInstruction::VectorStore { .. } => {
                        // Unit-stride vector loads and stores take their address from a register
                        // with no displacement field. The encodings below this comment previously
                        // ignored their address operand and used `sp`, so every one of them read
                        // or wrote the stack instead of what the program named.
                        return Err(self.unsupported(
                            func,
                            inst,
                            "no vector load or store form materializes its address; the address \
                             operand would be dropped",
                        ));
                    }
                    // Atomics & Memory Ordering (RV64A)
                    MachineInstruction::Barrier => {
                        // FENCE iorw, iorw
                        code.extend_from_slice(&0x0FF0000Fu32.to_le_bytes());
                    }
                    MachineInstruction::AtomicFetchAdd { dst, src, size } => {
                        let base_reg = self.atomic_address(func, inst, dst, &mut code)?;
                        let funct3 = self.amo_funct3(func, inst, *size)?;
                        let rs2 = get_gpr(src).map(u32::from).ok_or_else(|| {
                            self.unsupported(
                                func,
                                inst,
                                "the atomic addend must be a general-purpose register",
                            )
                        })?;
                        // AMOADD.W/D rd, rs2, (rs1): the addend register also receives the value
                        // the memory held, which is what this form declares as its definition.
                        let ins = 0x0000002Fu32
                            | (rs2 << 20)
                            | (base_reg << 15)
                            | (funct3 << 12)
                            | (rs2 << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::AtomicExchange { dst, src, size } => {
                        let base_reg = self.atomic_address(func, inst, dst, &mut code)?;
                        let funct3 = self.amo_funct3(func, inst, *size)?;
                        let rs2 = get_gpr(src).map(u32::from).ok_or_else(|| {
                            self.unsupported(
                                func,
                                inst,
                                "the atomic value must be a general-purpose register",
                            )
                        })?;
                        // AMOSWAP.W/D rd, rs2, (rs1)
                        let ins = 0x0800002Fu32
                            | (rs2 << 20)
                            | (base_reg << 15)
                            | (funct3 << 12)
                            | (rs2 << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::AtomicCompareExchange {
                        dst,
                        expected,
                        desired,
                        size,
                    } => {
                        let base_reg = self.atomic_address(func, inst, dst, &mut code)?;
                        let funct3 = self.amo_funct3(func, inst, *size)?;
                        let re = get_gpr(expected).map(u32::from).ok_or_else(|| {
                            self.unsupported(
                                func,
                                inst,
                                "the expected value must be a general-purpose register",
                            )
                        })?;
                        let rdes = get_gpr(desired).map(u32::from).ok_or_else(|| {
                            self.unsupported(
                                func,
                                inst,
                                "the desired value must be a general-purpose register",
                            )
                        })?;
                        let rd = 6u32; // scratch t1

                        // LR.W/D rd (t1), (rs1)
                        let lr = 0x1000002Fu32 | (base_reg << 15) | (funct3 << 12) | (rd << 7);
                        code.extend_from_slice(&lr.to_le_bytes());
                        // BNE rd, re, +12 (skip store if loaded value rd != expected re)
                        let bne = Self::encode_branch(ConditionCode::NotEqual, rd, re, 12)
                            .ok_or_else(|| {
                                self.unsupported(
                                    func,
                                    inst,
                                    "the load-compare-store sequence needs a not-equal branch, \
                                     which every RV64 condition set provides",
                                )
                            })?;
                        code.extend_from_slice(&bne.to_le_bytes());
                        // SC.W/D t0(5), rdes, (rs1)
                        let sc = 0x1800002Fu32
                            | (rdes << 20)
                            | (base_reg << 15)
                            | (funct3 << 12)
                            | (5 << 7);
                        code.extend_from_slice(&sc.to_le_bytes());
                    }
                    unsupported => {
                        return Err(CodegenError::new(
                            self.target.triple_string(),
                            format!(
                                "unsupported machine instruction {:?} for RISC-V target",
                                unsupported
                            ),
                        )
                        .with_arch("riscv64")
                        .with_function(func.name.clone()));
                    }
                }
            }
        }

        // Epilogue if missing
        if code.len() < 16 || code[code.len() - 4..] != 0x00008067u32.to_le_bytes() {
            let mut rest_off = frame_size as i32 - 24;
            for &gpr in &used_callee_gprs {
                Self::emit_load_sp(&mut code, gpr, rest_off);
                rest_off -= 8;
            }
            for &fpr in &used_callee_fprs {
                Self::emit_load_sp(&mut code, fpr, rest_off);
                rest_off -= 8;
            }

            Self::emit_load_sp(&mut code, 1, ra_off);
            Self::emit_load_sp(&mut code, 8, s0_off);

            if (frame_size as i64) <= 2047 {
                let imm12 = (frame_size as u32) & 0xFFF;
                let addi_sp = 0x00010113u32 | (imm12 << 20);
                code.extend_from_slice(&addi_sp.to_le_bytes());
            } else {
                Self::emit_li64(&mut code, 6, frame_size as i64);
                let add_sp = 0x00010133u32 | (6 << 20) | (2 << 15) | (2 << 7);
                code.extend_from_slice(&add_sp.to_le_bytes());
            }
            code.extend_from_slice(&0x00008067u32.to_le_bytes()); // ret
        }

        // Resolve branch fixups
        for (offset, target_label, is_cond, cc, r1, r2) in branch_fixups {
            // A target that does not exist left a placeholder branch in the stream: an
            // unconditional one jumps to its own address, so the function never ends.
            let &target_offset = label_offsets.get(&target_label).ok_or_else(|| {
                CodegenError::new(
                    self.target.triple_string(),
                    format!(
                        "branch targets label '{target_label}', which this function does not \
                         define; the placeholder branch would have been patched to nothing"
                    ),
                )
                .with_arch("riscv64")
                .with_function(func.name.clone())
            })?;
            let disp = target_offset as i64 - offset as i64;
            let ins = if is_cond {
                if !(-4096..=4095).contains(&disp) {
                    return Err(CodegenError::new(
                        self.target.triple_string(),
                        format!(
                            "conditional branch displacement {disp} to '{target_label}' does not \
                             fit the B-type immediate (+-4096 bytes); masking it would branch to \
                             a different label"
                        ),
                    )
                    .with_arch("riscv64")
                    .with_function(func.name.clone()));
                }
                Self::encode_branch(cc, r1, r2, disp as i32).ok_or_else(|| {
                    CodegenError::new(
                        self.target.triple_string(),
                        format!(
                            "condition {cc:?} has no RISC-V branch form on the way to \
                             '{target_label}'"
                        ),
                    )
                    .with_arch("riscv64")
                    .with_function(func.name.clone())
                })?
            } else {
                if !(-1048576..=1048575).contains(&disp) {
                    return Err(CodegenError::new(
                        self.target.triple_string(),
                        format!(
                            "branch displacement {disp} to '{target_label}' does not fit the \
                             JAL 21-bit offset (+-1 MiB)"
                        ),
                    )
                    .with_arch("riscv64")
                    .with_function(func.name.clone()));
                }
                let imm = ((disp >> 1) as u32) & 0xFFFFF;
                let imm20 = (imm >> 19) & 1;
                let imm10_1 = imm & 0x3FF;
                let imm11 = (imm >> 10) & 1;
                let imm19_12 = (imm >> 11) & 0xFF;
                (imm20 << 31) | (imm10_1 << 21) | (imm11 << 20) | (imm19_12 << 12) | 0x6F // JAL x0, disp
            };
            code[offset..offset + 4].copy_from_slice(&ins.to_le_bytes());
        }

        Ok((code, relocs))
    }

    /// Resolve `ParallelMove` into a sequence of single moves before encoding,
    /// breaking cycles with a caller-saved temporary. The encoder has no
    /// parallel-move form of its own, so an unresolved one reaching it would be
    /// a silently dropped instruction.
    fn expand_parallel_moves(func: &mut MachineFunction) -> Result<(), CodegenError> {
        let resolver = ParallelMoveResolver::for_riscv64();
        for block in &mut func.blocks {
            let mut expanded: Vec<MachineInstruction> =
                Vec::with_capacity(block.instructions.len());
            for inst in block.instructions.drain(..) {
                match inst {
                    MachineInstruction::ParallelMove { moves } => {
                        expanded.extend(resolver.resolve_moves(&moves)?);
                    }
                    other => expanded.push(other),
                }
            }
            block.instructions = expanded;
        }
        Ok(())
    }
}

impl CodegenBackend for RiscVBackend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut lowered = module.clone();
        let reg_file = RiscVRegisterFile;
        let allocator = LinearScanAllocator::new(&reg_file);
        for func in &mut lowered.functions {
            allocator.allocate(func).map_err(|e| {
                CodegenError::new(self.target.triple_string(), e.to_string())
                    .with_function(func.name.clone())
            })?;
        }
        Ok(lowered)
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        let (code, _relocs) = self.generate_function_with_relocs(func)?;
        Ok(code)
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());
        let mut text_bytes = Vec::new();
        let mut text_relocs = Vec::new();

        for func in &module.functions {
            let func_offset = text_bytes.len() as u64;
            let (code, func_relocs) = self.generate_function_with_relocs(func)?;
            let size = code.len() as u64;

            for mut reloc in func_relocs {
                reloc.offset += func_offset;
                text_relocs.push(reloc);
            }

            text_bytes.extend_from_slice(&code);

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                func_offset,
                size,
            )
            .with_binding(SymbolBinding::Global)
            .with_visibility(SymbolVisibility::Default);
            obj.add_symbol(sym);

            if func.is_exported {
                obj.add_export(func.name.clone());
            }
        }

        let mut text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(16)
            .with_data(text_bytes);
        text_sec.relocations = text_relocs;
        obj.add_section(text_sec);

        super::emit_module_data_sections(&mut obj, module);

        Ok(obj)
    }
}

fn get_reg(op: &MachineOperand) -> Option<u8> {
    match op {
        MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
        _ => None,
    }
}

/// Extract a general-purpose register number (`x0..x31`).
///
/// `f0..f31` and the vector registers are numbered 32..63, so a 5-bit `rd`/`rs`
/// field reached by a float register names a different register instead of
/// failing. Every integer arm takes its operands through this filter.
fn get_gpr(op: &MachineOperand) -> Option<u8> {
    get_reg(op).filter(|r| *r < 32)
}

/// Extract a floating-point register number; `f0..f31` are numbered 32..63.
fn get_fpr(op: &MachineOperand) -> Option<u8> {
    get_reg(op).filter(|r| *r >= 32 && *r < 64)
}

/// Find a virtual register that reached the encoder.
///
/// Allocation happens in `lower_module`; a virtual operand at encode time means
/// that step was skipped, which is an API misuse rather than an instruction the
/// backend cannot encode.
fn first_unallocated_reg(func: &MachineFunction) -> Option<MachineRegister> {
    func.blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .flat_map(|inst| inst.defs().into_iter().chain(inst.uses()))
        .find(|reg| matches!(reg, MachineRegister::Virtual(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use adesh_object::TargetDescriptor;

    #[test]
    fn test_riscv64_return_only_function_encodes() {
        let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
        let mut backend = RiscVBackend::new(target);

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert!(!code.is_empty());
    }

    #[test]
    fn test_riscv64_arithmetic_fp_and_atomic_encodes() {
        let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
        let mut backend = RiscVBackend::new(target);

        let mut func = MachineFunction::new("rv_calc");
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(11))),
        });
        block.push(MachineInstruction::FAdd {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(33))),
            size: 8,
        });
        block.push(MachineInstruction::AtomicFetchAdd {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(11))),
            size: 8,
        });
        block.push(MachineInstruction::Barrier);
        block.push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert!(code.len() >= 24);
    }

    #[test]
    fn test_riscv64_call_relocation_emission() {
        let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
        let mut backend = RiscVBackend::new(target);

        let mut module = NativeModule::new("test_calls");
        let mut func = MachineFunction::new("caller");
        func.is_exported = true;
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("callee_fn".to_string()),
            num_args: 0,
        });
        block.push(MachineInstruction::Return);
        module.add_function(func);

        let obj = backend.emit_object(&module).expect("emits ADOB");
        assert_eq!(obj.sections.len(), 1);
        let sec = &obj.sections[0];
        assert_eq!(sec.relocations.len(), 1);
        assert_eq!(sec.relocations[0].symbol_name, "callee_fn");
        assert_eq!(sec.relocations[0].kind, RelocationKind::RiscV_Call);
    }
}
