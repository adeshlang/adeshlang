//! Integration tests for AdeshLang Collections library (`adesh_alloc` & `collections`)

use adeshlang::stdlib::adesh_alloc::{
    BTreeMap, BTreeSet, BinaryHeap, BitSet, HashMap, HashSet, OrderedMap, OrderedSet,
    PriorityQueue, Queue, RingBuffer, Slice, Stack, Vec, VecDeque,
};

#[test]
fn test_vec_operations() {
    let mut v: Vec<i32> = Vec::new();
    assert_eq!(v.len(), 0);
    assert!(v.is_empty());

    v.push(10);
    v.push(20);
    v.push(30);
    assert_eq!(v.len(), 3);
    assert_eq!(v.get(0), Some(&10));
    assert_eq!(v.get(2), Some(&30));

    assert_eq!(v.pop(), Some(30));
    assert_eq!(v.len(), 2);
}

#[test]
fn test_vecdeque_operations() {
    let mut vd: VecDeque<i32> = VecDeque::new();
    vd.push_back(10);
    vd.push_front(5);
    assert_eq!(vd.front(), Some(&5));
    assert_eq!(vd.back(), Some(&10));
    assert_eq!(vd.pop_front(), Some(5));
    assert_eq!(vd.pop_back(), Some(10));
    assert!(vd.is_empty());
}

#[test]
fn test_hashmap_hashset() {
    let mut map: HashMap<String, i32> = HashMap::new();
    map.insert("a".to_string(), 1);
    map.insert("b".to_string(), 2);
    assert_eq!(map.get(&"a".to_string()), Some(&1));
    assert!(map.contains_key(&"b".to_string()));
    assert_eq!(map.remove(&"a".to_string()), Some(1));
    assert!(!map.contains_key(&"a".to_string()));

    let mut set: HashSet<String> = HashSet::new();
    assert!(set.insert("x".to_string()));
    assert!(!set.insert("x".to_string()));
    assert!(set.contains(&"x".to_string()));
    assert!(set.remove(&"x".to_string()));
}

#[test]
fn test_btree_map_set() {
    let mut bmap: BTreeMap<String, i32> = BTreeMap::new();
    bmap.insert("k1".to_string(), 100);
    assert_eq!(bmap.get(&"k1".to_string()), Some(&100));

    let mut bset: BTreeSet<String> = BTreeSet::new();
    assert!(bset.insert("item".to_string()));
    assert!(bset.contains(&"item".to_string()));
}

#[test]
fn test_heap_priority_queue() {
    let mut heap: BinaryHeap<i32> = BinaryHeap::new();
    heap.push(10);
    heap.push(50);
    heap.push(20);
    assert_eq!(heap.peek(), Some(&50));
    assert_eq!(heap.pop(), Some(50));
    assert_eq!(heap.pop(), Some(20));

    let mut pq: PriorityQueue<String, i32> = PriorityQueue::new();
    pq.push("low".to_string(), 1);
    pq.push("high".to_string(), 100);
    assert_eq!(pq.pop(), Some("high".to_string()));
}

#[test]
fn test_bitset_ringbuffer() {
    let mut bs = BitSet::new();
    bs.set(0);
    bs.set(64);
    assert!(bs.test(0));
    assert!(bs.test(64));
    assert!(!bs.test(10));
    bs.clear(0);
    assert!(!bs.test(0));

    let mut rb = RingBuffer::new(2);
    assert_eq!(rb.push(1), None);
    assert_eq!(rb.push(2), None);
    assert_eq!(rb.push(3), Some(1)); // evicts 1
    assert_eq!(rb.pop(), Some(2));
}

#[test]
fn test_queue_stack() {
    let mut q = Queue::new();
    q.enqueue(1);
    q.enqueue(2);
    assert_eq!(q.dequeue(), Some(1));

    let mut st = Stack::new();
    st.push(10);
    st.push(20);
    assert_eq!(st.pop(), Some(20));
}

#[test]
fn test_ordered_map_set() {
    let mut om = OrderedMap::new();
    om.insert("z".to_string(), 1);
    om.insert("a".to_string(), 2);
    assert_eq!(om.keys(), &["z".to_string(), "a".to_string()]);

    let mut os = OrderedSet::new();
    os.insert("first".to_string());
    os.insert("second".to_string());
    assert_eq!(os.elements(), &["first".to_string(), "second".to_string()]);
}

#[test]
fn test_slice() {
    let arr = [10, 20, 30, 40];
    let sl = Slice::from_slice(&arr);
    assert_eq!(sl.len(), 4);
    assert_eq!(sl.first(), Some(&10));
    assert_eq!(sl.last(), Some(&40));
    let (left, right) = sl.split_at(2);
    assert_eq!(left.len(), 2);
    assert_eq!(right.len(), 2);
}

#[test]
fn test_property_invariants_and_stress() {
    // 1. Vec Property: Push 10,000 items and verify order
    let mut v: Vec<usize> = Vec::new();
    for i in 0..10_000 {
        v.push(i);
    }
    assert_eq!(v.len(), 10_000);
    for i in (0..10_000).rev() {
        assert_eq!(v.pop(), Some(i));
    }
    assert!(v.is_empty());

    // 2. Queue FIFO Invariant
    let mut q: Queue<usize> = Queue::new();
    for i in 0..5_000 {
        q.enqueue(i);
    }
    for i in 0..5_000 {
        assert_eq!(q.dequeue(), Some(i));
    }

    // 3. Stack LIFO Invariant
    let mut s: Stack<usize> = Stack::new();
    for i in 0..5_000 {
        s.push(i);
    }
    for i in (0..5_000).rev() {
        assert_eq!(s.pop(), Some(i));
    }

    // 4. BinaryHeap Max Order Invariant
    let mut heap: BinaryHeap<i32> = BinaryHeap::new();
    let values = [45, 12, 89, 3, 67, 100, 23];
    for &val in &values {
        heap.push(val);
    }
    let mut prev = i32::MAX;
    while let Some(val) = heap.pop() {
        assert!(val <= prev);
        prev = val;
    }

    // 5. RingBuffer Eviction & Capacity Limit
    let mut rb = RingBuffer::new(5);
    for i in 0..10 {
        rb.push(i);
    }
    assert_eq!(rb.len(), 5);
    assert_eq!(rb.pop(), Some(5));
}
