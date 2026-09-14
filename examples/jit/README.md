# JIT Compilation Examples

AdeshLang provides multiple JIT compilation modes for different performance needs.

## JIT Modes

| Mode | Flag | Description |
|------|------|-------------|
| **Interpreter** | (default) | No JIT, reference implementation |
| **Basic JIT** | `--jit` | Standard JIT compilation |
| **Tiered JIT** | `--tiered` / `--tjit` | T0→T1→T2 progressive optimization |
| **Adaptive JIT** | `--adaptive` / `--ajit` | Speculative optimization with deopt |
| **Mixed** | `--mixed` | Interpreter + JIT for hot paths |
| **Safe** | `--safe` | Verified execution mode |

## Running Examples

### Basic JIT vs Interpreter

```bash
# Interpreter (baseline)
adeshlang run examples/jit/benchmark.adesh

# Basic JIT
adeshlang run examples/jit/benchmark.adesh --jit

# Compare with verbose output
adeshlang run examples/jit/benchmark.adesh --jit --verbose
```

### Tiered JIT

```bash
# Tiered compilation with profiling
adeshlang run examples/jit/tiered_demo.adesh --tiered

# With statistics
adeshlang run examples/jit/tiered_demo.adesh --tiered --stats
```

### Adaptive JIT

```bash
# Speculative optimization
adeshlang run examples/jit/adaptive_demo.adesh --adaptive

# Show deoptimization events
adeshlang run examples/jit/adaptive_demo.adesh --adaptive --verbose
```

## Performance Comparison

| Benchmark | Interpreter | JIT | Tiered | Adaptive |
|-----------|-------------|-----|--------|----------|
| fib(30)   | ~73s        | ~40ms | ~35ms | ~30ms |
| matmul(256)| ~5s        | ~50ms | ~45ms | ~40ms |
| sort(100k) | ~2s        | ~20ms | ~18ms | ~15ms |

Adaptive JIT provides the best performance for dynamic code patterns.
