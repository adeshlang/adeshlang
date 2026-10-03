//! Target backends module.

pub mod aarch64;
pub mod embedded;
pub mod riscv;
pub mod wasm;
pub mod x86_64;

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction};
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

/// The variant name of `inst` (e.g. `Add`), for diagnostics.
pub(crate) fn instruction_kind(inst: &MachineInstruction) -> String {
    format!("{inst:?}")
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string()
}

/// Loud error for a Machine IR instruction a proof-of-concept backend cannot
/// lower.
///
/// Silently skipping the instruction (or emitting a truncated encoding) would
/// produce object code that computes the wrong result without any diagnostic,
/// so the backends that only implement a subset of the instruction set refuse
/// to emit code for the rest.
pub(crate) fn unsupported_instruction(
    arch: &str,
    func: &MachineFunction,
    inst: &MachineInstruction,
) -> CodegenError {
    let kind = instruction_kind(inst);
    CodegenError::new(
        arch,
        format!(
            "instruction `{kind}` is not implemented by the {arch} backend; \
             refusing to emit object code that would silently miscompile"
        ),
    )
    .with_arch(arch)
    .with_function(func.name.clone())
    .with_instruction(kind)
    .with_suggestion(
        "lower through the x86_64 backend, or implement the instruction in this backend",
    )
}

/// Loud error for an instruction whose *implemented* form does not cover the
/// given operands (e.g. only the physical `reg, reg` form exists).
pub(crate) fn unsupported_operand_form(
    arch: &str,
    func: &MachineFunction,
    inst: &MachineInstruction,
) -> CodegenError {
    let kind = instruction_kind(inst);
    CodegenError::new(
        arch,
        format!(
            "no implemented {arch} encoding for `{kind}` with these operands; \
             refusing to emit object code that would silently miscompile"
        ),
    )
    .with_arch(arch)
    .with_function(func.name.clone())
    .with_instruction(kind)
    .with_suggestion(
        "run the register allocator (lower_module) first, or implement the missing operand forms",
    )
}
