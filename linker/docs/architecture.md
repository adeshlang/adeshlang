# Adesh Linker Architecture

The Adesh Linker (`adeshlink`) is engineered as an autonomous, self-contained linking infrastructure. Unlike conventional compiler toolchains that delegate to system linkers (such as GNU `ld`, LLVM `lld`, or Microsoft `link.exe`), the Adesh Linker performs all symbol resolution, section layout, relocation calculation, binary encoding, and executable packaging natively in Rust.

---

## 1. Architectural Philosophy

1. **Isolation from Frontend**: The linker has zero awareness of AST, HIR, LIR, syntax trees, or language frontend constructs. It operates exclusively on low-level object abstractions, symbol definitions, relocation records, and sections.
2. **Unified Core, Pluggable Backends**: A single format-agnostic linking engine handles input ingestion, symbol resolution, garbage collection, and address assignment. Format-specific emitters (ELF, PE, Mach-O, WASM) and architecture-specific handlers (x86_64, AArch64, ARM, RISC-V, WASM) plug cleanly into the core pipeline.
3. **Cross-Linking as a Fundamental Primitive**: Output format and target architecture are first-class parameters rather than host-derived defaults. A developer on Windows x86_64 can produce a Linux ELF or macOS Mach-O binary without foreign cross-compilation toolchain headers.
4. **Deterministic and Reproducible**: Given identical inputs, flags, and target triple, the output binary is guaranteed bit-for-bit identical regardless of execution time or host operating system.

---

## 2. Linking Pipeline

```text
┌────────────────────────────────────────────────────────┐
│ 1. Ingestion & Validation                              │
│    • Ingest object files (.o / .obj) and archives (.a) │
│    • Parse headers, validate magic, endianness & ABI  │
│    • Validate .adesh.meta metadata consistency         │
└──────────────────────────┬─────────────────────────────┘
                           │
┌──────────────────────────▼─────────────────────────────┐
│ 2. Symbol Resolution & Archive Extraction              │
│    • Register global, local, weak, and COMDAT symbols  │
│    • Lazily extract required object members from .a    │
│    • Resolve undefined symbols or diagnose duplicates  │
└──────────────────────────┬─────────────────────────────┘
                           │
┌──────────────────────────▼─────────────────────────────┐
│ 3. Optimization & Optimization Passes                  │
│    • Dead Section Elimination (--gc-sections)          │
│    • Identical Code Folding (--icf=safe / --icf=all)   │
└──────────────────────────┬─────────────────────────────┘
                           │
┌──────────────────────────▼─────────────────────────────┐
│ 4. Layout & Memory Mapping                             │
│    • Merge input sections into target segments         │
│    • Calculate Virtual Addresses and File Offsets      │
│    • Allocate GOT, PLT, and TLS regions where required │
└──────────────────────────┬─────────────────────────────┘
                           │
┌──────────────────────────▼─────────────────────────────┐
│ 5. Relocation Calculation & Patching                   │
│    • Compute absolute, PC-relative, and branch values  │
│    • Check for bit overflows and boundary alignment    │
│    • Patch target image buffers                        │
└──────────────────────────┬─────────────────────────────┘
                           │
┌──────────────────────────▼─────────────────────────────┐
│ 6. Binary Format Emission                              │
│    • Construct file headers, program headers, tables   │
│    • Emit ELF / PE-COFF / Mach-O / WASM artifact       │
│    • Optionally write Link Map and Report              │
└────────────────────────────────────────────────────────┘
```

---

## 3. Core Subsystems

| Subsystem | Module | Description |
|---|---|---|
| **Target Model** | `target.rs` | Target triple representation, pointer width, endianness, page sizes, alignment constraints. |
| **Object File** | `object.rs`, `object/*` | Format-agnostic object representation for sections, symbols, and relocations. |
| **Symbol Resolver** | `resolver.rs`, `symbol.rs` | Global symbol table, weak binding rules, duplicate detection, archive on-demand resolution. |
| **Layout Engine** | `layout.rs`, `section.rs` | Virtual address assignment, segment placement, section ordering, permission validation (W^X). |
| **Relocation Engine** | `relocation.rs`, `arch/*` | Relocation dispatch, target formula evaluation, bitmask application, overflow detection. |
| **Garbage Collector** | `gc.rs` | Section reachability graph traversal from entry point and explicit export roots. |
| **ICF Engine** | `icf.rs` | Content hashing and structural equivalence validation for duplicate function merging. |
| **Metadata Engine** | `metadata.rs` | Adesh binary contract (`.adesh.meta`) parser and validator. |
| **Cache & Incremental**| `cache.rs`, `incremental.rs` | Dependency hashing and persistent state caching for sub-millisecond incremental relinks. |
| **Diagnostics** | `diagnostics.rs`, `error.rs`| Structured error codes (`LNK001` - `LNK015`) with actionable diagnostics and suggestion notes. |
