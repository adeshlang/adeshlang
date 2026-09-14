# Performance Examples

Examples demonstrating performance optimization techniques in AdeshLang.

## Files

| File | Description |
|------|-------------|
| `fast_map_reduce.adesh` | JIT fast-path optimizations for map/filter/reduce operations. Includes benchmarking and performance tips. |

## Key Concepts

### JIT Fast Paths
The JIT compiler provides optimized code paths for:
- Integer and float arithmetic
- String operations (SSO-aware)
- Array indexing
- Object property access

### Hidden Classes
Objects with consistent property layouts share hidden classes, enabling:
- Fast inline caching for property access
- Efficient polymorphic dispatch

### Tips for Best Performance

1. **Use consistent types**: Avoid mixing types in the same variable or array
2. **Keep object shapes stable**: Initialize all properties in constructors
3. **Prefer simple predicates**: Use inline expressions over function calls
4. **Avoid allocations in hot paths**: Reuse objects where possible

## Running Examples

```bash
# Interpreter mode
adesh run examples/perf/fast_map_reduce.adesh

# JIT mode (faster)
adesh run examples/perf/fast_map_reduce.adesh --jit

# With timing statistics
adesh run examples/perf/fast_map_reduce.adesh --jit --time
```

## Related Documentation

- [JIT Optimizations](../jit/README.md)
- [Examples Index](../../docs/examples.md)
