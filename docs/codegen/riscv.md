# RISC-V Native Backend Specification

## 1. Overview

The RISC-V backend supports RV32I and RV64I with modular extension flags:

* **Base**: `RV32I`, `RV64I`
* **Extensions**: `M` (Integer Multiplication/Division), `A` (Atomics), `F` (Single-precision Floating Point), `D` (Double-precision Floating Point), `C` (Compressed 16-bit instructions), `V` (Vector extension).

---

## 2. ABI & Relocation Model

* **Calling Convention**: RISC-V standard ABI (Arguments in `a0` - `a7`, Return values in `a0`, `a1`, Saved registers `s0` - `s11`).
* **Relocations**: `RiscV_Call`, `RiscV_PcrelHi20`, `RiscV_PcrelLo12I`, `RiscV_PcrelLo12S`, `RiscV_RvcBranch`, `RiscV_RvcJump`.
* **Stack Management**: Standard 16-byte aligned stack frames using `addi sp, sp, -imm` and `sd ra, offset(sp)`.
