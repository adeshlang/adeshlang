# Native Machine Codegen Architecture

Adesh native codegen translates LIR directly to machine code instructions and relocatable objects for x86_64, AArch64, and RISC-V64.

---

## ⚡ Machine Code Generation

- **Instruction Encoding:** Pure Rust instruction encoders for x86_64, AArch64, RISC-V.
- **Register Allocation:** Linear scan and graph coloring allocation.
- **Relocation Generation:** Emits target relocations (`Absolute64`, `PcRelative32`, `AArch64Call26`, `RiscvCall`).
- **Independent Testing:** Every codegen module is validated through assemble-link-execute roundtrip tests.
