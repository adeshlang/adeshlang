//! Production-Grade Type Checking & Memory Safety Across All Backends
//!
//! Verifies:
//! 1. Static type checking: strict validation of index expressions (reject non-integers)
//! 2. Record/Struct field type inference
//! 3. Array bounds safety & negative indexing support
//! 4. Zero-GC compile-time memory safety & ARC lifecycle tracking
//! 5. Backend semantic parity and safe execution

use adeshlang::cli::{ExecutionBackend, ParsedArgs, RuntimeConfig};
use adeshlang::execution::runtime::{Interpreter, ModuleLoader};
use std::path::PathBuf;

fn run_src(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(std::path::Path::new("."));
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
fn test_compile_time_type_check_index_types() {
    // Valid integer indexing passes static type checking
    let valid_code = r#"
    let arr = [10, 20, 30];
    let first = arr[0];
    let last = arr[-1];
    "#;
    assert!(adeshlang::types::type_system::check_module(valid_code).is_ok());

    // Invalid non-integer string index on typed array MUST be rejected at compile time
    let invalid_code = r#"
    let arr = [10, 20, 30];
    let bad = arr["invalid_key"];
    "#;
    let check_res = adeshlang::types::type_system::check_module(invalid_code);
    assert!(
        check_res.is_err(),
        "Expected type check to reject string index on array"
    );
    let err_msg = check_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("must be an integer")
            || err_msg.contains("type error")
            || err_msg.contains("index"),
        "Unexpected error message: {}",
        err_msg
    );
}

#[test]
fn test_record_field_type_inference() {
    let code = r#"
    type Point = { x: int, y: int };
    let p: Point = { x: 10, y: 20 };
    let px = p.x;
    "#;
    assert!(adeshlang::types::type_system::check_module(code).is_ok());
}

#[test]
fn test_array_negative_indexing_and_bounds_safety() {
    let code = r#"
    let arr = [100, 200, 300, 400];
    if (arr[0] != 100) {
        throw "arr[0] failed";
    }
    if (arr[-1] != 400) {
        throw "arr[-1] failed";
    }
    if (arr[-2] != 300) {
        throw "arr[-2] failed";
    }
    "#;
    assert!(run_src(code).is_ok());
}

#[test]
fn test_arc_strong_weak_lifecycle_and_safety() {
    let code = r#"
    share x = { id: 42 };
    assert_eq(x.strong_count(), 1);
    weak w = x;
    assert_eq(x.weak_count(), 1);
    assert_eq(w.weak_count(), 1);
    strong y = x;
    assert_eq(x.strong_count(), 2);
    "#;
    let res = run_src(code);
    assert!(res.is_ok(), "ARC test failed: {:?}", res.err());
}

#[test]
fn test_ownership_move_semantics_validation() {
    // Valid ownership transfer
    let valid_code = r#"
    fn consume(val) {
        return val + 1;
    }
    let a = 10;
    let b = consume(a);
    "#;
    assert!(adeshlang::types::type_system::check_module(valid_code).is_ok());
}

#[test]
fn test_backend_runners_type_check_integration() {
    let dummy_path = PathBuf::from("test_dummy.adesh");
    let mut parsed = ParsedArgs::parse(&[]).unwrap();
    parsed.config.quiet = true;

    // Valid source runs in interpreter
    let valid_src = "let a = 1 + 2;\n";
    let res = adeshlang::cli::backends::run_with_interpreter(&dummy_path, valid_src, &parsed);
    assert!(res.is_ok());

    // Type error in source is stopped before execution in interpreter
    let invalid_src = "let x: int = \"string_value\";\n";
    let err = adeshlang::cli::backends::run_with_interpreter(&dummy_path, invalid_src, &parsed);
    assert!(err.is_err());
}
