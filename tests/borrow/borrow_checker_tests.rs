// Tests for borrow checking functionality

#[cfg(test)]
mod borrow_tests {
    use adeshlang::types::borrow_checker::{BorrowState, BorrowContext};

    #[test]
    fn test_unborrowed_to_shared() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(ctx.borrow_shared("x").is_ok());
        assert_eq!(ctx.get_state("x"), Some(BorrowState::Shared(1)));
    }

    #[test]
    fn test_multiple_shared_borrows() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(ctx.borrow_shared("x").is_ok());
        assert!(ctx.borrow_shared("x").is_ok());
        assert_eq!(ctx.get_state("x"), Some(BorrowState::Shared(2)));
    }

    #[test]
    fn test_shared_blocks_mut_borrow() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(ctx.borrow_shared("x").is_ok());
        assert!(ctx.borrow_mut("x").is_err());
    }

    #[test]
    fn test_mut_borrow_blocks_shared() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(ctx.borrow_mut("x").is_ok());
        assert!(ctx.borrow_shared("x").is_err());
    }

    #[test]
    fn test_cannot_double_mut_borrow() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(ctx.borrow_mut("x").is_ok());
        assert!(ctx.borrow_mut("x").is_err());
    }

    #[test]
    fn test_release_shared_borrow() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        ctx.borrow_shared("x").unwrap();
        ctx.borrow_shared("x").unwrap();
        assert!(ctx.unborrow_shared("x").is_ok());
        assert_eq!(ctx.get_state("x"), Some(BorrowState::Shared(1)));
        assert!(ctx.unborrow_shared("x").is_ok());
        assert_eq!(ctx.get_state("x"), Some(BorrowState::Unborrowed));
    }

    #[test]
    fn test_release_mut_borrow() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(ctx.borrow_mut("x").is_ok());
        assert!(ctx.unborrow_mut("x").is_ok());
        assert_eq!(ctx.get_state("x"), Some(BorrowState::Unborrowed));
    }

    #[test]
    fn test_move_unborrowed() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(ctx.move_var("x").is_ok());
        assert_eq!(ctx.get_state("x"), None);
    }

    #[test]
    fn test_cannot_move_borrowed() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        ctx.borrow_shared("x").unwrap();
        assert!(ctx.move_var("x").is_err());
    }

    #[test]
    fn test_cannot_move_mutably_borrowed() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        ctx.borrow_mut("x").unwrap();
        assert!(ctx.move_var("x").is_err());
    }

    #[test]
    fn test_is_borrowed_shared() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(!ctx.is_borrowed("x"));
        ctx.borrow_shared("x").unwrap();
        assert!(ctx.is_borrowed("x"));
    }

    #[test]
    fn test_is_borrowed_mut() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        assert!(!ctx.is_borrowed("x"));
        ctx.borrow_mut("x").unwrap();
        assert!(ctx.is_borrowed("x"));
    }

    #[test]
    fn test_sequential_borrows() {
        let mut ctx = BorrowContext::new();
        ctx.declare("p".to_string());
        
        // First shared borrow
        ctx.borrow_shared("p").unwrap();
        assert!(ctx.is_borrowed("p"));
        ctx.unborrow_shared("p").unwrap();
        assert!(!ctx.is_borrowed("p"));
        
        // Then mutable borrow
        ctx.borrow_mut("p").unwrap();
        assert!(ctx.is_borrowed("p"));
        ctx.unborrow_mut("p").unwrap();
        assert!(!ctx.is_borrowed("p"));
    }

    #[test]
    fn test_undefined_variable() {
        let mut ctx = BorrowContext::new();
        assert!(ctx.borrow_shared("unknown").is_err());
        assert!(ctx.borrow_mut("unknown").is_err());
    }

    #[test]
    fn test_error_message_quality() {
        let mut ctx = BorrowContext::new();
        ctx.declare("x".to_string());
        ctx.borrow_shared("x").unwrap();
        
        let err = ctx.borrow_mut("x");
        assert!(err.is_err());
        let msg = err.unwrap_err();
        assert!(msg.contains("Cannot create mutable reference"));
        assert!(msg.contains("already shared borrowed"));
    }
}
