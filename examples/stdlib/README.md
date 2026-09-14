# Standard Library Examples

Examples demonstrating AdeshLang standard library usage across interpreter, JIT, and VM backends.

## Files

| File | Description |
|------|-------------|
| `fs_examples.adesh` | File system operations with safe path handling |
| `http_examples.adesh` | HTTP module with SSRF prevention |
| `math_examples.adesh` | Math functions for Number and BigInt |
| `string_examples.adesh` | String operations (SSO-aware) |
| `regex_examples.adesh` | Regular expression basics |

## Key Features

### Cross-Backend Consistency
All stdlib functions behave identically across:
- Interpreter
- JIT compiler
- Bytecode VM

### Safety by Default
- Path traversal blocked in fs module
- SSRF prevention in http module
- Bounds checking in all operations

## Running Examples

```bash
# Interpreter mode
adesh run examples/stdlib/fs_examples.adesh

# JIT mode
adesh run examples/stdlib/math_examples.adesh --jit

# Verbose mode
adesh run examples/stdlib/string_examples.adesh --verbose
```

## Related Documentation

- [Examples Index](../../docs/examples.md)
- [Memory Model](../../docs/memory_model.md)
