//! Type Layout Engine
//!
//! Computes sizes and alignments for all language types.
//! Provides `sizeof()` and `alignof()` intrinsics for type-safe memory operations.
//!
//! Key rules:
//! - Primitives have fixed widths (u8=1, u16=2, u32=4, u64=8, u128=16, f32=4, f64=8)
//! - Structs are field-aligned with potential padding
//! - Arrays have size = element_size * count
//! - Pointers are 8 bytes (64-bit architectures)
//! - No unknowns: all types must have deterministic layouts

use crate::typesystem::checker::Ty;
use std::fmt;

/// Represents a computed type layout
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeLayout {
    /// Size in bytes
    pub size: usize,
    /// Alignment in bytes (power of 2)
    pub align: usize,
}

impl TypeLayout {
    pub fn new(size: usize, align: usize) -> Self {
        TypeLayout { size, align }
    }

    /// Pad size to next multiple of alignment
    fn pad_to_align(size: usize, align: usize) -> usize {
        size.div_ceil(align) * align
    }
}

impl fmt::Display for TypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}B (align {}B)", self.size, self.align)
    }
}

/// Compute layout for a type
/// Returns error if type size cannot be determined (e.g., completely unknown generic)
pub fn compute_layout(ty: &Ty) -> Result<TypeLayout, String> {
    match ty {
        // Primitives
        Ty::U8 | Ty::I8 => Ok(TypeLayout::new(1, 1)),
        Ty::U16 | Ty::I16 => Ok(TypeLayout::new(2, 2)),
        Ty::U32 | Ty::I32 | Ty::F32 => Ok(TypeLayout::new(4, 4)),
        Ty::U64 | Ty::I64 | Ty::F64Ty => Ok(TypeLayout::new(8, 8)),
        Ty::U128 | Ty::I128 => Ok(TypeLayout::new(16, 16)),

        // Logical types
        Ty::Bool => Ok(TypeLayout::new(1, 1)),
        Ty::Char => Ok(TypeLayout::new(4, 4)), // UTF-32
        Ty::Str => {
            // String is a heap reference (size + capacity + ptr)
            // Approximate as: 8 (ptr) + 8 (len) + 8 (cap) = 24 bytes
            Ok(TypeLayout::new(24, 8))
        }

        // Any, Unknown, Null, Void types - not layoutable
        Ty::Any | Ty::Unknown => Err("Cannot compute layout for Any/Unknown type".to_string()),
        Ty::Null => Err("Cannot compute layout for Null type".to_string()),
        Ty::Void | Ty::Never => Ok(TypeLayout::new(0, 1)), // Zero-sized types

        // Pointers - all 8 bytes on 64-bit
        Ty::Ptr(_) | Ty::PtrOwning(_) | Ty::PtrShared(_) | Ty::PtrMut(_) => {
            Ok(TypeLayout::new(8, 8))
        }

        // Array - should not appear at top-level with unknown size
        // Dynamic arrays are heap pointers
        Ty::Array(elem_ty) => {
            // Array without size info is not valid - must know size
            // For now, treat as error
            Err(format!(
                "Array type without size information: {:?}",
                elem_ty
            ))
        }

        // Map - key-value pairs stored in heap
        Ty::Map(_, _) => {
            // Maps are heap-allocated, represented as (ptr, len, cap)
            Ok(TypeLayout::new(24, 8))
        }

        // Tuples - layout depends on fields
        Ty::Tuple(fields) => {
            if fields.is_empty() {
                return Ok(TypeLayout::new(0, 1));
            }

            let mut size = 0;
            let mut max_align = 1;

            for field_ty in fields {
                let field_layout = compute_layout(field_ty)?;
                // Align size to field's alignment
                size = TypeLayout::pad_to_align(size, field_layout.align);
                size += field_layout.size;
                max_align = max_align.max(field_layout.align);
            }

            // Pad final size to alignment
            size = TypeLayout::pad_to_align(size, max_align);
            Ok(TypeLayout::new(size, max_align))
        }

        // Records (structs) - field-aligned layout
        Ty::Record { required, optional } => {
            let mut size = 0;
            let mut max_align = 1;

            for (_name, field_ty) in required.iter().chain(optional.iter()) {
                let field_layout = compute_layout(field_ty)?;
                // Align size to field's alignment
                size = TypeLayout::pad_to_align(size, field_layout.align);
                size += field_layout.size;
                max_align = max_align.max(field_layout.align);
            }

            // Pad final size to alignment
            size = TypeLayout::pad_to_align(size, max_align);
            Ok(TypeLayout::new(size, max_align))
        }

        // Union - size is max of all alternatives
        Ty::Union(choices) => {
            if choices.is_empty() {
                return Ok(TypeLayout::new(0, 1));
            }

            let mut max_size = 0;
            let mut max_align = 1;

            for choice_ty in choices {
                let choice_layout = compute_layout(choice_ty)?;
                max_size = max_size.max(choice_layout.size);
                max_align = max_align.max(choice_layout.align);
            }

            Ok(TypeLayout::new(max_size, max_align))
        }

        // Nullable - same as inner type plus tag byte
        Ty::Nullable(inner) => {
            let inner_layout = compute_layout(inner)?;
            // Add 1 byte for null tag, then pad to alignment
            let mut size = inner_layout.size + 1;
            size = TypeLayout::pad_to_align(size, inner_layout.align);
            Ok(TypeLayout::new(size, inner_layout.align))
        }

        // Function types - represented as function pointers (8 bytes)
        Ty::Func { .. } => Ok(TypeLayout::new(8, 8)),

        // Generics - cannot layout without instantiation
        Ty::GenericParam(_) => {
            Err("Cannot compute layout for generic parameter without instantiation".to_string())
        }
        Ty::GenericInstance { .. } => {
            Err("Generic instance layout requires full instantiation context".to_string())
        }

        // Nominal and algebraic types
        Ty::Class(_) | Ty::Interface(_) | Ty::Struct(_) => {
            // Managed heap object / struct handle
            Ok(TypeLayout::new(8, 8))
        }
        Ty::Enum(_) => {
            // Enum tag + payload representation
            Ok(TypeLayout::new(16, 8))
        }
        Ty::OptionTy(inner) => {
            let inner_layout = compute_layout(inner)?;
            let mut size = inner_layout.size + 1;
            size = TypeLayout::pad_to_align(size, inner_layout.align);
            Ok(TypeLayout::new(size, inner_layout.align))
        }
        Ty::ResultTy(ok, err) => {
            let ok_layout = compute_layout(ok)?;
            let err_layout = compute_layout(err)?;
            let max_size = ok_layout.size.max(err_layout.size);
            let max_align = ok_layout.align.max(err_layout.align);
            let mut size = max_size + 1;
            size = TypeLayout::pad_to_align(size, max_align);
            Ok(TypeLayout::new(size, max_align))
        }

        // Logical types (Int, Float) - use standard sizes
        Ty::Int => Ok(TypeLayout::new(8, 8)), // i64 equivalent
        Ty::Float => Ok(TypeLayout::new(8, 8)), // f64 equivalent
    }
}

/// Get size of a type in bytes
pub fn size_of(ty: &Ty) -> Result<usize, String> {
    compute_layout(ty).map(|layout| layout.size)
}

/// Get alignment of a type in bytes
pub fn align_of(ty: &Ty) -> Result<usize, String> {
    compute_layout(ty).map(|layout| layout.align)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_sizes() {
        assert_eq!(size_of(&Ty::U8).unwrap(), 1);
        assert_eq!(size_of(&Ty::U16).unwrap(), 2);
        assert_eq!(size_of(&Ty::U32).unwrap(), 4);
        assert_eq!(size_of(&Ty::U64).unwrap(), 8);
        assert_eq!(size_of(&Ty::U128).unwrap(), 16);

        assert_eq!(size_of(&Ty::I8).unwrap(), 1);
        assert_eq!(size_of(&Ty::I32).unwrap(), 4);
        assert_eq!(size_of(&Ty::I64).unwrap(), 8);

        assert_eq!(size_of(&Ty::F32).unwrap(), 4);
        assert_eq!(size_of(&Ty::F64Ty).unwrap(), 8);
    }

    #[test]
    fn test_bool_char_sizes() {
        assert_eq!(size_of(&Ty::Bool).unwrap(), 1);
        assert_eq!(size_of(&Ty::Char).unwrap(), 4); // UTF-32
    }

    #[test]
    fn test_pointer_size() {
        assert_eq!(size_of(&Ty::Ptr(Box::new(Ty::U32))).unwrap(), 8);
        assert_eq!(size_of(&Ty::PtrOwning(Box::new(Ty::I64))).unwrap(), 8);
        assert_eq!(size_of(&Ty::PtrShared(Box::new(Ty::U8))).unwrap(), 8);
        assert_eq!(size_of(&Ty::PtrMut(Box::new(Ty::F64Ty))).unwrap(), 8);
    }

    #[test]
    fn test_empty_tuple() {
        assert_eq!(size_of(&Ty::Tuple(vec![])).unwrap(), 0);
    }

    #[test]
    fn test_simple_tuple() {
        let ty = Ty::Tuple(vec![Ty::U8, Ty::U32]);
        let layout = compute_layout(&ty).unwrap();
        // u8 (1 byte) + padding (3 bytes) + u32 (4 bytes) = 8 bytes
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 4);
    }

    #[test]
    fn test_empty_record() {
        let ty = Ty::Record {
            required: vec![],
            optional: vec![],
        };
        assert_eq!(size_of(&ty).unwrap(), 0);
    }

    #[test]
    fn test_simple_record() {
        let ty = Ty::Record {
            required: vec![("a".to_string(), Ty::U8), ("b".to_string(), Ty::U32)],
            optional: vec![],
        };
        let layout = compute_layout(&ty).unwrap();
        // u8 (1) + padding (3) + u32 (4) = 8
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 4);
    }

    #[test]
    fn test_unknown_type_error() {
        assert!(size_of(&Ty::Any).is_err());
        assert!(size_of(&Ty::Unknown).is_err());
    }

    #[test]
    fn test_void_zero_size() {
        assert_eq!(size_of(&Ty::Void).unwrap(), 0);
        assert_eq!(size_of(&Ty::Never).unwrap(), 0);
    }
}
