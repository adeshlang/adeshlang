# Recursion Optimization Examples

AdeshLang provides powerful recursion optimization features that can dramatically improve performance.

## Available Optimizations

| Flag | Description |
|------|-------------|
| `--fast-recursion` | Enable all recursion optimizations |
| `--memo` / `--memoize` | Enable memoization for pure functions |
| `--tco` / `--tail-call` | Enable tail call optimization |
| `--no-recursion-opt` | Disable all recursion optimizations |

## Running Examples

### Naive Fibonacci (demonstrates memoization speedup)

```bash
# Without optimization (slow - O(2^n))
adeshlang run examples/recursion/naive_fib.adesh

# With memoization (fast - O(n))
adeshlang run examples/recursion/naive_fib.adesh --jit --fast-recursion

# With verbose output
adeshlang run examples/recursion/naive_fib.adesh --jit --fast-recursion --verbose
```

### Tail-Recursive Fibonacci

```bash
# With TCO enabled
adeshlang run examples/recursion/tail_fib.adesh --jit --tco

# Compare with/without TCO
adeshlang run examples/recursion/tail_fib.adesh --jit --no-recursion-opt
```

### Factorial with Memoization

```bash
adeshlang run examples/recursion/factorial.adesh --jit --memo
```

### GCD (Euclidean Algorithm)

```bash
adeshlang run examples/recursion/gcd.adesh --jit --fast-recursion
```

## Performance Comparison

| Function | Input | Without Opt | With Memoization |
|----------|-------|-------------|------------------|
| fib(30)  | 30    | ~73s        | ~0.00s          |
| fib(40)  | 40    | ~hours      | ~0.00s          |
| fib(50)  | 50    | ~years      | ~0.00s          |

Memoization transforms exponential O(2^n) complexity to linear O(n).
