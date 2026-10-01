//! Target backends module.

pub mod aarch64;
pub mod embedded;
pub mod riscv;
pub mod wasm;
pub mod x86_64;

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use adesh_object::{Architecture, TargetDescriptor};

/// Create a CodegenBackend instance for the specified target descriptor.
pub fn create_backend(target: TargetDescriptor) -> Result<Box<dyn CodegenBackend>, CodegenError> {
    match target.architecture {
        Architecture::X86_64 => Ok(Box::new(x86_64::X86_64Backend::new(target))),
        Architecture::AArch64 => Ok(Box::new(aarch64::AArch64Backend::new(target))),
        Architecture::RiscV32 | Architecture::RiscV64 => {
            Ok(Box::new(riscv::RiscVBackend::new(target)))
        }
        Architecture::Wasm32 | Architecture::Wasm64 => Ok(Box::new(wasm::WasmBackend::new(target))),
        Architecture::Embedded(_) => Ok(Box::new(embedded::EmbeddedBackend::new(target))),
        _ => Err(CodegenError::new(
            target.triple_string(),
            format!(
                "Unsupported target architecture `{:?}` for native code generation",
                target.architecture
            ),
        )
        .with_suggestion("Check supported targets or compile with Cranelift fallback backend.")),
    }
}
