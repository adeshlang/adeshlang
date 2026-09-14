# Borrowing and Move Semantics Examples

This directory contains examples demonstrating AdeshLang's Rust-inspired ownership, borrowing, and move semantics.

## Core Concepts

### Copy vs Move
- **Copy types** (primitives like numbers): Assignment duplicates the value
- **Move types** (strings, arrays, objects): Assignment transfers ownership

### Borrowing
- **Shared borrow** (`&value`): Read-only access, original remains usable
- **Mutable borrow** (`&mut value`): Exclusive write access (not yet implemented)

## Examples

### 1. `borrow_shared.adesh` - Basic Shared Borrowing
Demonstrates how Copy types work vs shared borrows for move types.

```adesh
let n = 42;
let n_copy = n;      // numbers are Copy; both valid

let msg = "hello";
let alias = &msg;    // shared borrow
print(alias);        // OK
let alias = null;    // drop the borrow by shadowing
print(msg);          // OK after borrow is released
```

**Output:**
```
42
42
hello
hello
```

---

### 2. `borrow_move.adesh` - Move Semantics
Shows how ownership transfers when assigning non-Copy values.

```adesh
let greeting = "namaste";

let greeting_owner = greeting;   // MOVE: greeting is now invalid
print(greeting_owner);           // OK
```

**Output:**
```
namaste
```

---

### 3. `borrow_mut_ok.adesh` - Shared Borrow (no &mut in language)
Shows that shared borrows allow reading without moving ownership; values are mutable by default.

```adesh
let msg = "shared";
let handle = &msg;  // shared borrow
print(handle);
print(msg);         // owner still usable
```

**Output:**
```
shared
shared
```

---

### 4. `borrow_multiple.adesh` - Multiple Shared Borrows
Demonstrates that multiple shared borrows are allowed simultaneously.

```adesh
let note = "adesh";
let a = &note;
let b = &note;       // multiple shared borrows allowed
print(a);
print(b);
print(note);         // owner still usable
```

**Output:**
```
adesh
adesh
adesh
```

---

### 5. `borrow_error.adesh` - Use-After-Move Error
Shows the runtime error when trying to use a moved value.

```adesh
let data = "important";
let moved_data = data;   // data is moved

print(moved_data);       // ✓ OK
print(data);             // ❌ ERROR: use of moved value 'data'
```

**Output:**
```
important
use of moved value 'data'
```

---

## Running the Examples

```bash
# Run individual examples
cargo run -- run examples/tests/borrow_shared.adesh
cargo run -- run examples/tests/borrow_move.adesh
cargo run -- run examples/tests/borrow_multiple.adesh
cargo run -- run examples/tests/borrow_error.adesh

# With profiling
adesh --profile run examples/tests/borrow_shared.adesh
```

## Key Takeaways

1. **Primitives are Copy**: Numbers, booleans, chars duplicate on assignment
2. **Strings are Move**: Ownership transfers; original becomes invalid
3. **Shared borrows (`&`)**: Allow read-only access without moving
4. **Runtime safety**: AdeshLang detects use-after-move errors at runtime
5. **Rust-like semantics**: Familiar ownership model for systems programming

## Future Features

- Compile-time borrow checking (currently runtime only)
- Borrow lifetimes and scope analysis
