use std::rc::Rc;

use adeshlang::types::value_optimized::BorrowHandle;
use adeshlang::utils::memory::OwnershipTracker;

#[test]
fn tracker_shared_borrow_and_release() {
    let t = Rc::new(OwnershipTracker::new_unique());
    // initially movable and valid
    assert!(t.is_valid());
    assert!(t.can_move());

    // a shared borrow should succeed and make can_move false
    assert!(t.try_borrow());
    assert!(t.is_borrowed());
    assert!(!t.can_move());

    // release borrow
    t.release_borrow();
    assert!(!t.is_borrowed());
    assert!(t.can_move());
}

#[test]
fn borrowhandle_clone_and_drop_updates_tracker() {
    let t = Rc::new(OwnershipTracker::new_unique());
    {
        let bh = BorrowHandle::new_shared(t.clone()).expect("new_shared");
        assert!(t.is_borrowed());
        let bh2 = bh.clone();
        // still borrowed
        assert!(t.is_borrowed());
        drop(bh);
        // still borrowed because bh2 exists
        assert!(t.is_borrowed());
        drop(bh2);
    }
    // all borrows dropped
    assert!(!t.is_borrowed());
}

#[test]
fn mark_moved_prevents_new_borrows_and_is_invalid() {
    let t = Rc::new(OwnershipTracker::new_unique());
    assert!(t.try_borrow());
    t.release_borrow();
    // mark moved
    t.mark_moved();
    assert!(!t.is_valid());
    // cannot borrow after moved
    assert!(!t.try_borrow());
}
