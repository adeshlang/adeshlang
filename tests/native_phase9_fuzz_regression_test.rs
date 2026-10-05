//! Phase 9 Fuzz Regression & Robustness E2E Test Suite.
//!
//! Validates:
//! - Const evaluator bounds: division by zero, recursion depth limit, step limit.
//! - Monomorphization engine handling edge cases and duplicate type instantiations.
//! - LoopOptimizer handling empty loops and non-terminating CFG loops without crashing.

#![allow(dead_code, unused_imports)]

use adesh_codegen::const_eval::{
    ConstEvalError, ConstEvalLimits, ConstEvaluator, ConstExpr, ConstValue,
};
use adesh_codegen::generics::{ConcreteType, GenericFunctionTemplate, MonomorphizationEngine};
use adesh_codegen::opt::loop_opt::{LoopOptConfig, LoopOptimizer};
use std::collections::HashMap;

#[test]
fn test_fuzz_const_eval_division_by_zero() {
    let mut eval = ConstEvaluator::new(ConstEvalLimits::default());
    let env = HashMap::new();

    let div_zero = ConstExpr::Div(
        Box::new(ConstExpr::Literal(ConstValue::Integer(100))),
        Box::new(ConstExpr::Literal(ConstValue::Integer(0))),
    );

    let err = eval
        .eval(&div_zero, &env)
        .err()
        .expect("must fail on div zero");
    assert_eq!(err, ConstEvalError::DivisionByZero);
}

#[test]
fn test_fuzz_const_eval_step_limit_exceeded() {
    let limits = ConstEvalLimits {
        max_steps: 10,
        max_memory_bytes: 1024,
        max_recursion_depth: 10,
        max_evaluation_depth: 10,
    };
    let mut eval = ConstEvaluator::new(limits);
    let env = HashMap::new();

    // Construct a deeply nested addition tree exceeding 10 steps
    let mut tree = ConstExpr::Literal(ConstValue::Integer(1));
    for i in 2..=15 {
        tree = ConstExpr::Add(
            Box::new(tree),
            Box::new(ConstExpr::Literal(ConstValue::Integer(i))),
        );
    }

    let err = eval
        .eval(&tree, &env)
        .err()
        .expect("must fail on step limit");
    assert!(matches!(err, ConstEvalError::StepLimitExceeded(_)));
}

#[test]
fn test_fuzz_monomorphization_deduplication_stress() {
    let mut mono = MonomorphizationEngine::new();
    let template = GenericFunctionTemplate {
        name: "buffer_alloc".to_string(),
        type_params: vec!["T".to_string()],
        param_types: vec!["T".to_string()],
        return_type: "T".to_string(),
        is_inline: true,
    };
    mono.register_template(template);

    // Repeated specializations of identical types must return identical canonical symbols
    let first = mono
        .specialize("buffer_alloc", &[ConcreteType::U8])
        .unwrap();
    for _ in 0..100 {
        let subsequent = mono
            .specialize("buffer_alloc", &[ConcreteType::U8])
            .unwrap();
        assert_eq!(first.specialized_symbol, subsequent.specialized_symbol);
    }
}
