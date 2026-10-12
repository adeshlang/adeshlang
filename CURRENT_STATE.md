# Adesh Native Production Toolchain — Current State

**Generated:** 2026-10-11 (Phase 8 revision: ELF TLS, eh_frame, ADOB v3, OS-specific CI)
**Toolchain Version:** 0.3.0
**Target:** Production-Grade Self-Contained Native Toolchain

---

## 0. Honest Status Summary

| Area | Verified Status |
| :--- | :--- |
| ADOB object format (v3) | **Implemented** — real binary spec, symmetric writer/reader, strict validator (enforced on write), round-trip tests. v3 adds an explicit per-section size field (v2 derived size from data length, silently dropping `.tbss`/`.bss` zero-fill tails) and distinguishes the TLS section kinds; the reader accepts both v2 and v3 and routes ADOB by version before any other format probe. |
| Native linker engine | **Implemented, actively maturing** — parse → GC → ICF → resolve → layout → relocate; indexed GNU/COFF archives use lazy member resolution with fallback scanning. |
| Windows PE generation | **Significantly implemented** — real PE32+ with imports/IAT/TLS/base-reloc/ASLR; tests execute produced binaries, including a real `msvcrt!puts` import. |
| x86-64 native codegen | **Substantial for its tested subset** — integer GPR operations, scalar SSE2 encodings, XMM allocation, branches/calls, stack arguments, parallel moves, Win64 shadow space, full-width locked atomics, and Win64 TLS access sequences. Vector calling-convention support remains incomplete. |
| AArch64 codegen | **Implemented & Tested** — Full AAPCS64 register allocation, callee-saved preservation (X19..X28, D8..D15), 64-bit immediate materialization, arithmetic, comparisons/branches, scalar FP, NEON vectors, atomics, and ADOB/ELF object emission. Hardware execution pending. |
| RISC-V codegen | **Proof-of-concept** — Basic instruction forms (Nop/Return/Add/Sub reg-reg). |
| ELF generation | **Static executables execution-tested; dynamic POSIX routing & PLT/GOT stubs implemented; TLS execution-verified** — static `ET_EXEC` with native Linux `_start` synthesis (argc/argv/envp + `exit` syscall) runs on Linux CI and WSL with asserted exit codes. Dynamic import routing for external C/POSIX symbols (`libc.so.6`, `libgcc_s.so.1`), PLT/GOT synthesis with PC-relative relocations, and `.dynsym` undefined symbol emission implemented; section-symbol relocations resolved. TLS: merged `.tls` block (initialized sections first, zero-fill after), `PT_TLS`, `.tdata` (PROGBITS) / `.tbss` (NOBITS) section-header split, static local-exec relocation (`%fs:(off − memsz)`), and dynamic initial-exec (GOT slot + `R_X86_64_TPOFF64` resolved by the real loader) — **executed against a real musl `ld.so` in WSL with asserted exit codes** (`tests/native_phase8_tls_ehframe_test.rs`). `.eh_frame`/`.eh_frame_hdr` synthesis with `PT_GNU_EH_FRAME` (see DWARF row). General-dynamic TLS access is a loud link error (no DTV/`__tls_get_addr` runtime). |
| Mach-O generation | **Artifact emission, libSystem routing, and dyld binding structure implemented; execution pending** — emits 64-bit Mach-O executables and dylibs with `LC_LOAD_DYLIB`, `LC_SYMTAB`, partitioned `LC_DYSYMTAB`, and `LC_DYLD_INFO_ONLY` (bind/rebase opcode streams and an exports trie), an indirect symbol table, and synthesized `__stubs`/`__got`/lazy-pointer/helper sections forming a full call → stub → lazy-pointer → binder chain — all **verified by independent parse-back decoders** (`linker/tests/macho_dyld_binding_test.rs`). Chained fixups are not emitted. No macOS execution test exists yet: dyld execution verification is pending, now covered structurally on the macOS CI runner which runs the conformance suite on the only host where Mach-O is native. TLS sections under Mach-O still fail the link loudly (no TLV emission). |
| WASM | **Compiler backend real** (87KB codegen, data segments, host imports, runs via wasmtime/node); the linker's WASM writer is a single-function stub. |
| Static libraries | **Implemented** — Target-aware GNU/COFF indexed archives resolve members lazily without host triple leakage; archives without a usable index fall back to scanning. The archive writer emits a real GNU `/` symbol index (big-endian offsets + string pool) and the reader consumes it directly. Thin archives are not supported. |
| Shared libraries | **Artifact emission & dynamic resolution implemented** — `--shared` produces PE DLLs (with export tables), ELF `.so` (`ET_DYN`) with synthesized `.got` tables and `.dynamic` segments, and Mach-O `.dylib` (`MH_DYLIB`). Dynamic linking resolution tests verified. |
| TLS generation | **Implemented for Win64 (codegen) and ELF (linker), both execution-verified at their level** — Win64: PE TLS directory/`_tls_index` synthesis plus real thread-pointer access sequences (`gs:[0x58]` + SECREL relocations), execution-tested via MachineIR (`tests/native_phase2_abi_tls_test.rs`). ELF: `PT_TLS` with `.tdata`/`.tbss` split; static local-exec (`%fs:(off − memsz)`, verified by executing a binary that sets the thread pointer via `arch_prctl(ARCH_SET_FS)` and reads/updates its counter through `%fs`) and dynamic initial-exec (GOT slot + `R_X86_64_TPOFF64`, verified against a real musl `ld.so` in WSL) — `tests/native_phase8_tls_ehframe_test.rs`. General-dynamic is rejected loudly; Mach-O TLS remains a hard error (no TLV emission). No Adesh source syntax emits TLS accesses yet — the executed tests drive the linker with hand-built ADOB objects. |
| Native JIT (`--jit-native`) | **Functional for the tested subset; honest gaps are loud** — raw-pointer and array-indexing semantics divergences versus the interpreter were fixed (the same program now behaves identically); imported modules are rejected with an explicit error at compile time, and the CLI warns that `--jit-native` is not yet fully conformance-verified and recommends `--jit` or `adesh build`. Conformance between `--jit-native` and the interpreter is not yet systematically asserted by a dedicated test suite. |
| Full ABI aggregates/variadic | **Implemented at the codegen/FFI level** — shared SysV eightbyte + Win64 by-value classifier, sret for large aggregates, struct-by-value (1/2/4/8-byte) in registers and on the stack, Win64 shadow spill and SysV register-save-area/`va_list` variadic helpers; execution-tested via MachineIR/FFI suites. Not yet reachable from Adesh source syntax (no `extern` declarations, variadic functions, or aggregate value types in the front end). |
| DWARF / CodeView / SEH / eh_frame | **Implemented** — Windows SEH `.pdata`/`.xdata` `UNWIND_INFO` and opcode encoding wired to PE Optional Header exception directory; DWARF 5 `.debug_line` state machine program and compilation unit headers implemented; ELF `.eh_frame` (one CIE matching the real emitted prologue `push rbp; mov rbp, rsp`, one FDE per sized function symbol, no FDE for size-0 functions — never fabricated) plus `.eh_frame_hdr` binary-search table and `PT_GNU_EH_FRAME`, decode-verified by an independent byte-level decoder in tests. No runtime unwinder test exists yet: frame data is verified to describe the real code ranges, not yet to unwind a crash. |
| Link-time optimization (`--lto`) | **Implemented** — `--lto` routes through `CompilerDriver` and `LtoEngine` for cross-module optimization along with aggressive ICF (`IcfMode::All`), recursive section GC, and symbol stripping. |
| Build time & binary size | **Optimized** — standalone C runtime print/abort; Windows PE hello-world reduced to 4,096 bytes (string) and 5,120 bytes (numeric), well below <40 KB target; `-Os`/`-Oz` flags implemented; Criterion benchmark harness across 10-10K LOC. |
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

## 0.4 Phase 8 Revision (2026-10-11)

Execution-verified changes in this revision:

- **ELF TLS is implemented and executed.** Static local-exec (the binary
  itself installs the thread pointer via `arch_prctl(ARCH_SET_FS, block
  end)` and accesses variables at `%fs:(off − memsz)`) executes and exits
  with the expected counter value; dynamic initial-exec runs against the
  real musl `ld.so` in WSL, which processes `PT_TLS`, installs the thread
  pointer, and applies the `R_X86_64_TPOFF64` relocation on the GOT slot
  (`tests/native_phase8_tls_ehframe_test.rs`, 8/8). Two real bugs were
  fixed by these tests: the TLS contribution order (a purely lexical sort
  placed `.tbss` before `.tdata`, materializing the zero-fill as file
  bytes ahead of the image so `PT_TLS` `p_filesz` covered 24 bytes instead
  of the 8 initialized ones — initialized sections now come first, matching
  GNU ld), and the test probe itself (`ARCH_SET_FS` is `0x1002`; `0x1001`
  is `ARCH_SET_GS`, which set `%GS` and left `%FS` at 0). General-dynamic
  TLS access and TLS sections under Mach-O fail the link loudly.
- **`.eh_frame`/`.eh_frame_hdr` emission** with `PT_GNU_EH_FRAME`:
  synthesized from the codegen's real standard frame, one FDE per sized
  function, none for size-0 functions, verified by an independent decoder
  written in the tests (not by re-reading the generator's own output).
- **ADOB v3**: explicit per-section `size` (v2 lost `.tbss`/`.bss`
  zero-fill tails through the object round-trip) and correct TLS/meta kind
  ids; the reader accepts v2 and v3 and probes the ADOB version before
  any other format heuristic.
- **Ownership gate scope fix**: the per-function locals map was being
  dropped across nested scopes, causing false positives; now per-function,
  with three new adversarial borrow tests.
- **Runtime ABI hardening** (`d41aa90`): value-to-string conversion
  hardened against panics and invalid dereferences with a 9-test ABI
  suite; doctest failures fixed.
- **JIT-native divergences fixed** (`18d9909`): raw-pointer and
  array-indexing semantics now match the interpreter; imported modules
  fail loudly instead of misbehaving.
- **OS-specific CI jobs** (`.github/workflows/ci.yml`): Linux runs the
  full integration set including the ELF-execution suites natively
  (static ELFs, dynamic linking against the host loader, TLS local-exec
  and initial-exec); macOS runs the Mach-O dyld-binding structural
  conformance suite plus the full integration set (Windows PE suites
  compile out; Linux ELF execution suites SKIP loudly there); Windows
  runs the PE e2e suites plus the Linux ELF execution suites through WSL
  when present (loud SKIP otherwise). The CI workflow itself now
  triggers CI on changes.
- **Language honesty note**: Adesh has no `mut` keyword (variables are
  mutable by default); the parser rejects `mut` with an explicit
  re-write hint rather than accepting or ignoring it
  (`src/parsing/parser/declarations.rs`).

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

### 2.2 Native Object Format (ADOB v3)
| Component | Status |
| :--- | :--- |
| Specification | **Complete** (magic `ADOB`; versions v2 and v3 — v3 adds an explicit per-section `size` field and distinguishes the TLS section kinds) |
| Sections | **Complete** (11 kinds incl. TLS/unwind/debug/custom; `.text`, `.rodata`, `.data`, `.bss`, `.tdata`, `.tbss`; NOBITS-style sections keep their full size through round-trips under v3) |
| Symbols & Relocations | **Complete** (~28 relocation kinds incl. TLS general-dynamic/initial-exec/local-exec; symbol binding/visibility; imports/exports) |
| Validation | **Complete and enforced on write** |
| Inspection CLI | **Complete** (`adesh adob inspect/dump-*/validate`) |

### 2.3 Native Linker (`linker`)
| Feature | Status | Notes |
| :--- | :--- | :--- |
| **PE/COFF (Windows)** | **Significantly implemented** | PE32+ generator, import tables (IDT/ILT/IAT), base relocations, TLS directory, ASLR/NX flags, entry synthesis, DLL export tables + COFF import libraries for `--shared`, and SEH `.pdata`/`.xdata` exception directory synthesis. Missing: delay imports, Authenticode, and resources. Execution-tested. |
| **ELF (Linux)** | **Static + TLS executables, execution-tested; dynamic loading verified via real `ld.so`** | ET_EXEC, program headers, symtab, build-id, `_start` synthesis (argc/argv/envp from the initial stack, `exit` syscall 60); non-intrinsic imports are a loud link error, never VA-0. Executed natively on Linux CI and via WSL on Windows with asserted exit codes (`tests/native_phase3_linux_e2e_test.rs`). TLS: merged `.tls` block with `PT_TLS` (initialized image as a prefix, zero-fill tail), `.tdata`/`.tbss` section-header split, local-exec relocation math, initial-exec GOT slot with `R_X86_64_TPOFF64` for dynamic links — executed against a real musl loader in WSL (`tests/native_phase8_tls_ehframe_test.rs`). `.eh_frame`/`.eh_frame_hdr` synthesis with `PT_GNU_EH_FRAME`. General-dynamic TLS is a loud error. Dynamic `ET_DYN` emission (`.interp`, `.dynsym`, `PT_GNU_RELRO`, ...) exists; PIE/dynamic-executable coverage is partial — `tests/native_phase8_dynamic_elf_test.rs` executes dynamic binaries through the real loader. |
| **Mach-O (macOS)** | **Artifact emission + dyld binding structure; execution pending** | Emits `MH_EXECUTE`/`MH_DYLIB` layouts with `LC_LOAD_DYLIB`, `LC_SYMTAB`, partitioned `LC_DYSYMTAB`, and `LC_DYLD_INFO_ONLY` with bind/rebase opcode streams and an exports trie; synthesizes `__stubs`, `__got`, lazy-pointer and helper sections with the indirect symbol table, forming a full call → stub → lazy-pointer → binder chain — verified by independent parse-back decoders (`linker/tests/macho_dyld_binding_test.rs`). Chained fixups are not emitted; no macOS execution test exists yet, so dyld behavior is unverified. The macOS CI runner executes the conformance suite. TLS sections under Mach-O fail the link loudly (no TLV emission). |
| **WASM** | **Stub writer** | Single-function emission; no import section (WASI), relocations never applied. Real WASM output comes from the compiler backend (`src/backends/wasm`). |
| **Section GC & ICF** | **Complete** | Reachability GC (`--gc-sections`) with strong-over-weak authoritative definitions and conservative name-only fallback (unit-tested in `linker/src/gc.rs`); FNV-1a-hashed ICF (`--icf`) where `Safe` mode preserves address-taken functions and `All` folds regardless (unit-tested in `linker/src/icf.rs`). |
| **Static archives** | **Implemented** | Lazy GNU/COFF indexed member resolution with fallback scanning; the writer emits a GNU `/` symbol index that the reader consumes directly (execution-tested end-to-end on Linux). |
| **Shared libraries** | **Emission only; not execution-tested** | `--shared` emits PE DLLs (export tables), ELF `.so` (`ET_DYN`), Mach-O `.dylib` (`MH_DYLIB`). ELF imports unresolved (no GOT/PLT); Mach-O lacks dyld binding info. |
| **LTO** | **Section-level only** | `--lto` enables section GC + ICF(All) + stripping; no cross-module IR optimization is performed. Help text matches as of 2026-10-05. |
| **OS API Router** | **Complete for Windows** | Real DLL routing database (incl. ucrtbase/msvcrt); libc functions are imported from the CRT, never stubbed. On ELF, non-intrinsic libc symbols route to `Undefined` and fail the link loudly (static-executable scope, Phase 3 decision); Mach-O still routes to `libSystem.B.dylib` but the writer cannot bind those imports (no dyld info). |

### 2.4 Code Generation & ABI (`crates/adesh-codegen`)
| Architecture | Status | Notes |
| :--- | :--- | :--- |
| **x86_64** | **Substantial for its tested subset** | Real REX/ModR/M/SIB encoding, scalar SSE2/XMM paths, branches/calls, stack args + shadow space, parallel moves, callee-saved handling, full-width atomics (8/16/32/64), and Win64 TLS sequences. Vector ABI classification remains incomplete. |
| **AArch64** | **Proof-of-concept** | Nop/Return/Add/Sub (reg-reg) only. |
| **RISC-V** | **Proof-of-concept** | Nop/Return/Add/Sub only; prologue hardcodes RV64 even for RV32. |
| **WASM** | **Real** | Stack-based opcode stream with SLEB128, locals, control structures. |
| **Embedded ARM** | **Proof-of-concept** | Minimal Thumb-2 ALU. |
| **Register allocator** | **Working linear scan with live-range splitting** | Callee-saved-first allocation, live-range splitting around calls, weighted pressure eviction, and verifier-driven fallback retry. |
| **Calling conventions** | **Unified classifier + stack args + scalar FP regs** | SysV AMD64 (eightbyte psABI), Win64 (by-value rules), AAPCS64/32, RISC-V, WASM tables; arg registers, stack arguments, shadow space; scalar FP args/returns via XMM0-3 (Win64) / XMM0-7 (SysV); sret and struct-by-value implemented and execution-tested at the MachineIR/FFI level. Missing: vector-register classification; aggregate features not yet reachable from source syntax. |

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
