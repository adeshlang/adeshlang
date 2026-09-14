//! BinaryHeap / Min-Max Heap Implementation

use crate::stdlib::adesh_alloc::vec::Vec;

/// A priority queue implemented with a binary heap
pub struct BinaryHeap<T> {
    data: Vec<T>,
}

impl<T> BinaryHeap<T>
where
    T: Ord,
{
    /// Creates an empty BinaryHeap
    pub fn new() -> Self {
        BinaryHeap { data: Vec::new() }
    }

    /// Pushes an item onto the heap
    pub fn push(&mut self, item: T) {
        self.data.push(item);
        self.sift_up(self.data.len() - 1);
    }

    /// Pops the maximum/highest priority item from the heap
    pub fn pop(&mut self) -> Option<T> {
        if self.data.is_empty() {
            return None;
        }
        let last = self.data.len() - 1;
        unsafe {
            let p = self.data.as_mut_slice().as_mut_ptr();
            std::ptr::swap(p, p.add(last));
        }
        let result = self.data.pop();
        if !self.data.is_empty() {
            self.sift_down(0);
        }
        result
    }

    /// Peeks at the maximum/highest priority item
    pub fn peek(&self) -> Option<&T> {
        self.data.get(0)
    }

    /// Returns the length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clears the heap
    pub fn clear(&mut self) {
        self.data.clear();
    }

    fn sift_up(&mut self, mut idx: usize) {
        let slice = self.data.as_mut_slice();
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if slice[idx] > slice[parent] {
                slice.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, mut idx: usize) {
        let len = self.data.len();
        let slice = self.data.as_mut_slice();
        loop {
            let left = idx * 2 + 1;
            let right = idx * 2 + 2;
            let mut largest = idx;

            if left < len && slice[left] > slice[largest] {
                largest = left;
            }
            if right < len && slice[right] > slice[largest] {
                largest = right;
            }

            if largest != idx {
                slice.swap(idx, largest);
                idx = largest;
            } else {
                break;
            }
        }
    }
}

impl<T> Default for BinaryHeap<T>
where
    T: Ord,
{
    fn default() -> Self {
        Self::new()
    }
}
