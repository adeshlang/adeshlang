# Adesh Native Linker & Multi-Domain Toolchain (`adeshlink`)

The **Adesh Native Linker (`adeshlink`)** is a self-contained, multi-format
binary linker and toolchain suite written in 100% safe Rust. It eliminates
external dependencies on LLVM (`lld`, `llvm-ar`, `llvm-nm`, `llvm-objdump`),
GNU Binutils (`ld`, `ar`, `nm`, `strip`), and Microsoft Visual C++
(`link.exe`, `lib.exe`).

**Status (2026-10-05):** only the **Windows x86-64 PE path is
execution-verified** (tests produce and run binaries). ELF and Mach-O
outputs are emitted and structurally validated but never executed. The
accelerator/quantum sections below are **container packaging scaffolding**:
no device machine code is generated from Adesh source and no GPU/NPU driver
is ever called. See `CURRENT_STATE.md` (canonical) for details.

---

## 1. Zero-Dependency End-Execution Binary Generation

- **Windows PE32+ (`.exe`, `.dll`):** Full DOS/PE headers, optional headers,
  Import Address Tables (IAT) (`kernel32.dll`, `msvcrt.dll`,
  `ucrtbase.dll`), base relocations (`.reloc`), ASLR/NX flags, TLS
  directory, entry synthesis, and DLL export tables for `--shared`.
  **Executables are execution-tested on Windows.** SEH `.pdata`/`.xdata`
  synthesis is not yet wired into the link path (a generator exists but is
  never invoked).
- **Linux ELF64 (`ET_EXEC`, `ET_DYN`):** ELF headers, program load segments
  (`PT_LOAD`), GNU stack permissions, and `.eh_frame_hdr` table structures.
  **Not run-tested**; `ET_DYN` objects have no GOT/PLT, so imported symbols
  are unresolved.
- **macOS Mach-O 64-bit:** `LC_SEGMENT_64`, `LC_MAIN`, and loader-command
  layouts. **Not functional for programs that link system libraries**: no
  dyld info / chained fixups / exports trie exists, so external symbols
  cannot bind.
- **WebAssembly Core 2.0 / WASI Preview 1:** the *linker's* WASM writer is a
  single-function stub. Real, runnable WASM comes from the compiler
  backend (`src/backends/wasm`), not from `adeshlink`.

## 2. MLIR & Polyhedral Dialect Layer

- MLIR **text** is generated from Adesh IR and processed by **external**
  `mlir-opt`/`mlir-translate`/`llc` (optional; missing tools fall back to
  the interpreter). Dialects include `affine`, `linalg`, `tensor`,
  `memref`, `gpu`, `nvvm`, `rocdl`, `spirv`, `quantum`, `adesh`.
- **MLIR Bytecode Container (`MLïR`):** a storage container that
  encodes/decodes module operations and attributes into binary sections;
  it is not a code generator.

## 3. GPU Packaging (CUDA, ROCm, Vulkan, Metal) — Scaffolding

- **NVIDIA CUDA Fatbin (magic `0xBA55ED50`):** a **custom** container that
  repackages caller-supplied PTX text / CUBIN bytes. It is not NVIDIA's
  fatbin layout and cannot be loaded by the CUDA driver.
- **AMD ROCm HSACO:** a partial-ELF container with a custom kernel
  descriptor; no MsgPack metadata is emitted (earlier docs claimed
  otherwise — that was aspirational).
- **Vulkan SPIR-V 1.6 (`.spv`):** the standalone header emitter exists, but
  the compute-shader path emits a fixed, empty shader and ignores
  descriptor set / binding parameters.
- **Apple MetalLib (`.metallib`):** wraps caller-supplied bytes in a
  magic-prefixed blob; no AIR bitcode is generated.
- **Baremetal GPU command buffers:** not implemented (earlier docs claimed
  "hardware command stream generation" — that was aspirational).
- There are **zero CUDA/HIP/Vulkan driver API calls**; no kernel can be
  launched from the toolchain.

## 4. NPU & TPU Neural Accelerator Packaging — Scaffolding

- **Arm Ethos-U MicroNPU:** packs self-defined command-packet structures
  (`ETHU` magic); these are not real Ethos-U command streams.
- **Apple Neural Engine (ANE):** **no implementation exists** (earlier docs
  claimed FP16/INT8 weight layouts — aspirational).
- **Google TPU:** a padded section container (`ADTP` magic); no XLA HLO
  module serialization exists (earlier claims were aspirational).

## 5. Quantum Computing — Library-Level

- **OpenQASM 3.0 Generator:** real; serializes the 15-gate circuit IR.
- **Circuit IR, gate decomposition, topological routing, calibration pulse
  records:** real library components (`linker/src/quantum/`).
- **State-vector simulator:** real, but **measurement is a deterministic
  0.5-threshold mock**.
- **QIR Container (`QIRB`):** embeds QASM text plus packed sections; it is
  not LLVM QIR. `__quantum__qis__*` / `__quantum__rt__*` symbol names exist
  as string constants only and are never emitted.
- **Hybrid Classical-Quantum execution:** not implemented.
- **No language-level `qubit` construct exists.**

## 6. Embedded Bare Chips & Microcontrollers

- **Linker Script Engine:** evaluates custom `MEMORY` and `SECTIONS`
  scripts with memory region constraints and overflow prevention.
- **Interrupt Vector Table (IVT):** ARM Cortex-M (M0/M3/M4/M7/M33) initial
  SP / Thumb vectors; RISC-V direct and vectored trap tables.
- **Zero-Dependency `crt0` Startup Synthesizer:** Flash→RAM data copy
  loop, BSS zeroing, stack pointer setup, jump to `main()`.
- **Flash Firmware Image Formats:** Flat Binary (`.bin`), Intel HEX
  (`.hex`), Motorola S-Records (`.srec`).
- *Note: the embedded codegen behind these is proof-of-concept; none of it
  is execution-verified on hardware.*

## 7. CLI Reference & Subcommands

| Command | Purpose |
| :--- | :--- |
| `adeshlink link` | Native linker producing executable binaries or libraries |
| `adeshlink ar` | Static archive manager (`rcs`, `t`, `x`) — note: written archives lack a symbol index |
| `adeshlink nm` | Symbol table listing with addresses and type letters |
| `adeshlink objdump` | Section headers (`-h`), hex dump (`-s`), and disassembly |
| `adeshlink readobj` | Detailed headers, segments, and symbol records |
| `adeshlink size` | Berkeley-format section sizes (`text data bss dec hex`) |
| `adeshlink strip` | Strip debug metadata and local symbols |
| `adeshlink inspect` | Full binary format and metadata inspection |
| `adeshlink symbols` | List all symbol table entries |
| `adeshlink sections` | List section headers and flags |
| `adeshlink relocations`| List relocation records |
| `adeshlink deps` | Symbol reference dependency graph |
| `adeshlink targets` | Target platform maturity tiers |
| `adeshlink version` | Version and host target provenance |
