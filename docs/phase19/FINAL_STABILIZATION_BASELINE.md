# Phase 19 Final Stabilization Baseline

**Recorded:** 2026-11-28T10:38Z  
**Repository:** `adeshlang/adeshlang`  
**Branch:** `cursor/phase-19-runtime-reliability-38d6`  
**Pull Request:** [#51](https://github.com/adeshlang/adeshlang/pull/51) (`OPEN`, `mergeable: MERGEABLE`, base: `main`)  
**Head Commit:** `70a7907da301864c0f2730330541d6d29d1222f7` (`70a7907`)  
**Merge-base with `origin/main`:** `1b77173dc233bc7b0798445757072691e61caa48` (`HEAD..origin/main` is empty)

---

## 1. CI Run Baseline

- **Latest GitHub Actions Run:** [`46132182980`](https://github.com/adeshlang/adeshlang/actions/runs/46132182980) (`CI`, `status: completed`, `conclusion: failure`, `headSha: 70a7907da301864c0f2730330541d6d29d1222f7`)

| Job Name | Job ID | Conclusion | Notes |
|---|---|---|---|
| `Light checks` | `140939553801` | **FAILURE** | Failed on `Check formatting` (`cargo fmt --check`); downstream host smoke steps (`Phase 7`, `Phase 18`, `Phase 19`) did not run. |
| `Workspace checks` | `140939553920` | **SUCCESS** | `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D clippy::correctness`, `--lib` tests, and ecosystem builds passed. Note: does not run `--tests` or `-- -D warnings`. |
| `cargo audit` | `140939553931` | **SUCCESS** | Advisory DB check passed. |
| `Mach-O dyld gate (x86_64-apple-darwin)` | `140939553922` | **SUCCESS** | Structural Mach-O + Phase 13/16/18 dyld execution passed. |
| `Mach-O dyld gate (aarch64-apple-darwin)` | `140939553991` | **SUCCESS** | Structural Mach-O + Phase 13/16/18 dyld execution passed. |
| `Windows PE gate` | `140939553986` | **FAILURE** | Failed compiling `adesh-runtime` (`error[E0277]` in `crates/adesh-runtime/src/unwind_abi.rs:322`). |
| `Low-width targets (i686, 8086, AVR, DSP56000)` | `140939554112` | **FAILURE** | `i686` and `AVR` passed; `i8086_dos_exec_test` failed (`i8086_32bit_legalized_exe_com_and_fast_build_and_execute`). |
| `Desktop native execution (${{ matrix.target }})` | `140939554818` | **SKIPPED** | Gated on `github.event_name == 'workflow_dispatch'`. |
| `AArch64 + RISC-V native execution (QEMU)` | `140939555192` | **SKIPPED** | Gated on `github.event_name == 'workflow_dispatch'`. |
| `musl TLS execution (dedicated)` | `140939555075` | **SKIPPED** | Gated on `github.event_name == 'workflow_dispatch'`. |
| `Checks (${{ matrix.os }})` | `140939555078` | **SKIPPED** | Gated on `github.event_name == 'workflow_dispatch'`. |
| `Runtime archives (${{ matrix.name }})` | `140939555152` | **SKIPPED** | Gated on `github.event_name == 'workflow_dispatch'`. |
| `Cross link (${{ matrix.name }})` | `140939555204` | **SKIPPED** | Gated on `github.event_name == 'workflow_dispatch'`. |

---

## 2. Active Failures & Local Baseline Issues

### 2.1 CI: `Light checks` (`140939553801`) — Formatting
- **Classification:** Formatting issue
- **Command:** `cargo fmt --check` (and `cargo fmt --all -- --check`)
- **Error:** Formatting diffs across 32 files (including `crates/adesh-codegen/src/{opt/verifier.rs,stdlib_builder.rs,targets/dsp56000/mod.rs,targets/i8086/mod.rs,targets/x86_64/mod.rs}`, `crates/adesh-runtime/src/{abi.rs,lib.rs,native_abi.rs,unwind_abi.rs}`, `linker/src/{avr/mod.rs,gc.rs,layout.rs,linker.rs,unwind.rs}`, `src/{backends/common/lir/lower/expressions.rs,backends/jit/native/runtime_bridge.rs,backends/native/lower.rs,parsing/cfg_borrow/*.rs,parsing/compile_time_memory_safety/lifetime.rs,typesystem/layouts/field_layout.rs}`, and `tests/{avr_simavr_exec_test.rs,dsp56000_codegen_lod_test.rs,support/dsp56k_vm.rs,i8086_dos_exec_test.rs,native_phase19_*.rs,phase19_differential_parity_test.rs}`).
- **Reproduction:** `cargo fmt --all -- --check`

### 2.2 CI: `Windows PE gate` (`140939553986`) — Windows `adesh-runtime` Build Failure
- **Classification:** Code defect (Windows-conditional `Send`/`Sync` bound)
- **Failing steps:** `Build the host runtime`, `Phase 18 source extern C (msvcrt puts)`, `Phase 18 source TLS (TEB)`, `Phase 19 Windows x64 SEH execution & structural verification`
- **Relevant source files:**
  - `crates/adesh-runtime/src/unwind_abi.rs` (around lines 217–221 and 309–325)
  - `crates/adesh-runtime/src/lib.rs` (line 76: `unsafe impl<T: Send> Sync for RuntimeMutex<T> {}`)
- **Exact error messages:**
  ```text
  error[E0277]: `*const Win64ExceptionRecord` cannot be sent between threads safely
     --> crates\adesh-runtime\src\unwind_abi.rs:322:27
      |
  322 |     static RESUME_STATES: RuntimeMutex<BTreeMap<u64, ResumeState>> =
      |                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  note: required because it appears within the type `Win64ExceptionRecord` (field `exception_record`)
  note: required because it appears within the type `ResumeState` (field `exc_record`)

  error[E0277]: `*const libc::c_void` cannot be sent between threads safely
     --> crates\adesh-runtime\src\unwind_abi.rs:322:27
  note: required because it appears within the type `Win64ExceptionRecord` (field `exception_address`)
  ```
- **Reproduction:** `cargo build -p adesh-runtime` on `x86_64-pc-windows-msvc` (or `cargo check -p adesh-runtime --target x86_64-pc-windows-msvc`).

### 2.3 CI: `Low-width targets` (`140939554112`) — i8086 32-bit Legalization Output Mismatch
- **Classification:** Code defect (i8086 32-bit arithmetic/comparison legalization or codegen)
- **Failing step:** `Execute 8086 DOS programs under DOSBox-X`
- **Relevant source files:**
  - `tests/i8086_dos_exec_test.rs` (line 351)
  - `crates/adesh-codegen/src/targets/i8086/`
  - `crates/adesh-codegen/src/legalize/` (or `PairLegalizeProfile::i8086`)
- **Exact error message:**
  ```text
  thread 'i8086_32bit_legalized_exe_com_and_fast_build_and_execute' panicked at tests/i8086_dos_exec_test.rs:351:9:
  assertion `left == right` failed: mismatch in W_EXE.TXT
    left: "105000\n-40000\n699678\n10390\n123456\n3\n3\n2"
   right: "105000\n-40000\n699678\n10309\n123456\n3\n1\n2"
  ```
  Line 4 actual `10390` vs expected `10309`; line 7 actual `3` vs expected `1`.
- **Reproduction:** `ADESH_REQUIRE_FOREIGN_EXEC=1 cargo test --no-fail-fast --test i8086_dos_exec_test -- --nocapture --test-threads=1`

### 2.4 Local Workspace Test Failure: `tests/day1_regressions.rs`
- **Classification:** Test defect / CLI contract mismatch (pre-existing on `origin/main` `1b77173dc`)
- **Failing test:** `tests/day1_regressions.rs::regression_cli_run_paths_do_not_hang`
- **Relevant source files:**
  - `tests/day1_regressions.rs` (line 193)
  - `src/main.rs`
- **Exact error message:**
  ```text
  thread 'regression_cli_run_paths_do_not_hang' panicked at tests/day1_regressions.rs:193:9:
  command failed: ["run", "--bytecode", "/tmp/adesh_day1_hang_regression_1795859224359655017.adesh", "--verbose"]
  stderr:
  error: file not found: '--bytecode'
  ```
- **Reproduction:** `cargo test --test day1_regressions`

### 2.5 Clippy (`-D warnings`) Baseline
- **Classification:** Code quality / lint warnings across `adesh-runtime`, `adesh-codegen`, `adesh-linker`, `adesh-monitor-tui`, and `adeshlang` (plus `missing_lints_inheritance` manifest warnings in 8 crate `Cargo.toml` files).
- **Reproduction:** `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- **Note:** Currently CI `Workspace checks` only passes `-D clippy::correctness`, whereas Phase 1 and Phase 12 require `-D warnings` to pass cleanly.

---

## 3. Documentation vs Implementation Snapshot (Pre-Stabilization)
- `CURRENT_STATE.md`, `TARGET_MATRIX.md`, `NEXT_IMPLEMENTATION_PLAN.md`: Currently claim Phase 19 is complete / Windows SEH is CI-wired, whereas CI run `46132182980` proves Windows SEH has never yet compiled or executed on Windows (`unwind_abi.rs:322`), `cargo fmt --check` fails, and `i8086` has an active 32-bit output regression (`10390` vs `10309`, `3` vs `1`).
- `docs/RUNTIME_ABI.md`: Describes `AdeshString`/`AdeshSlice`/`AdeshArray`/`AdeshOptionU64`/`AdeshResultU64`/`AdeshClosure`/`AdeshFatPointer` and `libadesh_std.adob` at a high level; does not yet document Phase 19 unwind ABI (`ALSD v1`, `adesh_eh_personality`, `adesh_win64_seh_handler`), tracked allocation/handle-tagging (`HANDLE_TAG_MASK`), or runtime bounds/FFI safety invariants.
- `docs/MEMORY_SAFETY.md` and `docs/cfg-design.md`: Have status notes acknowledging earlier draft claims; need updates reflecting the actual Phase 19 CFG/NLL/cleanup/runtime-boundary implementation and its verified boundaries.
- `docs/TOOLCHAIN_CAPABILITIES.md`: Last updated 2026-11-27 (`i686` update); needs reconciliation after Phase 19 stabilization.
