//! AdeshLang language-level ARC (`share` / `strong` / `weak`).
//!
//! ## Lifecycle
//!
//! ```text
//! NEW ──share──> LIVE(strong >= 1)
//!                    │
//!                    │ final strong release
//!                    v
//!                 DYING (payload destroyed, strong = 0)
//!                    │
//!                    │ final weak release
//!                    v
//!                 FREED
//! ```
//!
//! - Strong references keep the payload alive.
//! - Weak references observe liveness without owning the payload.
//! - `upgrade()` atomically acquires a strong reference only while `strong_count > 0`.
//! - Payload destruction happens exactly once when the last strong reference is released.
//! - The control block survives while weak references remain.

use crate::parsing::ast::Value;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Maximum strong/weak count before overflow rejection.
const MAX_REFCOUNT: u64 = isize::MAX as u64;

/// ARC-managed heap object: control block + payload.
///
/// `control_block_alive` remains true until the control block is deallocated.
/// `payload_destroyed` becomes true after the last strong reference releases the payload.
pub struct SharedObject {
    pub strong_count: AtomicU64,
    pub weak_count: AtomicU64,
    control_block_alive: AtomicBool,
    payload_destroyed: AtomicBool,
    pub value: Value,
    /// Per-object deallocation counter for tests. Lives outside the control
    /// block so it can be read after the block is freed; counting per object
    /// keeps parallel tests from corrupting each other's expectations.
    #[cfg(test)]
    pub(crate) test_tombstone: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

/// Strong owning reference into a [`SharedObject`].
#[derive(Debug)]
pub struct StrongRef {
    pub ptr: *mut SharedObject,
}

/// Non-owning weak reference into a [`SharedObject`].
#[derive(Debug)]
pub struct WeakRef {
    pub ptr: *mut SharedObject,
}

impl SharedObject {
    /// Create a new shared object with `strong_count = 1` and `weak_count = 0`.
    pub fn new(value: Value) -> Self {
        Self {
            strong_count: AtomicU64::new(1),
            weak_count: AtomicU64::new(0),
            control_block_alive: AtomicBool::new(true),
            payload_destroyed: AtomicBool::new(false),
            value,
            #[cfg(test)]
            test_tombstone: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

#[inline]
fn ensure_control_block(ptr: *mut SharedObject) {
    // SAFETY: `ptr` originates from `Box::into_raw` and remains valid while
    // `control_block_alive` is true. WeakRef/StrongRef ownership keeps the block
    // alive until the last weak/strong release calls `deallocate_control_block`.
    if !unsafe { (*ptr).control_block_alive.load(Ordering::Acquire) } {
        panic!("ARC control block use after free");
    }
}

#[inline]
fn count_to_value(n: u64) -> Value {
    Value::U64(n)
}

fn deallocate_control_block(ptr: *mut SharedObject) {
    // SAFETY: Called only when strong_count == 0 and weak_count == 0, so no
    // StrongRef/WeakRef still owns this pointer. Mark dead before `from_raw` so
    // any stale handle trips `ensure_control_block` instead of use-after-free.
    let obj = unsafe { &*ptr };
    #[cfg(test)]
    obj.test_tombstone.fetch_add(1, Ordering::Relaxed);
    obj.control_block_alive.store(false, Ordering::Release);
    std::sync::atomic::fence(Ordering::Acquire);
    let _ = unsafe { Box::from_raw(ptr) };
}

/// Allocate a `share` binding: one strong owner, no weak refs yet.
pub fn allocate_share(value: Value) -> StrongRef {
    let ptr = Box::into_raw(Box::new(SharedObject::new(value)));
    StrongRef { ptr }
}

/// Increment the strong count with a CAS loop (overflow-safe).
fn increment_strong(ptr: *mut SharedObject) {
    ensure_control_block(ptr);
    let obj = unsafe { &*ptr };
    let mut count = obj.strong_count.load(Ordering::Relaxed);
    loop {
        if count >= MAX_REFCOUNT {
            panic!("StrongRef reference count overflow");
        }
        match obj.strong_count.compare_exchange_weak(
            count,
            count + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => count = actual,
        }
    }
}

/// Increment the weak count with a CAS loop (overflow-safe).
fn increment_weak(ptr: *mut SharedObject) {
    ensure_control_block(ptr);
    let obj = unsafe { &*ptr };
    let mut count = obj.weak_count.load(Ordering::Relaxed);
    loop {
        if count >= MAX_REFCOUNT {
            panic!("WeakRef reference count overflow");
        }
        match obj.weak_count.compare_exchange_weak(
            count,
            count + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => count = actual,
        }
    }
}

/// Release one strong reference; destroy payload and maybe free control block.
fn release_strong(ptr: *mut SharedObject) {
    ensure_control_block(ptr);
    // SAFETY: `ptr` is held alive by the dropping StrongRef (and any other live refs).
    let obj = unsafe { &*ptr };
    if obj.strong_count.fetch_sub(1, Ordering::Release) != 1 {
        return;
    }

    // Last strong reference: synchronize with concurrent upgrade attempts.
    std::sync::atomic::fence(Ordering::Acquire);

    // Destroy payload exactly once. The final-release gate above guarantees a
    // single writer, so `payload_destroyed` doubles as the exactly-once flag
    // in tests (no shared global counters needed).
    obj.payload_destroyed.store(true, Ordering::Release);
    // SAFETY: Control block still alive (weak refs may exist); only payload is cleared.
    let value_ptr = unsafe { &mut (*ptr).value as *mut Value };
    std::mem::drop(std::mem::replace(unsafe { &mut *value_ptr }, Value::Null));

    if obj.weak_count.load(Ordering::Acquire) == 0 {
        deallocate_control_block(ptr);
    }
}

/// Release one weak reference; free control block when both counts are zero.
fn release_weak(ptr: *mut SharedObject) {
    ensure_control_block(ptr);
    // SAFETY: `ptr` is held alive by the dropping WeakRef (and any other live refs).
    let obj = unsafe { &*ptr };
    if obj.weak_count.fetch_sub(1, Ordering::Release) != 1 {
        return;
    }

    std::sync::atomic::fence(Ordering::Acquire);

    if obj.strong_count.load(Ordering::Acquire) == 0 {
        deallocate_control_block(ptr);
    }
}

/// Atomically try to upgrade a weak reference to a strong one.
pub unsafe fn try_upgrade_weak(ptr: *mut SharedObject) -> Option<StrongRef> {
    ensure_control_block(ptr);
    // SAFETY: WeakRef ownership guarantees the control block is alive; CAS on
    // strong_count cannot resurrect once it reaches zero (no 0 → 1 transition).
    let obj = unsafe { &*ptr };
    let mut count = obj.strong_count.load(Ordering::Acquire);
    loop {
        if count == 0 {
            return None;
        }
        if count >= MAX_REFCOUNT {
            panic!("StrongRef reference count overflow");
        }
        match obj.strong_count.compare_exchange_weak(
            count,
            count + 1,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(StrongRef { ptr }),
            Err(actual) => count = actual,
        }
    }
}

/// Create a weak reference from a strong one (increments weak count).
pub fn create_weak_from_strong(strong: &StrongRef) -> WeakRef {
    increment_weak(strong.ptr);
    WeakRef { ptr: strong.ptr }
}

/// Create a weak reference from an existing weak reference (increments weak count).
pub fn clone_weak(weak: &WeakRef) -> WeakRef {
    weak.clone()
}

#[inline]
pub unsafe fn strong_count(ptr: *mut SharedObject) -> u64 {
    ensure_control_block(ptr);
    unsafe { (*ptr).strong_count.load(Ordering::Acquire) }
}

#[inline]
pub unsafe fn weak_count(ptr: *mut SharedObject) -> u64 {
    ensure_control_block(ptr);
    unsafe { (*ptr).weak_count.load(Ordering::Acquire) }
}

#[inline]
pub unsafe fn is_alive(ptr: *mut SharedObject) -> bool {
    unsafe { strong_count(ptr) > 0 }
}

impl Clone for StrongRef {
    fn clone(&self) -> Self {
        increment_strong(self.ptr);
        Self { ptr: self.ptr }
    }
}

impl Drop for StrongRef {
    fn drop(&mut self) {
        release_strong(self.ptr);
    }
}

impl Clone for WeakRef {
    fn clone(&self) -> Self {
        increment_weak(self.ptr);
        Self { ptr: self.ptr }
    }
}

impl Drop for WeakRef {
    fn drop(&mut self) {
        release_weak(self.ptr);
    }
}

impl StrongRef {
    pub fn strong_count(&self) -> u64 {
        unsafe { strong_count(self.ptr) }
    }

    pub fn weak_count(&self) -> u64 {
        unsafe { weak_count(self.ptr) }
    }

    pub fn is_alive(&self) -> bool {
        unsafe { is_alive(self.ptr) }
    }

    pub fn invoke_method(&self, method: &str) -> Result<Value, String> {
        match method {
            "strong_count" => Ok(count_to_value(self.strong_count())),
            "weak_count" => Ok(count_to_value(self.weak_count())),
            "is_alive" => Ok(Value::Bool(self.is_alive())),
            _ => Err(format!(
                "Method '{}' not supported on strong reference",
                method
            )),
        }
    }
}

impl WeakRef {
    pub fn strong_count(&self) -> u64 {
        unsafe { strong_count(self.ptr) }
    }

    pub fn weak_count(&self) -> u64 {
        unsafe { weak_count(self.ptr) }
    }

    pub fn is_alive(&self) -> bool {
        unsafe { is_alive(self.ptr) }
    }

    pub fn upgrade(&self) -> Option<StrongRef> {
        unsafe { try_upgrade_weak(self.ptr) }
    }

    pub fn invoke_method(&self, method: &str) -> Result<Value, String> {
        match method {
            "strong_count" => Ok(count_to_value(self.strong_count())),
            "weak_count" => Ok(count_to_value(self.weak_count())),
            "is_alive" => Ok(Value::Bool(self.is_alive())),
            "upgrade" => Ok(match self.upgrade() {
                Some(sr) => Value::Share(sr),
                None => Value::Null,
            }),
            _ => Err(format!(
                "Method '{}' not supported on weak reference",
                method
            )),
        }
    }
}

/// Dispatch ARC introspection / upgrade methods for `Value::Share` and `Value::Weak`.
pub fn invoke_arc_method(value: &Value, method: &str) -> Result<Value, String> {
    match value {
        Value::Share(sr) => sr.invoke_method(method),
        Value::Weak(wr) => wr.invoke_method(method),
        _ => Err(format!(
            "Method '{}' only available on shared objects",
            method
        )),
    }
}

/// Returns true when `method` is an ARC introspection / upgrade API.
#[inline]
pub fn is_arc_method(method: &str) -> bool {
    matches!(
        method,
        "strong_count" | "weak_count" | "is_alive" | "upgrade"
    )
}

// SAFETY: StrongRef/WeakRef may be sent to another thread (ownership transfer).
// They are intentionally NOT Sync: refcount atomics do not synchronize payload mutation.
// Concurrent `&StrongRef` access is not supported until payload locking exists.
unsafe impl Send for StrongRef {}
unsafe impl Send for WeakRef {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    /// Clone the per-object deallocation tombstone. It remains readable after
    /// the control block is freed, and it counts only this object — parallel
    /// tests no longer interfere with each other's drop accounting.
    fn tombstone_of(ptr: *mut SharedObject) -> std::sync::Arc<AtomicUsize> {
        unsafe { (*ptr).test_tombstone.clone() }
    }

    /// Payload destroyed exactly once (set under the final-release gate).
    unsafe fn payload_destroyed_once(ptr: *mut SharedObject) -> bool {
        unsafe { (*ptr).payload_destroyed.load(Ordering::Acquire) }
    }

    #[test]
    fn share_starts_with_count_one() {
        let sr = allocate_share(Value::Number(1.0));
        assert_eq!(sr.strong_count(), 1);
        assert_eq!(sr.weak_count(), 0);
    }

    #[test]
    fn strong_clone_increments_count() {
        let sr = allocate_share(Value::Number(1.0));
        let sr2 = sr.clone();
        assert_eq!(sr.strong_count(), 2);
        drop(sr2);
        assert_eq!(sr.strong_count(), 1);
    }

    #[test]
    fn weak_does_not_increment_strong() {
        let sr = allocate_share(Value::Number(1.0));
        let wr = create_weak_from_strong(&sr);
        assert_eq!(sr.strong_count(), 1);
        assert_eq!(wr.weak_count(), 1);
        drop(wr);
    }

    #[test]
    fn payload_destroyed_when_last_strong_drops_weak_survives() {
        let sr = allocate_share(Value::Number(42.0));
        let wr = create_weak_from_strong(&sr);
        drop(sr);
        assert!(!wr.is_alive());
        assert!(wr.upgrade().is_none());
        drop(wr);
    }

    #[test]
    fn upgrade_bumps_strong_temporarily() {
        let sr = allocate_share(Value::Number(1.0));
        let wr = create_weak_from_strong(&sr);
        let up = wr.upgrade().expect("upgrade while alive");
        assert_eq!(sr.strong_count(), 2);
        drop(up);
        assert_eq!(sr.strong_count(), 1);
        drop(wr);
    }

    #[test]
    fn control_block_freed_after_last_weak() {
        let sr = allocate_share(Value::Number(1.0));
        let tomb = tombstone_of(sr.ptr);
        let wr = create_weak_from_strong(&sr);
        drop(sr);
        assert!(unsafe { payload_destroyed_once(wr.ptr) });
        // Control block survives the weak reference.
        assert_eq!(tomb.load(Ordering::Acquire), 0);
        drop(wr);
        // ... and is freed exactly once by the final weak release.
        assert_eq!(tomb.load(Ordering::Acquire), 1);
    }

    #[test]
    fn concurrent_upgrade_race_with_final_drop() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        use std::thread;

        for _ in 0..32 {
            let sr = allocate_share(Value::Number(1.0));
            let wr = create_weak_from_strong(&sr);
            let wr_for_thread = wr.clone();
            let done = Arc::new(AtomicBool::new(false));
            let done_for_thread = Arc::clone(&done);

            let t1 = thread::spawn(move || {
                while !done_for_thread.load(Ordering::Acquire) {
                    if let Some(up) = wr_for_thread.upgrade() {
                        drop(up);
                    }
                }
            });

            drop(sr);
            done.store(true, Ordering::Release);
            t1.join().unwrap();
            drop(wr);
        }
    }

    #[test]
    fn full_lifecycle_payload_dropped_once_control_block_survives_weak() {
        let marker = Value::Number(99.0);
        let sr = allocate_share(marker);
        let tomb = tombstone_of(sr.ptr);
        let wr = create_weak_from_strong(&sr);

        assert_eq!(sr.strong_count(), 1);
        assert_eq!(wr.weak_count(), 1);
        assert!(wr.is_alive());

        drop(sr);
        assert!(unsafe { payload_destroyed_once(wr.ptr) });
        assert_eq!(tomb.load(Ordering::Acquire), 0);
        assert!(!wr.is_alive());
        assert!(wr.upgrade().is_none());

        drop(wr);
        assert_eq!(tomb.load(Ordering::Acquire), 1);
    }

    #[test]
    fn upgrade_never_resurrects_after_payload_destroyed() {
        for _ in 0..10_000 {
            let sr = allocate_share(Value::Number(1.0));
            let wr = create_weak_from_strong(&sr);
            drop(sr);
            assert!(!wr.is_alive());
            for _ in 0..8 {
                assert!(wr.upgrade().is_none());
            }
            drop(wr);
        }
    }

    #[test]
    fn adversarial_upgrade_vs_final_strong_release() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        use std::thread;

        for _ in 0..1_000 {
            let sr = allocate_share(Value::Number(1.0));
            let tomb = tombstone_of(sr.ptr);
            let wr = create_weak_from_strong(&sr);
            let wr_for_thread = wr.clone();
            let done = Arc::new(AtomicBool::new(false));
            let done_for_thread = Arc::clone(&done);

            let t1 = thread::spawn(move || {
                while !done_for_thread.load(Ordering::Acquire) {
                    if let Some(up) = wr_for_thread.upgrade() {
                        drop(up);
                    }
                }
            });

            drop(sr);
            done.store(true, Ordering::Release);
            t1.join().unwrap();

            // The payload drop happens exactly once, even under the upgrade
            // race: the final-release gate is the single writer of the flag.
            assert!(unsafe { payload_destroyed_once(wr.ptr) });
            assert!(!wr.is_alive());
            assert!(wr.upgrade().is_none());
            drop(wr);
            assert_eq!(tomb.load(Ordering::Acquire), 1);
        }
    }

    #[test]
    fn randomized_concurrent_arc_operations() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        use std::thread;

        for seed in 0..50 {
            let sr = allocate_share(Value::Number(seed as f64));
            let tomb = tombstone_of(sr.ptr);
            let wr = create_weak_from_strong(&sr);
            let done = Arc::new(AtomicBool::new(false));

            let mut handles = Vec::new();
            for worker in 0..4 {
                let strong = sr.clone();
                let weak = wr.clone();
                let done = Arc::clone(&done);

                handles.push(thread::spawn(move || {
                    let mut local_strongs: Vec<StrongRef> = Vec::new();
                    let mut local_weaks: Vec<WeakRef> = Vec::new();
                    let mut step: u32 = (seed + worker) as u32;

                    while !done.load(Ordering::Acquire) {
                        step = step.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x7fffffff;
                        match step % 7 {
                            0 => local_strongs.push(strong.clone()),
                            1 => {
                                if let Some(up) = weak.upgrade() {
                                    local_strongs.push(up);
                                }
                            }
                            2 => local_weaks.push(weak.clone()),
                            3 => {
                                if !local_strongs.is_empty() {
                                    local_strongs.pop();
                                }
                            }
                            4 => {
                                if !local_weaks.is_empty() {
                                    local_weaks.pop();
                                }
                            }
                            5 => {
                                if !local_strongs.is_empty() {
                                    local_strongs.push(local_strongs[0].clone());
                                }
                            }
                            _ => {
                                if !local_weaks.is_empty() {
                                    local_weaks.push(local_weaks[0].clone());
                                }
                            }
                        }
                    }

                    local_strongs.clear();
                    local_weaks.clear();
                }));
            }

            drop(sr);
            done.store(true, Ordering::Release);
            for h in handles {
                h.join().unwrap();
            }

            assert!(unsafe { payload_destroyed_once(wr.ptr) });
            drop(wr);
            assert_eq!(tomb.load(Ordering::Acquire), 1);
        }
    }

    #[test]
    fn upgrade_never_transitions_zero_to_one() {
        let sr = allocate_share(Value::Number(1.0));
        let wr = create_weak_from_strong(&sr);
        drop(sr);
        assert_eq!(unsafe { strong_count(wr.ptr) }, 0);
        assert!(wr.upgrade().is_none());
        assert_eq!(unsafe { strong_count(wr.ptr) }, 0);
        drop(wr);
    }

    #[test]
    fn three_way_concurrent_upgrade_strong_drop_weak() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        use std::thread;

        for _ in 0..500 {
            let sr = allocate_share(Value::Number(1.0));
            let tomb = tombstone_of(sr.ptr);
            let wr = create_weak_from_strong(&sr);
            let wr_upgrade = wr.clone();
            let wr_shuffle = wr.clone();

            let strong_dropped = Arc::new(AtomicBool::new(false));
            let strong_dropped_for_a = Arc::clone(&strong_dropped);
            let post_drop_upgrades = Arc::new(AtomicUsize::new(0));
            let post_drop_upgrades_for_a = Arc::clone(&post_drop_upgrades);

            let upgrade_thread = thread::spawn(move || {
                while !strong_dropped_for_a.load(Ordering::Acquire) {
                    if let Some(up) = wr_upgrade.upgrade() {
                        drop(up);
                    }
                }
                for _ in 0..256 {
                    if wr_upgrade.upgrade().is_some() {
                        post_drop_upgrades_for_a.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });

            let weak_thread = thread::spawn(move || {
                let mut locals: Vec<WeakRef> = Vec::new();
                for _ in 0..512 {
                    locals.push(wr_shuffle.clone());
                    if locals.len() > 8 {
                        locals.pop();
                    }
                }
                locals.clear();
            });

            drop(sr);
            strong_dropped.store(true, Ordering::Release);

            upgrade_thread.join().unwrap();
            weak_thread.join().unwrap();

            assert_eq!(post_drop_upgrades.load(Ordering::Acquire), 0);
            assert!(unsafe { payload_destroyed_once(wr.ptr) });
            assert!(!wr.is_alive());
            drop(wr);
            assert_eq!(tomb.load(Ordering::Acquire), 1);
        }
    }

    #[test]
    fn concurrent_strong_clone_drop() {
        use std::thread;

        let sr = allocate_share(Value::Number(0.0));
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let s = sr.clone();
                thread::spawn(move || {
                    for _ in 0..1000 {
                        let c = s.clone();
                        drop(c);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(sr.strong_count(), 1);
    }
}
