# Adesh Binary ABI Specification

This document specifies the Application Binary Interface (ABI) rules, layout standards, calling conventions, and symbol mangling used in Adesh.

---

## 🎯 Calling Conventions

- **x86_64 System V AMD64 ABI (Linux, macOS, BSD):** First 6 integer/pointer args in `RDI, RSI, RDX, RCX, R8, R9`; float args in `XMM0-XMM7`.
- **x86_64 Microsoft x64 ABI (Windows):** First 4 integer/pointer args in `RCX, RDX, R8, R9` with 32-byte shadow space; float args in `XMM0-XMM3`.
- **AArch64 AAPCS64 (Linux, macOS, Windows):** Integer args in `X0-X7`, float args in `V0-V7`.
- **RISC-V LP64D (Linux):** Integer args in `A0-A7`, float args in `FA0-FA7`.

---

## 📐 Data Layout & Alignment (`repr(C)`)

| Type | Size (Bytes) | Alignment | C Counterpart |
|------|-------------|-----------|---------------|
| `i8` / `u8` | 1 | 1 | `int8_t` / `uint8_t` |
| `i16` / `u16` | 2 | 2 | `int16_t` / `uint16_t` |
| `i32` / `u32` | 4 | 4 | `int32_t` / `uint32_t` |
| `i64` / `u64` | 8 | 8 | `int64_t` / `uint64_t` |
| `f32` | 4 | 4 | `float` |
| `f64` | 8 | 8 | `double` |
| `bool` | 1 | 1 | `bool` |
| `ptr` | 4 / 8 | 4 / 8 | `void*` |

Field offsets follow standard C struct padding rules (`offsetof` padded to match maximum member alignment).

---

## 🔄 Parallel Move Resolution & Simultaneous Assignment

Argument passing, register shuffling, and return value movements operate under strict simultaneous assignment semantics:
- **Dependency Chains**: Resolved in topological dependency order (no clobbered sources).
- **Register Cycles (2-cycles/swaps, N-cycles)**: Detected via dependency graph analysis and broken using safely selected caller-saved scratch registers (e.g., `R10`, `R11`, `RAX` on x86-64) avoiding live ranges and reserved registers (`RSP`, `RBP`).
- **Memory/Stack Transfers**: Stack-to-stack moves are lowered through intermediate scratch registers to comply with machine instruction constraints.
- **Timing Boundary**: Moves are captured as `MachineInstruction::ParallelMove` before register allocation, allocated and rewritten by `LinearScanAllocator`, and expanded after register allocation with exact physical register knowledge.

---

## ⚡ Floating-Point Register Model & SSE2 ABI

AdeshLang native backend provides native IEEE-754 single (`f32`) and double (`f64`) precision floating-point support using x86-64 SSE / SSE2 scalar instructions:

### Register Class Partitioning
- **GPR Class (`RegisterClass::Gpr`)**: `RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8..R15` (Encoding 0..15).
- **Float Class (`RegisterClass::Float`)**: `XMM0..XMM15` (Encoding 16..31).
- Register allocation via `LinearScanAllocator` allocates virtual intervals strictly against their matching `RegisterClass`, preventing cross-class register pollution.

### Floating-Point Calling Conventions
- **Windows x64 ABI**:
  - Arguments 1–4 are passed in position-dependent slots: `RCX/XMM0`, `RDX/XMM1`, `R8/XMM2`, `R9/XMM3`.
  - Floating-point return value is returned in `XMM0`.
  - Caller-saved: `XMM0..XMM5`.
  - Callee-saved (non-volatile): `XMM6..XMM15` (preserved with 16-byte stack alignment during function prologue/epilogue).
  - Parallel move cycle-breaking scratch: `XMM15`.
- **System V AMD64 ABI**:
  - First 8 float arguments in `XMM0..XMM7`.
  - Floating-point return values in `XMM0` (and `XMM1` for complex/pair).
  - All `XMM0..XMM15` are caller-saved.
  - Varargs pass number of vector registers used in `RAX` (`AL`).

### Instruction Lowering
- **Arithmetic**: `ADDSS`/`ADDSD`, `SUBSS`/`SUBSD`, `MULSS`/`MULSD`, `DIVSS`/`DIVSD`.
- **Negation**: Bitwise sign-bit inversion via `XORPS`/`XORPD` with `0x8000000000000000`.
- **Comparisons**: `UCOMISS`/`UCOMISD` with unordered (NaN) parity handling (`SETE & SETNP`, `SETNE | SETP`, `SETB & SETNP`, `SETBE & SETNP`, `SETA`, `SETAE & SETNP`).
- **Conversions**:
  - `i32/i64 -> f32/f64`: `CVTSI2SS`, `CVTSI2SD`.
  - `f32/f64 -> i32/i64`: `CVTTSS2SI`, `CVTTSD2SI` (truncating towards zero).
  - `f32 <-> f64`: `CVTSS2SD`, `CVTSD2SS`.
- **Memory Movement & Spills**: Scalar loads and stores `MOVSS` (4-byte) and `MOVSD` (8-byte) with full stack slot isolation.


