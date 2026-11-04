# Adesh Native Target Support Matrix

**Generated:** 2026-10-24 (native codegen + linker audit)
**Last updated:** 2026-11-04 (Phase 12 item 4: `tests/native_phase12_cross_link_test.rs`
links a hello-world source program to each of the 8 advertised native targets on a
Windows host and asserts the produced **image bytes** — ELF/PE/Mach-O header plus the
`aot_*` symbol table — not the exit code. 8/8 pass. Measured per target: the three
x86-64 archives (glibc, musl, Windows) and `aarch64-unknown-linux-gnu` **link**;
`riscv64gc` is refused in codegen before the archive is resolved; `x86_64-apple-darwin`
resolves its archive and fails the Mach-O link at `error[LNK009]`; the two triples no
CI job builds (`aarch64-apple-darwin`, `aarch64-pc-windows-msvc`) fail with the
missing-archive error. AArch64 links because `emit_object` now defines the string pool
it references (`__str_0` was an undefined reference until this change). A `cross-link`
CI job stages the `runtime-archives` artifacts at `target/<triple>/debug/` and runs the
suite on all three OSes; neither new job has been dispatched.)
Earlier: 2026-11-03 (Phase 12 items 1–3: runtime archive resolution is
target-scoped — a cross build is offered only paths naming its own triple, the
memo cache is keyed by triple, and a missing archive names the triple, every
path searched and the build command; `scripts/build-runtime-archives.sh` builds
and stages the per-triple archives, packaging (`installer`, `Dockerfile`,
`scripts/release/*`) ships `lib/<triple>/`, and the CI `runtime-archives` job
builds 3 Linux + 2 Apple archives at debug and uploads them. The job has never
been dispatched, so no non-host archive exists yet.)
Earlier: 2026-11-02 Phase 11 tier realignment — `linker/src/target.rs`
now derives its tiers from what CI verifies and both CLIs print that same
table; RV32 is refused at backend selection and at entry-stub synthesis.
Earlier: 2026-11-01 silent-fallback gate — AArch64 and RISC-V both fail loudly
instead of emitting placeholders)
**Canonical status source:** `CURRENT_STATE.md` — reconciled in the same change
set as this audit; the two files agree.
**Verification rule:** a target is **execution-verified** only when a checked-in
test or CI job builds an artifact and runs it, asserting behavior. Emitting
bytes that nothing runs is **emission-tested**, not support. Since 2026-11-02
the tier labels in `linker/src/target.rs` follow this rule rather than
capability: Tier 1 = CI runs the artifact, Tier 2 = CI emits and parses it
back, Tier 3 = declared only. `linker/tests/phase11_target_tiers_test.rs`
pins each label to the suite that earns it.

---

## Matrix

| Target | Object format | Codegen | Linker / relocations | Execution verification | Pending / next blocker |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | PE32+ | **Native** — 64-bit GPR + scalar SSE2, XMM0-13, stack args, Win64 shadow space, callee-saved, atomics, Win64 TLS | **Native PE** — imports/IAT, TLS dir, base relocs, ASLR, DLL export tables; ABS64 + PC32 | **Execution-verified** — PE executables run incl. a real `msvcrt!puts` import (`linker/tests/pe_windows_e2e_test.rs`, `tests/native_x86_64_e2e_test.rs`, phase 2-8 suites) | Source-level `extern`/variadic/aggregate/atomic/TLS syntax; vector ABI; PE **DLL** load + export call test (only emission is tested today) |
| `x86_64-unknown-linux-gnu` | ELF64 | **Native** — same subset as Windows (SysV variant) | **Native ELF** — static + dynamic, PLT/GOT, JUMP_SLOT, COPY, GOTPCREL, PIE `RELATIVE`, TLS LE/IE, `.eh_frame`/`.eh_frame_hdr`, `_start` synthesis, GNU/COFF archives, GC + ICF | **Execution-verified** — static ELF (`tests/native_phase3_linux_e2e_test.rs`), dynamic ELF and a `dlopen`/`dlsym` shared library (`tests/native_phase8_dynamic_elf_test.rs`), TLS LE/IE (`tests/native_phase8_tls_ehframe_test.rs`). Minimum GLIBC baseline: **GLIBC 2.35 (Ubuntu 22.04 LTS)** | Runtime unwinder test (`.eh_frame` is decoder-verified only); source-level ABI surface; cross-target runtime archive for non-host triples |
| `x86_64-unknown-linux-musl` | ELF64 (static + dynamic) | **Native** — same x86-64 subset | **Native ELF** — same relocations; TLS TPOFF64 | **Emission-tested in CI** (added 2026-11-02: `tests/native_phase8_cross_compile_test.rs::test_cross_target_emission_and_validation` now emits and validates a musl ADOB on the ubuntu, Windows and macOS jobs). Execution with asserted exit codes is **WSL-only**: static local-exec and dynamic initial-exec TLS binaries run against the real musl `ld.so` on a developer machine | **No dedicated musl CI job** — CI never loads a musl artifact, so the target stays Tier 2; add a musl container/runner |
| `aarch64-unknown-linux-gnu` | ELF64 | **Implemented, loud about what it cannot encode** — AAPCS64, X19-X28/D8-D15 preservation, scalar FP, NEON, atomics. Since 2026-11-01 an unencodable `MachineInstruction` variant or operand form returns a `CodegenError` naming the instruction and its function; the encoder no longer emits a `NOP` placeholder, no longer drops instructions whose operands it did not recognize, and no longer substitutes a placeholder register. | **Structural only** — ADRP+ADD symbol materialization **is emitted** now (`AArch64_AdrPage21` + `AArch64_AddAbsLo12`), and `emit_object` bases each relocation on the accumulated `.text` length, so call relocations land on their real call sites (both were defects in the 2026-10-24 audit and are fixed). Still no `aarch64` runtime archive is produced by a released CI run. What changed: since 2026-11-03 a source build stops *loudly* when the archive is absent (Phase 12 item 1), and since 2026-11-04, **with an archive staged, a hello-world source program links** — `tests/native_phase12_cross_link_test.rs` measures a 277,416-byte ELF with `e_machine` 183 and 8 `aot_*` definitions on the AArch64 path. That link only became possible with the same change, which fixed the last codegen gap: `emit_object` in `crates/adesh-codegen/src/targets/aarch64/mod.rs` now emits `.rodata` / `.tls` / `.data` (the shared lowering fills the string pool for every backend and the encoder names `__str_<n>`, but only x86_64 was defining them), so the reference is no longer `error[LNK001]: undefined reference to __str_0`. Pinned by `test_aarch64_emit_object_defines_the_string_pool`, because the cross-OS link test needs an archive only some hosts can build. | **None** — no qemu/hardware run; static ELF structure tested (`linker/tests/platform_native_linking_conformance_test.rs`) | 1) qemu-aarch64 execution CI — the link is now the *smallest* remaining gap, so this is first; 2) FP/SIMD stack access (`LDR`/`STR Dd, [SP]`) — the loudness gate revealed that spilled float/vector values reach the encoder as stack slots, which it now refuses; before, it wrote them as 64-bit GPR loads with the register number truncated to 5 bits (`V30` became `X30`, the link register); 3) `aarch64-apple-darwin` and `aarch64-pc-windows-msvc` still have no archive builder in CI (only `aarch64-unknown-linux-gnu` is in the `runtime-archives` matrix). Done since the audit: the NOP fallthrough, real call-site relocation offsets, ADRP/ADD_LO12 emission, the string/TLS/data sections, and a link that consumes a per-target archive. |
| `aarch64-apple-darwin` | Mach-O 64 | **Implemented** — AAPCS64 path | **Native Mach-O** — BRANCH26, PAGE21, PAGEOFF12, TLVP, ADDEND; TLS is emitted as `__thread_data`/`__thread_bss` plus a `__thread_vars` descriptor table; dyld bind/rebase, exports trie, stubs/GOT parse-back tested | **None** — `linker/tests/macho_dyld_binding_test.rs` decodes the dyld structures but never runs under dyld | **Measured 2026-11-04:** a source build does not reach the Mach-O link — `adesh build --target aarch64-apple-darwin` fails at archive resolution ("No runtime archive for target aarch64-apple-darwin"), because no job has yet built the archive the `runtime-archives` macOS entry would produce. Then: dedicated link-and-run on the macOS CI runner; TLV execution; code-signature acceptance by real dyld |
| `x86_64-apple-darwin` | Mach-O 64 | **Native** — x86-64 subset | **Native Mach-O** — x86_64 SIGNED/BRANCH/GOT/TLV relocations, libSystem routing, runtime archive symbol lookup. **The first real source link measured on it fails inside the writer:** `adesh build --target x86_64-apple-darwin --emit=exe` resolves `target/x86_64-apple-darwin/release/libadesh_runtime.a`, then dies on `error[LNK009]: cannot rebase address 0x100001006` — a dyld fixup target must live in `__DATA_CONST` or `__DATA`, because `__TEXT` is mapped `r-x` with `maxprot r-x` and dyld cannot write the link-time pointer there. Pinned as a loud failure by `tests/native_phase12_cross_link_test.rs`; it is not silent, and it is now the blocker instead of "no archive" | **Indirect only** — `tests/static_imports_test.rs` `--aot` path executes Mach-O binaries on macOS CI; no dedicated link-and-run or bind/rebase runtime assertion | **Fix the `__TEXT` rebase (`LNK009`) so a source program links at all**, then a dedicated macOS execution test and runtime verification of `__stubs`/lazy binding |
| `riscv64gc-unknown-linux-gnu` | ELF64 | **Implemented, not reachable** — RV64GC LP64D, scalar FP, atomics, vector encodings, RVC alignment; unencodable variants fail loudly, but real source builds stop at the first symbol materialization — measured 2026-11-03 on a hello-world, `unsupported machine instruction Move { dst: Register(Physical(5)), src: Symbol("__str_0") }`; `ParallelMove` has no encoding either. Loud about operand forms as of 2026-11-01: the placeholder-register `unwrap_or` substitutions in the vector, load/store and atomic arms, the `sp` base `VectorLoad`/`VectorStore` hardcoded into its encoding, the `SetCc`/`BranchCc` fallthrough that turned `Parity`/`NotParity` into a constant `0` or onto `Equal`, the width defaults outside {1,2,4,8} and the masked `shamt` are all replaced by `CodegenError`s naming the instruction and function. Refused, not implemented: spilled atomic operands, indexed `Load`/`Store`, vector load/store. | **Native ELF** — CALL/JAL/BRANCH/HI20/LO12_I/LO12_S/PCREL_HI20/PCREL_LO12, `_start` synthesis, PLT/GOT; static structure tested | **None** | Symbol materialization in a `Move` (AUIPC + ADDI/ADD — the first thing a real program needs, per the measured hello-world failure above) and `ParallelMove`; then the rest of lowering for real programs, scratch materialization so a spilled atomic operand encodes instead of failing, qemu-riscv64 execution CI, target runtime archive. Since 2026-11-04 the refusal is *ordered* ahead of the archive question: `tests/native_phase12_cross_link_test.rs::riscv64gc_is_refused_by_codegen_before_the_archive_is_resolved` asserts a hello-world build fails in codegen and never reaches `No runtime archive`, so a staged archive can never be mistaken for the RISC-V blocker. Measured again the same day: `ADOB emit error: Adesh CodeGen Error` |
| `riscv32imac-unknown-none-elf` | ELF32 | **Refused at backend selection** — `create_backend` rejects `Architecture::RiscV32` with a `CodegenError` naming the triple and the reason (the RV backend is RV64-only), before any function body is encoded. Pinned by `tests/native_phase11_codegen_loudness_test.rs::riscv32_is_refused_at_backend_selection`. | **Refused for executables** — the linker synthesizes only x86_64/AArch64/RISC-V-64 `_start` stubs, so an RV32 link without a provided `_start` is a loud error instead of an RV64 stub. Pinned by `linker/tests/phase11_riscv32_entry_stub_test.rs`. | **None** — and no longer claimed otherwise: the target is Tier 3 and is not advertised as codegen-validated | Either implement ILP32 lowering + a real RV32 entry (Phase 15), or keep it refused |
| `wasm32-unknown-wasi` | WASM binary | **Real compiler backend** — emits only `wasi_snapshot_preview1.fd_write` plus in-module write/alloc/print adapters, a `__heap` global and `memory.grow`; exports `main`/`malloc`/`memory`; execution-verified under wasmtime by `tests/wasm_wasi_compiler_test.rs` | **Linker cross-links objects** — type/import/function/table/memory/global/export/element/data-count/code/data are emitted from a link plan; function/global/data/table-index/table-number/type-index `R_WASM_*` fixups are patched in place; a call or data reference another input defines binds to that definition and its phantom `env` import is dropped, while duplicate strong definitions, signature mismatches, undefined data addresses and table entries naming host functions are loud errors; the linker defines the one `funcref` table and fills it from the inputs' element segments with slot 0 reserved for a null trap, so `call_indirect` links; object export sections are parsed and the entry is exported (no start section, matching `wasm-ld`). Defined tables/globals remain errors. Note: `adesh build --target wasm32-*` still routes through the compiler backend; the linker path is reachable only via `adeshlink --target wasm32-*` / `--format wasm`. | **Execution-verified on two engines in CI** — real clang objects link and run with asserted output one at a time (`linker/tests/wasm_hello_world_test.rs`) and two at a time (`linker/tests/wasm_cross_object_test.rs`, `linker/tests/wasm_pure_module_test.rs`, `linker/tests/wasm_table_test.rs`) under V8, and all of them under Cranelift (`tests/wasm_wasmtime_execution_test.rs`, using the workspace's `wasmtime` 36 dependency and its own `fd_write` host); the table suite includes indirect calls inside one object, function pointers stored in data (checked against `wasm-ld`'s slot numbering), and a cross-object `call_indirect`; every suite is run by the ubuntu CI job; compiler path verified under wasmtime by `tests/wasm_wasi_compiler_test.rs` (import surface, stdout, heap allocation, loud `file:` rejection) | Host functions beyond `fd_write` (`proc_exit`, `args_get`, `environ_get`, `clock_time_get`, `random_get`); float printing is six-digit rounded decimal rather than shortest-round-trip |
| `wasm32-unknown-unknown` | WASM binary | **Real compiler backend** — `env.*` imports (`print_f64`, `print_str`, `alloc`, `concat2`, `file_write_str`) served by a generated Node loader; exports `main`/`malloc`/`memory` | **Same linker** — relocations applied, cross-object and indirect calls resolved, entry exported; defined tables/globals remain loud | **Execution-verified on two engines** (`linker/tests/wasm_hello_world_test.rs`, `wasm_cross_object_test.rs`, `wasm_pure_module_test.rs` and `wasm_table_test.rs` under V8, `tests/wasm_wasmtime_execution_test.rs` under wasmtime/Cranelift); two objects that call only each other produce a module with an empty import section that both engines instantiate with no host; compiler path verified via Node loader | Same as `wasm32-unknown-wasi`, minus the WASI runtime (non-WASI targets keep the `env.*` + Node loader path, covered by `tests/static_imports_test.rs`) |
| `thumbv7em-none-eabihf` | ELF32 | **Proof-of-concept** — 146-line Thumb-2 ALU subset only | Embedded library modules (linkscripts, IVT, crt0, flash/HEX/SREC writers) are unit-level only; no end-to-end firmware link | **None** | Either build an end-to-end firmware link + qemu/hardware test, or mark the target unsupported |
| `i686-unknown-linux-gnu` / `i686-pc-windows-gnu` | ELF32 / PE32 | **None** — no 32-bit x86 backend exists | **Silent fallback (found 2026-11-02, not yet closed):** `linker/src/arch/mod.rs:44` maps `Arch::X86` to `X86_64Arch`, so an i686 link runs the 64-bit relocation handler rather than failing. The target is no longer advertised by `adesh target list` / `adeshlink targets` (Tier 3), and `linker/tests/phase11_target_tiers_test.rs::refused_targets_are_not_advertised_as_supported` pins that the summary calls it out | **None** | Apply the Phase 11 rule here: refuse `Arch::X86` in `get_handler` with a named error, or implement an IA-32 handler. Until then no 32-bit x86 artifact can be trusted |
| `nvptx64-nvidia-cuda` | ADOB kernel package | **None** — source text re-embedded | Container packaging only | **None** | Unchanged scaffolding |
| `amdgcn-amd-amdhsa` | ADOB kernel package | **None** — source text re-embedded | Container packaging only | **None** | Unchanged scaffolding |
| `quantum-circuit-qasm3` | QIRB container | **None** — no language-level `qubit` construct | Container embed | State-vector simulator + QASM 3.0 export only | Unchanged; no hardware backend |

---

## Pending by target (summary)

| Target | Blocking gap | Severity |
| :--- | :--- | :--- |
| `aarch64-unknown-linux-gnu` | **Silent miscompile — FIXED 2026-11-01** (the encoder now returns a `CodegenError` for every unencodable `MachineInstruction` variant and operand form instead of emitting a `NOP` placeholder or dropping operands; pinned by `tests/native_phase11_codegen_loudness_test.rs`). **Host-runtime fallback — FIXED 2026-11-03** (a cross build is offered only paths naming its own triple, and the memo cache is keyed by triple; `linking.rs::runtime_archive_tests`). **Undefined `__str_0` at the link — FIXED 2026-11-04** (`emit_object` defines `.rodata`/`.tls`/`.data`; pinned by `test_aarch64_emit_object_defines_the_string_pool`). With those closed **a hello-world source program links** — 277,416-byte ELF, `e_machine` 183, 8 `aot_*` definitions. Still open: no CI-produced archive yet (the `runtime-archives` job has never been dispatched, and the `cross-link` job that would consume it neither); no execution test at all; FP/SIMD stack access (`LDR`/`STR Dd, [SP]`) is refused rather than encoded. | **Medium** — source builds link; execution is the gap, not the build. The silent-corruption and wrong-architecture-runtime classes are both closed. |
| `x86_64-pc-windows-msvc` | PE DLL execution test; source-level ABI surface; vector ABI | Medium |
| `x86_64-unknown-linux-gnu` | Runtime unwinder test; cross-target runtime archives; source-level ABI surface | Medium |
| `x86_64-unknown-linux-musl` | Dedicated musl CI job (emission is covered since 2026-11-02; loading the artifact is not) | Low |
| `aarch64-apple-darwin` | Any execution at all under dyld; TLV execution; and no archive builder has ever run for it, so a source build still stops at resolution (2026-11-04) | High |
| `x86_64-apple-darwin` | **New 2026-11-04:** the Mach-O writer puts a pointer fixup in `__TEXT`, so the first real source link fails `error[LNK009]: cannot rebase address …` — the target cannot link a source program at all, which outranks the execution gap. Then: dedicated execution test (today only indirect via static-import AOT) | **High** |
| `riscv64gc-unknown-linux-gnu` | **Silent miscompiles — FIXED 2026-11-01** (placeholder-register `unwrap_or` substitutions, the `sp`-based vector load/store, the `Parity`/`NotParity` fallthrough, the width and `shamt` defaults and unresolved/out-of-range branch fixups all return `CodegenError`s now; pinned by `tests/native_phase11_codegen_loudness_test.rs`). Still open: `ParallelMove` and the rest of lowering block real source builds; spilled atomic operands, indexed `Load`/`Store` and vector load/store are refused rather than encoded; no execution test. | **Critical** for source builds (unreachable today) — the silent-corruption class is closed. |
| `riscv32imac-unknown-none-elf` | Refused at backend selection and at `_start` synthesis — no RV64 bytes reach an RV32 image; ILP32 lowering itself is still absent | High (not functional) — **no longer advertised as validated** |
| `wasm32-unknown-wasi` / `-unknown-unknown` | The linker routes imports but provides no WASI runtime of its own. Multi-object linking, indirect calls, execution on two engines in CI, and the compiler's `wasi_snapshot_preview1.fd_write` ABI are **done** | Medium |
| `thumbv7em-none-eabihf` | No end-to-end firmware link or execution | Medium (PoC) |
| GPU / quantum | Unchanged scaffolding | — |

## Next important (ordered)

The phase-by-phase execution plan for these items is
`NEXT_IMPLEMENTATION_PLAN.md` (Phases 11–21).

1. **Backend correctness gate — DONE for AArch64 and RISC-V 2026-11-01.** The `_
   =>` NOP fallback and every silent operand-drop in
   `crates/adesh-codegen/src/targets/aarch64/mod.rs` now return a `CodegenError`
   naming the `MachineInstruction` variant and its function;
   `tests/native_phase11_codegen_loudness_test.rs` pins the gate (29 unencodable
   variants, 14 unencodable operand forms, the unallocated-register pre-encode
   check, a no-unintended-NOP scan over emitted `.text`, and BL-site relocation
   offsets). The same gate was applied to RISC-V the same day: 21 variants and 43
   operand forms refused, the `sp`-based vector load/store and `Parity`/`NotParity`
   defaults gone, plus branch-fixup range checks. x86_64 was already loud and the
   embedded backend ships as a loud stub; the wasm backend has its own
   unsupported-instruction suite. On 2026-11-02 the same rule was applied one
   level up: RV32 is refused while the backend is selected, and the linker
   refuses to hand an RV32 executable its RV64 `_start` stub. The one
   fallback found still open is in the linker, not the backends —
   `linker/src/arch/mod.rs:44` routes `Arch::X86` to the x86_64 handler (see
   the i686 row above). The remaining AArch64 blocker is a qemu execution job —
   the runtime archive and the string-pool definition gap behind `LNK001` are
   gone, and a hello-world links against a staged archive (2026-11-04).
2. **Execution verification for Mach-O** — link and run a produced binary on
   the macOS CI runner for both `aarch64-apple-darwin` and
   `x86_64-apple-darwin`, including a dyld-bound import and a TLV access. Every
   structural test passing means nothing until dyld accepts the image. Ordered
   ahead of it as of 2026-11-04: `x86_64-apple-darwin` cannot link a source
   program at all (`error[LNK009]`, a pointer fixup landing in `__TEXT`), and
   `aarch64-apple-darwin` has no archive in any CI run, so neither reaches the
   stage where a run could be attempted.
3. **Target runtime archives** — **done in code, and a cross link is now proven
   on a host that can stage an archive (2026-11-04).**
   Resolution is target-scoped: `target/<triple>/<profile>/`, an install's
   `lib/<triple>/` and `$ADESH_RUNTIME_DIR/<triple>/` are searched before
   anything else, host directories are only reachable for the host triple, and
   the missing-archive error names the triple, lists every path asked for and
   prints the command that stages one. `scripts/build-runtime-archives.sh` builds
   and stages the archive per triple (default `--profile debug`), the CI
   `runtime-archives` job runs it for the Linux cross targets on Ubuntu and for
   both Apple triples on macOS, Unix/Docker/Windows packaging ships
   `lib/<triple>/`, and a new `cross-link` job downloads those artifacts, stages
   them at `target/<triple>/debug/` and runs Phase 12 item 4 —
   `tests/native_phase12_cross_link_test.rs`, 8 tests, which asserts on the image
   bytes rather than the exit code. Locally measured on a Windows host: 4 of the 8
   targets link (three x86-64 archives and `aarch64-unknown-linux-gnu`), the rest
   fail loudly for reasons named in their matrix rows. One thing keeps this open:
   **neither job has been dispatched**, so every non-host archive in that run was
   built by hand on this machine and no CI log yet shows one being built or
   consumed.
4. **RISC-V lowering completion** — start with `ParallelMove`; then add
   qemu-riscv64 execution to CI.
5. **WASM linker coverage** — execution is now verified on two engines in CI
   (V8 through Node, Cranelift through the workspace `wasmtime` dependency),
   multi-object links resolve cross-object calls and data, and indirect calls
   dispatch through a linker-defined table. The compiler emits real
   `wasi_snapshot_preview1` imports for `-wasi` triples; the linker still
   routes imports but provides no WASI runtime of its own.
6. **Source-level ABI surface** — `extern`, variadic functions, aggregate
   parameters, atomics, and TLS syntax, so the Phase 2 MachineIR/FFI features
   become reachable from Adesh source.
7. **Shared-library execution** — PE DLL load + export call and Mach-O dylib
   load. ELF is already verified through `dlopen`/`dlsym`.
8. **Windows CI gap — DONE 2026-11-02.** The three
   `tests/native_phase8_jit_native_conformance_test.rs` failures were generated
   import statements embedding unescaped Windows paths (the lexer collapses an
   unknown `\X` to `X`, so `C:\Users\…` became `C:Users…` → OS error 123). The
   suite now renders every interpolated path through `adesh_string_body`, which
   escapes `\` and `"`; it passes 10/10 on Windows and is named in the Windows
   CI job.
9. **Realign target tiers — DONE 2026-11-02.** `Target::tier()` in
   `linker/src/target.rs` is derived from CI verification, not capability:
   Tier 1 = a CI job builds the artifact **and runs it** with asserted output or
   exit status (`x86_64` Windows/Linux-GNU, `wasm32` WASI and freestanding, the
   last on two engines); Tier 2 = emitted and parsed back, never executed
   (AArch64 Linux/macOS/Windows, `x86_64-apple-darwin`, RISC-V 64, musl);
   Tier 3 = declared only. The previous labels overstated AArch64 (it was
   `Tier1Supported`) and understated WASM (an earlier revision moved it to
   Tier 2 before CI ran its linked modules on two engines). The user-visible
   lists are now data-driven from the same table
   (`ADVERTISED_TARGETS` + `advertised_by_tier()` +
   `TargetTier::advertised_heading()`), so `adesh target list` and
   `adeshlink targets` cannot drift from `tier()`; Android
   (`aarch64-linux-android`, `armv7-linux-androideabi`, `aarch64-apple-ios`) is
   no longer offered at all, and `i686`, `riscv32*`, `armv7`/Thumb and
   `powerpc64le` appear only under Tier 3 with a summary that names the refusal.
   `x86_64-pc-windows-gnu` dropped from Tier 1 to Tier 2 for the same reason:
   every suite that links **and runs** a PE asks for the `-msvc` triple.
   `linker/tests/phase11_target_tiers_test.rs` pins
   each label to the suite that earns it and refuses any advertised summary
   that claims more than its tier, and
   `linker/tests/phase11_riscv32_entry_stub_test.rs` pins that a refused
   target is not labelled codegen-validated.

## Corrections applied to CURRENT_STATE.md (2026-10-24)

The following stale claims were corrected in `CURRENT_STATE.md` in the same
change set:

* **Mach-O TLS "fails the link loudly (no TLV emission)"** — the Mach-O writer
  emits `__thread_data`/`__thread_bss` plus `__thread_vars` descriptors and
  TLVP/ADDEND relocations. Execution is still unverified.
* **"Shared-library execution remains unverified"** — ELF shared-library
  execution *is* verified by `test_shared_lib_dlopen_dlsym_and_call_executes`
  in `tests/native_phase8_dynamic_elf_test.rs`. PE DLL and Mach-O dylib
  execution remain unverified.
* **AArch64 "Implemented & Tested"** — codegen and relocations are implemented,
  but the backend silently NOPs unsupported instructions and emits approximate
  relocation offsets. The stale row overstated correctness, not just execution.
  Both defects are now closed: real call-site relocation offsets landed first, and
  the silent NOP fallthrough became a loud `CodegenError` on 2026-11-01. Execution
  remains unverified, so the row still is not a "Tested" claim. On 2026-11-04 the
  "cannot build a source program at all" half of the claim also became false: with
  a per-target archive staged the target links, the last gap having been
  `emit_object` emitting string references without defining the string pool.
* **RISC-V "Proof-of-concept"** — the backend is broader than that (RV64GC, FP,
  atomics, vectors), but real source builds fail on `ParallelMove`, so neither
  label is accurate; treat it as "implemented backend, unreachable from source".
  The 2026-11-01 silent-fallback review gate found RISC-V was *not* uniformly loud
  there — its vector, load/store and atomic arms substituted placeholder registers
  (`unwrap_or`) for operands they did not recognize, `VectorLoad`/`VectorStore`
  hardcoded `sp` as the base, `SetCc`/`BranchCc` collapsed `Parity`/`NotParity` onto
  a constant `0` and onto `Equal`, and widths and shift counts outside their fields
  defaulted. That whole class is closed on 2026-11-01: each case is a `CodegenError`
  naming the instruction and function, and the forms that need unimplemented
  addressing (spilled atomics, indexed `Load`/`Store`, vector load/store) are refused
  instead of approximated.
* **`x86_64-apple-darwin` relocations listed as "—"** — x86_64 Mach-O
  relocations (SIGNED, BRANCH, GOT, TLV) are implemented and tested.
* **Native JIT "imported modules are rejected"** — imports are now handled by
  the shared static pre-link path for NJIT/AOT/WASM (`tests/static_imports_test.rs`).
  Conformance is still not systematically asserted; the Windows-only 3/10
  failure in the conformance suite is a test-side path-escaping bug.

## Target Discovery & Capability Inspection

The compiler provides introspection for all targets:

```bash
adesh target list
adesh target info x86_64-pc-windows-msvc
adesh target info aarch64-apple-darwin
```
