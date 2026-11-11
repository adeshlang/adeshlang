//! Proof-of-Concept: Native Codegen in-memory JIT execution.
//!
//! Validates the full execution pipeline:
//! Machine IR -> X86_64Backend -> in-memory executable page -> W^X seal -> invoke -> assert.
//! Cranelift is NOT used anywhere in this pipeline.

use adesh_codegen::backend::CodegenBackend;
use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::{RelocationKind, TargetDescriptor};

/// Simple RAII executable memory manager implementing strict W^X.
struct ExecutableMemory {
    ptr: *mut u8,
    size: usize,
}

impl ExecutableMemory {
    /// Allocate writable memory for copying code and patching relocations.
    fn allocate(size: usize) -> Result<Self, String> {
        let page_size = 4096;
        let rounded = (size + page_size - 1) & !(page_size - 1);

        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Memory::*;
            let ptr = VirtualAlloc(
                std::ptr::null_mut(),
                rounded,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );
            if ptr.is_null() {
                return Err("VirtualAlloc failed".to_string());
            }
            Ok(Self {
                ptr: ptr as *mut u8,
                size: rounded,
            })
        }

        #[cfg(unix)]
        unsafe {
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                rounded,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED {
                return Err("mmap failed".to_string());
            }
            Ok(Self {
                ptr: ptr as *mut u8,
                size: rounded,
            })
        }
    }

    /// Seal memory from Writable to Executable (Read + Execute) and flush instruction cache.
    fn seal_executable(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
            use windows_sys::Win32::System::Memory::*;
            use windows_sys::Win32::System::Threading::GetCurrentProcess;

            let mut old_protect = 0;
            let ok = VirtualProtect(
                self.ptr as _,
                self.size,
                PAGE_EXECUTE_READ,
                &mut old_protect,
            );
            if ok == 0 {
                return Err("VirtualProtect failed".to_string());
            }
            FlushInstructionCache(GetCurrentProcess(), self.ptr as _, self.size);
            Ok(())
        }

        #[cfg(unix)]
        unsafe {
            let ret = libc::mprotect(
                self.ptr as *mut _,
                self.size,
                libc::PROT_READ | libc::PROT_EXEC,
            );
            if ret != 0 {
                return Err("mprotect failed".to_string());
            }
            #[cfg(target_arch = "aarch64")]
            {
                #[cfg(target_os = "macos")]
                libc::sys_icache_invalidate(self.ptr, self.size);
                #[cfg(target_os = "linux")]
                {
                    let start = self.ptr as usize;
                    let end = start + self.size;
                    std::arch::aarch64::__clear_cache(start as *mut _, end as *mut _);
                }
            }
            Ok(())
        }
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr
    }
}

impl Drop for ExecutableMemory {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Memory::*;
            VirtualFree(self.ptr as _, 0, MEM_RELEASE);
        }

        #[cfg(unix)]
        unsafe {
            libc::munmap(self.ptr as _, self.size);
        }
    }
}

fn host_target() -> TargetDescriptor {
    #[cfg(target_os = "windows")]
    let triple = "x86_64-pc-windows-msvc";
    #[cfg(target_os = "linux")]
    let triple = "x86_64-unknown-linux-gnu";
    #[cfg(target_os = "macos")]
    let triple = "x86_64-apple-darwin";

    TargetDescriptor::from_triple(triple).expect("valid target triple")
}

#[test]
fn test_poc_native_codegen_immediate_return() {
    let target = host_target();

    // 1. Build a Machine IR function: main() -> returns 42
    let mut module = NativeModule::new("poc_imm_return");
    let mut func = MachineFunction::new("main");
    func.is_exported = true;

    // mov rax, 42
    func.entry_block_mut().push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(42),
    });
    // ret
    func.entry_block_mut().push(MachineInstruction::Return);
    module.add_function(func);

    // 2. Generate native machine code using Adesh's X86_64Backend
    let mut backend = X86_64Backend::new(target);
    let (code, relocs) = backend
        .generate_function_with_relocations(&module.functions[0])
        .expect("codegen failed");

    assert!(!code.is_empty(), "Generated machine code must not be empty");
    assert!(relocs.is_empty(), "Immediate return needs zero relocations");

    // 3. Allocate writable memory, copy code, seal to executable (W^X)
    let mut mem = ExecutableMemory::allocate(code.len()).expect("allocation failed");
    unsafe {
        std::ptr::copy_nonoverlapping(code.as_ptr(), mem.ptr, code.len());
    }
    mem.seal_executable().expect("seal failed");

    // 4. Invoke the function pointer directly and verify return value!
    type MainFn = extern "C" fn() -> i64;
    let main_fn: MainFn = unsafe { std::mem::transmute(mem.as_ptr()) };

    let result = main_fn();
    assert_eq!(result, 42, "Function must return 42");
}

#[test]
fn test_poc_native_codegen_inter_function_call_relocation() {
    let target = host_target();
    let mut backend = X86_64Backend::new(target);

    // Function 1: add_seven(x: i64) -> i64
    // Windows ABI: first arg in RCX (reg 1)
    // SysV ABI: first arg in RDI (reg 7)
    let arg_reg = if cfg!(windows) { 1 } else { 7 };

    let mut add_fn = MachineFunction::new("add_seven");
    add_fn.is_exported = true;
    // mov rax, arg_reg
    add_fn.entry_block_mut().push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(arg_reg))),
    });
    // add rax, 7
    add_fn.entry_block_mut().push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(7),
    });
    add_fn.entry_block_mut().push(MachineInstruction::Return);

    let (add_code, _) = backend
        .generate_function_with_relocations(&add_fn)
        .expect("add_seven codegen failed");

    // Function 2: main() -> calls add_seven(35) -> returns 42
    let mut main_fn_ir = MachineFunction::new("main");
    main_fn_ir.is_exported = true;
    // mov arg_reg, 35
    main_fn_ir.entry_block_mut().push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(arg_reg))),
        src: MachineOperand::Immediate(35),
    });
    // call add_seven
    main_fn_ir.entry_block_mut().push(MachineInstruction::Call {
        target: MachineOperand::Symbol("add_seven".to_string()),
        num_args: 1,
    });
    main_fn_ir.entry_block_mut().push(MachineInstruction::Return);

    let (main_code, relocs) = backend
        .generate_function_with_relocations(&main_fn_ir)
        .expect("main codegen failed");

    assert_eq!(relocs.len(), 1, "Expected 1 relocation for call to add_seven");
    let reloc = &relocs[0];
    assert_eq!(reloc.symbol_name, "add_seven");
    assert_eq!(reloc.kind, RelocationKind::PcRelative32);

    // Place both functions in one contiguous in-memory code block:
    // [ add_seven code ... ] [ main code ... ]
    let add_offset = 0usize;
    let main_offset = add_code.len();
    let total_size = add_code.len() + main_code.len();

    let mut mem = ExecutableMemory::allocate(total_size).expect("alloc failed");
    unsafe {
        // Copy add_seven
        std::ptr::copy_nonoverlapping(add_code.as_ptr(), mem.ptr.add(add_offset), add_code.len());
        // Copy main
        std::ptr::copy_nonoverlapping(main_code.as_ptr(), mem.ptr.add(main_offset), main_code.len());

        // Resolve PcRelative32 relocation:
        // Relocation offset is relative to main_code start.
        // Formula: S + A - P
        // S = (mem.ptr + add_offset) as i64
        // P = (mem.ptr + main_offset + reloc.offset) as i64
        // A = reloc.addend (which is -4 for x86-64 call_rel32)
        let s = (mem.ptr.add(add_offset) as usize) as i64;
        let p = (mem.ptr.add(main_offset).add(reloc.offset as usize) as usize) as i64;
        let a = reloc.addend;
        let disp32 = (s + a - p) as i32;

        let patch_loc = mem.ptr.add(main_offset).add(reloc.offset as usize) as *mut i32;
        patch_loc.write_unaligned(disp32);
    }

    mem.seal_executable().expect("seal failed");

    // Invoke main!
    type MainFn = extern "C" fn() -> i64;
    let main_fn: MainFn = unsafe { std::mem::transmute(mem.ptr.add(main_offset)) };

    let result = main_fn();
    assert_eq!(result, 42, "main calling add_seven(35) must return 42");
}
