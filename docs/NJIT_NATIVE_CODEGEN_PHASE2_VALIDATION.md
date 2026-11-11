# AdeshLang NJIT Migration Audit — Phase 2: Deep Technical Validation

**Author:** Principal Compiler Engineer & Systems Architect  
**Status:** Deep Investigation Complete — Evidence-Based Assessment  
**Date:** 2026-11-11  
**Scope:** `crates/adesh-codegen`, `crates/adesh-object`, `crates/adesh-runtime`, `linker`, `src/backends/native`, `src/backends/jit/native`

---

## 1. Verification of Previous Findings

We revisit every critical finding and architectural assumption from the Phase 1 audit and validate it strictly against the source code.

| # | Previous Finding / Claim | Verdict | Evidence / Source Reference | Technical Evaluation & Risk |
|---|---|---|---|---|
| 1 | **AOT HIR → Machine IR lowering is reusable by NJIT** | **CONFIRMED** | `src/backends/native/lower.rs:8577-8596` (`lower_hir_module`) | `lower_hir_module` accepts any typed `HirModule` and produces a clean `NativeModule` without requiring disk I/O or link-time assumptions. It lowers expressions, control flow, defers, lambdas, and handles cleanly. |
| 2 | **Native Codegen generates position-independent code (PIC) for JIT** | **PARTIALLY CONFIRMED** | `crates/adesh-codegen/src/targets/x86_64/mod.rs:1688-1720` | On Mach-O, `lea reg, [rip + disp32]` with `PcRelative32` is emitted (pure PIC). On Windows PE and Linux ELF, `mov reg, imm64` with `Absolute64` is currently emitted for global symbol address materialization. For JIT in-memory loading, `Absolute64` can be patched directly to the 64-bit host memory pointer! However, `call_rel32` (`RelocationKind::PcRelative32`) requires that the callee and caller be allocated within a 2GB ($\pm 2\text{GB}$) window, or use indirect register calls (`call r11` / trampolines). |
| 3 | **Existing relocation logic can be reused for in-memory relocation** | **CONFIRMED** | `crates/adesh-object/src/relocation.rs:83-115`, `linker/src/relocation.rs:412-580` | `linker/src/relocation.rs` contains exact formulas for `Absolute64` (`S + A`), `Absolute32` (`(S + A) as u32`), and `PcRelative32` (`S + A - P`). The mathematical implementation is identical for an in-memory slice `&mut [u8]` at host virtual address `P`. |
| 4 | **ADOB can represent everything required by the JIT loader** | **CONFIRMED** | `crates/adesh-object/src/format.rs`, `relocation.rs`, `symbol.rs` | `AdobObject` contains sections (`.text`, `.rodata`, `.data`), symbol tables (`AdobSymbol`), and relocations (`AdobRelocation`). It can be passed directly in-memory without disk serialization. |
| 5 | **AArch64 and RISC-V backends have sufficient functionality for JIT** | **PARTIALLY CONFIRMED** | `crates/adesh-codegen/src/targets/aarch64/mod.rs`, `crates/adesh-codegen/src/targets/riscv/mod.rs` | Both targets implement `generate_function_with_relocations` and emit valid instructions (AAPCS64 and RV64GC). However, full end-to-end runtime library coverage and test harnesses for ARM64/RISC-V are currently secondary to x86-64 in CI. |
| 6 | **Existing runtime memory allocation can be adapted to secure $W \oplus X$** | **CONFIRMED (With Refactoring)** | `crates/adesh-runtime/src/platform/windows/mod.rs:37-67` | Windows `virtual_alloc` currently uses `PAGE_EXECUTE_READWRITE` directly (W^X violation). It must be refactored into a two-phase allocator: `VirtualAlloc(PAGE_READWRITE)` -> write code & patch relocations -> `VirtualProtect(PAGE_EXECUTE_READ)`. Linux/macOS requires equivalent `mmap(PROT_READ | PROT_WRITE)` -> `mprotect(PROT_READ | PROT_EXEC)`. |
| 7 | **Runtime bridges can be reused without semantic or ABI changes** | **CONFIRMED** | `src/backends/jit/native/runtime_bridge.rs:21-140` | `runtime_bridge.rs` exports standard `extern "C"` functions taking raw integers, pointers, and 64-bit tagged handles (`HANDLE_TAG = 0x4000_0000_0000_0000`). `adesh-codegen`'s calling convention lowerer conforms to standard C ABI. |
| 8 | **Existing ABI lowering works for both AOT and JIT execution** | **CONFIRMED** | `crates/adesh-codegen/src/calling_convention/mod.rs`, `src/abi/win64.rs`, `src/abi/sysv64.rs` | Calling conventions (`WindowsX64CallingConvention`, `SystemVX64CallingConvention`, `Aapcs64CallingConvention`) adhere strictly to platform ABIs including shadow space (32 bytes on Win64), argument registers, and stack alignment (16 bytes). |
| 9 | **Machine IR supports all operations required by NJIT** | **CONFIRMED** | `crates/adesh-codegen/src/machine_ir/mod.rs:40-210` | Supports arithmetic, logic, floating point (SSE2/AVX), control flow (`BranchCc`), calls (`Call`), atomic operations, vector instructions, stack slots, and parallel moves. |

---

## 2. Deep Relocation and Object-Loading Investigation

### 2.1 Trace of the Relocation Pipeline

```
Machine IR (MachineInstruction::Call / Move)
      │
      ▼
Target Encoder (crates/adesh-codegen/src/targets/x86_64/mod.rs)
   - Emits opcode bytes (e.g., E8 00 00 00 00 for call rel32)
   - Records AdobRelocation { offset, symbol_name, kind, addend: -4 }
      │
      ▼
In-Memory AdobObject / FunctionCodeWithRelocs
   - Section .text: raw bytes
   - Relocations: Vec<AdobRelocation>
      │
      ▼
JitLoader (In-Memory Execution Destination)
   - Memory allocation: Allocates page at host virtual address `P_base`
   - Symbol resolution: Resolves `symbol_name` to host virtual address `S`
   - Relocation calculation:
       * For Absolute64: `*ptr = S + A`
       * For PcRelative32: `*ptr = (S + A - (P_base + offset)) as i32`
   - Page sealing: W^X transition to RX
   - I-Cache Flush: Ensure hardware coherence
```

### 2.2 Relocation Kinds Relevant for In-Memory JIT

| Relocation Kind (`adesh-object`) | Width | Formula | Implementation in Linker | JIT In-Memory Applicability |
|---|---|---|---|---|
| `RelocationKind::Absolute64` | 8 bytes | $S + A$ | `linker/src/relocation.rs:417-424` | **Trivial & Direct:** Write 64-bit absolute host pointer directly into code/data buffer. |
| `RelocationKind::PcRelative32` | 4 bytes | $S + A - P$ | `linker/src/relocation.rs:525-548` | **Direct with Range Check:** $P = \text{buffer\_vaddr} + \text{offset}$. Displacement must fit in signed `i32` ($\pm 2\text{GB}$). |
| `RelocationKind::PltRelative32` | 4 bytes | $L + A - P$ | `linker/src/relocation.rs:526` | **Handled as Direct Call or Trampoline:** If target is within 2GB, bind directly. If $>2\text{GB}$, bind to an in-memory PLT trampoline stub. |
| `RelocationKind::AArch64_Call26` | 4 bytes | $(S + A - P) \gg 2$ | `linker/src/relocation.rs:600-612` | Uncond branch $\pm 128\text{MB}$. Relocates `BL` / `B` instruction. |
| `RelocationKind::AArch64_AdrPage21` + `AddAbsLo12` | 8 bytes | `ADRP` + `ADD` page calculation | `linker/src/relocation.rs:613-640` | Computes $\pm 4\text{GB}$ page-relative address + 12-bit offset for constants. |

### 2.3 The 2GB Branch Range Challenge in 64-bit JIT

On 64-bit platforms (x86-64 and AArch64), `call rel32` uses a 32-bit signed displacement ($\pm 2\text{GB}$). 
In an operating system process:
- Host runtime code and C-runtime functions (`printf`, `malloc`, `adesh_rt_*`) usually reside in executable address space or loaded shared libraries (DLLs/so/dylib).
- If `VirtualAlloc` or `mmap` allocates JIT memory far away from the host executable binary ($> 2\text{GB}$ delta), a 32-bit relative displacement will **overflow**.

#### Solution:
1. **Near Allocation Strategy:** Pass a preferred base address to `VirtualAlloc` or `mmap` with `MAP_32BIT` (on Linux) or probe pages within $\pm 2\text{GB}$ of the host binary.
2. **JIT Trampoline Table (PLT):** When an external function is $> 2\text{GB}$ away, the JIT allocator places a small 14-byte 64-bit indirect jump trampoline in the local JIT code page:
   ```nasm
   ; x86-64 64-bit Trampoline
   mov r11, 0x1122334455667788  ; 49 BB imm64
   jmp r11                      ; 41 FF E3
   ```
   The `call rel32` targets this local trampoline, which never overflows!

---

## 3. JIT Memory Architecture & $W \oplus X$ Security

### 3.1 Security Requirements Across Operating Systems
Modern operating systems actively enforce $W \oplus X$ (Data Execution Prevention / DEP):
- **Windows:** Arbitrary Code Guard (ACG) forbids creating pages that are simultaneously writable and executable (`PAGE_EXECUTE_READWRITE`).
- **macOS (Apple Silicon):** Hardened Runtime and the Apple Silicon M1/M2/M3 MMU enforce that no page can be mapped `PROT_WRITE | PROT_EXEC`. macOS provides `pthread_jit_write_protect_np(0)` to switch the current thread into write mode and `pthread_jit_write_protect_np(1)` to switch into execute mode.
- **Linux:** PaX / SELinux environments disallow `PROT_EXEC` on pages that were modified without proper `mprotect` calls.

### 3.2 Concrete JIT Memory Manager Specification

```rust
// In crates/adesh-runtime/src/platform/jit_mem.rs

pub struct JitPage {
    ptr: *mut u8,
    size: usize,
    is_executable: bool,
}

impl JitPage {
    /// Allocate writable memory for code emission and relocation patching
    pub fn allocate_writable(size: usize) -> Result<Self, JitMemoryError> {
        let page_size = Self::page_size();
        let rounded_size = (size + page_size - 1) & !(page_size - 1);
        
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Memory::*;
            let ptr = VirtualAlloc(
                std::ptr::null_mut(),
                rounded_size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );
            if ptr.is_null() {
                return Err(JitMemoryError::AllocationFailed);
            }
            Ok(Self { ptr: ptr as *mut u8, size: rounded_size, is_executable: false })
        }

        #[cfg(unix)]
        unsafe {
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                rounded_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED {
                return Err(JitMemoryError::AllocationFailed);
            }
            Ok(Self { ptr: ptr as *mut u8, size: rounded_size, is_executable: false })
        }
    }

    /// Seal page to RX (Read-Execute) and flush instruction cache
    pub fn seal_executable(&mut self) -> Result<(), JitMemoryError> {
        if self.is_executable { return Ok(()); }

        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Memory::*;
            let mut old_protect = 0;
            let success = VirtualProtect(
                self.ptr as *mut _,
                self.size,
                PAGE_EXECUTE_READ,
                &mut old_protect,
            );
            if success == 0 {
                return Err(JitMemoryError::ProtectionChangeFailed);
            }
            // Flush CPU instruction cache
            use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
            use windows_sys::Win32::System::Threading::GetCurrentProcess;
            FlushInstructionCache(GetCurrentProcess(), self.ptr as *const _, self.size);
        }

        #[cfg(unix)]
        unsafe {
            let ret = libc::mprotect(
                self.ptr as *mut _,
                self.size,
                libc::PROT_READ | libc::PROT_EXEC,
            );
            if ret != 0 {
                return Err(JitMemoryError::ProtectionChangeFailed);
            }
            // Flush cache on Unix / ARM64
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
        }

        self.is_executable = true;
        Ok(())
    }
}
```

---

## 4. Suitability of `HIR → Machine IR` Lowering for JIT

### 4.1 Detailed Inspection of `src/backends/native/lower.rs`

Inspection of `src/backends/native/lower.rs` confirms:
1. **Entry Point Generation:** Lines 8560–8587 check if `main` exists. If not, it synthesizes an exported `main` that runs defers, cleans handles, sets return code `0`, and returns. This behavior matches JIT execution requirements perfectly!
2. **String Pool (.rodata):** String literals are deposited in `module.string_pool`. In AOT, `emit_object` serializes these into `.rodata`. In JIT, `string_pool` entries are simply allocated in a readable data block or as static C-strings.
3. **No Hardcoded Absolute Virtual Addresses:** `lower.rs` never emits fixed memory addresses. Everything is referred to by symbolic names (`MachineOperand::Symbol`) or virtual registers.
4. **Clean Decoupling:** `lower_hir_module` does not write files, invoke `ld`/`adeshlink`, or touch the disk. It is a pure, in-memory IR transformation:
   $$\text{HirModule} \xrightarrow{\text{lower\_hir\_module}} \text{NativeModule}$$

---

## 5. Function and Module Loading Dynamics

### 5.1 Internal & Cross-Module Function Calls
In Adesh programs, functions may call:
1. **Sibling functions** compiled in the same module.
2. **Functions in already-compiled modules** (e.g. from previously loaded imports).
3. **External runtime functions** (e.g. `adesh_rt_arc_new`, `printf`).

### 5.2 Resolution Strategy:
- **Phase 1: Ingestion.** The JIT compiler emits machine code for all functions in the module into a contiguous code block.
- **Phase 2: Local Symbol Registration.** The starting offset of each function within the code block is recorded in a local symbol map: `HashMap<String, usize>`.
- **Phase 3: Relocation Resolution.** For each `AdobRelocation`:
  - If the symbol is found in the local module, calculate `S = code_block_vaddr + local_offset`.
  - If the symbol is found in previously compiled modules or the global `RuntimeBridge`, retrieve its finalized pointer.
  - Patch the target offset using the appropriate formula (`PcRelative32` or `Absolute64`).
  - If a PC-relative call delta exceeds $2\text{GB}$, emit and link to a local trampoline.

---

## 6. End-to-End Prototype Specification (The Minimal Slice)

See [`docs/NJIT_NATIVE_CODEGEN_POC_SPEC.md`](file:///d:/Projects/AdeshLang/docs/NJIT_NATIVE_CODEGEN_POC_SPEC.md) for full implementation details.

### Test Progression for the POC:
1. **Step 1: Pure Constant Return**
   - Source: `fn main() -> int { return 42; }`
   - Validates: IR lowering, machine code emission, memory allocation, RX transition, direct function call.
2. **Step 2: Internal Function Call & Relocation**
   - Source: `fn add(a: int, b: int) -> int { return a + b; } fn main() -> int { return add(20, 22); }`
   - Validates: Inter-function branch relocation (`RelocationKind::PcRelative32`), SysV/Win64 ABI parameter passing, return value extraction.
3. **Step 3: External Runtime Bridge Call**
   - Source: `fn main() { print("Hello, Native JIT!"); }`
   - Validates: External symbol resolution (`print_str`), string constant allocation, runtime integration.

---

## 7. Reevaluated Staged Migration Roadmap

We refine the roadmap to focus on an immediate **Vertical Slice Prototype** on x86-64 before any broad refactoring.

```mermaid
flowchart LR
    P1[Stage 1: Vertical Slice POC] --> P2[Stage 2: x86-64 In-Memory Loader]
    P2 --> P3[Stage 3: Dual-Backend Flag --jit-backend=adesh]
    P3 --> P4[Stage 4: Parity Conformance Suite]
    P4 --> P5[Stage 5: Multi-Platform ARM64 / RISC-V]
    P5 --> P6[Stage 6: Cranelift Retirement]
```

### Stage 1: Vertical Slice Prototype (Week 1)
- **Goal:** Minimal test runner executing `42`, internal `add`, and `print("hello")` via `adesh-codegen` in memory.
- **Deliverable:** `tests/njit_native_codegen_poc_test.rs`.

### Stage 2: Production-Quality In-Memory JIT Loader (Week 2)
- **Goal:** Robust `JitMemoryManager` ($W \oplus X$, trampolines, cache flushing) and full `AdobRelocation` resolver.
- **Affected:** `crates/adesh-runtime`, `src/backends/jit/native/`.

### Stage 3: Dual-Backend Driver Integration (Week 3)
- **Goal:** Add `--jit-backend=adesh|cranelift` to the CLI. Route `HIR -> Machine IR -> JitLoader` when `adesh` is selected. Keep Cranelift as the default fallback.

### Stage 4: Conformance & Benchmarking (Week 4)
- **Goal:** Run all existing test suites (`native_phase7`, `native_phase8`) under `--jit-backend=adesh`. Measure compilation latency and CPU execution throughput.

### Stage 5: Multi-Platform Validation (Week 5)
- **Goal:** Enable and test Native JIT on macOS ARM64 and Linux ARM64.

### Stage 6: Cranelift Retirement (Week 6)
- **Goal:** Set `adesh` as default; remove Cranelift dependencies and ~5,880 lines of legacy code.
