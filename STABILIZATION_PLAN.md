# AdeshLang Master Stabilization Plan (Phases 1–5)

**Author:** Antigravity / Pair Programming  
**Status:** Approved Architectural Roadmap  
**Target:** Production Language Stability, Sound Memory Management, Unified Multi-Tier IR Pipeline, and Cross-Platform Native Toolchain  
**Canonical Baseline:** Commit `State` (Toolchain 0.3.0)  

---

## 1. Executive Summary & Guiding Principles

A comprehensive scan of the AdeshLang codebase revealed that while low-level native compilation and linking infrastructure ([`crates/adesh-codegen`](file:///d:/Projects/AdeshLang/crates/adesh-codegen), [`crates/adesh-object`](file:///d:/Projects/AdeshLang/crates/adesh-object), [`linker`](file:///d:/Projects/AdeshLang/linker)) is capable of generating compact (<5 KB) executables, the language remains unstable due to **semantic divergence across backends, unfreed runtime handle leaks, disconnected OOP lowering, and an unwired middle-end IR pipeline**.

Currently, the native backend relies on a monolithic 6,884-line bypass (`src/backends/native/lower.rs`) that attempts to jump directly from high-level `HIR` statements straight to low-level physical/virtual register `Machine IR`. In doing so, it completely bypasses the compiler's middle-end:
* Bypasses **MIR** (`src/ir/mir/`): skipping ownership analysis, borrow checking, automatic drop insertion, ARC insertion, and lifetime inference.
* Bypasses **VIR** (`src/ir/vir/`): skipping SSA construction (mem2reg, PHI nodes), global value numbering, SCCP, dead code elimination, and auto-vectorization (`src/ir/simd/`).

This document outlines a phased, dependency-ordered engineering plan to resolve all 6 major instability vectors and wire the **complete end-to-end multi-tier pipeline**:  
`AST → HIR → MIR → VIR → Machine IR → Native Codegen → ADOB v3 → Linker`.

### The 4 Guiding Principles
1. **Fix Silent Correctness and Memory Bugs First:** No language can be stable if objects leak indefinitely or constructors are silently ignored.
2. **Unified Multi-Tier Pipeline:** Wire the full `AST → HIR → MIR → VIR → Machine IR` pipeline so middle-end memory safety and optimizations apply to all targets.
3. **Consolidation Over Proliferation:** One compiler, one runtime, two canonical targets (Interpreter for REPL/interactive development, Native Toolchain for production binaries). Deprecate/freeze half-implemented intermediate execution engines.
4. **Verified Execution Parity:** Every language feature must execute with identical results on both the Interpreter and the Native binary, verified by automated conformance tests.

---

## 2. The 6 Instability Problems & Root Causes

| # | Instability Area | Code Locations | Root Cause |
|---|---|---|---|
| **1** | **Backend Fragmentation & Semantic Divergence** | `src/execution/`, `src/backends/lir/`, `src/backends/native/lower.rs` | 5 parallel execution backends (Interpreter, Bytecode VM, LIR, Cranelift, Native Toolchain) implement divergent subsets of the language. Constructs like `async`, closures, and pattern matching behave differently or abort. |
| **2** | **Memory Safety Model Disconnect** | `src/parsing/unified_safety_pass.rs`, `src/ir/mir/`, `src/parsing/compile_time_memory_safety/` | The unified safety pass is unwired. Native compilation bypasses `src/ir/mir/drop_insertion.rs` and `borrow_analysis.rs`. Active safety checks are heuristic AST scans that produce false positives. Variables are mutable by default without clear borrow exclusivity. |
| **3** | **Native Runtime Memory Leak (Handle Table)** | `crates/adesh-runtime/src/lib.rs`, `src/backends/native/lower.rs` | All composite types (strings, arrays, objects) are allocated into a global `HANDLE_SHARDS: Mutex<BTreeMap<u64, RuntimeValue>>`. Native lowering **never emits `aot_remove_value`**, causing perpetual memory leaks in loops. |
| **4** | **Broken Class Constructors & Field Access** | `src/backends/native/lower.rs:5810`, `src/backends/native/lower.rs:3967` | Native lowering looks for `ClassName_init`, while AST/stdlib constructors use `ClassName(args)`. Constructors are never called. Field assignment creates temporary heap strings and queries BTreeMaps. |
| **5** | **Standard Library Native Disconnect** | `src/stdlib/*.adesh` | Standard libraries (`IO.adesh`, `HTTP.adesh`) rely on interpreter-only dynamic builtins and Tokio pulled into the compiler binary, preventing native compilation via `adesh build`. |
| **6** | **Cross-Platform Linker Portability** | `linker/src/macho/`, `linker/src/elf/`, `crates/adesh-codegen/src/targets/` | Windows PE is execution-tested; macOS Mach-O emits layouts but lacks functional dynamic dyld binding and indirect symbol execution; Linux ELF dynamic linking is fragile. |

---

## 3. The Unified End-to-End Compiler Pipeline

To achieve complete stability, language features must pass through a disciplined, layered pipeline where each level has a single, well-defined responsibility:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. Surface AST (src/parsing/ast.rs)                                         │
│    • Concrete Syntax Tree: declarations, expressions, classes, decorators    │
│    • Source spans, syntax error diagnostics with hints                      │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. High-Level IR / HIR (src/parsing/hir.rs)                                 │
│    • Desugared AST: module imports resolved, early macro expansions         │
│    • Symbol resolution, Gradual Type Inference, interface/trait constraints │
│    • Normalizes constructors to canonical `constructor` methods             │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. Memory IR / MIR (src/ir/mir/)                                            │
│    • Control Flow Graph (CFG) of basic blocks and terminators               │
│    • Ownership graph construction & Use-After-Move detection                │
│    • CFG-based Borrow Analysis & Lifetime Inference (borrow_analysis.rs)   │
│    • Deterministic RAII Drop Insertion (drop_insertion.rs)                  │
│    • Automatic ARC retain/release insertion (arc_insertion.rs)              │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. Value IR / VIR (src/ir/vir/) — Target-Independent SSA                     │
│    • Pure SSA Form (Single Static Assignment) with PHI nodes (mem2reg)       │
│    • Machine-Independent Optimizations: GVN, SCCP, Constant Folding, DCE    │
│    • Loop Optimizations: LICM, Induction Variable Simplification            │
│    • SIMD Vectorizer (src/ir/simd/vectorize.rs): auto-vectorization passes   │
│    • Parallel Loop Analyzer (src/ir/parallel/analysis.rs)                    │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 5. Machine IR (crates/adesh-codegen/src/machine_ir/)                        │
│    • Target-Independent Machine Instructions with Virtual Registers         │
│    • ABI & Calling Convention Classification (SysV AMD64, Win64, AAPCS64)   │
│    • True unboxed struct/tuple stack layout calculation (sret & by-val)     │
│    • Stack frame slot layout (locals, spills, shadow space, callee-saved)   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 6. Target Codegen & Register Allocation (crates/adesh-codegen/src/targets/) │
│    • Linear Scan Register Allocator with live-range splitting at calls      │
│    • Parallel Move Resolver (resolves cyclic register swaps & spills)       │
│    • Target Instruction Selection & Encoding (x86-64, AArch64, RISC-V)      │
│    • Branch displacement fixups, stack canary insertion, peephole opt       │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 7. Native Object File Emission / ADOB v3 (crates/adesh-object/)             │
│    • Validated binary ADOB v3 object files                                  │
│    • Sections: .text, .rodata, .data, .bss, .tdata, .tbss                   │
│    • Relocation tables (~28 relocation kinds, PC-relative, absolute, TLS)   │
│    • Debug metadata: DWARF 5 .debug_line / CodeView / SEH .pdata/.xdata     │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 8. Native Linker / adeshlink (linker/)                                      │
│    • Multi-format layout & binary generation: PE32+ / ELF ET_EXEC/DYN / Mach-O│
│    • Reachability Section GC (--gc-sections) & Identical Code Folding (ICF) │
│    • Archive indexing (GNU / index) & lazy member resolution                │
│    • OS routing (UCRT / libc.so.6 / libSystem) & dynamic GOT/PLT/dyld binding│
│    • Final standalone executable output (<40 KB, zero external dependency)  │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. End-to-End Feature Wiring Matrix

Every language feature must be consistently represented across all 8 tiers:

| Language Feature | AST Representation | HIR Representation | MIR Representation | VIR (SSA) Representation | Machine IR Representation | Target Codegen & ADOB | Linker Representation |
|---|---|---|---|---|---|---|---|
| **Primitive Types** (`i32`, `i64`, `f64`, `bool`) | `ExprKind::Literal` | `HirExpr::Literal` | `MirConstant` / `MirOperand::Constant` | `VirInstruction::Constant` | `MachineOperand::Immediate` / `Register` | Physical GPR / XMM, scalar SSE/ALU instructions | Symbol values, raw `.rodata` floats |
| **Unboxed Structs & Tuples** | `StmtKind::Struct`, `ExprKind::StructLiteral` | `HirType::Tuple` / `StructDef` | `MirTypeDef` with exact field byte layouts; `MirPlace` projections | `VirType::Struct`, `ExtractValue` / `InsertValue` | `StackSlot` with contiguous offset; ABI register-pair or `sret` | Stack-relative `[RBP - disp]`, scalar moves | Not emitted to symbol table unless exported |
| **Classes & Methods** | `StmtKind::Class`, `ExprKind::New` | `HirClass`, normalized `constructor` method | Flattened `Class_method` functions; explicit `self` reference | SSA `Call` with explicit receiver value | `MachineInstruction::Call` to mangled symbol | Mangled symbol names, `RelocationKind::PcRelative32` | Resolved locally or exported in DLL/SO |
| **Enums & Pattern Matching** | `StmtKind::Enum`, `ExprKind::Match` | `HirPattern::EnumVariant` with type info | Tagged representation: tag comparison + payload place extraction | Switch / branch on 64-bit tag (TypeID + VariantIdx) | Binary search tree or compare chain (`SwitchLowering`) | Jump tables or conditional branches | Resolved in `.text` |
| **Lambdas & Closures** | `ExprKind::Fn` | `HirExpr::Lambda` | Heap-allocated env struct; free-variable capture places | Function pointer + env pointer pair | Indirect call with `%r10` static chain calling convention | `call reg` with static chain register | Heap env allocated via `aot_alloc` |
| **Ownership, Borrowing & Drops** | `&x`, `&mut x`, move | `HirExpr::Borrow`, `Move` | `BorrowAnalysis`, `OwnershipGraph`, `DropInsertion` blocks | Explicit `Drop` instructions at scope exits | Cleanup calls before `Return`/`Break`/`Continue` | Epilogue cleanup sequences | Bound to `aot_free` / type destructors |
| **Strings & Concat** | `ExprKind::Literal(Str)`, `+` | `HirExpr::Literal(String)` | `MirPlace` string handle; explicit `Drop` | Tracked string value ID | Virtual register holding handle; `aot_string_concat` | String bytes in `.rodata`; string handle in GPR | String literals emitted to `.rodata` |
| **Atomics & Fences** | Built-in atomics | `HirExpr::Atomic` | `MirStatement::Atomic` | `VirInstruction::AtomicRmw`, `Barrier` | `MachineInstruction::Atomic*` | `lock xadd`, `lock cmpxchg`, `mfence` | Hardware locked bus cycles |
| **SIMD Vectors** | `vec4<f32>` / `Simd` | `HirType::Simd` | Vector type with lane count | `VirInstruction::VectorOp` | XMM/YMM/NEON vector operands | Packed SSE (`addps`, `mulps`), NEON (`FADD`) | Aligned 16-byte stack/rodata slots |
| **Concurrency & Spawn** | `spawn { ... }` | `HirExpr::Spawn` | Task closure creation & scope boundary | `VirInstruction::Spawn` | Call to `adesh_task_scope_spawn` | Thread pool worker dispatch | Bound to `adesh-runtime::concurrency_v2` |

---

## 5. Strict Dependency Order of Requirements

To prevent circular dependencies and wasted effort, work must proceed in this exact order:

```
┌─────────────────────────────────────────────────────────────┐
│ Phase 1: Native Correctness & Memory Leak Elimination (P0)  │
│ - Fix Runtime Handle Drops (Stop Perpetual Memory Leaks)    │
│ - Fix Class Constructor Mangling & Dispatch                 │
│ - Fix Enum Tag Collisions                                   │
│ - Optimize Field Lookups (Interned Offset/Slots)            │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 2: Middle-End Pipeline Wiring (HIR → MIR → VIR) (P1)  │
│ - Complete HIR → MIR Lowering (CFG, Drops, Borrows)         │
│ - Complete MIR → VIR Lowering (SSA, PHI, Optimizations)     │
│ - Write VIR → Machine IR Lowerer (Retire lower.rs bypass)   │
│ - Enable SIMD Auto-Vectorization & Parallel Analysis        │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 3: Engine Consolidation & Conformance (P1)            │
│ - Deprecate Incomplete Engines (Bytecode VM, LIR)           │
│ - Canonicalize Interpreter vs Native Toolchain Semantics    │
│ - Unified Closure & Lambda Lowering                         │
│ - Multi-Backend Conformance Test Suite                      │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 4: Standard Library Native Refactoring (P2)           │
│ - Freestanding Native Core Stdlib (No Tokio/Interpreter)    │
│ - Package adesh_core & adesh_std as Native Static Libs      │
│ - Native Concurrency & TaskScope Integration                │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 5: Cross-Platform Linker & Toolchain Hardening (P2)   │
│ - macOS Mach-O dyld Execution & Binding                     │
│ - Linux Dynamic ELF GOT/PLT & Musl Hardening                │
│ - AArch64 Native Hardware Conformance Execution             │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. Phase-by-Phase Detailed Implementation Plan

### Phase 1: Native Correctness & Memory Leak Elimination (P0)
**Primary Goal:** Fix critical wrong-code paths and memory leaks in the native backend immediately so existing programs run correctly and clean up after themselves.

#### Task 1.1: Scope-Based Handle Deallocation (Fix Problem 3)
* **Location:** `src/backends/native/lower.rs`, `crates/adesh-runtime/src/lib.rs`
* **Changes:**
  1. Add `aot_release_handle(handle: u64)` to `crates/adesh-runtime/src/native_abi.rs` which safely calls `lock_shard(handle).remove(&handle)`.
  2. In `FunctionLoweringContext`:
     * Introduce `scope_handles: Vec<Vec<VirtualRegister>>` to track virtual registers that hold runtime composite handles allocated within each block scope.
     * Record handles created by `aot_make_string`, `aot_make_array`, `aot_make_object`, `aot_string_concat`, `aot_make_range`, etc.
  3. In `lower_statement` and block exits (`Block`, `If`, `While`, `ForIn`, `Return`):
     * Emit cleanup instructions calling `aot_release_handle` for all handles allocated in the exiting scope.
     * Ensure `break` and `continue` clean up all handles up to the loop boundary (matching `run_defers()`).
* **Verification:** Write `tests/native_handle_leak_test.rs` executing a 10,000-iteration string/array loop in a native binary; assert process resident memory (RSS) remains flat.

#### Task 1.2: Class Constructor Symbol Resolution & Dispatch (Fix Problem 4)
* **Location:** `src/parsing/hir_lower.rs`, `src/backends/native/lower.rs`
* **Changes:**
  1. In `src/parsing/hir_lower.rs`: Normalize constructor naming. If a class `Foo` has a method named `Foo` or `constructor`, lower it as `HirMethod { name: "constructor", ... }`.
  2. In `src/backends/native/lower.rs`:
     * Generate constructor functions with standard symbol naming `format!("{}_constructor", cls_name)`.
     * In `HirExpr::NewInstance(cls_name, args)`: Look up and call `{cls_name}_constructor` with `self` as the first argument. If not defined, skip gracefully without crashing.
* **Verification:** Add end-to-end test `tests/native_class_constructor_e2e_test.rs` declaring `class Point { Point(x, y) { self.x = x; self.y = y; } }`, instantiate with `new Point(10, 20)`, assert field values.

#### Task 1.3: Type-Qualified Enum Tags (Fix Problem 1 & 4)
* **Location:** `src/backends/native/lower.rs:142-153`
* **Changes:**
  1. Replace flat global variant mapping `("None" -> 0, "Ok" -> 0)` with a 64-bit combined tag:
     * High 32 bits = Enum Type ID (hash of enum name or unique enum index).
     * Low 32 bits = Variant Index.
  2. Update `HirPattern::EnumVariant` lowering to compare both the Enum Type ID and Variant Index, preventing `Option::None` from matching `Result::Ok`.
* **Verification:** Test `tests/native_enum_collision_test.rs` asserting pattern match correctly distinguishes across multiple enums with identical variant indices.

#### Task 1.4: Pre-Interned Field Offsets (Optimize Problem 4)
* **Location:** `src/backends/native/lower.rs:3967-3992`
* **Changes:**
  1. Instead of creating a dynamic `aot_make_string` handle on every field set/get, emit the field name as an interned `.rodata` string constant handle or pre-computed slot index.
  2. Add `aot_set_field_fast(obj: u64, field_cstr: *const c_char, val: u64)` accepting static string symbols directly.
* **Verification:** Microbenchmark comparing 100,000 field writes; assert >=5x throughput improvement.

---

### Phase 2: Middle-End Pipeline Wiring (`HIR → MIR → VIR → Machine IR`) (P1)
**Primary Goal:** Connect the compiler's middle-end so all optimizations, borrow checking, and automatic drop insertion happen machine-independently before lowering to Machine IR.

#### Task 2.1: Complete HIR → MIR Lowering (`src/ir/mir/lower.rs`)
* **Changes:**
  1. Ensure `lower_hir_to_mir` handles all HIR expressions (enums, structs, classes, lambdas, match).
  2. Run `drop_insertion.rs` to insert explicit `MirStatement::Drop` instructions at CFG block exit points.
  3. Run `borrow_analysis.rs` to validate borrow scopes and prevent aliasing bugs on the CFG.
* **Verification:** `tests/mir_lowering_test.rs` asserting CFG blocks contain valid drop and retain statements.

#### Task 2.2: Complete MIR → VIR Lowering (`src/ir/vir/lower.rs`)
* **Changes:**
  1. Lower MIR control flow to VIR SSA form with explicit PHI nodes.
  2. Run machine-independent optimizations: GVN, SCCP, dead code elimination, and constant folding.
  3. Connect `src/ir/simd/vectorize.rs` to automatically vectorize arithmetic loops on array/slice buffers.
* **Verification:** `tests/vir_optimization_test.rs` checking dead branches eliminated and PHI nodes correctly formed.

#### Task 2.3: Implement the VIR → Machine IR Lowerer (`src/backends/native/vir_lower.rs`)
* **Changes:**
  1. Build the clean, modular `vir_to_machine_ir` lowerer:
     * Maps VIR `ValueId` directly to `VirtualRegister`.
     * Lowers VIR SSA basic blocks directly to `MachineFunction` basic blocks.
     * Maps VIR operations (IntBinOp, FloatBinOp, CmpOp) directly to `MachineInstruction`.
     * Uses ABI classifier for function call arguments and returns.
  2. Gradually replace the monolithic 6,884-line `lower.rs` bypass.
* **Verification:** Run the full native integration test suite through the new `VIR → Machine IR` pipeline.

---

### Phase 3: Engine Consolidation & Conformance (P1)
**Primary Goal:** Stop maintaining divergent backends. Unify language semantics between the Interpreter and the Native Toolchain.

#### Task 3.1: Deprecate Incomplete Intermediate Engines (Fix Problem 1)
* **Location:** `src/main.rs`, `src/cli/`, `src/execution/vm/`, `src/backends/lir/`
* **Changes:**
  1. Deprecate the unfinished Bytecode VM (`--bytecode`/`--vm`) and LIR interpreter (`--jit`).
  2. Designate **two official backends**:
     * **Interpreter (`adesh run`)**: Fast startup, interactive development, REPL, script execution.
     * **Native Toolchain (`adesh build`)**: High-performance AOT compiled native binaries via Machine IR + `adeshlink`.
  3. Gate experimental backends under `--experimental-vm` so users do not hit undocumented crashes.

#### Task 3.2: Systematic Conformance Corpus (Fix Problem 1)
* **Location:** `tests/conformance/`, `tests/native_phase7_conformance_corpus_test.rs`
* **Changes:**
  1. Create a suite of 50+ language conformance tests covering all features.
  2. Implement a CI test runner that executes each test through **both** the Interpreter and `adesh build`, asserting exact identical stdout and exit codes.
* **Verification:** CI fails if any language construct produces divergent output between interpreter and native binary.

#### Task 3.3: True Unboxed Structs & Value Types (Fix Problem 3 & 2)
* **Location:** `crates/adesh-codegen/src/abi/`, `src/backends/native/`
* **Changes:**
  1. Wire the ABI type classifier (`AbiType::Struct`) from `crates/adesh-codegen/src/abi`.
  2. User structs (`struct Point { x: i64, y: i64 }`) allocate 16 bytes directly on the stack `[RBP - offset]`, with zero calls to `aot_make_object` or the handle table.
  3. Small structs pass in registers (RCX/RDX or RDI/RSI); large structs pass via hidden pointer (`sret`).
* **Verification:** Execution test asserting 0 handle allocations for struct manipulation.

---

### Phase 4: Standard Library Native Refactoring (P2)
**Primary Goal:** Decouple the standard library from the interpreter and build a robust, natively compilable standard library.

#### Task 4.1: Decouple Stdlib from Tokio and Dynamic Builtins (Fix Problem 5)
* **Location:** `src/stdlib/`, `crates/adesh-runtime/`
* **Changes:**
  1. Separate stdlib into two distinct layers:
     * `adesh_core`: Freestanding, zero-dependency foundation (math, memory, strings, slices, formatting, panics).
     * `adesh_std`: OS-level facilities (files, directory paths, environment, processes, raw sockets).
  2. Implement native C ABI bindings in `crates/adesh-runtime`:
     * File I/O: `sys_open`, `sys_read`, `sys_write`, `sys_close`, `sys_seek`.
     * Networking: `sys_socket`, `sys_bind`, `sys_connect`, `sys_send`, `sys_recv`.
     * Process: `sys_spawn`, `sys_wait`, `sys_exit`.
  3. Rewrite `IO.adesh` and `Process.adesh` to call these native C runtime functions directly.
* **Verification:** Compile `main.adesh` importing `IO` with `adesh build`; execute the standalone binary without interpreter dependencies.

#### Task 4.2: Native Module Compilation & Static Archiving (Fix Problem 5)
* **Location:** `src/toolchain/`, `linker/src/archive/`
* **Changes:**
  1. Enable compiling Adesh modules into native static archives (`.lib` on Windows, `.a` on Linux/macOS) using `adeshlink ar`.
  2. Precompile `adesh_core.lib` and `adesh_std.lib` during toolchain build.
  3. When `adesh build` compiles a user program with `import IO`, automatically link against the precompiled native stdlib archive.
* **Verification:** Build a multi-module program using native static library linking; verify linked output size and execution.

#### Task 4.3: Native Concurrency & Task Integration (Fix Problem 1 & 5)
* **Location:** `crates/adesh-runtime/src/concurrency_v2.rs`, `src/backends/native/`
* **Changes:**
  1. Unpark the `spawn` construct in native lowering.
  2. Map `spawn { expr }` to `TaskScope::spawn` using the runtime's existing `AdeshThreadPool`.
  3. Lower `join` to await task handle completion.
* **Verification:** Test `tests/native_concurrency_threads_e2e_test.rs` spawning 4 parallel worker tasks calculating sums; assert matching result.

---

### Phase 5: Cross-Platform Linker & Toolchain Hardening (P2)
**Primary Goal:** Complete the native linker so Linux, macOS, and AArch64 targets are first-class, fully execution-verified platforms.

#### Task 5.1: macOS Mach-O dyld Execution (Fix Problem 6)
* **Location:** `linker/src/macho/writer.rs`, `linker/src/macho/dyld.rs`
* **Changes:**
  1. Implement complete `LC_DYLD_INFO_ONLY` bind and rebase opcode emission.
  2. Synthesize indirect symbol table (`indirectsymoff`), `__stubs`, and `__got` sections so external calls to `libSystem.B.dylib` (`write`, `exit`, `malloc`) bind dynamically under dyld.
  3. Add macOS execution test running produced Mach-O binary on macOS CI runner, asserting exit code and stdout.
* **Verification:** `linker/tests/macho_execution_test.rs` executes dynamically linked Mach-O executable on macOS host.

#### Task 5.2: Linux Dynamic ELF Hardening (Fix Problem 6)
* **Location:** `linker/src/elf/writer.rs`, `linker/src/elf/relocation.rs`
* **Changes:**
  1. Complete dynamic GOT/PLT synthesis and `.gnu.hash` table generation.
  2. Support dynamic linking against both glibc (`libc.so.6`, `ld-linux-x86-64.so.2`) and Alpine musl (`ld-musl-x86_64.so.1`).
  3. Add Alpine musl Docker job to CI matrix.
* **Verification:** Static and dynamic ELF binaries execute in Ubuntu and Alpine CI environments with asserted exit codes.

#### Task 5.3: AArch64 Native Hardware Conformance (Fix Problem 6)
* **Location:** `crates/adesh-codegen/src/targets/aarch64/`
* **Changes:**
  1. Run the native conformance corpus on real AArch64 Linux (via AWS Graviton / GitHub Actions ARM runner or QEMU user emulation).
  2. Verify AAPCS64 callee-saved preservation, NEON vector operations, and ADRP/ADD relocations on hardware.
* **Verification:** AArch64 execution tests pass 100% on ARM runner.

---

## 7. Success Metrics & Exit Criteria

| Phase | Milestone | Acceptance Criteria |
|---|---|---|
| **Phase 1** | **Leak-Free Native Correctness** | 10,000-iteration string/array loop shows 0 memory growth; class constructors run and populate fields; enum matching has 0 false collisions. |
| **Phase 2** | **Full Pipeline Wiring (`HIR → MIR → VIR → Machine IR`)** | Monolithic `lower.rs` replaced by clean `vir_lower.rs`; MIR drop insertion automatically handles cleanups; VIR SSA optimizations run. |
| **Phase 3** | **Unified Language Semantics & Value Types** | 50+ test Conformance Corpus passes with 100% identical stdout/exit codes on both Interpreter and `adesh build`; unboxed structs allocate on stack with 0 handle table lookups. |
| **Phase 4** | **Native Standard Library** | `IO`, `Path`, and `Process` compile directly to native binaries with `adesh build` without Tokio or interpreter builtins. |
| **Phase 5** | **Tier 1 Cross-Platform** | Native binaries link and execute natively in CI across Windows x64, Linux x64 (glibc + musl), macOS x64/arm64, and Linux AArch64. |

---

## 8. Implementation Schedule & Ownership

```text
Week 1 - 2:  Phase 1 (Handle Deallocation, Constructor Resolution, Enum Tags)
Week 3 - 5:  Phase 2 (HIR → MIR → VIR → Machine IR Pipeline Wiring, Drop Insertion)
Week 6 - 7:  Phase 3 (Deprecate Dead VM/LIR, Conformance Corpus, Unboxed Value Types)
Week 8 - 10: Phase 4 (Native Core/Std Refactor, Precompiled Archives, Concurrency)
Week 11 - 12: Phase 5 (Mach-O dyld Execution, Musl/Glibc Hardening, ARM64 Runner)
```
