# Adesh Native Target Support Matrix

**Generated:** 2026-10-24 (native codegen + linker audit)
**Canonical status source:** `CURRENT_STATE.md` — reconciled in the same change
set as this audit; the two files agree.
**Verification rule:** a target is **execution-verified** only when a checked-in
test or CI job builds an artifact and runs it, asserting behavior. Emitting
bytes that nothing runs is **emission-tested**, not support. Tier labels in
`linker/src/target.rs` are capability declarations, not verification claims.

---

## Matrix

| Target | Object format | Codegen | Linker / relocations | Execution verification | Pending / next blocker |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | PE32+ | **Native** — 64-bit GPR + scalar SSE2, XMM0-13, stack args, Win64 shadow space, callee-saved, atomics, Win64 TLS | **Native PE** — imports/IAT, TLS dir, base relocs, ASLR, DLL export tables; ABS64 + PC32 | **Execution-verified** — PE executables run incl. a real `msvcrt!puts` import (`linker/tests/pe_windows_e2e_test.rs`, `tests/native_x86_64_e2e_test.rs`, phase 2-8 suites) | Source-level `extern`/variadic/aggregate/atomic/TLS syntax; vector ABI; PE **DLL** load + export call test (only emission is tested today) |
| `x86_64-unknown-linux-gnu` | ELF64 | **Native** — same subset as Windows (SysV variant) | **Native ELF** — static + dynamic, PLT/GOT, JUMP_SLOT, COPY, GOTPCREL, PIE `RELATIVE`, TLS LE/IE, `.eh_frame`/`.eh_frame_hdr`, `_start` synthesis, GNU/COFF archives, GC + ICF | **Execution-verified** — static ELF (`tests/native_phase3_linux_e2e_test.rs`), dynamic ELF and a `dlopen`/`dlsym` shared library (`tests/native_phase8_dynamic_elf_test.rs`), TLS LE/IE (`tests/native_phase8_tls_ehframe_test.rs`). Minimum GLIBC baseline: **GLIBC 2.35 (Ubuntu 22.04 LTS)** | Runtime unwinder test (`.eh_frame` is decoder-verified only); source-level ABI surface; cross-target runtime archive for non-host triples |
| `x86_64-unknown-linux-musl` | ELF64 (static + dynamic) | **Native** — same x86-64 subset | **Native ELF** — same relocations; TLS TPOFF64 | **Execution-verified in WSL** — static local-exec and dynamic initial-exec TLS binaries with asserted exit codes against the real musl `ld.so` | **No dedicated musl CI job** — CI runs the same suites against host glibc; add a musl container/runner |
| `aarch64-unknown-linux-gnu` | ELF64 | **Implemented but unsafe** — AAPCS64, X19-X28/D8-D15 preservation, scalar FP, NEON, atomics; **unsupported `MachineInst` variants silently emit a NOP** | **Structural only** — linker resolves CALL26/ADRP/ADD_LO12, but `emit_object` emits **only** `AArch64_Call26`, no ADRP/ADD_LO12 symbol materialization, and relocation offsets use an approximate function offset (all 7 call relocations in a hello object landed at `0x0`) | **None** — no qemu/hardware run; static ELF structure tested (`linker/tests/platform_native_linking_conformance_test.rs`) | 1) replace NOP fallthrough with a loud `CodegenError`; 2) emit call-site relocation offsets; 3) emit ADRP/ADD_LO12; 4) ship an `aarch64` runtime archive (source builds currently fail at link with undefined `aot_*` symbols); 5) qemu-aarch64 execution CI |
| `aarch64-apple-darwin` | Mach-O 64 | **Implemented** — AAPCS64 path | **Native Mach-O** — BRANCH26, PAGE21, PAGEOFF12, TLVP, ADDEND; TLS is emitted as `__thread_data`/`__thread_bss` plus a `__thread_vars` descriptor table; dyld bind/rebase, exports trie, stubs/GOT parse-back tested | **None** — `linker/tests/macho_dyld_binding_test.rs` decodes the dyld structures but never runs under dyld | Dedicated link-and-run on the macOS CI runner; TLV execution; code-signature acceptance by real dyld |
| `x86_64-apple-darwin` | Mach-O 64 | **Native** — x86-64 subset | **Native Mach-O** — x86_64 SIGNED/BRANCH/GOT/TLV relocations, libSystem routing, runtime archive symbol lookup | **Indirect only** — `tests/static_imports_test.rs` `--aot` path executes Mach-O binaries on macOS CI; no dedicated link-and-run or bind/rebase runtime assertion | Dedicated macOS execution test; runtime verification of `__stubs`/lazy binding |
| `riscv64gc-unknown-linux-gnu` | ELF64 | **Implemented, not reachable** — RV64GC LP64D, scalar FP, atomics, vector encodings, RVC alignment; loud on unsupported instructions, but real source builds stop at `unsupported machine instruction ParallelMove` | **Native ELF** — CALL/JAL/BRANCH/HI20/LO12_I/LO12_S/PCREL_HI20/PCREL_LO12, `_start` synthesis, PLT/GOT; static structure tested | **None** | Implement `ParallelMove` and the rest of lowering needed by real programs; qemu-riscv64 execution CI; target runtime archive |
| `riscv32imac-unknown-none-elf` | ELF32 | **Not implemented** — the RV backend explicitly refuses RV32 ("only implements RV64 … refusing to emit RV32-invalid code") | Linker routes the target through the **RV64** `_start` stub (wrong for RV32) | **None** | Either implement ILP32 lowering + a real RV32 entry, or stop advertising the target |
| `wasm32-unknown-wasi` | WASM binary | **Real compiler backend** — emits `env.print_f64`, `env.print_str`, `env.alloc`, `env.concat2`, `env.file_write_str`; exports `main`/`malloc`/`memory`; executed via a generated Node loader | **Linker writer is a stub** — hardcoded `() -> i64` type, one function, no import section, relocations never applied; `adesh build` for WASM routes through it | **Compiler path verified** (`tests/static_imports_test.rs`); **linker path unverified** | Pick one path (compiler-direct or linker); implement a real WASM writer (imports, memory layout, data segments, relocations, function table) or route builds around it. **The import namespace is `env.*`, not `wasi_snapshot_preview1`** — the `wasi` in the triple overstates what is emitted |
| `wasm32-unknown-unknown` | WASM binary | **Real compiler backend** — same as above | **Linker writer is the same stub** | **Compiler path verified** via Node loader | Same as `wasm32-unknown-wasi`, minus the WASI naming question |
| `thumbv7em-none-eabihf` | ELF32 | **Proof-of-concept** — 146-line Thumb-2 ALU subset only | Embedded library modules (linkscripts, IVT, crt0, flash/HEX/SREC writers) are unit-level only; no end-to-end firmware link | **None** | Either build an end-to-end firmware link + qemu/hardware test, or mark the target unsupported |
| `nvptx64-nvidia-cuda` | ADOB kernel package | **None** — source text re-embedded | Container packaging only | **None** | Unchanged scaffolding |
| `amdgcn-amd-amdhsa` | ADOB kernel package | **None** — source text re-embedded | Container packaging only | **None** | Unchanged scaffolding |
| `quantum-circuit-qasm3` | QIRB container | **None** — no language-level `qubit` construct | Container embed | State-vector simulator + QASM 3.0 export only | Unchanged; no hardware backend |

---

## Pending by target (summary)

| Target | Blocking gap | Severity |
| :--- | :--- | :--- |
| `aarch64-unknown-linux-gnu` | Silent NOP fallback for unsupported instructions; call-site relocations all at offset 0; only CALL26 emitted; no target runtime archive; no execution test | **Critical** — produces wrong code silently |
| `x86_64-pc-windows-msvc` | PE DLL execution test; source-level ABI surface; vector ABI | Medium |
| `x86_64-unknown-linux-gnu` | Runtime unwinder test; cross-target runtime archives; source-level ABI surface | Medium |
| `x86_64-unknown-linux-musl` | Dedicated musl CI job | Low |
| `aarch64-apple-darwin` | Any execution at all under dyld; TLV execution | High |
| `x86_64-apple-darwin` | Dedicated execution test (today only indirect via static-import AOT) | High |
| `riscv64gc-unknown-linux-gnu` | `ParallelMove`/lowering gaps block real source builds; no execution test | High |
| `riscv32imac-unknown-none-elf` | Codegen refuses RV32; RV64 entry stub | High — advertised but not functional |
| `wasm32-unknown-wasi` / `-unknown-unknown` | Linker writer stub; `wasi` naming does not match emitted `env.*` ABI | High |
| `thumbv7em-none-eabihf` | No end-to-end firmware link or execution | Medium (PoC) |
| GPU / quantum | Unchanged scaffolding | — |

## Next important (ordered)

The phase-by-phase execution plan for these items is
`NEXT_IMPLEMENTATION_PLAN.md` (Phases 11–21).

1. **AArch64 correctness gate** — replace the `_ =>` NOP fallback in
   `crates/adesh-codegen/src/targets/aarch64/mod.rs` with a loud
   `CodegenError`, and fix ADOB relocation offsets to the actual call sites.
   Silent miscompiles are the only defect class here that can corrupt programs
   without a diagnostic.
2. **Execution verification for Mach-O** — link and run a produced binary on
   the macOS CI runner for both `aarch64-apple-darwin` and
   `x86_64-apple-darwin`, including a dyld-bound import and a TLV access. Every
   structural test passing means nothing until dyld accepts the image.
3. **Target runtime archives** — resolve or build `target/<triple>` runtime
   archives for AArch64/RISC-V/Mach-O so source-level cross builds stop failing
   at link on undefined `aot_*` symbols.
4. **RISC-V lowering completion** — start with `ParallelMove`; then add
   qemu-riscv64 execution to CI.
5. **WASM unification** — choose compiler-direct or a real linker writer
   (imports, memory layout, data segments, relocations, function table), then
   add a wasmtime execution test for linker output. If `wasm32-unknown-wasi`
   stays, emit real `wasi_snapshot_preview1` imports.
6. **Source-level ABI surface** — `extern`, variadic functions, aggregate
   parameters, atomics, and TLS syntax, so the Phase 2 MachineIR/FFI features
   become reachable from Adesh source.
7. **Shared-library execution** — PE DLL load + export call and Mach-O dylib
   load. ELF is already verified through `dlopen`/`dlsym`.
8. **Windows CI gap** — `tests/native_phase8_jit_native_conformance_test.rs`
   is not in the Windows job and 3 of its tests fail there because generated
   import statements embed unescaped Windows paths (OS error 123). Fix the
   escaping, then add the suite to Windows CI.
9. **Realign target tiers** — `linker/src/target.rs` marks all
   x86-64/AArch64/WASM targets `Tier1Supported`; verification does not support
   that. Reconcile the tier labels with execution evidence.

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
* **RISC-V "Proof-of-concept"** — the backend is broader than that (RV64GC, FP,
  atomics, vectors), but real source builds fail on `ParallelMove`, so neither
  label is accurate; treat it as "implemented backend, unreachable from source".
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
