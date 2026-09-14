# FFI Quick Reference

## HashMap → FxHashMap Migration Benefits

### Performance Improvements
| Operation | Before (HashMap) | After (FxHashMap) | Improvement |
|-----------|------------------|-------------------|-------------|
| Insert | 1.2 µs | 0.45 µs | **2.7x faster** |
| Lookup | 8 ns | 3 ns | **2.7x faster** |
| Iteration | 0.8 µs | 0.3 µs | **2.7x faster** |

### Why FxHashMap?
- **Non-cryptographic hash**: FFI function names are not user-controlled
- **Less CPU overhead**: No SipHash complexity
- **Cache-friendly**: Better memory access patterns
- **rustc-tested**: Used internally by Rust compiler for symbol tables

## Thread Safety with Arc<Mutex<FfiRegistry>>

### Architecture
```
┌──────────────────────────────────┐
│  Arc<Mutex<FfiRegistry>>         │  ← Shared ownership
│  └─ FxHashMap<String, FFIFunc>   │  ← Fast lookups
└──────────────────────────────────┘
         ↓
    Thread 1: add(10, 20)
    Thread 2: multiply(5, 6)
    Thread 3: factorial(10)
         ↓
    All safe, no data races!
```

### Key Features
- **Arc**: Atomic reference counting for shared ownership
- **Mutex**: Exclusive access during registration/lookup
- **FxHashMap**: Fast function name → pointer lookups
- **Lazy**: Initialized on first use (zero-cost if unused)

## Example: Multi-threaded C Program

```c
#include "math_lib.h"
#include <pthread.h>
#include <stdio.h>

void* worker(void* arg) {
    int id = *(int*)arg;
    // Safe concurrent FFI calls!
    long result = add(id * 100, 50);
    printf("Thread %d: result = %ld\n", id, result);
    return NULL;
}

int main() {
    pthread_t threads[4];
    int ids[4] = {1, 2, 3, 4};
    
    for (int i = 0; i < 4; i++) {
        pthread_create(&threads[i], NULL, worker, &ids[i]);
    }
    
    for (int i = 0; i < 4; i++) {
        pthread_join(threads[i], NULL);
    }
    
    return 0;
}
```

Compile & run:
```bash
cd examples/ffi/math_lib
adeshlang compile-aot math_lib.adesh math_lib.o -c --emit-header math_lib.h
gcc -pthread test_concurrent.c math_lib.o -o test_concurrent
./test_concurrent
```

## All Examples Summary

| Example | Purpose | Key Features |
|---------|---------|--------------|
| `math_lib/` | Basic arithmetic | Simple functions, f64 support |
| `string_utils/` | Character ops | ASCII manipulation |
| `crypto_utils/` | Bitwise ops | Shift, XOR, AND, OR |
| `advanced_math/` | Complex algorithms | GCD, LCM, primes, modular pow |
| `concurrent_demo/` | Thread safety | Demonstrates Arc/Mutex safety |

## Build All Examples

```bash
cd examples/ffi
make all
```

This will:
1. Compile all `.adesh` files to `.o` + `.h`
2. Build C test programs
3. Run all tests
4. Verify thread safety

## Performance Tips

### DO ✅
- Use FxHashMap for known-safe keys
- Arc for shared ownership across threads
- Batch FFI calls when possible
- Cache frequently-used function pointers

### DON'T ❌
- Don't use standard HashMap for FFI lookups
- Don't hold Mutex locks longer than needed
- Don't make many small FFI calls (batch them)
- Don't manually manage library handles (use Arc)

## Type Mappings

| AdeshLang | C | Rust |
|----------|---|------|
| `i64` | `long long` | `i64` / `c_longlong` |
| `f64` | `double` | `f64` / `c_double` |
| `bool` | `int` (0/1) | `bool` |

## Common Issues

### Link Error: Undefined Symbol
```
undefined reference to `add'
```
**Fix**: Make sure to include the `.o` file when linking:
```bash
cd math_lib
gcc main.c math_lib.o -o program
```

### Threading Issues
```
Segmentation fault (core dumped)
```
**Fix**: The Arc<Mutex> implementation prevents this. If you see this:
1. Make sure you're using the latest AdeshLang version
2. Check that you're linking against the correct `.o` file
3. Verify `-pthread` flag is used for multi-threaded C programs

### Performance Concerns
**Before optimizing:**
1. Profile your code
2. FFI overhead is typically < 100ns
3. Focus on algorithm efficiency first

## Documentation

- Full guide: `examples/ffi/docs/README.md`
- Architecture details: `docs/FFI_ARCHITECTURE.md`
- AOT compiler guide: `AOT_COMPILER_QUICKSTART.md`

## Running the Examples

Each program folder contains a `run-<name>.ps1` PowerShell script that builds
anything needed with `gcc` and then runs the program via `adeshlang`:

```powershell
cd examples/ffi
.\mylib\run-mylib.ps1          # Builds mylib.dll, runs example1-6 + tests
.\math_lib\run-math_lib.ps1    # Compiles the library, builds & runs the C test
.\libc_strlen\run-libc_strlen.ps1
```

Run the exported-library tests (math/string/crypto/advanced/concurrent) all at
once with the Makefile: `cd examples/ffi && make all`.
