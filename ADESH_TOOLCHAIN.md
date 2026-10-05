# Adesh Native Toolchain & Multi-Domain Linker Suite

Adesh includes a **self-contained native binary toolchain (`adeshlink`)** that produces native executables, static archives, and container packages for GPU/NPU/TPU/quantum payloads **without external dependencies on LLVM, Clang, GCC, MSVC, or GNU Binutils**.

**Status (2026-10-05):** only the **Windows x86-64 PE path is
execution-verified** (binaries are produced and run by tests). ELF and
Mach-O outputs are emitted and structurally validated but never executed.
The accelerator and quantum features below are **container packaging
scaffolding**: they wrap metadata and caller-supplied or re-embedded
payloads; no device machine code is generated from Adesh source, and no
GPU/NPU drivers are called anywhere in the toolchain. See
`CURRENT_STATE.md` (canonical) for details.

---

## Multi-Domain Capabilities Summary

### 1. Zero-Dependency Native Binary Generation
- **Windows PE32+ (`.exe`, `.dll`):** Generates valid DOS/PE headers, Import
  Address Tables (`kernel32.dll`, `msvcrt.dll`, `ucrtbase.dll`), base
  relocations (`.reloc`), and ASLR/NX flags. Executables are
  **execution-tested** on Windows. SEH `.pdata`/`.xdata` synthesis is *not*
  yet wired into the link path (a generator exists but is never invoked).
- **Linux ELF64 (`ET_EXEC`, `ET_DYN`):** Generates ELF headers, program
  load segments, and `.eh_frame_hdr` table structures. **Not run-tested**;
  dynamic objects have no GOT/PLT, so imported symbols are unresolved.
- **macOS Mach-O 64-bit:** Generates `LC_SEGMENT_64`, `LC_MAIN`, and
  loader-command layouts. **Not functional for programs that link system
  libraries**: no dyld info / chained fixups / exports trie exists, so
  external symbols cannot bind.
- **WebAssembly Core 2.0 / WASI Preview 1:** the *compiler backend* emits
  real, runnable WASM (host imports, data segments, runs via
  Wasmtime/Node). The *linker's* WASM writer is a single-function stub.

### 2. MLIR & Polyhedral Dialects
- **Dialect Support:** MLIR **text** is generated from Adesh IR and
  processed by **external** `mlir-opt`/`mlir-translate`/`llc` (optional
  toolchain; missing tools fall back to the interpreter). Dialects
  include `affine`, `linalg`, `tensor`, `memref`, `gpu`, `nvvm`, `rocdl`,
  `spirv`, `quantum`, `adesh`.
- **MLIR Bytecode Container (`MLïR`):** Encodes/decodes module operations
  and attributes into binary sections for embedding; it is a storage
  container, not a code generator.

### 3. GPU Acceleration (CUDA, ROCm, Vulkan, Metal) — Packaging Scaffolding
- **NVIDIA CUDA:** A **custom** fatbin-style container (magic
  `0xBA55ED50`) that repackages caller-supplied PTX text / CUBIN bytes. It
  is *not* NVIDIA's fatbin layout and cannot be loaded by the CUDA driver.
- **AMD ROCm:** A partial-ELF container with a custom kernel descriptor.
  No MsgPack metadata is emitted; it is not a real AMDGPU code object.
- **Vulkan SPIR-V 1.6:** A standalone binary header emitter exists, but
  the compute-shader path emits a fixed, empty shader; descriptor set and
  binding parameters are ignored.
- **Apple MetalLib:** Wraps arbitrary caller-supplied bytes in a
  magic-prefixed blob; no AIR bitcode is generated.
- **Baremetal GPU:** No hardware command streams are generated.
- There are **zero CUDA/HIP/Vulkan driver API calls** in the toolchain;
  no kernel can be launched. The only real GPU path is the external MLIR
  pipeline above.

### 4. NPU & TPU Accelerators — Packaging Scaffolding
- **Arm Ethos-U:** Packs self-defined command-packet structures; these are
  not real Ethos-U command streams.
- **Apple Neural Engine (ANE):** **No ANE code exists** (earlier documents
  claimed FP16/INT8 weight layouts; that was aspirational).
- **Google TPU:** A padded section container (`ADTP` magic); no XLA HLO
  serialization exists.

### 5. Quantum Computing — Library-Level
- **OpenQASM 3.0 Export:** Real; serializes the 15-gate circuit IR
  (H, X, Y, Z, S, T, Rx, Ry, Rz, CX, CZ, Swap, Measure, Barrier) to QASM
  3.0 text.
- **Circuit IR, decomposition, and topological routing:** Real
  (library-level, `linker/src/quantum/`).
- **State-vector simulator:** Real, but **measurement is a deterministic
  0.5-threshold mock** (no RNG).
- **QIR Container (`QIRB`):** Embeds the QASM **text** plus packed
  sections; it is not LLVM QIR. The `__quantum__qis__*` /
  `__quantum__rt__*` symbol names exist as string constants only and are
  never emitted into binaries.
- **Hybrid Classical-Quantum execution:** Not implemented (symbol names
  and a config struct only).
- **No language-level `qubit` construct exists** in the parser or type
  system.

### 6. Embedded Bare Chips & Microcontrollers
- **Linker Script Engine:** Evaluates custom `MEMORY` and `SECTIONS`
  scripts with memory region constraints (`FLASH (rx)`, `RAM (rwx)`).
- **Interrupt Vector Table (IVT):** ARM Cortex-M (M0/M3/M4/M7/M33) and
  RISC-V trap vector tables.
- **Zero-Dependency `crt0` Startup Synthesizer:** Data segment copy loop,
  BSS zeroing, stack pointer setup, and jump to `main()`.
- **Flash Firmware Image Formats:** Flat Binary (`.bin`), Intel HEX
  (`.hex`), and Motorola S-Records (`.srec`).
- *Note: the embedded codegen behind these is proof-of-concept (minimal
  Thumb-2 ALU); none of it is execution-verified on hardware.*

---

## CLI Quick Reference

```bash
# 1. Compile & Link an Adesh Program to Native Executable (Windows x86-64
#    is the execution-verified target)
adesh build print.adesh -o print.exe
.\print.exe

# 2. Inspect with adeshlink tools
adeshlink nm print.o
adeshlink objdump print.o -h
adeshlink size print.o
adeshlink ar rcs libprint.a print.o
adeshlink strip print.o -o print_stripped.o
```

For full documentation and technical specifications, see
[docs/ADESH_LINKER_TOOLCHAIN.md](docs/ADESH_LINKER_TOOLCHAIN.md) and the
canonical status in [CURRENT_STATE.md](CURRENT_STATE.md).
