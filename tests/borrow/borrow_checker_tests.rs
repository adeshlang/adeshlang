// Tests for borrow checking functionality

#[cfg(test)]
mod borrow_tests {
    use adeshlang::utils::memory::{BorrowChecker, BorrowState};

    #[test]
    fn test_unborrowed_to_shared() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow("x"));
        assert_eq!(ctx.get_state("x"), Some(&BorrowState::ImmutablyBorrowed(1)));
    }

    #[test]
    fn test_multiple_shared_borrows() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow("x"));
        assert!(ctx.try_borrow("x"));
        assert_eq!(ctx.get_state("x"), Some(&BorrowState::ImmutablyBorrowed(2)));
    }

    #[test]
    fn test_shared_blocks_mut_borrow() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow("x"));
        assert!(!ctx.try_borrow_mut("x"));
    }

    #[test]
    fn test_mut_borrow_blocks_shared() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow_mut("x"));
        assert!(!ctx.try_borrow("x"));
    }

    #[test]
    fn test_cannot_double_mut_borrow() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow_mut("x"));
        assert!(!ctx.try_borrow_mut("x"));
    }

    #[test]
    fn test_release_shared_borrow() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow("x"));
        assert!(ctx.try_borrow("x"));
        ctx.release_borrow("x");
        assert_eq!(ctx.get_state("x"), Some(&BorrowState::ImmutablyBorrowed(1)));
        ctx.release_borrow("x");
        assert_eq!(ctx.get_state("x"), Some(&BorrowState::Unborrowed));
    }

    #[test]
    fn test_release_mut_borrow() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow_mut("x"));
        ctx.release_borrow_mut("x");
        assert_eq!(ctx.get_state("x"), Some(&BorrowState::Unborrowed));
    }

    #[test]
    fn test_move_unborrowed() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        ctx.mark_moved("x");
        assert_eq!(ctx.get_state("x"), Some(&BorrowState::Moved));
    }

    #[test]
    fn test_cannot_move_borrowed() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow("x"));
        // Moving marked value invalidates borrowing
        ctx.mark_moved("x");
        assert!(!ctx.try_borrow("x"));
    }

    #[test]
    fn test_cannot_move_mutably_borrowed() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow_mut("x"));
        ctx.mark_moved("x");
        assert!(!ctx.try_borrow_mut("x"));
    }

    #[test]
    fn test_is_borrowed_shared() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(!ctx.is_borrowed("x"));
        assert!(ctx.try_borrow("x"));
        assert!(ctx.is_borrowed("x"));
    }

    #[test]
    fn test_is_borrowed_mut() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(!ctx.is_borrowed("x"));
        assert!(ctx.try_borrow_mut("x"));
        assert!(ctx.is_borrowed("x"));
    }

    #[test]
    fn test_sequential_borrows() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("p".to_string());
        
        // First shared borrow
        assert!(ctx.try_borrow("p"));
        assert!(ctx.is_borrowed("p"));
        ctx.release_borrow("p");
        assert!(!ctx.is_borrowed("p"));
        
        // Then mutable borrow
        assert!(ctx.try_borrow_mut("p"));
        assert!(ctx.is_borrowed("p"));
        ctx.release_borrow_mut("p");
        assert!(!ctx.is_borrowed("p"));
    }

    #[test]
    fn test_undefined_variable() {
        let mut ctx = BorrowChecker::new();
        assert!(!ctx.try_borrow("unknown"));
        assert!(!ctx.try_borrow_mut("unknown"));
    }

    #[test]
    fn test_error_message_quality() {
        let mut ctx = BorrowChecker::new();
        ctx.declare("x".to_string());
        assert!(ctx.try_borrow("x"));
        
        assert!(!ctx.try_borrow_mut("x"));
        let errors = ctx.errors();
        assert!(!errors.is_empty());
        let msg = &errors[0].message;
        assert!(msg.contains("Cannot borrow 'x' mutably"));
        assert!(msg.contains("immutably borrowed"));
    }
}
