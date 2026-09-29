# Mach-O Backend (`macho`)

The Mach-O backend provides native binary emission for macOS, iOS, and Darwin platforms supporting 64-bit architectures (`x86_64` and `ARM64`/Apple Silicon).

---

## 1. Supported Specifications

- **Format**: Mach-O 64-bit (`MH_MAGIC_64` = `0xFEEDFACF`).
- **File Types**:
  - `MH_OBJECT` (Object file)
  - `MH_EXECUTE` (Main executable)
  - `MH_DYLIB` (Dynamic shared library)
- **CPUs**:
  - `CPU_TYPE_X86_64` (`0x01000007`)
  - `CPU_TYPE_ARM64` (`0x0100000C`)

---

## 2. Load Commands and Segments

The linker generates standard Apple Darwin load commands:

1. `LC_SEGMENT_64`:
   - `__PAGEZERO`: 4GB unmapped guard segment preventing NULL pointer dereferences.
   - `__TEXT`: Read-only/executable segment containing `__text`, `__const`, `__cstring`, and `__unwind_info`.
   - `__DATA`: Read-write segment containing `__data`, `__bss`, `__got`, and `__la_symbol_ptr`.
   - `__LINKEDIT`: Raw metadata segment storing symbol tables, string tables, dynamic symbols, and code signatures.
2. `LC_MAIN`: Entry point specification replacing legacy `LC_UNIXTHREAD`.
3. `LC_LOAD_DYLINKER`: Dynamic linker path (`/usr/lib/dyld`).
4. `LC_SYMTAB` & `LC_DYSYMTAB`: Symbol and dynamic symbol tables.
5. `LC_LOAD_DYLIB`: Linked framework and library references (`libSystem.B.dylib`).
