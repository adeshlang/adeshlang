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
