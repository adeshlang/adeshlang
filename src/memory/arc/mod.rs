use crate::memory::policy::ThreadMode;
use crate::parsing::ast::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod shared_object;
pub use shared_object::{
    SharedObject, StrongRef, WeakRef, allocate_share, clone_weak, create_weak_from_strong,
    invoke_arc_method, is_alive, is_arc_method, strong_count, try_upgrade_weak, weak_count,
};

pub type ArcId = u64;

/// Metadata for a single ARC-managed value.
///
/// The `Value` is stored directly (not wrapped in `Mutex` or `Arc`).
/// Thread safety is provided by the outer `Mutex<ArcManager>` that
/// guards all access to the `ArcManager` and its entries.
/// Reference counts use `AtomicUsize` so they remain correct even
/// when the manager is shared across threads via the global lock.
#[derive(Debug)]
pub struct ArcMetadata {
    strong_count: AtomicUsize,
    weak_count: AtomicUsize,
    value: Value,
}

pub struct ArcManager {
    arcs: HashMap<ArcId, ArcMetadata>,
    next_arc_id: ArcId,
    thread_mode: ThreadMode,
}

// ArcManager is always accessed through a Mutex<ArcManager>, so it is safe
// to share across threads. The atomic refcounts handle concurrent increment/decrement.
unsafe impl Send for ArcManager {}

impl ArcManager {
    pub fn new(thread_mode: ThreadMode) -> Self {
        ArcManager {
            arcs: HashMap::new(),
            next_arc_id: 1,
            thread_mode,
        }
    }

    /// Allocate a new ARC-managed value with strong_count = 1.
    pub fn allocate_arc(&mut self, value: Value) -> ArcId {
        let id = self.next_arc_id;
        self.next_arc_id += 1;

        let metadata = ArcMetadata {
            strong_count: AtomicUsize::new(1),
            weak_count: AtomicUsize::new(0),
            value,
        };

        self.arcs.insert(id, metadata);
        id
    }

    /// Increment the strong reference count for an ARC value.
    pub fn clone_arc(&self, arc_id: ArcId) -> Result<(), String> {
        if let Some(metadata) = self.arcs.get(&arc_id) {
            let count = metadata.strong_count.fetch_add(1, self.ordering());
            if count == usize::MAX {
                return Err("ARC reference count overflow".to_string());
            }
            Ok(())
        } else {
            Err(format!("Invalid ARC id: {}", arc_id))
        }
    }

    /// Decrement the strong reference count. When it reaches zero the value
    /// is dropped and, if no weak references remain, the entry is removed.
    pub fn drop_arc(&mut self, arc_id: ArcId) -> Result<(), String> {
        // Decrement first; if we hit zero we need to remove the entry.
        let prev_count = {
            let metadata = self
                .arcs
                .get(&arc_id)
                .ok_or_else(|| format!("Invalid ARC id: {}", arc_id))?;
            metadata.strong_count.fetch_sub(1, self.ordering())
        };

        if prev_count == 1 {
            // Last strong reference — drop the value and check weak count.
            let remove = {
                let metadata = self.arcs.get(&arc_id).unwrap();
                metadata.weak_count.load(self.ordering()) == 0
            };
            if remove {
                self.arcs.remove(&arc_id);
            } else {
                // Weak references still exist: drop the value but keep the entry
                // so weak_count can be decremented. Replace with Null as a sentinel.
                if let Some(metadata) = self.arcs.get_mut(&arc_id) {
                    metadata.value = Value::Null;
                }
            }
        }
        Ok(())
    }

    /// Create a weak reference (increments weak_count, returns the same ArcId).
    pub fn create_weak(&self, arc_id: ArcId) -> Result<ArcId, String> {
        if let Some(metadata) = self.arcs.get(&arc_id) {
            let count = metadata.weak_count.fetch_add(1, self.ordering());
            if count == usize::MAX {
                return Err("Weak reference count overflow".to_string());
            }
            Ok(arc_id)
        } else {
            Err(format!("Invalid ARC id: {}", arc_id))
        }
    }

    /// Decrement the weak reference count. When it reaches zero and strong_count
    /// is also zero, the entry is removed from the map.
    pub fn drop_weak(&mut self, arc_id: ArcId) -> Result<(), String> {
        let prev_weak = {
            let metadata = self
                .arcs
                .get(&arc_id)
                .ok_or_else(|| format!("Invalid ARC id: {}", arc_id))?;
            metadata.weak_count.fetch_sub(1, self.ordering())
        };

        if prev_weak == 1 {
            // Last weak reference — remove entry if strong_count is also zero.
            let strong_zero = {
                let metadata = self.arcs.get(&arc_id).unwrap();
                metadata.strong_count.load(self.ordering()) == 0
            };
            if strong_zero {
                self.arcs.remove(&arc_id);
            }
        }
        Ok(())
    }

    /// Get a clone of the value stored under `arc_id`.
    pub fn get_value(&self, arc_id: ArcId) -> Result<Value, String> {
        if let Some(metadata) = self.arcs.get(&arc_id) {
            Ok(metadata.value.clone())
        } else {
            Err(format!("Invalid ARC id: {}", arc_id))
        }
    }

    /// Overwrite the value stored under `arc_id`.
    pub fn set_value(&mut self, arc_id: ArcId, value: Value) -> Result<(), String> {
        if let Some(metadata) = self.arcs.get_mut(&arc_id) {
            metadata.value = value;
            Ok(())
        } else {
            Err(format!("Invalid ARC id: {}", arc_id))
        }
    }

    pub fn strong_count(&self, arc_id: ArcId) -> Result<usize, String> {
        if let Some(metadata) = self.arcs.get(&arc_id) {
            Ok(metadata.strong_count.load(self.ordering()))
        } else {
            Err(format!("Invalid ARC id: {}", arc_id))
        }
    }

    pub fn weak_count(&self, arc_id: ArcId) -> Result<usize, String> {
        if let Some(metadata) = self.arcs.get(&arc_id) {
            Ok(metadata.weak_count.load(self.ordering()))
        } else {
            Err(format!("Invalid ARC id: {}", arc_id))
        }
    }

    /// Choose the atomic ordering based on thread mode.
    /// Single-threaded mode uses Relaxed (no cross-thread synchronization needed).
    /// Multi-threaded mode uses SeqCst for correctness.
    #[inline]
    fn ordering(&self) -> Ordering {
        if self.thread_mode == ThreadMode::MultiThread {
            Ordering::SeqCst
        } else {
            Ordering::Relaxed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::policy::ThreadMode;

    #[test]
    fn test_arc_allocation() {
        let mut manager = ArcManager::new(ThreadMode::SingleThread);
        let value = Value::Number(42.0);
        let arc_id = manager.allocate_arc(value);
        assert!(manager.strong_count(arc_id).is_ok());
    }

    #[test]
    fn test_arc_clone_drop() {
        let mut manager = ArcManager::new(ThreadMode::SingleThread);
        let value = Value::Number(42.0);
        let arc_id = manager.allocate_arc(value);

        assert_eq!(manager.strong_count(arc_id).unwrap(), 1);
        manager.clone_arc(arc_id).unwrap();
        assert_eq!(manager.strong_count(arc_id).unwrap(), 2);
        manager.drop_arc(arc_id).unwrap();
        assert_eq!(manager.strong_count(arc_id).unwrap(), 1);
    }

    #[test]
    fn test_arc_get_set_value() {
        let mut manager = ArcManager::new(ThreadMode::SingleThread);
        let arc_id = manager.allocate_arc(Value::Number(10.0));

        // Check initial value
        match manager.get_value(arc_id).unwrap() {
            Value::Number(n) => assert!((n - 10.0).abs() < f64::EPSILON),
            _ => panic!("Expected Number(10.0)"),
        }

        // Set new value
        manager.set_value(arc_id, Value::Number(99.0)).unwrap();
        match manager.get_value(arc_id).unwrap() {
            Value::Number(n) => assert!((n - 99.0).abs() < f64::EPSILON),
            _ => panic!("Expected Number(99.0)"),
        }
    }

    #[test]
    fn test_arc_weak_references() {
        let mut manager = ArcManager::new(ThreadMode::SingleThread);
        let arc_id = manager.allocate_arc(Value::Number(42.0));

        manager.create_weak(arc_id).unwrap();
        assert_eq!(manager.weak_count(arc_id).unwrap(), 1);

        // Dropping strong ref should keep entry alive (weak ref exists)
        manager.drop_arc(arc_id).unwrap();
        assert_eq!(manager.strong_count(arc_id).unwrap(), 0);

        // Dropping weak ref should remove the entry
        manager.drop_weak(arc_id).unwrap();
        assert!(manager.strong_count(arc_id).is_err());
    }

    #[test]
    fn test_arc_multi_thread_mode() {
        let mut manager = ArcManager::new(ThreadMode::MultiThread);
        let arc_id = manager.allocate_arc(Value::Number(1.0));
        manager.clone_arc(arc_id).unwrap();
        assert_eq!(manager.strong_count(arc_id).unwrap(), 2);
        manager.drop_arc(arc_id).unwrap();
        assert_eq!(manager.strong_count(arc_id).unwrap(), 1);
    }
}
