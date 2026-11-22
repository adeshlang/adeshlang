//! IA-32 / i686 Native Codegen Backend (stub).
//!
//! Phase 2 foundation for 32-bit native execution. This backend intentionally
//! implements *only* the empty-function epilogue (`ret`); every other
//! `MachineInstruction` variant is refused with a loud `CodegenError` naming the
//! variant and function. It must never silently emit 64-bit instructions or drop
//! operands — doing so would miscompile an i686 image with x86-64 semantics.
//!
//! The design mirrors `crates/adesh-codegen/src/targets/embedded/mod.rs`:
//! honest about what it cannot do, loud when asked to do more.
//!
//! Memory-safe (`#![forbid(unsafe_code)]`), type-safe, no heap in the refusal
//! path, minimal executable size.

#![forbid(unsafe_code)]

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, NativeModule};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, MemoryPermissions, MemoryRegion, SectionKind,
    SymbolBinding, SymbolKind, SymbolVisibility, TargetCapabilities, TargetDescriptor,
    section_flags,
};

pub struct I686Backend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl I686Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }
}

impl CodegenBackend for I686Backend {
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
        // IA-32 stub: only the empty-function epilogue is encodable. `ret`
        // (0xC3) is correct only when there is no body to execute. Every other
        // instruction is refused loudly instead of being silently dropped or
        // encoded with x86-64 semantics.
        for block in &func.blocks {
            for inst in &block.instructions {
                if !matches!(inst, MachineInstruction::Nop | MachineInstruction::Return) {
                    return Err(crate::targets::unsupported_instruction("i686", func, inst));
                }
            }
        }
        // Simple IA-32 return: `ret` (0xC3).
        Ok(vec![0xC3])
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        // IA-32 standard memory layout: text at the ELF32 image base.
        obj.memory_regions.push(MemoryRegion::new(
            ".text",
            0x08048000,
            512 * 1024,
            MemoryPermissions::Rx,
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
            .with_binding(SymbolBinding::Global)
            .with_visibility(SymbolVisibility::Default);
            obj.add_symbol(sym);

            if func.is_exported {
                obj.add_export(func.name.clone());
            }
        }

        let text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(4)
            .with_memory_region(".text")
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
    fn return_only_function_encodes_ret() {
        let target = TargetDescriptor::from_triple("i686-unknown-linux-gnu").expect("triple");
        let mut backend = I686Backend::new(target);

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0xC3]); // ret
    }

    #[test]
    fn function_bodies_are_loud_errors() {
        let target = TargetDescriptor::from_triple("i686-unknown-linux-gnu").expect("triple");
        let mut backend = I686Backend::new(target);

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

    #[test]
    fn nop_and_return_are_accepted() {
        let target = TargetDescriptor::from_triple("i686-unknown-linux-gnu").expect("triple");
        let mut backend = I686Backend::new(target);

        let mut func = MachineFunction::new("nop_ret");
        func.blocks[0].push(MachineInstruction::Nop);
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes nop+ret");
        assert_eq!(code, vec![0xC3]);
    }
}
