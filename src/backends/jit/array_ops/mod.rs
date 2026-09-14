//! Baseline JIT Array Optimizations
//!
//! This module provides type-specialized array operations for the baseline JIT tier (Tier 1).
//!
//! ## Baseline JIT Strategy:
//! - Inline type guards with deoptimization
//! - Specialized paths for common element types (u8, i32, f32, f64)
//! - Bounds checking with branch prediction hints
//! - Direct memory access for specialized types
//!
//! ## Code Generation Pattern:
//! ```
//! if (array.element_type == ElementType::Word && array.kind == ArrayKind::Dynamic) {
//!     // Fast path: specialized i32 access
//!     let data_ptr = array.data_ptr as *const i32;
//!     if (index < array.len) {
//!         return unsafe { *data_ptr.offset(index) };
//!     }
//! }
//! // Slow path: deoptimize to interpreter
//! deoptimize_and_retry(array, index);
//! ```

use crate::parsing::ast::Value;
use crate::types::array_types::{ArrayElementType, ArrayKind};

/// JIT compilation context for array operations
pub struct JitArrayContext {
    /// Whether bounds checks can be eliminated
    pub bounds_check_eliminated: bool,
    /// Whether type is statically known
    pub static_type: Option<ArrayElementType>,
    /// Whether array is known to be non-null
    pub non_null: bool,
}

impl JitArrayContext {
    pub fn new() -> Self {
        JitArrayContext {
            bounds_check_eliminated: false,
            static_type: None,
            non_null: false,
        }
    }

    pub fn with_static_type(mut self, elem_type: ArrayElementType) -> Self {
        self.static_type = Some(elem_type);
        self
    }

    pub fn with_bounds_check_eliminated(mut self) -> Self {
        self.bounds_check_eliminated = true;
        self
    }
}

/// Baseline JIT array operations with specialization
pub struct BaselineJitArrayOps;

impl BaselineJitArrayOps {
    /// Specialized array access with inline type guard
    ///
    /// Generated code pattern:
    /// ```asm
    /// ; Type guard
    /// cmp [array + offset_kind], DYNAMIC
    /// jne deopt_label
    /// cmp [array + offset_elem_type], WORD
    /// jne deopt_label
    ///
    /// ; Bounds check (can be eliminated if provably safe)
    /// cmp index, [array + offset_len]
    /// jae bounds_error
    ///
    /// ; Access (specialized by element size)
    /// mov rax, [array + offset_data_ptr]
    /// mov eax, [rax + index*4]  ; *4 for i32
    /// ```
    pub fn jit_array_get_i32(
        array: &ArrayKind,
        index: usize,
        ctx: &JitArrayContext,
    ) -> Result<i32, String> {
        // Type guard: ensure we have the expected type
        if let Some(expected_type) = ctx.static_type {
            if array.element_type() != expected_type {
                return Err("Type guard failed: deoptimize".to_string());
            }
        }

        // Bounds check (can be eliminated by optimizer)
        if !ctx.bounds_check_eliminated {
            if index >= array.len() {
                return Err(format!(
                    "Bounds check failed: index {} >= length {}",
                    index,
                    array.len()
                ));
            }
        }

        // Specialized access based on array kind
        match array {
            ArrayKind::Dynamic(dyn_arr) => match &dyn_arr.data[index] {
                Value::I32(v) => Ok(*v),
                _ => Err("Element type mismatch".to_string()),
            },
            _ => Err("Unsupported array kind for specialized access".to_string()),
        }
    }

    /// Specialized f64 array access
    pub fn jit_array_get_f64(
        array: &ArrayKind,
        index: usize,
        ctx: &JitArrayContext,
    ) -> Result<f64, String> {
        // Type guard
        if let Some(expected_type) = ctx.static_type {
            if array.element_type() != expected_type {
                return Err("Type guard failed: deoptimize".to_string());
            }
        }

        // Bounds check
        if !ctx.bounds_check_eliminated {
            if index >= array.len() {
                return Err(format!(
                    "Bounds check failed: index {} >= length {}",
                    index,
                    array.len()
                ));
            }
        }

        match array {
            ArrayKind::Dynamic(dyn_arr) => match &dyn_arr.data[index] {
                Value::F64(v) => Ok(*v),
                _ => Err("Element type mismatch".to_string()),
            },
            _ => Err("Unsupported array kind".to_string()),
        }
    }

    /// Specialized u8 array access (common for strings/buffers)
    pub fn jit_array_get_u8(
        array: &ArrayKind,
        index: usize,
        ctx: &JitArrayContext,
    ) -> Result<u8, String> {
        if let Some(expected_type) = ctx.static_type {
            if array.element_type() != expected_type {
                return Err("Type guard failed: deoptimize".to_string());
            }
        }

        if !ctx.bounds_check_eliminated {
            if index >= array.len() {
                return Err(format!(
                    "Bounds check failed: index {} >= length {}",
                    index,
                    array.len()
                ));
            }
        }

        match array {
            ArrayKind::Dynamic(dyn_arr) => match &dyn_arr.data[index] {
                Value::U8(v) => Ok(*v),
                _ => Err("Element type mismatch".to_string()),
            },
            _ => Err("Unsupported array kind".to_string()),
        }
    }

    /// Specialized array sum for i32 (loop with type guards)
    ///
    /// Generated code:
    /// ```asm
    /// xor eax, eax           ; sum = 0
    /// xor ecx, ecx           ; i = 0
    /// mov r8, [array + len]  ; load length
    /// loop_start:
    ///   cmp rcx, r8
    ///   jae loop_end
    ///   mov rdx, [array + data_ptr]
    ///   add eax, [rdx + rcx*4]  ; sum += arr[i]
    ///   inc rcx
    ///   jmp loop_start
    /// loop_end:
    /// ```
    pub fn jit_array_sum_i32(array: &ArrayKind) -> Result<i32, String> {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                let mut sum = 0i32;
                for elem in &dyn_arr.data {
                    match elem {
                        Value::I32(v) => sum += v,
                        _ => return Err("Type mismatch in sum".to_string()),
                    }
                }
                Ok(sum)
            }
            _ => Err("Unsupported array kind".to_string()),
        }
    }

    /// Specialized array sum for f64
    pub fn jit_array_sum_f64(array: &ArrayKind) -> Result<f64, String> {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                let mut sum = 0.0f64;
                for elem in &dyn_arr.data {
                    match elem {
                        Value::F64(v) => sum += v,
                        _ => return Err("Type mismatch in sum".to_string()),
                    }
                }
                Ok(sum)
            }
            _ => Err("Unsupported array kind".to_string()),
        }
    }

    /// Specialized array map for i32 -> i32
    ///
    /// Generated code uses inline loop with type guards
    pub fn jit_array_map_i32<F>(array: &ArrayKind, f: F) -> Result<Vec<i32>, String>
    where
        F: Fn(i32) -> i32,
    {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                let mut result = Vec::with_capacity(dyn_arr.data.len());
                for elem in &dyn_arr.data {
                    match elem {
                        Value::I32(v) => result.push(f(*v)),
                        _ => return Err("Type mismatch in map".to_string()),
                    }
                }
                Ok(result)
            }
            _ => Err("Unsupported array kind".to_string()),
        }
    }

    /// Specialized array filter for i32
    pub fn jit_array_filter_i32<F>(array: &ArrayKind, predicate: F) -> Result<Vec<i32>, String>
    where
        F: Fn(i32) -> bool,
    {
        match array {
            ArrayKind::Dynamic(dyn_arr) => {
                let mut result = Vec::new();
                for elem in &dyn_arr.data {
                    match elem {
                        Value::I32(v) => {
                            if predicate(*v) {
                                result.push(*v);
                            }
                        }
                        _ => return Err("Type mismatch in filter".to_string()),
                    }
                }
                Ok(result)
            }
            _ => Err("Unsupported array kind".to_string()),
        }
    }
}

/// Inline cache for array operations
///
/// Stores recently seen array types to enable faster dispatch
pub struct ArrayInlineCache {
    /// Cached element type
    cached_type: Option<ArrayElementType>,
    /// Cached array kind
    _cached_kind: Option<u8>, // Discriminant
    /// Hit counter
    hits: u64,
    /// Miss counter
    misses: u64,
}

impl ArrayInlineCache {
    pub fn new() -> Self {
        ArrayInlineCache {
            cached_type: None,
            _cached_kind: None,
            hits: 0,
            misses: 0,
        }
    }

    /// Check if cached type matches
    pub fn check(&mut self, array: &ArrayKind) -> bool {
        let elem_type = array.element_type();

        if let Some(cached) = self.cached_type {
            if cached == elem_type {
                self.hits += 1;
                return true;
            }
        }

        // Cache miss: update cache
        self.misses += 1;
        self.cached_type = Some(elem_type);
        false
    }

    /// Get hit rate for profiling
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Branch prediction hints for bounds checking
#[inline(always)]
pub fn likely(b: bool) -> bool {
    // In real JIT, this would compile to branch prediction hints
    // e.g., __builtin_expect(b, 1) in LLVM
    b
}

#[inline(always)]
pub fn unlikely(b: bool) -> bool {
    // Compile to unlikely branch prediction
    // e.g., __builtin_expect(b, 0) in LLVM
    b
}

/// Optimized bounds checking with branch hints
pub fn checked_array_access<T>(data: &[T], index: usize, len: usize) -> Result<&T, String> {
    if likely(index < len) {
        // Fast path: index is in bounds (common case)
        Ok(&data[index])
    } else {
        // Slow path: out of bounds (rare case)
        Err(format!(
            "Index {} out of bounds for array of length {}",
            index, len
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::types::array_types::DynamicArray;

    use super::*;

    #[test]
    fn test_specialized_i32_access() {
        let data = vec![Value::I32(10), Value::I32(20), Value::I32(30)];
        let array = ArrayKind::Dynamic(DynamicArray::new(data));

        let ctx = JitArrayContext::new().with_static_type(ArrayElementType::Word);

        let val = BaselineJitArrayOps::jit_array_get_i32(&array, 1, &ctx).unwrap();
        assert_eq!(val, 20);
    }

    #[test]
    fn test_specialized_sum() {
        let data = vec![
            Value::I32(1),
            Value::I32(2),
            Value::I32(3),
            Value::I32(4),
            Value::I32(5),
        ];
        let array = ArrayKind::Dynamic(DynamicArray::new(data));

        let sum = BaselineJitArrayOps::jit_array_sum_i32(&array).unwrap();
        assert_eq!(sum, 15);
    }

    #[test]
    fn test_specialized_map() {
        let data = vec![Value::I32(1), Value::I32(2), Value::I32(3)];
        let array = ArrayKind::Dynamic(DynamicArray::new(data));

        let doubled = BaselineJitArrayOps::jit_array_map_i32(&array, |x| x * 2).unwrap();
        assert_eq!(doubled, vec![2, 4, 6]);
    }

    #[test]
    fn test_specialized_filter() {
        let data = vec![
            Value::I32(1),
            Value::I32(2),
            Value::I32(3),
            Value::I32(4),
            Value::I32(5),
        ];
        let array = ArrayKind::Dynamic(DynamicArray::new(data));

        let evens = BaselineJitArrayOps::jit_array_filter_i32(&array, |x| x % 2 == 0).unwrap();
        assert_eq!(evens, vec![2, 4]);
    }

    #[test]
    fn test_inline_cache() {
        let data = vec![Value::I32(1), Value::I32(2)];
        let array = ArrayKind::Dynamic(DynamicArray::new(data));

        let mut cache = ArrayInlineCache::new();

        // First access: cache miss
        assert!(!cache.check(&array));

        // Second access: cache hit
        assert!(cache.check(&array));
        assert!(cache.check(&array));

        // Hit rate should be 2/3
        assert_eq!(cache.hit_rate(), 2.0 / 3.0);
    }

    #[test]
    fn test_bounds_check_elimination() {
        let data = vec![Value::I32(10), Value::I32(20)];
        let array = ArrayKind::Dynamic(DynamicArray::new(data));

        let ctx = JitArrayContext::new()
            .with_static_type(ArrayElementType::Word)
            .with_bounds_check_eliminated();

        // Access with eliminated bounds check
        let val = BaselineJitArrayOps::jit_array_get_i32(&array, 1, &ctx).unwrap();
        assert_eq!(val, 20);
    }
}
