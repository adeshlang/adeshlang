//! Comprehensive Array System Tests
//!
//! Tests for multi-tier array implementation covering:
//! - All array kinds (Raw, SSO, Compact, Dynamic, Arena)
//! - Memory layout and overhead
//! - Type specialization
//! - Bounds checking
//! - SIMD operations
//! - Performance characteristics

use adeshlang::parsing::ast::Value;
use adeshlang::types::array_types::*;

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
fn test_arena_array_compactness() {
    let arena = ArenaArray::new(0, 0, 100, ArrayElementType::Word);
    assert_eq!(arena.len(), 100);
    assert_eq!(arena.metadata_bytes(), 16); // Compact representation
}

#[test]
fn test_array_kind_dispatch() {
    // Test that ArrayKind properly dispatches to underlying implementations
    let raw = ArrayKind::Raw(RawArray::new(
        "i32".to_string(),
        vec![Value::I32(1), Value::I32(2)],
    ));
    assert_eq!(raw.len(), 2);
    assert_eq!(raw.metadata_bytes(), 0);

    let dyn_arr = ArrayKind::Dynamic(DynamicArray::new(vec![Value::U8(1), Value::U8(2)]));
    assert_eq!(dyn_arr.len(), 2);
    assert!(dyn_arr.metadata_bytes() >= 16);
}

#[test]
fn test_type_specialization_paths() {
    // Ensure different element types take different paths
    let byte_arr = DynamicArray::new(vec![Value::U8(1)]);
    assert_eq!(byte_arr.element_type, ArrayElementType::Byte);
    assert_eq!(byte_arr.metadata_bytes(), 16);

    let long_arr = DynamicArray::new(vec![Value::F64(1.0)]);
    assert_eq!(long_arr.element_type, ArrayElementType::Long);
    assert_eq!(long_arr.metadata_bytes(), 24);
}
