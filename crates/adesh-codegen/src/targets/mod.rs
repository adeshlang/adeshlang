//! Target backends module.

pub mod aarch64;
pub mod embedded;
pub mod riscv;
pub mod wasm;
pub mod x86_64;

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, NativeModule};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, Architecture, SectionKind, SymbolBinding, SymbolKind,
    TargetDescriptor, section_flags,
};
use std::collections::HashMap;

/// Create a CodegenBackend instance for the specified target descriptor.
pub fn create_backend(target: TargetDescriptor) -> Result<Box<dyn CodegenBackend>, CodegenError> {
    if target.endianness == adesh_object::Endianness::Big {
        match target.architecture {
            Architecture::X86_64 => {
                return Err(CodegenError::new(
                    target.triple_string(),
                    "x86-64 architecture does not support big-endian byte order".to_string(),
                )
                .with_arch("x86_64")
                .with_suggestion("Configure target endianness to Little for x86_64."));
            }
            Architecture::Wasm32 | Architecture::Wasm64 => {
                return Err(CodegenError::new(
                    target.triple_string(),
                    "WebAssembly specification mandates little-endian memory semantics; big-endian is unsupported"
                        .to_string(),
                )
                .with_arch("wasm")
                .with_suggestion("Configure target endianness to Little for WebAssembly."));
            }
            _ => {}
        }
    }

    match target.architecture {
        Architecture::X86_64 => Ok(Box::new(x86_64::X86_64Backend::new(target))),
        Architecture::AArch64 => Ok(Box::new(aarch64::AArch64Backend::new(target))),
        Architecture::RiscV64 => Ok(Box::new(riscv::RiscVBackend::new(target))),
        Architecture::RiscV32 => Err(CodegenError::new(
            target.triple_string(),
            "the RISC-V backend implements RV64 only; an RV32 target would be encoded with \
             doubleword (64-bit) instructions and produce an image the RV32 hardware cannot run"
                .to_string(),
        )
        .with_arch("riscv32")
        .with_suggestion(
            "build for a riscv64 triple (e.g. riscv64gc-unknown-linux-gnu), or implement RV32 \
             (word-width loads and stores plus the ILP32 calling convention) in targets/riscv",
        )),
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

/// Emit `.rodata`, `.tls` and `.data` for a lowered module.
///
/// The shared lowering fills `NativeModule::string_pool` for every backend and
/// the encoders emit symbol-address pairs naming `__str_<n>`, so an object
/// without these sections links against an undefined symbol. AArch64 measured
/// `error[LNK001]: undefined reference to __str_0` until it emitted them
/// (2026-11-04); RISC-V needs the same definitions for its `AUIPC`/`ADDI` pairs.
/// x86_64 emits its own equivalent inline, with a hardcoded `.rodata` index.
pub(crate) fn emit_module_data_sections(obj: &mut AdobObject, module: &NativeModule) {
    if !module.string_pool.is_empty() {
        let rodata_index = obj.sections.len() as u32;
        let mut rodata = Vec::new();
        let mut offsets: HashMap<&str, u64> = HashMap::new();
        for (idx, s) in module.string_pool.iter().enumerate() {
            let off = *offsets.entry(s.as_str()).or_insert_with(|| {
                let cur = rodata.len() as u64;
                rodata.extend_from_slice(s.as_bytes());
                rodata.push(0);
                cur
            });
            obj.add_symbol(
                AdobSymbol::new_defined(
                    0,
                    format!("__str_{}", idx),
                    SymbolKind::Object,
                    rodata_index,
                    off,
                    s.len() as u64 + 1,
                )
                .with_binding(SymbolBinding::Local),
            );
        }
        obj.add_section(
            AdobSection::new(".rodata", SectionKind::Rodata)
                .with_flags(section_flags::READ | section_flags::ALLOC)
                .with_alignment(8)
                .with_data(rodata),
        );
    }

    if !module.tls_sections.is_empty() {
        let tls_index = obj.sections.len() as u32;
        let mut tls_data = Vec::new();
        for (name, init_bytes) in &module.tls_sections {
            let aligned_off = (tls_data.len() + 7) & !7;
            tls_data.resize(aligned_off, 0);
            let off = tls_data.len() as u64;
            let size = init_bytes.len() as u64;
            tls_data.extend_from_slice(init_bytes);
            obj.add_symbol(
                AdobSymbol::new_defined(0, name.clone(), SymbolKind::Object, tls_index, off, size)
                    .with_binding(SymbolBinding::Global),
            );
        }
        obj.add_section(
            AdobSection::new(".tls", SectionKind::Tls)
                .with_flags(
                    section_flags::READ
                        | section_flags::WRITE
                        | section_flags::TLS
                        | section_flags::ALLOC,
                )
                .with_alignment(16)
                .with_data(tls_data),
        );
    }

    if !module.data_sections.is_empty() {
        let data_index = obj.sections.len() as u32;
        let mut data_bytes = Vec::new();
        for (name, init_bytes) in &module.data_sections {
            let aligned_off = (data_bytes.len() + 7) & !7;
            data_bytes.resize(aligned_off, 0);
            let off = data_bytes.len() as u64;
            let size = init_bytes.len() as u64;
            data_bytes.extend_from_slice(init_bytes);
            obj.add_symbol(
                AdobSymbol::new_defined(0, name.clone(), SymbolKind::Object, data_index, off, size)
                    .with_binding(SymbolBinding::Global),
            );
        }
        obj.add_section(
            AdobSection::new(".data", SectionKind::Data)
                .with_flags(section_flags::READ | section_flags::WRITE | section_flags::ALLOC)
                .with_alignment(16)
                .with_data(data_bytes),
        );
    }
}
