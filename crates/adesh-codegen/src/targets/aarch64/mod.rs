//! AArch64 Native Codegen Backend.

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

/// AArch64 Register File (X0-X30, SP/XZR).
pub struct AArch64RegisterFile;

const AARCH64_ALL_REGS: [PhysicalRegister; 31] = [
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
];

impl RegisterFile for AArch64RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &AARCH64_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &AARCH64_ALL_REGS[0..29] // X0-X28
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &AARCH64_ALL_REGS[0..19]
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &AARCH64_ALL_REGS[19..29]
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &AARCH64_ALL_REGS[29..31] // FP (X29), LR (X30)
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
            allocator.allocate(func);
        }
        Ok(lowered)
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        let mut code = Vec::new();

        // STP X29, X30, [SP, #-16]! (0xA9BF7BFD)
        code.extend_from_slice(&0xA9BF7BFDu32.to_le_bytes());
        // MOV X29, SP (0x910003FD)
        code.extend_from_slice(&0x910003FDu32.to_le_bytes());

        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        // NOP (0xD503201F)
                        code.extend_from_slice(&0xD503201Fu32.to_le_bytes());
                    }
                    MachineInstruction::Return => {
                        // LDP X29, X30, [SP], #16 (0xA8C17BFD)
                        code.extend_from_slice(&0xA8C17BFDu32.to_le_bytes());
                        // RET (0xD65F03C0)
                        code.extend_from_slice(&0xD65F03C0u32.to_le_bytes());
                    }
                    MachineInstruction::Add { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            // ADD Xd, Xd, Xs (0x8B000000 | (Xs << 16) | (Xd << 5) | Xd)
                            let ins = 0x8B000000u32
                                | ((s.0 as u32) << 16)
                                | ((d.0 as u32) << 5)
                                | (d.0 as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    MachineInstruction::Sub { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            // SUB Xd, Xd, Xs
                            let ins = 0xCB000000u32
                                | ((s.0 as u32) << 16)
                                | ((d.0 as u32) << 5)
                                | (d.0 as u32);
                            code.extend_from_slice(&ins.to_le_bytes());
                        }
                    }
                    _ => {}
                }
            }
        }

        // Epilogue if missing
        if code.len() < 8 || code[code.len() - 4..] != 0xD65F03C0u32.to_le_bytes() {
            code.extend_from_slice(&0xA8C17BFDu32.to_le_bytes());
            code.extend_from_slice(&0xD65F03C0u32.to_le_bytes());
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
