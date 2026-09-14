# performance-guide.md

> Consolidated from 2 documentation files on 2026-08-29.

---


---

## Source: performance.md

# Performance and Memory

AdeshLang achieves predictable latency by combining deterministic memory management with aggressive escape analysis. There is no tracing GC and no implicit ARC.

---

## August 2026 Performance Optimizations

### ARC Manager: Zero Lock Contention

The high-level `ArcManager` previously wrapped every value in `Arc<Mutex<Value>>`,
causing a mutex lock on every `get_value`/`set_value` call. Since the manager is
already behind a `Mutex<ArcManager>`, the inner lock was pure overhead.

**Fix**: Values are now stored directly in `ArcMetadata`. The outer `Mutex`
provides all necessary thread safety. Access is O(1) with zero lock contention.

**Time complexity**: O(1) per operation (was O(1) with lock overhead)  
**Space complexity**: O(n) for n ARC values (no per-value Arc/Mutex allocation)

### Low-Level ARC: Destructor Support

The C-compatible ARC runtime (`src/runtime/arc.rs`) now supports user-defined
destructors via a 16-byte header (refcount + drop_fn pointer).

- `arc_alloc_with_drop(size, align, drop_fn)` — allocate with destructor
- `arc_set_drop_fn(ptr, drop_fn)` — attach destructor after allocation
- Destructors are called before deallocation when refcount reaches zero

**Time complexity**: O(1) for refcount ops, O(1) for deallocation + destructor call  
**Space complexity**: 16 bytes header per allocation (was 8 bytes)

### Real Criterion Benchmarks

All placeholder benchmarks replaced with real measurements:
- Value clone (number, bool, string, array, object)
- String interning (cache hit/miss, equality)
- HashMap comparison (std vs FastMap, lookup, insert)
- Lexer (small/medium/large source)
- Parser (mixed constructs)
- HIR lowering (fibonacci)
- Safety passes (unified, HIR, phase3)
- Constant folding
- Full pipeline (lex → parse → lower → safety)

---

## Why No GC

- Consistent frame times for games, audio, and embedded.
- No stop-the-world pauses; drops happen deterministically.
- Memory costs are explicit and auditable.

---

## Deterministic Memory Model

- Ownership + borrowing are checked in HIR; release builds carry no metadata.
- ARC is explicit (`share`) and optional; forbidden in regions and embedded.
- Regions provide arena-based bulk free with zero per-object teardown.
- SSO/SAO keep small values inline to avoid heap trips.

---

## Allocation Strategies

| Use Case | Allocator | Notes |
| -------- | --------- | ----- |
| AST / HIR | Bump | Monotonic, reset per pass |
| Temporaries | Arena | Thread-local, amortized growth |
| Small objects (≤2KB) | Slab | Fixed buckets for cache locality |
| Long-lived | Heap | Fallback when escape analysis fails |
| Regions | Arena | Bulk free at scope end |
| Embedded | Stack + Arena | Heap and ARC disabled |

Growth strategies and per-allocator stats are required to keep allocation profiles visible.

---

## Escape Analysis (HIR → MIR)

- If a value does not escape, is not shared, and is not returned, place it on stack or arena.
- Only escaping or shared values use the heap.
- This pass is performance-critical; it must run before ARC insertion.

---

## ARC Costs

- ARC refcount inc/dec emitted only after explicit `share`.
- Single-threaded mode uses `Rc`; multi-threaded uses `Arc` with atomic refs.
- Cycles require `weak`; avoiding cycles removes refcount churn.

---

## Debug vs Release

| Build | Overhead |
| ----- | -------- |
| Debug | Ownership state, borrow counters, poisoning, double-free/use-after-free checks |
| Release | Only moves, ARC inc/dec, and drops; no metadata or poisoning |

---

## Practices for Predictable Performance

- Prefer regions for short-lived graphs and batch work.
- Keep data SSO/SAO-sized when possible.
- Avoid hidden heap allocations in hot paths; profile slab vs arena choice.
- Use explicit `share` sparingly; break cycles with `weak`.
- Validate embedded builds regularly to prevent regressions.

## Final Design Statement

> AdeshLang uses deterministic, GC-free memory management based on ownership, borrowing, explicit ARC, and region-based allocation, delivering predictable performance and strong safety guarantees across interpreter, JIT, AOT, WASM, and embedded targets.


---

## Source: optimization_strategy.md

# Performance Optimization Strategy for AdeshLang

## Goal: Outperform Python, Ruby, and approach JavaScript speeds

## Current Performance
- fib(30) ≈ 54.89 seconds (tree-walking interpreter)
- Target: < 2 seconds (10-20x improvement minimum)

## Optimization Strategies (Ordered by Impact)

### 1. **Bytecode VM with Register-Based Architecture** ✅ (Already exists)
- Status: VM already implemented in `vm.rs`
- Impact: 5-10x speedup
- Action: Ensure all features compile to bytecode

### 2. **Tail Call Optimization (TCO)**
- Impact: Eliminates stack overhead for recursive calls
- Implementation: Detect tail calls in compiler, convert to loops
- Expected gain: 2-3x for recursive algorithms

### 3. **Function Inline Caching**
- Cache resolved function locations
- Cache property/method lookups
- Expected gain: 1.5-2x for method-heavy code

### 4. **Constant Folding & Dead Code Elimination**
- Evaluate constant expressions at compile time
- Remove unreachable code
- Expected gain: 1.2-1.5x

### 5. **Specialized Number Operations**
- Fast path for integer arithmetic (avoid f64 overhead when possible)
- SIMD for array operations
- Expected gain: 2-3x for numeric code

### 6. **JIT Compilation (Tier 2)**
- Hot function detection
- Compile to native code via Cranelift/LLVM
- Expected gain: 10-50x for hot loops

### 7. **Object Layout Optimization**
- Hidden classes (like V8)
- Inline property storage
- Expected gain: 1.5-2x for object-heavy code

### 8. **String Interning**
- Deduplicate strings
- Fast string comparison
- Expected gain: 1.2-1.5x

### 9. **Garbage Collection Optimization**
- Generational GC
- Incremental collection
- Expected gain: Reduce pause times, 1.1-1.3x throughput

### 10. **Parallel Execution**
- Worker threads for async operations
- Parallel array operations
- Expected gain: Near-linear with core count

## Implementation Priority

### Phase 1: Quick Wins (Week 1-2)
1. ✅ Ensure bytecode VM works for all code
2. Tail call optimization
3. Constant folding
4. Function call optimization

### Phase 2: Medium Impact (Week 3-4)
5. Inline caching
6. Specialized number operations
7. String interning

### Phase 3: Advanced (Month 2-3)
8. JIT compilation (hot path detection + Cranelift backend)
9. Hidden classes for objects
10. Generational GC

### Phase 4: Parallelization (Month 4+)
11. Async runtime improvements
12. Parallel operations

## Benchmarking Strategy

### Test Suite
1. Fibonacci (recursive - tests function call overhead)
2. Array operations (tests memory/iteration)
3. Object property access (tests dynamic dispatch)
4. String manipulation (tests allocation)
5. Mixed workloads (realistic code)

### Comparison Targets
- Python 3.11+ (CPython)
- Ruby 3.2+ (YARV)
- Node.js 20+ (V8)
- Lua 5.4 (reference for small interpreters)
- Go (compiled baseline)

## Expected Results

### After Phase 1 (bytecode + TCO + constant folding)
- fib(30): ~5-10 seconds (5-10x improvement)
- Competitive with Python, slower than Node.js

### After Phase 2 (inline caching + optimized ops)
- fib(30): ~2-4 seconds (10-25x improvement)
- Competitive with Node.js for simple code

### After Phase 3 (JIT)
- fib(30): ~0.1-0.5 seconds (100-500x improvement)
- Faster than Python/Ruby, competitive with Node.js
- Approaching Go/Rust speeds for hot code

## Key Technologies

- **Bytecode VM**: Stack/register hybrid (already implemented)
- **JIT Backend**: Cranelift (easy Rust integration) or LLVM
- **GC**: Mark-and-sweep → Generational
- **Profiling**: Built-in performance counters
- **AOT**: Optional ahead-of-time compilation to native

## Success Metrics

- [x] Faster than Python on fibonacci
- [ ] 10x faster than current interpreter
- [ ] Within 2x of Node.js on numeric benchmarks
- [ ] Within 5x of Node.js on object-heavy benchmarks
- [ ] < 50ms startup time
- [ ] < 10MB memory for simple scripts

