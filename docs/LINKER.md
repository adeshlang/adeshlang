# Adesh Native Linker (`adeshlink`)

`adeshlink` is a self-contained, multi-format, zero-dependency native linker built into the Adesh toolchain.

---

## 🚀 Key Features

1. **Multi-Format Support:** ELF32/64, PE32/PE32+, Mach-O 64, WASM, ADOB v2.
2. **Dead-Code Elimination (GC Sections):** Purges unreferenced sections (`--enable-dce`).
3. **Identical Code Folding (ICF):** Safe merging of identical function bodies.
4. **Incremental Compilation & Caching:** Fingerprints inputs, section hashes, and symbol maps.
5. **Reproducible Builds:** Deterministic archive ordering, symbol sorting, and build-id generation.
6. **Binary Diagnostics:** Structured error diagnostics with diagnostic error codes (`LNK001` - `LNK016`).
