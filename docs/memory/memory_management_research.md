# AdeshLang Memory Management Research

## Executive Summary

AdeshLang implements a **hybrid manual memory management system** with **no garbage collection (GC)** or **automatic reference counting (ARC)**. Instead, it uses a sophisticated combination of:

1. **Manual ownership tracking** with Rust-style borrowing semantics
2. **Reference counting** for shared data structures  
3. **Smart pointers** (`Shared<T>`, `Unique<T>`, `Weak<T>`)
4. **Multiple allocation strategies** (heap, arena, bump, slab)
5. **Small object optimizations** (SSO for strings, SAO for arrays)
6. **Debug-mode memory poisoning** for safety

## Memory Management Architecture

### 1. Core Memory Management Components

#### A. Dynamic Allocator (`src/memory/dynamic_allocator.rs`)
- **Three allocation modes**:
  - `Static`: Fixed-size heap with OOM on overflow
  - `Dynamic`: Auto-expanding heap with configurable growth
  - `Hybrid`: Arena pools with fallback to heap
- **Growth strategies**: Doubling, Linear, Slab-based
- **Slab allocator** for small objects (16B to 2KB size classes)
- **Arena pools** for short-lived allocations
- **Memory statistics tracking** with atomic counters

#### B. Ownership System (`src/utils/memory.rs`)
- **Ownership states**: `Unique`, `Shared`, `Moved`, `Borrowed`, `BorrowedMut`, `Poisoned`
- **Borrow tracking**: Prevents multiple mutable borrows or mutable+immutable conflicts
- **Lifetime validation**: Scope-based lifetime tracking for HIR analysis
- **Move semantics**: Values can be moved to transfer ownership

#### C. Smart Pointers
```rust
// Reference-counted shared ownership
Shared<T>     // Similar to Arc<T> but with allocation source tracking
Weak<T>       // Non-owning weak references
Unique<T>     // Move-only unique ownership with borrow checking
```

### 2. Value Representation (`src/types/value_optimized.rs`)

The runtime uses an optimized `Value` enum designed for performance:

```rust
pub enum Value {
    // Inline variants (no heap allocation)
    Number(f64),        // 8 bytes
    Bool(bool),         // 1 byte + padding  
    Char(char),         // 4 bytes
    Null,               // 0 bytes
    
    // Boxed variants (8-byte pointer)
    BigInt(Arc<BigInt>),           // Shared arbitrary precision integers
    Str(Arc<str>),                 // Interned strings for fast equality
    Array(Box<Vec<Value>>),        // Heap-allocated arrays
    Object(Arc<StringMap<Value>>), // Shared hash maps
    
    // Function types with shared ownership
    UserFunction(Arc<UserFnInner>),
    Class(Arc<UserClass>),
    Instance(Arc<UserInstance>),
    // ... other variants
}
```

**Key optimizations**:
- **Small enum size**: 16-32 bytes total (vs 80+ bytes in naive implementation)
- **Interned strings**: `Arc<str>` enables fast pointer-based equality
- **Shared complex types**: `Arc` for classes, objects, functions
- **Inline primitives**: Numbers, booleans, chars stored directly

### 3. Small Object Optimizations

#### A. Small String Optimization (SSO)
```rust
pub enum SsoString {
    Inline { buf: [u8; 22], len: u8 },  // Strings ≤22 bytes stored inline
    Heap(Arc<str>),                     // Larger strings on heap
}
```

#### B. Small Array Optimization (SAO)  
```rust
pub enum SaoArray<T> {
    Inline { data: [Option<T>; 8], len: u8 }, // Arrays ≤8 elements inline
    Heap(Vec<T>),                             // Larger arrays on heap
}
```

### 4. Allocation Strategies

#### A. Bump Allocator
- **Use case**: Short-lived AST/HIR nodes during compilation
- **Performance**: O(1) allocation, no individual deallocation
- **Memory model**: Arena-style, reset entire allocator at once

#### B. Slab Allocator  
- **Size classes**: 16, 32, 64, 128, 256, 512, 1024, 2048 bytes
- **Use case**: Small objects with predictable sizes
- **Performance**: O(1) allocation/deallocation within size class

#### C. Arena Allocator
- **Use case**: Function call temporaries, expression evaluation
- **Lifetime**: Bounded to function/expression scope
- **Thread-local**: Each thread has its own arena

#### D. Heap Allocator
- **Use case**: Large objects, long-lived data
- **Growth**: Configurable (doubling, linear, slab-based)
- **Tracking**: Full allocation/deallocation statistics

### 5. Memory Safety Features

#### A. Debug Mode Poisoning
```rust
#[cfg(debug_assertions)]
pub const POISON_PATTERN: u64 = 0xDEAD_BEEF_DEAD_BEEF;

// Poison freed memory to catch use-after-free bugs
unsafe fn poison_memory(ptr: *mut u8, size: usize);
```

#### B. Borrow Checker (HIR Analysis)
```rust
pub enum BorrowState {
    Unborrowed,
    ImmutablyBorrowed(usize),  // Count of active borrows
    MutablyBorrowed,
    Moved,
}
```

#### C. Ownership Validation
- **Compile-time**: HIR passes validate borrowing rules
- **Runtime**: Ownership trackers prevent invalid access patterns
- **Debug assertions**: Extensive validation in debug builds

## Memory Management Patterns

### 1. No Garbage Collection
AdeshLang explicitly **does not use garbage collection**. The TODO.md confirms:
```
- ⚠️ **No garbage collection** - Memory management is manual
```

### 2. No Automatic Reference Counting
While the system uses `Arc<T>` internally, this is **not automatic**. The language requires explicit memory management decisions by:
- Smart pointer selection (`Shared<T>` vs `Unique<T>`)
- Allocation strategy choice (heap vs arena vs bump)
- Explicit ownership transfer via move semantics

### 3. Rust-Inspired Ownership
The system borrows heavily from Rust's ownership model:
- **Move semantics**: Values can be moved to transfer ownership
- **Borrowing rules**: Prevent aliasing of mutable references
- **Lifetime tracking**: Scope-based validation
- **RAII**: Destructors handle cleanup automatically

### 4. Performance-First Design
- **Multiple allocators**: Choose optimal strategy per use case
- **Small object optimizations**: Avoid heap allocation for common cases
- **Interned strings**: Fast equality comparisons
- **Compact value representation**: Better cache utilization

## Comparison with Other Systems

| Feature | AdeshLang | Rust | Go | Java | JavaScript |
|---------|----------|------|----|----- |------------|
| **GC** | ❌ None | ❌ None | ✅ Mark & Sweep | ✅ Generational | ✅ Mark & Sweep |
| **ARC** | ❌ Manual only | ❌ Manual only | ❌ None | ❌ None | ❌ None |
| **Ownership** | ✅ Tracked | ✅ Compile-time | ❌ None | ❌ None | ❌ None |
| **Move Semantics** | ✅ Yes | ✅ Yes | ❌ Copy | ❌ Copy | ❌ Copy |
| **Borrowing** | ✅ Runtime checks | ✅ Compile-time | ❌ None | ❌ None | ❌ None |
| **Manual Memory** | ✅ Required | ✅ Safe | ❌ GC only | ❌ GC only | ❌ GC only |

## Memory Allocation Flow

### 1. Value Creation
```rust
// Small string - uses SSO
let s1 = Value::from_str("hello");        // Inline storage

// Large string - uses heap  
let s2 = Value::from_str("very long...");  // Arc<str> on heap

// Numbers - always inline
let n = Value::Number(42.0);              // Stack storage

// Arrays - size-dependent
let small_arr = vec![1, 2, 3];            // May use SAO
let large_arr = vec![1; 100];             // Heap allocation
```

### 2. Ownership Transfer
```rust
// Create unique ownership
let unique = Unique::new(data);

// Convert to shared ownership  
let shared = unique.into_shared();

// Create weak reference
let weak = shared.downgrade();

// Ownership tracking prevents use-after-move
// unique is now invalid (moved)
```

### 3. Memory Deallocation
```rust
impl Drop for Unique<T> {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        self.ownership.poison();  // Debug mode poisoning
        
        // Box<T> handles actual deallocation
    }
}
```

## Smart Pointer Usage Examples

### Example 1: Basic Unique Ownership
```rust
use adeshlang::utils::memory::{Unique, AllocSource};

// Create a unique pointer to some data
let mut unique_data = Unique::new(String::from("Hello, World!"));

// Access the data
println!("Data: {}", unique_data.get());

// Modify the data (unique ownership allows mutation)
*unique_data.get_mut() = String::from("Modified data");

// Try to borrow the data
if unique_data.try_borrow() {
    println!("Borrowed: {}", unique_data.get());
    unique_data.release_borrow();
}

// Convert to shared ownership (consumes the unique pointer)
let shared_data = unique_data.into_shared();
// unique_data is now invalid - ownership transferred

println!("Shared data: {}", shared_data.get());
println!("Reference count: {}", shared_data.strong_count()); // Should be 1
```

### Example 2: Shared Ownership with Multiple References
```rust
use adeshlang::utils::memory::{Shared, Weak, AllocSource};

// Create shared data
let shared1 = Shared::new(vec![1, 2, 3, 4, 5]);
println!("Initial ref count: {}", shared1.strong_count()); // 1

// Clone creates another reference to the same data
let shared2 = shared1.clone();
println!("After clone ref count: {}", shared1.strong_count()); // 2
println!("Both point to same data: {}", 
    std::ptr::eq(shared1.get(), shared2.get())); // true

// Access data through either reference
println!("Data via shared1: {:?}", shared1.get());
println!("Data via shared2: {:?}", shared2.get());

// Create weak references
let weak1 = shared1.downgrade();
let weak2 = shared2.downgrade();

println!("Weak references alive: {}", weak1.is_alive()); // true
println!("Strong count from weak: {}", weak1.strong_count()); // 2

// Drop one strong reference
drop(shared2);
println!("After dropping shared2: {}", shared1.strong_count()); // 1
println!("Weak still alive: {}", weak1.is_alive()); // true

// Try to upgrade weak reference
if let Some(upgraded) = weak1.upgrade() {
    println!("Successfully upgraded weak reference");
    println!("Data: {:?}", upgraded.get());
    println!("Strong count: {}", upgraded.strong_count()); // 2 (shared1 + upgraded)
}

// Drop the last strong reference
drop(shared1);
println!("After dropping shared1, weak alive: {}", weak1.is_alive()); // false

// Attempting to upgrade now fails
assert!(weak1.upgrade().is_none());
```

### Example 3: Complex Data Structure with Mixed Ownership
```rust
use adeshlang::utils::memory::{Shared, Unique, Weak, AllocSource};
use std::collections::HashMap;

// Simulate a node in a graph structure
#[derive(Debug)]
struct Node {
    id: u32,
    data: String,
    children: Vec<Shared<Node>>,
    parent: Option<Weak<Node>>, // Weak reference to prevent cycles
}

impl Node {
    fn new(id: u32, data: String) -> Self {
        Node {
            id,
            data,
            children: Vec::new(),
            parent: None,
        }
    }
}

// Create a small graph structure
let root = Shared::new(Node::new(1, "Root".to_string()));
let child1 = Shared::new(Node::new(2, "Child 1".to_string()));
let child2 = Shared::new(Node::new(3, "Child 2".to_string()));

// Set up parent-child relationships
// Note: In real implementation, we'd need interior mutability (RefCell/Mutex)
// This is a conceptual example showing the ownership patterns

println!("Root ref count: {}", root.strong_count()); // 1

// Simulate adding children (would need RefCell<Vec<Shared<Node>>> in practice)
// root.get_mut().children.push(child1.clone());
// root.get_mut().children.push(child2.clone());

// Set weak parent references to avoid cycles
// child1.get_mut().parent = Some(root.downgrade());
// child2.get_mut().parent = Some(root.downgrade());

println!("After setup:");
println!("Root ref count: {}", root.strong_count());
println!("Child1 ref count: {}", child1.strong_count());
println!("Child2 ref count: {}", child2.strong_count());

// The weak parent references don't increase root's ref count
// This prevents memory leaks from reference cycles
```

### Example 4: Arena and Bump Allocator Usage
```rust
use adeshlang::utils::memory::{Arena, BumpAllocator, arena_alloc, arena_reset};

// Thread-local arena allocation
let data1 = arena_alloc(String::from("Temporary data 1"));
let data2 = arena_alloc(vec![1, 2, 3, 4, 5]);

// These allocations are very fast (bump pointer allocation)
// but cannot be individually freed

// Reset the entire arena when done with temporary data
arena_reset();
// data1 and data2 are now invalid

// Manual arena usage
let mut arena = Arena::new();
let temp_string = arena.alloc(String::from("Arena allocated"));
let temp_number = arena.alloc(42i32);

println!("Arena generation: {}", arena.generation());
let (total_bytes, alloc_count, chunk_count) = arena.stats();
println!("Arena stats: {} bytes, {} allocations, {} chunks", 
    total_bytes, alloc_count, chunk_count);

// Reset arena (invalidates all allocations)
arena.reset();
println!("After reset generation: {}", arena.generation()); // Incremented

// Bump allocator for AST nodes during compilation
let mut bump = BumpAllocator::new();
let ast_node1 = bump.alloc(String::from("AST Node 1"));
let ast_node2 = bump.alloc(String::from("AST Node 2"));

// Very fast allocation, no individual deallocation
let (total, count, chunks) = bump.stats();
println!("Bump stats: {} bytes, {} allocations, {} chunks", total, count, chunks);

// Reset when compilation phase is done
bump.reset();
```

### Example 5: Memory Safety and Error Handling
```rust
use adeshlang::utils::memory::{Unique, OwnershipTracker, OwnershipKind, BorrowChecker};

// Ownership tracking example
let tracker = OwnershipTracker::new_unique();
assert_eq!(tracker.kind(), OwnershipKind::Unique);

// Successful borrowing
assert!(tracker.try_borrow());
assert_eq!(tracker.kind(), OwnershipKind::Borrowed);

// Multiple immutable borrows allowed
assert!(tracker.try_borrow());

// Mutable borrow fails while immutably borrowed
assert!(!tracker.try_borrow_mut());

// Release borrows
tracker.release_borrow();
tracker.release_borrow();
assert_eq!(tracker.kind(), OwnershipKind::Unique);

// Now mutable borrow succeeds
assert!(tracker.try_borrow_mut());
assert_eq!(tracker.kind(), OwnershipKind::BorrowedMut);

// Immutable borrow fails while mutably borrowed
assert!(!tracker.try_borrow());

tracker.release_borrow_mut();

// Borrow checker for compile-time analysis
let mut checker = BorrowChecker::new();
checker.declare("x".to_string());

// Multiple immutable borrows OK
assert!(checker.try_borrow("x"));
assert!(checker.try_borrow("x"));

// Mutable borrow fails
assert!(!checker.try_borrow_mut("x"));
assert!(checker.has_errors());

// Check errors
for error in checker.errors() {
    println!("Borrow error: {} - {}", error.variable, error.message);
}
```

### Example 6: Memory Allocation Strategies
```rust
use adeshlang::memory::{AllocatorConfig, AllocMode, GrowthStrategy, DynamicAllocator};

// Static allocator - fixed size heap
let static_config = AllocatorConfig::static_mode(1024 * 1024); // 1MB heap
let static_alloc = DynamicAllocator::new(static_config).unwrap();

// Try to allocate within limits
let ptr1 = static_alloc.alloc(512).unwrap();
println!("Allocated 512 bytes");

// This would fail if it exceeds the 1MB limit
match static_alloc.alloc(2 * 1024 * 1024) {
    Ok(_) => println!("Large allocation succeeded"),
    Err(e) => println!("Large allocation failed: {}", e),
}

static_alloc.dealloc(ptr1, 512);

// Dynamic allocator - grows as needed
let dynamic_config = AllocatorConfig::dynamic_mode(
    64 * 1024,    // Initial: 64KB
    16 * 1024 * 1024, // Max: 16MB
    GrowthStrategy::Doubling
);
let dynamic_alloc = DynamicAllocator::new(dynamic_config).unwrap();

// This will cause heap expansion
let large_ptr = dynamic_alloc.alloc(128 * 1024).unwrap(); // 128KB
println!("Dynamic allocation succeeded, heap expanded");

let stats = dynamic_alloc.stats();
println!("Allocator stats: {}", stats);
println!("Heap expansions: {}", stats.heap_expansions);

dynamic_alloc.dealloc(large_ptr, 128 * 1024);

// Hybrid allocator - arena + heap
let hybrid_config = AllocatorConfig::hybrid_mode(4096, 4); // 4KB arenas, 4 of them
let hybrid_alloc = DynamicAllocator::new(hybrid_config).unwrap();

// Small allocations use arena
let small_ptr1 = hybrid_alloc.alloc(64).unwrap();
let small_ptr2 = hybrid_alloc.alloc(128).unwrap();

// Large allocations use heap
let large_ptr2 = hybrid_alloc.alloc(8192).unwrap(); // 8KB

let hybrid_stats = hybrid_alloc.stats();
println!("Arena allocations: {}", hybrid_stats.arena_allocations);

// Reset arenas (invalidates arena-allocated pointers)
hybrid_alloc.reset_arenas();
// small_ptr1 and small_ptr2 are now invalid
// large_ptr2 is still valid (heap allocated)

hybrid_alloc.dealloc(large_ptr2, 8192);
```

These examples demonstrate the sophisticated memory management capabilities of AdeshLang, showing how developers can choose appropriate allocation strategies and ownership patterns for different use cases while maintaining memory safety through the ownership and borrowing system.

## Performance Characteristics

### 1. Allocation Performance
- **Bump allocator**: O(1) allocation, no fragmentation
- **Slab allocator**: O(1) for size classes, minimal fragmentation  
- **Arena allocator**: O(1) allocation, bulk deallocation
- **Heap allocator**: O(log n) with growth strategies

### 2. Memory Overhead
- **Value enum**: 16-32 bytes (vs 80+ bytes naive)
- **SSO strings**: 0 overhead for strings ≤22 bytes
- **SAO arrays**: 0 overhead for arrays ≤8 elements
- **Reference counting**: 16 bytes per `Arc<T>` (atomic counters)

### 3. Cache Performance
- **Compact values**: Better cache line utilization
- **Interned strings**: Pointer equality instead of string comparison
- **Locality**: Arena/bump allocators improve spatial locality

## Memory Safety Guarantees

### 1. Compile-Time Safety
- **HIR borrow checker**: Validates borrowing rules during compilation
- **Lifetime analysis**: Prevents references outliving their data
- **Move validation**: Ensures moved values aren't accessed

### 2. Runtime Safety  
- **Ownership tracking**: Prevents double-free and use-after-free
- **Borrow validation**: Runtime checks for borrowing violations
- **Debug poisoning**: Fills freed memory with poison pattern

### 3. Thread Safety
- **Atomic reference counting**: `Arc<T>` is thread-safe
- **Thread-local arenas**: Each thread has isolated temporary storage
- **Synchronized allocators**: Global allocator uses proper locking

## Limitations and Trade-offs

### 1. Manual Memory Management Burden
- **Programmer responsibility**: Must choose appropriate allocation strategies
- **Complexity**: Understanding ownership, borrowing, and lifetimes
- **Debugging**: Memory leaks possible with incorrect usage

### 2. No Automatic Collection
- **Cyclic references**: Must be broken manually (weak references)
- **Memory leaks**: Possible with incorrect smart pointer usage
- **Performance predictability**: No GC pauses, but manual management overhead

### 3. Runtime Overhead
- **Ownership tracking**: Additional metadata per value
- **Borrow checking**: Runtime validation costs
- **Reference counting**: Atomic operations for shared data

## Conclusion

AdeshLang implements a **sophisticated manual memory management system** that prioritizes:

1. **Performance**: Multiple specialized allocators, small object optimizations
2. **Safety**: Ownership tracking, borrowing validation, debug poisoning  
3. **Predictability**: No GC pauses, deterministic deallocation
4. **Flexibility**: Multiple allocation strategies for different use cases

The system is **not garbage collected** and **not automatically reference counted**. Instead, it requires explicit memory management decisions while providing safety mechanisms to prevent common memory errors. This approach offers the performance benefits of manual memory management with many of the safety guarantees typically associated with garbage-collected languages.

The design is heavily influenced by Rust's ownership model but adapted for a dynamic language runtime, making it unique among interpreted languages in its approach to memory management.