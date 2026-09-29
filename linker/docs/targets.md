# Target Abstraction (`targets`)

The Adesh Linker uses a comprehensive, decoupled Target Model representing hardware architecture, operating system, binary format, and ABI conventions.

---

## 1. Supported Target Triples

| Target String | Architecture | OS | Format | Pointer Width | Default Page Size |
|---|---|---|---|---|---|
| `x86_64-linux` | x86_64 | Linux | ELF64 | 64-bit | 4096 bytes (4KB) |
| `aarch64-linux` | AArch64 | Linux | ELF64 | 64-bit | 65536 bytes (64KB) |
| `arm-linux` | ARM (32) | Linux | ELF32 | 32-bit | 4096 bytes (4KB) |
| `riscv64-linux` | RISC-V 64 | Linux | ELF64 | 64-bit | 4096 bytes (4KB) |
| `riscv32-linux` | RISC-V 32 | Linux | ELF32 | 32-bit | 4096 bytes (4KB) |
| `x86_64-windows` | x86_64 | Windows | PE32+ | 64-bit | 4096 bytes (4KB) |
| `aarch64-windows`| AArch64 | Windows | PE32+ | 64-bit | 4096 bytes (4KB) |
| `x86_64-macos` | x86_64 | macOS | Mach-O 64 | 64-bit | 4096 bytes (4KB) |
| `aarch64-macos` | AArch64 | macOS | Mach-O 64 | 64-bit | 16384 bytes (16KB)|
| `wasm32-wasi` | WASM32 | Bare/WASI| WASM | 32-bit | 65536 bytes (64KB)|
| `wasm32-none` | WASM32 | None | WASM | 32-bit | 65536 bytes (64KB)|
| `wasm64` | WASM64 | None | WASM | 64-bit | 65536 bytes (64KB)|

---

## 2. ABI & Relocation Model

The target descriptor specifies:
- `pointer_width`: 32 or 64 bits.
- `endianness`: Little-Endian (`LE`) or Big-Endian (`BE`).
- `relocation_model`: Static, PIC (Position-Independent Code), or Dynamic.
- `code_model`: Small, Kernel, Medium, Large.
- `default_entry`: Target entry symbol (`_start`, `main`, `mainCRTStartup`, `_main`).
