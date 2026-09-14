# Combined Memory Management Examples

This directory contains comprehensive examples that demonstrate AdeshLang's memory model in realistic scenarios, combining multiple features together.

## Examples

### 1. ownership_with_borrowing.adesh
Demonstrates the interaction between ownership and borrowing:
- Owned values (`Buffer` struct)
- Immutable borrows (`&Buffer`)
- Mutable borrows (`&mut Buffer`)
- Function parameters with reference types

**Key Concepts:**
- Borrow checker prevents data races
- Multiple immutable borrows allowed
- Single mutable borrow at a time
- No mutations during immutable borrows

### 2. arc_with_weak.adesh
Demonstrates shared ownership and weak references:
- `share` keyword for ARC allocation
- `weak` keyword for weak references
- Multiple strong references to same data
- Weak references prevent reference cycles

**Key Concepts:**
- Shared ownership without GC
- Reference counting overhead
- Cycle-breaking with weak references
- Automatic cleanup when all strong refs dropped

### 3. region_with_ownership.adesh
Demonstrates arena-based memory management:
- `region` blocks for scoped allocation
- Owned values within arena scope
- Move semantics within region
- Automatic bulk cleanup at scope exit

**Key Concepts:**
- Deterministic deallocation
- No per-object overhead
- Cache-friendly allocation
- Exact lifetime control

### 4. embedded_mode_example.adesh
Demonstrates embedded systems safe code:
- Stack allocation only
- No heap (no `share`, no `alloc`)
- Borrowing for zero-copy passing
- Deterministic performance

**Key Concepts:**
- No garbage collection
- No runtime overhead
- Predictable memory layout
- Suitable for bare metal/embedded

## Memory Model Overview

```
┌─────────────────────────────────────────────────────┐
│         AdeshLang Memory Management                  │
├─────────────────────────────────────────────────────┤
│                                                       │
│  Ownership      Stack allocation, single owner      │
│  ├─ Move        Transfer ownership, invalidate old  │
│  ├─ Drop        Automatic cleanup on scope exit    │
│  └─ Borrow      Temporary access via references    │
│                                                       │
│  Borrowing      Reference-based access              │
│  ├─ Immutable   Multiple &T allowed                │
│  ├─ Mutable     Single &mut T allowed              │
│  └─ Rules       NLL (Non-Lexical Lifetimes)        │
│                                                       │
│  Shared Ref     Reference counting (ARC)            │
│  ├─ share       Create strong reference             │
│  ├─ weak        Create weak reference               │
│  └─ Upgrade     weak -> strong (may fail)          │
│                                                       │
│  Regions        Arena-based allocation              │
│  ├─ region {}   Scoped arena for bulk alloc        │
│  ├─ Efficient   Cache-friendly allocation          │
│  └─ RAII        Automatic cleanup at scope         │
│                                                       │
│  Unsafe         Manual memory management            │
│  ├─ alloc       Unsafe heap allocation             │
│  ├─ free        Unsafe deallocation                │
│  └─ careful     Programmer's responsibility        │
│                                                       │
└─────────────────────────────────────────────────────┘
```

## Execution

Run individual examples:
```bash
adesh ownership_with_borrowing.adesh
adesh arc_with_weak.adesh
adesh region_with_ownership.adesh
adesh embedded_mode_example.adesh
```

## Key Guarantees

1. **No Garbage Collection** - Deterministic deallocation
2. **Memory Safety** - Type system prevents use-after-free, double-free, data races
3. **Zero Overhead** - No runtime checks in release mode
4. **Flexible Options** - Ownership, borrowing, ARC, regions, unsafe for different needs

## Memory Layout

### Stack Allocation (Ownership)
- Fastest access
- Limited by stack size
- Perfect for small values
- No indirection

### Heap Allocation (ARC/Shared)
- Unlimited size
- Reference counting overhead
- Shared ownership across boundaries
- Cycle-prone (use weak for cycles)

### Arena Allocation (Region)
- Bulk allocation/deallocation
- Cache-friendly
- Perfect for temporary values
- Exact scope control

### Unsafe Manual (alloc/free)
- Full control
- No safety checks
- Programmer responsible
- Use sparingly

## Performance Implications

| Method | Speed | Memory | Flexibility |
|--------|-------|--------|------------|
| Ownership | Fastest | Tight | Less |
| Borrowing | Fastest | Tight | More |
| ARC | Slower | Extra | Most |
| Region | Fastest | Tight | Medium |
| Unsafe | Varies | Varies | Unlimited |

## Best Practices

1. **Use Ownership** by default - simplest and fastest
2. **Use Borrows** to avoid copies - zero cost
3. **Use ARC** for truly shared data - measure cost
4. **Use Regions** for temporary objects - cache friendly
5. **Use Unsafe** rarely - only when necessary

## Related Documentation

- `docs/memory_model.md` - Detailed specification
- `docs/embedded.md` - Embedded systems guide
- `docs/performance.md` - Performance tuning guide
