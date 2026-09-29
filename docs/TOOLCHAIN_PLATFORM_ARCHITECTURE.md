# Adesh Toolchain Platform Architecture & Roadmap

This document defines the formal architecture, design principles, and execution roadmap for the **Adesh Native Toolchain Platform**.

---

## 1. The Three-Pillar Architecture

The Adesh ecosystem is divided into three cleanly decoupled pillars:

```text
                                 ADESH TOOLCHAIN PLATFORM
                                            │
           ┌────────────────────────────────┼────────────────────────────────┐
           │                                │                                │
        adeshc                          adeshlink                         adeshrt
  (Compiler & Codegen)           (Binary Integration & Linking)        (Execution & Runtime)
           │                                │                                │
   ├── Frontend (Lexer/Parser)      ├── Multi-pass Symbol Resolution  ├── Safe GC-free RAII drops
   ├── AST / HIR / LIR Engine       ├── Relocation Patching (x86/ARM) ├── Compiler-RT Intrinsics
   ├── Extensible MLIR / Dialects   ├── Formal ABI v1 (.adesh.meta)   ├── Scope-aware Unwind Tables
   ├── Quantum IR & Qubit Router    ├── Executable Formats:           ├── Async Green-Thread Runtime
   ├── Machine Code Encoders:       │   ├── Windows PE32+ (.exe/.dll) ├── Embedded Bootstraps (crt0)
   │   ├── x86_64 Machine Code      │   ├── Linux ELF64 (PIE/Static)  ├── GPU Driver Bridges (Vulkan/CUDA)
   │   ├── AArch64 Machine Code     │   ├── macOS Mach-O 64-bit       └── Quantum Simulation &
   │   ├── RISC-V RV32/RV64         │   └── WebAssembly (WASM 2.0)        QPU Provider Dispatch
   │   ├── Vulkan SPIR-V 1.6        ├── Embedded Linker Scripts:
   │   └── NVIDIA PTX / CUBIN       │   ├── MEMORY / SECTIONS Layout
   └── Native Object Writer (.o)    │   ├── Interrupt Vector Tables
                                    │   └── Flat Image (BIN/HEX/SREC)
                                    ├── Dead Code Elimination (GC)
                                    └── Identical Code Folding (ICF)
```

---

## 2. Capability & Execution Boundaries

| Domain | `adeshc` Compiles | `adeshlink` Links & Packages | Hardware / OS Executes | Maturity Tier |
| :--- | :---: | :---: | :---: | :---: |
| **Windows x86_64** | Native x86_64 Instructions | PE32+ Image, IAT, SEH `.pdata` | Windows NT Loader | **Tier 1 (Production)** |
| **Linux x86_64 / ARM64** | Native x86_64 / AArch64 | ELF64, `.eh_frame_hdr`, PT_LOAD | Linux Kernel (`sys_execve`) | **Tier 1 (Production)** |
| **macOS Apple Silicon / Intel** | AArch64 / x86_64 | Mach-O 64-bit, `LC_MAIN` | macOS `dyld` | **Tier 1 (Production)** |
| **WebAssembly** | WASM OpCodes | Core 2.0 / WASI Preview 1 Module | Wasmtime / Browser Engine | **Tier 1 (Production)** |
| **ARM Cortex-M (M0-M7)** | Thumb-2 Instructions | Linker Script, IVT, HEX/BIN | Microcontroller Hardware | **Tier 1 (Production)** |
| **RISC-V Bare-Metal (RV32/64)** | RISC-V Instructions | Trap Vector, `crt0`, SREC | RISC-V Hardware Core | **Tier 1 (Production)** |
| **Vulkan SPIR-V** | Compute Shader IR | SPIR-V 1.6 Binary & Bindings | Vulkan Driver / GPU | **Tier 2 (Structured)** |
| **NVIDIA CUDA** | GPU IR / PTX Text | Multi-Arch Fatbin (`0xBA55ED50`) | NVIDIA Driver & GPU | **Tier 2 (Structured)** |
| **AMD ROCm** | GPU IR | AMDGPU HSACO & Kernel Descriptors | AMD ROCR / HSA Runtime | **Tier 2 (Structured)** |
| **NPU (Arm Ethos-U / TPU)** | Neural IR | Command Stream (`ETHU` / `ADTP`) | NPU Firmware / Accelerator | **Tier 2 (Structured)** |
| **Quantum (QIR / OpenQASM)** | Quantum Gate AST | QIR Container (`QIRB`) & Pulses | Cloud QPU / `adeshrt` Simulator | **Tier 2 (Structured)** |

---

## 3. Detailed Phase Roadmap

### Phase 1: Foundation (COMPLETED)
- [x] 100% Safe-Rust native linker (`adeshlink`) with zero external C/C++ toolchain dependencies.
- [x] End-to-end executable binary generation: Windows PE32+ (`.exe`), Linux ELF64, macOS Mach-O, WebAssembly WASM.
- [x] Verified execution of compiled binaries on native Windows loader without external runtime DLLs.
- [x] Complete built-in toolchain CLI replacements (`ar`, `nm`, `objdump`, `readobj`, `size`, `strip`, `inspect`, `symbols`, `sections`, `relocations`, `deps`, `targets`, `version`).
- [x] Embedded Linker Script Engine (`MEMORY` & `SECTIONS` evaluation), Interrupt Vector Tables (`ArmCortexVectorTable`, `RiscvTrapVectorTable`), `crt0` startup synthesizers, and Flash firmware formats (Raw `.bin`, Intel `.hex`, Motorola `.srec`).
- [x] Formal Adesh ABI v1 specification, scope-aware RAII drop action tables (`.adesh.unwind_map`), and compiler-RT intrinsics (`memcpy`, `memset`, `memcmp`, `__multi3`, `__chkstk`, `__stack_chk_fail`).

### Phase 2: Native Codegen & Accelerator Independence (ACTIVE)
- [ ] Pure-Rust bit-level Machine Code Encoders for x86_64, AArch64, and RISC-V (eliminating external codegen dependencies).
- [ ] Pure-Rust Vulkan SPIR-V 1.6 compute shader instruction encoder with descriptor set decorations.
- [ ] Pure-Rust NVIDIA PTX / CUBIN multi-architecture fatbin generation.
- [ ] Pure-Rust AMDGPU ROCm HSACO code object packaging.
- [ ] High-speed contiguous buffer serialization and hash-indexed symbol tables for sub-millisecond compile & link times.

### Phase 3: Deep Neural & Quantum Toolchain
- [ ] MLIR dialect lowering pipeline (`affine`, `linalg`, `tensor` $\to$ hardware kernels).
- [ ] Arm Ethos-U & Google TPU systolic tile schedule emission.
- [ ] OpenQASM 3.0 / QIR emission with hardware coupling graph SWAP routing and state-vector simulation.
