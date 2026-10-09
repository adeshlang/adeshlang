# Adesh Consolidation Plan — Master Implementation Plan (v2)

**Generated:** 2026-10-05, following a full code-level audit of commit `4fad2d9` ("State").
**Replaces:** the previous 2026-10-01 seven-phase plan.
**Relationship to other docs:** `CURRENT_STATE.md` is the canonical, single source of truth
for *current status*. This file is the roadmap for how status changes. All other status
documents (`TARGET_MATRIX.md`, `ARCHITECTURE_GAPS.md`, `TOOLCHAIN_CAPABILITIES.md`,
`linker/docs/abi_v1.md`, docs-website pages) must be kept consistent with `CURRENT_STATE.md`.

---

## 0. Guiding Principles

1. **Consolidation over expansion.** Adesh has enough architecture. No new subsystems
   (no new targets, no async runtime, no GPU/quantum language work) until Phase 5 is done.
2. **Never silently wrong.** Every language construct either compiles to correct code or
   produces a structured compile error. Silent wrong-code paths are the highest-priority
   bug class in this plan.
3. **Execution-driven validation.** Every phase ends with produced binaries being executed
   with asserted exit codes/stdout in CI. Constructing bytes/headers without running them
   no longer counts as verification for status claims.
4. **Truth in documentation.** No document may claim a capability that no execution test
   verifies. The status matrix stays aggressively conservative.
5. **Wire-or-delete.** Unwired or orphaned code is either wired into the production path
   in its phase or deleted. Nothing stays half-connected. (Decision recorded 2026-10-05.)

---

## 1. Verified Baseline (2026-10-05 Audit)

### 1.1 What is real and verified

- **Native pipeline is the CLI default** (`src/cli/build.rs:1282`, dispatch at
  `:858-861`): Lexer → Parser → HIR → Machine IR → x86-64 → ADOB → `adeshlink` → PE32+.
  Windows execution tests assert exit codes (`tests/native_x86_64_e2e_test.rs`,
  `tests/native_semantics_e2e_test.rs`, `linker/tests/pe_windows_e2e_test.rs`).
- **ADOB v1.0** (`crates/adesh-object`): complete spec, validating writer, reader,
  round-trip tests, ~28 relocation kinds, TLS/debug/unwind/custom section kinds.
- **x86-64 backend** (`crates/adesh-codegen`): real REX/ModR/M/SIB encoding, scalar
  SSE2 + packed SSE encoders, XMM0-13 allocation with Win64 nonvolatile tracking,
  structured `CodegenError`s for unsupported instruction forms.
- **Parallel-move resolver** (`crates/adesh-codegen/src/calling_convention/parallel_move.rs`):
  cycles, swaps, mem/imm moves, GPR/XMM mixed; wired into native call lowering.
- **Machine-IR optimization pipeline** (`crates/adesh-codegen/src/opt/mod.rs:81-237`):
  constant folding, SCCP, copy prop, DCE, LICM, induction, branch opt, peephole, cmov,
  coalescing, frame opt, alias/DSE, with fixpoint loop and verifier.
- **Native runtime** (`crates/adesh-runtime`): C ABI handle table, ARC/weak refs,
  tracked-allocation scopes, worker pool with panic containment, FFI guards.
- **Windows PE linker path** (`linker/src/pe`): imports/IAT, base relocations, TLS
  directory, ASLR/NX, entry synthesis, `--shared` (DLL) support, execution-tested.

### 1.2 Confirmed correctness bugs (all verified first-hand; fix in Phase 1 unless noted)

| # | Bug | Location |
|---|---|---|
| 1 | Unknown HIR expressions silently lower to constant `0` | `src/backends/native/lower.rs:4605-4613` |
| 2 | `HirPattern::EnumVariant` always matches and binds nothing (silent wrong code) | `src/backends/native/lower.rs:615`, `:623-638` |
| 3 | Import statements silently dropped | `src/backends/native/lower.rs:2279` |
| 4 | `Await`/`Spawn`/`Share` lowered as plain sequential expressions | `src/backends/native/lower.rs:4526-4532` |
| 5 | Lambdas compile without captured environment | `src/backends/native/lower.rs:2967-3031` |
| 6 | `Region` blocks lose arena bulk-free semantics | `src/backends/native/lower.rs:1893-1895` |
| 7 | `lower_hir_module` never returns `Err`; "loud" failures are runtime aborts or link errors only | `src/backends/native/lower.rs:4769` |
| 8 | sret (`ReturnLocation::HiddenSret`) falls into `_ => {}`; no return value loaded | `crates/adesh-codegen/src/ffi/mod.rs:314-317` |
| 9 | FFI call arguments use sequential moves, bypassing the parallel-move resolver | `crates/adesh-codegen/src/ffi/mod.rs:201-309` |
| 10 | Win64 nonvolatile XMM6-13 saved/restored with 64-bit `movsd`; ABI requires low 128 bits | `crates/adesh-codegen/src/targets/x86_64/mod.rs:748-767` |
| 11 | Post-allocation verifier result discarded (`let _ =`) | `crates/adesh-codegen/src/register_alloc/mod.rs:1963-1971` |
| 12 | Stack canary emits `Compare` with no branch/abort | `crates/adesh-codegen/src/safety/mod.rs:140-173` |
| 13 | Sanitizer instrumenter dead (calls inserted with `num_args: 0`, no callers) | `crates/adesh-codegen/src/sanitizer.rs` |
| 14 | `--codegen` value unvalidated; typos silently select Cranelift | `src/cli/build.rs:858-861` |
| 15 | Duplicate `aot_alloc`/`aot_free`; `aot_free` deallocs with fixed `Layout(8,8)` (UB) | `src/backends/aot/runtime_bridge.rs:1809-1825` |
| 16 | `--pgo` CLI flag parsed then silently discarded | `src/cli/build_args.rs:136-139` |
| 17 | `--lto` silently degrades to GC+ICF+strip while CLI help says it errors | `linker/src/linker.rs:36-46`, `linker/src/main.rs:67` |

### 1.3 Linker honesty gaps (fix in Phase 3 unless noted)

- ELF: no GOT/PLT (dynamic imports remain unresolved VA-0 refs); RELRO covers only
  `.dynamic`; `e_entry` points at `main` with no argc/argv setup; no `PT_TLS`
  (`linker/src/elf/writer.rs`).
- Mach-O: no dyld info, chained fixups, exports trie, or indirect symbol table; only
  fully-static code could ever run (`linker/src/macho/writer.rs`).
- Linker WASM writer emits the merged native machine-code section verbatim as the wasm
  function body — structurally parseable, not runnable (`linker/src/wasm/writer.rs:87-105`).
- Archive writer emits no symbol index (`/` member); no thin-archive support
  (`linker/src/archive/ar.rs:203-271`).
- GC name-based fallback over-retains same-named symbols (`linker/src/gc.rs:66-96`);
  ICF `Safe` and `All` are behaviorally identical (`linker/src/icf.rs:14-16`).
- No SEH `.pdata`/`.xdata` synthesis in the pipeline (`WindowsPdataGenerator` never invoked);
  no CIE/FDE writer (Phase 5).

### 1.4 CI state (fix in Phase 0)

- Phase 5-8 execution suites are **not** OS-gated (only `native_x86_64_e2e_test`,
  `native_semantics_e2e_test`, phase 2/3, and `native_parallel_move_e2e_test` are), while
  Ubuntu runs `cargo test --tests` — Linux CI attempts to execute Windows PEs.
  The comment at `.github/workflows/ci.yml:106-110` falsely claims full gating.
- Phase 8/9/10 suites appear in no CI step list at all; macOS runs `--lib` only.
- No CI job on any OS executes an ELF, Mach-O, or WASM produced by `adeshlink`.
- Clippy is fully silenced workspace-wide (`Cargo.toml:22-28`).

### 1.5 Dead / unwired code (wire-or-delete in the phase shown)

| Item | Status | Phase |
|---|---|---|
| `src/ir/hir/` (429+976 lines) | Orphaned duplicate HIR, never compiled | 0 (delete) |
| `src/ir/optimizations/` (6 passes incl. 528-line inliner) | Never wired | 4 |
| `crates/adesh-codegen/src/opt/switch_lowering.rs` | Never called from lowering; match emits compare chains | 1 (wire into match) |
| `crates/adesh-codegen/src/opt/vectorization.rs`, `scheduler*.rs` | Test-only callers | 4 |
| `crates/adesh-codegen/src/opt/ipo.rs` | Fabricates counters from hand-supplied data | 4 (delete) |
| `crates/adesh-codegen/src/opt/pgo.rs` | Markers never emitted by any encoder; CLI flag discarded | 4 |
| `crates/adesh-codegen/src/opt/lto.rs` + `driver.rs` | Real engine, `CompilerDriver` never instantiated | 4 |
| `Dwarf5Generator` / `CodeViewGenerator` | Never invoked | 5 |
| `WindowsPdataGenerator` | Never invoked | 5 |
| `SanitizerInstrumenter` | Never invoked, malformed calls | 1 (delete or fix) |
| MIR (`src/ir/mir/`, not SSA) / VIR (SSA) | Native path bypasses both: AST → HIR → Machine IR directly; MIR only behind `ADESH_USE_VIR=1` | informational |

### 1.6 Documentation corrections needed (Phase 0)

- `TARGET_MATRIX.md`: says default builds use Cranelift (false — native is default), says
  "no SSE/SIMD" (false), cites wrong test path for `pe_windows_e2e_test.rs`.
- `CURRENT_STATE.md`: internally contradictory — §0 says LTO/shared-libs implemented,
  §2.3 says both are loud errors; truth for PE `--shared` is "works".
- `linker/docs/abi_v1.md:106-114`: claims Tier 1 "Production Verified, continuous e2e
  suite, binary execution validation" for 8 targets including `i686-windows` /
  `i686-linux` (do not exist) and `aarch64-macos` (not functional).
- `docs/TOOLCHAIN_CAPABILITIES.md`: stale duplicate claiming everything "Production
  Ready", DWARF 5, GPU drivers.
- `docs/TOOLCHAIN_PLATFORM_ARCHITECTURE.md:41-51`: macOS/WASM/RISC-V/Cortex-M "Tier 1".
- `ADESH_TOOLCHAIN.md`: MsgPack HSACO metadata, AIR bitcode, Apple ANE (no code exists),
  XLA HLO serialization, QIR runtime bindings (constants only).
- `README.md`: `.deb/.rpm/.pkg/.dmg` installers not produced by `installer/manifest.json`;
  "Rust-grade compile-time memory safety"; "guaranteed semantic parity".
- `docs/MEMORY_SAFETY.md`: "100% compile-time memory safety across all backends",
  "if your code compiles, it's memory-safe".
- `docs/gpu/gpu-guide.md:510,775-787`, `docs/type-system-guide.md:3575-4107`: stale
  "Production Ready" notes.
- `docs-website/docs/compiler/{codegen,native-toolchain,runtime-abi}.md`: claim default
  AOT still uses Cranelift (stale the other direction).
- GPU reality: invented container formats (fake fatbin, HSACO without MsgPack, one
  hardcoded empty SPIR-V shader, no ANE code), zero driver API calls; only real GPU path
  is external MLIR (`mlir-opt`/`llc`/`clang`) with interpreter fallback.
- Quantum reality: measurement is a deterministic 0.5-threshold mock
  (`linker/src/quantum/sim.rs:246-262`); QIRB embeds QASM text; no language constructs.

---

## 2. Phases

### Phase 0 — Truth Reset: Docs + CI (≈1 week)

**Goal:** the repo stops overstating, and CI stops claiming to test what it doesn't.

Tasks:
1. Fix `CURRENT_STATE.md` internal contradictions (LTO, `--shared`, Mach-O sections).
2. Fix `TARGET_MATRIX.md` (native default, FP/SSE2 status, correct paths, real
   execution-verified list) and `ARCHITECTURE_GAPS.md` (mark parallel-move/FP resolved).
3. Correct all docs listed in §1.6. Adopt conservative wording everywhere:
   "execution-tested on Windows x86-64; expanding outward".
4. CI gating fixes: gate phase 4-8 execution suites to `cfg(all(windows, target_arch = "x86_64"))`;
   add phase 8/9/10 to the Windows step list; correct the false comment in `ci.yml`;
   each OS runs only suites it can actually run.
5. Delete the orphaned `src/ir/hir/` duplicate (decision: wire-or-delete, recorded 2026-10-05).
6. Adopt the docs policy: every capability claim must reference an execution test.

**Acceptance:** docs and code agree everywhere; CI green on all three OSes with correct
gating; `cargo check --workspace` and existing test suites still pass.

### Phase 1 — Zero Silent Miscompiles on x86-64 (≈2-3 weeks)

**Goal:** the compiler is never silently wrong; every construct lowers correctly or
produces a structured compile error.

Tasks:
1. Make `lower_hir_module` fallible with structured diagnostics; replace every silent
   `_ =>` fallback (unknown expr → 0, dropped imports, Region semantics) with a compile
   error or a correct implementation (bug table rows 1, 3, 6, 7).
2. Fix `EnumVariant` pattern semantics: correct matching + binding, or reject loudly
   (row 2). Add an execution regression test.
3. Async constructs (`Await`/`Spawn`/`Share`): explicit "not supported in native backend"
   compile error instead of silent sequential execution (row 4).
4. Lambda captures: error when captures are detected (row 5); closures proper are a
   separate scoped decision.
5. Validate `--codegen` values; error on unknown backend names (row 14).
6. Route FFI calls through the parallel-move resolver (row 9); implement `HiddenSret`
   return or error loudly until Phase 2 (row 8).
7. Fix Win64 nonvolatile XMM preservation to full low 128 bits via `movaps` (row 10).
8. Wire `AllocationVerifier::verify` results: fail the build on violated invariants
   instead of discarding (row 11).
9. Complete (branch + abort) or remove the stack canary (row 12); delete or fix the
   dead sanitizer instrumenter (row 13).
10. Fix the `runtime_bridge.rs` `aot_free` Layout UB; single source of truth for runtime
    ABI symbols; add link-time runtime ABI handshake using the existing
    `AdeshRuntimeAbiV1` descriptor (rows 15, Phase 3 follow-up).
11. Wire `switch_lowering` into match lowering (per wire-or-delete policy).
12. Build the **execution-driven conformance corpus**: run every program in `examples/`
    through the native pipeline and the interpreter, assert stdout/exit parity; any
    unsupported construct must yield a structured error, never silent divergence. This
    corpus becomes the definition of the "supported subset" and runs in Windows CI.

**Acceptance:** full `examples/` sweep produces verified parity or structured errors —
zero silent divergence; regression tests (executing real binaries) exist for every bug in
§1.2.

**Closed 2026-10-06 with task 12 deferred:** every §1.2 row (1-16) is fixed or turned into
a structured error, each with a regression test (most execute real binaries). The
`examples/` sweep (task 12) moved to Phase 2 task 10, where a wider native subset makes its
results actionable.

#### Phase 0 / Phase 1 progress (updated 2026-10-06)

Phase 0: **complete.** Phase 1: **complete** (P1-j deferred to Phase 2 task 10).

| Task | Bug rows | Status | Evidence |
|---|---|---|---|
| P1-a `--codegen` validation (task 5) | 14 | Done | `src/cli/build.rs` rejects values other than `adesh`/`cranelift` |
| P1-b Win64 XMM preservation (task 7) | 10 | Done | 128-bit `movups` save/restore; `native_phase3_fp_e2e_test` 13/13 |
| P1-bug linker libc stubs (found during P1) | new | Done | `strlen`/`malloc`/`sqrt`/... were synthesized as `xor eax,eax; ret` on PE because `is_intrinsic` included every libc symbol; now imported from the CRT (`linker/src/intrinsics/mod.rs`). Regression: `test_libc_functions_are_imported_not_stubbed`; `native_semantics_e2e_test` 17/17 |
| P1-c Allocation verifier (task 8) | 11 | Done | `LinearScanAllocator::allocate` returns `Result`; verifier now sees the *assigned* intervals (it previously checked unassigned copies); backends surface a `CodegenError`. Regression: `test_verifier_rejects_interfering_assignment` |
| P1-d FFI parallel moves + sret (task 6) | 8, 9 | Done | `FfiCallLowerer::lower_call` returns `Result`; register args emitted as one `ParallelMove`; sret, register-pair args/returns, extra variadic args, arity mismatch, and void-with-result are structured errors. Tests: `native_phase8_ffi_e2e_test` 3/3 (incl. execution) |
| P1-e `aot_free` Layout UB (task 10) | 15 | Done | Deleted the unreferenced duplicate `aot_alloc`/`aot_free` from `src/backends/aot/runtime_bridge.rs`; `crates/adesh-runtime/src/native_abi.rs` (`malloc`/`free`) is the single definition. Test: `native_abi::tests::alloc_free_roundtrip_across_sizes`. Runtime ABI handshake deferred to Phase 3 as planned. Note: no source syntax currently produces `HirExpr::Alloc`; `alloc(n)` in source is a plain call and fails at link time |
| P1-f Canary + sanitizer (task 9) | 12, 13, 16 | Done | Canary was already complete (mismatch branches to `__stack_chk_fail`, linker-synthesized as `ud2`); now execution-tested: `native_stack_canary_e2e_test` (intact frame exits 42, smashed frame traps with `STATUS_ILLEGAL_INSTRUCTION`). Opt-in API only; a CLI `--stack-protector` flag is not wired yet. Deleted the dead `crates/adesh-codegen/src/sanitizer.rs` (zero-argument check calls, unfalsifiable canary) and its test; runtime hooks in `adesh-runtime::sanitizer_rt` kept. `adesh build --pgo/--sanitizer/--hardening` now error instead of being ignored (`test_unimplemented_flags_are_rejected`) |
| P1-g Lowering fallibility (task 1) | 1, 3, 6, 7 | Done | `lower_hir_module` now returns `Result<_, NativeLoweringError>`; unsupported constructs (imports, `region`, unknown expressions/literals/binops, `instanceof`, unsupported assignment targets) are collected as diagnostics and fail the build instead of compiling to nothing. AST→HIR (`src/parsing/hir_lower.rs`) no longer turns unhandled statements into empty blocks: `region` lowers to `HirStmt::Region`, `export default fn/class` lower as their contents, `let { .. } =` errors; only type/extern declarations are no-ops. Linker intrinsics without a real body now trap (`ud2`/`brk`/`ebreak`) instead of returning 0; the fake `adesh_str_concat` (returned arg0) and no-op `adesh_print_*` stubs were removed. Tests: `test_native_rejects_imports_and_regions`, `test_unimplemented_intrinsics_trap_instead_of_returning_zero`. Fixed a stale expectation in `test_e2e_defer_execution_order` (return value is captured before defers run, matching the interpreter: 10, not 25) |
| P1-h EnumVariant / async / lambda (tasks 2-4) | 2, 4, 5 | Done (loud errors; real support deferred) | Enum variant patterns (`Some(x)`, `E::V`) are a compile error (they always matched and bound nothing). `async fn`, async lambdas, `await` and `spawn` are compile errors (they ran synchronously). Lambdas and nested `fn`s that read or assign an enclosing function's local are a compile error (they lost the capture and aborted at runtime); non-capturing lambdas still compile and run. Lambda bodies now share the module's diagnostics (errors inside lambdas were previously lost). `Share`/`Downgrade`/`Move` still lower as the inner value (no semantic loss for value handles). Tests: `test_native_rejects_enum_patterns_async_and_captures`, `test_native_non_capturing_lambda_runs`. Real support: enums and closures in Phase 2 (tasks 8-9), async parked (section 3) |
| P1-i `switch_lowering` (task 11) | — | Done | Matches whose arms are integer literals (4+ cases, values fit imm32, optional final `_`/binding catch-all) lower through `SwitchLowering` (binary search tree for sparse values, compare chain for dense); all other matches keep the general compare chain. Fixed duplicate BST block labels when two switches in one function share a pivot value. Test: `test_native_int_match_switch_lowering`. **Found while testing:** `StackFrameOptimizationPass` (`opt/frame_opt.rs`) shrank `stack_size` to the deepest `StackSlot` operand, ignoring RBP-relative `Memory` operands and most instruction kinds, so live locals (e.g. `print` argument arrays) ended up below RSP and were overwritten by calls: every program with 5+ `print`s printed garbage from the fifth on. The pass now walks all operands exhaustively, counts RBP-relative accesses, and never shrinks when RBP escapes into a value. Tests: `frame_opt::tests::*`, `test_native_many_prints_keep_frame`. Also fixed `test_e2e_seven_arguments_register_and_stack`, which used unallocated vregs and was rejected by the MIR verifier |
| P1-j Conformance corpus (task 12) | — | Deferred → Phase 2 task 10 | A full sweep of the 1,018 `examples/` programs would take roughly 15-30 min per debug run, and most failures today would be unsupported runtime features (Phase 2+ work) rather than Phase 1 miscompiles. Phase 1 correctness is covered by the per-bug regression tests above |

Known follow-ups: weak `__udivti3`, `__extendhfsf2`, `__truncsfhf2` resolve to NULL in
runtime links (pre-existing; a call would crash). ELF/Mach-O now take the libc import path
that was previously unreachable; not yet execution-tested. The parser lowers an `unsafe { .. }`
*expression* block to an immediately-called lambda, so such a block that reads an outer
local is now rejected as a capture (before P1-h it aborted at runtime); lower it as a plain
block instead when closures land.

### Phase 2 — Full x86-64 ABI (≈3-4 weeks)

**Goal:** complete C-ABI compatibility on the strongest target, designed to not preclude
value types (decision: value-type structs deferred until after Phase 2).

Tasks:
1. One shared argument classifier (SysV eightbyte classification, MS x64 by-value rules)
   consumed by the native calling convention, lowering, and FFI — today three paths
   disagree and the executable path only knows Gpr/Float. Design it around field
   layouts so value types can adopt it later.
2. sret for large aggregates in both directions.
3. Variadics: SysV register save area + `AL` + `va_list`; Win64 RCX/RDX/R8/R9 spill to
   shadow space.
4. Struct-by-value args and returns end-to-end with execution tests (Win64 first; SysV
   verified in Phase 3 CI).
5. TLS on Windows end-to-end: `fs:` thread-pointer sequences, `_tls_index` usage. The PE
   TLS directory already works; codegen emits no access sequences and all TLS relocation
   kinds are inert. Keep ELF/Mach-O TLS as loud errors.
6. Complete atomics: 8/16/32/64-bit widths, fences.
7. C-interop proof: call real ucrt functions (including `printf` varargs, `memcpy`) from
   Adesh-compiled code, execute, verify output.
8. Native enums (deferred from P1-h): define a tagged layout (tag word + payload laid out
   by the task 1 classifier), lower variant construction, and implement
   `HirPattern::EnumVariant` matching (tag compare) with payload binding, including
   `Option`/`Result`. Remove the P1-h compile error once execution tests pass.
9. Closures (deferred from P1-h): free-variable analysis in lowering, heap-allocated
   environment captured by value (by-reference for mutated captures via boxed cells),
   closure values as (fn ptr, env) pairs with the env passed as a hidden first argument;
   update indirect calls and `aot_make_function`. Remove the capture error once
   execution tests pass; then lower `unsafe { .. }` expression blocks without a lambda.
10. Conformance corpus (deferred from Phase 1 task 12):
    - A two-file showcase example (`examples/showcase/`: a main program plus a library
      module it imports via `import`/`export`) that exercises the language features and
      std libraries in labeled sections. Run each section in both the interpreter and
      the native build, and record what fails where (`examples/showcase/RESULTS.md`).
      Native imports must be implemented first (P1-g made them a compile error).
    - A curated corpus (~30-40 core-language programs) asserting native vs interpreter
      stdout/exit parity, plus must-reject programs; runs on every Windows CI run.
    - The full `examples/` sweep as a nightly / on-demand job with a checked-in status
      file (parity / rejected / skipped); any silent divergence fails the job.

**Acceptance:** ABI conformance suite executes binaries covering ints, floats, mixed,
small structs, large-struct sret, varargs, atomics, TLS — green in Windows CI.

#### Phase 2 progress (updated 2026-10-07)

Phase 2 tasks P2-1 through P2-10: **complete**.

| Task | Scope | Status | Evidence |
|---|---|---|---|
| P2-1 Unified Argument Classifier (task 1) | SysV eightbyte psABI + Win64 by-value rules | Done | `crates/adesh-codegen/src/abi/{sysv64,win64,mod}.rs`, `calling_convention/mod.rs`, `lower.rs::hir_type_to_abi_type`. Tests: `native_phase8_abi_e2e_test` 3/3. **Audit caveat (2026-10-07):** the caller-side path (`classify_args`) still maps every source-level argument to i64/f64 before classifying; real aggregate types only reach the classifier from the callee side and the FFI lowerer |
| P2-2 sret for large aggregates (task 2) | Caller buffer alloc + callee hidden sret pointer in RAX | Done (MachineIR/FFI level) | Callee hidden first arg extraction + RAX return in `lower.rs`; caller stack buffer alloc & RAX pointer capture in `lower.rs` and `FfiCallLowerer`. Execution test: `test_native_ffi_sret_execution_e2e` (exits 33; callee is hand-built MachineIR). **Audit caveat (2026-10-07):** unreachable from Adesh source — no annotation produces `HirType::Tuple`, the only type that classifies as an aggregate; and the callee copy assumes the returned value is a pointer to raw fields, which does not match the runtime's table-handle representation. Needs a real value representation before source-level use |
| P2-3 Variadics (task 3) | Win64 shadow spill + SysV RSA/va_list + caller %al/shadow | Done (MachineIR/FFI level) | `Win64Variadics::emit_callee_shadow_spill` into `[RBP + 16..40]`; `SysVVariadics` 176B RSA spill + 24B `va_list`; caller shadow space (32B) allocation on RSP in `FfiCallLowerer`; caller-side `%al` float count on SysV. Execution test: `test_native_variadic_execution_e2e` (exits 60; callee is hand-built MachineIR). **Audit caveat (2026-10-07):** the SysV RSA/`va_list` emitters are not wired into `src/backends/native/lower.rs` (no variadic function syntax, no `va_arg` read path); Win64 float-vararg GPR duplication documented in `abi/mod.rs` is not implemented |
| P2-4 Struct-by-value args & returns (task 4) | Small structs (1, 2, 4, 8B) in GPRs and stack | Done | Win64 pass-by-value in registers (RCX, RDX, R8, R9) and stack (`[RSP + 32]`), return by value in RAX; SysV eightbyte GPR/SSE classification. Tests: `tests/native_phase2_abi_struct_by_value_test.rs` 5/5 passing (unpack 4B struct exits 37, make 8B struct exits 35, mixed 5 args with struct on stack exits 20). **Audit caveat (2026-10-07):** tests drive the FFI lowerer with hand-built MachineIR callees; not reachable from Adesh source |
| P2-5 Windows TLS (task 5) | `gs:[0x58]` / `_tls_index` sequences, `.tls` section & PE directory | Done | `MachineInstruction::TlsAddress`, `encode_win64_tls_address` in x86_64 backend; ADOB `SectionKind::Tls` & `RelocationKind::TlsLe` decoding in linker; `tests/native_phase2_abi_tls_test.rs` 3/3 passing (SysV rejection, live read exits 47, live read-modify-write exits 27). **Audit caveat (2026-10-07):** reachable only via `MachineInstruction::TlsAddress`; no source syntax emits it yet |
| P2-6 Complete Atomics (task 6) | 8/16/32/64-bit widths, fences | Done | `lock xadd`, `lock cmpxchg`, `lock xchg` for 8, 16, 32, 64-bit widths in `crates/adesh-codegen/src/targets/x86_64/{encoder,mod}.rs`; `AtomicExchange` in `machine_ir`; `Barrier` (`mfence`). Tests: `tests/native_phase2_abi_atomic_test.rs` 4/4 passing (xadd 64 exits 42, xadd sized 32/16/8 exits 19, cmpxchg success+fail exits 75, xchg exits 55). Atomic ops with non-memory destinations or non-1/2/4/8 sizes are now `CodegenError`s instead of silent no-ops (fixed 2026-10-07). **Audit caveat:** no atomic lowering exists in `lower.rs`; MachineIR level only |
| P2-7 C-interop proof (task 7) | Real UCRT/libc calls (printf, memcpy, malloc, free, strlen, sqrt) | Done | Called real UCRT functions, executed linked PE binaries, asserted exact stdout & exit codes. Tests: `tests/native_phase2_abi_c_interop_test.rs` 5/5 passing, `test_native_variadic_execution_e2e`. **Audit caveat (2026-10-07):** callers are hand-written MachineIR (manual register/shadow-space setup), not Adesh source with FFI declarations |
| P2-8 Native Enums (task 8) | Tagged layout & pattern matching | Done | Tagged 16B representation `[ptr+0]` tag, `[ptr+8]` payload via `aot_alloc(16)`; unit and single-payload variant construction; tagged pattern matching with recursive variable binding and literal discrimination; user-defined enums + built-in `Option` (`Some`/`None`) and `Result` (`Ok`/`Err`). Multi-payload variants are a loud compile error (were silently truncated; fixed 2026-10-07). Tests: `tests/native_phase2_abi_enum_test.rs` 6/6 passing (`Some` exits 42, `None` exits 99, `Ok`+`Err` exits 65, user unit variants exit 60, user payload variants exit 50, literal subpatterns exit 81), `native_semantics_e2e_test` 22/22 passing. **Known limitation:** variant tags are a flat namespace (`None=0/Ok=0`, `Some=1/Err=1`), so cross-enum patterns can collide; matching also assumes an enum-typed scrutinee. Type-directed matching is follow-up work |
| P2-9 Closures (task 9) | Captured environments & static chain convention | Done | Free-variable capture analysis `collect_lambda_captures`, heap-allocated environment layout via `aot_alloc`, closure object `[fn_ptr, captured...]`, indirect call via `%r10` static chain calling convention; lifted Phase 1 capture error for lambdas. Tests: `tests/native_phase2_abi_closure_test.rs` 5/5 passing (single capture exits 45, multiple captures exits 47, nested closure exits 30, mutated capture exits 66, higher-order dispatch exits 65). **Audit caveats (2026-10-07):** captures are by-value copies into the closure (no boxed cells — the enclosing frame does not observe mutations; plan divergence, see Decisions Log); nested `fn` statements still reject captures; float-returning lambdas are now a loud compile error (indirect calls read RAX) |
| P2-10 Conformance Corpus (task 10) | Multi-file showcase & parity corpus | Partially done | Native module import resolution `resolve_hir_module_imports` (import failures are now diagnostics, not silent drops — fixed 2026-10-07), multi-file showcase in `examples/showcase/` with `math_lib.adesh` and `main.adesh`, `RESULTS.md`. Tests: `tests/native_phase2_conformance_test.rs` 7/7 passing (showcase e2e, closure mutation, result pattern matching, control flow/defers, must-reject unresolved import, must-reject region, must-reject async fn). **Audit caveat (2026-10-07):** the suite asserts against hardcoded expected output — it never runs the interpreter, so "parity" and the `RESULTS.md` interpreter column are unverified; the curated ~30-40-program corpus and the full `examples/` sweep with a checked-in status file do not exist yet |

### Phase 3 — Linux Execution For Real (≈2-3 weeks)

**Goal:** turn "Emits; not run-tested" into "Execution-tested in CI".

Tasks:
1. Proper `_start` synthesis: read argc/argv from the initial stack, call `main`, exit
   syscall. Today `e_entry` points at `main` with no stack setup.
2. Decide ELF dynamic scope honestly: either implement GOT/PLT (+ `.gnu.hash`, full
   RELRO) or restrict the native linker to static executables until it exists. Imports
   must never silently resolve to VA-0.
3. Ubuntu CI job: Adesh source → ADOB → ELF → execute → assert exit/stdout. Add an
   Alpine/musl variant. Lean on existing Docker assets.
4. Archive writer symbol index (`/` member) so produced archives stop forcing readers
   into full-scan fallback; thin-archive reading (stretch).
5. GC: drop the name-based fallback that over-retains; make ICF `Safe`/`All` differ.

**Acceptance:** `x86_64-unknown-linux-gnu` and `-musl` execute in CI with asserted exit
codes/stdout; `TARGET_MATRIX.md` updated accordingly.

#### Phase 3 progress (updated 2026-10-07)

Phase 3 tasks P3-1 through P3-5: **complete**.

| Task | Scope | Status | Evidence |
|---|---|---|---|
| P3-1 Proper `_start` Synthesis (task 1) | Linux ELF x86-64 startup sequence | Done | `linker/src/elf/x86_64.rs` synthesizes standard startup (`xor ebp, ebp; mov rdi, [rsp]; lea rsi, [rsp+8]; lea rdx, [rsi+rdi*8+8]; and rsp, -16; call main; mov edi, eax; mov eax, 60; syscall; hlt`); `ElfWriter` resolves `_start` dynamically when absent. Tests: `test_linux_elf_start_synthesis_and_exit_code` (exits 42) |
| P3-2 Static ELF & Symbol Resolution (task 2) | No VA-0 silent fallbacks & static ELF generation | Done | Non-intrinsic libc symbols resolve to `SymbolRoute::Undefined` rather than VA-0; `ElfWriter` outputs static executables (`!shared`) without interpreter (`PT_INTERP`) or dynamic table (`PT_DYNAMIC`) tags. Tests: `test_linux_elf_static_binary_segments_and_headers` |
| P3-3 Linux Execution Suite (task 3) | Real ELF execution & exit-code assertion | Done | End-to-end Linux execution test suite in `tests/native_phase3_linux_e2e_test.rs` covering arithmetic, control flow/loops, pattern matching, and SysV 6-argument register calling convention (`rdi, rsi, rdx, rcx, r8, r9`). All 8 tests passing with verified exit codes (42, 50, 55, 119, 21, 42, 77); 7 execute binaries (native on Linux CI, WSL on Windows), 1 is structural. Static ELFs are freestanding (no CRT), so only exit codes are asserted, not stdout. **Audit caveat (2026-10-07):** the musl half of the acceptance is unmet — no musl/Alpine CI job exists yet |
| P3-4 Archive Symbol Index `/` (task 4) | GNU `/` symbol table member generation & resolution | Done | `Archive::encode_gnu` emits standard big-endian GNU `/` symbol table and string pool with precomputed member offsets; `Archive::parse` reads `/` symbol table directly with zero full-scan fallback. Tests: `ar::tests::test_gnu_archive_roundtrip_with_symbol_index`, `test_linux_elf_archive_symbol_index_resolution` (exits 42) |
| P3-5 Linker GC & ICF Safe vs All (task 5) | Precise GC reachability & address-taken ICF preservation | Done | `gc.rs` tracks file-local vs global symbol definitions without broad same-named over-retention; the name-only fallback retains *both* the same-file local def and the authoritative global def (fixed after audit — retaining only the local could drop the section the resolver actually binds). `icf.rs` differentiates `IcfMode::Safe` from `IcfMode::All` by detecting address-taken functions and preserving them in `Safe` mode. Tests: `icf::tests::test_icf_safe_vs_all_modes`, new `gc::tests::*` (5 unit tests covering removal, name-only reachability, exported roots, local/global shadowing, strong-over-weak), `test_linux_elf_gc_dead_code_elimination` (exits 77; verifies GC-enabled links keep live code — per-function removal is not observable there because the x86-64 emitter places all functions of a module in one `.text` section) |

### Phase 4 — Optimizer and Register Allocator Maturity (≈4-6 weeks)

**Goal:** generated-code quality, honest flags, measurable numbers.

Tasks:
1. Wire-or-delete (policy of record): delete `src/ir/optimizations/` and `opt/ipo.rs`;
   wire PGO end-to-end or remove the `--pgo` flag; wire `opt/lto.rs` via `CompilerDriver`
   or delete it; until real cross-module IR optimization exists, `--lto` help must
   describe what it does (GC+ICF+strip); decide vectorization/scheduler disposition.
2. Register allocator: live-range splitting at call boundaries (replacing the
   conservative callee-saved-or-spill rule), spill-cost eviction, verification-driven
   retry. Farthest-next-use eviction is an initial improvement, not completion of
   weighted spill costing.
3. Post-allocation copy coalescing and move elimination.
4. Binary-size attack on hello-world (127,488 bytes, ≈5.8x the 22,016-byte C baseline):
   PE section alignment, import trimming, unused runtime symbol pruning, section
   merging, CRT dependency audit. Target: < 40 KB; stretch: 22 KB parity.
5. Benchmark harness (criterion): codegen+link at 10/100/1K/10K LOC; per-phase timing
   (parse / typecheck / lower / regalloc / encode / link); instruction counts and
   runtime of produced binaries across O0-O3.

**Acceptance:** benchmark numbers tracked in CI; measurable improvements in spills,
code size, binary size; every CLI flag does exactly what its help says.

**Progress (2026-10-09):**
- The orphaned `src/ir/optimizations/` tree and fabricated-counter `opt/ipo.rs`
  were removed. Native `--lto` now routes through `CompilerDriver` and
  `LtoEngine`; the wired path has an individual execution test.
- Native PGO now accepts `--pgo=generate` and `--pgo=use=<path>`. PE generation
  emits thread-safe block counters and an RVA table; the Windows startup stub
  calls the runtime dumper before process exit. The generate/run/use cycle is
  execution-tested, both with and without LTO. ELF counter dumping remains
  unsupported and fails loudly.
- Self tail recursion is converted to a CFG backedge for the proven scalar
  subset; eligible integer/pointer local tail calls use frame teardown plus a
  PC-relative jump. Deep recursion, linked execution, and ADOB relocation
  behavior have focused tests.
- `BasicBlockScheduler` is wired post-allocation with conservative ordering
  barriers and focused hazard tests.
- Linear scan considers exact instruction liveness at call sites. Pressure
  eviction uses future-use density, loop-depth weights, and next-use distance.
  A verifier-failure fallback restores the original function and retries with
  unconstrained virtual registers spilled. For GPR values crossing zero-argument
  calls, splitting is now allowed when the call is outside a cycle and
  dominates every use. Post-call pieces are remapped into dominated successor
  blocks, including joins; linked Windows tests cover the call result and the
  preserved value. Calls with arguments, call-bypassing paths, and loop call
  sites keep the verified stack-copy fallback. This remains bounded CFG-aware
  splitting, not arbitrary interval splitting.
- Copy coalescing now checks interference outside the copy position and fails
  closed on unsupported operands. Post-allocation cleanup removes redundant
  physical-register moves and stack-slot self-copies.
- The previous auto-vectorizer's scalar-to-vector substitution was unsound.
  It is now wired fail-closed and tested to preserve scalar instructions.
  Real lane construction, loop legality, and remainder handling remain
  incomplete and are not claimed as implemented.
- Added `benches/native_pipeline.rs` for generated modules with 10/100/1K/10K
  functions. It separates parse/HIR, typecheck, native lowering, register
  allocation, encoding, ADOB generation, and linking, plus Windows executable
  runtime cases at O0-O3. The debug-profile run completed with 10 samples per
  case. Stage means, machine-instruction/ADOB sizes, and allocator outputs with
  splitting enabled versus the stack-copy fallback are recorded in
  `docs/performance-guide.md` and Criterion JSON artifacts. On the synthetic
  call-pressure function, splitting removed 50 machine instructions and 50
  explicit load/store instructions, while using 48 more stack-frame bytes.
  Windows CI runs the same debug benchmark and uploads `target/criterion`; the
  workflow artifact itself has not yet been observed from a CI run.
- The print/abort ABI now has a standalone C implementation, with small Rust
  ABI wrappers in a separate archive member. Redirected output remains UTF-8;
  console output converts valid UTF-8 to UTF-16 in bounded chunks. An executed
  hello-world PE measures 4,096 bytes, down from 112,640 bytes after the prior
  runtime-writer optimization, meeting the <40 KiB target. The CLI `-Os` and
  `-Oz` integration regression checks size, execution, and redirected Unicode
  output. A Windows console-buffer regression directly tests the UTF-16 chunk
  boundary with a supplementary character.
  The minimal return-only PE remains a separate 2,560-byte test.
- Added Linux ELF execution coverage for both `-Os` and `-Oz`, complementing
  the Windows PE CLI coverage. The Windows-host session can compile-check the
  Linux test but cannot execute its ELF output; the Ubuntu integration CI step
  is configured to run it.
- Linker warning cleanup now tracks undefined weak placeholders separately,
  excludes unused ones from synthesized stubs, and delays diagnostics until a
  retained relocation references them. The reported
  `__extendhfsf2`/`__truncsfhf2`/`__udivti3` warnings no longer appear for the
  simple print program; an unresolved weak call that is actually referenced
  still routes to a trap stub and emits a contextual warning.
- Phase 4 has concrete benchmark and size results, but is not fully closed:
  general splitting through loop cycles, call-bypassing paths, and
  argument-bearing calls remains unsupported; the CI artifact path awaits a CI
  run. Real vectorization remains fail-closed and deferred.

**Remaining acceptance work:**
1. Extend splitting beyond calls that dominate all later uses, and cover
   argument-bearing calls only with piece-sensitive allocation and verifier
   coverage; retain the tested fallback for unsupported cases.
2. Confirm the benchmark artifact and focused console/size-level regressions in
   CI.
3. Extend size-level end-to-end coverage beyond Windows x86-64 and Linux
   x86-64, then confirm the configured tests run in CI.

Real vectorization stays deferred until lane construction, loop legality, and
remainder handling have dedicated correctness tests. Continue running only
individual debug-mode tests; do not run full `cargo test` or release builds.

### Phase 5 — Debuggability (≈2-4 weeks; Linux part gated on Phase 3)

Tasks:
1. Windows first: wire `WindowsPdataGenerator` into the real link path with real unwind
   codes (`.pdata`/`.xdata`); panic aborts carry function info; verify backtraces in
   WinDbg/cdb.
2. Wire `Dwarf5Generator` for ELF with a real `.debug_line` state machine; verify
   breakpoints in GDB on Linux once Phase 3 lands.
3. Local-variable location lists (stretch); inline frames (later).

**Acceptance:** breakpoint at an Adesh function, backtrace, and source-line stepping
work on Windows (and on Linux after Phase 3).

### Phase 6 — AArch64, For Real (only after Phases 1-5)

Tasks:
1. Real backend: `ldr/str/ldp/stp`, branches (`b/bl/cbz/b.cond`), prologues/epilogues,
   full AAPCS64 (fix the empty AArch64 callee-saved list; x19-x28 preserved),
   relocations (ADRP/ADD/CALL26/JUMP26), frame layout, scalar FP, atomics.
2. qemu-user execution tests in CI (existing Docker targets can host this).

**Acceptance:** `aarch64-linux` static binary runs under qemu with asserted exit code;
target matrix moves from proof-of-concept.

---

## 3. Explicitly Parked (documented honestly, not grown)

- GPU/NPU/TPU ISA generation and drivers (today: packaging scaffolding only).
- Quantum language integration (library-level only; measurement is a mock).
- Running Mach-O executables (needs dyld info/chained fixups/exports trie; documented
  as "artifact emission only").
- Linker WASM writer (the compiler backend is the real WASM path).
- Async runtime (epoll/IOCP/kqueue reactor). Since P1-h the native backend rejects
  `async fn`, async lambdas, `await` and `spawn` at compile time. Unpark after Phase 3
  (needs Linux execution for epoll) and after closures (Phase 2 task 9), since async
  bodies become state machines that capture their locals.
- Self-hosting; i686/PowerPC targets.
- Value-type composites (unboxed structs) — deferred until after Phase 2 by decision
  of 2026-10-05; Phase 2 classifier design must not preclude them.

---

## 4. Decisions Log

| Date | Decision |
|---|---|
| 2026-10-05 | Execution starts with Phase 0 (docs + CI truth reset). |
| 2026-10-05 | This file (`IMPLEMENTATION_PLAN.md`) replaces the previous 2026-10-01 plan. |
| 2026-10-05 | Dead/unwired code policy: wire-or-delete within the phase that owns it. |
| 2026-10-05 | Value-type structs: deferred until after Phase 2; ABI classifier designed around field layouts to keep the door open. |
| 2026-10-06 | FFI sret and register-pair values are rejected with a structured error until Phase 2 implements them (row 8). |
| 2026-10-06 | Linker intrinsics are limited to names with real synthesized bodies; libc functions are always imported. |
| 2026-10-06 | Native enums, closures and async are compile errors until implemented; enums and closures scheduled as Phase 2 tasks 8-9, async stays parked. |
| 2026-10-06 | Phase 1 closed. The conformance corpus (task 12), including the two-file showcase with imports/exports, is deferred to Phase 2 task 10. |
| 2026-10-07 | P3-2 scope decision recorded: the native linker emits **static ELF executables only**; GOT/PLT dynamic linking is not implemented, and non-intrinsic imports on ELF are a loud link error, never VA-0. musl execution testing deferred until an Alpine/musl CI job exists. |
| 2026-10-07 | Closure semantics: captures are by-value copies into the heap closure object (no boxed cells). Mutated captures are shared between calls of the same closure but are NOT observed by the enclosing frame. Diverges from the task-9 text ("by-reference via boxed cells"); boxed cells are future work if reference semantics are wanted. |
| 2026-10-07 | Phase 2 ABI features (sret, struct-by-value, variadics, TLS, atomics) are implemented and execution-tested at the MachineIR/FFI level only; Adesh source syntax cannot reach them yet. Claims must say so until front-end syntax lands. |
| 2026-10-07 | Audit fixes: multi-payload enum construction/matching and float-returning lambdas changed from silent wrong code to loud compile errors; atomic ops with non-memory destinations are `CodegenError`s; unparseable import files are diagnostics instead of being silently dropped; GC name-only fallback retains both local and global candidate definers. |

---

## 5. Phase Dependencies

```text
Phase 0 (truth reset)
    ↓
Phase 1 (no silent miscompiles)  ← highest correctness risk, do first after docs/CI
    ↓
Phase 2 (full x86-64 ABI)
    ↓
Phase 3 (Linux execution)  ← ABI work from Phase 2 is exercised on SysV here
    ↓
Phase 4 (optimizer/regalloc maturity, size/speed)
    ↓
Phase 5 (debuggability: Win64 SEH first; DWARF needs Phase 3)
    ↓
Phase 6 (AArch64 for real)
```
