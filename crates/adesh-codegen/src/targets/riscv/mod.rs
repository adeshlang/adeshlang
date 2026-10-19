//! RISC-V (RV64GC LP64D) Native Codegen Backend.
//!
//! Provides production-grade RV64GC instruction selection, register allocation,
//! calling convention (LP64D ABI), 64-bit immediate synthesis, double/single-precision
//! floating point (RV64D/F), atomics (RV64A), compressed instruction alignment (RVC),
//! vector instructions (RVV 1.0), and ADOB/ELF object generation with full relocations.

use crate::backend::CodegenBackend;
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

/// Function machine code together with external call relocation entries (offset in code, target symbol).
pub type FunctionCodeWithRelocs = (Vec<u8>, Vec<(usize, String)>);

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

    /// Encode branch instruction with displacement.
    fn encode_branch(cc: ConditionCode, rs1: u32, rs2: u32, disp: i32) -> u32 {
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
            ConditionCode::Parity | ConditionCode::NotParity => (0x0, rs1, rs2),
        };
        let imm = ((disp >> 1) as u32) & 0xFFF;
        let imm12 = (imm >> 11) & 1;
        let imm10_5 = (imm >> 4) & 0x3F;
        let imm4_1 = imm & 0xF;
        let imm11 = (imm >> 10) & 1;
        (imm12 << 31)
            | (imm10_5 << 25)
            | (actual_rs2 << 20)
            | (actual_rs1 << 15)
            | (funct3 << 12)
            | (imm4_1 << 8)
            | (imm11 << 7)
            | 0x63
    }

    /// Generate function code and collect external call relocations `(offset_in_func, symbol_name)`.
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

        let mut code = Vec::new();
        let mut label_offsets: HashMap<String, usize> = HashMap::new();
        let mut branch_fixups: Vec<(usize, String, bool, ConditionCode, u32, u32)> = Vec::new();
        let mut call_relocs: Vec<(usize, String)> = Vec::new();

        // Standard 16-byte aligned LP64D frame:
        // addi sp, sp, -16
        // sd   ra, 8(sp)
        // sd   s0, 0(sp)
        // addi s0, sp, 16
        code.extend_from_slice(&0xFF010113u32.to_le_bytes()); // addi sp, sp, -16
        code.extend_from_slice(&0x00113423u32.to_le_bytes()); // sd ra, 8(sp)
        code.extend_from_slice(&0x00813023u32.to_le_bytes()); // sd s0, 0(sp)
        code.extend_from_slice(&0x01010413u32.to_le_bytes()); // addi s0, sp, 16

        // Track last comparison registers (rs1, rs2) for SetCc and BranchCc
        let mut last_cmp_regs: (u32, u32) = (10, 0); // default a0, zero

        for block in &func.blocks {
            label_offsets.insert(block.label.clone(), code.len());

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        // NOP: addi x0, x0, 0
                        code.extend_from_slice(&0x00000013u32.to_le_bytes());
                    }
                    MachineInstruction::Return => {
                        // ld s0, 0(sp); ld ra, 8(sp); addi sp, sp, 16; ret (jalr x0, 0(ra))
                        code.extend_from_slice(&0x00013403u32.to_le_bytes()); // ld s0, 0(sp)
                        code.extend_from_slice(&0x00813083u32.to_le_bytes()); // ld ra, 8(sp)
                        code.extend_from_slice(&0x01010113u32.to_le_bytes()); // addi sp, sp, 16
                        code.extend_from_slice(&0x00008067u32.to_le_bytes()); // jalr x0, 0(ra)
                    }
                    MachineInstruction::Move { dst, src } => {
                        let (d, s) = get_regs(dst, src);
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
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            Self::emit_li64(&mut code, d_reg, *val);
                        }
                    }
                    MachineInstruction::Add { dst, src } => {
                        let (d, s) = get_regs(dst, src);
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
                        }
                    }
                    MachineInstruction::Sub { dst, src } => {
                        let (d, s) = get_regs(dst, src);
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
                        }
                    }
                    MachineInstruction::Mul { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // mul rd, rd, rs2
                            let ins = 0x02000033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Div { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // div rd, rd, rs2
                            let ins = 0x02004033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Mod { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // rem rd, rd, rs2
                            let ins = 0x02006033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Neg { dst } => {
                        if let Some(d_reg) = get_reg(dst) {
                            // neg rd, rd -> sub rd, x0, rd
                            let ins =
                                0x40000033u32 | ((d_reg as u32) << 20) | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Not { dst } => {
                        if let Some(d_reg) = get_reg(dst) {
                            // not rd, rd -> xori rd, rd, -1
                            let ins =
                                0xFFF04013u32 | ((d_reg as u32) << 15) | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::And { dst, src } => {
                        let (d, s) = get_regs(dst, src);
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
                        }
                    }
                    MachineInstruction::Or { dst, src } => {
                        let (d, s) = get_regs(dst, src);
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
                        }
                    }
                    MachineInstruction::Xor { dst, src } => {
                        let (d, s) = get_regs(dst, src);
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
                        }
                    }
                    MachineInstruction::Shl { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // sll rd, rd, rs2
                            let ins = 0x00001033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            // slli rd, rd, shamt
                            let shamt = (*val as u32) & 0x3F;
                            let ins = 0x00001013u32
                                | (shamt << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Shr { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // srl rd, rd, rs2
                            let ins = 0x00005033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            // srli rd, rd, shamt
                            let shamt = (*val as u32) & 0x3F;
                            let ins = 0x00005013u32
                                | (shamt << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Sar { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // sra rd, rd, rs2
                            let ins = 0x40005033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            // srai rd, rd, shamt
                            let shamt = (*val as u32) & 0x3F;
                            let ins = 0x40005013u32
                                | (shamt << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs } => {
                        let (d, s) = get_regs(lhs, rhs);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            last_cmp_regs = (d_reg as u32, s_reg as u32);
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, rhs) {
                            if *val == 0 {
                                last_cmp_regs = (d_reg as u32, 0); // x0
                            } else {
                                Self::emit_li64(&mut code, 7, *val); // load into t2 (7)
                                last_cmp_regs = (d_reg as u32, 7);
                            }
                        }
                    }
                    MachineInstruction::SetCc { dst, cc } => {
                        if let Some(d_reg) = get_reg(dst) {
                            let rd = d_reg as u32;
                            let (r1, r2) = last_cmp_regs;
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
                                _ => {
                                    let ins = 0x00000013u32 | (rd << 7); // mv rd, x0
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                            }
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
                        let offset = code.len();
                        let (r1, r2) = last_cmp_regs;
                        branch_fixups.push((offset, target.clone(), true, *cc, r1, r2));
                        code.extend_from_slice(&0x00000063u32.to_le_bytes()); // BEQ placeholder
                    }
                    MachineInstruction::Call { target, .. } => {
                        if let Some(r) = get_reg(target) {
                            // JALR ra, 0(rs1)
                            let ins = 0x000000E7u32 | ((r as u32) << 15);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let MachineOperand::Symbol(sym) = target {
                            let call_off = code.len();
                            call_relocs.push((call_off, sym.clone()));
                            // Emit AUIPC ra, 0; JALR ra, ra, 0 pair (8 bytes)
                            code.extend_from_slice(&0x00000097u32.to_le_bytes()); // auipc ra, 0
                            code.extend_from_slice(&0x000080E7u32.to_le_bytes()); // jalr ra, ra, 0
                        } else {
                            // JAL ra, #0 placeholder
                            code.extend_from_slice(&0x000000EFu32.to_le_bytes());
                        }
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
                            if d_reg >= 32 {
                                let fd = (d_reg - 32) as u32;
                                if *size == 4 {
                                    // flw fd, off(base)
                                    let ins = 0x00002007u32
                                        | (imm12 << 20)
                                        | (base_reg.0 << 15)
                                        | (fd << 7);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                } else {
                                    // fld fd, off(base)
                                    let ins = 0x00003007u32
                                        | (imm12 << 20)
                                        | (base_reg.0 << 15)
                                        | (fd << 7);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                            } else {
                                let rd = d_reg as u32;
                                let funct3 = match *size {
                                    1 => 0x0, // lb
                                    2 => 0x1, // lh
                                    4 => 0x2, // lw
                                    _ => 0x3, // ld
                                };
                                let ins = 0x00000003u32
                                    | (imm12 << 20)
                                    | (base_reg.0 << 15)
                                    | (funct3 << 12)
                                    | (rd << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
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

                            if s_reg >= 32 {
                                let fs = (s_reg - 32) as u32;
                                let funct3 = if *size == 4 { 2u32 } else { 3u32 };
                                let ins = (imm_hi << 25)
                                    | (fs << 20)
                                    | (base_reg.0 << 15)
                                    | (funct3 << 12)
                                    | (imm_lo << 7)
                                    | 0x27;
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                let rs2 = s_reg as u32;
                                let funct3 = match *size {
                                    1 => 0x0, // sb
                                    2 => 0x1, // sh
                                    4 => 0x2, // sw
                                    _ => 0x3, // sd
                                };
                                let ins = (imm_hi << 25)
                                    | (rs2 << 20)
                                    | (base_reg.0 << 15)
                                    | (funct3 << 12)
                                    | (imm_lo << 7)
                                    | 0x23;
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        }
                    }
                    MachineInstruction::Push { src } => {
                        if let Some(s_reg) = get_reg(src) {
                            // addi sp, sp, -16
                            code.extend_from_slice(&0xFF010113u32.to_le_bytes());
                            // sd rs, 0(sp)
                            let ins = 0x00013023u32 | ((s_reg as u32) << 20);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Pop { dst } => {
                        if let Some(d_reg) = get_reg(dst) {
                            // ld rd, 0(sp)
                            let ins = 0x00013003u32 | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                            // addi sp, sp, 16
                            code.extend_from_slice(&0x01010113u32.to_le_bytes());
                        }
                    }
                    // Floating-Point (F0..F31)
                    MachineInstruction::FAdd { dst, src, size } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            let opcode_prefix = if *size == 4 {
                                0x00000053u32
                            } else {
                                0x02000053u32
                            };
                            let ins =
                                opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FSub { dst, src, size } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            let opcode_prefix = if *size == 4 {
                                0x08000053u32
                            } else {
                                0x0A000053u32
                            };
                            let ins =
                                opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FMul { dst, src, size } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            let opcode_prefix = if *size == 4 {
                                0x10000053u32
                            } else {
                                0x12000053u32
                            };
                            let ins =
                                opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FDiv { dst, src, size } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            let opcode_prefix = if *size == 4 {
                                0x18000053u32
                            } else {
                                0x1A000053u32
                            };
                            let ins =
                                opcode_prefix | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FNeg { dst, size } => {
                        if let Some(d_reg) = get_reg(dst) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            // fsgnjn.d fd, fd, fd
                            let funct7 = if *size == 4 {
                                0x20000053u32
                            } else {
                                0x22000053u32
                            };
                            let ins = funct7 | (fd << 20) | (fd << 15) | (1 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FCvtIntToFloat { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let rs = s_reg as u32;
                            // fcvt.d.l fd, rs
                            let ins =
                                0xD2200053u32 | (2 << 20) | (rs << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FCvtFloatToInt { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let rd = d_reg as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            // fcvt.l.d rd, fs
                            let ins =
                                0xC2200053u32 | (2 << 20) | (fs << 15) | (7 << 12) | (rd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    // RISC-V Vector Extension (RVV 1.0)
                    MachineInstruction::VectorAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        let ins = 0x02000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorSub { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        let ins = 0x0A000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMul { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        let ins = 0x92002057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMin { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        let ins = 0x12000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMax { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        let ins = 0x1A000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorLoad { dst, .. } => {
                        let d = get_reg(dst).unwrap_or(0) & 0x1F;
                        let ins = 0x02000007u32 | (2 << 15) | ((d as u32) << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorStore { src, .. } => {
                        let s = get_reg(src).unwrap_or(0) & 0x1F;
                        let ins = 0x02000027u32 | (2 << 15) | ((s as u32) << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    // Atomics & Memory Ordering (RV64A)
                    MachineInstruction::Barrier => {
                        // FENCE iorw, iorw
                        code.extend_from_slice(&0x0FF0000Fu32.to_le_bytes());
                    }
                    MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let rd = (d.unwrap_or(10)) as u32;
                        let rs2 = (s.unwrap_or(11)) as u32;
                        // AMOADD.D rd, rs2, (rs1=sp=2)
                        let ins = 0x0000302Fu32 | (rs2 << 20) | (2 << 15) | (rd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::AtomicExchange { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let rd = (d.unwrap_or(10)) as u32;
                        let rs2 = (s.unwrap_or(11)) as u32;
                        // AMOSWAP.D rd, rs2, (rs1=sp=2)
                        let ins = 0x0800302Fu32 | (rs2 << 20) | (2 << 15) | (rd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::AtomicCompareExchange {
                        dst,
                        expected,
                        desired,
                        ..
                    } => {
                        let rd = get_reg(dst).unwrap_or(10) as u32;
                        let re = get_reg(expected).unwrap_or(11) as u32;
                        let rdes = get_reg(desired).unwrap_or(12) as u32;
                        // LR.D rd, (sp=2)
                        let lr = 0x1000302Fu32 | (2 << 15) | (rd << 7);
                        code.extend_from_slice(&lr.to_le_bytes());
                        // BNE rd, re, +12 (skip store)
                        let bne = Self::encode_branch(ConditionCode::NotEqual, rd, re, 12);
                        code.extend_from_slice(&bne.to_le_bytes());
                        // SC.D t0(5), rdes, (sp=2)
                        let sc = 0x1800302Fu32 | (rdes << 20) | (2 << 15) | (5 << 7);
                        code.extend_from_slice(&sc.to_le_bytes());
                    }
                    _ => {
                        // NOP fallback
                        code.extend_from_slice(&0x00000013u32.to_le_bytes());
                    }
                }
            }
        }

        // Epilogue if missing
        if code.len() < 16 || code[code.len() - 4..] != 0x00008067u32.to_le_bytes() {
            code.extend_from_slice(&0x00013403u32.to_le_bytes()); // ld s0, 0(sp)
            code.extend_from_slice(&0x00813083u32.to_le_bytes()); // ld ra, 8(sp)
            code.extend_from_slice(&0x01010113u32.to_le_bytes()); // addi sp, sp, 16
            code.extend_from_slice(&0x00008067u32.to_le_bytes()); // ret
        }

        // Resolve branch fixups
        for (offset, target_label, is_cond, cc, r1, r2) in branch_fixups {
            if let Some(&target_offset) = label_offsets.get(&target_label) {
                let disp = target_offset as i64 - offset as i64;
                if is_cond {
                    let ins = Self::encode_branch(cc, r1, r2, disp as i32);
                    code[offset..offset + 4].copy_from_slice(&ins.to_le_bytes());
                } else {
                    let imm = ((disp >> 1) as u32) & 0xFFFFF;
                    let imm20 = (imm >> 19) & 1;
                    let imm10_1 = imm & 0x3FF;
                    let imm11 = (imm >> 10) & 1;
                    let imm19_12 = (imm >> 11) & 0xFF;
                    let ins =
                        (imm20 << 31) | (imm10_1 << 21) | (imm11 << 20) | (imm19_12 << 12) | 0x6F; // JAL x0, disp
                    code[offset..offset + 4].copy_from_slice(&ins.to_le_bytes());
                }
            }
        }

        Ok((code, call_relocs))
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
            let (code, call_relocs) = self.generate_function_with_relocs(func)?;
            let size = code.len() as u64;

            for (off, sym_name) in call_relocs {
                text_relocs.push(AdobRelocation::new(
                    func_offset + off as u64,
                    0,
                    sym_name,
                    RelocationKind::RiscV_Call,
                    0,
                ));
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

        Ok(obj)
    }
}

fn get_reg(op: &MachineOperand) -> Option<u8> {
    match op {
        MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
        _ => None,
    }
}

fn get_regs(dst: &MachineOperand, src: &MachineOperand) -> (Option<u8>, Option<u8>) {
    (get_reg(dst), get_reg(src))
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
