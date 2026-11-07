# Adesh Toolchain Platform Architecture & Roadmap

This document defines the formal architecture, design principles, and
execution roadmap for the **Adesh Native Toolchain Platform**.

**Status:** last re-checked 2026-11-03, against the tier table in §2 and
`linker/src/target.rs`. The canonical status document is
[`CURRENT_STATE.md`](../CURRENT_STATE.md); the tiers below describe what is
actually verified, not aspirations. Four targets are execution-verified in CI —
Windows x86-64 (MSVC), Linux x86-64 (GNU) and the two WASM triples; every other
native target is emission-tested or declared only.

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
   │   └── RISC-V (RV64GC)          │   └── WebAssembly (links + Node-e2e)
   └── Native Object Writer (.o)    ├── Dead Code Elimination (GC)
                                    └── Identical Code Folding (ICF)
```

Components sometimes listed here in earlier revisions that do **not** exist
yet: an async green-thread runtime (no epoll/IOCP/kqueue reactor), GPU
driver bridges (zero CUDA/HIP/Vulkan calls), Vulkan SPIR-V / PTX / CUBIN
encoders, and quantum QPU provider dispatch.

## 2. Capability & Execution Boundaries (tiers realigned 2026-11-02)

The **Maturity Tier** column is `Target::tier()` from `linker/src/target.rs`,
which `adesh target list` and `adeshlink targets` print from the same table.
It reports **what CI verifies**, not what the backend claims: Tier 1 = CI
builds an artifact and runs it with asserted output or exit status; Tier 2 =
CI emits the artifact and parses it back, never executes it; Tier 3 =
declared only.

| Domain | `adeshc` Compiles | `adeshlink` Links & Packages | Verified how | Maturity Tier |
| :--- | :---: | :---: | :---: | :---: |
| **Windows x86_64** | Native x86-64 (integer + scalar SSE2 FP subset) | PE32+ image, IAT, base relocs, TLS dir | **Binaries executed in CI (exit codes asserted), including a real `msvcrt!puts` import** | **Tier 1 — Execution-verified** |
| **Linux x86_64 (GNU)** | Native x86-64 (same subset) | ELF64 static + dynamic, PLT/GOT, TLS, `.eh_frame` | **Executed in CI** — static ELF exit codes, a `dlopen`/`dlsym` shared library against the real `ld.so`, and static/dynamic TLS | **Tier 1 — Execution-verified** |
| **Linux x86_64 (Musl)** | Native x86-64 (same subset) | ELF64, TPOFF64 TLS | Executed manually in WSL against the musl loader; **no dedicated musl CI job** | **Tier 2 — Emission-tested in CI** |
| **Linux AArch64** | Implemented AAPCS64 with scalar FP, NEON and atomics; every unencodable instruction or operand form is a `CodegenError` (no `NOP` placeholder since 2026-11-01) | ELF64 static, ADRP/ADD symbol materialization, per-call-site relocations | Structure checks only (`linker/tests/platform_native_linking_conformance_test.rs`). Nothing builds an `aarch64` runtime archive yet, so a source build stops at runtime resolution — since 2026-11-03 it stops with a named missing-archive error rather than linking the host runtime | **Tier 2 — Emission-tested in CI** |
| **macOS (AArch64 / x86_64)** | Implemented (AAPCS64 / x86-64 subset) | Mach-O 64 with `LC_DYLD_INFO_ONLY` rebase/bind, exports trie, stubs/GOT, TLS descriptor table | The dyld structures are **emitted and decoded back** (`linker/tests/macho_dyld_binding_test.rs`); nothing has run under dyld | **Tier 2 — Emission-tested in CI** |
| **WebAssembly** | WASM opcodes (compiler backend; `wasi_snapshot_preview1.fd_write` for `-wasi` targets, `env.*` + Node loader otherwise) | Linker emits modules from one or more clang objects, applies index/data/table relocations, resolves cross-object calls and data, and defines the function table for `call_indirect` with slot 0 reserved for a null trap; entry exported | Compiler WASI output executes under wasmtime with asserted stdout (`tests/wasm_wasi_compiler_test.rs`); linked clang objects run in CI on two engines (V8 through Node, Cranelift through the workspace `wasmtime` dependency) with asserted stdout, including a two-object module that imports nothing and indirect calls that dispatch through the linker's table | **Tier 1 — Execution-verified in CI on two engines** (the linker still provides no WASI runtime of its own) |
| **ARM Cortex-M (M0-M7)** | Minimal Thumb-2 ALU (a 146-line subset) | Linker script, IVT, HEX/BIN | Structure checks only | **Tier 3 — Declared / proof-of-concept** |
| **RISC-V 64** | Implemented RV64GC LP64D with scalar FP, atomics and vector encodings; unencodable forms are `CodegenError`s since 2026-11-01. Source builds reach the linker since 2026-11-07 (`ParallelMove` + `auipc`/`addi` symbol materialization); the blocker is now the target runtime archive | ELF64 static, CALL/JAL/BRANCH/HI20/LO12, `_start`, PLT/GOT | Structure checks only; no execution | **Tier 2 — Emission-tested in CI** |
| **RISC-V 32** | **Refused at backend selection** — the RISC-V backend is RV64-only, so an RV32 triple is rejected before a function is encoded | **Refused for executables** — the linker will not synthesize its RV64 `_start` stub for an RV32 image | `tests/native_phase11_codegen_loudness_test.rs`, `linker/tests/phase11_riscv32_entry_stub_test.rs` pin the refusals | **Tier 3 — Declared; not functional and no longer advertised as validated** |
| **GPU (CUDA / ROCm / Vulkan)** | Source re-embedding only | Custom container packaging (not driver-loadable formats) | Structure checks only; no driver calls | **Tier 3 — Scaffolding (parked)** |
| **NPU (Ethos-U / ANE / TPU)** | None | Custom packet/section packaging | Structure checks only; no ANE implementation exists | **Tier 3 — Scaffolding (parked)** |
| **Quantum (QIR / OpenQASM)** | No language construct | QIRB container (embeds QASM text) | Library-level simulator (mock measurement) | **Tier 3 — Scaffolding (parked)** |

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
- [x] AArch64 and RISC-V backends beyond the original 4-operation
  proof-of-concepts: both emit real source programs (AArch64 links against a staged
  archive since 2026-11-04; RISC-V reaches the linker since 2026-11-07). Neither is
  execution-tested — see `TARGET_MATRIX.md` for the per-triple blocker.
- [ ] Vulkan SPIR-V / NVIDIA PTX / AMDGCN code generation — **parked**;
  requires a kernel-launch runtime that does not exist. Not active work.
