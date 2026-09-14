//! MIR Type System
//!
//! Type representation in MIR with ownership and lifetime information.

use std::fmt;

/// MIR Type with ownership semantics
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirType {
    /// Unit type ()
    Unit,

    /// Boolean
    Bool,

    /// Signed integers
    I8,
    I16,
    I32,
    I64,
    I128,

    /// Unsigned integers
    U8,
    U16,
    U32,
    U64,
    U128,

    /// Floating point
    F32,
    F64,

    /// String
    String,

    /// Reference with borrow kind
    Ref {
        kind: RefKind,
        inner: Box<MirType>,
    },

    /// Pointer (unsafe)
    Ptr(Box<MirType>),

    /// Tuple
    Tuple(Vec<MirType>),

    /// Array with known size
    Array(Box<MirType>, usize),

    /// Slice (dynamically sized)
    Slice(Box<MirType>),

    /// Struct
    Struct(String),

    /// Enum
    Enum(String),

    /// Function type
    Function {
        params: Vec<MirType>,
        ret: Box<MirType>,
    },

    /// ARC-wrapped type (reference counted)
    Arc(Box<MirType>),

    /// Weak reference
    Weak(Box<MirType>),

    /// Never type (!)
    Never,
}

/// Reference kind (immutable or mutable)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Shared, // &T
    Mut,    // &mut T
}

/// Ownership kind for values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipKind {
    /// Value is owned and will be dropped
    Owned,

    /// Value is borrowed (shared reference)
    Borrowed,

    /// Value is mutably borrowed
    BorrowedMut,

    /// Value is reference counted (ARC)
    Shared,

    /// Value is Copy (can be duplicated)
    Copy,
}

impl MirType {
    /// Check if this type is Copy
    pub fn is_copy(&self) -> bool {
        match self {
            MirType::Unit
            | MirType::Bool
            | MirType::I8
            | MirType::I16
            | MirType::I32
            | MirType::I64
            | MirType::I128
            | MirType::U8
            | MirType::U16
            | MirType::U32
            | MirType::U64
            | MirType::U128
            | MirType::F32
            | MirType::F64
            | MirType::Ref { .. }
            | MirType::Ptr(_) => true,

            MirType::Tuple(types) => types.iter().all(|t| t.is_copy()),
            MirType::Array(inner, _) => inner.is_copy(),

            _ => false,
        }
    }

    /// Check if this type requires a drop
    pub fn needs_drop(&self) -> bool {
        match self {
            MirType::Unit
            | MirType::Bool
            | MirType::I8
            | MirType::I16
            | MirType::I32
            | MirType::I64
            | MirType::I128
            | MirType::U8
            | MirType::U16
            | MirType::U32
            | MirType::U64
            | MirType::U128
            | MirType::F32
            | MirType::F64
            | MirType::Ref { .. }
            | MirType::Ptr(_)
            | MirType::Never => false,

            MirType::String | MirType::Arc(_) | MirType::Weak(_) => true,

            MirType::Tuple(types) => types.iter().any(|t| t.needs_drop()),
            MirType::Array(inner, _) | MirType::Slice(inner) => inner.needs_drop(),

            // Conservative: assume struct/enum need drop
            MirType::Struct(_) | MirType::Enum(_) => true,

            MirType::Function { .. } => false,
        }
    }

    /// Get the size in bytes (for known-size types)
    pub fn size_bytes(&self) -> Option<usize> {
        match self {
            MirType::Unit => Some(0),
            MirType::Bool | MirType::I8 | MirType::U8 => Some(1),
            MirType::I16 | MirType::U16 => Some(2),
            MirType::I32 | MirType::U32 | MirType::F32 => Some(4),
            MirType::I64 | MirType::U64 | MirType::F64 | MirType::Ref { .. } | MirType::Ptr(_) => {
                Some(8)
            }
            MirType::I128 | MirType::U128 => Some(16),

            MirType::Tuple(types) => {
                let mut size = 0;
                for ty in types {
                    size += ty.size_bytes()?;
                }
                Some(size)
            }

            MirType::Array(inner, count) => {
                let elem_size = inner.size_bytes()?;
                Some(elem_size * count)
            }

            // Unknown size
            _ => None,
        }
    }
}

impl fmt::Display for MirType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MirType::Unit => write!(f, "()"),
            MirType::Bool => write!(f, "bool"),
            MirType::I8 => write!(f, "i8"),
            MirType::I16 => write!(f, "i16"),
            MirType::I32 => write!(f, "i32"),
            MirType::I64 => write!(f, "i64"),
            MirType::I128 => write!(f, "i128"),
            MirType::U8 => write!(f, "u8"),
            MirType::U16 => write!(f, "u16"),
            MirType::U32 => write!(f, "u32"),
            MirType::U64 => write!(f, "u64"),
            MirType::U128 => write!(f, "u128"),
            MirType::F32 => write!(f, "f32"),
            MirType::F64 => write!(f, "f64"),
            MirType::String => write!(f, "String"),
            MirType::Ref { kind, inner } => match kind {
                RefKind::Shared => write!(f, "&{}", inner),
                RefKind::Mut => write!(f, "&mut {}", inner),
            },
            MirType::Ptr(inner) => write!(f, "*{}", inner),
            MirType::Tuple(types) => {
                write!(f, "(")?;
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", ty)?;
                }
                write!(f, ")")
            }
            MirType::Array(inner, size) => write!(f, "[{}; {}]", inner, size),
            MirType::Slice(inner) => write!(f, "[{}]", inner),
            MirType::Struct(name) => write!(f, "{}", name),
            MirType::Enum(name) => write!(f, "{}", name),
            MirType::Function { params, ret } => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", ret)
            }
            MirType::Arc(inner) => write!(f, "Arc<{}>", inner),
            MirType::Weak(inner) => write!(f, "Weak<{}>", inner),
            MirType::Never => write!(f, "!"),
        }
    }
}
