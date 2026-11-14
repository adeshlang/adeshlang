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
6. **The workspace gate is part of every phase's DoD.** `cargo fmt --all -- --check`
   must be clean, and `cargo clippy --workspace --all-targets --all-features` must
   report no errors (it denies `clippy::correctness`) and no new warnings in the
   crates a phase touches. See `CURRENT_STATE.md` §0.6 for the measured state and for
   the crates that currently silence their own lints — a green clippy run there is not
   evidence of a clean crate.

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
| x86-64 Windows | PE executables, imports, TLS, atomics, SEH emission; PE DLL loading/export calls/rebasing locally verified (CI pending) | source-level ABI surface; vector ABI |
| x86-64 Linux (gnu) | static/dynamic ELF, `dlopen` `.so`, TLS LE/IE, `.eh_frame` decode | runtime unwinding; cross-target archives |
| x86-64 Linux (musl) | TLS local-exec/initial-exec in WSL | dedicated musl CI job |
| AArch64 Linux | codegen/ELF with ADRP/ADD and real call-site relocations; unsupported forms fail loudly. Startup, four default/fast source cases, S/D/Q transfers, scalar/vector spills, v4f32 add/sub/mul, vector call preservation and scalar FP32/FP64 add/sub/mul/div execute locally under QEMU (CI pending; §0.19) | Broader source/ABI conformance; GOT/TLS execution; CI-produced runtime archive; FP negation/comparison/conversions; wider vector types |
| AArch64 / x86-64 macOS | Mach-O emission, dyld bind/rebase, TLV emission, codesign structure | any dyld execution; TLV at runtime; dylib loading |
| RISC-V 64 | codegen reaches the linker from real source (`ParallelMove` and `auipc`/`addi` symbol materialization, both 2026-11-07 — `CURRENT_STATE.md` §0.12); loud errors for unsupported instruction variants; structural ELF; the `PCREL_HI20`/`PCREL_LO12` pair proven on linked image bytes | target runtime archive (the only thing between the CLI and a linked image); GOT/PLT sequences; linker relaxation; execution |
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

1. **AArch64: loud unsupported instructions. — DONE 2026-11-01.**
   In `crates/adesh-codegen/src/targets/aarch64/mod.rs` the encoder's `_ =>`
   arm emitted `NOP`. It is gone: every `MachineInstruction` variant without an
   AArch64 encoding (29 — `ParallelMove`, `Mod`, `Neg`, `Not`, `Shl`, `Shr`, `Sar`,
   `Test`, `Push`, `Pop`, `Custom`, `FNeg`, `FCmp`, the three `FCvt*`, `VectorDiv`,
   `VectorAnd`, `VectorOr`, `VectorXor`, `VectorLoad`, `VectorStore`,
   `VectorBroadcast`, `VectorShuffle`, `VectorReduceAdd`, `VectorBlend`,
   `AtomicLoad`, `AtomicStore`, `TlsAddress`) returns a `CodegenError` naming the
   variant (via `{:?}`) and the containing function. The silent operand drops went
   with it: helper encoders return `bool`/`Option` and each call site fails with a
   message naming the unsupported form instead of emitting nothing, and unallocated
   virtual registers are refused before encoding starts. `tests/native_phase11_codegen_loudness_test.rs`
   pins all of it. Revealed two previously silent miscompiles, now loud: FP/vector
   spill slots encoded as 64-bit GPR loads with the register number truncated to 5
   bits (`V30` → `X30`, the link register), and `VectorAdd` between stack slots
   encoded as `ADD V0.4S, V0, V0`. Implementing those needs `LDR`/`STR Dd, [SP]` —
   tracked with Phase 14.
2. **AArch64: real relocation offsets. — DONE 2026-11-01** (offsets since
   2026-10-24/25; the multi-call, multi-function ADOB test named in this item
   landed with item 1 — `aarch64_call_relocations_land_on_their_bl_sites` decodes
   5 call sites across 2 functions and asserts each relocation offset is 4-byte
   aligned, distinct, and lands on a word whose opcode is a `BL`).
3. **AArch64: fail before emitting a wrong object. — DONE 2026-11-01**
   ADRP/ADD_LO12 symbol materialization is emitted (`AArch64_AdrPage21` +
   `AArch64_AddAbsLo12`), and a `Call` whose target is neither a register nor a
   symbol is refused rather than encoded as an unrelocated `BL #0`.
4. **RISC-V: move the RV32 refusal to target validation. — DONE 2026-11-02.**
   `create_backend` in `crates/adesh-codegen/src/targets/mod.rs` rejects
   `Architecture::RiscV32` with a `CodegenError` naming the canonical triple,
   the reason (RV64 doubleword encodings would produce an image RV32 hardware
   cannot run) and a `riscv64gc` alternative — the refusal happens while the
   backend is selected, so no function body is ever encoded. Measured on the
   real CLI: `adesh build --target riscv32imac-unknown-none-elf` exits 1 with
   `Target:`/`Architecture:` lines and **no** `Function:` line, while the same
   file on `riscv64gc-unknown-linux-gnu` fails later, at `Function: main`. The
   linker half: `_start` synthesis in `linker/src/linker.rs` is gated on
   x86_64/AArch64/RISC-V-64, so an RV32 executable is a named error instead of
   an RV64 stub. `ParallelMove` stays refused for RV64, which is item 7's
   remaining capability gap, not this gate.
5. **JIT test honesty. — DONE 2026-11-02.**
   The three Windows failures in
   `tests/native_phase8_jit_native_conformance_test.rs` came from generated
   `import … from "C:\Users\…"` statements: the lexer maps an unknown escape
   `\X` to `X`, so the path silently became `C:Users…` and the OS rejected it
   (error 123). Every interpolated path now goes through `adesh_string_body`,
   which escapes `\` and `"`; the suite is 10/10 on Windows and is named in the
   Windows CI job.
6. **Tier realignment. — DONE 2026-11-02.**
   `Target::tier()` in `linker/src/target.rs` is now derived from what CI
   verifies, with each arm commented by the suite that earns it: **Tier 1** =
   CI builds an artifact and **runs it** with asserted output or exit status
   (`x86_64` Windows GNU/MSVC, `x86_64-unknown-linux-gnu`, and the WASM pair —
   CI executes linked modules on V8 and Cranelift, so they moved *up* from the
   earlier `Tier2Experimental`); **Tier 2** = emitted and parsed back, never
   executed (AArch64 Linux/macOS/Windows — demoted from `Tier1Supported`,
   `x86_64-apple-darwin`, RISC-V 64, musl); **Tier 3** = declared only.
   The lists a user reads are the same data: `ADVERTISED_TARGETS` +
   `advertised_by_tier()` + `TargetTier::advertised_heading()` feed `adesh
   target list` (`src/main.rs`), `adeshlink targets` (`linker/src/main.rs`) and
   `src/toolchain/cli/help.rs`. Android (`aarch64-linux-android`,
   `armv7-linux-androideabi`, `aarch64-apple-ios`) and the old "Mobile" /
   "Embedded" prose groups are gone from both CLIs; `i686`, `riscv32*`, `armv7`
   /Thumb and `powerpc64le` are still printed, but under Tier 3 with a summary
   that names the refusal, and
   `refused_targets_are_not_advertised_as_supported` forbids a Tier-3 summary
   from reading like support. `x86_64-pc-windows-gnu` also dropped Tier 1 → Tier
   2: every suite that links **and runs** a PE asks for `-msvc`, so the shared
   PE writer has no execution evidence for the MinGW flavour — which needed
   `LibcFlavor::Msvc`, because `triple_string()` normalizes to `arch-os` and
   cannot tell the two apart. Docs updated
   in this change set: `TARGET_MATRIX.md` (verification rule, RV32/i686/musl
   rows, "Next important" 8 and 9), `CURRENT_STATE.md` (§0 RISC-V/WASM/JIT rows,
   §0.5 gaps, new §0.7), `docs/TOOLCHAIN_CAPABILITIES.md` and
   `docs/TOOLCHAIN_PLATFORM_ARCHITECTURE.md` (the "PoC (4 ops)" rows and the
   tier columns), `ARCHITECTURE_GAPS.md` (§1.5 new, §2.1, §3.2–§3.5 re-checked
   against the code), `ABI_MATRIX.md` (RV32 row).
   Found and recorded, not fixed: `linker/src/arch/mod.rs:44` maps `Arch::X86`
   to `X86_64Arch`, the same silent-fallback class the gate closed in the
   backends.
7. **RISC-V: loud unsupported instructions and operand forms. — DONE 2026-11-01.**
   The mirror of item 1 for `crates/adesh-codegen/src/targets/riscv/mod.rs`: 21
   `MachineInstruction` variants with no encoding and 43 unencodable operand forms
   return a `CodegenError` naming the variant or form and the containing function.
   Every placeholder-register `unwrap_or` in the vector, load/store and atomic arms
   is gone, `VectorLoad`/`VectorStore` are refused instead of hardcoding `sp`, the
   14 `ConditionCode` variants are enumerated with `Parity`/`NotParity` refused,
   widths outside {1,2,4,8} and `shamt` outside 0..64 are refused, and branch
   fixups fail on an undefined label or a displacement past B-type ±4096 / JAL
   ±1 MiB. Revealed two live miscompiles, now fixed: `Move` with an immediate
   source truncated a non-GPR destination into `x0` and discarded the value, and
   the atomic arms replaced a register address with `sp`. Proven by the 8 RISC-V
   tests in `tests/native_phase11_codegen_loudness_test.rs`. `ParallelMove` stays
   unimplemented and refused, so source-level RISC-V builds remain blocked — that
   is capability work, not this gate.

### Tests and CI
- `tests/native_phase11_codegen_loudness_test.rs` — **landed 2026-11-01, 15 tests**
  (6 AArch64 + 8 RISC-V + 1 RV32 refusal):
  - AArch64 unsupported `MachineInstruction` variants return `Err` naming the
    variant and the function (29 variants).
  - AArch64 unsupported operand forms return `Err` (14 cases: FP/SIMD stack slots,
    a `Call` targeting a label, vector operands that are not `V` registers, an
    atomic with a stack-slot destination, and similar).
  - RISC-V unsupported `MachineInstruction` variants return `Err` (21 variants,
    including every `Vector*` op with no RV encoding here, `AtomicLoad`/
    `AtomicStore`, `TlsAddress` and `ParallelMove`).
  - RISC-V unsupported operand forms return `Err` (43 cases: indexed loads and
    stores, atomics whose address is not a register, non-`V` vector operands,
    float accesses to a stack slot wider than 4/8 bytes, access sizes outside
    {1,2,4,8}, `shamt` outside 0..64, and `SetCc`/`BranchCc` on `Parity`).
  - An unallocated virtual register reaching either encoder fails with an
    actionable message that names `lower_module` as the missing step.
  - Emitted `.text` contains no unintended `0xD503201F`/`0x00000013`, and a `NOP`
    appears only when explicitly asked for (both backends).
  - AArch64 multi-call object relocation offsets are distinct, 4-byte aligned, and
    each lands on a word that is really a `BL`; the RISC-V equivalent pins 5
    `RiscV_Call` relocs, each on an `AUIPC` followed by its `JALR`. (5 sites over
    2 functions in both.)
  - RISC-V `BranchCc` refuses a displacement past the B-type ±4096 immediate
    instead of truncating it into the field.
  - RISC-V atomics encode with the address operand's own register, not the
    `sp` placeholder the pre-gate code hardcoded.
  - `riscv32_is_refused_at_backend_selection` (added 2026-11-02, item 4):
    `create_backend` returns an error naming the canonical triple and `RV64`,
    offers `riscv64gc`, and — asserted by the absence of a `Function:` line —
    fails **before** any function is encoded; `riscv64gc` still selects a
    backend.
- `linker/tests/phase11_riscv32_entry_stub_test.rs` — **landed 2026-11-02,
  3 tests**: an RV32 executable with no `_start` is refused and no image is
  written, RV64 still gets its synthesized stub, and RV32 is not labelled
  codegen-validated.
- `linker/tests/phase11_target_tiers_test.rs` — **landed 2026-11-02, 5 tests**:
  the Tier-1 set is exactly the targets a CI job executes; the Tier-2 set is
  the emitted-and-parsed-back set; the refused targets are Tier 3; every entry
  in `ADVERTISED_TARGETS` recomputes to the tier heading it is printed under,
  the groups come out 1→2→3, and no Tier-3 summary is worded as supported.
- `x86_64-unknown-linux-musl` was added to
  `tests/native_phase8_cross_compile_test.rs::test_cross_target_emission_and_validation`
  so its Tier-2 label rests on an emission check CI actually runs, not only on a
  developer's WSL session.
- New `linker/tests/phase11_relocation_offsets_test.rs` if the ADOB-level
  assertion fits better in the linker crate. (Not needed so far — the ADOB-level
  call-site assertion lives in the codegen suite above.)
- Add both suites to the Ubuntu (portable) and Windows jobs in
  `.github/workflows/ci.yml`. — **landed locally 2026-11-02**: a
  `Test Phase 11 loudness and tier suites` step (all three hosts, since the
  suites are host-independent) plus `native_phase8_jit_native_conformance_test`
  in the Windows job. **Not yet proven by a CI run** — the workflow is
  `workflow_dispatch`-only, so the steps go unexecuted until someone dispatches
  one; a green local run is not the gate this phase defines.

### Exit criteria
- [ ] No backend contains a silent instruction fallback (review gate for
      `NOP fallback` and for `unwrap_or(0)` used as an emit placeholder).
      **AArch64: clean** — no `NOP fallback` remains anywhere under
      `crates/adesh-codegen/src/targets/`, and the only `0xD503201F` emission left
      in the AArch64 encoder is the explicit `MachineInstruction::Nop` arm
      (`targets/aarch64/mod.rs:634`), which
      `aarch64_nop_is_emitted_only_when_asked_for` pins. **RISC-V: clean since
      2026-11-01** — `grep -n "unwrap_or" targets/riscv/mod.rs` returns nothing,
      every placeholder-register substitution in the vector, load/store and atomic
      arms is now a `CodegenError`, `VectorLoad`/`VectorStore` are refused instead
      of hardcoding `sp`, `SetCc`/`BranchCc` enumerate all 14 condition codes with
      `Parity`/`NotParity` refused, widths outside {1,2,4,8} and `shamt` outside
      0..64 are refused rather than defaulted, and branch fixups fail on an
      undefined label or a displacement past B-type ±4096 / JAL ±1 MiB. Checked at
      the same time: x86_64 routes every unencodable form through `unsupported()`
      (its `index.map(..).unwrap_or(0)` calls feed the REX.X bit only, and
      `emit_mem_operand` encodes `None` as the no-index SIB), and the embedded and
      wasm backends each ship a loud-unsupported suite
      (`targets::embedded::tests::function_bodies_are_loud_errors`,
      `targets::wasm::tests::unsupported_instructions_are_loud_errors`).
      **Linker exception closed 2026-11-12:** `Arch::X86` selects `X86Arch`,
      which refuses unsupported IA-32 relocations with `LNK008`, pinned by
      `linker/tests/phase11_x86_handler_refusal_test.rs`. This does not
      implement IA-32 linking; relocation-free images bypass the handler.
- [ ] A hello-world AArch64 `--emit=adob` contains no unintended NOPs. The
      backend-level equivalent is asserted
      (`aarch64_emitted_text_has_no_unintended_nop`); the end-to-end form is
      blocked because hello-world `--target aarch64 --emit=adob` still fails at
      link on the missing target runtime archive (Phase 12).
- [x] Every call relocation in a multi-call AArch64 object points at its call —
      **done since 2026-10-24/25**: `emit_object` in
      `crates/adesh-codegen/src/targets/aarch64/mod.rs` bases each relocation on the
      accumulated `.text` length rather than `index * 8`, and
      `tests/native_phase11_codegen_loudness_test.rs` has pinned it since
      2026-11-01 (`aarch64_call_relocations_land_on_their_bl_sites`, and the
      RISC-V `AUIPC` equivalent).
- [x] `adesh target list` tiers match `TARGET_MATRIX.md` — **structurally
      guaranteed 2026-11-02**: both CLIs render `ADVERTISED_TARGETS` through
      `advertised_by_tier()`, which recomputes `tier()` per triple and panics on
      one that does not parse, and `phase11_target_tiers_test` pins each label to
      the suite that earns it. `TARGET_MATRIX.md`,
      `docs/TOOLCHAIN_PLATFORM_ARCHITECTURE.md` and
      `docs/TOOLCHAIN_CAPABILITIES.md` now print the same Tier 1/2/3 split.
- [ ] Windows JIT conformance suite runs in CI and passes. **Fixed locally
      2026-11-02** (10/10 on Windows, suite named in the Windows job); this stays
      unchecked until a `workflow_dispatch` run shows it green in CI, because CI
      is the arbiter this phase exists to enforce.

---

## Phase 12 — Target runtime archives and linkable cross builds

**Effort:** M · **Depends on:** Phase 11

### Goal
Make `adesh build --target <triple>` produce a linked artifact for every
advertised native target, with no host-architecture fallback.

### Work items

1. **Target-aware runtime archive resolution.** **DONE 2026-11-03.**
   `get_static_runtime_lib` in `src/backends/aot/cranelift_impl/linking.rs` is
   now a `RuntimeArchiveSearch` that builds one ordered candidate list:
   `target/<triple>/<profile>/` (plus `deps/` and Cargo's hashed
   `adesh_runtime-<hash>.a`), an install's `lib/<triple>/`, and
   `$ADESH_RUNTIME_DIR/<triple>/` — then, **only when the requested triple is
   the host**, `target/<profile>/`, the install `lib/`, and a flat
   `$ADESH_RUNTIME_DIR`. `target/<triple>/release/` is in the list even for a
   debug toolchain, because a cross archive is built deliberately.
   Two host-only paths that used to be reachable from a cross build are gone:
   the interleaved host candidates in the old `search_project_roots`, and the
   `libadeshlang.a` / `libadesh_runtime.rlib` entries (the root crate is
   `crate-type = ["rlib", "cdylib"]`, so an `adeshlang` static archive has never
   been producible, and an `.rlib` carries Rust bytecode, not the C runtime
   objects). The legacy `adeshlang.lib` / `libadeshlang.a` install names are
   gone too, and for the same reason: nothing can produce them, so the only
   `adeshlang.*` archive on disk is the DLL's **import library**, which the
   hashed-name search happily matched. A host link then "found" a runtime that
   defines no `aot_*` symbol and died on `undefined reference to aot_print_cstr`
   instead of the missing-archive error — found while staging the first archive
   with the item-2 script on 2026-11-03.
   `RUNTIME_LIB_CACHE` was a process-wide `OnceLock<Option<PathBuf>>` — a host
   build followed by a cross build in one process handed the cross build the
   host archive with no search path able to stop it. It is now
   `Mutex<Option<(String, PathBuf)>>` and the triple is part of the hit test
   (`cached_archive_matches`).
   Measured on a Windows host: `adesh build hello.adesh --emit=exe` finds
   `target/debug/adesh_runtime.lib`, links, prints `hello`, exits 0;
   `--target aarch64-unknown-linux-gnu` fails with "No runtime archive for
   target aarch64-unknown-linux-gnu", lists every candidate, prints
   `scripts/build-runtime-archives.sh --profile <profile> --targets <triple>`,
   and says the host archive was not considered — with a host archive present
   on disk. `adl doctor` still reports the host archive.
   `linking.rs::runtime_archive_tests` pins 7 cases: a cross build is offered
   only paths containing its own triple; a host build still reaches its build
   tree, install `lib/` and a flat `ADESH_RUNTIME_DIR`; target-scoped paths sort
   before host paths; `<triple>/release/` is searched for every triple; the
   error names the triple, prints every searched path and the build command; a
   cached host answer does not satisfy another triple; `is_host_triple` accepts
   only the real host. `is_host` is a test parameter rather than a call, so the
   cross assertions hold on an aarch64 Linux runner too.
   **Re-measured 2026-11-03 after item 2** (see item 1 note above): with every
   host and triple-scoped archive moved out of the tree, `adesh build
   --target=x86_64-pc-windows-msvc` now prints the missing-archive error rather
   than resolving an `adeshlang` import library, and with only
   `target/x86_64-pc-windows-msvc/debug/adesh_runtime.lib` present it links and
   the artifact runs (`phase12 archive probe`, exit 0). 7/7
   `runtime_archive_tests` pass.
   *Re-measured 2026-11-04:* `cargo build --lib --bin adesh` exits 0 (the
   `variables.rs` block cleared), `cargo test --lib` is **940 passed / 0 failed /
   1 ignored**, `cargo test --test native_semantics_e2e_test` **23 passed**, and
   the `repair.rs` / `doctor.rs` message changes are compile-verified.
2. **Runtime archive build tooling.** **DONE 2026-11-03/04** (packaging wired; the
   install-payload claim is verified by staging probe and an `ISCC.exe` compile,
   not by a full release build — no release build was run in this phase).
   `scripts/build-runtime-archives.sh` runs `cargo build -p adesh-runtime
   --target <triple>` per triple, normalises Cargo's hashed `deps/` copy to the
   canonical `target/<triple>/<profile>/{libadesh_runtime.a|adesh_runtime.lib}`,
   checks the `!<arch>` magic, and reports built / skipped / failed with a
   non-zero exit — one triple's failure never hides another's. `--profile`
   defaults to **debug**; `--list` prints the plan without building, and
   `--install-targets` runs `rustup target add` first. The archive-name function
   is a copy of `runtime_archive_names` with a comment naming the file it must
   track.
   Packaging now stages per-target archives:
   `scripts/release/package_unix.sh` puts the distribution's own archive at
   `lib/libadesh_runtime.a` **and** `lib/<triple>/libadesh_runtime.a`, then adds
   any `target/<triple>/<profile>/libadesh_runtime.a` already built (release wins
   over debug), and fails the package when the distribution's archive is missing.
   `scripts/release/build_windows_dist.ps1` does the same for
   `adesh_runtime.lib`, and its payload gate now accepts only
   `adesh_runtime.*` — previously it counted `adeshlang.lib`, the DLL's import
   library. Two Windows packaging bugs surfaced here:
   `Get-ChildItem "$dir" -Include "*.lib"` matched nothing at all (`-Include`
   only filters when `-Path` carries a wildcard), so a Windows distribution
   shipped **no** library payload; and the hashed `deps/` twin of the runtime
   archive would otherwise have doubled a 20 MB file inside the installer.
   `installer/windows/installer.iss` logs the per-triple archive too and no longer
   calls `adeshlang.lib` / `adeshlang.dll` the runtime library. `Dockerfile`
   stages `/opt/adeshlang/lib/<host triple>/` and makes the archive required.
   `adl repair` and `adl doctor` were both pointing at `adeshlang.lib` /
   `libadeshlang.a`: repair copied that name into the install and printed
   "Restored runtime library", and when it found nothing it ran
   `cargo build --release --lib`, which builds the root crate's cdylib and its
   import library — never a runtime archive. Repair now resolves through
   `get_static_runtime_lib`, stages the resolved archive into `lib/` and
   `lib/<host triple>/`, and builds with `cargo build -p adesh-runtime --target
   <host triple>` when nothing is present — at the profile the binary searches
   first, so a debug `adl repair` builds debug instead of forcing a release
   archive it would never have needed.
   Measured: `--profile debug --targets x86_64-pc-windows-msvc` staged
   `target/x86_64-pc-windows-msvc/debug/adesh_runtime.lib` (20,895,594 bytes) and
   `adesh build --target=x86_64-pc-windows-msvc` linked from it with every host
   archive hidden; the foreign-triple path printed the skip line and exited 1;
   the packaging block was run against a sandbox tree and produced
   `lib/libadesh_runtime.a`, `lib/<dist triple>/…` and cross-triple directories
   while leaving `target/debug/deps/` hashed copies out; the PowerShell staging
   and gate blocks were run against `target/x86_64-pc-windows-msvc/debug` and
   produced `lib\adesh_runtime.lib` + `lib\x86_64-pc-windows-msvc\adesh_runtime.lib`,
   then threw the gate message when neither existed. `installer.iss` compiles with
   Inno Setup 6 (`ISCC.exe`, 120.9 s, successful).
   *2026-11-04:* `ADESH_HOME=<temp> adl repair` printed `✓ Runtime archive staged`
   for `lib\adesh_runtime.lib` and `lib\x86_64-pc-windows-msvc\adesh_runtime.lib`
   (20,895,594 bytes each) with no release build, and `adl doctor` then reported
   `✓ Runtime Library`; `scripts/build-runtime-archives.sh --list` prints all
   8 triples at their debug paths and `--targets x86_64-pc-windows-msvc`
   re-staged the host archive (built 1 / skipped 0 / failed 0, exit 0). The
   documentation set landed with the step: `TARGET_MATRIX.md` (header, aarch64
   link and gap rows, "Next important" item 3), `ARCHITECTURE_GAPS.md` (§1.5
   import-library masquerade, §3.5 restated as "no archive exists yet", header),
   `docs/AOT_COMPILATION.md` (live resolution section, a new "Building the
   archives" subsection, blockers list, troubleshooting entry, the re-measured
   error sample, legacy name notes), `docs/RELEASE.md` (§3.1 Windows steps in
   script order with the `-Include` bug and the `[3c/6]` gate, §4.1 payload
   rules for `package_unix.sh`), `docs/cli-guide.md` ("Native runtime archives per
   target") and `CURRENT_STATE.md` §0.9.
3. **Cross-target CI.** **CODE-COMPLETE 2026-11-03, run pending** (this workflow
   is `workflow_dispatch`-only and pushing or dispatching needs explicit
   approval). `.github/workflows/ci.yml` gains a `runtime-archives` job with two
   matrix entries: `linux` on `ubuntu-22.04` for
   `x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
   riscv64gc-unknown-linux-gnu` (after `apt-get install
   gcc-aarch64-linux-gnu libc6-dev-arm64-cross gcc-riscv64-linux-gnu
   libc6-dev-riscv64-cross`, because the runtime's `build.rs` runs `cc` and
   `native_output.c` includes `<unistd.h>`), and `darwin` on `macos-latest` for
   `aarch64-apple-darwin x86_64-apple-darwin`. Each entry runs
   `bash scripts/build-runtime-archives.sh --profile debug --targets …`, collects
   `target/<triple>/debug/<archive>` into `runtime-archives/<triple>/` — the
   layout an install's `lib/<triple>/` uses — and uploads it as
   `runtime-archives-<name>-debug-<run>`.
   Two deviations from the phase text, both recorded in the workflow comment:
   the Apple archives are built on a macOS runner rather than cross-built from
   Ubuntu, because an `*-apple-darwin` `cc` invocation needs the macOS SDK; and
   the profile is debug, because this job's job is to prove each target builds,
   while release archives for distribution come from the packaging scripts.
   Verified locally: the YAML parses (`yaml.safe_load`), all 17 `run` blocks pass
   `bash -n`, and the collect loop was run against a tree holding one ELF-named
   and one COFF-named archive plus a hashed `target/debug/deps/` copy — it
   staged the two canonical archives under their triples and ignored the hashed
   one.
4. **Link-only coverage.** **DONE 2026-11-04.**
   New `tests/native_phase12_cross_link_test.rs`: compile a hello-world to
   each triple, link with `adeshlink`, and assert success plus the expected
   `aot_*` definitions resolve. Execution is Phase 13.
   Written as 8 tests, **8/8 passing on a Windows host** (`cargo test --test
   native_phase12_cross_link_test`, 27.26 s). One deviation from the item text:
   the suite drives `adesh build --target <triple> --emit=exe` rather than calling
   `adeshlink` on a hand-made object, because the defect class it had to close is
   the *whole* pipeline quietly consuming the wrong archive — a direct
   `adeshlink` invocation would have re-implemented the resolver's own search and
   proved nothing about it. Every assertion is on the produced **image bytes**:
   ELF `e_machine`/`e_type`/`PT_INTERP` and the `aot_*` definitions read out of
   `.symtab`/`.dynstr`, PE `Machine`, Mach-O `cputype`/`filetype`. A link that
   returns exit 0 with a host-architecture image, or with `aot_*` calls unbound,
   fails here. Execution is still not claimed — the one image the suite runs is the
   host-triple control (`the_host_link_consumes_its_archive_and_the_image_runs`),
   which proves the definitions those header checks read are reachable. The
   gate test prints a per-triple `LINKED`/`REFUSED` table into the CI log, and the
   two archive-dependent tests print a `SKIP:` naming
   `scripts/build-runtime-archives.sh` instead of passing silently on a fresh
   checkout.
   Measured state (Windows host, archives staged locally by the item-2 script;
   `x86_64-apple-darwin` used an existing `target/x86_64-apple-darwin/release/`
   archive, which the resolver reaches because `<triple>/release/` is searched for
   every triple by design):
   | Triple | Outcome |
   | :--- | :--- |
   | `x86_64-unknown-linux-gnu` | LINKED — ELF `e_machine` 62, 8 `aot_*` defs, 36,208 B |
   | `x86_64-unknown-linux-musl` | LINKED — ELF `e_machine` 62, 8 `aot_*` defs, 36,064 B |
   | `aarch64-unknown-linux-gnu` | LINKED — ELF `e_machine` 183, 8 `aot_*` defs, 277,416 B |
   | `riscv64gc-unknown-linux-gnu` | REFUSED in codegen, before archive resolution |
   | `aarch64-apple-darwin` | REFUSED — `No runtime archive for target …` |
   | `x86_64-apple-darwin` | REFUSED at the link on 2026-11-04 — `error[LNK009]: cannot rebase address 0x100001006`; a dyld fixup target must live in `__DATA_CONST` or `__DATA`. **LINKED since 2026-11-05** (item 5 below) |
   | `x86_64-pc-windows-msvc` | LINKED — PE `Machine` 0x8664, 47,616 B, image executes |
   | `aarch64-pc-windows-msvc` | REFUSED — `No runtime archive for target …` |
   Two findings the suite produced rather than merely pinned:
   * **AArch64 did not link until this step fixed it.** With an `aarch64-linux-gnu`
     archive staged, `adesh build --target aarch64-unknown-linux-gnu` failed
     `error[LNK001]: undefined reference to \`__str_0\`` — the shared lowering fills
     `NativeModule::string_pool` for every backend and the AArch64 encoder emits
     `ADRP`/`ADD` pairs naming `__str_<n>`, but `emit_object` only ever wrote
     `.text`; x86_64 has always emitted `.rodata`/`.tls`/`.data`. Added
     `emit_module_data_sections` to
     `crates/adesh-codegen/src/targets/aarch64/mod.rs` (same layout rules: 8-byte
     alignment for `.tls`/`.data`, null-terminated deduplicated pool, pool symbols
     `Local` and the TLS/global symbols `Global`), which takes the target from
     "refused at the link" to "links". Pinned by
     `test_aarch64_emit_object_defines_the_string_pool` in the same crate — a unit
     pin, not only the cross-OS test, because the cross test needs an archive only a
     host with a cross `gcc` can build. 7/7 `aarch64` codegen tests pass.
     The suite then keeps the ordering honest: if a future change makes the archive
     the reason again, `the_aarch64_archive_resolves_so_the_archive_is_never_the_blocker`
     fails, and if it links it prints `PROMOTED:` and names the docs to update.
   * **`x86_64-apple-darwin` failed inside the link, and it was a compiler bug.**
     Phase 12 could not close it as a packaging item; item 5 below did, the same day
     the criterion was written. It is now the named blocker in `TARGET_MATRIX.md`, and
     it sat ahead of the Phase 13 "run it under dyld" plan for that triple.
   CI wiring (follow-up to item 3): `.github/workflows/ci.yml` gains a `cross-link`
   job — `needs: runtime-archives`, all three OSes, downloads the
   `runtime-archives-*-debug-<run>` artifacts, stages each at
   `target/<triple>/debug/` (the first path the resolver searches), **fails if the
   artifact arrived with no archive in it** rather than letting the suite's `SKIP:`
   lines read as CI doing its job, builds the host archive for the control test, and
   runs the suite with `--nocapture --test-threads=1`. All three OSes run it on
   purpose: a Windows or macOS runner is where a Linux image is linked with no Linux
   kernel in sight, and a Linux runner is where a Mach-O image is linked with no
   macOS host. Local verification: YAML parses, the `run` blocks pass `bash -n`, and
   the suite passes locally against hand-staged archives. Like `runtime-archives`,
   **the job has never been dispatched** — this workflow is `workflow_dispatch`-only
   and pushing or dispatching needs explicit approval.
5. **Close the Mach-O `LNK009` blocker item 4 exposed — DONE 2026-11-05.**
   `x86_64-apple-darwin` now links a source program. The absolute pointer the writer
   refused was the compiler's: the x86_64 backend materializes a symbol address as
   `movabs r64, imm64` + `Absolute64`, correct on PE (a base-relocation directory
   records absolutes) and on this toolchain's fixed-base ELF images (measured
   `e_type` 2, `ET_EXEC` — no dynamic relocs at all), and impossible on Mach-O, where
   the writer always sets `MH_PIE` and dyld rebases only `__DATA`/`__DATA_CONST`.
   `Move { reg <- Symbol }` in `crates/adesh-codegen/src/targets/x86_64/mod.rs` now
   picks the form per image format: `lea r64, [rip + disp32]` + `PcRelative32`
   (addend −4) for Mach-O, `movabs` otherwise, so the image needs no dynamic fixup.
   Three things this step is worth recording for, beyond the target flipping to
   LINKED:
   * **A guard on the wrong field fails silently.** The first version branched on
     `TargetDescriptor::object_format`, which names the container this toolchain
     *emits* — `Adob` for every triple, darwin included — so the condition could
     never be true, the fix changed no behaviour, and the identical error came back.
     `TargetDescriptor::image_format()` now derives the image from the operating
     system, the way `crates/adesh-codegen/src/target_spec.rs` already did, and
     `image_format_comes_from_the_operating_system_not_the_emitted_container` pins
     both halves of the distinction.
   * **An error that names only the symptom misdirects.** `LNK009` quoted a rule and
     an address, so the fault looked like the writer choosing a bad section and spent
     a day documented as a linker gap (`ARCHITECTURE_GAPS.md` §3.5 carries the
     correction). Mach-O rebase records now carry the object, section, offset and
     symbol behind the address, and the refusal prints them — on this run, `.text`
     offset 0x6 in the compiler's own ADOB, pointing at `__str_0`.
   * **A passing link is not a correct pointer.** The image was decoded rather than
     trusted: `main` opens with `lea rcx, [rip+0xff5]` at `.text`+4, and `0x10000100b`
     plus `0xff5` is `0x100002000`, which is `.rodata`+0 — the program's own string
     there; the same source built for the host triple prints that string.
   Pinned by `tests/native_phase12_cross_link_test.rs::the_mach_o_image_needs_no_fixup_inside_text`
   (9/9 in the suite), which requires the link, asserts `MH_PIE` is still set so nobody
   cures the error by dropping position independence, and whose non-vacuity was checked
   by reverting the guard — it then fails with the original `LNK009`. Unit pins:
   `a_symbol_address_is_pc_relative_only_on_mach_o` (both arms, byte-exact) and the
   `image_format` test. Regression coverage: `cargo test -p adesh-codegen` (89 + 17),
   `-p adesh-object`, `-p adesh-linker` (56 plus every integration suite, including
   `macho_dyld_binding_test.rs`, which decodes the streams the changed layout records
   feed), and the two other darwin-touching suites (8 + 19). `cargo fmt` and
   `cargo clippy --all-targets` on the three crates are clean.
   What this does **not** close: the image has never been executed — a Windows host
   cannot — so the triple stays Tier 2 and Phase 13 item 4 is next for it.

### Exit criteria

**2026-11-08 verification update:** the RV64 runtime archive is now built and
staged locally (44,077,682 bytes; 104 defined `aot_*` symbols). Source linking
remains blocked by ELF relocation handling, not archive availability: compressed
and ADD/SUB fixups pass eight tests; the next observed refusal is relocation
type 60 at offset `0x10b`. CI run 42396244918
failed before producing archives. Windows packaging produced its payload but
continued after a compiler failure; fail-fast guards are now locally tested,
and a clean rebuild remains required. See `CURRENT_STATE.md` §0.13; this update
supersedes the older archive-blocked descriptions below.

- [ ] AArch64, RISC-V 64, and both Mach-O triples link a source program.
      **AArch64/Linux: met 2026-11-04** (see item 4 — it links once the archive is
      staged, and the archive is no longer the blocker for that triple).
      **`x86_64-apple-darwin`: met 2026-11-05** (item 5 — it links a source program
      into a `MH_EXECUTE`+`MH_PIE` image and only execution is missing).
      **RISC-V 64: blocked on the archive, no longer in codegen** (2026-11-07).
      `ParallelMove` and `auipc`/`addi` symbol materialization landed, so a source
      program now passes encoding and reaches the resolver; the suite was flipped
      from asserting a codegen refusal to asserting the missing-archive error
      (`riscv64gc_passes_codegen_and_stops_at_the_runtime_archive`), and the two
      `RISCV_TRIPLE` exclusions in that suite were removed. No archive for the
      triple exists on this host and none has been produced by a job that ran —
      `runtime-archives` does list `riscv64gc` and installs
      `gcc-riscv64-linux-gnu` (`.github/workflows/ci.yml:269`, `:290`), it has just
      never been dispatched. Evidence: `CURRENT_STATE.md` §0.12.
      **`aarch64-apple-darwin`: blocked on the archive**, which no job that has
      run can build — and not on the `LNK009` class, since the AArch64 backend emits
      no `Absolute64` relocation at all. Item 4 is no longer the thing holding this
      criterion open; `TARGET_MATRIX.md` names the remaining blocker per triple. The
      archives themselves are still only hand-staged: neither the `runtime-archives`
      nor the new `cross-link` job has been dispatched.
- [x] Missing archive errors name the triple, searched paths, and build command.
      **Done 2026-11-03** — measured from the real CLI, and pinned by
      `missing_archive_errors_name_the_triple_every_path_and_the_command`, which
      `cargo test --tests` already runs on both CI hosts. The command it prints is
      the item-2 script, which stages to the path that just failed to resolve;
      re-measured with every archive hidden, so nothing else could answer.
      **2026-11-04:** the same contract is now asserted through the real CLI on a
      triple nothing in this repo or the CI matrix builds
      (`aarch64-pc-windows-msvc`, in
      `tests/native_phase12_cross_link_test.rs::a_missing_target_archive_fails_with_the_resolution_error`),
      so the fix command, the profile flag and the triple among the searched paths
      cannot drift from what the resolver prints without a test failing. The test
      also says so loudly if a future change stages that archive, instead of
      asserting a stale expectation.
- [x] No foreign build can silently consume a host-arch runtime archive.
      **Done 2026-11-03** — structural, not a warning: host-scoped directories
      are not in a foreign triple's candidate list, and the memo cache is keyed
      by triple. Pinned by `cross_builds_are_offered_only_paths_that_name_their_own_triple`,
      `target_specific_paths_are_searched_before_host_paths`, and
      `a_host_archive_answer_never_satisfies_a_cross_build`; also observed live,
      with `target/debug/adesh_runtime.lib` present, an
      `aarch64-unknown-linux-gnu` build refusing it.
      **2026-11-04:** the claim is now checked on the artifact rather than on the
      resolver's internals — `a_cross_build_does_not_fall_back_to_the_host_build_tree`
      hides the target archive and asserts the failure is the resolution error, and
      `a_staged_x86_64_linux_archive_is_consumed_by_a_cross_os_link` proves a
      target-scoped archive was consumed by comparing the two loaders (`PT_INTERP`)
      of images built from the same source and the same architecture. A foreign
      build that quietly took the host archive would produce the host's header, and
      the header assertions catch that.
- [ ] Installer output includes per-target runtime archives.
      **Code-complete 2026-11-03, release build pending.** Unix,
      Docker and Windows packaging all stage `lib/<triple>/` alongside the flat
      `lib/`, fail loudly when the distribution's own archive is absent, and
      `installer.iss` now logs the per-triple copy. Verified against a sandbox
      tree and against `target/x86_64-pc-windows-msvc/debug`, and the Inno script
      compiles. Left open because no full release has been produced since — this
      phase ran debug builds only, by instruction — and because
      `dist/windows-x86_64/lib/` — which the packaging gate now requires — has
      never existed in a built distribution: the Windows `-Include` bug meant a
      Windows install shipped no libraries at all. The root lib compiles again
      (2026-11-04: `cargo build --lib --bin adesh` exit 0, `cargo test --lib`
      940 passed), so one debug-or-release packaging run is enough to close this
      criterion.

---

## Phase 13 — Execution verification matrix

**Effort:** M · **Depends on:** Phase 12

### Goal
Give every native target a CI job that runs its output and asserts behavior.

### Work items

1. **qemu user-mode harness.**
   **Linker harness implemented 2026-11-12:** shared
   `linker/tests/support/foreign_exec.rs` runs tools on Linux or Windows/WSL,
   captures output and enforces timeouts. Missing tools print `SKIP` locally
   and fail when `ADESH_REQUIRE_FOREIGN_EXEC=1` (the new independent
   `foreign-linker-execution` CI job). Windows can select a distribution
   with `ADESH_TEST_WSL_DISTRO`. QEMU itself runs inside a Linux timeout,
   including when launched through WSL. CI execution is pending.
   The AArch64 source suite now reuses this helper. RISC-V integration remains
   open. CI installs `qemu-user`; required execution prevents silent skips.
2. **AArch64 execution suite.**
   **Linker-startup subset locally verified 2026-11-12:**
   `linker/tests/phase13_aarch64_exec_test.rs` assembles real ELF input,
   links via the CLI with synthesized `_start`, and executes under QEMU.
   It asserts argc/argv/envp, environment contents, stack alignment,
   stdout and exit 37 for three argument counts. This exposed and fixed
   two wrong startup instructions (argc base register and envp shift).
   The old image SIGSEGVs; fixing just the argc load exits 84 (bad envp).
   **Source subset locally verified 2026-11-12; CI pending:**
   `tests/native_phase13_aarch64_exec_test.rs` builds print, exit, nested-call
   arithmetic and loop fixtures using the real CLI in default and fast modes.
   All eight outputs execute with asserted stdout/exit status. This uses the
   CLI's default dynamic ELF path and target libc sysroot rather than replacing
   it with a freestanding test. ELF64/AArch64/ET_EXEC and PT_INTERP are checked.
   The loop exposed reversed CSET predicates, now fixed with a failing-before
   unit regression over all condition variants. The required foreign-execution
   CI job builds the runtime archive and installs target libc. Local required
   QEMU execution passes; CI is not dispatched, so the phase remains open.
   See `CURRENT_STATE.md` §0.16 for validation and concurrent-build limitations.
3. **RISC-V execution suite.**
   `tests/native_phase13_riscv64_exec_test.rs`: same shape under
   `qemu-riscv64`.
   **Unblocked in codegen 2026-11-07, now blocked on the archive.** Phase 12's
   earlier refusal-during-encoding pin no longer applies: `ParallelMove` and
   `auipc`/`addi` symbol materialization landed (`CURRENT_STATE.md` §0.12), so the
   build reaches the runtime resolver and stops there. This suite therefore needs
   two things, not one: a `riscv64gc` runtime archive (the CI `runtime-archives`
   matrix already knows how to build one) and `qemu-riscv64`. Until the archive
   exists there is no image to run, so the suite must SKIP loudly on the archive
   as well as on the emulator.
4. **macOS link-and-run.**
   `tests/native_phase13_macho_exec_test.rs` (macOS only): `adesh build`,
   execute, assert stdout/exit; include a libSystem import (`puts`), a TLS
   access, and `--shared` dylib loaded with `dlopen`/`dlsym`.
   **Unblocked 2026-11-05.** The step that had to come first — `x86_64-apple-darwin`
   could not link a source program at all — is done: the x86_64 backend was baking
   an absolute symbol address into `.text`, which a PIE Mach-O image cannot carry,
   and now emits `lea`+PC-relative addressing there, so the triple produces a real
   `MH_EXECUTE`+`MH_PIE` image. (This was misrecorded for a day as the Mach-O
   *writer* choosing a bad section; the writer's refusal was correct, and
   `ARCHITECTURE_GAPS.md` §3.5 carries that correction.) The suite this item needs
   can now be written against an image that exists, and `linker`'s
   `the_mach_o_image_needs_no_fixup_inside_text` already pins the link so a
   regression cannot hide behind this item's absence. `aarch64-apple-darwin` still
   has no archive from any job that has run, so only the runner's native
   architecture is reachable today.
5. **PE DLL loading. Locally verified 2026-11-12; CI pending.**
   `linker/tests/phase13_pe_dll_test.rs` builds through `adeshlink --shared`,
   loads with `LoadLibraryW`, resolves `probe` with `GetProcAddress`, and
   asserts its result. A second simultaneous copy verifies absolute-pointer
   rebasing. API-level fixtures verify standard and explicit initializer
   selection, `DLL_PROCESS_ATTACH`, rejection when DllMain returns FALSE,
   and errors for missing or non-executable explicit initializers.
   Export-only DLLs now have a zero entry RVA instead of the executable's
   first-text fallback. The Windows test runs in an isolated child with a
   30-second timeout. Local result: 5 passed; the ignored child is invoked
   by its parent. Named Windows CI step added; not dispatched.
6. **Dedicated musl job.**
   Add a job or container step using musl tooling so
   `tests/native_phase8_tls_ehframe_test.rs` runs against musl explicitly
   instead of only through the host glibc loader.

### Exit criteria
- [ ] AArch64 and RISC-V binaries execute under qemu in CI with asserted output.
      AArch64 linker-startup fixture passes locally (2026-11-12); required
      `foreign-linker-execution` job wired, not dispatched. Source-program
      coverage and RISC-V execution remain open.
- [ ] macOS executes both Mach-O architectures (at least the runner's native one).
      `tests/native_desktop_exec_test.rs` and the `desktop-execution` CI job
      (macos-15-intel, macos-15) are wired with `codesign --verify`; not run
      (2026-11-13, `CURRENT_STATE.md` §0.20).
- [ ] PE DLL load + export call executes in CI. Locally verified 2026-11-12;
      Windows CI step wired, execution pending.
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
   **Partly done (2026-10-31):** ADRP + ADD with `AArch64_AdrPage21` and
   `AArch64_AddAbsLo12` relocations is emitted from codegen. Still open: the
   GOT-based variants for dynamic links.
2. **TLS sequences:** local-exec and initial-exec for ELF AArch64, including
   `TPIDR_EL0` addressing and linker relaxation.
3. **Lowering gaps:** compile the existing conformance corpus for AArch64 and
   fix each loud error in turn (parallel moves, stack arguments, aggregates,
   variadics, conversions, vector operations as scoped).
   **The data-emission half of this closed first (2026-11-04, Phase 12 item 4):**
   `emit_object` now writes `.rodata`/`.tls`/`.data`, so a corpus program links
   instead of failing `LNK001: undefined reference to __str_0`. What remains is
   instruction-level lowering — run the corpus and take the refusals in order.
   **FP/SIMD stack transfers implemented and locally executed (2026-11-13):**
   S/D/Q loads/stores honor 4/8/16-byte widths; unaligned and large frame
   displacements use a reserved address scratch. Large SP-based address
   materialization is corrected to the extended ADD form. A failing-before
   encoding regression and GNU-oracle/QEMU execution cover nine roundtrips
   and a 40-value scalar FP allocator spill case. The required CI job names
   `cargo test -p adesh-codegen --test aarch64_stack_exec`; not dispatched.
   **Vector spill lowering locally executed (2026-11-13):** aligned 16-byte
   slots, metadata-based classes, target-aware verification, distinct scratch
   reloads and result writeback. Full-register copies and x86 MOVUPS preserve
   upper lanes. AArch64 v4f32 add/sub/mul now select floating instructions;
   other unimplemented lane types for those operations fail loudly.
   Forty-vector AArch64/x86 QEMU fixtures assert every lane, with AArch64
   clobbering-call and D8..D15 preservation checks. Required CI names
   `aarch64_stack_exec` and `x86_vector_spill_exec`; not dispatched.
   **Scalar FP32 binary arithmetic locally executed (2026-11-13):**
   FAdd/FSub/FMul/FDiv select S or D instructions from their existing `size`
   field and reject unsupported widths. Two unit tests and QEMU execution
   failed before the fix. GNU-assembled references now verify 72 FP32/FP64
   result/FPSR cases, including special values and poisoned upper halves,
   plus forty-value FP32 allocator spills. Required CI names
   `aarch64_scalar_fp_exec`; not dispatched. FP negation/comparison/conversions,
   wider vector types and full ABI conformance remain open.
   See `CURRENT_STATE.md` §§0.17–0.19.
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

1. **`ParallelMove`:** ~~implement register/stack parallel moves~~
   **Done 2026-11-07** — `crates/adesh-codegen/src/targets/riscv/mod.rs` now uses
   `ParallelMoveResolver::for_riscv64()` (the same resolver x86-64 uses), with a
   t0/t1/t2 scratch pool; reserved ra/sp/gp/tp/s0 and the float register file are
   excluded. Evidence: `CURRENT_STATE.md` §0.12.
2. **Remaining lowering:** stack arguments, calls, prologue/epilogue, scalar
   FP ABI (LP64D), atomics and vector encodings reachable from the corpus.
3. **Symbol materialization:** PCREL_HI20/PCREL_LO12 pairs for addresses —
   **done 2026-11-07**, proven on linked image bytes by
   `linker/tests/riscv64_pcrel_symbol_pair_test.rs` (17 pairs reconstruct their
   symbol exactly, including the lo12 sign-extension and hi20 round-up cases, plus
   a backward reference; the A/B without the `+4` LO12 addend lands 4 bytes low).
   **Still open here:** GOT and PLT sequences for dynamic links, and linker
   relaxation correctness — the `+4` contract holds only while the pair stays
   adjacent and uncompressed, which the backend currently guarantees by emitting
   no RVC.
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
   affected instruction variant. — **done 2026-11-01** (29 variants enumerated;
   see Phase 11 item 1).
2. Fix AArch64 relocation offsets by recording emitted `BL` sites. — **done**
   (offsets since 2026-10-24/25; pinned by
   `aarch64_call_relocations_land_on_their_bl_sites` since 2026-11-01).
3. Reject RV32 at target validation. — **done 2026-11-02** (backend factory and
   linker entry-stub gate; see Phase 11 item 4).
4. Fix the Windows JIT import-path escaping and add the suite to Windows CI. —
   **done 2026-11-02** (10/10 locally, suite named in the Windows job; see
   Phase 11 item 5).
5. Realign `linker/src/target.rs` tiers and the stale companion docs. — **done
   2026-11-02** (`tier()` derived from CI verification, one advertised-target
   table feeding both CLIs, and `TARGET_MATRIX.md`, `CURRENT_STATE.md`,
   `docs/TOOLCHAIN_CAPABILITIES.md`,
   `docs/TOOLCHAIN_PLATFORM_ARCHITECTURE.md`, `ARCHITECTURE_GAPS.md`,
   `ABI_MATRIX.md` reconciled; see Phase 11 item 6).
6. Land `tests/native_phase11_codegen_loudness_test.rs` and wire it into CI. —
   **suite landed 2026-11-01, now 15 tests, all passing; CI wiring written
   2026-11-02 but not yet executed** — the workflow is dispatch-only, so the new
   steps have never run in CI and this line stays open until one does.
7. Apply the silent-fallback review gate to the RISC-V encoder. — **done
   2026-11-01.** The placeholder-register `unwrap_or` substitutions in the vector,
   load/store and atomic arms are `CodegenError`s naming the instruction and its
   function (`grep -n "unwrap_or" targets/riscv/mod.rs` is empty);
   `VectorLoad`/`VectorStore` are refused outright instead of hardcoding an `sp`
   base, since no vector address form materializes its operand; every
   `ConditionCode` variant is enumerated in `SetCc` and the branch encoder, and the
   two that used to fall through — `Parity`/`NotParity`, which emitted
   `addi rd, x0, 0` and then mapped to `Equal` — are refused; access sizes outside
   {1,2,4,8}, float stack widths outside {4,8} and `shamt` outside 0..64 are
   refused instead of defaulted; branch fixups fail on an undefined label or a
   displacement past B-type ±4096 / JAL ±1 MiB; and an unallocated virtual register
   is caught before encoding. Fixed two live miscompiles the gate surfaced:
   `Move` with an immediate source truncated a non-GPR destination register into
   `x0` (emitting a bare `addi x0, x0, imm` that discarded the value), and the
   atomic arms replaced a register address with `sp`. Proven by the 8 RISC-V tests
   in `tests/native_phase11_codegen_loudness_test.rs` and the updated
   `tests/native_phase7_riscv64_e2e_test.rs`; `ParallelMove` lowering remains
   unimplemented, which is why source-level RISC-V builds are still blocked —
   that is Phase 15 capability work, not this gate.
8. **DONE 2026-11-12** (`X86Arch` refuses every relocation with LNK008; `linker/tests/phase11_x86_handler_refusal_test.rs`). Original note: apply the same rule to `linker/src/arch/mod.rs:44` — found while writing the
   Tier-3 comments on 2026-11-02 and deliberately left open. `Arch::X86` returns
   `X86_64Arch`, so an i686 link uses the 64-bit relocation handler instead of
   failing. Either refuse it with a named error or implement an IA-32 handler;
   whichever is chosen needs its own test, because "Tier 3 and un-advertised" is
   not the same as "cannot silently produce a wrong image".

---

## 8. Open work — workspace format & lint gate (2026-10-31)

The gate itself is described in `CONTRIBUTING.md` and measured in
`CURRENT_STATE.md` §0.6. Ordered next actions, each with the command that proves it:

1. **Finish the in-flight `typeof` / `__type__` work so the tree formats.**
   `cargo fmt --all` then `cargo fmt --all -- --check` must exit 0 — re-measured
   2026-11-01 after the RISC-V loudness gate landed, it fails with **47 diffs in
   7 files**: `src/backends/wasm/compiler.rs` (41 sites, the bulk of the in-flight
   work), and one site each in `src/backends/aot/cranelift_impl/instructions/
   calls/mod.rs`, `src/backends/native/lower.rs`, `crates/adesh-runtime/src/lib.rs`,
   `src/execution/runtime_core/{exec/stmt,interpreter_core}.rs`, and
   `src/parsing/hir_lower.rs`. No Phase 11 file is among them.
2. **Clear the 25 remaining `adesh-runtime` warnings.** All of them are in
   `aot_typeof` in `crates/adesh-runtime/src/lib.rs`: 23 hand-built nul-terminated
   strings (`b"u8\0".as_ptr() as *const c_char` → `c"u8".as_ptr() …`, the conversion
   already applied across `sys_io.rs`) and 2 `collapsible_if` (an edition-2024
   let-chain fixes each without restructuring).
   Prove: `cargo clippy -p adesh-runtime --lib --all-features`.
3. **Clear the three stray warnings this round left behind.** An unused
   `is_export` binding in `src/execution/runtime_core/interpreter_core.rs`, an
   unused `doc` in `als/src/diagnostics.rs`, and four in
   `tests/native_phase9_pgo_e2e_test.rs` — two needless `mut` and two struct-update
   suffixes added to literals that already specify every field, which should simply
   be reverted.
4. **Un-silence the linker, then the root crate.** Remove
   `#![allow(clippy::all)]` from `linker/src/lib.rs` and `linker/src/main.rs`
   (it suppresses even the denied `correctness` group), then trim the `#![allow(...)]`
   block in `src/lib.rs` lint by lint. Expect a real batch of findings; do each in its
   own commit so a regression is attributable.
5. **Only then raise the CI bar.** Add `-- -D warnings` to the `Run Clippy` step in
   `.github/workflows/ci.yml` once the workspace is genuinely warning-free, so the
   gate becomes enforceable instead of aspirational.
6. **Re-verify and dispatch:** `cargo clippy --workspace --all-targets
   --all-features -j 2`, then commit, `gh workflow run CI --ref main`, and
   `gh run watch <id> --exit-status`.
