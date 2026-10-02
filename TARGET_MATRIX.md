# Adesh Native Target Support Matrix

**Generated:** 2026-10-02 (revised after code-level audit)
**Format:** Multi-Target Platform Matrix

---

| Target Triple | Arch | OS | Object Format | Codegen | Relocations | Linking | Runtime | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | x86_64 | Windows | PE/COFF (PE32+) | **Native (integer GPR subset)** | ABS64, PC32 (emitted + resolved) | **Native PE (execution-tested)** | **Native Win32** | **Working native pipeline** |
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | ELF64 | **Native (integer GPR subset)** | ABS64, PC32 | **Native ELF (static)** | **Native POSIX** | **Emits; not run-tested** |
| `x86_64-unknown-linux-musl` | x86_64 | Linux | ELF64 (static) | **Native (integer GPR subset)** | ABS64, PC32 | **Native ELF (static)** | **Native POSIX** | **Emits; not run-tested** |
| `aarch64-unknown-linux-gnu` | AArch64 | Linux | ELF64 | **Proof-of-concept** | CALL26, ADRP (linker side) | **Native ELF (static)** | **Native POSIX** | **Proof-of-concept** |
| `aarch64-apple-darwin` | AArch64 | macOS | Mach-O 64 | **Proof-of-concept** | BRANCH26 (constant only) | **Native Mach-O (skeletal)** | **Native Darwin** | **Not functional (cannot link libSystem)** |
| `x86_64-apple-darwin` | x86_64 | macOS | Mach-O 64 | **Native (integer GPR subset)** | — | **Native Mach-O (skeletal)** | **Native Darwin** | **Not functional (cannot link libSystem)** |
| `riscv64gc-unknown-linux-gnu` | RISC-V 64 | Linux | ELF64 | **Proof-of-concept** | CALL (linker side) | **Native ELF (static)** | **Native POSIX** | **Proof-of-concept** |
| `riscv32imac-unknown-none-elf` | RISC-V 32 | Bare-metal | ELF32 | **Proof-of-concept** | — | **Native ELF (static)** | **No-std bare-metal** | **Proof-of-concept** |
| `wasm32-unknown-wasi` | WASM | WASI | WASM binary | **Native (compiler backend)** | Not applied by linker writer | **Compiler path; linker writer is a stub** | **WASI runtime** | **Works via compiler backend** |
| `wasm32-unknown-unknown` | WASM | Web | WASM binary | **Native (compiler backend)** | — | **Compiler path** | **Web stubs** | **Works via compiler backend** |
| `thumbv7em-none-eabihf` | ARM Cortex-M | Bare-metal | ELF32 | **Proof-of-concept** | — | **Native ELF (static)** | **Embedded minimal** | **Proof-of-concept** |
| `nvptx64-nvidia-cuda` | GPU (PTX) | CUDA | ADOB kernel package | **None (source re-embedded)** | — | **Container packaging** | **External driver** | **Scaffolding** |
| `amdgcn-amd-amdhsa` | GPU (ROCm) | AMD HSA | ADOB kernel package | **None (source re-embedded)** | — | **Container packaging** | **External driver** | **Scaffolding** |
| `quantum-circuit-qasm3` | QPU | Abstract | QIRB container | **None (no language construct)** | — | **Container embed** | **State-vector sim only** | **Simulator + QASM 3.0 export only** |

---

## Notes

* **Execution-verified:** only `x86_64-pc-windows-msvc` has tests that run
  produced binaries (`tests/pe_windows_e2e_test.rs`,
  `tests/native_x86_64_e2e_test.rs`). ELF and Mach-O outputs are checked by
  magic bytes only.
* **Native codegen subset:** x86-64 covers 64-bit integer GPR operations,
  intra-function branches, cross-function calls (PC32 relocations), stack
  arguments with Win64 shadow space, and callee-saved preservation. Floats are
  materialized as bit patterns into GPRs; there is no SSE/SIMD/atomics
  support yet, and unsupported forms fail loudly.
* **Default executable builds still use Cranelift**; the native pipeline is
  selected via `--emit-adob` / `--emit-asm` and linked with `adeshlink`.
* **Shared libraries and true LTO are hard errors** in the native linker, with
  suggestions to use `--external-toolchain` or `-O3` (GC/ICF) instead.

## Target Discovery & Capability Inspection

The compiler provides introspection for all targets:
```bash
adesh target list
adesh target info x86_64-pc-windows-msvc
adesh target info aarch64-apple-darwin
```
