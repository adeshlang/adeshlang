//! RISC-V (RV32 / RV64) Native Codegen Backend.

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use crate::register_alloc::{LinearScanAllocator, RegisterFile};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility,
    TargetCapabilities, TargetDescriptor, section_flags,
};

/// RISC-V Register File (x0-x31).
pub struct RiscVRegisterFile;

const RISCV_ALL_REGS: [PhysicalRegister; 32] = [
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
];

impl RegisterFile for RiscVRegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &RISCV_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &RISCV_ALL_REGS[5..32] // t0-t6, s0-s11, a0-a7
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &RISCV_ALL_REGS[10..18] // a0-a7
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &RISCV_ALL_REGS[18..28] // s2-s11
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &RISCV_ALL_REGS[0..5] // zero, ra, sp, gp, tp
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
            allocator.allocate(func);
        }
        Ok(lowered)
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        let mut code = Vec::new();

        // addi sp, sp, -16 (0xFF010113)
        code.extend_from_slice(&0xFF010113u32.to_le_bytes());
        // sd ra, 8(sp) (0x00113423)
        code.extend_from_slice(&0x00113423u32.to_le_bytes());

        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        // nop -> addi x0, x0, 0 (0x00000013)
                        code.extend_from_slice(&0x00000013u32.to_le_bytes());
                    }
                    MachineInstruction::Return => {
                        // ld ra, 8(sp) (0x00813083)
                        code.extend_from_slice(&0x00813083u32.to_le_bytes());
                        // addi sp, sp, 16 (0x01010113)
                        code.extend_from_slice(&0x01010113u32.to_le_bytes());
                        // ret -> jalr x0, 0(ra) (0x00008067)
                        code.extend_from_slice(&0x00008067u32.to_le_bytes());
                    }
                    MachineInstruction::Add { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            // add rd, rd, rs2 (0x00000033 | (rs2 << 20) | (rs1 << 15) | (rd << 7))
                            let ins = 0x00000033u32
                                | ((s.0 as u32) << 20)
                                | ((d.0 as u32) << 15)
                                | ((d.0 as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Sub { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            // sub rd, rd, rs2
                            let ins = 0x40000033u32
                                | ((s.0 as u32) << 20)
                                | ((d.0 as u32) << 15)
                                | ((d.0 as u32) << 7);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    _ => {}
                }
            }
        }

        if code.len() < 12 || code[code.len() - 4..] != 0x00008067u32.to_le_bytes() {
            code.extend_from_slice(&0x00813083u32.to_le_bytes());
            code.extend_from_slice(&0x01010113u32.to_le_bytes());
            code.extend_from_slice(&0x00008067u32.to_le_bytes());
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
