# SIMD + Parallel Execution — Quick Reference

> **Full implementation guide:** [simd_parallel/README.md](simd_parallel/README.md)

## Usage

```adesh
import Simd;
import Parallel;

// Namespace methods
let total = Simd.sum([1, 2, 3, 4]);
let dot   = Simd.dot(a, b);
let fused = Simd.fusedMulAdd(a, b, d);

// Vector instance object (object.method())
let v = Simd.vector([1, 2, 3, 4]);
print(v.sum());
print(v.dot([4, 3, 2, 1]));
print(v.scale(2.0));

// Parallel namespace (use fn() syntax for callbacks)
Parallel.forEach(0, n, fn(i) { print(i); });
let sum = Parallel.sum(data);
let mapped = Parallel.map(data, fn(x) { return x * 2; });
print(Parallel.workers());
```

**Operators** `a + b`, `a * b`, `scalar * array` work without import (element-wise, SIMD-optimized at runtime).

**Concatenation** is `Simd.concat(a, b)` — `+` on two arrays is **not** concat.

## Simd API

| Method | Description |
|--------|-------------|
| `Simd.sum(arr)` | Sum reduction |
| `Simd.mean(arr)` | Mean |
| `Simd.min(arr)` / `Simd.max(arr)` | Min / max |
| `Simd.dot(a, b)` | Dot product |
| `Simd.add(a, b)` / `sub` / `mul` / `div` | Element-wise |
| `Simd.fusedMulAdd(a, b, d)` | Fused a×b+d |
| `Simd.fusedSqrtMulAdd(a, b, c)` | Fused √(a×b+c) |
| `Simd.concat(a, b)` | Concatenate arrays |
| `Simd.vector(arr)` | Instance handle |

### Vector instance methods

`sum()`, `mean()`, `min()`, `max()`, `abs()`, `sqrt()`, `len()`, `dot(other)`, `add(other)`, `mul(other)`, `scale(scalar)`, `toArray()`

## Parallel API

| Method | Description |
|--------|-------------|
| `Parallel.forEach(start, end, fn)` | Parallel index loop |
| `Parallel.map(arr, fn)` | Map |
| `Parallel.reduce(arr, init, fn)` | Reduce |
| `Parallel.sum(arr)` | Parallel sum |
| `Parallel.workers()` | Worker count |

## Examples

```bash
# SIMD Examples
cargo run --bin adeshlang -- run examples/simd/vector_add.adesh
cargo run --bin adeshlang -- run examples/simd/dot_product.adesh
cargo run --bin adeshlang -- run examples/simd/matrix_simd.adesh

# Numerics Examples
cargo run --bin adeshlang -- run examples/numerics/matrix_operations.adesh
cargo run --bin adeshlang -- run examples/numerics/matrix_multidimensional.adesh

# Parallel Execution Examples
cargo run --bin adeshlang -- run examples/parallel/parallel_for.adesh
cargo run --bin adeshlang -- run examples/parallel/parallel_reduce.adesh
cargo run --bin adeshlang -- run examples/parallel/matrix_parallel.adesh
```

## Tests

```bash
cargo test --test simd_parallel_tests
```
