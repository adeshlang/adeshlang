# AArch64 Native Backend Specification

## 1. Overview

The AArch64 backend targets ARMv8-A and ARMv9-A processors (Linux, macOS Apple Silicon, Android, iOS, Windows ARM64).

---

## 2. Features & Instruction Set

* **Registers**: 31 general-purpose registers (`X0` - `X30`) and `SP`.
* **Stack Management**: Frame setup with `STP X29, X30, [SP, #-16]!` and teardown with `LDP X29, X30, [SP], #16`.
* **Instruction Encodings**: Fixed 32-bit width encodings for arithmetic (`ADD`, `SUB`), branch (`B`, `BL`, `CBZ`, `CBNZ`), and return (`RET`).
* **Calling Convention**: AAPCS64 (Arguments in `X0` - `X7`, Return values in `X0`, `X1`, Callee-saved `X19` - `X28`).
* **Relocations**: `AArch64_Call26`, `AArch64_AdrPage21`, `AArch64_AddAbsLo12`.
