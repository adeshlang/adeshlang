# Adesh Native Linker (`adeshlink`)

`adeshlink` is a high-performance, self-contained, multi-format, zero-dependency native binary linker built into the Adesh toolchain. It requires **no external dependencies** (no LLVM, Clang, GCC, MSVC, or GNU binutils) to produce native platform binaries across Windows (PE32+), Linux (ELF64), macOS (Mach-O 64), WebAssembly (WASM), and Bare-Metal / Embedded MCUs.

---

## 🚀 Key Architectural Features

1. **Universal Multi-Format Binary Emission:**
   - **Windows:** PE32 / PE32+ `.exe` executables and `.dll` dynamic libraries with import directories, base relocations, export directories, and optional digital certificate sections.
   - **Linux & BSD:** ELF32 / ELF64 executables and shared objects (`.so`) with dynamic symbol tables, program headers, and GNU hash/notes.
   - **macOS:** Mach-O 64-bit Mach headers, segment load commands (`__TEXT`, `__DATA`, `__LINKEDIT`), dysymtab, and code signature load commands.
   - **WebAssembly:** Standalone WASM binaries with type, function, table, memory, global, export, element, code, and data sections.
   - **Embedded / Bare Metal:** Memory mapped sections with Interrupt Vector Tables (IVT), flash scripts, and startup code.

2. **First-Class ADOB Ingestion:**
   - Native object ingestion of ADOB (Adesh Native Object Binary) files containing target descriptors, sections, relocations, safety metadata, thread-safety models, and security flags.

3. **Compiler-RT & Adesh Runtime Intrinsics Engine:**
   - Built-in synthesis of arithmetic, memory, and runtime primitives (`__multi3`, `memcpy`, `memset`, `memcmp`, `memmove`, `__chkstk`, `__stack_chk_guard`, `__stack_chk_fail`, `adesh_rt_*`, `adesh_*`).
   - Enables zero-dependency static binary generation directly from object files without requiring external C runtime libraries (`libc.a` or `msvcrt.lib`).

4. **Dead-Code Elimination (GC Sections):**
   - Resolves symbol reference graphs starting from root symbols (such as `main` or entry point) and eliminates unreferenced functions and data sections (`--gc-sections`).

5. **Identical Code Folding (ICF):**
   - Identifies and folds identical functions and read-only constants into a single memory block (`--icf=safe` or `--icf=all`).

6. **Reproducible & Hardened Binary Layouts:**
   - Bit-for-bit deterministic binary emission (`--deterministic`) with sorted symbol tables, deterministic timestamp headers, and reproducible SHA-256 build IDs.
   - Hardened security layouts: W^X page protections, Data Execution Prevention (DEP/NX), Position Independent Executables (PIE/ASLR), and read-only relocations (RELRO).

7. **Telemetry & Link Maps:**
   - Detailed text or JSON link layout maps (`--map=output.map --map-format=json`).
   - Comprehensive link telemetry reports with timing, section sizes, memory savings, and symbol resolution traces (`--report`).

---

## 🛠️ CLI Usage & Options

`adeshlink` can be invoked as a standalone CLI tool or driven automatically by `adesh build`.

```bash
# Link object files into a standalone native executable
adeshlink main.adob runtime.adob -o app.exe

# Link with specific target triple and output format
adeshlink -o app.elf --target=x86_64-unknown-linux-gnu main.o helper.o

# Enable dead-code elimination, ICF, and link map generation
adeshlink -o app.exe --gc-sections --icf=all --map=app.map main.adob

# Built-in replacement tools for binutils / llvm-tools
adeshlink ar rcs libmath.a add.o sub.o   # Static archive manager
adeshlink nm app.exe                     # List symbols
adeshlink objdump -h app.exe             # Inspect section headers
adeshlink readobj app.exe                # Detailed object inspection
adeshlink size app.exe                   # Berkeley/SysV size summary
adeshlink strip -s app.exe               # Strip debug symbols
```

---

## ⚙️ Diagnostic Codes

`adeshlink` emits clear, structured error codes:

| Code | Category | Description |
|---|---|---|
| `LNK001` | Symbol Resolution | Undefined symbol(s) detected during resolution |
| `LNK002` | Duplicate Symbol | Multiply defined symbol collision |
| `LNK003` | Relocation Out of Range | Relocation offset exceeds target field capacity |
| `LNK004` | Unsupported Relocation | Target architecture does not support relocation type |
| `LNK005` | Invalid Object Format | Malformed input object binary or corrupt magic |
| `LNK006` | Section Overlap | Virtual address collision during section layout |
| `LNK007` | Target Mismatch | Mixed architecture object files detected |
