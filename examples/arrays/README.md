# Array and Tuple Examples

This folder demonstrates array and tuple operations across execution backends.

Contents:
- `simple_aot_test.adesh`: Fixed-size arrays, push/pop/clear/extend, spread.
- `advanced_array_ops.adesh`: Index set (`set_index`), first/last/len, extend, spread, capacity/metadata.
- `tuple_examples.adesh`: Tuple creation, length, first/last, destructuring, indexing.

Supported operations:
- Arrays: `len`, `capacity`, `metadata_size`, `append/push`, `pop`, `clear`, `extend`, `first`, `last`, `get_index` (`arr[i]`), `set_index` (`arr.set_index(i, v)`), spread `[...arr]`.
- Tuples: `len`, `first`, `last`, indexing (`t[i]`), destructuring `let (a,b,...) = t`.

Backend notes:
- Interpreter/JIT: Full support including higher-order methods (`map`, `filter`, `reduce`).
- AOT (Cranelift): Array basics implemented; `map/filter/reduce` currently no-op for mutation and return values. Lambda-based transforms are not yet lowered to native calls.

Run examples (debug):

```
cargo run -- run examples/arrays/advanced_array_ops.adesh --jit
cargo run -- run examples/arrays/tuple_examples.adesh --jit
cargo run -- compile-aot examples/arrays/simple_aot_test.adesh target/aot/simple_aot_test.exe --debug
.\n+target\aot\simple_aot_test.exe
```

Future work:
- AOT support for invoking lambdas (`map/filter/reduce`).
- Capacity tracking semantics for `clear` (retain vs reset).
