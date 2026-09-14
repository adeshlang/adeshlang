//! VIR Type System
//!
//! Backend-neutral type representation.

use std::fmt;

/// VIR Type (explicit, no ownership annotations)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirType {
    /// Void/Unit
    Void,

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

    /// Pointer (raw, untyped)
    Ptr,

    /// Typed pointer
    TypedPtr(Box<VirType>),

    /// Struct (by name, monomorphized)
    Struct(String),

    /// Enum (by name, monomorphized)
    Enum(String),

    /// Array with known size
    Array {
        elem: Box<VirType>,
        size: usize,
    },

    /// Tuple
    Tuple(Vec<VirType>),

    /// Function pointer
    FuncPtr {
        params: Vec<VirType>,
        ret: Box<VirType>,
    },
}

impl VirType {
    /// Get size in bytes (for known-size types)
    pub fn size_bytes(&self) -> Option<usize> {
        match self {
            VirType::Void => Some(0),
            VirType::Bool | VirType::I8 | VirType::U8 => Some(1),
            VirType::I16 | VirType::U16 => Some(2),
            VirType::I32 | VirType::U32 | VirType::F32 => Some(4),
            VirType::I64 | VirType::U64 | VirType::F64 | VirType::Ptr | VirType::TypedPtr(_) => {
                Some(8)
            }
            VirType::I128 | VirType::U128 => Some(16),
            VirType::Tuple(types) => {
                let mut size = 0;
                for ty in types {
                    size += ty.size_bytes()?;
                }
                Some(size)
            }
            VirType::Array { elem, size } => {
                let elem_size = elem.size_bytes()?;
                Some(elem_size * size)
            }
            _ => None,
        }
    }

    /// Get alignment in bytes
    pub fn align_bytes(&self) -> Option<usize> {
        match self {
            VirType::Void => Some(1),
            VirType::Bool | VirType::I8 | VirType::U8 => Some(1),
            VirType::I16 | VirType::U16 => Some(2),
            VirType::I32 | VirType::U32 | VirType::F32 => Some(4),
            VirType::I64 | VirType::U64 | VirType::F64 | VirType::Ptr | VirType::TypedPtr(_) => {
                Some(8)
            }
            VirType::I128 | VirType::U128 => Some(16),
            VirType::Tuple(types) => types.iter().filter_map(|t| t.align_bytes()).max(),
            VirType::Array { elem, .. } => elem.align_bytes(),
            _ => None,
        }
    }

    /// Check if this is an integer type
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            VirType::I8
                | VirType::I16
                | VirType::I32
                | VirType::I64
                | VirType::I128
                | VirType::U8
                | VirType::U16
                | VirType::U32
                | VirType::U64
                | VirType::U128
        )
    }

    /// Check if this is a float type
    pub fn is_float(&self) -> bool {
        matches!(self, VirType::F32 | VirType::F64)
    }

    /// Check if this is a pointer type
    pub fn is_pointer(&self) -> bool {
        matches!(self, VirType::Ptr | VirType::TypedPtr(_))
    }
}

impl fmt::Display for VirType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VirType::Void => write!(f, "void"),
            VirType::Bool => write!(f, "bool"),
            VirType::I8 => write!(f, "i8"),
            VirType::I16 => write!(f, "i16"),
            VirType::I32 => write!(f, "i32"),
            VirType::I64 => write!(f, "i64"),
            VirType::I128 => write!(f, "i128"),
            VirType::U8 => write!(f, "u8"),
            VirType::U16 => write!(f, "u16"),
            VirType::U32 => write!(f, "u32"),
            VirType::U64 => write!(f, "u64"),
            VirType::U128 => write!(f, "u128"),
            VirType::F32 => write!(f, "f32"),
            VirType::F64 => write!(f, "f64"),
            VirType::Ptr => write!(f, "*void"),
            VirType::TypedPtr(inner) => write!(f, "*{}", inner),
            VirType::Struct(name) => write!(f, "{}", name),
            VirType::Enum(name) => write!(f, "{}", name),
            VirType::Array { elem, size } => write!(f, "[{}; {}]", elem, size),
            VirType::Tuple(types) => {
                write!(f, "(")?;
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", ty)?;
                }
                write!(f, ")")
            }
            VirType::FuncPtr { params, ret } => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", ret)
            }
        }
    }
}
