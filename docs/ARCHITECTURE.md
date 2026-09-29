# Adesh Self-Contained Native Toolchain Architecture

The Adesh native toolchain is built as a self-contained, end-to-end native compilation and linking pipeline. It operates without requiring external toolchains such as LLVM, LLD, Clang, GCC, GNU binutils, or MSVC link.exe during normal compilation.

---

## 🏛️ End-to-End Pipeline Architecture

```
Adesh Source (.adesh)
    ↓
Adesh Frontend (Lexer & Parser)
    ↓
AST (Abstract Syntax Tree)
    ↓
HIR (High-Level Intermediate Representation)
    ↓
Ownership / Borrow / Lifetime Analysis
    ↓
Typed IR / VIR (Value SSA Representation)
    ↓
Optimization & Canonicalization Passes
    ↓
Adesh LIR (Low-Level IR)
    ↓
Native Codegen (x86_64, AArch64, RISC-V64)
    ↓
Adesh Object Format (ADOB v2 / Native Object)
    ↓
adeshlink (Adesh Native Linker Engine)
    ↓
Executable / Shared Library / Static Library / WASM / Firmware
```

---

## 🔧 Core Components

1. **Compiler Frontend:** Parses surface syntax, builds AST, and lowers to HIR.
2. **Analysis Passes:** Performs compile-time ownership, borrow checking, and type inference.
3. **Native Codegen:** Generates machine code and relocatable objects directly.
4. **adeshlink Engine:** Resolves symbols, computes relocations, garbage-collects dead sections (`--enable-dce`), folds identical functions (ICF), and emits final native binaries (ELF, PE, Mach-O, WASM, ADOB v2).
5. **Runtime System:** Self-contained runtime library (`ADESH_RUNTIME_ABI_V1`) providing allocation, panic unwinding, and FFI interop.
