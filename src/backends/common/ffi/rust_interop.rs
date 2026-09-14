//! Rust FFI Interop
//!
//! Provides compatibility layer for Rust FFI.

use crate::runtime::abi::versioning::TypeRepr;

/// Rust ABI mapping
pub struct RustAbi;

impl RustAbi {
    /// Maps Adesh ARC to Rust Arc
    ///
    /// This is safe because:
    /// - Both use atomic reference counting
    /// - Both have the same drop semantics
    /// - Layout is compatible with repr(C)
    ///
    /// # Safety
    /// - adesh_arc must point to a valid ARC-managed object
    /// - The ARC runtime must be initialized
    /// - The type T must match the actual object type
    pub unsafe fn arc_to_rust_arc<T>(adesh_arc: crate::memory::arc::ArcId) -> std::sync::Arc<T> {
        let ptr = adesh_arc as *const T;

        // Call arc_retain to ensure the object stays alive
        // while we create the Rust Arc
        let data_ptr = ptr as *mut u8;
        unsafe {
            crate::runtime::arc::arc_retain(data_ptr);

            // Create Arc from raw pointer
            // The Arc will manage its own reference count
            std::sync::Arc::from_raw(ptr)
        }
    }

    /// Maps Rust Arc to Adesh ARC
    ///
    /// # Safety
    /// - The Arc must contain a valid object
    /// - The object must be compatible with AdeshLang's ARC runtime
    pub unsafe fn rust_arc_to_arc<T>(rust_arc: std::sync::Arc<T>) -> crate::memory::arc::ArcId {
        // Convert Arc to raw pointer
        let ptr = std::sync::Arc::into_raw(rust_arc);

        // The Arc's reference count is transferred to AdeshLang's ARC
        // We need to call arc_retain since we're creating a new AdeshLang reference
        let data_ptr = ptr as *mut u8;
        unsafe {
            crate::runtime::arc::arc_retain(data_ptr);
        }

        ptr as u64
    }

    /// Checks if a type has compatible representation
    pub fn is_repr_compatible(repr: &TypeRepr) -> bool {
        // C repr is always compatible
        matches!(
            repr.layout,
            crate::runtime::abi::versioning::StructLayout::C
        )
    }
}

/// Rust FFI function wrapper
pub struct RustFfiFunction {
    pub name: String,
    pub is_unsafe: bool,
    pub has_repr_c: bool,
}

impl RustFfiFunction {
    /// Creates a new Rust FFI function
    pub fn new(name: String) -> Self {
        RustFfiFunction {
            name,
            is_unsafe: false,
            has_repr_c: true,
        }
    }

    /// Marks the function as unsafe
    pub fn mark_unsafe(mut self) -> Self {
        self.is_unsafe = true;
        self
    }

    /// Generates Rust FFI declaration
    pub fn generate_rust_decl(&self) -> String {
        let unsafe_kw = if self.is_unsafe { "unsafe " } else { "" };
        let repr = if self.has_repr_c { "#[repr(C)]\n" } else { "" };

        format!(
            "{}{}extern \"C\" fn {}(...) -> ...",
            repr, unsafe_kw, self.name
        )
    }
}

/// Drop handler for FFI boundary
///
/// Ensures proper cleanup when values cross FFI boundaries
pub struct FfiDropHandler {
    /// Whether this handler owns the value
    owns_value: bool,
}

impl FfiDropHandler {
    /// Creates a new drop handler
    pub fn new(owns_value: bool) -> Self {
        FfiDropHandler { owns_value }
    }

    /// Returns whether this handler owns the value
    pub fn owns_value(&self) -> bool {
        self.owns_value
    }

    /// Transfers ownership to Rust
    pub fn transfer_to_rust<T>(value: T) -> *mut T {
        Box::into_raw(Box::new(value))
    }

    /// Takes ownership from Rust
    ///
    /// # Safety
    /// - ptr must be valid
    /// - ptr must have been created by transfer_to_rust
    /// - ptr must not be used after this call
    pub unsafe fn take_from_rust<T>(ptr: *mut T) -> T {
        unsafe { *Box::from_raw(ptr) }
    }

    /// Borrows from Rust without taking ownership
    ///
    /// # Safety
    /// - ptr must be valid for the duration of the borrow
    pub unsafe fn borrow_from_rust<'a, T>(ptr: *const T) -> &'a T {
        unsafe { &*ptr }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_ffi_function() {
        let func = RustFfiFunction::new("test_func".to_string());
        assert!(func.has_repr_c);
        assert!(!func.is_unsafe);
    }

    #[test]
    fn test_unsafe_rust_function() {
        let func = RustFfiFunction::new("unsafe_func".to_string()).mark_unsafe();
        assert!(func.is_unsafe);
    }

    #[test]
    fn test_ffi_drop_handler() {
        let handler = FfiDropHandler::new(true);
        assert!(handler.owns_value);
    }
}
