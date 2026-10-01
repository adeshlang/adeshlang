# Adesh Toolchain Capabilities Matrix

**Generated:** 2026-10-01  
**Authority:** Master Toolchain Capability Registry  

---

## Toolchain Capabilities by Target

| Target | Codegen | Object (ADOB) | Linker | Runtime | Threads | ABI | Debug | LTO | Status |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **x86_64 Windows (MSVC)** | **Native** | **v1.0** | **Native PE** | **Native** | **Win32** | **MS x64** | **Line/DWARF**| **GC/ICF** | **Production Ready** |
| **x86_64 Linux (GNU/Musl)**| **Native** | **v1.0** | **Native ELF**| **Native** | **POSIX** | **SysV AMD64**| **DWARF 5** | **GC/ICF** | **Production Ready** |
| **AArch64 Linux** | **Native** | **v1.0** | **Native ELF**| **Native** | **POSIX** | **AAPCS64** | **DWARF 5** | **GC/ICF** | **Production Ready** |
| **AArch64 macOS (Darwin)** | **Native** | **v1.0** | **Native Mach-O**| **Native** | **Darwin** | **AAPCS64** | **DWARF 5** | **GC/ICF** | **Production Ready** |
| **x86_64 macOS (Darwin)** | **Native** | **v1.0** | **Native Mach-O**| **Native** | **Darwin** | **SysV AMD64**| **DWARF 5** | **GC/ICF** | **Production Ready** |
| **RISC-V 64 Linux** | **Native** | **v1.0** | **Native ELF**| **Native** | **POSIX** | **RV64GC** | **DWARF 5** | **GC/ICF** | **Production Ready** |
| **RISC-V 32 Embedded** | **Native** | **v1.0** | **Native ELF**| **BareMetal**| **None** | **RV32IMAC** | **DWARF 5** | **GC/ICF** | **Production Ready** |
| **WASM (WASI / Web)** | **Native** | **v1.0** | **Native WASM**| **WASI** | **WASI-Th** | **WASM** | **SourceMap** | **GC/ICF** | **Production Ready** |
| **ARM Cortex-M (Thumb-2)** | **Native** | **v1.0** | **Native ELF**| **Embedded**| **RTOS** | **AAPCS32** | **DWARF 5** | **GC/ICF** | **Production Ready** |
| **GPU (CUDA / ROCm)** | **Kernel IR**| **ADOB Pkg** | **Packager** | **Driver** | **GPU Grid**| **GPU ABI** | **Kernel Sym**| **DeviceOpt**| **Accelerator Tier** |
| **NPU / TPU** | **Tensor IR**| **ADOB Pkg** | **Packager** | **DMA/Driver**| **Async**| **Tensor ABI**| **Tensor Sym**| **Graph Opt**| **Accelerator Tier** |
| **Quantum (QPU / Sim)** | **AQIR** | **QIRB Pkg** | **Q-Linker** | **Sim/QPU** | **Q-Thread**| **AQIR ABI** | **Circuit Dbg**| **Gate Decomp**| **Quantum Native** |

---

## Subsystem Self-Containment Assessment

* **External Compiler Dependency (LLVM/GCC/Clang):** **0% Required for Core Pipeline**. The native toolchain directly encodes machine code and links executables.
* **External Linker Dependency (GNU ld/lld/link.exe):** **0% Required**. The Adesh Linker synthesizes PE32+, ELF32/64, Mach-O, and WASM binaries with custom OS API classification and section resolution.
* **External Object Tooling Dependency (llvm-ar/objdump/nm):** **0% Required**. Native ADOB tools (`adesh adob`, `adesh objdump`, `adesh nm`, `adesh size`) handle binary analysis.
