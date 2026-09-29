# PE/COFF Backend (`pe`)

The Portable Executable (PE) and Common Object File Format (COFF) backend enables native linking on Windows for x86_64, AArch64, and x86 architectures without relying on Microsoft `link.exe` or `lld-link`.

---

## 1. Supported Specifications

- **Formats**: PE32+ (64-bit Windows) and PE32 (32-bit Windows).
- **Artifact Types**:
  - Relocatable COFF Object Files (`.obj`)
  - Executable Files (`.exe`)
  - Dynamic Link Libraries (`.dll`)
- **Subsystems**:
  - `IMAGE_SUBSYSTEM_WINDOWS_CUI` (Console application)
  - `IMAGE_SUBSYSTEM_WINDOWS_GUI` (Graphical application)

---

## 2. Binary Layout

1. **MS-DOS Header & Stub**: Standard `MZ` header pointing to the PE signature at offset `0x3C`.
2. **PE Signature**: `PE\0\0` (`0x00004550`).
3. **COFF File Header**: Machine type (`0x8664` for AMD64, `0xAA64` for ARM64), section count, timestamp, symbol table pointer, and characteristics (`EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE`).
4. **Optional Header**: Magic (`0x020B` for PE32+), AddressOfEntryPoint, BaseOfCode, ImageBase (default: `0x140000000`), SectionAlignment (`0x1000`), FileAlignment (`0x200`), Subsystem, DllCharacteristics (`DYNAMIC_BASE | NX_COMPAT | HIGH_ENTROPY_VA | TERMINAL_SERVER_AWARE`), and Data Directory entries.
5. **Section Table**: Section headers defining `.text`, `.rdata`, `.data`, `.pdata`, `.xdata`, `.idata`, `.edata`, `.reloc`, and `.adesh`.

---

## 3. Import & Export Architecture

- **Import Directory Table (`.idata`)**: Constructs Image Import Descriptors, Import Lookup Tables (ILTs), Import Address Tables (IATs), and Hint/Name tables for DLL dependencies (`kernel32.dll`, `ntdll.dll`, `msvcrt.dll`, etc.).
- **Export Directory Table (`.edata`)**: Generates Export Directory, Export Address Table (EAT), Export Name Pointer Table (ENT), and Ordinal Table when `--shared` or `--export` flags are passed.
- **Base Relocations (`.reloc`)**: Generates base relocation blocks (`IMAGE_REL_BASED_DIR64` and `IMAGE_REL_BASED_HIGHLOW`) enabling Windows ASLR (Address Space Layout Randomization).
