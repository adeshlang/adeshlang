# Adesh Native Production Toolchain — Architecture Gaps Analysis

**Generated:** 2026-10-02 (revised after code-level audit and native codegen fixes)
**Scope:** Gaps between Current Baseline and Self-Contained Native Production Targets

---

## 1. Compiler Frontend & IR Lowering Gaps

### 1.1 Native IR Lowering Coverage (`HIR/MIR -> Machine IR`)
* **Current State:** `src/backends/native/lower.rs` now lowers integers,
  arithmetic, comparisons, If/While/Block, and calls (with stack arguments,
  Win64 shadow space, and incoming stack-parameter loads). Default AOT builds
  still route through the Cranelift backend; the native path is selected via
  `--emit-adob` / `--emit-asm`.
* **Architectural Gap — statement coverage:**
  - Match/switch lowering to jump tables (the `opt/jump_table.rs` pass exists
    but is not wired into the native path).
  - For-in loops, break/continue, structured memory access (field offsets,
    array indexing, slicing).
  - String concatenation and string operations beyond literal materialization.
  - Defer/panic lowering and runtime C ABI call emission beyond print/println.

### 1.2 Parallel-Move Resolution at Call Sites
* **Current State:** argument values are moved into parameter registers
  sequentially. If the register allocator happens to place an argument's
  value in a parameter register that an earlier move already overwrote, the
  call passes a wrong value.
* **Architectural Gap:** implement a classic parallel-move (swap-cycle)
  resolver for the argument shuffle before each call.

### 1.3 Float / SIMD Support in Native Codegen
* **Current State:** `FloatImmediate` is materialized as raw bits into a GPR.
  There is no XMM usage at all; vector instructions fail loudly.
* **Architectural Gap:** SSE2 (x86-64 baseline) encoders, XMM register file
  integration, float arithmetic/comparison lowering, and FP argument/return
  registers (XMM0–XMM7) in the calling conventions.

### 1.4 Heterogeneous IR (Device Kernels)
* **Current State:** GPU/NPU/TPU modules produce ADOB objects containing
  metadata and re-embedded source bytes.
* **Architectural Gap:** structured translation of device kernels into
  SPIR-V or PTX embedded in ADOB `.gpu_kernel` sections, plus a kernel-launch
  runtime (currently zero driver API calls exist).

## 2. ABI & Calling Convention Gaps

### 2.1 Argument Classification and Variadic
* **Current State:** calling conventions provide argument/return register
  lists, stack-passed arguments, and the Win64 32-byte shadow space
  (allocated in the caller). AAPCS64's callee-saved list is empty (x19–x28
  must be preserved) and needs correction once the AArch64 backend is real.
* **Architectural Gap:**
  - Struct-by-value classification (SysV register classes, memory fallback).
  - Hidden return-slot pointers (sret) for large aggregates.
  - Variadic arguments: register save area, `va_list`, AL for SysV varargs.
  - FP/vector argument assignment (XMM0–XMM7 / V0–V7).

### 2.2 Thread-Local Storage (TLS)
* **Current State:** PE TLS directory synthesis (`.tls` merge, `_tls_index`,
  `IMAGE_TLS_DIRECTORY64`) works. `TlsModel` is an enum serialized in ADOB
  metadata only; the TLS relocation kinds are inert (resolved as plain
  absolutes); ELF/Mach-O TLS input sections are a hard error; codegen emits
  no thread-pointer access sequences (`fs:`, `tpidr_el0`).
* **Architectural Gap:**
  - Codegen emission of TLS access sequences per model.
  - ELF: `PT_TLS` program header (constant defined but unused) plus
    DTPMOD/TPOFF dynamic relocation handling.
  - Mach-O: TLS section and `__DATA,__thread_vars` support.

## 3. Native Linker Gaps

### 3.1 True IR-Level Link-Time Optimization (LTO)
* **Current State:** `--lto` fails loudly by design. `crates/adesh-codegen`
  contains an `LtoEngine` (module merge, dead-function elimination,
  single-block inlining) that is dead code with unsound inlining (no register
  renaming or argument binding).
* **Architectural Gap:** define an ADOB `.adesh_ir` bitcode section holding
  serialized MIR; unpack and optimize across modules at link time; emit
  machine code post-optimization. Sound inlining requires operand rewriting.

### 3.2 DWARF / CodeView Debug Information
* **Current State:** the link path only strips debug sections.
  `Dwarf5Generator` emits one compile-unit DIE and `CodeViewGenerator` two
  records; neither is invoked. Unwind table builders (`.eh_frame_hdr`,
  `.pdata`/`.xdata`) exist for tests only, with zero-code xdata stubs; there
  is no CIE/FDE writer.
* **Architectural Gap:** DWARF 5 `.debug_info/.debug_abbrev/.debug_str`, a
  `.debug_line` state machine, CodeView PDB streams for Windows, and wiring
  the unwind generators into the real link path.

### 3.3 Dynamic Shared Library (`.so`, `.dll`, `.dylib`) Synthesis
* **Current State:** `--shared` fails loudly (previously it silently produced
  a static executable). The PE export-table builder (`pe/export.rs`) is dead
  code and `is_dll` is always false; ELF has no PT_DYNAMIC/.dynsym; Mach-O
  never emits `LC_LOAD_DYLIB` or `MH_DYLIB`.
* **Architectural Gap:**
  - PE: EAT/ENT synthesis, `IMAGE_FILE_DLL`, import-library generation.
  - ELF: `.dynamic`, `DT_SONAME`/`DT_NEEDED`, `.dynsym`, `.hash`, PLT/GOT.
  - Mach-O: `LC_ID_DYLIB`, exports trie, rebase/binding opcodes.

### 3.4 Mach-O Executability
* **Current State:** headers, segments, `LC_MAIN`, `LC_SYMTAB` are emitted;
  the symtab's `n_sect` is hardcoded and no `LC_LOAD_DYLIB` exists, so
  libSystem cannot be linked; outputs are never executed.
* **Architectural Gap:** `LC_LOAD_DYLIB` routing, `LC_DYLD_INFO`/chained
  fixups, exports trie, and an execution test on macOS.

### 3.5 ELF Verification
* **Current State:** static ET_EXEC emission only; no test ever runs a
  produced ELF (magic-byte checks only); no dynamic linking.
* **Architectural Gap:** CI execution test on Linux (exit-code assertion like
  the Windows e2e test), then the dynamic-linking work above.

### 3.6 Archive Interoperability
* **Current State:** `adeshlink ar` reads GNU/BSD archives (with symbol
  index rebuild) but writes archives **without a symbol-index member**, and
  silently skips unparseable `.lib` COFF import-library members.
* **Architectural Gap:** emit the `/` symbol index on write; thin-archive
  support; import-library member parsing.

## 4. Runtime & OS Abstraction Gaps

### 4.1 Native Platform Event Loop & Async Engine
* **Current State:** no async machinery exists in the runtime crate; the
  thread pool worker bodies are empty stubs; the HTTP standard library uses
  Tokio; the interpreter has JS-style Promise/`setTimeout` queues.
* **Architectural Gap:** a pure zero-dependency async engine — Linux
  `epoll`/`io_uring`, Windows IOCP, macOS/BSD `kqueue` — with an executor,
  wakers, and async-fn → state-machine lowering in native codegen
  (`crates/adesh-codegen/src/concurrency` currently lowers atomics only).

### 4.2 Stack Unwinding & Personality Routine
* **Current State:** panics abort or log; unwind table generators are
  test-only with zero-code xdata stubs; there is no CIE/FDE emitter.
* **Architectural Gap:** `.eh_frame` FDE/CIE writer plus a native personality
  routine (`__adesh_personality_v0`) for panic unwinding and defer cleanup.

## 5. Quantum Toolchain Gaps

### 5.1 End-to-End Quantum Execution Pipeline
* **Current State:** circuit building, OpenQASM 3.0 export, basis gate
  decomposition, topological routing, and the state-vector simulator are
  implemented in `linker/src/quantum/` (measurement is a deterministic
  threshold mock; the router leaves a SWAP insertion stub; `hybrid.rs` defines
  runtime symbol names without implementations). The main compiler has no
  quantum language constructs at all.
* **Architectural Gap:** language syntax and semantic analysis for
  `let q = qubit[2]; H(q[0]); CX(q[0], q[1]); let r = measure(q);`, JIT
  execution through the simulator, AOT compilation to QIR/QASM bundles, and
  eventually real hardware drivers (none exist; no cloud QPU integration).

## 6. Accelerator Codegen Gaps

### 6.1 Device ISA Emission and Launch Runtime
* **Current State:** `compile_kernel` re-embeds the kernel source as bytes;
  fatbin/code-object writers require externally produced payloads; the one
  SPIR-V writer emits a fixed empty shader; the MLIR GPU path requires
  external `mlir-opt`/`mlir-translate`/`llc`; there are zero CUDA/HIP/Vulkan
  driver calls (no kernel can be launched).
* **Architectural Gap:** real PTX/AMDGCN/SPIR-V generation from Adesh kernel
  code and a driver-based launch path.
