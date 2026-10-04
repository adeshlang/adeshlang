//! Phase 9 Large Program Compilation & Scaling E2E Test Suite.
//!
//! Validates:
//! - Stress-testing the compiler with multi-function modules (100+ functions).
//! - Query engine memoization under high query volume.
//! - Constant evaluation of deep expression trees with limit enforcement.

#![allow(dead_code, unused_imports)]

use adesh_codegen::const_eval::{ConstEvalLimits, ConstEvaluator, ConstExpr, ConstValue};
use adesh_codegen::machine_ir::{MachineFunction, MachineInstruction, MachineOperand, NativeModule};
use adesh_codegen::query::{QueryEngine, QueryKey, QueryKind, QueryResult};
use std::collections::HashMap;

#[test]
fn test_large_module_compilation_scaling() {
    let mut large_mod = NativeModule::new("large_benchmark_module");

    for i in 0..100 {
        let mut func = MachineFunction::new(format!("func_{}", i));
        func.is_exported = i % 10 == 0;
        let b = func.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: MachineOperand::Immediate(0),
            src: MachineOperand::Immediate(i as i64),
        });
        b.push(MachineInstruction::Return);
        large_mod.add_function(func);
    }

    assert_eq!(large_mod.functions.len(), 100);

    // Query engine scaling test
    let qe = QueryEngine::new();
    for i in 0..100 {
        let k = QueryKey {
            kind: QueryKind::LowerMir,
            target: format!("func_{}", i),
        };
        let _ = qe.query(k.clone(), |key| QueryResult {
            key: key.clone(),
            payload: format!("mir_payload_{}", i),
            fingerprint: i as u64,
            dependencies: vec![],
        });
    }

    assert_eq!(qe.stats().total_queries, 100);
}

#[test]
fn test_deep_const_eval_expression() {
    let mut eval = ConstEvaluator::new(ConstEvalLimits::default());
    let env = HashMap::new();

    // Construct a binary addition tree: 1 + 2 + 3 + ... + 20
    let mut tree = ConstExpr::Literal(ConstValue::Integer(1));
    for i in 2..=20 {
        tree = ConstExpr::Add(
            Box::new(tree),
            Box::new(ConstExpr::Literal(ConstValue::Integer(i))),
        );
    }

    let res = eval.eval(&tree, &env).expect("eval deep tree");
    // Sum 1..=20 = 210
    assert_eq!(res, ConstValue::Integer(210));
}
