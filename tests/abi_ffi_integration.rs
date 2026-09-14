//! Integration tests for ABI versioning and FFI safety
//!
//! Tests version compatibility, calling conventions, and FFI boundary validation

use adeshlang::backends::common::ffi::safety::*;
use adeshlang::runtime::abi::versioning::*;

// ===== ABI Version Tests =====

#[test]
fn test_abi_version_current() {
    let current = AbiVersion::CURRENT;
    assert_eq!(current.major, 1);
    assert_eq!(current.minor, 0);
    assert_eq!(current.patch, 0);
}

#[test]
fn test_abi_version_new() {
    let v = AbiVersion::new(2, 3, 4);
    assert_eq!(v.major, 2);
    assert_eq!(v.minor, 3);
    assert_eq!(v.patch, 4);
}

#[test]
fn test_abi_version_to_string() {
    let v = AbiVersion::new(1, 2, 3);
    assert_eq!(v.to_string(), "1.2.3");
}

#[test]
fn test_abi_version_forward_compatible() {
    let v1_0_0 = AbiVersion::new(1, 0, 0);
    let v1_1_0 = AbiVersion::new(1, 1, 0);
    let v1_2_5 = AbiVersion::new(1, 2, 5);

    // Higher minor versions are compatible with lower
    assert!(v1_1_0.is_compatible_with(&v1_0_0));
    assert!(v1_2_5.is_compatible_with(&v1_0_0));
    assert!(v1_2_5.is_compatible_with(&v1_1_0));
}

#[test]
fn test_abi_version_not_backward_compatible() {
    let v1_0_0 = AbiVersion::new(1, 0, 0);
    let v1_1_0 = AbiVersion::new(1, 1, 0);

    // Lower minor versions are NOT compatible with higher
    assert!(!v1_0_0.is_compatible_with(&v1_1_0));
}

#[test]
fn test_abi_version_major_incompatible() {
    let v1_0_0 = AbiVersion::new(1, 0, 0);
    let v2_0_0 = AbiVersion::new(2, 0, 0);
    let v1_5_0 = AbiVersion::new(1, 5, 0);

    // Different major versions are incompatible
    assert!(!v2_0_0.is_compatible_with(&v1_0_0));
    assert!(!v1_0_0.is_compatible_with(&v2_0_0));
    assert!(!v2_0_0.is_compatible_with(&v1_5_0));
}

#[test]
fn test_abi_version_breaking_change() {
    let v1_0_0 = AbiVersion::new(1, 0, 0);
    let v2_0_0 = AbiVersion::new(2, 0, 0);
    let v1_1_0 = AbiVersion::new(1, 1, 0);

    assert!(v2_0_0.is_breaking_change_from(&v1_0_0));
    assert!(!v1_1_0.is_breaking_change_from(&v1_0_0));
}

#[test]
fn test_abi_version_ordering() {
    let v1_0_0 = AbiVersion::new(1, 0, 0);
    let v1_1_0 = AbiVersion::new(1, 1, 0);
    let v2_0_0 = AbiVersion::new(2, 0, 0);

    assert!(v1_0_0 < v1_1_0);
    assert!(v1_1_0 < v2_0_0);
    assert!(v1_0_0 < v2_0_0);
}

// ===== Calling Convention Tests =====

#[test]
fn test_calling_convention_names() {
    assert_eq!(CallingConvention::C.name(), "C");
    assert_eq!(CallingConvention::Adesh.name(), "adesh");
    assert_eq!(CallingConvention::System.name(), "system");
    assert_eq!(CallingConvention::Fast.name(), "fast");
}

#[test]
fn test_calling_convention_equality() {
    assert_eq!(CallingConvention::C, CallingConvention::C);
    assert_ne!(CallingConvention::C, CallingConvention::Adesh);
}

// ===== ABI Attribute Tests =====

#[test]
fn test_abi_attribute_default() {
    let attr = AbiAttribute::default();
    assert_eq!(attr.calling_convention, CallingConvention::Adesh);
    assert!(!attr.is_ffi_boundary);
}

#[test]
fn test_abi_attribute_extern_c() {
    let attr = AbiAttribute::extern_c();
    assert_eq!(attr.calling_convention, CallingConvention::C);
    assert!(attr.is_ffi_boundary);
}

// ===== Struct Layout Tests =====

#[test]
fn test_struct_layout_equality() {
    assert_eq!(StructLayout::C, StructLayout::C);
    assert_ne!(StructLayout::C, StructLayout::Adesh);
    assert_ne!(StructLayout::Adesh, StructLayout::Packed);
}

// ===== Type Repr Tests =====

#[test]
fn test_type_repr_default() {
    let repr = TypeRepr::default();
    assert_eq!(repr.layout, StructLayout::Adesh);
    assert_eq!(repr.align, 0);
    assert!(!repr.transparent);
}

#[test]
fn test_type_repr_c() {
    let repr = TypeRepr::c_repr();
    assert_eq!(repr.layout, StructLayout::C);
    assert_eq!(repr.align, 0);
    assert!(!repr.transparent);
}

#[test]
fn test_type_repr_packed() {
    let repr = TypeRepr::packed();
    assert_eq!(repr.layout, StructLayout::Packed);
    assert_eq!(repr.align, 1);
    assert!(!repr.transparent);
}

// ===== FFI Safety Checker Tests =====

#[test]
fn test_ffi_safety_checker_new() {
    let checker = FfiSafetyChecker::new();
    assert!(checker.is_safe());
    assert_eq!(checker.errors().len(), 0);
}

#[test]
fn test_ffi_safety_safe_function() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "safe_func",
        CallingConvention::C,
        &[
            ("x".to_string(), TypeInfo::Primitive),
            ("y".to_string(), TypeInfo::Primitive),
            ("ptr".to_string(), TypeInfo::Pointer),
        ],
    );

    assert!(checker.is_safe());
}

#[test]
fn test_ffi_safety_arc_boundary_error() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "unsafe_arc",
        CallingConvention::C,
        &[("arc_param".to_string(), TypeInfo::Arc)],
    );

    assert!(!checker.is_safe());
    assert_eq!(checker.errors().len(), 1);

    match &checker.errors()[0] {
        FfiSafetyError::ArcCrossedBoundary { function, param } => {
            assert_eq!(function, "unsafe_arc");
            assert_eq!(param, "arc_param");
        }
        _ => panic!("Expected ArcCrossedBoundary error"),
    }
}

#[test]
fn test_ffi_safety_borrow_boundary_error() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "unsafe_borrow",
        CallingConvention::C,
        &[("borrow_param".to_string(), TypeInfo::Borrow)],
    );

    assert!(!checker.is_safe());

    match &checker.errors()[0] {
        FfiSafetyError::BorrowCrossedBoundary { .. } => {}
        _ => panic!("Expected BorrowCrossedBoundary error"),
    }
}

#[test]
fn test_ffi_safety_borrow_mut_boundary_error() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "unsafe_borrow_mut",
        CallingConvention::C,
        &[("borrow_mut_param".to_string(), TypeInfo::BorrowMut)],
    );

    assert!(!checker.is_safe());
}

#[test]
fn test_ffi_safety_non_c_repr_struct_error() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "bad_struct_func",
        CallingConvention::C,
        &[(
            "s".to_string(),
            TypeInfo::Struct {
                name: "MyStruct".to_string(),
                repr: TypeRepr::default(), // Not C repr
            },
        )],
    );

    assert!(!checker.is_safe());

    match &checker.errors()[0] {
        FfiSafetyError::NonCReprStruct {
            function,
            type_name,
        } => {
            assert_eq!(function, "bad_struct_func");
            assert_eq!(type_name, "MyStruct");
        }
        _ => panic!("Expected NonCReprStruct error"),
    }
}

#[test]
fn test_ffi_safety_c_repr_struct_ok() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "good_struct_func",
        CallingConvention::C,
        &[(
            "s".to_string(),
            TypeInfo::Struct {
                name: "MyStruct".to_string(),
                repr: TypeRepr::c_repr(), // C repr
            },
        )],
    );

    assert!(checker.is_safe());
}

#[test]
fn test_ffi_safety_invalid_calling_convention() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "wrong_cc",
        CallingConvention::Adesh, // Should be C for FFI
        &[],
    );

    assert!(!checker.is_safe());

    match &checker.errors()[0] {
        FfiSafetyError::InvalidCallingConvention {
            function,
            expected,
            found,
        } => {
            assert_eq!(function, "wrong_cc");
            assert_eq!(expected, "C");
            assert_eq!(found, "adesh");
        }
        _ => panic!("Expected InvalidCallingConvention error"),
    }
}

#[test]
fn test_ffi_safety_implicit_ownership_transfer() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "ownership_func",
        CallingConvention::C,
        &[("owned".to_string(), TypeInfo::Owned)],
    );

    assert!(!checker.is_safe());

    match &checker.errors()[0] {
        FfiSafetyError::ImplicitOwnershipTransfer { .. } => {}
        _ => panic!("Expected ImplicitOwnershipTransfer error"),
    }
}

#[test]
fn test_ffi_safety_multiple_errors() {
    let mut checker = FfiSafetyChecker::new();

    checker.check_function(
        "multiple_errors",
        CallingConvention::Adesh, // Wrong calling convention
        &[
            ("arc".to_string(), TypeInfo::Arc),       // ARC boundary
            ("borrow".to_string(), TypeInfo::Borrow), // Borrow boundary
        ],
    );

    assert!(!checker.is_safe());
    assert_eq!(checker.errors().len(), 3); // CC + ARC + Borrow
}

#[test]
fn test_ffi_safety_error_messages() {
    let arc_error = FfiSafetyError::ArcCrossedBoundary {
        function: "test".to_string(),
        param: "arc_param".to_string(),
    };

    let msg = arc_error.message();
    assert!(msg.contains("ARC"));
    assert!(msg.contains("test"));
    assert!(msg.contains("arc_param"));
}

// ===== Rust Interop Tests =====

#[test]
fn test_rust_abi_repr_compatible() {
    use adeshlang::backends::common::ffi::rust_interop::*;

    let c_repr = TypeRepr::c_repr();
    assert!(RustAbi::is_repr_compatible(&c_repr));

    let adesh_repr = TypeRepr::default();
    assert!(!RustAbi::is_repr_compatible(&adesh_repr));
}

#[test]
fn test_rust_ffi_function_basic() {
    use adeshlang::backends::common::ffi::rust_interop::*;

    let func = RustFfiFunction::new("test_func".to_string());
    assert_eq!(func.name, "test_func");
    assert!(!func.is_unsafe);
    assert!(func.has_repr_c);
}

#[test]
fn test_rust_ffi_function_unsafe() {
    use adeshlang::backends::common::ffi::rust_interop::*;

    let func = RustFfiFunction::new("unsafe_func".to_string()).mark_unsafe();
    assert!(func.is_unsafe);
}

#[test]
fn test_ffi_drop_handler_ownership() {
    use adeshlang::backends::common::ffi::rust_interop::*;

    let _handler_own = FfiDropHandler::new(true);
    let _handler_no_own = FfiDropHandler::new(false);
    // Just test that they can be created
}
