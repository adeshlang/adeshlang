//! Optimized Runtime Value
//!
//! Compact `Value` enum optimized for interpreter/VM performance:
//! - Inline small variants (`Number`, `Bool`, `Char`, `Null`)
//! - Box or `Arc` large variants (strings, arrays, maps, objects)
//! - Interned strings via `utils::interner` for fast equality
//! - Shared ownership of complex types via `Arc`
// Optimized Value representation for India language runtime
// Key improvements:
// 1. Box large variants to reduce enum size from ~80 bytes to ~16 bytes
// 2. Use Arc<str> for interned strings (share memory, fast equality)
// 3. Use Arc for shared reference-counted types
// 4. Inline small types (Number, Bool, Char, Null) for zero-cost

use crate::parsing::error::LangError;
use crate::utils::collections::StringMap;
use num_bigint::BigInt;
use std::rc::Rc;
use std::sync::Arc;

/// Optimized runtime value representation
///
/// Design goals:
/// - Small enum size (16 bytes) for better cache utilization
/// - Cheap cloning for common types (Number, Bool, Null)
/// - Shared ownership via Arc for complex types
/// - Interned strings for memory efficiency
#[derive(Clone)]
pub enum Value {
    // ============================================
    // INLINE VARIANTS (no heap allocation)
    // Total size: 8-16 bytes per variant
    // ============================================
    /// IEEE 754 double-precision number (8 bytes)
    Number(f64),

    /// Boolean value (1 byte + padding)
    Bool(bool),

    /// Unicode character (4 bytes)
    Char(char),

    /// Null/undefined value (0 bytes)
    Null,

    // ============================================
    // BOXED VARIANTS (8-byte pointer)
    // Reduces enum size, allocates on heap
    // ============================================
    /// Arbitrary precision integer
    BigInt(Arc<BigInt>),

    /// Interned string (shared via Arc, fast equality)
    Str(Arc<str>),

    /// Array of values
    Array(Box<Vec<Value>>),

    /// Tuple of values (fixed-size, heterogeneous)
    Tuple(Box<Vec<Value>>),

    /// Hash map object (property access)
    Object(Arc<StringMap<Value>>),

    /// Set of unique values
    Set(Box<Vec<Value>>),

    /// Complex number (real, imaginary)
    Complex(f64, f64),

    // ============================================
    // FUNCTION TYPES
    // ============================================
    /// Native Rust function
    Function(NativeFn),

    /// User-defined function (optimized with Arc)
    UserFunction(Arc<UserFnInner>),

    /// Bound method on instance
    BoundMethod(Arc<UserFnInner>, Arc<UserInstance>),

    /// Native bound method (array.push, etc.)
    BoundNative(Arc<str>, Box<Value>),

    // ============================================
    // CLASS/OOP TYPES
    // ============================================
    /// Class definition (shared)
    Class(Arc<UserClass>),

    /// Class instance (shared mutable fields)
    Instance(Arc<UserInstance>),

    /// Super reference for parent method access
    Super(Arc<UserClass>, Arc<UserInstance>),

    // ============================================
    // TYPE SYSTEM TYPES
    // ============================================
    /// Struct definition
    Struct(Arc<UserStruct>),

    /// Enum definition
    Enum(Arc<UserEnum>),

    /// Enum variant constructor
    EnumCtor(Arc<UserEnum>, Arc<str>),

    /// Interface definition
    Interface(Arc<UserInterface>),

    // ============================================
    // ASYNC TYPES
    // ============================================
    /// Promise (async computation)
    Promise(u64),

    /// Shared borrow of another value (lifetime/tracking via OwnershipTracker)
    Ref(Box<Value>, BorrowHandle),

    // ============================================
    // ERROR TYPE
    // ============================================
    /// Runtime error value
    Error(Box<LangError>),

    // ============================================
    // ARC OWNERSHIP SYSTEM TYPES
    // ============================================
    /// Strong reference counting pointer
    Share(StrongRef),

    /// Weak reference counting pointer
    Weak(WeakRef),
}

/// Borrow handle that maintains borrow counts on clone/drop.
#[derive(Debug)]
pub struct BorrowHandle {
    tracker: Rc<crate::utils::memory::OwnershipTracker>,
}

impl BorrowHandle {
    pub fn new_shared(tracker: Rc<crate::utils::memory::OwnershipTracker>) -> Result<Self, String> {
        if tracker.try_borrow() {
            Ok(Self { tracker })
        } else {
            Err("borrow conflict (immutable borrow)".to_string())
        }
    }
}

impl Clone for BorrowHandle {
    fn clone(&self) -> Self {
        // Shared borrow clones increment the immutable count.
        if !self.tracker.try_borrow() {
            panic!("borrow clone failed: borrow rules violated");
        }
        Self {
            tracker: self.tracker.clone(),
        }
    }
}

impl Drop for BorrowHandle {
    fn drop(&mut self) {
        self.tracker.release_borrow();
    }
}

// Placeholder types (to be imported from ast.rs)
#[derive(Clone)]
pub struct NativeFn(
    pub  Arc<
        dyn Fn(&mut dyn crate::parsing::ast::BuiltinEnv, Vec<Value>) -> Result<Value, String>
            + Send
            + Sync,
    >,
);

#[derive(Clone)]
pub struct UserFnInner {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<(String, Option<crate::parsing::ast::Expr>, Option<String>)>,
    pub body: Arc<Vec<crate::parsing::ast::Stmt>>,
    pub closure: usize,
    pub captured: Option<Arc<StringMap<Value>>>,
    pub visibility: Option<crate::parsing::ast::Visibility>,
    pub ret_type: Option<String>,
    pub is_async: bool,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_constructor: bool,
    pub is_getter: bool,
    pub is_setter: bool,
    pub is_operator: bool,
    pub operator_symbol: Option<String>,
}

#[derive(Clone)]
pub struct UserClass {
    pub name: String,
    pub methods: StringMap<Vec<Arc<UserFnInner>>>,
    pub static_methods: StringMap<Vec<Arc<UserFnInner>>>,
    pub static_properties: StringMap<Value>,
    pub operators: StringMap<Arc<UserFnInner>>,
    pub getters: StringMap<Arc<UserFnInner>>,
    pub setters: StringMap<Arc<UserFnInner>>,
    pub parent: Option<Arc<UserClass>>,
    pub implements: Vec<String>,
    pub is_abstract: bool,
}

use std::sync::RwLock;

#[derive(Clone)]
pub struct UserInstance {
    pub class_name: Arc<str>,
    pub fields: Arc<RwLock<StringMap<Value>>>,
    pub class: Arc<UserClass>,
    pub prop_cache: Arc<RwLock<StringMap<Value>>>,
}

// ============================================
// ARC OWNERSHIP RUNTIME STRUCTURES
// ============================================
use std::sync::atomic::{AtomicU64, Ordering};

pub struct SharedObject {
    pub strong_count: AtomicU64,
    pub weak_count: AtomicU64,
    pub value: Value,
}

#[derive(Debug)]
pub struct StrongRef {
    pub ptr: *mut SharedObject,
}

impl Clone for StrongRef {
    fn clone(&self) -> Self {
        unsafe {
            let old = (*self.ptr).strong_count.fetch_add(1, Ordering::Relaxed);
            if old >= (u64::MAX / 2) {
                panic!("StrongRef reference count overflow");
            }
        }
        Self { ptr: self.ptr }
    }
}

impl Drop for StrongRef {
    fn drop(&mut self) {
        unsafe {
            let old_strong = (*self.ptr).strong_count.fetch_sub(1, Ordering::SeqCst);
            if old_strong == 1 {
                // Destroy contained value
                let _old_val = std::mem::replace(&mut (*self.ptr).value, Value::Null);

                // Release memory if weak count is also zero
                if (*self.ptr).weak_count.load(Ordering::SeqCst) == 0 {
                    let _ = Box::from_raw(self.ptr);
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct WeakRef {
    pub ptr: *mut SharedObject,
}

impl Clone for WeakRef {
    fn clone(&self) -> Self {
        unsafe {
            let old = (*self.ptr).weak_count.fetch_add(1, Ordering::Relaxed);
            if old >= (u64::MAX / 2) {
                panic!("WeakRef reference count overflow");
            }
        }
        Self { ptr: self.ptr }
    }
}

impl Drop for WeakRef {
    fn drop(&mut self) {
        unsafe {
            let old_weak = (*self.ptr).weak_count.fetch_sub(1, Ordering::SeqCst);
            if old_weak == 1 {
                // Release memory if strong count is also zero
                if (*self.ptr).strong_count.load(Ordering::SeqCst) == 0 {
                    let _ = Box::from_raw(self.ptr);
                }
            }
        }
    }
}

unsafe impl Send for StrongRef {}
unsafe impl Sync for StrongRef {}
unsafe impl Send for WeakRef {}
unsafe impl Sync for WeakRef {}

#[derive(Clone)]
pub struct UserStruct {
    pub name: String,
    pub fields: Vec<String>,
}

#[derive(Clone)]
pub struct UserEnum {
    pub name: String,
    pub variants: Vec<(String, Option<String>)>,
}

#[derive(Clone)]
pub struct UserInterface {
    pub name: String,
    pub methods: Vec<crate::parsing::ast::Function>,
}

// ============================================
// VALUE IMPLEMENTATION
// ============================================

impl Value {
    /// Check if value is truthy (for conditionals)
    #[inline]
    pub fn truthy(&self) -> bool {
        match self {
            Value::Ref(inner, _) => inner.truthy(),
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Number(n) => *n != 0.0,
            Value::BigInt(bi) => **bi != BigInt::from(0),
            Value::Char(_) => true,
            Value::Str(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Tuple(t) => !t.is_empty(),
            Value::Set(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// Get type name as string
    #[inline]
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Ref(inner, _) => inner.type_name(),
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::BigInt(_) => "bigint",
            Value::Char(_) => "char",
            Value::Str(_) => "string",
            Value::Array(_) => "array",
            Value::Tuple(_) => "tuple",
            Value::Set(_) => "set",
            Value::Object(_) => "object",
            Value::Class(_) => "class",
            Value::Instance(_) => "instance",
            Value::BoundMethod(_, _) => "function",
            Value::UserFunction(_) | Value::Function(_) => "function",
            Value::Enum(_) => "enum",
            Value::EnumCtor(_, _) => "enumctor",
            Value::Promise(_) => "promise",
            Value::Super(_, _) => "super",
            Value::Complex(_, _) => "complex",
            Value::Struct(_) => "struct",
            Value::Interface(_) => "interface",
            Value::BoundNative(_, _) => "function",
            Value::Error(_) => "error",
            Value::Share(_) => "strong",
            Value::Weak(_) => "weak",
        }
    }

    /// Create interned string value
    #[inline]
    pub fn from_string(s: String) -> Self {
        Value::Str(crate::utils::interner::intern_string(s))
    }

    /// Create interned string value from &str
    #[inline]
    pub fn from_str(s: &str) -> Self {
        Value::Str(crate::utils::interner::intern(s))
    }

    /// Get the numeric value if this is a Number, otherwise None.
    #[inline]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Check if two values are equal (deep comparison)
    pub fn equals(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Ref(a, _), Value::Ref(b, _)) => a.equals(b),
            (Value::Ref(a, _), b) => a.equals(b),
            (a, Value::Ref(b, _)) => a.equals(b),
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                if a.is_nan() || b.is_nan() {
                    false
                } else {
                    (a - b).abs() < 1e-12
                }
            }
            (Value::BigInt(a), Value::BigInt(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Char(a), Value::Str(b)) => b.len() == 1 && b.chars().next() == Some(*a),
            (Value::Str(a), Value::Char(b)) => a.len() == 1 && a.chars().next() == Some(*b),
            (Value::Str(a), Value::Str(b)) => {
                // Fast pointer comparison for interned strings with fallback
                Arc::ptr_eq(a, b) || a == b
            }
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (Value::Tuple(a), Value::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (Value::Set(a), Value::Set(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter().all(|x| b.iter().any(|y| x.equals(y)))
            }
            (Value::Object(a), Value::Object(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter().all(|(k, v)| b.get(k).map_or(false, |bv| v.equals(bv)))
            }
            (Value::Complex(ar, ai), Value::Complex(br, bi)) => {
                (ar - br).abs() < 1e-12 && (ai - bi).abs() < 1e-12
            }
            (Value::Share(a), Value::Share(b)) => a.ptr == b.ptr,
            (Value::Weak(a), Value::Weak(b)) => a.ptr == b.ptr,
            _ => false,
        }
    }

    /// Strict equality (type and value must match exactly)
    #[inline]
    pub fn strict_equals(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other) && self.equals(other)
    }
}

// ============================================
// DEBUG IMPLEMENTATION
// ============================================

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", n),
            Value::BigInt(bi) => write!(f, "{}", bi),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Ref(inner, _) => write!(f, "{:?}", inner),
            Value::Char(c) => write!(f, "'{}'", c),
            Value::Str(s) => write!(f, "\"{}\"", s),
            Value::Null => write!(f, "null"),
            Value::Array(a) => {
                write!(f, "[")?;
                for (i, v) in a.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", v)?;
                }
                write!(f, "]")
            }
            Value::Tuple(t) => {
                if t.is_empty() {
                    write!(f, "()")
                } else if t.len() == 1 {
                    write!(f, "({:?},)", &t[0])
                } else {
                    write!(f, "(")?;
                    for (i, v) in t.iter().enumerate() {
                        if i != 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{:?}", v)?;
                    }
                    write!(f, ")")
                }
            }
            Value::Object(map) => {
                write!(f, "{{")?;
                let mut first = true;
                for (k, v) in map.iter() {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{}: {:?}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Set(s) => {
                write!(f, "{{")?;
                for (i, v) in s.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", v)?;
                }
                write!(f, "}}")
            }
            Value::Complex(r, i) => write!(f, "{}+{}j", r, i),
            Value::Function(_) => write!(f, "<native fn>"),
            Value::BoundNative(name, _) => write!(f, "<bound native {}>", name),
            Value::UserFunction(u) => write!(f, "<fn {}>", u.name),
            Value::Class(c) => write!(f, "<class {}>", c.name),
            Value::Instance(i) => write!(f, "<{} instance>", i.class_name),
            Value::BoundMethod(u, i) => write!(f, "<bound method {}.{}>", i.class_name, u.name),
            Value::Super(_, _) => write!(f, "<super>"),
            Value::Struct(s) => write!(f, "<struct {}>", s.name),
            Value::Enum(e) => write!(f, "<enum {}>", e.name),
            Value::Interface(i) => write!(f, "<interface {}>", i.name),
            Value::EnumCtor(e, v) => write!(f, "<enum ctor {}::{}>", e.name, v),
            Value::Error(le) => write!(f, "<error {}>", le.message),
            Value::Promise(id) => write!(f, "<promise {}>", id),
            Value::Share(sr) => {
                let strong = unsafe { (*sr.ptr).strong_count.load(Ordering::SeqCst) };
                let weak = unsafe { (*sr.ptr).weak_count.load(Ordering::SeqCst) };
                let val = unsafe { &(*sr.ptr).value };
                write!(
                    f,
                    "<strong ref count={}, weak_count={}, value={:?}>",
                    strong, weak, val
                )
            }
            Value::Weak(wr) => {
                let strong = unsafe { (*wr.ptr).strong_count.load(Ordering::SeqCst) };
                let weak = unsafe { (*wr.ptr).weak_count.load(Ordering::SeqCst) };
                write!(f, "<weak ref count={}, weak_count={}>", strong, weak)
            }
        }
    }
}

// ============================================
// SIZE ASSERTIONS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_size() {
        // Ensure Value enum is reasonably sized
        let size = std::mem::size_of::<Value>();
        println!("Value size: {} bytes", size);

        // Should be significantly smaller than old implementation (80+ bytes)
        // Target: 16-32 bytes depending on platform
        assert!(size <= 32, "Value size is {} bytes, expected <= 32", size);
    }

    #[test]
    fn test_interned_strings() {
        let s1 = Value::from_str("hello");
        let s2 = Value::from_str("hello");

        if let (Value::Str(a), Value::Str(b)) = (&s1, &s2) {
            // Same pointer for interned strings
            assert!(Arc::ptr_eq(a, b));
        } else {
            panic!("Expected string values");
        }
    }

    #[test]
    fn test_truthy() {
        assert!(Value::Bool(true).truthy());
        assert!(!Value::Bool(false).truthy());
        assert!(!Value::Null.truthy());
        assert!(Value::Number(1.0).truthy());
        assert!(!Value::Number(0.0).truthy());
        assert!(Value::from_str("hello").truthy());
        assert!(!Value::from_str("").truthy());
    }

    #[test]
    fn test_type_name() {
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Bool(true).type_name(), "boolean");
        assert_eq!(Value::Number(42.0).type_name(), "number");
        assert_eq!(Value::from_str("test").type_name(), "string");
    }
}
