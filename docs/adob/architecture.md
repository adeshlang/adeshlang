# ADOB Architecture & Pipeline Design

## 1. Architectural Philosophy

ADOB sits at the heart of the Adesh toolchain. The compiler never directly produces PE (`.exe`), ELF, or Mach-O binaries. Instead, the process is structured in three strictly decoupled stages:

```text
       Adesh Source
            │
            ▼
      Adesh Frontend
            │
            ▼
        Adesh LIR
            │
            ▼
   Native Code Generation
 (CPU / GPU / Accelerator)
            │
            ▼
     ADOB Object File
            │
            ▼
    Adesh Linker (`adeshlink`)
            │
   ┌────────┼────────┐
   ▼        ▼        ▼
  PE       ELF     Mach-O
(Win)    (Linux)   (macOS)
```

---

## 2. Separation of Concerns

* **Compiler**: Performs syntax parsing, semantic analysis, type checking, AST lowering, LIR SSA optimization, Machine IR lowering, register allocation, instruction encoding, and ADOB object writing.
* **ADOB**: Serves as the self-contained contract carrying instructions, relocations, symbol tables, debug line tables, unwind metadata, and accelerator kernels across tools.
* **Linker (`adeshlink`)**: Ingests ADOB files, resolves symbol definitions and archives, applies relocations, deduplicates COMDAT blocks, folds identical code (ICF), injects the Adesh runtime/startup routines, and writes native executable containers.

---

## 3. Heterogeneous & Fat Binaries (`AdobBundle`)

To support multi-target applications (e.g. CPU host code combined with CUDA kernels, Apple Neural Engine graphs, or multi-architecture fat binaries), `AdobBundle` encapsulates multiple ADOB images:

```text
program.adb
 ├── Image 0: x86_64 CPU (Windows)
 ├── Image 1: AArch64 CPU (Linux)
 ├── Image 2: NVIDIA CUDA Kernel
 └── Image 3: TPU Matrix Graph
```

At link time or deployment time, `adeshlink` extracts the appropriate image or bundles them into a multi-device executable package.
