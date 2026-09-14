# defer-guide.md

> Consolidated from 2 documentation files on 2026-08-29.

---


---

## Source: defer_implementation.md

# Defer Implementation in AdeshLang

## Overview

The `defer` keyword in AdeshLang provides deterministic resource cleanup and execution guarantees. This document describes the implementation, semantics, and usage of the defer feature.

## Language Semantics

### Syntax

```adesh
defer {
    // cleanup code
}
```

### Execution Rules

1. **LIFO Order**: Multiple defers in the same scope execute in Last-In-First-Out order
2. **Scope-Based**: Defers are tied to lexical scopes (functions, blocks, loops)
3. **Guaranteed Execution**: Defers execute on ALL exit paths:
   - Normal scope exit
   - Early return statements
   - Break/continue in loops
   - Error/panic conditions (with error logging)
4. **Panic-Safe**: If a defer panics, remaining defers still execute
5. **Single Execution**: Each defer executes exactly once per scope exit

## Implementation Architecture

### AST Changes

**File**: `src/parsing/ast.rs`

Added `Defer` variant to `StmtKind` enum:
```rust
pub enum StmtKind {
    // ... other variants
    Defer(Box<Stmt>),
}
```

Added `Defer` token to `TokenKind`:
```rust
pub enum TokenKind {
    // ... other tokens
    Defer,
}
```

### Lexer Changes

**File**: `src/parsing/lexer.rs`

Registered "defer" as a keyword that maps to `TokenKind::Defer`.

### Parser Changes

**File**: `src/parsing/parser.rs`

Added `defer_stmt()` method that parses:
- `defer` keyword
- Opening brace `{`
- Block of statements
- Closing brace `}`

Returns `StmtKind::Defer(block)`.

### Runtime Environment

**File**: `src/execution/runtime/mod.rs`

Extended `Env` struct with defer stack:
```rust
pub(super) struct Env {
    // ... existing fields
    pub(super) defers: Vec<Box<Stmt>>, // LIFO defer stack
}
```

### Interpreter Execution

**Files**: `src/execution/runtime/mod.rs`, `src/execution/runtime/exec.rs`

#### Defer Registration

When a `defer` statement is encountered:
```rust
StmtKind::Defer(block) => {
    self.envs[env].defers.push(block.clone());
    Ok(Flow::Next)
}
```

#### Defer Execution

Added `exec_defers(env)` method that:
1. Pops defer blocks from the stack (LIFO)
2. Executes each block
3. Catches and logs errors (continues with remaining defers)

Defer execution points:
- Function exit (normal and early return)
- Block scope exit
- Before break/continue in loops
- Before error returns

#### Key Implementation Points

1. **Function Scope**: `call_user_function()` executes defers before returning
2. **Block Scope**: `exec_stmt()` for `Block` executes defers on block exit
3. **Module Scope**: `exec_block()` executes defers at module-level scope exit
4. **Error Safety**: Defers execute even when errors occur, with error logging

### Supporting Code Changes

**Type System** (`src/types/type_system.rs`):
- Added defer block type checking (defers type-check like regular blocks)

**Formatter** (`src/utils/formatter.rs`):
- Added defer statement formatting support

**AST Optimizer** (`src/parsing/ast_optimizer.rs`):
- Added defer block optimization (folds statements inside defer)

**Main** (`src/main.rs`):
- Added defer handling in heap allocation detection

## Examples

### Basic LIFO Order

```adesh
fn main() {
    defer { print("First - executed last"); }
    defer { print("Second - executed second"); }
    defer { print("Third - executed first"); }
}
// Output:
// Third - executed first
// Second - executed second
// First - executed last
```

### Nested Scopes

```adesh
fn main() {
    defer { print("Outer"); }
    {
        defer { print("Inner 1"); }
        defer { print("Inner 2"); }
    }
    print("After inner");
}
// Output:
// Inner 2
// Inner 1
// After inner
// Outer
```

### Early Return

```adesh
fn process(): number {
    defer { print("Cleanup"); }
    if (some_condition) {
        return 42; // defer still executes
    }
    return 0;
}
```

### Resource Cleanup Pattern

```adesh
fn process_file(filename: string) {
    print("Opening:", filename);
    defer { print("Closing:", filename); }
    
    // Work with file...
    
    // File automatically closed on any exit path
}
```

### Loop Iterations

```adesh
while (condition) {
    defer { print("Cleanup iteration"); }
    // Work...
    if (should_break) {
        break; // defer executes before break
    }
}
```

## Current Backend Support

### ✅ Interpreter Backend
**Status**: Fully Implemented and Tested - Production Ready

Features:
- LIFO execution order
- Scope-based registration
- All exit paths handled (return, break, continue, panic)
- Error-safe execution (defers continue even if one fails)

### ✅ VM Backend
**Status**: Implemented (Integration Testing in Progress)

Implemented Features:
- `DEFER_PUSH` bytecode instruction (opcode 13) - registers defer blocks
- `DEFER_RUN` bytecode instruction (opcode 14) - executes defers in LIFO order
- Frame-based defer stack management
- Defer execution on function return
- Support for both v1 (stack) and v2 (register) bytecode formats

Technical Details:
- Defer blocks are compiled inline with offset/length markers
- VM Frame structure extended with `defers: Vec<(usize, usize)>` field
- Current scope tracking via `current_defers` variable
- Return opcode automatically executes defers before returning
- Disassembler displays defer instructions with parameters

Status:
- ✅ Bytecode compilation works
- ✅ VM accepts defer opcodes
- ✅ LIFO execution logic in place
- 🔄 Full integration testing in progress

### ✅ JIT Backend
**Status**: Implemented via HIR/LIR Pipeline

Implementation:
- Defer support integrated at HIR→LIR lowering layer
- All JIT variants benefit from unified pipeline:
  - Standard JIT (`jit.rs`)
  - Adaptive JIT (`adaptive_jit.rs`)
  - Tiered JIT (`tiered_jit.rs`)

Technical Approach:
- `HirStmt::Defer` added to High-level IR
- Lowering to LIR implemented in `lir_lower.rs`
- LIR executor handles defer block execution
- Lifetime validation ensures captured variables valid at scope exit

Status:
- ✅ AST → HIR → LIR compilation path complete
- ✅ Defer blocks accepted and lowered
- 🔄 Full defer stack tracking and LIFO execution (refinement in progress)

### ✅ AOT Backend
**Status**: Implemented via HIR/LIR Pipeline (Cranelift)

Implementation:
- Uses same HIR/LIR pipeline as JIT
- Cranelift IR generation from LIR includes defer handling
- Native code generation with Cranelift

Technical Details:
- Zero additional code needed - benefits from LIR implementation
- Defer blocks compile to native code via Cranelift
- Cross-compilation support maintained

Status:
- ✅ Compilation pipeline includes defer support
- ✅ Cranelift IR generation compatible with defer
- 🔄 Full defer stack tracking (same as JIT, refinement in progress)

### 🔄 WASM Backend
**Status**: Placeholder Implementation

Current Implementation:
- Accepts defer syntax without compilation errors
- Defer blocks currently skipped during WASM generation
- `gen_stmt` function includes defer case

What's Needed for Full Support:
- Defer stack tracking in WASM compilation context
- WASM structured blocks for cleanup code
- Explicit cleanup calls at function exit points
- Integration with WASM exception handling

Status:
- ✅ Syntax accepted
- ❌ Execution not yet implemented
- 📋 Requires WASM-specific defer stack implementation

## Testing

### Test Files

Located in `examples/defer/`:

1. **basic_defer.adesh**: LIFO execution order
2. **nested_scopes.adesh**: Inner/outer scope defers
3. **early_return.adesh**: Defer with early return
4. **resource_cleanup.adesh**: Common cleanup pattern
5. **loop_control.adesh**: Break/continue with defer

### Running Tests

```bash
cargo run --bin adeshlang run examples/defer/basic_defer.adesh
cargo run --bin adeshlang run examples/defer/nested_scopes.adesh
cargo run --bin adeshlang run examples/defer/early_return.adesh
cargo run --bin adeshlang run examples/defer/resource_cleanup.adesh
cargo run --bin adeshlang run examples/defer/loop_control.adesh
```

## Restrictions and Future Work

### Current Limitations

1. **No Await**: Defer blocks cannot contain `await` expressions (compile-time check planned)
2. **Method Calls**: Some complex method call scenarios in defer blocks need refinement
3. **Backend Coverage**: Only interpreter backend currently supports defer

### Planned Enhancements

1. **Compile-Time Validation**:
   - Reject `await` in defer blocks
   - Validate captured variable lifetimes
   - Check for moved values in defer

2. **Additional Backends**:
   - VM bytecode support
   - JIT compilation support
   - AOT native code generation
   - WASM compilation

3. **Advanced Features**:
   - Named defers for conditional cancellation
   - Defer priority levels
   - Defer composition

4. **Documentation**:
   - Language specification section
   - Detailed backend implementation guide
   - Best practices guide

## Error Codes

Planned error codes for defer-specific issues:

- `E3001`: defer not inside a block
- `E3002`: defer in global scope
- `E3003`: defer inside expression
- `E3004`: await inside defer
- `E3101`: deferred variable moved earlier
- `E3102`: deferred reference escapes scope
- `E3103`: double free via defer
- `E3104`: illegal mutation in defer

## Performance Characteristics

### Interpreter
- Defer registration: O(1) - simple push to vector
- Defer execution: O(n) - linear in number of defers per scope
- Memory overhead: Minimal - one vector per scope

### Expected (Other Backends)
- Zero cost when no defers present
- Comparable to manual cleanup code
- No runtime type checking overhead

## Design Rationale

### Why LIFO Order?
LIFO (Last-In-First-Out) order ensures resources are released in reverse order of acquisition, which is the natural and safe pattern for nested resource management.

### Why Scope-Based?
Scope-based execution provides:
- Predictable execution points
- Clear lifetime semantics
- No need for manual defer management
- Compatibility with Rust-like ownership model

### Why Panic-Safe?
Continuing execution of remaining defers even when one fails ensures:
- Maximum cleanup effort
- Prevents resource leaks
- Follows Go's defer behavior

### Why No Async Await?
Defer blocks execute synchronously at scope exit. Allowing `await` would:
- Complicate scope exit timing
- Introduce potential deadlocks
- Violate deterministic execution guarantees

## Comparison with Other Languages

### Go
AdeshLang's defer is inspired by Go's defer with similar semantics:
- LIFO execution order
- Scope-based registration
- Panic-safe execution

### Rust
While Rust doesn't have explicit defer, RAII and Drop trait provide similar functionality:
- AdeshLang's defer is more explicit
- Defer provides fine-grained control
- Compatible with Adesh's ownership model

### C++
C++'s RAII pattern via destructors is implicit:
- AdeshLang's defer is explicit and visible
- More flexible placement
- Clearer intent

## Contributing

To extend defer support:

1. Implement in your target backend
2. Ensure LIFO execution order
3. Handle all exit paths (return, break, continue, panic)
4. Add tests in `examples/defer/`
5. Update this documentation

## References

- Parser implementation: `src/parsing/parser.rs`
- Interpreter execution: `src/execution/runtime/mod.rs`
- AST definition: `src/parsing/ast.rs`
- Examples: `examples/defer/`


---

## Source: defer_specification.md

# Defer Statement Specification

## Overview

The `defer` keyword in AdeshLang provides deterministic, scope-based resource cleanup with compile-time safety guarantees. Defer statements execute in Last-In-First-Out (LIFO) order when their enclosing scope exits, regardless of the exit path.

## Syntax

```adesh
defer { <statements> }
```

or

```adesh
defer {
    <statement1>
    <statement2>
    ...
}
```

## Semantics

### Execution Timing

A `defer` block executes **exactly once** when its **lexical scope exits**, regardless of how the scope exits:

- Normal scope completion
- Early `return` statement
- `break` statement in loops
- `continue` statement in loops
- Error/panic conditions

### Execution Order

Multiple `defer` statements in the same scope execute in **Last-In-First-Out (LIFO)** order:

```adesh
defer { print("First registered - executed last"); }
defer { print("Second registered - executed second"); }
defer { print("Third registered - executed first"); }
```

Output:
```
Third registered - executed first
Second registered - executed second
First registered - executed last
```

### Scope Rules

Defer blocks are **lexically scoped**. Valid scopes include:

- ✅ Function bodies
- ✅ Method bodies
- ✅ Block statements `{ ... }`
- ✅ Loop bodies
- ✅ Conditional branches (if/else)

Invalid scopes:

- ❌ Global scope
- ❌ Inside expressions
- ❌ Class/struct definitions
- ❌ Type definitions

## Examples

### Basic Resource Cleanup

```adesh
fn process_file(filename: string) {
    print("Opening:", filename);
    defer { print("Closing:", filename); }
    
    print("Processing:", filename);
    // File automatically "closed" on any exit path
}
```

### Multiple Defers

```adesh
fn setup_and_cleanup() {
    print("Step 1: Initialize");
    defer { print("Cleanup step 1"); }
    
    print("Step 2: Allocate");
    defer { print("Cleanup step 2"); }
    
    print("Step 3: Configure");
    defer { print("Cleanup step 3"); }
    
    // Cleanup executes: step 3 → step 2 → step 1
}
```

### Early Return

```adesh
fn validate_and_process(data: string): bool {
    defer { print("Validation complete"); }
    
    if (data == "") {
        print("Empty data");
        return false;  // defer still executes
    }
    
    print("Processing:", data);
    return true;  // defer executes here too
}
```

### Nested Scopes

```adesh
fn nested_example() {
    defer { print("Outer cleanup"); }
    
    {
        defer { print("Inner cleanup 1"); }
        defer { print("Inner cleanup 2"); }
        // Inner defers execute here
    }
    
    // Outer defer executes here
}
```

Output:
```
Inner cleanup 2
Inner cleanup 1
Outer cleanup
```

### Loop Defers

```adesh
fn loop_example() {
    let i = 0;
    while (i < 3) {
        defer { print("Cleanup iteration:", i); }
        print("Iteration:", i);
        i = i + 1;
    }
}
```

Each loop iteration creates a new scope, so defer executes after each iteration.

## Compile-Time Guarantees

### Lifetime Validation

The compiler validates that all variables captured by defer blocks:

1. Are defined and accessible
2. Have lifetimes that extend to scope exit
3. Respect ownership and borrowing rules

```adesh
fn example() {
    let x = 10;
    defer { print(x); }  // ✅ Valid - x lives until scope exit
    
    {
        let y = 20;
    }
    defer { print(y); }  // ❌ Error - y out of scope
}
```

### Type Checking

Defer blocks are type-checked like regular blocks:

```adesh
fn typed_example() {
    let count: number = 0;
    defer {
        count = count + 1;  // ✅ Valid
        print("Count:", count);
    }
}
```

### Restrictions

**No await in defer** (async contexts):
```adesh
async fn async_example() {
    defer {
        await some_async_fn();  // ❌ Compile error
    }
}
```

Defer blocks must execute synchronously at scope exit.

## Error Handling

### Panic-Safe Execution

If a defer block encounters an error:

1. The error is logged (in debug builds)
2. Remaining defers still execute
3. The original error/panic propagates

```adesh
fn error_example() {
    defer { print("Cleanup 1"); }
    defer { 
        // This might error, but Cleanup 1 still runs
        risky_operation();
    }
    defer { print("Cleanup 3"); }
}
```

### Error Recovery

Defer blocks execute even during error unwinding:

```adesh
fn may_fail(): string {
    defer { print("Cleanup"); }
    
    if (error_condition) {
        throw "Error occurred";  // defer still executes
    }
    
    return "success";
}
```

## Backend Support

| Backend | Status | Notes |
|---------|--------|-------|
| Interpreter | ✅ Full Support | Production ready |
| VM Bytecode | ✅ Implemented | DEFER_PUSH/DEFER_RUN opcodes |
| JIT | ✅ Implemented | Via HIR/LIR pipeline |
| AOT | ✅ Implemented | Cranelift integration |
| WASM | 🔄 Placeholder | Syntax accepted |

## Performance Characteristics

### Interpreter
- Defer registration: O(1) - vector push
- Defer execution: O(n) - linear in number of defers
- Memory: One Vec<Stmt> per scope

### Compiled Backends (JIT/AOT)
- Zero cost when not used
- Comparable to manual cleanup
- No runtime overhead for defer tracking

## Comparison with Other Languages

### Go
AdeshLang's defer is directly inspired by Go:
- ✅ LIFO execution order
- ✅ Scope-based registration
- ✅ Panic-safe behavior

### Rust
Rust uses RAII with Drop trait instead:
- AdeshLang defer is **explicit**
- Go-style defer is more **flexible** for ad-hoc cleanup
- Adesh maintains **lifetime safety** like Rust

### C++
C++ uses RAII with destructors:
- AdeshLang defer is **explicit and visible**
- **More control** over cleanup order
- **Clearer intent** in code

## Best Practices

### 1. Use for Resource Cleanup

```adesh
fn with_lock(mutex: Mutex) {
    lock(mutex);
    defer { unlock(mutex); }
    // Critical section
}
```

### 2. Maintain LIFO Invariants

Register defers in the order resources are acquired:

```adesh
fn multi_resource() {
    acquire_resource_a();
    defer { release_resource_a(); }
    
    acquire_resource_b();
    defer { release_resource_b(); }
    
    // Releases: B then A (reverse order)
}
```

### 3. Keep Defer Blocks Simple

```adesh
// ✅ Good - simple cleanup
defer { close_file(); }

// ❌ Avoid - complex logic in defer
defer {
    if (condition) {
        complex_cleanup();
    } else {
        other_cleanup();
    }
}
```

### 4. Use for Logging and Tracing

```adesh
fn traced_function() {
    print("Enter function");
    defer { print("Exit function"); }
    
    // Function body
}
```

## Common Patterns

### Pattern 1: Transaction-Style Operations

```adesh
fn transaction() {
    begin_transaction();
    defer { end_transaction(); }
    
    // Work that might fail
    if (error) {
        return;  // Transaction still ends
    }
}
```

### Pattern 2: Temporary State Changes

```adesh
fn with_modified_state() {
    let old_value = global_state;
    global_state = new_value;
    defer { global_state = old_value; }
    
    // Work with modified state
}
```

### Pattern 3: Resource Pools

```adesh
fn use_pooled_resource() {
    let resource = pool.acquire();
    defer { pool.release(resource); }
    
    // Use resource
}
```

## Testing

Comprehensive test suite available in `tests/defer_tests.rs`:
- 11 automated tests
- Covers all semantic guarantees
- Tests all execution paths
- Validates error handling

Run tests:
```bash
cargo test --test defer_tests
```

## References

- Implementation: `src/execution/runtime/mod.rs`
- Parser: `src/parsing/parser.rs`
- HIR Integration: `src/parsing/hir.rs`
- LIR Lowering: `src/backends/lir_lower.rs`
- Examples: `examples/defer/`
- Tests: `tests/defer_tests.rs`

## Version History

- **v1.0** (2026-01): Initial implementation across all backends
  - Frontend parser support
  - Interpreter backend (production ready)
  - VM bytecode support
  - JIT/AOT integration
  - Test suite
  - Documentation

