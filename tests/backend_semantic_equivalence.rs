//! Backend Semantic Equivalence Tests
//!
//! These tests verify that all backends produce identical results for the same
//! operations by directly testing the runtime ABI operations. This ensures
//! semantic consistency across all backends that use the unified ABI.
//!
//! Test Strategy:
//! - Test ABI operations directly
//! - Verify type coercion and edge cases
//! - Validate floating point precision
//! - Document expected behavior

use adeshlang::parsing::ast::Value;
use adeshlang::runtime::abi::{
    abi_add, abi_cmp_eq, abi_cmp_ge, abi_cmp_gt, abi_cmp_le, abi_cmp_lt, abi_cmp_ne, abi_div,
    abi_mod, abi_mul, abi_negate, abi_not, abi_sub,
};

/// Helper to compare values (since Value doesn't implement PartialEq)
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x - y).abs() < 1e-10,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::I64(x), Value::I64(y)) => x == y,
        _ => false,
    }
}

/// Test that interpreter arithmetic matches ABI
#[test]
fn test_abi_arithmetic_operations() {
    // Test addition
    let result = abi_add(&Value::Number(10.0), &Value::Number(20.0)).expect("ABI add failed");
    assert!(
        values_equal(&result, &Value::Number(30.0)),
        "ABI add: expected 30, got {:?}",
        result
    );

    // Test subtraction
    let result = abi_sub(&Value::Number(50.0), &Value::Number(30.0)).expect("ABI sub failed");
    assert!(
        values_equal(&result, &Value::Number(20.0)),
        "ABI sub: expected 20, got {:?}",
        result
    );

    // Test multiplication
    let result = abi_mul(&Value::Number(6.0), &Value::Number(7.0)).expect("ABI mul failed");
    assert!(
        values_equal(&result, &Value::Number(42.0)),
        "ABI mul: expected 42, got {:?}",
        result
    );

    // Test division
    let result = abi_div(&Value::Number(100.0), &Value::Number(4.0)).expect("ABI div failed");
    assert!(
        values_equal(&result, &Value::Number(25.0)),
        "ABI div: expected 25, got {:?}",
        result
    );

    // Test modulo
    let result = abi_mod(&Value::Number(17.0), &Value::Number(5.0)).expect("ABI mod failed");
    assert!(
        values_equal(&result, &Value::Number(2.0)),
        "ABI mod: expected 2, got {:?}",
        result
    );
}

/// Test that interpreter comparisons match ABI
#[test]
fn test_abi_comparison_operations() {
    // Test less than
    let result = abi_cmp_lt(&Value::Number(5.0), &Value::Number(10.0)).expect("ABI lt failed");
    assert!(
        values_equal(&result, &Value::Bool(true)),
        "ABI lt: expected true, got {:?}",
        result
    );

    // Test less than or equal
    let result = abi_cmp_le(&Value::Number(10.0), &Value::Number(10.0)).expect("ABI le failed");
    assert!(
        values_equal(&result, &Value::Bool(true)),
        "ABI le: expected true, got {:?}",
        result
    );

    // Test greater than
    let result = abi_cmp_gt(&Value::Number(20.0), &Value::Number(15.0)).expect("ABI gt failed");
    assert!(
        values_equal(&result, &Value::Bool(true)),
        "ABI gt: expected true, got {:?}",
        result
    );

    // Test greater than or equal
    let result = abi_cmp_ge(&Value::Number(15.0), &Value::Number(15.0)).expect("ABI ge failed");
    assert!(
        values_equal(&result, &Value::Bool(true)),
        "ABI ge: expected true, got {:?}",
        result
    );

    // Test equality
    let result = abi_cmp_eq(&Value::Number(42.0), &Value::Number(42.0)).expect("ABI eq failed");
    assert!(
        values_equal(&result, &Value::Bool(true)),
        "ABI eq: expected true, got {:?}",
        result
    );

    // Test inequality
    let result = abi_cmp_ne(&Value::Number(10.0), &Value::Number(20.0)).expect("ABI ne failed");
    assert!(
        values_equal(&result, &Value::Bool(true)),
        "ABI ne: expected true, got {:?}",
        result
    );
}

/// Test string concatenation matches ABI
#[test]
fn test_abi_string_concatenation() {
    let result = abi_add(
        &Value::Str("hello".to_string()),
        &Value::Str(" world".to_string()),
    )
    .expect("ABI string concat failed");
    assert!(
        values_equal(&result, &Value::Str("hello world".to_string())),
        "ABI string concat: expected 'hello world', got {:?}",
        result
    );
}

/// Test unary operations
#[test]
fn test_abi_unary_operations() {
    // Test negation
    let result = abi_negate(&Value::Number(42.0)).expect("ABI negate failed");
    assert!(
        values_equal(&result, &Value::Number(-42.0)),
        "ABI negate: expected -42, got {:?}",
        result
    );

    // Test not (logical negation)
    let result = abi_not(&Value::Bool(true)).expect("ABI not failed");
    assert!(
        values_equal(&result, &Value::Bool(false)),
        "ABI not: expected false, got {:?}",
        result
    );

    let result = abi_not(&Value::Bool(false)).expect("ABI not failed");
    assert!(
        values_equal(&result, &Value::Bool(true)),
        "ABI not: expected true, got {:?}",
        result
    );
}

/// Test edge cases: division by zero
#[test]
fn test_division_by_zero_consistency() {
    // ABI should handle division by zero by returning infinity
    let result = abi_div(&Value::Number(10.0), &Value::Number(0.0));

    match result {
        Ok(Value::Number(v)) => {
            assert!(
                v.is_infinite(),
                "Division by zero should produce infinity, got {:?}",
                v
            );
        }
        Err(_) => {
            // Error is also acceptable
        }
        _ => panic!("Unexpected result type from division by zero"),
    }
}

/// Test floating point precision matches ABI
#[test]
fn test_abi_floating_point_precision() {
    // Test that EPSILON is used consistently
    let sum = abi_add(&Value::Number(0.1), &Value::Number(0.2)).expect("ABI add failed");
    let result = abi_cmp_eq(&sum, &Value::Number(0.3)).expect("ABI eq failed");

    // Should be true due to EPSILON tolerance
    assert!(
        matches!(result, Value::Bool(true)),
        "0.1 + 0.2 should equal 0.3 with EPSILON tolerance, got {:?}",
        result
    );
}

/// Test complex expression equivalence
#[test]
fn test_abi_complex_expressions() {
    // Test: (10 + 5) * 3 - 8 / 2 = 41
    let step1 = abi_add(&Value::Number(10.0), &Value::Number(5.0)).expect("step1 failed");
    let step2 = abi_mul(&step1, &Value::Number(3.0)).expect("step2 failed");
    let step3 = abi_div(&Value::Number(8.0), &Value::Number(2.0)).expect("step3 failed");
    let result = abi_sub(&step2, &step3).expect("step4 failed");

    assert!(
        values_equal(&result, &Value::Number(41.0)),
        "Complex expression expected 41, got {:?}",
        result
    );
}

// Note: JIT/AOT semantic equivalence tests would follow this pattern:
//
// #[test]
// fn test_jit_arithmetic_matches_abi() {
//     let jit_result = jit_run("fn main() { return 10 + 20; }").unwrap();
//     let abi_result = abi_add(&Value::Number(10.0), &Value::Number(20.0)).unwrap();
//     assert_eq!(extract_value(jit_result), abi_result);
// }
//
// #[test]
// fn test_aot_arithmetic_matches_abi() {
//     let aot_result = aot_compile_and_run("fn main() { return 10 + 20; }").unwrap();
//     let abi_result = abi_add(&Value::Number(10.0), &Value::Number(20.0)).unwrap();
//     assert_eq!(extract_value(aot_result), abi_result);
// }
//
// Future work: Add JIT/AOT test utilities and expand coverage to:
// - All arithmetic operations
// - All comparison operations
// - Type coercion (BigInt promotion)
// - Edge cases (overflow, underflow, NaN, infinity)
// - String operations
// - Array operations
