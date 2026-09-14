# AdeshLang Collections Standard Library — Detailed API Reference & Cheat-Sheet

The `Collections` standard library in AdeshLang provides 15 high-performance, cache-efficient, zero-GC data structures built around AdeshLang's ownership and RAII memory model.

---

## Table of Contents
1. [Module Import & Namespace Initialization](#1-module-import--namespace-initialization)
2. [Vec](#2-vec)
3. [Slice](#3-slice)
4. [HashMap](#4-hashmap)
5. [HashSet](#5-hashset)
6. [VecDeque](#6-vecdeque)
7. [BTreeMap](#7-btreemap)
8. [BTreeSet](#8-btreeset)
9. [BinaryHeap](#9-binaryheap)
10. [PriorityQueue](#10-priorityqueue)
11. [BitSet](#11-bitset)
12. [RingBuffer](#12-ringbuffer)
13. [Queue](#13-queue)
14. [Stack](#14-stack)
15. [OrderedMap](#15-orderedmap)
16. [OrderedSet](#16-orderedset)
17. [Performance & Time Complexity Matrix](#17-performance--time-complexity-matrix)
18. [Executable Demonstration References](#18-executable-demonstration-references)

---

## 1. Module Import & Namespace Initialization

Importing the Collections module registers the `Collections` namespace containing constructors for all 15 collection data structures:

```adesh
import Collections;
```

---

## 2. Collection Overview & APIs

- **Vec**: Dynamic array (`push`, `pop`, `get`, `set`, `insert`, `remove`, `swapRemove`, `first`, `last`, `contains`, `indexOf`, `reverse`, `sort`, `toArray`, `iter`).
- **Slice**: Non-owning contiguous view (`len`, `isEmpty`, `first`, `last`, `get`, `contains`, `toArray`, `iter`).
- **HashMap**: Key-value map (`insert`, `get`, `remove`, `containsKey`, `keys`, `values`, `entries`, `iter`).
- **HashSet**: Unique item set (`insert`, `contains`, `remove`, `values`, `toArray`, `iter`).
- **VecDeque**: Double-ended queue (`pushBack`, `pushFront`, `popBack`, `popFront`, `front`, `back`).
- **BTreeMap & BTreeSet**: Sorted map and set structures ($O(\log N)$).
- **BinaryHeap & PriorityQueue**: Max-heap and priority queue ($O(\log N)$).
- **BitSet & RingBuffer**: Compact bitfield and fixed-capacity circular buffer.
- **Queue & Stack**: FIFO Queue and LIFO Stack wrappers.
- **OrderedMap & OrderedSet**: Insertion-ordered map and set wrappers.

---

## 17. Performance & Time Complexity Matrix

| Collection | Operation | Time Complexity | Space Complexity |
|---|---|---|---|
| **Vec** | push / pop / get | $O(1)$ amortized | $O(N)$ |
| **Slice** | get / sub-slice | $O(1)$ | $O(1)$ view |
| **VecDeque** | pushBack / pushFront / pop | $O(1)$ amortized | $O(N)$ |
| **HashMap** | insert / get / remove | $O(1)$ expected | $O(N)$ |
| **HashSet** | insert / contains / remove | $O(1)$ expected | $O(N)$ |
| **BTreeMap** | insert / get / remove | $O(\log N)$ | $O(N)$ |
| **BinaryHeap** | push / pop | $O(\log N)$ | $O(N)$ |
| **RingBuffer** | push / pop | $O(1)$ | $O(\text{capacity})$ |
| **Queue** | enqueue / dequeue | $O(1)$ | $O(N)$ |
| **Stack** | push / pop | $O(1)$ | $O(N)$ |

---

## 18. Executable Demonstration References (35 Scripts)

Executable examples demonstrating all 15 Collections are located in `examples/libraries/collections/`:
- [`vec_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/vec_examples.adesh)
- [`slice_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/slice_examples.adesh)
- [`hashmap_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/hashmap_examples.adesh)
- [`iterator_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/iterator_examples.adesh)
- [`queue_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/queue_examples.adesh)
- [`stack_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/stack_examples.adesh)
- [`ordered_collections.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/ordered_collections.adesh)
- [`set_operations.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/set_operations.adesh)
- [`lru_cache_example.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/lru_cache_example.adesh)
- [`graph_adjacency_map.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/graph_adjacency_map.adesh)
- [`frequency_counter.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/frequency_counter.adesh)
- [`word_frequency.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/word_frequency.adesh)
- [`group_by.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/group_by.adesh)
- [`deduplicate.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/deduplicate.adesh)
- [`top_k.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/top_k.adesh)
- [`task_scheduler.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/task_scheduler.adesh)
- [`priority_tasks.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/priority_tasks.adesh)
- [`binary_search.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/binary_search.adesh)
- [`sorting.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/sorting.adesh)
- [`graph_traversal.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/graph_traversal.adesh)
- [`bfs.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/bfs.adesh)
- [`dfs.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/dfs.adesh)
- [`adjacency_list.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/adjacency_list.adesh)
- [`cache_example.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/cache_example.adesh)
- [`nested_collections.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/nested_collections.adesh)
- [`ownership_collections.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/ownership_collections.adesh)
- [`borrowing_collections.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/borrowing_collections.adesh)
- [`vecdeque_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/vecdeque_examples.adesh)
- [`hashset_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/hashset_examples.adesh)
- [`btreemap_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/btreemap_examples.adesh)
- [`heap_priorityqueue_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/heap_priorityqueue_examples.adesh)
- [`bitset_ringbuffer_examples.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/bitset_ringbuffer_examples.adesh)
- [`sliding_window_max.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/sliding_window_max.adesh)
- [`kth_largest_element.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/kth_largest_element.adesh)
- [`two_sum.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/collections/two_sum.adesh)
