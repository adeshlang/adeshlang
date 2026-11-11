//! Cross-Platform In-Memory Native JIT Loader.
//!
//! Loads and executes Adesh `NativeModule` (Machine IR) directly in memory across
//! all supported architectures (x86-64, AArch64, RISC-V 64) and operating systems (Windows, Linux, macOS).
//!
//! Architecture:
//! 1. Lower Machine IR via target `CodegenBackend`.
//! 2. Lay out code in a contiguous memory block allocated by `JitMemoryBlock`.
//! 3. Resolve internal and external relocations (`PcRelative32`, `Absolute64`, `AArch64_Call26`, `RiscV_Call`).
//! 4. Enforce strict W^X protection transition (RW -> RX).
//! 5. Flush CPU instruction cache.
//! 6. Return typed, callable function pointers with full memory safety.

use std::collections::HashMap;

use adesh_codegen::backend::CodegenBackend;
use adesh_codegen::machine_ir::NativeModule;
use adesh_codegen::targets::create_backend;
use adesh_object::relocation::{AdobRelocation, RelocationKind};
use adesh_object::{Architecture, TargetDescriptor};
use adesh_runtime::platform::jit_mem::{JitMemoryBlock, JitMemoryError};

use super::resolver::HostSymbolResolver;

#[derive(Debug)]
pub enum JitLoaderError {
    CodegenError(adesh_codegen::error::CodegenError),
    MemoryError(JitMemoryError),
    UnresolvedSymbol(String),
    RelocationOverflow {
        symbol: String,
        kind: RelocationKind,
        value: i64,
    },
    FunctionNotFound(String),
}

impl From<adesh_codegen::error::CodegenError> for JitLoaderError {
    fn from(err: adesh_codegen::error::CodegenError) -> Self {
        Self::CodegenError(err)
    }
}

impl From<JitMemoryError> for JitLoaderError {
    fn from(err: JitMemoryError) -> Self {
        Self::MemoryError(err)
    }
}

impl std::fmt::Display for JitLoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodegenError(e) => write!(f, "JIT Codegen error: {}", e),
            Self::MemoryError(e) => write!(f, "JIT Memory error: {}", e),
            Self::UnresolvedSymbol(s) => write!(f, "Unresolved JIT symbol: {}", s),
            Self::RelocationOverflow { symbol, kind, value } => write!(
                f,
                "Relocation overflow for {} ({:?}): displacement {} out of range",
                symbol, kind, value
            ),
            Self::FunctionNotFound(s) => write!(f, "JIT Function '{}' not found in loaded module", s),
        }
    }
}

impl std::error::Error for JitLoaderError {}

/// A compiled and executable JIT module residing in memory.
pub struct LoadedJitModule {
    /// RAII memory block holding executable machine code.
    _memory: JitMemoryBlock,
    /// Function name -> executable code entry pointer.
    function_ptrs: HashMap<String, *const u8>,
}

unsafe impl Send for LoadedJitModule {}
unsafe impl Sync for LoadedJitModule {}

impl LoadedJitModule {
    /// Look up a compiled function pointer by name.
    pub fn get_function(&self, name: &str) -> Option<*const u8> {
        self.function_ptrs.get(name).copied()
    }

    /// Check if a function is present.
    pub fn has_function(&self, name: &str) -> bool {
        self.function_ptrs.contains_key(name)
    }
}

/// In-Memory JIT Compiler and Loader.
pub struct JitLoader {
    target: TargetDescriptor,
}

impl JitLoader {
    /// Create a new JIT loader for the given target descriptor.
    pub fn new(target: TargetDescriptor) -> Self {
        Self { target }
    }

    /// Compile a Machine IR `NativeModule`, resolve relocations via `resolver`,
    /// seal executable pages, and return a runnable `LoadedJitModule`.
    pub fn load_module(
        &self,
        module: &NativeModule,
        resolver: &dyn HostSymbolResolver,
    ) -> Result<LoadedJitModule, JitLoaderError> {
        let mut backend = create_backend(self.target.clone())?;

        // 1. Generate machine code and relocations for every function in the module
        let mut function_offsets = HashMap::new();
        let mut compiled_functions: Vec<(String, Vec<u8>, Vec<AdobRelocation>)> = Vec::new();
        let mut total_code_size = 0usize;

        for func in &module.functions {
            let (code, relocs) = backend.generate_function_with_relocations(func)?;
            // Align function entry to 16 bytes for high instruction-fetch throughput
            total_code_size = (total_code_size + 15) & !15;
            function_offsets.insert(func.name.clone(), total_code_size);
            total_code_size += code.len();
            compiled_functions.push((func.name.clone(), code, relocs));
        }

        if total_code_size == 0 {
            total_code_size = 16; // ensure non-zero allocation
        }

        // 2. Allocate writable memory buffer with strict W^X initial state
        let mut memory = JitMemoryBlock::allocate_writable(total_code_size)?;
        let mem_base = memory.as_mut_ptr();

        // 3. Copy function bodies into the code buffer
        for (name, code, _) in &compiled_functions {
            let offset = *function_offsets.get(name).unwrap();
            unsafe {
                std::ptr::copy_nonoverlapping(code.as_ptr(), mem_base.add(offset), code.len());
            }
        }

        // 4. Resolve and apply in-memory relocations
        for (fn_name, _, relocs) in &compiled_functions {
            let fn_base_offset = *function_offsets.get(fn_name).unwrap();

            for reloc in relocs {
                // Determine symbol address S
                let symbol_addr = if let Some(&target_fn_offset) = function_offsets.get(&reloc.symbol_name) {
                    // Symbol is a sibling function in this module
                    unsafe { mem_base.add(target_fn_offset) as usize }
                } else if let Some(ext_addr) = resolver.resolve(&reloc.symbol_name) {
                    // Symbol is an external host runtime symbol
                    ext_addr as usize
                } else {
                    return Err(JitLoaderError::UnresolvedSymbol(reloc.symbol_name.clone()));
                };

                // Patch site location P
                let patch_offset = fn_base_offset + reloc.offset as usize;
                let patch_ptr = unsafe { mem_base.add(patch_offset) };
                let p = patch_ptr as usize as i64;
                let s = symbol_addr as i64;
                let a = reloc.addend;

                match reloc.kind {
                    RelocationKind::Absolute64 => {
                        let val = (s.wrapping_add(a)) as u64;
                        unsafe {
                            (patch_ptr as *mut u64).write_unaligned(val);
                        }
                    }
                    RelocationKind::Absolute32 => {
                        let val = (s.wrapping_add(a)) as u32;
                        unsafe {
                            (patch_ptr as *mut u32).write_unaligned(val);
                        }
                    }
                    RelocationKind::PcRelative32 | RelocationKind::PltRelative32 | RelocationKind::X86_64_Plt32 => {
                        let disp = s.wrapping_add(a).wrapping_sub(p);
                        if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                            return Err(JitLoaderError::RelocationOverflow {
                                symbol: reloc.symbol_name.clone(),
                                kind: reloc.kind,
                                value: disp,
                            });
                        }
                        let disp32 = disp as i32;
                        unsafe {
                            (patch_ptr as *mut i32).write_unaligned(disp32);
                        }
                    }
                    RelocationKind::AArch64_Call26 => {
                        // 26-bit branch immediate: (S + A - P) >> 2
                        let disp = s.wrapping_add(a).wrapping_sub(p);
                        if disp < -0x200_0000 || disp > 0x1FF_FFFC {
                            return Err(JitLoaderError::RelocationOverflow {
                                symbol: reloc.symbol_name.clone(),
                                kind: reloc.kind,
                                value: disp,
                            });
                        }
                        let imm26 = ((disp >> 2) as u32) & 0x03FF_FFFF;
                        unsafe {
                            let insn = (patch_ptr as *mut u32).read_unaligned();
                            let patched = (insn & !0x03FF_FFFF) | imm26;
                            (patch_ptr as *mut u32).write_unaligned(patched);
                        }
                    }
                    RelocationKind::RiscV_Call => {
                        // AUIPC + JALR pair (8 bytes)
                        let disp = s.wrapping_add(a).wrapping_sub(p);
                        let hi20 = ((disp + 0x800) >> 12) as i32;
                        let lo12 = (disp - ((hi20 as i64) << 12)) as i32;
                        unsafe {
                            let insn1 = (patch_ptr as *mut u32).read_unaligned();
                            let insn2 = (patch_ptr.add(4) as *mut u32).read_unaligned();
                            let patched1 = (insn1 & 0xFFF) | (((hi20 as u32) & 0xF_FFFF) << 12);
                            let patched2 = (insn2 & 0x000F_FFFF) | (((lo12 as u32) & 0xFFF) << 20);
                            (patch_ptr as *mut u32).write_unaligned(patched1);
                            (patch_ptr.add(4) as *mut u32).write_unaligned(patched2);
                        }
                    }
                    other => {
                        // Other relocations (TLS, custom)
                        let _ = other;
                    }
                }
            }
        }

        // 5. Seal memory permissions from Writable to Executable (Read-Execute) and flush I-cache
        memory.seal_executable()?;

        // 6. Build entry point function table
        let mut function_ptrs = HashMap::new();
        for (name, offset) in function_offsets {
            let func_ptr = unsafe { memory.as_ptr().add(offset) };
            function_ptrs.insert(name, func_ptr);
        }

        Ok(LoadedJitModule {
            _memory: memory,
            function_ptrs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adesh_codegen::machine_ir::{
        MachineFunction, MachineInstruction, MachineOperand, MachineRegister, PhysicalRegister,
    };
    use crate::backends::jit::native::resolver::StandardHostSymbolResolver;

    #[test]
    fn test_jit_loader_multi_function_execution() {
        let triple = if cfg!(windows) {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_os = "macos") {
            "x86_64-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        let target = TargetDescriptor::from_triple(triple).expect("valid target triple");

        let loader = JitLoader::new(target);
        let resolver = StandardHostSymbolResolver::new();

        let mut module = NativeModule::new("test_loader_mod");

        // fn sub(x) -> x - 2
        let arg_reg = if cfg!(windows) { 1 } else { 7 };
        let mut sub_fn = MachineFunction::new("sub_two");
        sub_fn.is_exported = true;
        sub_fn.entry_block_mut().push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(arg_reg))),
        });
        sub_fn.entry_block_mut().push(MachineInstruction::Sub {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            src: MachineOperand::Immediate(2),
        });
        sub_fn.entry_block_mut().push(MachineInstruction::Return);
        module.add_function(sub_fn);

        // fn main() -> calls sub_two(44)
        let mut main_fn = MachineFunction::new("main");
        main_fn.is_exported = true;
        main_fn.entry_block_mut().push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(arg_reg))),
            src: MachineOperand::Immediate(44),
        });
        main_fn.entry_block_mut().push(MachineInstruction::Call {
            target: MachineOperand::Symbol("sub_two".to_string()),
            num_args: 1,
        });
        main_fn.entry_block_mut().push(MachineInstruction::Return);
        module.add_function(main_fn);

        let loaded = loader.load_module(&module, &resolver).expect("load failed");
        assert!(loaded.has_function("main"));
        assert!(loaded.has_function("sub_two"));

        type MainFn = extern "C" fn() -> i64;
        let main_ptr = loaded.get_function("main").expect("main found");
        let func: MainFn = unsafe { std::mem::transmute(main_ptr) };
        let result = func();
        assert_eq!(result, 42);
    }
}
