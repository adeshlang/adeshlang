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

    /// Checked alignment padding; rejects zero/non-power-of-two alignments and overflow.
    pub fn checked_pad_to_align(size: usize, align: usize) -> Result<usize, String> {
        if align == 0 || !align.is_power_of_two() {
            return Err(format!("Invalid non-power-of-two alignment: {}", align));
        }
        let rem = size % align;
        if rem == 0 {
            Ok(size)
        } else {
            size.checked_add(align - rem)
                .ok_or_else(|| format!("Layout alignment overflow: size={} align={}", size, align))
        }
    }
}

impl fmt::Display for TypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}B (align {}B)", self.size, self.align)
    }
}

/// Compute layout for a fixed-size array `[elem_ty; count]` on a target with `ptr_bytes` pointer width.
pub fn compute_array_layout_for_target(
    elem_ty: &Ty,
    count: usize,
    ptr_bytes: usize,
) -> Result<TypeLayout, String> {
    let elem = compute_layout_for_target(elem_ty, ptr_bytes)?;
    let total = elem.size.checked_mul(count).ok_or_else(|| {
        format!(
            "Array layout size overflow: elem_size={} * count={}",
            elem.size, count
        )
    })?;
    Ok(TypeLayout::new(total, elem.align))
}

/// Compute layout for a type assuming a 64-bit target (`ptr_bytes = 8`).
/// Returns error if type size cannot be determined or overflows `usize`.
pub fn compute_layout(ty: &Ty) -> Result<TypeLayout, String> {
    compute_layout_for_target(ty, 8)
}

/// Compute layout for a type for a specific target pointer width (`2`, `4`, or `8` bytes).
pub fn compute_layout_for_target(ty: &Ty, ptr_bytes: usize) -> Result<TypeLayout, String> {
    if !matches!(ptr_bytes, 2 | 4 | 8) {
        return Err(format!(
            "Unsupported target pointer width {} bytes (expected 2, 4, or 8)",
            ptr_bytes
        ));
    }
    let ptr_align = ptr_bytes;
    let string_or_map_size = ptr_bytes
        .checked_mul(3)
        .ok_or_else(|| "Pointer-triple size overflow".to_string())?;
    let int_size = if ptr_bytes >= 8 { 8 } else { ptr_bytes };

    match ty {
        // Primitives
        Ty::U8 | Ty::I8 => Ok(TypeLayout::new(1, 1)),
        Ty::U16 | Ty::I16 => Ok(TypeLayout::new(2, 2)),
        Ty::U32 | Ty::I32 | Ty::F32 => Ok(TypeLayout::new(4, 4)),
        Ty::U64 | Ty::I64 | Ty::F64Ty => Ok(TypeLayout::new(8, 8.min(ptr_bytes.max(4)))),
        Ty::U128 | Ty::I128 => Ok(TypeLayout::new(16, 16.min(ptr_bytes.max(4)))),

        // Logical types
        Ty::Bool => Ok(TypeLayout::new(1, 1)),
        Ty::Char => Ok(TypeLayout::new(4, 4.min(ptr_bytes.max(2)))), // UTF-32
        Ty::Str => Ok(TypeLayout::new(string_or_map_size, ptr_align)),

        // Any, Unknown, Null, Void types - not layoutable
        Ty::Any | Ty::Unknown => Err("Cannot compute layout for Any/Unknown type".to_string()),
        Ty::Null => Err("Cannot compute layout for Null type".to_string()),
        Ty::Void | Ty::Never => Ok(TypeLayout::new(0, 1)), // Zero-sized types

        // Pointers - target pointer width
        Ty::Ptr(_) | Ty::PtrOwning(_) | Ty::PtrShared(_) | Ty::PtrMut(_) => {
            Ok(TypeLayout::new(ptr_bytes, ptr_align))
        }

        // Array - dynamic without static count
        Ty::Array(elem_ty) => Err(format!(
            "Array type without size information: {:?}",
            elem_ty
        )),

        // Map - key-value pairs stored in heap (ptr, len, cap)
        Ty::Map(_, _) => Ok(TypeLayout::new(string_or_map_size, ptr_align)),

        // Tuples - layout depends on fields
        Ty::Tuple(fields) => {
            if fields.is_empty() {
                return Ok(TypeLayout::new(0, 1));
            }

            let mut size: usize = 0;
            let mut max_align: usize = 1;

            for field_ty in fields {
                let field_layout = compute_layout_for_target(field_ty, ptr_bytes)?;
                size = TypeLayout::checked_pad_to_align(size, field_layout.align)?;
                size = size
                    .checked_add(field_layout.size)
                    .ok_or_else(|| "Tuple layout size overflow".to_string())?;
                max_align = max_align.max(field_layout.align);
            }

            size = TypeLayout::checked_pad_to_align(size, max_align)?;
            Ok(TypeLayout::new(size, max_align))
        }

        // Records (structs) - field-aligned layout
        Ty::Record { required, optional } => {
            let mut size: usize = 0;
            let mut max_align: usize = 1;

            for (_name, field_ty) in required.iter().chain(optional.iter()) {
                let field_layout = compute_layout_for_target(field_ty, ptr_bytes)?;
                size = TypeLayout::checked_pad_to_align(size, field_layout.align)?;
                size = size
                    .checked_add(field_layout.size)
                    .ok_or_else(|| "Record layout size overflow".to_string())?;
                max_align = max_align.max(field_layout.align);
            }

            size = TypeLayout::checked_pad_to_align(size, max_align)?;
            Ok(TypeLayout::new(size, max_align))
        }

        // Union - size is max of all alternatives (padded to max_align)
        Ty::Union(choices) => {
            if choices.is_empty() {
                return Ok(TypeLayout::new(0, 1));
            }

            let mut max_size: usize = 0;
            let mut max_align: usize = 1;

            for choice_ty in choices {
                let choice_layout = compute_layout_for_target(choice_ty, ptr_bytes)?;
                max_size = max_size.max(choice_layout.size);
                max_align = max_align.max(choice_layout.align);
            }

            max_size = TypeLayout::checked_pad_to_align(max_size, max_align)?;
            Ok(TypeLayout::new(max_size, max_align))
        }

        // Nullable - inner type plus 1-byte discriminant tag, padded to max alignment
        Ty::Nullable(inner) => {
            let inner_layout = compute_layout_for_target(inner, ptr_bytes)?;
            let with_tag = inner_layout
                .size
                .checked_add(1)
                .ok_or_else(|| "Nullable layout size overflow".to_string())?;
            let size = TypeLayout::checked_pad_to_align(with_tag, inner_layout.align)?;
            Ok(TypeLayout::new(size, inner_layout.align))
        }

        // Function types - represented as target function pointers
        Ty::Func { .. } => Ok(TypeLayout::new(ptr_bytes, ptr_align)),

        // Generics - cannot layout without instantiation
        Ty::GenericParam(_) => {
            Err("Cannot compute layout for generic parameter without instantiation".to_string())
        }
        Ty::GenericInstance { .. } => {
            Err("Generic instance layout requires full instantiation context".to_string())
        }

        // Nominal and algebraic types
        Ty::Class(_) | Ty::Interface(_) | Ty::Struct(_) => {
            Ok(TypeLayout::new(ptr_bytes, ptr_align))
        }
        Ty::Enum(_) => {
            // Enum tag (pointer-width) + payload (pointer-width) representation
            let size = ptr_bytes
                .checked_mul(2)
                .ok_or_else(|| "Enum layout size overflow".to_string())?;
            Ok(TypeLayout::new(size, ptr_align))
        }
        Ty::OptionTy(inner) => {
            let inner_layout = compute_layout_for_target(inner, ptr_bytes)?;
            let with_tag = inner_layout
                .size
                .checked_add(1)
                .ok_or_else(|| "Option layout size overflow".to_string())?;
            let size = TypeLayout::checked_pad_to_align(with_tag, inner_layout.align)?;
            Ok(TypeLayout::new(size, inner_layout.align))
        }
        Ty::ResultTy(ok, err) => {
            let ok_layout = compute_layout_for_target(ok, ptr_bytes)?;
            let err_layout = compute_layout_for_target(err, ptr_bytes)?;
            let max_size = ok_layout.size.max(err_layout.size);
            let max_align = ok_layout.align.max(err_layout.align);
            let with_tag = max_size
                .checked_add(1)
                .ok_or_else(|| "Result layout size overflow".to_string())?;
            let size = TypeLayout::checked_pad_to_align(with_tag, max_align)?;
            Ok(TypeLayout::new(size, max_align))
        }

        // Logical types (Int, Float)
        Ty::Int => Ok(TypeLayout::new(int_size, int_size)),
        Ty::Float => Ok(TypeLayout::new(8, 8.min(ptr_bytes.max(4)))),
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

    #[test]
    fn test_target_pointer_widths_and_checked_overflow() {
        // 16-bit target (i8086 / AVR)
        let p16 = compute_layout_for_target(&Ty::Ptr(Box::new(Ty::U8)), 2).unwrap();
        assert_eq!(p16, TypeLayout::new(2, 2));
        // 32-bit target (i686 / WASM32)
        let p32 = compute_layout_for_target(&Ty::Ptr(Box::new(Ty::U8)), 4).unwrap();
        assert_eq!(p32, TypeLayout::new(4, 4));
        // 64-bit target (x86_64 / AArch64 / RISC-V 64)
        let p64 = compute_layout_for_target(&Ty::Ptr(Box::new(Ty::U8)), 8).unwrap();
        assert_eq!(p64, TypeLayout::new(8, 8));

        // Checked overflow on array layout
        assert!(compute_array_layout_for_target(&Ty::U64, usize::MAX, 8).is_err());
        assert!(TypeLayout::checked_pad_to_align(usize::MAX, 8).is_err());
        assert!(TypeLayout::checked_pad_to_align(16, 3).is_err());
        assert!(compute_layout_for_target(&Ty::U8, 3).is_err());
    }
}
