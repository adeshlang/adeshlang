//! Comprehensive Array System Tests
//!
//! Tests for multi-tier array implementation covering:
//! - All array kinds (Raw, SSO, Compact, Dynamic, Arena)
//! - Memory layout and overhead
//! - Type specialization
//! - Bounds checking
//! - SIMD operations
//! - Performance characteristics

use crate::execution::array_ops::{ArrayOps, specialized, bounds_check, layout};
use crate::parsing::ast::Value;
use crate::types::array_types::*;

#[test]
fn test_array_element_type_sizes() {
    assert_eq!(ArrayElementType::Byte.element_size(), 1);
    assert_eq!(ArrayElementType::Short.element_size(), 2);
    assert_eq!(ArrayElementType::Word.element_size(), 4);
    assert_eq!(ArrayElementType::Long.element_size(), 8);
    assert_eq!(ArrayElementType::Extended.element_size(), 16);
}

#[test]
fn test_array_metadata_overhead() {
    // Byte/Short/Word types use u32 len/cap (4 bytes each)
    assert_eq!(ArrayElementType::Byte.total_metadata_bytes(), 16); // 8 (ptr) + 4 (len) + 4 (cap)
    assert_eq!(ArrayElementType::Short.total_metadata_bytes(), 16);
    assert_eq!(ArrayElementType::Word.total_metadata_bytes(), 16);

    // Long/Extended/Any types use usize len/cap (8 bytes each on 64-bit)
    assert_eq!(ArrayElementType::Long.total_metadata_bytes(), 24); // 8 (ptr) + 8 (len) + 8 (cap)
    assert_eq!(ArrayElementType::Extended.total_metadata_bytes(), 24);
    assert_eq!(ArrayElementType::Any.total_metadata_bytes(), 24);
}

#[test]
fn test_raw_array_zero_overhead() {
    let raw = RawArray::new("u8".to_string(), vec![]);
    assert_eq!(raw.metadata_bytes(), 0); // Zero overhead
}

#[test]
fn test_sso_array_inline() {
    // Small array should use inline storage
    let sso = SSOArray::new(ArrayElementType::Byte, vec![]);
    assert!(sso.is_inline());
    assert_eq!(sso.metadata_bytes(), 2); // discriminant + len
}

#[test]
fn test_sso_array_heap() {
    // Large array should use heap storage
    // 30 bytes exceeds 22-byte threshold
    let large_data: Vec<Value> = (0..30).map(|_| Value::U8(0)).collect();
    let sso = SSOArray::new(ArrayElementType::Byte, large_data);
    assert!(!sso.is_inline());
    assert_eq!(sso.metadata_bytes(), 16); // ptr + len + cap
}

#[test]
fn test_compact_array_limits() {
    // Compact array supports up to 65535 elements
    let valid = CompactArray::new(ArrayElementType::Word, vec![]);
    assert!(valid.is_some());

    // Test at boundary
    let at_max: Vec<Value> = (0..65535).map(|_| Value::I32(0)).collect();
    let compact_max = CompactArray::new(ArrayElementType::Word, at_max);
    assert!(compact_max.is_some());

    assert_eq!(compact_max.unwrap().metadata_bytes(), 16);
}

#[test]
fn test_dynamic_array_inference() {
    // Test automatic type inference
    let u8_array = DynamicArray::new(vec![Value::U8(1), Value::U8(2), Value::U8(3)]);
    assert_eq!(u8_array.element_type, ArrayElementType::Byte);
    assert_eq!(u8_array.concrete_type, "u8");

    let f64_array = DynamicArray::new(vec![Value::F64(1.0), Value::F64(2.0)]);
    assert_eq!(f64_array.element_type, ArrayElementType::Long);
    assert_eq!(f64_array.concrete_type, "f64");

    let i32_array = DynamicArray::new(vec![Value::I32(10), Value::I32(20)]);
    assert_eq!(i32_array.element_type, ArrayElementType::Word);
    assert_eq!(i32_array.concrete_type, "i32");
}

#[test]
fn test_array_allocator_strategy() {
    // Small arrays should use SSO
    let small = ArrayAllocator::allocate(ArrayElementType::Byte, 10, false);
    assert!(matches!(small, ArrayKind::SSO(_)));

    // Medium arrays should use Compact
    let medium = ArrayAllocator::allocate(ArrayElementType::Word, 1000, false);
    assert!(matches!(medium, ArrayKind::Compact(_)));

    // Fixed-size arrays should use Raw
    let fixed = ArrayAllocator::allocate(ArrayElementType::Byte, 10, true);
    assert!(matches!(fixed, ArrayKind::Raw(_)));

    // Large arrays should use Dynamic
    let large = ArrayAllocator::allocate(ArrayElementType::Long, 100000, false);
    assert!(matches!(large, ArrayKind::Dynamic(_)));
}

#[test]
fn test_specialized_numeric_sum() {
    // Test u8 sum
    let u8_data: Vec<u8> = vec![1, 2, 3, 4, 5];
    assert_eq!(specialized::sum_u8(&u8_data), 15);

    // Test f32 sum
    let f32_data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    assert_eq!(specialized::sum_f32(&f32_data), 15.0);

    // Test f64 sum
    let f64_data: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    assert_eq!(specialized::sum_f64(&f64_data), 15.0);
}

#[test]
fn test_specialized_dot_product() {
    let a = vec![1.0f32, 2.0, 3.0];
    let b = vec![4.0f32, 5.0, 6.0];

    let result = specialized::dot_product_f32(&a, &b).unwrap();
    assert_eq!(result, 32.0); // 1*4 + 2*5 + 3*6 = 32

    // Test mismatched lengths
    let c = vec![1.0f32, 2.0];
    let error = specialized::dot_product_f32(&a, &c);
    assert!(error.is_err());
}

#[test]
fn test_specialized_matvec() {
    let matrix = vec![
        vec![1.0f32, 2.0, 3.0],
        vec![4.0, 5.0, 6.0],
        vec![7.0, 8.0, 9.0],
    ];
    let vector = vec![1.0f32, 1.0, 1.0];

    let result = specialized::matvec_f32(&matrix, &vector).unwrap();
    assert_eq!(result, vec![6.0, 15.0, 24.0]);
}

#[test]
fn test_bounds_check_elimination() {
    // Static bounds checks
    assert!(bounds_check::is_statically_safe(5, 10));
    assert!(!bounds_check::is_statically_safe(10, 10));
    assert!(!bounds_check::is_statically_safe(11, 10));

    // Loop bounds
    assert!(bounds_check::loop_bounds_safe(0, 10, 10));
    assert!(bounds_check::loop_bounds_safe(5, 10, 15));
    assert!(!bounds_check::loop_bounds_safe(0, 20, 10));
}

#[test]
fn test_memory_layout_analysis() {
    // SSO threshold
    assert!(layout::can_use_sso(ArrayElementType::Byte, 22));
    assert!(!layout::can_use_sso(ArrayElementType::Byte, 23));

    assert!(layout::can_use_sso(ArrayElementType::Word, 5)); // 5*4 = 20 bytes
    assert!(!layout::can_use_sso(ArrayElementType::Word, 6)); // 6*4 = 24 bytes

    // Recommendations
    assert_eq!(layout::recommend_kind(ArrayElementType::Byte, 10, false), "SSOArray");
    assert_eq!(layout::recommend_kind(ArrayElementType::Word, 100, false), "CompactArray");
    assert_eq!(layout::recommend_kind(ArrayElementType::Long, 100000, false), "DynamicArray");
    assert_eq!(layout::recommend_kind(ArrayElementType::Byte, 10, true), "RawArray");
}

#[test]
fn test_arena_array_compactness() {
    let arena = ArenaArray::new(0, 0, 100, ArrayElementType::Word);
    assert_eq!(arena.len(), 100);
    assert_eq!(arena.metadata_bytes(), 16); // Compact representation
}

#[test]
fn test_array_kind_dispatch() {
    // Test that ArrayKind properly dispatches to underlying implementations
    let raw = ArrayKind::Raw(RawArray::new("i32".to_string(), vec![Value::I32(1), Value::I32(2)]));
    assert_eq!(raw.len(), 2);
    assert_eq!(raw.metadata_bytes(), 0);

    let dyn_arr = ArrayKind::Dynamic(DynamicArray::new(vec![Value::U8(1), Value::U8(2)]));
    assert_eq!(dyn_arr.len(), 2);
    assert!(dyn_arr.metadata_bytes() >= 16);
}

#[test]
fn test_cache_friendliness() {
    // Create small array that fits in L1 cache
    let small_data: Vec<Value> = (0..100).map(|i| Value::I32(i)).collect();
    let small = ArrayKind::Dynamic(DynamicArray::new(small_data));
    assert!(layout::fits_in_l1_cache(&small));

    // Create large array that doesn't fit in L1 cache (32KB)
    let large_data: Vec<Value> = (0..10000).map(|i| Value::I32(i)).collect();
    let large = ArrayKind::Dynamic(DynamicArray::new(large_data));
    assert!(!layout::fits_in_l1_cache(&large));
}

#[test]
fn test_type_specialization_paths() {
    // Ensure different element types take different paths
    let byte_arr = DynamicArray::new(vec![Value::U8(1)]);
    assert_eq!(byte_arr.element_type, ArrayElementType::Byte);
    assert_eq!(byte_arr.metadata_bytes(), 16 + byte_arr.concrete_type.len());

    let long_arr = DynamicArray::new(vec![Value::F64(1.0)]);
    assert_eq!(long_arr.element_type, ArrayElementType::Long);
    assert_eq!(long_arr.metadata_bytes(), 24 + long_arr.concrete_type.len());
}

/// Performance regression tests
#[cfg(test)]
mod perf_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bench_array_access_patterns() {
        const SIZE: usize = 10000;

        // Create arrays
        let data: Vec<Value> = (0..SIZE).map(|i| Value::I32(i as i32)).collect();
        let dyn_arr = ArrayKind::Dynamic(DynamicArray::new(data));

        // Sequential access (should be fast)
        let start = Instant::now();
        let mut sum = 0i32;
        for i in 0..SIZE {
            if let Ok(Value::I32(v)) = ArrayOps::get(&dyn_arr, i) {
                sum += v;
            }
        }
        let sequential_time = start.elapsed();

        println!("Sequential access: {:?}, sum={}", sequential_time, sum);
        assert_eq!(sum, (SIZE * (SIZE - 1) / 2) as i32);
    }

    #[test]
    fn bench_specialized_vs_generic() {
        const SIZE: usize = 10000;

        // Specialized f32 sum
        let f32_data: Vec<f32> = (0..SIZE).map(|i| i as f32).collect();

        let start = Instant::now();
        let specialized_sum = specialized::sum_f32(&f32_data);
        let specialized_time = start.elapsed();

        println!("Specialized f32 sum: {:?}, result={}", specialized_time, specialized_sum);

        // Ensure correctness
        let expected: f32 = (0..SIZE).map(|i| i as f32).sum();
        assert_eq!(specialized_sum, expected);
    }
}
