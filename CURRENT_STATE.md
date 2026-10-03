# Adesh Native Production Toolchain — Current State

**Generated:** 2026-10-02 (revised after a full code-level audit and native codegen correctness fixes)
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
| ELF generation | **Static executables only** — ET_EXEC with program headers/symtab/build-id; the linker pipeline test passes. No dynamic section, PT_DYNAMIC, .dynsym, GOT/PLT, .gnu.hash, or RELRO; not OS-execution-tested here. |
| Mach-O generation | **Incomplete** — the linker pipeline test passes, but there is no LC_LOAD_DYLIB, dyld info/chained fixups, or complete runtime linking; not macOS-execution-tested. |
| WASM | **Compiler backend real** (87KB codegen, data segments, host imports, runs via wasmtime/node); the linker's WASM writer is a single-function stub. |
| Static libraries | **Implemented, with limitations** — GNU/COFF indexed archives can resolve members lazily; archives without a usable index fall back to scanning. The archive writer still does not emit a symbol index. |
| Shared libraries | **Not supported — fails loudly** (`--shared` is a hard error; previously it silently produced a static executable). Only external-linker delegation (`--external-linker`) can produce shared libraries. |
| TLS generation | **Incomplete** — PE TLS directory/`_tls_index` synthesis is real. TLS relocation kinds are inert, codegen emits no thread-pointer sequences, and ELF/Mach-O TLS inputs are a hard error. |
| Full ABI aggregates/variadic | **Incomplete** — register lists, stack-passed arguments, and Win64 shadow space are implemented; no struct-by-value classification, sret, FP/vector argument registers, or variadic support. |
| DWARF / CodeView | **Incomplete** — link path only strips debug sections. `Dwarf5Generator`/`CodeViewGenerator` emit a single DIE / two records and are never invoked. No `.debug_line` state machine. |
| True LTO / ThinLTO | **Not implemented** — `--lto` is a hard error. `LtoEngine` exists but is dead code with unsound inlining. GC + ICF are not LTO. |
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
| **Native lowering (HIR → Machine IR)** | **Partial** | Focused end-to-end tests cover integers, scalar floats/math, strings, ranges, membership, indexing/mutation, methods, short-circuit values, try/catch, and defer. This is not full language coverage; unsupported constructs must still be audited. |

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
| **PE/COFF (Windows)** | **Significantly implemented** | PE32+ generator, import tables (IDT/ILT/IAT), base relocations, TLS directory, ASLR/NX flags, entry synthesis. Missing: delay imports, Authenticode, resources, exports (`build_export_table` is dead code). Execution-tested. |
| **ELF (Linux)** | **Static executables only** | ET_EXEC, program headers, symtab, build-id; linker pipeline test passes. No dynamic linking support (PT_DYNAMIC/.dynsym/GOT/PLT/.gnu.hash/RELRO); no OS execution test. |
| **Mach-O (macOS)** | **Incomplete** | Linker pipeline test passes; no LC_LOAD_DYLIB, dyld info, chained fixups, or exports trie. No macOS execution test. |
| **WASM** | **Stub writer** | Single-function emission; no import section (WASI), relocations never applied. Real WASM output comes from the compiler backend (`src/backends/wasm`). |
| **Section GC & ICF** | **Complete** | Reachability GC (`--gc-sections`), SHA256 ICF (`--icf`). |
| **Static archives** | **Implemented, with limitations** | Lazy GNU/COFF indexed member resolution with fallback scanning; writer output still lacks a symbol index. |
| **Shared libraries** | **Unsupported (loud error)** | PE export tables / ELF dynamic / Mach-O dylib synthesis not implemented; `--shared` errors. |
| **LTO** | **Unsupported (loud error)** | `--lto` errors; GC/ICF are the available link-time optimizations. |
| **OS API Router** | **Complete for Windows** | Real DLL routing database; ELF/Mach-O paths classify to `Undefined`. |

### 2.4 Code Generation & ABI (`crates/adesh-codegen`)
| Architecture | Status | Notes |
| :--- | :--- | :--- |
| **x86_64** | **Substantial for its tested subset** | Real REX/ModR/M/SIB encoding, scalar SSE2/XMM paths, branches/calls, stack args + shadow space, parallel moves, and callee-saved handling. Full FP/vector ABI classification and atomics remain incomplete. |
| **AArch64** | **Proof-of-concept** | Nop/Return/Add/Sub (reg-reg) only. |
| **RISC-V** | **Proof-of-concept** | Nop/Return/Add/Sub only; prologue hardcodes RV64 even for RV32. |
| **WASM** | **Real** | Stack-based opcode stream with SLEB128, locals, control structures. |
| **Embedded ARM** | **Proof-of-concept** | Minimal Thumb-2 ALU. |
| **Register allocator** | **Working linear scan** | Callee-saved-first allocation, spill slots below locals; no CFG awareness, no live-range splitting around calls. |
| **Calling conventions** | **Register lists + stack args** | SysV AMD64, Win64, AAPCS64/32, RISC-V, WASM tables; arg registers, stack arguments, shadow space. Missing: aggregate classification, sret, FP/vector registers, variadic. |

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
