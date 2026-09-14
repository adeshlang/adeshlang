# AdeshLang Memory Safety Examples

This directory contains comprehensive examples demonstrating AdeshLang's memory safety features, including pointer management, borrow rules, type layouts, and thread-aware tracking.

## Example Files

### 1. Pointer Management Examples

#### `pointer_basic.adesh` - Basic Pointer Operations
Demonstrates fundamental pointer allocation, access, and RAII cleanup.
- Allocation with `alloc()`
- Element-based indexing
- Automatic cleanup on scope exit
- Type-safe element sizing

**Run**: `adesh run examples/memory/pointer_basic.adesh`

#### `pointer_raii.adesh` - RAII with Control Flow
Shows automatic cleanup on all control flow exits.
- Return statements with cleanup
- Break statements with cleanup
- Continue statements with cleanup
- Nested scopes

**Run**: `adesh run examples/memory/pointer_raii.adesh`

#### `pointer_uaf_fail.adesh` - Use-After-Free Detection
Demonstrates runtime detection of use-after-free bugs.
- Allocation, free, then access (fails at runtime)
- Expected: "Use-after-free: pointer X was already freed"

**Run**: `adesh run examples/memory/pointer_uaf_fail.adesh` (expect error)

#### `pointer_bounds_fail.adesh` - Bounds Checking
Demonstrates runtime bounds checking on pointer accesses.
- Out-of-bounds access attempt
- Expected: "Typed pointer load out of bounds"

**Run**: `adesh run examples/memory/pointer_bounds_fail.adesh` (expect error)

#### `pointer_fn_args.adesh` - Function Parameter Passing
Shows passing pointers as function arguments without ownership transfer.
- Function accepts pointer parameter
- Caller retains ownership
- Caller responsible for cleanup

**Run**: `adesh run examples/memory/pointer_fn_args.adesh`

#### `pointer_return.adesh` - Ownership Transfer on Return
Demonstrates ownership transfer when returning pointers from functions.
- Function allocates and returns pointer
- Caller becomes owner
- Caller must free

**Run**: `adesh run examples/memory/pointer_return.adesh`

### 2. Borrow Rules Examples

#### `borrow_ok.adesh` - Valid Borrow Patterns
Demonstrates valid borrow patterns that compile and run.
- Shared references to immutable data
- Multiple shared references coexisting
- Mutable references with exclusive access
- Sequential borrowing of same pointer

**Run**: `adesh run examples/memory/borrow_ok.adesh`

#### `borrow_fail.adesh` - Invalid Borrow Patterns
Demonstrates invalid patterns that fail compile-time checks.
- Attempting to mutate through shared reference
- Mixing shared and mutable references
- Double mutable borrow attempts

**Run**: `adesh run examples/memory/borrow_fail.adesh` (expect compile error)

### 3. Type Layout Examples

#### `type_layout.adesh` - Type Size and Alignment
Demonstrates `sizeof()` and `alignof()` intrinsics.
- Primitive type sizes (u8, i32, f64, etc.)
- Pointer sizes (always 8 bytes on 64-bit)
- Tuple and struct layouts
- String type layout (24 bytes)
- Array type layouts

**Run**: `adesh run examples/memory/type_layout.adesh`

### 4. Ownership and Escape Analysis Examples

#### `ownership_transfer.adesh` - Ownership Semantics
Demonstrates ownership transfer and moves.
- Owning pointers (move-only)
- Implicit moves on assignment
- Explicit moves with `move` keyword
- Ownership in function calls

**Run**: `adesh run examples/memory/ownership_transfer.adesh`

#### `escape_analysis.adesh` - Escape Control
Shows controlled vs uncontrolled escapes.
- Pointers escaping to global scope (with warning)
- Pointers captured in closures
- Explicit `transfer` keyword for approved escapes
- Invalid escape attempts

**Run**: `adesh run examples/memory/escape_analysis.adesh`

### 5. Concurrency Examples

#### `thread_aware_tracking.adesh` - Thread-Aware Pointer Safety
Demonstrates thread ownership and cross-thread access control.
- Pointers allocated in one thread
- Read access from other threads (warn)
- Write access restrictions (error without synchronization)
- Safe synchronization patterns (mutex, channels)

**Run**: `adesh run examples/concurrency/thread_aware_tracking.adesh`

### 6. Embedded Mode Examples

#### `embedded_alloc_fail.adesh` - Embedded Mode Constraint
Shows compile-time rejection of heap allocation in embedded mode.
- Attempts to call `alloc()` in embedded build
- Expected: "Embedded mode forbids heap allocation via alloc()"

**Run**: `adesh build --embedded examples/memory/embedded_alloc_fail.adesh` (expect error)

## Quick Reference by Topic

### Use-After-Free Prevention
- `pointer_uaf_fail.adesh` - Runtime detection
- Tests: `tests/borrow/borrow_checker_tests.rs`

### Bounds Checking
- `pointer_bounds_fail.adesh` - Runtime validation
- `pointer_basic.adesh` - Valid access patterns

### RAII (Automatic Cleanup)
- `pointer_raii.adesh` - All control flow exits
- `pointer_basic.adesh` - Scope-based cleanup

### Borrow Rules
- `borrow_ok.adesh` - Valid patterns
- `borrow_fail.adesh` - Invalid patterns
- Documentation: `docs/borrow_rules.md`

### Type Safety
- `type_layout.adesh` - Size/alignment intrinsics
- Documentation: `docs/type_layout.md`
- Tests: `tests/types/type_layout_tests.rs`

### Thread Safety
- `thread_aware_tracking.adesh` - Thread-aware pointers
- Documentation: `docs/concurrency_safety.md`

## Testing

All examples can be tested with the standard AdeshLang test suite:

```bash
# Run all tests
cargo test

# Run only borrow checker tests
cargo test borrow_checker_tests

# Run only type layout tests
cargo test type_layout_tests

# Run a specific example
adesh run examples/memory/pointer_basic.adesh
```

## Key Concepts Illustrated

| Concept | Example | Expected Outcome |
|---------|---------|------------------|
| Basic allocation/free | `pointer_basic.adesh` | ✅ Runs successfully |
| RAII cleanup | `pointer_raii.adesh` | ✅ Automatic cleanup on exit |
| Use-after-free | `pointer_uaf_fail.adesh` | ❌ Runtime error |
| Bounds checking | `pointer_bounds_fail.adesh` | ❌ Runtime error |
| Shared references | `borrow_ok.adesh` | ✅ Runs successfully |
| Exclusive access | `borrow_ok.adesh` | ✅ Mutable ref blocks others |
| Type sizes | `type_layout.adesh` | ✅ Prints sizes correctly |
| Ownership transfer | `ownership_transfer.adesh` | ✅ Move semantics work |
| Thread tracking | `thread_aware_tracking.adesh` | ⚠️ Warns on cross-thread access |
| Embedded constraint | `embedded_alloc_fail.adesh` | ❌ Compile-time error |

## Safety Guarantees

All examples demonstrate:
- ✅ **No use-after-free**: Runtime state tracking detects freed pointer access
- ✅ **No double-free**: Freed pointers marked and rejected on second free
- ✅ **No buffer overflow**: Bounds checking on all pointer accesses
- ✅ **Deterministic cleanup**: RAII on all control flow exits
- ✅ **No memory leaks**: RAII ensures cleanup in all paths
- ✅ **Type-safe indexing**: Element-based addressing prevents type confusion

## For More Information

- **Pointer System**: See [docs/pointers.md](../docs/pointers.md)
- **Borrow Rules**: See [docs/borrow_rules.md](../docs/borrow_rules.md)
- **Type Layouts**: See [docs/type_layout.md](../docs/type_layout.md)
- **Concurrency**: See [docs/concurrency_safety.md](../docs/concurrency_safety.md)
- **Memory Model**: See [docs/memory_model.md](../docs/memory_model.md)
