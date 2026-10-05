//! Phase 10 Production Generics & Monomorphization E2E Test Suite.
//!
//! Validates:
//! - Parametric polymorphic function specialization.
//! - Deduplication for identical concrete types.
//! - Enforcing instantiation depth limits to prevent code explosion.

#![allow(dead_code, unused_imports)]

use adesh_codegen::generics_v2::{
    ConcreteType, GenericError, GenericFunctionDef, GenericLimits, SpecializationEngineV2,
};
use std::collections::HashMap;

#[test]
fn test_generic_monomorphization_and_deduplication() {
    let mut limits = GenericLimits::default();
    limits.max_specializations_per_function = 10;
    let mut engine = SpecializationEngineV2::new(limits);

    let func = GenericFunctionDef {
        name: "max".to_string(),
        type_params: vec!["T".to_string()],
        constraints: HashMap::new(),
        return_type: "T".to_string(),
    };

    let spec_i32 = engine
        .specialize(&func, vec![ConcreteType::I32], 1)
        .expect("specialize max<i32>");
    assert_eq!(spec_i32.specialized_name, "max__I32");

    let spec_f64 = engine
        .specialize(&func, vec![ConcreteType::F64], 1)
        .expect("specialize max<f64>");
    assert_eq!(spec_f64.specialized_name, "max__F64");

    // Deduplication check: repeated specialization for i32 returns existing instance
    let spec_i32_dup = engine
        .specialize(&func, vec![ConcreteType::I32], 1)
        .expect("deduplicate max<i32>");
    assert_eq!(spec_i32.specialized_name, spec_i32_dup.specialized_name);
    assert_eq!(engine.total_specializations(), 2);
}

#[test]
fn test_generic_depth_limit_enforcement() {
    let limits = GenericLimits {
        max_instantiation_depth: 4,
        max_specializations_per_function: 16,
        max_total_specializations: 64,
    };
    let mut engine = SpecializationEngineV2::new(limits);

    let func = GenericFunctionDef {
        name: "recursive_gen".to_string(),
        type_params: vec!["T".to_string()],
        constraints: HashMap::new(),
        return_type: "T".to_string(),
    };

    let result = engine.specialize(&func, vec![ConcreteType::I64], 5);
    match result {
        Err(GenericError::RecursionLimitExceeded(name, depth)) => {
            assert_eq!(name, "recursive_gen");
            assert_eq!(depth, 5);
        }
        _ => panic!("Expected RecursionLimitExceeded error"),
    }
}
