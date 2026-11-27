# Adesh Native Execution — Low-Width Architecture Implementation Plan (32 / 24 / 16 / 8-bit)

**Created:** 2026-11-22
**Baseline:** `CURRENT_STATE.md`, `TARGET_MATRIX.md`, `ARCHITECTURE_GAPS.md` (audited 2026-11-22)
**Relationship:** extends `NEXT_IMPLEMENTATION_PLAN.md` (Phases 11–21). That plan finishes the 64-bit native toolchain; this one adds sub-64-bit native targets. WASM is unchanged — it is a separate execution model, not a native CPU target.
**Status:** Phases 0–2, 4, 5, and 7 are done for the verified subsets; Phase 6 ended in an emission-tested DSP56000 backend with an explicit no-simulator decision; Phase 3 is decided (RV32 refused, Thumb PoC retained as Tier 3). Linux i686, i8086 DOS, and AVR are Tier 1 because the `low-width-execution` CI job executes them. See the 2026-11-26 outcome section below.

---

## Current implementation snapshot — 2026-11-25

### Completed and verified

- **Phase 0 contract gate:** `tests/low_width_contract_test.rs` accepts the implemented i686 backend/relocation subset and still refuses RV32 and unsupported Thumb bodies.
- **Phase 1 width model:** `PointerWidth::{U8,U16,U24,U32,U64}` is serialized through ADOB and mirrored in the linker; unknown width ids are refused.
- **IA-32 relocation handling:** `R_386_32`, `R_386_PC32`, `R_386_PLT32`, `R_386_16`, and `R_386_8` decode to width-correct handlers. Unknown relocations and unsupported PIC/GOT forms fail loudly.
- **x86 REL implicit addends:** the ELF reader reads signed 1-, 2-, 4-, or 8-byte in-object addends according to the x86/x86-64 relocation width. This fixes IA-32 call displacement (`-4`) and the same latent x86-64 defect.
- **Static ELF32 writer:** `linker/src/elf/writer32.rs` emits static non-PIE ELFCLASS32 ET_EXEC images with 32-bit headers, load segments, GNU stack, optional build-id note, symbols, and section names. Dynamic/PIE ELF32 and PE32 remain explicitly out of scope.
- **i686 backend:** 32-bit GPRs, cdecl stack arguments, EAX scalar returns, 4-byte scalar stack slots, loads/stores, arithmetic, comparisons/branches, SETcc, calls, module data sections, callee-saved preservation, and register-pressure spills execute correctly. Genuine 64-bit integer values/signatures are refused rather than truncated.
- **Runtime:** `i686-unknown-linux-gnu` is included in `scripts/build-runtime-archives.sh` and the runtime-archive CI matrix. The Linux output member is non-PIC and self-contained, using direct i386 syscalls; its archive was rebuilt and exercised locally.
- **Source execution:** `tests/native_phase22_i686_exec_test.rs` builds six source fixtures plus one refusal case through the real CLI/internal linker and runs the positive cases in default and `--fast` modes. All seven tests pass under `qemu-i386`, asserting exact stdout and exit status with empty stderr.
- **Verification:** full `cargo test -p adesh-linker`, `cargo test -p adesh-codegen`, and `cargo test --test low_width_contract_test` pass. Formatting/clippy and CI execution are tracked separately below.

### Boundaries / next work

- The verified language subset is integer/string source execution, not full i686 language or runtime support.
- Aggregates/enums, indexed memory, closures/captures, handle-vector/runtime ABI buffers, genuine 64-bit integers, and remaining hardcoded 8-byte paths are still refused or outside the verified subset. x87 `f64`, static TLS and 64-bit runtime-helper parameter marshalling are implemented (see the 2026-11-27 section below); `f32` stays refused.
- Linux i686 is **Tier 1** for the verified subset: the `low-width-execution` CI job runs it under `qemu-i386`. Windows PE32 remains Tier 3/refused.

---

## Outcome and deviations — 2026-11-26

What was built differs from the original plan in three places. These are decisions, not omissions.

| Plan item | What was decided / built | Evidence |
| :--- | :--- | :--- |
| Phase 4 target `avr-unknown-none` | Built as `avr-unknown-unknown` (ATmega328P, `avr5`), the spelling the linker's advertised list uses. ELF32 `EM_AVR` plus Intel HEX; Harvard flash/SRAM model (Profile C). | `avr_simavr_exec_test`; Tier 1 in `linker/src/target.rs` |
| Phase 5 target MSP430 or Adesh-defined 16-bit | Built **i8086 real-mode DOS** (`i8086-unknown-dos`, MZ `.EXE`/`.COM`, Profile B) instead of MSP430. Reason: DOSBox-X is an installable, deterministic emulator on `ubuntu-24.04`, so execution is CI-provable; MSP430 has no upstream emulator. | `i8086_dos_exec_test`; Tier 1 |
| Phase 6 24-bit spike | Chose **Motorola DSP56000** (24-bit words, `.LOD` output, Profile D). Outcome: **implement, emission-tested only.** No DSP56000 simulator is installable on the runner, so it stays Tier 2 and never claims execution. | `dsp56000_codegen_lod_test`; `docs/DSP56000_ARCHITECTURE.md` |
| Phase 3 RV32 | **Decision: RV32 stays refused (Tier 3).** The RV64 backend is 2000+ lines of LP64D-specific lowering; an ILP32 port needs its own backend, `_start`, and a runtime archive with no `std` target, which is a separate project. The loud refusal and its tests remain the contract. | `low_width_contract_test`, `phase11_riscv32_entry_stub_test` |
| Phase 3 ARM32/Thumb | **Decision: keep the 146-line Thumb PoC as Tier 3 with "proof-of-concept" wording**; it is not advertised as supported anywhere. Promotion requires a real Thumb-2 backend plus `qemu-arm` execution. | `TARGET_MATRIX.md`, advertised list |

Other deliverables beyond the original plan:

- `tests/low_width_differential_test.rs`: seeded straight-line i16 programs run on i686, 8086, and AVR and compared with a reference evaluator.
- `tests/low_width_conformance_test.rs`: shared fixtures and refusal checks per width.
- The i686 signed-print ABI defect (64-bit parameter read from a 4-byte slot) found by the differential suite and fixed via `aot_print_i32_narrow`.
- CI job `low-width-execution` (`ubuntu-24.04`, `ADESH_REQUIRE_FOREIGN_EXEC=1`): green on `655841f`. Apt set: `gcc-i686-linux-gnu qemu-user gcc-avr avr-libc simavr dosbox-x` (`gcc-multilib` conflicts with the cross compiler and must not be added).

### Still open

- DSP56000 execution evidence (simulator or hardware) before any Tier 1/execution claim.
- i686 language coverage: aggregates/enums, genuine 64-bit integers, indexed memory, closures, `f32`, dynamic/PIE ELF32, PE32 (design and blockers in the 2026-11-27 section below).
- 8086: 32-bit integers, FP, OS-dependent runtime. AVR: floating point, hardware validation.
- RV32 and Thumb-2 backends (decisions above; only worth starting with an owner and a `qemu-riscv32`/`qemu-arm` CI job).
- The 8 workspace targets that failed outside the low-width path are fixed (table in `CURRENT_STATE.md`); CI confirms the full set.

---

## i686 language coverage follow-up — 2026-11-27

### Implemented & verified capability matrix (i686-unknown-linux-gnu)

| Feature | Status | Evidence & scope |
| :--- | :--- | :--- |
| 32-bit integer arithmetic (`i8`/`u8`/`i16`/`u16`/`i32`/`u32`) | **Emulator executed** (verified subset) | `native_phase22_i686_exec_test` (`qemu-i386`), `low_width_differential_test` |
| x87 `f64` arithmetic & comparisons (`fucomip` NaN-aware) | **Emulator executed** (verified subset) | Frame-resident `fstp qword` at instruction boundaries prevents x87 80-bit precision drift; CFG `verify_x87_stack_discipline` enforces balanced stack; DCE preserves unused `ST(0)` pops (`native_phase22_i686_exec_test`, `low_width_differential_test`) |
| x87 `f64` parameters & `ST(0)` returns | **Emulator executed** (ABI verified) | `i686_gcc_interop_test::i686_adesh_and_gcc_bidirectional_double_abi_interop` (Adesh↔GCC `double` calls, mixed int/`double`, recursion, multi-return) |
| `f64` printing (`aot_print_f64_narrow`) | **Emulator executed** (differential fuzzed) | `low_width_differential_test::f64_print_and_arithmetic_differential_fuzz_on_i686` (68 cases: `±0.0`, subnormals, `MIN_POSITIVE`, `MAX`, `±inf`, `NaN`, 48 PRNG bit patterns) |
| `f32` | **Unsupported** (loud refusal) | Refused in `lower.rs` and `adesh-codegen` naming `i686-linux-gnu`; `f32_is_refused_on_i686`, `unsupported_i686_features_fail_loudly_with_diagnostics` |
| Static single-thread TLS (`thread_local let`) | **Emulator executed & structurally validated** (single thread only) | `.tdata`/`.tbss`, `PT_TLS`, `R_386_TLS_LE` (`gs:[0]` + Variant II offset), `set_thread_area` in `_start` when `PT_TLS` present; `native_phase22_i686_exec_test`, `i686_elf32_readelf_structural_tls_and_section_audit` (`readelf -h -l -S -s -r`) |
| Multithreaded TLS | **Unverified** | No `clone`/thread creation path exists; per-thread TLS initialization is not tested or claimed |
| 64-bit runtime helper marshalling (`aot_print_cstr`, `aot_print_i64`, `aot_print_u64`, `aot_print_bool`, `adesh_native_print_cstr`) | **Emulator executed** (listed helpers only) | `wide_helper_params` / `marshal_wide_helper_args` expand 64-bit helper params into two cdecl 32-bit slots; `sixty_four_bit_runtime_helper_parameters_are_marshalled_on_i686`, `helper_marshalling_narrow_signed_unsigned_and_mixed_widths` |
| Genuine `i64`/`u64` operations & signatures | **Unsupported** (loud refusal) | Explicitly refused in `lower.rs` & `I686_LINUX_DESC`; `genuine_64_bit_integer_is_refused`, `unsupported_i686_features_fail_loudly_with_diagnostics` |
| Aggregates, runtime handles & Rust `adesh-runtime` stdlib | **Unsupported** (loud refusal) | 64-bit `HANDLE_TAG` truncation risk + Rust staticlib libc/TLS-IE incompatibility; `i686_unsupported_by_value_large_struct_param_is_refused_loudly`, `low_width_conformance_test` |
| Static non-PIE ELF32 | **Emulator executed & structurally validated** (verified subset) | `writer32.rs` `.text`/`.rodata`/`.data`/`.bss`/`.tdata`/`.tbss`, dynamic `shstrtab` index, `R_386_{32,PC32,PLT32,16,8,TLS_LE}`; `readelf` + `qemu-i386` |
| Dynamic ELF32 / PIE ELF32 (`--shared`, `dynamic=true`) | **Unsupported** (loud refusal) | `phase11_x86_handler_refusal_test`, `unsupported_i686_features_fail_loudly_with_diagnostics` |
| PE32 (`i686-pc-windows-*`) | **Unsupported / Unverified** (loud refusal) | `PeWriter` refuses `Arch::X86` with `UnsupportedFormat`; `phase11_x86_handler_refusal_test`; no Wine/Windows execution |

Defects fixed during PR #50 hardening:
- Unused `f64` call return values previously allowed DCE to delete the `ST(0)` pop (`Move { dst: Virtual, src: Physical(XMM0) }`), leaking x87 stack slots across loop iterations; fixed in `crates/adesh-codegen/src/opt/dce.rs` and guarded by CFG-level `verify_x87_stack_discipline` (`crates/adesh-codegen/src/targets/i686/x87.rs`).
- `emit_global_load`/`emit_global_store` on 32-bit targets loaded/stored only 4 bytes for `f64` globals/TLS variables, corrupting the upper 32 bits; fixed with 8-byte two-word loads/stores (`src/backends/native/lower.rs`).
- `u32`-range untyped literals (`4000000000`) on `X86` fell through `normalize_target_int_type` as `I64` and were falsely refused; fixed alongside unsigned `int -> f64`, `f64 -> u32/u16/u8`, signed `i8/i16` cast sign-extension, and NaN-safe `float as bool`.
- `TLS_PROLOGUE` in `linker/src/elf/i686.rs` clobbered `edi` (the `mmap2` base) before copying `.tdata`, `writer32.rs` lacked `.tdata`/`.tbss` splitting, `SHF_TLS`, pure-`NOBITS` RW `PT_LOAD` handling, and dynamic `shstrtab` index calculation, and `reader.rs` lacked `R_386_TLS_LE` (17).
- `crates/adesh-codegen/src/register_alloc/mod.rs` now seeds `func.vreg_widths` into `spill_widths`, guards `Move` width propagation by `vreg_class`, and uses per-vreg resolved widths for callee-saved preservation slots.

### Deferred, with design and CI plan

These need the prerequisites listed; none can be validated locally with the standing "light local runs" constraint, so each is specified for CI.

1. **Genuine 64-bit integers on i686 (`i64`/`u64`, 64-bit handles).** Today every `I64`/`U64` literal, operation and signature is refused in `lower.rs` (`64-bit integer operations and values on i686`). Plan: (a) generalize `crates/adesh-codegen/src/legalize` (`PairLegalizeProfile`, `AddWithCarry`/`SubWithBorrow`/`CompareWithBorrow`) from 32→16 to a 64→32 profile with `slot_step = 4`, `ret = EDX:EAX`, and `adc`/`sbb` in the i686 encoder; (b) add `__muldi3`/`__divdi3`/`__udivdi3`/`__moddi3`/shift helpers to `native_output.c` (no libgcc in the freestanding link); (c) set vreg width 8 for `I64`/`U64`, remove the refusal sites, and pass 64-bit arguments as two slots using the same expansion as `marshal_wide_helper_args`; (d) differential-test against the existing seeded i16 reference evaluator widened to i64. Done when `genuine_64_bit_integer_is_refused` is replaced by positive execution tests and `wide_refused.adesh` becomes a differential case.
2. **Aggregates, strings-as-handles, arrays, enums with payload.** Blocked on two things. First, aggregates travel as `u64` runtime handles tagged with `HANDLE_TAG = 0x4000_0000_0000_0000` (`crates/adesh-runtime/src/lib.rs`); truncating them to 32 bits loses the tag, so item 1 (or a 32-bit tag) must land first. Second, the helpers (`aot_make_array`, `aot_object_*`, ...) live in the Rust staticlib, which cannot be linked into the static ELF32 image: it pulls in libc and TLS initial-exec relocations (LNK008, reloc type 18). Options: build `adesh-runtime` for `i686-unknown-linux-gnu` with a freestanding `no_std` allocator (large), or serve it from a dynamic/PIE ELF32 against glibc (item 3). CI plan: add the `i686` runtime staticlib to `scripts/build-runtime-archives.sh`, then a fixture per aggregate kind under `tests/fixtures/phase22_i686/` and the same `source_case` harness.
3. **Dynamic and PIE ELF32.** `writer32.rs` emits static `ET_EXEC` only. Needs `PT_INTERP`, `PT_DYNAMIC`, `.dynsym/.dynstr/.rel.dyn/.rel.plt`, `R_386_GLOB_DAT/JUMP_SLOT/RELATIVE/COPY` emission and PIC prologues (`call __x86.get_pc_thunk`) in the i686 backend, plus `R_386_TLS_*` general-dynamic/initial-exec handling. Execution needs the i386 loader and libc inside `qemu-i386`: install `libc6-i386` (or use `qemu-i386 -L /usr/i686-linux-gnu`, which `gcc-i686-linux-gnu` already provides with `libc6-dev-i386-cross`) and run with `ADESH_REQUIRE_FOREIGN_EXEC=1`. This also unblocks item 2's glibc route.
4. **PE32 (`i686-pc-windows-*`).** Needs a PE32 optional header and import table writer, an i686 stdcall/cdecl Windows ABI, `fs:[0x2c]` TLS, and a Windows runtime member. It cannot be execution-validated on `ubuntu-24.04` without Wine; add a `wine`-enabled CI leg (or a `windows-latest` runner) before promoting past Tier 2.
5. **RV32 and Thumb-2 backends.** Unchanged decisions above: RV32 needs its own ILP32 backend, `_start`, and a runtime archive, then a `qemu-riscv32` CI leg; Thumb-2 needs a real backend and `qemu-arm`. Start only with an owner for each.
6. **DSP56000 execution evidence.** Remains Tier 2. Needs a DSP56300-family simulator (for example the open-source `dsp56k` emulators) wired into a `ADESH_REQUIRE_FOREIGN_EXEC` test, or hardware logs, before any execution claim.

---

## 0. Scope, definitions, and feasibility

### 0.1 What "native execution" must mean here

Three different execution models exist in the codebase. Only two can carry low-width targets:

| Model | Can it run 8/16/24/32-bit code? | In scope? |
| :--- | :--- | :--- |
| In-process JIT (`crates/adesh-codegen/src/jit.rs`) | **No.** It allocates W^X host pages and executes host machine code; its own header lists only x86-64, AArch64, RISC-V 64 (`jit.rs:1-8`, factory at `jit.rs:555-556`). x86-64 cannot JIT AVR or i686 code. | No — document as a hard boundary, not a bug. |
| Cross AOT → `adeshlink` → target image → emulator | Yes. ADOB → ELF32/PE32/raw binary → `qemu-i386`, `qemu-riscv32`, `qemu-arm`, `simavr`, vendor simulator. | **Yes — this is the definition of "supported" used below.** |
| Emitted-machine-code interpreter | Yes. A small harness decodes and executes the bytes the backend emitted (not the HIR). Needed where no emulator exists. | Yes, for 16/24-bit fallback harnesses only. |

A target is done only when a checked-in test builds source, emits a target image, **runs the emitted bytes**, and asserts stdout bytes and exit status — the rule already stated in `NEXT_IMPLEMENTATION_PLAN.md` §1.1 and `TARGET_MATRIX.md:182-188`.

### 0.2 Do not conflate data width with architecture width

`i8`/`u8`/`i16`/`u16` already exist as POD data types (`docs/ABI.md:18-27`, `src/backends/native/lower.rs:75-82,151-168`) and atomics on them are tested on x86-64 (`tests/native_phase2_abi_atomic_test.rs`). That is **not** 8/16-bit architecture support. An architecture width means: pointer/address width, integer register width, legal operation widths, alignment rules, stack/frame layout, ABI, relocation widths, and image class. All of those must be modelled.

### 0.3 Feasibility and target selection (decision before coding)

| Width | Candidate target | Emulator / harness | Verdict |
| :--- | :--- | :--- | :--- |
| 32 | `i686-unknown-linux-gnu` (IA-32 SysV), `riscv32imac-unknown-none-elf` (RV32 ILP32), `armv7-*` (AAPCS32) | `qemu-i386`, `qemu-riscv32`, `qemu-arm` | **Real target.** Real hardware, real ABIs, real emulators. Do first. |
| 16 | `msp430-unknown-none-elf` (real 16-bit MCU ISA) or an Adesh-defined 16-bit bare-metal reference target | No upstream QEMU for MSP430; vendor simulator or an emitted-bytes interpreter harness | **Hard but achievable.** Requires new arch, new ELF/raw layout, new harness. |
| 8 | `avr-unknown-none` — `EmbeddedArchitecture::Avr` is already declared (`crates/adesh-object/src/target.rs:52-64`) | `simavr` (or an emitted-bytes interpreter) | **Hard.** Harvard address spaces, 8-bit stack, tiny SRAM; needs a separate memory model. |
| 24 | No mainstream general-purpose ISA. 24-bit addressing exists mainly in DSPs; 3-byte unaligned pointers have no local ABI/object convention. | None available | **Research spike only.** May legitimately end with "keep Tier 3 / declared-only". Do not promise execution until a concrete target and harness are named. |

**Ordering rule:** 32-bit first (validates the shared width model on real emulators), then 8/16-bit embedded (validates non-64-bit pointer and Harvard/address-space handling), then 24-bit last (highest uncertainty). Do not start Phase 6 before Phase 4 has produced a working emitted-bytes harness.

### 0.4 Target / emulator decisions (locked for Phase 1+)

| Width | Target triple | Execution harness | Install / CI | Notes |
| :--- | :--- | :--- | :--- | :--- |
| 32 | `i686-unknown-linux-gnu` | `qemu-i386 -L <sysroot>` | Ubuntu + `gcc-multilib` + `qemu-user` | SysV i386, stack-passed args |
| 32 | `riscv32imac-unknown-none-elf` | `qemu-riscv32 -L <sysroot>` | Ubuntu + `gcc-riscv32-linux-gnu` + `qemu-user` | Bare-metal, ILP32 |
| 8 | `avr-unknown-none` | `simavr` or emitted-bytes interpreter | Ubuntu + `simavr` (or checked-in harness) | Harvard, 8-bit registers, 16-bit PC |
| 16 | `msp430-unknown-none-elf` *(proposed)* | vendor simulator or emitted-bytes interpreter | no upstream QEMU; interpreter fallback | **Decision pending** — confirm vs Adesh-defined 16-bit reference in Phase 0 |
| 24 | none identified | none | n/a | **Spike** — may abort with loud refusal |

**Hard boundary:** the in-process JIT stays host-ISA only (`crates/adesh-codegen/src/jit.rs:7`). No low-width target can be JIT-executed on the host. Cross AOT + emulator/interpreter is the only supported path, matching the repo's existing "no capability without execution" rule.

**Quality constraints (every new backend):** forbid `unsafe`, heap-free assertion paths, minimal executable size, compile-time type/memory safety by construction.

---

## 1. Rules for every phase

Inherited from `NEXT_IMPLEMENTATION_PLAN.md` §1, plus low-width-specific additions:

1. **No capability without execution.** Emission-tested is never "supported". Assert stdout and exit status.
2. **No silent fallbacks.** Refusing a width/instruction/relocation with a structured `CodegenError`/`LNK008` is mandatory. In particular: never emit a 64-bit instruction for a 32/16/8-bit target — that is exactly the RV32 refusal reason at `crates/adesh-codegen/src/targets/mod.rs:125-135`.
3. **One phase, one documentation update.** Each phase updates `CURRENT_STATE.md`, `TARGET_MATRIX.md`, `ABI_MATRIX.md`, `docs/TOOLCHAIN_CAPABILITIES.md`, and the stale `examples/architecture_guide.adesh` in the same change set.
4. **CI is the arbiter.** Each phase names the CI job. Tier promotion only after CI runs the artifact.
5. **No cross-architecture fallbacks.** Missing sysroot/emulator/runtime archive is a loud error naming the triple and the exact install command (`NEXT_IMPLEMENTATION_PLAN.md` §1.5).
6. **No width guessing.** Every width-dependent decision comes from `TargetSpec` / `TargetDescriptor`, never from an ad-hoc `is_64bit()` check.
7. **Assertion on emitted bytes for 8/16/24-bit.** Interpreter harnesses must decode the bytes produced by the backend, with a byte-level test proving the harness is not just re-running the HIR.
8. **Workspace gate is part of DoD.** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets --all-features` no errors and no new warnings in touched crates.

**Phase DoD checklist**

- [ ] Work items complete.
- [ ] New/updated tests pass locally and in the named CI job.
- [ ] Old behavior demonstrably fails the new tests (regression proven).
- [ ] `CURRENT_STATE.md` §0 and `TARGET_MATRIX.md` row updated.
- [ ] No new silent `NOP`/`unwrap_or`/default-width fallback (grep gate).
- [ ] `cargo fmt` + `cargo clippy` clean for touched crates.

**Effort scale:** S = days, M = 1–2 weeks, L = 2–4 weeks, XL = 4+ weeks.

---

## 2. Verified baseline — the exact gaps this plan closes

| Gap | Evidence (file:line) |
| :--- | :--- |
| Native backend factory accepts only x86-64 / AArch64 / RV64 (plus WASM and an embedded stub). RV32 is explicitly refused; i686/ARM fall into the catch-all error. | `crates/adesh-codegen/src/targets/mod.rs:97-147` |
| Pointer width has only `U32` / `U64`. `default_pointer_width()` maps everything non-64-bit to 32 bits, so 24/16/8-bit cannot be represented. | `crates/adesh-object/src/target.rs:104-110,209-224`; `linker/src/target.rs:243-260` |
| ADOB serializes pointer width as 4 or 8 only — no encoding for 1/2/3-byte pointers. | `crates/adesh-object/src/writer.rs:116-120` |
| `TargetSpec` hardcodes the 64-bit arch set, defaults every other arch to 32 bits, and hardcodes ELF64/PE32+/MachO64 image formats and 16-byte stack alignment. | `crates/adesh-codegen/src/target_spec.rs:52-59,74-86,100-103` |
| Linker's ELF writer is 64-bit only (`e_ident[4] = ELFCLASS64`); `ELFCLASS32` exists only in the reader. No ELF32/PE32/raw image emission. | `linker/src/elf/writer.rs:1609`; `linker/src/elf/header.rs:5-6`; `linker/src/elf/reader.rs:134,194,218,247` |
| IA-32 linker handler refuses every relocation with `LNK008` — it does not implement IA-32 linking. | `linker/src/arch/x86.rs:21-29`; `linker/tests/phase11_x86_handler_refusal_test.rs` |
| Thumb embedded backend is a 146-line proof of concept: only `Nop`/`Return` allowed, emits `bx lr` (`0x70,0x47`) for everything it accepts. | `crates/adesh-codegen/src/targets/embedded/mod.rs:40-56,116-145` |
| No 24-bit model anywhere. All `24-bit` matches are ANSI escapes, Mach-O addends, FP mantissas, or NaN tags. | repo-wide search, 2026-11-22 |
| JIT is host-ISA only. | `crates/adesh-codegen/src/jit.rs:1-8,555-556` |
| Tier labels derive from CI verification, not from declared capability — new targets start at Tier 3. | `linker/src/target.rs:296-317`; `linker/tests/phase11_target_tiers_test.rs` |
| Target matrix already says i686/RV32/Thumb are Tier 3 with named blockers; those rows must be rewritten by the relevant phases. | `TARGET_MATRIX.md:211,214-215,233-235` |

---

## 3. Phase map and dependencies

```
Phase 0  Contract: what "supported" means; target + emulator selection   (blocks all)
   │
   ├── Phase 1  Width-aware target model (cross-cutting data model)  ──► blocks 2-6
   │
   ├── Phase 2  IA-32 / i686: backend + ELF32 link + qemu-i386 exec
   │                 │
   │                 └── Phase 3  RV32 + ARM32: prove the model generalises
   │
   ├── Phase 4  AVR 8-bit: backend + image + simulation harness
   │                 │
   │                 └── Phase 6  24-bit research spike (gated on Phase 4 harness)
   │
   ├── Phase 5  MSP430 / 16-bit reference: backend + harness
   │
   └── Phase 7  Conformance, tiers, docs, CI gate (last)
```

Phases 2, 4, and 5 can run in parallel after Phase 1. Phase 3 depends on Phase 2 only for harness conventions, not for code. Phase 6 is a spike with an explicit abort path.

---

## Phase 0 — Contract and truth gate — DONE 2026-11-22

**Status:** complete. Contract tests exist and have been updated as later phases implemented i686 behavior.

**Effort:** S · **Depends on:** none · **Blocks:** all

### Goal
Freeze the semantics of "32/24/16/8-bit native execution" before any type or enum is widened, so no phase can silently redefine success.

### Work items
1. Write the definition from §0.1 into `docs/ARCHITECTURE.md` and `docs/TOOLCHAIN_CAPABILITIES.md`: in-process JIT remains host-ISA only; low-width support means AOT image + emulator/interpreter execution.
2. Name the exact target triples, emulator binaries, and CI runners for each phase. Record install commands (e.g. `qemu-user`, `gcc-multilib`, `simavr`, `msp430-gcc`).
3. Decide the 16-bit target: real MSP430 vs Adesh-defined reference. Decide the 24-bit target or record "no viable target" as a valid Phase 6 outcome.
4. Add `tests/low_width_contract_test.rs` asserting the *negative* facts today: i686/RV32 are refused at `create_backend`, i686 relocations are refused with `LNK008`, Thumb bodies are refused. These tests must keep passing until their implementing phase, then be updated in that phase.
5. Reconcile the stale claims in `examples/architecture_guide.adesh:10-13,21-27` ("ARM32 currently supported") with `TARGET_MATRIX.md`.

### Exit criteria
- [x] Signed-off target/emulator table in `docs/ARCHITECTURE.md`.
- [x] Contract test checked in and green (`tests/low_width_contract_test.rs`).
- [x] Stale ARM32 example corrected.

### CI
Existing `ci.yml` job (no new runner needed for docs/tests).

---

## Phase 1 — Width-aware target model (cross-cutting) — DONE 2026-11-22

**Status:** complete. Makes pointer width a first-class target property expressible for U8/U16/U24/U32/U64.

**Files changed:**
- `crates/adesh-object/src/target.rs` — `PointerWidth::{U8,U16,U24,U32,U64}`; `bytes()`/`bits()`; `default_pointer_width()` is now an explicit per-architecture table (was `is_64bit()` binary).
- `crates/adesh-object/src/writer.rs:116-122` — ADOB ptr-width id = bytes (U8=1…U64=8).
- `crates/adesh-object/src/reader.rs:250-266` — ADOB reader decodes ids 1/2/3/4/8; unknown id is a loud `AdobError` (no silent U64 fallback).
- `linker/src/target.rs` — mirror `PointerWidth` enum + `bytes()`/`bits()`; `image_base()` gains arms for the new widths (placeholder until targets define real bases).
- `linker/src/object/adob.rs:50-54` — AdobV2 codec gains U8=2/U16=3/U24=4 ids (existing U64=0/U32=1 unchanged).
- `crates/adesh-codegen/src/target_spec.rs:74` — `pointer_width_bits` derived from `descriptor.pointer_width.bits()` (was hardcoded by arch).
- `tests/adob_integration_test.rs` — `adob_pointer_width_round_trips_for_all_widths` (5 widths round-trip; validation passes).

**Tests:** `cargo test --test adob_integration_test` (3/3) and `--test low_width_contract_test` (5/5) pass; `cargo fmt --all -- --check` clean; no new clippy warnings.

**Exit criteria met:** width model expressible; ADOB round-trips all widths; unknown width ids are loud errors; no silent `is_64bit()` guesses remain in width-dependent code.

---

## Phase 2 — IA-32 / i686 native execution (first 32-bit target) — DONE 2026-11-25, Tier 1 2026-11-26

**Status:** implementation complete for the static Linux integer/string subset and executed in CI by `low-width-execution` (green on `655841f`); Linux i686 is Tier 1 for that subset.

**Effort:** XL · **Depends on:** Phase 1 · **Blocks:** Phase 3 harness conventions

### Goal
`adesh build --target i686-unknown-linux-gnu` produces an ELF32 that runs under `qemu-i386` with asserted stdout and exit status.

### Work items
1. **DONE:** Dedicated 32-bit backend in `crates/adesh-codegen/src/targets/i686/`: EAX…EDI GPRs, 32-bit frames, cdecl stack arguments in 4-byte slots, EAX scalar returns, width-checked loads/stores/arithmetic/branches, SETcc, calls, parallel moves, module data, callee-saved registers, and spills. No REX or 64-bit encodings are emitted; unsupported forms fail loudly.
2. **DONE:** `create_backend` accepts `Architecture::X86` for i686, and contract tests now pin acceptance instead of the obsolete refusal.
3. **DONE for the verified scalar subset:** i686 lowering uses 32-bit scalar/pointer slots and refuses genuine 64-bit values/signatures. Aggregates and other width-sensitive ABI surfaces remain outside Phase 2.
4. **DONE:** `X86Arch` applies `Absolute8/16/32`, `PcRelative32`, and `PltRelative32`; unknown/GOT forms fail loudly. The ELF reader preserves x86 REL implicit addends at their relocation-defined widths.
5. **DONE for static non-PIE ELF32:** writer, entry stub, build-id note, parse-back tests, and executable linking. Dynamic/PIE ELF32 and PE32 remain refused.
6. **DONE:** the runtime archive script and CI archive matrix include i686. A local WSL archive was rebuilt; Linux output uses direct syscalls and a non-PIC member.
7. **DONE:** `tests/native_phase22_i686_exec_test.rs` executes hello-world, integer exit, loop/arithmetic, function calls, integer operators, and six-argument register pressure in default and `--fast` modes, plus a loud 64-bit refusal case.

### Exit criteria
- [x] More than three source programs execute under `qemu-i386` in default and fast modes with byte-exact stdout and exit status.
- [x] i686 is accepted at backend selection; the old loudness test is updated, not deleted.
- [x] ELF32 parse-back tests assert `ELFCLASS32`, `e_machine == 3`, and defined symbols.
- [x] Target documentation reflects local execution evidence; Linux i686 is Tier 2 with a pinning test. Tier 1 awaits CI execution.

### CI
The archive is in the `runtime-archives` matrix and the execution suites run in the dedicated `low-width-execution` job, which was the promotion gate and has passed.

---

## Phase 3 — RV32 and ARM32 (prove the model generalises) — DECIDED: not implemented

**Status:** RV32 remains refused and the Thumb PoC remains Tier 3; see the outcome section. The work items below are the entry criteria if this is ever resumed.

**Effort:** L · **Depends on:** Phase 2 (harness conventions) · **Blocks:** none

### Goal
A second and third 32-bit target execute, proving Phase 1's width model is not IA-32-specific.

### Work items
1. RV32 ILP32: extend `crates/adesh-codegen/src/targets/riscv/mod.rs` to emit word-width loads/stores (`lw`/`sw` not `ld`/`sd`), 32-bit register width, ILP32 call convention (`a0`–`a7`, `sp` alignment 16, `ra`). Remove the special-case refusal at `targets/mod.rs:125-135`; keep a loud error for RV64-only instructions. This is the path suggested by the existing error message at `:132-134`.
2. RV32 linker: real RV32 relocation handler (`linker/src/arch/riscv32.rs` exists) and RV32 `_start` synthesis; replace the entry-stub refusal pinned by `linker/tests/phase11_riscv32_entry_stub_test.rs`. Add ELF32 image support from Phase 2.
3. Add `tests/native_phase23_riscv32_exec_test.rs` under `qemu-riscv32` with the same three fixtures as Phase 2; assert differential parity with RV64 where the source subset overlaps.
4. ARM32/AAPCS32: either promote the Thumb PoC (`crates/adesh-codegen/src/targets/embedded/mod.rs`, currently 146 lines emitting only `bx lr`) into a real Thumb-2 backend with `qemu-arm` execution, or formally mark `thumbv7em-none-eabihf` unsupported and delete the stale examples. Do not leave it as a "supported-looking" stub.
5. Update `ABI_MATRIX.md` (AAPCS32 is currently only a declaration) and `docs/TOOLCHAIN_CAPABILITIES.md:31-46`.

### Exit criteria
- [ ] RV32 executes three fixtures under `qemu-riscv32` with asserted output.
- [ ] ARM32 is either execution-verified or explicitly Tier 3 with no "supported" wording anywhere.
- [ ] `native_phase11_codegen_loudness_test.rs` and `phase11_*_refusal_test.rs` updated to the new reality.

### CI
`foreign-linker-execution` job extended with `qemu-riscv32`; optional `arm32-execution` job if ARM32 is implemented.

---

## Phase 4 — AVR 8-bit native execution — DONE 2026-11-26 (Tier 1)

**Status:** built as `avr-unknown-unknown`; executes under `simavr` locally and in CI.

**Effort:** XL · **Depends on:** Phase 1 · **Blocks:** Phase 6

### Goal
`Embedded(Avr)` becomes a real 8-bit backend: emitted AVR bytes execute under `simavr`, with asserted output/exit.

### Work items
1. Decide memory model first: AVR is Harvard (separate flash/SRAM), 8-bit registers (r0–r31, X/Y/Z pointer pairs), 16-bit PC, small SRAM, no conventional process ABI. Document the chosen model in `docs/ARCHITECTURE.md` before encoding anything. `EmbeddedArchitecture::Avr` is already declared at `crates/adesh-object/src/target.rs:52-64`.
2. Replace the single Thumb-only `EmbeddedBackend` (`crates/adesh-codegen/src/targets/embedded/mod.rs:12-108`) with per-subarch backends or dispatch by `EmbeddedArchitecture`; keep `unsupported_instruction` loud (`:40-56`). Never emit Thumb bytes for AVR — the current code would do exactly that because `create_backend` accepts all `Architecture::Embedded(_)` (`targets/mod.rs:137`).
3. Implement 8-bit Machine IR constraints: 8-bit register class, 16-bit address pairs, no 64-bit immediates, no host-style stack frames; add a width-validation pass that refuses any Machine IR wider than the target.
4. ADOB + linker: 1-byte pointer width, AVR relocation kinds, raw-binary/ELF32-with-AVR-machine image writer, and a `_start`/`crt0` equivalent in `linker/src/`. No host runtime archive — bare-metal only.
5. Build `tests/low_width_avr_exec_test.rs` using `simavr` (or the emitted-bytes interpreter defined in Phase 0): load the produced image, run a program that writes a known byte to a designated output register/port, assert the byte and halt status. Include a byte-level test proving the harness executes backend output.
6. Keep the target Tier 3 until the CI job actually runs it.

### Exit criteria
- [ ] AVR bytes execute under the harness; exact output byte asserted.
- [ ] No Thumb encoding reachable for an AVR target (negative test).
- [ ] `TARGET_MATRIX.md` AVR row and `ABI_MATRIX.md` updated.

### CI
`avr-execution` job (Ubuntu + `simavr` or checked-in interpreter harness).

---

## Phase 5 — 16-bit native execution — DONE 2026-11-26 (Tier 1, i8086 DOS instead of MSP430)

**Status:** executes under DOSBox-X locally and in CI.

**Effort:** XL · **Depends on:** Phase 1 (and Phase 4's harness pattern) · **Blocks:** none

### Goal
The Phase 0 choice (MSP430 or Adesh-defined 16-bit reference) executes emitted machine code with asserted behaviour.

### Work items
1. Add the `Architecture` variant and triple parsing for the chosen 16-bit target; pointer width `U16`, 2-byte alignment, 16-bit register class.
2. New codegen backend under `crates/adesh-codegen/src/targets/` modelled on Phase 4 but with 16-bit registers and address space. Refuse 32/64-bit operand forms loudly.
3. ADOB `U16` pointer width (Phase 1), 16-bit relocation kinds, and ELF32/raw image emission.
4. Emulator harness: if no upstream QEMU, build the emitted-bytes interpreter from Phase 0 and prove it decodes backend bytes (not HIR) with a byte-level fixture.
5. `tests/low_width_16bit_exec_test.rs`: arithmetic + branch + memory round-trip with asserted output/halt state.
6. Tier/docs update.

### Exit criteria
- [ ] Emitted 16-bit machine code executes; asserted result.
- [ ] Harness non-vacuity proven by a deliberately corrupted byte changing the result.

### CI
`low-width-16bit-execution` job.

---

## Phase 6 — 24-bit research spike (gated) — DONE 2026-11-26 (DSP56000, emission-tested, Tier 2)

**Status:** decision record: implement the DSP56000 backend and `.LOD` writer; withhold execution claims until a simulator exists.

**Effort:** M (spike) · **Depends on:** Phase 4 harness · **May legally abort**

### Goal
Determine whether a 24-bit native target is implementable with real semantics, or must remain declared-only.

### Work items
1. Identify a concrete 24-bit target: a DSP with 24-bit addresses, or a precisely specified Adesh-defined 24-bit reference ISA (instruction encodings, 3-byte pointer format, alignment, overflow/wrap rules, endianness of a 3-byte scalar). Record it in `docs/ARCHITECTURE.md`.
2. Prototype `U24` end to end: `PointerWidth::U24` (Phase 1), 3-byte relocation width, ADOB serialization, and image layout. **Do not touch production backends until the spike passes.**
3. Extend the Phase 4/5 emitted-bytes interpreter with a 24-bit mode; execute at least one add/load/store/branch fixture.
4. Write a decision record: implement (→ a full Phase-7-style backend phase) or keep Tier 3 with the reason (no hardware, no emulator, ambiguous ABI). If aborting, add a loud refusal at `create_backend` and a test pinning it — the same honest pattern as today's RV32 refusal.

### Exit criteria
- [ ] Written decision with named target and harness, OR a loud refusal + Tier 3 row.
- [ ] No partial/placeholder 24-bit backend merged.

### CI
Spike tests in the existing workspace job.

---

## Phase 7 — Conformance, tiers, and documentation gate (last) — MOSTLY DONE 2026-11-26

**Status:** conformance + differential suites, tier table, docs, CI job, and the no-silent-fallback gate (`crates/adesh-codegen/tests/low_width_no_silent_fallback_test.rs`) are done. TODO-7.4 (8 pre-existing failing workspace targets) is fixed locally; CI confirms.

**Effort:** M · **Depends on:** Phases 2–6

### Goal
Make the repository's advertised truth match the new targets and lock it in CI.

### Work items
1. Update `CURRENT_STATE.md` §0, `TARGET_MATRIX.md` matrix + pending tables, `ABI_MATRIX.md`, `docs/TOOLCHAIN_CAPABILITIES.md`, `docs/ARCHITECTURE.md`, `ARCHITECTURE_GAPS.md`, and `examples/architecture_guide.adesh`.
2. Update `linker/src/target.rs` tier table and `ADVERTISED_TARGETS` so `adesh target list` / `adeshlink targets` show only CI-verified targets; pin with `linker/tests/phase11_target_tiers_test.rs`.
3. Add a shared conformance corpus run on every new width: hello-world, arithmetic, branching, loops, functions, arrays — mirroring `native_phase14_aarch64_conformance_test` and `native_phase15_riscv64_conformance_test` — with differential output against x86-64 where the subset overlaps.
4. Add the no-silent-fallback grep gate: no `_ => { NOP }`, no `unwrap_or` for operand widths, no default pointer width in new backends.
5. Final workspace gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features`, full `cargo test --workspace`.

### Exit criteria
- [ ] Docs agree with code and CI logs.
- [ ] Every new target is Tier 1 (CI executes) or Tier 3 (loudly refused) — no Tier 2 "emission-tested but looks supported" targets from this plan.

### CI
All new jobs from Phases 2–6 must be green in one run.

---

## 4. TODO — execution checklist

Copy this into the phase's PR description as work starts; tick only after the exit criteria above are met.

### Immediate (do these first, before any backend code)
- [x] **TODO-0.1** Decide and document the meaning of "native execution" for low-width targets (AOT + emulator/interpreter; JIT stays host-only).
- [x] **TODO-0.2** Choose exact triples + emulator binaries + CI runners for 32/16/8-bit; record install commands.
- [x] **TODO-0.3** Decide 16-bit target (MSP430 vs Adesh-defined reference) and whether 24-bit has any viable target at all.
- [x] **TODO-0.4** Add `tests/low_width_contract_test.rs`; it has been updated as i686 behavior was implemented.
- [x] **TODO-0.5** Fix stale ARM32 "supported" text in `examples/architecture_guide.adesh:10-13,21-27`.

### Phase 1 — width model
- [x] **TODO-1.1** Add `U8`/`U16`/`U24` to `PointerWidth` in both `crates/adesh-object/src/target.rs` and `linker/src/target.rs`; audit all 23 referencing files.
- [x] **TODO-1.2** Replace `is_64bit()`-based `default_pointer_width()` with an explicit per-architecture table.
- [x] **TODO-1.3** Extend triple parsing / `triple_string()` / ABI selection for new triples.
- [x] **TODO-1.4** Extend ADOB pointer-width codec + `AdobV2` with new ids; keep unknown ids loud; add round-trip tests.
- [x] **TODO-1.5** Refactor `TargetSpec` for pointer width, stack alignment, image format, new ABIs; assert width/arch agreement.
- [x] **TODO-1.6** Extend `RelocationKind` width table for 1/2/3-byte relocations.
- [x] **TODO-1.7** Add `ObjectFormat` class/raw-image declarations (writer implementation in later phases).

### Phase 2 — i686
- [x] **TODO-2.1** Create the 32-bit x86 backend for the verified scalar subset: stack args, EAX return, width-checked loads/stores/arithmetic/branches/calls, parallel moves, callee-saved registers, and spills. No REX/64-bit operands.
- [x] **TODO-2.2** Wire `Architecture::X86` into `create_backend` for i686; remove the catch-all error for that triple.
- [x] **TODO-2.3** Use pointer-width-aware lowering and 4-byte scalar slots for the verified i686 subset; refuse genuine 64-bit values/signatures. Broader aggregate/FFI layouts remain outside Phase 2.
- [x] **TODO-2.4** Implement IA-32 relocation handling and width-correct x86 REL implicit addends; refuse unknown/GOT forms loudly.
- [x] **TODO-2.5** Implement static non-PIE ELF32 image emission and 32-bit `_start`, including parse-back/build-id tests. Dynamic/PIE ELF32 and PE32 are not implemented.
- [x] **TODO-2.6** Add i686 to the runtime archive script and CI archive matrix; rebuild and exercise the local archive with non-PIC/direct-syscall Linux output.
- [x] **TODO-2.7** Add and pass `tests/native_phase22_i686_exec_test.rs` under `qemu-i386` in default and fast modes, with exact stdout/exit assertions and a 64-bit refusal case.

### Phase 3 — RV32 / ARM32
- [ ] **TODO-3.1** RV32 word-width loads/stores, ILP32 convention, remove `create_backend` refusal.
- [ ] **TODO-3.2** RV32 relocations + ELF32 `_start`; update the entry-stub refusal test.
- [ ] **TODO-3.3** Add `tests/native_phase23_riscv32_exec_test.rs` under `qemu-riscv32`.
- [x] **TODO-3.4** Promote or formally retire the Thumb PoC — no ambiguous stub.
- [ ] **TODO-3.5** Update `ABI_MATRIX.md` AAPCS32 row.

### Phase 4 — AVR 8-bit
- [x] **TODO-4.1** Document AVR Harvard memory model and bare-metal ABI.
- [x] **TODO-4.2** Split `EmbeddedBackend` per subarchitecture; guarantee no Thumb bytes for AVR.
- [x] **TODO-4.3** Add 8-bit register class and width-validation refusal pass.
- [x] **TODO-4.4** AVR relocations + raw/ELF32 image + `crt0`.
- [x] **TODO-4.5** Add `tests/low_width_avr_exec_test.rs` with `simavr`/interpreter and non-vacuity check.

### Phase 5 — 16-bit
- [x] **TODO-5.1** Add chosen 16-bit arch/triple, `U16` pointer, 2-byte alignment.
- [x] **TODO-5.2** Implement 16-bit backend; refuse wider operand forms loudly.
- [x] **TODO-5.3** 16-bit relocations + image emission.
- [x] **TODO-5.4** Emulator/interpreter harness + `tests/low_width_16bit_exec_test.rs`.

### Phase 6 — 24-bit
- [x] **TODO-6.1** Specify a concrete 24-bit target/ISA or record "no viable target".
- [x] **TODO-6.2** Spike `U24` through ADOB + 3-byte relocations (no production backend yet).
- [x] **TODO-6.3** Execute one 24-bit fixture in the interpreter OR add a loud `create_backend` refusal + Tier 3 row.

### Phase 7 — close out
- [x] **TODO-7.1** Update all status docs and target lists/tiers.
- [x] **TODO-7.2** Add shared low-width conformance corpus with differential output.
- [x] **TODO-7.3** Add no-silent-fallback grep gate.
- [ ] **TODO-7.4** Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features`, `cargo test --workspace`.

---

## 5. Risks and open questions

| Risk / question | Mitigation |
| :--- | :--- |
| **No real 24-bit target.** Most likely outcome. | Allow Phase 6 to abort honestly: loud refusal + Tier 3, matching the current RV32 pattern. Do not invent an ISA just to claim support. |
| **8/16-bit "execution" depends on obscure emulators.** | Phase 0 must name the emulator before coding; if none exists, build an emitted-bytes interpreter and prove non-vacuity. |
| **Harvard architecture (AVR) breaks the current single-address-space object model.** | Decide the memory model in Phase 4 item 1 before encoding. May need separate flash/data sections in ADOB. |
| **Cross-cutting `PointerWidth` change touches 23 files.** | Phase 1 is a dedicated phase with exhaustive-match compiler errors as the audit tool; no feature work in the same PR. |
| **ADOB codec compatibility.** Adding width/relocation ids must not downgrade unknown values (past bug: `TARGET_MATRIX.md:124-130`). | Round-trip tests for every new id; unknown ids must return `LNK008`. |
| **Runtime archives for new triples.** Cross-target resolution is already loud and triple-scoped (`TARGET_MATRIX.md:273-291`); extend the existing script, do not add host fallbacks. | Phase 2 item 6; same pattern for other triples with a runtime. |
| **A 32-bit backend cannot simply be derived from x86-64.** REX prefixes, 64-bit immediates, and register numbering differ fundamentally. | Dedicated i686 backend; reuse only encoder patterns, never encoding tables verbatim. |
| **Docs drift.** Several existing docs are already stale (`ARCHITECTURE_GAPS.md:31`, `TARGET_MATRIX.md:208-215`, `examples/architecture_guide.adesh`). | Rule 3 and Phase 7; treat contradictions as bugs. |

---

## 6. Verification commands used for this plan

```bash
rg -n "Unsupported target architecture|RiscV32|Architecture::Embedded" crates/adesh-codegen/src/targets/mod.rs
rg -n "PointerWidth|U32|U64" crates/adesh-object/src/target.rs linker/src/target.rs
rg -n "ptr_width_id|ELFCLASS64|ELFCLASS32" crates/adesh-object/src/writer.rs linker/src/elf
rg -n "24-bit|24 bit" --glob '*.rs' --glob '*.md'
cargo test -p adesh-object -p adesh-codegen -p adesh-linker
```
