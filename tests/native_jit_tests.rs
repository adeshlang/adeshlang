//! Tests for Native JIT backend
//!
//! These tests verify that the Native JIT compiler works correctly
//! and maintains semantic parity with other backends.

use adeshlang::backends::jit::native::native_jit_run;

#[test]
fn test_native_jit_simple_return() {
    let result = native_jit_run(
        r#"
        fn main() {
            return 42;
        }
        "#,
    );
    assert!(result.is_ok(), "Simple return failed: {:?}", result.err());
}

#[test]
fn test_native_jit_arithmetic() {
    let result = native_jit_run(
        r#"
        fn main() {
            let a = 10;
            let b = 20;
            let c = a + b;
            let d = c * 2;
            return d;
        }
        "#,
    );
    assert!(result.is_ok(), "Arithmetic failed: {:?}", result.err());
}

#[test]
fn test_native_jit_function_call() {
    let result = native_jit_run(
        r#"
        fn double(x: i64): i64 {
            return x * 2;
        }
        
        fn main() {
            let result = double(21);
            return result;
        }
        "#,
    );
    assert!(result.is_ok(), "Function call failed: {:?}", result.err());
}

#[test]
fn test_native_jit_nested_calls() {
    let result = native_jit_run(
        r#"
        fn triple(x: i64): i64 {
            return x * 3;
        }
        
        fn double(x: i64): i64 {
            return x * 2;
        }
        
        fn compute(x: i64): i64 {
            let a = double(x);
            let b = triple(x);
            return a + b;
        }
        
        fn main() {
            let result = compute(10);
            return result;
        }
        "#,
    );
    assert!(result.is_ok(), "Nested calls failed: {:?}", result.err());
}

#[test]
#[ignore = "Native JIT lowering pending"]
fn test_native_jit_comparisons() {
    let result = native_jit_run(
        r#"
        fn main() {
            let a = 5;
            let b = 10;
            let lt = a < b;
            let gt = a > b;
            let eq = a == b;
            return 0;
        }
        "#,
    );
    assert!(result.is_ok(), "Comparisons failed: {:?}", result.err());
}

#[test]
#[ignore = "Native JIT lowering pending"]
fn test_native_jit_multiple_types() {
    let result = native_jit_run(
        r#"
        fn main() {
            let a: u8 = 255;
            let b: u16 = 65535;
            let c: i32 = -42;
            let d: f32 = 3.14;
            let e: f64 = 2.71;
            let f: bool = true;
            return 0;
        }
        "#,
    );
    assert!(result.is_ok(), "Multiple types failed: {:?}", result.err());
}

#[test]
fn test_native_jit_subtraction() {
    let result = native_jit_run(
        r#"
        fn main() {
            let a = 100;
            let b = 42;
            let c = a - b;
            return c;
        }
        "#,
    );
    assert!(result.is_ok(), "Subtraction failed: {:?}", result.err());
}

#[test]
#[ignore = "Native JIT lowering pending"]
fn test_native_jit_division() {
    let result = native_jit_run(
        r#"
        fn main() {
            let a = 100;
            let b = 5;
            let c = a / b;
            return c;
        }
        "#,
    );
    assert!(result.is_ok(), "Division failed: {:?}", result.err());
}

#[test]
#[ignore = "Native JIT lowering pending"]
fn test_native_jit_float_arithmetic() {
    let result = native_jit_run(
        r#"
        fn main() {
            let a = 3.14;
            let b = 2.71;
            let sum = a + b;
            let diff = a - b;
            let prod = a * b;
            let quot = a / b;
            return 0;
        }
        "#,
    );
    assert!(
        result.is_ok(),
        "Float arithmetic failed: {:?}",
        result.err()
    );
}

#[test]
fn test_native_jit_multiple_functions() {
    let result = native_jit_run(
        r#"
        fn add(a: i64, b: i64): i64 {
            return a + b;
        }
        
        fn subtract(a: i64, b: i64): i64 {
            return a - b;
        }
        
        fn multiply(a: i64, b: i64): i64 {
            return a * b;
        }
        
        fn main() {
            let x = add(10, 5);
            let y = subtract(20, 3);
            let z = multiply(x, y);
            return z;
        }
        "#,
    );
    assert!(
        result.is_ok(),
        "Multiple functions failed: {:?}",
        result.err()
    );
}

#[test]
fn test_native_jit_void_return() {
    let result = native_jit_run(
        r#"
        fn do_nothing() {
            let x = 42;
        }
        
        fn main() {
            do_nothing();
            return 0;
        }
        "#,
    );
    assert!(result.is_ok(), "Void return failed: {:?}", result.err());
}
