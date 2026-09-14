# language-reference.md

> Consolidated from 6 documentation files on 2026-08-29.

---


---

## Source: language.md

# AdeshLang: Language Guide and Specification

## Overview

AdeshLang is a multi-paradigm language blending familiar constructs from JavaScript/TypeScript, Python, Dart, and Rust while emphasizing performance and portability. It runs in two modes:

- Interpreter: fast iteration with rich runtime features
- Compiler: portable bytecode (`.indbc`) executed on a VM, and WebAssembly (`.wasm`) for the web and host embedding

Design goals:
- Dual compatibility with interpretation and compilation
- Portability via bytecode and WebAssembly
- Clear, ergonomic syntax with gradual typing planned
- Concurrency with promises, channels, timers, and structured control flow
- Path to native AOT for “C-like” speed without sacrificing developer experience

## Syntax Basics

- Files optionally begin with a directive:
  - `@compile` — compile to portable bytecode and execute via VM
  - `@compile wasm` — compile to WebAssembly
- Comments: `// line comment`
- Semicolons end statements; blocks use braces `{}`
- Identifiers and literals are similar to JS/TS

## Types

Built-in runtime values (see `src/ast.rs:82`):
- `Number` (f64), `Bool`, `Str`, `Null`
- Collections: `Array`, `Tuple`, `Object`, `Set`
- Higher-level: `Function`, `UserFunction`, `Class`, `Instance`, `Struct`, `Enum`, `Interface`
- Async: `Promise` (internal id routed through the interpreter microtasks)

Type annotations (parser/runtime support):
- Variables and parameters can declare optional type annotations (see `src/parser.rs:97`, `src/parser.rs:136`)
- Runtime respects simple annotation strings for guards (see `src/runtime.rs:1663`)
- Type checking is conservative but present (`src/type_system.rs:96`, `src/typechecker.rs:1`)

## Variables

- Declarations: `let name = expr;` or `const NAME = expr;`
- Optional type annotation: `let x: Number = 10;`
- Exported bindings: `export let foo = 1;` (see `src/ast.rs:36`)

## Functions

- Declaration: `fn add(a, b){ return a + b; }`
- Async function: `async fn work(){ ... }`
- Optional return type: `fn f(a): Number { ... }`
- Function literals: `fn(x){ ... }` used inline
- Arrow functions are parsed and supported syntactically

WASM backend currently supports numeric functions and returns; return inference applies when the last statement is a numeric expression (see `src/wasm.rs:335`).

## Classes and Interfaces

- Classes: methods, optional `extends`, `implements`, and `abstract` marker (see `src/ast.rs:70`)
- `new ClassName(args)` creates instances (see `src/ast.rs:22`)
- **Visibility modifiers** (`public`, `private`, `protected`) are fully enforced at runtime:
  - `public` (default): accessible anywhere
  - `protected`: accessible in class and subclasses
  - `private`: accessible only within defining class
  - See [OOP Visibility Guide](OOP_VISIBILITY_GUIDE.md) for detailed usage
- **Properties** (getters/setters) provide encapsulated field access with custom logic:
  - `get propertyName()` defines a getter
  - `set propertyName(value)` defines a setter
  - Properties look like fields but execute methods
  - See [OOP Properties Guide](OOP_PROPERTIES_GUIDE.md) for detailed usage
- Interfaces define method signatures; structural enforcement is planned

## Structs and Enums

- `struct` with named fields (see `src/ast.rs:77`)
- `enum` with variants (see `src/ast.rs:80`)

## Modules

- Import: `import "./utils.adesh" as u;`
- Export: on declarations (functions, classes, variables)
- Module loader resolves relative paths; simple exports/imports supported at runtime (`src/runtime.rs:1`)

## Expressions and Collections

- Arithmetic: `+ - * /`
- Grouping: `(expr)`
- Arrays: `[1,2,3]`
- Tuples: `(1,2,3)` with single-element tuple `(x,)`
- Objects: `{ key: value }`
- Sets: `{1,2,3}`
- Calls: `callee(arg1, arg2)`
- Property access and assignment: `obj.name`, `obj.name = value`

## Control Flow

- `if (cond) { ... } else { ... }`
- `while (cond) { ... }`
- `for (name in iter) { ... }`
- `break`, `continue`, `return`

WASM supports `if/else` and `while` with numeric conditions compiled to integer truthiness (`src/wasm.rs:274`, `src/wasm.rs:286`).

## Errors

- `throw Error("message")`; `try { ... } catch(e) { ... }`
- Rich diagnostics include file/line/col and caret (`src/error.rs:1`)

## Async and Concurrency

- Promises and microtasks engine; timers integrated with the interpreter (`src/runtime.rs:1`)
- Builtins:
  - `sleep(ms)` returns a Promise and integrates with timers (`src/runtime.rs:390`)
  - `makeChannel()` returns an object with `send(v)` and `recv()`; `recv()` returns a Promise and resolves when data arrives (see `src/runtime.rs:348`)

Structured concurrency patterns build atop promises and channels; `async/await` is parsed and supported in the interpreter semantics.

## Dual Execution: Interpreter and Compile

- Interpreter path is default: `cargo run -- run path/to/file.adesh`
- Compile bytecode when directive present:
  - `@compile` → emits portable bytecode and runs via VM
  - `@compile wasm` → emits WebAssembly
- Directive detection and stripping: `src/main.rs:196`, `src/main.rs:213`

## Bytecode and VM

- Bytecode emitter compiles AST to `.indbc` and supports constants, globals, calls, prints, arithmetic (`src/bytecode.rs:1`)
- VM executes bytecode using a stack machine (`src/vm.rs:1`)
- Disassembler shows opcodes and constants (CLI `disassemble`)

## WebAssembly Backend

Current features (see `src/wasm.rs:1`):
- Imports layout under module `env`:
  - `print_f64(f64) -> ()` (type index 0)
  - `print_str(i32, i32) -> ()` (type index 1)
  - `alloc(i32) -> i32` (type index 3) for future dynamic strings
- Exports:
  - `main` function
  - `memory` for host access
- Data segments for string literals starting at offset 1024 (`src/wasm.rs:121`)
- Numeric expressions and printing; string literal printing via pointer/length
- Locals and assignments for numeric variables
- Functions:
  - Multiple numeric parameters
  - Calls within expressions and conditions
  - Return inference when last statement is a numeric expression
- Control flow: `if/else`, `while` compiled with block/loop constructs

CLI command to compile WebAssembly: `ind compile-wasm <in.adesh> <out.wasm>` (`src/main.rs:156`)

### Host Binding Notes

Host must supply imports:
- `env.print_f64`: print a `f64` value
- `env.print_str`: print a string from memory using `(ptr, len)`
- `env.alloc`: allocate memory and return a pointer

Memory is exported as `memory` and can be read/written from the host. String literals are embedded into memory via the data section.

## CLI

Commands (see `src/main.rs:17` for usage banner):
- `ind run <file.ind>` — interpret or compile/run depending on `@compile`
- `ind repl` — interactive REPL
- `ind init <dir>` — project scaffolding (creates `main.ind`, `utils.ind`) (`src/main.rs:21`)
- `ind compile <in.ind> <out.bin>` — emit `.indbc`
- `ind compile-wasm <in.ind> <out.wasm>` — emit `.wasm`
- `ind disassemble <out.bin>` — disassemble bytecode

## Examples and Tests

- Examples cover features, classes, async/promises, modules, and WASM demos under `examples/`
- Rust unit tests validate lexer, parser, runtime, type system, and WASM header correctness (`src/wasm.rs:438`)
- Python integration test invokes compiled binary for end-to-end checks (`tests/test.py`)

## Roadmap

Backend and Performance:
- Native AOT via Cranelift or LLVM for peak performance
- MIR/HIR pipelines enabling SSA, inlining, constant folding, DCE, loop optimizations
- VM improvements: inline caching, threaded dispatch, superinstructions

Type System and Safety:
- Gradual typing with inference and annotations
- Generics and trait/interface bounds
- Union/intersection, nullable types, flow-sensitive narrowing
- Monomorphization for optimized AOT code

Runtime and Concurrency:
- Structured concurrency primitives and channels with select
- Deterministic memory via arenas/regions; optional ARC/GC
- Expanded async runtime features: schedulers, timers, and I/O hooks

WebAssembly:
- Rich string and collection support via memory helpers and intrinsics
- Multi-type functions (`i32`, `f64`, booleans), proper boolean conditions
- System interfaces for host I/O, FS, network via imports

Tooling:
- CLI enhancements: `fmt`, `lint`, `test`, `bench`
- Language Server Protocol (LSP) for IDE features
- Benchmarks comparing interpreter, VM, WASM, and native AOT targets

## Getting Started

- Initialize a project: `ind init ./myproj` (creates `main.ind` and `utils.ind`)
- Run a program: `ind run examples/features.ind`
- Compile bytecode: `ind compile examples/features.ind examples/out.indbc`
- Compile WebAssembly: `ind compile-wasm examples/compile_wasm_fn.ind examples/out.wasm`

For directive-based compile, add `@compile` or `@compile wasm` to the first non-comment line of the file and use `ind run`.


---

## Source: semantics.md

# Adesh Language Semantics

> **Semantics** — Core semantic principles and execution model for AdeshLang
> Covers language philosophy, semantic vs syntactic focus, and execution guarantees.

---

## Language Philosophy

AdeshLang prioritizes **meaning over syntax**, **relationships over assignments**, and **deterministic execution** across all backends. The language is designed to be:

### Semantic-First, Not Instruction-First

Unlike imperative languages that focus on sequences of instructions, AdeshLang emphasizes:

1. **Intent over Instructions** - Express what you want, not how to do it
2. **Relationships over Assignments** - Define connections between values
3. **Deterministic Execution** - Same input always produces same output
4. **Backend-Agnostic Semantics** - Semantics remain consistent across execution modes

### Core Principles

#### 1. Meaning Over Syntax

AdeshLang values clear semantic intent over syntactic brevity:

```adesh
// Clear intent: what we're doing and why
let max_connections: u32 = 1000u32;
let timeout_seconds: f64 = 30.0;

// Type annotations make intent explicit
fn calculate_distance(x1: f64, y1: f64, x2: f64, y2: f64): f64 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    return Math.sqrt(dx * dx + dy * dy);
}
```

#### 2. Explicit Over Implicit

The language favors explicit operations that make behavior clear:

```adesh
// Explicit type conversion
let value: u8 = 255u8;
let larger: u32 = value as u32;  // Explicit cast

// Explicit memory management
let data = allocate_buffer(1024);  // Explicit allocation
// ... use data ...
// Automatic cleanup at scope end (deterministic)
```

#### 3. Safety by Design

AdeshLang incorporates safety checks without compromising performance:

- **Compile-time checks:** Type checking, borrow checking, lifetime analysis
- **Runtime checks:** Bounds checking, overflow detection, null safety
- **Memory safety:** Ownership and borrowing system (no garbage collector)

---

## Type System Semantics

### Static Typing with Inference

AdeshLang uses strong static typing with powerful type inference:

```adesh
// Explicit typing
let x: i32 = 42;

// Type inference
let y = 42;           // Inferred as Number (f64)
let z = 42i32;        // Inferred as i32

// Function return type inference
fn double(x: i32) -> i32 {
    x * 2
}
```

### Type Safety Guarantees

1. **No implicit conversions** - All type conversions must be explicit
2. **Overflow checking** - Numeric operations check for overflow
3. **Bounds checking** - Array accesses are bounds-checked
4. **No null pointers** - Use Option<T> for nullable values

```adesh
// Type safety in action
let small: u8 = 255u8;
let big: u32 = small as u32;   // OK: explicit widening cast

let value: i32 = 1000;
// let byte: u8 = value;        // ERROR: no implicit narrowing
let byte: u8 = value as u8;     // OK: explicit (with runtime check)
```

---

## Execution Model

### Deterministic Execution

**Core Guarantee:** Given the same inputs, a AdeshLang program always produces the same outputs, regardless of:
- Execution backend (interpreter, JIT, AOT)
- Optimization level
- Target platform

This guarantee applies to:
- Pure functions (no side effects)
- Deterministic I/O operations
- Parallel execution with proper synchronization

### Backend-Agnostic Semantics

AdeshLang semantics are defined independently of execution strategy:

```adesh
// This code has identical semantics across all backends
fn fibonacci(n: i64) -> i64 {
    if n <= 1 {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

// Works identically in:
// - Interpreter
// - JIT compiler
// - AOT compiler
// - WebAssembly
```

### Multi-Backend Execution

AdeshLang supports multiple execution backends with semantic consistency:

#### 1. Interpreter
- Direct AST/HIR interpretation
- Fastest startup time
- Best for development and debugging
- Full language feature support

#### 2. Bytecode VM
- Compiles to bytecode
- Balanced startup and execution speed
- Portable across platforms

#### 3. JIT Compilation
- Just-in-time native code generation
- Fast execution (10-20x faster than interpreter)
- Optimized for hot code paths

#### 4. Native JIT
- Immediate native code compilation
- Fastest execution (100-200x faster than interpreter)
- Optimal for compute-intensive workloads

#### 5. AOT Compilation
- Ahead-of-time native compilation
- Zero startup overhead
- Produces standalone executables

#### 6. WebAssembly
- Compile to WASM
- Run in browsers and WASM runtimes
- Near-native performance

---

## Memory Semantics

### Ownership-Based Memory Management

AdeshLang uses Rust-inspired ownership for deterministic, GC-free memory management:

#### Ownership Rules

1. **Each value has a single owner**
2. **Ownership can be transferred (moved)**
3. **Values are dropped when owner goes out of scope**
4. **No garbage collector** - deterministic cleanup

```adesh
fn example() {
    let s1 = "Hello";     // s1 owns the string
    let s2 = s1;          // Ownership moved to s2
    // print(s1);         // ERROR: s1 no longer valid
    print(s2);            // OK: s2 is the owner
}  // s2 dropped here (deterministic)
```

#### Borrowing

Temporary access without transferring ownership:

```adesh
// Immutable borrow
fn length(s: &String) -> i32 {
    s.len()  // Can read but not modify
}

// Mutable borrow
fn append(s: &mut String, suffix: &str) {
    s.push_str(suffix);  // Can modify
}

let text = "Hello";
let len = length(&text);    // Borrow text
append(&mut text, " World"); // Mutable borrow
```

### Memory Safety Guarantees

1. **No dangling pointers** - References are always valid
2. **No data races** - Mutable access is exclusive
3. **No memory leaks** (in safe code) - Deterministic cleanup
4. **No use-after-free** - Compile-time prevention

---

## Operator Semantics

### Arithmetic Operators

All arithmetic operations have well-defined overflow behavior:

```adesh
// Checked arithmetic (default)
let a: u8 = 255u8;
// let b = a + 1;    // Runtime error: overflow

// Explicit wrapping (when needed)
// let c = a.wrapping_add(1);  // Wraps to 0

// Saturating arithmetic
// let d = a.saturating_add(1);  // Saturates at 255
```

### Comparison Operators

Type-aware comparisons:

```adesh
// Numeric comparison
let a = 10;
let b = 20;
let result = a < b;  // true

// Strict equality (type and value)
let x = 42;
let y = 42.0;
// let eq = x === y;  // false (different types)
let eq2 = x == y;     // true (numeric equality)
```

### Logical Operators

Short-circuit evaluation:

```adesh
// && short-circuits
let result = false && expensive_computation();  // expensive_computation not called

// || short-circuits  
let result = true || expensive_computation();   // expensive_computation not called
```

---

## Function Semantics

### Pure Functions

Functions with no side effects:

```adesh
// Pure function - always returns same output for same input
fn square(x: i32) -> i32 {
    x * x
}

// Purity enables:
// - Memoization
// - Parallel execution
// - Aggressive optimization
```

### Side Effects

Functions can perform I/O and mutations:

```adesh
// Impure function - has side effects
fn write_log(message: &str) {
    print(message);  // I/O side effect
}
```

### First-Class Functions

Functions are values that can be passed around:

```adesh
// Function as value
let operation = fn(x: i32, y: i32) -> i32 { x + y };
let result = operation(5, 3);  // 8

// Higher-order functions
fn apply_twice(f: fn(i32) -> i32, x: i32) -> i32 {
    f(f(x))
}

let double = fn(x: i32) -> i32 { x * 2 };
let result = apply_twice(double, 3);  // 12
```

---

## Control Flow Semantics

### Expressions vs Statements

In AdeshLang, most constructs are expressions that produce values:

```adesh
// if is an expression
let max = if a > b { a } else { b };

// match is an expression
let description = match value {
    0 => "zero",
    1 => "one",
    _ => "many"
};
```

### Pattern Matching

Exhaustive pattern matching:

```adesh
// Compiler ensures all cases are handled
match option {
    Some(x) => print(x),
    None => print("no value")
}

// Non-exhaustive match is a compile error
// match option {
//     Some(x) => print(x)
// }  // ERROR: non-exhaustive pattern
```

### Loops

Deterministic loop behavior:

```adesh
// For loop
for i in 0..10 {
    print(i);
}

// While loop
let mut count = 0;
while count < 10 {
    count += 1;
}

// Break and continue
for i in 0..100 {
    if i % 2 == 0 { continue; }
    if i > 50 { break; }
    print(i);
}
```

---

## Error Handling Semantics

### Result Type

Errors are values, not exceptions:

```adesh
// Function that can fail
fn divide(a: f64, b: f64) -> Result<f64, string> {
    if b == 0.0 {
        return Err("division by zero");
    }
    return Ok(a / b);
}

// Handling errors
match divide(10.0, 2.0) {
    Ok(result) => print(result),
    Err(error) => print("Error: " + error)
}
```

### Option Type

Null safety through Option<T>:

```adesh
// Nullable value
let maybe_value: Option<i32> = Some(42);

// Safe access
match maybe_value {
    Some(x) => print(x),
    None => print("no value")
}

// Chaining operations
let result = maybe_value
    .map(|x| x * 2)
    .unwrap_or(0);
```

---

## Concurrency Semantics

### Data Race Freedom

The type system prevents data races at compile time:

```adesh
// Shared immutable access is safe
let data = vec![1, 2, 3];
let handle1 = spawn(|| read_data(&data));
let handle2 = spawn(|| read_data(&data));

// Exclusive mutable access
let mut data = vec![1, 2, 3];
// let handle = spawn(|| data.push(4));  // ERROR: would cause data race
```

### Message Passing

Safe concurrent communication:

```adesh
let channel = Channel::new();
let sender = channel.sender();
let receiver = channel.receiver();

spawn(move || {
    sender.send(42);
});

let value = receiver.recv();  // Blocks until message arrives
```

---

## Module System Semantics

### Explicit Imports

All dependencies must be explicitly imported:

```adesh
// Import specific items
use std::collections::Vec;
use std::io::print;

// Import entire module
use std::math::*;
```

### Visibility Rules

Module contents are private by default:

```adesh
// Private by default
fn internal_helper() { }

// Explicit public
pub fn public_api() { }

// Public type with private field
pub struct Point {
    pub x: f64,
    pub y: f64,
    private_id: u32  // Not accessible outside module
}
```

---

## Compile-Time Guarantees

AdeshLang provides strong compile-time guarantees:

1. **Type safety** - No type errors at runtime
2. **Memory safety** - No use-after-free, no dangling pointers
3. **Thread safety** - No data races
4. **Exhaustiveness** - All cases in match handled
5. **Lifetime safety** - All references are valid

These guarantees apply across all execution backends and optimization levels.

---

## Runtime Behavior

### Panic vs Error

- **Errors** (Result/Option): Expected failures, handle gracefully
- **Panics**: Unexpected failures, program cannot continue

```adesh
// Error: expected failure case
fn parse_int(s: string) -> Result<i32, string> {
    // ... parsing logic
}

// Panic: unexpected failure (bug in code)
fn get_element(array: [i32], index: usize) -> i32 {
    if index >= array.len() {
        panic("index out of bounds");  // Should never happen in correct code
    }
    return array[index];
}
```

### Debug vs Release Builds

- **Debug builds:** All checks enabled, better error messages
- **Release builds:** Optimizations enabled, minimal runtime checks (only safety-critical)

---

## Future Semantic Extensions

Planned semantic features for future versions:

1. **Effect system** - Track side effects in type system
2. **Linear types** - Guarantee single use of resources
3. **Dependent types** - Types that depend on values
4. **Proof carrying code** - Machine-verifiable correctness

---

## Related Documentation

- [TYPE_SYSTEM.md](TYPE_SYSTEM.md) - Complete type system reference
- [memory_model.md](memory_model.md) - Memory ownership and borrowing
- [literals.md](literals.md) - Numeric literal formats
- [functions.md](functions.md) - Function definitions and semantics

---

*Last updated: February 2026*


---

## Source: functions.md

# Functions in AdeshLang

> **Functions** — Comprehensive guide to function definition, calling, and advanced features
> Covers basic functions, parameters, return types, closures, higher-order functions, and async operations.

---

## Overview

Functions are first-class values in AdeshLang, supporting:
- Named and anonymous functions
- Type annotations for parameters and return values
- Default parameters and variadic arguments
- Closures with automatic capture
- Higher-order functions
- Async/await for asynchronous operations
- Consistent behavior across all execution backends

---

## Basic Function Syntax

### Function Declaration

```adesh
fn function_name(param1: Type1, param2: Type2): ReturnType {
    // function body
    return value;
}
```

### Simple Example

```adesh
fn add(a: i64, b: i64): i64 {
    return a + b;
}

let result = add(5, 3);  // 8
```

### Without Return Type Annotation

```adesh
fn greet(name: string) {
    print("Hello, " + name + "!");
}

greet("World");  // Prints: Hello, World!
```

---

## Parameters

### Required Parameters

```adesh
fn multiply(x: i64, y: i64): i64 {
    return x * y;
}

multiply(4, 5);  // 20
```

### Default Parameters

```adesh
fn power(base: i64, exponent: i64 = 2): i64 {
    let result: i64;
    result = 1;
    let i: i64;
    i = 0;
    while i < exponent {
        result = result * base;
        i = i + 1;
    }
    return result;
}

power(3);     // 9 (3^2)
power(3, 3);  // 27 (3^3)
```

### Rest Parameters (Variadic)

```adesh
fn sum_all(...numbers): i64 {
    let total: i64;
    total = 0;
    for num in numbers {
        total = total + num;
    }
    return total;
}

sum_all(1, 2, 3, 4, 5);  // 15
```

---

## Return Values

### Explicit Return

```adesh
fn factorial(n: i64): i64 {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}
```

### Implicit Return (Expression-bodied)

```adesh
fn square(x: i64): i64 {
    x * x  // Last expression is returned
}
```

### Multiple Return Values (via Tuple)

```adesh
fn divide_with_remainder(dividend: i64, divisor: i64): (i64, i64) {
    let quotient = dividend / divisor;
    let remainder = dividend % divisor;
    return (quotient, remainder);
}

let (q, r) = divide_with_remainder(17, 5);  // q=3, r=2
```

### Early Returns

```adesh
fn find_first_positive(numbers: [i64]): Option<i64> {
    for num in numbers {
        if num > 0 {
            return Some(num);  // Early return
        }
    }
    return None;
}
```

---

## Anonymous Functions (Lambdas)

### Function Literal Syntax

```adesh
// Assigned to variable
let double = fn(x: i64): i64 {
    return x * 2;
};

double(5);  // 10
```

### Arrow Function (Concise Syntax)

```adesh
let triple = fn(x: i64): i64 => x * 3;

triple(4);  // 12
```

### Inline Anonymous Function

```adesh
// Used directly without assignment
let result = (fn(a: i64, b: i64): i64 => a + b)(10, 20);  // 30
```

---

## Closures

Functions can capture variables from their enclosing scope:

### Basic Closure

```adesh
fn make_counter(): fn(): i64 {
    let count: i64;
    count = 0;
    
    return fn(): i64 {
        count = count + 1;
        return count;
    };
}

let counter = make_counter();
print(counter());  // 1
print(counter());  // 2
print(counter());  // 3
```

### Closure with Parameters

```adesh
fn make_adder(x: i64): fn(i64): i64 {
    return fn(y: i64): i64 {
        return x + y;  // Captures x
    };
}

let add5 = make_adder(5);
print(add5(10));  // 15
print(add5(20));  // 25
```

### Multiple Captures

```adesh
fn create_multiplier(factor: i64, offset: i64): fn(i64): i64 {
    return fn(value: i64): i64 {
        return value * factor + offset;  // Captures factor and offset
    };
}

let transform = create_multiplier(3, 10);
print(transform(5));  // 25 (5 * 3 + 10)
```

---

## Higher-Order Functions

Functions that take other functions as parameters or return functions:

### Function as Parameter

```adesh
fn apply_twice(f: fn(i64): i64, x: i64): i64 {
    return f(f(x));
}

fn increment(n: i64): i64 {
    return n + 1;
}

print(apply_twice(increment, 5));  // 7
```

### Map Operation

```adesh
fn map(array: [i64], transformer: fn(i64): i64): [i64] {
    let result = [];
    for element in array {
        result.push(transformer(element));
    }
    return result;
}

let numbers = [1, 2, 3, 4, 5];
let doubled = map(numbers, fn(x: i64): i64 => x * 2);
// doubled = [2, 4, 6, 8, 10]
```

### Filter Operation

```adesh
fn filter(array: [i64], predicate: fn(i64): bool): [i64] {
    let result = [];
    for element in array {
        if predicate(element) {
            result.push(element);
        }
    }
    return result;
}

let numbers = [1, 2, 3, 4, 5, 6];
let evens = filter(numbers, fn(x: i64): bool => x % 2 == 0);
// evens = [2, 4, 6]
```

### Reduce Operation

```adesh
fn reduce(array: [i64], accumulator: fn(i64, i64): i64, initial: i64): i64 {
    let result = initial;
    for element in array {
        result = accumulator(result, element);
    }
    return result;
}

let numbers = [1, 2, 3, 4, 5];
let sum = reduce(numbers, fn(acc: i64, x: i64): i64 => acc + x, 0);
// sum = 15
```

---

## Recursive Functions

### Direct Recursion

```adesh
fn fibonacci(n: i64): i64 {
    if n <= 1 {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

print(fibonacci(10));  // 55
```

### Tail Recursion

```adesh
fn factorial_tail(n: i64, accumulator: i64 = 1): i64 {
    if n <= 1 {
        return accumulator;
    }
    return factorial_tail(n - 1, n * accumulator);
}

print(factorial_tail(5));  // 120
```

**Optimization:** Use `--tco` flag for tail-call optimization:
```bash
adesh run --tco program.adesh
```

### Mutual Recursion

```adesh
fn is_even(n: i64): bool {
    if n == 0 {
        return true;
    }
    return is_odd(n - 1);
}

fn is_odd(n: i64): bool {
    if n == 0 {
        return false;
    }
    return is_even(n - 1);
}

print(is_even(4));  // true
print(is_odd(4));   // false
```

---

## Async Functions

### Basic Async Function

```adesh
async fn fetch_data(url: string): Promise<string> {
    // Simulated async operation
    await sleep(1000);
    return "Data from " + url;
}

// Usage
async fn main() {
    let data = await fetch_data("https://example.com");
    print(data);
}
```

### Parallel Async Operations

```adesh
async fn fetch_multiple(): [string] {
    let promise1 = fetch_data("url1");
    let promise2 = fetch_data("url2");
    let promise3 = fetch_data("url3");
    
    // Wait for all
    let results = await Promise.all([promise1, promise2, promise3]);
    return results;
}
```

### Error Handling in Async

```adesh
async fn safe_fetch(url: string): Result<string, string> {
    try {
        let data = await fetch_data(url);
        return Ok(data);
    } catch error {
        return Err("Failed to fetch: " + error);
    }
}
```

---

## Function Overloading

AdeshLang supports type-based function overloading:

```adesh
fn process(value: i64): string {
    return "Integer: " + string(value);
}

fn process(value: string): string {
    return "String: " + value;
}

fn process(value: f64): string {
    return "Float: " + string(value);
}

print(process(42));      // "Integer: 42"
print(process("hello")); // "String: hello"
print(process(3.14));    // "Float: 3.14"
```

---

## Pure Functions

Functions without side effects are automatically detected:

```adesh
fn pure_calculation(x: i64, y: i64): i64 {
    // No side effects - pure function
    return x * x + y * y;
}

// Compiler can:
// - Memoize results
// - Reorder calls
// - Execute in parallel
// - Optimize aggressively
```

### Marking Pure Functions

```adesh
// Explicit pure annotation (future feature)
fn @pure distance(x1: f64, y1: f64, x2: f64, y2: f64): f64 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    return Math.sqrt(dx * dx + dy * dy);
}
```

---

## Function Composition

### Manual Composition

```adesh
fn compose(f: fn(i64): i64, g: fn(i64): i64): fn(i64): i64 {
    return fn(x: i64): i64 {
        return f(g(x));
    };
}

fn add10(x: i64): i64 { return x + 10; }
fn double(x: i64): i64 { return x * 2; }

let transform = compose(add10, double);
print(transform(5));  // 20 (double(5) = 10, then add10(10) = 20)
```

### Pipeline Operator (Future)

```adesh
// Proposed syntax
let result = value
    |> transform1
    |> transform2
    |> transform3;
```

---

## Generic Functions

### Type Parameters

```adesh
fn identity<T>(value: T): T {
    return value;
}

print(identity<i64>(42));      // 42
print(identity<string>("hi")); // "hi"
```

### Generic with Constraints

```adesh
fn max<T: Ord>(a: T, b: T): T {
    if a > b {
        return a;
    }
    return b;
}

print(max<i64>(5, 10));        // 10
print(max<string>("a", "z"));  // "z"
```

---

## Method Syntax

Functions can be called as methods on objects:

```adesh
class Point {
    x: f64,
    y: f64,
    
    fn distance_from_origin(): f64 {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }
    
    fn distance_to(other: Point): f64 {
        let dx = this.x - other.x;
        let dy = this.y - other.y;
        return Math.sqrt(dx * dx + dy * dy);
    }
}

let p1 = Point { x: 3.0, y: 4.0 };
let p2 = Point { x: 0.0, y: 0.0 };

print(p1.distance_from_origin());  // 5.0
print(p1.distance_to(p2));          // 5.0
```

---

## Backend-Specific Behavior

### Consistent Semantics

All backends execute functions with identical semantics:

```adesh
fn compute(x: i64): i64 {
    return x * x + 2 * x + 1;
}

// Same result in:
// - Interpreter
// - Bytecode VM
// - JIT
// - Native JIT
// - AOT compilation
```

### Performance Characteristics

| Backend | Function Call Overhead | Optimization Level |
|---------|----------------------|-------------------|
| Interpreter | High | None |
| Bytecode VM | Medium | Minimal |
| JIT | Low | Medium |
| Native JIT | Very Low | High |
| AOT | Minimal | Maximum |

### Backend Selection

```bash
# Interpreter (default)
adesh run program.adesh

# JIT compilation
adesh run --jit program.adesh

# Native JIT (fastest)
adesh run --njit program.adesh

# AOT compilation
adesh build program.adesh
./program
```

---

## Function Optimization

### Inline Functions

Small functions are automatically inlined:

```adesh
fn small_helper(x: i64): i64 {
    return x + 1;  // Likely inlined
}
```

### Memoization

Enable memoization for pure functions:

```adesh
// Run with --memo flag
adesh run --memo program.adesh

fn expensive_calculation(n: i64): i64 {
    // Results are cached automatically
    return fibonacci(n);
}
```

### Tail-Call Optimization

```bash
# Enable TCO
adesh run --tco program.adesh

fn sum_range(n: i64, acc: i64 = 0): i64 {
    if n == 0 {
        return acc;
    }
    return sum_range(n - 1, acc + n);  // Tail call - optimized
}
```

---

## Best Practices

### 1. Use Type Annotations

```adesh
// Good: Clear types
fn calculate(a: i64, b: i64): i64 {
    return a * b;
}

// Avoid: Missing types (less clear)
fn calculate(a, b) {
    return a * b;
}
```

### 2. Keep Functions Small and Focused

```adesh
// Good: Single responsibility
fn validate_email(email: string): bool {
    return email.contains("@") && email.contains(".");
}

fn send_email(to: string, subject: string, body: string) {
    if !validate_email(to) {
        return Err("Invalid email");
    }
    // Send email logic
}
```

### 3. Prefer Pure Functions

```adesh
// Good: Pure function
fn calculate_total(items: [f64]): f64 {
    let sum = 0.0;
    for item in items {
        sum = sum + item;
    }
    return sum;
}

// Avoid: Side effects
let global_total = 0.0;
fn add_to_global(value: f64) {
    global_total = global_total + value;  // Mutates global state
}
```

### 4. Document Complex Functions

```adesh
/**
 * Calculates the greatest common divisor using Euclid's algorithm.
 * 
 * @param a First integer
 * @param b Second integer
 * @return The GCD of a and b
 */
fn gcd(a: i64, b: i64): i64 {
    if b == 0 {
        return a;
    }
    return gcd(b, a % b);
}
```

---

## Error Handling in Functions

### Using Result Type

```adesh
fn divide(a: f64, b: f64): Result<f64, string> {
    if b == 0.0 {
        return Err("Division by zero");
    }
    return Ok(a / b);
}

// Usage
match divide(10.0, 2.0) {
    Ok(result) => print("Result: " + string(result)),
    Err(error) => print("Error: " + error)
}
```

### Using Option Type

```adesh
fn find_index(array: [i64], target: i64): Option<i64> {
    let i: i64;
    i = 0;
    for element in array {
        if element == target {
            return Some(i);
        }
        i = i + 1;
    }
    return None;
}
```

---

## Advanced Patterns

### Currying

```adesh
fn add(a: i64): fn(i64): i64 {
    return fn(b: i64): i64 {
        return a + b;
    };
}

let add5 = add(5);
print(add5(10));  // 15
```

### Partial Application

```adesh
fn multiply(a: i64, b: i64, c: i64): i64 {
    return a * b * c;
}

// Create partially applied function
fn multiply_by_2_and_3(x: i64): i64 {
    return multiply(2, 3, x);
}

print(multiply_by_2_and_3(4));  // 24
```

### Function Memoization (Manual)

```adesh
fn memoize(f: fn(i64): i64): fn(i64): i64 {
    let cache = {};
    return fn(x: i64): i64 {
        if cache.has(x) {
            return cache[x];
        }
        let result = f(x);
        cache[x] = result;
        return result;
    };
}

let fib_memoized = memoize(fibonacci);
```

---

## Related Documentation

- [semantics.md](semantics.md) - Language semantics and principles
- [backends.md](backends.md) - Execution backend details
- [TYPE_SYSTEM.md](TYPE_SYSTEM.md) - Type system reference
- [ASYNC_IMPLEMENTATION.md](ASYNC_IMPLEMENTATION.md) - Async/await details

---

## Examples

Find practical function examples in:
- `examples/functions/` - Function usage patterns
- `examples/jit/` - Performance optimization examples
- `examples/async/` - Async function examples
- `examples/recursion/` - Recursive function patterns

---

*Last updated: February 2026*


---

## Source: error_model.md

# AdeshLang Error Model

This document describes the unified error model in AdeshLang v0.3, providing consistent error structures across all execution backends.

---

## Table of Contents

- [Overview](#overview)
- [Error Structure](#error-structure)
- [Error Categories](#error-categories)
- [Backend-Specific Errors](#backend-specific-errors)
- [Error Formatting](#error-formatting)
- [Recovery and Suggestions](#recovery-and-suggestions)

---

## Overview

AdeshLang v0.3 introduces a unified error model that provides consistent error reporting across:
- Interpreter
- JIT compiler
- Bytecode VM
- WASM lowering
- Type system
- Borrow checker

All errors follow the same structure and formatting, making it easier for developers to understand and fix issues.

---

## Error Structure

### Core Error Type

Every error in AdeshLang follows this structure:

```rust
pub struct AdeshError {
    /// Error category/kind
    pub kind: ErrorKind,
    
    /// Human-readable error message
    pub message: String,
    
    /// Source location where error occurred
    pub span: Option<Span>,
    
    /// Additional context and suggestions
    pub notes: Vec<Note>,
    
    /// Severity level
    pub severity: Severity,
}

pub struct Span {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub length: u32,
}

pub struct Note {
    pub message: String,
    pub span: Option<Span>,
    pub kind: NoteKind,
}

pub enum NoteKind {
    Help,
    Hint,
    Info,
    Related,
}

pub enum Severity {
    Error,
    Warning,
    Info,
}
```

### JSON Representation

For LSP and tooling, errors serialize to JSON:

```json
{
  "kind": "BorrowConflict",
  "message": "cannot borrow `x` as mutable because it is already borrowed as immutable",
  "span": {
    "file": "example.adesh",
    "line": 10,
    "column": 5,
    "length": 8
  },
  "notes": [
    {
      "message": "immutable borrow occurs here",
      "span": { "file": "example.adesh", "line": 8, "column": 9, "length": 1 },
      "kind": "Related"
    },
    {
      "message": "consider using a different variable or scope",
      "kind": "Help"
    }
  ],
  "severity": "Error"
}
```

---

## Error Categories

### ErrorKind Enum

```rust
pub enum ErrorKind {
    // Parse errors
    SyntaxError,
    UnexpectedToken,
    UnterminatedString,
    
    // Type errors
    TypeMismatch,
    UndefinedVariable,
    UndefinedFunction,
    UndefinedType,
    InvalidCast,
    
    // Borrow errors
    BorrowConflict,
    UseAfterMove,
    InvalidBorrow,
    LifetimeError,
    EscapeError,
    
    // Runtime errors
    NullPointerAccess,
    IndexOutOfBounds,
    DivisionByZero,
    StackOverflow,
    
    // Visibility errors
    PrivateAccess,
    ProtectedAccess,
    
    // Memory errors
    AllocationFailed,
    WeakUpgradeFailed,
    DoubleFree,
    
    // IO errors
    FileNotFound,
    PermissionDenied,
    PathTraversal,
    SSRFBlocked,
    
    // JIT errors
    CompilationFailed,
    DeoptimizationRequired,
    ICMiss,
    
    // VM errors
    InvalidOpcode,
    InvalidBytecode,
    
    // WASM errors
    WasmLoweringFailed,
    UnsupportedFeature,
}
```

---

## Backend-Specific Errors

### Interpreter Errors

```
error[E0001]: undefined variable `foo`
  --> example.adesh:5:10
   |
 5 |     print(foo);
   |           ^^^ not found in this scope
   |
   = help: consider declaring `foo` with `let foo = ...`
```

### JIT Errors

```
error[E0100]: JIT compilation failed
  --> example.adesh:15:1
   |
15 | fn hot_function(x) {
   | ^^^^^^^^^^^^^^^^^^ function too complex for current tier
   |
   = note: function has 500 instructions, tier-1 limit is 256
   = help: split function or use interpreter mode
```

### VM Errors

```
error[E0200]: index out of bounds
  --> example.adesh:10:5
   |
10 |     arr[100]
   |     ^^^^^^^^ index 100 is out of bounds for array of length 5
   |
   = note: valid indices are 0..4
```

### Borrow Checker Errors

```
error[E0300]: cannot borrow `data` as mutable
  --> example.adesh:8:9
   |
 6 |     let ref1 = &data;
   |                ----- immutable borrow occurs here
 7 |     
 8 |     let ref2 = &mut data;
   |         ^^^^ mutable borrow attempted here
 9 |     
10 |     print(ref1);
   |           ---- immutable borrow later used here
   |
   = help: consider removing the mutable borrow or ending the immutable borrow earlier
```

### Type System Errors

```
error[E0400]: type mismatch
  --> example.adesh:3:12
   |
 3 |     let x: Number = "hello";
   |            ^^^^^^   ------- this is a String
   |            |
   |            expected Number
   |
   = note: union type Number | String would allow both
```

---

## Error Formatting

### Terminal Output

Errors are formatted with ANSI colors for terminal display:

```
┌─ Color Coding ──────────────────────────────┐
│  Red:     Error indicators (^^^)            │
│  Yellow:  Warnings                          │
│  Blue:    Notes and info                    │
│  Green:   Suggestions/help                  │
│  Cyan:    Line numbers                      │
│  White:   Source code                       │
└─────────────────────────────────────────────┘
```

### Format Template

```
{severity}[{code}]: {message}
  --> {file}:{line}:{column}
   |
{line_num} | {source_line}
   |  {caret_indicator}
   |
   = {note_kind}: {note_message}
```

---

## Recovery and Suggestions

### Automatic Suggestions

The error model includes actionable suggestions:

| Error | Suggestion |
|-------|------------|
| `UndefinedVariable` | "consider declaring with `let`" |
| `BorrowConflict` | "end the borrow before reuse" |
| `TypeMismatch` | "use union type or explicit cast" |
| `PrivateAccess` | "use a getter method" |
| `IndexOutOfBounds` | "check length before access" |

### Error Codes

All errors have unique codes for documentation reference:

| Range | Category |
|-------|----------|
| E0001-E0099 | Syntax/Parse errors |
| E0100-E0199 | JIT errors |
| E0200-E0299 | VM errors |
| E0300-E0399 | Borrow errors |
| E0400-E0499 | Type errors |
| E0500-E0599 | Visibility errors |
| E0600-E0699 | IO/Security errors |
| E0700-E0799 | WASM errors |

---

## Implementation

### Creating Errors

```rust
// Simple error
let err = AdeshError::new(
    ErrorKind::UndefinedVariable,
    format!("undefined variable `{}`", name),
)
.with_span(span)
.with_note(Note::help("consider declaring the variable"));

// Borrow conflict with related spans
let err = AdeshError::new(
    ErrorKind::BorrowConflict,
    format!("cannot borrow `{}` as mutable", name),
)
.with_span(mutable_borrow_span)
.with_note(Note::related("immutable borrow occurs here", immutable_span))
.with_note(Note::help("consider using separate scopes"));
```

### Error Display

```rust
impl Display for AdeshError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // Format with colors if terminal supports it
        if supports_color() {
            self.format_colored(f)
        } else {
            self.format_plain(f)
        }
    }
}
```

---

## Related Documentation

- [VM Design](vm_design.md)
- [Memory Model](memory_model.md)
- [Borrow Checking](../examples/borrow/borrow_rules.adesh)

---

*Last Updated: December 2024 - AdeshLang v0.3*


---

## Source: examples.md

# AdeshLang Examples Index

This document provides a comprehensive index of all example files in the AdeshLang project, organized by category with descriptions and usage notes.

---

## Table of Contents

- [Memory Model Examples](#memory-model-examples)
- [Type System Examples](#type-system-examples)
- [Borrow Checking Examples](#borrow-checking-examples)
- [JIT Optimization Examples](#jit-optimization-examples)
- [Async Programming Examples](#async-programming-examples)
- [Object-Oriented Programming](#object-oriented-programming)
- [Recursion & Memoization](#recursion--memoization)
- [Data Structures](#data-structures)
- [Concurrency](#concurrency)
- [GPU Computing](#gpu-computing)
- [WebAssembly](#webassembly)
- [Utilities & Misc](#utilities--misc)

---

## Memory Model Examples

Located in `examples/memory/`

| File | Description |
|------|-------------|
| `shared_unique_weak.adesh` | Demonstrates smart pointer types: Shared<T> for reference counting, Unique<T> for move-only ownership, Weak<T> for breaking cycles. Shows clone(), move(), upgrade() operations. |
| `sso_sao_examples.adesh` | Shows Small String Optimization (SSO) for strings under 22 bytes and Small Array Optimization (SAO) for arrays under 8 elements. Includes memory usage comparisons. |

### What These Demonstrate:
- Reference counting with Shared<T>
- Move semantics with Unique<T>
- Cycle breaking with Weak<T>
- Debug poisoning behavior (conceptual)
- SSO threshold and performance benefits
- SAO for small collections
- Memory allocation patterns

### Memory Model Notes:
- Shared pointers use atomic reference counting
- Unique pointers transfer ownership on move
- Weak references don't prevent deallocation
- SSO avoids heap allocation for short strings
- SAO keeps small arrays inline

---

## Type System Examples

Located in `examples/types/`

| File | Description |
|------|-------------|
| `test_numeric_literals.adesh` | Consolidated type-system showcase covering numeric literal formats, fixed-width floats, `type`/`typeof`, object syntax, `hasKey`, union/nullable handling, and control-flow narrowing. |

### What These Demonstrate:
- Numeric literal formats and arithmetic edge cases
- Fixed-width float assignment and inference
- `type` / `typeof` checks and control-flow narrowing
- Object literals with numeric keys and nested objects
- `hasKey` and `in` membership checks
- Nullable handling and safe access patterns
- Branching edge cases with `if` / `elif`

### Type System Notes:
- Union types allow multiple possible types for a value
- Nullable types must be checked before access
- Type narrowing refines types in conditional branches
- Visibility is enforced at runtime

---

## Borrow Checking Examples

Located in `examples/borrow/`

| File | Description |
|------|-------------|
| `borrow_rules.adesh` | Shows valid and invalid borrow patterns: shared borrows, mutable borrows, closure capture rules, and how to structure code for borrow safety. |

### What These Demonstrate:
- Multiple immutable borrows (allowed)
- Single mutable borrow (allowed)
- Mutable + immutable conflict (error)
- Closure capture by reference
- Closure capture by move
- Return ownership patterns

### Borrow Rules Summary:
1. Multiple immutable borrows are allowed simultaneously
2. Only one mutable borrow at a time
3. Cannot have mutable and immutable borrows simultaneously
4. Closures capture by reference by default
5. Use `move` keyword for ownership transfer

---

## JIT Optimization Examples

Located in `examples/jit/`

| File | Description |
|------|-------------|
| `jit_optimizations.adesh` | Comprehensive demo of JIT features: hidden classes, inline caching (IC), type specialization, constant folding, and fast paths for primitives. |
| `benchmark.adesh` | Performance benchmarks comparing interpreter vs JIT modes. |
| `tiered_demo.adesh` | Demonstrates T0→T1→T2 tiered compilation with automatic tier promotion. |
| `adaptive_demo.adesh` | Shows adaptive JIT with speculative optimization and deoptimization. |

### What These Demonstrate:
- Hidden class creation and transitions
- Monomorphic and polymorphic IC behavior
- Integer/float/string fast paths
- Constant folding at compile time
- JIT performance improvements
- Tier promotion mechanics

### JIT Performance Tips:
- Keep object shapes consistent for hidden class optimization
- Avoid type changes at same call sites for better IC hit rates
- Use `const` for compile-time constant folding
- Run with `--jit` for compute-intensive code

---

## Async Programming Examples

Located in `examples/async/`

| File | Description |
|------|-------------|
| `async_safe_closure.adesh` | Safe async patterns with closures under lifetime rules. Shows cancellation-safe timers, ownership across await points, and promise chains with closure state. |
| `simple_promise.adesh` | Basic Promise creation and resolution. |
| `async_fn_test.adesh` | Testing async function declarations and await. |

### What These Demonstrate:
- Safe closure capturing in async contexts
- Cancellation-safe timer patterns
- Promise chain with preserved state
- Async iterator patterns
- Ownership transfer across await

### Async Safety Notes:
- Values survive across await points
- Use cancellation flags for interruptible work
- Closures capture references safely in async contexts

---

## Object-Oriented Programming

Located in `examples/oop/`

| File | Description |
|------|-------------|
| `abstract_classes_demo.adesh` | Abstract classes, interfaces, and implementation patterns. |
| `class.adesh` | Basic class definition and instantiation. |
| `inheritance_test.adesh` | Single inheritance with method overriding. |
| `visibility_demo.adesh` | Public, private, protected visibility enforcement. |
| `oop_test_suite.adesh` | Comprehensive OOP feature tests. |
| `enhanced_oop.adesh` | Advanced OOP patterns and idioms. |

### What These Demonstrate:
- Class definition and constructors
- Inheritance and super calls
- Interface implementation
- Abstract methods
- Static members
- Visibility enforcement

---

## Recursion & Memoization

Located in `examples/recursion/` and `examples/fib/`

| File | Description |
|------|-------------|
| `recursion/naive_fib.adesh` | Naive recursive Fibonacci (demonstrates memoization benefit). |
| `recursion/dp_fib.adesh` | Dynamic programming Fibonacci. |
| `recursion/tail_fib.adesh` | Tail-recursive Fibonacci for TCO. |
| `recursion/factorial.adesh` | Factorial with various optimizations. |
| `recursion/gcd.adesh` | Greatest common divisor implementations. |

### Optimization Flags:
```bash
--fast-recursion    # Enable all recursion optimizations
--memo              # Enable memoization only
--tco               # Enable tail call optimization only
```

---

## Data Structures

Located in `examples/data_structures/`

| File | Description |
|------|-------------|
| `linked_list.adesh` | Linked list implementation. |
| `binary_tree.adesh` | Binary tree with traversals. |
| `queue.adesh` | Queue implementation. |
| `stack.adesh` | Stack implementation. |

---

## Concurrency

Located in `examples/concurrency/`

| File | Description |
|------|-------------|
| `channel.adesh` | Message passing with channels. |
| `parallel_map.adesh` | Parallel map operations. |

---

## GPU Computing

Located in `examples/gpu/`

| File | Description |
|------|-------------|
| `matmul_gpu.adesh` | Matrix multiplication on GPU. |
| `reduction.adesh` | Parallel reduction operations. |
| `device_info.adesh` | Query GPU device information. |

### Device Flags:
```bash
--device=cuda       # NVIDIA GPU
--device=metal      # Apple GPU
--device=vulkan     # Cross-platform GPU
```

---

## WebAssembly

Located in `examples/wasm/`

| File | Description |
|------|-------------|
| `wasm_basic.adesh` | Basic WASM compilation. |
| `wasm_functions.adesh` | Function export/import. |
| `wasm_control.adesh` | Control flow in WASM. |

---

## Utilities & Misc

Located in `examples/misc/` and `examples/utils/`

| File | Description |
|------|-------------|
| `misc/basic.adesh` | Basic language features demo. |
| `misc/features.adesh` | Feature showcase. |
| `utils/util_demo.adesh` | Utility function usage. |

---

## Running Examples

### Basic Execution
```bash
adesh run examples/memory/shared_unique_weak.adesh
```

### With JIT Compilation
```bash
adesh run examples/jit/jit_optimizations.adesh --jit
```

### With Verbose Output
```bash
adesh run examples/borrow/borrow_rules.adesh --verbose
```

### With Memory Statistics
```bash
adesh run examples/memory/sso_sao_examples.adesh --memory
```

---

## Related Documentation

- [Memory Model Details](memory_model.md)
- [Language Reference](language.md)
- [CLI Reference](cli.md)
- [Async Implementation](ASYNC_IMPLEMENTATION.md)

---

*Last Updated: December 2024*


---

## Source: warnings.md

# AdeshLang Warning System & Suppression Guide

The AdeshLang compiler and runtime include a static analysis diagnostic engine. Warnings are formatted with precise line and column numbers, allowing terminals and IDEs (VSCode, Cursor, Windows Terminal) to render them as direct clickable links.

---

## 1. Warning Output Format

Warnings follow a standard Rust-like diagnostic format:

```text
warning: path/to/file.adesh:line:col: message
```

### Examples

```text
warning: examples/Libraries/http/master_http_showcase.adesh:56:43: unused variable `reqBuilder`
warning: examples/Libraries/http/master_http_showcase.adesh:79:5: unused parameter `req`
warning: src/main.adesh:12:1: 'JSON' is a built-in global library and does not need to be imported
```

---

## 2. Disabling Warnings in Source Code

You can control warning reporting directly in your source code using comment directives or identifier conventions.

### 2.1 File-Level Directives

To suppress all warnings (or specific warning categories) across an entire file, add any of the following directives near the top of the file (within the first 30 lines):

```adesh
// @allow(warnings)
// @allow(unused)
// adesh-allow: warnings
// adesh-allow: unused
// adesh-disable-warnings
```

#### File-Level Directive Example

```adesh
// @allow(warnings)

import HTTP;

fn main() {
    let client = HTTP.ClientBuilder().build(); // No warning emitted for unused 'client'
}
```

---

### 2.2 Line-Level Directives

To suppress warnings for a specific declaration or line of code, place a directive comment on the line itself or on the line immediately preceding it:

```adesh
// @allow(warnings)
let tempBuffer = Array(); // Preceding line directive suppresses warning for 'tempBuffer'

let unusedConfig = {}; // @allow(unused) Inline directive suppresses warning
```

---

### 2.3 Identifier Naming Conventions

Any variable, parameter, function, or class whose name begins with an underscore (`_`) is automatically ignored by the unused warning pass:

```adesh
fn handleRequest(_req, res) {
    // '_req' is marked as intentionally unused, so no warning is emitted.
    return res.json({ ok: true });
}
```

---

## 3. Disabling Warnings via Command Line (CLI)

You can pass warning suppression flags to the `adesh` / `adeshlang` CLI executable.

### Supported CLI Flags

- `--no-warnings`
- `--disable-warnings`
- `--allow-warnings`
- `-Wallow`
- `-Wno-unused`
- `--no-unused-warnings`

### Usage Examples

```powershell
# Run a script with all warnings disabled
cargo run --bin adeshlang -- run --no-warnings examples/Libraries/http/master_http_showcase.adesh

# Using -Wallow flag
cargo run --bin adeshlang -- run -Wallow main.adesh
```

---

## 4. Disabling Warnings via Environment Variables

You can disable warnings globally across scripts, test suites, or CI/CD pipelines using environment variables:

- `ADESHLANG_DISABLE_WARNINGS=1`
- `ADESHLANG_NO_WARNINGS=1`

### PowerShell Example

```powershell
$env:ADESHLANG_DISABLE_WARNINGS = "1"
cargo run --bin adeshlang -- run main.adesh
```

### Bash / Linux / macOS Example

```bash
export ADESHLANG_DISABLE_WARNINGS=1
cargo run --bin adeshlang -- run main.adesh
```

---

## 5. Summary Table

| Suppression Method | Syntax / Command | Scope |
| :--- | :--- | :--- |
| **File Directive** | `// @allow(warnings)` or `// @allow(unused)` | Entire File |
| **Line Directive** | `// @allow(unused)` on or above line | Single Line / Declaration |
| **Variable Naming** | `let _var = 42;` or `fn(_req, res)` | Single Symbol |
| **CLI Flag** | `adesh run --no-warnings script.adesh` | Single CLI Command Execution |
| **Environment Var** | `ADESHLANG_DISABLE_WARNINGS=1` | Entire Shell Session / Process |

