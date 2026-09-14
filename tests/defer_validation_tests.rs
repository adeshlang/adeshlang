//! Tests for defer compile-time validation
//!
//! Tests that defer blocks reject invalid constructs like await

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
fn test_defer_rejects_await() {
    let result = run_defer_code(
        r#"
        async fn test_async() {
            defer {
                await some_async_fn();  // Should be rejected
            }
            print("This should not run");
        }
        
        fn main() {
            test_async();
        }
    "#,
    );

    // Should fail with error about await in defer
    assert!(result.is_err(), "Expected error for await in defer");
    let error_msg = result.unwrap_err();
    assert!(
        error_msg.contains("await") || error_msg.contains("defer"),
        "Error message should mention await or defer: {}",
        error_msg
    );
}

#[test]
fn test_defer_allows_sync_calls() {
    let result = run_defer_code(
        r#"
        fn cleanup() {
            print("Cleanup called");
        }
        
        fn main() {
            defer {
                cleanup();  // Regular function calls are fine
            }
            print("Main execution");
        }
    "#,
    );

    // Should succeed
    assert!(
        result.is_ok(),
        "Defer with sync calls should work: {:?}",
        result.err()
    );
}

#[test]
fn test_nested_defer_no_await() {
    let result = run_defer_code(
        r#"
        fn main() {
            defer {
                {
                    // Nested blocks without await are fine
                    print("Nested defer");
                }
            }
            print("Main");
        }
    "#,
    );

    assert!(
        result.is_ok(),
        "Nested defer without await should work: {:?}",
        result.err()
    );
}
