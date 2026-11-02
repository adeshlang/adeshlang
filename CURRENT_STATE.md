# Adesh Native Production Toolchain — Current State

**Generated:** 2026-10-24 (native codegen + linker audit revision)
**Last updated:** 2026-11-02 (Phase 11 tier realignment and truth-in-the-CLI:
`linker/src/target.rs` derives its tiers from CI verification, both `adesh
target list` and `adeshlink targets` render that one table, RV32 is refused at
backend selection and at entry-stub synthesis, the Windows JIT suite escapes
import paths and is named in CI — §0.5, §0.7. Previous revision: 2026-11-01,
Phase 11 silent-fallback gate — AArch64 and RISC-V codegen both fail loudly
instead of emitting `NOP` placeholders or placeholder registers and condition
codes (§0, §0.5, §2.4); 2026-10-31, §0.6 workspace format & lint gate)
**Toolchain Version:** 0.3.0
**Target:** Production-Grade Self-Contained Native Toolchain
**Companion per-target document:** `TARGET_MATRIX.md` (same audit date) — the two
files must agree; any contradiction between them is a bug in the documentation.

---

## 0. Honest Status Summary

| Area | Verified Status |
| :--- | :--- |
| ADOB object format (v3) | **Implemented** — real binary spec, symmetric writer/reader, strict validator (enforced on write), round-trip tests. v3 adds an explicit per-section size field (v2 derived size from data length, silently dropping `.tbss`/`.bss` zero-fill tails) and distinguishes the TLS section kinds; the reader accepts both v2 and v3 and routes ADOB by version before any other format probe. |
| Native linker engine | **Implemented, actively maturing** — parse → GC → ICF → resolve → layout → relocate; indexed GNU/COFF archives use lazy member resolution with fallback scanning. |
| Windows PE generation | **Significantly implemented** — real PE32+ with imports/IAT/TLS/base-reloc/ASLR; tests execute produced binaries, including a real `msvcrt!puts` import. |
| x86-64 native codegen | **Substantial for its tested subset** — integer GPR operations, scalar SSE2 encodings, XMM allocation, branches/calls, stack arguments, parallel moves, Win64 shadow space, full-width locked atomics, and Win64 TLS access sequences. Vector calling-convention support remains incomplete. |
| AArch64 codegen | **Implemented, loud about what it cannot encode, not execution-verified** — Full AAPCS64 register allocation, callee-saved preservation (X19..X28, D8..D15), 64-bit immediate materialization, arithmetic, comparisons/branches, scalar FP, NEON vectors, atomics, ADRP+ADD symbol materialization, and ADOB/ELF object emission. **Status of the 2026-10-24 audit defects — all three are now closed.** (1) The approximate call-relocation offset is **fixed** (`emit_object` accumulates `text_bytes.len()` per function and adds it to each relocation). (2) ADRP/ADD page+pageoff symbol materialization **is now emitted** (`RelocationKind::AArch64_AdrPage21` + `AArch64_AddAbsLo12`). (3) The silent `NOP` fallback is **fixed (2026-11-01)**: the encoder match no longer ends in `_ => { NOP }`. Its final arm is a binding, `unsupported =>`, that returns a `CodegenError`, so even a `MachineInstruction` variant added later fails loudly instead of emitting a `NOP`. Every `MachineInstruction` variant without an AArch64 encoding — 29 of them, `ParallelMove` through `TlsAddress` — returns a `CodegenError` naming the variant and the containing function, and so does every operand form the encoders cannot express (an unallocated virtual register reaching the encoder, a `Call` whose target is neither a register nor a symbol, FP/SIMD stack slots, vector operands that are not `V` registers, atomics whose destination is not an `X` register). No instruction is dropped and no placeholder register is substituted. `tests/native_phase11_codegen_loudness_test.rs` pins the gate. Two miscompiles surfaced when the gate went in and are now loud instead of silent: spilled float/vector values reach the encoder as stack slots, which previously encoded as 64-bit GPR loads with the register number truncated to 5 bits (`V30` became `X30`, the link register); and `VectorAdd` between two stack slots encoded as `ADD V0.4S, V0, V0`. Realizing those forms needs `LDR`/`STR Dd, [SP]` — not implemented. Source builds still fail at link because no `aarch64` target runtime archive is resolved, and no qemu/hardware execution test exists. |
| RISC-V codegen | **Implemented backend, not reachable from source** — RV64GC (LP64D), scalar FP, atomics, vector encodings, and RVC alignment; unsupported instruction *variants* fail loudly. Real source builds currently stop at codegen with `unsupported machine instruction ParallelMove`. **Loud about operand forms since 2026-11-01**, when the AArch64 review gate was applied here: the placeholder-register `unwrap_or` substitutions in the vector, load/store and atomic arms are gone, `VectorLoad`/`VectorStore` are refused instead of emitting `vle.d v0, (sp)`, `SetCc`/`BranchCc` enumerate all 14 condition codes with `Parity`/`NotParity` refused, memory and atomic widths outside {1,2,4,8} are refused rather than defaulting to doubleword, an out-of-range `shamt` is refused rather than masked, and a branch whose target label is undefined or whose displacement escapes B-type ±4096 / JAL ±1 MiB fails instead of being patched to a placeholder. Each refusal is a `CodegenError` naming the instruction and the function; `tests/native_phase11_codegen_loudness_test.rs` pins 21 unencodable variants and 43 operand forms. What the gate turned up stays missing: spilled atomic operands are refused (AArch64 has the same contract; scratch materialization is not implemented), indexed addressing for `Load`/`Store` is refused, and `VectorLoad`/`VectorStore` have no working form at all. RV32 is refused **while the backend is selected** (`create_backend` returns a `CodegenError` naming the canonical triple, so no RV64 doubleword prologue can reach an RV32 object), and the linker refuses to synthesize its RV64 `_start` stub for an RV32 executable — both refusals are pinned by `tests/native_phase11_codegen_loudness_test.rs` and `linker/tests/phase11_riscv32_entry_stub_test.rs`. No execution test. |
| ELF generation | **Static and dynamic x86-64 executables execution-tested; TLS execution-verified** — AOT executables dynamically import libc and libgcc (`libc.so.6`, `libgcc_s.so.1`). ELF dynamic links emit PLT/JUMP_SLOT imports, local GOTPCREL slots, and COPY relocations for supported imported data objects such as `environ`; fixed-base `ET_EXEC` does not emit PIE-only `RELATIVE` relocations. Locally defined general-/local-dynamic TLS sequences are relaxed to initial-/local-exec forms; unsupported shapes remain loud errors. A glibc-target AOT print repro was linked and executed in Ubuntu 24.04. **Shared-library loading is execution-verified**: `tests/native_phase8_dynamic_elf_test.rs` builds `libadd42.so` (`ET_DYN` + `DT_SONAME` + exported dynsym) and drives it from a PIE caller through `dlopen`/`dlsym` with an asserted result. Static `ET_EXEC` with native Linux `_start` synthesis (argc/argv/envp + `exit` syscall 60) runs on Linux CI and WSL with asserted exit codes. TLS: merged `.tls` block (initialized sections first, zero-fill after), `PT_TLS`, `.tdata` (PROGBITS) / `.tbss` (NOBITS) section-header split, static local-exec relocation (`%fs:(off − memsz)`), and dynamic initial-exec (GOT slot + `R_X86_64_TPOFF64` resolved by the real loader) — **executed against a real musl `ld.so` in WSL with asserted exit codes** (`tests/native_phase8_tls_ehframe_test.rs`). `.eh_frame`/`.eh_frame_hdr` synthesis with `PT_GNU_EH_FRAME` (see DWARF row). |
| Mach-O generation | **Artifact emission, libSystem routing, and dyld binding structure implemented; execution pending** — emits 64-bit Mach-O executables and dylibs with `LC_LOAD_DYLIB`, `LC_SYMTAB`, partitioned `LC_DYSYMTAB`, and `LC_DYLD_INFO_ONLY` (bind/rebase opcode streams and an exports trie), an indirect symbol table, and synthesized `__stubs`/`__got`/lazy-pointer/helper sections forming a full call → stub → lazy-pointer → binder chain — all **verified by independent parse-back decoders** (`linker/tests/macho_dyld_binding_test.rs`). Both x86-64 and AArch64 images are supported; relocations include x86-64 SIGNED/BRANCH/GOT/TLV and ARM64 BRANCH26/PAGE21/PAGEOFF12/TLVP/ADDEND. TLS is emitted as `__thread_data`/`__thread_bss` plus a `__thread_vars` TLV descriptor table bound through `__tlv_bootstrap`. Chained fixups are not emitted. No macOS execution test exists yet: dyld execution verification is pending, covered structurally on the macOS CI runner which runs the conformance suite on the only host where Mach-O is native. |
| WASM | **Compiler emits WASI Preview 1 for `-wasi` triples; linker links multi-object programs; both executed on two engines in CI** — for `adesh build --target wasm32-*-wasi` the compiler backend imports exactly one host function, `wasi_snapshot_preview1.fd_write`, and emits its own write/alloc/print adapters, a mutable `__heap` global, and on-demand `memory.grow`, so the module runs on a bare WASI host (`tests/wasm_wasi_compiler_test.rs` drives the real CLI and asserts stdout under wasmtime). Non-WASI wasm targets (`--emit=wasm`, `wasm32-unknown-unknown`) keep the `env.*` imports (`print_f64`, `print_str`, `alloc`, `concat2`, `file_write_str`) served by a generated Node loader. The linker's WASM writer emits type/import/function/table/memory/global/export/element/data-count/code/data sections from a link plan built out of the parsed objects, applies function/global/data/table-index/table-number/type-index `R_WASM_*` relocations in place, resolves calls and data references that another input defines (dropping the phantom import and binding to the definition), rejects two strong definitions of one name and a signature disagreement loudly, defines the one `funcref` table and fills it from the inputs' element segments with slot 0 reserved for a null trap (so `call_indirect` links), parses object export sections (including `export_name` renames), and exports the entry instead of emitting a start section. `linker/tests/wasm_hello_world_test.rs`, `linker/tests/wasm_cross_object_test.rs`, `linker/tests/wasm_pure_module_test.rs`, `linker/tests/wasm_table_test.rs` and `tests/wasm_wasmtime_execution_test.rs` link real clang objects one and two at a time and execute the results with asserted output on **two engines** — V8 through Node and Cranelift through the workspace's `wasmtime` 36 dependency (which supplies its own `wasi_snapshot_preview1.fd_write` host, because wasmtime 33+ dropped WASI preview1, so no runner-installed CLI could). A two-object program whose calls stay inside the link produces a module with an **empty import section** that both engines instantiate with no host at all. The ubuntu CI job runs the nine WASM linker suites, the wasmtime execution suite, and the compiler's WASI suite. Defined tables/globals and non-MVP element segments remain loud errors. `wasm32-wasi`/`wasm32-none` are `Tier1Supported` — CI builds and runs their linked
modules on two engines with asserted output, which is exactly what the tier
now means (the linker still supplies no WASI runtime of its own; the host
provides `fd_write`). Executable builds never go through the linker — see the WASM executable-path note below. |
| Static libraries | **Implemented** — Target-aware GNU/COFF indexed archives resolve members lazily without host triple leakage; archives without a usable index fall back to scanning. The archive writer emits a real GNU `/` symbol index (big-endian offsets + string pool) and the reader consumes it directly. Thin archives are not supported. |
| Shared libraries | **Artifact emission for all three formats; ELF loading execution-verified** — `--shared` produces PE DLLs (with export tables), ELF `.so` (`ET_DYN` + `DT_SONAME` + exported dynsym + synthesized `.got`/`.dynamic`), and Mach-O `.dylib` (`MH_DYLIB` + export trie + dyld bind info). ELF `.so` is loaded with `dlopen`, resolved with `dlsym`, and called in `tests/native_phase8_dynamic_elf_test.rs`. PE DLL and Mach-O `.dylib` loading are emission-tested only. |
| TLS generation | **Implemented for Win64 (codegen) and ELF (linker), both execution-verified at their level** — Win64: PE TLS directory/`_tls_index` synthesis plus real thread-pointer access sequences (`gs:[0x58]` + SECREL relocations), execution-tested via MachineIR (`tests/native_phase2_abi_tls_test.rs`). ELF: `PT_TLS` with `.tdata`/`.tbss` split; static local-exec (`%fs:(off − memsz)`, verified by executing a binary that sets the thread pointer via `arch_prctl(ARCH_SET_FS)` and reads/updates its counter through `%fs`) and dynamic initial-exec (GOT slot + `R_X86_64_TPOFF64`, verified against a real musl `ld.so` in WSL) — `tests/native_phase8_tls_ehframe_test.rs`. General-dynamic is rejected loudly. Mach-O emits TLV sections (`__thread_data`/`__thread_bss` plus `__thread_vars` descriptors) but has no execution test. No Adesh source syntax emits TLS accesses yet — the executed tests drive the linker with hand-built ADOB objects. |
| Native JIT (`--jit-native` / `--njit`) | **Functional for the tested subset; imports handled by the shared static pre-link** — raw-pointer and array-indexing semantics now match the interpreter. Static imports are resolved and tree-shaken before codegen by the same path used by AOT/WASM (`src/cli/import_linking.rs`, `tests/static_imports_test.rs`); a module import that still reaches the native JIT compiler errors loudly (`"import of module '…' is not supported by --jit-native"` in `src/backends/jit/native/compiler.rs`). Conformance is asserted by `tests/native_phase8_jit_native_conformance_test.rs`, which is **10/10 on Windows and named in the Windows CI job since 2026-11-02** — three tests used to embed an unescaped Windows path in generated Adesh source (OS error 123); interpolated paths now go through `adesh_string_body`, which escapes `\` and `"`. |
| Full ABI aggregates/variadic | **Implemented at the codegen/FFI level** — shared SysV eightbyte + Win64 by-value classifier, sret for large aggregates, struct-by-value (1/2/4/8-byte) in registers and on the stack, Win64 shadow spill and SysV register-save-area/`va_list` variadic helpers; execution-tested via MachineIR/FFI suites. Reachability from source is split by backend: `extern "C"`/`extern` blocks **do parse** (`src/parsing/parser/extern_ffi.rs`, `TokenKind` for `extern` in `src/parsing/lexer.rs`) and **the interpreter binds them through libffi** (`StmtKind::ExternFunction` / `StmtKind::ExternBlock` in `src/execution/runtime_core/interpreter_core.rs` via `register_extern_function` + `wrap_foreign_function`), but HIR lowering discards those declarations (`src/parsing/hir_lower.rs`, the arm that lowers `ExternFunction`/`ExternBlock` to an empty block), so the native-JIT and AOT paths never see them. Variadic functions and aggregate value types remain unreachable from Adesh source syntax in every backend. |
| DWARF / CodeView / SEH / eh_frame | **Implemented** — Windows SEH `.pdata`/`.xdata` `UNWIND_INFO` and opcode encoding wired to PE Optional Header exception directory; DWARF 5 `.debug_line` state machine program and compilation unit headers implemented; ELF `.eh_frame` (one CIE matching the real emitted prologue `push rbp; mov rbp, rsp`, one FDE per sized function symbol, no FDE for size-0 functions — never fabricated) plus `.eh_frame_hdr` binary-search table and `PT_GNU_EH_FRAME`, decode-verified by an independent byte-level decoder in tests. No runtime unwinder test exists yet: frame data is verified to describe the real code ranges, not yet to unwind a crash. |
| Link-time optimization (`--lto`) | **Implemented at two levels** — the CLI routes native emission through `CompilerDriver` with `enable_lto`, where `LtoEngine` performs global dead-function elimination and inlining over the module set (the CLI currently passes a single module, so cross-module inlining has nothing to act on); the linker then applies section GC, ICF (`IcfMode::All`), and symbol stripping. |
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
subset. *(Superseded in part: scalar FP argument/return registers, sret, and
struct-by-value were added later and are execution-tested at the
MachineIR/FFI level; vector classification and source-level reachability are
still missing. See §0 and §2.4.)*

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
  *(Superseded 2026-10-24: dyld info, exports trie, and the indirect symbol
  table are now emitted and parse-back verified; execution is still
  unverified. See §0.5.)*
- Corrected the x86-64 calling-convention row: scalar FP argument/return
  registers are implemented (Win64 XMM0-3, SysV XMM0-7); aggregate
  classification, sret, vector classification, and variadics are not.
  *(Superseded in part: aggregate/sret/variadic support was added at the
  codegen/FFI level and is execution-tested, but is still not reachable from
  Adesh source syntax, and vector classification remains missing. See §0.)*
- Every native e2e test that links and executes a produced **Windows PE**
  binary is gated `#[cfg(all(target_os = "windows", target_arch = "x86_64"))]`,
  so Ubuntu's `cargo test --tests` compiles them out instead of attempting to
  run Windows PEs; all phase 8 suites were added to the Windows CI step
  list (`.github/workflows/ci.yml`). The Linux ELF execution suites run
  natively on Linux CI and through WSL on Windows.
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

- **Linux AOT dynamic linking now covers the bundled Rust standard library.**
  ELF AOT executables link against the target platform loader/libc instead of
  attempting an incomplete static link. The linker classifies the libc symbols
  referenced by bundled Rust std CGUs, emits COPY relocations for imported
  data objects, and uses real GOT slots for local GOTPCREL references. COPY
  slots are preserved by section GC; PIE/shared images alone receive
  `R_X86_64_RELATIVE` relocations for local GOT addresses. Locally defined
  x86-64 GD/LD TLS sequences are relaxed to IE/LE forms. The end-to-end
  glibc-target print repro linked and executed in Ubuntu 24.04; the linker
  regression test checks local GOT-slot contents, displacement, and PIE
  rebasing metadata.
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
  TLS access remains a loud error. *(Superseded 2026-10-24: Mach-O TLS no
  longer fails the link — TLV sections are emitted, though untested under
  dyld. See §0.5.)*
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
  fail loudly instead of misbehaving. *(Follow-up 2026-10-24: static imports
  are now resolved before codegen by the shared pre-link path; a module
  import that reaches the JIT compiler itself still fails loudly. See §0.5.)*
- **Cross-platform AOT CI follow-up**: GNU ELF imports now add `libm.so.6`
  when recognized math functions such as `sin` or `log` are imported; musl
  continues to use its libc math implementation. Mach-O archive scanning
  normalizes Darwin's external-symbol underscore so portable `aot_*` names
  resolve from the Rust runtime static library. Native-JIT tests are gated
  off on non-x86-64 hosts because Cranelift PLT support is x86-64-only;
  interpreter and AOT regression coverage remains enabled on Apple Silicon.
- **OS-specific CI jobs** (`.github/workflows/ci.yml`), started **manually only**
  (`on: workflow_dispatch`; a full matrix run is expensive, so nothing triggers on
  push or pull request): Linux runs the
  full integration set including the ELF-execution suites natively
  (static ELFs, dynamic linking against the host loader, TLS local-exec
  and initial-exec) and the WASM suites on two engines; macOS runs the Mach-O dyld-binding structural
  conformance suite plus the full integration set (Windows PE suites
  compile out; Linux ELF execution suites SKIP loudly there); Windows
  runs the PE e2e suites plus the Linux ELF execution suites through WSL
  when present (loud SKIP otherwise).
- **Language honesty note**: Adesh has no `mut` keyword (variables are
  mutable by default); the parser rejects `mut` with an explicit
  re-write hint rather than accepting or ignoring it
  (`src/parsing/parser/declarations.rs`).

## 0.5 Native Codegen & Linker Audit (2026-10-24)

Scope: `crates/adesh-codegen`, `src/backends/native`, and `linker/` for every
advertised target. Method: source inspection plus targeted test runs on a
Windows x86-64 development host. Execution claims below are only those a
checked-in test asserts; everything else is labelled emission-tested or
unverified.

Local test results (Windows x86-64, debug build; recorded in the 2026-10-24 audit and
not re-run since — the two structural suites have grown since, so the fractions below
describe that run, not today's suite size):

| Suite | Result |
| :--- | :--- |
| `adesh-codegen --lib` | 84 passed |
| `adesh-linker` library and integration tests | all passed |
| `linker/tests/macho_dyld_binding_test.rs` | all passed structurally (10 tests in the file as of 2026-10-31) |
| `linker/tests/platform_native_linking_conformance_test.rs` | all passed structurally (9 tests in the file as of 2026-10-31) |
| AArch64-specific suites | 7/7 |
| RISC-V-specific suites | 5/5 |
| Cross-target / cross-compile suites | 3/3 and 2/2 |
| `tests/static_imports_test.rs` | 17/17 |
| `tests/native_phase8_jit_native_conformance_test.rs` | 7/10 on Windows (test-side path escaping; suite is absent from Windows CI) |

Phase 11 loudness-gate run (Windows x86-64, debug build, 2026-11-01 — the suites
the AArch64 change touches, re-run after it landed):

| Suite | Result |
| :--- | :--- |
| `adesh-codegen --lib` | 87 passed |
| `tests/native_phase11_codegen_loudness_test.rs` | 6/6 (new) |
| `native_phase6_aarch64_full_e2e`, `native_phase7_aarch64_e2e`, `native_phase7_cross_target`, `native_phase8_cross_compile`, `native_phase8_determinism`, `native_phase8_pic_e2e`, `native_phase8_simd_e2e`, `native_phase8_abi_e2e`, `codegen_integration`, `adob_integration` | all passed, 0 failures |
| `adesh-linker` `codegen_machine_code_test` | 3/3 |
| `cargo clippy -p adesh-codegen --lib --all-features` | clean; `rustfmt --check` clean on the touched files |

Phase 11 loudness-gate run — RISC-V (Windows x86-64, debug build, 2026-11-01 — the
suites the RISC-V change touches, re-run after it landed):

| Suite | Result |
| :--- | :--- |
| `adesh-codegen --lib` | 87 passed |
| `tests/native_phase11_codegen_loudness_test.rs` | 14/14 (6 AArch64 + 8 new RISC-V) |
| `native_phase7_riscv64_e2e`, `native_phase7_cross_target`, `native_phase8_cross_compile`, `native_phase8_determinism`, `codegen_integration`, `adob_integration`, `test_all_backends_input` | all passed, 0 failures (31 tests) |
| `cargo clippy -p adesh-codegen --lib --all-features` | clean; `rustfmt --check` clean on the touched files |
| `cargo fmt --all --check` | still fails, **47 diffs in 7 files**, none of them touched by this change: `src/backends/wasm/compiler.rs` (41 sites, the in-flight `typeof`/`__type__` work), and one site each in `crates/adesh-runtime/src/lib.rs`, `src/backends/aot/cranelift_impl/instructions/calls/mod.rs`, `src/backends/native/lower.rs`, `src/execution/runtime_core/exec/stmt.rs`, `src/execution/runtime_core/interpreter_core.rs`, `src/parsing/hir_lower.rs` — the §8 workspace-format item, not this step |

Audited defects and gaps:

- **AArch64 silent miscompile — FIXED (2026-11-01)** — the instruction encoder no
  longer ends in a `_ =>` arm that emits a `NOP`; its final arm is a binding
  (`unsupported =>`) that returns an error, so a variant added later fails loudly
  too. Unsupported `MachineInstruction` variants (29 of them,
  `ParallelMove` through `TlsAddress`) and unsupported operand forms return a
  `CodegenError` that names the variant and the containing function; an unallocated
  virtual register is refused before encoding starts. Previously such instructions
  became a `NOP`: a hello-world `--emit=adob` produced 0xC0 bytes of `.text`
  containing 7 NOPs. `tests/native_phase11_codegen_loudness_test.rs` (6 tests) pins
  the gate, including a scan asserting emitted `.text` contains no unintended `NOP`
  and that every `AArch64_Call26` relocation lands on a word that is really a `BL`.
  Two miscompiles were proven by the new tests and are now loud rather than silent:
  spilled FP/vector operands arriving as stack slots previously encoded as 64-bit
  GPR loads with the register number truncated to 5 bits (`V30` became `X30`, the
  link register), and `VectorAdd` between stack slots became `ADD V0.4S, V0, V0`.
  Making those forms work needs `LDR`/`STR Dd, [SP]` (not implemented). The suite
  is not yet wired into CI.
- **RISC-V silent register substitution — FIXED (2026-11-01)** — the encoder no
  longer substitutes a placeholder register. Every arm now routes its operands
  through class-checked helpers — `get_gpr`/`get_fpr` (float and vector registers
  are numbered 32..63, so a 5-bit field reached by `f30` names `x30` instead of
  failing), `atomic_address` + `materialize_address`, `amo_funct3`, `mem_funct3` +
  `fp_mem_funct3`, `shift_amount`, `vector_arith` — and an unrecognized form
  returns a `CodegenError` naming the instruction and the function. Three concrete
  miscompiles the AArch64 gate had found here are now loud, and each was pinned by
  a test first: `VectorLoad`/`VectorStore` dropped their address operand and emitted
  `vle.d v0, (sp)` (refused outright, since making them work needs unit-stride
  vector addressing that is not implemented); `AtomicFetchAdd`/`AtomicExchange`/
  `AtomicCompareExchange` with an operand the allocator had spilled encoded against
  `sp`, updating the frame — and an `AMO` whose destination *was* understood still
  wrote `amoadd.d a0, a1, (sp)` because `rd` defaulted to 10 and the base to 2,
  which is now `amoadd.d a5, a5, (a4)`: the addend register is also the destination,
  matching what `defs()` declares; `Move` of an integer immediate into a float
  register emitted `addi x0, x0, imm`, discarding the value into the zero register.
  Widths default no more either: an access size outside {1,2,4,8} and a shift amount
  outside `shamt` both fail instead of silently using the doubleword form or a
  masked count, and the prologue save/restore helpers dropped their vestigial size
  parameter. A virtual register that reaches the encoder is refused with an error
  naming `lower_module`. `riscv64_rejects_unencodable_operand_forms` pins 43 forms
  and `riscv64_rejects_every_unencodable_instruction` the 21 variants with no arm.
  An atomic whose operands the allocator spilled is now a loud error rather than a
  frame write — the same contract AArch64 has; making it work needs scratch
  materialization (not implemented).
- **RISC-V silent condition-code fallback — FIXED (2026-11-01)** — `SetCc` and
  `BranchCc` enumerate all 14 `ConditionCode` variants, and `Parity`/`NotParity`
  are refused because RV64 has no parity flag to read; `encode_branch` returns
  `Option<u32>` so there is no path that maps them onto `beq`. Flag state is an
  `Option<(u32, u32)>` invalidated at every block entry and after every `Call`
  (which clobbers the scratch registers an immediate operand was materialized
  into), so a `setcc`/`branchcc` with no compare in its own block is an error
  instead of a stale register pair. Branch fixups can no longer resolve to a
  placeholder: an undefined target label, a conditional displacement beyond B-type
  ±4096, and a jump beyond JAL ±1 MiB each fail with the label named.
- **AArch64 relocation offsets — FIXED (2026-10-24/25)** — `emit_object` no longer
  derives a per-function `func_offset` as `index * 8`; it accumulates
  `text_bytes.len()` per function and adds that base to every relocation offset, so
  call relocations land on their real call sites. ADRP/ADD symbol materialization is
  also emitted now (`AArch64_AdrPage21` + `AArch64_AddAbsLo12` relocation kinds), so
  the "only `AArch64_Call26`" limitation recorded by the audit no longer holds.
- **AArch64 source builds do not link** — no `target/aarch64-*` runtime
  archive is resolved, leaving `aot_make_i64`, `aot_make_string`,
  `aot_print_cstr`, `aot_print_newline`, and `aot_string_concat` undefined.
- **RISC-V source builds stop at codegen** — `ParallelMove` is unsupported.
  RV32 is refused outright, and `riscv32imac-unknown-none-elf` /
  `thumbv7em-none-eabihf` have no end-to-end firmware path.
- **WASM executable path** — `linker/src/wasm/writer.rs` emits a module from a
  link plan and `linker/src/wasm/relocations.rs` applies the `R_WASM_*` fixups.
  `linker/tests/wasm_hello_world_test.rs` links each of two single-object clang
  hello-world variants (plain, and `export_name("_start")`) and executes them
  under Node with asserted stdout; `linker/tests/wasm_cross_object_test.rs` and
  `linker/tests/wasm_pure_module_test.rs` link two objects at a time — a caller
  whose callee imports a host function, a reader/writer of another object's
  data, and a pair that references nothing outside itself. The same fixtures are
  linked and executed again under Cranelift by
  `tests/wasm_wasmtime_execution_test.rs`, which uses the workspace's own
  `wasmtime` 36 dependency and provides the `wasi_snapshot_preview1.fd_write`
  host in-test — wasmtime 33+ dropped WASI preview1, so a `wasmtime run` of the
  CLI could never execute these modules and the runner does not need the binary.
  The last two suites instantiate the two-object modules with no host at all, so
  what `adeshlink` produces for a self-contained program is an independent
  module, not one that needs a runtime supplied to it. The ubuntu CI job runs
  every one of these suites. Function pointers link too: the plan defines one
  `funcref` table, fills it from the inputs' element segments with slot 0
  reserved for a null trap, and `linker/tests/wasm_table_test.rs` executes an
  indirect call inside one object, function pointers stored in data, and a
  cross-object `call_indirect` — on both engines. Defined tables/globals and
  non-MVP element segments are still loud errors.
  `adesh build` does **not** route WASM executables through the linker (the
  claim in an earlier revision was wrong): `src/cli/build.rs` sends every wasm
  target to the compiler backend, and the older `WasmWriter::encode_binary` call
  site is unreachable because that branch returns first. The working path is the
  compiler backend: it emits `wasi_snapshot_preview1.fd_write` for `-wasi`
  triples and the `env.*` imports plus a generated Node loader for every other
  wasm target. `adeshlink --target wasm32-*` is the only route to the linker
  writer.
- **Mach-O has no execution test** on either architecture; dyld acceptance of
  the produced bind/rebase/stub chain is unverified.
- **Target tiers now report verification — closed 2026-11-02.** `Target::tier()`
  in `linker/src/target.rs` used to label every x86-64 **and AArch64** target
  `Tier1Supported` and RISC-V `Tier2Experimental` regardless of evidence. It is
  now derived from what CI does: **Tier 1** = a CI job builds the artifact and
  runs it with asserted output or exit status (`x86_64-pc-windows-*`,
  `x86_64-unknown-linux-gnu`, `wasm32-unknown-wasi`, `wasm32-unknown-unknown`
  — the WASM pair executes on V8 and Cranelift, which is why they moved *up*
  from the earlier realignment while AArch64 moved down); **Tier 2** = emitted
  and parsed back in CI, never executed (AArch64 Linux/macOS/Windows,
  `x86_64-apple-darwin`, RISC-V 64, musl — musl is emission-covered by
  `native_phase8_cross_compile_test` but no CI job loads it); **Tier 3** =
  declared only. `linker/tests/phase11_target_tiers_test.rs` pins each label
  and forbids an advertised summary from claiming more than its tier.
- **The target lists are data-driven — closed 2026-11-02.** `adesh target list`
  (`src/main.rs`) and `adeshlink targets` (`linker/src/main.rs`) both render
  `ADVERTISED_TARGETS` / `advertised_by_tier()` / `TargetTier::advertised_heading()`,
  so neither can advertise a target the tier table grades higher than its
  evidence. What changed hands: Android (`aarch64-linux-android`,
  `armv7-linux-androideabi`, `aarch64-apple-ios`) and the old "Mobile"/"Embedded"
  prose groups are **gone**; `i686`, `riscv32*`, `armv7`/Thumb and
  `powerpc64le` are still printed, but under **Tier 3 with a summary that names
  the refusal**, and `refused_targets_are_not_advertised_as_supported` forbids a
  Tier-3 summary from reading like support. Reader-only formats (MIPS,
  LoongArch, SPARC, AIX XCOFF, SPIR-V, Hexagon, ANE, Ethos-U, TPU) are not
  offered as build targets at all.
- **`x86_64-pc-windows-gnu` moved Tier 1 → Tier 2 while realigning.** Every
  suite that links **and runs** a PE asks for `-msvc`; nothing requests a MinGW
  triple, so the shared PE writer's `-gnu` flavour has no execution evidence.
  `LibcFlavor::Msvc` was added to `linker/src/target.rs` to make that
  distinguishable — `triple_string()` normalizes to `arch-os` and cannot tell
  the two apart on its own.
- **Windows CI gap — closed 2026-11-02.** The native-JIT conformance suite failed
  3/10 because generated `import ... from "C:\..."` statements embedded an
  unescaped absolute path (the lexer turns an unknown `\X` into `X`, so
  `C:\Users\…` became `C:Users…` → OS error 123). Every interpolated path now
  goes through `adesh_string_body`, the suite passes 10/10, and it is named in
  the Windows CI job alongside the Phase 11 loudness and tier suites.
- **A silent fallback remains in the linker** — `linker/src/arch/mod.rs:44`
  maps `Arch::X86` to `X86_64Arch`, so an i686 link runs the 64-bit relocation
  handler instead of failing. It is Tier 3 and un-advertised, and is recorded
  in `TARGET_MATRIX.md` as the next application of the Phase 11 rule outside the
  backends.

Verified improvements recorded by this audit (previously understated or
contradicted in this file):

- Mach-O TLV emission exists (`__thread_vars` descriptors plus x86-64
  `X86_64_RELOC_TLV` and ARM64 TLVP/ADDEND), and Mach-O x86-64 relocations
  (SIGNED/BRANCH/GOT/TLV) exist.
- ELF shared libraries execute: `dlopen`/`dlsym`/indirect call is asserted.
- Static imports (named/default/transitive/duplicate, tree-shaking, loud
  failures) work across NJIT, AOT, and WASM.
- `--lto` does run the IR-level `LtoEngine` before the linker's section pass.

Consequence under the Documentation Policy: no AArch64, Mach-O or RISC-V
capability may be called "implemented" until an artifact is executed and
asserted in CI — they are Tier 2, and `TARGET_MATRIX.md` says "emission-tested"
for those rows. The WASM linker path no longer belongs in that list: CI has run
its linked modules on two engines with asserted output since 2026-10-31, which
is why `linker/src/target.rs` now grades it Tier 1. The ordered remediation
priorities are under "Next important" in `TARGET_MATRIX.md`; the phase-by-phase
execution plan is `NEXT_IMPLEMENTATION_PLAN.md` (Phases 11–21).

## 0.6 Workspace Format & Lint Gate (2026-10-31)

Measured on this working tree, uncommitted (Windows x86-64, `-j 2`). **Re-measured 2026-11-01**; the in-flight `typeof`/`__type__` edits grew both counts since the 2026-10-31 pass.

| Check | Command | Result |
| :--- | :--- | :--- |
| Formatting | `cargo fmt --all -- --check` | **Failing** — re-measured 2026-11-01 after the RISC-V loudness gate landed: **47 diffs across 7 files**, all inside the in-flight work: `src/backends/wasm/compiler.rs` (41 sites), and one site each in `src/backends/aot/cranelift_impl/instructions/calls/mod.rs`, `src/backends/native/lower.rs`, `crates/adesh-runtime/src/lib.rs`, `src/execution/runtime_core/exec/stmt.rs`, `src/execution/runtime_core/interpreter_core.rs`, `src/parsing/hir_lower.rs`. `cargo fmt --all` clears them. The workspace was fmt-clean before those edits landed. None of the seven is a file this round touched — the three it did touch (`targets/riscv/mod.rs`, `tests/native_phase11_codegen_loudness_test.rs`, `tests/native_phase7_riscv64_e2e_test.rs`) pass `rustfmt --edition 2024 --check` individually. |
| Clippy errors | `cargo clippy --workspace --all-targets --all-features` | **Exit 0 — no errors.** `correctness` is denied workspace-wide, so this is a real gate, not a silence. |
| Clippy warnings | same command | **31 remain** (was 30), in four places (see below). CI does not fail on these: the `Run Clippy` step has no `-D warnings`. |

Where the warnings live:

- `adesh-runtime` — **25** (was 24): 23 `manual_c_str_literals` (still hand-building
  nul-terminated strings, e.g. `b"u8\0".as_ptr() as *const c_char`, in `aot_typeof`
  in `crates/adesh-runtime/src/lib.rs`) and 2 `collapsible_if` in the same file
  (the `__type__` lookups). `crates/adesh-runtime/src/sys_io.rs` was already
  converted to `c"…"` literals.
- `adeshlang` (root lib) — **1**: unused variable `is_export` on
  `StmtKind::TypeAlias(ta, is_export)` in
  `src/execution/runtime_core/interpreter_core.rs` — part of the in-flight work.
- `als` — **1**: unused variable `doc` in `als/src/diagnostics.rs`.
- `tests/native_phase9_pgo_e2e_test.rs` — **4**: 2 needless `mut`, and 2 cases
  where this pass added struct-update syntax to a literal that already specified
  every field, so the update is a no-op. Those two are self-inflicted by the lint
  sweep and should be reverted rather than "fixed" another way.

Cleared to zero warnings in this round: `adesh-editor`, `adesh-format`,
`adesh-wasm`, `adesh-codegen`, `adesh-object`, `mobile/bridge` (safety docs), plus
the workspace's `correctness` errors everywhere.

**What "clean" does not mean here.** Two crates silence their own lints, and an
in-source attribute overrides Cargo's `[lints]` flags, so their zero-warning output
is not evidence:

- `linker/src/lib.rs` and `linker/src/main.rs` set `#![allow(clippy::all)]`, which
  suppresses even the denied `correctness` group in the linker.
- `src/lib.rs` carries a long `#![allow(...)]` block with per-lint rationale
  (`collapsible_if`, `manual_strip`, `needless_borrow`, `too_many_arguments`, …)
  covering the root crate's own sources. It does **not** apply to `tests/` or
  `benches/`, which are separate crates — that is why the test-only fixes in this
  round were necessary.

Ordered next actions for this gate are recorded in `NEXT_IMPLEMENTATION_PLAN.md` §8.
Nothing from this round is committed: the tree still holds the `typeof`/`__type__`
work alongside it, and that work is what currently fails `cargo fmt --all -- --check`.

## 0.7 Phase 11 Tier Realignment (2026-11-02)

Phase 11's remaining items — RV32 refusal at validation, Windows JIT test
honesty, and tier realignment — landed with this change set. The rule the step
enforces is that a tier describes **what CI verifies**, never what a backend
claims.

**Code changes**

- `linker/src/target.rs`: `Target::tier()` re-derived from CI evidence, each arm
  commented with the suite that earns it. Tier 1 — `x86_64-pc-windows-msvc`,
  `x86_64-unknown-linux-gnu`, `wasm32` WASI and freestanding. Tier 2 —
  AArch64 (Linux/macOS/Windows), `x86_64-apple-darwin`,
  `x86_64-pc-windows-gnu`, `x86_64-unknown-linux-musl`, RISC-V 64. Tier 3 —
  everything else, including the targets previously printed as
  production-ready (`i686`, `armv7`, `riscv32*`). Distinguishing the two
  Windows flavours needed a new `LibcFlavor::Msvc`, because `triple_string()`
  normalizes to `arch-os` and reports `-gnu` and `-msvc` identically.
  New `AdvertisedTarget`/`ADVERTISED_TARGETS`/`advertised_by_tier()` and
  `TargetTier::advertised_heading()` make one table feed every user-facing list.
- `src/main.rs` (`adesh target list`), `linker/src/main.rs` (`adeshlink
  targets`) and `src/toolchain/cli/help.rs` render that table instead of
  hand-maintained lists; Android, iOS, i686 and ARM32 are no longer offered.
- `crates/adesh-codegen/src/targets/mod.rs`: `create_backend` refuses
  `Architecture::RiscV32` with a `CodegenError` that names the canonical triple,
  the reason and a `riscv64gc` alternative — before a single function body is
  encoded, so no RV64 doubleword prologue can reach an RV32 object.
- `linker/src/linker.rs`: `_start` synthesis is gated on x86_64/AArch64/RISC-V-64;
  an RV32 executable without a provided `_start` is a loud error rather than a
  64-bit stub.
- `tests/native_phase8_jit_native_conformance_test.rs`: interpolated paths go
  through `adesh_string_body`, which escapes `\` and `"` — the lexer collapses an
  unknown `\X` to `X`, so an unescaped `C:\Users\…` import path named a
  nonexistent file (OS error 123).
- `.github/workflows/ci.yml`: a `Test Phase 11 loudness and tier suites` step on
  all three hosts, and `native_phase8_jit_native_conformance_test` added to the
  Windows job. **Local edit only — the workflow is `workflow_dispatch`-only, so
  no CI run has executed these steps yet.**

**Measured on this working tree (Windows x86-64, debug, `-j 1`)**

| Check | Result |
| :--- | :--- |
| `cargo test --test native_phase11_codegen_loudness_test` | **15 passed** — the 14 from 2026-11-01 plus `riscv32_is_refused_at_backend_selection` |
| `cargo test -p adesh-linker --test phase11_target_tiers_test` | **5 passed** (new) |
| `cargo test -p adesh-linker --test phase11_riscv32_entry_stub_test` | **3 passed** (new) |
| `cargo test -p adesh-linker --test abi_and_unwind_actions_test` | **4 passed** — `test_target_maturity_tier_classification` rewritten for the verification semantics |
| `cargo test -p adesh-linker` (whole package) | **all suites passed** — re-run after `LibcFlavor::Msvc` was introduced and the `tier()` Windows arms were split; 0 failures across the package |
| `cargo test --test native_phase8_jit_native_conformance_test` | **10 passed** (was 7/10 on this host before the escaping fix) |
| `cargo test --test native_phase8_cross_compile_test` | **2 passed** with `x86_64-unknown-linux-musl` added to the emission triple list |
| `cargo test --lib` | **933 passed**, 0 failed — after the `src/main.rs` / `src/toolchain/cli/help.rs` list changes |
| `cargo test --test diagnostics_cli` | **8 passed** |
| `adesh build --target riscv32imac-unknown-none-elf` on a hello-world | **exit 1** with `Target: riscv32-none-eabi` / `Architecture: riscv32` and no `Function:` line — proof the refusal happens before encoding. The same file on `riscv64gc-unknown-linux-gnu` exits 1 too, but with `Function: main` and an unencodable `Move`, i.e. a different, later failure |
| `rustfmt --edition 2024 --check` on every file touched here | **clean** |
| `cargo clippy -p adesh-codegen -p adesh-linker --all-targets` | **no errors, no warnings** |

**Open at the time of writing**

- `x86_64-unknown-linux-musl` was added to
  `tests/native_phase8_cross_compile_test.rs::test_cross_target_emission_and_validation`
  so its Tier-2 label is backed by an emission check CI actually runs, not only
  on a developer's WSL session. It passes locally (2/2); like the CI steps
  above, it is only *claimed* for CI until a dispatch run proves it.
- `linker/src/arch/mod.rs:44` still routes `Arch::X86` to `X86_64Arch`. Found
  while writing the Tier-3 comments; recorded in `TARGET_MATRIX.md` and left
  open rather than fixed in passing, since closing it needs the same
  refuse-loudly decision as RV32 and its own test.
- The Phase 11 CI steps are unverified by CI itself until a dispatch run.

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
   (static and dynamic executable pipelines executed, including `dlopen`
   shared libraries and TLS), Mach-O (emission plus dyld binding structure,
   no execution test), WASM (linker links one- and two-object clang programs,
   applies relocations, resolves cross-object calls and indirect calls through
   a table it defines, and CI executes the results on two engines — V8/Node and
   wasmtime/Cranelift; the compiler's own WASI Preview 1 path is
   execution-verified for `-wasi` triples, while the linker still supplies no
   WASI runtime), archive
   ingestion/production, section GC, ICF, OS API routing, link maps.
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
| **ELF (Linux)** | **Static + TLS executables, execution-tested; dynamic loading verified via real `ld.so`** | ET_EXEC, program headers, symtab, build-id, `_start` synthesis (argc/argv/envp from the initial stack, `exit` syscall 60); in static `ET_EXEC` links, non-intrinsic imports are a loud link error, never VA-0. Executed natively on Linux CI and via WSL on Windows with asserted exit codes (`tests/native_phase3_linux_e2e_test.rs`). TLS: merged `.tls` block with `PT_TLS` (initialized image as a prefix, zero-fill tail), `.tdata`/`.tbss` section-header split, local-exec relocation math, initial-exec GOT slot with `R_X86_64_TPOFF64` for dynamic links — executed against a real musl loader in WSL (`tests/native_phase8_tls_ehframe_test.rs`). `.eh_frame`/`.eh_frame_hdr` synthesis with `PT_GNU_EH_FRAME`. General-dynamic TLS is a loud error. Dynamic `ET_DYN` emission (`.interp`, `.dynsym`, `PT_GNU_RELRO`, ...) is execution-tested: `tests/native_phase8_dynamic_elf_test.rs` runs dynamic binaries through the real loader, and also `dlopen`s a produced `.so`, resolves an export with `dlsym`, and calls it. Non-intrinsic imports are resolved through PLT/JUMP_SLOT entries; the ELF writer emits local GOTPCREL slots, COPY relocations for imported data, and PIE-only `R_X86_64_RELATIVE` rebasing. |
| **Mach-O (macOS)** | **Artifact emission + dyld binding structure; execution pending** | Emits `MH_EXECUTE`/`MH_DYLIB` layouts with `LC_LOAD_DYLIB`, `LC_SYMTAB`, partitioned `LC_DYSYMTAB`, and `LC_DYLD_INFO_ONLY` with bind/rebase opcode streams and an exports trie; synthesizes `__stubs`, `__got`, lazy-pointer and helper sections with the indirect symbol table, forming a full call → stub → lazy-pointer → binder chain — verified by independent parse-back decoders (`linker/tests/macho_dyld_binding_test.rs`). Chained fixups are not emitted; no macOS execution test exists yet, so dyld behavior is unverified (the macOS CI runner executes the structural conformance suite). Both x86-64 and AArch64 images are supported, with x86-64 SIGNED/BRANCH/GOT/TLV and ARM64 BRANCH26/PAGE21/PAGEOFF12/TLVP/ADDEND relocations. TLS is emitted as `__thread_data`/`__thread_bss` plus a `__thread_vars` TLV descriptor table bound through `__tlv_bootstrap`; it has no execution test. |
| **WASM** | **Linker links multi-object programs and indirect calls; CI executes its output on two engines** | The module writer emits type/import/function/table/memory/global/export/element/data-count/code/data from a link plan built out of the parsed objects; `linker/src/wasm/relocations.rs` patches the function/global/data/table placeholders in place; a call or data reference that another input defines binds to that definition and its phantom `env` import is dropped, while two strong definitions of one name (`LNK002`), a signature disagreement (`LNK007`), the address of data nobody defines (`LNK001`) and a table entry naming a host function (`LNK001`) are loud errors; the plan defines the one `funcref` table, fills it from element segments with slot 0 reserved for a null trap, and binds `call_indirect`'s table-index/table-number/type-index fixups; object export sections are parsed and the entry is exported (no start section). Real clang objects are linked singly and two at a time — including indirect calls inside one object, function pointers stored in data, and a cross-object `call_indirect` — then executed in the ubuntu CI job under V8 (`linker/tests/wasm_hello_world_test.rs`, `wasm_cross_object_test.rs`, `wasm_pure_module_test.rs`, `wasm_table_test.rs`) and under Cranelift (`tests/wasm_wasmtime_execution_test.rs`, on the workspace's `wasmtime` dependency with an in-test `fd_write` host), all with asserted output. A two-object program that calls only itself links to a module with an empty import section, which both engines instantiate with no host. Defined tables/globals and non-MVP element segments remain loud errors. Real WASM output for normal builds still comes from the compiler backend (`src/backends/wasm`), which emits `wasi_snapshot_preview1.fd_write` for `-wasi` triples — `tests/wasm_wasi_compiler_test.rs` builds through the real CLI and executes the result under wasmtime with asserted stdout — and `env.*` host imports plus a generated Node loader for every other wasm target; the linker itself supplies no WASI runtime. |
| **Section GC & ICF** | **Complete** | Reachability GC (`--gc-sections`) with strong-over-weak authoritative definitions and conservative name-only fallback (unit-tested in `linker/src/gc.rs`); FNV-1a-hashed ICF (`--icf`) where `Safe` mode preserves address-taken functions and `All` folds regardless (unit-tested in `linker/src/icf.rs`). |
| **Static archives** | **Implemented** | Lazy GNU/COFF indexed member resolution with fallback scanning; the writer emits a GNU `/` symbol index that the reader consumes directly (execution-tested end-to-end on Linux). |
| **Shared libraries** | **Emission for all three formats; ELF loading execution-tested** | `--shared` emits PE DLLs (export tables), ELF `.so` (`ET_DYN` + `DT_SONAME` + exported dynsym + GOT/`.dynamic`), and Mach-O `.dylib` (`MH_DYLIB` + export trie + dyld bind info). ELF `.so` is loaded via `dlopen`/`dlsym` and called in `tests/native_phase8_dynamic_elf_test.rs`; PE DLL and Mach-O `.dylib` loading remain emission-tested only. |
| **LTO** | **IR-level plus section-level** | `--lto` runs `LtoEngine` (global dead-function elimination and inlining) through `CompilerDriver`, then enables linker GC + ICF(All) + stripping. The CLI currently feeds one module, so cross-module inlining has nothing to act on; there is no whole-program multi-module pipeline yet. Help text matches as of 2026-10-05. |
| **OS API Router** | **Complete for Windows; dynamic routing on ELF and Mach-O** | Real DLL routing database (incl. ucrtbase/msvcrt); libc functions are imported from the CRT, never stubbed. On ELF, dynamic links emit PLT/JUMP_SLOT imports resolved by the real loader (static links still fail loudly on unresolved symbols); Mach-O routes to `libSystem.B.dylib` and the writer emits dyld bind info for those imports. |

### 2.4 Code Generation & ABI (`crates/adesh-codegen`)
| Architecture | Status | Notes |
| :--- | :--- | :--- |
| **x86_64** | **Substantial for its tested subset** | Real REX/ModR/M/SIB encoding, scalar SSE2/XMM paths, branches/calls, stack args + shadow space, parallel moves, callee-saved handling, full-width atomics (8/16/32/64), and Win64 TLS sequences. Vector ABI classification remains incomplete. |
| **AArch64** | **Implemented, loud about what it cannot encode, execution-unverified** | AAPCS64 register allocation, callee-saved X19-X28/D8-D15, 64-bit immediate materialization, scalar FP, NEON, atomics, and ADRP+ADD (`AArch64_AdrPage21`/`AArch64_AddAbsLo12`) symbol materialization. Both audited codegen defects are fixed: `emit_object` bases each relocation on the accumulated `.text` length instead of `index * 8`, and (2026-11-01) the encoder's `_ =>` NOP fallback is gone — every unencodable variant and operand form returns a `CodegenError` naming the instruction and its function. Pinned by `tests/native_phase11_codegen_loudness_test.rs`. Refused rather than mis-encoded today: FP/SIMD stack access (needs `LDR`/`STR Dd, [SP]`), and the 29 variants with no AArch64 encoding. No target runtime archive and no execution test. |
| **RISC-V** | **Implemented backend, loud about what it cannot encode, blocked from source** | RV64GC LP64D, scalar FP, atomics, vector encodings, and RVC alignment; unsupported instruction *variants* fail loudly. Real source builds stop at `ParallelMove`; RV32 is explicitly refused and the linker still uses an RV64 entry stub for RV32 targets. The silent-fallback class the AArch64 gate closed was fixed here the same day (2026-11-01): placeholder `unwrap_or` registers are gone from the vector, load/store and atomic arms, `VectorLoad`/`VectorStore` no longer hardcode `sp` as their base, `SetCc`/`BranchCc` enumerate every condition code instead of defaulting `Parity`/`NotParity` to a constant `0` or to `Equal`, widths and shift amounts outside their encodable ranges are refused, and unresolved or out-of-range branch targets fail instead of being patched to a placeholder. Pinned by `tests/native_phase11_codegen_loudness_test.rs` (21 variants, 43 operand forms). Refused rather than mis-encoded today: spilled atomic operands, indexed `Load`/`Store`, and any vector load or store. No execution test. |
| **WASM** | **Real compiler backend; emits WASI Preview 1 for `-wasi` triples; linker writes, relocates and cross-links modules** | Stack-based opcode stream with SLEB128, locals, and control structures; for `-wasi` triples it imports only `wasi_snapshot_preview1.fd_write` and emits in-module write/alloc/print adapters, a `__heap` global and `memory.grow`, while other wasm targets keep `env.*` imports consumed by a generated Node loader. The linker (separate crate path) emits, relocates and cross-links clang objects, and needs no external toolchain to do it. |
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
