# WebAssembly Backend (`wasm`)

Unlike traditional linkers that treat WebAssembly as an ELF-derivative, the Adesh Linker implements a dedicated WASM binary pipeline conforming to the W3C WebAssembly Core Specification and WASI (WebAssembly System Interface).

---

## 1. Supported Specifications

- **Format**: W3C WebAssembly Binary Format (Magic `\0asm`, Version `0x01`).
- **Target Profiles**:
  - `wasm32-wasi` (WASI Preview 1 system interface: `fd_write`, `args_get`, `proc_exit`, etc.)
  - `wasm32-none` (Standalone/embedded browser runtime)
  - `wasm64` (Memory64 profile)

---

## 2. Binary Sections

1. **Type Section (ID 1)**: Function signatures (`valtype` vectors).
2. **Import Section (ID 2)**: Host functions, memory, tables, and globals imported from the environment.
3. **Function Section (ID 3)**: Type indices for all module-defined functions.
4. **Table Section (ID 4)**: Function pointers and indirect call dispatch tables.
5. **Memory Section (ID 5)**: Linear memory limits (initial pages, maximum pages).
6. **Global Section (ID 6)**: Mutable and immutable global variables (e.g., stack pointer `__stack_pointer`).
7. **Export Section (ID 7)**: Public functions, memory (`memory`), and globals exported to host.
8. **Start Section (ID 8)**: Optional initialization entry function.
9. **Element Section (ID 9)**: Table segment initializers.
10. **Code Section (ID 10)**: Function bodies containing local declarations and WASM bytecodes.
11. **Data Section (ID 11)**: Static data segments placed at linear memory offsets.
12. **Custom Section (ID 0)**: Metadata sections including `name`, `producers`, and `.adesh.meta`.
