# Adesh Target Triples & Architecture Matrix

This document defines the canonical target triple format, architecture mappings, calling conventions, and supported compute devices across the Adesh native compilation pipeline.

---

## 1. Target Triple Format

Adesh uses an LLVM/Rust-compatible canonical target triple schema extended for heterogeneous devices:

```text
<architecture>-<vendor>-<operating_system>-<abi/environment>
```

When specifying heterogeneous offload or device-specific backends:
```text
<device>:<architecture>-<vendor>-<operating_system>-<abi>
```

### Supported Standard Triples

| Target Triple | Compute Device | Architecture | OS | Default ABI | Output Platform |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | CPU | `X86_64` | `Windows` | `WindowsX64` | PE/COFF `.exe`, `.obj`, `.adob` |
| `x86_64-unknown-linux-gnu` | CPU | `X86_64` | `Linux` | `SystemVX64` | ELF, `.adob` |
| `aarch64-unknown-linux-gnu` | CPU | `AArch64` | `Linux` | `Aapcs64` | ELF, `.adob` |
| `aarch64-apple-darwin` | CPU | `AArch64` | `MacOS` | `Aapcs64` | Mach-O, `.adob` |
| `riscv64gc-unknown-linux-gnu` | CPU | `RiscV64` | `Linux` | `RiscV64` | ELF, `.adob` |
| `riscv32imac-unknown-none-elf` | CPU / MCU | `RiscV32` | `Unknown` (Bare Metal) | `RiscV32` | ELF, `.adob` |
| `thumbv7em-none-eabihf` | Embedded CPU | `Arm` | `Unknown` (Bare Metal) | `Aapcs32` | ELF, `.adob` |
| `wasm32-unknown-unknown` | Virtual CPU | `Wasm32` | `Unknown` | `Wasm32` | WebAssembly `.wasm`, `.adob` |
| `wasm64-unknown-unknown` | Virtual CPU | `Wasm64` | `Unknown` | `Wasm64` | WebAssembly `.wasm`, `.adob` |
| `nvptx64-nvidia-cuda` | GPU | `Gpu(Cuda)` | `Unknown` | `Custom("cuda")` | PTX / SASS in `.adob` |
| `amdgcn-amd-amdhsa` | GPU | `Gpu(Rocm)` | `Unknown` | `Custom("amdhsa")` | HSACO in `.adob` |
| `npu-generic-tensor` | NPU | `Npu(Generic)`| `Unknown` | `Custom("npu")` | Quantized Model/Ops in `.adob` |
| `tpu-v4-google` | TPU | `Tpu(Generic)`| `Unknown` | `Custom("tpu")` | XLA HLO / Bytecode in `.adob` |

---

## 2. Target Descriptors in ADOB

Every generated ADOB file embeds a `TargetDescriptor`:

```rust
pub struct TargetDescriptor {
    pub architecture: Architecture,
    pub operating_system: OperatingSystem,
    pub environment: Environment,
    pub abi: Abi,
    pub object_format: ObjectFormat,
    pub pointer_width: PointerWidth,
    pub endianness: Endianness,
    pub features: TargetFeatures,
}
```

This ensures `adeshlink` and runtime loaders validate compatibility before linking or loading modules.

---

## 3. Register Allocator & Calling Convention Selection

The codegen automatically binds the target to its respective calling convention and physical register set:

| Architecture | Calling Convention Class | Allocatable GPRs | Callee-Saved GPRs |
| :--- | :--- | :--- | :--- |
| `x86_64 (Windows)` | `WindowsX64CallingConvention` | RCX, RDX, R8, R9, R10, R11, RAX, RBX, RSI, RDI, R12-R15 | RBX, RSI, RDI, RBP, R12-R15 |
| `x86_64 (System V)`| `SystemVX64CallingConvention` | RDI, RSI, RDX, RCX, R8, R9, RAX, R10, R11, RBX, R12-R15 | RBX, RBP, R12-R15 |
| `AArch64` | `Aapcs64CallingConvention` | X0-X15, X16-X17 (scratch), X19-X28 | X19-X28, FP (X29), LR (X30) |
| `RISC-V 64/32` | `RiscVCallingConvention` | A0-A7, T0-T6, S1-S11 | S0/FP, S1-S11 |
| `ARM32 / Thumb` | `Aapcs32CallingConvention` | R0-R3, R4-R11 | R4-R11, LR (R14) |
| `WASM32/64` | `WasmCallingConvention` | Local variables (virtual stack model) | N/A |

---

## 4. Multi-Architecture Bundling (Fat Binaries)

For heterogeneous deployments (e.g. Host CPU + CUDA GPU Kernel + NPU Model), Adesh packages multiple target ADOBs inside an `AdobBundle`:

```text
┌──────────────────────────────────────────────┐
│                  AdobBundle                  │
│                                              │
│  [Entry 0] Host CPU (x86_64-pc-windows-msvc) │
│  [Entry 1] Device GPU (nvptx64-nvidia-cuda)  │
│  [Entry 2] Device NPU (npu-generic-tensor)   │
└──────────────────────────────────────────────┘
```

The runtime loader or linker dynamically queries `bundle.find_target(&target)` or unpacks the device slices at startup.
