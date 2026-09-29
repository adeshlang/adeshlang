# ELF Backend (`elf`)

The Adesh Linker ELF backend implements autonomous parsing, layout, relocation, and emission of Executable and Linkable Format (ELF) binaries for Linux, FreeBSD, and bare-metal platforms across 64-bit and 32-bit architectures.

---

## 1. Supported Specifications

- **ELF Formats**: ELF64 and ELF32 (Little-Endian and Big-Endian).
- **File Types**:
  - `ET_REL` (Relocatable Object File)
  - `ET_EXEC` (Position-Dependent Executable)
  - `ET_DYN` (Position-Independent Executable / Shared Object)
- **Machine Architectures**:
  - `EM_X86_64` (AMD64 / x86_64)
  - `EM_AARCH64` (ARM64)
  - `EM_ARM` (ARM 32-bit)
  - `EM_RISCV` (RISC-V 32-bit and 64-bit)

---

## 2. Segments and Sections

The ELF layout engine arranges sections into standard memory segments (`PT_LOAD`):

1. **Read-Only / Executable Segment (`RX`)**:
   - `ELF Header` & `Program Headers`
   - `.init` / `.text` (Machine code)
   - `.rodata` (Constant strings, jump tables, read-only structures)
   - `.eh_frame_hdr` / `.eh_frame` (Exception handling frames)
   - `.adesh.meta` (Adesh toolchain binary metadata)
2. **Read-Write Segment (`RW`)**:
   - `.data` (Initialized mutable variables)
   - `.bss` (Zero-initialized memory, occupies 0 bytes on disk)
   - `.tdata` / `.tbss` (Thread-local storage definitions)
   - `.got` / `.got.plt` (Global Offset Tables)
3. **Special Segments**:
   - `PT_GNU_STACK`: Controls stack executability flags (default: non-executable `RW`).
   - `PT_NOTE`: Contains GNU build-id notes (`.note.gnu.build-id`).
   - `PT_TLS`: Specifies thread-local storage template bounds.

---

## 3. Relocation Handling

The ELF backend supports both `SHT_RELA` (with explicit addends) and `SHT_REL` (with implicit addends in section payload). Architecture-specific relocations (e.g., `R_X86_64_64`, `R_X86_64_PC32`, `R_X86_64_PLT32`, `R_AARCH64_CALL26`, `R_RISCV_CALL`) are dispatched through the central relocation handler.
