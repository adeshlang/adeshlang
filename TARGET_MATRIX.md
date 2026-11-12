# Adesh Native Target Support Matrix

**Generated:** 2026-10-24 (native codegen + linker audit)
**AArch64 source update — 2026-11-12:** four real CLI source cases execute
under QEMU in both default and fast modes with a staged target runtime and
target libc. Print, exit, nested calls and a loop pass (eight executions).
The loop exposed reversed CSET predicates, now fixed with a failing-before
encoding regression. Required CI coverage is wired, not dispatched; Tier 2
is unchanged. See `CURRENT_STATE.md` §0.16.

**AArch64 startup update — 2026-11-12:** the linker-generated ELF entry now
passes QEMU execution locally (GNU assembly input, argc/argv/envp, stack
alignment, stdout and exit status). The old image crashed; its argc load
and envp scaling were wrongly encoded. Dedicated required CI job wired,
not dispatched. This is linker-startup coverage, not Adesh-source/codegen
execution conformance by itself; source coverage is recorded above.
AArch64 stays Tier 2. See `CURRENT_STATE.md` §0.15.

**PE DLL update — 2026-11-12:** Win64 DLL load, exported call, rebasing and
initializer behavior pass locally through `linker/tests/phase13_pe_dll_test.rs`.
Export-only DLLs now have a zero entry RVA; explicit missing/data entries fail.
The Windows CI step is wired but has not run. See `CURRENT_STATE.md` §0.14.

**Verification update — 2026-11-08:** the RV64 archive is built and staged locally
(44,077,682 bytes; 104 defined `aot_*` symbols). The remaining source-link blocker
is ELF relocation support, not packaging. Compressed branch/jump and ADD/SUB fixups pass eight
focused tests; the current refusal is relocation type 60 at offset `0x10b`. No RV64 execution is verified.
CI run 42396244918 failed before producing Apple archives. Windows payloads exist,
but compiler errors during packaging prevent a clean-release claim. See
`CURRENT_STATE.md` §0.13. This supersedes the archive-blocked descriptions below.

**Previous update:** 2026-11-07 (RISC-V **passes codegen**: `riscv64gc-unknown-linux-gnu`
no longer dies on the first symbol materialization. The backend now emits
`auipc rd, %pcrel_hi(sym)` + `addi rd, rd, %pcrel_lo(sym)` for a `Move` whose source is
a `Symbol`, expands `ParallelMove` through `ParallelMoveResolver::for_riscv64()` before
encoding, and defines the string pool it references via the shared
`targets::emit_module_data_sections`. Measured on a hello-world: the command that
reported `unsupported machine instruction Move { dst: Physical(5), src: Symbol("__str_0") }`
now reports `No runtime archive for target riscv64gc-unknown-linux-gnu` — the blocker
moved from the backend to packaging.
The relocation contract that link depends on is proven on **linked image bytes**, not
object bytes, by `linker/tests/riscv64_pcrel_symbol_pair_test.rs` (3 tests). RISC-V
defines `%pcrel_lo` relative to the `AUIPC`, but the relocation engine subtracts each
relocation's own address, so codegen passes `addend = 4` on `RiscV_PcrelLo12I`. Measured:
`.text` at `0x401000`, `.rodata` at `0x402000`; 17 pairs spanning a `0x1080` delta range
all reconstruct their symbol exactly; pairs 8–15 have `lo12` bit 11 set, so `hi20` rounds
up past the true delta (pair 8: delta `0x1840` = `0x2000` + sign-extended `0x840`). The
A/B that makes the `+4` load-bearing: the same object with addend 0 still links and still
produces well-formed instructions, but lands four bytes low. A backward reference at
`.text+0x2000` to `main` at `.text+0` decodes `hi20 = -2`, exercising the arithmetic
shift on a negative delta.
A silent-corruption bug surfaced while writing that test: `AdobV2::encode` mapped every
relocation kind outside its table to id 0 (`Absolute64`) and `decode` mapped unknown ids
back to `Absolute64`, and the table had no RISC-V entries — so `adeshlink strip` on a
RISC-V ADOB object turned 4-byte PC-relative fixups into 8-byte absolute ones and the
object still linked, just wrong. Ids 30–33 now round-trip and both catch-alls refuse with
`LNK008` instead of downgrading; kinds with no assigned id are refused rather than given
a newly invented persisted id.
Verification: `cargo test -p adesh-linker` green (59 lib + all integration suites,
including the 3 new image tests and 4 ADOB codec tests), `-p adesh-codegen` **107 passed**,
`native_phase11_codegen_loudness_test` **20 passed**, `native_phase12_cross_link_test`
**9 passed**, `native_phase7_riscv64_e2e_test` **5 passed**, `cargo fmt` and
`cargo clippy -p adesh-linker -p adesh-codegen --all-targets` clean.
Still **not execution-verified**: no riscv64 runtime archive exists on this host (building
one needs `gcc-riscv64-linux-gnu` + `libc6-dev-riscv64-cross`, which the never-dispatched
CI `runtime-archives` matrix already installs) and no qemu is present.)
Earlier: 2026-11-05 (Mach-O `LNK009` closed: `x86_64-apple-darwin` **links a
source program**. The compiler materialized a symbol address as `movabs` + an absolute
relocation inside `.text`; a Mach-O image is always PIE and dyld applies rebase fixups
only in `__DATA`/`__DATA_CONST`, so the first real source link failed
`error[LNK009]` for *every* program. The x86_64 backend now emits
`lea r64, [rip + disp32]` + a PC-relative relocation when the image format is Mach-O
and keeps `movabs` for PE and ELF. Byte-verified on the linked image:
`lea rcx, [rip+0xff5]` at `.text+4` resolves to `.rodata+0` — the program's own string.
Pinned by `tests/native_phase12_cross_link_test.rs::the_mach_o_image_needs_no_fixup_inside_text`
(9/9 in the suite now), whose non-vacuity was checked by reverting the guard: the test
fails with `LNK009`. Two findings came out of the fix: the guard originally could not
fire because `TargetDescriptor::object_format` names the ADOB *container* and is always
`Adob`, so `linker`-side image choice now comes from a new `TargetDescriptor::image_format()`;
and `LNK009` used to be unactionable, so Mach-O rebase records carry the object, section,
offset and symbol that produced them.
Earlier: 2026-11-04 (Phase 12 item 4: `tests/native_phase12_cross_link_test.rs`
links a hello-world source program to each of the 8 advertised native targets on a
Windows host and asserts the produced **image bytes** — ELF/PE/Mach-O header plus the
`aot_*` symbol table — not the exit code. 8/8 pass. Measured per target: the three
x86-64 archives (glibc, musl, Windows) and `aarch64-unknown-linux-gnu` **link**;
`riscv64gc` is refused in codegen before the archive is resolved; `x86_64-apple-darwin`
resolves its archive and fails the Mach-O link at `error[LNK009]` — closed the next
day, above; the two triples no CI job builds (`aarch64-apple-darwin`,
`aarch64-pc-windows-msvc`) fail with the missing-archive error. AArch64 links because
`emit_object` now defines the string pool it references (`__str_0` was an undefined
reference until this change). A `cross-link` CI job stages the `runtime-archives`
artifacts at `target/<triple>/debug/` and runs the suite on all three OSes; neither new
job has been dispatched.)
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
| `x86_64-pc-windows-msvc` | PE32+ | **Native** — 64-bit GPR + scalar SSE2, XMM0-13, stack args, Win64 shadow space, callee-saved, atomics, Win64 TLS | **Native PE** — imports/IAT, TLS dir, base relocs, ASLR, DLL export tables; ABS64 + PC32 | **Execution-verified** — PE executables run incl. a real `msvcrt!puts` import (`linker/tests/pe_windows_e2e_test.rs`, `tests/native_x86_64_e2e_test.rs`, phase 2-8 suites); DLL load/export call, rebasing and initialization locally verified (`linker/tests/phase13_pe_dll_test.rs`) | Source-level `extern`/variadic/aggregate/atomic/TLS syntax; vector ABI; PE DLL CI execution (suite passes locally) |
| `x86_64-unknown-linux-gnu` | ELF64 | **Native** — same subset as Windows (SysV variant) | **Native ELF** — static + dynamic, PLT/GOT, JUMP_SLOT, COPY, GOTPCREL, PIE `RELATIVE`, TLS LE/IE, `.eh_frame`/`.eh_frame_hdr`, `_start` synthesis, GNU/COFF archives, GC + ICF | **Execution-verified** — static ELF (`tests/native_phase3_linux_e2e_test.rs`), dynamic ELF and a `dlopen`/`dlsym` shared library (`tests/native_phase8_dynamic_elf_test.rs`), TLS LE/IE (`tests/native_phase8_tls_ehframe_test.rs`). Minimum GLIBC baseline: **GLIBC 2.35 (Ubuntu 22.04 LTS)** | Runtime unwinder test (`.eh_frame` is decoder-verified only); source-level ABI surface; cross-target runtime archive for non-host triples |
| `x86_64-unknown-linux-musl` | ELF64 (static + dynamic) | **Native** — same x86-64 subset | **Native ELF** — same relocations; TLS TPOFF64 | **Emission-tested in CI** (added 2026-11-02: `tests/native_phase8_cross_compile_test.rs::test_cross_target_emission_and_validation` now emits and validates a musl ADOB on the ubuntu, Windows and macOS jobs). Execution with asserted exit codes is **WSL-only**: static local-exec and dynamic initial-exec TLS binaries run against the real musl `ld.so` on a developer machine | **No dedicated musl CI job** — CI never loads a musl artifact, so the target stays Tier 2; add a musl container/runner |
| `aarch64-unknown-linux-gnu` | ELF64 | **Implemented, loud about what it cannot encode** — AAPCS64, X19-X28/D8-D15 preservation, scalar FP, NEON, atomics. Since 2026-11-01 an unencodable `MachineInstruction` variant or operand form returns a `CodegenError` naming the instruction and its function; the encoder no longer emits a `NOP` placeholder, no longer drops instructions whose operands it did not recognize, and no longer substitutes a placeholder register. | **Static startup execution-verified locally; broader relocation coverage remains structural** — ADRP+ADD symbol materialization **is emitted** now (`AArch64_AdrPage21` + `AArch64_AddAbsLo12`), and `emit_object` bases each relocation on the accumulated `.text` length, so call relocations land on their real call sites (both were defects in the 2026-10-24 audit and are fixed). Still no `aarch64` runtime archive is produced by a released CI run. What changed: since 2026-11-03 a source build stops *loudly* when the archive is absent (Phase 12 item 1), and since 2026-11-04, **with an archive staged, a hello-world source program links** — `tests/native_phase12_cross_link_test.rs` measures a 277,416-byte ELF with `e_machine` 183 and 8 `aot_*` definitions on the AArch64 path. That link only became possible with the same change, which fixed the last codegen gap: `emit_object` in `crates/adesh-codegen/src/targets/aarch64/mod.rs` now emits `.rodata` / `.tls` / `.data` (the shared lowering fills the string pool for every backend and the encoder names `__str_<n>`, but only x86_64 was defining them), so the reference is no longer `error[LNK001]: undefined reference to __str_0`. Pinned by `test_aarch64_emit_object_defines_the_string_pool`, because the cross-OS link test needs an archive only some hosts can build. | **Linker startup executed locally under QEMU (2026-11-12)** — GNU ELF input through `adeshlink`, validated argc/argv/envp, stack alignment, stdout and exit status (`linker/tests/phase13_aarch64_exec_test.rs`). Four Adesh-source fixtures also execute locally in default/fast modes through target runtime/libc (`tests/native_phase13_aarch64_exec_test.rs`, §0.16); required CI job wired, not dispatched | 1) Run the required QEMU CI job; linker startup and four source fixtures now pass locally; 2) FP/SIMD stack access (`LDR`/`STR Dd, [SP]`) — the loudness gate revealed that spilled float/vector values reach the encoder as stack slots, which it now refuses; before, it wrote them as 64-bit GPR loads with the register number truncated to 5 bits (`V30` became `X30`, the link register); 3) `aarch64-apple-darwin` and `aarch64-pc-windows-msvc` still have no archive builder in CI (only `aarch64-unknown-linux-gnu` is in the `runtime-archives` matrix). Done since the audit: the NOP fallthrough, real call-site relocation offsets, ADRP/ADD_LO12 emission, the string/TLS/data sections, and a link that consumes a per-target archive. |
| `aarch64-apple-darwin` | Mach-O 64 | **Implemented** — AAPCS64 path | **Native Mach-O** — BRANCH26, PAGE21, PAGEOFF12, TLVP, ADDEND; TLS is emitted as `__thread_data`/`__thread_bss` plus a `__thread_vars` descriptor table; dyld bind/rebase, exports trie, stubs/GOT parse-back tested | **None** — `linker/tests/macho_dyld_binding_test.rs` decodes the dyld structures but never runs under dyld | **Measured 2026-11-04:** a source build does not reach the Mach-O link — `adesh build --target aarch64-apple-darwin` fails at archive resolution ("No runtime archive for target aarch64-apple-darwin"), because no job has yet built the archive the `runtime-archives` macOS entry would produce. Then: dedicated link-and-run on the macOS CI runner; TLV execution; code-signature acceptance by real dyld |
| `x86_64-apple-darwin` | Mach-O 64 | **Native** — x86-64 subset. A symbol address is materialized per **image** format: `lea r64, [rip + disp32]` + `PcRelative32` (addend −4) on Mach-O, `movabs` + `Absolute64` on PE and ELF, because dyld cannot rebase a pointer inside `__TEXT`. Gated on `TargetDescriptor::image_format()` — the descriptor's `object_format` is always `Adob` (it names the emitted container), so branching on it silently never fires; pinned by `crates/adesh-object/src/target.rs::image_format_comes_from_the_operating_system_not_the_emitted_container` and `crates/adesh-codegen`'s `a_symbol_address_is_pc_relative_only_on_mach_o` | **Native Mach-O** — x86_64 SIGNED/BRANCH/GOT/TLV relocations, libSystem routing, runtime archive symbol lookup. **Links a source program since 2026-11-05:** `adesh build --target x86_64-apple-darwin --emit=exe` resolves `target/x86_64-apple-darwin/release/libadesh_runtime.a` and produces a 17,194-byte `MH_MAGIC_64` / `MH_EXECUTE` / `MH_PIE` image, `cputype` `0x01000007`, with the `aot_*` runtime from the archive and `_write`/`__exit` bound through `__got`. Before that it failed `error[LNK009]: cannot rebase address 0x100001006` for *every* program, because the compiler put an absolute pointer in `__text`; the refusal is now actionable — rebase records name the object, section, offset and symbol | **Indirect only** — `tests/static_imports_test.rs` `--aot` path executes Mach-O binaries on macOS CI; no dedicated link-and-run or bind/rebase runtime assertion. The image is parsed back (header, sections, the string pointer the `lea` resolves to) but never run: a Windows host cannot execute it | A dedicated macOS execution test (build + run + asserted output on the macOS runner) and runtime verification of `__stubs`/lazy binding; `x86_64-apple-darwin` stays Tier 2 — link-verified, not run — until then |
| `riscv64gc-unknown-linux-gnu` | ELF64 | **Implemented, reaches the linker** — RV64GC LP64D, scalar FP, atomics, vector encodings, RVC alignment; unencodable variants fail loudly. Since 2026-11-07 a `Move` whose source is a `Symbol` encodes as `auipc rd, %pcrel_hi(sym)` + `addi rd, rd, %pcrel_lo(sym)` (LO12 addend 4: the engine subtracts each relocation's own address, while RISC-V defines the low half relative to the AUIPC), `ParallelMove` is expanded by `ParallelMoveResolver::for_riscv64()` before encoding, and `emit_object` defines the string pool it references. Measured on a hello-world: the 2026-11-03 failure `unsupported machine instruction Move { dst: Register(Physical(5)), src: Symbol("__str_0") }` is gone and the build now stops at `No runtime archive for target riscv64gc-unknown-linux-gnu`. Loud about operand forms as of 2026-11-01: the placeholder-register `unwrap_or` substitutions in the vector, load/store and atomic arms, the `sp` base `VectorLoad`/`VectorStore` hardcoded into its encoding, the `SetCc`/`BranchCc` fallthrough that turned `Parity`/`NotParity` into a constant `0` or onto `Equal`, the width defaults outside {1,2,4,8} and the masked `shamt` are all replaced by `CodegenError`s naming the instruction and function. Refused, not implemented: spilled atomic operands, indexed `Load`/`Store`, vector load/store. | **Native ELF** — CALL/JAL/BRANCH/HI20/LO12_I/LO12_S/PCREL_HI20/PCREL_LO12, `_start` synthesis, PLT/GOT; static structure tested | **None** | **Target runtime archive** — the only thing between this triple and a link on a host that has one; building it needs a cross C toolchain (`gcc-riscv64-linux-gnu` + `libc6-dev-riscv64-cross`) because `crates/adesh-runtime/build.rs` compiles `native_output.c` for the target. Then the rest of lowering for real programs, scratch materialization so a spilled atomic operand encodes instead of failing, and qemu-riscv64 execution CI. `tests/native_phase12_cross_link_test.rs::riscv64gc_passes_codegen_and_stops_at_the_runtime_archive` pins the ordering: no `unsupported machine instruction` may appear, and with no archive staged the error must be the resolution error naming the triple and `scripts/build-runtime-archives.sh`; with an archive staged it asserts the link outcome and prints `PROMOTED:`. The relocation contract is pinned on linked image bytes by `linker/tests/riscv64_pcrel_symbol_pair_test.rs`. |
| `riscv32imac-unknown-none-elf` | ELF32 | **Refused at backend selection** — `create_backend` rejects `Architecture::RiscV32` with a `CodegenError` naming the triple and the reason (the RV backend is RV64-only), before any function body is encoded. Pinned by `tests/native_phase11_codegen_loudness_test.rs::riscv32_is_refused_at_backend_selection`. | **Refused for executables** — the linker synthesizes only x86_64/AArch64/RISC-V-64 `_start` stubs, so an RV32 link without a provided `_start` is a loud error instead of an RV64 stub. Pinned by `linker/tests/phase11_riscv32_entry_stub_test.rs`. | **None** — and no longer claimed otherwise: the target is Tier 3 and is not advertised as codegen-validated | Either implement ILP32 lowering + a real RV32 entry (Phase 15), or keep it refused |
| `wasm32-unknown-wasi` | WASM binary | **Real compiler backend** — emits only `wasi_snapshot_preview1.fd_write` plus in-module write/alloc/print adapters, a `__heap` global and `memory.grow`; exports `main`/`malloc`/`memory`; execution-verified under wasmtime by `tests/wasm_wasi_compiler_test.rs` | **Linker cross-links objects** — type/import/function/table/memory/global/export/element/data-count/code/data are emitted from a link plan; function/global/data/table-index/table-number/type-index `R_WASM_*` fixups are patched in place; a call or data reference another input defines binds to that definition and its phantom `env` import is dropped, while duplicate strong definitions, signature mismatches, undefined data addresses and table entries naming host functions are loud errors; the linker defines the one `funcref` table and fills it from the inputs' element segments with slot 0 reserved for a null trap, so `call_indirect` links; object export sections are parsed and the entry is exported (no start section, matching `wasm-ld`). Defined tables/globals remain errors. Note: `adesh build --target wasm32-*` still routes through the compiler backend; the linker path is reachable only via `adeshlink --target wasm32-*` / `--format wasm`. | **Execution-verified on two engines in CI** — real clang objects link and run with asserted output one at a time (`linker/tests/wasm_hello_world_test.rs`) and two at a time (`linker/tests/wasm_cross_object_test.rs`, `linker/tests/wasm_pure_module_test.rs`, `linker/tests/wasm_table_test.rs`) under V8, and all of them under Cranelift (`tests/wasm_wasmtime_execution_test.rs`, using the workspace's `wasmtime` 36 dependency and its own `fd_write` host); the table suite includes indirect calls inside one object, function pointers stored in data (checked against `wasm-ld`'s slot numbering), and a cross-object `call_indirect`; every suite is run by the ubuntu CI job; compiler path verified under wasmtime by `tests/wasm_wasi_compiler_test.rs` (import surface, stdout, heap allocation, loud `file:` rejection) | Host functions beyond `fd_write` (`proc_exit`, `args_get`, `environ_get`, `clock_time_get`, `random_get`); float printing is six-digit rounded decimal rather than shortest-round-trip |
| `wasm32-unknown-unknown` | WASM binary | **Real compiler backend** — `env.*` imports (`print_f64`, `print_str`, `alloc`, `concat2`, `file_write_str`) served by a generated Node loader; exports `main`/`malloc`/`memory` | **Same linker** — relocations applied, cross-object and indirect calls resolved, entry exported; defined tables/globals remain loud | **Execution-verified on two engines** (`linker/tests/wasm_hello_world_test.rs`, `wasm_cross_object_test.rs`, `wasm_pure_module_test.rs` and `wasm_table_test.rs` under V8, `tests/wasm_wasmtime_execution_test.rs` under wasmtime/Cranelift); two objects that call only each other produce a module with an empty import section that both engines instantiate with no host; compiler path verified via Node loader | Same as `wasm32-unknown-wasi`, minus the WASI runtime (non-WASI targets keep the `env.*` + Node loader path, covered by `tests/static_imports_test.rs`) |
| `thumbv7em-none-eabihf` | ELF32 | **Proof-of-concept** — 146-line Thumb-2 ALU subset only | Embedded library modules (linkscripts, IVT, crt0, flash/HEX/SREC writers) are unit-level only; no end-to-end firmware link | **None** | Either build an end-to-end firmware link + qemu/hardware test, or mark the target unsupported |
| `i686-unknown-linux-gnu` / `i686-pc-windows-gnu` | ELF32 / PE32 | **None** — no 32-bit x86 backend exists | **Silent fallback closed 2026-11-12:** `linker/src/arch/mod.rs` now maps `Arch::X86` to `X86Arch` (`linker/src/arch/x86.rs`), which refuses every relocation with `LNK008` naming the missing IA-32 handler instead of applying 64-bit semantics. Pinned by `linker/tests/phase11_x86_handler_refusal_test.rs`. An i686 object with no relocations still links, since no handler call is made; no IA-32 handler exists. The target is no longer advertised by `adesh target list` / `adeshlink targets` (Tier 3), and `linker/tests/phase11_target_tiers_test.rs::refused_targets_are_not_advertised_as_supported` pins that the summary calls it out | **None** | Keep unsupported; implement a real IA-32 handler and format/codegen path before promoting. Refusing relocations alone does not validate relocation-free images |
| `nvptx64-nvidia-cuda` | ADOB kernel package | **None** — source text re-embedded | Container packaging only | **None** | Unchanged scaffolding |
| `amdgcn-amd-amdhsa` | ADOB kernel package | **None** — source text re-embedded | Container packaging only | **None** | Unchanged scaffolding |
| `quantum-circuit-qasm3` | QIRB container | **None** — no language-level `qubit` construct | Container embed | State-vector simulator + QASM 3.0 export only | Unchanged; no hardware backend |

---

## Pending by target (summary)

| Target | Blocking gap | Severity |
| :--- | :--- | :--- |
| `aarch64-unknown-linux-gnu` | **Silent miscompile — FIXED 2026-11-01** (the encoder now returns a `CodegenError` for every unencodable `MachineInstruction` variant and operand form instead of emitting a `NOP` placeholder or dropping operands; pinned by `tests/native_phase11_codegen_loudness_test.rs`). **Host-runtime fallback — FIXED 2026-11-03** (a cross build is offered only paths naming its own triple, and the memo cache is keyed by triple; `linking.rs::runtime_archive_tests`). **Undefined `__str_0` at the link — FIXED 2026-11-04** (`emit_object` defines `.rodata`/`.tls`/`.data`; pinned by `test_aarch64_emit_object_defines_the_string_pool`). With those closed **a hello-world source program links** — 277,416-byte ELF, `e_machine` 183, 8 `aot_*` definitions. Still open: no CI-produced archive yet (the `runtime-archives` job has never been dispatched, and the `cross-link` job that would consume it neither); linker-startup and four source QEMU tests pass locally (default/fast modes), but required CI execution is pending; FP/SIMD stack access (`LDR`/`STR Dd, [SP]`) is refused rather than encoded. | **Medium** — source builds link and the tested integer/print subset executes locally; CI execution remains pending. The silent-corruption and wrong-architecture-runtime classes are both closed. |
| `x86_64-pc-windows-msvc` | PE DLL CI execution (locally verified); source-level ABI surface; vector ABI | Medium |
| `x86_64-unknown-linux-gnu` | Runtime unwinder test; cross-target runtime archives; source-level ABI surface | Medium |
| `x86_64-unknown-linux-musl` | Dedicated musl CI job (emission is covered since 2026-11-02; loading the artifact is not) | Low |
| `aarch64-apple-darwin` | Any execution at all under dyld; TLV execution; and no archive builder has ever run for it, so a source build still stops at resolution (2026-11-04). Measured again 2026-11-05: the archive is the *only* blocker — the AArch64 backend emits no `Absolute64` relocation at all (a repo-wide grep over `crates/adesh-codegen/src/targets` finds them only in the x86_64 backend), so the `LNK009` class that blocked `x86_64-apple-darwin` is not waiting behind it | High |
| `x86_64-apple-darwin` | **`LNK009` — FIXED 2026-11-05.** The compiler baked a symbol address into `__text` as `movabs` + an absolute relocation; a Mach-O image is always PIE and dyld rebases only writable segments, so *every* source link failed in the writer. The x86_64 backend now emits `lea r64, [rip + disp32]` for a Mach-O image (and keeps `movabs` for PE/ELF), and the target links: 17,194-byte `MH_EXECUTE`+`MH_PIE` image with the archive's `aot_*` runtime and `_write`/`__exit` bound via `__got`, with the `lea` verified to resolve onto the program's own string. Still open: any execution under real dyld — the image is parsed back, never run (a Windows host cannot), which is why it stays Tier 2 | **Medium** — it links; running it is the gap |
| `riscv64gc-unknown-linux-gnu` | **Silent miscompiles — FIXED 2026-11-01** (placeholder-register `unwrap_or` substitutions, the `sp`-based vector load/store, the `Parity`/`NotParity` fallthrough, the width and `shamt` defaults and unresolved/out-of-range branch fixups all return `CodegenError`s now; pinned by `tests/native_phase11_codegen_loudness_test.rs`). Still open: the rest of lowering for programs beyond hello-world; spilled atomic operands, indexed `Load`/`Store` and vector load/store are refused rather than encoded; no execution test. `ParallelMove` and symbol materialization are implemented as of 2026-11-07. | **Critical** for source builds — the silent-corruption class is closed, and since 2026-11-07 a hello-world reaches the linker instead of dying in codegen. |
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
   program at all (`error[LNK009]` — measured again below, the x86_64 backend
   baking an absolute address into `.text`, not a fixup placed by the writer), and
   `aarch64-apple-darwin` has no archive in any CI run, so neither reaches the
   stage where a run could be attempted. **The first of those two is closed
   2026-11-05** — `x86_64-apple-darwin` links a source program (see the matrix
   row), so its only remaining gap is the run itself; `aarch64-apple-darwin`
   still stops at archive resolution, which is now the sole item keeping it back.
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
4. **RISC-V archive + execution** — `ParallelMove` and symbol materialization
   landed 2026-11-07, so a hello-world now encodes completely and reaches the
   linker. The remaining blocker is a `riscv64gc-unknown-linux-gnu` runtime
   archive (needs a cross C toolchain), then qemu-riscv64 execution in CI and
   the rest of lowering for programs larger than hello-world.
5. **WASM linker coverage** — execution is now verified on two engines in CI
   (V8 through Node, Cranelift through the workspace `wasmtime` dependency),
   multi-object links resolve cross-object calls and data, and indirect calls
   dispatch through a linker-defined table. The compiler emits real
   `wasi_snapshot_preview1` imports for `-wasi` triples; the linker still
   routes imports but provides no WASI runtime of its own.
6. **Source-level ABI surface** — `extern`, variadic functions, aggregate
   parameters, atomics, and TLS syntax, so the Phase 2 MachineIR/FFI features
   become reachable from Adesh source.
7. **Shared-library execution** — PE DLL load/export call, rebasing and
   initialization pass locally (2026-11-12); the named Windows CI step awaits
   execution. Mach-O dylib loading remains unverified. ELF is already verified
   through `dlopen`/`dlsym`.
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
  execution were unverified at that audit. PE DLL execution is now locally
  verified (2026-11-12, §0.14); Mach-O dylib execution remains unverified.
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
  atomics, vectors), and since 2026-11-07 a hello-world encodes completely and
  stops at the missing runtime archive, so "unreachable from source" is no longer
  accurate either; treat it as "implemented backend, archive-blocked".
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
