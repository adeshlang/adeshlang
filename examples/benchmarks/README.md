# AdeshLang Benchmarks

Performance benchmarks for testing AdeshLang across different execution backends.

## Purpose

These benchmarks help:
- Compare performance across backends (interpreter, JIT, Native JIT, AOT)
- Verify optimization effectiveness
- Identify performance bottlenecks
- Demonstrate real-world performance characteristics

## Running Benchmarks

### Quick Benchmark
```bash
# Run with interpreter (baseline)
time adesh run examples/benchmarks/01_numeric_ops.adesh

# Run with JIT
time adesh run --jit examples/benchmarks/01_numeric_ops.adesh

# Run with Native JIT (fastest)
time adesh run --njit examples/benchmarks/01_numeric_ops.adesh
```

### Compare All Backends
```bash
#!/bin/bash
echo "Interpreter:"
time adesh run examples/benchmarks/01_numeric_ops.adesh

echo "JIT:"
time adesh run --jit examples/benchmarks/01_numeric_ops.adesh

echo "Native JIT:"
time adesh run --njit examples/benchmarks/01_numeric_ops.adesh
```

## Benchmark Suite

### 01_numeric_ops.adesh
Tests numeric operations performance:
- Basic arithmetic (addition, multiplication)
- Hex literal operations
- Binary literal operations
- Function call overhead
- Recursive functions (Fibonacci)
- Nested loops
- Typed numeric operations

**Expected Performance:**
- Interpreter: Baseline (1x)
- JIT: 5-10x faster
- Native JIT: 100-200x faster

## Performance Expectations

### Backend Comparison

| Backend | Relative Speed | Best For |
|---------|---------------|----------|
| Interpreter | 1x | Development, debugging |
| JIT | 5-10x | Balanced performance |
| Native JIT | 100-200x | Production, compute-heavy |
| AOT | 100-250x | Standalone apps |

### Operation Performance

| Operation | Interpreter | Native JIT | Speedup |
|-----------|------------|------------|---------|
| Arithmetic | Slow | Fast | 100x+ |
| Function calls | Moderate | Fast | 50-100x |
| Recursion | Slow | Fast | 150-200x |
| Loops | Slow | Fast | 100-150x |

## Tips for Accurate Benchmarking

1. **Warm-up**: Run each benchmark 2-3 times, use the best time
2. **Consistent Environment**: Close other applications
3. **Build Mode**: Use release builds for accurate results
   ```bash
   cargo build --release
   ```
4. **Multiple Runs**: Average results from 5-10 runs
5. **Optimization Levels**: Try different `-O` flags
   ```bash
   adesh run --jit -O3 benchmark.adesh
   ```

## Sample Results

Example timing on modern hardware:

```
Interpreter:      5.234s
JIT:             0.621s  (8.4x faster)
Native JIT:      0.045s  (116x faster)
```

## Creating New Benchmarks

Template for new benchmark:

```adesh
print("=== My Benchmark ===");

fn benchmark_operation(iterations: i64): i64 {
    let result: i64;
    result = 0;
    let i: i64;
    i = 0;
    while i < iterations {
        // Your operation here
        result = result + 1;
        i = i + 1;
    }
    return result;
}

let result = benchmark_operation(10000);
print("Result: ");
print(result);
```

## Profiling

For detailed profiling:

```bash
# With profiling enabled
adesh run --profile examples/benchmarks/01_numeric_ops.adesh

# With memory profiling
adesh run --memory examples/benchmarks/01_numeric_ops.adesh
```

## Related Documentation

- [backends.md](../../docs/backends.md) - Backend architecture details
- [PERFORMANCE_COMPARISON.md](../../PERFORMANCE_COMPARISON.md) - Detailed performance analysis

---

*Benchmarks are designed to be consistent across all execution backends for fair comparison.*
