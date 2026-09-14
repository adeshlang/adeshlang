//! PriorityQueue Implementation with explicit priorities

use crate::stdlib::adesh_alloc::binary_heap::BinaryHeap;

struct PriorityItem<T, P> {
    item: T,
    priority: P,
}

impl<T, P: Ord> Eq for PriorityItem<T, P> {}

impl<T, P: Ord> PartialEq for PriorityItem<T, P> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl<T, P: Ord> Ord for PriorityItem<T, P> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl<T, P: Ord> PartialOrd for PriorityItem<T, P> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Priority queue supporting elements with explicit priorities
pub struct PriorityQueue<T, P> {
    heap: BinaryHeap<PriorityItem<T, P>>,
}

impl<T, P: Ord> PriorityQueue<T, P> {
    /// Creates an empty PriorityQueue
    pub fn new() -> Self {
        PriorityQueue {
            heap: BinaryHeap::new(),
        }
    }

    /// Pushes an item with a priority
    pub fn push(&mut self, item: T, priority: P) {
        self.heap.push(PriorityItem { item, priority });
    }

    /// Pops the item with the highest priority
    pub fn pop(&mut self) -> Option<T> {
        self.heap.pop().map(|pi| pi.item)
    }

    /// Peeks at the item with the highest priority
    pub fn peek(&self) -> Option<&T> {
        self.heap.peek().map(|pi| &pi.item)
    }

    /// Returns the length
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Returns true if empty
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Clears the queue
    pub fn clear(&mut self) {
        self.heap.clear();
    }
}

impl<T, P: Ord> Default for PriorityQueue<T, P> {
    fn default() -> Self {
        Self::new()
    }
}
