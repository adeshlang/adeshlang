# AdeshLang Examples

Comprehensive examples demonstrating AdeshLang's features and capabilities.

## Quick Start

```bash
# Basic example
adeshlang run examples/misc/basic.adesh

# With JIT compilation
adeshlang run examples/misc/basic.adesh --jit

# With verbose output
adeshlang run examples/misc/basic.adesh --jit --verbose
```

## Example Categories

### Core Language Features

| Directory | Description |
|-----------|-------------|
| `oop/` | Classes, inheritance, polymorphism |
| `async/` | Async/await, promises |
| `data_structures/` | Lists, dicts, sets, tuples |
| `decorators/` | Function and class decorators |
| `formatting/` | F-strings, format(), % formatting |
| `loops/` | For, while, do-while loops |
| `math/` | Math functions and constants |
| `print/` | Print formatting and colors |
| `input/` | User input handling |
| `errors/` | Error handling and custom errors |
| `datatype/` | Associated methods for strings, arrays, tuples, sets, dictionaries, numbers, and complex values |

### Performance & Optimization

| Directory | Description | Key Flags |
|-----------|-------------|-----------|
| `recursion/` | Memoization, TCO, trampolining | `--fast-recursion` |
| `jit/` | JIT modes and benchmarks | `--jit`, `--tiered`, `--adaptive` |
| `fib/` | Fibonacci implementations | `--jit --fast-recursion` |

### Advanced Features

| Directory | Description | Key Flags |
|-----------|-------------|-----------|
| `ml/` | Tensors, neural networks | `--jit` |
| `concurrency/` | Async, threads, channels | `--parallel` |
| `gpu/` | GPU acceleration | `--device=cuda/metal` |
| `wasm/` | WebAssembly compilation | `--wasm` |
| `adl/` | ADL ecosystem manifest, lockfile, and package layout examples | `adl install`, `adl build` |
| `package_management_demo/` | End-to-end package manager demo showing library creation, installation, and imports | `adesh install`, `adesh run` |

## CLI Flags Reference

### Execution Modes

```bash
# Interpreter (default)
adeshlang run file.adesh

# JIT compilation
adeshlang run file.adesh --jit

# Tiered JIT (T0 -> T1 -> T2)
adeshlang run file.adesh --tiered

# Adaptive JIT (speculative optimization)
adeshlang run file.adesh --adaptive

# Mixed mode (interpreter + JIT for hot paths)
adeshlang run file.adesh --mixed

# Safe mode (verified execution)
adeshlang run file.adesh --safe
```

### Optimization Flags

```bash
# Recursion optimizations
--fast-recursion    # Enable all recursion optimizations
--memo              # Enable memoization only
--tco               # Enable tail call optimization only
--no-recursion-opt  # Disable recursion optimizations

# Parallelism
--parallel          # Enable parallel execution
--threads=N         # Set thread pool size
```

### Output Flags

```bash
--verbose           # Detailed output
--quiet             # Minimal output
--stats             # Show statistics
--time              # Show timing information
```

### Device Selection

```bash
--device=cpu        # CPU execution (default)
--device=cuda       # NVIDIA GPU
--device=metal      # Apple GPU
--device=vulkan     # Cross-platform GPU
```

## Performance Examples

### Recursion with Memoization

```bash
# Without memoization (slow)
adeshlang run examples/recursion/naive_fib.adesh

# With memoization (>1000x faster)
adeshlang run examples/recursion/naive_fib.adesh --jit --fast-recursion
```

**Expected Performance:**
| Input | No Opt | With Memoization |
|-------|--------|------------------|
| fib(30) | ~73s | ~0.00s |
| fib(40) | hours | ~0.00s |
| fib(50) | years | ~0.00s |

### JIT Compilation

```bash
# Interpreter baseline
adeshlang run examples/jit/benchmark.adesh

# JIT compilation
adeshlang run examples/jit/benchmark.adesh --jit
```

**Expected Speedup:** ~400x for compute-intensive code

### GPU Acceleration

```bash
adeshlang run examples/gpu/matmul_gpu.adesh --device=cuda
```

**Expected Speedup:** 10-100x for large matrix operations

## Running All Examples

```bash
# Run all examples in a directory
for f in examples/recursion/*.adesh; do
    echo "Running $f..."
    adeshlang run "$f" --jit --fast-recursion
done
```

## Writing Your Own Examples

1. Create a `.adesh` file in the appropriate directory
2. Add a `main()` function as the entry point
3. Use `print()` for output
4. Run with desired flags:

```adesh
// my_example.adesh
fn main() {
    print("Hello, AdeshLang!");
    
    let result = expensive_computation();
    print("Result: {result}");
}

main();
```

```bash
adeshlang run my_example.adesh --jit --verbose
```

## Documentation

- Native datatype methods: `datatype/README.md`
- Native datatype API reference: `datatype/API_REFERENCE.md`

- Full CLI documentation: `docs/cli.md`
- Language reference: `docs/language.md`
- API reference: `docs/api.md`
- ADL ecosystem progress: `ADL_ECOSYSTEM_PROGRESS.md`
