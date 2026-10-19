# AdeshLang Diagnostic Error Codes Index

This document provides a canonical, comprehensive index of all diagnostic codes emitted by the AdeshLang compiler, linker, and runtime.

AdeshLang adopts a strict, two-tier error-code design:
1. **Rust-Compatible Codes (`E0001`–`E9999`)**: Used exclusively when the semantic condition and developer remediation directly correspond to an equivalent Rust compiler error code.
2. **Adesh-Specific Codes (`AD0001`–`AD9999`)**: Dedicated to Adesh-specific language semantics, novel execution models, embedded modes, and binary packaging.
3. **Linker Codes (`LNK001`–`LNK016`)**: Dedicated to object-file resolution, relocation, section mapping, and final executable layout.

To inspect any error code directly from the terminal, run:
```bash
adesh --explain <CODE>
# Example:
adesh --explain E0308
adesh --explain AD0001
adesh --explain list
```

---

## Table of Contents

- [Rust-Compatible Error Codes](#rust-compatible-error-codes)
  - [E0106: Missing lifetime specifier](#e0106)
  - [E0277: Trait or interface bound not satisfied](#e0277)
  - [E0308: Mismatched types](#e0308)
  - [E0381: Use of possibly uninitialized variable](#e0381)
  - [E0382: Use of moved value](#e0382)
  - [E0384: Reassignment to immutable variable](#e0384)
  - [E0416: Identifier bound more than once in pattern](#e0416)
  - [E0425: Cannot find value or variable in scope](#e0425)
  - [E0428: Duplicate identifier defined in same scope](#e0428)
  - [E0499: Cannot borrow variable as mutable more than once at a time](#e0499)
  - [E0502: Cannot borrow variable as mutable while already borrowed as immutable](#e0502)
  - [E0505: Cannot move out of variable because it is borrowed](#e0505)
  - [E0506: Cannot assign to variable because it is borrowed](#e0506)
  - [E0515: Cannot return reference to local variable or temporary](#e0515)
  - [E0597: Borrowed value does not live long enough](#e0597)
  - [E0631: Closure or function argument type mismatch in higher-order call](#e0631)
  - [E0733: Async recursion requires heap indirection](#e0733)
- [Adesh-Specific Diagnostic Codes](#adesh-specific-diagnostic-codes)
  - [AD0001: Unrecognized character or malformed token](#ad0001)
  - [AD0002: Syntax error or unrecoverable parse sequence](#ad0002)
  - [AD0003: Await expression outside async function context](#ad0003)
  - [AD0004: Embedded mode heap allocation prohibited](#ad0004)
  - [AD0005: Corrupt or invalid object-file format / ADOB payload](#ad0005)
  - [AD0006: Unsupported relocation type or target relocation overflow](#ad0006)
  - [AD0007: Unhandled runtime trap or virtual machine fault](#ad0007)
- [Adesh Native Linker Codes](#adesh-native-linker-codes)
  - [LNK001 – LNK016 Overview](#linker-codes-overview)

---

## Rust-Compatible Error Codes

<a id="e0106"></a>
### E0106 — Missing lifetime specifier

- **Severity**: Error
- **Phase**: TypeCheck / Ownership
- **Rust Mapping Justification**: Identical to `rustc E0106`. Occurs when a function returns a reference or a struct contains a reference without specifying its lifetime association.
- **Erroneous Code**:
  ```adesh
  fn first(list: &[int]) -> &int {
      return &list[0];
  }
  ```
- **Corrected Code**:
  ```adesh
  fn first<'a>(list: &'a [int]) -> &'a int {
      return &list[0];
  }
  ```
- **Common Causes**:
  - Returning a borrowed reference from a function taking multiple reference arguments without lifetime annotations.
  - Defining a struct with borrowed fields without lifetime parameters.
- **Suggested Fixes**:
  - Add explicit lifetime annotations matching the input parameters to the return type.
  - Return an owned value rather than a reference.

---

<a id="e0277"></a>
### E0277 — Trait or interface bound not satisfied

- **Severity**: Error
- **Phase**: TypeCheck
- **Rust Mapping Justification**: Identical to `rustc E0277`. Triggered when a type parameter does not satisfy the trait, interface, or protocol bound demanded by a function or struct definition.
- **Erroneous Code**:
  ```adesh
  fn display<T: Printable>(item: T) {
      item.print();
  }
  let s = Socket::new();
  display(s); // Socket does not implement Printable
  ```
- **Corrected Code**:
  ```adesh
  impl Printable for Socket {
      fn print(&self) { println!("Socket"); }
  }
  display(s);
  ```
- **Common Causes**:
  - Passing a concrete type that lacks an required `impl` block for the requested interface.
- **Suggested Fixes**:
  - Implement the missing interface on the type.
  - Relax the generic constraint on the target function.

---

<a id="e0308"></a>
### E0308 — Mismatched types

- **Severity**: Error
- **Phase**: TypeCheck
- **Rust Mapping Justification**: Direct equivalent to `rustc E0308`. An expression had a type that was incompatible with the type expected by its surrounding context.
- **Erroneous Code**:
  ```adesh
  let count: i32 = "seven";
  ```
- **Corrected Code**:
  ```adesh
  let count: i32 = 7;
  // or
  let count_str: string = "seven";
  ```
- **Common Causes**:
  - Providing a string where an integer is expected.
  - Returning a value that does not match the declared function return type.
  - Omitting an explicit cast or type conversion.
- **Suggested Fixes**:
  - Provide a value matching the declared type.
  - Change the type annotation to match the assigned value.
  - Use an explicit conversion function or constructor.

---

<a id="e0381"></a>
### E0381 — Use of possibly uninitialized variable

- **Severity**: Error
- **Phase**: Ownership / Semantic
- **Rust Mapping Justification**: Direct equivalent to `rustc E0381`. A variable is evaluated in an expression before it has been guaranteed an initial value across all control flow branches.
- **Erroneous Code**:
  ```adesh
  let total: int;
  if condition {
      total = 10;
  }
  println("{}", total); // uninitialized if !condition
  ```
- **Corrected Code**:
  ```adesh
  let total: int = 0;
  if condition {
      total = 10;
  }
  println("{}", total);
  ```
- **Common Causes**:
  - Declaring a variable without a default assignment and missing an `else` branch.
- **Suggested Fixes**:
  - Assign a default initial value at the declaration site.
  - Ensure all conditional branches initialize the variable before read access.

---

<a id="e0382"></a>
### E0382 — Use of moved value

- **Severity**: Error
- **Phase**: Ownership
- **Rust Mapping Justification**: Direct equivalent to `rustc E0382`. A variable whose type does not implement `Copy` was moved and subsequently accessed.
- **Erroneous Code**:
  ```adesh
  let msg = Buffer::from("hello");
  consume(msg);
  println("{}", msg); // error[E0382]: use of moved value `msg`
  ```
- **Corrected Code**:
  ```adesh
  let msg = Buffer::from("hello");
  consume(msg.clone());
  println("{}", msg);
  // or pass by reference:
  inspect(&msg);
  ```
- **Common Causes**:
  - Passing an owned heap resource into a function by value.
  - Accessing a variable in a loop after moving it in the first iteration.
- **Suggested Fixes**:
  - Pass the value by reference (`&msg`) if ownership transfer is unnecessary.
  - Explicitly clone or duplicate the value with `.clone()`.

---

<a id="e0384"></a>
### E0384 — Reassignment to immutable variable

- **Severity**: Error
- **Phase**: Semantic / Ownership
- **Rust Mapping Justification**: Direct equivalent to `rustc E0384`. An immutable variable was reassigned a new value.
- **Erroneous Code**:
  ```adesh
  let count = 10;
  count = 20; // error[E0384]: cannot assign twice to immutable variable
  ```
- **Corrected Code**:
  ```adesh
  let mut count = 10;
  count = 20;
  ```
- **Common Causes**:
  - Omitting the `mut` keyword in the variable declaration.
- **Suggested Fixes**:
  - Add `mut` to the `let` binding: `let mut name = value;`.

---

<a id="e0416"></a>
### E0416 — Identifier bound more than once in pattern

- **Severity**: Error
- **Phase**: Parser / Semantic
- **Rust Mapping Justification**: Direct equivalent to `rustc E0416`. The same variable name is bound multiple times within a single pattern or match arm.
- **Erroneous Code**:
  ```adesh
  match pair {
      (x, x) => println("equal: {}", x),
  }
  ```
- **Corrected Code**:
  ```adesh
  match pair {
      (x, y) if x == y => println("equal: {}", x),
  }
  ```
- **Common Causes**:
  - Attempting to enforce equality using duplicate identifiers in pattern matching.
- **Suggested Fixes**:
  - Use distinct identifier names in the pattern and add a pattern guard `if a == b`.

---

<a id="e0425"></a>
### E0425 — Cannot find value or variable in scope

- **Severity**: Error
- **Phase**: Semantic
- **Rust Mapping Justification**: Direct equivalent to `rustc E0425`. An identifier was referenced that has not been declared or imported in the current scope.
- **Erroneous Code**:
  ```adesh
  fn compute() {
      return unknown_metric + 10;
  }
  ```
- **Corrected Code**:
  ```adesh
  fn compute() {
      let unknown_metric = 42;
      return unknown_metric + 10;
  }
  ```
- **Common Causes**:
  - Typo in the variable or function name.
  - Missing `import` statement for a module or dependency.
  - Accessing a local variable outside its declaring block.
- **Suggested Fixes**:
  - Check the spelling of the identifier.
  - Import the module exporting the name.
  - Declare the variable with `let` before use.

---

<a id="e0428"></a>
### E0428 — Duplicate identifier defined in same scope

- **Severity**: Error
- **Phase**: Semantic
- **Rust Mapping Justification**: Direct equivalent to `rustc E0428`. A type, function, constant, or variable has already been defined with the same name in this scope.
- **Erroneous Code**:
  ```adesh
  fn calculate() -> int { return 1; }
  fn calculate() -> int { return 2; }
  ```
- **Corrected Code**:
  ```adesh
  fn calculate_primary() -> int { return 1; }
  fn calculate_secondary() -> int { return 2; }
  ```
- **Common Causes**:
  - Declaring two functions, structs, or constants with identical names in the same module.
- **Suggested Fixes**:
  - Rename one of the definitions.
  - Place related definitions in separate modules.

---

<a id="e0499"></a>
### E0499 — Cannot borrow variable as mutable more than once at a time

- **Severity**: Error
- **Phase**: Ownership
- **Rust Mapping Justification**: Direct equivalent to `rustc E0499`. Multiple concurrent active mutable borrows exist for the same value.
- **Erroneous Code**:
  ```adesh
  let mut buffer = Buffer::new();
  let ref1 = &mut buffer;
  let ref2 = &mut buffer; // error[E0499]
  ref1.write("a");
  ```
- **Corrected Code**:
  ```adesh
  let mut buffer = Buffer::new();
  {
      let ref1 = &mut buffer;
      ref1.write("a");
  }
  let ref2 = &mut buffer;
  ref2.write("b");
  ```
- **Common Causes**:
  - Retaining a mutable reference while attempting a second mutable borrow in the same scope.
- **Suggested Fixes**:
  - Limit the lifetime of the first borrow with curly braces `{}` or complete modifications sequentially.

---

<a id="e0502"></a>
### E0502 — Cannot borrow variable as mutable while already borrowed as immutable

- **Severity**: Error
- **Phase**: Ownership
- **Rust Mapping Justification**: Direct equivalent to `rustc E0502`. A value cannot be borrowed mutably while immutable references to it are still active.
- **Erroneous Code**:
  ```adesh
  let mut data = vec![1, 2, 3];
  let first = &data[0];
  data.push(4); // error[E0502]: cannot borrow `data` as mutable because it is also borrowed as immutable
  println("{}", first);
  ```
- **Corrected Code**:
  ```adesh
  let mut data = vec![1, 2, 3];
  let first_val = data[0]; // copy the value
  data.push(4);
  println("{}", first_val);
  ```
- **Common Causes**:
  - Mutating a collection while holding an iterator or element reference.
- **Suggested Fixes**:
  - Clone or copy scalar elements before modifying the underlying collection.
  - Finish using immutable references before calling mutating methods.

---

<a id="e0505"></a>
### E0505 — Cannot move out of variable because it is borrowed

- **Severity**: Error
- **Phase**: Ownership
- **Rust Mapping Justification**: Direct equivalent to `rustc E0505`. A value was moved while an active borrow of that value exists.
- **Erroneous Code**:
  ```adesh
  let data = DataBuffer::new();
  let r = &data;
  consume(data); // error[E0505]: cannot move out of `data` because it is borrowed
  r.read();
  ```
- **Corrected Code**:
  ```adesh
  let data = DataBuffer::new();
  let r = &data;
  r.read();
  consume(data); // move after borrow ends
  ```
- **Common Causes**:
  - Moving ownership before completing reads via an existing reference.
- **Suggested Fixes**:
  - Reorder statements so ownership transfer happens after the last reference read.

---

<a id="e0506"></a>
### E0506 — Cannot assign to variable because it is borrowed

- **Severity**: Error
- **Phase**: Ownership
- **Rust Mapping Justification**: Direct equivalent to `rustc E0506`. Reassigning or mutating a variable is prohibited while an active borrow is outstanding.
- **Erroneous Code**:
  ```adesh
  let mut x = 10;
  let r = &x;
  x = 20; // error[E0506]
  println("{}", r);
  ```
- **Corrected Code**:
  ```adesh
  let mut x = 10;
  let r = &x;
  println("{}", r);
  x = 20;
  ```
- **Common Causes**:
  - Overwriting a variable whose address is held by an active reference.
- **Suggested Fixes**:
  - Complete all uses of the reference before overwriting the variable.

---

<a id="e0515"></a>
### E0515 — Cannot return reference to local variable or temporary

- **Severity**: Error
- **Phase**: Ownership / Lowering
- **Rust Mapping Justification**: Direct equivalent to `rustc E0515`. Returning a reference to data allocated on the current stack frame would create a dangling pointer.
- **Erroneous Code**:
  ```adesh
  fn create_ref() -> &String {
      let s = String::from("local");
      return &s; // error[E0515]
  }
  ```
- **Corrected Code**:
  ```adesh
  fn create_ref() -> String {
      let s = String::from("local");
      return s; // transfer ownership
  }
  ```
- **Common Causes**:
  - Returning `&local_var` from a function.
- **Suggested Fixes**:
  - Return the owned value by value instead of returning a reference.
  - Store the data in a caller-supplied buffer or arena.

---

<a id="e0597"></a>
### E0597 — Borrowed value does not live long enough

- **Severity**: Error
- **Phase**: Ownership
- **Rust Mapping Justification**: Direct equivalent to `rustc E0597`. A reference outlives the scope of the data it points to.
- **Erroneous Code**:
  ```adesh
  let r: &int;
  {
      let x = 42;
      r = &x;
  } // x is dropped here
  println("{}", r); // error[E0597]
  ```
- **Corrected Code**:
  ```adesh
  let x = 42;
  let r = &x;
  println("{}", r);
  ```
- **Common Causes**:
  - Assigning a reference to a variable declared in an outer scope from an inner block.
- **Suggested Fixes**:
  - Declare the borrowed value in the outer scope.

---

<a id="e0631"></a>
### E0631 — Closure or function argument type mismatch in higher-order call

- **Severity**: Error
- **Phase**: TypeCheck
- **Rust Mapping Justification**: Direct equivalent to `rustc E0631`. A closure or function passed to a higher-order function expects different parameter types or return types than the signature required.
- **Erroneous Code**:
  ```adesh
  fn process(items: &[int], f: fn(string) -> int) { ... }
  fn square(x: int) -> int { return x * x; }
  process(nums, square); // error[E0631]
  ```
- **Corrected Code**:
  ```adesh
  fn process(items: &[int], f: fn(int) -> int) { ... }
  process(nums, square);
  ```
- **Common Causes**:
  - Incorrect parameter type in callback or closure.
- **Suggested Fixes**:
  - Update callback signature to match the expected parameter types.

---

<a id="e0733"></a>
### E0733 — Async recursion requires heap indirection

- **Severity**: Error
- **Phase**: TypeCheck / Lowering
- **Rust Mapping Justification**: Direct equivalent to `rustc E0733`. An `async fn` recursively calls itself without heap indirection (`Box` or dynamic promise), causing an infinite state machine size.
- **Erroneous Code**:
  ```adesh
  async fn countdown(n: int) {
      if n > 0 { countdown(n - 1).await; }
  }
  ```
- **Corrected Code**:
  ```adesh
  fn countdown(n: int) -> BoxFuture<()> {
      BoxFuture::new(async move {
          if n > 0 { countdown(n - 1).await; }
      })
  }
  ```
- **Common Causes**:
  - Direct recursion in an async routine.
- **Suggested Fixes**:
  - Wrap recursive future instantiation in heap allocation or use an iterative loop.

---

## Adesh-Specific Diagnostic Codes

<a id="ad0001"></a>
### AD0001 — Unrecognized character or malformed token

- **Severity**: Error
- **Phase**: Lexer
- **Erroneous Code**:
  ```adesh
  let x = @#$%;
  let s = "\xZZ";
  ```
- **Corrected Code**:
  ```adesh
  let x = 42;
  let s = "\x7F";
  ```
- **Common Causes**:
  - Pasting non-ASCII characters or unsupported operators.
  - Invalid escape sequence in string literal.
- **Suggested Fixes**:
  - Remove unexpected symbols or replace invalid escape sequences.

---

<a id="ad0002"></a>
### AD0002 — Syntax error or unrecoverable parse sequence

- **Severity**: Error
- **Phase**: Parser
- **Erroneous Code**:
  ```adesh
  fn test() {
      let a: int = ;
  }
  ```
- **Corrected Code**:
  ```adesh
  fn test() {
      let a: int = 10;
  }
  ```
- **Common Causes**:
  - Missing expression, mismatched parentheses `()`, or unclosed curly braces `{}`.
- **Suggested Fixes**:
  - Check syntax near the caret; provide the expected expression or close open delimiters.

---

<a id="ad0003"></a>
### AD0003 — Await expression outside async function context

- **Severity**: Error
- **Phase**: TypeCheck / Semantic
- **Erroneous Code**:
  ```adesh
  fn calculate() {
      let res = fetch_data().await;
  }
  ```
- **Corrected Code**:
  ```adesh
  async fn calculate() {
      let res = fetch_data().await;
  }
  ```
- **Common Causes**:
  - Calling `.await` inside a synchronous function.
- **Suggested Fixes**:
  - Mark the enclosing function with `async fn`.
  - Use `block_on(fetch_data())` if executing synchronously from an entry point.

---

<a id="ad0004"></a>
### AD0004 — Embedded mode heap allocation prohibited

- **Severity**: Error
- **Phase**: Ownership / Semantic
- **Erroneous Code**:
  ```adesh
  // Compiled with --embedded
  let buffer = alloc(1024);
  ```
- **Corrected Code**:
  ```adesh
  let mut buffer: [u8; 1024] = [0; 1024]; // stack allocation
  // or use region arena
  ```
- **Common Causes**:
  - Invoking global heap allocator `alloc()` or dynamic heap constructs when compiling for bare-metal embedded targets.
- **Suggested Fixes**:
  - Use fixed-size stack arrays, static memory buffers, or region arena allocations.

---

<a id="ad0005"></a>
### AD0005 — Corrupt or invalid object-file format / ADOB payload

- **Severity**: Error
- **Phase**: Linker / Object
- **Common Causes**:
  - Truncated `.o` or `.adob` file.
  - Incompatible endianness or invalid magic header bytes.
- **Suggested Fixes**:
  - Recompile intermediate object files with the matching version of the Adesh toolchain.

---

<a id="ad0006"></a>
### AD0006 — Unsupported relocation type or target relocation overflow

- **Severity**: Error
- **Phase**: Linker
- **Common Causes**:
  - Absolute 32-bit relocation referencing an address exceeding 4GB range in a 64-bit target.
  - Target architecture does not support the relocation requested.
- **Suggested Fixes**:
  - Compile with position-independent code enabled (`-fPIC` / `--pie`).
  - Verify linking matches the target architecture triple.

---

<a id="ad0007"></a>
### AD0007 — Unhandled runtime trap or virtual machine fault

- **Severity**: Error
- **Phase**: Runtime
- **Erroneous Code**:
  ```adesh
  let zero = 0;
  let res = 10 / zero;
  ```
- **Common Causes**:
  - Integer division by zero, out-of-bounds array indexing, or explicit `panic!()`.
- **Suggested Fixes**:
  - Validate divisors before division operations.
  - Check array bounds using `.len()` or safe slice access methods.

---

<a id="linker-codes-overview"></a>
## Adesh Native Linker Codes

The native linker (`adesh-linker`) produces structured `LNK###` errors for binary generation:

| Code | Title | Description |
| :--- | :--- | :--- |
| `LNK001` | UndefinedSymbol | Undefined symbol referenced by an input object. |
| `LNK002` | DuplicateSymbol | Duplicate strong symbol definition found across objects. |
| `LNK003` | ArchitectureMismatch | Incompatible architecture between object files and target. |
| `LNK004` | FormatMismatch | Object file format does not match output binary format (ELF/PE/Mach-O). |
| `LNK005` | RelocationOverflow | Calculated address offset overflows field bit-width. |
| `LNK006` | InvalidObject | Malformed or corrupt object header/payload. |
| `LNK007` | AbiMismatch | Incompatible calling convention or ABI between modules. |
| `LNK008` | UnsupportedRelocation | Relocation type not implemented for the current CPU arch. |
| `LNK009` | InvalidSection | Invalid section alignment, flags, or configuration. |
| `LNK010` | InvalidTarget | Target triple unrecognized or unsupported. |
| `LNK011` | EntryPointNotFound | `main` or specified entry point symbol not found. |
| `LNK012` | InvalidArchive | Static library `.a` member or symbol table corrupted. |
| `LNK013` | SecurityViolation | Layout violates memory protection invariants (e.g. W^X). |
| `LNK014` | MetadataMismatch | Incompatible `.adesh.meta` binary metadata. |
| `LNK015` | IoError | Failed to read input or write executable binary. |
| `LNK016` | MemoryExhausted | Target memory region or address space exhausted. |
