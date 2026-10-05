# Adesh Native Production Toolchain — Architecture Gaps Analysis

**Generated:** 2026-10-05 (Phase 0 documentation truth-reset revision)
**Canonical status source:** `CURRENT_STATE.md` (this file must agree with it)
**Scope:** Gaps between Current Baseline and Self-Contained Native Production Targets

---

## 1. Compiler Frontend & IR Lowering Gaps

### 1.1 Native IR Lowering Coverage (`HIR -> Machine IR`)
* **Current State:** `src/backends/native/lower.rs` lowers integers,
  arithmetic, comparisons, If/While/ForIn/Break/Continue, match (compare
  chains), float arithmetic via SSE2, strings/concat, ranges, membership,
  arrays/index mutation, methods, short-circuit values, try/catch, and
  defer. **Default executable builds use the native path**; Cranelift is
  opt-in (`--codegen=cranelift`). `--emit-adob` / `--emit-asm` emit
  intermediate artifacts only.
* **Architectural Gap — statement coverage (tracked in
  `IMPLEMENTATION_PLAN.md` Phase 1):**
  - Match/pattern lowering still emits per-arm compare chains; the
    `opt/switch_lowering.rs` (jump table) pass exists but is **not wired
    into the native path**.
  - Silent fallbacks must become structured compile errors (unknown
    expressions currently lower to `0`; `EnumVariant` patterns always
    match and bind nothing; imports are dropped; async constructs lower
    sequentially; lambdas ignore captured environments).
  - Defer/panic lowering and runtime C ABI call coverage beyond the
    implemented `aot_*` set; complete composite-type support (today all
    composites are boxed runtime handles, not value types).

### 1.2 Parallel-Move Resolution at Call Sites — RESOLVED
* **Status:** Resolved. `ParallelMoveResolver`
  (`crates/adesh-codegen/src/calling_convention/parallel_move.rs`) handles
  register chains, 2-cycles, N-cycles, stack↔register, stack↔stack,
  immediates, and mixed GPR/XMM moves with scratch registers; it is wired
  into native call lowering and exercised by
  `tests/native_parallel_move_e2e_test.rs`.
* **Remaining gap:** the FFI lowerer (`crates/adesh-codegen/src/ffi/mod.rs`)
  still emits sequential argument moves and bypasses the resolver; its
  `HiddenSret` return path silently emits no move. Both are tracked for
  Phase 1/2.

### 1.3 Float / SIMD Support in Native Codegen — SCALAR FP RESOLVED
* **Status:** Scalar floating point is implemented and execution-tested:
  SSE2 scalar encoders (addss/subss/mulss/divss and double forms,
  `ucomiss`/`ucomisd`, `cvt*` conversions), XMM0-13 allocation, Win64
  nonvolatile XMM6-13 handling, and scalar FP argument/return registers
  (XMM0-3 Win64 / XMM0-7 SysV). Packed SSE encodings exist in the encoder.
* **Architectural Gap:** vector/SIMD argument classification for the C ABI,
  `comiss`/`comisd` forms, wider atomics (only `lock xadd` /
  `lock cmpxchg` on 64-bit GPRs exist), and SIMD lowering from source
  (`opt/vectorization.rs` is not wired).

### 1.4 Heterogeneous IR (Device Kernels)
* **Current State:** GPU/NPU/TPU modules produce ADOB objects containing
  metadata and re-embedded source bytes. The container writers use custom
  formats (not NVIDIA fatbin, not AMDGPU MsgPack code objects); the one
  SPIR-V writer emits a fixed empty shader; the MLIR GPU path shells out to
  external `mlir-opt`/`llc`; there are zero CUDA/HIP/Vulkan driver calls.
* **Architectural Gap:** structured translation of device kernels into
  real PTX/AMDGCN/SPIR-V embedded in ADOB `.gpu_kernel` sections, plus a
  kernel-launch runtime. **Parked** until the core targets are solid
  (`IMPLEMENTATION_PLAN.md`).

## 2. ABI & Calling Convention Gaps

### 2.1 Argument Classification and Variadic
* **Current State:** calling conventions provide argument/return register
  lists, stack-passed arguments, the Win64 32-byte shadow space (allocated
  in the caller), and scalar FP argument/return registers (XMM0-3 Win64 /
  XMM0-7 SysV). AAPCS64's callee-saved list is empty (x19–x28 must be
  preserved) and needs correction once the AArch64 backend is real.
* **Architectural Gap:**
  - Struct-by-value classification (SysV register classes, memory
    fallback) in the *executable* path (the descriptive classifier in
    `crates/adesh-codegen/src/abi/mod.rs` is consumed by FFI lowering only).
  - Hidden return-slot pointers (sret) for large aggregates (the
    `ReturnLocation::HiddenSret` arm is currently a silent no-op in FFI
    lowering).
  - Variadic arguments: register save area, `va_list`, AL for SysV
    varargs; Win64 register spill to shadow space.
  - Vector/SIMD argument assignment beyond scalar FP.

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

### 3.1 Link-Time Optimization (LTO)
* **Status:** Section-level only. `--lto` / `--enable-lto` enable aggressive
  Identical Code Folding (`IcfMode::All`), recursive section GC
  (`gc_sections`), symbol stripping, and multi-pass dead data pruning. The
  CLI help text now describes this accurately (2026-10-05).
* **Architectural Gap:** true cross-module IR optimization. The
  machine-IR LTO engine (`crates/adesh-codegen/src/opt/lto.rs`: global dead
  function elimination + cross-module inlining) exists but its
  `CompilerDriver` is never instantiated, and there is no `.adesh_ir`
  section carrying IR between compilation units (only `.adesh.meta`,
  `.adesh.drop_table`, `.adesh.unwind_map` exist).

### 3.2 DWARF / CodeView Debug Information
* **Current State:** the link path only strips debug sections.
  `Dwarf5Generator` emits one compile-unit DIE and `CodeViewGenerator` two
  records; neither is invoked. Unwind table builders (`.eh_frame_hdr`,
  `.pdata`/`.xdata`) exist for tests only, with zero-code xdata stubs; there
  is no CIE/FDE writer and `WindowsPdataGenerator` is never called.
* **Architectural Gap:** DWARF 5 `.debug_info/.debug_abbrev/.debug_str`, a
  `.debug_line` state machine, CodeView PDB streams for Windows, and wiring
  the unwind generators into the real link path
  (`IMPLEMENTATION_PLAN.md` Phase 5).

### 3.3 Dynamic Shared Library (`.so`, `.dll`, `.dylib`) Synthesis — EMISSION ONLY
* **Status:** Artifact emission is implemented for all three formats
  (structure-tested only; not execution-tested):
  - ELF: `.dynamic`, `DT_SONAME`, `DT_NEEDED`, `.dynsym`, `.dynstr`,
    `.hash`, `PT_DYNAMIC`, `PT_GNU_RELRO` (covers `.dynamic` bytes only),
    `PT_INTERP`, and `ET_DYN`.
  - Mach-O: `MH_DYLIB`, `LC_ID_DYLIB`, `LC_LOAD_DYLIB`
    (`/usr/lib/libSystem.B.dylib`), `LC_BUILD_VERSION`, and partitioned
    `LC_DYSYMTAB`.
  - PE: DLL export tables and COFF import libraries.
  - OS Router: routes C library imports to `libc.so.6` on ELF and
    `libSystem.B.dylib` on macOS Mach-O.
* **Architectural Gap:** ELF imports are not resolved (no GOT/PLT), Mach-O
  has no dyld binding info, and no shared object has ever been loaded or
  executed by a test.

### 3.4 Mach-O Executability
* **Status:** Emission implemented; executability is not. The writer
  synthesizes 64-bit Mach-O layouts (`MH_EXECUTE`/`MH_DYLIB`, 4GB
  `__PAGEZERO`, `LC_BUILD_VERSION`, `LC_LOAD_DYLIB`, `LC_MAIN`, `LC_SYMTAB`,
  `LC_DYSYMTAB`), but there is no `LC_DYLD_INFO`/`LC_DYLD_CHAINED_FIXUPS`
  (no rebase/bind opcodes), no exports trie, and no indirect symbol table
  (`indirectsymoff = 0`), so undefined symbols cannot bind. A dyld
  executable with any external call would fault; only fully-static
  `LC_MAIN` code could run.
* **Architectural Gap:** dyld info or chained fixups, exports trie,
  indirect symbol table, stub helpers, `__DATA_CONST`, and a real macOS
  execution test. **Parked** until Linux execution lands
  (`IMPLEMENTATION_PLAN.md`).

### 3.5 ELF Executables & Dynamic Linking — STATIC + UNRESOLVED DYNAMIC
* **Status:** static standalone executables are emitted (`ET_EXEC` with
  `_start` syscall-exit synthesis for x86_64/AArch64, gated on
  `entry_va == 0`; otherwise `e_entry` points at `main` with no argc/argv
  setup). Dynamic shared objects are emitted (`ET_DYN` with `PT_INTERP`,
  `.dynsym`, `.dynstr`, SYSV `.hash`, `.dynamic`) but all dynamic
  relocation kinds (GotRelative32/Got64/PltRelative32/Tls*) are applied as
  plain symbol-VA writes; there is no GOT/PLT.
* **Architectural Gap:** GOT/PLT synthesis, `.gnu.hash`, full RELRO,
  `PT_TLS`, proper `_start` argc/argv → `main` → exit setup, and **a Linux
  execution test** (`IMPLEMENTATION_PLAN.md` Phase 3).

### 3.6 Archive Interoperability
* **Current State:** `adeshlink ar` reads GNU/BSD archives (with symbol
  index rebuild) but writes archives **without a symbol-index member**, and
  silently skips unparseable `.lib` COFF import-library members.
* **Architectural Gap:** emit the `/` symbol index on write; thin-archive
  support; import-library member parsing.

## 4. Runtime & OS Abstraction Gaps

### 4.1 Native Platform Event Loop & Async Engine
* **Current State:** no async machinery exists in the runtime crate. The
  reusable `AdeshThreadPool` is implemented (worker reuse, per-task panic
  containment) and backs native parallel iteration; the cooperative
  `AsyncExecutor` is skeletal (ready queue + busy-loop `block_on`); there
  is no epoll/IOCP/kqueue reactor anywhere. The HTTP standard library uses
  Tokio; the interpreter has JS-style Promise/`setTimeout` queues. Native
  lowering compiles `Await`/`Spawn` as plain sequential expressions
  (tracked for a loud error in Phase 1).
* **Architectural Gap:** a zero-dependency reactor (Linux `epoll`/`io_uring`,
  Windows IOCP, macOS/BSD `kqueue`), an executor with real wakers, and
  async-fn → state-machine lowering in native codegen. **Parked** until
  Phase 1 makes async constructs fail loudly.

### 4.2 Stack Unwinding & Personality Routine
* **Current State:** panics abort or log; unwind table generators are
  test-only with zero-code xdata stubs; there is no CIE/FDE emitter.
* **Architectural Gap:** `.eh_frame` FDE/CIE writer plus a native
  personality routine (`__adesh_personality_v0`) for panic unwinding and
  defer cleanup.

## 5. Quantum Toolchain Gaps

### 5.1 End-to-End Quantum Execution Pipeline
* **Current State:** circuit building, OpenQASM 3.0 export, basis gate
  decomposition, topological routing, and the state-vector simulator are
  implemented in `linker/src/quantum/` (measurement is a deterministic
  threshold mock; the router leaves a SWAP insertion stub; `hybrid.rs`
  defines runtime symbol names without implementations). The main compiler
  has no quantum language constructs at all.
* **Architectural Gap:** language syntax and semantic analysis for
  `let q = qubit[2]; H(q[0]); CX(q[0], q[1]); let r = measure(q);`, JIT
  execution through the simulator, AOT compilation to QIR/QASM bundles, and
  eventually real hardware drivers (none exist; no cloud QPU integration).
  **Parked** (`IMPLEMENTATION_PLAN.md`).

## 6. Accelerator Codegen Gaps

### 6.1 Device ISA Emission and Launch Runtime
* **Current State:** `compile_kernel` re-embeds the kernel source as bytes;
  container writers require externally produced payloads and use invented
  formats (not real fatbin/HSACO/metallib); the one SPIR-V writer emits a
  fixed empty shader; the MLIR GPU path requires external
  `mlir-opt`/`mlir-translate`/`llc`; there are zero CUDA/HIP/Vulkan driver
  calls (no kernel can be launched). No Apple ANE code exists.
* **Architectural Gap:** real PTX/AMDGCN/SPIR-V generation from Adesh kernel
  code and a driver-based launch path. **Parked** until the core targets are
  solid; documentation must describe this subsystem as "container packaging
  scaffolding" until then.
