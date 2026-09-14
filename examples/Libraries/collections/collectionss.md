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
