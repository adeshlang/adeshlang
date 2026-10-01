# Adesh Native Object Binary (`ADOB`) Specification

**ADOB** (*Adesh Native Object Binary*) is the target-neutral, relocatable native object container format for the Adesh ecosystem. It replaces platform-coupled object representations (ELF, COFF, Mach-O) in the compilation stage, serving as a unified intermediate representation for CPUs, GPUs, NPUs, TPUs, and accelerators.

---

## 🏗️ Binary Structure

Every ADOB file is laid out as a contiguous byte sequence with strictly validated bounds and alignment:

```text
┌────────────────────────────────────────────────────────┐
│                      ADOB Header                       │
│     Magic: "ADOB" | Version: 1.0.0 | Flags: u32        │
├────────────────────────────────────────────────────────┤
│                   Target Descriptor                    │
│   Compute Device | Architecture | OS | ABI | Width     │
├────────────────────────────────────────────────────────┤
│                   Safety & Security                    │
│    Memory Safety | CFI (CET/BTI) | Stack Canaries      │
├────────────────────────────────────────────────────────┤
│                  Thread-Safety & TLS                   │
│     Memory Ordering Model | TLS Model | Threading      │
├────────────────────────────────────────────────────────┤
│                  Section Headers Table                 │
│      .text, .data, .rodata, .bss, .tdata, .tbss        │
├────────────────────────────────────────────────────────┤
│                   Symbol Table Table                   │
│      Function, Object, TLS, Import, Export, Weak       │
├────────────────────────────────────────────────────────┤
│                   Relocation Records                   │
│      Absolute, Relative, PLT, GOT, TLS Relocations     │
├────────────────────────────────────────────────────────┤
│                   Payload Data Bytes                   │
│          Section Raw Bytes (Code, Constants)           │
└────────────────────────────────────────────────────────┘
```

---

## 🛡️ Header & Descriptors

### 1. File Magic & Versioning
- **Magic:** `0x41 0x44 0x4F 0x42` (`"ADOB"`)
- **Version:** `Major (u8)`, `Minor (u8)`, `Patch (u8)` (Current: `1.0.0`)
- **Header Flags:** Bitflags for `EXECUTABLE`, `RELOCATABLE`, `SHARED`, `STRIPPED`, `HARDENED`, `DETERMINISTIC`.

### 2. Target Descriptor
ADOB decouples the compute execution device from target architecture:
- **Compute Device:** `Cpu`, `Gpu`, `Npu`, `Tpu`, `Dsp`, `Fpga`, `Accelerator`, `Embedded`, `Custom`.
- **Architecture:** `X86_64`, `X86`, `Arm`, `AArch64`, `RiscV32`, `RiscV64`, `PowerPc`, `PowerPc64`, `Mips`, `Wasm32`, `Wasm64`, `Gpu(...)`, `Npu(...)`, `Tpu(...)`, `Embedded(...)`, `Custom(...)`.
- **Operating System:** `None` (Bare metal), `Linux`, `Windows`, `MacOs`, `FreeBsd`, `Android`, `Ios`, `Wasi`.
- **ABI:** `SystemV`, `WindowsX64`, `Aapcs64`, `Aapcs32`, `Riscv`, `Wasm`, `Custom`.
- **Pointer Width:** 16-bit, 32-bit, 64-bit, 128-bit.
- **Endianness:** Little Endian (`0`), Big Endian (`1`).

---

## 🔒 Safety, Security, and Optimization Metadata

ADOB objects encode critical runtime verification metadata directly inside the object:

- **Safety Metadata:**
  - `memory_safety_level`: Strict, Safe, Unsafe.
  - `bounds_checking_eliminated`: Indicates whether Array BCE pass was applied.
  - `stack_canaries_enabled`: Confirms stack canary guards (`__stack_chk_guard`) are present.
  - `cfi_enabled`: Control Flow Integrity validation (`endbr64` / ARM BTI).
  - `w_xor_x_enforced`: W^X page safety verification.
- **Thread Safety Metadata:**
  - `memory_order_model`: `Relaxed`, `Acquire`, `Release`, `AcqRel`, `SeqCst`.
  - `tls_model`: `None`, `LocalExec`, `InitialExec`, `GeneralDynamic`, `LocalDynamic`.
  - `atomic_lowering`: Tracks atomic barrier instructions.
- **Optimization Metadata:**
  - Optimization level (0, 1, 2, 3, Os, Oz).
  - Peephole optimization passes applied.
  - Dead Code Elimination (DCE) flags.
  - Constant folding and strength reduction indicators.

---

## 📦 Multi-Architecture Fat Bundles (`AdobBundle`)

ADOB supports packaging multiple target architectures into a single universal bundle (`.adob` fat binary):

```rust
let mut bundle = AdobBundle::new();
bundle.add_member(x86_64_obj);
bundle.add_member(aarch64_obj);
bundle.add_member(riscv64_obj);
bundle.add_member(gpu_kernel_obj);

let bytes = bundle.to_bytes()?;
```

---

## 🔍 CLI Commands for ADOB

```bash
# Emit an ADOB object from source
adesh build input.adesh --emit-adob -o module.adob

# Inspect ADOB header, target descriptor, and metadata
adesh adob inspect module.adob

# Run strict structural and memory validation checks
adesh adob validate module.adob

# Dump symbol table
adesh adob dump-symbols module.adob

# Dump section headers
adesh adob dump-sections module.adob

# Dump relocation table
adesh adob dump-relocations module.adob
```
