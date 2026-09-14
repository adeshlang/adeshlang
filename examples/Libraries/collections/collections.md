# AdeshLang Collections Standard Library Documentation

## Overview

The `Collections` standard library in AdeshLang provides high-performance, memory-safe, non-GC collection structures and data structures tailored to AdeshLang's ownership and RAII model.

## Importing Collections

```adesh
import Collections;
```

or with explicit namespace imports:

```adesh
import "std:Collections" as Collections;
```

---

## Selection Guide

| Requirement | Collection | Description |
|---|---|---|
| Cache-efficient circular sequence | `VecDeque` | Double-ended queue with growable ring buffer |
| Unique value lookup | `HashSet` | Fast O(1) expected set operations |
| Ordered key-value map | `BTreeMap` | Sorted search tree map with log(N) lookup |
| Max/Min Priority Queue | `BinaryHeap` | Max-heap implementation for priority items |
| Explicit Priority Queue | `PriorityQueue` | Queue prioritizing elements by explicit priority score |
| Compact Bit Vector | `BitSet` | Memory-dense bit array with fast bitwise operations |
| Bounded Circular Stream | `RingBuffer` | Overwriting or bounded fixed-capacity circular buffer |

---

## Collection APIs & Usage Examples

### 1. VecDeque

Growable double-ended ring buffer queue.

```adesh
let deque = Collections.VecDeque();
deque.pushBack(10);
deque.pushFront(5);
print(deque.popFront()); // 5
print(deque.popBack());  // 10
```

### 2. HashSet

High-performance hash set.

```adesh
let set = Collections.HashSet();
set.insert("adesh");
print(set.contains("adesh")); // true
print(set.len()); // 1
```

### 3. BTreeMap

Sorted key-value dictionary built on B-Tree logic.

```adesh
let map = Collections.BTreeMap();
map.insert("name", "Ajay");
print(map.get("name")); // "Ajay"
```

### 4. BinaryHeap

Binary max-heap.

```adesh
let heap = Collections.BinaryHeap();
heap.push(10);
heap.push(50);
print(heap.pop()); // 50
```

### 5. PriorityQueue

Explicit priority assignment queue.

```adesh
let pq = Collections.PriorityQueue();
pq.push("low_task", 1);
pq.push("high_task", 100);
print(pq.pop()); // "high_task"
```

### 6. BitSet

Dynamic bit array.

```adesh
let bs = Collections.BitSet();
bs.set(42);
print(bs.test(42)); // true
print(bs.test(10)); // false
```

### 7. RingBuffer

Fixed-capacity overwriting circular stream buffer.

```adesh
let ring = Collections.RingBuffer(2);
ring.push("first");
ring.push("second");
let prev = ring.push("third"); // returns evicted item "first"
```

---

## Memory & Ownership Guarantee

- **No Garbage Collector Required**: All collections manage internal allocations deterministically using AdeshLang's ownership/RAII semantics.
- **Zero-Copy & Safe Wrappers**: Thread-safe Mutex and atomic pointer management guarantees reference validity without data races across all execution backends (Interpreter, Bytecode VM, JIT, Native JIT, LLVM AOT, WASM).


# AdeshLang Collections Standard Library — Comprehensive Guide & Reference

The `Collections` standard library (`import Collections;`) provides 15 high-performance, memory-safe, zero-GC compatible data structures and iterator primitives.

## 1. Collection Selection Guide

| Requirement | Collection | API Constructor | Performance / Time Complexity |
| :--- | :--- | :--- | :--- |
| Dynamic growable sequence | `Vec` | `Collections.Vec()` | $O(1)$ amortized push/pop, $O(1)$ random access |
| Non-owning view of sequence | `Slice` | `Collections.Slice(arr, start, end)` | $O(1)$ indexing & sub-slicing |
| Double-ended queue / Ring buffer | `VecDeque` | `Collections.VecDeque()` | $O(1)$ push/pop front & back |
| Fast key-value mapping | `HashMap` | `Collections.HashMap()` | Expected $O(1)$ lookup, insert, remove |
| Unique value set | `HashSet` | `Collections.HashSet()` | Expected $O(1)$ insertion & lookup |
| Sorted key-value mapping | `BTreeMap` | `Collections.BTreeMap()` | $O(\log N)$ lookup, insert, remove |
| Sorted unique set | `BTreeSet` | `Collections.BTreeSet()` | $O(\log N)$ insertion & lookup |
| Max Priority Heap | `BinaryHeap` | `Collections.BinaryHeap()` | $O(\log N)$ push/pop, $O(1)$ peek |
| Priority-scored queue | `PriorityQueue` | `Collections.PriorityQueue()` | $O(\log N)$ push/pop, $O(1)$ peek |
| Compact bit array | `BitSet` | `Collections.BitSet()` | $O(1)$ bit test, set, clear |
| Fixed-capacity streaming buffer | `RingBuffer` | `Collections.RingBuffer(cap)` | $O(1)$ push (auto-evicts oldest), pop |
| FIFO Queue | `Queue` | `Collections.Queue()` | $O(1)$ enqueue & dequeue |
| LIFO Stack | `Stack` | `Collections.Stack()` | $O(1)$ push & pop |
| Insertion-ordered Map | `OrderedMap` | `Collections.OrderedMap()` | Expected $O(1)$ lookup with ordered keys |
| Insertion-ordered Set | `OrderedSet` | `Collections.OrderedSet()` | Expected $O(1)$ lookup with ordered items |

---

## 2. API Reference & Code Examples

### Vec
```adesh
import Collections;

let vec = Collections.Vec();
vec.push(100);
vec.push(200);
vec.push(300);

print("Length: ", vec.len());        // 3
print("First: ", vec.first());        // 100
print("Last: ", vec.last());          // 300
print("Popped: ", vec.pop());        // 300
print("Contains 100: ", vec.contains(100)); // true
```

### HashMap & HashSet
```adesh
import Collections;

let map = Collections.HashMap();
map.insert("language", "AdeshLang");
map.insert("version", "0.3.0");

print("Language: ", map.get("language"));
print("Keys: ", map.keys());

let set = Collections.HashSet();
set.insert("apple");
set.insert("banana");
print("Contains apple: ", set.contains("apple"));
```

### VecDeque
```adesh
import Collections;

let deque = Collections.VecDeque();
deque.pushBack(20);
deque.pushFront(10);
print("Front: ", deque.front()); // 10
print("Back: ", deque.back());   // 20
```

### RingBuffer
```adesh
import Collections;

let ring = Collections.RingBuffer(2);
ring.push("event_1");
ring.push("event_2");
let evicted = ring.push("event_3"); // evicts event_1
print("Evicted item: ", evicted);   // event_1
```

### Queue & Stack
```adesh
import Collections;

let q = Collections.Queue();
q.enqueue("Task 1");
q.enqueue("Task 2");
print("Dequeued: ", q.dequeue()); // Task 1

let s = Collections.Stack();
s.push("Page A");
s.push("Page B");
print("Popped: ", s.pop());       // Page B
```

---

## 3. Backend Compatibility Matrix

| Backend | Collections Standard Library Status |
| :--- | :--- |
| **Interpreter** | Fully Supported ✅ |
| **Bytecode VM** | Fully Supported ✅ |
| **JIT** | Fully Supported ✅ |
| **Native JIT** | Fully Supported ✅ |
| **LLVM AOT** | Fully Supported ✅ |
| **WASM** | Fully Supported ✅ |

---

## 4. Ownership & Safety Semantics
- Collections cleanly own their elements and destroy them when the collection is dropped.
- References returned by lookup methods (`get`, `first`, `last`, `peek`) are borrow-safe.
- Iterators and `toArray()` return array views compatible with `for item in collection.iter()`.
