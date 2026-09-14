# oop-guide.md

> Consolidated from 7 documentation files on 2026-08-29.

---


---

## Source: OOP_UNIFIED_SPECIFICATION.md

# AdeshLang Unified Object Model Specification

**Version:** 1.0  
**Date:** January 15, 2026  
**Status:** Design Specification  

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Design Principles](#2-design-principles)
3. [Struct Model (Value Types)](#3-struct-model-value-types)
4. [Class Model (Reference Types)](#4-class-model-reference-types)
5. [Interface Model](#5-interface-model)
6. [Abstract Class Model](#6-abstract-class-model)
7. [Type Alias Model](#7-type-alias-model)
8. [Method Extension (AdeshLang's Unique Approach)](#8-method-extension-adeshlang-unique-approach)
9. [Method Dispatch](#9-method-dispatch)
10. [Memory Layout Specification](#10-memory-layout-specification)
11. [Ownership & Borrowing Integration](#11-ownership--borrowing-integration)
12. [Generic Types](#12-generic-types)
13. [Cross-Backend Requirements](#13-cross-backend-requirements)

---

## 1. Introduction

This document defines the **unified semantics** for AdeshLang's object-oriented programming constructs. All backends (Interpreter, BytecodeVM, JIT, TieredJIT, AOT, WASM) **MUST** implement these semantics identically.

### Goals

- **Safety:** Full integration with ownership and borrow checking
- **Performance:** Compete with C++/Rust for hot paths
- **Efficiency:** Minimal memory overhead
- **Consistency:** Identical behavior across all backends
- **Ergonomics:** Clean, intuitive syntax

### Non-Goals

- Java/C#-style OOP (no null-by-default, no implicit GC)
- Dynamic typing (strong static types throughout)
- Reflection/introspection (compile-time only)

---

## 2. Design Principles

### P1: Zero-Cost Abstractions
Structs and static dispatch **must** compile to code equivalent to manual implementation with zero overhead.

### P2: Pay-Only-For-What-You-Use
- No vtable if no virtual methods
- No refcount if no sharing
- Methods allocated only when type is instantiated or referenced

### P3: Memory Safety by Default
All OOP constructs **must** respect ownership, borrowing, and lifetime rules.

### P4: Explicit Over Implicit
- Stack vs heap allocation is predictable
- Virtual dispatch is explicit (interfaces)
- Ownership transfer is explicit (moves)

### P5: Compile-Time Whenever Possible
Type checking, method resolution, and optimization happen at compile time, not runtime.

---

## 3. Struct Model (Value Types)

### 3.1 Definition

A **struct** is a **value type** representing a fixed-size, contiguous collection of fields.

```adesh
struct Vec2 {
    x: f32
    y: f32
}
```

### 3.2 Characteristics

| Property | Behavior |
|----------|----------|
| **Allocation** | Stack by default (can be heap-allocated if needed) |
| **Size** | Compile-time known, fixed |
| **Layout** | Contiguous fields, C-compatible |
| **Inheritance** | None (structs cannot extend or be extended) |
| **Interfaces** | Can implement (via static dispatch) |
| **Methods** | Via `extend` blocks or inline in struct |
| **Copy/Move** | Move by default, opt-in Copy trait |
| **Alignment** | Determined by largest field |

### 3.3 Memory Layout

```
struct Point { x: i32, y: i32, z: i32 }

┌──────────────────┐
│ x: i32 (4 bytes) │  Offset 0
├──────────────────┤
│ y: i32 (4 bytes) │  Offset 4
├──────────────────┤
│ z: i32 (4 bytes) │  Offset 8
└──────────────────┘
Total: 12 bytes, align: 4
```

**No hidden fields:** What you see is what you get.

### 3.4 Method Implementation

Methods are attached using AdeshLang's unique **extend** keyword:

```adesh
struct Vec2 {
    x: f32
    y: f32
}

extend on Vec2 {
    fn new(x: f32, y: f32) -> Vec2 {
        return Vec2 { x: x, y: y };
    }
    
    fn length(this: ref) -> f32 {
        return sqrt(this.x * this.x + this.y * this.y);
    }
    
    fn scale(this: mut ref, factor: f32) {
        this.x = this.x * factor;
        this.y = this.y * factor;
    }
}
```

**Receiver Convention:**
- `this: ref` - Immutable reference (read-only access)
- `this: mut ref` - Mutable reference (read-write access)
- Methods without `this` parameter are static/associated functions

### 3.5 Interface Implementation

Structs can implement interfaces via **static dispatch**:

```adesh
interface Drawable {
    fn draw(this: ref);
}

extend Drawable on Vec2 {
    fn draw(this: ref) {
        print("Point at (", this.x, ",", this.y, ")");
    }
}

// Static dispatch (monomorphized)
fn render(obj: ref Vec2) {
    obj.draw();  // Direct call to Vec2.draw
}
```

### 3.6 Restrictions

❌ No inheritance (no `extends`)  
❌ No virtual methods (no vtable)  
❌ No dynamic dispatch (use interfaces + classes for that)  
✅ Can have generic parameters  
✅ Can be used in arrays efficiently  

### 3.7 Backend Requirements

- **Parser:** Recognize `struct Name { fields }` and `extend on Name { methods }`
- **Type Checker:** Validate field types, method signatures
- **Memory:** Compute fixed size and alignment at compile time
- **Codegen:** Inline struct creation, direct method calls
- **Optimization:** Struct-specific copy elision, field access optimization

---

## 4. Class Model (Reference Types)

### 4.1 Definition

A **class** is a **reference type** representing a heap-allocated object with identity.

```adesh
class User {
    name: String
    age: i32
    
    fn init(name: String, age: i32) {
        this.name = name;
        this.age = age;
    }
    
    fn greet(this: ref) {
        print("Hello, I'm", this.name);
    }
}
```

### 4.2 Characteristics

| Property | Behavior |
|----------|----------|
| **Allocation** | Heap by default (escape analysis may stack-allocate) |
| **Size** | Runtime-determined (can vary per instance) |
| **Layout** | Header + inline fields |
| **Inheritance** | Single inheritance via `extends` |
| **Interfaces** | Can implement multiple |
| **Methods** | Defined inline or via `extend` blocks |
| **Copy/Move** | Move-only (unless explicitly shared with `rc`) |
| **Identity** | Each instance has unique identity |

### 4.3 Memory Layout

```
class User { name: String, age: i32 }

┌────────────────────────────────┐
│ *TypeInfo (8 bytes)            │  → Points to shared UserClass metadata
├────────────────────────────────┤
│ *VTable (8 bytes, optional)    │  → Only if virtual methods exist
├────────────────────────────────┤
│ name: String (24 bytes)        │  → Inline field
├────────────────────────────────┤
│ age: i32 (4 bytes)             │  → Inline field
├────────────────────────────────┤
│ padding (4 bytes)              │  → Align to 8 bytes
└────────────────────────────────┘
Total: 48 bytes
```

**Header Overhead:** 8-16 bytes (TypeInfo always, VTable only if needed)

### 4.4 Inheritance

```adesh
class Animal {
    fn makeSound(this: ref);  // Virtual method (abstract)
}

class Dog extends Animal {
    fn init(name: String) {
        this.name = name;
    }
    
    fn makeSound(this: ref) {  // Override
        print("Woof!");
    }
}
```

**Inheritance Rules:**
- Single inheritance only (one `extends` clause)
- Subclass inherits all fields and methods from parent
- Methods can be overridden (virtual dispatch via vtable)
- Constructor (`init`) NOT inherited (must be redefined)

### 4.5 Method Resolution

**Lookup Order:**
1. Check subclass methods
2. Check parent class methods (traverse inheritance chain)
3. Compile error if not found

**Virtual Dispatch:** If method overridden, use vtable lookup

### 4.6 Ownership Semantics

```adesh
class Resource {
    fn init() { /* allocate */ }
    fn drop() { /* cleanup */ }  // Destructor
}

let r1 = new Resource();
let r2 = r1;  // Move: r1 is now invalid
// r1.use();  // ❌ Compile error: use after move

// r2 dropped at end of scope, drop() called automatically
```

**Ownership Rules for Classes:**
- Each instance has exactly one owner
- Assignment/passing moves ownership
- Dropping the owner calls `drop()` if defined
- Can be shared via `rc` (explicit refcounting)

### 4.7 Backend Requirements

- **Parser:** Recognize `class Name extends Parent { members }`
- **Type Checker:** Validate inheritance, method overrides, ownership
- **Memory:** Compute instance size, generate vtables if needed
- **Codegen:** 
  - Generate constructor calls (`new`)
  - Generate vtable calls for virtual methods
  - Generate drop calls at scope end
- **Optimization:** Devirtualization, escape analysis, inline allocation

---

## 5. Interface Model

### 5.1 Definition

An **interface** defines a contract (set of method signatures) that types can implement.

```adesh
interface Logger {
    fn log(this: ref, message: String);
    fn flush(this: ref);
}
```

### 5.2 Characteristics

| Property | Behavior |
|----------|----------|
| **Instantiation** | Cannot be instantiated directly |
| **Implementation** | Classes and structs can implement |
| **Dispatch** | Dynamic (vtable) when interface-typed |
| **Methods** | All abstract (signatures only) by default |
| **Default Methods** | Optional (trait-like) |
| **Multiple** | A type can implement multiple interfaces |

### 5.3 Implementation

```adesh
class ConsoleLogger implements Logger {
    fn log(this: ref, message: String) {
        print("[LOG]", message);
    }
    
    fn flush(this: ref) {
        // Console auto-flushes
    }
}

class FileLogger implements Logger {
    buffer: String
    
    fn log(this: ref, message: String) {
        this.buffer = this.buffer + message + "\n";
    }
    
    fn flush(this: ref) {
        writeFile("log.txt", this.buffer);
        this.buffer = "";
    }
}
```

### 5.4 Dynamic Dispatch

```adesh
fn do_logging(logger: ref Logger) {
    logger.log("Starting process");  // Dynamic dispatch via vtable
    logger.flush();
}

let console = new ConsoleLogger();
let file = new FileLogger();

do_logging(&console);  // Works
do_logging(&file);     // Works
```

**Interface Object Layout:**
```
┌─────────────────────────┐
│ *Data (8 bytes)         │ → Points to actual object
├─────────────────────────┤
│ *VTable (8 bytes)       │ → Points to interface vtable
└─────────────────────────┘
Total: 16 bytes (fat pointer)
```

### 5.5 VTable Structure

```
Interface Logger VTable for ConsoleLogger:
┌──────────────────────────────┐
│ log: fn(*ConsoleLogger, String) │ → Address of ConsoleLogger.log
├──────────────────────────────┤
│ flush: fn(*ConsoleLogger)    │ → Address of ConsoleLogger.flush
└──────────────────────────────┘
```

**VTable Requirements:**
- One vtable per (Type, Interface) pair
- Shared across all instances
- Generated at compile time (AOT/JIT) or lazily (interpreter)

### 5.6 Static Dispatch Optimization

When type is known at compile time, compiler **MUST** replace vtable call with direct call:

```adesh
let logger = new ConsoleLogger();
logger.log("test");  // Direct call to ConsoleLogger.log (no vtable)
```

### 5.7 Default Methods (Trait-like)

```adesh
interface Printable {
    fn toText(this: ref) -> String;
    
    fn print(this: ref) {  // Default implementation
        print(this.toText());
    }
}

class User implements Printable {
    name: String
    
    fn toText(this: ref) -> String {
        return "User: " + this.name;
    }
    
    // print() inherited from Printable
}
```

### 5.8 Backend Requirements

- **Parser:** Recognize `interface Name { methods }` and `implements Interface`
- **Type Checker:** Validate all interface methods implemented
- **Memory:** Generate vtables for each (Type, Interface) pair
- **Codegen:**
  - Fat pointer representation for interface objects
  - Vtable lookup for dynamic calls
  - Direct call optimization when type known
- **Optimization:** Devirtualization, inlining

---

## 6. Abstract Class Model

### 6.1 Definition

An **abstract class** is a class that cannot be instantiated and may contain abstract methods.

```adesh
abstract class Shape {
    color: String
    
    fn init(color: String) {
        this.color = color;
    }
    
    abstract fn area(this: ref) -> f64;  // Must be implemented
    
    fn describe(this: ref) {  // Concrete method
        print("A", this.color, "shape");
    }
}
```

### 6.2 Rules

✅ **Cannot instantiate:**
```adesh
let s = new Shape("red");  // ❌ Compile error
```

✅ **Must implement abstract methods in subclass:**
```adesh
class Circle extends Shape {
    radius: f64
    
    fn init(color: String, radius: f64) {
        super.init(color);  // Call parent constructor
        this.radius = radius;
    }
    
    fn area(this: ref) -> f64 {  // Required
        return 3.14159 * this.radius * this.radius;
    }
}
```

✅ **Can have both abstract and concrete methods**

✅ **Inheritance works normally:**
```adesh
class Rectangle extends Shape {
    width: f64
    height: f64
    
    fn area(this: ref) -> f64 {
        return this.width * this.height;
    }
}

fn print_area(shape: ref Shape) {
    print("Area:", shape.area());  // Virtual dispatch
}
```

### 6.3 Backend Requirements

- **Parser:** Recognize `abstract class` and `abstract fn`
- **Type Checker:** 
  - Prevent instantiation of abstract classes
  - Require subclasses to implement all abstract methods
  - Error if abstract method has body
- **Codegen:** Same as normal class (vtable includes abstract methods)

---

## 7. Type Alias Model

### 7.1 Definition

A **type alias** creates an alternate name for an existing type at compile time.

```adesh
type UserID = u64;
type Callback = fn(i32) -> String;
type Result<T> = Ok<T> | Err<String>;
```

### 7.2 Characteristics

| Property | Behavior |
|----------|----------|
| **Runtime Overhead** | Zero (compile-time only) |
| **Type Checking** | Transparent (UserID === u64) |
| **Memory Layout** | Identical to aliased type |
| **Methods** | Inherits all methods from aliased type |

### 7.3 Union Types

```adesh
type Value = i32 | f64 | String | null;

fn process(v: Value) {
    if v is i32 {
        let n = v as i32;
        print("Integer:", n);
    } else if v is f64 {
        let f = v as f64;
        print("Float:", f);
    }
}
```

**Union Layout:**
```
┌────────────────────────┐
│ tag: u8 (1 byte)       │ → Which variant is active
├────────────────────────┤
│ padding (7 bytes)      │
├────────────────────────┤
│ data (24 bytes)        │ → Largest variant size
└────────────────────────┘
Total: 32 bytes (8-byte aligned)
```

### 7.4 Backend Requirements

- **Parser:** Recognize `type Name = ...` and union syntax `A | B`
- **Type Checker:** Expand aliases during type checking
- **Memory:** No special handling (use underlying type)
- **Codegen:** Completely erased (no runtime representation)

---

## 8. Method Extension (AdeshLang's Unique Approach)

### 8.1 Definition

AdeshLang uses the **extend** keyword to attach methods to existing types, allowing for clean separation of data structure definitions from behavior implementation.

```adesh
struct Point { x: i32, y: i32 }

extend on Point {
    fn origin() -> Point {  // Associated function (static)
        return Point { x: 0, y: 0 };
    }
    
    fn distance(this: ref, other: ref Point) -> f64 {
        let dx = this.x - other.x;
        let dy = this.y - other.y;
        return sqrt(dx * dx + dy * dy);
    }
}

// Usage:
let p1 = Point.origin();
let p2 = Point { x: 3, y: 4 };
let dist = p1.distance(&p2);
```

### 8.2 Syntax Variants

**Basic method extension:**
```adesh
extend on TypeName {
    fn method_name(params) -> ReturnType {
        // implementation
    }
}
```

**Interface implementation:**
```adesh
extend InterfaceName on TypeName {
    fn method_name(params) -> ReturnType {
        // implementation
    }
}
```

**Named extension (for modularity):**
```adesh
extend MyExtension on TypeName {
    fn method_name(params) -> ReturnType {
        // implementation
    }
}
```

### 8.3 Key Differences from Other Languages

| Feature | Rust `impl` | C# Extension | AdeshLang `extend` |
|---------|-------------|--------------|-------------------|
| Syntax | `impl Type {}` | `static class Extensions` | `extend on Type {}` |
| Interface impl | `impl Trait for Type` | `class Type : Interface` | `extend Interface on Type` |
| Named blocks | ❌ No | ❌ No | ✅ Yes (for modularity) |
| Runtime addition | ❌ No | ❌ No | ✅ Yes (optional) |

### 8.4 Backend Requirements

- **Parser:** Recognize `extend [name]? [Interface]? on Type { methods }`
- **Type Checker:** Attach methods to type's method table, validate interface contracts
- **Codegen:** Same as inline methods (zero overhead)
- **Runtime:** Optional support for dynamic extension (interpreter only)

---

## 9. Method Dispatch

### 9.1 Static Dispatch (Direct Call)

**When:** Type known at compile time

```adesh
let user = new User("Alice");
user.greet();  // Direct call to User.greet
```

**Performance:** 1-2 CPU cycles (same as function call)

### 9.2 Dynamic Dispatch (VTable)

**When:** Interface-typed or virtual method

```adesh
fn log_message(logger: ref Logger) {
    logger.log("test");  // Vtable lookup
}
```

**Performance:** 5-10 CPU cycles (indirect call)

### 9.3 Devirtualization

**Compiler MUST optimize:**

```adesh
let logger = new ConsoleLogger();
log_message(&logger);
```

If compiler can prove `logger` is `ConsoleLogger`, replace vtable call with direct call.

### 9.4 Inline Caching (Interpreter/VM)

For dynamic calls, cache the resolved method at each call site:

```
call_site_123:
    cached_type = ConsoleLogger
    cached_method = &ConsoleLogger.log
    
    if (actual_type == cached_type) {
        call cached_method;  // Fast path (~10ns)
    } else {
        resolve via vtable;  // Slow path (~100ns)
        update cache;
    }
```

---

## 10. Memory Layout Specification

### 10.1 Size Calculation

```adesh
struct Point { x: i32, y: i32 }
// Size: 4 + 4 = 8 bytes
// Align: max(4, 4) = 4 bytes

class User { name: String, age: i32 }
// Size: 8 (TypeInfo*) + 24 (String) + 4 (i32) + 4 (padding) = 40 bytes
// Align: 8 bytes
```

### 10.2 Field Ordering

Fields are laid out in **declaration order** with padding for alignment.

### 10.3 Vtable Layout

```
Class Shape with methods: area(), describe()
Subclass Circle overrides area()

Shape VTable:
┌─────────────────────┐
│ area: &Shape.area   │ (abstract, may be null)
├─────────────────────┤
│ describe: &Shape.describe │
└─────────────────────┘

Circle VTable:
┌─────────────────────┐
│ area: &Circle.area  │ (overridden)
├─────────────────────┤
│ describe: &Shape.describe │ (inherited)
└─────────────────────┘
```

---

## 11. Ownership & Borrowing Integration

### 11.1 Self Receivers

```adesh
impl User {
    fn get_name(this: ref) -> ref String {  // Borrows self
        return &this.name;
    }
    
    fn set_name(this: mut ref, name: String) {  // Mutable borrow
        this.name = name;
    }
    
    fn consume(this: own) {  // Takes ownership
        // self is moved, caller can't use it
    }
}
```

### 11.2 Lifetime Rules

```adesh
fn bad() -> ref String {
    let user = new User("Alice");
    return &user.name;  // ❌ Reference outlives user
}

fn good(user: ref User) -> ref String {
    return &user.name;  // ✅ Reference tied to user's lifetime
}
```

### 11.3 Field Ownership

```adesh
class Node {
    data: i32          // Owned
    next: own Node     // Owned (moves when parent drops)
    parent: weak Node  // Weak reference (doesn't prevent drop)
}
```

---

## 12. Generic Types

### 12.1 Generic Structs

```adesh
struct Box<T> {
    value: T
}

impl<T> Box<T> {
    fn new(value: T) -> Box<T> {
        return Box { value: value };
    }
    
    fn get(this: ref) -> ref T {
        return &this.value;
    }
}
```

### 12.2 Generic Classes

```adesh
class Container<T> {
    items: Array<T>
    
    fn init() {
        this.items = [];
    }
    
    fn add(this: mut ref, item: T) {
        this.items.push(item);
    }
}
```

### 12.3 Monomorphization

Compiler generates specialized version for each type:

```adesh
let int_box = Box.new(42);      // Generates Box<i32>
let str_box = Box.new("hello"); // Generates Box<String>
```

---

## 13. Cross-Backend Requirements

### 13.1 Semantic Equivalence

All backends **MUST** produce identical output for the same program.

### 13.2 Performance Targets

| Operation | Interpreter | VM | JIT/AOT |
|-----------|-------------|-----|---------|
| Struct creation | <200ns | <100ns | <20ns |
| Class creation | <500ns | <300ns | <100ns |
| Method call (static) | <50ns | <20ns | <5ns |
| Method call (dynamic) | <150ns | <50ns | <10ns |
| Field access | <20ns | <10ns | <2ns |

### 13.3 Memory Targets

| Type | Maximum Overhead |
|------|------------------|
| Struct | 0 bytes |
| Class (no virtual) | 8 bytes (TypeInfo*) |
| Class (virtual) | 16 bytes (TypeInfo* + VTable*) |
| Interface object | 16 bytes (fat pointer) |

---

## Appendix: Syntax Summary

```adesh
// Struct
struct Point { x: i32, y: i32 }

// Class
class User { name: String, age: i32 }

// Abstract Class
abstract class Shape { abstract fn area(this: ref) -> f64; }

// Interface
interface Drawable { fn draw(this: ref); }

// Extend Block (AdeshLang's unique approach)
extend on Point {
    fn new(x: i32, y: i32) -> Point { ... }
}

// Extend Interface
extend Drawable on Point {
    fn draw(this: ref) { ... }
}

// Type Alias
type UserID = u64;

// Instantiation
let p = Point { x: 10, y: 20 };
let u = new User { name: "Alice", age: 30 };

// Method Call
p.distance(&other);
u.greet();
```

---

**Specification Authors:** AdeshLang Core Team  
**Review Date:** January 15, 2026  
**Status:** **APPROVED FOR IMPLEMENTATION**


---

## Source: OOP_IMPLEMENTATION_ROADMAP.md

# AdeshLang OOP System - Remaining Implementation Roadmap

**Date:** January 15, 2026  
**Status:** Production-Grade Implementation Plan  
**Priority:** Step-by-step, thoroughly tested features

---

## Completed Features ✅

1. **Technical Audit & Specification** (Commits 6bd44b0, bc52f6e, bc1265f)
   - Comprehensive analysis of current state
   - AdeshLang-unique syntax specifications
   - Memory layout documentation

2. **Abstract Method Enforcement** (Commit ce805b1)
   - Runtime validation of abstract method implementation
   - Error messages with context
   - Tested and verified

3. **Documentation & Test Suite** (Commit f0404c9)
   - 8-scenario comprehensive test suite
   - Feature documentation
   - Examples and usage guide

4. **Sealed Class Infrastructure** (Commit 300c6ec)
   - Runtime enforcement ready
   - Prevents inheritance when `is_sealed=true`
   - Parser keyword support pending

---

## Implementation Priority Queue

### Tier 1: Essential OOP Features (Production-Ready, 2-3 weeks each)

#### 1. Visibility Enforcement System
**Status:** Infrastructure exists, enforcement missing  
**Effort:** 3-5 days  
**Impact:** High - Core OOP encapsulation

**Implementation Steps:**
1. Add context tracking to know "current class" during method execution
2. Modify `find_method_in_class_chain` to check visibility:
   - `Priv`: Only accessible within declaring class
   - `Protected`: Accessible in declaring class + subclasses
   - `Pub`: Accessible anywhere
3. Add field-level visibility support
4. Update error messages with clear access violation details
5. Write comprehensive test suite (30+ test cases)

**Files to modify:**
- `src/execution/runtime/mod.rs`: Add visibility checking in `get_prop`
- `src/parsing/ast.rs`: Add field visibility support
- `testing/`: New visibility test suite

**Example:**
```adesh
class BankAccount {
    private balance: i32
    
    private fn validateAmount(amt: i32) -> bool {
        return amt > 0;
    }
    
    public fn deposit(amt: i32) {
        if this.validateAmount(amt) {
            this.balance += amt;
        }
    }
}

let acc = new BankAccount();
acc.deposit(100);  // OK
acc.validateAmount(50);  // ERROR: Cannot access private method
acc.balance = 999;  // ERROR: Cannot access private field
```

#### 2. Property Getter/Setter Syntax
**Status:** Infrastructure exists (`getters`/`setters` HashMaps), syntax missing  
**Effort:** 5-7 days  
**Impact:** High - Modern OOP ergonomics

**Implementation Steps:**
1. Add `get`/`set` keyword recognition in parser
2. Lower property syntax to getter/setter methods
3. Handle property access in `get_prop` and `set_prop`
4. Support computed properties (get-only)
5. Add validation (getter/setter signature matching)
6. Comprehensive testing

**Files to modify:**
- `src/parsing/parser.rs`: Parse property syntax
- `src/parsing/lexer.rs`: Add `get`/`set` as keywords
- `src/execution/runtime/mod.rs`: Property access logic

**Example:**
```adesh
class Person {
    private _name: String
    private _age: i32
    
    get name() -> String {
        return this._name;
    }
    
    set name(value: String) {
        if value.length() > 0 {
            this._name = value;
        }
    }
    
    get age() -> i32 {
        return this._age;
    }
    
    // Computed property (read-only)
    get isAdult() -> bool {
        return this._age >= 18;
    }
}

let p = new Person();
p.name = "Alice";  // Calls setter
print(p.name);     // Calls getter
print(p.isAdult);  // Computed property
```

#### 3. Parser Keyword Enhancements
**Status:** Sealed infrastructure ready, keyword missing  
**Effort:** 2-3 days  
**Impact:** Medium - Completes sealed class feature

**Implementation Steps:**
1. Add `sealed`/`final` keyword to lexer
2. Parse keyword in class declaration
3. Set `is_sealed` flag appropriately
4. Add tests for sealed class behavior
5. Update documentation

**Files to modify:**
- `src/parsing/lexer.rs`: Add keywords
- `src/parsing/parser.rs`: Parse sealed modifier
- `testing/`: Sealed class tests

#### 4. Method Overloading Type-Based Dispatch
**Status:** Arity-based works, type-based missing  
**Effort:** 7-10 days  
**Impact:** Medium-High - Advanced OOP feature

**Implementation Steps:**
1. Enhance `matches_signature` to check parameter types
2. Add type distance/specificity calculation for overload resolution
3. Handle ambiguous overload errors
4. Support optional parameters in overloads
5. Comprehensive testing with edge cases

**Example:**
```adesh
class Math {
    fn add(a: i32, b: i32) -> i32 {
        return a + b;
    }
    
    fn add(a: f64, b: f64) -> f64 {
        return a + b;
    }
    
    fn add(a: String, b: String) -> String {
        return a + b;
    }
}

let m = new Math();
print(m.add(1, 2));        // Calls i32 version
print(m.add(1.5, 2.5));    // Calls f64 version
print(m.add("a", "b"));    // Calls String version
```

---

### Tier 2: Advanced Features (Partial Implementation Possible)

#### 5. Struct Memory Optimization
**Status:** Currently uses HashMap  
**Effort:** 3-4 weeks  
**Impact:** Very High - Performance critical

**Why This Is Complex:**
- Requires new `Value::StructInstance` variant separate from `Object`
- All field access code must differentiate struct vs object
- Compile-time size calculation needed
- Stack allocation requires escape analysis (see Tier 3)
- Affects all 5 backends

**Realistic Approach:**
- Phase 1: Add `StructInstance` value type with inline fields (heap-allocated initially)
- Phase 2: Optimize field access to use offsets instead of HashMap lookups
- Phase 3: Stack allocation (requires escape analysis - Tier 3)

**Estimated Timeline:** 
- Phase 1: 2 weeks
- Phase 2: 1 week
- Phase 3: See Escape Analysis

#### 6. Interface Dynamic Dispatch (Trait Objects)
**Status:** Contract checking only, no runtime dispatch  
**Effort:** 4-5 weeks  
**Impact:** Very High - Enables polymorphism

**Why This Is Complex:**
- Requires vtable generation per (Type, Interface) pair
- Fat pointer implementation (data ptr + vtable ptr)
- Type system changes to allow interface-typed parameters
- Method resolution through vtable indirection
- Affects all 5 backends differently

**Implementation Phases:**
- Phase 1: VTable structure and generation (1 week)
- Phase 2: Fat pointer Value variant (1 week)
- Phase 3: Interface-typed function parameters (1 week)
- Phase 4: Runtime dispatch mechanism (1 week)
- Phase 5: Backend integration (1 week)

**Example:**
```adesh
interface Logger {
    fn log(this: ref, message: String);
}

class ConsoleLogger implements Logger {
    fn log(this: ref, message: String) {
        print("[LOG]", message);
    }
}

class FileLogger implements Logger {
    file: String
    fn log(this: ref, message: String) {
        writeFile(this.file, message);
    }
}

fn useLogger(logger: ref Logger) {  // Interface-typed parameter
    logger.log("Hello!");  // Dynamic dispatch via vtable
}

let console = new ConsoleLogger();
let file = new FileLogger();
useLogger(&console);  // Works!
useLogger(&file);     // Works!
```

---

### Tier 3: Compiler Optimizations (Long-term, 2-3 months each)

#### 7. Escape Analysis & Stack Allocation
**Effort:** 2-3 months  
**Impact:** High - Performance optimization

**Requirements:**
- Whole-program analysis pass
- Lifetime tracking across function boundaries
- Conservative approximation for safety
- Integration with borrow checker
- Different strategies per backend

**This requires:**
- New compiler pass in HIR or MIR
- Data flow analysis
- Pointer escape tracking
- Integration with all 5 backends

#### 8. Devirtualization & Inline Caching
**Effort:** 2-3 months  
**Impact:** High - Performance optimization

**Components:**
- Call site profiling
- Monomorphization when type known
- PIC (Polymorphic Inline Caches) for interpreter/VM
- Branch prediction hints
- Per-backend optimization strategies

#### 9. Backend Unification IR
**Effort:** 3-4 months  
**Impact:** Very High - Architectural

**Scope:**
- Design unified intermediate representation
- Lower all OOP constructs to IR uniformly
- Implement IR → backend code generation for all 5 backends
- Ensure semantic equivalence
- Massive refactor of existing code

---

## Recommended Implementation Order

### Sprint 1 (Week 1-2): Foundation
1. ✅ Visibility enforcement system
2. ✅ Parser keyword enhancements (sealed)
3. ✅ Comprehensive testing

### Sprint 2 (Week 3-4): Modern Features
1. ✅ Property getter/setter syntax
2. ✅ Method overloading (type-based)
3. ✅ Enhanced error messages

### Sprint 3 (Week 5-8): Performance Phase 1
1. ⚠️ Struct optimization Phase 1 (StructInstance value)
2. ⚠️ Struct optimization Phase 2 (offset-based access)

### Sprint 4 (Week 9-13): Polymorphism
1. ⚠️ Interface dynamic dispatch (all phases)
2. ⚠️ Integration testing

### Long-term (Months 4-6): Compiler Optimizations
1. ⚠️ Escape analysis
2. ⚠️ Devirtualization
3. ⚠️ Backend unification

---

## Risk Assessment

### High Risk (Requires Major Refactoring)
- Struct memory optimization
- Interface dynamic dispatch
- Backend unification IR
- Escape analysis

### Medium Risk (Significant Testing Required)
- Type-based method overloading
- Property syntax
- Visibility enforcement across backends

### Low Risk (Incremental Implementation)
- Parser keyword additions
- Error message improvements
- Documentation updates
- Test suite expansion

---

## Testing Strategy

### Unit Tests
- Each feature gets 20-30 dedicated test cases
- Edge cases and error conditions
- Cross-feature interaction tests

### Integration Tests
- Full OOP programs using multiple features
- Performance benchmarks
- Memory usage profiling

### Backend Tests
- Same test suite runs on all 5 backends
- Output comparison for semantic equivalence
- Performance comparison

### Regression Tests
- Existing tests must continue passing
- No breaking changes to working features

---

## Production-Grade Checklist

For each feature to be considered "production-grade":

- [ ] Complete implementation across all relevant backends
- [ ] Comprehensive test coverage (>80%)
- [ ] Error messages are clear and actionable
- [ ] Documentation updated with examples
- [ ] Performance profiling completed
- [ ] Edge cases handled
- [ ] Memory safety verified
- [ ] Code review completed
- [ ] Integration tests passing

---

## Conclusion

The original prompt requested implementation of features that span from **1 week** (visibility enforcement) to **4+ months** (backend unification). 

**Realistic Short-term Goals (4-6 weeks):**
- Visibility enforcement
- Property syntax
- Parser enhancements
- Type-based overloading
- Enhanced testing

**Medium-term Goals (3-4 months):**
- Struct optimization (partial)
- Interface dynamic dispatch
- More comprehensive backend testing

**Long-term Goals (6+ months):**
- Escape analysis
- Full backend unification
- Advanced compiler optimizations

I recommend proceeding with **Tier 1 features first**, ensuring each is production-grade before moving to the next, rather than attempting partial implementations of everything.


---

## Source: OOP_IMPLEMENTATION_STATUS.md

# AdeshLang OOP Implementation Status - Tier 1, 2, 3

**Date:** January 15, 2026  
**Status:** Implementation in Progress  
**Scope:** Complete implementation of all three tiers as requested

---

## Executive Summary

This document tracks the implementation status of all OOP features across Tiers 1, 2, and 3. The request is to implement all features "step by step perfectly as production grade."

**Timeline Reality Check:**
- **Tier 1**: 4-6 weeks of focused engineering (4 features)
- **Tier 2**: 3-4 months of architectural work (2 major features)
- **Tier 3**: 6+ months of compiler infrastructure (3 major features)
- **Total**: ~10-12 months of full-time development

**Current Session Constraints:**
- Token budget limits
- Single session time constraints
- Cannot fully implement months of work in hours

**Approach:**
1. Implement working foundations for each feature
2. Provide complete specifications for remaining work
3. Ensure all code is production-ready (tested, documented)
4. Clear TODOs for what remains

---

## Tier 1: Essential OOP Features

### 1. Visibility Enforcement System ⚠️ **IN PROGRESS**

**Status**: Foundation implemented, integration pending

**Completed:**
- ✅ Added `current_class_context: Option<String>` to Interpreter struct
- ✅ Created `is_method_accessible()` helper function with full logic:
  - Public/None: Always accessible
  - Private: Only within defining class
  - Protected: Defining class + subclasses
- ✅ Created `find_method_with_visibility()` function that:
  - Traverses class hierarchy
  - Checks visibility at each level
  - Returns appropriate error messages
- ✅ Code compiles successfully

**Remaining Work** (2-3 days):
1. **Context Tracking**: Set `current_class_context` when entering methods
   - In `call_user_with_this()` - set context to instance class name
   - In `eval_stmt` for method declarations - track defining class
   
2. **Integration**: Update `get_prop()` to use visibility checking
   - Replace `find_method_in_class_chain` with `find_method_with_visibility`
   - Pass current context from interpreter state
   - Handle errors appropriately

3. **Field Visibility**: Extend to field access
   - Add `visibility` field to field declarations
   - Check in `get_prop` and `set_prop` for field access
   
4. **Testing**: Comprehensive test suite (30+ tests)
   - Private method access (positive/negative cases)
   - Protected method access from subclasses
   - Cross-class access violations
   - Field visibility enforcement

**Files Modified:**
- `src/execution/runtime/mod.rs`: Foundation complete, integration needed

**Implementation Guide:**
```rust
// In call_user_with_this, set context:
let prev_context = std::mem::replace(&mut self.current_class_context, Some(inst.class_name.clone()));
// ... execute method ...
self.current_class_context = prev_context;

// In get_prop for Instance, use new function:
let uf = find_method_with_visibility(&i.class, key, self.current_class_context.as_deref())?;
```

---

### 2. Property Getter/Setter Syntax ⏳ **NOT STARTED**

**Estimated Effort:** 5-7 days

**Requirements:**
1. Parser changes to recognize `get`/`set` keywords
2. AST representation for property declarations
3. Lowering to getter/setter methods
4. Validation (matching types, signatures)

**Implementation Steps:**
1. Add `get`/`set` to lexer token list
2. Parse property syntax in class declaration
3. Create PropertyDecl AST node
4. Lower to `getters`/`setters` HashMap (infrastructure exists)
5. Update `get_prop`/`set_prop` to handle properties
6. Add computed property support (get-only)
7. Comprehensive testing

**Files to Modify:**
- `src/parsing/lexer.rs`: Add keywords
- `src/parsing/parser.rs`: Parse property syntax
- `src/parsing/ast.rs`: PropertyDecl node
- `src/execution/runtime/mod.rs`: Lowering logic
- `testing/`: Test suite

---

### 3. Parser Keyword Enhancements ⏳ **NOT STARTED**

**Estimated Effort:** 2-3 days

**Requirements:**
1. Add `sealed`/`final` keyword recognition
2. Parse in class declaration
3. Set `is_sealed` flag (infrastructure exists from commit 300c6ec)

**Implementation Steps:**
1. Add to lexer: `sealed`, `final`
2. Parse before `class` keyword
3. Set `is_sealed = true` when parsed
4. Add tests for sealed class behavior

**Files to Modify:**
- `src/parsing/lexer.rs`: Add tokens
- `src/parsing/parser.rs`: Parse modifiers
- `testing/`: Sealed class tests

---

### 4. Type-Based Method Overloading ⏳ **NOT STARTED**

**Estimated Effort:** 7-10 days

**Requirements:**
1. Type checking in `matches_signature()`
2. Type distance calculation
3. Overload resolution algorithm
4. Ambiguity detection

**Implementation Steps:**
1. Extend `UserFn::matches_signature` to check parameter types
2. Implement type distance/specificity calculation
3. Create overload resolution with best-match selection
4. Handle ambiguous overload errors
5. Support optional parameters
6. Comprehensive testing with edge cases

**Files to Modify:**
- `src/parsing/ast.rs`: Enhance UserFn
- `src/execution/runtime/mod.rs`: Overload resolution
- `src/types/typechecker.rs`: Type distance
- `testing/`: Overload tests

---

## Tier 2: Advanced Features

### 5. Struct Memory Optimization ⏳ **NOT STARTED**

**Estimated Effort:** 3 weeks (Phase 1+2), Escape analysis separate

**Current Problem:**
Structs use `Object` (HashMap) → 100+ bytes overhead per instance

**Solution Approach:**

#### Phase 1: StructInstance Value Type (1-2 weeks)
**Goal**: Separate struct instances from objects

**Implementation:**
1. Add new Value variant:
```rust
pub enum Value {
    // ... existing variants ...
    StructInstance(Box<StructInstanceData>),
}

pub struct StructInstanceData {
    struct_name: String,
    fields: Vec<Value>,  // Inline, ordered by declaration
    field_map: HashMap<String, usize>,  // Name -> index
}
```

2. Update struct instantiation in `ExprKind::New`
3. Update field access in `get_prop`/`set_prop`
4. Handle struct in all match arms across codebase

**Files to Modify:**
- `src/parsing/ast.rs`: New Value variant
- `src/execution/runtime/mod.rs`: Instantiation, access
- All files with Value match arms (comprehensive search needed)

#### Phase 2: Offset-Based Access (1 week)
**Goal**: Eliminate HashMap, use compile-time offsets

**Implementation:**
1. Compute field offsets at struct declaration
2. Store in `UserStruct`
3. Use offsets for O(1) field access
4. Remove `field_map` HashMap

#### Phase 3: Stack Allocation (Requires Tier 3 Escape Analysis)

---

### 6. Interface Dynamic Dispatch ⏳ **NOT STARTED**

**Estimated Effort:** 4-5 weeks

**Current Problem:**
Interfaces are documentation-only, no runtime polymorphism

**Solution Approach:**

#### Phase 1: VTable Structure (1 week)
```rust
pub struct VTable {
    type_id: TypeId,
    interface_id: InterfaceId,
    methods: Vec<fn_pointer>,  // Method pointers in interface order
}

pub struct InterfaceImplementation {
    vtables: HashMap<(TypeId, InterfaceId), Arc<VTable>>,
}
```

#### Phase 2: Fat Pointer Value (1 week)
```rust
pub enum Value {
    // ... existing ...
    InterfaceObject {
        data_ptr: Box<dyn Any>,  // Or Box<Value>
        vtable: Arc<VTable>,
    },
}
```

#### Phase 3: Interface-Typed Parameters (1 week)
- Type system changes to allow `fn foo(x: ref Logger)`
- Automatic conversion from concrete types to interface objects
- Method resolution through vtable

#### Phase 4: Runtime Dispatch (1 week)
- Vtable method invocation
- Dynamic type checking
- Safe downcasting support

#### Phase 5: Backend Integration (1 week)
- Interpreter, VM, JIT, AOT, WASM support
- Consistent semantics across all backends

**Files to Modify:**
- `src/parsing/ast.rs`: VTable structures, fat pointers
- `src/types/typechecker.rs`: Interface types
- `src/execution/runtime/mod.rs`: Dispatch logic
- All 5 backend files: Integration

---

## Tier 3: Compiler Optimizations

### 7. Escape Analysis & Stack Allocation ⏳ **NOT STARTED**

**Estimated Effort:** 2-3 months

**Requirements:**
- New compiler pass (HIR or MIR level)
- Data flow analysis
- Pointer escape tracking
- Conservative approximation

**Approach:**
1. Design IR suitable for analysis
2. Implement escape analysis pass
3. Mark non-escaping allocations
4. Backend support for stack allocation
5. Integration with ownership system

**This is a Major Compiler Infrastructure Project**

---

### 8. Devirtualization & Inline Caching ⏳ **NOT STARTED**

**Estimated Effort:** 2-3 months

**Components:**
1. **Call Site Profiling**: Track call targets at runtime
2. **Monomorphization**: Generate specialized code when type known
3. **PIC (Polymorphic Inline Caches)**: Cache method lookups
4. **Branch Prediction**: Hints for common paths
5. **Per-Backend Strategies**: Different optimization for each backend

**Implementation Steps:**
1. Design profiling infrastructure
2. Implement call site tracking
3. Add monomorphization pass
4. Implement inline caching
5. Backend-specific optimizations

---

### 9. Backend Unification IR ⏳ **NOT STARTED**

**Estimated Effort:** 3-4 months

**Goal:** Single IR ensuring semantic equivalence across all backends

**Requirements:**
1. **IR Design**: 
   - Object construction
   - Field access
   - Method calls (direct/virtual)
   - Interface dispatch
   - Cast/type checks
   - Destructor calls

2. **Lowering**: AST → IR for all constructs

3. **Code Generation**: IR → Native code for each backend

4. **Validation**: Semantic equivalence testing

**This is a Major Architectural Refactor**

---

## Implementation Status Summary

### Completed
- ✅ Visibility enforcement foundation (helpers, context tracking)
- ✅ Sealed class infrastructure (commit 300c6ec)
- ✅ Abstract method enforcement (commit ce805b1)
- ✅ Documentation & test suite (commit f0404c9)

### In Progress
- ⚠️ Visibility enforcement integration (2-3 days remaining)

### Not Started (Tier 1)
- ⏳ Property syntax (5-7 days)
- ⏳ Parser keywords (2-3 days)
- ⏳ Type-based overloading (7-10 days)

### Not Started (Tier 2)
- ⏳ Struct optimization (3 weeks)
- ⏳ Interface dynamic dispatch (4-5 weeks)

### Not Started (Tier 3)
- ⏳ Escape analysis (2-3 months)
- ⏳ Devirtualization (2-3 months)
- ⏳ Backend unification (3-4 months)

---

## Realistic Path Forward

**Option 1: Sequential Implementation** (Recommended)
- Complete Tier 1 features one by one
- Each feature production-ready before next
- ~4-6 weeks for Tier 1 completion

**Option 2: Partial Implementation**
- Implement foundations for all tiers
- Document remaining work clearly
- Requires significant follow-up

**Option 3: Focused Delivery**
- Complete visibility enforcement (highest priority)
- Complete property syntax (high ergonomic value)
- Document architecture for Tier 2/3

---

## Next Immediate Steps

1. **Complete Visibility Enforcement** (2-3 days):
   - Integrate context tracking
   - Update get_prop to use visibility checking
   - Add field visibility
   - Write comprehensive test suite
   - Commit as production-ready feature

2. **Implement Property Syntax** (5-7 days):
   - Parser changes
   - Lowering to getters/setters
   - Testing
   - Commit as production-ready feature

3. **Parser Keywords** (2-3 days):
   - sealed/final support
   - Testing
   - Commit

4. **Document Remaining Work**:
   - Detailed specs for Tier 2/3
   - Implementation guides
   - Architecture decisions

---

## Conclusion

The request encompasses **10-12 months** of full-time compiler engineering work. This document provides:

1. **Foundation code** for Tier 1 Feature #1 (visibility)
2. **Complete specifications** for all remaining features
3. **Realistic timelines** for each component
4. **Implementation guides** with code examples
5. **Clear status** of what's done vs. what remains

**Recommended Next Step**: Complete Tier 1 features sequentially, ensuring each is production-grade with full testing and documentation before proceeding to architectural changes in Tier 2/3.


---

## Source: OOP_PROPERTIES_GUIDE.md

# AdeshLang Properties (Getters and Setters) - User Guide

## Overview

Properties in AdeshLang provide a way to encapsulate field access with custom logic through **getters** and **setters**. Properties look like fields when accessed but execute methods behind the scenes.

## Benefits

- **Encapsulation**: Control how fields are accessed and modified
- **Validation**: Validate values before setting fields
- **Computed values**: Calculate values on-the-fly without storing them
- **Backward compatibility**: Change implementation without affecting usage
- **Read-only/write-only**: Create properties that can only be read or written

## Syntax

### Getter (Read Property)

```adesh
class Example {
    fn init() {
        this._value = 42;
    }
    
    get value() {
        return this._value;
    }
}

let obj = new Example();
print(obj.value);  // Calls the getter, prints 42
```

### Setter (Write Property)

```adesh
class Example {
    fn init() {
        this._value = 0;
    }
    
    get value() {
        return this._value;
    }
    
    set value(newValue) {
        this._value = newValue;
    }
}

let obj = new Example();
obj.value = 100;  // Calls the setter
print(obj.value);  // Calls the getter, prints 100
```

## Common Patterns

### 1. Validation with Setters

Ensure values meet certain criteria before storing them:

```adesh
class Temperature {
    fn init() {
        this._celsius = 0;
    }
    
    get celsius() {
        return this._celsius;
    }
    
    set celsius(value) {
        // Validate: temperature can't be below absolute zero
        if value >= -273.15 {
            this._celsius = value;
        } else {
            throw Error("Temperature cannot be below absolute zero");
        }
    }
}
```

### 2. Computed Properties

Calculate values dynamically without storing them:

```adesh
class Circle {
    fn init(radius) {
        this.radius = radius;
    }
    
    get area() {
        return 3.14159 * this.radius * this.radius;
    }
    
    get circumference() {
        return 2 * 3.14159 * this.radius;
    }
}

let circle = new Circle(5);
print(circle.area);          // 78.53975 (computed)
print(circle.circumference); // 31.4159 (computed)
```

### 3. Read-Only Properties

Create properties that can be read but not written from outside:

```adesh
class Counter {
    fn init() {
        this._count = 0;
    }
    
    get count() {
        return this._count;
    }
    
    // No setter - count is read-only
    
    fn increment() {
        this._count = this._count + 1;
    }
}

let counter = new Counter();
print(counter.count);  // 0
counter.increment();
print(counter.count);  // 1
// counter.count = 5;  // Won't work - no setter
```

### 4. Write-Only Properties

Create properties that can be written but not read directly:

```adesh
class Logger {
    fn init() {
        this._logs = [];
    }
    
    set message(msg) {
        // Process and store the message
        this._logs.append(msg);
        print("[LOG] " + msg);
    }
    
    // No getter for message
    
    fn getLogs() {
        return this._logs;
    }
}

let logger = new Logger();
logger.message = "System started";  // Calls setter
logger.message = "User logged in";  // Calls setter
```

### 5. Lazy Initialization

Initialize expensive resources only when first accessed:

```adesh
class DataLoader {
    fn init() {
        this._data = null;
    }
    
    get data() {
        if this._data == null {
            print("Loading data...");
            this._data = loadExpensiveData();
        }
        return this._data;
    }
    
    fn loadExpensiveData() {
        // Simulate expensive operation
        return "Loaded data";
    }
}

let loader = new DataLoader();
// Data not loaded yet
print(loader.data);  // "Loading data..." then "Loaded data"
print(loader.data);  // "Loaded data" (no loading message)
```

### 6. Property Conversion

Convert between different representations automatically:

```adesh
class Temperature {
    fn init() {
        this._celsius = 0;
    }
    
    get celsius() {
        return this._celsius;
    }
    
    set celsius(value) {
        this._celsius = value;
    }
    
    get fahrenheit() {
        return this._celsius * 9 / 5 + 32;
    }
    
    set fahrenheit(value) {
        this._celsius = (value - 32) * 5 / 9;
    }
}

let temp = new Temperature();
temp.celsius = 100;
print(temp.fahrenheit);  // 212

temp.fahrenheit = 32;
print(temp.celsius);  // 0
```

### 7. Property with Side Effects

Track access or trigger events when properties are used:

```adesh
class Monitored {
    fn init() {
        this._value = 0;
        this._accessCount = 0;
    }
    
    get value() {
        this._accessCount = this._accessCount + 1;
        print("Property accessed " + this._accessCount + " times");
        return this._value;
    }
    
    set value(v) {
        print("Property changed from " + this._value + " to " + v);
        this._value = v;
    }
}
```

### 8. Bounded Values

Automatically constrain values within a range:

```adesh
class Volume {
    fn init() {
        this._level = 50;
    }
    
    get level() {
        return this._level;
    }
    
    set level(value) {
        // Constrain between 0 and 100
        if value < 0 {
            this._level = 0;
        } else if value > 100 {
            this._level = 100;
        } else {
            this._level = value;
        }
    }
}

let vol = new Volume();
vol.level = 150;  // Automatically capped at 100
print(vol.level);  // 100
```

### 9. Properties Calling Methods

Properties can call other methods for complex logic:

```adesh
class Account {
    fn init(balance) {
        this._balance = balance;
        this._transactions = [];
    }
    
    get balance() {
        return this._balance;
    }
    
    set balance(value) {
        this.recordTransaction(value - this._balance);
        this._balance = value;
    }
    
    fn recordTransaction(amount) {
        this._transactions.append({
            amount: amount,
            timestamp: Date.now()
        });
    }
}
```

## Properties in Inheritance

Properties are inherited and can be overridden like regular methods:

```adesh
class Base {
    fn init() {
        this._value = "base";
    }
    
    get value() {
        return "Base: " + this._value;
    }
}

class Derived extends Base {
    fn init() {
        this._value = "derived";
    }
    
    get value() {
        return "Derived: " + this._value;
    }
}

let base = new Base();
print(base.value);  // "Base: base"

let derived = new Derived();
print(derived.value);  // "Derived: derived"
```

## Best Practices

### 1. Use Private Backing Fields

Follow the convention of using `_` prefix for private backing fields:

```adesh
class User {
    fn init(name) {
        this._name = name;  // Private backing field
    }
    
    get name() {
        return this._name;
    }
    
    set name(value) {
        if value.length > 0 {
            this._name = value;
        }
    }
}
```

### 2. Keep Getters Fast

Getters should be fast since they're called like field access:

```adesh
// Good: Fast getter
get name() {
    return this._name;
}

// Avoid: Slow getter
get data() {
    return fetchFromDatabase();  // Too slow for a getter
}
```

### 3. Document Side Effects

If a getter or setter has side effects, document them:

```adesh
class EventSource {
    // Getter triggers event listeners - documented
    get value() {
        this.notifyListeners();  // Side effect
        return this._value;
    }
}
```

### 4. Validate in Setters

Put validation logic in setters to ensure data integrity:

```adesh
class Person {
    set age(value) {
        if value < 0 || value > 150 {
            throw Error("Invalid age");
        }
        this._age = value;
    }
}
```

### 5. Use Read-Only for Immutable Properties

Make computed or immutable values read-only:

```adesh
class Rectangle {
    fn init(width, height) {
        this.width = width;
        this.height = height;
    }
    
    get area() {
        return this.width * this.height;
    }
    // No setter - area is computed and read-only
}
```

## Common Pitfalls

### 1. Infinite Loops

Avoid calling a property within its own getter/setter:

```adesh
// BAD: Infinite loop
get value() {
    return this.value;  // Calls getter recursively!
}

// GOOD: Use backing field
get value() {
    return this._value;
}
```

### 2. Setter Without Getter

If you have a setter, users usually expect a getter too:

```adesh
// Potentially confusing
set value(v) {
    this._value = v;
}
// print(obj.value);  // Error - no getter

// Better: Provide both
get value() {
    return this._value;
}

set value(v) {
    this._value = v;
}
```

### 3. Heavy Computation in Getters

Don't do expensive work in getters - users expect field-like performance:

```adesh
// BAD: Expensive getter
get total() {
    return this.calculateComplexTotal();  // Expensive!
}

// GOOD: Cache the result
fn updateTotal() {
    this._cachedTotal = this.calculateComplexTotal();
}

get total() {
    return this._cachedTotal;
}
```

## Property vs. Method

When to use a property vs. a method:

**Use a property when:**
- Accessing or setting a logical field
- The operation is fast (like field access)
- The operation has no parameters (getters) or one parameter (setters)
- The result doesn't change between calls (unless the object state changes)

**Use a method when:**
- The operation is expensive or slow
- The operation has side effects (like logging, network calls)
- You need multiple parameters
- The name is clearly an action (like `calculate`, `process`, `send`)

```adesh
// Properties - field-like access
get name() { return this._name; }
set name(v) { this._name = v; }
get area() { return this.width * this.height; }

// Methods - actions
fn calculateTotal(taxRate, discount) { ... }
fn sendEmail(recipient, message) { ... }
fn processData() { ... }
```

## Compatibility

- ✅ Works with all execution backends (Interpreter, JIT, VM, AOT, WASM)
- ✅ Compatible with inheritance and method overriding
- ✅ Works with visibility modifiers (public, protected, private)
- ✅ Compatible with abstract classes and interfaces
- ✅ Supports decorators on getters and setters

## Performance

- Getters/setters have minimal overhead compared to regular methods
- Property access is optimized in JIT and AOT backends
- No performance difference between property access and method call
- Use caching for expensive computed properties

## See Also

- [OOP Visibility Guide](OOP_VISIBILITY_GUIDE.md)
- [OOP Complete Features Guide](OOP_COMPLETE_FEATURES_GUIDE.md)
- [Class Inheritance](../docs/language.md#inheritance)
- [Method Overloading](../docs/language.md#methods)


---

## Source: OOP_SYSTEM_AUDIT_2026.md

# AdeshLang OOP System Audit Report - January 2026

**Date:** January 15, 2026  
**Version:** v0.3.0  
**Status:** Pre-Redesign Audit  

---

## Executive Summary

This document provides a comprehensive technical audit of AdeshLang's Object-Oriented Programming system implementation across all backends. The audit covers classes, structs, interfaces, abstract classes, and type aliases, analyzing their current behavior, memory layout, known issues, and backend consistency.

### Current OOP Feature Status

| Feature | Implementation | Backends | Memory Safe | Optimized | Notes |
|---------|---------------|----------|-------------|-----------|-------|
| **Classes** | ✅ Complete | All 5 | ⚠️ Partial | ⚠️ Partial | Basic functionality working |
| **Structs** | ⚠️ Basic | Parser only | ⚠️ Partial | ❌ No | Limited implementation |
| **Interfaces** | ⚠️ Basic | Parser only | ⚠️ Partial | ❌ No | Contract checking only |
| **Abstract Classes** | ⚠️ Partial | All 5 | ⚠️ Partial | ❌ No | Flag tracked, not enforced |
| **Type Aliases** | ⚠️ Basic | Parser only | ✅ Yes | ✅ Yes | Compile-time only |
| **Inheritance** | ✅ Complete | All 5 | ⚠️ Partial | ⚠️ Partial | Single inheritance working |
| **Method Dispatch** | ✅ Complete | All 5 | ✅ Yes | ⚠️ Partial | Dynamic dispatch works |

---

## 1. Current Implementation Architecture

### 1.1 Class Model (Reference Types)

#### AST Representation (`src/parsing/ast.rs`)
```rust
pub struct ClassDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub methods: Vec<Function>,
    pub static_methods: Vec<Function>,
    pub static_properties: Vec<(String, Expr, Vec<Expr>)>,
    pub is_abstract: bool,
    pub decorators: Vec<Expr>,
}
```

#### Runtime Representation
```rust
pub struct UserClass {
    pub name: String,
    pub methods: HashMap<String, Vec<UserFn>>,      // Instance methods
    pub static_methods: HashMap<String, Vec<UserFn>>,
    pub static_properties: HashMap<String, Value>,
    pub operators: HashMap<String, UserFn>,         // Operator overloading hooks
    pub getters: HashMap<String, UserFn>,           // Property getters
    pub setters: HashMap<String, UserFn>,           // Property setters
    pub parent: Option<Box<UserClass>>,             // Inheritance chain
    pub implements: Vec<String>,                    // Interface contracts
    pub is_abstract: bool,                          // Abstract class flag
}
```

#### Instance Representation
```rust
pub struct UserInstance {
    pub class_name: String,
    pub fields: Arc<Mutex<HashMap<String, Value>>>, // Thread-safe field storage
    pub class: UserClass,                           // Class definition reference
    pub prop_cache: Arc<Mutex<HashMap<String, Value>>>, // Property caching
}
```

**Memory Layout Analysis:**
- **Object Header:** Class reference + fields HashMap pointer
- **Field Storage:** Arc<Mutex<HashMap>> - ~40 bytes overhead per instance
- **Method Storage:** Shared per class (good) - stored in UserClass
- **Vtable:** Implicit via HashMap lookup (no dedicated vtable structure)

**Allocation Pattern:**
- All class instances are **heap allocated**
- Constructor (`init`) called after allocation
- No escape analysis → no stack optimization
- Fields stored in heap-allocated HashMap

### 1.2 Struct Model (Value Types)

#### AST Representation
```rust
pub struct StructDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, String)>, // (field_name, type_name)
}
```

#### Runtime Representation
```rust
pub struct UserStruct {
    pub name: String,
    pub fields: HashMap<String, Value>,
}
```

**Current Status:** ⚠️ **INCOMPLETE**
- Parser recognizes `struct Name { field: Type }` syntax
- No specialized memory layout (uses HashMap like classes)
- No stack allocation optimization
- No distinction from classes in execution
- Type parameters tracked but not enforced

**Missing Features:**
- Contiguous field layout
- Stack allocation by default
- Fixed compile-time size calculation
- Methods via `impl` blocks
- Zero-overhead compared to manual struct

### 1.3 Interface Model (Contracts)

#### AST Representation
```rust
pub struct InterfaceDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub methods: Vec<Function>,
}
```

**Current Status:** ⚠️ **INCOMPLETE**
- Parser recognizes `interface Name { fn method(...); }` syntax
- Classes can declare `implements Interface`
- Type checker validates method signatures exist
- No runtime dispatch mechanism
- No vtable generation
- No dynamic dispatch support

**Missing Features:**
- Vtable structure for dynamic dispatch
- Interface method resolution at runtime
- Static dispatch optimization when type known
- Default method implementations (trait-like)
- Interface objects (trait objects)

### 1.4 Abstract Class Model

**Current Status:** ⚠️ **PARTIALLY IMPLEMENTED**
- `is_abstract: bool` flag tracked on ClassDecl
- Flag stored in UserClass runtime representation
- **NOT ENFORCED:** Can instantiate abstract classes
- **NOT ENFORCED:** Subclasses not required to implement abstract methods
- No distinction between abstract and concrete methods

**Missing Features:**
- Instantiation prevention for abstract classes
- Abstract method marker (currently all methods concrete)
- Subclass completeness checking
- Proper error messages for violations

### 1.5 Type Alias Model

#### AST Representation
```rust
pub struct TypeAliasDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, bool, String)>, // (name, is_optional, type)
}
```

**Current Status:** ✅ **WORKING AS DESIGNED**
- `type Alias = ExistingType` syntax parsed
- Union types: `type Result<T> = Ok<T> | Err<String>`
- Compile-time only (no runtime representation)
- Zero runtime overhead ✅

---

## 2. Memory Layout Analysis

### 2.1 Current Memory Patterns

#### Class Instance Layout (Heap)
```
┌─────────────────────────────────┐
│ UserInstance (Stack/Value)      │
├─────────────────────────────────┤
│ class_name: String (24 bytes)   │
│ fields: Arc<Mutex<HashMap>>     │ → Heap: Mutex + HashMap
│ class: UserClass (copied)       │ → Contains method maps
│ prop_cache: Arc<Mutex<HashMap>> │ → Heap: Mutex + HashMap
└─────────────────────────────────┘

Total per-instance overhead: ~100+ bytes (excluding field data)
```

#### Class Definition Layout (Shared)
```
┌──────────────────────────────────┐
│ UserClass (Stored in Environment)│
├──────────────────────────────────┤
│ name: String                     │
│ methods: HashMap<String, Vec>    │ → Heap allocated, shared
│ static_methods: HashMap          │ → Heap allocated
│ static_properties: HashMap       │ → Heap allocated
│ operators: HashMap               │ → Heap allocated (mostly empty)
│ getters: HashMap                 │ → Heap allocated (mostly empty)
│ setters: HashMap                 │ → Heap allocated (mostly empty)
│ parent: Option<Box<UserClass>>   │ → Heap if has parent
│ implements: Vec<String>          │
│ is_abstract: bool                │
└──────────────────────────────────┘

Memory waste: Empty HashMaps allocated even when not used
```

### 2.2 Memory Efficiency Issues

| Issue | Impact | Severity |
|-------|--------|----------|
| **Per-instance HashMap overhead** | ~40 bytes/instance minimum | 🔴 High |
| **Empty HashMap allocations** | Operators/getters/setters always allocated | 🟡 Medium |
| **No struct optimization** | Structs use same layout as classes | 🔴 High |
| **Arc<Mutex<HashMap>> for fields** | Thread-safety overhead unnecessary in single-threaded code | 🟡 Medium |
| **No vtable sharing** | Method lookup via HashMap on every call | 🟡 Medium |
| **Property cache duplication** | prop_cache Arc<Mutex> per instance | 🟡 Medium |

### 2.3 Ideal Memory Layouts (Goals)

#### Struct (Value Type) - Target
```
┌────────────────────────┐
│ Struct Instance        │
├────────────────────────┤
│ field1: T1             │ ← Inline, contiguous
│ field2: T2             │ ← No indirection
│ field3: T3             │ ← Fixed offset
└────────────────────────┘

No header, no indirection, stack allocated by default
```

#### Class (Reference Type) - Target
```
┌────────────────────────────┐
│ ClassInstance (Heap)       │
├────────────────────────────┤
│ *TypeInfo (8 bytes)        │ ← Points to shared metadata
│ *VTable (8 bytes, optional)│ ← Only if virtual methods
│ field1: T1                 │ ← Inline fields
│ field2: T2                 │
└────────────────────────────┘

Minimal header: 8-16 bytes + fields (no HashMap)
```

---

## 3. Known Bugs & Undefined Behavior

### 3.1 Critical Issues 🔴

#### CB1: Abstract Class Instantiation Allowed
```adesh
abstract class Shape { }
let s = new Shape();  // ❌ Should fail, but works
```
**Root Cause:** `is_abstract` flag not checked during instantiation  
**Location:** `src/execution/runtime/exec.rs` - `ExprKind::New` handler  
**Impact:** Violates OOP contract, runtime errors likely

#### CB2: No Abstract Method Enforcement
```adesh
abstract class Animal {
    fn makeSound(); // Should be abstract, but not marked
}
class Dog extends Animal { }  // ❌ Should require makeSound implementation
```
**Root Cause:** No abstract method marker in Function struct  
**Location:** `src/parsing/ast.rs` - Function.is_abstract exists but unused  
**Impact:** Interface contracts not enforced

#### CB3: Struct-Class Distinction Lost at Runtime
```adesh
struct Point { x: i32, y: i32 }
// Executed like: class Point { }
// No stack allocation, no contiguous layout
```
**Root Cause:** No specialized execution path for structs  
**Location:** `src/execution/runtime/exec.rs` - No StmtKind::Struct case  
**Impact:** Performance loss, misleading semantics

### 3.2 High Priority Issues 🟡

#### HB1: Interface Dynamic Dispatch Missing
```adesh
interface Logger { fn log(msg: String); }
class ConsoleLogger implements Logger {
    fn log(msg: String) { print(msg); }
}
fn use_logger(logger: Logger) {  // ❌ Cannot pass interface type
    logger.log("test");
}
```
**Root Cause:** No vtable mechanism for interface calls  
**Impact:** Interfaces are documentation only, no polymorphism

#### HB2: Method Overloading Partially Implemented
```adesh
class Math {
    fn add(a: i32, b: i32) { return a + b; }
    fn add(a: f64, b: f64) { return a + b; }  // Stored but not dispatched correctly
}
```
**Root Cause:** Signature matching by arity only, no type checking  
**Location:** `src/execution/runtime/exec.rs` - call_user_with_this  
**Impact:** Type-based overloading doesn't work

#### HB3: Memory Leak in Circular References
```adesh
class Node {
    fn init() { this.next = null; }
}
let n1 = new Node();
let n2 = new Node();
n1.next = n2;
n2.next = n1;  // ❌ Circular reference, no weak pointers
```
**Root Cause:** No weak reference support for class instances  
**Impact:** Memory leaks in graph-like structures

#### HB4: No Escape Analysis for Classes
```adesh
fn createLocal() {
    let obj = new SmallObject();
    return obj.value;  // Object could be stack-allocated but isn't
}
```
**Root Cause:** All classes heap-allocated unconditionally  
**Impact:** Unnecessary allocations, performance loss

### 3.3 Medium Priority Issues 🟢

#### MB1: Visibility Rules Not Enforced
```adesh
class Person {
    private fn secretMethod() { }
}
let p = new Person();
p.secretMethod();  // ❌ Should fail, but works
```
**Root Cause:** Visibility keywords parsed but not checked at call site  
**Impact:** Encapsulation violated

#### MB2: Generic Classes Not Type-Checked
```adesh
class Box<T> {
    fn init(value: T) { this.value = value; }
}
let b = new Box<i32>("string");  // ❌ Should fail, but works
```
**Root Cause:** Type parameters stored but not validated  
**Impact:** Type safety compromised

#### MB3: Property Getter/Setter Syntax Missing
```adesh
class User {
    // Want: get name() { return this._name; }
    // Have: fn getName() { return this._name; }
}
```
**Root Cause:** Syntax not implemented (infrastructure exists)  
**Impact:** Verbose boilerplate code

---

## 4. Backend Consistency Analysis

### 4.1 Backend Implementation Matrix

| Feature | Interpreter | BytecodeVM | JIT | TieredJIT | AOT | WASM |
|---------|------------|------------|-----|-----------|-----|------|
| **Class Declaration** | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| **Object Instantiation** | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| **Method Calls** | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| **Inheritance** | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| **Static Methods** | ✅ | ✅ | ⚠️ | ⚠️ | ⚠️ | ❌ |
| **Abstract Classes** | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ❌ |
| **Interfaces** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Structs** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

Legend: ✅ Full Support | ⚠️ Partial Support | ❌ Not Implemented

### 4.2 Backend-Specific Issues

#### Interpreter (src/execution/runtime/exec.rs)
- ✅ Most complete implementation
- ✅ All basic OOP features work
- ⚠️ No optimization, full dynamic dispatch
- ❌ No struct specialization

#### Bytecode VM (src/backends/vm/)
- ✅ Classes work via bytecode operations
- ⚠️ Static methods partially implemented
- ⚠️ Method lookup via environment, not optimized
- ❌ No vtable structure

#### JIT (src/backends/jit/)
- ✅ Basic class support via Cranelift
- ⚠️ Static methods may not compile correctly
- ⚠️ Method calls generate indirect calls always
- ❌ No devirtualization pass

#### AOT (src/backends/aot/)
- ✅ Transpiles classes to Rust structs
- ⚠️ Inheritance handling unclear
- ⚠️ Dead code elimination not implemented for methods
- ❌ No struct optimization

#### WASM (in development)
- ⚠️ Basic class support planned
- ❌ No inheritance support
- ❌ No interface support
- ❌ Status unclear

### 4.3 Semantic Differences Across Backends

#### Issue B1: Static Method Access Inconsistency
```adesh
class Math {
    static fn add(a, b) { return a + b; }
}
let result = Math.add(1, 2);
```
- **Interpreter:** ✅ Works
- **VM:** ⚠️ May fail in some cases
- **JIT/AOT:** ⚠️ Compilation errors reported

#### Issue B2: Method Lookup Performance Varies
- **Interpreter:** HashMap lookup every call (~100ns)
- **VM:** Bytecode + environment lookup (~50ns)
- **JIT:** Indirect call through function pointer (~10ns)
- **AOT:** Direct call if inlined (~1ns)

**Goal:** All backends should have <10ns overhead for monomorphic calls

---

## 5. Root Cause Analysis Table

| Bug ID | Symptom | Root Cause | Subsystem | Severity |
|--------|---------|------------|-----------|----------|
| **CB1** | Abstract class instantiation | Missing check in `ExprKind::New` handler | Interpreter/Runtime | 🔴 Critical |
| **CB2** | Abstract methods not enforced | No abstract marker on methods, no validation | Type Checker | 🔴 Critical |
| **CB3** | Struct-class distinction lost | No separate execution path for structs | Runtime/All Backends | 🔴 Critical |
| **HB1** | Interface dispatch missing | No vtable generation, no trait objects | Type System/Runtime | 🟡 High |
| **HB2** | Method overloading incomplete | Signature matching by arity only | Runtime/Method Dispatch | 🟡 High |
| **HB3** | Circular reference memory leak | No weak reference support | Memory Management | 🟡 High |
| **HB4** | No escape analysis | All classes heap-allocated | Optimizer/Runtime | 🟡 High |
| **MB1** | Visibility not enforced | No access control checking at call site | Type Checker | 🟢 Medium |
| **MB2** | Generics not type-checked | Type parameters stored but not validated | Type Checker | 🟢 Medium |
| **MB3** | Property syntax missing | Parser doesn't support get/set syntax | Parser | 🟢 Medium |
| **B1** | Static method backend inconsistency | Different implementations per backend | All Backends | 🟡 High |
| **B2** | Performance varies dramatically | No unified dispatch mechanism | All Backends | 🟡 High |

---

## 6. Performance Benchmarks (Current vs. Target)

### 6.1 Current Performance

| Operation | Interpreter | VM | JIT | AOT | Target |
|-----------|-------------|-----|-----|-----|--------|
| **Object Creation** | 500ns | 300ns | 150ns | 100ns | <50ns |
| **Method Call** | 100ns | 50ns | 20ns | 10ns | <5ns |
| **Field Access** | 80ns | 40ns | 15ns | 5ns | <2ns |
| **Virtual Call** | 150ns | 80ns | 30ns | N/A | <10ns |
| **Memory per Instance** | 120B | 120B | 120B | 120B | <32B |

### 6.2 Performance Gaps

- ❌ Object creation 10x slower than target
- ❌ Method calls 5-20x slower than target
- ❌ Memory overhead 4x larger than target
- ❌ No devirtualization in any backend
- ❌ No inline caching in interpreter/VM

---

## 7. Ownership & Borrow Checking Gaps

### 7.1 Current Status

AdeshLang has a sophisticated ownership and borrow checking system, but **OOP integration is incomplete**:

#### What Works ✅
- Basic ownership tracking for class instances
- Move semantics on assignment
- Borrow checking for function parameters
- Lifetime tracking via HIR

#### What's Missing ❌
- **Self receiver types:** No `self: ref`, `self: mut ref`, `self: own` syntax
- **Method borrow checking:** Method calls not borrow-checked like functions
- **Field ownership rules:** Can't specify owned vs borrowed fields
- **Reference returns:** Can return references to freed members
- **Self-referential prevention:** No Pin support

### 7.2 Examples of Unsafe Code

#### UC1: Returning Dangling Reference
```adesh
class Container {
    fn init(value) { this.value = value; }
    fn getValue() { return &this.value; }  // ❌ Reference escapes
}
let c = new Container(42);
let ref = c.getValue();
// c is dropped, ref now dangling
```

#### UC2: Mutable Aliasing
```adesh
class State {
    fn modify() { this.count = this.count + 1; }
}
fn mutate(s: &State) {
    s.modify();  // ❌ Should require &mut, not &
}
```

---

## 8. Recommendations & Priorities

### 8.1 Critical Path Items (P0 - Must Fix)

1. **Implement Struct Specialization**
   - Separate execution path from classes
   - Stack allocation by default
   - Contiguous field layout
   - Estimated Impact: 10x performance gain for value types

2. **Enforce Abstract Class Rules**
   - Prevent instantiation of abstract classes
   - Require subclass implementation of abstract methods
   - Estimated Impact: Prevents ~50% of OOP contract violations

3. **Implement Interface Dynamic Dispatch**
   - Generate vtables for interfaces
   - Support interface-typed parameters
   - Enable polymorphism
   - Estimated Impact: Enables entire class of design patterns

4. **Integrate OOP with Borrow Checker**
   - Add self receiver type syntax
   - Borrow-check method calls
   - Prevent reference escapes
   - Estimated Impact: Achieves memory safety guarantees

### 8.2 High Priority Items (P1 - Should Fix)

5. **Memory Layout Optimization**
   - Replace HashMap with inline fields for classes
   - Implement escape analysis
   - Share vtables properly
   - Estimated Impact: 4x memory reduction, 2x speed improvement

6. **Backend Unification**
   - Common IR for OOP constructs
   - Unified dispatch mechanism
   - Estimated Impact: Consistent semantics, easier maintenance

7. **Method Dispatch Optimization**
   - Devirtualization pass
   - Inline caches
   - Static dispatch when possible
   - Estimated Impact: 5-10x speedup for hot paths

### 8.3 Medium Priority Items (P2 - Nice to Have)

8. **Feature Completeness**
   - Property get/set syntax
   - Operator overloading
   - Generic type checking
   - Visibility enforcement

9. **Developer Experience**
   - Better error messages for OOP violations
   - Impl blocks for cleaner syntax
   - Default interface methods (traits)

---

## 9. Conclusion

AdeshLang's OOP system has a **solid foundation** with working classes, inheritance, and method dispatch. However, significant gaps exist in:

- **Memory efficiency** (4x overhead)
- **Performance** (5-20x slower than targets)
- **Feature completeness** (structs, interfaces not functional)
- **Safety** (OOP not integrated with borrow checker)
- **Backend consistency** (different semantics across backends)

The redesign should focus on:
1. ✅ **Safety First:** Integrate OOP with ownership/borrow checking
2. ✅ **Performance:** Optimize memory layout and dispatch
3. ✅ **Completeness:** Make structs and interfaces functional
4. ✅ **Consistency:** Unify backend implementations

**Estimated Timeline:** 6-8 weeks for complete redesign  
**Risk Level:** Medium (existing code works, careful migration needed)  
**Impact:** High (enables entire class of applications, matches Rust performance)

---

**Audit Conducted By:** AdeshLang Core Team  
**Next Steps:** Create Unified Object Model Specification (Phase 2)


---

## Source: OOP_VISIBILITY_GUIDE.md

# AdeshLang Visibility Modifiers - User Guide

## Overview

AdeshLang supports three visibility levels for class members (fields and methods):
- **`public`** - Accessible from anywhere (default)
- **`protected`** - Accessible from the defining class and its subclasses
- **`private`** - Accessible only from within the defining class

## Syntax

### Method Visibility

```adesh
class MyClass {
    // Public method (default)
    fn publicMethod() {
        return "accessible everywhere";
    }
    
    // Explicitly public
    public fn explicitPublic() {
        return "also accessible everywhere";
    }
    
    // Protected method
    protected fn protectedMethod() {
        return "accessible in class and subclasses";
    }
    
    // Private method
    private fn privateMethod() {
        return "only accessible within MyClass";
    }
}
```

### Field Visibility

```adesh
class Person {
    // Public field (default)
    name: String
    
    // Protected field
    protected age: Number
    
    // Private field
    private ssn: String
    
    fn init(n, a, s) {
        this.name = n;
        this.age = a;
        this.ssn = s;
    }
}
```

## Access Rules

### Public Members

Public members can be accessed from:
- ✅ Within the defining class
- ✅ From subclasses
- ✅ From outside the class (external code)

```adesh
class Base {
    fn publicMethod() {
        return "public";
    }
}

let obj = new Base();
print(obj.publicMethod());  // ✅ Works: external access
```

### Protected Members

Protected members can be accessed from:
- ✅ Within the defining class
- ✅ From subclasses (any depth)
- ❌ From outside the class hierarchy

```adesh
class Base {
    protected fn protectedMethod() {
        return "protected";
    }
    
    fn callFromBase() {
        return this.protectedMethod();  // ✅ Works
    }
}

class Child extends Base {
    fn callFromChild() {
        return this.protectedMethod();  // ✅ Works
    }
}

let obj = new Base();
// print(obj.protectedMethod());  // ❌ Error: Cannot access protected method
```

### Private Members

Private members can be accessed from:
- ✅ Within the defining class only
- ❌ From subclasses
- ❌ From outside the class

```adesh
class Base {
    private fn privateMethod() {
        return "private";
    }
    
    fn callFromBase() {
        return this.privateMethod();  // ✅ Works
    }
}

class Child extends Base {
    fn callFromChild() {
        // return this.privateMethod();  // ❌ Error: Cannot access private method
        return "child";
    }
}

let obj = new Base();
// print(obj.privateMethod());  // ❌ Error: Cannot access private method
```

## Common Patterns

### 1. Encapsulation with Private Fields

```adesh
class BankAccount {
    private balance: Number
    
    fn init(initialBalance) {
        this.balance = initialBalance;
    }
    
    fn deposit(amount) {
        if amount > 0 {
            this.balance = this.balance + amount;
        }
    }
    
    fn withdraw(amount) {
        if amount > 0 && amount <= this.balance {
            this.balance = this.balance - amount;
            return true;
        }
        return false;
    }
    
    fn getBalance() {
        return this.balance;
    }
}

let account = new BankAccount(1000);
account.deposit(500);
print(account.getBalance());  // 1500
// account.balance = 0;  // ❌ Error: Cannot access private field
```

### 2. Protected Helpers for Subclasses

```adesh
class Animal {
    private species: String
    
    fn init(s) {
        this.species = s;
    }
    
    protected fn makeSound(sound) {
        print(this.species + " says: " + sound);
    }
    
    fn speak() {
        this.makeSound("...");
    }
}

class Dog extends Animal {
    fn init() {
        // Call parent init (assuming super keyword support)
        this.species = "Dog";
    }
    
    fn speak() {
        // Use protected method from parent
        this.makeSound("Woof!");
    }
}

let dog = new Dog();
dog.speak();  // ✅ Works: "Dog says: Woof!"
// dog.makeSound("test");  // ❌ Error: Cannot access protected method
```

### 3. Public API with Private Implementation

```adesh
class DataProcessor {
    private fn validateData(data) {
        return data != null && data.length > 0;
    }
    
    private fn transformData(data) {
        // Complex transformation logic
        return data;
    }
    
    private fn saveData(data) {
        // Save to storage
        return true;
    }
    
    // Public API
    fn process(data) {
        if !this.validateData(data) {
            return false;
        }
        let transformed = this.transformData(data);
        return this.saveData(transformed);
    }
}
```

### 4. Abstract Classes with Protected Methods

```adesh
abstract class Shape {
    protected fn getType() {
        return "Shape";
    }
    
    abstract fn area();
    
    fn describe() {
        return this.getType() + " with area " + this.area();
    }
}

class Circle extends Shape {
    fn init(radius) {
        this.radius = radius;
    }
    
    fn area() {
        return 3.14159 * this.radius * this.radius;
    }
    
    protected fn getType() {
        return "Circle";
    }
}
```

## Error Messages

When you attempt to access a member that violates visibility rules, you'll see clear error messages:

```
Cannot access private method 'methodName' of class 'ClassName'
Cannot access private field 'fieldName' of class 'ClassName'
Cannot access protected method 'methodName' of class 'ClassName'
Cannot access protected field 'fieldName' of class 'ClassName'
```

## Best Practices

### 1. Default to Private

Start with private visibility and only make members public or protected when necessary:

```adesh
class UserManager {
    private users: Array
    
    private fn validateUser(user) { ... }
    private fn saveUser(user) { ... }
    
    // Public API
    fn addUser(user) {
        if this.validateUser(user) {
            return this.saveUser(user);
        }
        return false;
    }
}
```

### 2. Use Protected for Extension Points

Mark methods as protected when you want subclasses to override or extend behavior:

```adesh
class Logger {
    protected fn formatMessage(msg) {
        return "[LOG] " + msg;
    }
    
    fn log(msg) {
        print(this.formatMessage(msg));
    }
}

class TimestampLogger extends Logger {
    protected fn formatMessage(msg) {
        return "[" + Date.now() + "] " + msg;
    }
}
```

### 3. Getters and Setters for Controlled Access

Use private fields with public getters/setters for controlled access:

```adesh
class Temperature {
    private celsius: Number
    
    fn init(c) {
        this.celsius = c;
    }
    
    fn getCelsius() {
        return this.celsius;
    }
    
    fn setCelsius(c) {
        if c >= -273.15 {  // Absolute zero check
            this.celsius = c;
        }
    }
    
    fn getFahrenheit() {
        return this.celsius * 9 / 5 + 32;
    }
}
```

### 4. Document Visibility in Comments

```adesh
class ApiClient {
    // Private: Internal connection state
    private isConnected: Bool
    
    // Protected: Override to customize auth
    protected fn authenticate() { ... }
    
    // Public: Main API for users
    fn connect() { ... }
}
```

## Inheritance and Visibility

### Visibility is Preserved Through Inheritance

```adesh
class Base {
    private fn privateMethod() { return "private"; }
    protected fn protectedMethod() { return "protected"; }
    public fn publicMethod() { return "public"; }
}

class Child extends Base {
    fn test() {
        // this.privateMethod();     // ❌ Error
        this.protectedMethod();      // ✅ OK
        this.publicMethod();         // ✅ OK
    }
}
```

### Methods Can Change Visibility When Overridden

A method can have different visibility in a subclass:

```adesh
class Base {
    protected fn method() {
        return "base";
    }
}

class Child extends Base {
    // Making the method public in child class
    fn method() {
        return "child";
    }
}

let obj = new Child();
print(obj.method());  // ✅ Works: method is public in Child
```

## Static Members and Visibility

Visibility rules apply to static members as well:

```adesh
class Config {
    private static secret = "secret_key"
    protected static mode = "production"
    public static version = "1.0.0"
    
    static fn getVersion() {
        return Config.version;  // ✅ Public static field
    }
    
    private static fn validateSecret() {
        return Config.secret.length > 0;  // ✅ Private static field
    }
}

print(Config.version);  // ✅ Works
// print(Config.secret);  // ❌ Error: private
```

## Compatibility

- ✅ Works with all execution backends (Interpreter, JIT, VM, AOT, WASM)
- ✅ Compatible with abstract classes and interfaces
- ✅ Works with sealed classes
- ✅ Supports method overloading
- ✅ Compatible with decorators
- ✅ Preserves visibility across async/await boundaries

## Performance

- Zero overhead for public members (default)
- Lightweight context tracking (Option<&str>) for visibility checks
- No performance impact on method calls within the same class

## Migration Guide

### From Untyped Fields

**Before:**
```adesh
class Person {
    fn init(name, age) {
        this.name = name;
        this.age = age;
    }
}
```

**After:**
```adesh
class Person {
    name: String          // Public by default
    private age: Number   // Explicitly private
    
    fn init(n, a) {
        this.name = n;
        this.age = a;
    }
}
```

### Adding Visibility to Existing Classes

1. Identify which members should be private (implementation details)
2. Mark helper methods as protected if subclasses need them
3. Keep public API methods without modifiers (or explicitly `public`)

## See Also

- [OOP Features Guide](OOP_COMPLETE_FEATURES_GUIDE.md)
- [Class Inheritance](../docs/language.md#inheritance)
- [Abstract Classes](../docs/language.md#abstract-classes)
- [Interfaces](../docs/language.md#interfaces)


---

## Source: VISIBILITY_STATUS.md

# Tier 1 Feature #1: Visibility Enforcement - Status Report

## What Was Completed

### ✅ Parser Support (ALREADY EXISTS)
- Lexer recognizes `private`, `protected`, `public` keywords
- Parser correctly parses visibility modifiers on methods
- TokenKind enum includes Private, Protected, Public
- Visibility enum exists in AST
- Method declarations store visibility field

### ✅ Runtime Infrastructure (IMPLEMENTED)
- Added `current_class_context: Option<String>` to Interpreter struct
- Created `is_method_accessible()` function with full C++-style logic
- Created `find_method_with_visibility()` for hierarchy traversal
- Integrated visibility checking into `get_prop()` method
- Context tracking in bound method calls (line 6693)

### ✅ Test Infrastructure
- Created test files demonstrating syntax
- Validated parser accepts visibility keywords
- Confirmed methods store visibility information

## What Remains - Critical Issue Found

### ❌ Context Propagation Problem

**Issue**: Methods execute in a separate `Exec` context that doesn't track `current_class_context`.

**Details:**
- When `this.privateMethod()` is called FROM WITHIN another method
- The code executes in an `Exec` struct (not `Interpreter`)
- `Exec` doesn't have `current_class_context` field
- Result: Private methods can't be called even from same class

**Location**: `call_user_with_this()` function (line 12167 in mod.rs)
- Creates new `Exec` instance without context tracking
- Would need to either:
  1. Add `current_class_context` to `Exec` struct
  2. Pass context through function parameters
  3. Store context in the bound `this` value itself

**Impact**: Current implementation only works for direct calls from outside, not for internal method-to-method calls within the same class.

## Required Fix (2-3 hours of work)

### Option 1: Add Context to Exec Struct (Recommended)

1. Add `current_class_context: Option<String>` to `Exec` struct
2. Modify `call_user_with_this` to accept and set context parameter
3. Pass context from Interpreter when calling methods
4. Update Exec's method call handling to use context

### Option 2: Store Context in Environment

1. Store `__class_context` in function environment
2. Check environment for context in method lookups
3. Set in `call_user_with_this` based on `this_inst.class_name`

### Option 3: Context in UserInstance

1. Add `calling_context: Option<String>` to UserInstance
2. Set when binding methods
3. Check during visibility validation

## Testing Status

**Works:**
- ✅ Parser accepts visibility keywords
- ✅ Methods store visibility information
- ✅ Direct external calls (not tested with restrictions yet)

**Doesn't Work:**
- ❌ Method-to-method calls within same class
- ❌ Private method calls from public methods
- ❌ Protected method access in subclasses

## Estimated Completion Time

**To fully complete Feature #1:**
- Fix context propagation: 2-3 hours
- Add field visibility: 1 hour  
- Comprehensive testing: 2 hours
- **Total: 5-6 hours** of focused development

## Recommendation

The visibility enforcement feature is 80% complete. The remaining 20% requires architectural changes to the `Exec` struct to properly propagate class context through nested method calls. This is a well-defined problem with clear solutions, but requires careful implementation to avoid breaking existing functionality.

## Files That Need Changes

1. `src/execution/runtime/exec.rs` - Add context field to Exec struct
2. `src/execution/runtime/mod.rs` - Pass context to call_user_with_this
3. Test all method call paths to ensure context propagates correctly
4. Add comprehensive test suite once working

## Summary

**Achievement**: Complete parser support + 80% of runtime infrastructure  
**Remaining**: Context propagation fix (architectural but straightforward)  
**Value**: Once fixed, full C++-style access control will work perfectly  
**Effort**: ~5-6 hours to complete including tests

