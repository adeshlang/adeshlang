# Adesh Native Toolchain & Multi-Domain Linker Suite

Adesh includes a **fully self-contained native binary toolchain (`adeshlink`)** designed to produce native executables, static archives, GPU fatbins, AI accelerator payloads, quantum circuit packages, and bare-metal firmware **without external dependencies on LLVM, Clang, GCC, MSVC, or GNU Binutils**.

---

## ⚡ Multi-Domain Capabilities Summary

### 1. Zero-Dependency Native Binary Generation
- **Windows PE32+ (`.exe`, `.dll`):** Generates valid DOS/PE headers, Import Address Tables (`kernel32.dll`, `msvcrt.dll`), base relocations (`.reloc`), and 64-bit SEH `.pdata`/`.xdata` unwind tables. Tested and executed directly on Windows NT.
- **Linux ELF64 (`ET_EXEC`, `ET_DYN` PIE):** Generates ELF64 headers, program load segments (`PT_LOAD`), GNU stack permissions (`PT_GNU_STACK`), and binary search `.eh_frame_hdr` frame unwinding tables.
- **macOS Mach-O 64-bit:** Generates `LC_SEGMENT_64`, `LC_MAIN`, and dynamic loader commands.
- **WebAssembly Core 2.0 / WASI Preview 1:** Generates standard WASM binary modules.

### 2. MLIR & Polyhedral Dialects
- **Dialects Supported:** `affine`, `linalg`, `tensor`, `memref`, `gpu`, `nvvm`, `rocdl`, `spirv`, `quantum`, `adesh`.
- **MLIR Bytecode Container (`MLïR`):** Encodes/decodes module operations, attributes, tensor constants, and polyhedral schedule maps directly into binary sections.

### 3. GPU Acceleration (CUDA, ROCm, Vulkan, Metal, Baremetal)
- **NVIDIA CUDA:** Multi-arch Fatbin container (`0xBA55ED50`, SM70–SM100) embedding PTX text and CUBIN ELF64 objects.
- **AMD ROCm:** AMDGPU ELF64 HSACO code objects with kernel descriptors and MsgPack metadata.
- **Vulkan SPIR-V 1.6:** Pure standalone SPIR-V binary generator (`0x07230203`) with compute shader entry points and descriptor set decorations.
- **Apple MetalLib:** `.metallib` shader archives with function dictionaries and AIR bitcode payloads.
- **Baremetal GPU:** Hardware command stream generation for direct compute queue dispatches.

### 4. NPU & TPU Accelerators
- **Arm Ethos-U:** Emits hardware command stream packets (`ETHU` magic) with weight/bias payload compression and IFM/OFM 4D tensor shapes.
- **Apple Neural Engine (ANE):** Neural Engine intermediate tensor packages, FP16/INT8 quantized weight layouts.
- **Google TPU (V3/V4/V5/Trillium):** TPU executable container (`ADTP` magic) with XLA HLO module serialization, Matrix Multiply Unit (MXU) systolic tile layout (128x128 matrix systolic arrays), VPU vector lanes, and HBM memory layouts.

### 5. Quantum Computing (QIR, OpenQASM 3.0, Pulse Control)
- **Quantum Intermediate Representation (QIR):** Emits QIR binary packages (`QIRB` magic) with standard QIR runtime symbol bindings (`__quantum__qis__*`, `__quantum__rt__*`).
- **OpenQASM 3.0 Generator:** Serializes quantum gate instruction streams (H, X, Y, Z, S, T, Rx, Ry, Rz, CX, CZ, Swap, Measure, Barrier).
- **Microwave Pulse Calibration (`.quantum.pulses`):** Physical drive channel pulse records (Gaussian, DRAG, Square, Cosine) with frequency (GHz), phase, amplitude, duration (ns), and drag beta parameters.
- **Hybrid Classical-Quantum Linker:** Interweaves host CPU execution threads with embedded quantum circuit payloads.

### 6. Embedded Bare Chips & Microcontrollers
- **Linker Script Engine:** Evaluates custom `MEMORY` and `SECTIONS` scripts with memory region constraints (`FLASH (rx)`, `RAM (rwx)`).
- **Interrupt Vector Table (IVT):** ARM Cortex-M (M0/M3/M4/M7/M33) and RISC-V trap vector tables.
- **Zero-Dependency `crt0` Startup Synthesizer:** Synthesizes the Flash $\to$ RAM data segment copy loop, BSS zeroing, stack pointer setup, and jump to `main()`.
- **Flash Firmware Image Formats:** Flat Binary (`.bin`), Intel HEX (`.hex`), and Motorola S-Records (`.srec`).

---

## 🛠️ CLI Quick Reference

```bash
# 1. Compile & Link an Adesh Program to Native Executable
adesh build print.adesh -o print.exe
.\print.exe

# 2. Inspect with adeshlink tools
adeshlink nm print.o
adeshlink objdump print.o -h
adeshlink size print.o
adeshlink ar rcs libprint.a print.o
adeshlink strip print.o -o print_stripped.o
```

For full documentation and technical specifications, see [docs/ADESH_LINKER_TOOLCHAIN.md](docs/ADESH_LINKER_TOOLCHAIN.md).
