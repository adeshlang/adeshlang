# AdeshLang FFI Examples & Documentation

Complete guide to AdeshLang's Foreign Function Interface (FFI) for seamless C/C++/Rust interoperability.

**Key Features:**
- **Thread-Safe**: Arc<Mutex<FfiRegistry>> ensures safe concurrent access
- **Fast Lookups**: FxHashMap (rustc-hash) for optimal performance
- **Zero-Copy**: Direct function pointer calls with minimal overhead

## Folder Layout

Each program gets its own folder, so the `.adesh` source, C source, headers,
libraries, binaries, and test drivers that belong to one program live together:

```
ffi/
├── Makefile              # Builds the exported AdeshLang libraries + C test drivers
├── docs/                 # Documentation (this README, quick reference, $cImport docs)
├── mylib/                # mylib C library + example1-6 + test_cimport/test_bool
├── mymath/               # mymath C library + c_import_example
├── wingui/               # wingui C library + demo_window + test_window_*
├── math_lib/             # Exported math library + C test driver + binaries
├── string_utils/         # Exported string library + C test driver + binaries
├── crypto_utils/         # Exported crypto library + C test driver + binaries
├── advanced_math/        # Exported advanced-math library + C test driver
├── concurrent_demo/      # Exported concurrent library + C test driver
├── use_from_rust/        # Rust consumer of the exported libraries
├── libc_strlen/          # strlen demo against the platform runtime
├── math_functions/       # libm demo (sqrt/pow/sin/cos)
├── multi_abi/            # C/Rust/System ABI demo
├── ffi_import_demo/      # C runtime demo (strlen/getenv/strcmp/atoi/llabs)
├── advanced_types/       # Type-mapping reference demo
├── custom_lib/           # Custom C library demo
├── debug_test/           # Small strlen debug test
└── test_ffi_complete/    # Full FFI feature test
```

## New Working Examples!

We've created a comprehensive C library (mylib) with 50+ functions and 6 example programs that demonstrate real FFI usage:

### Using $cImport (Automatic Header Parsing)

AdeshLang now supports automatic C header parsing with the `$cImport` directive:

```adesh
$cImport("mylib.h")

fn main() {
    // All functions from mylib.h are now available!
    let result = add_int(10, 20);
    print(result);
}
```

The `$cImport` directive:
- Parses C header files automatically
- Generates extern declarations for all functions
- Adds the header's directory to the library search path
- Works with all backends (interpreter, JIT, AOT)

### Quick Test
```bash
cd examples/ffi

# mylib demos live with mylib.c/.h/.dll in mylib/:
cd mylib
adeshlang run example6_cimport.adesh -L . -l mylib  # Uses $cImport!
adeshlang run test_cimport.adesh -L . -l mylib      # Comprehensive test
adeshlang run example1_basic_math.adesh -L . -l mylib
adeshlang run example2_advanced_math.adesh -L . -l mylib
adeshlang run example3_strings.adesh -L . -l mylib
adeshlang run example4_random_time.adesh -L . -l mylib
adeshlang run example5_boolean.adesh -L . -l mylib
```

### Example Programs
1. **example1_basic_math.adesh** - Integer and floating-point arithmetic
2. **example2_advanced_math.adesh** - Trigonometry, logarithms, factorials
3. **example3_strings.adesh** - String operations and printing
4. **example4_random_time.adesh** - Random number generation and timestamps
5. **example5_boolean.adesh** - Even/odd checking and prime number finding
6. **example6_cimport.adesh** - **NEW!** Demonstrates $cImport automatic header parsing
7. **test_cimport.adesh** - **NEW!** Comprehensive $cImport test

### C Library (mylib)
The included mylib library provides:
- Math operations (basic, advanced, trigonometry)
- String functions (length, compare, print)
- Array operations (sum, average, min, max, sort)
- Random number generation (seeded and unseeded)
- Time functions (Unix timestamps)
- Boolean validation (even, odd, prime)
- Memory allocation functions
- Type conversion utilities

**Build the library:**
```bash
cd examples/ffi/mylib

# Windows (GCC/MinGW)
gcc -shared -o mylib.dll mylib.c -lm

# Linux
gcc -shared -fPIC -o libmylib.so mylib.c -lm

# macOS
gcc -shared -fPIC -o libmylib.dylib mylib.c -lm
```

**Important Note on Boolean Returns:**
Due to an FFI quirk, C functions returning boolean values (0/1) should NOT be compared with `== 1`. Use them directly in conditionals:

```adesh
extern "C" {
    fn is_even(n: i32) -> i32;
}

fn main() {
    // ✓ CORRECT
    if is_even(42) {
        print("Even!\n");
    }
    
    // ✗ WRONG (will not work)
    if is_even(42) == 1 {
        print("Even!\n");
    }
}
```

## Table of Contents

- [Quick Start](#quick-start)
- [Included Examples](#included-examples)
- [Architecture](#architecture)
- [How FFI Works](#how-ffi-works)
- [Step-by-Step Guide](#step-by-step-guide)
- [Type Mappings](#type-mappings)
- [API Reference](#api-reference)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)
- [Advanced Topics](#advanced-topics)

## Architecture

AdeshLang's FFI system is built for performance and safety:

```
┌─────────────────────────────────────────┐
│  Arc<Mutex<FfiRegistry>>                │
│  ├─ FxHashMap<String, ForeignFunction>  │  ← Fast lookups
│  ├─ Vec<LibraryHandle>                  │  ← Keep libs loaded
│  └─ Vec<SearchPath>                     │
└─────────────────────────────────────────┘
         ↓ Thread-safe access
    Multiple threads can safely
    call FFI functions concurrently
```

**Benefits:**
- **Arc** enables shared ownership across threads
- **Mutex** ensures exclusive write access when registering functions
- **FxHashMap** provides faster lookups than standard HashMap (~2-3x)
- **Lazy initialization** defers registry creation until first use

## Quick Start

### Minimal Example (30 seconds)

1. **Create `add.adesh`:**
```adesh
export fn add(a: i64, b: i64): i64 {
    return a + b;
}
```

2. **Compile to C-compatible library:**
```bash
adeshlang compile-aot add.adesh add.o -c --emit-header add.h
```

3. **Use in C:**
```c
#include "add.h"
#include <stdio.h>

int main() {
    printf("Result: %ld\n", add(10, 20));  // Output: Result: 30
    return 0;
}
```

4. **Build & run:**
```bash
gcc test.c add.o -o test.exe
./test.exe
```

## Included Examples

Three complete library examples with C test programs:

### 1. **Math Library** (`math_lib.adesh`)

Basic arithmetic and mathematical functions.

**Functions:**
- `add(a: i64, b: i64): i64` - Integer addition
- `multiply(a: i64, b: i64): i64` - Integer multiplication
- `power(base: i64, exp: i64): i64` - Integer exponentiation
- `factorial(n: i64): i64` - Iterative factorial
- `add_floats(a: f64, b: f64): f64` - Floating-point addition
- `multiply_floats(a: f64, b: f64): f64` - Floating-point multiplication

**Build & Test:**
```bash
cd examples/ffi/math_lib
adeshlang compile-aot math_lib.adesh math_lib.o -c --emit-header math_lib.h
gcc test_math.c math_lib.o -o test_math.exe
./test_math.exe
```

**Output:**
```
=== Math Library Tests ===
add(10, 20) = 30
multiply(7, 6) = 42
power(2, 10) = 1024
factorial(5) = 120
add_floats(3.14, 2.86) = 6.00
multiply_floats(2.5, 4.0) = 10.00
```

### 2. **String Utilities** (`string_utils.adesh`)

Character and digit manipulation functions.

**Functions:**
- `char_to_lower(ch: i64): i64` - Convert ASCII uppercase to lowercase
- `sum_of_digits(n: i64): i64` - Sum of digits in a number
- `is_even(n: i64): i64` - Check if number is even (returns 0 or 1)
- `is_odd(n: i64): i64` - Check if number is odd (returns 0 or 1)

**Build & Test:**
```bash
cd examples/ffi/string_utils
adeshlang compile-aot string_utils.adesh string_utils.o -c --emit-header string_utils.h
gcc test_string.c string_utils.o -o test_string.exe
./test_string.exe
```

### 3. **Crypto Utilities** (`crypto_utils.adesh`)

Bitwise operations and mathematical functions.

**Functions:**
- `xor_values(a: i64, b: i64): i64` - XOR operation
- `and_values(a: i64, b: i64): i64` - AND operation
- `left_shift/right_shift` - Bit shifting
- `count_set_bits(n: i64): i64` - Count 1s in binary
- `fibonacci(n: i64): i64` - Iterative Fibonacci

### 4. **Advanced Math** (`advanced_math.adesh`) 🆕

Complex mathematical algorithms demonstrating advanced FFI usage.

**Functions:**
- `gcd(a: i64, b: i64): i64` - Greatest common divisor (Euclidean algorithm)
- `lcm(a: i64, b: i64): i64` - Least common multiple
- `is_prime(n: i64): i64` - Primality test (optimized trial division)
- `nth_fibonacci(n: i64): i64` - Nth Fibonacci number (iterative)
- `sum_range(start: i64, end: i64): i64` - Sum of range
- `modular_pow(base: i64, exp: i64, mod: i64): i64` - Modular exponentiation
- `sqrt_approx(x: f64): f64` - Square root via binary search
- `circle_area(r: f64): f64` - Circle area calculation
- `circle_circumference(r: f64): f64` - Circle circumference

**Build & Test:**
```bash
cd examples/ffi/advanced_math
adeshlang compile-aot advanced_math.adesh advanced_math.o -c --emit-header advanced_math.h
gcc test_advanced_math.c advanced_math.o -o test_advanced_math.exe -lm
./test_advanced_math.exe
```

**Output:**
```
=== Advanced Math Library Tests ===

GCD Tests:
  gcd(48, 18) = 6
  gcd(100, 50) = 50

LCM Tests:
  lcm(12, 18) = 36
  lcm(21, 6) = 42

Prime Tests:
  is_prime(17) = 1 (1=prime)
  is_prime(20) = 0 (0=composite)
  is_prime(97) = 1 (1=prime)

Fibonacci Tests:
  nth_fibonacci(10) = 55
  nth_fibonacci(15) = 610

✅ All tests passed!
```

### 5. **Concurrent Demo** (`concurrent_demo.adesh`) 🆕

Demonstrates thread-safe FFI function calls using Arc<Mutex<FfiRegistry>>.

**Functions:**
- `hash_simple(value: i64): i64` - Simple hash function
- `counter_increment(current: i64, step: i64): i64` - Counter operations
- `compute_intensive(n: i64): i64` - CPU-intensive workload
- `process_batch(start: i64, count: i64): i64` - Batch processing
- `validate_range(value: i64, min: i64, max: i64): i64` - Range validation

**Build & Test:**
```bash
cd examples/ffi/concurrent_demo
adeshlang compile-aot concurrent_demo.adesh concurrent_demo.o -c --emit-header concurrent_demo.h
gcc -pthread test_concurrent.c concurrent_demo.o -o test_concurrent.exe
./test_concurrent.exe
```

**Output:**
```
=== Concurrent FFI Thread Safety Demo ===
Testing Arc<Mutex<FfiRegistry>> thread safety...

Thread 0 completed 3000 FFI calls
Thread 1 completed 3000 FFI calls
Thread 2 completed 3000 FFI calls
Thread 3 completed 3000 FFI calls

Total FFI calls across all threads: 12000
✅ Thread safety test completed successfully!
   Arc<Mutex<FfiRegistry>> ensures safe concurrent access
```

**Key Features Demonstrated:**
- Multiple threads calling FFI functions concurrently
- No data races or undefined behavior
- Arc enables shared ownership across thread boundaries
- Mutex ensures exclusive access during registry operations

**Build & Test:**
```bash
cd examples/ffi/string_utils
adeshlang compile-aot string_utils.adesh string_utils.o -c --emit-header string_utils.h
gcc test_string.c string_utils.o -o test_string.exe
./test_string.exe
```

### 3. **Crypto Utilities** (`crypto_utils.adesh`)

Bitwise operations and mathematical algorithms.

**Functions:**
- `xor_values(a: i64, b: i64): i64` - Bitwise XOR
- `and_values(a: i64, b: i64): i64` - Bitwise AND
- `or_values(a: i64, b: i64): i64` - Bitwise OR
- `left_shift(value: i64, bits: i64): i64` - Bit shift left
- `right_shift(value: i64, bits: i64): i64` - Bit shift right
- `count_set_bits(n: i64): i64` - Count 1-bits in number
- `fibonacci(n: i64): i64` - nth Fibonacci number
- `gcd(a: i64, b: i64): i64` - Greatest common divisor

**Build & Test:**
```bash
cd examples/ffi/crypto_utils
adeshlang compile-aot crypto_utils.adesh crypto_utils.o -c --emit-header crypto_utils.h
gcc test_crypto.c crypto_utils.o -o test_crypto.exe
./test_crypto.exe
```

## How FFI Works

AdeshLang FFI works in three simple steps:

### 1. **Write AdeshLang Functions**

Mark functions you want to export with the `export` keyword:

```adesh
export fn multiply(a: i64, b: i64): i64 {
    return a * b;
}

export fn scale(x: f64): f64 {
    var result: f64;
    result = x * 2.5;
    return result;
}
```

**Requirements:**
- Must use `export` keyword
- Must have explicit parameter types (e.g., `a: i64`)
- Must have explicit return type (e.g., `: i64`)
- Only supports primitive types (no arrays/structs)

### 2. **Compile to Object File + Header**

The compiler generates two files:

```bash
adeshlang compile-aot library.adesh library.o -c --emit-header library.h
```

**Generated `library.h`:**
```c
#ifndef ADESH_EXPORTS_H
#define ADESH_EXPORTS_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exported Functions */
int64_t multiply(int64_t a, int64_t b);
double scale(double x);

#ifdef __cplusplus
}
#endif

#endif
```

**Generated `library.o`:**
- Machine code object file (platform/architecture specific)
- Contains compiled function implementations
- Ready to link with C/C++ code

### 3. **Use in C/C++ Code**

Include the header and link the object file:

```c
#include "library.h"
#include <stdio.h>

int main() {
    int64_t prod = multiply(6, 7);
    double scaled = scale(4.0);
    
    printf("Product: %ld\n", prod);      // Output: Product: 42
    printf("Scaled: %.1f\n", scaled);    // Output: Scaled: 10.0
    
    return 0;
}
```

**Compilation:**
```bash
gcc main.c library.o -o program.exe
./program.exe
```

## Step-by-Step Guide

### Creating Your First FFI Library

#### Step 1: Create AdeshLang File

Create `greeting.adesh`:
```adesh
export fn greet_count(times: i64): i64 {
    var count: i64;
    count = 0;
    while count < times {
        count = count + 1;
    }
    return count;
}

export fn double_value(x: i64): i64 {
    return x * 2;
}
```

#### Step 2: Compile to FFI

```bash
adeshlang compile-aot greeting.adesh greeting.o -c --emit-header greeting.h
```

This creates:
- `greeting.o` - Machine code
- `greeting.h` - C header file

#### Step 3: Create C Program

Create `main.c`:
```c
#include <stdio.h>
#include "greeting.h"

int main() {
    printf("Greeting count: %ld\n", greet_count(5));
    printf("Double 21: %ld\n", double_value(21));
    return 0;
}
```

#### Step 4: Compile & Link

```bash
gcc main.c greeting.o -o greeting.exe
./greeting.exe
```

**Output:**
```
Greeting count: 5
Double 21: 42
```

### Linking Multiple Libraries

You can compile multiple AdeshLang libraries and link them together:

```bash
# Compile each library
adeshlang compile-aot math.adesh math.o -c --emit-header math.h
adeshlang compile-aot strings.adesh strings.o -c --emit-header strings.h

# Link with C program
gcc app.c math.o strings.o -o app.exe
```

**C code:**
```c
#include "math.h"
#include "strings.h"

int main() {
    int64_t sum = add(10, 20);
    // use functions from both libraries
    return 0;
}
```

## Type Mappings

AdeshLang types automatically map to C/C++ standard types:

| AdeshLang | C Type | Bytes | Range |
|----------|--------|-------|-------|
| `i8` | `int8_t` | 1 | -128 to 127 |
| `i16` | `int16_t` | 2 | -32,768 to 32,767 |
| `i32` | `int32_t` | 4 | -2.1M to 2.1M |
| `i64` | `int64_t` | 8 | -9.2E18 to 9.2E18 |
| `u8` | `uint8_t` | 1 | 0 to 255 |
| `u16` | `uint16_t` | 2 | 0 to 65,535 |
| `u32` | `uint32_t` | 4 | 0 to 4.3B |
| `u64` | `uint64_t` | 8 | 0 to 18.4E18 |
| `f32` | `float` | 4 | IEEE-754 single |
| `f64` | `double` | 8 | IEEE-754 double |
| `bool` | `bool` | 1 | true/false (C99) |
| `ptr` | `void*` | 8 | Generic pointer |

## API Reference

### compile-aot Command

```bash
adeshlang compile-aot <file.adesh> <output.o> -c --emit-header <output.h>
```

**Options:**
- `-c` - Generate C-compatible object file (required)
- `--emit-header <file>` - Generate C header file

**Example:**
```bash
adeshlang compile-aot my_lib.adesh my_lib.o -c --emit-header my_lib.h
```

### Export Syntax

```adesh
export fn function_name(param1: type1, param2: type2): return_type {
    // implementation
    return value;
}
```

**Valid Type Annotations:**
- Primitive: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `f32`, `f64`, `bool`
- Pointer: `ptr`
- Example: `export fn foo(a: i64, b: f64): i64`

### Function Call from C

```c
#include "library.h"

// Calling exported AdeshLang function from C
int64_t result = my_function(arg1, arg2);
```

## Best Practices

### 1. **Function Design**

✅ **Good:**
```adesh
// Clear purpose, no side effects
export fn calculate_tax(amount: i64): i64 {
    return amount * 8 / 100;  // 8% tax
}
```

❌ **Avoid:**
```adesh
// Unclear semantics, mixed concerns
export fn process_data(x: i64): i64 {
    // performs multiple unrelated operations
    return x * 2 + 3;
}
```

### 2. **Type Safety**

✅ **Good:**
```adesh
export fn square(n: i64): i64 {
    return n * n;
}

export fn average(a: f64, b: f64): f64 {
    var sum: f64;
    sum = a + b;
    return sum / 2.0;
}
```

❌ **Avoid:**
```adesh
// Type confusion in FFI boundary
export fn ambiguous(x: i64): i64 {
    return x;  // unclear intent - is this float disguised as i64?
}
```

### 3. **Variable Declaration**

AdeshLang requires explicit type annotations and separate initialization:

```adesh
// Correct
var result: i64;
result = 0;
var i: i64;
i = 0;
while i < n {
    result = result + i;
    i = i + 1;
}
return result;
```

### 4. **Testing**

```c
// Comprehensive test coverage
#include <assert.h>
#include "library.h"

void test_basic_operations() {
    assert(add(5, 3) == 8);
    assert(multiply(4, 5) == 20);
    assert(factorial(5) == 120);
}

void test_boundary_cases() {
    assert(add(0, 0) == 0);
    assert(multiply(1, 100) == 100);
}

void test_floating_point() {
    double result = add_floats(1.5, 2.5);
    assert(result > 3.99 && result < 4.01);  // Account for float precision
}

int main() {
    test_basic_operations();
    test_boundary_cases();
    test_floating_point();
    printf("All tests passed!\n");
    return 0;
}
```

### 5. **Performance**

- Minimize FFI boundary crossings for tight loops
- Pre-compute values when possible
- Batch related operations

```adesh
// Good: Single FFI call for batch operation
export fn process_values(count: i64): i64 {
    var sum: i64;
    sum = 0;
    var i: i64;
    i = 0;
    while i < count {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}
```

## Troubleshooting

### "Export not recognized"

**Error:**
```
Parser error: Unknown keyword 'export'
```

**Solution:** Ensure `export` keyword is used before `fn`:
```adesh
export fn my_func(): i64 {  // ✓ Correct
    return 42;
}

fn my_func(): i64 {  // ✗ Missing 'export'
    return 42;
}
```

### Type mismatch in generated header

**Problem:** C code doesn't match generated header types

**Solution:** Verify parameter and return type annotations:
```adesh
// Bad: Missing parameter type
export fn add(a, b): i64 {  // ✗ No types!
    return a + b;
}

// Good: Explicit types
export fn add(a: i64, b: i64): i64 {  // ✓ Clear
    return a + b;
}
```

### "Object file linking failed"

**Error:**
```
undefined reference to `my_function'
```

**Solution:** 
1. Verify `-c` flag in compile-aot: `adeshlang compile-aot lib.adesh lib.o -c`
2. Include object file in gcc: `gcc main.c lib.o -o main.exe`

### Floating-point precision issues

**Problem:** Results slightly different from expected

**Solution:** Use epsilon comparisons in C:
```c
#include <math.h>

double result = add_floats(0.1, 0.2);
double expected = 0.3;
double epsilon = 1e-9;

if (fabs(result - expected) < epsilon) {
    printf("Test passed\n");
}
```

### "Invalid type" error during compilation

**Problem:**
```
AOT compilation error: Function verification failed for add_floats
```

**Solution:** Ensure variable types are declared before use:
```adesh
// Bad: Direct return of binary op
export fn add_floats(a: f64, b: f64): f64 {
    return a + b;  // May cause issues
}

// Good: Assign to typed variable first
export fn add_floats(a: f64, b: f64): f64 {
    var result: f64;
    result = a + b;
    return result;
}
```

## Advanced Topics

### Creating a Numeric Computation Library

Combine multiple functions for comprehensive mathematical operations:

```adesh
export fn is_prime(n: i64): i64 {
    if n < 2 {
        return 0;
    }
    var i: i64;
    i = 2;
    while i * i <= n {
        if n % i == 0 {
            return 0;
        }
        i = i + 1;
    }
    return 1;
}

export fn next_prime(n: i64): i64 {
    var candidate: i64;
    candidate = n + 1;
    while is_prime(candidate) == 0 {
        candidate = candidate + 1;
    }
    return candidate;
}
```

### Batch Processing Pattern

```adesh
export fn process_sequence(start: i64, end: i64): i64 {
    var sum: i64;
    sum = 0;
    var i: i64;
    i = start;
    while i <= end {
        sum = sum + i * i;  // Process each element
        i = i + 1;
    }
    return sum;  // Return single result
}
```

### Memory-Efficient Algorithms

```adesh
// Iterative instead of recursive to avoid stack overflow
export fn large_factorial(n: i64): i64 {
    var result: i64;
    result = 1;
    var i: i64;
    i = 2;
    while i <= n {
        result = result * i;
        i = i + 1;
    }
    return result;
}
```

## Building a Complete Project

Example project structure:

```
project/
├── adesh/
│   ├── math.adesh
│   ├── crypto.adesh
│   └── utils.adesh
├── src/
│   ├── main.c
│   ├── app.c
│   ├── app.h
│   └── Makefile
└── README.md
```

**Makefile:**
```makefile
ADESH = ../../target/debug/adeshlang
CC = gcc
CFLAGS = -Wall -O2

.PHONY: all clean test

all: app.exe

# Compile AdeshLang libraries
math.o: ../adesh/math.adesh
	$(ADESH) compile-aot ../adesh/math.adesh math.o -c --emit-header math.h

crypto.o: ../adesh/crypto.adesh
	$(ADESH) compile-aot ../adesh/crypto.adesh crypto.o -c --emit-header crypto.h

# Build application
app.exe: app.c main.c math.o crypto.o
	$(CC) $(CFLAGS) app.c main.c math.o crypto.o -o app.exe

test: app.exe
	./app.exe

clean:
	rm -f *.o *.exe *.h
```

## Summary

AdeshLang FFI makes it easy to:
- ✅ Export functions with explicit types
- ✅ Generate C-compatible headers automatically  
- ✅ Create high-performance native libraries
- ✅ Integrate with existing C/C++ projects
- ✅ Support multiple platforms

For more examples, see the included library examples in this directory.

---

**Version:** AdeshLang 0.2+  
**Last Updated:** December 2025  
**Status:** Production Ready
