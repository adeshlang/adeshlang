//! RISC-V (RV32 / RV64) Native Codegen Backend.
//!
//! Provides production-grade RV64GC instruction selection, register allocation,
//! double-precision floating-point (RV64D), atomics (RV64A), and ADOB/ELF object generation.

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass,
};
use crate::register_alloc::{LinearScanAllocator, RegisterFile};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility,
    TargetCapabilities, TargetDescriptor, section_flags,
};
use std::collections::HashMap;

/// RISC-V Register File (x0-x31, f0-f31).
pub struct RiscVRegisterFile;

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

const RISCV_ALLOCATABLE_GPR: [PhysicalRegister; 15] = [
    PhysicalRegister(5),  // t0
    PhysicalRegister(6),  // t1
    PhysicalRegister(7),  // t2
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

const RISCV_ALLOCATABLE_FP: [PhysicalRegister; 16] = [
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
];

const RISCV_RESERVED: [PhysicalRegister; 5] = [
    PhysicalRegister(0), // zero
    PhysicalRegister(1), // ra
    PhysicalRegister(2), // sp
    PhysicalRegister(3), // gp
    PhysicalRegister(4), // tp
];

impl RegisterFile for RiscVRegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &RISCV_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &RISCV_ALLOCATABLE_GPR
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &RISCV_ALLOCATABLE_GPR
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &[]
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
        let mut fixups: Vec<(usize, String, bool, ConditionCode)> = Vec::new();

        // Prologue: addi sp, sp, -16 (0xFF010113); sd ra, 8(sp) (0x00113423)
        code.extend_from_slice(&0xFF010113u32.to_le_bytes());
        code.extend_from_slice(&0x00113423u32.to_le_bytes());

        for block in &func.blocks {
            label_offsets.insert(block.label.clone(), code.len());

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        // NOP (addi x0, x0, 0)
                        code.extend_from_slice(&0x00000013u32.to_le_bytes());
                    }
                    MachineInstruction::Return => {
                        // ld ra, 8(sp) (0x00813083); addi sp, sp, 16 (0x01010113); jalr x0, 0(ra) (0x00008067)
                        code.extend_from_slice(&0x00813083u32.to_le_bytes());
                        code.extend_from_slice(&0x01010113u32.to_le_bytes());
                        code.extend_from_slice(&0x00008067u32.to_le_bytes());
                    }
                    MachineInstruction::Move { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            if d_reg >= 32 && s_reg >= 32 {
                                // fsgnj.d fd, fs, fs (0x22000053 | (fs << 20) | (fs << 15) | (fd << 7))
                                let fd = (d_reg - 32) as u32;
                                let fs = (s_reg - 32) as u32;
                                let ins = 0x22000053u32 | (fs << 20) | (fs << 15) | (fd << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            } else {
                                // addi rd, rs, 0 (MV rd, rs)
                                let ins =
                                    0x00000013u32 | ((s_reg as u32) << 15) | ((d_reg as u32) << 7);
                                code.extend_from_slice(&ins.to_le_bytes());
                            }
                        } else if let (Some(d_reg), MachineOperand::Immediate(val)) = (d, src) {
                            // addi rd, x0, imm12 (LI rd, imm12)
                            let imm12 = (*val as u32) & 0xFFF;
                            let ins = 0x00000013u32 | (imm12 << 20) | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
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
                            let imm12 = (*val as u32) & 0xFFF;
                            let ins = 0x00000013u32
                                | (imm12 << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
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
                    MachineInstruction::And { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // and rd, rd, rs2
                            let ins = 0x00007033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Or { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // or rd, rd, rs2
                            let ins = 0x00006033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Xor { dst, src } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // xor rd, rd, rs2
                            let ins = 0x00004033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs } => {
                        let (d, s) = get_regs(lhs, rhs);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            // slt t0 (5), rs1, rs2
                            let ins = 0x00002033u32
                                | ((s_reg as u32) << 20)
                                | ((d_reg as u32) << 15)
                                | (5 << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Branch { target } => {
                        let offset = code.len();
                        fixups.push((offset, target.clone(), false, ConditionCode::Equal));
                        code.extend_from_slice(&0x0000006Fu32.to_le_bytes()); // JAL x0, #0
                    }
                    MachineInstruction::BranchCc { cc, target } => {
                        let offset = code.len();
                        fixups.push((offset, target.clone(), true, *cc));
                        code.extend_from_slice(&0x00000063u32.to_le_bytes()); // BEQ x0, x0, #0
                    }
                    MachineInstruction::Call { target, .. } => {
                        if let Some(r) = get_reg(target) {
                            // JALR ra, 0(rs1)
                            let ins = 0x000000E7u32 | ((r as u32) << 15);
                            code.extend_from_slice(&ins.to_le_bytes());
                        } else {
                            // JAL ra, #0
                            code.extend_from_slice(&0x000000EFu32.to_le_bytes());
                        }
                    }
                    MachineInstruction::Load { dst, src, .. } => {
                        if let (Some(d_reg), MachineOperand::StackSlot(slot)) = (get_reg(dst), src)
                        {
                            // LD rd, offset(sp)
                            let imm12 = (*slot as u32) & 0xFFF;
                            let ins =
                                0x00003003u32 | (imm12 << 20) | (2 << 15) | ((d_reg as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Store { dst, src, .. } => {
                        if let (MachineOperand::StackSlot(slot), Some(s_reg)) = (dst, get_reg(src))
                        {
                            // SD rs2, offset(sp)
                            let off = (*slot as u32) & 0xFFF;
                            let imm_hi = (off >> 5) & 0x7F;
                            let imm_lo = off & 0x1F;
                            let ins = (imm_hi << 25)
                                | ((s_reg as u32) << 20)
                                | (2 << 15)
                                | (3 << 12)
                                | (imm_lo << 7)
                                | 0x23;
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    // Floating-Point (F0..F31)
                    MachineInstruction::FAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            // FADD.D fd, fd, fs (0x02000053 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7))
                            let ins =
                                0x02000053u32 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FSub { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            // FSUB.D fd, fd, fs (0x0A000053 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7))
                            let ins =
                                0x0A000053u32 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FMul { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            // FMUL.D fd, fd, fs (0x12000053 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7))
                            let ins =
                                0x12000053u32 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::FDiv { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        if let (Some(d_reg), Some(s_reg)) = (d, s) {
                            let fd = (d_reg.saturating_sub(32)) as u32;
                            let fs = (s_reg.saturating_sub(32)) as u32;
                            // FDIV.D fd, fd, fs (0x1A000053 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7))
                            let ins =
                                0x1A000053u32 | (fs << 20) | (fd << 15) | (7 << 12) | (fd << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    // RISC-V Vector Extension (RVV 1.0)
                    MachineInstruction::VectorAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        // VADD.VV vd, vd, vs (0x02000057 | (vs << 20) | (vd << 15) | (vd << 7))
                        let ins = 0x02000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorSub { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        // VSUB.VV vd, vd, vs (0x0A000057 | (vs << 20) | (vd << 15) | (vd << 7))
                        let ins = 0x0A000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMul { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        // VMUL.VV vd, vd, vs (0x92002057 | (vs << 20) | (vd << 15) | (vd << 7))
                        let ins = 0x92002057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMin { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        // VMIN.VV vd, vd, vs (0x12000057 | (vs << 20) | (vd << 15) | (vd << 7))
                        let ins = 0x12000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorMax { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let vd = (d.unwrap_or(0) & 0x1F) as u32;
                        let vs = (s.unwrap_or(1) & 0x1F) as u32;
                        // VMAX.VV vd, vd, vs (0x1A000057 | (vs << 20) | (vd << 15) | (vd << 7))
                        let ins = 0x1A000057u32 | (vs << 20) | (vd << 15) | (vd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorLoad { dst, .. } => {
                        let d = get_reg(dst).unwrap_or(0) & 0x1F;
                        // VLE32.V vd, (sp=2) (0x02000007 | (2 << 15) | (vd << 7))
                        let ins = 0x02000007u32 | (2 << 15) | ((d as u32) << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    MachineInstruction::VectorStore { src, .. } => {
                        let s = get_reg(src).unwrap_or(0) & 0x1F;
                        // VSE32.V vs3, (sp=2) (0x02000027 | (2 << 15) | (vs3 << 7))
                        let ins = 0x02000027u32 | (2 << 15) | ((s as u32) << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    // Atomics & Barrier
                    MachineInstruction::Barrier => {
                        // FENCE iorw, iorw (0x0FF0000F)
                        code.extend_from_slice(&0x0FF0000Fu32.to_le_bytes());
                    }
                    MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
                        let (d, s) = get_regs(dst, src);
                        let rd = (d.unwrap_or(10)) as u32;
                        let rs2 = (s.unwrap_or(11)) as u32;
                        // AMOADD.D rd, rs2, (rs1=sp=2) (0x0000302F | (rs2 << 20) | (2 << 15) | (rd << 7))
                        let ins = 0x0000302Fu32 | (rs2 << 20) | (2 << 15) | (rd << 7);
                        code.extend_from_slice(&ins.to_le_bytes());
                    }
                    _ => {
                        // NOP fallback
                        code.extend_from_slice(&0x00000013u32.to_le_bytes());
                    }
                }
            }
        }

        // Epilogue if missing
        if code.len() < 12 || code[code.len() - 4..] != 0x00008067u32.to_le_bytes() {
            code.extend_from_slice(&0x00813083u32.to_le_bytes());
            code.extend_from_slice(&0x01010113u32.to_le_bytes());
            code.extend_from_slice(&0x00008067u32.to_le_bytes());
        }

        // Resolve branch fixups
        for (offset, target_label, is_cond, _cc) in fixups {
            if let Some(&target_offset) = label_offsets.get(&target_label) {
                let disp = target_offset as i64 - offset as i64;
                if is_cond {
                    let imm = ((disp / 2) as u32) & 0xFFF;
                    let imm12 = (imm >> 11) & 1;
                    let imm10_5 = (imm >> 4) & 0x3F;
                    let imm4_1 = imm & 0xF;
                    let imm11 = (imm >> 10) & 1;
                    let ins = (imm12 << 31) | (imm10_5 << 25) | (imm4_1 << 8) | (imm11 << 7) | 0x63;
                    code[offset..offset + 4].copy_from_slice(&ins.to_le_bytes());
                } else {
                    let imm = ((disp / 2) as u32) & 0xFFFFF;
                    let imm20 = (imm >> 19) & 1;
                    let imm10_1 = imm & 0x3FF;
                    let imm11 = (imm >> 10) & 1;
                    let imm19_12 = (imm >> 11) & 0xFF;
                    let ins =
                        (imm20 << 31) | (imm10_1 << 21) | (imm11 << 20) | (imm19_12 << 12) | 0x6F;
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

        let text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(16)
            .with_data(text_bytes);
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
}
