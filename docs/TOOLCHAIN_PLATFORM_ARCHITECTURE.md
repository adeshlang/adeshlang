# Adesh Toolchain Platform Architecture & Roadmap

This document defines the formal architecture, design principles, and
execution roadmap for the **Adesh Native Toolchain Platform**.

**Status (2026-10-05):** revised during the Phase 0 documentation truth
reset. The canonical status document is
[`CURRENT_STATE.md`](../CURRENT_STATE.md); the tiers below describe what is
actually verified, not aspirations. Only the Windows x86-64 path is
execution-verified.

---

## 1. The Three-Pillar Architecture

The Adesh ecosystem is divided into three cleanly decoupled pillars:

```text
                                 ADESH TOOLCHAIN PLATFORM
                                            │
           ┌────────────────────────────────┼────────────────────────────────┐
           │                                │                                │
        adeshc                          adeshlink                         adeshrt
  (Compiler & Codegen)           (Binary Integration & Linking)        (Execution & Runtime)
           │                                │                                │
   ├── Frontend (Lexer/Parser)      ├── Multi-pass Symbol Resolution  ├── GC-free runtime + ARC
   ├── AST / HIR Engine             ├── Relocation Patching (x86-64)   ├── tracked-allocation scopes
   ├── MLIR Text Lowering           ├── Formal ABI v1 (.adesh.meta)    ├── worker pool (threads)
   │   (external mlir-opt/llc)      ├── Executable Formats:           ├── handle table + panic guards
   ├── Machine Code Encoders:       │   ├── Windows PE32+ (execution-tested)
   │   ├── x86_64 (substantial)     │   ├── Linux ELF64 (emits; not run-tested)
   │   ├── AArch64 (PoC: 4 ops)     │   ├── macOS Mach-O (skeletal)
   │   └── RISC-V (PoC: 4 ops)      │   └── WebAssembly (writer is a stub)
   └── Native Object Writer (.o)    ├── Dead Code Elimination (GC)
                                    └── Identical Code Folding (ICF)
```

Components sometimes listed here in earlier revisions that do **not** exist
yet: an async green-thread runtime (no epoll/IOCP/kqueue reactor), GPU
driver bridges (zero CUDA/HIP/Vulkan calls), Vulkan SPIR-V / PTX / CUBIN
encoders, and quantum QPU provider dispatch.

## 2. Capability & Execution Boundaries (verified 2026-10-05)

| Domain | `adeshc` Compiles | `adeshlink` Links & Packages | Verified how | Maturity Tier |
| :--- | :---: | :---: | :---: | :---: |
| **Windows x86_64** | Native x86-64 (integer + scalar SSE2 FP subset) | PE32+ image, IAT, base relocs, TLS dir | **Binaries executed in CI (exit codes asserted)** | **Tier 1 — Execution-verified** |
| **Linux x86_64** | Native x86-64 (same subset) | ELF64 static (`ET_EXEC`) | Structure/magic-byte checks only | **Tier 2 — Emits; not run-tested** |
| **Linux AArch64** | PoC (Nop/Return/Add/Sub) | ELF64 static | Structure checks only | **Tier 3 — Proof-of-concept** |
| **macOS (AArch64 / x86_64)** | PoC / native subset | Mach-O 64 (no dyld info/exports trie) | Structure checks only; cannot bind system libraries | **Tier 3 — Artifact emission** |
| **WebAssembly** | WASM opcodes (compiler backend) | Linker writer is a stub | Compiler output runs via Wasmtime/Node | **Tier 2 via compiler backend** |
| **ARM Cortex-M (M0-M7)** | Minimal Thumb-2 ALU | Linker script, IVT, HEX/BIN | Structure checks only | **Tier 3 — Proof-of-concept** |
| **RISC-V (RV32/64)** | PoC (4 ops) | Trap vector, `crt0`, SREC | Structure checks only | **Tier 3 — Proof-of-concept** |
| **GPU (CUDA / ROCm / Vulkan)** | Source re-embedding only | Custom container packaging (not driver-loadable formats) | Structure checks only; no driver calls | **Scaffolding (parked)** |
| **NPU (Ethos-U / ANE / TPU)** | None | Custom packet/section packaging | Structure checks only; no ANE implementation exists | **Scaffolding (parked)** |
| **Quantum (QIR / OpenQASM)** | No language construct | QIRB container (embeds QASM text) | Library-level simulator (mock measurement) | **Scaffolding (parked)** |

## 3. Detailed Phase Roadmap

The roadmap is maintained in
[`IMPLEMENTATION_PLAN.md`](../IMPLEMENTATION_PLAN.md) (v2, 2026-10-05). Its
phase ordering is: Phase 0 truth reset → Phase 1 zero silent miscompiles →
Phase 2 full x86-64 ABI → Phase 3 Linux execution → Phase 4 optimizer and
register-allocator maturity → Phase 5 debuggability → Phase 6 AArch64 for
real. GPU, quantum, macOS executables, the linker WASM writer, and the
async runtime are explicitly **parked** until the core targets are solid.

### Foundation (completed, as previously listed)
- [x] Safe-Rust native linker (`adeshlink`) with zero external C/C++
  toolchain dependencies.
- [x] Executable binary generation: Windows PE32+ (**execution-verified**),
  Linux ELF64 and macOS Mach-O (emission-validated only), linker WASM
  (stub).
- [x] Verified execution of compiled binaries on the native Windows loader
  without external runtime DLLs.
- [x] Built-in toolchain CLI replacements (`ar`, `nm`, `objdump`, `readobj`,
  `size`, `strip`, `inspect`, `symbols`, `sections`, `relocations`, `deps`,
  `targets`, `version`).
- [x] Embedded linker script engine (`MEMORY` & `SECTIONS` evaluation),
  interrupt vector tables, `crt0` startup synthesizers, and flash firmware
  formats (Raw `.bin`, Intel `.hex`, Motorola `.srec`).
- [x] Formal Adesh ABI v1 specification, scope-aware RAII drop action tables
  (`.adesh.unwind_map`), and compiler-RT intrinsics (`memcpy`, `memset`,
  `memcmp`, `__multi3`, `__chkstk`, `__stack_chk_fail`).

### Native Codegen (subset working; completion tracked in IMPLEMENTATION_PLAN.md)
- [x] Pure-Rust bit-level machine code encoding for x86_64 (integer +
  scalar SSE2; execution-tested on Windows).
- [ ] AArch64 and RISC-V real backends (currently 4-operation
  proof-of-concepts) — `IMPLEMENTATION_PLAN.md` Phase 6.
- [ ] Vulkan SPIR-V / NVIDIA PTX / AMDGCN code generation — **parked**;
  requires a kernel-launch runtime that does not exist. Not active work.
