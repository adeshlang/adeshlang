//! Field Layout Computation System - Phase 0.2
//!
//! Computes byte offsets for all fields in structs and classes.
//! This is the heart of the memory optimization - instead of using HashMap,
//! we compute exact offsets at class definition time and access fields
//! directly via Vec<u8> + offset arithmetic.
//!
//! Field Layout Algorithm:
//! 1. For each field in declaration order
//! 2. Pad current offset to field's alignment requirement
//! 3. Place field at padded offset
//! 4. Advance offset by field size
//! 5. Update max alignment seen
//! 6. Final size = pad(offset, max_align)
//!
//! Example:
//! struct Point {
//!   x: i32,      // offset 0, size 4, align 4
//!   y: i32,      // offset 4, size 4, align 4
//!   z: i64,      // offset 8, size 8, align 8
//! }
//! Final: size = 16, align = 8

use crate::types::type_info::FieldInfo;
use crate::types::visibility::Visibility;
use std::fmt;

/// Layout information for a single type
#[derive(Debug, Clone)]
pub struct FieldLayout {
    /// Total size in bytes needed to store all fields
    pub size: usize,

    /// Alignment requirement (power of 2)
    pub align: usize,

    /// Information about each field
    pub fields: Vec<FieldInfo>,

    /// Mapping from field name to offset (for fast lookup)
    field_offsets: std::collections::HashMap<String, usize>,

    /// Mapping from field name to index in self.fields (for fast O(1) lookup)
    field_indices: std::collections::HashMap<String, usize>,
}

impl FieldLayout {
    /// Creates a new empty layout
    pub fn new() -> Self {
        FieldLayout {
            size: 0,
            align: 1,
            fields: vec![],
            field_offsets: std::collections::HashMap::new(),
            field_indices: std::collections::HashMap::new(),
        }
    }

    /// Adds a field to the layout
    /// Returns the computed offset for this field
    pub fn add_field(
        &mut self,
        name: String,
        field_size: usize,
        field_align: usize,
        visibility: Visibility,
        type_name: String,
    ) -> usize {
        // Pad current size to field's alignment
        let offset = self.pad_to_align(self.size, field_align);

        // Create field info
        let field = FieldInfo::new(
            name.clone(),
            offset,
            field_size,
            field_align,
            visibility,
            type_name,
        );

        // Update layout
        let next_idx = self.fields.len();
        self.fields.push(field);
        self.field_offsets.insert(name.clone(), offset);
        self.field_indices.insert(name, next_idx);
        self.size = offset + field_size;
        self.align = self.align.max(field_align);

        offset
    }

    /// Finalizes the layout (pad to alignment)
    pub fn finalize(&mut self) {
        self.size = self.pad_to_align(self.size, self.align);
    }

    /// Gets the final size after padding
    pub fn get_final_size(&self) -> usize {
        self.pad_to_align(self.size, self.align)
    }

    /// Looks up a field offset by name
    pub fn get_field_offset(&self, name: &str) -> Option<usize> {
        self.field_offsets.get(name).copied()
    }

    /// Finds field info by name
    pub fn find_field(&self, name: &str) -> Option<&FieldInfo> {
        self.field_indices.get(name).map(|&idx| &self.fields[idx])
    }

    /// Pads size to alignment boundary
    fn pad_to_align(&self, size: usize, align: usize) -> usize {
        if align == 0 {
            return size;
        }
        ((size + align - 1) / align) * align
    }
}

impl Default for FieldLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for FieldLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FieldLayout {{ size: {}, align: {}, fields: {} }}",
            self.size,
            self.align,
            self.fields.len()
        )
    }
}

/// Layout computer for different type kinds
pub struct LayoutComputer;

impl LayoutComputer {
    /// Computes layout for primitive types
    pub fn compute_primitive(ty_name: &str) -> Option<(usize, usize)> {
        match ty_name {
            "u8" | "i8" => Some((1, 1)),
            "u16" | "i16" => Some((2, 2)),
            "u32" | "i32" | "f32" => Some((4, 4)),
            "u64" | "i64" | "f64" => Some((8, 8)),
            "u128" | "i128" => Some((16, 16)),
            "bool" => Some((1, 1)),
            "char" => Some((4, 4)),
            "String" | "string" => Some((24, 8)), // ptr + len + cap
            "null" => Some((0, 1)),               // Zero-sized
            _ => None,
        }
    }

    /// Computes layout for pointer types
    pub fn compute_pointer() -> (usize, usize) {
        (8, 8) // 64-bit pointers
    }

    /// Computes layout for array types
    pub fn compute_array(
        element_size: usize,
        element_align: usize,
        count: usize,
    ) -> (usize, usize) {
        (element_size * count, element_align)
    }

    /// Computes layout for tuple types
    pub fn compute_tuple(field_types: &[(usize, usize)]) -> (usize, usize) {
        let mut size = 0;
        let mut align = 1;

        for (field_size, field_align) in field_types {
            // Pad to alignment
            size = ((size + field_align - 1) / field_align) * field_align;
            size += field_size;
            align = align.max(*field_align);
        }

        // Pad to alignment
        size = ((size + align - 1) / align) * align;
        (size, align)
    }
}

/// Computes layout for a class with inheritance
pub fn compute_class_layout(
    fields: Vec<(String, String, Visibility)>,
    parent_layout: Option<&FieldLayout>,
) -> FieldLayout {
    let mut layout = FieldLayout::new();

    // If there's a parent, start after parent fields
    if let Some(parent) = parent_layout {
        layout.size = parent.get_final_size();
        layout.align = parent.align;
        // Copy parent fields
        for field in &parent.fields {
            layout.fields.push(field.clone());
            layout
                .field_offsets
                .insert(field.name.clone(), field.offset);
        }
    }

    // Add this class's fields
    for (name, ty_name, visibility) in fields {
        let (field_size, field_align) = LayoutComputer::compute_primitive(&ty_name)
            .or_else(|| {
                if ty_name.starts_with("ref ") || ty_name.starts_with("mut ") {
                    Some(LayoutComputer::compute_pointer())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                // For custom types, assume same as pointer for now
                // Real implementation would look up in TypeRegistry
                LayoutComputer::compute_pointer()
            });

        layout.add_field(name, field_size, field_align, visibility, ty_name);
    }

    layout.finalize();
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_layout() {
        let mut layout = FieldLayout::new();
        layout.add_field("x".to_string(), 4, 4, Visibility::Public, "i32".to_string());
        layout.add_field("y".to_string(), 4, 4, Visibility::Public, "i32".to_string());
        layout.finalize();

        assert_eq!(layout.get_field_offset("x"), Some(0));
        assert_eq!(layout.get_field_offset("y"), Some(4));
        assert_eq!(layout.get_final_size(), 8);
    }

    #[test]
    fn test_alignment_padding() {
        let mut layout = FieldLayout::new();
        layout.add_field("a".to_string(), 1, 1, Visibility::Public, "u8".to_string());
        layout.add_field("b".to_string(), 8, 8, Visibility::Public, "i64".to_string());
        layout.finalize();

        assert_eq!(layout.get_field_offset("a"), Some(0));
        assert_eq!(layout.get_field_offset("b"), Some(8)); // Padded to 8-byte alignment
        assert_eq!(layout.get_final_size(), 16);
    }

    #[test]
    fn test_primitive_sizes() {
        assert_eq!(LayoutComputer::compute_primitive("u8"), Some((1, 1)));
        assert_eq!(LayoutComputer::compute_primitive("i32"), Some((4, 4)));
        assert_eq!(LayoutComputer::compute_primitive("i64"), Some((8, 8)));
        assert_eq!(LayoutComputer::compute_primitive("i128"), Some((16, 16)));
    }

    #[test]
    fn test_pointer_size() {
        let (size, align) = LayoutComputer::compute_pointer();
        assert_eq!(size, 8);
        assert_eq!(align, 8);
    }

    #[test]
    fn test_tuple_layout() {
        let fields = vec![(4, 4), (4, 4), (8, 8)];
        let (size, align) = LayoutComputer::compute_tuple(&fields);
        assert_eq!(align, 8);
        assert!(size >= 16); // At least i32 + i32 + i64
    }

    #[test]
    fn test_class_with_parent() {
        let mut parent_layout = FieldLayout::new();
        parent_layout.add_field("x".to_string(), 4, 4, Visibility::Public, "i32".to_string());
        parent_layout.finalize();

        let child_fields = vec![("y".to_string(), "i32".to_string(), Visibility::Public)];
        let child_layout = compute_class_layout(child_fields, Some(&parent_layout));

        assert_eq!(child_layout.get_field_offset("x"), Some(0));
        assert_eq!(child_layout.get_field_offset("y"), Some(4));
    }
}
