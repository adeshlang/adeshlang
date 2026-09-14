//! FFI Safety Validator
//!
//! Validates FFI boundary safety at compile time.

use crate::runtime::abi::versioning::{CallingConvention, StructLayout, TypeRepr};

/// FFI safety error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfiSafetyError {
    /// ARC type crossed FFI boundary
    ArcCrossedBoundary { function: String, param: String },
    /// Borrow type crossed FFI boundary
    BorrowCrossedBoundary { function: String, param: String },
    /// Non-C-repr struct crossed FFI boundary
    NonCReprStruct { function: String, type_name: String },
    /// Invalid calling convention
    InvalidCallingConvention {
        function: String,
        expected: String,
        found: String,
    },
    /// Ownership transfer not explicit
    ImplicitOwnershipTransfer { function: String, param: String },
}

impl FfiSafetyError {
    /// Returns the error message
    pub fn message(&self) -> String {
        match self {
            FfiSafetyError::ArcCrossedBoundary { function, param } => {
                format!(
                    "Error in '{}': ARC type '{}' cannot cross FFI boundary. \
                     Use raw pointers or manual reference counting at the boundary.",
                    function, param
                )
            }
            FfiSafetyError::BorrowCrossedBoundary { function, param } => {
                format!(
                    "Error in '{}': Borrow type '{}' cannot cross FFI boundary. \
                     Borrows have Adesh-specific semantics that don't map to C.",
                    function, param
                )
            }
            FfiSafetyError::NonCReprStruct {
                function,
                type_name,
            } => {
                format!(
                    "Error in '{}': Struct '{}' must have #[repr(C)] to cross FFI boundary. \
                     Add #[repr(C)] annotation to ensure C-compatible layout.",
                    function, type_name
                )
            }
            FfiSafetyError::InvalidCallingConvention {
                function,
                expected,
                found,
            } => {
                format!(
                    "Error in '{}': Expected calling convention '{}' but found '{}'. \
                     FFI functions must use C calling convention.",
                    function, expected, found
                )
            }
            FfiSafetyError::ImplicitOwnershipTransfer { function, param } => {
                format!(
                    "Error in '{}': Implicit ownership transfer of '{}' across FFI boundary. \
                     Use explicit transfer functions or document ownership semantics.",
                    function, param
                )
            }
        }
    }
}

/// FFI type safety checker
pub struct FfiSafetyChecker {
    errors: Vec<FfiSafetyError>,
}

impl FfiSafetyChecker {
    /// Creates a new FFI safety checker
    pub fn new() -> Self {
        FfiSafetyChecker { errors: Vec::new() }
    }

    /// Checks a function for FFI safety
    pub fn check_function(
        &mut self,
        function_name: &str,
        calling_convention: CallingConvention,
        params: &[(String, TypeInfo)],
    ) {
        // Check calling convention
        if calling_convention != CallingConvention::C {
            self.errors.push(FfiSafetyError::InvalidCallingConvention {
                function: function_name.to_string(),
                expected: "C".to_string(),
                found: calling_convention.name().to_string(),
            });
        }

        // Check each parameter
        for (param_name, type_info) in params {
            self.check_type(function_name, param_name, type_info);
        }
    }

    /// Checks a type for FFI safety
    fn check_type(&mut self, function_name: &str, param_name: &str, type_info: &TypeInfo) {
        match type_info {
            TypeInfo::Arc => {
                self.errors.push(FfiSafetyError::ArcCrossedBoundary {
                    function: function_name.to_string(),
                    param: param_name.to_string(),
                });
            }
            TypeInfo::Borrow | TypeInfo::BorrowMut => {
                self.errors.push(FfiSafetyError::BorrowCrossedBoundary {
                    function: function_name.to_string(),
                    param: param_name.to_string(),
                });
            }
            TypeInfo::Struct { name, repr } => {
                if repr.layout != StructLayout::C {
                    self.errors.push(FfiSafetyError::NonCReprStruct {
                        function: function_name.to_string(),
                        type_name: name.clone(),
                    });
                }
            }
            TypeInfo::Owned => {
                self.errors.push(FfiSafetyError::ImplicitOwnershipTransfer {
                    function: function_name.to_string(),
                    param: param_name.to_string(),
                });
            }
            _ => {} // Other types are safe
        }
    }

    /// Returns all errors found
    pub fn errors(&self) -> &[FfiSafetyError] {
        &self.errors
    }

    /// Returns true if there are no errors
    pub fn is_safe(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Type information for FFI checking
#[derive(Debug, Clone)]
pub enum TypeInfo {
    /// Primitive type (safe)
    Primitive,
    /// Raw pointer (safe but requires documentation)
    Pointer,
    /// ARC (not safe)
    Arc,
    /// Borrow (not safe)
    Borrow,
    /// Mutable borrow (not safe)
    BorrowMut,
    /// Owned value (requires explicit transfer)
    Owned,
    /// Struct with representation info
    Struct { name: String, repr: TypeRepr },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_ffi_function() {
        let mut checker = FfiSafetyChecker::new();
        checker.check_function(
            "safe_func",
            CallingConvention::C,
            &[
                ("x".to_string(), TypeInfo::Primitive),
                ("ptr".to_string(), TypeInfo::Pointer),
            ],
        );
        assert!(checker.is_safe());
    }

    #[test]
    fn test_unsafe_arc_crossing() {
        let mut checker = FfiSafetyChecker::new();
        checker.check_function(
            "unsafe_func",
            CallingConvention::C,
            &[("arc_param".to_string(), TypeInfo::Arc)],
        );
        assert!(!checker.is_safe());
        assert_eq!(checker.errors().len(), 1);
    }

    #[test]
    fn test_non_c_repr_struct() {
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
    }
}
