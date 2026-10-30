# Adesh Native Toolchain — Next Implementation Plan (Phases 11–21)

**Created:** 2026-10-24
**Baseline audit:** `CURRENT_STATE.md` §0.5 and `TARGET_MATRIX.md`
**Scope:** finish the self-contained native toolchain so every advertised target
is either execution-verified in CI or explicitly labelled unsupported.
**Relationship to earlier plans:** this supersedes the remaining open items in
`IMPLEMENTATION_PLAN.md` (Phases 0–7) and `STABILIZATION_PLAN.md` for the native
codegen/linker track. It does not replace the language-stability work; Phase 18
below depends on it.

---

## 1. Rules for every phase

These are non-negotiable and apply to each phase's exit criteria.

1. **No capability without execution.** A backend or linker feature is done
   only when a checked-in test builds an artifact, runs it on the matching
   host (or under qemu for cross targets), and asserts output or exit status.
   Byte-emission tests are labelled "emission-tested", never "supported".
2. **No silent fallbacks.** Unsupported instructions, relocations, or formats
   must return a structured error naming the construct and function. A `NOP` or
   zero displacement in place of unsupported behavior is a release blocker.
3. **One phase, one documentation update.** Every phase updates
   `CURRENT_STATE.md`, `TARGET_MATRIX.md`, and any affected companion doc in
   the same change set. Contradictions between docs are treated as bugs.
4. **CI is the arbiter.** A test that only runs on a developer machine does not
   count. Each phase names the CI job it lands in.
5. **No cross-architecture fallbacks.** A missing target runtime archive or
   sysroot is a loud, actionable error with the exact build command; never fall
   back to a host-arch artifact.

**Definition of Done for a phase**

- [ ] Work items complete.
- [ ] New/updated tests pass locally and in the named CI job.
- [ ] Old behavior demonstrably fails the new tests (regression proven).
- [ ] `CURRENT_STATE.md` §0 and `TARGET_MATRIX.md` rows updated.
- [ ] No new `TODO`/silent fallback introduced (grep gate).
- [ ] Phase exit checklist complete.

**Effort scale:** S = days, M = 1–2 weeks, L = 2–4 weeks, XL = 4+ weeks.

---

## 2. Current verified baseline (what the plan builds on)

| Area | Verified today | Not verified today |
| :--- | :--- | :--- |
| x86-64 Windows | PE executables, imports, TLS, atomics, SEH emission | PE DLL loading; source-level ABI surface; vector ABI |
| x86-64 Linux (gnu) | static/dynamic ELF, `dlopen` `.so`, TLS LE/IE, `.eh_frame` decode | runtime unwinding; cross-target archives |
| x86-64 Linux (musl) | TLS local-exec/initial-exec in WSL | dedicated musl CI job |
| AArch64 Linux | codegen and structural ELF emission | execution; correct relocation offsets; ADRP/ADD_LO12; target runtime archive |
| AArch64 / x86-64 macOS | Mach-O emission, dyld bind/rebase, TLV emission, codesign structure | any dyld execution; TLV at runtime; dylib loading |
| RISC-V 64 | codegen and structural ELF; loud errors | source-level lowering (`ParallelMove`); execution |
| RISC-V 32 | target declared | codegen refuses RV32; linker uses RV64 `_start` |
| WASM | compiler emits `wasi_snapshot_preview1.fd_write` for `-wasi` triples (asserted under wasmtime) and `env.*` + a Node loader otherwise; linker applies relocations, resolves cross-object and indirect calls through a table it defines, and CI executes its linked clang objects on two engines (Node + wasmtime) | linker-side WASI runtime; `proc_exit`/`args_get`/`environ_get`/`clock_time_get`/`random_get` |
| Embedded ARM | Thumb-2 ALU subset | end-to-end firmware link/execution |
| GPU / quantum | containers and simulator | ISA generation, launch runtime, language construct |

---

## 3. Phase map and dependencies

```
Phase 11  Safety & truth gate              (blocks everything)
   │
   ├── Phase 12  Target runtime archives ──► Phase 13  Execution matrix
   │                                              │
   │                                              ├── Phase 14  AArch64 completion
   │                                              ├── Phase 15  RISC-V + RV32 decision
   │                                              └── Phase 16  Mach-O hardening
   │
   ├── Phase 17  WASM/WASI unification
   │
   └── Phase 18  Source-level ABI surface ──► Phase 19  Unwinding/memory/stdlib
                                                   │
                                                   └── Phase 20  Async runtime
Phase 21  Embedded/GPU/quantum disposition + release conformance gate (last)
```

Phases 14, 15, 16, and 17 can proceed in parallel once Phase 13 has one working
foreign-execution harness. Phase 18 depends on Phase 13 so FFI can be asserted
on at least two architectures.

---

## Phase 11 — Safety and truth gate

**Effort:** S · **Depends on:** none · **Blocks:** all

### Goal
Eliminate silent miscompiles and make every documented target tier match
verified reality before adding features.

### Work items

1. **AArch64: loud unsupported instructions.**
   In `crates/adesh-codegen/src/targets/aarch64/mod.rs` the encoder's `_ =>`
   arm (line ~850) emits `NOP`. Change the lowering/encoding path to return a
   structured `CodegenError` naming the `MachineInstruction` variant and the
   containing function. Enumerate every currently-unhandled variant so the
   error is actionable, not generic.
2. **AArch64: real relocation offsets.**
   `emit_object` (line ~919) assigns every call in a function the offset
   `function_index * 8`. Record the actual offset when the `BL` is emitted
   (a per-function `(text_offset, symbol)` list) and use it both for the
   relocation and for any addend. Add a multi-call, multi-function object test
   that decodes the ADOB and asserts each relocation offset points at its `BL`.
3. **AArch64: fail before emitting a wrong object.**
   If ADRP/ADD_LO12 symbol materialization is not implemented yet (Phase 14),
   return an error from symbol-address lowering instead of silently emitting a
   placeholder. This is the prerequisite for trusting any AArch64 output.
4. **RISC-V: move the RV32 refusal to target validation.**
   Reject `riscv32*` at driver/target-spec construction with a clear message
   instead of reaching codegen and failing on `ParallelMove` or emitting RV64
   prologues. Keep the loud `unsupported instruction` behavior for RV64.
5. **JIT test honesty.**
   Fix the three Windows failures in
   `tests/native_phase8_jit_native_conformance_test.rs` by escaping the
   absolute import path before writing generated Adesh source (the helper is
   used at lines ~386, ~426, ~463). Add the suite to the Windows CI step.
6. **Tier realignment.**
   In `linker/src/target.rs`, derive tier labels from verification status:
   execution-verified targets are Tier 1; emission-tested targets are Tier 2;
   unimplemented targets are removed or rejected. Update
   `TOOLCHAIN_CAPABILITIES.md`, `ARCHITECTURE_GAPS.md`, and `ABI_MATRIX.md`
   rows that still say AArch64/RISC-V/Mach-O are "PoC (4 ops)" or "Not
   functional".

### Tests and CI
- New `tests/native_phase11_codegen_loudness_test.rs`:
  - AArch64 unsupported `MachineInst` returns `Err`.
  - AArch64 multi-call object relocation offsets are distinct and match the
    emitted `BL` sites.
  - No `0xD503201F` appears in emitted text except explicit padding.
  - RV32 target validation fails with a clear message.
- New `linker/tests/phase11_relocation_offsets_test.rs` if the ADOB-level
  assertion fits better in the linker crate.
- Add both suites to the Ubuntu (portable) and Windows jobs in
  `.github/workflows/ci.yml`.

### Exit criteria
- [ ] No backend contains a silent instruction fallback (review gate for
      `NOP fallback` and for `unwrap_or(0)` used as an emit placeholder).
- [ ] A hello-world AArch64 `--emit=adob` contains no unintended NOPs.
- [ ] Every call relocation in a multi-call AArch64 object points at its call.
- [ ] `adesh target list` tiers match `TARGET_MATRIX.md`.
- [ ] Windows JIT conformance suite runs in CI and passes.

---

## Phase 12 — Target runtime archives and linkable cross builds

**Effort:** M · **Depends on:** Phase 11

### Goal
Make `adesh build --target <triple>` produce a linked artifact for every
advertised native target, with no host-architecture fallback.

### Work items

1. **Target-aware runtime archive resolution.**
   Rework `get_static_runtime_lib` in
   `src/backends/aot/cranelift_impl/linking.rs` to search, in order:
   `target/<triple>/<profile>/`, `target/<triple>/release/`, a bundled
   `lib/<triple>/` directory, and an explicit `ADESH_RUNTIME_DIR`. If the
   triple's arch/OS differs from the host and no archive exists, return an
   error listing every searched path and the command to build it. Remove the
   current fallback to the host archive for foreign triples.
2. **Runtime archive build tooling.**
   Add a workspace task (an `xtask` subcommand or
   `scripts/build-runtime-archives.sh`) that runs
   `cargo build -p adesh-runtime --release --target <triple>` for each supported
   triple and stages the `.a`/`.lib` under `target/<triple>/release/`.
   Integrate it into installer packaging so releases ship per-target archives.
3. **Cross-target CI.**
   On Ubuntu, `rustup target add aarch64-unknown-linux-gnu
   riscv64gc-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin` and
   build the runtime staticlibs. Cache the staged archives as CI artifacts.
   (Staticlibs do not need a final executable linker; verify each triple's
   build in CI and cache the result.)
4. **Link-only coverage.**
   New `tests/native_phase12_cross_link_test.rs`: compile a hello-world to
   each triple, link with `adeshlink`, and assert success plus the expected
   `aot_*` definitions resolve. Execution is Phase 13.

### Exit criteria
- [ ] AArch64, RISC-V 64, and both Mach-O triples link a source program.
- [ ] Missing archive errors name the triple, searched paths, and build command.
- [ ] No foreign build can silently consume a host-arch runtime archive.
- [ ] Installer output includes per-target runtime archives.

---

## Phase 13 — Execution verification matrix

**Effort:** M · **Depends on:** Phase 12

### Goal
Give every native target a CI job that runs its output and asserts behavior.

### Work items

1. **qemu user-mode harness.**
   Add a shared test helper (e.g. `tests/support/foreign_exec.rs`) that:
   locates `qemu-aarch64`/`qemu-riscv64`, runs the binary with a timeout,
   captures stdout/stderr, and returns the exit status. When the emulator is
   absent, the test prints a loud SKIP (never a silent pass). CI installs
   `qemu-user-static` so the tests actually run.
2. **AArch64 execution suite.**
   `tests/native_phase13_aarch64_exec_test.rs`: build static ELF hello-world
   and arithmetic/control-flow cases, run under `qemu-aarch64`, assert stdout
   and exit codes. Static `ET_EXEC` avoids sysroot complications.
3. **RISC-V execution suite.**
   `tests/native_phase13_riscv64_exec_test.rs`: same shape under
   `qemu-riscv64`.
4. **macOS link-and-run.**
   `tests/native_phase13_macho_exec_test.rs` (macOS only): `adesh build`,
   execute, assert stdout/exit; include a libSystem import (`puts`), a TLS
   access, and `--shared` dylib loaded with `dlopen`/`dlsym`.
5. **PE DLL loading.**
   `tests/native_phase13_pe_dll_test.rs` (Windows only): build `--shared`,
   `LoadLibraryW` + `GetProcAddress` the export, call it, assert the result.
6. **Dedicated musl job.**
   Add a job or container step using musl tooling so
   `tests/native_phase8_tls_ehframe_test.rs` runs against musl explicitly
   instead of only through the host glibc loader.

### Exit criteria
- [ ] AArch64 and RISC-V binaries execute under qemu in CI with asserted output.
- [ ] macOS executes both Mach-O architectures (at least the runner's native one).
- [ ] PE DLL load + export call executes.
- [ ] musl job runs the TLS suite against musl.
- [ ] Matrix rows flip to execution-verified only for suites that actually ran.

---

## Phase 14 — AArch64 backend completion

**Effort:** L · **Depends on:** Phase 13

### Goal
Reach x86-64 feature parity for the supported language subset on AArch64.

### Work items

1. **Symbol address materialization:** ADRP + ADD_LO12 (and GOT-based variants
   for dynamic links), emitted from the codegen rather than left to the linker.
2. **TLS sequences:** local-exec and initial-exec for ELF AArch64, including
   `TPIDR_EL0` addressing and linker relaxation.
3. **Lowering gaps:** compile the existing conformance corpus for AArch64 and
   fix each loud error in turn (parallel moves, stack arguments, aggregates,
   variadics, conversions, vector operations as scoped).
4. **ABI correctness:** AAPCS64 stack alignment, callee-saved FP/SIMD, HFA
   handling where the language exposes aggregates, variadic register-save
   area.
5. **ELF dynamic linking:** PLT/GOT import sequences and COPY relocations for
   data imports, mirroring the x86-64 path.
6. **Differential testing:** run the corpus on x86-64 and AArch64 and compare
   stdout/exit codes; add mismatches to the corpus as regression cases.

### Exit criteria
- [ ] The conformance corpus passes on AArch64 under qemu.
- [ ] No corpus case hits an unsupported-instruction error.
- [ ] Differential output parity with x86-64 for the corpus.
- [ ] AArch64 row in `TARGET_MATRIX.md` = execution-verified.

---

## Phase 15 — RISC-V completion and RV32 disposition

**Effort:** L · **Depends on:** Phase 13

### Goal
Make RV64GC usable from source; resolve the advertised-but-unimplemented RV32
target one way or the other.

### Work items

1. **`ParallelMove`:** implement register/stack parallel moves (cycles,
   register-to-stack, stack-to-register) in
   `crates/adesh-codegen/src/targets/riscv/mod.rs`, reusing the shared
   parallel-move resolver used by x86-64 where possible.
2. **Remaining lowering:** stack arguments, calls, prologue/epilogue, scalar
   FP ABI (LP64D), atomics and vector encodings reachable from the corpus.
3. **Symbol materialization:** PCREL_HI20/PCREL_LO12 pairs for addresses, GOT
   and PLT sequences for dynamic links, and linker relaxation correctness.
4. **Execution:** qemu-riscv64 corpus run with differential parity against
   x86-64.
5. **RV32 decision (explicit):**
   - Option A: implement ILP32 lowering, RV32 `_start`/entry, and RV32 linker
     relocation widths; add qemu-riscv32 execution.
   - Option B: remove `riscv32imac-unknown-none-elf` from `target list`, reject
     it at validation, and document the removal in `TARGET_MATRIX.md` and
     `TOOLCHAIN_CAPABILITIES.md`.
   Do not leave the current state (accepted target, RV64 stub, wrong code).

### Exit criteria
- [ ] Real Adesh source programs build, link, and run under qemu-riscv64.
- [ ] RV32 either executes under qemu-riscv32 or is rejected everywhere.
- [ ] RISC-V row in `TARGET_MATRIX.md` reflects verified behavior.

---

## Phase 16 — Mach-O production hardening

**Effort:** M · **Depends on:** Phase 13

### Goal
Move Mach-O from "dyld structure looks right" to "dyld runs it reliably".

### Work items

1. **Runtime bring-up:** fix whatever Phase 13 exposes under real dyld,
   including `LC_BUILD_VERSION`/platform min-version, entry convention,
   section protection (`__TEXT`, `__DATA_CONST`), and alignment.
2. **Code signature:** verify the ad-hoc signature with `codesign -v` in CI;
   cover both architectures. Keep chained fixups unimplemented unless the macOS
   runner's dyld rejects classic bind opcodes; if it does, implement the
   minimal chained-fixup format and document the requirement.
3. **TLV execution:** run a thread-local counter through `__thread_vars`,
   `__tlv_bootstrap`, and `__thread_data`/`__thread_bss`; assert the value.
4. **Data imports:** imported data symbols (e.g. `environ`) with correct bind
   semantics and rebasing.
5. **Dylib behavior:** `install_name`, `LC_ID_DYLIB`, `dlopen` from a PIE
   caller, export trie correctness for exported functions.

### Exit criteria
- [ ] Executables run on both macOS architectures available in CI.
- [ ] `codesign -v` passes on produced artifacts.
- [ ] TLV access executes correctly.
- [ ] `dlopen` of a produced dylib resolves and calls an export.
- [ ] Mach-O rows in `TARGET_MATRIX.md` = execution-verified.

---

## Phase 17 — WASM/WASI unification

**Effort:** M–L · **Depends on:** Phase 11

### Goal
One working WASM path, with an ABI that matches the target name.

### Work items

1. **Decision record:** either
   - **A (recommended short term):** make the compiler backend the canonical
     `adesh build --target wasm32-*` path and make `adeshlink --wasm` a loud
     "not implemented" error; or
   - **B (in progress):** implement a real `linker/src/wasm/writer.rs` (import
     section, function/type tables, memory layout, data segments with applied
     relocations, exports, entry as an export) and route builds through it.
     *Done so far:* object reader (relocations, `linking` symbols, link facts,
     export section), `WasmLinkInfo` (type/import/function/data/export
     numbering, memory and stack placement), the module writer, and
     `linker/src/wasm/relocations.rs`, which patches function/global/data-address
     fixups in place. Two real clang hello-world objects link and are executed
     on two engines (V8 through Node, Cranelift through the workspace's
     `wasmtime` dependency) with asserted stdout, in CI. Cross-object
     references now resolve too: `WasmDefinitions` collects the functions and
     data every input defines, so a reference to a name another input defines
     becomes a call or a segment address instead of a host import — a two-object
     link can import nothing at all. Duplicate strong definitions are `LNK002`,
     a call-site signature that disagrees with the definition is `LNK007`, and an
     address of data no input defines is `LNK001`. Function pointers now link
     too: the plan defines one `funcref` table, re-bases every input's element
     entries into it (slot 0 reserved for a null trap), and binds
     table-index/table-number/type-index relocations, so `call_indirect` runs
     hostlessly in V8 and Cranelift — for an indirect call inside one object,
     function pointers stored in data, and a cross-object callee.
     *Remaining:* defined tables/globals and non-MVP element segments
     (`LNK017`); the link emits a single table.
2. **WASI preview1:** *Done for `fd_write`.* `-wasi` targets emit exactly one
   import, `wasi_snapshot_preview1.fd_write`, and carry their own write/alloc/
   print adapters (`src/backends/wasm/wasi.rs`), a `__heap` global and
   on-demand `memory.grow`; `print_*` go through `fd_write`, and
   `print(..., { file: ... })` is a loud compile error because Preview 1's
   `fd_write` cannot open files. `tests/wasm_wasi_compiler_test.rs` builds
   through the real CLI and executes under wasmtime with asserted stdout;
   `env.*` imports remain only for explicit non-WASI wasm targets.
   *Remaining:* `proc_exit`, `args_get`, `environ_get`, `clock_time_get`, and
   `random_get` (no language construct emits them yet).
3. **Execution tests:** both engines run in the ubuntu CI job —
   `linker/tests/wasm_hello_world_test.rs` under Node and
   `tests/wasm_wasmtime_execution_test.rs` under wasmtime, sharing the same
   clang fixtures. The wasmtime suite drives the workspace's `wasmtime` 36
   library and provides the `wasi_snapshot_preview1.fd_write` host in-test,
   because wasmtime 33+ dropped WASI preview1 and no runner-installed CLI can
   execute these modules. *Remaining:* assert an exit code (the host currently
   has no `proc_exit`). Cross-object programs and table programs are covered — a
   two-object module that imports nothing, an indirect call, and function
   pointers stored in data all run on both engines.
4. **Target naming:** *Done.* `adesh build --target wasm32-*-wasi` emits WASI
   imports; `--emit=wasm` and `wasm32-unknown-unknown` keep `env.*` plus the
   generated Node loader. `tests/wasm_wasi_compiler_test.rs` asserts both
   import surfaces.

### Exit criteria
- [x] `adesh build --target wasm32-unknown-wasi` produces a module that runs
      in wasmtime with real WASI imports and correct output
      (`tests/wasm_wasi_compiler_test.rs`: one import, `fd_write`, asserted
      stdout including floats and a heap-allocating struct access).
- [x] No executable build silently routes through the linker's WASM path while
      it lacks table/indirect-call support (the linker now links tables and
      indirect calls; the remaining WASI runtime gap is item 2's remainder).
- [x] WASM row in `TARGET_MATRIX.md` = execution-verified under Node and
      wasmtime (done — both engines, in the ubuntu CI job).

---

## Phase 18 — Source-level ABI surface

**Effort:** L · **Depends on:** Phase 13

### Goal
Expose the already-tested codegen/FFI capabilities to Adesh source, so they are
not MachineIR-only laboratory features.

### Work items

1. **Language design review first:** agree syntax and semantics for:
   `extern "C"` declarations, symbol/link names, variadic parameters,
   aggregate value parameters/returns, atomic operations, and thread-local
   variables.
2. **Front end:** parser, AST, HIR, and lowering support for the agreed
   syntax; structured errors for unsupported combinations.
3. **Codegen wiring:** extern calls become imports with the correct relocation
   kind; aggregates use the shared SysV/Win64/AAPCS64 classifier; variadic
   calls set up register-save areas and `va_list`; atomics lower to the
   existing locked instruction forms; TLS lowers to the platform access model.
4. **Tests:** a source-level C-interop suite that calls `puts`/`printf` and a
   small extern data symbol on Windows, Linux, and AArch64 (qemu); aggregate
   and variadic tests at the language level.
5. **Update `ABI_MATRIX.md`** to distinguish "codegen-level" from
   "language-reachable".

### Exit criteria
- [ ] A `.adesh` source file declares and calls an extern C function on all
      execution-verified targets.
- [ ] Variadic, aggregate, atomic, and TLS source tests execute and assert
      results.
- [ ] ABI docs mark each feature as reachable or not.

---

## Phase 19 — Unwinding, memory lifetime, and native stdlib

**Effort:** L · **Depends on:** Phase 18 (partly independent)

### Goal
Make native programs safe to unwind through, leak-free for composite values,
and able to build the standard library without interpreter-only dependencies.

### Work items

1. **Runtime unwinding:**
   - ELF: register `.eh_frame`/`.eh_frame_hdr` and unwind a real throw/catch or
     panic through multiple frames using the host unwinder.
   - Windows: verify SEH `.pdata`/`.xdata` by triggering and handling an
     exception.
   - Connect `defer`/cleanup blocks to unwind landing pads.
2. **Memory lifetime:** emit tracked-scope allocation/free from native lowering
   (today `Alloc`/`Free` still call `aot_alloc`/`aot_free`); add loop and
   closure leak tests that assert the handle table returns to baseline.
3. **Native standard library:** remove Tokio/interpreter builtins from native
   paths for the supported stdlib subset; precompile it into a per-target
   `libadesh_std.adob` archive; link it by default with an opt-out flag.
4. **Tests:** unwind suite, leak suite, and a stdlib program built and run
   natively on Windows/Linux (qemu for AArch64 once available).

### Exit criteria
- [ ] A thrown error/panic unwinds through at least three frames on ELF and
      Windows with cleanup executed.
- [ ] No composite-value leak in loops (baseline handle count restored).
- [ ] A representative stdlib program builds natively with no interpreter-only
      symbol.

---

## Phase 20 — Async runtime and concurrency

**Effort:** XL · **Depends on:** Phase 19

### Goal
Provide a real native async execution model instead of the Tokio dependency in
the standard library.

### Work items

1. **Runtime:** executor, waker/vtable, timer reactor, and I/O readiness
   integration in `crates/adesh-runtime`; integrate with the existing worker
   pool.
2. **Lowering:** coroutine/state-machine lowering for `async fn` and `await`,
   or a decision to keep them interpreter-only with a loud build error.
3. **Concurrency safety:** atomics/TLS from Phase 18 used by the runtime;
   document the memory model.
4. **Tests:** async timer, async I/O, spawn/join, and cancellation tests on
   Windows and Linux; qemu coverage where scheduling is deterministic.

### Exit criteria
- [ ] Async programs execute natively with no Tokio dependency.
- [ ] Spawn/join and cancellation tests pass under CI.
- [ ] Documented memory model in `ABI_MATRIX.md` or a runtime design doc.

---

## Phase 21 — Embedded/GPU/quantum disposition and release gate

**Effort:** M · **Depends on:** Phases 14–20

### Goal
Stop advertising unfinished targets and lock the release gate.

### Work items

1. **Embedded:** either complete an end-to-end firmware link (linker script,
   IVT, crt0, flash/HEX/SREC output) with qemu/hardware execution for
   `thumbv7em-none-eabihf`, or mark it unsupported and remove it from the
   advertised list.
2. **GPU/quantum:** explicitly park them. Document the contract: kernels require
   external ISA; no launch runtime; no language-level `qubit`. Remove any
   wording that implies otherwise.
3. **JIT conformance:** a differential harness that runs a corpus on the
   interpreter and `--njit`, compares output/exit codes, and fails on
   divergence. Include it in Windows CI.
4. **Final conformance gate:** run the full corpus across every
   execution-verified target (Windows PE, Linux ELF gnu/musl, AArch64 qemu,
   RISC-V qemu, macOS Mach-O, WASM/wasmtime) and record results.
5. **Documentation sweep:** `CURRENT_STATE.md`, `TARGET_MATRIX.md`,
   `TOOLCHAIN_CAPABILITIES.md`, `ARCHITECTURE_GAPS.md`, `ABI_MATRIX.md`,
   `README.md`, and the docs site must all agree with the gate results.

### Release gate checklist
- [ ] `cargo test --workspace` green on Windows, Linux, and macOS.
- [ ] qemu and wasmtime jobs green on Linux.
- [ ] Every advertised target is either execution-verified or explicitly
      labelled experimental/unsupported in `adesh target list` and all docs.
- [ ] No silent-fallback constructs remain (grep gate).
- [ ] No capability claim lacking an execution test.
- [ ] Installer ships per-target runtime archives and passes a smoke build.

---

## 4. Suggested execution order (one screen)

| Order | Phase | Why now |
| :--- | :--- | :--- |
| 1 | 11 Safety gate | Stops wrong-code generation; cheap and unblocks trust |
| 2 | 12 Runtime archives | Without them no cross-target build can link |
| 3 | 13 Execution matrix | Converts all later work from emission to verified behavior |
| 4 | 14 AArch64 completion | Largest supported non-x86 target; qemu available |
| 5 | 15 RISC-V + RV32 | Resolves an advertised-but-broken target |
| 6 | 16 Mach-O hardening | Native macOS CI already exists |
| 7 | 17 WASM/WASI | Removes a stub path and a mislabeled ABI |
| 8 | 18 Source ABI surface | Makes Phase 2 codegen features useful |
| 9 | 19 Unwinding/memory/stdlib | Required for production programs |
| 10 | 20 Async runtime | Largest remaining runtime gap |
| 11 | 21 Disposition + gate | Ends with honest, CI-backed claims |

---

## 5. Risks and mitigations

| Risk | Impact | Mitigation |
| :--- | :--- | :--- |
| qemu on CI is flaky or unavailable | Blocks cross-target verification | Pin a known qemu version, run static binaries, use timeouts, keep a loud SKIP path for local runs |
| Cross runtime staticlib build fails without cross toolchain | Blocks Phase 12 | Staticlibs need no linker; validate per-triple early and cache in CI |
| macOS runner architecture differs from target | Misses one Mach-O path | Test the native runner arch for execution and the other via emission + structural checks, documenting the gap |
| Unwinding requires host libgcc/libunwind ABI details | Phase 19 slips | Start with a single-frame throw, expand to multi-frame; keep `.eh_frame` decoder tests as the invariant |
| Source ABI design becomes a language debate | Phase 18 slips | Timebox a design review; ship `extern "C"` + variadics first, aggregates second |
| RV32/RV64 stub is forgotten | Silent wrong output | Phase 11 moves the rejection to target validation; Phase 15 makes the final decision |
| WASI Preview 1 runtime diverges from the compiler's ABI | Phase 17 slips | The compiler's `-wasi` path is pinned by an import-surface + stdout test; keep the linker path opt-in (`adeshlink --target wasm32-*`) until it supplies its own WASI host |
| Documentation drifts again | Loss of credibility | Phase DoD includes doc updates; Phase 21 adds a doc-consistency check |

---

## 6. Explicitly parked (not part of this plan)

These are acknowledged and must stay clearly labelled as unsupported:

- GPU/NPU/TPU ISA generation and kernel launch runtime (container packaging
  only). Requires external `mlir-opt`/`llc` today.
- Quantum hardware backends and a language-level `qubit` construct; the
  simulator and QASM 3.0 export remain library-level.
- Thin archives, delay imports, Authenticode, and resources.
- Full vector ABI classification beyond what the language exposes.
- Multi-module whole-program LTO (the CLI feeds one module today; the engine
  exists but has no multi-module driver).

---

## 7. Immediate next actions (Phase 11 checklist)

1. Replace the AArch64 `NOP` fallback with a structured error and list every
   affected instruction variant.
2. Fix AArch64 relocation offsets by recording emitted `BL` sites.
3. Reject RV32 at target validation.
4. Fix the Windows JIT import-path escaping and add the suite to Windows CI.
5. Realign `linker/src/target.rs` tiers and the three stale companion docs.
6. Land `tests/native_phase11_codegen_loudness_test.rs` and wire it into CI.
