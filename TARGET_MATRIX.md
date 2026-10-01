# Adesh Native Target Support Matrix

**Generated:** 2026-10-01  
**Format:** Multi-Target Platform Matrix

---

| Target Triple | Arch | OS | Object Format | Codegen | Relocations | Linking | Runtime | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | x86_64 | Windows | PE/COFF (PE32+) | **Native (x86_64)** | DIR64, REL32, ADDR32 | **Native (PE)** | **Native Win32** | **Production Ready** |
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | ELF64 | **Native (x86_64)** | 64, PC32, PLT32, GOT32 | **Native (ELF)** | **Native POSIX** | **Production Ready** |
| `x86_64-unknown-linux-musl` | x86_64 | Linux | ELF64 (Static) | **Native (x86_64)** | 64, PC32 | **Native (ELF)** | **Native POSIX** | **Production Ready** |
| `aarch64-unknown-linux-gnu` | AArch64 | Linux | ELF64 | **Native (AArch64)** | ABS64, CALL26, ADR_PREL | **Native (ELF)** | **Native POSIX** | **Production Ready** |
| `aarch64-apple-darwin` | AArch64 | macOS | Mach-O 64 | **Native (AArch64)** | UNSIGNED_64, BRANCH26 | **Native (Mach-O)**| **Native Darwin**| **Production Ready** |
| `x86_64-apple-darwin` | x86_64 | macOS | Mach-O 64 | **Native (x86_64)** | UNSIGNED_64, BRANCH32 | **Native (Mach-O)**| **Native Darwin**| **Production Ready** |
| `riscv64gc-unknown-linux-gnu` | RISC-V 64 | Linux | ELF64 | **Native (RV64)** | CALL, JAL, BRANCH | **Native (ELF)** | **Native POSIX** | **Production Ready** |
| `riscv32imac-unknown-none-elf` | RISC-V 32 | BareMetal | ELF32 | **Native (RV32)** | 32, PCREL_HI20/LO12 | **Native (ELF)** | **No-Std BareMetal**| **Production Ready** |
| `wasm32-unknown-wasi` | WASM | WASI | WASM Binary | **Native (WASM)** | WASM Reloc Type | **Native (WASM)**| **WASI Runtime** | **Production Ready** |
| `wasm32-unknown-unknown` | WASM | Web | WASM Binary | **Native (WASM)** | None (Linear Memory) | **Native (WASM)**| **Web Stubs** | **Production Ready** |
| `thumbv7em-none-eabihf` | ARM (Cortex-M)| BareMetal | ELF32 | **Native (Thumb-2)** | CALL, JUMP24 | **Native (ELF)** | **Embedded Minimal**| **Production Ready** |
| `nvptx64-nvidia-cuda` | GPU (PTX) | CUDA | ADOB Kernel | **Native (PTX/SPIR-V)** | Kernel Sym | **ADOB Packager**| **CUDA Driver** | **Accelerator Tier**|
| `amdgcn-amd-amdhsa` | GPU (ROCm) | AMD HSA | ADOB Kernel | **Native (SPIR-V)** | Kernel Sym | **ADOB Packager**| **ROCm Driver** | **Accelerator Tier**|
| `quantum-circuit-qasm3` | QPU | Abstract | OpenQASM 3.0 / QIR | **Native (AQIR)** | Syndrome Map | **Quantum Linker**| **StateVector Sim**| **Quantum Native** |

---

## Target Discovery & Capability Inspection

The compiler provides introspection for all targets:
```bash
adesh target list
adesh target info x86_64-pc-windows-msvc
adesh target info aarch64-apple-darwin
```
