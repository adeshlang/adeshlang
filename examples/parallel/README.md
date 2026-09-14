# SIMD + Safe Parallelism — Implementation Guide

This document describes the **SIMD vector math** and **safe parallel execution** subsystem in AdeshLang: architecture, runtime behavior, public API, compiler integration, and how to extend it.

For a short API cheat sheet, see [../simd_parallel.md](../simd_parallel.md).

---

## Table of contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Language surface](#language-surface)
4. [Runtime behavior](#runtime-behavior)
5. [Standard library API](#standard-library-api)
6. [Type system integration](#type-system-integration)
7. [Compiler IR layer](#compiler-ir-layer)
8. [Parallel scheduler](#parallel-scheduler)
9. [Examples](#examples)
10. [Testing](#testing)
11. [Source file map](#source-file-map)
12. [Known limitations & roadmap](#known-limitations--roadmap)

---

## Overview

AdeshLang provides two complementary performance layers:

| Layer | Purpose | When it activates |
|-------|---------|-------------------|
| **SIMD** | Element-wise numeric array math, reductions, fusion | Array operators (`+`, `-`, `*`, `/`) and `import Simd;` |
| **Parallel** | Multi-core loops, map/reduce over arrays | `import Parallel;` and internal scheduler for large numeric workloads |

Design principles:

- **Import-first namespace API** — like `Math` or `Collections`, not global builtins (`simd_sum` / `parallel_for` globals were removed).
- **Zero cost when unused** — the work-stealing scheduler is lazily initialized via `OnceLock`; no worker threads until parallel code runs.
- **Element-wise array semantics** — `a + b` on arrays is NumPy-style element-wise addition, **not** concatenation. Use `Simd.concat(a, b)` to join arrays.
- **Scalar broadcasting** — `scalar * array`, `array + scalar`, and mixed `Array` / `DynArray` operands are supported at runtime.
- **Deep integration** — types, HIR, runtime ABI, interpreter, and compiler IR hooks exist; native backend SIMD lowering is infrastructure-ready but not fully wired into the main compile pipeline yet.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         AdeshLang source (.adesh)                        │
│   import Simd;  import Parallel;   let y = alpha * x + b;               │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
         ┌──────────────────┐            ┌──────────────────┐
         │   Type checker   │            │   Parser / HIR   │
         │ array_broadcast  │            │ HirType::Simd    │
         │ binary inference │            │ Simd<T, N> syntax│
         └────────┬─────────┘            └────────┬─────────┘
                  │                               │
                  └───────────────┬───────────────┘
                                  ▼
                    ┌─────────────────────────────┐
                    │     Interpreter / Runtime    │
                    │  abi_add/mul/sub/div → SIMD  │
                    │  Parallel.map/forEach/reduce │
                    └──────────────┬──────────────┘
                                   │
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
    ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
    │ runtime/simd/   │  │ runtime/        │  │ ir/simd +       │
    │ ops, broadcast, │  │ scheduler/      │  │ ir/parallel     │
    │ fusion, value   │  │ work-stealing   │  │ (compile-time)  │
    └─────────────────┘  └─────────────────┘  └─────────────────┘
```

### Data flow for `alpha * x + y` (SAXPY)

1. **Parse** — AST: `Binary(+, Binary(*, alpha, x), y)` with correct precedence (`*` binds tighter than `+`).
2. **Type check** — `array_broadcast_binary()` infers `[f64]` for each step.
3. **Evaluate `alpha * x`** — interpreter detects array involvement → `abi_mul` → `array_mul` → `ScalarLeft` broadcast.
4. **Evaluate `result + y`** — `abi_add` → `array_add` → `ElementWise` (handles `Array` + `DynArray` mix).
5. **SIMD fast path** — `extract_f64_slice` + lane-unrolled loops when all elements are numeric `f64`.

---

## Language surface

### Imports

```adesh
import Simd;      // vector math namespace
import Parallel;  // parallel map/reduce/forEach
```

`Simd` and `Parallel` are **not** auto-injected as globals (same pattern as `Math`). They must be imported explicitly.

### Operator syntax (no import required)

```adesh
let a = [1.0, 2.0, 3.0, 4.0];
let b = [10.0, 20.0, 30.0, 40.0];

let c = a + b;           // element-wise: [11, 22, 33, 44]
let d = a + 100.0;       // broadcast scalar right
let e = 2.5 * a;         // broadcast scalar left
let f = a * b + 1.0;     // chained ops

// SAXPY
let alpha = 2.5;
let x = [1.0, 2.0, 3.0, 4.0];
let y = [10.0, 20.0, 30.0, 40.0];
let saxpy = alpha * x + y;   // [12.5, 25, 37.5, 50]
```

### Closures for Parallel APIs

AdeshLang uses **`fn(arg) { ... }`** syntax (not Rust-style `|arg|`):

```adesh
import Parallel;

Parallel.forEach(0, 10, fn(i) {
    print("iteration ", i);
});

let doubled = Parallel.map(data, fn(x) { return x * 2; });

let product = Parallel.reduce(data, 1.0, fn(acc, x) {
    return acc * x;
});
```

Closures passed to `Parallel.*` capture their defining environment (globals, outer functions, builtins like `print`).

### Explicit SIMD types (annotations)

```adesh
// Parser supports:
//   Simd<f64, 8>
//   vec4<f32>
```

These map to `HirType::Simd(element_type, lanes)` in the HIR layer.

---

## Runtime behavior

### Array representation

| Runtime `Value` | Origin | Notes |
|-----------------|--------|-------|
| `Value::Array(Vec<Value>)` | Some builtins, SIMD ops results | Homogeneous dynamic vector |
| `Value::DynArray` | **Array literals** `[1.0, 2.0, ...]` | Primary form from interpreter |
| `Value::RawArray(type, Vec<Value>)` | Typed raw arrays | Supported in broadcast + SIMD ops |

Broadcast detection (`runtime/simd/broadcast.rs`) treats all three as arrays and handles cross-type pairs (e.g. `Array` result + `DynArray` operand).

### Broadcast modes

| Mode | Example | Behavior |
|------|---------|----------|
| `ElementWise` | `[1,2] + [3,4]` | Same-length element-wise op |
| `ScalarRight` | `[1,2,3] + 5` | Apply scalar to each element |
| `ScalarLeft` | `2 * [1,2,3]` | Apply scalar to each element |
| `Incompatible` | `[1,2] + "hi"` | Runtime error |

### Interpreter dispatch path

Binary operators in `interpreter_core.rs`:

1. Fixed-width integer fast paths (when both operands are `I32`, `F64`, etc.)
2. Operator overloading on class instances
3. **Array involvement check** → delegate to unified ABI (`abi_add`, `abi_sub`, `abi_mul`, `abi_div`)
4. Scalar fallback via `bin_num`

The unified ABI (`runtime/abi/ops.rs`) routes any operand involving an array to `runtime/simd/ops.rs`.

### Expression fusion

Fused kernels avoid intermediate arrays:

| API | Computes | Implementation |
|-----|----------|----------------|
| `Simd.fusedMulAdd(a, b, d)` | `a * b + d` | Single pass over `f64` slices |
| `Simd.fusedSqrtMulAdd(a, b, c)` | `sqrt(a * b + c)` | Single pass |

Fusion logic lives in `runtime/simd/fusion.rs`; public entry points are in `runtime/simd/ops.rs`.

### CPU feature detection

`runtime/simd/cpu_features.rs` detects host ISA (SSE2, AVX2, AVX-512, NEON) and selects SIMD lane width for runtime loops. This is used by the interpreter fast paths today; compiler backends can reuse `SimdIsa` from `ir/simd/types.rs`.

---

## Standard library API

### `Simd` namespace (`import Simd;`)

Registered in `runtime/stdlib_src/simd/mod.rs` as a module object (not individual globals).

#### Namespace methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `sum` | `Simd.sum(arr)` | Sum reduction |
| `mean` | `Simd.mean(arr)` | Arithmetic mean |
| `min` / `max` | `Simd.min(arr)` | Element-wise min/max reduction |
| `abs` | `Simd.abs(arr)` | Element-wise absolute value |
| `sqrt` | `Simd.sqrt(arr)` | Element-wise square root |
| `dot` | `Simd.dot(a, b)` | Dot product |
| `add` / `sub` / `mul` / `div` | `Simd.add(a, b)` | Explicit element-wise ops |
| `fusedMulAdd` | `Simd.fusedMulAdd(a, b, d)` | Fused `a*b+d` |
| `fusedSqrtMulAdd` | `Simd.fusedSqrtMulAdd(a, b, c)` | Fused `sqrt(a*b+c)` |
| `concat` | `Simd.concat(a, b)` | Array concatenation |
| `vector` | `Simd.vector(arr)` | Returns vector instance handle |
| `workers` | `Simd.workers()` | Available CPU cores (diagnostic) |

#### `Simd.vector(arr)` instance methods

`sum()`, `mean()`, `min()`, `max()`, `abs()`, `sqrt()`, `len()`, `dot(other)`, `add(other)`, `mul(other)`, `scale(scalar)`, `toArray()`

Instance state is held in a native `SimdVector` handle backed by `Arc<Mutex<Vec<Value>>>`.

### `Parallel` namespace (`import Parallel;`)

Registered in `runtime/stdlib_src/concurrency/parallel_ops.rs`.

| Method | Signature | Description |
|--------|-----------|-------------|
| `forEach` | `Parallel.forEach(start, end, fn)` | Index loop `[start, end)` |
| `map` | `Parallel.map(arr, fn)` | Map over array |
| `reduce` | `Parallel.reduce(arr, init, fn)` | Left-fold reduce |
| `sum` | `Parallel.sum(arr)` | Parallel numeric sum (uses SIMD `array_sum` + scheduler for large inputs) |
| `workers` | `Parallel.workers()` | Active worker thread count |

**Interpreter note:** User closures in `map` / `reduce` / `forEach` currently execute **sequentially** in the interpreter (Rust `Send` / borrow-check limits on captured AdeshLang closures). Numeric `Parallel.sum` on large `f64` arrays uses the native scheduler. Full parallel closure execution is planned for compiled backends.

---

## Type system integration

### Array arithmetic inference

`typesystem/type_system/expression_inference.rs` defines `array_broadcast_binary()`:

- `[T] op [T]` → `[T]` (with numeric promotion on elements)
- `[T] op scalar` → `[T]`
- `scalar op [T]` → `[T]`
- Unknown array literals infer element type from context

Applied to `+`, `-`, `*`, `/` when operands are array-like.

### `HirType::Simd`

```rust
// parsing/hir.rs
Simd(Box<HirType>, u32)  // Simd<element, lanes>
```

Parser support in `parsing/parser/type_annotations.rs` for `Simd<f32, 8>` and `vec4<f32>`.

### Traits

`HirType::Simd` implements `Send` and `Sync` trait bounds in `typesystem/traits/mod.rs` so SIMD vector types can be used in concurrent contexts at the type level.

### LIR lowering

`backends/common/lir/lower/types.rs` lowers `HirType::Simd` to the appropriate LIR representation for backends that emit vector code.

---

## Compiler IR layer

Infrastructure for compile-time vectorization and parallelization lives under `src/ir/simd/` and `src/ir/parallel/`. These modules are used by tests and cost models today; automatic integration into the main `adeshlang compile` pipeline is **partial**.

### SIMD IR (`src/ir/simd/`)

| Module | Role |
|--------|------|
| `types.rs` | `SimdElement`, `SimdType`, `SimdIsa` |
| `instructions.rs` | Architecture-neutral SIMD instruction blocks |
| `analysis.rs` | Loop vectorization candidacy (aliasing, trip count) |
| `cost_model.rs` | `ExecutionStrategy`: Scalar / Simd / Parallel / ParallelSimd |
| `vectorize.rs` | `AutoVectorizer` pass |
| `lowering.rs` | Host ISA detection, lowering targets |

Default cost model thresholds:

- `min_simd_iterations = 8`
- `min_parallel_iterations = 1024`

### Parallel IR (`src/ir/parallel/`)

| Module | Role |
|--------|------|
| `analysis.rs` | Iteration independence, borrow-range splitting |
| `cost_model.rs` | `ParallelStrategy`: Sequential, StaticChunks, WorkStealing, NestedFlatten |
| `transform.rs` | `ParallelTransformer` — chunk planning |

Parallel safety checks reject loops with:

- Conflicting writes across iterations
- Side effects in the loop body
- Unsafe aliasing (unless disjoint ranges proven)

---

## Parallel scheduler

Location: `src/runtime/scheduler/`

### Components

| Module | Purpose |
|--------|---------|
| `mod.rs` | Global `OnceLock<Scheduler>`, `parallel_for`, `parallel_reduce` |
| `worker.rs` | Per-core worker queues, work stealing |
| `task.rs` | Task descriptors |
| `sync.rs` | Atomics, `Mutex`, `RwLock`, `Barrier`, `Semaphore`, `Condvar` wrappers |
| `channel.rs` | Bounded/unbounded channels with move semantics |

### Work-stealing algorithm

1. `parallel_for(start, end, body)` splits `[start, end)` into chunks sized by `iterations / num_workers` (minimum 64).
2. Each worker pushes chunks onto a **local queue**.
3. Idle workers **steal** from the tail of other workers' queues.
4. `parallel_reduce` uses tree reduction with optional deterministic ordering.

### Lifecycle

```rust
global_scheduler()   // lazy init on first parallel op
is_parallel_runtime_active()
shutdown_scheduler()   // graceful worker shutdown
num_workers()        // core count
```

---

## Examples

All examples are runnable with:

```bash
cargo run --bin adeshlang -- run examples/<path>.adesh
```

| Example | Path | Demonstrates |
|---------|------|--------------|
| Matrix / SAXPY | `examples/numerics/matrix_operations.adesh` | Operator SAXPY, dot, fusion, mean-centering, `Simd.vector().scale()` |
| Multidimensional Numerics | `examples/numerics/matrix_multidimensional.adesh` | 2D algebra, Transpose, Gram matrix, 3D batch matmul, N-D strided tensor reductions |
| Vector add | `examples/simd/vector_add.adesh` | `a+b`, broadcast, reductions, fusion |
| Dot product | `examples/simd/dot_product.adesh` | `Simd.dot`, instance `.dot()` |
| SIMD Matrix Math | `examples/simd/matrix_simd.adesh` | 2D matmul, SIMD FMA, Transpose, 3D batch matmul, 4D tensor operations |
| Reductions | `examples/simd/reductions.adesh` | `sum/mean/min/max/abs/sqrt`, `Parallel.sum` |
| Explicit SIMD | `examples/simd/explicit_simd.adesh` | `Simd.add/mul`, instance API |
| Parallel for | `examples/parallel/parallel_for.adesh` | `Parallel.forEach`, `Parallel.sum` |
| Parallel reduce | `examples/parallel/parallel_reduce.adesh` | `Parallel.reduce`, product fold |
| Parallel Matrix Math | `examples/parallel/matrix_parallel.adesh` | Parallel 2D matmul row reduction, Parallel 3D batch matmul, N-D tensor parallel map/sum |
| Legacy concurrency demo | `examples/concurrency/parallel_ops.adesh` | `Parallel.map/reduce/forEach` with closures |

---

## Testing

Targeted test suite (do **not** require full `cargo test`):

```bash
cargo test --test simd_parallel_tests
```

**30 tests** covering:

- SIMD type parsing (`vec4<f32>`, `Simd<f64, 8>`)
- `SimdValue` arithmetic, FMA, reductions
- Element-wise array ops + scalar broadcast
- Fusion kernels (`fusedMulAdd`, `fusedSqrtMulAdd`)
- Cost model strategy selection
- Parallel loop independence analysis
- `parallel_for` execution
- Zero-overhead check (`is_parallel_runtime_active` false before use)
- Differential scalar vs SIMD path correctness

---

## Source file map

```
src/
├── ir/
│   ├── simd/           # Compiler SIMD IR, vectorizer, cost model
│   └── parallel/       # Compiler parallel IR, analysis, transform
├── runtime/
│   ├── simd/           # Interpreter SIMD ops, broadcast, fusion
│   ├── scheduler/      # Work-stealing scheduler, sync, channels
│   ├── abi/ops.rs      # Unified + - * / routing to SIMD
│   └── stdlib_src/
│       ├── simd/mod.rs           # `import Simd` module object
│       └── concurrency/
│           └── parallel_ops.rs   # `import Parallel` module object
├── parsing/
│   ├── hir.rs                    # HirType::Simd
│   └── parser/type_annotations.rs
├── typesystem/
│   ├── traits/mod.rs             # Send/Sync for Simd types
│   └── type_system/
│       └── expression_inference.rs  # array_broadcast_binary
├── backends/common/lir/lower/
│   └── types.rs                  # Simd → LIR type lowering
└── execution/runtime_core/
    └── interpreter_core.rs       # Array op dispatch, import handlers,
                                  # closure capture for fn() literals

tests/
└── simd_parallel_tests.rs

docs/
├── simd_parallel.md              # Quick API reference
└── simd_parallel/README.md       # This document

examples/
├── numerics/matrix_operations.adesh
├── simd/*.adesh
├── parallel/*.adesh
└── concurrency/parallel_ops.adesh
```

---

## Known limitations & roadmap

### Current limitations

| Area | Status |
|------|--------|
| Native codegen SIMD | IR + lowering infrastructure exists; not fully wired into Cranelift/LLVM pipeline |
| Auto loop vectorization | `AutoVectorizer` pass implemented; not yet run in default compile pipeline |
| Parallel user closures | Sequential in interpreter; scheduler used for index loops and numeric reduce |
| Ownership-aware `parallel_for` rejection | Analysis types exist; full compile-time diagnostic not complete |
| Global builtins | `simd_sum`, `parallel_for`, etc. removed — use `import Simd` / `import Parallel` |

### Semantic choices

- **`+` on arrays is element-wise**, not concat. This is intentional (NumPy-style). Use `Simd.concat`.
- **Array literals** produce `DynArray`; SIMD ops may return `Array`. Broadcast layer handles both.
- **Mixed numeric types** in arrays coerce via `f64` fast paths; integer-only arrays use generic element-wise fallback.

### Roadmap

1. Wire `AutoVectorizer` + `ParallelTransformer` into the main IR pipeline after HIR lowering.
2. Emit native SIMD via Cranelift/LLVM from `ir/simd/lowering.rs`.
3. Parallel closure execution in interpreter via captured-environment task spawning (with `Send` checks).
4. Compile-time rejection of non-`Send` captures in `Parallel.forEach` bodies.
5. SIMD auto-vectorization diagnostics (`-Wsimd` style messages when loops fall back to scalar).

---

## Quick reference

```adesh
import Simd;
import Parallel;

// No import needed for operators:
let c = a * 2.0 + b;

// Namespace API:
let s = Simd.sum(data);
let v = Simd.vector(data);
print(v.dot(other));

Parallel.forEach(0, n, fn(i) { /* body */ });
let total = Parallel.sum(data);
print(Parallel.workers());
```

For questions or contributions, start with `tests/simd_parallel_tests.rs` and the examples under `examples/simd/` and `examples/parallel/`.
