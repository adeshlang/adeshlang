//! WebAssembly (WASM32 / WASM64) Native Codegen Backend.

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, NativeModule};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility,
    TargetCapabilities, TargetDescriptor, section_flags,
};

pub struct WasmBackend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl WasmBackend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }
}

impl CodegenBackend for WasmBackend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        Ok(module.clone())
    }

    fn generate_function(&mut self, _func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        // WASM function body: 0 locals (0x00), end opcode (0x0B)
        Ok(vec![0x00, 0x0B])
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
            .with_alignment(1)
            .with_data(text_bytes);
        obj.add_section(text_sec);

        Ok(obj)
    }
}
