# Adesh Linker (`adeshlink`)

A self-contained, multi-format, multi-architecture native linker for the Adesh ecosystem.

Inspired by the philosophy of Go's `cmd/link`, `adeshlink` is a standalone compiler-infrastructure tool designed to link native object files into final executable binaries, shared libraries, static archives, and WebAssembly artifacts without requiring external system linkers (such as GNU `ld`, LLVM `lld`, Microsoft `link.exe`, Apple `ld64`) or toolchains (GCC/Clang).

---

## Key Capabilities

- **Zero External Toolchain Dependencies**: Self-contained ELF, PE/COFF, Mach-O, and WASM binary parsers, writers, relocation engines, and layout generators implemented entirely in Rust.
- **Multi-Format & Multi-Architecture**:
  - **Formats**: ELF32/ELF64, PE32/PE32+ (COFF), Mach-O 64-bit, WASM32/WASM64, and Static Archives (`.a` / `.lib`).
  - **Architectures**: x86_64, AArch64, ARM, RISC-V 64/32, WASM32/WASM64, x86.
- **Cross-Linking by Default**: Target architecture is fully decoupled from the host operating system. Compile on Windows, link for Linux x86_64 or macOS ARM64 seamlessly.
- **Advanced Optimization & Dead-Code Elimination**:
  - Section Garbage Collection (`--gc-sections`) with symbol reachability analysis.
  - Identical Code Folding (`--icf=safe`, `--icf=all`).
- **Adesh Link Metadata (`.adesh.meta`) Validation**: ABI versioning, compiler versioning, runtime requirements, and capability verification.
- **Deterministic Builds**: Bit-for-bit reproducible binary outputs with `--deterministic` and reproducible `--build-id`.
- **Intelligent Diagnostics**: Clear, structured linker error codes (`LNK001` through `LNK015`) explaining missing symbols, duplicate symbols, relocation overflows, and ABI mismatches with context.
- **Inspection Tools**: Built-in object and binary inspection subcommands (`inspect`, `symbols`, `sections`, `relocations`, `deps`, `map`).

---

## Architecture Overview

```text
Adesh Source / Objects
        │
        ▼
┌──────────────────────────────────────────────────────────┐
│                    Adesh Linker Core                     │
├──────────────────────────────────────────────────────────┤
│ • Input Object & Archive Parsers                         │
│ • Symbol Resolution Engine (Strong / Weak / COMDAT)      │
│ • Section Garbage Collection (--gc-sections)             │
│ • Identical Code Folding (--icf)                         │
│ • Adesh Link Metadata Validation (.adesh.meta)           │
│ • Section Layout & Memory Mapping Engine                 │
│ • Generic & Arch-Specific Relocation Engine              │
│ • Incremental Link Cache & State Tracking                │
│ • Structured Diagnostics Engine                          │
└──────────────┬────────────────────────────┬──────────────┘
               │                            │
   ┌───────────┼───────────┐    ┌───────────┼───────────┐
   ▼           ▼           ▼    ▼           ▼           ▼
  ELF          PE        Mach-O WASM      x86_64     AArch64 / RISC-V
   │           │           │    │
   ▼           ▼           ▼    ▼
 Linux      Windows      macOS Web / WASI
```

---

## CLI Usage

```bash
# Link native ELF executable for Linux x86_64
adeshlink --target x86_64-linux main.o runtime.o -o app

# Link PE32+ executable for Windows x86_64
adeshlink --target x86_64-windows main.obj runtime.lib -o app.exe

# Link WASM artifact
adeshlink --target wasm32-wasi main.o -o app.wasm

# Dead code elimination & ICF
adeshlink --gc-sections --icf=safe main.o libfoo.a -o app

# Select portable link-time passes, or prioritize final binary size
adeshlink -O2 main.o libfoo.a -o app
adeshlink -Oz main.o libfoo.a -o app

# Explicit ThinLTO requests fail until IR-bearing input support is integrated
adeshlink -O3 --lto=thin main.o libfoo.a -o app

# Generate detailed link map and report
adeshlink --map=app.map --report main.o -o app

# Inspect object files and binaries
adeshlink inspect foo.o
adeshlink symbols app
adeshlink sections app
adeshlink relocations app
```

`-O0` through `-O3`, `-Os`, and `-Oz` select linker GC, ICF and stripping
policies. Compiler code generation has its own optimization level. Native
objects do not carry cross-module IR here: `--lto=thin|full` returns an error
without producing an executable, rather than silently skipping IR-level LTO.
CLI aliases `-s`, `--strip-all`, `--output`, `--entry=`, `-Map`, and
`--icf=none` are available. Unknown `lld` flags fail with an explicit error
instead of being silently ignored. These are not full LLVM `lld`/LTO parity.

The format tests check ELF, Mach-O, WASM, embedded firmware images,
GPU/NPU/TPU bundles, and quantum packages. Generating an artifact is not
proof it runs on a device or another OS. In particular, non-PE TLS objects
currently fail with an explicit error until the ELF/Mach-O TLS loader metadata
is implemented. The Windows Rust-backed hello-world and pretty-print samples
print, exit 0, and produce no stderr on this host. Other execution paths
still require testing on representative runtimes and hardware.

---

## Documentation

- [Linker Architecture](docs/architecture.md)
- [ELF Backend](docs/elf.md)
- [PE/COFF Backend](docs/pe.md)
- [Mach-O Backend](docs/macho.md)
- [WebAssembly Backend](docs/wasm.md)
- [Relocation Engine](docs/relocations.md)
- [Target Abstraction](docs/targets.md)
- [Incremental Linking](docs/incremental.md)
- [Security & Hardening](docs/security.md)
- [Performance & Benchmarks](docs/performance.md)
