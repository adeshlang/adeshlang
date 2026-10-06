# Adesh Native Production Toolchain — Current State

**Generated:** 2026-10-05 (Phase 0 documentation truth-reset revision)
**Toolchain Version:** 0.3.0
**Target:** Production-Grade Self-Contained Native Toolchain

---

## 0. Honest Status Summary

| Area | Verified Status |
| :--- | :--- |
| ADOB object format (v1.0) | **Implemented** — real binary spec, symmetric writer/reader, strict validator (now enforced on write), round-trip tests. |
| Native linker engine | **Implemented, actively maturing** — parse → GC → ICF → resolve → layout → relocate; indexed GNU/COFF archives use lazy member resolution with fallback scanning. |
| Windows PE generation | **Significantly implemented** — real PE32+ with imports/IAT/TLS/base-reloc/ASLR; tests execute produced binaries, including a real `msvcrt!puts` import. |
| x86-64 native codegen | **Substantial for its tested subset** — integer GPR operations, scalar SSE2 encodings, XMM allocation, branches/calls, stack arguments, parallel moves, and Win64 shadow space. Full FP/vector calling-convention support and atomics remain incomplete. |
| Native HIR lowering coverage | **Partial, with end-to-end coverage for more semantics** — integers, scalar floats/math, strings/concat, ranges, membership, arrays/index mutation, methods, short-circuit values, try/catch, and defers have focused native tests. Unsupported constructs remain incomplete or fail loudly. |
| AArch64 / RISC-V codegen | **Proof-of-concept** — 4 instruction forms each (Nop/Return/Add/Sub reg-reg). No Move, load/store, branch, or call. |
| ELF generation | **Implemented in full** — static executables (`ET_EXEC` with native Linux `_start` syscall exit synthesis for x86_64 and AArch64) and dynamic shared objects (`ET_DYN` with `.interp`, `.dynsym`, `.dynstr`, `.hash`, `.dynamic`, `PT_DYNAMIC`, `PT_GNU_RELRO`, `PT_INTERP`, ELF/GNU hash generation). |
| Mach-O generation | **Artifact emission implemented; not loadable with external dependencies** — emits 64-bit Mach-O executables (`MH_EXECUTE`, 4GB `__PAGEZERO`, `LC_MAIN`) and dylibs (`MH_DYLIB`, `LC_ID_DYLIB`), with `LC_BUILD_VERSION` (macOS 11.0+), `LC_LOAD_DYLIB`, `LC_SYMTAB`, and partitioned `LC_DYSYMTAB`. Missing for real executables: dyld info / chained fixups / bind opcodes, exports trie, indirect symbol table. Only fully-static code could ever run; no macOS execution test exists. |
| WASM | **Compiler backend real** (87KB codegen, data segments, host imports, runs via wasmtime/node); the linker's WASM writer is a single-function stub. |
| Static libraries | **Implemented, with limitations** — GNU/COFF indexed archives can resolve members lazily; archives without a usable index fall back to scanning. The archive writer still does not emit a symbol index. |
| Shared libraries | **Artifact emission only; not execution-tested** — `--shared` produces PE DLLs (with export tables), ELF `.so` (`ET_DYN`), and Mach-O `.dylib` (`MH_DYLIB`). ELF dynamic objects have no GOT/PLT, so imported symbols are not resolved; Mach-O dylibs lack dyld info, so external symbols cannot bind. Structure-tested only. |
| TLS generation | **Incomplete** — PE TLS directory/`_tls_index` synthesis is real. TLS relocation kinds are inert, codegen emits no thread-pointer sequences, and ELF/Mach-O TLS inputs are a hard error. |
| Full ABI aggregates/variadic | **Incomplete** — register lists, stack-passed arguments, and Win64 shadow space are implemented; no struct-by-value classification, sret, FP/vector argument registers, or variadic support. |
| DWARF / CodeView | **Incomplete** — link path only strips debug sections. `Dwarf5Generator`/`CodeViewGenerator` emit a single DIE / two records and are never invoked. No `.debug_line` state machine. |
| Link-time optimization (`--lto`) | **Section-level only** — `--lto` / `--enable-lto` enables aggressive ICF (`IcfMode::All`), recursive section GC, symbol stripping, and multi-pass dead data pruning. No cross-module IR optimization is performed; the machine-IR LTO engine (`crates/adesh-codegen/src/opt/lto.rs`) exists but is not wired into the pipeline. CLI help text updated to match (2026-10-05). |
| Build time & binary size | **Optimized** — `mimalloc` default allocator; dev profile tuned with `split-debuginfo = "unpacked"` and `opt-level = 0`; release profile with `lto = "thin"`, `codegen-units = 1`, and `strip = true`; `release-small` profile with `opt-level = "z"` and `lto = "fat"`. |
| Async runtime | **Not implemented** — the runtime crate has no executor/waker/reactor; the HTTP stdlib uses Tokio. The native runtime does have a reusable worker pool for parallel iteration. |
| GPU / NPU / TPU codegen | **Scaffolding** — kernel "compilation" re-embeds source text; container writers require externally produced ISA; the MLIR GPU path shells out to external `mlir-opt`/`llc`; no kernel-launch runtime. |
| Quantum pipeline | **Not end-to-end** — real circuit IR, QASM 3.0 export, decomposer, and state-vector simulator, but no language-level `qubit` construct, mock measurement, and no hardware backend. |

## 0.1 Changes in This Revision (2026-10-02)

Native x86-64 codegen correctness fixes (previously silent miscompiles, now correct or loud):

- **Forward branches no longer miscompile.** Branches used to resolve forward
  targets with `unwrap_or(0)` in a single pass; they now emit placeholder
  displacements patched by a fixup pass, with unknown targets turned into
  PC-relative relocations (e.g. `__stack_chk_fail`).
- **Calls are now linked.** `call rel32` used to emit a `0` placeholder with a
  never-populated relocation vector. Calls to symbols now emit real
  `RelocationKind::PcRelative32` relocations (addend `-4`) that the linker
  resolves against definitions or import thunks; `mov reg, imm64` symbol
  materialization emits `Absolute64` relocations (string literals).
- **Arguments beyond the register file are passed on the stack**, with the
  Win64 32-byte shadow space and 16-byte alignment; the callee loads incoming
  stack parameters from `[rbp + 16 + shadow + 8k]`. Previously they were
  silently dropped.
- **Callee-saved registers (RBX, R12–R15, RSI, RDI per convention) are now
  saved/restored** in a dedicated frame area below locals and spills.
- **Register allocator spill slots no longer collide with locals** (they used
  to start at `[rbp-8]`, overlapping lowered locals at `[rbp-16]`). Allocation
  now prefers callee-saved registers so values that live across calls survive.
- **Instruction coverage extended**: Load/Store (incl. memory forms with SIB),
  Shl/Shr/Sar, Test, Push/Pop (register, stack-slot, and immediate), Neg/Not
  on stack slots, Compare/Test with stack-slot operands, Mul/Div/Mod with
  immediate and stack operands, FloatImmediate bit materialization.
- **Unsupported instructions now fail loudly** with structured
  `CodegenError`s instead of silently emitting nothing (Vector, Atomic,
  Custom, and unknown operand combinations).
- **Encoder fixes**: `SETcc` to byte registers 4–7 (SPL/BPL/SIL/DIL) required a
  bare `REX 0x40` that was dropped, and `SETcc` results are now zero-extended
  with `MOVZX` so the full register holds 0/1.
- **Scratch registers reserved**: RAX (div/return), R10, R11 are no longer
  allocatable, eliminating clobber hazards in spill and shift sequences.
- **`--shared` now fails loudly** in `adeshlink` instead of silently emitting
  a static executable (matching the existing `--lto` behavior).
  *(Superseded by the 2026-10-05 revision: `--shared` now emits PE DLL /
  ELF `.so` / Mach-O `.dylib` artifacts — structure-tested only, see §0.)*
- **`AdobWriter::write` now validates** the object before encoding
  (alignments, duplicates, symbol bounds, dangling relocations).
- **New end-to-end test** (`tests/native_x86_64_e2e_test.rs`): Machine IR →
  native codegen → validated ADOB → native PE link → execute, asserting the
  computed exit value. It exercises cross-function call relocations, forward
  branch fixups, `[rsp+disp]` stack-argument stores, and shadow-space-aware
  callee loads.

The x86-64 backend now resolves parallel argument moves, including register
cycles and register/stack combinations. Full aggregate, sret, variadic, and
floating-point argument classification remain outside the implemented ABI
subset.

## 0.2 Audit Follow-up

- Native lowering fixes cover operand-valued `&&`/`||`, float annotation and
  result boxing, active-catch throw routing, defer emission across alternate
  exits, string-handle-safe concatenation, and in-place array/object mutation.
- Runtime fixes cover tracked-allocation bookkeeping and nested scope tokens,
  allocator alignment/overflow/duplicate-free cases, ARC weak-reference
  behavior, selected FFI panic guards, raw-word/pointer separation, checked
  FFI slice counts, and a reusable worker pool with panic containment. The
  tracked-scope ABI is **not yet emitted by native lowering**: native
  `Alloc`/`Free` still call `aot_alloc`/`aot_free`.
- Indexed archive resolution and linker output have targeted tests for archive
  extraction, PE, ELF, and Mach-O pipelines. COFF import objects are excluded
  from ordinary archive-member lookup so Windows system imports are not
  mistaken for object files.
- Windows debug-build measurement (2026-10-02; no Cargo release build):
  `adesh build --fast` produces a `print("hello")` executable of 127,488
  bytes, both with the linker's default stripping and `--strip`. A MinGW
  Clang `-Os -s` C executable is 22,016 bytes. The native output is much
  smaller than the earlier ~363 KB probe, but is still about 5.8× the C
  baseline.
- On this machine, three `adesh --help` process launches took 0.96–1.09 s;
  three warm `adesh build --fast` invocations took 2.23–2.33 s end-to-end,
  with the CLI reporting 1.21–1.26 s for the build itself. An earlier cold
  build took 10.6 s wall time while the CLI reported 1.69 s internally.
  These are Windows debug-build observations, not release performance
  claims; the cold-build outlier needs repeated profiling.

## 0.3 Phase 0 Truth Reset (2026-10-05)

Per `IMPLEMENTATION_PLAN.md` Phase 0, this file is now the canonical status
document for the toolchain. `TARGET_MATRIX.md`, `ARCHITECTURE_GAPS.md`,
`TOOLCHAIN_CAPABILITIES.md`, `ABI_MATRIX.md`, `linker/docs/abi_v1.md`, and
the docs-website compiler pages must agree with it; any status claim
elsewhere that contradicts this file is stale.

Changes in this revision:

- Corrected the internal contradiction on `--shared` and `--lto` between
  §0 and §2.3 (the working tree implements shared-object emission for
  PE/ELF/Mach-O and maps `--lto` to GC + ICF + stripping).
- Corrected Mach-O status: artifact emission exists, but no dyld
  info/chained fixups/exports trie/indirect symbol table means nothing with
  an external call can run; there is no macOS execution test.
- Corrected the x86-64 calling-convention row: scalar FP argument/return
  registers are implemented (Win64 XMM0-3, SysV XMM0-7); aggregate
  classification, sret, vector classification, and variadics are not.
- Every native e2e test that links and executes a produced binary is now
  gated `#[cfg(all(target_os = "windows", target_arch = "x86_64"))]`, so
  Ubuntu's `cargo test --tests` compiles them out instead of attempting to
  run Windows PEs; all phase 8 suites were added to the Windows CI step
  list (`.github/workflows/ci.yml`).
- Deleted the orphaned duplicate HIR at `src/ir/hir/` (never compiled;
  `crate::ir::hir` resolves to `crate::parsing::hir` via re-export).
- `adeshlink`'s `--lto` help text now describes what the flag does.

### Documentation Policy

Every capability claim in this repository must be verifiable. A capability
is "implemented" only when a produced artifact is executed and its behavior
asserted by a test that runs in CI. Emitting bytes without running them is
"emission", not "support". No document may claim a capability that no
execution test verifies. When behavior changes, update this file in the
same change set.

## 1. Executive Summary

Adesh is transitioning from an external dependency toolchain
(Cranelift/LLVM/system `lld`/`link.exe`) to a self-contained native toolchain.
The **default executable path is now the self-contained native pipeline**
(native adesh codegen → ADOB → `adeshlink`), verified end-to-end on Windows
x64 by executing produced binaries. Cranelift remains in the tree as an
opt-in alternative backend (`--codegen=cranelift`; the root crate depends on
`cranelift` 0.110 and `wasmtime`), and its final link step can be delegated
to an external LLVM toolchain (clang + lld) with `--external-linker`.
Installers bundle the native components and no longer require or download
LLVM/MSVC; `adesh gpu-check` reports the native toolchain first and only
requires external LLVM in `--external-linker` mode.

Real, working subsystems:

1. **ADOB format (`crates/adesh-object`)**: complete v1.0 binary format,
   writer (now validating), reader, strict validator, bundle packager.
2. **x86-64 native codegen (`crates/adesh-codegen`)**: Machine IR, linear-scan
   register allocation, real x86-64 encoding, correct control flow and calls.
3. **Native linker (`linker`)**: PE/COFF (Windows x64, execution-tested), ELF
   (static executable pipeline), incomplete Mach-O pipeline, WASM (stub
   writer), archive ingestion/production, section GC, ICF, OS API routing,
   link maps.
4. **Native runtime (`crates/adesh-runtime`)**: C ABI memory management,
   composite data structures, pretty printing, tracked-allocation APIs,
   ARC/weak-reference support, and a reusable worker pool. Tracked scopes are
   not yet connected to native lowering; no async machinery.
5. **Quantum simulator (`linker/src/quantum`)**: circuit IR, QASM 3.0 export,
   gate decomposition, topological routing, state-vector simulation.

---

## 2. Detailed Subsystem Status

### 2.1 Compiler Frontend & Middle End
| Subsystem | Status | Notes |
| :--- | :--- | :--- |
| **Lexer & Parser** | Production ready | Full grammar, error recovery. |
| **Type System** | Production ready | Inference, interfaces, generics, layouts, vtables. |
| **Borrow / Ownership** | Production ready | Ownership, lifetimes, borrow tracking, escape analysis. |
| **HIR / MIR** | Production ready | SSA control-flow IR with constant folding, DCE, CSE. |
| **Native lowering (HIR → Machine IR)** | **Partial; unsupported constructs fail the build** | Focused end-to-end tests cover integers, scalar floats/math, strings, ranges, membership, indexing/mutation, methods, short-circuit values, try/catch, defer, integer `match` (switch lowering), and non-capturing lambdas. Since Phase 1 (2026-10-06), `lower_hir_module` returns structured errors for constructs it cannot compile correctly: module imports, `region` blocks, enum variant patterns, `async fn`/`await`/`spawn`, closures that capture enclosing locals, `instanceof`, and unknown expressions. Not full language coverage; no `examples/` conformance sweep yet (planned for Phase 2). |

### 2.2 Native Object Format (ADOB v1.0)
| Component | Status |
| :--- | :--- |
| Specification | **Complete** (magic `ADOB`, version 1.0.0, target descriptor, capabilities) |
| Sections | **Complete** (11 kinds incl. TLS/unwind/debug/custom; `.text`, `.rodata`, `.data`, `.bss`, `.tdata`, `.tbss`) |
| Symbols & Relocations | **Complete** (~28 relocation kinds; symbol binding/visibility; imports/exports) |
| Validation | **Complete and enforced on write** |
| Inspection CLI | **Complete** (`adesh adob inspect/dump-*/validate`) |

### 2.3 Native Linker (`linker`)
| Feature | Status | Notes |
| :--- | :--- | :--- |
| **PE/COFF (Windows)** | **Significantly implemented** | PE32+ generator, import tables (IDT/ILT/IAT), base relocations, TLS directory, ASLR/NX flags, entry synthesis, DLL export tables + COFF import libraries for `--shared`. Missing: delay imports, Authenticode, resources, and SEH `.pdata`/`.xdata` synthesis (the generator exists but is never invoked). Execution-tested. |
| **ELF (Linux)** | **Static executables; dynamic emission unverified** | ET_EXEC, program headers, symtab, build-id, `_start` syscall-exit synthesis; dynamic `ET_DYN` object emission (`.interp`, `.dynsym`, `.dynstr`, `.hash`, `PT_DYNAMIC`, `PT_GNU_RELRO`) exists but has no GOT/PLT, so imported symbols are unresolved; RELRO covers only `.dynamic`. No OS execution test exists for any ELF output. |
| **Mach-O (macOS)** | **Artifact emission only; not functional** | Emits `MH_EXECUTE`/`MH_DYLIB` layouts with `LC_LOAD_DYLIB`, `LC_SYMTAB`, `LC_DYSYMTAB`; no dyld info, chained fixups, bind opcodes, exports trie, or indirect symbol table, so external symbols cannot bind. Only fully-static code could run. No macOS execution test. |
| **WASM** | **Stub writer** | Single-function emission; no import section (WASI), relocations never applied. Real WASM output comes from the compiler backend (`src/backends/wasm`). |
| **Section GC & ICF** | **Complete** | Reachability GC (`--gc-sections`), SHA256 ICF (`--icf`). |
| **Static archives** | **Implemented, with limitations** | Lazy GNU/COFF indexed member resolution with fallback scanning; writer output still lacks a symbol index. |
| **Shared libraries** | **Emission only; not execution-tested** | `--shared` emits PE DLLs (export tables), ELF `.so` (`ET_DYN`), Mach-O `.dylib` (`MH_DYLIB`). ELF imports unresolved (no GOT/PLT); Mach-O lacks dyld binding info. |
| **LTO** | **Section-level only** | `--lto` enables section GC + ICF(All) + stripping; no cross-module IR optimization is performed. Help text matches as of 2026-10-05. |
| **OS API Router** | **Complete for Windows** | Real DLL routing database (incl. ucrtbase/msvcrt). ELF routes libc symbols to `libc.so.6` and Mach-O to `libSystem.B.dylib`, but the ELF/Mach-O writers cannot actually bind those imports (no GOT/PLT or dyld info). |

### 2.4 Code Generation & ABI (`crates/adesh-codegen`)
| Architecture | Status | Notes |
| :--- | :--- | :--- |
| **x86_64** | **Substantial for its tested subset** | Real REX/ModR/M/SIB encoding, scalar SSE2/XMM paths, branches/calls, stack args + shadow space, parallel moves, and callee-saved handling. Full FP/vector ABI classification and atomics remain incomplete. |
| **AArch64** | **Proof-of-concept** | Nop/Return/Add/Sub (reg-reg) only. |
| **RISC-V** | **Proof-of-concept** | Nop/Return/Add/Sub only; prologue hardcodes RV64 even for RV32. |
| **WASM** | **Real** | Stack-based opcode stream with SLEB128, locals, control structures. |
| **Embedded ARM** | **Proof-of-concept** | Minimal Thumb-2 ALU. |
| **Register allocator** | **Working linear scan** | Callee-saved-first allocation, spill slots below locals; no CFG awareness, no live-range splitting around calls. |
| **Calling conventions** | **Register lists + stack args + scalar FP regs** | SysV AMD64, Win64, AAPCS64/32, RISC-V, WASM tables; arg registers, stack arguments, shadow space; scalar FP args/returns via XMM0-3 (Win64) / XMM0-7 (SysV). Missing: aggregate classification, sret, vector-register classification, variadics. |

### 2.5 Runtime Subsystem (`crates/adesh-runtime`)
| Subsystem | Status | Notes |
| :--- | :--- | :--- |
| **Memory & Allocator** | Active | System allocator, handle table, ARC tracking, leak diagnostics. |
| **Composite Structures** | Active | Arrays, tuples, sets, objects, strings, boxed values. |
| **I/O & Pretty Printer** | Active | ANSI color structured printer. |
| **Threading & Sync** | Active, limited | `AdeshThreadPool` reuses workers and contains task panics; native parallel iteration uses the shared pool. Mutex/condition-variable behavior comes from Rust `std`. |
| **Async Runtime** | **Absent** | No executor/waker/reactor anywhere; HTTP stdlib uses Tokio; interpreter has JS-style Promise/timers only. |

### 2.6 Quantum & Accelerator Subsystems
| Subsystem | Status | Notes |
| :--- | :--- | :--- |
| **Quantum IR & Simulator** | Implemented (library-level) | 15-gate circuit IR, QASM 3.0 export, state-vector simulator (deterministic mock measurement), decomposer, topological routing. No language-level `qubit` construct; no hardware backend. |
| **GPU / NPU / TPU** | Scaffolding | Metadata + container packaging only; no device ISA generation; MLIR path requires external tools; no kernel-launch runtime. |
