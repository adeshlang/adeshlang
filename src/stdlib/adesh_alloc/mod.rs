//! Adesh Alloc Library
//!
//! This is Layer 2 of the Adesh standard library.
//!
//! ## Principles
//! - Heap-dependent but OS-independent
//! - No GC - uses reference counting (ARC)
//! - Pluggable allocator abstraction
//! - Zero-cost abstractions
//!
//! ## Contents
//! - Allocator trait abstraction
//! - ARC (Atomic Reference Counting)
//! - Weak references
//! - Vec (growable array)
//! - String (growable text)
//! - HashMap (hash table)
//! - Box (unique heap pointer)

pub mod allocator;
pub mod arc;
pub mod binary_heap;
pub mod bitset;
pub mod boxed;
pub mod btree;
pub mod hashmap;
pub mod hashset;
pub mod ordered_map;
pub mod ordered_set;
pub mod priority_queue;
pub mod queue;
pub mod ring_buffer;
pub mod slice;
pub mod stack;
pub mod string;
pub mod vec;
pub mod vecdeque;
pub mod weak;

// Re-export commonly used items
pub use allocator::{Allocator, GlobalAllocator};
pub use arc::Arc;
pub use binary_heap::BinaryHeap;
pub use bitset::BitSet;
pub use boxed::Box;
pub use btree::{BTreeMap, BTreeSet};
pub use hashmap::HashMap;
pub use hashset::HashSet;
pub use ordered_map::OrderedMap;
pub use ordered_set::OrderedSet;
pub use priority_queue::PriorityQueue;
pub use queue::Queue;
pub use ring_buffer::RingBuffer;
pub use slice::Slice;
pub use stack::Stack;
pub use string::String;
pub use vec::Vec;
pub use vecdeque::VecDeque;
pub use weak::Weak;
