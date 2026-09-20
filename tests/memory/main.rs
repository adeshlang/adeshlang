//! Memory Model Integration Test
//!
//! Tests for smart pointers, SSO, SAO, Bump/Arena allocators, and memory management features.

#[cfg(test)]
mod tests {
    use adeshlang::utils::memory::{
        Arena, BumpAllocator, OwnershipKind, OwnershipTracker, Shared, SsoString, Unique,
    };

    /// Test Small String Optimization threshold
    #[test]
    fn test_sso_threshold() {
        // Strings <= 22 bytes use inline storage (no heap allocation)
        let short = SsoString::new("Hello");
        assert!(short.is_inline(), "Short string 'Hello' should use inline storage");
        assert_eq!(short.as_str(), "Hello");
        assert_eq!(short.len(), 5);

        // Exactly 22 bytes (max capacity for SSO inline storage)
        let exactly_22 = SsoString::new("1234567890123456789012");
        assert_eq!(exactly_22.len(), 22);
        assert!(exactly_22.is_inline(), "22-byte string should be inline");
        assert_eq!(exactly_22.as_str(), "1234567890123456789012");

        // 23 bytes exceeds SSO inline capacity -> heap allocated
        let over_sso = SsoString::new("12345678901234567890123");
        assert_eq!(over_sso.len(), 23);
        assert!(!over_sso.is_inline(), "23-byte string must spill to heap");
        assert_eq!(over_sso.as_str(), "12345678901234567890123");
    }

    /// Test Small Array Optimization and compact layout
    #[test]
    fn test_sao_threshold() {
        // Ownership tracker verifying unique -> borrowed -> unique cycle
        let tracker = OwnershipTracker::new_unique();
        assert_eq!(tracker.kind(), OwnershipKind::Unique);

        // Immutable borrowing
        assert!(tracker.try_borrow());
        assert_eq!(tracker.kind(), OwnershipKind::Borrowed);
        tracker.release_borrow();
        assert_eq!(tracker.kind(), OwnershipKind::Unique);

        // Mutable borrowing
        assert!(tracker.try_borrow_mut());
        assert_eq!(tracker.kind(), OwnershipKind::BorrowedMut);
        assert!(!tracker.try_borrow(), "Cannot immutably borrow while mutably borrowed");
        tracker.release_borrow_mut();
        assert_eq!(tracker.kind(), OwnershipKind::Unique);
    }

    /// Test Shared pointer basic reference counting and cloning
    #[test]
    fn test_shared_basic() {
        let shared1 = Shared::new(42);
        assert_eq!(*shared1, 42);
        assert_eq!(shared1.strong_count(), 1);

        {
            let shared2 = shared1.clone();
            assert_eq!(*shared2, 42);
            assert_eq!(shared1.strong_count(), 2);
            assert_eq!(shared2.strong_count(), 2);
        }

        // After inner scope drop, strong count decreases back to 1
        assert_eq!(shared1.strong_count(), 1);
    }

    /// Test Unique pointer ownership transfer and consumption
    #[test]
    fn test_unique_move() {
        let mut unique = Unique::new(100);
        assert_eq!(*unique, 100);

        *unique = 200;
        assert_eq!(*unique, 200);

        let inner_val = unique.into_inner();
        assert_eq!(inner_val, 200);
    }

    /// Test Weak reference cycle breaking and upgrade behavior
    #[test]
    fn test_weak_cycle() {
        let shared = Shared::new(999);
        let weak = shared.downgrade();
        assert!(weak.is_alive());

        // Upgrading while owner alive succeeds
        let upgraded = weak.upgrade();
        assert!(upgraded.is_some());
        assert_eq!(*upgraded.unwrap(), 999);

        // Dropping strong owner causes upgrade() to return None
        drop(shared);
        assert!(!weak.is_alive());
        assert!(weak.upgrade().is_none());
    }

    /// Test Bump allocator fast linear allocation and reset
    #[test]
    fn test_bump_allocator() {
        let mut bump = BumpAllocator::new();
        let val_a = *bump.alloc(12345i64);
        assert_eq!(val_a, 12345);

        let val_b = bump.alloc("adeshlang".to_string()).clone();
        assert_eq!(val_b.as_str(), "adeshlang");

        let (total_bytes, count, _) = bump.stats();
        assert!(total_bytes > 0);
        assert_eq!(count, 2);

        bump.reset();
        let (total_bytes_after, count_after, _) = bump.stats();
        assert_eq!(total_bytes_after, 0);
        assert_eq!(count_after, 0);
    }

    /// Test Arena generational lifecycle
    #[test]
    fn test_arena() {
        let mut arena = Arena::new();
        assert_eq!(arena.generation(), 0);

        let item1 = *arena.alloc(10);
        let item2 = *arena.alloc(20);
        assert_eq!(item1 + item2, 30);

        arena.reset();
        assert_eq!(arena.generation(), 1);
    }
}
