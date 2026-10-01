# Adesh Native Production Toolchain — Master Implementation Plan

**Generated:** 2026-10-01  
**Architecture:** Self-Contained Native Multi-Target Compiler & Linker Pipeline

---

## Roadmap Overview

```text
Phase 1: Direct MIR -> Machine IR Lowering & Full Native Instruction Emission
    ↓
Phase 2: ABI Subsystem & Calling Convention Robustness (SysV, MS x64, AAPCS64, RV64)
    ↓
Phase 3: Linker Enhancements (Full DLL/SO/Dylib Generation, TLS, DWARF/CodeView)
    ↓
Phase 4: Modular Zero-Dependency Native Runtime & Allocator Subsystems
    ↓
Phase 5: LTO & ThinLTO with ADOB Bitcode Container Integration
    ↓
Phase 6: Native Quantum (AQIR -> Hardware/Simulator) & Heterogeneous GPU Pipelines
    ↓
Phase 7: Self-Hosting Verification & Deterministic Golden Testing
```

---

## Phase 1: Direct MIR -> Machine IR Lowering & Native Codegen

### Objectives:
1. Implement a complete MIR lowering engine in `crates/adesh-codegen/src/lowering/` that transforms `src/ir/mir/` structures into `NativeModule` and `MachineFunction`.
2. Map all MIR instructions:
   - Arithmetic / Logic / Comparison / Bitwise operations.
   - Structured branches (`BranchCc`, `Jump`, `SwitchTable`).
   - Local variable stack slots and spill area offsets.
   - Function calls with caller/callee-saved register management.
   - Runtime C ABI function calls (`adesh_rt_alloc`, `adesh_rt_print`, `adesh_rt_free`, `adesh_rt_retain`).
3. Connect `--emit=adob` and `--emit=exe` in `src/cli/build.rs` to route through native lowering directly to `adesh-linker`.

---

## Phase 2: Production ABI Subsystem

### Objectives:
1. Implement precise parameter classification across all 4 primary target ABIs:
   - **System V AMD64**: Integer/pointer in `rdi, rsi, rdx, rcx, r8, r9`; Float in `xmm0-xmm7`; stack passing with 16-byte alignment.
   - **Microsoft x64**: 4-register fastcall `rcx, rdx, r8, r9` (or `xmm0-xmm3`), 32-byte shadow stack allocation in caller frame.
   - **AAPCS64 (AArch64)**: General registers `x0-x7`, vector/FP `v0-v7`, 16-byte aligned stack.
   - **RISC-V (RV64/RV32)**: Arguments `a0-a7`, FP `fa0-fa7`.
2. Stack frame epilogue and prologue generation with proper SEH unwind codes (for Windows x64 `.pdata`/`.xdata`) and DWARF CIE/FDE generation.
3. Struct and aggregate passing rules (by-value in registers if $\le 16$ bytes, by-reference hidden pointer otherwise).

---

## Phase 3: Linker Dynamic Linking & Debug Generation

### Objectives:
1. **Dynamic Shared Libraries**:
   - Windows PE DLL: synthesize Export Address Table (EAT), Export Name Table (ENT), Export Ordinal Table (EOT), and base `.reloc` table.
   - Linux ELF Shared Object (`.so`): synthesize `DT_SONAME`, `DT_NEEDED`, `.dynsym`, `.dynstr`, `.hash`, `.gnu.hash`, `.plt`, and `.got`.
   - macOS Mach-O (`.dylib`): synthesize `LC_ID_DYLIB`, export trie, and dynamic rebase/bind streams.
2. **Thread-Local Storage (TLS)**:
   - Generate PE TLS Directory (`IMAGE_TLS_DIRECTORY64`) pointing to `_tls_start`, `_tls_end`, `_tls_index`, `_tls_callbacks`.
   - ELF `PT_TLS` segment with `R_X86_64_TPOFF32` / `R_AARCH64_TLSLE_ADD_TPREL_HI12`.
3. **Debug Info**:
   - Emit DWARF 5 `.debug_info`, `.debug_abbrev`, `.debug_line`, `.debug_str` for Linux/macOS.
   - Emit CodeView symbol stream for Windows PDB generation.

---

## Phase 4: Runtime Subsystems & OS Abstraction

### Objectives:
1. **Modular Allocator Layer**:
   - `SystemAllocator` (defaulting to OS VirtualAlloc/mmap).
   - `ArenaAllocator` / `BumpAllocator` for short-lived scopes.
   - `PoolAllocator` for fixed-size object caching.
2. **Platform Threading & Concurrency**:
   - Windows: `CreateThread`, `WaitOnAddress`, `WakeByAddressSingle`.
   - Linux: `pthread_create`, `futex` syscalls.
   - macOS: `pthread` & GCD QoS hooks.
3. **Zero-Dependency Event Loop**:
   - Epoll/IOCP/kqueue unified reactor trait.

---

## Phase 5: Link-Time Optimization (LTO)

### Objectives:
1. Define ADOB section `.adesh_ir` holding serialized MIR bytecode and symbol call graphs.
2. When `-O3 --lto` or `--lto=thin` is supplied:
   - Linker collects `.adesh_ir` sections across all input ADOB objects and static archives.
   - Runs cross-module inlining, devirtualization, dead function removal, and global value propagation.
   - Invokes target backend to generate optimized final machine code.

---

## Phase 6: Quantum & Heterogeneous Accelerator Pipeline

### Objectives:
1. **Language-Level Quantum Integration**:
   - Parse and type-check `qubit`, `qubit[n]`, `measure()`, and quantum gates.
   - Generate AQIR instructions in MIR.
   - In JIT/Simulator mode: route directly to `StateVectorSimulator`.
   - In AOT mode: emit OpenQASM 3.0 / QIR package into ADOB `.quantum_pkg` section.
2. **GPU / Tensor Pipeline**:
   - SPIR-V code generator for compute kernels.
   - Runtime CUDA / Vulkan Compute dispatch bridge.

---

## Phase 7: Self-Hosting & Verification

### Objectives:
1. Golden test suite: compile all `examples/` through the native pipeline and execute them on the host.
2. Automated binary validation: check headers, section alignments, relocations, import tables, and permission flags.
3. Eliminate bootstrap dependencies progressively until Adesh compiles Adesh natively.
