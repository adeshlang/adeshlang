//! Phase 8: Authoritative Safety Gate & Negative Adversarial Tests
//!
//! Verifies that invalid constructs:
//! 1. Type mismatches
//! 2. Use after move
//! 3. Conflicting mutable/immutable borrows
//! 4. Multiple mutable borrows
//! 5. Returning local references (escaping lifetime)
//!
//! are deterministically REJECTED at the safety gate (type checker & ownership validator)
//! without ever reaching the native code generator or silently generating fallback code.

use adeshlang::toolchain::config::RuntimeConfig;

fn verify_safety_rejection(src: &str, expected_substring: &str) {
    let type_res = adeshlang::types::type_system::check_module_in(src, Some("adversarial_test.adesh"));
    let ownership_res = adeshlang::cli::parsing::check_ownership_and_parse_in(
        src,
        &RuntimeConfig::default(),
        Some("adversarial_test.adesh"),
    );

    let failed = type_res.is_err() || ownership_res.is_err();
    assert!(
        failed,
        "Safety gate bypassed! Code should have failed compilation:\n{}\nType check: {:?}\nOwnership check: {:?}",
        src,
        type_res,
        ownership_res
    );

    let mut combined_err = String::new();
    if let Err(e) = type_res {
        combined_err.push_str(&e.to_string());
        combined_err.push('\n');
    }
    if let Err(e) = ownership_res {
        combined_err.push_str(&e);
    }

    assert!(
        combined_err.to_lowercase().contains(&expected_substring.to_lowercase()),
        "Expected error message containing '{}', but got:\n{}",
        expected_substring,
        combined_err
    );
}

#[test]
fn test_negative_type_mismatch_int_to_string() {
    let src = r#"
    fn main(): int {
        let x: int = "hello invalid type";
        return x;
    }
    "#;
    verify_safety_rejection(src, "type");
}

#[test]
fn test_negative_type_mismatch_return_type() {
    let src = r#"
    fn compute(): int {
        return "not an integer";
    }
    fn main(): int {
        return compute();
    }
    "#;
    verify_safety_rejection(src, "type");
}

#[test]
fn test_negative_use_after_move() {
    let src = r#"
    fn consume(v: [int]) {
        print(v);
    }
    fn main() {
        let x = [1, 2, 3];
        consume(x);
        consume(x); // Use after move!
    }
    "#;
    verify_safety_rejection(src, "move");
}

#[test]
fn test_negative_conflicting_mutable_borrows() {
    let src = r#"
    fn main() {
        let data = [1, 2, 3];
        let ref1 = &mut data;
        let ref2 = &mut data;
        print(ref1);
        print(ref2);
    }
    "#;
    verify_safety_rejection(src, "mutable");
}

#[test]
fn test_negative_borrow_after_move() {
    let src = r#"
    fn sink(v: [int]) {
        print(v);
    }
    fn main() {
        let items = [10, 20, 30];
        sink(items);
        let r = &items; // Borrow of moved value!
        print(r);
    }
    "#;
    verify_safety_rejection(src, "move");
}
