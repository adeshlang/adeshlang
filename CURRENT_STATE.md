# Adesh Native Production Toolchain — Current State

**Generated:** 2026-10-24 (native codegen + linker audit revision)
**Last updated:** 2026-11-07 (RISC-V passes codegen — §0.12. A `Move` from a `Symbol` now
encodes as `auipc`/`addi`, `ParallelMove` is expanded before encoding, and `emit_object`
defines the string pool it names, so `riscv64gc-unknown-linux-gnu` hello-world builds stop
at `No runtime archive` instead of `unsupported machine instruction`. The `+4` LO12 addend
that makes the pair correct is proven on **linked image bytes**: 17/17 pairs reconstruct
their symbol exactly across a `0x1080` delta span, 8 of them with `lo12` bit 11 set so
`hi20` rounds up, and an A/B link with the addend removed lands four bytes low. Writing
that test surfaced a silent-corruption bug — `AdobV2::encode`/`decode` downgraded every
RISC-V relocation kind to `Absolute64`, which `adeshlink strip` exercises on real objects;
ids 30–33 now round-trip and unknown kinds refuse with `LNK008`. Linker suite green,
codegen 107, loudness 20, cross-link 9, phase7 RISC-V 5, fmt and clippy clean. Still no
execution: no riscv64 archive on this host, no qemu. Earlier: 2026-11-05 (Mach-O link fix — §0.11. `x86_64-apple-darwin` now links a
source program. The `LNK009` measured on 2026-11-04 was in the compiler, not the writer:
the x86_64 backend materialized a symbol address as `movabs` + `Absolute64` inside
`.text`, and a Mach-O image is always PIE while dyld applies rebase fixups only in
`__DATA`/`__DATA_CONST`, so no source program could link. The backend emits
`lea r64, [rip + disp32]` + `PcRelative32` when the image is Mach-O and keeps `movabs`
for PE and ELF; the linked image is byte-verified (the `lea` resolves onto the program's
own string in `.rodata`, `MH_PIE` set, archive `aot_*` symbols present). Two findings
along the way: the first attempt at this fix could not fire, because
`TargetDescriptor::object_format` names the ADOB *container* and is always `Adob` —
image format now comes from a new `image_format()`; and `LNK009` was unactionable, so
Mach-O rebase records carry the object, section, offset and symbol behind them. Suite is
9/9. Same day, earlier: Phase 12 item 4 — `tests/native_phase12_cross_link_test.rs`
links a hello-world to each of the 8
advertised triples and asserts on the **image bytes** (ELF/PE/Mach-O headers plus the
`aot_*` symbols), not the exit code: 8/8 pass, 4 targets link (three x86-64 archives
and `aarch64-unknown-linux-gnu`), the rest fail loudly for named reasons. Getting
AArch64 to link needed a codegen fix — `emit_object` referenced `__str_<n>` without
defining the string pool, so the link failed `LNK001`; `emit_module_data_sections`
now emits `.rodata`/`.tls`/`.data` as x86_64 always has. Measuring Mach-O found a
codegen defect instead of a packaging gap: `x86_64-apple-darwin` resolved its archive
and failed `LNK009`, because the x86_64 backend baked an absolute symbol address into
`.text`, which no PIE image can carry — first blamed on the writer, closed 2026-11-05
(§0.11). A `cross-link` CI
job stages the `runtime-archives` artifacts at `target/<triple>/debug/` on all three
OSes and runs the suite (neither job dispatched yet). Same day, earlier revision:
Phase 12 items 2
and 3 — `scripts/build-runtime-
archives.sh` builds and stages one runtime archive per triple at the debug
profile, packaging now ships `lib/<triple>/` archives on Unix, Docker and
Windows, `adl repair` and `adl doctor` name the archive that actually exists and
repair builds at the profile the binary searches first,
`.github/workflows/ci.yml` gains a `runtime-archives` job for the Linux and Apple
cross targets (code-complete, not yet dispatched), and the archive story is
written through `TARGET_MATRIX.md`, `ARCHITECTURE_GAPS.md`,
`docs/AOT_COMPILATION.md`, `docs/RELEASE.md` and `docs/cli-guide.md`. Measuring it
removed the last
`adeshlang.*` runtime-archive candidate: on a real host that name is the cdylib's
import library, so a host link resolved "a runtime", linked it, and died on
`undefined reference to aot_print_cstr`. §0.9. Previous revision the same day:
Phase 12 item 1 — runtime archive resolution is
target-scoped: `src/backends/aot/cranelift_impl/linking.rs` searches only
locations that name the requested triple before it consults anything else, the
process-wide memo cache is keyed by triple, and a missing archive is an error
naming the triple, every path searched and the command that builds it; §0.8.
Previous revision: 2026-11-02, Phase 11 tier realignment and truth-in-the-CLI:
`linker/src/target.rs` derives its tiers from CI verification, both `adesh
target list` and `adeshlink targets` render that one table, RV32 is refused at
backend selection and at entry-stub synthesis, the Windows JIT suite escapes
import paths and is named in CI — §0.5, §0.7. Earlier: 2026-11-01,
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
| AArch64 codegen | **Implemented, loud about what it cannot encode, not execution-verified** — Full AAPCS64 register allocation, callee-saved preservation (X19..X28, D8..D15), 64-bit immediate materialization, arithmetic, comparisons/branches, scalar FP, NEON vectors, atomics, ADRP+ADD symbol materialization, and ADOB/ELF object emission. **Status of the 2026-10-24 audit defects — all three are now closed.** (1) The approximate call-relocation offset is **fixed** (`emit_object` accumulates `text_bytes.len()` per function and adds it to each relocation). (2) ADRP/ADD page+pageoff symbol materialization **is now emitted** (`RelocationKind::AArch64_AdrPage21` + `AArch64_AddAbsLo12`). (3) The silent `NOP` fallback is **fixed (2026-11-01)**: the encoder match no longer ends in `_ => { NOP }`. Its final arm is a binding, `unsupported =>`, that returns a `CodegenError`, so even a `MachineInstruction` variant added later fails loudly instead of emitting a `NOP`. Every `MachineInstruction` variant without an AArch64 encoding — 29 of them, `ParallelMove` through `TlsAddress` — returns a `CodegenError` naming the variant and the containing function, and so does every operand form the encoders cannot express (an unallocated virtual register reaching the encoder, a `Call` whose target is neither a register nor a symbol, FP/SIMD stack slots, vector operands that are not `V` registers, atomics whose destination is not an `X` register). No instruction is dropped and no placeholder register is substituted. `tests/native_phase11_codegen_loudness_test.rs` pins the gate. Two miscompiles surfaced when the gate went in and are now loud instead of silent: spilled float/vector values reach the encoder as stack slots, which previously encoded as 64-bit GPR loads with the register number truncated to 5 bits (`V30` became `X30`, the link register); and `VectorAdd` between two stack slots encoded as `ADD V0.4S, V0, V0`. Realizing those forms needs `LDR`/`STR Dd, [SP]` — not implemented. **A source build now links.** Until 2026-11-04 it stopped at the link with `error[LNK001]: undefined reference to __str_0`, because `emit_object` wrote `.text` only: the shared lowering fills the string pool for every backend and the encoder emits `ADRP`/`ADD` pairs naming `__str_<n>`, but only x86_64 defined those sections. `emit_module_data_sections` now emits `.rodata`, `.tls` and `.data` on the AArch64 path (pinned by `test_aarch64_emit_object_defines_the_string_pool`), and `tests/native_phase12_cross_link_test.rs` measures a 277,416-byte ELF with `e_machine` 183 and 8 `aot_*` definitions for `adesh build --target aarch64-unknown-linux-gnu --emit=exe` against a staged archive. What still does not exist: an archive produced by a CI run (the `runtime-archives` job has never been dispatched) and any qemu/hardware execution test. |
| RISC-V codegen | **Implemented backend, reaches the linker; archive-blocked** — RV64GC (LP64D), scalar FP, atomics, vector encodings, and RVC alignment; unsupported instruction *variants* fail loudly. Since 2026-11-07 (§0.12) a `Move` whose source is a `Symbol` encodes as `auipc rd, %pcrel_hi(sym)` + `addi rd, rd, %pcrel_lo(sym)`, `ParallelMove` is expanded by `ParallelMoveResolver::for_riscv64()` before encoding, and `emit_object` defines the string pool it names through the shared `emit_module_data_sections`. `adesh build hello.adesh --target riscv64gc-unknown-linux-gnu` now measures `No runtime archive for target riscv64gc-unknown-linux-gnu`; the 2026-11-03 failure `unsupported machine instruction Move { dst: Register(Physical(PhysicalRegister(5))), src: Symbol("__str_0") }` is gone. **Loud about operand forms since 2026-11-01**, when the AArch64 review gate was applied here: the placeholder-register `unwrap_or` substitutions in the vector, load/store and atomic arms are gone, `VectorLoad`/`VectorStore` are refused instead of emitting `vle.d v0, (sp)`, `SetCc`/`BranchCc` enumerate all 14 condition codes with `Parity`/`NotParity` refused, memory and atomic widths outside {1,2,4,8} are refused rather than defaulting to doubleword, an out-of-range `shamt` is refused rather than masked, and a branch whose target label is undefined or whose displacement escapes B-type ±4096 / JAL ±1 MiB fails instead of being patched to a placeholder. Each refusal is a `CodegenError` naming the instruction and the function; `tests/native_phase11_codegen_loudness_test.rs` pins 21 unencodable variants and 43 operand forms. What the gate turned up stays missing: spilled atomic operands are refused (AArch64 has the same contract; scratch materialization is not implemented), indexed addressing for `Load`/`Store` is refused, and `VectorLoad`/`VectorStore` have no working form at all. RV32 is refused **while the backend is selected** (`create_backend` returns a `CodegenError` naming the canonical triple, so no RV64 doubleword prologue can reach an RV32 object), and the linker refuses to synthesize its RV64 `_start` stub for an RV32 executable — both refusals are pinned by `tests/native_phase11_codegen_loudness_test.rs` and `linker/tests/phase11_riscv32_entry_stub_test.rs`. No execution test. |
| ELF generation | **Static and dynamic x86-64 executables execution-tested; TLS execution-verified** — AOT executables dynamically import libc and libgcc (`libc.so.6`, `libgcc_s.so.1`). ELF dynamic links emit PLT/JUMP_SLOT imports, local GOTPCREL slots, and COPY relocations for supported imported data objects such as `environ`; fixed-base `ET_EXEC` does not emit PIE-only `RELATIVE` relocations. Locally defined general-/local-dynamic TLS sequences are relaxed to initial-/local-exec forms; unsupported shapes remain loud errors. A glibc-target AOT print repro was linked and executed in Ubuntu 24.04. **Shared-library loading is execution-verified**: `tests/native_phase8_dynamic_elf_test.rs` builds `libadd42.so` (`ET_DYN` + `DT_SONAME` + exported dynsym) and drives it from a PIE caller through `dlopen`/`dlsym` with an asserted result. Static `ET_EXEC` with native Linux `_start` synthesis (argc/argv/envp + `exit` syscall 60) runs on Linux CI and WSL with asserted exit codes. TLS: merged `.tls` block (initialized sections first, zero-fill after), `PT_TLS`, `.tdata` (PROGBITS) / `.tbss` (NOBITS) section-header split, static local-exec relocation (`%fs:(off − memsz)`), and dynamic initial-exec (GOT slot + `R_X86_64_TPOFF64` resolved by the real loader) — **executed against a real musl `ld.so` in WSL with asserted exit codes** (`tests/native_phase8_tls_ehframe_test.rs`). `.eh_frame`/`.eh_frame_hdr` synthesis with `PT_GNU_EH_FRAME` (see DWARF row). |
| Mach-O generation | **Artifact emission, libSystem routing, and dyld binding structure implemented; a real source program links; execution pending** — emits 64-bit Mach-O executables and dylibs with `LC_LOAD_DYLIB`, `LC_SYMTAB`, partitioned `LC_DYSYMTAB`, and `LC_DYLD_INFO_ONLY` (bind/rebase opcode streams and an exports trie), an indirect symbol table, and synthesized `__stubs`/`__got`/lazy-pointer/helper sections forming a full call → stub → lazy-pointer → binder chain — all **verified by independent parse-back decoders** (`linker/tests/macho_dyld_binding_test.rs`). Both x86-64 and AArch64 images are supported; relocations include x86-64 SIGNED/BRANCH/GOT/TLV and ARM64 BRANCH26/PAGE21/PAGEOFF12/TLVP/ADDEND. TLS is emitted as `__thread_data`/`__thread_bss` plus a `__thread_vars` TLV descriptor table bound through `__tlv_bootstrap`. Chained fixups are not emitted. No macOS execution test exists yet: dyld execution verification is pending, covered structurally on the macOS CI runner which runs the conformance suite on the only host where Mach-O is native. **Measuring a real source link on top of that (2026-11-04) found a blocker the structural tests could not see, and it is closed (2026-11-05, §0.11):** `adesh build --target x86_64-apple-darwin --emit=exe` used to fail `error[LNK009]: cannot rebase address 0x100001006`, because the x86_64 backend materialized a symbol address as `movabs` + `Absolute64` inside `.text` while a Mach-O image is always PIE and dyld rebases only `__DATA`/`__DATA_CONST` (`__TEXT` is `r-x` with `maxprot r-x`). The backend now emits `lea r64, [rip + disp32]` + `PcRelative32` for a Mach-O image and keeps `movabs` for PE/ELF, and the triple links a source program: 17,178 bytes, `MH_MAGIC_64`, `cputype` `0x01000007`, `MH_EXECUTE` with `MH_PIE`, the archive's `aot_*` runtime, `_write`/`__exit` bound through `__got`. The linker's own refusal was correct and stays — accepting an absolute in `__TEXT` would have produced an image that dies at launch. `aarch64-apple-darwin` still does not reach the link: no job has built its archive, and it cannot hit this defect because the AArch64 backend emits no `Absolute64` relocation. |
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
- **AArch64 source builds now link — the gap moved from build to execution** —
  until 2026-11-03 the cross build consumed the host archive and the link then
  failed with `aot_make_i64`, `aot_make_string`, `aot_print_cstr`,
  `aot_print_newline`, and `aot_string_concat` unresolved; Phase 12 item 1 (§0.8)
  made that halt loud and one step earlier. Item 4 (2026-11-04, §0.10) then found
  the build could not link even *with* an archive — `error[LNK001]: undefined
  reference to __str_0`, the string pool referenced but never defined — and fixed
  it. With `target/aarch64-unknown-linux-gnu/debug/libadesh_runtime.a` staged the
  target links a hello-world into a 277,416-byte `e_machine` 183 ELF. What is still
  missing: an archive built by a CI run (the `runtime-archives` job has never been
  dispatched) and any execution test.
- **Mach-O `x86_64-apple-darwin` links a source program as of 2026-11-05 (§0.11)** —
  it resolves its archive and produces a 17,178-byte `MH_EXECUTE`+`MH_PIE` image with
  the archive's `aot_*` runtime and `_write`/`__exit` bound through `__got`. Until this
  step the same command failed `error[LNK009]: cannot rebase address 0x100001006`: the
  codegen baked a symbol address into `.text` as `movabs` + `Absolute64`, and dyld
  cannot rebase a pointer inside `__TEXT`, which `maxprot r-x` forbids it writing
  (measured 2026-11-04, §0.10). What is still missing is any execution: the image is
  parsed back, never run, so the triple stays Tier 2. `aarch64-apple-darwin` stops one
  step earlier — no job has built its archive — and does not share the defect, because
  the AArch64 backend emits no `Absolute64` relocation at all.
- **RISC-V source builds stop at the runtime archive** — symbol materialization and
  `ParallelMove` landed 2026-11-07 (§0.12), so a hello-world encodes completely and the
  build reports `No runtime archive for target riscv64gc-unknown-linux-gnu`. No such
  archive exists on this host and no qemu is present, so nothing has been executed.
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

## 0.8 Phase 12 Item 1 — Target Runtime Archive Resolution (2026-11-03)

Phase 12's goal is a linked artifact for every advertised native target "with
no host-architecture fallback". Item 1 is the second half of that sentence: the
compiler now refuses a cross build instead of silently consuming the host's
runtime.

**The defect.** `get_static_runtime_lib` built one candidate list in which
host and target paths were interleaved — `target/debug/libadesh_runtime.a`
appeared before `target/<triple>/debug/libadesh_runtime.a` for the same
profile, and the legacy `libadeshlang.a` / `libadesh_runtime.rlib` entries were
host paths too. A `--target aarch64-unknown-linux-gnu` build therefore resolved
to the x86-64 archive sitting in the host tree. The link either failed on the
archive's object headers or, where it did not, produced an image that cannot
run — and the toolchain reported success. `RUNTIME_LIB_CACHE` made it worse: a
single `OnceLock<Option<PathBuf>>` for the whole process, so a host build
followed by a cross build answered the cross build from the host's resolution
without consulting any path at all.

**Code changes** (`src/backends/aot/cranelift_impl/linking.rs`)

- `RuntimeArchiveSearch` produces the ordered candidate list:
  `target/<triple>/<profile>/{,deps/}` (including Cargo's hashed
  `adesh_runtime-<hash>.a`), an install's `lib/<triple>/`, and
  `$ADESH_RUNTIME_DIR/<triple>/` — then, **only for the host triple**,
  `target/<profile>/`, the install `lib/`/root, and a flat
  `$ADESH_RUNTIME_DIR`. `release` is in the profile list even for a debug
  toolchain: a cross archive is built deliberately, not incidentally.
- `is_host_triple` compares architecture, operating system **and libc
  environment**. The environment matters: a glibc and a musl archive are both
  x86-64 ELF, so nothing at the file level distinguishes them, but a musl image
  needs `libc.musl-x86_64.so.1` as its `DT_NEEDED` and glibc symbols carry
  version tags. Swapping them is a load-time failure after a successful link.
- Host-only fallbacks removed: the `libadeshlang.a` / `libadeshlang.rlib`
  entries (the root crate is `crate-type = ["rlib", "cdylib"]`, so an
  `adeshlang` static archive has never been producible) and the
  `libadesh_runtime.rlib` entry (an `.rlib` carries Rust bytecode for `rustc`,
  not the C runtime objects the linker needs). `adeshlang.*` initially stayed in
  the bundled-install search; measuring item 2 removed it, because the only
  `adeshlang.*` archive on a real host is the DLL's import library — see §0.9.
- `RUNTIME_LIB_CACHE` is now `Mutex<Option<(String, PathBuf)>>` and
  `cached_archive_matches` requires the triple to match before the memo counts.
- `get_static_runtime_lib` returns "No runtime archive for target `<triple>`",
  lists every candidate it asked for, prints the command that stages an archive
  at the path that just failed, and — for a cross build — states that nothing
  built for this machine was considered.
- `runtime_project_roots` / `runtime_install_roots` replace the old inline root
  discovery; the `is_dev_env` probe is gone because those functions return
  nothing outside a checkout, which is what the probe was for.

**Measured** (Windows 11 x86-64 host, `target/debug/adesh_runtime.lib` present
for the whole session)

| Check | Result |
| :--- | :--- |
| `cargo test --lib runtime_archive_tests` | **7 passed** |
| `cargo test --lib` | **940 passed, 0 failed, 1 ignored** |
| `cargo test --test native_semantics_e2e_test` | **23 passed** — the host AOT path still links and runs |
| `cargo test --test native_phase8_runtime_e2e_test` | **6 passed** |
| `adesh build hello.adesh --emit=exe` | **built**, printed `hello`, exit 0 |
| `adesh build hello.adesh --target aarch64-unknown-linux-gnu --emit=exe` | **exit 1**, "No runtime archive for target aarch64-unknown-linux-gnu", 14 searched paths, the build command, and the cross-build sentence |
| `adesh build hello.adesh --target riscv64gc-unknown-linux-gnu --emit=exe` | **exit 1** at codegen: `unsupported machine instruction Move { dst: Register(Physical(PhysicalRegister(5))), src: Symbol("__str_0") }` — reached before archive resolution, and unchanged by this step |
| `adl doctor` | **✓ Runtime Library : D:\Projects\AdeshLang\target\debug\adesh_runtime.lib** |
| `rustfmt --edition 2024 --check` on the touched file | **clean** |
| `cargo clippy --lib` | **no warning names `cranelift_impl/linking.rs`** |

**Consequences for the documentation** — `docs/AOT_COMPILATION.md` described the
old resolution in three places, one of them as a correction ("use the host OS to
pick the library format") that was the opposite of the truth; its Runtime
Library Resolution section, cross-compilation blocker commands, error-message
sample, file-locations table and historical changelogs are updated, with a
header note marking pre-2026-11-03 passages as historical records.

**Open at the time of writing**

- Item 1 only makes the failure honest. No `aarch64`, `riscv64`, or Mach-O
  runtime archive exists yet — that was Phase 12 item 2 (build tooling) and item
  3 (CI), which landed the same day as §0.9. What still does not exist is an
  archive *built by a CI run* and item 4's
  `tests/native_phase12_cross_link_test.rs`, the proof that a cross build links.
- `--target x86_64-unknown-linux-musl` is now treated as foreign on a glibc
  host, which is correct but means a musl build needs its own archive; the item-2
  script can build one (`--targets x86_64-unknown-linux-musl`, with `musl-gcc`
  present), and the CI job does not list it yet.
- The host-scoped search still accepts an archive found next to the executable
  without verifying that its object members match the target; the guarantee here
  is that a foreign triple is never *offered* a host directory, not that an
  archive mislabelled into a `lib/<triple>/` directory is caught.

## 0.9 Phase 12 Items 2 and 3 — Archive Tooling, Packaging, Cross CI (2026-11-03/04)

Item 1 made a missing target archive an honest failure. Items 2 and 3 are the
parts that make it not missing: a script that builds and stages one per triple,
packaging that ships what the resolver looks for, and a CI job that runs the
script for the cross targets. All profiles here are **debug**; the user's
standing instruction for this session is no release builds, so the script
defaults to debug and the CI job passes `--profile debug` explicitly.

**New: `scripts/build-runtime-archives.sh`.** One `cargo build -p adesh-runtime
--target <triple>` per triple, independent failures, summary counts, non-zero
exit if anything is missing. Flags: `--profile debug|release` (default debug),
`--targets "<list>"`, `--all`, `--install-targets`, `--list`, `--help`. It
normalises Cargo's hashed `deps/adesh_runtime-<hash>.lib` to the canonical
`target/<triple>/<profile>/` name so resolution has one predictable path, and it
checks the `!<arch>` magic rather than trusting a file to exist — an empty or
half-written archive is exactly as useless to a link as no archive, and much
harder to diagnose. `archive_name_for` is a copy of `runtime_archive_names` in
`linking.rs` with a comment naming it; those two must not drift.

**The defect this found.** Staging the first archive meant hiding every other
one, and the build still "succeeded" — far enough to reach the link, which then
failed on `undefined reference to aot_print_cstr`. Item 1 had left
`adeshlang.lib` / `libadeshlang.a` in `runtime_archive_names` for pre-split
installs, and `push_archive_dir`'s hashed-name search turned that name into
`target/debug/deps/adeshlang-<hash>.lib`: the root crate's **cdylib import
library**. It is a structurally valid COFF archive containing no `aot_*`
definition. So a host build resolved "a runtime archive", linked an import
library, and produced a confusing symbol error instead of the honest missing-
archive error — and the same name in `lib/` is what an installed Windows
toolchain had been shipping as "the runtime".

- `runtime_archive_names` is now one entry per ABI: `adesh_runtime.lib` /
  `libadesh_runtime.a`. `docs/AOT_COMPILATION.md` and the plan keep the history.
- `adl repair` was worse than wrong: it copied `adeshlang.lib` into the install
  and printed "Restored runtime library", and when that name was nowhere to find
  it ran `cargo build --release --lib`, which produces the root crate's cdylib
  and its import library — never a runtime archive. Repair now goes through
  `get_static_runtime_lib`, stages the resolved archive into `lib/` **and**
  `lib/<host triple>/`, and on a miss (inside a checkout) builds
  `cargo build -p adesh-runtime --target <host triple>` before looking again.
  The profile follows the binary: a debug `adl repair` builds debug, so repairing
  a checkout no longer forces a 20 MB release archive the resolver would have
  taken from `target/debug/` anyway; the release form is what an installed
  release toolchain asks for, and the printed script command matches.
- `adl doctor`'s failure line named `adeshlang.lib` / `libadeshlang.a`; it now
  names the real archive, the host triple, and the script.

**Measured** (Windows 11 x86-64 host; `--profile debug` throughout)

| Check | Result |
| :--- | :--- |
| `scripts/build-runtime-archives.sh --help` / `--list` | usage text; 8 triples with their staged paths, exit 0 |
| `--targets "x86_64-pc-windows-msvc"` | staged `target/x86_64-pc-windows-msvc/debug/adesh_runtime.lib`, **20,895,594 bytes**, built 1 / skipped 0 / failed 0, exit 0 |
| `--targets "aarch64-unknown-linux-gnu riscv64gc-unknown-linux-gnu"` | two `target not installed — run rustup target add …` lines, built 0 / **skipped 2**, **exit 1** |
| `--profile turbo` | `--profile must be debug or release`, exit 2 |
| All archives hidden, `--target=x86_64-pc-windows-msvc` build | **exit 1**, "No runtime archive for target x86_64-pc-windows-msvc", 16 paths, `scripts/build-runtime-archives.sh --profile debug --targets x86_64-pc-windows-msvc` |
| Only the script-staged archive present (host `target/debug/` copies moved out) | **built**, ran, printed `phase12 archive probe`, exit 0 |
| `cargo test --lib runtime_archive` | **7 passed** (after the name change, and again on the rebuilt tree) |
| `cargo build --bin adesh` | **exit 0** |
| `cargo clippy --lib` | 1 warning, `src/backends/wasm/compiler.rs:3800` unreachable pattern — the user's in-flight file, not named in this step |
| `rustfmt --edition 2024 --check` on `linking.rs`, `repair.rs`, `doctor.rs`, `update/manifest.rs` | clean |
| `adl doctor` (archives hidden / restored) | `✗ … adesh_runtime.lib not found for x86_64-pc-windows-msvc (… build it with scripts/build-runtime-archives.sh …)` → `✓ … target\x86_64-pc-windows-msvc\debug\adesh_runtime.lib` |
| `package_unix.sh` staging block, sandbox | `lib/libadesh_runtime.a` + `lib/<dist triple>/` + `lib/<other triple>/` for cross archives built in the tree; `target/release/deps/libadesh_runtime-<hash>.a` correctly left out; missing archive → error + exit 1 |
| `build_windows_dist.ps1` staging + gate, run against `target/x86_64-pc-windows-msvc/debug` | `lib\adesh_runtime.lib` and `lib\x86_64-pc-windows-msvc\adesh_runtime.lib`; gate **throws** when neither exists |
| `ISCC.exe installer.iss` | **Successful compile (120.9 s)** — the changed `[Code]` verification block parses |
| `.github/workflows/ci.yml` | parses; 17 `run` blocks pass `bash -n`; the collect loop staged 2 canonical archives and ignored a hashed `target/debug/deps/` copy |

**The Windows packaging bug this found.** `build_windows_dist.ps1` collected
libraries with `Get-ChildItem "$dir" -Include "*.lib","*.a"`. `-Include` only
filters when `-Path` carries a wildcard, so those two calls matched nothing, and
`dist/windows-x86_64/lib/` has never existed in a built Windows distribution —
an installed toolchain shipped **no** runtime archive, which the installer's own
check then reported as "runtime library not bundled" while the fallback list it
tested included the import library it called the runtime. Both calls now use
`-Path "$dir\*"`, the hashed `deps/` twin of the runtime archive is filtered out
(it would have duplicated 20 MB inside the installer), and the payload gate
accepts only `adesh_runtime.*` and requires the per-triple directory.

**CI: the `runtime-archives` job (code-complete, not yet run).** Two matrix
entries — `linux` on `ubuntu-22.04` (`x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu riscv64gc-unknown-linux-gnu`, after installing
`gcc-aarch64-linux-gnu libc6-dev-arm64-cross gcc-riscv64-linux-gnu
libc6-dev-riscv64-cross`) and `darwin` on `macos-latest`
(`aarch64-apple-darwin x86_64-apple-darwin`). Each adds its rustup targets, runs
the script at `--profile debug`, collects
`target/<triple>/debug/<archive>` into `runtime-archives/<triple>/` — the shape
an install's `lib/<triple>/` uses — and uploads it.
Two deviations from the phase text, both commented in the workflow: the Apple
archives build on a macOS runner (crates/adesh-runtime/src/native_output.c
includes `<unistd.h>`, and the runtime's `build.rs` runs `cc` for the target, so
an `*-apple-darwin` archive needs the macOS SDK an Ubuntu runner does not have),
and the profile is debug because this job proves each target builds while the
packaging scripts remain the source of release archives.

**Re-measured 2026-11-04** (after the `adl repair` profile change and the
documentation sweep, debug throughout): `cargo test --lib` **940 passed, 0
failed, 1 ignored**; `cargo test --lib runtime_archive_tests` **7 passed**;
`cargo test --test native_semantics_e2e_test` **23 passed** (the host AOT path
still links and runs PE artifacts); `cargo build --lib --bin adesh` exit 0 with
the single pre-existing warning in the user's `src/backends/wasm/compiler.rs`;
`rustfmt --edition 2024 --check` clean on all four touched Rust files;
`cargo clippy --lib` names none of them. `ADESH_HOME=<temp> adl repair` printed
`✓ Runtime archive staged:` twice — `lib\adesh_runtime.lib` and
`lib\x86_64-pc-windows-msvc\adesh_runtime.lib`, 20,895,594 bytes each — and
`adl doctor` then reported `✓ Runtime Library`.
`scripts/build-runtime-archives.sh --list` printed all 8 triples with their
debug staged paths, and `--targets x86_64-pc-windows-msvc` re-staged the host
archive (built 1 / skipped 0 / failed 0, exit 0).

**Documentation shipped with the step.** `NEXT_IMPLEMENTATION_PLAN.md` items 2
and 3 carry the measured status; `TARGET_MATRIX.md` (header, the aarch64 link
and gap rows, "Next important" item 3) now says the tooling exists and the job
has never run, rather than "nothing builds an archive"; `ARCHITECTURE_GAPS.md`
§1.5 records the import-library masquerade next to the host-fallback it sits
with, and §3.5 states the gap as "no archive exists yet", not "no tooling
exists"; `docs/AOT_COMPILATION.md`'s live sections (cross-compilation
infrastructure, Runtime Library Resolution + a new "Building the archives"
subsection, the blockers list, the troubleshooting entry and the measured error
sample) describe the script and the single archive name; `docs/RELEASE.md` §3.1
lists the Windows steps in the order the script runs them (including the
`-Include` bug and the `[3c/6]` gate) and §4.1 gains the payload rules for
`package_unix.sh`; `docs/cli-guide.md` has a "Native runtime archives per
target" subsection under the environment table.

**Open after this step**

- The job has never run. `ci.yml` is `workflow_dispatch`-only and dispatching or
  pushing needs the user's approval, so no `aarch64`, `riscv64` or Mach-O archive
  has been built by CI, and Phase 12's first exit criterion stays open until
  item 4's link test runs against real per-target archives.
  *(Still true of CI, and now the only half of this bullet that is: item 4 landed
  on 2026-11-04 and ran against hand-staged archives — see §0.10.)*
- No full release build has run since the packaging changes (the root lib is in
  the user's `variables.rs` edits and was briefly non-compiling during this
  step; the blocks were verified by targeted probes instead).
- `scripts/build-runtime-archives.sh --targets x86_64-unknown-linux-musl` needs
  `musl-gcc`; the CI job does not list musl, so a musl archive is still
  build-it-yourself.
- Resolution still trusts a file's location over its contents: an archive
  mislabelled into `lib/<triple>/` would be used. The `!<arch>` check is in the
  build script, not in the linker.

## 0.10 Phase 12 Item 4 — Cross-Target Link Coverage (2026-11-04)

Phase 12's goal is a linked artifact for every advertised native target with no
host-architecture fallback. Items 1–3 made a missing archive honest, buildable and
shippable; item 4 is the half that proves the two ends fit together, and it is
`tests/native_phase12_cross_link_test.rs` — **8 tests, 8/8 passing on this Windows
host** (`cargo test --test native_phase12_cross_link_test`, 27.26 s).

**Every assertion is on the produced image's bytes, not on the exit code.** The
defect class item 1 exposed was a link that *reported success* while carrying the
wrong runtime, so an exit-code test would have passed on the exact bug this suite
exists to catch. ELF `e_ident`/`e_type`/`e_machine`, the `aot_*` names read out of
`.symtab`+`.dynstr`, PE `Machine`, Mach-O `cputype`/`filetype` and `PT_INTERP` are
decoded by the test itself from the file on disk. `adesh build --target <triple>
--emit=exe` drives the pipeline rather than calling `adeshlink` on a hand-made
object, because the resolver's own search is part of what is being proved. The one
image that is executed is the host-triple control, which is what shows the `aot_*`
definitions those header checks read are reachable rather than merely recorded;
running a cross-built image is Phase 13 and the file says so.

Measured state on this host (archives staged locally; `x86_64-apple-darwin` used an
existing `target/x86_64-apple-darwin/release/libadesh_runtime.a`, which the resolver
reaches because `<triple>/release/` is searched for every triple by design):

| Triple | Outcome |
| :--- | :--- |
| `x86_64-unknown-linux-gnu` | **LINKED** — ELF `e_machine` 62, 8 `aot_*` definitions, 36,208 B |
| `x86_64-unknown-linux-musl` | **LINKED** — ELF `e_machine` 62, 8 `aot_*` definitions, 36,064 B |
| `aarch64-unknown-linux-gnu` | **LINKED** — ELF `e_machine` 183, 8 `aot_*` definitions, 277,416 B |
| `riscv64gc-unknown-linux-gnu` | **REFUSED in codegen** on 2026-11-04 — `ADOB emit error: Adesh CodeGen Error`, before the archive is resolved. **Closed 2026-11-07** (§0.12): the same command now passes codegen and reports `No runtime archive for target riscv64gc-unknown-linux-gnu` |
| `aarch64-apple-darwin` | **REFUSED** — `No runtime archive for target aarch64-apple-darwin` |
| `x86_64-apple-darwin` | **REFUSED at the link** on 2026-11-04 — `error[LNK009]: cannot rebase address 0x100001006`; a dyld fixup target must live in `__DATA_CONST` or `__DATA`. **Closed 2026-11-05** (§0.11): the same command now links |
| `x86_64-pc-windows-msvc` | **LINKED** — PE `Machine` 0x8664, 47,616 B, and the image runs |
| `aarch64-pc-windows-msvc` | **REFUSED** — `No runtime archive for target aarch64-pc-windows-msvc` |

**AArch64 had to be fixed to get there.** With a real `aarch64-linux-gnu` archive
staged, the build failed `error[LNK001]: undefined reference to __str_0`. The shared
lowering fills `NativeModule::string_pool` for every backend and the AArch64 encoder
emits `ADRP`/`ADD` pairs naming `__str_<n>`, but `emit_object` wrote `.text` and
nothing else — x86_64 has always emitted `.rodata`, `.tls` and `.data`. Added
`emit_module_data_sections` to `crates/adesh-codegen/src/targets/aarch64/mod.rs` with
the same layout rules (8-byte alignment for `.tls`/`.data`, one null-terminated copy
per distinct pool string, pool symbols `Local`, TLS/global symbols `Global`), and the
target went from refused-at-the-link to linking. The pin is a unit test —
`test_aarch64_emit_object_defines_the_string_pool` — because the cross-OS link test
needs an archive only a host with a cross `gcc` can produce; 7/7 `aarch64` codegen
tests pass. Note the ordering this suite defends: before item 1 the *same* command
failed at archive resolution, which let a codegen gap hide behind a packaging gap.
`the_aarch64_archive_resolves_so_the_archive_is_never_the_blocker` fails if that
regression returns, and prints `PROMOTED:` with the docs to update once the target
links.

**`x86_64-apple-darwin` reached the Mach-O writer and failed there — but the fault was
in the compiler, not the linker, and that is a correction to this section's original
claim ("a linker bug, not a packaging gap").** Measured §0.11: the address the writer
refused was a pointer the *codegen* had baked into `.text`, and the writer's refusal
was correct — dyld has no fixup it may apply inside `__TEXT`, so accepting it would
have produced an image that dies at launch. Every existing Mach-O test builds its
input objects by hand, so nothing had connected a real source program to that path
before now, and the failure looked like a linker defect because the error named the
writer. It is the blocker recorded for the triple in `TARGET_MATRIX.md`, and it came
before Phase 13's execution work.

**CI wiring.** `.github/workflows/ci.yml` gains a `cross-link` job: `needs:
runtime-archives`, on all three OSes, downloading the
`runtime-archives-*-debug-<run>` artifacts and staging each at
`target/<triple>/debug/` — the first path the resolver searches. It **fails if the
artifact arrives with no archive in it**, because otherwise the suite's own loud
`SKIP:` lines would read as CI doing its job. It also builds the host archive, which
the control test needs and which the cross matrix does not contain for Windows. All
three OSes run it on purpose: linking a Linux image on a Windows or macOS runner, and
a Mach-O image on a Linux runner, is the claim the phase makes. Like
`runtime-archives`, **it has never been dispatched** — pushing or dispatching needs
explicit approval.

**Verification of this step.** `cargo test --test native_phase12_cross_link_test` →
8 passed / 0 failed. `cargo test -p adesh-codegen --lib aarch64` → 7 passed.
`cargo fmt -p adesh-codegen -- --check` and `rustfmt --check` on the new test file →
clean. `cargo clippy -p adesh-codegen --lib` and `cargo clippy --test
native_phase12_cross_link_test` name none of the changed files. The per-triple table
above is the gate test's own `LINKED`/`REFUSED` output from this run, and the suite
prints that table into any CI log.

**Open after this step**

- Neither `runtime-archives` nor `cross-link` has been dispatched, so every non-host
  archive in the table above was built by hand on this machine (the AArch64 one with
  `clang --target=aarch64-unknown-linux-gnu` plus an extracted
  `libc6-dev-arm64-cross` header set). No CI log has produced or consumed one.
- Phase 12's first exit criterion is now target-specific rather than
  archive-specific: `riscv64gc` needed symbol materialization and `ParallelMove` in
  codegen — both landed 2026-11-07 (§0.12), so it now needs only an archive;
  `aarch64-apple-darwin` / `aarch64-pc-windows-msvc` need an archive from a
  job that can build one, and `x86_64-apple-darwin` — which on this date still needed
  its `__TEXT` rebase fixed — has since had it closed and links (§0.11), leaving
  execution as its only gap.
- `riscv64gc` **is** in the CI `runtime-archives` Linux matrix
  (`.github/workflows/ci.yml`, which installs `gcc-riscv64-linux-gnu
  libc6-dev-riscv64-cross`), contrary to what this bullet originally claimed; the job
  has simply never been dispatched. This host could not build one without that cross
  toolchain.
- Nothing yet asserts that a *mislabelled* archive is caught: resolution trusts a
  file's location, and the `!<arch>` magic check lives in the build script, not the
  linker (carried over from §0.9).

## 0.11 Mach-O `LNK009` Closed — `x86_64-apple-darwin` Links a Source Program (2026-11-05)

§0.10 left one target that resolved an archive, reached the link, and still failed:
`x86_64-apple-darwin`. This step fixes that. A hello-world now produces a real Mach-O
executable, and the target's only remaining gap is that nothing runs it.

**What the failure actually was.** The writer refused to rebase `0x100001006`, and once
the refusal named its contributor the answer was immediate: the address was
`.text`+0x6 in the compiler's own ADOB, pointing at `__str_0`. The x86_64 backend
materializes a symbol address as `movabs r64, imm64` with an `Absolute64` relocation,
which is right on PE (a base-relocation directory records absolutes) and right on the
non-PIE ELF images this toolchain emits — and impossible on Mach-O, where the writer
always sets `MH_PIE` and dyld applies rebase fixups only inside `__DATA`/`__DATA_CONST`.
So every source program for the triple failed, not some edge case: the first instruction
a `print` needs is a symbol address. The backend now picks the form per **image**
format, `lea r64, [rip + disp32]` + `PcRelative32` with addend −4 for Mach-O, `movabs`
for PE and ELF. A PC-relative displacement is final at link time, so the image needs no
dynamic fixup at all.

**Why the first attempt did nothing, which is the part worth keeping.** The branch was
originally written against `TargetDescriptor::object_format`, and the fix changed no
behavior and reproduced the identical message. `object_format` names the container this
toolchain *emits* — every triple, darwin included, is `Adob` — so a guard comparing it
to `MachO` cannot fire and fails silently rather than loudly. Image format now comes
from `TargetDescriptor::image_format()`, derived from the operating system the same way
`crates/adesh-codegen/src/target_spec.rs` already derives it, and
`crates/adesh-object/src/target.rs::image_format_comes_from_the_operating_system_not_the_emitted_container`
pins both halves: that `object_format` is `Adob` for all four tested triples, and that
`image_format()` separates them. The renamed `FunctionEncoding::image_format` field
carries a comment saying which descriptor field is the wrong one, because this cost a
full debugging cycle.

**The error was also unactionable.** `LNK009` named an address and a rule but nothing
that produced the address, so it read as a linker defect and pointed investigation at
the writer for a compiler bug. Mach-O rebase records now carry the input object, output
section, offset and symbol, and the refusal prints them:

```
error[LNK009]: cannot rebase address 0x100001006 (`.text`+0x6 in `...hello_23908.adob`,
pointing at `__str_0`): a dyld fixup target must live in `__DATA_CONST` or `__DATA` ...
```

**Measured image** (`adesh build hello.adesh --target x86_64-apple-darwin --emit=exe`):
17,178 bytes, `MH_MAGIC_64`, `cputype` `0x01000007`, `cpusubtype` `0x80000003`
(`CPU_SUBTYPE_X86_64_ALL|LIB64`), `filetype` `MH_EXECUTE`, `MH_PIE` set in `flags`.
Segments `__PAGEZERO`/`__TEXT`/`__DATA_CONST`/`__DATA`/`__LINKEDIT`; sections `.text`
at `0x100001000`, `.rodata` at `0x100002000`, `__got` at `0x100003000`; 16 symbols
including `main` at `0x100001000` and `aot_print_cstr` at `0x100001100`, with `_write`
and `__exit` routed through libSystem. The archive contributed the `aot_*` runtime, so
this is the same Phase 12 resolution path the Linux links use.

**Correctness, not just success.** A link that stops failing is not proof the pointer is
right, so the image was decoded rather than trusted. `main` opens
`55 | 48 89 e5 | 48 8d 0d f5 0f 00 00` — `push rbp`, `mov rbp, rsp`, then
`lea rcx, [rip+0xff5]` at `.text`+4. Instruction end `0x10000100b` + `0xff5` =
`0x100002000` = `.rodata`+0, whose bytes are `lnk009 probe` — the program's own string,
exactly. As a control, the same source built for the host triple prints
`lnk009 probe`, so the two paths agree on the value the linker computed.

**Verification of this step.**
`cargo test --test native_phase12_cross_link_test` → **9 passed / 0 failed**, the new
test being `the_mach_o_image_needs_no_fixup_inside_text`: it requires the link (a
failure prints the LNK009 diagnosis and the codegen site to fix), asserts the header
matches the triple, and asserts `MH_PIE` is still set so nobody "fixes" the error by
dropping position independence. Its non-vacuity was checked directly — reverting the
guard to `object_format` makes it fail with the original
`cannot rebase address 0x100001006 ... pointing at __str_0`. Unit pins:
`a_symbol_address_is_pc_relative_only_on_mach_o` (both encoding arms, byte-exact, with
the −4 addend and the disp32 field offset) and the `image_format` test above.
`cargo test -p adesh-codegen` → 89 + 17 passed; `cargo test -p adesh-object` → all
passed; `cargo test -p adesh-linker` → 56 + every integration suite passed, including
`macho_dyld_binding_test.rs`, which decodes the rebase and bind streams the changed
layout records feed; `cargo test --test native_phase8_tls_ehframe_test --test
phase_7_integration` (the other two suites that touch darwin) → 8 + 19 passed.
`cargo fmt` and `cargo clippy -p adesh-object -p adesh-codegen -p adesh-linker
--all-targets` are clean.

**Open after this step**

- The Mach-O image has never been executed. It is parsed back, and that is the whole of
  its verification, so the triple stays Tier 2 by this file's own rule. A macOS runner
  must build, run and assert its output, and check a dyld-bound import and a TLV access.
- `aarch64-apple-darwin` still stops at archive resolution. Checked so the same class of
  bug is not waiting there: the AArch64 backend emits no `Absolute64` relocation at all,
  so its blocker is only the archive no job has built.
- Neither `runtime-archives` nor `cross-link` has been dispatched, so the Apple archive
  this used is a hand-built `target/x86_64-apple-darwin/release/libadesh_runtime.a`.
- PE and ELF still take the `movabs` path. If an ELF link ever becomes PIE-by-default,
  an absolute pointer in `.text` breaks the same way: this toolchain's ELF layout turns
  every `Absolute64` in an alloc section into an `R_X86_64_RELATIVE`, including one in
  code, which a loader cannot write into an execute-only segment. Measured not
  reachable today — `x86_64-unknown-linux-gnu` produces `e_type` 2 (`ET_EXEC`,
  fixed-base), so no dynamic relocs are emitted for it at all — but it is the same
  defect one flag away.

## 0.12 RISC-V Passes Codegen — Symbol Materialization, `ParallelMove`, and a Silent Relocation Downgrade (2026-11-07)

Phase 12 exit criterion 1 names two triples that must link a source program:
`riscv64gc-unknown-linux-gnu` and `aarch64-apple-darwin`. This step closes the RISC-V
*backend* half. The triple still does not link on this host, and the reason is now
packaging rather than codegen — which is the distinction the phase is written against.

**What was missing.** Three things, all in `crates/adesh-codegen/src/targets/riscv/mod.rs`:

- A `Move` whose source is `MachineOperand::Symbol` had no arm, so the first string
  literal in any program aborted encoding. It now emits
  `auipc rd, %pcrel_hi(sym)` + `addi rd, rd, %pcrel_lo(sym)` with `RiscV_PcrelHi20`
  (addend 0) and `RiscV_PcrelLo12I` (addend 4) relocations.
- `ParallelMove` had no encoding either, and an unresolved one reaching the encoder was
  a silently dropped instruction. `generate_function_with_relocs` now expands it through
  `ParallelMoveResolver::for_riscv64()` before encoding. The scratch pool is t0/t1/t2
  (x5–x7): they are the ABI's caller-saved temporaries, the encoder already clobbers
  them for immediate and address synthesis, and they are the only GPR scratch ids below
  16 — `PhysicalRegister::class` reports `Float` for ids 16..31, so a higher pool entry
  would never be selected for an integer cycle. Reserved: ra, sp, gp, tp, s0/fp.
- `emit_object` emitted no `.rodata`, so the `__str_<n>` the new pair names would have
  been an undefined reference — the exact `LNK001` AArch64 hit on 2026-11-04. The
  data-section emitter is now shared (`targets::emit_module_data_sections`) instead of
  duplicated; AArch64 calls the same function and its local 87-line copy is gone.

**The `+4` addend is the part worth keeping.** RISC-V defines `%pcrel_lo(sym)` relative
to the `AUIPC`, but `linker/src/relocation.rs` computes `S + A − place_va` for *each*
relocation using its own address. For an adjacent pair the `ADDI` is four bytes past its
`AUIPC`, so codegen passes `addend = 4` on the LO12 to cancel the gap. Nothing in the
object format enforces this, and a wrong pair still links — it just loads the wrong
address at run time. The compensation holds only while the pair stays adjacent and
uncompressed; the backend emits no RVC, so it does today.

**Measured on linked image bytes**, not object bytes
(`linker/tests/riscv64_pcrel_symbol_pair_test.rs`, 3 tests, no runtime archive needed —
`riscv64gc-unknown-none-elf` links statically and the linker synthesizes `_start`):

| Measurement | Value |
| :--- | :--- |
| `.text` / `.rodata` virtual addresses | `0x401000` / `0x402000` (delta `0x1000`) |
| Pairs decoded | 17, spanning a `0x1080` delta range — every low-12 residue |
| Exact reconstructions | 17/17 (`auipc_va + sext20(hi20)<<12 + sext12(lo12) == symbol_va`) |
| Pairs with `lo12` bit 11 set | 8 (indices 8–15), so `hi20` rounds up past the true delta |
| Rounding example | pair 8: delta `0x1840` = `0x2000` + sign-extended `0x840` (−1984) |
| A/B with LO12 addend 0 | still links, still well-formed instructions, lands 4 bytes low |
| Backward reference (`.text+0x2000` → `main` at `.text+0`) | `hi20 = -2`, `lo12 = 0`, exact |

The A/B is what makes the `+4` load-bearing rather than incidental: without it the
failure is invisible at link time. The test also asserts the fixups did not clobber the
`AUIPC` opcode/rd or the `ADDI` opcode/rd/rs1, and that section GC kept both sections.

**A silent-corruption bug found on the way.** The first version of that test built its
object with the linker's own `ObjectWriter` and failed with `relocation at 0x84 exceeds
section size 0x88` — a 4-byte relocation being bounds-checked as 8 bytes. Cause:
`AdobV2::encode` maps every relocation kind outside its table to id 0 (`Absolute64`) via
`_ => 0`, `decode` maps unknown ids back to `Absolute64` via `_ => Absolute64`, and the
table had **no RISC-V entries at all**. This is not test-only: `adeshlink strip`
(`linker/src/main.rs`) re-encodes an ADOB object through this codec while preserving
`sec.relocations`, so stripping a RISC-V object turned every PC-relative fixup into an
8-byte absolute one and the result still linked, just wrong. Ids 30–33 (the authoritative
universal-ADOB ids for `RiscV_Call`/`PcrelHi20`/`PcrelLo12I`/`PcrelLo12S`) now round-trip,
and both catch-alls refuse with `LNK008` naming the kind or id instead of downgrading.
Kinds with no assigned id are refused rather than given a newly invented persisted id —
that is a format decision, not a bug fix. The image test uses `adesh_object::AdobWriter`,
the production encoder, so it travels the path real codegen output takes.

**Measured CLI promotion.** `adesh build hello.adesh --target riscv64gc-unknown-linux-gnu
--emit=exe` previously reported `unsupported machine instruction Move { dst:
Register(Physical(PhysicalRegister(5))), src: Symbol("__str_0") }`. It now reports
`No runtime archive for target riscv64gc-unknown-linux-gnu` with the searched paths and
`scripts/build-runtime-archives.sh --profile debug --targets …`. The cross-link suite's
expectation was rewritten to match
(`riscv64gc_passes_codegen_and_stops_at_the_runtime_archive`), and its two loops that
excluded `RISCV_TRIPLE` on the grounds that "codegen refuses it, so no archive can make it
link" no longer exclude it — the exclusion had outlived its justification.

**Verification** (Windows x86-64 host, debug):

| Suite | Result |
| :--- | :--- |
| `cargo test -p adesh-linker` | all green — 59 lib tests (4 ADOB codec) plus every integration suite, including the 3 new image tests |
| `cargo test -p adesh-codegen` | **107 passed**, 0 failed |
| `cargo test --test native_phase11_codegen_loudness_test` | **20 passed** — 5 new RISC-V tests (symbol pair, parallel move, cycle break, indexed-source refusal, string pool) |
| `cargo test --test native_phase12_cross_link_test` | **9 passed**, prints `PROMOTED: riscv64gc-unknown-linux-gnu passes codegen` |
| `cargo test --test native_phase7_riscv64_e2e_test` | **5 passed** |
| `cargo fmt -p adesh-linker -p adesh-codegen --check`, `rustfmt --edition 2024 --check tests/native_phase12_cross_link_test.rs` | clean |
| `cargo clippy -p adesh-linker -p adesh-codegen --all-targets` | clean |

The loudness gate needed an honest edit rather than a weakened assertion:
`riscv64_rejects_every_unencodable_instruction` had pinned `ParallelMove` as unencodable,
so once it encoded, the gate failed. `ParallelMove` was removed from the refusal list and
replaced by positive tests that pin the expansion and the cycle break
(`mv_pairs == [(5,11),(11,10),(10,5)]`, i.e. the scratch really is used).

**What this does not claim.** No RISC-V program has been executed. There is no
`riscv64gc-unknown-linux-gnu` runtime archive on this host and no qemu anywhere on it;
building the archive needs a cross C toolchain because `crates/adesh-runtime/build.rs`
compiles `native_output.c` for the target. The CI `runtime-archives` matrix already
installs `gcc-riscv64-linux-gnu` and `libc6-dev-riscv64-cross` but has never been
dispatched, and dispatching it needs the user's approval. `aarch64-apple-darwin`, the
other half of exit criterion 1, still needs a macOS/SDK build of its archive. RISC-V
therefore stays **emission-tested and image-relocation-tested, not execution-verified**,
and its tier label does not move.

## 0.13 Phase 12 verification follow-up (2026-11-08)

- Kali WSL now has GCC cross toolchains for AArch64 and RV64, QEMU 11.1.0,
  and Rust cross targets. The RV64 runtime archive built from committed source
  is 44,077,682 bytes with 104 defined `aot_*` symbols and RV64 object members;
  it is staged under `target/riscv64gc-unknown-linux-gnu/debug/`.
- This closes archive availability locally, not source linking. The real CLI
  exposed compressed ELF relocations incorrectly decoded as ABS64. Two-byte
  RVC branch/jump fixups and ELF/ADOB mappings now pass six focused tests.
  NONE/RELAX markers patch no bytes; other unsupported RISC-V kinds fail LNK008.
  ADD/SUB relocations for 8/16/32/64-bit fields now implement endian-aware wrapping
  read-modify-write semantics; all eight focused tests pass. The real source-link
  retry advances past ADD64 and stops at unsupported relocation type 60 at offset
  `0x10b`. The cross-link test permits this loud failure: its passing assertion is
  not a successful link. No RISC-V source image has been execution-verified.
- Full linker regression attempts were blocked by Windows LNK1104 executable
  locks. The six focused tests ran; the full suite must not be recorded as passed.
- CI run 42396244918 failed: archive jobs inherited `RUSTC_WRAPPER=sccache`
  without installing it, while check jobs failed formatting. The local workflow
  now scopes sccache to the job that installs it; that change has not been
  verified by a new CI run. No Apple archive evidence was produced.
- Windows packaging produced a ZIP and installer and passed its staged toolchain
  check, with flat and per-triple runtime archives present. However, an earlier
  linker compilation failed with nine argument-count errors, so this is not a
  clean release. The packaging script now checks every Cargo build exit status
  and frontend install/build failures before staging. A PowerShell harness
  verified parsing, a rejected mock Cargo exit 23, and an accepted exit 0.
  The payload smoke test now passes: the packaged compiler, invoked from its
  distribution directory, builds a 47,616-byte AMD64 PE that prints
  `phase12 packaged runtime`. Flat and per-triple runtime copies are identical:
  15,305,560 bytes, SHA-256
  `f557f97de29950d07a44be93d33d4122c2d98c2903cb5754dadd4e63889bbcd7`.
  A fresh successful build remains required; this smoke test does not establish
  that the bundled linker matches current source.

Phase 12 remains open. Phase 13 execution verification and Phase 14 have not started.

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
| **Mach-O (macOS)** | **Artifact emission + dyld binding structure; a source program links; execution pending** | Emits `MH_EXECUTE`/`MH_DYLIB` layouts with `LC_LOAD_DYLIB`, `LC_SYMTAB`, partitioned `LC_DYSYMTAB`, and `LC_DYLD_INFO_ONLY` with bind/rebase opcode streams and an exports trie; synthesizes `__stubs`, `__got`, lazy-pointer and helper sections with the indirect symbol table, forming a full call → stub → lazy-pointer → binder chain — verified by independent parse-back decoders (`linker/tests/macho_dyld_binding_test.rs`). Chained fixups are not emitted; no macOS execution test exists yet, so dyld behavior is unverified (the macOS CI runner executes the structural conformance suite). Both x86-64 and AArch64 images are supported, with x86-64 SIGNED/BRANCH/GOT/TLV and ARM64 BRANCH26/PAGE21/PAGEOFF12/TLVP/ADDEND relocations. TLS is emitted as `__thread_data`/`__thread_bss` plus a `__thread_vars` TLV descriptor table bound through `__tlv_bootstrap`; it has no execution test. **The link-level blocker found on 2026-11-04 is closed (2026-11-05, §0.11):** `adesh build --target x86_64-apple-darwin --emit=exe` used to fail `error[LNK009]` because the compiler put an absolute symbol address in `.text` and dyld could never rebase it inside `__TEXT` (`maxprot r-x`); the x86_64 backend now uses `lea`+PC-relative addressing when the image is Mach-O, so the triple links a source program and only execution is missing. `aarch64-apple-darwin` still stops earlier, at archive resolution, since no job has built its archive — and it cannot meet the same defect, because the AArch64 backend emits no `Absolute64` relocation. `tests/native_phase12_cross_link_test.rs` pins both: the gate test reports each triple's outcome, and `the_mach_o_image_needs_no_fixup_inside_text` requires the Mach-O link and that it stayed PIE. |
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
| **AArch64** | **Implemented, loud about what it cannot encode, execution-unverified** | AAPCS64 register allocation, callee-saved X19-X28/D8-D15, 64-bit immediate materialization, scalar FP, NEON, atomics, and ADRP+ADD (`AArch64_AdrPage21`/`AArch64_AddAbsLo12`) symbol materialization. Both audited codegen defects are fixed: `emit_object` bases each relocation on the accumulated `.text` length instead of `index * 8`, and (2026-11-01) the encoder's `_ =>` NOP fallback is gone — every unencodable variant and operand form returns a `CodegenError` naming the instruction and its function. Pinned by `tests/native_phase11_codegen_loudness_test.rs`. Refused rather than mis-encoded today: FP/SIMD stack access (needs `LDR`/`STR Dd, [SP]`), and the 29 variants with no AArch64 encoding. **A source build links** (2026-11-04): `emit_object` also emits `.rodata`/`.tls`/`.data`, which it had never done — the encoder names `__str_<n>` for every string literal, so a hello-world link failed `LNK001: undefined reference to __str_0` until then. `tests/native_phase12_cross_link_test.rs` measures the linked 277,416-byte `e_machine` 183 ELF with 8 `aot_*` definitions against a staged archive; no CI job has produced one yet, and there is still no execution test. |
| **RISC-V** | **Implemented backend, loud about what it cannot encode, archive-blocked** | RV64GC LP64D, scalar FP, atomics, vector encodings, and RVC alignment; unsupported instruction *variants* fail loudly. Since 2026-11-07 (§0.12) a `Move` register ← `Symbol` encodes as `auipc rd, %pcrel_hi(sym)` + `addi rd, rd, %pcrel_lo(sym)` with `RiscV_PcrelHi20`/`RiscV_PcrelLo12I` (LO12 addend 4), `ParallelMove` is expanded by `ParallelMoveResolver::for_riscv64()` before encoding, and `emit_object` defines the string pool it names — so a hello-world reaches archive resolution instead of dying in the encoder. The `+4` contract is pinned on linked image bytes by `linker/tests/riscv64_pcrel_symbol_pair_test.rs`. RV32 is refused at backend selection and the linker refuses to synthesize an RV64 `_start` stub for an RV32 executable (2026-11-02) — neither triple reaches an object file. The silent-fallback class the AArch64 gate closed was fixed here the same day (2026-11-01): placeholder `unwrap_or` registers are gone from the vector, load/store and atomic arms, `VectorLoad`/`VectorStore` no longer hardcode `sp` as their base, `SetCc`/`BranchCc` enumerate every condition code instead of defaulting `Parity`/`NotParity` to a constant `0` or to `Equal`, widths and shift amounts outside their encodable ranges are refused, and unresolved or out-of-range branch targets fail instead of being patched to a placeholder. Pinned by `tests/native_phase11_codegen_loudness_test.rs` (21 variants, 43 operand forms). Refused rather than mis-encoded today: spilled atomic operands, indexed `Load`/`Store`, and any vector load or store. No execution test. |
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
