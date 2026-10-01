# Adesh Native Production Toolchain — Architecture Gaps Analysis

**Generated:** 2026-10-01  
**Scope:** Gaps between Current Baseline and Self-Contained Native Production Targets

---

## 1. Compiler Frontend & IR Lowering Gaps

### 1.1 Direct Native IR Lowering (`HIR/MIR -> Machine IR`)
* **Current State:** The CLI `--emit=adob` compiles AST to HIR, but currently lowers each function using a minimal stub (`Move 0; Return`) in `src/cli/build.rs`. Full AOT builds invoke the Cranelift backend.
* **Architectural Gap:** Need an end-to-end lowering pipeline from `src/ir/mir/` (or `hir/`) into `adesh_codegen::machine_ir::NativeModule`.
* **Missing Features:**
  - Lowering of complex expressions (arithmetic, logic, bitwise, string concatenation).
  - Control flow lowering (if/else, while, loop, for-in, break, continue, match/switch).
  - Structured memory access (struct field offset get/set, array indexing, slicing).
  - Runtime C ABI call emission for heap allocations, string printing, ARC retain/release.
  - Calling convention parameter passing and return register binding.

### 1.2 Heterogeneous IR (Heterogeneous MIR -> Device Kernels)
* **Current State:** GPU/NPU/TPU modules in `crates/adesh-codegen` produce ADOB objects containing metadata and raw bytes.
* **Architectural Gap:** Need structured translation of device kernels (`@gpu`, `@kernel`) into SPIR-V or device bytecode, embedded directly into ADOB `.gpu_kernel` sections.

---

## 2. ABI & Calling Convention Gaps

### 2.1 Stack Frame & Calling Convention Parameter Classification
* **Current State:** `SystemVX64CallingConvention` and `WindowsX64CallingConvention` provide argument registers (`RDI, RSI, RDX...` vs `RCX, RDX, R8, R9`).
* **Architectural Gap:**
  - Variadic arguments (`...`) parameter promotion and spill area.
  - Floating point / Vector register argument assignment (`XMM0-XMM7` / `V0-V7`) for mixed-signature functions.
  - Large aggregate return values via hidden first-pointer argument (sret / return slot pointer).
  - Windows x64 32-byte shadow space allocation in prologue and cleanup in epilogue.

### 2.2 Thread-Local Storage (TLS) ABI
* **Current State:** ADOB format defines `.tdata` / `.tbss` and `TlsGd`/`TlsLd`/`TlsIe`/`TlsLe` relocation kinds.
* **Architectural Gap:** Linker needs to generate OS-specific TLS directories (PE TLS Directory with callbacks, ELF `PT_TLS` segment and thread pointer `%fs:0` / `%gs:0` offset calculations).

---

## 3. Native Linker Gaps

### 3.1 True IR-Level Link-Time Optimization (LTO)
* **Current State:** Linker detects `--lto` and returns a diagnostic noting IR LTO requires bitcode inputs. Sections are GC'd via `--gc-sections` and folded via `--icf`.
* **Architectural Gap:** Define an ADOB `.adesh_ir` bitcode section holding serialized MIR. During linking with `--lto`, unpack MIR modules across all input ADOB files, run interprocedural optimizations (global inlining, devirtualization, dead argument elimination), and emit machine code at link time.

### 3.2 DWARF / CodeView Debug Information
* **Current State:** Line records are stored in ADOB metadata and `.debug_line` is emitted.
* **Architectural Gap:** Complete DWARF 5 `.debug_info`, `.debug_abbrev`, and `.debug_str` generation for Linux/macOS, and CodeView PDB stream writing for Windows native debugging in WinDbg / VS.

### 3.3 Dynamic Shared Library (`.so`, `.dll`, `.dylib`) Synthesis
* **Current State:** Linker emits PE executables and basic static libraries (`.a`/`.lib`).
* **Architectural Gap:**
  - Export Address Table (EAT) and Export Name Pointer Table (ENT) synthesis for Windows DLLs.
  - Mach-O dylib dynamic load commands and rebase/binding opcodes.
  - ELF Dynamic Table entries (`DT_SONAME`, `DT_NEEDED`, `DT_SYMTAB`, `DT_STRTAB`, `DT_HASH`).

---

## 4. Runtime & OS Abstraction Gaps

### 4.1 Native Platform Event Loop & Async Engine
* **Current State:** CLI uses Tokio for async standard library components.
* **Architectural Gap:** Pure zero-dependency async I/O multiplexer:
  - Linux: `epoll` / `io_uring`
  - Windows: I/O Completion Ports (IOCP)
  - macOS/BSD: `kqueue`

### 4.2 Stack Unwinding & Personality Routine
* **Current State:** Panics default to abort or handle logging.
* **Architectural Gap:** DWARF `.eh_frame` FDE/CIE writer paired with a native personality routine (`__adesh_personality_v0`) for stack unwinding on panic.

---

## 5. Quantum Toolchain Gaps

### 5.1 End-to-End Quantum Execution Pipeline
* **Current State:** Circuit building, OpenQASM 3.0 export, Basis gate decomposition, topological routing, and state-vector simulator are implemented in `linker/src/quantum/`.
* **Architectural Gap:** Integrate syntax and semantic analysis so `let q = qubit[2]; H(q[0]); CX(q[0], q[1]); let r = measure(q);` automatically executes through the simulator in JIT or compiles to QIR/OpenQASM binary bundle in AOT.
