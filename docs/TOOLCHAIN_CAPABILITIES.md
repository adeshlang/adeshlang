# Adesh Toolchain Capabilities Matrix

**Generated:** 2026-10-05 (Phase 0 documentation truth-reset revision)
**Last updated:** 2026-11-02 (Phase 11 tier realignment: the *CI tier* column is
the tier `linker/src/target.rs` reports, and `adesh target list` /
`adeshlink targets` print exactly those rows from the same table)
**Canonical sources:** `CURRENT_STATE.md` (status) and
[`TARGET_MATRIX.md`](../TARGET_MATRIX.md) (per-target verification evidence).
This page must agree with both; earlier copies of this file claiming
"Production Ready" status for every target were stale and have been replaced.

---

## Toolchain Capabilities by Target

Honest per-target status. "Debug" means what the link path actually emits:
only section stripping (the DWARF 5 / CodeView generators exist but are
never invoked and emit a single DIE / two records). "LTO" in this table
means what `--lto` does today: section GC + ICF + stripping (no
cross-module IR optimization).

The **CI tier** column is `Target::tier()` from `linker/src/target.rs` and
reports *verification*, not capability: **1** = CI builds an artifact for the
target and runs it with asserted output or exit status; **2** = CI emits and
parses the artifact back but never executes it; **3** = declared only — no
compiler backend, or a link path that CI never runs.

| Target | Codegen | Object (ADOB) | Linker | Runtime | ABI | Debug | LTO | CI tier | Status |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **x86_64 Windows (MSVC)** | **Native (integer + scalar SSE2 FP subset)** | **v1.0** | **Native PE (execution-tested)** | **Native** | **Win64 regs + stack args + shadow + scalar FP** | **Strip only** | **GC/ICF only** | **1** | **Working native pipeline — PE executables run in CI, including a real `msvcrt!puts` import** |
| **x86_64 Linux (GNU)** | **Native (integer + scalar SSE2 FP subset)** | **v1.0** | **Native ELF (static + dynamic, PLT/GOT, TLS, `.eh_frame`)** | **Native** | **SysV regs + stack args + scalar FP** | **Strip only** | **GC/ICF only** | **1** | **Executes in CI** — static ELF, dynamic ELF plus a `dlopen`/`dlsym` shared library, and TLS exit codes are asserted |
| **x86_64 Windows (MinGW flavour)** | **Native (same x86-64 subset)** | **v1.0** | **Same PE32+ writer as `-msvc`** | **Native** | **Win64** | **Strip only** | **GC/ICF only** | **2** | **No CI evidence of its own** — every suite that links and *runs* a PE asks for `x86_64-pc-windows-msvc`; nothing requests a `-gnu` triple, so this flavour cannot claim Tier 1 |
| **x86_64 Linux (Musl)** | **Native (same x86-64 subset)** | **v1.0** | **Native ELF** | **Native (musl `ld.so`)** | **SysV + TPOFF64** | **Strip only** | **GC/ICF only** | **2** | **Executed manually in WSL against the real musl loader; no dedicated musl CI job**, so it is emission-tested for CI purposes |
| **AArch64 Linux** | **Implemented, loud about what it cannot encode** — AAPCS64, scalar FP, NEON, atomics; since 2026-11-01 every unencodable variant or operand form is a `CodegenError`, never a `NOP` placeholder | **v1.0** | **Native ELF (static) — ADRP/ADD symbol materialization and per-call-site relocations** | **Native** | **AAPCS64 regs + FP/SIMD regs** | **Strip only** | **GC/ICF only** | **2** | **Structure-verified only.** No `aarch64` runtime archive, so source builds fail at link (Phase 12); FP/SIMD stack access is refused rather than encoded; no qemu/hardware run |
| **AArch64 / x86_64 macOS** | **Implemented** (AAPCS64 / x86-64 subset) | **v1.0** | **Native Mach-O — dyld bind/rebase, exports trie, stubs/GOT; TLS descriptor table** | **Native** | **Partial** | **Strip only** | **GC/ICF only** | **2** | **Emission- and parse-back-tested; never run under dyld.** `linker/tests/macho_dyld_binding_test.rs` decodes the dyld structures but does not execute them |
| **RISC-V 64 Linux** | **Implemented, loud about what it cannot encode** — RV64GC LP64D, scalar FP, atomics, vector encodings; unencodable forms are `CodegenError`s since 2026-11-01 | **v1.0** | **Native ELF (static) — CALL/JAL/BRANCH/HI20/LO12, `_start`, PLT/GOT** | **Native** | **RV regs** | **Strip only** | **GC/ICF only** | **2** | **Structure-verified only.** Source builds stop at `ParallelMove` in lowering; no execution test |
| **RISC-V 32 Embedded** | **Refused at backend selection** — the RISC-V backend is RV64-only and says so before any function is encoded | **v1.0** | **Refused for executables** — the linker will not hand an RV32 target the RV64 `_start` stub | **Bare-metal** | **N/A (RV32 lowering not implemented)** | **Strip only** | **GC/ICF only** | **3** | **Not functional and no longer advertised as codegen-validated** (`linker/tests/phase11_riscv32_entry_stub_test.rs`) |
| **WASM (WASI / Web)** | **Native (compiler backend)** | **v1.0** | **Real writer (relocations, cross-object and indirect calls)** | **WASI (in-test host)** | **WASI Preview 1 `fd_write` / `env.*` for non-WASI targets** | **None** | **None** | **1** | **Execution-verified on two engines in CI** — V8 and wasmtime/Cranelift run linked modules with asserted output |
| **ARM Cortex-M (Thumb-2)** | **Proof-of-concept** — a 146-line Thumb-2 ALU subset | **v1.0** | **Native ELF (static)** | **Embedded** | **Partial** | **Strip only** | **GC/ICF only** | **3** | **Proof-of-concept.** No end-to-end firmware link and no execution |
| **i686 (x86 32-bit)** | **None** | **v1.0** | **Silently routed to the x86_64 arch handler** (`linker/src/arch/mod.rs:44`) | **N/A** | **N/A** | **Strip only** | **GC/ICF only** | **3** | **Not functional, not advertised.** The 32-bit delegation is a Phase 11 finding to be closed with the same loud-fallback rule |
| **GPU (CUDA / ROCm)** | **Source re-embedding only** | **ADOB packaging** | **Packager** | **Driver (external)** | **N/A** | **None** | **N/A** | **3** | **Scaffolding (container packaging; no device ISA, no driver calls)** |
| **NPU / TPU** | **Weight packaging only** | **ADOB packaging** | **Packager** | **External** | **N/A** | **None** | **N/A** | **3** | **Scaffolding (no Apple ANE implementation exists)** |
| **Quantum (QPU / Sim)** | **None (no language construct)** | **QIRB container** | **Container embed** | **State-vector sim only** | **N/A** | **None** | **N/A** | **3** | **Simulator + QASM export only (measurement is a deterministic mock)** |

---

## Subsystem Self-Containment Assessment

* **External Compiler Dependency (LLVM/GCC/Clang):** **0% required by
  default.** `adesh build` runs the self-contained pipeline (native adesh
  codegen → ADOB → `adeshlink`) with no external tools; the Cranelift AOT
  backend (`--codegen=cranelift`) is an alternative codegen and can
  delegate its final link to an external LLVM toolchain with
  `--external-linker`. Native executables are verified end-to-end on
  Windows x64 by executing the produced binaries.
* **External Object Tooling (llvm-ar/objdump/nm):** **0% required.** Native
  ADOB tools (`adesh adob`, `adesh objdump`, `adesh nm`, `adesh size`) and
  `adeshlink ar` (note: written archives lack a symbol-index member).

## Capability Facts (verified 2026-10-05)

* ADOB objects are **validated on write** (alignments, duplicates, symbol
  bounds, dangling relocations).
* The native x86-64 backend emits **real machine code** with branch fixups
  and PC32/ABS64 relocations; unsupported instruction forms fail with
  structured `CodegenError`s instead of silently emitting nothing.
* `--shared` emits PE DLL / ELF `.so` / Mach-O `.dylib` artifacts
  (structure-tested only). `--lto` maps to section GC + ICF + stripping;
  neither is a cross-module IR optimization, and the help texts say so.
* End-to-end proof: `tests/native_x86_64_e2e_test.rs` compiles Machine IR →
  native code → ADOB → PE link → execution, asserting the computed exit
  code.
