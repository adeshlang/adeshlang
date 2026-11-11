# AdeshLang NJIT → Native Codegen Migration Checklist

## Phase 0: Architecture and Discovery (Complete)
- [x] Complete repository-wide dependency mapping for Native JIT (`src/backends/jit/native/`).
- [x] Audit `adesh-codegen`, `adesh-object`, and `adesh-linker` capabilities and test suites.
- [x] Produce architecture audit document: `docs/NJIT_NATIVE_CODEGEN_MIGRATION_AUDIT.md`.
- [x] Validate baseline test suites (`adesh-codegen` 90/90 tests pass, `native_phase8` 10/10 tests pass).

---

## Phase 1: In-Memory Executable Memory Management & W^X Runtime
- [ ] Implement `JitMemoryManager` with RAII lifetime guarantees.
- [ ] Implement dual-state page allocation (RW during emission/relocation, RX during execution).
- [ ] Add cross-platform virtual memory support (`VirtualAlloc`/`VirtualProtect` on Windows, `mmap`/`mprotect` on Unix).
- [ ] Implement instruction cache flushing (`FlushInstructionCache` on Windows, `__builtin___clear_cache` / `sys_icache_invalidate` on POSIX/ARM64).
- [ ] Write unit tests for memory protection transitions and executable invocation.

---

## Phase 2: In-Memory Relocation Resolver
- [ ] Extract shared relocation application formulas from `adesh-linker/src/relocation.rs` into reusable logic.
- [ ] Implement `InProcessRelocationResolver` for `AdobRelocation` records:
  - [ ] PC-relative 32-bit branch fixups (`RelocationKind::Pc32`).
  - [ ] 64-bit absolute address fixups (`RelocationKind::Abs64`).
  - [ ] Small code model displacements and PLT call trampolines if needed.
- [ ] Add unit tests verifying in-memory relocation patching on synthetic code buffers.

---

## Phase 3: Unified Host Symbol Resolution
- [ ] Define `HostSymbolResolver` trait.
- [ ] Consolidate symbols from `print_helpers.c`, `runtime_bridge.rs`, and `arc_bridge.rs` into a unified registry.
- [ ] Add dynamic fallback lookup (`GetProcAddress` on Windows, `dlsym` on Unix) for C-runtime functions (`printf`, `malloc`, `free`, `memcpy`).
- [ ] Write unit tests for symbol resolution of both runtime builtins and dynamic symbols.

---

## Phase 4: HIR → Machine IR JIT Pipeline Integration
- [ ] Update JIT driver (`src/backends/jit/native/mod.rs`) to support `--jit-backend=adesh` alongside `--jit-backend=cranelift`.
- [ ] Lower HIR directly to `NativeModule` using `src/backends/native/lower.rs`.
- [ ] Run `adesh-codegen` optimization pipeline and register allocator (`LinearScanAllocator`).
- [ ] Emit machine code via `CodegenBackend::generate_function_with_relocations`.
- [ ] Load code into `JitMemoryManager`, apply relocations, and extract entry point.
- [ ] Execute entry point via `execute_native_jit_with_timing`.

---

## Phase 5: Conformance Testing & Multi-Platform Validation
- [ ] Run `tests/native_phase8_jit_native_conformance_test.rs` under the new Native Codegen JIT.
- [ ] Run `tests/native_phase7_conformance_corpus_test.rs` across Interpreter, Cranelift JIT, and Native Codegen JIT.
- [ ] Validate on Windows x86-64.
- [ ] Validate on Linux x86-64.
- [ ] Enable and validate NJIT on macOS ARM64 (Apple Silicon) using the AArch64 backend.
- [ ] Compare compilation and execution latency benchmarks.

---

## Phase 6: Cranelift Retirement
- [ ] Verify 100% feature and performance parity between Cranelift NJIT and Native Codegen NJIT.
- [ ] Set `--jit-backend=adesh` as default for NJIT.
- [ ] Deprecate `cranelift-jit` and `cranelift-codegen` in workspace `Cargo.toml`.
- [ ] Remove `src/backends/jit/native/compiler.rs` (5,880 lines of Cranelift lowering).
