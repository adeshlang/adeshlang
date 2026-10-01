# ADOB (Adesh Native Object Binary) Format Specification

## 1. Overview & Vision

**ADOB (Adesh Native Object Binary)** is the first-class, target-neutral relocatable object format designed specifically for the Adesh ecosystem. ADOB decouples the compiler frontend and intermediate lowering pipelines from operating-system-specific container formats (PE/COFF, ELF, Mach-O, WASM).

Rather than emitting platform binaries directly, the Adesh compiler produces ADOB modules. The Adesh Linker (`adeshlink`) then ingests ADOB objects, performs resolution, optimization (ICF, dead-code elimination, LTO), and emits the final platform executable or package.

---

## 2. File Format Structure

An ADOB binary has the following layout:

```text
┌─────────────────────────────────────────────────────────────┐
│ ADOB Header (Magic `ADOB`, Version Major/Minor/Patch, Flags) │
├─────────────────────────────────────────────────────────────┤
│ Target Descriptor (Device, Arch, OS, Env, ABI, Ptr, Endian) │
├─────────────────────────────────────────────────────────────┤
│ Target Feature Descriptors (AVX2, NEON, RVV, Atomics, etc.) │
├─────────────────────────────────────────────────────────────┤
│ Section Table & Section Payloads                           │
├─────────────────────────────────────────────────────────────┤
│ Relocation Tables (per section)                             │
├─────────────────────────────────────────────────────────────┤
│ Symbol Table (Local, Global, Weak, Unique, TLS, Kernel)     │
├─────────────────────────────────────────────────────────────┤
│ Import & Export Tables                                      │
├─────────────────────────────────────────────────────────────┤
│ Memory Regions (Embedded Flash, SRAM, MMIO)                 │
├─────────────────────────────────────────────────────────────┤
│ Security Metadata (Stack Guard, CFI, PAC, BTI, Shadow Stack)│
├─────────────────────────────────────────────────────────────┤
│ Accelerator Metadata (GPU Kernels, NPU/TPU Tensor Graphs)   │
├─────────────────────────────────────────────────────────────┤
│ Extension Table (UUID-tagged extensible payloads)          │
├─────────────────────────────────────────────────────────────┤
│ Build & Reproducibility Metadata                            │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Header Fields

* **`ADOB_MAGIC`**: `0x424F4441` (`b"ADOB"` in little-endian).
* **`version_major`**: `u16` (Major format version. Mismatch results in rejection).
* **`version_minor`**: `u16` (Minor backward-compatible extensions).
* **`version_patch`**: `u16` (Bugfix and revision identifier).
* **`flags`**: `u32` (Bitflags for object properties, such as position-independent code, debug stripped, deterministic build).
* **`section_count`**: `u32`
* **`symbol_count`**: `u32`
* **`import_count`**: `u32`
* **`export_count`**: `u32`
* **`extension_count`**: `u32`

---

## 4. Section Structure

Each section defines:
* **`name`**: Length-prefixed UTF-8 string (e.g., `.text`, `.rodata`, `.data`, `.bss`, `.gpu_kernel`).
* **`kind`**: Enumeration (`Text`, `Rodata`, `Data`, `Bss`, `Tls`, `Unwind`, `Debug`, `AdeshMeta`, `Comdat`, `Extension`, `Custom`).
* **`flags`**: `READ (0x1)`, `WRITE (0x2)`, `EXECUTE (0x4)`, `ALLOC (0x8)`, `TLS (0x10)`, `MERGE (0x20)`, `STRINGS (0x40)`.
* **`alignment`**: Non-zero power-of-two byte alignment requirement.
* **`comdat_group`**: Optional string tag for deduplicating inline templates/generics.
* **`memory_region`**: Optional name of the embedded physical memory region (e.g. `.flash`, `.sram`).
* **`data`**: Raw section payload bytes.
* **`relocations`**: Sequence of section-relative relocations.

---

## 5. Relocation Model

Relocations in ADOB support universal and architecture-specific fixups:

| Relocation Kind | Size | Description |
| :--- | :--- | :--- |
| `Absolute64` | 8 bytes | Direct 64-bit absolute address |
| `Absolute32` | 4 bytes | Direct 32-bit absolute address |
| `PcRelative32` | 4 bytes | 32-bit PC-relative displacement |
| `PcRelative64` | 8 bytes | 64-bit PC-relative displacement |
| `PltRelative32` | 4 bytes | Procedure Linkage Table call displacement |
| `GotRelative32` | 4 bytes | Global Offset Table entry offset |
| `X86_64_GotPcrel` | 4 bytes | x86-64 GOT PC-relative reference |
| `AArch64_Call26` | 4 bytes | ARM64 branch / call 26-bit offset |
| `AArch64_AdrPage21` | 4 bytes | ARM64 page-relative 21-bit offset |
| `RiscV_Call` | 8 bytes | RISC-V `auipc` + `jalr` pair |
| `RiscV_PcrelHi20` | 4 bytes | RISC-V high 20-bit PC-relative offset |
| `WasmFunctionIndex` | 4 bytes | WebAssembly function table index |
| `Custom(u32)` | Variable | Target-specific custom accelerator relocation |

---

## 6. Symbol Model

Symbols represent addresses, routines, variables, or accelerator entry points:
* **`binding`**: `Local`, `Global`, `Weak`, `Unique`.
* **`visibility`**: `Default`, `Hidden`, `Protected`, `Internal`.
* **`kind`**: `Function`, `Object`, `Section`, `TLS`, `Import`, `Export`, `AcceleratorKernel`, `Runtime`.
* **`is_defined`**: Boolean flag indicating whether the symbol is defined in this module or unresolved.
* **`section_index`**: Index into section table for defined symbols.
* **`value`**: Offset within section or absolute value.
* **`size`**: Byte size of the entity.

---

## 7. Extensibility & Forward Compatibility

ADOB includes an `EXTENSION_TABLE`. Each extension contains a 128-bit UUID, semantic version `(major, minor)`, UTF-8 tag, and raw payload. Readers safely ignore unknown extension UUIDs while preserving all core object contents.
