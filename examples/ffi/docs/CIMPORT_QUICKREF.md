# $cImport Quick Reference

## Syntax

```adesh
$cImport("header_file.h")
```

## Examples

### Simple Usage
```adesh
$cImport("mylib.h")

fn main() {
    let result = add(5, 3);
    print(result);
}
```

### Multiple Headers
```adesh
$cImport("math.h")
$cImport("string.h")
$cImport("custom.h")
```

### Run Command
```bash
adeshlang run -l mylib program.adesh
```

## Type Mappings

| C | AdeshLang |
|---|----------|
| `int` | `i32` |
| `float` | `f32` |
| `double` | `f64` |
| `char*` | `ptr<u8>` |
| `void*` | `ptr<void>` |
| `size_t` | `usize` |
| `int32_t` | `i32` |
| `uint64_t` | `u64` |

## Supported Backends

✅ Interpreter  
✅ JIT  
✅ AOT  
✅ Bytecode VM

## Notes

- Works with all standard C function declarations
- Automatically adds header directory to library search path
- Falls back gracefully on parse errors
- No runtime overhead
