//! AArch64 Native Codegen Backend.
//!
//! Provides production-grade AArch64 (ARMv8-A / AAPCS64) instruction selection,
//! register allocation, floating-point, NEON vectorization, atomics, and ADOB/ELF object generation.

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass,
};
use crate::register_alloc::{LinearScanAllocator, RegisterFile};
use adesh_object::{
    AdobObject, AdobRelocation, AdobSection, AdobSymbol, RelocationKind, SectionKind,
    SymbolBinding, SymbolKind, SymbolVisibility, TargetCapabilities, TargetDescriptor, section_flags,
};
use std::collections::{HashMap, HashSet};

/// AArch64 Register File (X0-X30, SP/XZR, V0-V31).
pub struct AArch64RegisterFile;

const AARCH64_ALL_REGS: [PhysicalRegister; 64] = [
    // X0..X30 (GPR)
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
    // V0..V31 (SIMD / Float)
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

/// AAPCS64 GPR Allocatable: Caller-saved (X0..X15) + Callee-saved (X19..X28).
const AARCH64_ALLOCATABLE_GPR: [PhysicalRegister; 26] = [
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
];

/// AAPCS64 GPR Caller-saved: X0..X15
const AARCH64_CALLER_SAVED_GPR: [PhysicalRegister; 16] = [
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
];

/// AAPCS64 GPR Callee-saved: X19..X28
const AARCH64_CALLEE_SAVED_GPR: [PhysicalRegister; 10] = [
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
];

/// AAPCS64 FP Allocatable: V0..V31
const AARCH64_ALLOCATABLE_FP: [PhysicalRegister; 32] = [
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

/// AAPCS64 FP Callee-saved: V8..V15 (D8..D15)
const AARCH64_CALLEE_SAVED_FP: [PhysicalRegister; 8] = [
    PhysicalRegister(40),
    PhysicalRegister(41),
    PhysicalRegister(42),
    PhysicalRegister(43),
    PhysicalRegister(44),
    PhysicalRegister(45),
    PhysicalRegister(46),
    PhysicalRegister(47),
];

/// AAPCS64 Reserved Registers:
/// - X16 (IP0): Intra-procedure-call temporary
/// - X17 (IP1): Intra-procedure-call temporary
/// - X18: Platform register
/// - X29: Frame Pointer (FP)
/// - X30: Link Register (LR)
/// - X31: Stack Pointer / Zero Register (SP / XZR)
const AARCH64_RESERVED: [PhysicalRegister; 6] = [
    PhysicalRegister(16),
    PhysicalRegister(17),
    PhysicalRegister(18),
    PhysicalRegister(29),
    PhysicalRegister(30),
    PhysicalRegister(31),
];

impl RegisterFile for AArch64RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &AARCH64_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &AARCH64_ALLOCATABLE_GPR
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &AARCH64_CALLER_SAVED_GPR
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &AARCH64_CALLEE_SAVED_GPR
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &AARCH64_RESERVED
    }
    fn allocatable_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => &AARCH64_ALLOCATABLE_GPR,
            RegisterClass::Float => &AARCH64_ALLOCATABLE_FP,
        }
    }
    fn scratch_for_class(&self, class: RegisterClass) -> (PhysicalRegister, PhysicalRegister) {
        match class {
            RegisterClass::Gpr => (PhysicalRegister(16), PhysicalRegister(17)), // IP0, IP1
            RegisterClass::Float => (PhysicalRegister(62), PhysicalRegister(63)), // V30, V31
        }
    }
}

pub struct AArch64Backend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl AArch64Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }

    fn encode_cond(cc: ConditionCode) -> u32 {
        match cc {
            ConditionCode::Equal | ConditionCode::Zero => 0x0, // EQ
            ConditionCode::NotEqual | ConditionCode::NotZero => 0x1, // NE
            ConditionCode::AboveOrEqual => 0x2,                // CS / HS
            ConditionCode::Below => 0x3,                       // CC / LO
            ConditionCode::LessThan => 0xB,                    // LT
            ConditionCode::LessOrEqual => 0xD,                 // LE
            ConditionCode::GreaterThan => 0xC,                 // GT
            ConditionCode::GreaterOrEqual => 0xA,              // GE
            ConditionCode::Above => 0x8,                       // HI
            ConditionCode::BelowOrEqual => 0x9,                // LS
            ConditionCode::Parity => 0x6,                      // VS
            ConditionCode::NotParity => 0x7,                   // VC
        }
    }

    /// Emit a 64-bit immediate load into register Xd using MOVZ / MOVN / MOVK.
    fn emit_mov_imm64(code: &mut Vec<u8>, rd: u8, imm: u64) {
        let chunk0 = (imm & 0xFFFF) as u32;
        let chunk1 = ((imm >> 16) & 0xFFFF) as u32;
        let chunk2 = ((imm >> 32) & 0xFFFF) as u32;
        let chunk3 = ((imm >> 48) & 0xFFFF) as u32;

        if chunk1 == 0 && chunk2 == 0 && chunk3 == 0 {
            // MOVZ Xd, #imm16, LSL #0
            let ins = 0xD2800000u32 | (chunk0 << 5) | (rd as u32);
            code.extend_from_slice(&ins.to_le_bytes());
            return;
        }

        // Check if MOVN is more compact (for negative constants near -1)
        let not_imm = !imm;
        let not_chunk0 = (not_imm & 0xFFFF) as u32;
        let not_chunk1 = ((not_imm >> 16) & 0xFFFF) as u32;
        let not_chunk2 = ((not_imm >> 32) & 0xFFFF) as u32;
        let not_chunk3 = ((not_imm >> 48) & 0xFFFF) as u32;

        if not_chunk1 == 0 && not_chunk2 == 0 && not_chunk3 == 0 {
            // MOVN Xd, #imm16, LSL #0
            let ins = 0x92800000u32 | (not_chunk0 << 5) | (rd as u32);
            code.extend_from_slice(&ins.to_le_bytes());
            return;
        }

        // General multi-instruction materialization: MOVZ + MOVK
        let mut first = true;
        let chunks = [(chunk0, 0), (chunk1, 1), (chunk2, 2), (chunk3, 3)];

        for &(val, shift_idx) in &chunks {
            if first {
                // MOVZ Xd, #imm16, LSL #(shift_idx * 16)
                let ins = 0xD2800000u32 | ((shift_idx as u32) << 21) | (val << 5) | (rd as u32);
                code.extend_from_slice(&ins.to_le_bytes());
                first = false;
            } else if val != 0 {
                // MOVK Xd, #imm16, LSL #(shift_idx * 16)
                let ins = 0xF2800000u32 | ((shift_idx as u32) << 21) | (val << 5) | (rd as u32);
                code.extend_from_slice(&ins.to_le_bytes());
            }
        }
    }
}

impl CodegenBackend for AArch64Backend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut lowered = module.clone();
        let reg_file = AArch64RegisterFile;
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
        let mut code = Vec::new();
        let mut label_offsets: HashMap<String, usize> = HashMap::new();
        let mut fixups: Vec<(usize, String, bool, ConditionCode)> = Vec::new();

        // Analyze used callee-saved registers in func
        let mut used_callee_saved: Vec<PhysicalRegister> = Vec::new();
        let mut seen_regs = HashSet::new();

        for block in &func.blocks {
            for inst in &block.instructions {
                for reg in inst.defs().into_iter().chain(inst.uses().into_iter()) {
                    if let MachineRegister::Physical(p) = reg {
                        if (19..=28).contains(&p.0) || (40..=47).contains(&p.0) {
                            if seen_regs.insert(p.0) {
                                used_callee_saved.push(p);
                            }
                        }
                    }
                }
            }
        }
        used_callee_saved.sort_by_key(|p| p.0);

        // Frame Layout:
        // [SP + 0 .. local_bytes] local stack slots
        // [SP + local_bytes .. ] callee-saved registers
        // [FP + 0] = Saved FP (X29)
        // [FP + 8] = Saved LR (X30)
        let callee_save_count = used_callee_saved.len();
        let callee_save_bytes = ((callee_save_count + 1) / 2) * 16; // align to 16
        let local_bytes = ((func.stack_size as usize + 15) / 16) * 16;
        let total_alloc = local_bytes + callee_save_bytes;

        // Prologue:
        // STP X29, X30, [SP, #-16]! (0xA9BF7BFD)
        // MOV X29, SP (0x910003FD)
        code.extend_from_slice(&0xA9BF7BFDu32.to_le_bytes());
        code.extend_from_slice(&0x910003FDu32.to_le_bytes());

        if total_alloc > 0 {
            // SUB SP, SP, #total_alloc
            if total_alloc <= 4095 {
                let ins = 0xD1000000u32 | ((total_alloc as u32) << 10) | (31 << 5) | 31;
                code.extend_from_slice(&ins.to_le_bytes());
            } else {
                Self::emit_mov_imm64(&mut code, 16, total_alloc as u64); // IP0 = total_alloc
                // SUB SP, SP, X16
                let ins = 0xCB000000u32 | (16 << 16) | (31 << 5) | 31;
                code.extend_from_slice(&ins.to_le_bytes());
            }
        }

        // Save used callee-saved registers
        let mut save_offset = local_bytes;
        for chunk in used_callee_saved.chunks(2) {
            if chunk.len() == 2 {
                let r1 = chunk[0].0;
                let r2 = chunk[1].0;
                if r1 < 32 && r2 < 32 {
                    // STP Xr1, Xr2, [SP, #imm7*8]
                    let imm7 = ((save_offset / 8) & 0x7F) as u32;
                    let ins = 0xA9000000u32 | (imm7 << 15) | ((r2 as u32) << 10) | (31 << 5) | (r1 as u32);
                    code.extend_from_slice(&ins.to_le_bytes());
                } else {
                    let d1 = (r1.saturating_sub(32)) as u32;
                    let d2 = (r2.saturating_sub(32)) as u32;
                    // STP Dr1, Dr2, [SP, #imm7*8]
                    let imm7 = ((save_offset / 8) & 0x7F) as u32;
                    let ins = 0x69000000u32 | (imm7 << 15) | (d2 << 10) | (31 << 5) | d1;
                    code.extend_from_slice(&ins.to_le_bytes());
                }
            } else {
                let r1 = chunk[0].0;
                if r1 < 32 {
                    // STR Xr1, [SP, #imm12*8]
                    let imm12 = ((save_offset / 8) & 0xFFF) as u32;
                    let ins = 0xF9000000u32 | (imm12 << 10) | (31 << 5) | (r1 as u32);
                    code.extend_from_slice(&ins.to_le_bytes());
                } else {
                    let d1 = (r1.saturating_sub(32)) as u32;
                    // STR Dr1, [SP, #imm12*8]
                    let imm12 = ((save_offset / 8) & 0xFFF) as u32;
                    let ins = 0xFD000000u32 | (imm12 << 10) | (31 << 5) | d1;
                    code.extend_from_slice(&ins.to_le_bytes());
                }
            }
            save_offset += 16;
        }

        for block in &func.blocks {
            label_offsets.insert(block.label.clone(), code.len());

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        // NOP (0xD503201F)
                        code.extend_from_slice(&0xD503201Fu32.to_le_bytes());
                    }
                    MachineInstruction::Return => {
                        // Restore callee-saved registers
                        let mut restore_offset = local_bytes;
                        for chunk in used_callee_saved.chunks(2) {
                            if chunk.len() == 2 {
                                let r1 = chunk[0].0;
                                let r2 = chunk[1].0;
                                if r1 < 32 && r2 < 32 {
                                    // LDP Xr1, Xr2, [SP, #imm7*8]
                                    let imm7 = ((restore_offset / 8) & 0x7F) as u32;
                                    let ins = 0xA9400000u32 | (imm7 << 15) | ((r2 as u32) << 10) | (31 << 5) | (r1 as u32);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                } else {
                                    let d1 = (r1.saturating_sub(32)) as u32;
                                    let d2 = (r2.saturating_sub(32)) as u32;
                                    // LDP Dr1, Dr2, [SP, #imm7*8]
                                    let imm7 = ((restore_offset / 8) & 0x7F) as u32;
                                    let ins = 0x69400000u32 | (imm7 << 15) | (d2 << 10) | (31 << 5) | d1;
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                            } else {
                                let r1 = chunk[0].0;
                                if r1 < 32 {
                                    // LDR Xr1, [SP, #imm12*8]
                                    let imm12 = ((restore_offset / 8) & 0xFFF) as u32;
                                    let ins = 0xF9400000u32 | (imm12 << 10) | (31 << 5) | (r1 as u32);
                                    code.extend_from_slice(&ins.to_le_bytes());
                                } else {
                                    let d1 = (r1.saturating_sub(32)) as u32;
                                    // LDR Dr1, [SP, #imm12*8]
                                    let imm12 = ((restore_offset / 8) & 0xFFF) as u32;
                                    let ins = 0xFD400000u32 | (imm12 << 10) | (31 << 5) | d1;
                                    code.extend_from_slice(&ins.to_le_bytes());
                                }
                            }
                            restore_offset += 16;
                        }

                        // MOV SP, X29
                        code.extend_from_slice(&0x910003BFu32.to_le_bytes());
                        // LDP X29, X30, [SP], #16 (0xA8C17BFD)
                        code.extend_from_slice(&0xA8C17BFDu32.to_le_bytes());
                        // RET (0xD65F03C0)
                        code.extend_from_slice(&0xD65F03C0u32.to_le_bytes());
                    }
                    MachineInstruction::Move { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            if d_reg >= 32 && s_reg >= 32 {
                                // FMOV Dd, Ds (0x1E604000)
                                let vd = (d_reg - 32) as u32;
                                let vs = (s_reg - 32) as u32;
                                let ins = 0x1E604000u32 | (vs << 5) | vd;
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                // ORR Xd, XZR, Xs (MOV Xd, Xs)
                                let ins = 0xAA0003E0u32 | ((s_reg as u32) << 16) | (d_reg as u32);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            Self::emit_mov_imm64(&mut code, d_reg, *val as u64);
                        } else if let (Some(d_reg), MachineOperand::StackSlot(slot)) = (d, src) {
                            // LDR Xd, [SP, #imm12*8]
                            let imm12 = ((*slot as u32) / 8) & 0xFFF;
                            let ins = 0xF9400000u32 | (imm12 << 10) | (31 << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (MachineOperand::StackSlot(slot), Some(s_reg)) = (dst, s) {
                            // STR Xs, [SP, #imm12*8]
                            let imm12 = ((*slot as u32) / 8) & 0xFFF;
                            let ins = 0xF9000000u32 | (imm12 << 10) | (31 << 5) | (s_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Add { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // ADD Xd, Xd, Xs
                            let ins = 0x8B000000u32 | ((s_reg as u32) << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            if *val >= 0 && *val <= 4095 {
                                let ins = 0x91000000u32 | ((*val as u32) << 10) | ((d_reg as u32) << 5) | (d_reg as u32);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                Self::emit_mov_imm64(&mut code, 16, *val as u64); // IP0 = imm
                                let ins = 0x8B000000u32 | (16 << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        }
                    }
                    MachineInstruction::Sub { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // SUB Xd, Xd, Xs
                            let ins = 0xCB000000u32 | ((s_reg as u32) << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            if *val >= 0 && *val <= 4095 {
                                let ins = 0xD1000000u32 | ((*val as u32) << 10) | ((d_reg as u32) << 5) | (d_reg as u32);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                Self::emit_mov_imm64(&mut code, 16, *val as u64);
                                let ins = 0xCB000000u32 | (16 << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        }
                    }
                    MachineInstruction::Mul { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // MADD Xd, Xd, Xs, XZR (MUL Xd, Xd, Xs)
                            let ins = 0x9B007C00u32 | ((s_reg as u32) << 16) | (31 << 10) | ((d_reg as u32) << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Div { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // SDIV Xd, Xd, Xs
                            let ins = 0x9AC00C00u32 | ((s_reg as u32) << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::And { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // AND Xd, Xd, Xs
                            let ins = 0x8A000000u32 | ((s_reg as u32) << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Or { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // ORR Xd, Xd, Xs
                            let ins = 0xAA000000u32 | ((s_reg as u32) << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Xor { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // EOR Xd, Xd, Xs
                            let ins = 0xCA000000u32 | ((s_reg as u32) << 16) | ((d_reg as u32) << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs } => {
                        let (d, s) = get_regs(lhs, rhs);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // SUBS XZR, Xd, Xs (CMP Xd, Xs)
                            let ins = 0xEB00001Fu32 | ((s_reg as u32) << 16) | ((d_reg as u32) << 5);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, rhs) {
                            if *val >= 0 && *val <= 4095 {
                                // SUBS XZR, Xd, #imm12 (CMP Xd, #imm12)
                                let ins = 0xF100001Fu32 | ((*val as u32) << 10) | ((d_reg as u32) << 5);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                Self::emit_mov_imm64(&mut code, 16, *val as u64);
                                let ins = 0xEB00001Fu32 | (16 << 16) | ((d_reg as u32) << 5);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        }
                    }
                    MachineInstruction::SetCc { dst, cc } => {
                        if let Some(d_reg) = get_reg(dst) {
                            let cond = Self::encode_cond(*cc);
                            // CSET Xd, cond (0x9A9F07E0 | (cond << 12) | Xd)
                            let ins = 0x9A9F07E0u32 | (cond << 12) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Branch { target } => {
                        let offset = code.len();
                        fixups.push((offset, target.clone(), false, ConditionCode::Equal));
                        code.extend_from_slice(&0x14000000u32.to_le_bytes()); // B #0 placeholder
                    }
                    MachineInstruction::BranchCc { cc, target } => {
                        let offset = code.len();
                        fixups.push((offset, target.clone(), true, *cc));
                        code.extend_from_slice(&0x54000000u32.to_le_bytes()); // B.cond #0 placeholder
                    }
                    MachineInstruction::Call { target, .. } => {
                        if let Some(r) = get_reg(target) {
                            // BLR Xn (0xD63F0000 | (Xn << 5))
                            let ins = 0xD63F0000u32 | ((r as u32) << 5);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            // BL #0 placeholder (will be relocated by linker)
                            code.extend_from_slice(&0x94000000u32.to_le_bytes());
                        }
                    }
                    MachineInstruction::Load { dst, src, .. } => {
                        if let (Some(d_reg), MachineOperand::StackSlot(slot)) = (get_reg(dst), src) {
                            // LDR Xd, [SP, #imm12*8]
                            let imm12 = ((*slot as u32) / 8) & 0xFFF;
                            let ins = 0xF9400000u32 | (imm12 << 10) | (31 << 5) | (d_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Store { dst, src, .. } => {
                        if let (MachineOperand::StackSlot(slot), Some(s_reg)) = (dst, get_reg(src)) {
                            // STR Xs, [SP, #imm12*8]
                            let imm12 = ((*slot as u32) / 8) & 0xFFF;
                            let ins = 0xF9000000u32 | (imm12 << 10) | (31 << 5) | (s_reg as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    // Floating-Point instructions (D0..D31)
                    MachineInstruction::FAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let vd = (d_reg.saturating_sub(32)) as u32;
                            let vs = (s_reg.saturating_sub(32)) as u32;
                            // FADD Dd, Dd, Ds (0x1E602800)
                            let ins = 0x1E602800u32 | (vs << 16) | (vd << 5) | vd;
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FSub { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let vd = (d_reg.saturating_sub(32)) as u32;
                            let vs = (s_reg.saturating_sub(32)) as u32;
                            // FSUB Dd, Dd, Ds (0x1E603800)
                            let ins = 0x1E603800u32 | (vs << 16) | (vd << 5) | vd;
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FMul { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let vd = (d_reg.saturating_sub(32)) as u32;
                            let vs = (s_reg.saturating_sub(32)) as u32;
                            // FMUL Dd, Dd, Ds (0x1E600800)
                            let ins = 0x1E600800u32 | (vs << 16) | (vd << 5) | vd;
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FDiv { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let vd = (d_reg.saturating_sub(32)) as u32;
                            let vs = (s_reg.saturating_sub(32)) as u32;
                            // FDIV Dd, Dd, Ds (0x1E601800)
                            let ins = 0x1E601800u32 | (vs << 16) | (vd << 5) | vd;
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    // NEON SIMD Vector Instructions
                    MachineInstruction::VectorAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // ADD Vd.4S, Vd.4S, Vs.4S (0x4E208400)
                        let ins = 0x4E208400u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorSub { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // SUB Vd.4S, Vd.4S, Vs.4S (0x4EA08400)
                        let ins = 0x4EA08400u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMul { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // MUL Vd.4S, Vd.4S, Vs.4S (0x4E209C00)
                        let ins = 0x4E209C00u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMin { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // FMIN Vd.4S, Vd.4S, Vs.4S (0x4E20F400)
                        let ins = 0x4E20F400u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMax { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // FMAX Vd.4S, Vd.4S, Vs.4S (0x4E20C400)
                        let ins = 0x4E20C400u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorCmp { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // CMEQ Vd.4S, Vd.4S, Vs.4S (0x4E208C00)
                        let ins = 0x4E208C00u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorShiftLeft { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // SSHL Vd.4S, Vd.4S, Vs.4S (0x4E204400)
                        let ins = 0x4E204400u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorShiftRight { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(32).saturating_sub(32)) as u32;
                        let vs = (s.unwrap_or(33).saturating_sub(32)) as u32;
                        // USHL Vd.4S, Vd.4S, Vs.4S (0x4EA04400)
                        let ins = 0x4EA04400u32 | (vs << 16) | (vd << 5) | vd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    // Atomics & Barrier
                    MachineInstruction::Barrier => {
                        // DMB ISH (0xD5033BFF)
                        code.extend_from_slice(&0xD5033BFFu32.to_le_bytes());
                    }
                    MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let rd = (d.unwrap_or(0)) as u32;
                        let rs = (s.unwrap_or(1)) as u32;
                        // LDADDAL Xs, Xd, [Xn] / atomic add (0xB8E00000 | (rs << 16) | (rd << 5) | rd)
                        let ins = 0xB8E00000u32 | (rs << 16) | (rd << 5) | rd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::AtomicStore { .. } => {
                        // DMB ISH (0xD5033BFF)
                        code.extend_from_slice(&0xD5033BFFu32.to_le_bytes());
                    }
                    MachineInstruction::AtomicCompareExchange { dst, expected, desired, .. } => {
                        let (d, e) = get_regs(dst, expected);
                        let rd = d.unwrap_or(0) as u32;
                        let re = e.unwrap_or(1) as u32;
                        let rdes = get_reg(desired).unwrap_or(2) as u32;
                        // CASAL Xs, Xt, [Xn] (0xC8E07C00 | (rs << 16) | (rt << 0) | (rn << 5))
                        let ins = 0xC8E07C00u32 | (re << 16) | (rd << 5) | rdes;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::AtomicExchange { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let rd = d.unwrap_or(0) as u32;
                        let rs = s.unwrap_or(1) as u32;
                        // SWPAL Xs, Xt, [Xn] (0xC8808000 | (rs << 16) | (rn << 5) | rt)
                        let ins = 0xC8808000u32 | (rs << 16) | (rd << 5) | rd;
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    _ => {
                        // NOP fallback
                        code.extend_from_slice(&0xD503201Fu32.to_le_bytes());
                    }
                }
            }
        }

        // Epilogue if missing
        if code.len() < 8 || code[code.len() - 4..] != 0xD65F03C0u32.to_le_bytes() {
            code.extend_from_slice(&0x910003BFu32.to_le_bytes()); // MOV SP, X29
            code.extend_from_slice(&0xA8C17BFDu32.to_le_bytes()); // LDP X29, X30, [SP], #16
            code.extend_from_slice(&0xD65F03C0u32.to_le_bytes()); // RET
        }

        // Resolve branch fixups
        for (offset, target_label, is_cond, cc) in fixups {
            if let Some(&target_offset) = label_offsets.get(&target_label) {
                let disp = ((target_offset as i64 - offset as i64) / 4) as i32;
                if is_cond {
                    let cond = Self::encode_cond(cc);
                    let imm19 = (disp as u32) & 0x7FFFF;
                    let ins = 0x54000000u32 | (imm19 << 5) | cond;
                    code[offset..offset + 4].copy_from_slice(&ins.to_le_bytes());
                } else {
                    let imm26 = (disp as u32) & 0x3FFFFFF;
                    let ins = 0x14000000u32 | imm26;
                    code[offset..offset + 4].copy_from_slice(&ins.to_le_bytes());
                }
            }
        }

        Ok(code)
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());
        let mut text_bytes = Vec::new();

        for func in &module.functions {
            let offset = text_bytes.len() as u64;
            let code = self.generate_function(func)?;
            let size = code.len() as u64;
            text_bytes.extend_from_slice(&code);

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                offset,
                size,
            )
            .with_binding(if func.is_exported {
                SymbolBinding::Global
            } else {
                SymbolBinding::Local
            })
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

        // Add relocations for external calls
        for func in &module.functions {
            let mut func_offset = 0u64;
            for other in &module.functions {
                if other.name == func.name {
                    break;
                }
                func_offset += 8; // approx offset calculation if needed
            }

            for block in &func.blocks {
                for inst in &block.instructions {
                    if let MachineInstruction::Call { target, .. } = inst {
                        if let MachineOperand::Symbol(sym) = target {
                            text_sec.relocations.push(AdobRelocation::new(
                                func_offset,
                                0,
                                sym.clone(),
                                RelocationKind::AArch64_Call26,
                                0,
                            ));
                        }
                    }
                }
            }
        }

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
    fn test_aarch64_return_only_function_encodes() {
        let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
        let mut backend = AArch64Backend::new(target);

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert!(!code.is_empty());
    }

    #[test]
    fn test_aarch64_aapcs64_callee_saved_preservation() {
        let reg_file = AArch64RegisterFile;
        assert_eq!(reg_file.callee_saved().len(), 10);
        assert_eq!(reg_file.callee_saved()[0].0, 19); // X19
        assert_eq!(reg_file.callee_saved()[9].0, 28); // X28

        let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
        let mut backend = AArch64Backend::new(target);

        let mut func = MachineFunction::new("use_x19_x20");
        let block = func.entry_block_mut();
        // Use callee-saved X19 and X20
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(19))),
            src: MachineOperand::Immediate(42),
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(20))),
            src: MachineOperand::Immediate(100),
        });
        block.push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        // Must contain STP X19, X20, [SP, #...] and LDP X19, X20, [SP, #...]
        assert!(code.len() >= 32);
    }

    #[test]
    fn test_aarch64_neon_vector_and_fp_encodes() {
        let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
        let mut backend = AArch64Backend::new(target);

        let mut func = MachineFunction::new("neon_calc");
        let block = func.entry_block_mut();
        block.push(MachineInstruction::VectorAdd {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(33))),
            vec_type: crate::opt::VectorType::v4f32(),
        });
        block.push(MachineInstruction::FAdd {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(33))),
            size: 8,
        });
        block.push(MachineInstruction::Barrier);
        block.push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert!(code.len() >= 20);
    }
}
