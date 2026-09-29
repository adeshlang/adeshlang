# Security & Hardening (`security`)

The Adesh Linker enforces rigorous security properties across binary layout, memory segment permissions, and defensive input parsing.

---

## 1. Defensive Parsing & Memory Safety

All binary format decoders (ELF, PE, Mach-O, WASM, Archive) operate under zero-trust assumptions:
- **Strict Bounds Checking**: All slice and offset accesses are bounds-checked. Corrupted section headers, truncated string tables, and malformed symbols produce structured `LinkError` diagnostics without panicking.
- **Arithmetic Overflow Checks**: Offset and size calculations utilize checked arithmetic to prevent integer overflow exploits.
- **Resource Constraints**: Maximum section count and symbol count limits prevent denial-of-service via resource exhaustion.

---

## 2. Binary Hardening Flags (`--hardened`)

When `--hardened` is enabled:
1. **W^X Enforcement (Write XOR Execute)**: No memory segment may be simultaneously writable and executable. Code sections are `RX`, data sections are `RW`, read-only data is `R`.
2. **Non-Executable Stack (`PT_GNU_STACK` / `IMAGE_DLLCHARACTERISTICS_NX_COMPAT`)**: Explicitly sets non-executable stack flags.
3. **Address Space Layout Randomization (ASLR)**:
   - Linux: Positions binaries for PIE (`ET_DYN`).
   - Windows: Sets `DYNAMIC_BASE` and `HIGH_ENTROPY_VA`.
   - macOS: Generates position-independent executable with `MH_PIE`.
4. **Read-Only Relocations (RELRO)**: Emits metadata for GNU RELRO support.
