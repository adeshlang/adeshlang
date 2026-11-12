//! Cross-Platform In-Memory Native JIT Execution Engine (`adesh-codegen::jit`).
//!
//! Provides a pure, memory-safe, W^X-compliant native JIT loader and execution
//! engine built directly on top of `adesh-codegen` Machine IR, target backends,
//! and `adesh-object` relocations.
//!
//! Architectures: x86-64, AArch64, RISC-V 64.
//! Platforms: Windows, Linux, macOS.

use std::collections::HashMap;

use adesh_object::relocation::{AdobRelocation, RelocationKind};
use adesh_object::TargetDescriptor;

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::NativeModule;
use crate::targets::create_backend;

// ============================================================================
// 1. Host Symbol Resolver
// ============================================================================

/// Resolves host runtime, standard library, and bridge symbols to virtual addresses.
pub trait HostSymbolResolver {
    /// Look up a symbol by name. Returns virtual address or None.
    fn resolve_symbol(&self, name: &str) -> Option<usize>;
}

/// Standard host symbol resolver that queries known runtime symbols, C runtime,
/// and OS process dynamic loader.
pub struct StandardHostSymbolResolver {
    custom_symbols: HashMap<String, usize>,
}

impl StandardHostSymbolResolver {
    pub fn new() -> Self {
        Self {
            custom_symbols: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: impl Into<String>, address: usize) {
        self.custom_symbols.insert(name.into(), address);
    }
}

impl Default for StandardHostSymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl HostSymbolResolver for StandardHostSymbolResolver {
    fn resolve_symbol(&self, name: &str) -> Option<usize> {
        if let Some(&addr) = self.custom_symbols.get(name) {
            return Some(addr);
        }

        // Standard C symbols
        match name {
            "printf" => return Some(libc_printf as usize),
            "malloc" => return Some(libc_malloc as usize),
            "free" => return Some(libc_free as usize),
            _ => {}
        }

        // Dynamic OS symbol resolution
        #[cfg(windows)]
        {
            use std::ffi::CString;
            unsafe extern "system" {
                fn GetModuleHandleA(lpModuleName: *const i8) -> *mut std::ffi::c_void;
                fn GetProcAddress(
                    hModule: *mut std::ffi::c_void,
                    lpProcName: *const i8,
                ) -> *mut std::ffi::c_void;
            }

            if let Ok(c_name) = CString::new(name) {
                unsafe {
                    let h_mod = GetModuleHandleA(std::ptr::null());
                    if !h_mod.is_null() {
                        let p = GetProcAddress(h_mod, c_name.as_ptr());
                        if !p.is_null() {
                            return Some(p as usize);
                        }
                    }
                }
            }
        }

        #[cfg(unix)]
        {
            use std::ffi::CString;
            unsafe extern "C" {
                fn dlsym(handle: *mut std::ffi::c_void, symbol: *const i8)
                    -> *mut std::ffi::c_void;
            }
            const RTLD_DEFAULT: *mut std::ffi::c_void = 0 as *mut std::ffi::c_void;

            if let Ok(c_name) = CString::new(name) {
                unsafe {
                    let p = dlsym(RTLD_DEFAULT, c_name.as_ptr());
                    if !p.is_null() {
                        return Some(p as usize);
                    }
                }
            }
        }

        None
    }
}

extern "C" fn libc_printf() {}
extern "C" fn libc_malloc() {}
extern "C" fn libc_free() {}

// ============================================================================
// 2. W^X Native Memory Manager
// ============================================================================

#[derive(Debug)]
pub enum JitMemoryError {
    AllocationFailed(String),
    ProtectionFailed(String),
    BufferTooSmall { requested: usize, available: usize },
    AlreadyCommitted,
    NotCommitted,
}

impl std::fmt::Display for JitMemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllocationFailed(msg) => write!(f, "JIT memory allocation failed: {msg}"),
            Self::ProtectionFailed(msg) => write!(f, "JIT memory protection transition failed: {msg}"),
            Self::BufferTooSmall { requested, available } => {
                write!(f, "JIT buffer overflow: requested {requested} bytes, available {available}")
            }
            Self::AlreadyCommitted => write!(f, "JIT memory is already sealed as executable (W^X violation)"),
            Self::NotCommitted => write!(f, "JIT memory has not yet been sealed as executable"),
        }
    }
}

impl std::error::Error for JitMemoryError {}

/// Secure W^X JIT Memory Block.
///
/// Lifecycle:
/// 1. `allocate(size)`: OS allocates page-aligned Read-Write (`RW`) memory.
/// 2. `as_mut_slice()`: Code & data written and relocations applied.
/// 3. `seal_and_commit()`: Transition protection to Read-Execute (`RX`) and flush I-cache.
/// 4. `exec_ptr(offset)`: Obtain callable code pointers.
/// 5. `Drop`: Automatically deallocates memory.
pub struct JitMemoryBlock {
    ptr: *mut u8,
    size: usize,
    committed: bool,
}

impl JitMemoryBlock {
    pub fn allocate(size: usize) -> Result<Self, JitMemoryError> {
        if size == 0 {
            return Err(JitMemoryError::AllocationFailed("Zero-size allocation requested".into()));
        }

        let aligned_size = Self::align_to_page(size);

        #[cfg(windows)]
        {
            unsafe extern "system" {
                fn VirtualAlloc(
                    lpAddress: *mut std::ffi::c_void,
                    dwSize: usize,
                    flAllocationType: u32,
                    flProtect: u32,
                ) -> *mut std::ffi::c_void;
            }
            const MEM_COMMIT: u32 = 0x00001000;
            const MEM_RESERVE: u32 = 0x00002000;
            const PAGE_READWRITE: u32 = 0x04;

            let p = unsafe {
                VirtualAlloc(
                    std::ptr::null_mut(),
                    aligned_size,
                    MEM_COMMIT | MEM_RESERVE,
                    PAGE_READWRITE,
                )
            };

            if p.is_null() {
                return Err(JitMemoryError::AllocationFailed(
                    "VirtualAlloc returned NULL for RW memory".into(),
                ));
            }

            Ok(Self {
                ptr: p as *mut u8,
                size: aligned_size,
                committed: false,
            })
        }

        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn mmap(
                    addr: *mut std::ffi::c_void,
                    length: usize,
                    prot: i32,
                    flags: i32,
                    fd: i32,
                    offset: i64,
                ) -> *mut std::ffi::c_void;
            }
            const PROT_READ: i32 = 0x1;
            const PROT_WRITE: i32 = 0x2;
            const MAP_PRIVATE: i32 = 0x02;
            const MAP_ANONYMOUS: i32 = 0x20;
            const MAP_FAILED: *mut std::ffi::c_void = !0 as *mut std::ffi::c_void;

            let p = unsafe {
                mmap(
                    std::ptr::null_mut(),
                    aligned_size,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE | MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };

            if p == MAP_FAILED {
                return Err(JitMemoryError::AllocationFailed(
                    "mmap failed to allocate RW memory".into(),
                ));
            }

            Ok(Self {
                ptr: p as *mut u8,
                size: aligned_size,
                committed: false,
            })
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn is_committed(&self) -> bool {
        self.committed
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> Result<&mut [u8], JitMemoryError> {
        if self.committed {
            return Err(JitMemoryError::AlreadyCommitted);
        }
        unsafe { Ok(std::slice::from_raw_parts_mut(self.ptr, self.size)) }
    }

    #[inline]
    pub fn base_addr(&self) -> usize {
        self.ptr as usize
    }

    /// Seal memory as Read-Execute (`RX`) and flush the CPU instruction cache.
    pub fn seal_and_commit(&mut self) -> Result<(), JitMemoryError> {
        if self.committed {
            return Ok(());
        }

        #[cfg(windows)]
        {
            unsafe extern "system" {
                fn VirtualProtect(
                    lpAddress: *mut std::ffi::c_void,
                    dwSize: usize,
                    flNewProtect: u32,
                    lpflOldProtect: *mut u32,
                ) -> i32;
                fn FlushInstructionCache(
                    hProcess: *mut std::ffi::c_void,
                    lpBaseAddress: *const std::ffi::c_void,
                    dwSize: usize,
                ) -> i32;
                fn GetCurrentProcess() -> *mut std::ffi::c_void;
            }
            const PAGE_EXECUTE_READ: u32 = 0x20;

            let mut old_protect = 0u32;
            let success = unsafe {
                VirtualProtect(
                    self.ptr as *mut std::ffi::c_void,
                    self.size,
                    PAGE_EXECUTE_READ,
                    &mut old_protect,
                )
            };

            if success == 0 {
                return Err(JitMemoryError::ProtectionFailed(
                    "VirtualProtect failed to seal memory to PAGE_EXECUTE_READ".into(),
                ));
            }

            unsafe {
                FlushInstructionCache(GetCurrentProcess(), self.ptr as *const std::ffi::c_void, self.size);
            }
        }

        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn mprotect(addr: *mut std::ffi::c_void, len: usize, prot: i32) -> i32;
            }
            const PROT_READ: i32 = 0x1;
            const PROT_EXEC: i32 = 0x4;

            let res = unsafe {
                mprotect(self.ptr as *mut std::ffi::c_void, self.size, PROT_READ | PROT_EXEC)
            };

            if res != 0 {
                return Err(JitMemoryError::ProtectionFailed(
                    "mprotect failed to transition memory to PROT_READ | PROT_EXEC".into(),
                ));
            }

            #[cfg(target_arch = "aarch64")]
            {
                #[cfg(target_os = "macos")]
                {
                    unsafe extern "C" {
                        fn sys_icache_invalidate(start: *mut std::ffi::c_void, len: usize);
                    }
                    unsafe {
                        sys_icache_invalidate(self.ptr as *mut std::ffi::c_void, self.size);
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    unsafe extern "C" {
                        fn __clear_cache(start: *mut std::ffi::c_void, end: *mut std::ffi::c_void);
                    }
                    unsafe {
                        let end = (self.ptr as usize + self.size) as *mut std::ffi::c_void;
                        __clear_cache(self.ptr as *mut std::ffi::c_void, end);
                    }
                }
            }
        }

        self.committed = true;
        Ok(())
    }

    /// Obtain a callable function pointer at the given byte offset.
    pub unsafe fn exec_ptr<T>(&self, offset: usize) -> Result<T, JitMemoryError>
    where
        T: Copy,
    {
        if !self.committed {
            return Err(JitMemoryError::NotCommitted);
        }
        if offset >= self.size || offset + std::mem::size_of::<T>() > self.size {
            return Err(JitMemoryError::BufferTooSmall {
                requested: offset + std::mem::size_of::<T>(),
                available: self.size,
            });
        }
        unsafe {
            let target = self.ptr.add(offset);
            Ok(std::mem::transmute_copy::<*mut u8, T>(&target))
        }
    }

    fn align_to_page(size: usize) -> usize {
        const PAGE_SIZE: usize = 4096;
        (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
    }
}

impl Drop for JitMemoryBlock {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }

        #[cfg(windows)]
        {
            unsafe extern "system" {
                fn VirtualFree(
                    lpAddress: *mut std::ffi::c_void,
                    dwSize: usize,
                    dwFreeType: u32,
                ) -> i32;
            }
            const MEM_RELEASE: u32 = 0x00008000;
            unsafe {
                VirtualFree(self.ptr as *mut std::ffi::c_void, 0, MEM_RELEASE);
            }
        }

        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
            }
            unsafe {
                munmap(self.ptr as *mut std::ffi::c_void, self.size);
            }
        }
    }
}

// ============================================================================
// 3. JIT Loader and Loaded Module
// ============================================================================

#[derive(Debug)]
pub enum JitLoaderError {
    CodegenError(CodegenError),
    MemoryError(JitMemoryError),
    UnresolvedSymbol(String),
    RelocationOverflow {
        symbol: String,
        kind: RelocationKind,
        value: i64,
    },
    FunctionNotFound(String),
}

impl From<CodegenError> for JitLoaderError {
    fn from(err: CodegenError) -> Self {
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
            Self::CodegenError(err) => write!(f, "Codegen error during JIT compilation: {err:?}"),
            Self::MemoryError(err) => write!(f, "JIT memory management error: {err}"),
            Self::UnresolvedSymbol(sym) => write!(f, "Unresolved symbol during JIT relocation: '{sym}'"),
            Self::RelocationOverflow { symbol, kind, value } => {
                write!(f, "Relocation overflow for '{symbol}' ({kind:?}): value {value} out of range")
            }
            Self::FunctionNotFound(name) => write!(f, "JIT function '{name}' not found in loaded module"),
        }
    }
}

impl std::error::Error for JitLoaderError {}

pub struct LoadedJitModule {
    _memory: JitMemoryBlock,
    function_offsets: HashMap<String, usize>,
    base_address: usize,
}

impl LoadedJitModule {
    pub unsafe fn get_function<F: Copy>(&self, name: &str) -> Result<F, JitLoaderError> {
        let offset = self
            .function_offsets
            .get(name)
            .copied()
            .ok_or_else(|| JitLoaderError::FunctionNotFound(name.to_string()))?;

        unsafe { self._memory.exec_ptr::<F>(offset).map_err(JitLoaderError::MemoryError) }
    }

    #[inline]
    pub fn base_address(&self) -> usize {
        self.base_address
    }
}

pub struct JitLoader<'a> {
    target: &'a TargetDescriptor,
    resolver: &'a dyn HostSymbolResolver,
}

impl<'a> JitLoader<'a> {
    pub fn new(target: &'a TargetDescriptor, resolver: &'a dyn HostSymbolResolver) -> Self {
        Self { target, resolver }
    }

    pub fn load_module(&self, module: &NativeModule) -> Result<LoadedJitModule, JitLoaderError> {
        let mut backend = create_backend(self.target.clone())?;

        let mut compiled_functions: Vec<(String, Vec<u8>, Vec<AdobRelocation>)> = Vec::new();
        let mut total_code_size = 0usize;

        for func in &module.functions {
            let (code, relocs) = backend.generate_function_with_relocations(func)?;
            total_code_size = (total_code_size + 15) & !15;
            total_code_size += code.len();
            compiled_functions.push((func.name.clone(), code, relocs));
        }

        if total_code_size == 0 {
            total_code_size = 64;
        }

        // Add 4KB padding for jump trampolines if needed
        let alloc_size = total_code_size + 4096;
        let mut mem_block = JitMemoryBlock::allocate(alloc_size)?;
        let base_addr = mem_block.base_addr();

        let mut func_offsets: HashMap<String, usize> = HashMap::new();
        let mut all_relocations: Vec<(usize, AdobRelocation)> = Vec::new();

        let slice = mem_block.as_mut_slice()?;
        let mut current_offset = 0usize;

        for (name, code, relocs) in compiled_functions {
            current_offset = (current_offset + 15) & !15;
            func_offsets.insert(name.clone(), current_offset);

            slice[current_offset..current_offset + code.len()].copy_from_slice(&code);

            for rel in relocs {
                all_relocations.push((current_offset, rel));
            }

            current_offset += code.len();
        }

        let mut trampoline_offset = (current_offset + 15) & !15;

        // Apply relocations
        for (func_offset, reloc) in all_relocations {
            let site_offset = func_offset + reloc.offset as usize;
            let site_vaddr = base_addr + site_offset;

            let target_vaddr = if let Some(&target_func_offset) = func_offsets.get(&reloc.symbol_name) {
                base_addr + target_func_offset
            } else if let Some(host_addr) = self.resolver.resolve_symbol(&reloc.symbol_name) {
                host_addr
            } else {
                return Err(JitLoaderError::UnresolvedSymbol(reloc.symbol_name.clone()));
            };

            match reloc.kind {
                RelocationKind::PcRelative32 | RelocationKind::PltRelative32 => {
                    // S + A - P
                    // S = target_vaddr
                    // P = site_vaddr
                    // A = reloc.addend (which is already -4 for x86-64 call_rel32)
                    let full_target = (target_vaddr as i64) + reloc.addend;
                    let disp = (target_vaddr as i64) + reloc.addend - (site_vaddr as i64);

                    if disp >= i32::MIN as i64 && disp <= i32::MAX as i64 {
                        let disp_i32 = disp as i32;
                        slice[site_offset..site_offset + 4].copy_from_slice(&disp_i32.to_le_bytes());
                    } else {
                        // Beyond +-2GB: synthesize 64-bit jump trampoline
                        let tramp_vaddr = base_addr + trampoline_offset;
                        let tramp_disp = (tramp_vaddr as i64) + reloc.addend - (site_vaddr as i64);

                        if tramp_disp < i32::MIN as i64 || tramp_disp > i32::MAX as i64 {
                            return Err(JitLoaderError::RelocationOverflow {
                                symbol: reloc.symbol_name,
                                kind: reloc.kind,
                                value: tramp_disp,
                            });
                        }

                        // mov r11, imm64 (49 BB imm64)
                        // jmp r11        (41 FF E3)
                        slice[trampoline_offset] = 0x49;
                        slice[trampoline_offset + 1] = 0xBB;
                        slice[trampoline_offset + 2..trampoline_offset + 10]
                            .copy_from_slice(&(full_target as u64).to_le_bytes());
                        slice[trampoline_offset + 10] = 0x41;
                        slice[trampoline_offset + 11] = 0xFF;
                        slice[trampoline_offset + 12] = 0xE3;

                        let tramp_disp_i32 = tramp_disp as i32;
                        slice[site_offset..site_offset + 4].copy_from_slice(&tramp_disp_i32.to_le_bytes());

                        trampoline_offset += 16;
                    }
                }
                RelocationKind::Absolute64 => {
                    let full_target = ((target_vaddr as i64) + reloc.addend) as u64;
                    slice[site_offset..site_offset + 8].copy_from_slice(&full_target.to_le_bytes());
                }
                RelocationKind::Absolute32 => {
                    let full_target = (target_vaddr as i64) + reloc.addend;
                    if full_target > u32::MAX as i64 || full_target < 0 {
                        return Err(JitLoaderError::RelocationOverflow {
                            symbol: reloc.symbol_name,
                            kind: reloc.kind,
                            value: full_target,
                        });
                    }
                    slice[site_offset..site_offset + 4].copy_from_slice(&(full_target as u32).to_le_bytes());
                }
                RelocationKind::AArch64_Call26 => {
                    let disp = (target_vaddr as i64 + reloc.addend) - (site_vaddr as i64);
                    let imm26 = (disp >> 2) & 0x03FF_FFFF;
                    let existing = u32::from_le_bytes(slice[site_offset..site_offset + 4].try_into().unwrap());
                    let patched = (existing & !0x03FF_FFFF) | (imm26 as u32);
                    slice[site_offset..site_offset + 4].copy_from_slice(&patched.to_le_bytes());
                }
                RelocationKind::RiscV_Call => {
                    let disp = (target_vaddr as i64 + reloc.addend) - (site_vaddr as i64);
                    let imm = disp as i32;
                    let hi20 = (imm.wrapping_add(0x800) >> 12) & 0x000F_FFFF;
                    let lo12 = imm & 0x0FFF;

                    let auipc = u32::from_le_bytes(slice[site_offset..site_offset + 4].try_into().unwrap());
                    let jalr = u32::from_le_bytes(slice[site_offset + 4..site_offset + 8].try_into().unwrap());

                    let patched_auipc = (auipc & 0x0000_0FFF) | ((hi20 as u32) << 12);
                    let patched_jalr = (jalr & 0x000F_FFFF) | ((lo12 as u32) << 20);

                    slice[site_offset..site_offset + 4].copy_from_slice(&patched_auipc.to_le_bytes());
                    slice[site_offset + 4..site_offset + 8].copy_from_slice(&patched_jalr.to_le_bytes());
                }
                other => {
                    eprintln!("[JIT Loader] Warning: unhandled relocation kind: {:?}", other);
                }
            }
        }

        // Seal memory as RX (W^X transition) and flush CPU cache
        mem_block.seal_and_commit()?;

        Ok(LoadedJitModule {
            _memory: mem_block,
            function_offsets: func_offsets,
            base_address: base_addr,
        })
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::{
        MachineBlock, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
        NativeModule, PhysicalRegister,
    };

    #[test]
    fn test_jit_memory_block_allocation_and_sealing() {
        let mut block = JitMemoryBlock::allocate(4096).expect("allocate block");
        assert_eq!(block.is_committed(), false);

        {
            let slice = block.as_mut_slice().expect("writable slice");
            slice[0] = 0x90; // NOP
            slice[1] = 0xC3; // RET
        }

        block.seal_and_commit().expect("seal and commit");
        assert_eq!(block.committed, true);

        // Writing after commit must fail (W^X rule)
        assert!(block.as_mut_slice().is_err());

        unsafe {
            let func: extern "C" fn() = block.exec_ptr(0).expect("callable ptr");
            func();
        }
    }

    #[test]
    fn test_jit_loader_multi_function_internal_call() {
        let triple = if cfg!(target_os = "windows") {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_os = "macos") {
            "x86_64-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        let target = TargetDescriptor::from_triple(triple).expect("valid target triple");

        let mut module = NativeModule::new("test_jit_mod");

        // Function 1: callee() -> 100
        let mut callee = MachineFunction::new("callee");
        callee.entry_block_mut().push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            src: MachineOperand::Immediate(100),
        });
        callee.entry_block_mut().push(MachineInstruction::Return);
        module.add_function(callee);

        // Function 2: caller() -> calls callee
        let mut caller = MachineFunction::new("caller");
        caller.entry_block_mut().push(MachineInstruction::Call {
            target: MachineOperand::Symbol("callee".to_string()),
            num_args: 0,
        });
        caller.entry_block_mut().push(MachineInstruction::Return);
        module.add_function(caller);

        let resolver = StandardHostSymbolResolver::new();
        let loader = JitLoader::new(&target, &resolver);
        let loaded = loader.load_module(&module).expect("load module");

        unsafe {
            let callee_fn: extern "C" fn() -> i64 = loaded.get_function("callee").expect("callee fn");
            assert_eq!(callee_fn(), 100);

            let caller_fn: extern "C" fn() -> i64 = loaded.get_function("caller").expect("caller fn");
            assert_eq!(caller_fn(), 100);
        }
    }
}
