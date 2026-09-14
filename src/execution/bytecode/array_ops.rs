//! Specialized Array Operations
//!
//! This module provides type-specialized implementations of array operations
//! that dispatch to optimized paths based on element type and array kind.
//!
//! Tier 0 (Interpreter): Uses runtime dispatch with type checks
//! Tier 1 (Baseline JIT): Inline type guards and specialized paths
//! Tier 2 (Optimizing JIT): Fully specialized with SIMD, vectorization
//! AOT: Static specialization with monomorphization

use crate::parsing::ast::Value;
use crate::types::array_types::{ArrayElementType, ArrayKind, DynamicArray};

/// Array operation dispatcher
///
/// Routes operations to specialized implementations based on array kind and element type
pub struct ArrayOps;

impl ArrayOps {
    /// Get array length (specialized by kind)
    pub fn len(array: &ArrayKind) -> usize {
        array.len()
    }

    /// Get element at index (bounds-checked)
    ///
    /// Tier 0: Runtime bounds check
    /// Tier 1: Guarded inline check with deopt on fail
    /// Tier 2: Eliminated bounds check if provably safe
    pub fn get(array: &ArrayKind, index: usize) -> Result<Value, String> {
        // Bounds check
        if index >= array.len() {
            return Err(format!(
                "Index {} out of bounds for array of length {}",
                index,
                array.len()
            ));
        }

        match array {
            ArrayKind::Raw(raw) => {
                // Direct access with zero-overhead in optimized tiers
                Ok(raw.data[index].clone())
            }
            ArrayKind::SSO(sso) => {
                // SSO-specific access - handles both inline and heap storage
                match sso {
                    crate::typesystem::array_types::SSOArray::Inline {
                        len,
                        element_type,
                        data,
                    } => {
                        if index >= *len as usize {
                            return Err(format!(
                                "Index {} out of bounds for inline SSO array of length {}",
                                index, len
                            ));
                        }
                        // Read value from inline storage based on element type
                        let elem_size = element_type.element_size();
                        let offset = index * elem_size;
                        match crate::typesystem::array_types::SSOArray::read_value_from_bytes(
                            &data[offset..offset + elem_size],
                            element_type,
                        ) {
                            Some(value) => Ok(value),
                            None => Err(format!(
                                "Failed to read SSO array element at index {}",
                                index
                            )),
                        }
                    }
                    crate::typesystem::array_types::SSOArray::Heap {
                        ptr,
                        len,
                        element_type,
                        ..
                    } => {
                        if index >= *len as usize {
                            return Err(format!(
                                "Index {} out of bounds for heap SSO array of length {}",
                                index, len
                            ));
                        }
                        // Read value from heap storage
                        let elem_size = element_type.element_size();
                        let offset = index * elem_size;
                        unsafe {
                            let data_slice = std::slice::from_raw_parts(ptr.add(offset), elem_size);
                            match crate::typesystem::array_types::SSOArray::read_value_from_bytes(
                                data_slice,
                                element_type,
                            ) {
                                Some(value) => Ok(value),
                                None => Err(format!(
                                    "Failed to read SSO array element at index {}",
                                    index
                                )),
                            }
                        }
                    }
                }
            }
            ArrayKind::Compact(compact) => {
                // Compact array access with u16 length/capacity
                if index >= compact.len as usize {
                    return Err(format!(
                        "Index {} out of bounds for compact array of length {}",
                        index, compact.len
                    ));
                }
                // Access via pointer
                unsafe {
                    let value = compact.ptr.add(index).read();
                    Ok(value)
                }
            }
            ArrayKind::Dynamic(dyn_arr) => {
                // Standard Vec access
                Ok(dyn_arr.data[index].clone())
            }
            ArrayKind::Arena(arena) => {
                // Arena-based indexed access using arena ID and offset
                if index >= arena.len as usize {
                    return Err(format!(
                        "Index {} out of bounds for arena array of length {}",
                        index, arena.len
                    ));
                }
                // Access via arena allocator
                // For production implementation, this would use the arena allocator
                // to resolve arena_id and element_offset to actual memory
                // For now, we return an error indicating arena access needs allocator context
                Err(format!(
                    "Arena array access requires allocator context (index {})",
                    index
                ))
            }
        }
    }

    /// Set element at index (bounds-checked)
    pub fn set(array: &mut ArrayKind, index: usize, value: Value) -> Result<(), String> {
        // Bounds check
        if index >= array.len() {
            return Err(format!(
                "Index {} out of bounds for array of length {}",
                index,
                array.len()
            ));
        }

        match array {
            ArrayKind::Raw(raw) => {
                // Type check: ensure value matches element type
                if !Self::validate_element_type(&value, &raw.element_type) {
                    return Err(format!(
                        "Type mismatch: cannot assign {:?} to array of type {}",
                        value, raw.element_type
                    ));
                }
                raw.data[index] = value;
                Ok(())
            }
            ArrayKind::SSO(sso) => sso.set(index, value),
            ArrayKind::Compact(compact) => {
                // Set in compact array
                if index >= compact.len as usize {
                    return Err(format!("Index {} out of bounds", index));
                }
                // Type validation would go here in production
                // For now, write directly to pointer
                unsafe {
                    compact.ptr.add(index).write(value);
                }
                Ok(())
            }
            ArrayKind::Dynamic(dyn_arr) => {
                // Type check and set
                if !Self::validate_element_type_enum(&value, &dyn_arr.element_type) {
                    return Err(format!(
                        "Type mismatch: cannot assign {:?} to array of type {}",
                        value, dyn_arr.concrete_type
                    ));
                }
                dyn_arr.data[index] = value;
                Ok(())
            }
            ArrayKind::Arena(_arena) => {
                // Arena array set operation requires allocator context
                Err("Arena array set operation requires allocator context".to_string())
            }
        }
    }

    /// Append element to array (type-checked)
    pub fn append(array: &mut ArrayKind, value: Value) -> Result<(), String> {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                // Type check
                if !Self::value_matches_element_type(&value, dyn_arr.element_type) {
                    return Err(format!(
                        "Cannot append {:?} to array of type {:?}",
                        value, dyn_arr.element_type
                    ));
                }
                dyn_arr.data.push(value);
                Ok(())
            }
            ArrayKind::Raw(_) => Err("Cannot append to fixed-size raw array".to_string()),
            _ => Err("Append operation not yet implemented for this array kind".to_string()),
        }
    }

    /// Map operation with specialized implementations
    ///
    /// Tier 0: Interpreted loop
    /// Tier 1: Inline loop with type guards
    /// Tier 2: Vectorized SIMD loop for numeric types
    pub fn map<F>(array: &ArrayKind, f: F) -> Result<ArrayKind, String>
    where
        F: Fn(&Value) -> Result<Value, String>,
    {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                let mut result = Vec::with_capacity(dyn_arr.data.len());
                for elem in &dyn_arr.data {
                    result.push(f(elem)?);
                }
                Ok(ArrayKind::Dynamic(DynamicArray::new(result)))
            }
            _ => Err("Map operation not yet fully implemented".to_string()),
        }
    }

    /// Filter operation
    ///
    /// Tier 2: Predicated execution with SIMD gather
    pub fn filter<F>(array: &ArrayKind, predicate: F) -> Result<ArrayKind, String>
    where
        F: Fn(&Value) -> bool,
    {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                let result: Vec<Value> = dyn_arr
                    .data
                    .iter()
                    .filter(|v| predicate(v))
                    .cloned()
                    .collect();
                Ok(ArrayKind::Dynamic(DynamicArray::new(result)))
            }
            _ => Err("Filter operation not yet fully implemented".to_string()),
        }
    }

    /// Reduce operation
    ///
    /// Tier 2: Horizontal reduction with SIMD for associative ops
    pub fn reduce<F>(array: &ArrayKind, init: Value, f: F) -> Result<Value, String>
    where
        F: Fn(Value, &Value) -> Result<Value, String>,
    {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                let mut acc = init;
                for elem in &dyn_arr.data {
                    acc = f(acc, elem)?;
                }
                Ok(acc)
            }
            _ => Err("Reduce operation not yet fully implemented".to_string()),
        }
    }

    /// Check if value matches element type
    fn value_matches_element_type(value: &Value, elem_type: ArrayElementType) -> bool {
        match (value, elem_type) {
            (Value::U8(_) | Value::I8(_) | Value::Bool(_), ArrayElementType::Byte) => true,
            (Value::U16(_) | Value::I16(_), ArrayElementType::Short) => true,
            (Value::U32(_) | Value::I32(_) | Value::F32(_), ArrayElementType::Word) => true,
            (Value::U64(_) | Value::I64(_) | Value::F64(_), ArrayElementType::Long) => true,
            (Value::U128(_) | Value::I128(_), ArrayElementType::Extended) => true,
            (_, ArrayElementType::Any) => true,
            _ => false,
        }
    }

    /// Validate element type by string (for Raw, Compact, Dynamic arrays)
    fn validate_element_type(value: &Value, type_name: &str) -> bool {
        // Simple type matching - in production this would use full type system
        match (value, type_name) {
            (Value::U8(_), "u8") | (Value::I8(_), "i8") | (Value::Bool(_), "bool") => true,
            (Value::U16(_), "u16") | (Value::I16(_), "i16") => true,
            (Value::U32(_), "u32") | (Value::I32(_), "i32") => true,
            (Value::U64(_), "u64") | (Value::I64(_), "i64") => true,
            (Value::F32(_), "f32") => true,
            (Value::F64(_), "f64") => true,
            (Value::U128(_), "u128") | (Value::I128(_), "i128") => true,
            (Value::Str(_), "String" | "str") => true,
            // Any type accepts anything
            (_, _) if type_name == "Any" || type_name == "any" => true,
            _ => false,
        }
    }

    /// Validate element type enum (for SSO arrays)
    fn validate_element_type_enum(value: &Value, elem_type: &ArrayElementType) -> bool {
        Self::value_matches_element_type(value, *elem_type)
    }
}

/// Specialized numeric array operations (for JIT/AOT)
///
/// These are templates for generating specialized machine code
pub mod specialized {

    /// Specialized u8 array sum (SIMD-ready)
    ///
    /// In Tier 2: Compiles to SIMD horizontal sum using PHADD or similar
    pub fn sum_u8(data: &[u8]) -> u64 {
        data.iter().map(|&x| x as u64).sum()
    }

    /// Specialized f32 array sum (SIMD-ready)
    ///
    /// In Tier 2: Compiles to SSE/AVX horizontal add
    pub fn sum_f32(data: &[f32]) -> f32 {
        data.iter().sum()
    }

    /// Specialized f64 array sum (SIMD-ready)
    pub fn sum_f64(data: &[f64]) -> f64 {
        data.iter().sum()
    }

    /// Specialized i32 array map (SIMD-ready)
    ///
    /// In Tier 2: Vectorized loop with packed operations
    pub fn map_i32<F>(data: &[i32], f: F) -> Vec<i32>
    where
        F: Fn(i32) -> i32,
    {
        data.iter().map(|&x| f(x)).collect()
    }

    /// Specialized f32 array map (SIMD-ready)
    pub fn map_f32<F>(data: &[f32], f: F) -> Vec<f32>
    where
        F: Fn(f32) -> f32,
    {
        data.iter().map(|&x| f(x)).collect()
    }

    /// Dot product for f32 arrays (SIMD-optimized)
    ///
    /// In Tier 2: Compiles to FMA (fused multiply-add) instructions
    pub fn dot_product_f32(a: &[f32], b: &[f32]) -> Result<f32, String> {
        if a.len() != b.len() {
            return Err("Arrays must have same length for dot product".to_string());
        }

        Ok(a.iter().zip(b.iter()).map(|(x, y)| x * y).sum())
    }

    /// Matrix-vector multiply (specialized for f32)
    ///
    /// In Tier 2: Tiled computation with register blocking
    pub fn matvec_f32(matrix: &[Vec<f32>], vec: &[f32]) -> Result<Vec<f32>, String> {
        if matrix.is_empty() {
            return Ok(Vec::new());
        }

        let cols = matrix[0].len();
        if vec.len() != cols {
            return Err("Vector length must match matrix columns".to_string());
        }

        Ok(matrix
            .iter()
            .map(|row| dot_product_f32(row, vec).unwrap_or(0.0))
            .collect())
    }
}

/// Bounds check elimination utilities (for optimizing JIT)
pub mod bounds_check {
    /// Check if index is provably in bounds at compile time
    pub fn is_statically_safe(index: usize, array_len: usize) -> bool {
        index < array_len
    }

    /// Check if loop iteration bounds are known safe
    pub fn loop_bounds_safe(start: usize, end: usize, array_len: usize) -> bool {
        end <= array_len && start <= end
    }

    /// Mark bounds check as eliminated (for JIT backend)
    #[inline(always)]
    pub fn assume_in_bounds<T>(slice: &[T], index: usize) -> &T {
        // Safety: Caller must guarantee index < slice.len()
        // In release builds with optimizations, this becomes a no-op
        debug_assert!(index < slice.len());
        unsafe { slice.get_unchecked(index) }
    }
}

/// Memory layout utilities for different array kinds
pub mod layout {
    use super::*;

    /// Calculate total memory footprint of an array
    pub fn total_bytes(array: &ArrayKind) -> usize {
        let metadata = array.metadata_bytes();
        let elem_type = array.element_type();
        let elem_size = elem_type.element_size();
        let data_bytes = array.len() * elem_size;

        metadata + data_bytes
    }

    /// Check if array is cache-friendly (fits in L1 cache)
    pub fn fits_in_l1_cache(array: &ArrayKind) -> bool {
        total_bytes(array) <= 32768 // 32KB typical L1 cache
    }

    /// Check if array is small enough for SSO
    pub fn can_use_sso(elem_type: ArrayElementType, len: usize) -> bool {
        elem_type.element_size() * len <= 22
    }

    /// Recommend optimal array kind for given parameters
    pub fn recommend_kind(elem_type: ArrayElementType, len: usize, is_fixed: bool) -> &'static str {
        if is_fixed {
            return "RawArray";
        }

        if can_use_sso(elem_type, len) {
            return "SSOArray";
        }

        if len <= 65535 {
            return "CompactArray";
        }

        "DynamicArray"
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        execution::array_ops::{bounds_check, specialized},
        types::array_types::*,
    };

    #[test]
    fn test_specialized_sum() {
        let data_u8: Vec<u8> = vec![1, 2, 3, 4, 5];
        assert_eq!(specialized::sum_u8(&data_u8), 15);

        let data_f32: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(specialized::sum_f32(&data_f32), 15.0);
    }

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let result = specialized::dot_product_f32(&a, &b).unwrap();
        assert_eq!(result, 32.0); // 1*4 + 2*5 + 3*6 = 32
    }

    #[test]
    fn test_bounds_check() {
        assert!(bounds_check::is_statically_safe(5, 10));
        assert!(!bounds_check::is_statically_safe(10, 10));
        assert!(!bounds_check::is_statically_safe(11, 10));
    }

    #[test]
    fn test_layout_recommendations() {
        // Small byte array should use SSO
        assert_eq!(
            super::layout::recommend_kind(ArrayElementType::Byte, 10, false),
            "SSOArray"
        );

        // Medium word array should use Compact
        assert_eq!(
            super::layout::recommend_kind(ArrayElementType::Word, 100, false),
            "CompactArray"
        );

        // Large array should use Dynamic
        assert_eq!(
            super::layout::recommend_kind(ArrayElementType::Long, 100000, false),
            "DynamicArray"
        );

        // Fixed-size should use Raw
        assert_eq!(
            super::layout::recommend_kind(ArrayElementType::Byte, 10, true),
            "RawArray"
        );
    }
}
