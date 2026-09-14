//! Tests for defer keyword functionality
//!
//! Tests cover:
//! - Basic LIFO execution
//! - Nested scopes
//! - Early returns
//! - Loop control flow (break/continue)
//! - Multiple defers in same scope

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_defer_code(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
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

#[test]
fn test_basic_defer_lifo() {
    let result = run_defer_code(
        r#"
        fn main() {
            print("Start");
            defer { print("Defer 1 - executed last"); }
            defer { print("Defer 2 - executed second"); }
            defer { print("Defer 3 - executed first"); }
            print("End");
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Basic defer test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_with_early_return() {
    let result = run_defer_code(
        r#"
        fn test_return(): number {
            print("Function start");
            defer { print("Cleanup 1"); }
            defer { print("Cleanup 2"); }
            print("Before return");
            return 42;
        }
        
        fn main() {
            let result = test_return();
            print("Result:", result);
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Defer with early return test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_nested_scopes() {
    let result = run_defer_code(
        r#"
        fn main() {
            print("Outer start");
            defer { print("Outer defer"); }
            
            {
                print("Inner start");
                defer { print("Inner defer 1"); }
                defer { print("Inner defer 2"); }
                print("Inner end");
            }
            
            print("After inner block");
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Nested scopes defer test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_with_break() {
    let result = run_defer_code(
        r#"
        fn main() {
            let i = 0;
            while (i < 5) {
                defer { print("Loop iteration defer"); }
                print("Iteration:", i);
                i = i + 1;
                if (i == 3) {
                    break;
                }
            }
            print("After loop");
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Defer with break test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_with_variable_capture() {
    let result = run_defer_code(
        r#"
        fn main() {
            let x = 10;
            defer { print("Captured x:", x); }
            x = 20;
            defer { print("Captured x after change:", x); }
            print("Final x:", x);
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Defer variable capture test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_multiple_defers_in_function() {
    let result = run_defer_code(
        r#"
        fn test_multiple() {
            defer { print("Defer 1"); }
            print("Statement 1");
            defer { print("Defer 2"); }
            print("Statement 2");
            defer { print("Defer 3"); }
            print("Statement 3");
        }
        
        fn main() {
            test_multiple();
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Multiple defers test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_in_conditional() {
    let result = run_defer_code(
        r#"
        fn test_conditional(flag: bool) {
            print("Start");
            if (flag) {
                defer { print("If defer"); }
                print("In if branch");
            } else {
                defer { print("Else defer"); }
                print("In else branch");
            }
            print("End");
        }
        
        fn main() {
            test_conditional(true);
            print("---");
            test_conditional(false);
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Defer in conditional test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_resource_cleanup_pattern() {
    let result = run_defer_code(
        r#"
        fn process_data(name: string) {
            print("Opening:", name);
            defer { print("Closing:", name); }
            
            print("Processing:", name);
            print("Working with:", name);
        }
        
        fn main() {
            process_data("file1.txt");
            process_data("file2.txt");
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Defer resource cleanup test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_empty_block() {
    let result = run_defer_code(
        r#"
        fn main() {
            print("Before defer");
            defer { }
            print("After defer");
        }
    "#,
    );

    // Should not panic with empty defer block
    assert!(
        result.is_ok(),
        "Empty defer block test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_with_function_call() {
    let result = run_defer_code(
        r#"
        fn cleanup_helper() {
            print("Helper cleanup called");
        }
        
        fn main() {
            print("Start");
            defer { cleanup_helper(); }
            print("Middle");
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Defer with function call test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_defer_with_multiple_statements() {
    let result = run_defer_code(
        r#"
        fn main() {
            defer {
                print("First statement in defer");
                print("Second statement in defer");
                print("Third statement in defer");
            }
            print("Main execution");
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Defer with multiple statements test failed: {:?}",
        result.err()
    );
}
