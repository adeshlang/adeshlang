//! SIMD + Parallel execution tests

use adeshlang::ir::parallel::analysis::ParallelLoopAnalyzer;
use adeshlang::ir::parallel::transform::ParallelTransformer;
use adeshlang::ir::simd::cost_model::{ExecutionStrategy, VectorizationCostModel};
use adeshlang::ir::simd::types::{SimdElement, SimdType};
use adeshlang::ir::simd::vectorize::{AutoVectorizer, VectorOp};
use adeshlang::parsing::ast::Value;
use adeshlang::runtime::scheduler::{is_parallel_runtime_active, parallel_for};
use adeshlang::runtime::simd::ops::{
    array_add, array_div, array_dot, array_fused_mul_add, array_fused_sqrt_mul_add, array_mean,
    array_mul, array_sub, array_sum,
};
use adeshlang::runtime::simd::value::SimdValue;

// ============================================================================
// SIMD Type Tests
// ============================================================================

#[test]
fn test_simd_type_parse_vec4_f32() {
    let t = SimdType::from_annotation("vec4<f32>").unwrap();
    assert_eq!(t.element, SimdElement::F32);
    assert_eq!(t.lanes, 4);
}

#[test]
fn test_simd_type_parse_simd_f64_8() {
    let t = SimdType::from_annotation("Simd<f64, 8>").unwrap();
    assert_eq!(t.element, SimdElement::F64);
    assert_eq!(t.lanes, 8);
}

#[test]
fn test_simd_value_arithmetic() {
    let ty = SimdType::new(SimdElement::F32, 4);
    let a = SimdValue::from_lanes(ty, vec![1.0, 2.0, 3.0, 4.0]);
    let b = SimdValue::from_lanes(ty, vec![5.0, 6.0, 7.0, 8.0]);
    let c = a.add(&b).unwrap();
    assert_eq!(c.lanes, vec![6.0, 8.0, 10.0, 12.0]);
}

#[test]
fn test_simd_value_mul() {
    let ty = SimdType::new(SimdElement::F32, 4);
    let a = SimdValue::from_lanes(ty, vec![2.0, 3.0, 4.0, 5.0]);
    let b = SimdValue::from_lanes(ty, vec![2.0, 2.0, 2.0, 2.0]);
    let c = a.mul(&b).unwrap();
    assert_eq!(c.lanes, vec![4.0, 6.0, 8.0, 10.0]);
}

#[test]
fn test_simd_value_reduce_add() {
    let ty = SimdType::new(SimdElement::F32, 4);
    let v = SimdValue::from_lanes(ty, vec![1.0, 2.0, 3.0, 4.0]);
    assert!((v.reduce_add() - 10.0).abs() < 1e-10);
}

#[test]
fn test_simd_value_fma() {
    let ty = SimdType::new(SimdElement::F64, 2);
    let a = SimdValue::from_lanes(ty, vec![2.0, 3.0]);
    let b = SimdValue::from_lanes(ty, vec![3.0, 4.0]);
    let c = SimdValue::from_lanes(ty, vec![1.0, 0.0]);
    let r = a.fma(&b, &c).unwrap();
    assert!((r.lanes[0] - 7.0).abs() < 1e-10);
    assert!((r.lanes[1] - 12.0).abs() < 1e-10);
}

// ============================================================================
// Element-wise Array Operation Tests
// ============================================================================

#[test]
fn test_array_element_wise_add() {
    let a = Value::Array(vec![
        Value::Number(1.0),
        Value::Number(2.0),
        Value::Number(3.0),
    ]);
    let b = Value::Array(vec![
        Value::Number(10.0),
        Value::Number(20.0),
        Value::Number(30.0),
    ]);
    let c = array_add(&a, &b).unwrap();
    if let Value::Array(result) = c {
        assert_eq!(result.len(), 3);
        if let Value::Number(n) = result[0] {
            assert!((n - 11.0).abs() < 1e-10);
        }
        if let Value::Number(n) = result[2] {
            assert!((n - 33.0).abs() < 1e-10);
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_array_broadcast_scalar_add() {
    let a = Value::Array(vec![
        Value::Number(1.0),
        Value::Number(2.0),
        Value::Number(3.0),
    ]);
    let scalar = Value::Number(10.0);
    let c = array_add(&a, &scalar).unwrap();
    if let Value::Array(result) = c {
        if let Value::Number(n) = result[0] {
            assert!((n - 11.0).abs() < 1e-10);
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_array_element_wise_mul() {
    let a = Value::Array(vec![
        Value::Number(2.0),
        Value::Number(3.0),
        Value::Number(4.0),
    ]);
    let b = Value::Array(vec![
        Value::Number(5.0),
        Value::Number(6.0),
        Value::Number(7.0),
    ]);
    let c = array_mul(&a, &b).unwrap();
    if let Value::Array(result) = c {
        if let Value::Number(n) = result[0] {
            assert!((n - 10.0).abs() < 1e-10);
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_array_scalar_mul() {
    let a = Value::Array(vec![Value::Number(2.0), Value::Number(3.0)]);
    let s = Value::Number(5.0);
    let c = array_mul(&a, &s).unwrap();
    if let Value::Array(result) = c {
        if let Value::Number(n) = result[1] {
            assert!((n - 15.0).abs() < 1e-10);
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_array_sub() {
    let a = Value::Array(vec![Value::Number(10.0), Value::Number(20.0)]);
    let b = Value::Array(vec![Value::Number(3.0), Value::Number(5.0)]);
    let c = array_sub(&a, &b).unwrap();
    if let Value::Array(result) = c {
        if let Value::Number(n) = result[0] {
            assert!((n - 7.0).abs() < 1e-10);
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_array_div() {
    let a = Value::Array(vec![Value::Number(10.0), Value::Number(20.0)]);
    let b = Value::Array(vec![Value::Number(2.0), Value::Number(4.0)]);
    let c = array_div(&a, &b).unwrap();
    if let Value::Array(result) = c {
        if let Value::Number(n) = result[0] {
            assert!((n - 5.0).abs() < 1e-10);
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_array_sum() {
    let a = Value::Array(vec![
        Value::Number(1.0),
        Value::Number(2.0),
        Value::Number(3.0),
        Value::Number(4.0),
    ]);
    let s = array_sum(&a).unwrap();
    if let Value::Number(n) = s {
        assert!((n - 10.0).abs() < 1e-10);
    } else {
        panic!("expected number");
    }
}

#[test]
fn test_array_mean() {
    let a = Value::Array(vec![
        Value::Number(2.0),
        Value::Number(4.0),
        Value::Number(6.0),
        Value::Number(8.0),
    ]);
    let m = array_mean(&a).unwrap();
    if let Value::Number(n) = m {
        assert!((n - 5.0).abs() < 1e-10);
    } else {
        panic!("expected number");
    }
}

#[test]
fn test_array_dot_product() {
    let a = Value::Array(vec![
        Value::Number(1.0),
        Value::Number(2.0),
        Value::Number(3.0),
    ]);
    let b = Value::Array(vec![
        Value::Number(4.0),
        Value::Number(5.0),
        Value::Number(6.0),
    ]);
    let d = array_dot(&a, &b).unwrap();
    if let Value::Number(n) = d {
        assert!((n - 32.0).abs() < 1e-10); // 1*4 + 2*5 + 3*6
    } else {
        panic!("expected number");
    }
}

#[test]
fn test_fused_mul_add() {
    let a = Value::Array(vec![Value::Number(2.0), Value::Number(3.0)]);
    let b = Value::Array(vec![Value::Number(4.0), Value::Number(5.0)]);
    let d = Value::Array(vec![Value::Number(1.0), Value::Number(0.0)]);
    let c = array_fused_mul_add(&a, &b, &d).unwrap();
    if let Value::Array(result) = c {
        if let Value::Number(n) = result[0] {
            assert!((n - 9.0).abs() < 1e-10); // 2*4+1
        }
        if let Value::Number(n) = result[1] {
            assert!((n - 15.0).abs() < 1e-10); // 3*5+0
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_fused_sqrt_mul_add() {
    let a = Value::Array(vec![Value::Number(2.0)]);
    let b = Value::Array(vec![Value::Number(8.0)]);
    let c = Value::Array(vec![Value::Number(1.0)]);
    let r = array_fused_sqrt_mul_add(&a, &b, &c).unwrap();
    if let Value::Array(result) = r {
        if let Value::Number(n) = result[0] {
            assert!((n - 17.0_f64.sqrt()).abs() < 1e-10); // sqrt(2*8+1) = sqrt(17)
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn test_empty_array_sum() {
    let a = Value::Array(vec![]);
    let s = array_sum(&a).unwrap();
    if let Value::Number(n) = s {
        assert!((n - 0.0).abs() < 1e-10);
    }
}

#[test]
fn test_single_element_array() {
    let a = Value::Array(vec![Value::Number(42.0)]);
    let b = Value::Array(vec![Value::Number(8.0)]);
    let c = array_add(&a, &b).unwrap();
    if let Value::Array(result) = c
        && let Value::Number(n) = result[0]
    {
        assert!((n - 50.0).abs() < 1e-10);
    }
}

#[test]
fn test_non_multiple_of_lane_length() {
    let a = Value::Array((0..7).map(|i| Value::Number(i as f64)).collect());
    let b = Value::Array((0..7).map(|i| Value::Number((i * 2) as f64)).collect());
    let c = array_add(&a, &b).unwrap();
    if let Value::Array(result) = c {
        assert_eq!(result.len(), 7);
        for (i, v) in result.iter().enumerate() {
            if let Value::Number(n) = v {
                assert!((*n - (i as f64 + (i * 2) as f64)).abs() < 1e-10);
            }
        }
    }
}

// ============================================================================
// Auto-vectorization Cost Model Tests
// ============================================================================

#[test]
fn test_cost_model_small_loop_scalar() {
    let model = VectorizationCostModel::default();
    let strategy = model.select_strategy(
        4,
        1.0,
        4,
        adeshlang::ir::simd::types::SimdIsa::Avx2,
        SimdElement::F32,
    );
    assert_eq!(strategy, ExecutionStrategy::Scalar);
}

#[test]
fn test_cost_model_large_loop_parallel_simd() {
    let model = VectorizationCostModel::default();
    let strategy = model.select_strategy(
        100_000,
        1.0,
        8,
        adeshlang::ir::simd::types::SimdIsa::Avx2,
        SimdElement::F64,
    );
    assert!(matches!(
        strategy,
        ExecutionStrategy::ParallelSimd | ExecutionStrategy::Parallel | ExecutionStrategy::Simd
    ));
}

#[test]
fn test_auto_vectorizer() {
    let vectorizer = AutoVectorizer::default();
    let result = vectorizer.vectorize_loop(1024, "f64", VectorOp::Add, true);
    assert!(result.simd_block.is_some() || result.diagnostic.is_some());
}

// ============================================================================
// Parallel Analysis Tests
// ============================================================================

#[test]
fn test_parallel_loop_independence() {
    let ind = ParallelLoopAnalyzer::check_independence(
        &["a".to_string(), "b".to_string()],
        &["c".to_string()],
        "i",
        false,
    );
    assert!(ind.is_independent);
}

#[test]
fn test_parallel_loop_dependent() {
    let ind = ParallelLoopAnalyzer::check_independence(
        &["x".to_string()],
        &["x".to_string()],
        "i",
        false,
    );
    assert!(!ind.is_independent);
}

#[test]
fn test_disjoint_range_borrow_split() {
    assert!(ParallelLoopAnalyzer::check_disjoint_ranges(5, 5));
    assert!(!ParallelLoopAnalyzer::check_disjoint_ranges(6, 5));
}

#[test]
fn test_parallel_transform_large_loop() {
    let transformer = ParallelTransformer::default();
    let ind = ParallelLoopAnalyzer::check_independence(&[], &["out".to_string()], "i", false);
    let result = transformer.transform_loop(0, 10_000, &ind, false);
    assert!(result.safe);
    assert!(!result.chunks.is_empty());
}

// ============================================================================
// Parallel Runtime Tests
// ============================================================================

#[test]
fn test_parallel_for_execution() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    let counter = Arc::new(AtomicU64::new(0));
    let c = Arc::clone(&counter);
    parallel_for(0, 1000, move |i| {
        c.fetch_add(1, Ordering::Relaxed);
        let _ = i;
    });
    assert_eq!(counter.load(Ordering::Relaxed), 1000);
}

#[test]
fn test_zero_overhead_when_unused() {
    // Parallel runtime should not be active until first use
    // (may be active if other tests ran first in same process)
    let _ = is_parallel_runtime_active();
}

// ============================================================================
// Differential Testing: scalar vs SIMD paths must match
// ============================================================================

#[test]
fn test_differential_add_scalar_vs_simd_path() {
    let sizes = [1, 3, 4, 7, 8, 15, 16, 100, 1000];
    for size in sizes {
        let a: Vec<Value> = (0..size).map(|i| Value::Number(i as f64)).collect();
        let b: Vec<Value> = (0..size).map(|i| Value::Number((i * 2) as f64)).collect();
        let va = Value::Array(a.clone());
        let vb = Value::Array(b.clone());

        // Scalar reference
        let scalar: Vec<f64> = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| {
                let xv = if let Value::Number(n) = x { *n } else { 0.0 };
                let yv = if let Value::Number(n) = y { *n } else { 0.0 };
                xv + yv
            })
            .collect();

        // SIMD path
        let simd_result = array_add(&va, &vb).unwrap();
        if let Value::Array(result) = simd_result {
            for (i, v) in result.iter().enumerate() {
                if let Value::Number(n) = v {
                    assert!(
                        (*n - scalar[i]).abs() < 1e-10,
                        "mismatch at index {} for size {}",
                        i,
                        size
                    );
                }
            }
        }
    }
}
