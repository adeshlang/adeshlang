# Adesh Native Target Support Matrix

**Generated:** 2026-10-05 (Phase 0 documentation truth-reset revision)
**Canonical status source:** `CURRENT_STATE.md` (this file must agree with it)
**Format:** Multi-Target Platform Matrix

---

| Target Triple | Arch | OS | Object Format | Codegen | Relocations | Linking | Runtime | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | x86_64 | Windows | PE/COFF (PE32+) | **Native (integer + scalar SSE2 FP subset)** | ABS64, PC32 (emitted + resolved) | **Native PE (execution-tested)** | **Native Win32** | **Working native pipeline** |
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | ELF64 | **Native (integer + scalar SSE2 FP subset)** | ABS64, PC32 | **Native ELF (static)** | **Native POSIX** | **Emits; not run-tested** |
| `x86_64-unknown-linux-musl` | x86_64 | Linux | ELF64 (static) | **Native (integer + scalar SSE2 FP subset)** | ABS64, PC32 | **Native ELF (static)** | **Native POSIX** | **Emits; not run-tested** |
| `aarch64-unknown-linux-gnu` | AArch64 | Linux | ELF64 | **Proof-of-concept** | CALL26, ADRP (linker side) | **Native ELF (static)** | **Native POSIX** | **Proof-of-concept** |
| `aarch64-apple-darwin` | AArch64 | macOS | Mach-O 64 | **Proof-of-concept** | BRANCH26 (constant only) | **Native Mach-O (skeletal)** | **Native Darwin** | **Not functional (cannot link libSystem)** |
| `x86_64-apple-darwin` | x86_64 | macOS | Mach-O 64 | **Native (integer + scalar SSE2 FP subset)** | — | **Native Mach-O (skeletal)** | **Native Darwin** | **Not functional (cannot link libSystem)** |
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
  produced binaries (`linker/tests/pe_windows_e2e_test.rs`,
  `tests/native_x86_64_e2e_test.rs`, `tests/native_semantics_e2e_test.rs`,
  and the phase 2-8 e2e suites; every binary-executing test is gated
  `#[cfg(all(target_os = "windows", target_arch = "x86_64"))]`). ELF and
  Mach-O outputs are checked by structure/magic bytes only — no CI job
  executes an ELF, Mach-O, or linker-produced WASM binary.
* **Native codegen subset:** x86-64 covers 64-bit integer GPR operations,
  scalar SSE2 floating point (add/sub/mul/div in single and double forms,
  `ucomis` comparisons, int↔float conversions), XMM0-13 allocation with
  Win64 nonvolatile XMM6-13 handling, packed SSE encodings (present in the
  encoder but not yet used for the C ABI), intra-function branches,
  cross-function calls (PC32 relocations), stack arguments with Win64
  shadow space, callee-saved preservation, and partial atomics (`lock xadd`
  / `lock cmpxchg`, 64-bit GPR only). Unsupported forms fail loudly.
  Missing: aggregate/struct-by-value classification, sret, variadics, and
  full vector ABI support.
* **Default executable builds use the native pipeline** (native adesh
  codegen → ADOB → built-in `adeshlink`). Cranelift is opt-in via
  `--codegen=cranelift`; `--emit-adob` / `--emit-asm` emit intermediate
  artifacts only.
* **Shared libraries:** `--shared` emits PE DLL / ELF `.so` / Mach-O
  `.dylib` artifacts (structure-tested only). ELF imports are unresolved
  (no GOT/PLT) and Mach-O lacks dyld binding info, so neither is
  execution-verified. **LTO:** `--lto` maps to section GC + ICF + symbol
  stripping; no cross-module IR optimization is performed.

## Target Discovery & Capability Inspection

The compiler provides introspection for all targets:
```bash
adesh target list
adesh target info x86_64-pc-windows-msvc
adesh target info aarch64-apple-darwin
```
