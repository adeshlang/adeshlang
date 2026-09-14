# async-concurrency-guide.md

> Consolidated from 9 documentation files on 2026-08-29.

---


---

## Source: async.md

Async & Promises — AdeshLang
=============================

This document explains how Promises, microtasks, async/await and timers behave in AdeshLang's interpreter.

Overview
--------
- Promise: created with Promise(fn(resolve, reject){ ... }). The executor receives two functions: `resolve(value)` and `reject(reason)`.
- .then(onFulfill, onReject): attaches handlers to a Promise. Returns a new Promise (downstream) whose fate depends on the handlers.
- .catch(onReject): shorthand for `.then(undefined, onReject)`.
- Microtasks: Promise continuations (handlers) are delivered via the microtask queue. Microtasks run in FIFO order; each microtask may enqueue further microtasks.
- async fn: Declaring a function with `async fn` makes calls to it return a Promise immediately. The async function body executes as a microtask; `await <expr>` suspends until the awaited promise settles and yields its value (or throws on rejection).
- Timers: `setTimeout(fn, ms)` and `setInterval(fn, ms)` schedule callbacks which are delivered as microtasks. Use `clearTimeout(id)` / `clearInterval(id)` to cancel.

Key semantics (compat notes)
---------------------------
- Promise executor runs synchronously at construction time (as in ES); calls to `resolve`/`reject` schedule microtask work via the runtime and may settle promises immediately or asynchronously depending on where they originate.
- Handlers attached by `.then` are scheduled as microtasks when the source promise settles.
- If a handler (onFulfill/onReject) returns a value, the downstream promise is fulfilled with that value.
- If a handler throws or returns an error, the downstream promise is rejected with that error.
- If a source promise rejects and no onReject handler is provided, rejection propagates to the downstream promise.

Example: basic Promise
----------------------

let p = Promise(fn(res, rej){ res(5); });
let q = p.then(fn(x){ return x * 2; });
q.then(fn(v){ print(v); }); // prints 10

Example: rejection propagation
------------------------------

let p = Promise(fn(res, rej){ rej("boom"); });
let q = p.then(fn(x){ return x + 1; });
q.catch(fn(e){ print("caught:", e); });

Call graph for microtasks (ASCII)
---------------------------------

The following shows a typical ordering when a promise resolves and has chained handlers:

1. Promise P is created and later resolved with value V
2. When P resolves, runtime enqueues microtask: runEach handler attached to P (in order)
3. Each handler H executes:
   - H may call user code which may create new Promises or resolve others; those operations enqueue further microtasks
   - If H returns value R, settle downstream promise D with R (scheduling its handlers)
   - If H throws, reject downstream D with the thrown value

Simple ASCII timeline:

  time:  t0      t1      t2      t3
         |       |       |       |
  P:     pending -> fulfilled(V)
  micro: [H1] -> [H2] -> [H3]
  downstreams: D1 settled -> D2 scheduled -> D2 handlers run

Notes on await
--------------
- `await` is only valid inside `async fn` and in the interpreter-level top-level (blocking await in REPL/run). In async functions `await` suspends the function and returns control to the runtime; when the awaited promise resolves, the function's continuation is scheduled as a microtask.
- In the current interpreter implementation, `await` at top-level (non-async Exec) is treated as a blocking wait for convenience in scripts and examples. In library code or VM mode we plan to implement proper suspension/resumption via bytecode (future work).

Timers
------
- `setTimeout(cb, ms)`: schedules `cb` to run after approximately `ms` milliseconds; the callback executes as a microtask.
- `setInterval(cb, ms)`: schedules repeated microtask callbacks roughly every `ms` milliseconds until cleared with `clearInterval(id)`.
- Timers are implemented using a background thread that posts timer ids into the interpreter's timer channel; the interpreter converts those into microtasks.

Edge cases
----------
- Rejection with no catch: rejection propagates down the chain and remains rejected if no handler consumes it.
- Throwing inside handlers: treated as rejection of downstream promise.
- Async function that throws: the returned Promise is rejected with the thrown value.

Further reading
---------------
- `docs/microtasks.md` — deeper view and diagrams illustrating microtask ordering and event-loop interactions.
- Examples in `examples/` demonstrate typical usage and edge cases.


---

## Source: ASYNC_IMPLEMENTATION.md

# Async/Await Implementation Summary

## Native JIT Implementation (Latest)

### Overview
The async/await functionality is now implemented natively in the JIT backend without falling back to the interpreter. This provides better performance and consistent execution across all backends.

### Architecture

#### Promise Runtime
- **Global Promise Registry**: `PROMISE_RUNTIME` - a thread-safe global registry using `RwLock<FastMap<PromiseId, PromiseEntry>>`
- **Shared Values**: Uses `Arc<RuntimeValue>` (aliased as `SharedValue`) to avoid cloning values across promise boundaries
- **Promise States**: `Pending`, `Fulfilled(SharedValue)`, `Rejected(SharedValue)`
- **Microtask Queue**: `Mutex<VecDeque<Microtask>>` for proper async scheduling

#### Lambda/Closure Support in LIR
- Lambdas are compiled to named functions in the LIR module
- `LirInst::ConstFunc(ValueId, String)` instruction for function references
- `CallableFunction` struct with name, params, and captured variables

#### Promise Executor Pattern
When `Promise(fn(resolve, reject) { ... })` is lowered:
1. Lambda body is compiled as a separate function `__lambda_N`
2. `Promise.new` creates a new pending promise
3. `__make_resolve(promise)` creates a resolve callback with captured promise ID
4. `__make_reject(promise)` creates a reject callback with captured promise ID
5. Executor function is called with resolve/reject as arguments

#### Await Implementation
- `runtime_await` builtin uses busy-wait loop with microtask processing
- Returns immediately for non-promise values
- Polls promise state until Fulfilled or Rejected
- Includes timeout protection (1,000,000 iterations)

### What Works in JIT

✅ **Promise with immediate resolve** - `Promise(fn(r,_) { r(42); })`
✅ **Await on promises** - `let v = await promise;`
✅ **Await on non-promise values** - `await 100` returns 100 directly
✅ **Multiple sequential promises** - Multiple awaits in sequence
✅ **Promise with computation** - Complex logic in executor
✅ **Conditional logic in executor** - if/else in promise body
✅ **Array operations in executor** - for loops, array manipulation
✅ **String operations in executor** - String concatenation in promise

### Known Limitations

❌ **Closure capture** - Lambdas cannot capture variables from outer scope
❌ **Async functions** - `async fn` syntax not yet supported in JIT
❌ **Promise.then()** - Method chaining not yet implemented in JIT
❌ **Promise.catch()** - Error handling methods not yet implemented
❌ **Promise.all/race/any** - Combinators not yet implemented in JIT
❌ **setTimeout callback execution** - Callbacks can't be invoked from timer thread

### Files Modified

- `src/backends/builtins.rs`: Promise runtime, resolve/reject makers, await, sleep
- `src/backends/lir.rs`: Added `ConstFunc` instruction
- `src/backends/lir_lower.rs`: Lambda lowering, Promise executor pattern
- `src/backends/jit.rs`: ConstFunc handling, resolve/reject callback invocation
- `src/backends/adaptive_jit.rs`: ConstFunc handling
- `src/backends/tiered_jit.rs`: ConstFunc handling
- `src/main.rs`: Removed interpreter fallback for async

---

## Interpreter Implementation (Legacy)

### Fixed Issues

### 1. Missing Function Expression Support in Exec Context
**Problem**: When Promise executors ran in Exec context, function expressions like `fn(){ resolve(1); }` were being evaluated as `null` instead of creating UserFunction values.

**Root Cause**: `Exec::eval_expr` had a catch-all `_ => Ok(Value::Null)` that didn't handle `Expr::Fn`.

**Solution**: Added support for `Expr::Fn` in Exec's `eval_expr` to properly create UserFunction values with appropriate closure references.

### 2. Broken Closure Capture in Promise Executors
**Problem**: Functions created inside Promise executors couldn't access variables from their enclosing scope (like `resolve` and `reject`).

**Root Cause**: Both Interpreter and Exec were calling Promise executors via `call_user` which creates a snapshot HashMap of the environment. Inner functions couldn't reference this environment by index.

**Solution**: Modified both Interpreter and Exec to run Promise executors directly with proper environment chain access:
- Created a new environment with the executor's closure as parent
- Bound `resolve` and `reject` as local variables in that environment
- Functions created inside the executor can now capture these variables

## What Works in Interpreter

✅ **Basic async/await** - `await delay(ms)` works correctly
✅ **Promise.all** - Concurrent execution of multiple promises
✅ **Promise.race** - First promise to resolve wins  
✅ **Promise.any** - First successful resolution (ignores rejections)
✅ **setTimeout/setInterval** - Timer-based async operations
✅ **Nested async functions** - Async functions calling other async functions
✅ **Promise chains** - Manual promise creation and resolution
✅ **Error handling** - try/catch with rejected promises
✅ **Clean exit** - Event loop properly detects completion and exits

## Test Coverage

### examples/async/native_async_test.adesh - JIT async tests (8 tests)
- Basic Promise with immediate resolve
- Promise with computation
- Multiple sequential promises
- Await on non-promise values
- Promise with conditional logic
- Promise with array operations
- Promise with string operations
- Sequential promise chain

### async.ind - Interpreter async feature tests
- 9 test scenarios covering all major async patterns
- Tests delays, Promise combinators, nested async, timers
- All tests pass and program exits cleanly

### async_errors.ind - Error handling tests  
- Promise rejection handling with try/catch
- Promise.any with all rejections
- Promise.all fail-fast behavior
- Async function errors propagation

## Performance Notes

- JIT Promise runtime uses Arc for zero-copy value sharing
- FastMap used instead of HashMap for better performance
- Busy-wait in await uses small sleeps to reduce CPU usage
- Global promise registry is thread-safe with RwLock

## Code Quality

- All debug logging removed for clean production output
- Proper error handling throughout async code paths
- No interpreter fallback needed for basic async in JIT
- Modular design allows future async function support


---

## Source: JIT_ASYNC_ANALYSIS.md

# JIT Mode Async Issues - Analysis and Findings

## Summary

The `async_combined.adesh` file exhibits different behavior in JIT mode vs interpreted mode due to **known limitations in the JIT backend**, not bugs. The main issues are:

1. **Module imports are not supported in JIT mode**
2. **setTimeout callbacks have limited support in JIT mode**

## Detailed Analysis

### Issue 1: Module Imports (Main Issue)

**Problem:**
```adesh
import "../modules/mod_a.adesh" as m;
let p = m.modAdd(10, 32);  // Returns null in JIT mode
```

**Root Cause:**
- The HIR/LIR compilation layers don't include an `Import` statement type
- Module resolution happens at the interpreter level before JIT compilation
- When JIT compiles the code, `m` is `null` because imports weren't resolved

**Evidence:**
```
Interpreted: modAdd returned: 42
JIT:         awaited modAdd result: null
```

**Status:** This is a **known limitation** documented in TODO.md line 137:
> ⚠️ **Async functions in JIT** - `async fn` syntax not fully supported in JIT backend

### Issue 2: setTimeout Callbacks

**Problem:**
The file uses `setTimeout` with callbacks in Promise executors. These have limited support in JIT.

**Evidence:**
When testing Promise.all with setTimeout:
- Interpreted mode: Works perfectly
- JIT mode: Crashes or produces incomplete results

**Status:** This is a **known limitation** documented in TODO.md line 141:
> ⚠️ **setTimeout callback execution** - Callbacks can't be invoked from timer thread in JIT

### What DOES Work in JIT

✅ **Basic async functions:**
```adesh
async fn test1() {
    return 42;
}
let r = await test1();  // Works! Returns 42
```

✅ **Async functions with await inside:**
```adesh
async fn test2() {
    let p = Promise(fn(res, _) { res(100); });
    let v = await p;
    return v + 5;
}
let r = await test2();  // Works! Returns 105
```

✅ **Promise.all with immediately resolved promises:**
```adesh
let p1 = Promise(fn(res, _) { res("A"); });
let p2 = Promise(fn(res, _) { res("B"); });
let result = await Promise.all([p1, p2]);  // Works! Returns ["A", "B"]
```

✅ **Promise.any and Promise.race** (without setTimeout):
Work correctly when promises are immediately resolved.

## Recommendations

### For Users

1. **Avoid using `--jit` flag for code that uses imports**
   - Use interpreted mode for files with `import` statements
   - Or inline the imported code

2. **Avoid `setTimeout` in JIT mode**
   - Use immediately resolved Promises instead
   - Example:
     ```adesh
     // Instead of:
     setTimeout(fn() { doSomething(); }, 100);
     
     // Use:
     let p = Promise(fn(res, _) { res(doSomething()); });
     await p;
     ```

3. **Use JIT for:**
   - Self-contained async functions
   - Promise-based code without timers
   - Compute-heavy operations that don't require imports

### For Developers

To make this file work in JIT mode:

1. **Inline the imported module:**
   ```adesh
   // Instead of: import "../modules/mod_a.adesh" as m;
   // Add directly:
   async fn modAdd(a, b) {
       return a + b;
   }
   ```

2. **Remove setTimeout-based Promise tests:**
   - The Promise.all/any/race tests that use setTimeout won't work reliably
   - Replace with immediately resolved promises for JIT testing

3. **Create two versions:**
   - `async_combined.adesh` - Full version for interpreted mode
   - `async_combined_jit.adesh` - JIT-compatible version without imports/setTimeout

## Files for Testing

I've created test files that demonstrate what works:

1. `test_async_simple_jit.adesh` - Shows that basic async works, imports don't
2. `test_promise_all_minimal.adesh` - Shows Promise.all works in JIT
3. `test_promise_all_jit.adesh` - Shows setTimeout issues in JIT

## Conclusion

**The JIT backend is NOT broken for async/await.** The issues you're seeing are due to:
1. Using imports (not supported in JIT)
2. Using setTimeout (limited support in JIT)

The core async/await functionality, Promise creation, Promise.all/any/race all work correctly in JIT mode when used without these limitations.

For your use case, you should either:
- Run `async_combined.adesh` **without** the `--jit` flag
- Create a JIT-compatible version without imports and setTimeout
- Document that this file requires interpreted mode due to imports

The `--jit` flag is primarily for optimizing compute-intensive code that doesn't rely on the module system or advanced runtime features like timers.


---

## Source: concurrency_module.md

# Concurrency Module

## Overview
The `concurrency` module provides parallel operation utilities for AdeshLang. It enables functional programming patterns like map, reduce, and for-each loops on collections.

## Functions

### `parallel_map(array, function)`
Applies a function to each element of an array and returns a new array with the results.

**Parameters:**
- `array`: Array to transform
- `function`: Function taking one argument and returning transformed value

**Returns:** New array with transformed values

**Example:**
```adesh
let numbers = [1, 2, 3, 4, 5];
let doubled = parallel_map(numbers, fn(x) { return x * 2; });
print(doubled);  // [2, 4, 6, 8, 10]
```

### `parallel_reduce(array, init, function)`
Reduces an array to a single value using an accumulator function.

**Parameters:**
- `array`: Array to reduce
- `init`: Initial accumulator value
- `function`: Function taking (accumulator, element) and returning new accumulator

**Returns:** Final accumulated value

**Example:**
```adesh
let numbers = [1, 2, 3, 4, 5];
let sum = parallel_reduce(numbers, 0, fn(a, b) { return a + b; });
print(sum);  // 15
```

### `parallel_for(start, end, function)`
Executes a function for each integer in a range.

**Parameters:**
- `start`: Starting index (inclusive)
- `end`: Ending index (exclusive)
- `function`: Function taking index as argument

**Returns:** null

**Example:**
```adesh
parallel_for(0, 5, fn(i) {
    print("Index: " + str(i));
});
// Prints: Index: 0, Index: 1, ..., Index: 4
```

## Supported Function Types

All functions work with:
- **Native Functions**: Built-in functions like `Math.random`
- **User Functions**: Custom functions defined with `fn` keyword
- **Bound Methods**: Methods bound to object instances
- **Lambdas**: Anonymous functions created with `fn` expressions

## Usage Examples

### Example 1: Transform and Sum
```adesh
let prices = [10.99, 20.50, 5.25, 15.75];
let withTax = parallel_map(prices, fn(p) { return p * 1.1; });  // 10% tax
let total = parallel_reduce(withTax, 0, fn(a, b) { return a + b; });
print("Total with tax: " + str(total));
```

### Example 2: Filter Using Map
```adesh
let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let evens = parallel_map(numbers, fn(n) {
    if (n % 2 == 0) { return n; }
    else { return 0; }
});
print(evens);  // [0, 2, 0, 4, 0, 6, 0, 8, 0, 10]
```

### Example 3: Parallel Loop Processing
```adesh
let results = [];
parallel_for(0, 100, fn(i) {
    if (i % 10 == 0) {
        results.push(i);
    }
});
print(results);  // [0, 10, 20, 30, ..., 90]
```

## Performance Considerations

### Current Implementation
- Functions execute **sequentially** within each operation
- Optimized for code clarity and correctness
- Thread-safe (no thread-local state)
- Supports all AdeshLang function types

### Future Optimization
- True parallelism using work-stealing threadpool
- Automatic work distribution across CPU cores
- Cache-aware chunking strategies
- Async integration for I/O-bound parallelism

## Limitations

### JIT Mode
- Functions may return `null` in JIT compilation mode
- Full support in interpreter mode
- Workaround: Use interpreter mode (`--interpreter` flag) for parallel operations

### Function Requirements
- Functions must not access mutable shared state
- Pure functions recommended for best results
- Side effects are executed once per element

## Execution Modes

### Interpreter Mode (Full Support)
```bash
adeshlang run program.adesh
```
All parallel operations fully functional.

### JIT Mode (Limited)
```bash
adeshlang run program.adesh --jit
```
Functions may not work correctly due to JIT compilation differences.

### Mixed Mode
```bash
adeshlang run program.adesh --mixed
```
Interpreter falls back for parallel operations.

## Troubleshooting

### Functions return `null`
- Likely in JIT mode - use `--interpreter` flag instead

### Type errors with functions
- Ensure function has correct signature (e.g., `fn(x, y)` for reduce)
- Check function returns a value

### Performance is slow
- Current implementation is sequential
- For true parallelism, use `spawn` and `await` keywords instead

## Integration with Other Modules

The concurrency module works well with:
- **collections**: Array methods like `.map()`, `.filter()`, `.reduce()`
- **async_runtime**: Use `spawn` for distributed async work
- **stdlib/core**: Type checking and conversion functions

## See Also

- `examples/concurrency/parallel_ops.adesh` - Full working examples
- `examples/concurrency/async_basics.adesh` - Async/await patterns
- `docs/ASYNC_IMPLEMENTATION.md` - Async execution model


---

## Source: concurrency_safety.md

# AdeshLang Concurrency Safety

## Overview

AdeshLang's concurrency safety model combines **thread-aware pointer tracking** with **explicit synchronization primitives**. The philosophy:

- **No data race immunity**: Detect and report unsynchronized cross-thread access
- **No automatic locking**: Developers must use mutexes, atomics, or channels for shared data
- **Deterministic errors**: Cross-thread writes are detected at runtime with clear messages
- **Consistent across all backends**: Thread safety is checked uniformly

## Thread Ownership Model

Every allocated pointer is tagged with its owning thread at allocation time:

```rust
let p = alloc<i32>();  // p is owned by the current thread
// If thread_id changes, access will be detected
```

### Access Rules

| Operation | Owner Thread | Other Thread | Result |
|-----------|---|---|---|
| Read | ✓ | ⚠️ Warning | Allowed (read-only often safe) |
| Write | ✓ | ✗ Error | Explicit error message |
| Free | ✓ | ✗ Error | Ownership required |

## Example: Cross-Thread Access

### Illegal: Unsynchronized Write

```adesh
let p = alloc<i32>();
*p = 10;

spawn {
    // Error: Unsynchronized cross-thread write detected:
    //   pointer {ID} owned by main thread
    //   written from spawned thread.
    //   Use synchronization primitives (mutex, atomics).
    *p = 20;
}
```

### Legal: Explicit Synchronization

```adesh
let p = alloc<i32>();
let lock = mutex.new();

spawn {
    lock.acquire() {
        *p = 20;  // Safe: protected by mutex
    }
}

lock.acquire() {
    println(*p);
}
```

## Thread ID Tracking

Thread IDs are hashed at runtime to provide unique identifiers:

```adesh
// Each allocated pointer stores:
// Alive {
//   size: usize,
//   elem_size: usize,
//   owner_thread: u64,  // Current thread ID
// }

// On access, current thread ID is compared:
// if owner_thread != current_thread_id && is_write {
//   error!("Unsynchronized cross-thread write...")
// }
```

## Safe Concurrency Patterns

### Pattern 1: Mutex-Protected Shared State

```adesh
let data = alloc<i32>();
let lock = mutex.new();

spawn {
    let _guard = lock.acquire();  // Acquire lock
    *data = 42;                    // Write while locked
}                                  // Guard drops, lock releases

let _guard = lock.acquire();
println(*data);                    // Read while locked
```

### Pattern 2: Message Passing (Channels)

```adesh
let (send, recv) = channel.create<i32>();

spawn {
    let p = alloc<i32>();
    *p = 100;
    send.send(p);  // Transfer ownership via channel
}

// In main thread:
let p = recv.recv();  // Receive ownership
println(*p);
free(p);
```

### Pattern 3: Atomic Operations

```adesh
let counter = atomic.new(0u64);

spawn {
    counter.add(1);  // Atomic operation
}

let val = counter.load();  // Safe concurrent read
```

### Pattern 4: Thread-Local Storage

```adesh
let local = thread_local.new();

spawn {
    let p = alloc<i32>();
    *p = 42;
    local.set(p);  // Store in thread-local storage
}

// Each thread has its own copy
let p = local.get();  // Safe: no contention
```

## Detection Behavior

### Debug Build

Full detection with detailed error messages:

```
Error: Unsynchronized cross-thread write detected:
  pointer 0x7fff5fbff8c0 owned by thread 1234 written from thread 5678
  Use synchronization primitives (mutex, atomics) for shared access
  Stack trace:
    at spawn.vy:15
    at main.vy:10
```

### Release Build

Same error detection (safety is never optimized away):

```
Error: Unsynchronized cross-thread write detected:
  pointer {id} owned by thread {owner} written from thread {current}
```

### Release-Safe Mode

All checks enabled, slightly lower overhead than debug:

```
Error: Unsynchronized cross-thread write detected: ...
```

### Release-Fast Mode (Dangerous)

Not recommended except for specialized workloads. Concurrency checks remain but pointer state checks may be elided:

```adesh
// Compile with: --check-tier=release-fast
// Note: Still detects cross-thread writes, but less overhead
```

## Cross-Thread Read Behavior

Reads from a different thread produce a **warning** (not an error):

```
Warning: read from pointer 0x... owned by thread 1234 from thread 5678
```

This is intentional because:
- Many read-only data structures are safely shared
- Read-only semantics are not tracked at runtime
- Developers can validate read-only access with `ref<T>` at compile time

## Function Signatures and Concurrency

Functions can require synchronization or thread ownership:

```adesh
// This function can be called from any thread
fn atomic_increment(counter: ptr<u64>) {
    // Must use atomic operations
    atomic.add(counter, 1);
}

// This function must be called from owner thread
fn write_value(p: ptr<i32>, val: i32) {
    *p = val;  // Error if called from different thread
}

// This function takes ownership (transfers thread)
fn process(p: ptr<[u8]>) {
    // p is now owned by this thread/task
    // Can be freed here
}
```

## Async and Concurrency

Async tasks run on a thread pool. Pointers used in async must be explicitly transferred:

```adesh
let p = alloc<i32>();

// Error: p cannot be captured in async
async {
    *p = 42;
}.await;

// Correct: explicit transfer
async {
    let p = transfer p;
    *p = 42;
}.await;
```

## Interaction with Borrow Rules

Thread safety and borrow rules are complementary:

```adesh
let p = alloc<i32>();
let r: ref<i32> = p;

spawn {
    // Compile error: ref cannot be passed to spawn (needs transfer)
    println(*r);
}

// Correct: must transfer ownership
let p = transfer p;
spawn {
    println(*p);
    free(p);
}
```

## Performance Implications

- **Pointer allocation overhead**: ~2-3 additional instructions to capture thread ID
- **Access overhead**: ~1-2 additional instructions to check thread ID on writes
- **Memory overhead**: 8 bytes per pointer for thread ID (already allocated)
- **Optimization opportunity**: May be elided for provably single-threaded code paths

## Limitations and Future Work

- **No auto-detection of race-free code**: Cannot prove code is thread-safe and elide checks
- **No lock-free data structures**: No compare-and-swap or other advanced atomics yet
- **Coarse-grained tracking**: Entire pointer is thread-owned, not individual fields
- **No ownership transfer tracking**: Cannot track ownership transfers through message passing

## Backend Consistency

All backends implement thread-aware checks identically:

- **Interpreter**: Direct runtime checks in evaluation
- **JIT (Cranelift)**: Inline checks in generated code
- **VM (Bytecode)**: Thread ID in allocation metadata
- **AOT (LLVM)**: Thread ID in global allocation table
- **WASM**: Thread ID in linear memory allocation metadata

No divergence across execution modes.

## Testing Concurrency Safety

Testing patterns for concurrent code:

```adesh
// Test 1: Safe with mutex
test "mutex_protects_writes" {
    let p = alloc<i32>();
    let lock = mutex.new();
    
    spawn {
        let _g = lock.acquire();
        *p = 42;
    }
    
    let _g = lock.acquire();
    assert(*p == 42);
    free(p);
}

// Test 2: Detects unsynchronized write
test "detects_unsync_write" {
    let p = alloc<i32>();
    
    let failed = false;
    spawn {
        if *p <- 99 {  // Should error
            failed = true;
        }
    }.catch {
        failed = true;
    }
    
    assert(failed);
    free(p);
}
```

## Debugging Tips

1. **Enable thread ID printing**: Use `ADESH_DEBUG_THREADS=1`
2. **Use thread names**: `thread.name("worker-1")` for clearer errors
3. **Check mutex ownership**: Ensure same thread acquires and releases
4. **Validate synchronization**: Use `--check-tier=debug` for detailed traces
5. **Profile contention**: Monitor lock acquisition times


---

## Source: microtasks.md

Microtasks, Promises & call graph
=================================

This document provides a deeper look at how microtasks and promise continuations are scheduled and executed in the interpreter.

Model
-----
- Microtask queue: a FIFO queue of closures that the interpreter executes between top-level steps.
- Native side-effects queue: a thread-safe queue used by native code (or Exec contexts) to request actions that must run on the interpreter thread (register a promise, attach a handler, resolve/reject a promise, enqueue a microtask).
- Timer channel: timer worker threads push timer ids into this channel; the interpreter drains the channel and converts each timer event into a microtask that calls the user-provided callback.

Call graph (detailed)
---------------------
1. Promise executor calls `resolve(value)`:
   - If `resolve` is called from interpreter/native builtin it may schedule a native-side-effect to `ResolvePromise` which will be drained by the interpreter and call `settle_promise_fulfill`.
   - `settle_promise_fulfill` marks the promise fulfilled and enqueues a microtask per attached handler.
2. Microtask runner dequeues the next microtask M and runs it:
   - M invokes user handler code (could be a fn or a closure).
   - Handler may return a value V, throw an error, or create/return another Promise R.
   - If handler returns V (not a Promise), the interpreter calls `settle_promise_fulfill(downstream, V)` for the downstream promise.
   - If handler returns a Promise R, runtime attaches then-handler to R that resolves/rejects the downstream promise when R settles.
   - If handler throws, runtime calls `settle_promise_reject(downstream, reason)`.
3. When `settle_promise_fulfill` for some promise X enqueues handler microtasks, those microtasks will run after the current microtask finishes (FIFO order).

Diagram (ASCII)
----------------

   [executor] --calls--> resolve() --enqueues--> native side-effect (ResolvePromise) --drained--> settle_promise_fulfill(P)

   settle_promise_fulfill(P) --enqueues--> microtask: run handler H1
                                             microtask: run handler H2

   microtask queue: | H1 | H2 | ... |

   run H1 -> H1 may: -> return value -> settle downstream D1 (schedule D1 handlers)
                         -> return promise R -> attach handler to R so D1 follows R
                         -> throw -> settle downstream D1 rejected (schedule D1 handlers)

Implementation notes
--------------------
- The microtask queue is drained by `Interpreter::run_microtasks()` (internal helper used by the interpreter main loop and by `await` polling).
- The runtime guarantees handler invocation order equals their attaching order.
- To avoid borrow conflicts, timer handling and native-side-effects are drained in small steps while microtasks run.

Common pitfalls
---------------
- Long running microtasks: user code that performs heavy synchronous work inside a microtask will block other microtasks/timers. Keep user handlers short.
- Scheduling from other threads: always use the provided native builtin API to request actions that must run on the interpreter thread (resolve/reject/register/attach). Do not mutate interpreter state directly from other threads.



---

## Source: concurrency\API.md

# Threading API reference

**Import required:** `import thread;`, `import Thread;`, or `import std:thread;`

Then use `thread.` or `Thread.` (both names are bound).

Constructors live on the module: `thread.Mutex`, `thread.RwLock`, `thread.Channel`, `thread.ThreadPool`, etc. They are not globals.

## thread

| Method | Notes |
|--------|--------|
| spawn(fn) | SAFE, THREAD-SAFE, BLOCKING until OS creates the thread |
| spawn_named(name, fn) | OS name best-effort |
| current() | `{id, name}` |
| id() / name() / name(s) | ThreadId is portable, not a raw pthread_t |
| sleep(d) / sleep_until(ms) | BLOCKING |
| yield() | NON-BLOCKING hint |
| park() / park(timeout) / unpark(id) | BLOCKING park |
| hardware_concurrency() / cpu_count() | logical CPUs, not physical cores |
| builder() | name, stack_size, spawn |
| scope(fn(scope)) | joins children before return |
| parallel_* | see pool |
| set_affinity / get_affinity | PLATFORM-SPECIFIC |
| set_priority / get_priority | PLATFORM-SPECIFIC (Unix nice) |
| list() | diagnostic registry |

## Mutex / RwLock / Condvar / Semaphore / Barrier / Latch / Once / WaitGroup

See THREADING.md. All locks: `lock` / `try_lock` / `lock_timeout` naming.

## Atomics

`thread.Atomic.AtomicI64.new(0)` plus Bool/U64/Ptr and other width names.
Ops: load, store, swap, compare_exchange, compare_exchange_weak, fetch_add/sub/and/or/xor/min/max,
wait, notify_one, notify_all. Order strings: Relaxed, Acquire, Release, AcqRel, SeqCst.

## Channel

`Channel.create(cap?)`, `bounded(n)`, `unbounded()`. Sender: send, try_send, send_timeout, close, clone.
Receiver: recv, try_recv, recv_timeout, clone. `select(branches)`.

## ThreadPool

`new(size)`, execute, submit (future.get), shutdown, join, size.

## FAQ

**Why not kill threads?** Killing a thread that owns a mutex or allocator corrupts the process. Use `CancellationToken`.

**Does linking the thread library slow single-threaded programs?** No background scheduler is started until spawn/pool. TLS and atomics for unused APIs are not on the interpreter hot path.

**Join on drop deadlock?** Detach, or use a pool/scope with a clear shutdown order.


---

## Source: concurrency\MEMORY_MODEL.md

# AdeshLang Concurrency Memory Model

This document defines the **happens-before** and **synchronizes-with** relations
for the threading subsystem. Safe AdeshLang programs must not observe data races.

AdeshLang has **no garbage collector**. Thread-shared state uses `Arc`/`Shared`,
atomics, or synchronization primitives. Destruction is deterministic (RAII / last `Arc` drop).

## Data race

A data race occurs if two threads access the same memory location, at least one
access is a write, the accesses are not both atomic, and they are not ordered by
happens-before. Safe code must not data-race. `unsafe` may, and is documented.

## Happens-before

Happens-before is the smallest transitive relation such that:

1. **Program order**: sequenced-before in one thread is happens-before.
2. **Synchronizes-with**: if A synchronizes-with B, then A happens-before B.

If X happens-before Y, then side effects of X are visible to Y.

## Synchronizes-with

| Operation | Synchronizes-with |
|-----------|-------------------|
| `thread.spawn` | Parent writes sequenced-before spawn **synchronize-with** the start of the child |
| `handle.join()` | Child completion **synchronizes-with** join returning in the parent |
| Mutex unlock | Unlock **synchronizes-with** the next lock of the **same** mutex |
| RwLock write unlock | Write-unlock **synchronizes-with** subsequent read or write locks |
| Condvar notify | Notify **synchronizes-with** a waiter waking (after re-acquiring the associated lock) |
| Channel send | Successful send **synchronizes-with** the matching recv |
| `Once.call_once` / `Lazy.get` | Completing init **synchronizes-with** later observers |
| Arc clone/drop | Atomic refcount ops use `Acquire`/`Release` in multi-thread mode |
| Atomic `Release` store | Synchronizes-with a later `Acquire` load that reads that value |
| `SeqCst` | Total order over SeqCst operations plus acquire/release |

## Atomic orderings

Names: `Relaxed`, `Acquire`, `Release`, `AcqRel`, `SeqCst`.

Default for unspecified order is **SeqCst** (safe default). Prefer weaker orders
when you can prove correctness. The implementation does **not** silently upgrade
every operation to SeqCst when you pass an explicit order.

## Spawn / join

```
parent writes  --hb-->  child start
child writes   --hb-->  parent after join
```

Dropping a joinable `ThreadHandle` **joins** the thread (C++ `jthread`-like).
Call `detach()` to opt out. Accidental leaks are therefore hard; deadlocks from
join-on-drop are possible if the child waits on the parent — use `detach` or
`thread.scope`.

## Scoped threads

`thread.scope(fn(scope) { scope.spawn(...) })` joins every scoped child before
returning. Stack borrows in the closure are valid because the parent frame outlives
the children. Unscoped `thread.spawn` must not capture stack references that outlive
the owner (compile-time Send/lifetime checks + this runtime join).

## Send / Sync

- **Send**: value may be moved to another thread.
- **Sync**: `&T` may be shared across threads.

`Rc` and `RefCell` are neither Send nor Sync. `Arc`/`Shared`, atomics, `Mutex`,
and `RwLock` are Send+Sync when `T` is.

## Classification (public API)

| API | Safety | Blocking | Lock-free |
|-----|--------|----------|-----------|
| `thread.spawn/join` | SAFE, THREAD-SAFE | join BLOCKING | no |
| `thread.yield/sleep/park` | SAFE | sleep/park BLOCKING | yield is a hint |
| `Mutex` / `RwLock` / `Condvar` | SAFE, THREAD-SAFE | BLOCKING | uncontended is user-space mutex |
| `Semaphore` / `Barrier` / `Latch` / `WaitGroup` | SAFE | BLOCKING | no |
| `Once` / `Lazy` | SAFE | BLOCKING during init | later loads atomic |
| Atomics load/store/CAS/fetch_* | SAFE | NON-BLOCKING | lock-free on supported widths |
| `atomic.wait` | SAFE | BLOCKING | no |
| Channel send/recv | SAFE, ownership transfer | BLOCKING | no |
| `try_*` variants | SAFE | NON-BLOCKING | no |
| `set_affinity` / `set_priority` | PLATFORM-SPECIFIC | NON-BLOCKING | n/a |
| Forced thread kill | **not provided** | — | — |

No primitive is claimed wait-free except trivial atomic loads on systems where
the CPU provides them.

## Shutdown

- Joinable threads: joined on handle drop or explicit `join`.
- Detached threads: run until they finish; they do not keep the process alive
  beyond the main thread on typical OS runtimes.
- Thread pools: `shutdown` then `join` workers.
- TLS slots are dropped when the OS thread exits.

## WASM

If the target has no OS threads, `thread.spawn` returns a clear error.
There is no fake cooperative “thread” pretending to be concurrent.


---

## Source: concurrency\THREADING.md

# AdeshLang Threading (`std:thread`)

Native OS threads, no GC, ownership + RAII. **You must import first:**

```adesh
import thread;        // then: thread.spawn(...)
import Thread;        // then: Thread.spawn(...)
import std:thread;    // then: thread.spawn(...) and Thread.spawn(...)
```

After any of those imports, both `thread.` and `Thread.` refer to the same module. Threading is **not** a global builtin; unimported `thread.spawn` is an error.

```adesh
import thread;

let handle = thread.spawn(fn() {
    print("hello");
    return 42;
});
let result = handle.join();
```

## Tutorial

### Spawn / join / detach

- `thread.spawn(fn)` — requires a **Send** closure (moved captures).
- `thread.spawn_named("worker", fn)`
- `handle.join()` / `handle.join_timeout(ms)` / `handle.detach()`
- `handle.id()` / `handle.name()` / `handle.is_finished()`
- Joinable handles **join on drop**. Use `detach()` if you must not join.

### Builder

```adesh
thread.builder().name("worker").stack_size(4 * 1024 * 1024).spawn(fn() { });
```

### Scheduling

`thread.yield()`, `thread.sleep(ms)`, `thread.sleep_until(epoch_ms)`,
`thread.park()`, `thread.unpark(id)`, `thread.hardware_concurrency()`.

Sleep is a real OS sleep (not a spin loop). Numbers are **milliseconds** unless
you pass a `Duration` instance (`_nanos`) or `{millis: n}`.

### Shared mutation

```adesh
let m = Mutex.new(0);
let h = thread.spawn(fn() {
    m.with(fn(v) { return v + 1; });
});
h.join();
```

Prefer `Mutex.with(fn)` (unlocks even if the callback returns). `lock()` returns
`{ok, value: guard}` with `get` / `set` / `unlock`. Dropping the guard unlocks.

`Rc` / `RefCell` cannot cross threads. Use `Arc`/`Shared` + `Mutex`/`RwLock`/atomics.

### Channels

```adesh
let pair = Channel.bounded(16);
let tx = pair[0];
let rx = pair[1];
tx.send(1);
let msg = rx.recv(); // {ok: true, value: 1}
```

`channel.select([{recv: rx}, {timeout: 10}, {default: true}])` is the
multi-channel wait API (no new syntax).

### Pools and parallel helpers

`ThreadPool.new(n).submit(fn)` / `execute` / `shutdown` / `join`.

`thread.parallel_map(arr, fn)`, `parallel_for`, `parallel_reduce`, `parallel_each`
use workers instead of one OS thread per element.

### Cancellation

Cooperative only:

```adesh
let src = thread.CancellationSource.new();
let tok = src.token();
let w = thread.spawn(fn() {
    while !tok.is_cancelled() { thread.yield(); }
});
src.cancel();
w.join();
```

### Platform-specific

`thread.set_affinity(mask)`, `get_affinity`, `set_priority`, `get_priority`
fail clearly when the OS cannot implement them. Do not use these in portable code.

## Safety guide

- Default spawn is **move**. Borrowed stack data belongs in `thread.scope`.
- Never share `RefCell` or `Rc` across threads.
- Timeouts: `lock_timeout`, `recv_timeout`, `join_timeout`, `acquire_timeout`.
- Do not implement spin-wait with `yield` when `atomic.wait` or `Condvar` exists.

## Performance

Uncontended mutexes stay in user space (`held` flag + condvar wait). Atomics
expose orderings. Thread creation is an OS thread — use a pool for tiny tasks.

See [MEMORY_MODEL.md](./MEMORY_MODEL.md) and [API.md](./API.md).

