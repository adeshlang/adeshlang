//! Borrow Checker Integration Test
//!
//! Regression and semantic tests for the borrow checker including:
//! - Conflicting borrow detection
//! - Lifetime tracking through loops and closures
//! - Move vs borrow capture analysis
//! - Error message quality and diagnostics

mod borrow_checker_tests;

#[cfg(test)]
mod tests {
    use adeshlang::memory::raii::transform_ast_with_raii;
    use adeshlang::parsing::compile_time_memory_safety::check_memory_safety_compile_time;
    use adeshlang::parsing::hir_lower::ast_to_hir;
    use adeshlang::parsing::lexer::Lexer;
    use adeshlang::parsing::parser::Parser;
    use adeshlang::utils::memory::{BorrowChecker, BorrowState, LifetimeTracker, ScopeId};
    use adeshlang::{Interpreter, ModuleLoader};
    use std::path::Path;

    fn check_code_safety(src: &str) -> Result<(), String> {
        let mut lexer = Lexer::new(src);
        let tokens = lexer
            .tokenize()
            .map_err(|e| format!("Lex error: {:?}", e))?;
        let mut parser = Parser::new(tokens, Some("borrow_test.adesh".to_string()));
        let ast = parser
            .parse_program()
            .map_err(|e| format!("Parse error: {:?}", e))?;
        let ast = transform_ast_with_raii(ast);
        let hir = ast_to_hir(&ast, false).map_err(|e| format!("HIR error: {:?}", e))?;
        check_memory_safety_compile_time(&hir)
    }

    fn run_code(src: &str) -> Result<(), String> {
        let src_owned = src.to_string();
        std::thread::Builder::new()
            .name("borrow_test_thread".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let mut loader = ModuleLoader::new(Path::new("."));
                let mut interp = Interpreter::new();
                interp
                    .run_module(&src_owned, &mut loader, None)
                    .map_err(|e| e.to_string())
            })
            .unwrap()
            .join()
            .unwrap()
    }

    /// Test basic borrow conflict detection
    #[test]
    fn test_borrow_conflict_detection() {
        // Conflicting mutable and immutable borrows must be rejected
        let conflict_src = r#"
        fn main() {
            let data = [10, 20, 30];
            let ref1 = &mut data;
            let ref2 = &mut data;
            print(ref1);
            print(ref2);
        }
        "#;
        let res = check_code_safety(conflict_src);
        assert!(
            res.is_err(),
            "Expected conflicting mutable borrows to be rejected"
        );
        let err = res.unwrap_err();
        assert!(
            err.to_lowercase().contains("cannot borrow") || err.to_lowercase().contains("mutable"),
            "Error should indicate borrow conflict, got: {}",
            err
        );

        // Single borrow followed by usage is valid
        let valid_src = r#"
        fn main() {
            let data = [1, 2, 3];
            let r = &data;
            print(r);
        }
        "#;
        let valid_res = check_code_safety(valid_src);
        assert!(
            valid_res.is_ok(),
            "Expected valid single borrow to pass, got: {:?}",
            valid_res.err()
        );
    }

    /// Test lifetime tracking through loops
    #[test]
    fn test_lifetime_through_loops() {
        let mut tracker = LifetimeTracker::new();
        tracker.register_var("outer".to_string(), false);

        let loop_scope = tracker.enter_scope();
        tracker.register_var("inner".to_string(), false);

        // Inner variable inside loop body cannot escape to outer scope
        let escape_res = tracker.validate_no_escape("inner", ScopeId(0));
        assert!(
            escape_res.is_err(),
            "Loop inner variable must not escape to outer scope"
        );

        // Outer variable is accessible within loop scope
        let outer_res = tracker.validate_no_escape("outer", loop_scope);
        assert!(
            outer_res.is_ok(),
            "Outer variable should be valid within loop scope"
        );

        tracker.exit_scope();
    }

    /// Test closure capture analysis
    #[test]
    fn test_closure_capture_analysis() {
        // Test that closure can capture shared reference and read it correctly
        let code = r#"
        fn main() {
            let x = 100;
            let get_x = fn() {
                return x + 5;
            };
            assert_eq(get_x(), 105);
            assert_eq(x, 100);
        }
        main();
        "#;
        let res = run_code(code);
        assert!(res.is_ok(), "Closure capture test failed: {:?}", res.err());
    }

    /// Test error message quality includes variable name
    #[test]
    fn test_error_message_includes_variable_name() {
        let mut checker = BorrowChecker::new();
        checker.declare("targeted_buffer".to_string());
        assert!(checker.try_borrow_mut("targeted_buffer"));

        // Second borrow fails and records error with variable name
        assert!(!checker.try_borrow("targeted_buffer"));
        assert!(checker.has_errors());

        let errors = checker.get_errors();
        assert!(!errors.is_empty(), "Expected error records");
        let matched = errors
            .iter()
            .any(|e| e.variable == "targeted_buffer" && e.message.contains("targeted_buffer"));
        assert!(
            matched,
            "Error message must explicitly mention variable name 'targeted_buffer', got: {:?}",
            errors
        );
    }

    /// Test error message includes borrow kind
    #[test]
    fn test_error_message_includes_borrow_kind() {
        let mut checker = BorrowChecker::new();
        checker.declare("resource".to_string());
        assert!(checker.try_borrow_mut("resource"));

        assert!(!checker.try_borrow("resource"));
        let errors = checker.get_errors();
        let has_kind_detail = errors.iter().any(|e| {
            e.message.to_lowercase().contains("immutably")
                || e.message.to_lowercase().contains("mutably")
        });
        assert!(
            has_kind_detail,
            "Error message must specify borrow kind (mutable/immutable), got: {:?}",
            errors
        );
    }

    /// Test error message includes lifetime span
    #[test]
    fn test_error_message_includes_span() {
        let conflict_src = r#"
        fn compute() {
            let buffer = [1, 2, 3];
            let b1 = &mut buffer;
            let b2 = &mut buffer;
            print(b1);
            print(b2);
        }
        "#;
        let res = check_code_safety(conflict_src);
        assert!(res.is_err(), "Expected safety check error");
        let err = res.unwrap_err();
        assert!(
            !err.is_empty(),
            "Error message should contain details about the borrow conflict span"
        );
    }

    /// Test conflicting borrows table format
    #[test]
    fn test_conflicting_borrows_table() {
        let mut checker = BorrowChecker::new();
        checker.declare("shared_data".to_string());

        // Initially unborrowed
        assert_eq!(
            checker.get_state("shared_data"),
            Some(&BorrowState::Unborrowed)
        );

        // First immutable borrow
        assert!(checker.try_borrow("shared_data"));
        assert_eq!(
            checker.get_state("shared_data"),
            Some(&BorrowState::ImmutablyBorrowed(1))
        );

        // Second immutable borrow increments count
        assert!(checker.try_borrow("shared_data"));
        assert_eq!(
            checker.get_state("shared_data"),
            Some(&BorrowState::ImmutablyBorrowed(2))
        );

        // Mutable borrow blocked by immutable borrow
        assert!(!checker.try_borrow_mut("shared_data"));
        assert!(checker.has_errors());

        // Release one immutable borrow
        checker.release_borrow("shared_data");
        assert_eq!(
            checker.get_state("shared_data"),
            Some(&BorrowState::ImmutablyBorrowed(1))
        );

        // Release second immutable borrow
        checker.release_borrow("shared_data");
        assert_eq!(
            checker.get_state("shared_data"),
            Some(&BorrowState::Unborrowed)
        );

        // Now mutable borrow succeeds
        checker.clear_errors();
        assert!(checker.try_borrow_mut("shared_data"));
        assert_eq!(
            checker.get_state("shared_data"),
            Some(&BorrowState::MutablyBorrowed)
        );
    }

    /// Test nested loop borrow tracking
    #[test]
    fn test_nested_loop_borrows() {
        let mut tracker = LifetimeTracker::new();
        tracker.register_var("root".to_string(), true);

        let outer_loop = tracker.enter_scope();
        tracker.register_var("outer_iter".to_string(), false);

        let inner_loop = tracker.enter_scope();
        tracker.register_var("inner_iter".to_string(), false);

        // Inner cannot escape to outer loop or global
        assert!(
            tracker
                .validate_no_escape("inner_iter", outer_loop)
                .is_err()
        );
        assert!(
            tracker
                .validate_no_escape("inner_iter", ScopeId(0))
                .is_err()
        );

        // Outer iter can be accessed in inner loop
        assert!(tracker.validate_no_escape("outer_iter", inner_loop).is_ok());

        // Global root can be accessed in both
        assert!(tracker.validate_no_escape("root", inner_loop).is_ok());
        assert!(tracker.validate_no_escape("root", outer_loop).is_ok());

        tracker.exit_scope(); // exit inner
        tracker.exit_scope(); // exit outer
    }

    /// Test move in closure
    #[test]
    fn test_move_in_closure() {
        let mut checker = BorrowChecker::new();
        checker.declare("unique_item".to_string());

        // Mark as moved into closure
        checker.mark_moved("unique_item");
        assert_eq!(checker.get_state("unique_item"), Some(&BorrowState::Moved));

        // Attempting to borrow moved value fails
        assert!(!checker.try_borrow("unique_item"));
        assert!(!checker.try_borrow_mut("unique_item"));
        assert!(checker.has_errors());
    }

    /// Test borrow in closure
    #[test]
    fn test_borrow_in_closure() {
        let mut checker = BorrowChecker::new();
        checker.declare("captured_ref".to_string());

        // Closure holds immutable borrow
        assert!(checker.try_borrow("captured_ref"));

        // Outer scope can also read immutably
        assert!(checker.try_borrow("captured_ref"));

        // But outer scope cannot mutate while closure holds borrow
        assert!(!checker.try_borrow_mut("captured_ref"));

        // Once closure finishes and releases
        checker.release_borrow("captured_ref");
        checker.release_borrow("captured_ref");
        checker.clear_errors();

        // Mutable borrow succeeds
        assert!(checker.try_borrow_mut("captured_ref"));
    }
}
