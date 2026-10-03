//! Cranelift compatibility adapter (placeholder).
//!
//! Despite the historical name, this adapter does **not** wrap Cranelift: the
//! crate has no Cranelift dependency, and no Cranelift code is generated or
//! converted here. It only emits the empty-function epilogue for functions
//! with no body; anything else is a loud error rather than a silently wrong
//! object file. A real adapter would lower Machine IR to `cranelift-codegen`
//! `Function`s and reuse Cranelift's compilation pipeline.

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, NativeModule};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility,
    TargetCapabilities, TargetDescriptor, section_flags,
};

/// Cranelift Codegen Backend Adapter.
pub struct CraneliftAdapter {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl CraneliftAdapter {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }
}

impl CodegenBackend for CraneliftAdapter {
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
        // Only the empty-function epilogue is modelled (see the module docs):
        // this sequence is the correct encoding only when there is no body to
        // execute. Any real body used to be silently discarded, so refuse
        // loudly instead.
        for block in &func.blocks {
            for inst in &block.instructions {
                if !matches!(inst, MachineInstruction::Nop | MachineInstruction::Return) {
                    return Err(crate::targets::unsupported_instruction(
                        "cranelift-adapter",
                        func,
                        inst,
                    ));
                }
            }
        }
        // Simple return sequence compatible with Cranelift emitted frames
        #[cfg(target_arch = "x86_64")]
        {
            Ok(vec![0x55, 0x48, 0x89, 0xE5, 0x5D, 0xC3]) // push rbp; mov rbp, rsp; pop rbp; ret
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Ok(vec![0xC3])
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::{MachineOperand, MachineRegister, VirtualRegister};
    use adesh_object::TargetDescriptor;

    #[test]
    fn return_only_function_encodes() {
        let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("triple");
        let mut backend = CraneliftAdapter::new(target);

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert!(!code.is_empty());
    }

    #[test]
    fn function_bodies_are_loud_errors() {
        let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("triple");
        let mut backend = CraneliftAdapter::new(target);

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
