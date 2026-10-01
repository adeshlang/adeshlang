# Adesh ABI & Calling Convention Specification Matrix

**Generated:** 2026-10-01  
**Scope:** Target ABI Rules, Register Usage, and Stack Alignment

---

## 1. Primary Calling Conventions

| ABI Name | Target Architecture | Integer Arg Registers | FP / Vector Arg Registers | Return Registers | Callee-Saved Registers | Shadow Space | Stack Alignment |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **System V AMD64** | x86_64 (Linux, macOS, BSD) | `RDI, RSI, RDX, RCX, R8, R9` | `XMM0 - XMM7` | `RAX, RDX` / `XMM0, XMM1` | `RBX, RSP, RBP, R12, R13, R14, R15` | None (128-byte red zone) | 16 Bytes |
| **Microsoft x64** | x86_64 (Windows) | `RCX, RDX, R8, R9` | `XMM0 - XMM3` | `RAX` / `XMM0` | `RBX, RBP, RDI, RSI, RSP, R12 - R15, XMM6 - XMM15` | 32 Bytes (Mandatory) | 16 Bytes |
| **AAPCS64** | AArch64 (Linux, macOS, Windows) | `X0 - X7` | `V0 - V7` | `X0, X1` / `V0, V1` | `X19 - X28, X29 (FP), X30 (LR), D8 - D15` | None | 16 Bytes |
| **Standard RISC-V**| RISC-V (RV64GC / RV32IMAC) | `a0 - a7` (`x10 - x17`) | `fa0 - fa7` (`f10 - f17`) | `a0, a1` / `fa0, fa1` | `s0 - s11` (`x8, x9, x18 - x27`), `fs0 - fs11` | None | 16 Bytes |
| **AAPCS32 (Thumb)**| ARM Cortex-M / ARM32 | `r0 - r3` | `s0 - s15` / `d0 - d7` | `r0, r1` / `s0, s1` | `r4 - r11, r13 (SP), r14 (LR)` | None | 8 Bytes |
| **WASM ABI** | WASM32 / WASM64 | Stack / WASM Params | WASM Params | Stack / Multi-Value | Controlled by WebAssembly VM | None | Native |

---

## 2. Foreign Function Interface (FFI) ABI Mapping

Adesh natively supports `extern "C"`, `extern "system"`, and `extern "native"` declarations:

* `extern "C"`: Maps to the platform's standard C ABI (System V on Linux/macOS, MS x64 on Windows).
* `extern "system"`: Maps to Win32 `stdcall` on x86, or Microsoft x64 on x86_64; standard C on POSIX.
* `extern "native"`: Adesh high-performance calling convention with multi-register returns and fat pointers.

---

## 3. Aggregate & Struct Passing Rules

1. **Primitives ($\le 8$ bytes)**: Passed directly in a single integer or floating-point register.
2. **Small Structs ($\le 16$ bytes)**:
   - System V AMD64: Split into eightbytes; classified as INTEGER / SSE and passed in up to 2 registers.
   - Microsoft x64: Passed by value if 1, 2, 4, or 8 bytes; otherwise passed by reference via an implicit pointer.
   - AAPCS64: Up to 16 bytes passed in consecutive `Xn` registers or Homogeneous Floating-point/Vector Aggregates (HFA/HVA) in `Vn`.
3. **Large Structs ($> 16$ bytes)**: Passed via hidden first pointer argument (`sret` / return slot) allocated on the caller stack.
