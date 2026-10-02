# Adesh Toolchain Capabilities Matrix

**Generated:** 2026-10-02 (revised after code-level audit)
**Authority:** Master Toolchain Capability Registry

---

## Toolchain Capabilities by Target

Honest per-target status. "Debug" means what the link path actually emits:
only section stripping (the DWARF 5 / CodeView generators exist but are never
invoked and emit a single DIE / two records). "LTO" means true IR-level
link-time optimization (not GC/ICF).

| Target | Codegen | Object (ADOB) | Linker | Runtime | ABI | Debug | LTO | Status |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **x86_64 Windows (MSVC)** | **Native (integer GPR subset)** | **v1.0** | **Native PE (execution-tested)** | **Native** | **Win64 regs + stack args + shadow** | **Strip only** | **None (error)** | **Working native pipeline** |
| **x86_64 Linux (GNU/Musl)** | **Native (integer GPR subset)** | **v1.0** | **Native ELF (static only)** | **Native** | **SysV regs + stack args** | **Strip only** | **None (error)** | **Emits; not run-tested** |
| **AArch64 Linux** | **PoC (4 ops)** | **v1.0** | **Native ELF (static only)** | **Native** | **AAPCS64 regs only** | **Strip only** | **None (error)** | **Proof-of-concept** |
| **AArch64 / x86_64 macOS** | **PoC (4 ops)** | **v1.0** | **Native Mach-O (skeletal)** | **Native** | **Partial** | **Strip only** | **None (error)** | **Not functional** |
| **RISC-V 64 Linux** | **PoC (4 ops)** | **v1.0** | **Native ELF (static only)** | **Native** | **RV regs only** | **Strip only** | **None (error)** | **Proof-of-concept** |
| **RISC-V 32 Embedded** | **PoC (4 ops)** | **v1.0** | **Native ELF (static only)** | **Bare-metal** | **Partial** | **Strip only** | **None (error)** | **Proof-of-concept** |
| **WASM (WASI / Web)** | **Native (compiler backend)** | **v1.0** | **Stub writer** | **WASI** | **WASM** | **None** | **None** | **Compiler path works; linker path is a stub** |
| **ARM Cortex-M (Thumb-2)** | **PoC** | **v1.0** | **Native ELF (static only)** | **Embedded** | **Partial** | **Strip only** | **None (error)** | **Proof-of-concept** |
| **GPU (CUDA / ROCm)** | **Source re-embedding only** | **ADOB packaging** | **Packager** | **Driver (external)** | **N/A** | **None** | **N/A** | **Scaffolding** |
| **NPU / TPU** | **Weight packaging only** | **ADOB packaging** | **Packager** | **External** | **N/A** | **None** | **N/A** | **Scaffolding** |
| **Quantum (QPU / Sim)** | **None (no language construct)** | **QIRB container** | **Container embed** | **State-vector sim only** | **N/A** | **None** | **N/A** | **Simulator + QASM export only** |

---

## Subsystem Self-Containment Assessment (qualified)

* **External Compiler Dependency (LLVM/GCC/Clang):** **0% required by
  default.** `adesh build` runs the self-contained pipeline (native adesh
  codegen → ADOB → `adeshlink` native linking) with no external tools; the
  Cranelift AOT backend (`--codegen=cranelift`) is an alternative codegen and
  can delegate its final link to an external LLVM toolchain with
  `--external-linker`. Native executables are verified end-to-end on Windows
  x64 by executing the produced binaries.
* **External Linker Dependency (GNU ld/lld/link.exe):** **0% required** for
  static PE executables (execution-tested) and static ELF executables
  (emission only, not run-tested). Shared libraries, Mach-O executables that
  link system libraries, and dynamic linking require external delegation
  (`--external-linker`; legacy alias `--external-toolchain`).
* **External Object Tooling (llvm-ar/objdump/nm):** **0% required.** Native
  ADOB tools (`adesh adob`, `adesh objdump`, `adesh nm`, `adesh size`) and
  `adeshlink ar` (note: written archives lack a symbol-index member).

## Capability Facts (verified 2026-10-02)

* ADOB objects are **validated on write** (alignments, duplicates, symbol
  bounds, dangling relocations).
* The native x86-64 backend emits **real machine code** with branch fixups and
  PC32/ABS64 relocations; unsupported instruction forms fail with structured
  `CodegenError`s instead of silently emitting nothing.
* `--shared` and `--lto` **fail loudly** with actionable diagnostics; the
  linker never silently claims a feature it does not perform.
* End-to-end proof: `tests/native_x86_64_e2e_test.rs` compiles Machine IR →
  native code → ADOB → PE link → execution, asserting the computed exit code.
