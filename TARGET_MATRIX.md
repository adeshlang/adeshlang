# Adesh Native Target Support Matrix

**Generated:** 2026-10-11 (Phase 8 revision: ELF TLS + eh_frame execution-verified, OS-specific CI)
**Canonical status source:** `CURRENT_STATE.md` (this file must agree with it)
**Format:** Multi-Target Platform Matrix

---

| Target Triple | Arch | OS | Object Format | Codegen | Relocations | Linking | Runtime | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | x86_64 | Windows | PE/COFF (PE32+) | **Native (integer + scalar SSE2 FP subset)** | ABS64, PC32 (emitted + resolved) | **Native PE (execution-tested)** | **Native Win32** | **Execution-verified** |
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | ELF64 | **Native (integer + scalar SSE2 FP subset)** | ABS64, PC32, GOTPCREL, COPY, TLS LE/IE + GD/LD relaxation | **Native ELF (static and dynamic execution-tested; libc/libgcc imports, local GOT slots, TLS + eh_frame_hdr)** | **Native POSIX** | **Execution-verified** |
| `x86_64-unknown-linux-musl` | x86_64 | Linux | ELF64 (static + dynamic) | **Native (integer + scalar SSE2 FP subset)** | ABS64, PC32, GOTPCREL, COPY (x86-64), TLS LE/IE + TPOFF64 | **Native ELF (static + TLS execution-verified; dynamic initial-exec against real musl `ld.so` verified in WSL)** | **Native POSIX** | **Execution-verified** (static local-exec + dynamic initial-exec TLS binaries executed with asserted exit codes; no dedicated musl CI job — CI runs the same suites against the host glibc loader) |
| `aarch64-unknown-linux-gnu` | AArch64 | Linux | ELF64 | **Implemented (AAPCS64, FP, NEON, Atomics)** | CALL26, ADRP, ADD_LO12 (emitted + resolved) | **Native ELF (static emission tested)** | **Native POSIX** | **Tested** (Codegen & relocation tested; execution pending) |
| `aarch64-apple-darwin` | AArch64 | macOS | Mach-O 64 | **Implemented (AAPCS64 codegen)** | BRANCH26 (constant only) | **Native Mach-O (libSystem routing; Rust runtime archive symbol lookup)** | **Native Darwin** | **Implemented** (Structure, runtime archive lookup, and dynamic routing tested; execution pending) |
| `x86_64-apple-darwin` | x86_64 | macOS | Mach-O 64 | **Native (integer + scalar SSE2 FP subset)** | — | **Native Mach-O (libSystem routing; Rust runtime archive symbol lookup)** | **Native Darwin** | **Implemented** (Structure, runtime archive lookup, and dynamic routing tested; execution pending) |
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

* **Execution-verified:** `x86_64-pc-windows-msvc` (PE executables: `linker/tests/pe_windows_e2e_test.rs`,
  `tests/native_x86_64_e2e_test.rs`, `tests/native_semantics_e2e_test.rs`, and phase 2-8 e2e suites)
  and `x86_64-unknown-linux-gnu` (static ELF executables:
  `tests/native_phase3_linux_e2e_test.rs` executing synthesized `_start`, SysV 6-arg calling convention,
  `.a` GNU `/` symbol table archives, and GC-enabled links — run natively on Linux CI and via WSL
  on Windows; dynamic ELF binaries executed through the real loader by
  `tests/native_phase8_dynamic_elf_test.rs`). Dead-section removal itself is unit-tested in
  `linker/src/gc.rs`; ICF `Safe`/`All` differentiation is unit-tested in `linker/src/icf.rs`.
  `x86_64-unknown-linux-musl` TLS binaries are executed with asserted exit codes by
  `tests/native_phase8_tls_ehframe_test.rs` in the WSL-musl development environment: a static
  local-exec binary installs its thread pointer via `arch_prctl(ARCH_SET_FS)` and accesses its
  counter at `%fs:(off − memsz)`; a dynamic initial-exec binary is processed by the real musl
  `ld.so` (PT_TLS copy + thread-pointer setup + `R_X86_64_TPOFF64` on the GOT slot). CI runs the
  same suites against the host glibc loader on Linux and via WSL on Windows; there is no dedicated
  musl CI job. `.eh_frame`/`.eh_frame_hdr` and `PT_GNU_EH_FRAME` are verified by independent
  byte-level decoders in the same test file (no runtime unwinder test yet).
  Mach-O output is verified beyond structure/magic: `linker/tests/macho_dyld_binding_test.rs`
  parse-back-decodes `LC_DYLD_INFO_ONLY` bind/rebase streams, the exports trie, the indirect
  symbol table, and the synthesized stub/GOT/helper chain — but dyld execution on a real macOS
  host is still pending (no execution test exists). WASM linker output remains checked by
  structure/magic bytes only.
  macOS CI keeps AOT regressions enabled and compiles native-JIT-only tests out on Apple Silicon
  because Cranelift native JIT PLT support is currently x86-64-only.
* **Native codegen subset:** x86-64 covers 64-bit integer GPR operations,
  scalar SSE2 floating point (add/sub/mul/div in single and double forms,
  `ucomis` comparisons, int↔float conversions), XMM0-13 allocation with
  Win64 nonvolatile XMM6-13 handling (full 128-bit `movups` save/restore),
  packed SSE encodings (present in the
  encoder but not yet used for the C ABI), intra-function branches,
  cross-function calls (PC32 relocations), stack arguments with Win64
  shadow space, callee-saved preservation, and full-width atomics
  (`lock xadd` / `lock cmpxchg` / `lock xchg` at 8/16/32/64 bits, `mfence`).
  Phase 2 added a shared SysV/Win64 argument classifier, sret, variadic
  helpers, struct-by-value, and Win64 TLS access sequences — these are
  execution-tested at the MachineIR/FFI level but are not yet reachable
  from Adesh source syntax (no `extern`/variadic/atomic/TLS surface
  syntax). Unsupported forms fail loudly.
  Missing: vector ABI support and source-level syntax for the Phase 2 ABI
  features.
* **Default executable builds use the native pipeline** (native adesh
  codegen → ADOB → built-in `adeshlink`). Cranelift is opt-in via
  `--codegen=cranelift`; `--emit-adob` / `--emit-asm` emit intermediate
  artifacts only.
* **Shared libraries:** `--shared` emits PE DLL / ELF `.so` / Mach-O
  `.dylib` artifacts. ELF x86-64 dynamic imports use PLT/GOT and
  local GOTPCREL slots; libc data imports use COPY relocations, while PIE
  and shared images rebase local GOT addresses with `R_X86_64_RELATIVE`.
  GNU ELF math imports add `libm.so.6` to `DT_NEEDED`; musl uses its libc
  math implementation. ELF dynamic executables have been executed in Ubuntu 24.04; shared-library
  execution remains unverified. Mach-O dyld structure is tested but execution
  remains unverified. **LTO:** `--lto` maps to section GC + ICF + symbol
  stripping; no cross-module IR optimization is performed.

## Target Discovery & Capability Inspection

The compiler provides introspection for all targets:
```bash
adesh target list
adesh target info x86_64-pc-windows-msvc
adesh target info aarch64-apple-darwin
```
