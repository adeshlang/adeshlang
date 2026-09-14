# memory-safety-guide.md

> Consolidated from 7 documentation files on 2026-08-29.

---


---

## Source: MEMORY_SAFETY_GUIDE.md

# AdeshLang Complete Memory Safety Guide

## Table of Contents

1. [Introduction](#introduction)
2. [Ownership System](#ownership-system)
3. [Borrowing Rules](#borrowing-rules)
4. [Lifetimes](#lifetimes)
5. [Unsafe Code](#unsafe-code)
6. [Concurrency](#concurrency)
7. [Best Practices](#best-practices)

---

## Introduction

AdeshLang provides **compile-time memory safety** without garbage collection. All memory safety is validated during compilation, resulting in zero runtime overhead.

### Key Principles

1. **Every value has exactly one owner**
2. **Ownership can be transferred (moved)**
3. **Values can be borrowed (shared or exclusive)**
4. **Compiler enforces all rules at compile-time**

---

## Ownership System

### Single Owner Rule

Every value has exactly one owner at any time.

```adesh
let x = vec![1, 2, 3];  // x owns the vector
let y = x;               // Ownership transferred to y
// x is no longer valid
```

### Move Semantics

By default, assignment moves ownership:

```adesh
fn consume(data: Vec<int>) {
    // data is moved here
}

let v = vec![1, 2, 3];
consume(v);
// v is no longer accessible
```

### Copy Types

Primitive types are `Copy` and don't move:

```adesh
let x = 42;
let y = x;  // x is copied, both x and y valid
```

### Managed Pointers

**Box<T>** - Unique ownership:
```adesh
let b = Box::new(42);
let c = b;  // b moved to c
```

**Rc<T>** - Shared ownership (single-threaded):
```adesh
let r1 = Rc::new(vec![1, 2, 3]);
let r2 = r1.clone();  // Reference count increased
// Both r1 and r2 valid
```

**Arc<T>** - Shared ownership (thread-safe):
```adesh
let a1 = Arc::new(vec![1, 2, 3]);
let a2 = a1.clone();  // Atomic reference count
// Can send a2 to another thread
```

---

## Borrowing Rules

### Shared Borrows (&T)

Multiple shared borrows allowed:

```adesh
let v = vec![1, 2, 3];
let r1 = &v;
let r2 = &v;  // OK: multiple shared borrows
println(r1.len());
println(r2.len());
```

### Exclusive Borrows (&mut T)

Only one exclusive borrow OR multiple shared borrows:

```adesh
let mut v = vec![1, 2, 3];
let r = &mut v;
r.push(4);  // OK
// Cannot create other borrows while r exists
```

### Borrow Scope

Borrows end at their last use (Non-Lexical Lifetimes):

```adesh
let mut data = vec![1, 2, 3];
let r = &data;
println(r);  // r's last use

data.push(4);  // OK! r no longer active
```

### Two-Phase Borrows

Mutable borrows can be initialized gradually:

```adesh
let mut map = HashMap::new();
map.insert("key", vec![]);
map.get_mut("key").push(42);  // Two-phase borrow
```

---

## Lifetimes

### Lifetime Rules

1. References cannot outlive their referent
2. Compiler tracks lifetimes automatically
3. No explicit lifetime annotations needed (inferred)

### Common Patterns

**Return borrowed data:**
```adesh
fn first(data: &Vec<int>) -> &int {
    &data[0]  // Lifetime tied to data
}
```

**Struct with references:**
```adesh
class Container {
    data: &Vec<int>  // Lifetime inferred from usage
}
```

### Dangling Reference Prevention

```adesh
fn bad() -> &int {
    let x = 42;
    return &x;  // ❌ ERROR: returning reference to local
}
```

---

## Unsafe Code

### Unsafe Blocks

Raw pointer operations require `unsafe`:

```adesh
let x = 42;
unsafe {
    let ptr = &x as *const int;
    let value = *ptr;  // OK in unsafe block
}
```

### Provenance Tracking

Compiler tracks pointer origins:

```adesh
unsafe {
    let ptr = Box::into_raw(Box::new(42));
    let value = *ptr;  // Provenance: FromAllocation
    Box::from_raw(ptr);  // Proper cleanup
}
```

### Unsafe Rules

1. Raw pointers only in `unsafe {}` blocks
2. Cannot escape unsafe pointers to safe code
3. Caller responsible for safety invariants
4. Document safety requirements

---

## Concurrency

### Send and Sync Traits

**Send:** Can transfer between threads
```adesh
let data = Arc::new(vec![1, 2, 3]);
spawn(|| {
    use(data);  // OK: Arc<T> is Send
});
```

**Sync:** Can share references between threads
```adesh
let data = Arc::new(Mutex::new(vec![1, 2, 3]));
let r = &data;
spawn(|| {
    use(r);  // OK: Arc<Mutex<T>> is Sync
});
```

### Data Race Prevention

```adesh
let rc = Rc::new(vec![1, 2, 3]);
spawn(|| {
    use(rc);  // ❌ ERROR: Rc<T> is not Send
});
```

---

## Best Practices

### 1. Prefer Borrowing

```adesh
// Good: borrow when possible
fn process(data: &Vec<int>) { ... }

// Avoid: moving unless necessary
fn process(data: Vec<int>) { ... }
```

### 2. Use Non-Lexical Lifetimes

```adesh
// NLL allows this:
let mut v = vec![1, 2, 3];
let r = &v;
println(r);  // r ends here
v.push(4);   // OK!
```

### 3. Leverage Escape Analysis

```adesh
// Compiler optimizes to stack:
fn no_escape() {
    let b = Box::new(42);
    use(b);
}  // Stack allocated!
```

### 4. Document Unsafe

```adesh
/// SAFETY: ptr must be valid and aligned
unsafe fn deref(ptr: *const int) -> int {
    *ptr
}
```

### 5. Use Managed Pointers Appropriately

- **Box<T>**: Unique ownership, heap allocated
- **Rc<T>**: Shared ownership, single-threaded
- **Arc<T>**: Shared ownership, multi-threaded
- **RefCell<T>**: Interior mutability (runtime checks)

---

## Error Messages

AdeshLang provides clear error messages:

```
error[E0382]: use of moved value: `v`
  --> main.adesh:4:10
   |
2  | let v = vec![1, 2, 3];
   |     - move occurs here
3  | consume(v);
   |         - value moved here
4  | println(v);
   |         ^ value used after move
   |
help: consider borrowing instead:
   |
3  | consume(&v);
```

---

## Summary

AdeshLang's memory safety system:

- ✅ Zero runtime overhead
- ✅ No garbage collector
- ✅ No manual memory management
- ✅ Prevents segfaults
- ✅ Prevents data races
- ✅ Prevents memory leaks

**Compile once, run safely forever.**


---

## Source: borrow_rules.md

# AdeshLang Borrow Rules

## Overview

AdeshLang introduces compile-time borrow rules to enforce aliasing and ownership safety **without requiring lifetimes or explicit `mut` keyword**. These rules complement the runtime pointer safety system and prevent entire classes of bugs:

- **Use-after-free** (runtime + compile-time detection)
- **Double-free** (runtime detection + ownership tracking)
- **Data races** (thread-aware pointer tracking)
- **Uncontrolled aliasing** (borrow rules)

## Design Principles

- **All variables are mutable by default** - no `mut` keyword
- **Access modes are auto-inferred** by the compiler
- **No explicit lifetimes** - automatically tracked
- **C-like speed** with **Rust-like safety** and **Python ergonomics**

## Two Reference Modes (Auto-Inferred)

AdeshLang uses a single reference syntax `&T` with auto-inferred access mode:

### Shared References (Auto-Inferred for Read-Only)

- **Triggered when**: Compiler detects only read operations
- **Semantics**: Read-only access to data owned elsewhere
- **Copyability**: **Copyable** - multiple refs to same data is safe
- **Borrowing**: Many can coexist on same data

```adesh
let p = alloc<i32>();
*p = 42;
let r1 = &p;  // Borrow - compiler sees read-only usage → shared
let r2 = &p;  // Another shared ref - OK
print(*r1);   // Read access
```

### Exclusive References (Auto-Inferred for Mutation)

- **Triggered when**: Compiler detects mutation through the reference
- **Semantics**: Exclusive mutable access to data owned elsewhere
- **Copyability**: **Non-copyable** - only one exclusive ref exists at a time
- **Borrowing**: Only one can exist; cannot coexist with shared refs

```adesh
let p = alloc<i32>();
let mr = &p;      // Borrow - compiler sees mutation below → exclusive
*mr = 100;        // Mutate through ref (triggers exclusive mode)
let r = &p;       // Compile error: p already exclusively borrowed
```

## Borrow Rules (Compile-Time Enforced)

### Rule 1: Exclusive Access for Mutation

For any given `ptr<T>`, at any point in time:
- **Either** many shared refs (read-only usage)
- **Or** exactly one exclusive ref (mutation detected)
- **But never** both simultaneously

### Rule 2: References Don't Escape Without Transfer

An exclusive reference cannot outlive its scope unless explicitly transferred:

```adesh
fn create_ref() {
    let p = alloc<i32>();
    let mr = &p;          // Borrow - cannot escape this scope
    fn inner(mr: &i32) {
        *mr = 42;
    }
    inner(mr);            // OK: explicit pass
    global.store(mr);     // Error: cannot escape scope
}
```

### Rule 3: Shared References Are Read-Only

The compiler enforces that `ref<T>` can never be mutated:

```adesh
let p = alloc<i32>();
let r: ref<i32> = p;
*r = 42;  // Compile error: cannot mutate through ref<T>
```

### Rule 4: No Borrowing After Move

A `ptr<T>` cannot be borrowed after it's moved:

```adesh
let p = alloc<i32>();
let r: ref<i32> = p;
let q = p;           // Move p
let r2: ref<i32> = p;  // Error: p was moved
```

## Borrow State Machine

Each variable with a `ptr<T>` type tracks a borrow state:

```
┌──────────────┐
│ Unborrowed   │ ← Initial state
└──────┬───────┘
       │ borrow_shared()  ┌───────────────┐
       └────────────────→ │ Shared(n >= 1)│
                          └───────┬───────┘
                                  │ unborrow_shared()
                        or another borrow_shared() to increment
                        
┌──────────────┐ borrow_mut()  ┌────────────────┐
│ Unborrowed   │─────────────→  │ MutBorrowed    │
└──────────────┘               └────┬───────────┘
                                    │ unborrow_mut()
                                    → Unborrowed
```

**Invalid transitions**: 
- Cannot call `borrow_shared()` while `MutBorrowed`
- Cannot call `borrow_mut()` while `Shared(n > 0)` or `MutBorrowed`

## Scope-Based Borrow Release

Borrows are automatically released at scope boundaries:

```adesh
{
    let p = alloc<i32>();
    {
        let r: ref<i32> = p;   // Borrow starts
        println(r);
    }                           // Borrow ends here
    let mr: mutref<i32> = p;    // OK: no active borrows
}                               // Must free p before scope ends
free(p);
```

## Explicit `transfer` Keyword

To pass ownership explicitly or store pointers in globals/closures, use `transfer`:

```adesh
let p = alloc<i32>();
global.data = transfer p;  // Ownership transferred
// p is now invalid
```

## Function Signatures

Functions can specify borrow requirements:

```adesh
fn read_value(r: ref<i32>) {
    let val = *r;
    println(val);
}

fn modify_value(mr: mutref<i32>) {
    *mr = 42;
}

fn take_ownership(p: ptr<i32>) {
    *p = 100;
    free(p);
}

let p = alloc<i32>();
read_value(p);         // Implicit borrow for reading
modify_value(p);       // Implicit borrow for mutation
take_ownership(p);     // Ownership transfer
```

## Interaction with Runtime Checks

**Borrow rules are compile-time only** - they restrict what the compiler allows, but runtime checks remain active as the ultimate safety net:

- Compile-time rules **prevent** certain bad patterns from being written
- Runtime checks **detect** if a pattern escapes the compiler's checks
- All backends (Interpreter, JIT, VM, AOT, WASM) enforce runtime checks uniformly

```adesh
// This won't compile:
let p = alloc<i32>();
let r1: ref<i32> = p;
let mr: mutref<i32> = p;  // Compile error: already borrowed

// If this somehow got past the compiler, runtime would error:
unsafe_raw_mutate(p);  // Would error at runtime: unsynchronized write
```

## Error Messages

Borrow rule violations produce clear, deterministic errors:

```
Error: Cannot create mutable reference to 'p': already shared borrowed
  → Expected p to be unborrowed, found Shared(2)
  → Release borrows before creating mutref

Error: Mutable reference 'mr' cannot escape scope
  → Attempted to pass mutref to global storage
  → Use explicit transfer keyword: transfer mr
```

## Limitations and Future Work

- **No lifetime parameters**: Borrows are scope-based only
- **No region tracking**: All allocations are in the global heap
- **No higher-ranked traits**: No advanced type system features
- **Conservative for function calls**: Cross-function borrowing uses explicit passing

These limitations keep the compiler simple while maintaining safety through conservative rules.


---

## Source: memory_model.md

# AdeshLang Memory Model (Final)

AdeshLang uses a deterministic, GC-free memory model built on ownership, borrowing, explicit ARC, raw pointers, and region-based allocation. There are **no hidden heap allocations**, **no implicit reference counting**, and **no tracing GC**.

---

## Layered Storage

| Layer | Mechanism |
| ------ | ---------- |
| Primitives | Stack |
| Structs | Stack / inline |
| Small strings | SSO |
| Small arrays | SAO |
| Temporaries | Arena |
| Owned heap objects | Unique ownership |
| Shared objects | ARC (`Rc` / `Arc`) |
| Cycles | `Weak` references |
| Regions | Arena bulk-free |
| **Raw pointers** | **`unsafe` + RAII** |
| Embedded | Stack + Arena only |

---

## 1. Ownership System

Ownership is mandatory and tracked in HIR.

- Every value has exactly one owner.
- Assignment moves ownership; moved values become invalid.
- No implicit copies; drops run at end-of-scope in reverse declaration order.

```adesh
let a = Obj();
let b = a;        // move
print(a);         // compile error: use after move
```

Compiler obligations:
- Track ownership/moves in HIR.
- Emit errors for use-after-move, double free, and invalid ownership access.

---

## 2. Borrowing (Auto-Inferred Access Modes)

Borrowing is enforced at compile time; debug-only runtime checks are available.

- **All variables are mutable by default** - no `mut` keyword needed
- **Access modes are auto-inferred** by the borrow checker based on usage:
  - Read-only usage → shared borrow (multiple allowed)
  - Mutation detected → exclusive borrow (only one allowed)
- Borrows are scoped; borrowed values cannot be moved.
- User code never writes lifetime annotations or explicit `&mut`.

```adesh
fn read(x: &Obj) { print(x); }              // Shared borrow (read-only detected)
fn write(x: &Obj) { x.update(); }           // Exclusive borrow (mutation detected)
```

**Auto-Inference Rules:**
1. If a reference is only read from → shared borrow
2. If a reference is written to or mutated → exclusive borrow
3. Compiler infers at call site and function definition

Release builds incur zero runtime overhead; debug builds add lightweight counters.

---

## 3. Raw Pointers (`unsafe`)

AdeshLang provides **explicit, deterministic, memory-safe raw pointers** within `unsafe` blocks.

- **Typed pointers:** `*u8`, `*i32`, `*f64`, etc.
- **Element-based indexing:** `ptr[0]`, `ptr[1]` (not byte offsets)
- **State tracking:** Detects use-after-free, double-free, invalid pointers
- **Bounds checking:** All accesses validated at runtime
- **RAII cleanup:** Automatic `free()` on scope exit, return, break, continue
- **No GC:** Deterministic deallocation only

```adesh
unsafe {
    let p: *i32 = alloc(16);  // 16 bytes = 4 i32 elements
    p[0] = 100;
    p[1] = 200;
    let val = p[0];
    // free(p) automatically inserted by RAII
}
```

### Safety Guarantees
- ✅ Use-after-free detection
- ✅ Double-free prevention
- ✅ Bounds checking
- ✅ Invalid pointer detection
- ✅ Debug mode memory poisoning
- ❌ NO undefined behavior

See [docs/pointers.md](pointers.md) for complete pointer documentation.

---

## 4. Explicit ARC

ARC is opt-in and explicit via `share`. No ARC is performed implicitly.

```adesh
let a = Obj();
let s = share a;   // starts ARC
```

- Modes: `Rc<T>` (single-thread) and `Arc<T>` (multi-thread) selected by CLI flag:
  - `adesh run main.adesh --single-thread` (default)
  - `adesh run main.adesh --multi-thread`
- Cycles must use `weak`.
- ARC is forbidden inside regions and in embedded builds.

Weak references:

```adesh
weak let parent;
```

---

## 4. Regions / Arenas

Regions provide arena-backed allocation with bulk free.

```adesh
region Temp {
    let a = Obj();
    let b = Obj();
} // all region memory freed here
```

- No references may escape the region; compile-time enforced.
- No ARC inside regions.
- Zero per-object deallocation cost.

---

## 5. Embedded Mode

`adesh build --embedded` switches the runtime to stack + arena only.

- Heap disabled; ARC and `weak` disabled.
- Deterministic memory suitable for HAL/FFI.
- Compiler errors on any heap allocation, ARC, or weak reference when embedded flag is active.

---

## 6. Unsafe Mode

Unsafe blocks opt out of borrow checking and ARC.

```adesh
unsafe {
    let p = alloc<u8>(128);
    free(p);
}
```

Use only for drivers, allocators, and runtime internals.

---

## 7. Debug vs Release

| Build | Behavior |
| ----- | -------- |
| Debug | Ownership state tracking, borrow counters, memory poisoning (`0xDEADBEEF`), double-free and use-after-free detection, optional cycle warnings |
| Release | No metadata, no borrow tracking, no poisoning; only moves, ARC inc/dec, and drops run. HIR is trusted. |

---

## 8. Allocation Strategies

| Use Case | Allocator |
| -------- | --------- |
| AST / HIR | Bump |
| Temporaries | Arena |
| Small objects (≤2KB) | Slab |
| Long-lived | Heap |
| Regions | Arena |
| Embedded | Stack + Arena |

Growth strategies, allocation stats, and thread-local arenas are required.

---

## 9. Escape Analysis

During HIR → MIR lowering, the compiler allocates on stack/arena when values do not escape, are not shared, and are not returned. Escaping values are heap allocated.

---

## 10. Deterministic Drop

- Reverse declaration order.
- End-of-scope destruction.
- Region bulk free.
- ARC frees at refcount zero.
- No delayed finalizers.

---

## 11. Compiler Tasks

HIR must track ownership, borrow state, moves, region boundaries, and escape analysis. Codegen must emit ARC inc/dec, drop calls, region cleanup, and errors for memory violations.

---

## 12. Small Value Optimizations

- **SSO**: Small strings stored inline (no heap) up to the configured threshold.
- **SAO**: Small arrays stored inline for cache locality.

---

## Final Design Statement

> AdeshLang uses deterministic, GC-free memory management based on ownership, borrowing, explicit ARC, and region-based allocation, delivering predictable performance and strong safety guarantees across interpreter, JIT, AOT, WASM, and embedded targets.

---

*Last Updated: December 2025*


---

## Source: memory_allocator.md

# Memory Allocator

> **Dynamic Memory Allocator** — configurable allocation strategies for AdeshLang runtime

---

## Overview

AdeshLang provides a flexible memory allocation system with three modes:

1. **Static Mode**: Fixed-size heap with strict bounds checking
2. **Dynamic Mode**: Auto-expanding heap with configurable growth strategies
3. **Hybrid Mode**: Arena pools for short-lived allocations with heap fallback

This document covers configuration, usage, error handling, and integration with the runtime.

---

## Allocation Modes

### Static Mode

Fixed-size heap that returns OOM error when exceeded. Best for:
- Embedded systems with fixed memory budgets
- Testing memory limits
- Predictable memory usage

```adesh
// Configure static heap of 1MB
// Exceeding this limit triggers OOM error
@allocator(mode="static", heap_size=1048576)
```

### Dynamic Mode

Auto-expanding heap with configurable growth strategies:

| Strategy | Description |
|----------|-------------|
| `doubling` | Double capacity on each expansion (default) |
| `linear(N)` | Add N bytes on each expansion |
| `slab` | Fixed-size class allocation |

```adesh
// Configure dynamic heap with doubling growth
@allocator(mode="dynamic", initial=1048576, max=1073741824, growth="doubling")
```

### Hybrid Mode

Arena pools for short-lived allocations with heap fallback:

```adesh
// Configure hybrid with 4 arenas of 64KB each
@allocator(mode="hybrid", arena_size=65536, arena_count=4)
```

---

## Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `mode` | string | `"dynamic"` | Allocation mode: static, dynamic, hybrid |
| `initial_heap_size` | int | 1MB | Initial heap size in bytes |
| `max_heap_size` | int | 1GB | Maximum heap size (for static mode) |
| `growth_strategy` | string | `"doubling"` | Growth strategy for dynamic mode |
| `arena_pool_size` | int | 64KB | Size of each arena (hybrid mode) |
| `arena_count` | int | 4 | Number of arenas in pool |
| `debug_poison` | bool | debug only | Poison freed memory |
| `enable_profiling` | bool | false | Enable memory profiling |

---

## Memory Report

Use `memoryReport()` to inspect allocator statistics:

```adesh
let report = memoryReport();
print(report);
// Output:
// Memory Allocator Report
// ========================
// Mode: dynamic
// Growth Strategy: doubling
//
// AllocatorStats {
//   allocations: 1542
//   deallocations: 1200
//   current_bytes: 45.2 KB
//   peak_bytes: 128.5 KB
//   heap_expansions: 3
//   oom_events: 0
//   arena_allocations: 0
//   arena_resets: 0
//   fragmentation: 12%
// }
//
// Heap Capacity: 512.0 KB / 1.0 GB
```

---

## Error Handling

Out-of-memory conditions return errors through the unified error model:

```adesh
try {
    let huge = Array(1000000000);  // May trigger OOM
} catch (e) {
    if (e.kind == "RuntimeError" && e.message.contains("Out of memory")) {
        print("OOM: Consider using larger heap or dynamic mode");
    }
}
```

### Error Types

| Error | Description |
|-------|-------------|
| `Out of memory` | Requested allocation exceeds available heap |
| `Allocation size overflow` | Requested size exceeds maximum |

---

## Slab Allocation

Small objects use slab allocation for O(1) allocation and reduced fragmentation:

| Size Class | Object Size |
|------------|-------------|
| 0 | 16 bytes |
| 1 | 32 bytes |
| 2 | 64 bytes |
| 3 | 128 bytes |
| 4 | 256 bytes |
| 5 | 512 bytes |
| 6 | 1024 bytes |
| 7 | 2048 bytes |

Objects larger than 2048 bytes use direct heap allocation.

---

## Arena Allocation (Hybrid Mode)

Arenas provide fast bump allocation for short-lived objects:

1. Objects are allocated from a contiguous arena
2. Individual deallocation is not possible
3. Entire arena is reset at once
4. Best for function-scoped temporaries

```adesh
// Arena automatically resets at end of function scope
fn processData(data) {
    // These allocations use arena
    let temp1 = transform(data);
    let temp2 = filter(temp1);
    return summarize(temp2);
    // Arena reset here
}
```

---

## Safety Requirements

The allocator enforces these safety guarantees:

1. **No buffer overruns**: All allocations are bounds-checked
2. **Size overflow protection**: Large sizes are rejected
3. **Safe OOM handling**: Errors returned, never crash
4. **Debug poisoning**: Freed memory is poisoned in debug builds

---

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|------------|-------|
| Small alloc | O(1) | Slab allocation |
| Large alloc | O(1) amortized | With doubling growth |
| Dealloc | O(1) | Direct or slab return |
| Realloc (grow) | O(n) | Copy required |
| Arena reset | O(1) | Bulk operation |

---

## Thread Safety

The allocator is thread-safe:

- All statistics use atomic operations
- Slab maps use mutex locks
- Arena pools are protected by locks

For maximum performance in single-threaded code, consider using thread-local arenas.

---

## Examples

### Static Mode - Fixed Heap

```adesh
// examples/memory_allocator/static_mode.adesh
@allocator(mode="static", heap_size=1024)

let arr1 = [1, 2, 3, 4];  // OK
let arr2 = Array(1000);    // OOM Error!
```

### Dynamic Growth

```adesh
// examples/memory_allocator/dynamic_growth.adesh
@allocator(mode="dynamic", initial=256, growth="doubling")

for (let i = 0; i < 100; i++) {
    let data = Array(100);  // Heap grows as needed
}

print(memoryReport());
```

### Hybrid Strategy

```adesh
// examples/memory_allocator/hybrid_strategy.adesh
@allocator(mode="hybrid", arena_size=4096, arena_count=2)

fn processItem(item) {
    // Uses arena for temporaries
    let transformed = transform(item);
    let filtered = filter(transformed);
    return result(filtered);
}

for (let item in items) {
    processItem(item);  // Arena resets each iteration
}
```

---

## Stress Testing

```adesh
// examples/memory_allocator/stress_test.adesh
@allocator(mode="dynamic", enable_profiling=true)

// Allocate many short-lived objects
for (let i = 0; i < 10000; i++) {
    let obj = { id: i, data: Array(10) };
    // obj goes out of scope
}

let stats = memoryReport();
print("Peak memory:", stats.peak_bytes);
print("Fragmentation:", stats.fragmentation_ratio, "%");
```

---

## Integration with Runtime

The allocator integrates with:

1. **Interpreter**: Uses slab/arena for Value storage
2. **VM**: Bytecode execution uses bump allocator
3. **JIT**: Fast paths for small objects
4. **GC**: Coordinates with garbage collector

---

## Migration Notes

- Default behavior is unchanged (dynamic mode)
- Static mode is opt-in via configuration
- Arena mode requires explicit reset or scope exit
- Profiling must be explicitly enabled

---

## See Also

- [Memory Model](memory_model.md) - Smart pointers and ownership
- [Error Model](error_model.md) - Unified error handling
- [VM Design](vm_design.md) - Virtual machine architecture


---

## Source: memory_gaps.md

# AdeshLang Memory & Safety — Implementation Status

**Last Updated**: August 23, 2026  
**Status**: ✅ 100% Memory Safety — All 12 gaps fixed

---

## ✅ August 2026: All 12 Memory Safety Gaps Fixed

All gaps identified in the comprehensive scan have been resolved. Build is clean
(zero errors, zero warnings). All safety tests pass.

### Fixed Gaps Summary

| # | Severity | Gap | Status |
|---|----------|-----|--------|
| 1 | LOW | Deref not enforced outside unsafe | ✅ Fixed |
| 2 | HIGH | Send/Sync violations were warnings | ✅ Fixed (now errors) |
| 3 | MEDIUM | Drop insertion missing ForIn/TryCatch | ✅ Fixed |
| 4 | HIGH | RAII parent-scope pointer bug | ✅ Fixed |
| 5 | HIGH | While-loop borrow analysis missing | ✅ Fixed |
| 6 | HIGH | Escape analysis was stubs | ✅ Fixed |
| 7 | HIGH | Lifetime constraint solving incomplete | ✅ Fixed |
| 8 | MEDIUM | Move semantics checker was stub | ✅ Fixed |
| 9 | MEDIUM | Unified pass phases not wired | ✅ Fixed |
| 10 | CRITICAL | Interprocedural analysis incomplete | ✅ Fixed |
| 11 | LOW | (Same as #1) | ✅ Fixed |
| 12 | — | VIR validation was no-op | ✅ Fixed |

### VIR Pipeline Fixes

- Block labels bug fixed (all jump targets were 0)
- Missing instructions added (ConstString, LoadLocal, StoreLocal, aggregates)
- MIR→VIR terminator catch-all fixed (Call/Drop were silently dropped)
- MIR borrow analysis implemented (was stub)
- Interpreter engine intrinsics implemented (Sin, Cos, Tan, Log, Exp, Pow)

---

## ✅ COMPLETED: Compile-Time Borrow Checking (Phase 4)

### Implemented Features ✅

* Borrow rules **enforced in HIR** via `borrow_check.rs` (470 lines)
* Auto-inference model (NO `&mut` syntax - compiler detects mutation)
* Compile-time rejection of:
  * ✅ free while borrowed
  * ✅ move while borrowed
  * ✅ conflicting borrow types (shared + exclusive)
* Function-scoped borrow tracking
* 7 validation methods implemented
* Cross-backend support (Interpreter, JIT, AOT, WASM, VM)
* 6/6 borrow tests passing

### Implementation Details

**File**: `src/parsing/borrow_check.rs`

**Key Methods**:
- `check_module()` - Module-level analysis
- `check_function()` - Function-scoped validation
- `is_borrowed()` - Query borrow state
- `is_moved()` - Query move state
- `validate_assignment()` - Assignment safety
- `validate_call_args()` - Function call validation
- `validate_free()` - Free operation safety

**Status**: ✅ **PRODUCTION READY**

---

## 🔄 PHASE 5: Advanced Borrow Safety (Planned - Jan 2026)

## 1️⃣ CFG-Based Borrow Propagation (High Priority)

### Missing Rules

* Borrow state **not propagated through CFG joins**
* No borrow state merge logic at control-flow joins
* Conflicting borrow states across branches not detected

**Example:**
```adesh
let x = Obj();
if condition {
    borrow_mut(&x);  // Branch 1: exclusive
} else {
    borrow(&x);      // Branch 2: shared
}
// What state does x have here? Currently undefined
```

### Planned Techniques

* HIR CFG construction
* Borrow state merge at join points
* Conflict detection across branches
* Path-sensitive analysis

**Estimated Effort**: 2-3 days  
**Priority**: High

---

## 2️⃣ Function Boundary Validation (High Priority)

### Missing Rules

* Borrow rules **not enforced in HIR/LIR yet**
* Borrow state **not propagated through CFG joins**
* No compile-time rejection of:

  * free while borrowed
  * move while borrowed
* Borrow rules **not validated at function boundaries**
* No borrow validation across function calls
* No borrow checking for method receivers (`self`)

## 2️⃣ Function Boundary Validation (High Priority)

### Missing Rules

* No lifetime tracking across function boundaries (interprocedural analysis)
* No validation of returned `ref<T>` / `mutref<T>`
* No restriction on returning references to local data
* No lifetime bounds on function signatures

**Example:**
```adesh
fn store_ref(r: &i32) {
    global_ref = r;  // ❌ Should fail: storing ref beyond lifetime
}

fn return_local() -> &i32 {
    let x = 42;
    return &x;  // ❌ Should fail: returning reference to local
}
```

### Planned Techniques

* Function-level ownership contracts
* Reference escape validation on returns
* Call-site ownership checking
* Interprocedural borrow analysis

**Estimated Effort**: 3-4 days  
**Priority**: High

---

## 3️⃣ Method Receiver Validation (High Priority)

### Missing Rules

* Method `self` parameter not validated like function arguments
* No borrow checking for method receivers
* Conflicts not detected: `obj.write(); obj.read();`

**Example:**
```adesh
struct Obj { value: i32 }
impl Obj {
    fn read(&self) { print(self.value); }      // Shared
    fn write(&mut self) { self.value = 10; }   // Exclusive (inferred)
}

let obj = Obj { value: 5 };
obj.write();
obj.read();  // Should validate borrow ordering
```

### Planned Techniques

* Treat `self` parameter like function arguments
* Validate receiver borrows in method calls
* Detect conflicts in method chains

**Estimated Effort**: 1-2 days  
**Priority**: High

---

## 4️⃣ Leak Detection (All Code Paths) (Medium Priority)

### Current Status

RAII cleanup implemented but not verified on all paths

**Issue**: Memory leaks possible on early returns/breaks

**Example:**
```adesh
unsafe {
    let p: *i32 = alloc(16);
    if error_condition {
        return;  // ❌ Memory leak! p not freed
    }
    free(p);  // Only freed on normal path
}
```

### Planned Techniques

* CFG-based must-free analysis
* Ownership state tracking per basic block
* Leak diagnostics with path explanation
* Verify cleanup on all exit paths (return, break, continue, panic)

**Estimated Effort**: 2-3 days  
**Priority**: Medium

---

## 5️⃣ Concurrency & Data-Race Safety (Medium Priority)

### Missing Rules

* No compile-time aliasing rules for multithreaded code
* No prevention of sharing `ptr<T>` across threads
* No enforcement that shared data uses synchronization

**Example:**
```adesh
let x = Obj();
spawn(|| {
    x.update();  // ❌ Should require Mutex or Arc
});
x.read();        // Data race!
```

### Planned Techniques

* `Send` and `Sync` trait equivalents
* Thread-safe pointer wrappers (`Mutex<T>`, `Arc<T>`)
* Compile-time check for thread boundaries
* Happens-before reasoning

**Estimated Effort**: 4-5 days  
**Priority**: Medium

---

## 🔄 PHASE 6: Ergonomics & Optimization (Planned - Feb 2026)

## 6️⃣ Non-Lexical Lifetimes (NLL) (Low Priority)

### Enhancement Goal

Shorten borrow lifetimes for better ergonomics

**Example:**
```adesh
let x = Obj();
let r = &x;
print(r);      // r last used here
// Currently: r lives until end of scope
// Optimization: r could end here, allowing:
x.update();    // ✅ Should work (r no longer used)
```

### Planned Techniques

* Track last usage of borrows
* End borrows at last use (not scope end)
* Similar to Rust's NLL

**Estimated Effort**: 5-7 days  
**Priority**: Low (ergonomics improvement)

---

## 7️⃣ Improved Error Messages (Low Priority)

### Enhancement Goal

Add visual borrow flow diagrams

**Example:**
```
error: cannot borrow `x` as mutable while borrowed as immutable
  --> example.adesh:5:10
   |
 3 | let r1 = &x;      // immutable borrow starts
   |          -- first borrow occurs here
 5 | let r3 = &mut x;  // ❌ cannot borrow mutably
   |          ^^^^^^ mutable borrow here
 6 | print(r1);
   |       -- first borrow still in use
```

**Estimated Effort**: 1-2 days  
**Priority**: Low (developer experience)

---

## 8️⃣ Escape Analysis Optimization (Medium Priority)

### Enhancement Goal

Automatically promote heap → stack allocations

**Current:**
```adesh
let x = box Obj();  // Explicit heap
```

**Optimized:**
```adesh
let x = Obj();  // Compiler detects: never escapes → stack
```

### Planned Techniques

* Enhance `src/parsing/escape_analysis.rs`
* Add stack-promotion pass in LIR lowering
* Reduce heap allocations automatically

**Estimated Effort**: 1-2 weeks  
**Priority**: Medium (performance win)

---

## Summary of Phases

| Phase | Status | Focus | Timeline |
|-------|--------|-------|----------|
| Phase 4 | ✅ Complete | Auto-inferred borrow checking | Dec 2025 |
| Phase 5 | 🔄 Planned | CFG propagation, function boundaries | Jan 2026 |
| Phase 6 | 📋 Planned | NLL, ergonomics, optimization | Feb 2026 |

---

## References

**Documentation:**
- [MEMORY_SAFETY_STATUS.md](../MEMORY_SAFETY_STATUS.md) - Complete status
- [memory_model.md](memory_model.md) - Memory model overview
- [borrow_rules.md](borrow_rules.md) - Borrowing rules

**Implementation:**
- `src/parsing/borrow_check.rs` (470 lines) - Main checker
- `src/parsing/lifetime_tracking.rs` (345 lines) - Lifetime validation
- `src/backends/lir_lower.rs` (lines 2370-2376) - LIR lowering
- `src/backends/builtins.rs` (lines 998-1050) - Runtime builtins

**Last Updated**: December 30, 2025

### Missing Features

* No arena allocators
* No region-based allocation
* No stack allocation (`alloca`)
* No per-thread heaps
* No scoped allocators

### Missing Techniques

* Arena lifetime tracking
* Region drop semantics
* Fast bulk deallocation

---

## 7️⃣ Performance Optimization (Not Implemented)

### Missing Rules

* No runtime check elision in release builds
* No check tier selection via compiler flags
* No backend-specific fast paths

### Missing Techniques

* Debug / release-safe / release-fast modes
* Conditional compilation of checks
* Pointer table sharding
* Lock-free pointer table

---

## 8️⃣ Backend Coverage Gaps

### Missing Verification

* AOT backend not fully validated with `unsafe_heap`
* WASM backend not fully wired to pointer table
* Thread ownership semantics undefined for WASM
* Embedded WASM heap enforcement incomplete

### Missing Techniques

* Unified backend conformance tests
* Backend-specific memory adapters
* WASM linear memory validation

---

## 9️⃣ Type System Gaps (Beyond Size Fidelity)

### Missing Rules

* No zero-sized type optimization
* No niche optimization (null pointer optimization)
* No `repr(C)` / `repr(packed)` controls
* No generic type layout caching

### Missing Techniques

* Layout caching
* ABI-specific layout modes
* Generic monomorph layout specialization

---

## 🔟 Diagnostics & Tooling (Not Implemented)

### Missing Features

* No ownership graph visualization
* No borrow trace diagnostics
* No `--trace-ownership` runtime flag
* No memory event timeline

### Missing Techniques

* Ownership debug logs
* Allocation/free tracing
* Pointer lifetime visualization

---

## 1️⃣1️⃣ Formal Guarantees (Not Implemented)

### Missing Artifacts

* No formal memory model specification
* No written concurrency model
* No proof-style invariants documentation

### Missing Techniques

* Memory model spec (C11-style, simplified)
* Happens-before rules
* Compiler/runtime invariant definitions

---

## 🎯 Minimal "Done" Definition (Still Pending)

AdeshLang memory system is **not complete** until:

* Borrow rules enforced at compile time
* All ownership escapes are explicit
* All leaks are compile-time errors
* Thread-unsafe writes are impossible
* AOT + WASM fully validated
* Performance tiers implemented


---

## Source: ARC_DESIGN.md

# AdeshLang ARC Design

This document is the authoritative design reference for the **interpreter canonical**
language-level ARC model (`share`, `strong`, `weak`). Implementation:
`src/memory/arc/shared_object.rs`.

This path is **not** the same as legacy `ArcManager` handles, JIT ARC stubs, or
`NanValue` / `ObjectRegistry`. Those systems remain separate until unified in follow-up work.

## Language concepts

| Keyword | Role |
|---------|------|
| `share` | Creates the ARC-managed object with initial `strong_count = 1` |
| `strong` | Adds another strong owner (increments `strong_count`) |
| `weak` | Adds a non-owning observer (increments `weak_count`, not `strong_count`) |

### Weak reassignment

`weak b = a` where `a` is already a weak binding **clones** the weak reference:
`weak_count` increases by one and both bindings remain valid. Variable reads clone
`Value::Weak`, which clones the underlying `WeakRef`.

## Lifecycle

```text
NEW
  |
  v
LIVE (strong_count >= 1)     payload alive, control block alive
  |
  | last strong release
  v
DYING (strong_count = 0, weak_count >= 0)   payload destroyed once, control block alive
  |
  | last weak release
  v
FREED                        control block deallocated
```

Control-block state is tracked with `control_block_alive` and `payload_destroyed` atomics
inside `SharedObject`. Use-after-free of the control block panics via `ensure_control_block`.

## Invariants

1. Weak references never keep the payload alive.
2. `weak.upgrade()` uses an atomic CAS loop on `strong_count`; it never resurrects a destroyed object.
3. Payload destruction happens exactly once when the last strong reference is released.
4. The control block outlives the payload while any weak reference exists.
5. `strong_count()` / `weak_count()` report live atomic counts and return `Value::U64` to the language.
6. Reference count overflow is rejected before an invalid count is committed (CAS loop, no post-increment check).

## `weak.upgrade()` algorithm

```text
load strong_count (Acquire)
if strong_count == 0 → return null
CAS(strong_count, strong_count + 1) with Acquire success ordering
  success → return StrongRef
  failure → retry with updated count
```

There is no window where the payload is destroyed and then resurrected: destruction only
occurs after `fetch_sub` observes the last strong release, and upgrade cannot CAS from zero.

## Count semantics

`strong_count()` equals the number of live `StrongRef` owners (including the implicit owner
created by `share`). Each `strong` binding adds one owner. Temporary interpreter clones used
for method dispatch must not leak into the visible count — ARC method calls (`strong_count`,
`weak_count`, `is_alive`, `upgrade`) use a fast path that avoids redundant `Share` clones in:

- `interpreter_core.rs` (eval call + builtin method dispatch)
- `exec/expression_eval/calls.rs`
- `exec/expression_eval/builtin_methods.rs`
- `arc_dispatch.rs` (borrow variable receivers from the environment)

**Function calls:** passing a `share` variable to a function clones one strong reference for
the callee parameter (legitimate language ownership). Parameter binding **moves** the argument
value out of the call vector instead of cloning it again, so the count is caller binding(s)
plus parameter binding(s), not inflated by a duplicate call-vector slot.

**Upgrade temporaries:** `weak.upgrade()` returns a new strong owner. A `let maybe = w.upgrade()`
binding holds that owner until `maybe` goes out of scope. Assigning again with `strong live = maybe`
clones another strong reference (two owners: `maybe` plus `live`). Prefer scoped `let` bindings or
release the upgrade result before counting again.

Counts are exposed as unsigned 64-bit integers (`Value::U64`), not `f64`, so large refcounts
remain exact within `u64`.

## Ownership vs synchronization

**ARC ownership is not synchronization.**

Atomic reference counts protect the control block and payload lifetime. They do **not** make
concurrent mutation of the shared payload safe.

```adesh
share x = { field: 0 }
strong a = x
strong b = x
a.field = 1   // not safe across threads without external synchronization
b.field = 2
```

The runtime stores payloads as plain `Value` inside `SharedObject`. The interpreter assumes
single-threaded execution for payload mutation. Multi-threaded programs must use explicit
synchronization (`mutex`, `rwlock`, atomics) for shared mutable state.

The compiler/runtime must not imply that `share` alone permits unsynchronized concurrent writes.

## Send / Sync

- `StrongRef` and `WeakRef` implement `Send` (ownership may move across threads).
- They intentionally do **not** implement `Sync`. Refcount atomics do not synchronize payload
  mutation; concurrent `&StrongRef` / `&WeakRef` access is unsupported until payload locking exists.

## Concurrency (reference counting only)

- Reference count updates use atomics with `Acquire`/`Release` fences at destruction boundaries.
- `upgrade()` uses `compare_exchange_weak` with `Acquire` on success.
- Payload mutation is not synchronized; see **Ownership vs synchronization** above.
- Stress coverage: unit tests include adversarial upgrade vs drop, concurrent clone/drop, and
  randomized multi-threaded upgrade/clone/drop scenarios.

## Related systems (separate from this document)

- **NanValue / ObjectRegistry** (`src/runtime/nanvalue.rs`): separate 8-byte runtime representation
  for VM values; not the same as language `share`/`strong`/`weak`.
- **ArcManager** (`src/memory/arc/mod.rs`): legacy handle-based ARC for `U64` handles; used by some
  bridge paths.
- **JIT backends** (`src/backends/jit/*`): separate ARC stubs for compiled code; not the interpreter
  `SharedObject` path.
- **value_optimized.rs**: duplicate StrongRef/WeakRef types for optimized value paths; not consolidated here.

## Canonical construction

All interpreter `share` / `strong` / `weak` / upgrade / count operations must route through
`memory::arc::shared_object` (`allocate_share`, `create_weak_from_strong`, `invoke_arc_method`).
Do not construct `SharedObject` or mutate refcounts directly in interpreter code.

## Examples

Executable semantics are demonstrated in `examples/arc/` and validated by `tests/arc_tests.rs`.


---

## Source: pointers.md

# AdeshLang Pointer System

## Overview

AdeshLang provides **explicit, deterministic, memory-safe raw pointers** within `unsafe` blocks. The pointer system is designed to:

- ✅ Provide explicit control over memory allocation
- ✅ Prevent use-after-free via state tracking
- ✅ Enforce bounds checking on all accesses
- ✅ Support typed pointer arithmetic
- ✅ Use RAII for deterministic cleanup
- ❌ NO garbage collection
- ❌ NO implicit allocations
- ❌ NO undefined behavior

## Pointer Type Syntax

AdeshLang supports typed pointer declarations:

```adesh
*u8     // Pointer to unsigned 8-bit integer (byte)
*i32    // Pointer to signed 32-bit integer
*f64    // Pointer to 64-bit floating-point
*u64    // Pointer to unsigned 64-bit integer
```

## Basic Usage

### Allocation and Deallocation

```adesh
unsafe {
    let ptr: *u8 = alloc(100);  // Allocate 100 bytes
    ptr[0] = 42;                 // Write to index 0
    let val = ptr[0];            // Read from index 0
    free(ptr);                   // Explicitly free memory
}
```

### RAII Automatic Cleanup

Pointers allocated within `unsafe` blocks are automatically freed when they go out of scope:

```adesh
unsafe {
    let p = alloc(10);
    p[0] = 5;
    // `free(p)` is automatically inserted at scope exit
}
```

## Typed Pointer Arithmetic

AdeshLang uses **element-based indexing**, not byte-based:

```adesh
unsafe {
    let p: *i32 = alloc(16);  // 16 bytes = 4 i32 elements
    p[0] = 10;   // Writes bytes 0-3
    p[1] = 20;   // Writes bytes 4-7
    p[2] = 30;   // Writes bytes 8-11
    p[3] = 40;   // Writes bytes 12-15
}
```

### Memory Address Calculation

For a pointer of type `*T`:
```
address = base_ptr + (index * sizeof(T))
```

| Type   | Size (bytes) |
|--------|-------------|
| `*u8`  | 1           |
| `*i16` | 2           |
| `*i32` | 4           |
| `*f32` | 4           |
| `*i64` | 8           |
| `*f64` | 8           |

### Bounds Checking

All pointer accesses are bounds-checked:

```adesh
unsafe {
    let p: *i32 = alloc(8);  // 8 bytes = 2 i32 elements
    p[0] = 1;  // ✅ Valid
    p[1] = 2;  // ✅ Valid
    p[2] = 3;  // ❌ Runtime error: out of bounds
}
```

The bounds check verifies:
```
(index + 1) * sizeof(T) <= allocation_size
```

## Memory Safety Guarantees

### 1. Use-After-Free Detection

```adesh
unsafe {
    let p = alloc(10);
    free(p);
    let x = p[0];  // ❌ Runtime error: use-after-free
}
```

### 2. Double-Free Prevention

```adesh
unsafe {
    let p = alloc(10);
    free(p);
    free(p);  // ❌ Runtime error: double-free
}
```

### 3. Invalid Pointer Detection

```adesh
unsafe {
    let p: *u8 = 12345;  // Invalid pointer ID
    let x = p[0];  // ❌ Runtime error: invalid pointer
}
```

Pointer handles are obfuscated per-process, making them non-guessable. Forged handles fail validation and cannot be used to access memory.

### 4. Debug Mode Poisoning

In debug builds, freed memory is poisoned with `0xDE` pattern:

```adesh
#[cfg(debug_assertions)]
unsafe {
    let p = alloc(10);
    free(p);
    // Memory at p is now filled with 0xDEDEDEDE...
}
```

In release builds, freed memory is zeroed to prevent data remanence or accidental data theft after deallocation.

## RAII and Control Flow

RAII cleanup handles **all** control flow exits:

### Early Return

```adesh
fn process() -> i32 {
    unsafe {
        let p = alloc(100);
        if condition {
            return 42;  // free(p) inserted here
        }
        return 0;  // free(p) inserted here
    }
}
```

### Break and Continue

```adesh
unsafe {
    for i in 0..10 {
        let p = alloc(10);
        if i == 5 {
            break;  // free(p) inserted here
        }
        if i % 2 == 0 {
            continue;  // free(p) inserted here
        }
        // free(p) inserted at loop end
    }
}
```

## Function Parameters and Return Values

### Passing Pointers to Functions

```adesh
fn fill(buf: *u8, len: u64) unsafe {
    for i in 0..len {
        buf[i] = i as u8;
    }
}

unsafe {
    let p = alloc(10);
    fill(p, 10);  // Ownership remains with caller
    free(p);
}
```

**Rules:**
- Passing a pointer does **NOT** transfer ownership
- Caller retains responsibility for freeing
- Callee cannot free the pointer
- Pointer must remain valid for call duration

### Returning Pointers

```adesh
fn alloc_buffer(n: u64) -> *u8 unsafe {
    return alloc(n);  // Ownership transfers to caller
}

unsafe {
    let buf = alloc_buffer(100);
    // Caller now owns `buf` and must free it
    free(buf);
}
```

**Rules:**
- Ownership transfers to caller
- Caller becomes responsible for cleanup
- RAII applies at caller scope

## Embedded Mode

When building for embedded systems:

```bash
adesh build --embedded
```

Pointer operations are **forbidden** and will cause compile-time errors:

```adesh
unsafe {
    let p = alloc(10);  // ❌ Compile error: alloc forbidden in embedded mode
}
```

## Debug vs Release Behavior

### Debug Build

- Pointer poisoning on free (`0xDE` pattern)
- Pointer state validation
- Allocation map integrity checks
- Verbose error messages

### Release Build

- Pointer state checks remain **ON**
- Poisoning disabled
- Minimal runtime overhead

## Safety Contract

> **AdeshLang raw pointers are usable only inside `unsafe` blocks.**  
> **All pointer dereferences are bounds-checked, state-validated, and deterministically freed via RAII.**  
> **Use-after-free, invalid access, and out-of-bounds behavior always result in runtime errors, never undefined behavior.**

## Comparison with Other Languages

| Feature                  | AdeshLang     | Rust         | C            |
|-------------------------|-------------|--------------|--------------|
| Bounds checking         | Always      | `unsafe` only| Never        |
| Use-after-free detection| Always      | Compile-time | Never        |
| Automatic cleanup       | RAII        | RAII         | Manual       |
| Garbage collection      | NO          | NO           | NO           |
| Typed pointer math      | YES         | YES          | YES          |
| Requires `unsafe`       | YES         | YES          | NO           |

## Best Practices

1. **Prefer stack allocation** when possible
2. **Use `unsafe` blocks minimally**
3. **Let RAII handle cleanup** instead of manual `free()`
4. **Never pass pointers across ownership boundaries** without clear documentation
5. **Test pointer code in debug builds** to catch poisoning issues
6. **Avoid pointer arithmetic** until you understand element sizes

## Examples

See `examples/memory/` for comprehensive pointer usage examples:

- `pointer_basic.adesh` - Basic allocation and access
- `pointer_types.adesh` - Typed pointer demonstration
- `pointer_raii.adesh` - RAII cleanup examples
- `pointer_uaf_fail.adesh` - Use-after-free detection
- `pointer_bounds_fail.adesh` - Bounds check failure
- `pointer_fn_args.adesh` - Function parameter passing
- `pointer_return.adesh` - Returning pointers from functions

