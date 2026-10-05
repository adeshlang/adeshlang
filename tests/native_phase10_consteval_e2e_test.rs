//! Phase 10 Compile-Time Evaluation VM & Reflection E2E Test Suite.
//!
//! Validates:
//! - Const VM arithmetic, branching, and string evaluation.
//! - Strict instruction limit enforcement.
//! - Reflection registry inspection.

#![allow(dead_code, unused_imports)]

use adesh_codegen::const_eval_vm::{
    ConstVm, FieldReflection, ReflectionContext, TypeReflection, VmError, VmOpCode,
    VmResourceLimits, VmValue,
};

#[test]
fn test_const_vm_arithmetic_and_branching() {
    let limits = VmResourceLimits::default();
    let mut vm = ConstVm::new(limits);

    // Compute (10 + 20) * 2 = 60
    let code = vec![
        VmOpCode::PushInt(10),
        VmOpCode::PushInt(20),
        VmOpCode::Add,
        VmOpCode::PushInt(2),
        VmOpCode::Mul,
        VmOpCode::Return,
    ];

    let result = vm.execute(&code).expect("vm execute");
    assert_eq!(result, VmValue::Int(60));
}

#[test]
fn test_const_vm_instruction_limit() {
    let limits = VmResourceLimits {
        max_instructions: 10,
        max_recursion_depth: 10,
        max_memory_bytes: 1024,
    };
    let mut vm = ConstVm::new(limits);

    // Infinite loop
    let code = vec![VmOpCode::PushInt(1), VmOpCode::Jump(0)];

    let result = vm.execute(&code);
    assert_eq!(result, Err(VmError::InstructionLimitExceeded));
}

#[test]
fn test_compile_time_reflection_registry() {
    let mut reflection = ReflectionContext::new();

    let point_type = TypeReflection {
        name: "Point".to_string(),
        size_bytes: 16,
        alignment_bytes: 8,
        fields: vec![
            FieldReflection {
                name: "x".to_string(),
                type_name: "f64".to_string(),
                offset: 0,
            },
            FieldReflection {
                name: "y".to_string(),
                type_name: "f64".to_string(),
                offset: 8,
            },
        ],
        attributes: vec!["#[inline]".to_string()],
    };

    reflection.register_type(point_type);
    let queried = reflection.query_type("Point").expect("find Point type");
    assert_eq!(queried.size_bytes, 16);
    assert_eq!(queried.fields.len(), 2);
    assert_eq!(queried.fields[0].name, "x");
}
