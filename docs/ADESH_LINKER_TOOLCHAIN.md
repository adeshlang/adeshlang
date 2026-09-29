# Adesh Native Linker & Multi-Domain Toolchain (`adeshlink`)

The **Adesh Native Linker (`adeshlink`)** is a production-grade, self-contained, multi-format binary linker and toolchain suite written in 100% safe Rust. It eliminates all external dependencies on LLVM (`lld`, `llvm-ar`, `llvm-nm`, `llvm-objdump`), GNU Binutils (`ld`, `ar`, `nm`, `strip`), and Microsoft Visual C++ (`link.exe`, `lib.exe`).

---

## 1. Zero-Dependency End-Execution Binary Generation

`adeshlink` generates complete, executable native binaries without requiring any external compiler runtime or linker:

- **Windows PE32+ (`.exe`, `.dll`):** Generates full DOS/PE headers, optional headers with subsystem configurations, Import Address Tables (IAT) with write permissions (`kernel32.dll`, `msvcrt.dll`), base relocations (`.reloc`), and 64-bit SEH `.pdata`/`.xdata` exception unwind tables. Verified on the native Windows NT loader.
- **Linux ELF64 (`ET_EXEC`, `ET_DYN` PIE):** Generates ELF headers, program load segments (`PT_LOAD`), GNU stack permissions (`PT_GNU_STACK`), and binary search `.eh_frame_hdr` frame unwinding tables.
- **macOS Mach-O 64-bit:** Generates `LC_SEGMENT_64`, `LC_MAIN`, and dynamic loader commands.
- **WebAssembly Core 2.0 / WASI Preview 1:** Generates standard WASM binary modules with Type, Function, Memory, Global, Export, Code, and Data sections.

---

## 2. MLIR & Polyhedral Dialect Linking Layer

`adeshlink` features a native MLIR dialect container and bytecode serializer (`.mlir.bytecode`):

- **Dialects Supported:**
  - `affine`: Polyhedral iteration domains, loop nest tiling, and dependency analysis.
  - `linalg`: Structured linear algebra operations (`matmul`, `conv_2d`, `pooling`) on tensors.
  - `tensor`: N-dimensional shape definitions and layout transformations.
  - `memref`: Strided memory buffer descriptors and view slicing.
  - `gpu`, `nvvm`, `rocdl`, `spirv`: Hardware accelerator abstractions.
  - `quantum`: Quantum gate operations and circuit representations.
  - `adesh`: High-level language constructs and ownership tracking.
- **MLIR Bytecode Container (`MLïR`):** Encodes/decodes module operations, attributes, tensor constants, and polyhedral schedule maps directly into binary sections.

---

## 3. GPU Hardware Acceleration (CUDA, ROCm, Vulkan, Metal, Baremetal)

`adeshlink` provides in-depth binary container generators for all major GPU platforms:

- **NVIDIA CUDA Fatbin (`CUDA_FATBIN_MAGIC = 0xBA55ED50`):** Multi-architecture packaging (SM70 Volta through SM100 Blackwell), embedding PTX text and CUBIN ELF64 objects with kernel symbol tables.
- **AMD ROCm HSACO (`.amdgpu_code_object`):** ELF64 AMDGPU target code objects containing kernel descriptors with SGPR/VGPR counts, wavefront configurations, and MsgPack metadata.
- **Khronos Vulkan SPIR-V 1.6 (`.spv`):** Full standalone SPIR-V binary generator (`0x07230203`) emitting `OpCapability`, `OpMemoryModel`, `OpEntryPoint`, `OpExecutionMode`, descriptor set bindings, and function blocks.
- **Apple MetalLib (`.metallib`):** Metal shader library packaging with function dictionaries and AIR bitcode payloads.
- **Baremetal GPU Command Buffers:** Hardware command stream generation for direct compute queue dispatching on discrete/integrated GPU rings.

---

## 4. NPU & TPU Neural Accelerator Infrastructure

- **Arm Ethos-U MicroNPU:** Emits hardware command stream packets (`ETHU` magic) with weight/bias payload compression, IFM/OFM 4D tensor shapes, and SRAM/Flash memory mapping.
- **Apple Neural Engine (ANE):** Neural Engine intermediate tensor packages, FP16/INT8 quantized weight layouts, and activation memory configurations.
- **Google TPU (V3/V4/V5/Trillium):** TPU executable container (`ADTP` magic) with XLA HLO module serialization, Matrix Multiply Unit (MXU) systolic tile layout (128x128 matrix systolic arrays), VPU vector lanes, and HBM memory layouts.

---

## 5. Quantum Computing (QIR, OpenQASM 3.0, Pulse Control)

- **Quantum Intermediate Representation (QIR):** Emits QIR binary packages (`QIRB` magic) with standard QIR runtime symbol bindings (`__quantum__qis__*`, `__quantum__rt__*`), qubit allocation records, and measurement syndrome tables.
- **OpenQASM 3.0 Generator:** Serializes quantum gate instruction streams (H, X, Y, Z, S, T, Rx, Ry, Rz, CX, CZ, Swap, Measure, Barrier) into standard OpenQASM 3.0 representations.
- **Microwave Pulse Calibration (`.quantum.pulses`):** Physical drive channel pulse records (Gaussian, DRAG, Square, Cosine) with frequency (GHz), phase (radians), amplitude, duration (ns), and drag beta parameters.
- **Hybrid Classical-Quantum Linker:** Interweaves host CPU execution threads with embedded quantum circuit payloads.

---

## 6. Embedded Bare Chips & Microcontrollers

- **Linker Script Engine:** Evaluates custom `MEMORY` and `SECTIONS` scripts with memory region constraints (`FLASH (rx)`, `RAM (rwx)`), boundary allocation, and overflow prevention.
- **Interrupt Vector Table (IVT):**
  - **ARM Cortex-M (M0/M3/M4/M7/M33):** Initial SP, Thumb Reset_Handler, NMI, HardFault, MemManage, BusFault, UsageFault, SVCall, PendSV, SysTick, and device IRQ vectors.
  - **RISC-V (RV32I/RV64):** Trap vector tables in direct and vectored modes.
- **Zero-Dependency `crt0` Startup Synthesizer:** Synthesizes the Flash $\to$ RAM data segment copy loop, BSS zeroing, stack pointer setup, and jump to `main()`.
- **Flash Firmware Image Formats:**
  - **Flat Binary (`.bin`):** Direct flash memory image.
  - **Intel HEX (`.hex`):** Extended linear address records (Type 04) with two's complement checksums.
  - **Motorola S-Records (`.srec`):** S0/S3/S7 record sequences.

---

## 7. CLI Reference & Subcommands

| Command | Purpose |
| :--- | :--- |
| `adeshlink link` | Native linker producing executable binaries or libraries |
| `adeshlink ar` | Static archive manager (`rcs`, `t`, `x`) |
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
