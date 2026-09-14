# Defer Examples

This directory contains examples demonstrating the `defer` keyword in AdeshLang.

## What is `defer`?

The `defer` keyword schedules a block of code to execute when the current scope exits, regardless of how it exits (normal return, break, continue, or error). Deferred blocks execute in **LIFO (Last-In-First-Out)** order, ensuring predictable cleanup semantics.

## Examples

### 1. `basic_defer.adesh`
Demonstrates basic defer usage and LIFO execution order.

### 2. `nested_scopes.adesh`
Shows how defer works with nested scopes - inner scope defers execute before outer scope defers.

### 3. `early_return.adesh`
Demonstrates that defers execute even when a function returns early.

### 4. `loop_control.adesh`
Shows defer behavior with break and continue statements in loops.

### 5. `resource_cleanup.adesh`
Common pattern: using defer for automatic resource cleanup (file handles, locks, etc.).

## Running the Examples

```bash
# Run with interpreter
cargo run --bin adeshlang examples/defer/basic_defer.adesh

# Run with JIT (when supported)
cargo run --bin adeshlang --jit examples/defer/basic_defer.adesh

# Run with AOT (when supported)
cargo run --bin adeshlang --aot examples/defer/basic_defer.adesh
```

## Defer Guarantees

1. **LIFO Execution**: Defers execute in reverse order of declaration
2. **Scope-Based**: Defers are tied to lexical scopes (functions, blocks, loops)
3. **Always Executes**: Defers run on all exit paths (return, break, continue, panic)
4. **Panic-Safe**: If a defer panics, remaining defers still execute
5. **No Await**: Defer blocks cannot contain `await` expressions
6. **Deterministic**: Execution order is guaranteed at compile time

## Common Use Cases

- Resource cleanup (files, network connections, locks)
- Logging exit points
- Cleanup temporary state
- Transaction rollback
- Mutex unlocking
- Memory deallocation

## Restrictions

- Cannot use `await` inside defer blocks
- Cannot capture moved values
- Cannot escape references from the defer's scope
- Global scope defers are not allowed
