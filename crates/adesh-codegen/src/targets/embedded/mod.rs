//! Embedded and Bare-Metal Codegen Backend (Cortex-M, RV32IMAC, Freestanding).

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, NativeModule};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, MemoryPermissions, MemoryRegion, SectionKind,
    SymbolBinding, SymbolKind, SymbolVisibility, TargetCapabilities, TargetDescriptor,
    section_flags,
};

pub struct EmbeddedBackend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl EmbeddedBackend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }
}

impl CodegenBackend for EmbeddedBackend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        Ok(module.clone())
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        // This backend only models the empty-function epilogue: `bx lr` is the
        // correct encoding only when there is no body to execute. Every other
        // instruction used to be silently discarded (any function body was
        // emitted as a bare return), so refuse loudly instead.
        for block in &func.blocks {
            for inst in &block.instructions {
                if !matches!(inst, MachineInstruction::Nop | MachineInstruction::Return) {
                    return Err(crate::targets::unsupported_instruction(
                        "embedded", func, inst,
                    ));
                }
            }
        }
        // Simple embedded return: bx lr (0x4770 in Thumb-2)
        Ok(vec![0x70, 0x47])
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        // Embedded standard memory layout regions: Flash (0x08000000) and SRAM (0x20000000)
        obj.memory_regions.push(MemoryRegion::new(
            ".flash",
            0x08000000,
            512 * 1024,
            MemoryPermissions::Rx,
        ));
        obj.memory_regions.push(MemoryRegion::new(
            ".sram",
            0x20000000,
            128 * 1024,
            MemoryPermissions::Rw,
        ));

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
            .with_alignment(4)
            .with_memory_region(".flash")
            .with_data(text_bytes);
        obj.add_section(text_sec);

        Ok(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::{MachineOperand, MachineRegister, VirtualRegister};
    use adesh_object::TargetDescriptor;

    #[test]
    fn return_only_function_encodes() {
        let target = TargetDescriptor::from_triple("thumbv7em-none-eabihf").expect("triple");
        let mut backend = EmbeddedBackend::new(target);

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0x70, 0x47]); // bx lr
    }

    #[test]
    fn function_bodies_are_loud_errors() {
        let target = TargetDescriptor::from_triple("thumbv7em-none-eabihf").expect("triple");
        let mut backend = EmbeddedBackend::new(target);

        let mut func = MachineFunction::new("boom");
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
            src: MachineOperand::Immediate(1),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let err = backend
            .generate_function(&func)
            .expect_err("bodies are unimplemented");
        assert!(err.reason.contains("not implemented"), "{}", err.reason);
        assert_eq!(err.instruction.as_deref(), Some("Move"));
    }
}
