# ARC — Automatic Reference Counting

**ARC** is AdeshLang's shared-ownership memory model. It lets you give a single
heap value *multiple owners* — the value lives as long as anyone holds a strong
reference, and is freed *the instant* the last strong reference drops. There is
no garbage collector; cleanup is deterministic and scope-driven.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Keywords](#2-keywords)
3. [Built-in Methods](#3-built-in-methods)
4. [Lifecycle](#4-lifecycle)
5. [Patterns & Usage](#5-patterns--usage)
6. [Cycle Breaking](#6-cycle-breaking)
7. [Edge Cases](#7-edge-cases)
8. [Implementation Notes](#8-implementation-notes)
9. [Comparison with Other Languages](#9-comparison-with-other-languages)
10. [Examples](#10-examples)
11. [Tests](#11-tests)

---

## 1. Overview

ARC solves a simple problem: **who owns this data, and when should it be freed?**

| Approach | Strategy | Free happens… |
|----------|----------|---------------|
| Single owner (Rust) | Strict ownership tree | At scope exit of the sole owner |
| Garbage Collector (Java/Go) | Tracing GC | Unpredictably (pause, cycle) |
| **ARC (AdeshLang)** | **Shared refcounting** | **When the last strong ref drops** |

ARC is *opt-in* — you only pay for reference counting when you write `share`.
Everything else uses AdeshLang's default ownership rules.

### When to use ARC

| Scenario | Use ARC? | Why |
|----------|----------|-----|
| Single owner, tree-like structure | ❌ No | Ownership is simpler |
| Multiple owners of the same data | ✅ Yes | Share with `share` |
| Observer / event patterns | ✅ Yes | Weak refs prevent leaks |
| Cyclic data (graphs, doubly-linked lists) | ✅ Yes | Weak breaks the cycle |
| Hot-path performance | ⚠️ Sparingly | Refcounting has overhead |

---

## 2. Keywords

AdeshLang provides exactly three ARC keywords:

| Keyword | Effect | Strong count | Weak count | Prevents free? |
|---------|--------|:---:|:---:|:---:|
| `share value` | Wraps a value in a ref-counted container | `+1` | `0` | ✅ Yes |
| `strong ref = shared` | Adds an additional strong reference | `+1` | — | ✅ Yes |
| `weak ref = shared` | Adds a non-owning weak reference | — | `+1` | ❌ No |

### `share` — the starting point

`share` is how you *enter* the ARC world. It takes any value and returns a
reference-counted handle.

```adesh
share data = { name: "Alice", age: 30 };
// strong_count = 1, weak_count = 0
```

After `share`, the value lives on the heap inside a container that tracks how
many strong and weak references exist.

### `strong` — adding owners

Each `strong` assignment increments the strong reference count. The allocation
stays alive as long as `strong_count > 0`.

```adesh
strong ref1 = data;   // strong_count = 2
strong ref2 = data;   // strong_count = 3
```

### `weak` — observing without owning

A `weak` reference does **not** increment strong_count, so it never prevents
deallocation. Use it for observers, caches, and back-pointers in cycles.

```adesh
weak observer = data;  // strong_count unchanged, weak_count = 1
```

---

## 3. Built-in Methods

Every ARC handle exposes these methods:

| Method | Returns | Callable on | Description |
|--------|---------|-------------|-------------|
| `.strong_count()` | `i64` | shared, strong, weak | Current number of strong references |
| `.weak_count()` | `i64` | shared, strong, weak | Current number of weak references |
| `.is_alive()` | `bool` | shared, strong, weak | Whether strong_count > 0 |
| `.upgrade()` | value \| `null` | **weak only** | Promotes weak → strong; `null` if freed |

### `.strong_count()` and `.weak_count()`

These are purely informational — useful for debugging, testing, and learning.

```adesh
share data = { x: 1 };
strong a = data;
strong b = data;
weak   w = data;

print(data.strong_count());  // 3  (share + a + b)
print(data.weak_count());    // 1  (w)
```

### `.is_alive()`

Returns `true` as long as at least one strong reference exists. Safe to call
on any handle, including weak ones.

```adesh
weak w = data;
if (w.is_alive()) {
    // safe to upgrade
}
```

### `.upgrade()`

Only meaningful on weak references. Tries to promote the weak ref to a strong
one. Returns the value if successful, or `null` if the object has been freed.

```adesh
let maybe = w.upgrade();
if (maybe != null) {
    print(maybe.name);  // use the value
}
// The temporary strong ref from upgrade() drops here
```

---

## 4. Lifecycle

### Creation

```
share data = { value: 42 };
    │
    ▼
┌──────────────────────────┐
│  ReferenceCounted<T>     │
│  strong_count: 1         │
│  weak_count:   0         │
│  ┌─────────────────────┐ │
│  │ { value: 42 }       │ │
│  └─────────────────────┘ │
└──────────────────────────┘
```

### Adding references

```
strong a = data;   // strong_count: 2
weak   w = data;   // strong_count: 2, weak_count: 1
```

### Scope exit

When a strong reference variable goes out of scope, `strong_count` decreases
automatically. When it reaches 0, the value is freed immediately.

```adesh
share data = { value: 42 };
{
    strong tmp = data;    // count: 2
    // use tmp...
} // tmp dropped → count: 1 → still alive

// data is still usable here (count: 1)
```

### Deallocation

The allocation is freed when `strong_count` reaches 0. Weak references do NOT
prevent this — after deallocation, any existing weak refs return `.is_alive()
= false` and `.upgrade() = null`.

```
strong_count reaches 0
    │
    ▼
┌──────────────────────────┐
│  run destructor (if any) │
├──────────────────────────┤
│  free heap memory        │
└──────────────────────────┘
    │
    ▼
All weak refs now return null on upgrade()
```

---

## 5. Patterns & Usage

### 5.1 Basic shared ownership

```adesh
share config = { host: "localhost", port: 8080 };
strong c1 = config;
strong c2 = config;

print(c1.host);   // "localhost"
print(c2.port);   // 8080
```

### 5.2 Passing through function calls

When you pass a shared value to a function, the callee receives a cloned strong reference.
`strong_count()` inside the function includes that parameter owner (+1 vs the caller-only count).

```adesh
share data = { v: 0 };
print(data.strong_count());  // 1

fn process(val) {
  print("inside, count:", val.strong_count());  // 2 (data + val)
}

process(data);
print(data.strong_count());  // 1
```

### 5.3 Reassignment

Reassigning a strong variable decrements the old target's count and increments
the new target's count. After `handle = b`, use `handle.strong_count()` to inspect
the new target if the source `share` binding is considered moved by the checker.

### 5.4 Observer pattern (weak)

```adesh
share subject = { topic: "news" };
weak obs1 = subject;
weak obs2 = subject;

fn notify(obs) {
    {
        let live = obs.upgrade();
        if (live != null) {
            print("got topic:", live.topic);
        }
    }
}

notify(obs1);
notify(obs2);
```

### 5.5 ARC with classes

```adesh
class Counter {
    value: i64;
    fn init(v: i64) { self.value = v; }
    fn inc() { self.value = self.value + 1; }
}

share c = Counter(0);
strong a = c;
strong b = c;

a.inc();
b.inc();
print(c.value);     // 2
```

---

## 6. Cycle Breaking

The most important use of `weak` is breaking reference cycles. Without weak
refs, two objects holding strong references to each other would leak memory.

### Problem: memory leak

```
Parent ──strong──→ Child
  ↑                  │
  └─────strong───────┘

Both counts never reach 0 → memory leak
```

### Solution: weak back-pointer

```
Parent ──strong──→ Child
  ↑                  │
  └─────weak─────────┘

When external strong refs drop, Child's count reaches 0 and gets freed.
Parent's count also reaches 0 because Child only held a weak ref.
```

### Tree / hierarchy pattern

```adesh
share parent = { name: "parent" };
share child  = { name: "child" };

strong parent_owns_child = child;   // strong: parent owns child
weak   child_sees_parent = parent;  // weak: child observes parent

// safe navigation: upgrade before use
let p = child_sees_parent.upgrade();
if (p != null) {
    print(p.name);
}
```

### Doubly-linked list pattern

```adesh
share nodeA = { value: 100 };
share nodeB = { value: 200 };

strong a_next = nodeB;  // A → B (strong)
weak   b_prev = nodeA;  // B → A (weak)

let prev = b_prev.upgrade();  // safe back-navigation
```

---

## 7. Edge Cases

### 7.1 Sharing primitives / empty objects

```adesh
share empty = { };        // works fine
share num   = { n: 0 };   // works fine
```

### 7.2 Diamond sharing

```
      base
     /    \
  left    right
     \    /
     joined
```

```adesh
share base = { id: 1 };
strong left  = base;    // count: 2
strong right = base;    // count: 3
strong joined_l = left;  // count: 4
strong joined_r = right; // count: 5
// all released at end of scope
```

### 7.3 Repeated upgrade

Calling `.upgrade()` multiple times on the same weak ref creates temporary
strong references that each bump and drop the count.

```adesh
weak w = data;
let u1 = w.upgrade();   // count bumps
let u2 = w.upgrade();   // count bumps again
let u3 = w.upgrade();   // count bumps again
// count = original + 3 while u1..u3 are in scope
```

### 7.4 Upgrade after deallocation

Once all strong references are gone, `upgrade()` returns `null`.

```adesh
share ephemeral = { id: 1 };
weak watcher = ephemeral;
// When ephemeral goes out of scope:
// watcher.is_alive() → false
// watcher.upgrade()  → null
```

### 7.5 Deeply nested scopes

Each scope boundary correctly increments/decrements counts.

```adesh
share d = { level: 0 };
{
    strong l1 = d;   // count: 2
    {
        strong l2 = d;  // count: 3
        {
            strong l3 = d;  // count: 4
        } // count: 3
    } // count: 2
} // count: 1
```

---

## 8. Implementation Notes

### Where ARC lives

ARC is implemented in the AdeshLang **interpreter** runtime, not in the compiler
backend. The keywords `share`, `strong`, and `weak` are parsed by the compiler
but the actual reference counting happens at runtime.

### Internal structure

```
Heap layout for a shared value:
┌──────────────────────────────────┐
│  strong_count: AtomicI64         │
│  weak_count:   AtomicI64         │
│  value:        T                 │
└──────────────────────────────────┘
```

- Strong count and weak count are stored adjacent to the value on the heap.
- The allocation is exactly `sizeof(Counts) + sizeof(T)`.
- When `strong_count` transitions from 1 to 0, the value is dropped and the
  memory is freed.

### Thread safety

- In **single-thread** mode (default), counts use plain integers (no atomics).
- In **multi-thread** mode (`--multi-thread`), counts use atomic operations.
- The mode is selected via CLI flag: `adesh run main.adesh --multi-thread`.

### Limitations

| Context | ARC support |
|---------|:---:|
| Interpreter | ✅ Full |
| JIT | ✅ Full |
| AOT | ✅ Full |
| WASM | ✅ Full |
| Inside `region` blocks | ❌ Forbidden |
| Embedded builds | ❌ Forbidden |

ARC is forbidden inside regions (regions handle their own memory) and in
embedded builds (no heap allocator).

### Performance characteristics

| Operation | Cost | Note |
|-----------|:----:|------|
| `share` | Heap alloc + init | One-time cost |
| `strong` | Increment | Atomic in multi-thread mode |
| `weak` | Increment | Atomic in multi-thread mode |
| `.strong_count()` | Read | Plain load |
| `.weak_count()` | Read | Plain load |
| `.is_alive()` | Read + compare | `strong_count > 0` |
| `.upgrade()` | Compare-and-swap | Atomic in multi-thread mode |
| Scope exit (strong) | Decrement + maybe free | Free only when count reaches 0 |
| Scope exit (weak) | Decrement | Never triggers free |

---

## 9. Comparison with Other Languages

| | AdeshLang ARC | Rust `Rc<T>` / `Arc<T>` | Swift ARC | Java GC |
|---|---|---|---|---|
| Opt-in? | ✅ `share` keyword | ✅ `Rc::new()` | ❌ Implicit for classes | ❌ Always on |
| Weak refs | `weak` keyword | `Weak<T>` | `weak` keyword | Soft/WeakReference |
| Free guarantee | Deterministic at scope exit | Deterministic at scope exit | Deterministic at scope exit | Nondeterministic |
| Cycle detection | Manual (use `weak`) | Manual (use `Weak<T>`) | Manual (use `weak`) | Automatic (tracing) |
| Thread-safe option | `--multi-thread` flag | `Arc<T>` vs `Rc<T>` | Automatic (not for classes) | N/A |
| Count overhead | 2 × 8 bytes per allocation | 2 × usize per allocation | 2 × words per object | Tracing overhead |
| Syntax | `share x = value` | `Rc::new(value)` | `class Foo { }` | `new Object()` |

---

## 10. Examples

Each example is a standalone `.adesh` file you can run with `adesh run`.

| File | What it demonstrates |
|------|---------------------|
| [`01_basic_share.adesh`](01_basic_share.adesh) | Creating a shared value, initial counts, adding strong refs |
| [`02_strong_references.adesh`](02_strong_references.adesh) | Multiple owners, scope lifetimes, reassignment |
| [`03_weak_references.adesh`](03_weak_references.adesh) | Non-owning weak refs, `is_alive`, `upgrade` |
| [`04_scope_cleanup.adesh`](04_scope_cleanup.adesh) | Deterministic deallocation as scopes exit |
| [`05_upgrade_and_failure.adesh`](05_upgrade_and_failure.adesh) | Successful and null-returning `upgrade()` |
| [`06_reference_counts.adesh`](06_reference_counts.adesh) | Introspecting `strong_count` / `weak_count` at runtime |
| [`07_arc_in_functions.adesh`](07_arc_in_functions.adesh) | Passing shared/weak refs through function calls |
| [`08_breaking_cycles.adesh`](08_breaking_cycles.adesh) | Using weak refs to prevent reference cycles and memory leaks |
| [`09_arc_with_classes.adesh`](09_arc_with_classes.adesh) | Shared ownership of class instances, event-bus observer pattern |
| [`10_edge_cases.adesh`](10_edge_cases.adesh) | Empty objects, diamond sharing, repeated upgrade, deep nesting |

**Suggested order:** Start with 01–03 to learn the basics, then 04–07 for
practical patterns, then 08–09 for real-world usage, and 10 for edge cases.

---

## 11. Tests

Focused, isolated test files live in the [`tests/`](tests/) directory. Each file
prints `✓ PASS` / `✗ FAIL` lines and a summary.

### Test index

| File | What it tests |
|------|---------------|
| [`t01_basic_share.adesh`](tests/t01_basic_share.adesh) | Initial strong/weak counts after `share` |
| [`t02_strong_refs.adesh`](tests/t02_strong_refs.adesh) | Count increments per strong ref |
| [`t03_weak_refs.adesh`](tests/t03_weak_refs.adesh) | Weak refs leave strong_count unchanged |
| [`t04_scope_cleanup.adesh`](tests/t04_scope_cleanup.adesh) | Counts track nested scope entry/exit |
| [`t05_is_alive.adesh`](tests/t05_is_alive.adesh) | `is_alive()` returns correct value |
| [`t06_upgrade.adesh`](tests/t06_upgrade.adesh) | `upgrade()` value, count bump, null-safety |
| [`t07_reassignment.adesh`](tests/t07_reassignment.adesh) | Old target loses count, new target gains it |
| [`t08_function_calls.adesh`](tests/t08_function_calls.adesh) | ARC through params and return values |
| [`t09_cycle_breaking.adesh`](tests/t09_cycle_breaking.adesh) | Weak back-pointers break reference cycles |
| [`t10_edge_cases.adesh`](tests/t10_edge_cases.adesh) | Empty objects, diamond, repeated upgrade, deep nesting |
| [`t11_arc_with_classes.adesh`](tests/t11_arc_with_classes.adesh) | Shared class instances, shared mutation |
| [`run_all.adesh`](tests/run_all.adesh) | **All T01–T11 in one file** (master suite) |

### Running tests

```bash
# Full suite
adesh run examples/arc/tests/run_all.adesh

# Single test
adesh run examples/arc/tests/t01_basic_share.adesh
```

---

## Quick Reference

```adesh
// ── 1. Create ──
share data = { name: "Alice", score: 100 };
// strong_count = 1, weak_count = 0

// ── 2. Add strong references ──
strong ref_a = data;   // strong_count = 2
strong ref_b = data;   // strong_count = 3

// ── 3. Add weak references ──
weak observer = data;  // strong_count = 3, weak_count = 1

// ── 4. Inspect counts ──
print(data.strong_count());  // 3
print(data.weak_count());    // 1

// ── 5. Check liveness and upgrade ──
if (observer.is_alive()) {
    let live = observer.upgrade();
    if (live != null) {
        print(live.name);     // "Alice"
    }
}

// ── 6. Break cycles ──
share parent = { id: 1 };
share child  = { id: 2 };
strong parent_owns_child = child;   // strong: won't leak
weak   child_sees_parent = parent;  // weak: breaks cycle

// ── 7. Scope cleanup ──
{
    strong tmp = data;
    // strong_count = 4 here
} // tmp dropped → strong_count = 3
```

> ARC is an interpreter-level feature. The `share`, `strong`, and `weak`
> keywords and all built-in ARC methods are processed by the AdeshLang
> interpreter at runtime.
