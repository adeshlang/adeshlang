# AdeshLang Memory Pointer Examples

This directory contains comprehensive examples demonstrating AdeshLang's pointer system.

## Basic Examples

### `pointer_basic.adesh`
Fundamental pointer operations: allocation, access, and RAII cleanup.

**Run:**
```bash
adesh run examples/memory/pointer_basic.adesh
```

### `pointer_types.adesh`
Demonstrates typed pointer arithmetic with `*u8`, `*i32`, and `*f64`.

**Run:**
```bash
adesh run examples/memory/pointer_types.adesh
```

## RAII and Control Flow

### `pointer_raii.adesh`
Shows automatic cleanup on early returns, breaks, and continues.

**Run:**
```bash
adesh run examples/memory/pointer_raii.adesh
```

## Failure Cases (Expected Errors)

### `pointer_uaf_fail.adesh`
**Demonstrates:** Use-after-free detection  
**Expected:** Runtime error on line accessing freed pointer

**Run:**
```bash
adesh run examples/memory/pointer_uaf_fail.adesh
# Expected: ERROR: Use-after-free: pointer X was already freed
```

### `pointer_bounds_fail.adesh`
**Demonstrates:** Bounds checking on pointer access  
**Expected:** Runtime error on out-of-bounds access

**Run:**
```bash
adesh run examples/memory/pointer_bounds_fail.adesh
# Expected: ERROR: Pointer load out of bounds
```

## Function Usage

### `pointer_fn_args.adesh`
Passing pointers to functions (no ownership transfer).

**Run:**
```bash
adesh run examples/memory/pointer_fn_args.adesh
```

### `pointer_return.adesh`
Returning pointers from functions (ownership transfer to caller).

**Run:**
```bash
adesh run examples/memory/pointer_return.adesh
```

## Key Concepts

### Typed Pointer Arithmetic
- `*u8` uses byte-level indexing (1 byte per element)
- `*i32` uses 4-byte indexing
- `*f64` uses 8-byte indexing
- Formula: `address = base + (index * sizeof(type))`

### RAII Guarantees
- Pointers are freed automatically at scope exit
- Cleanup happens on ALL control flow paths (return, break, continue)
- Manual `free()` is optional but allowed

### Safety Checks
- ✅ Bounds checking on every access
- ✅ Use-after-free detection
- ✅ Double-free prevention
- ✅ Invalid pointer detection
- ✅ Debug mode memory poisoning

### Ownership Rules
- Passing to function: caller retains ownership
- Returning from function: ownership transfers to caller
- RAII applies at the owning scope

## Testing Tips

1. **Run in debug mode** to enable memory poisoning:
   ```bash
   adesh build --debug examples/memory/pointer_basic.adesh
   adesh run pointer_basic.adeshbc
   ```

2. **Test failure cases** to verify error handling:
   ```bash
   # These should fail with clear error messages
   adesh run examples/memory/pointer_uaf_fail.adesh
   adesh run examples/memory/pointer_bounds_fail.adesh
   ```

3. **Check RAII cleanup** with complex control flow:
   ```bash
   adesh run examples/memory/pointer_raii.adesh
   ```

## See Also

- [docs/pointers.md](../../docs/pointers.md) - Complete pointer system documentation
- [docs/memory_model.md](../../docs/memory_model.md) - Memory model overview
- [README.md](../../README.md) - AdeshLang main documentation
