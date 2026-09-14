//! Phase 7 Integration Tests
//!
//! Comprehensive tests for Phase 7 implementation:
//! - 7.1: Interpreter Execution Engine
//! - 7.2: Bytecode VM
//! - 7.3: LLVM Backend
//! - 7.4: Cross-backend validation

#[cfg(test)]
mod tests {
    use adeshlang::backends::llvm::{LLVMBackendConfig, LLVMCodegen};
    use adeshlang::backends::lowering::{
        BytecodeInstr, BytecodeVM, InterpreterExecutor, InterpreterValue,
    };

    /// Phase 7.1: Interpreter Execution Integration
    #[test]
    fn phase_7_1_interpreter_basic_arithmetic() {
        let executor = InterpreterExecutor::new();

        {
            let mut ctx = executor.context_mut();
            ctx.set_value(0, InterpreterValue::Int(10));
            ctx.set_value(1, InterpreterValue::Int(5));
        }

        let result = executor.exec_int_binop("add", 10, 5);
        assert_eq!(result, 15);

        let result = executor.exec_int_binop("mul", 10, 5);
        assert_eq!(result, 50);
    }

    /// Phase 7.1: Interpreter with function calls
    #[test]
    fn phase_7_1_interpreter_with_scope() {
        let executor = InterpreterExecutor::new();

        {
            let mut ctx = executor.context_mut();
            ctx.set_value(0, InterpreterValue::Int(42));
            ctx.push_scope();
            ctx.set_value(1, InterpreterValue::Int(99));
        }

        {
            let ctx = executor.context();
            assert_eq!(ctx.get_value(1), Some(InterpreterValue::Int(99)));
        }

        {
            let mut ctx = executor.context_mut();
            ctx.pop_scope();
        }

        {
            let ctx = executor.context();
            assert_eq!(ctx.get_value(0), Some(InterpreterValue::Int(42)));
        }
    }

    /// Phase 7.1: Interpreter exception handling
    #[test]
    fn phase_7_1_interpreter_exceptions() {
        let executor = InterpreterExecutor::new();

        {
            let mut ctx = executor.context_mut();
            assert!(!ctx.has_exception());

            ctx.set_exception("Division by zero".to_string());
            assert!(ctx.has_exception());

            let exc = ctx.get_exception();
            assert_eq!(exc, Some("Division by zero".to_string()));

            ctx.clear_exception();
            assert!(!ctx.has_exception());
        }
    }

    /// Phase 7.2: Bytecode VM Basic Operations
    #[test]
    fn phase_7_2_bytecode_vm_arithmetic() {
        let instrs = vec![
            BytecodeInstr::PushInt(100),
            BytecodeInstr::PushInt(50),
            BytecodeInstr::AddInt,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().expect("VM execution failed");
        assert_eq!(result.as_int(), 150);
    }

    /// Phase 7.2: Bytecode VM with complex expressions
    #[test]
    fn phase_7_2_bytecode_vm_complex() {
        // (5 + 3) * 2
        let instrs = vec![
            BytecodeInstr::PushInt(5),
            BytecodeInstr::PushInt(3),
            BytecodeInstr::AddInt,
            BytecodeInstr::PushInt(2),
            BytecodeInstr::MulInt,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().expect("VM execution failed");
        assert_eq!(result.as_int(), 16);
    }

    /// Phase 7.2: Bytecode VM with comparisons
    #[test]
    fn phase_7_2_bytecode_vm_comparison() {
        let instrs = vec![
            BytecodeInstr::PushInt(10),
            BytecodeInstr::PushInt(5),
            BytecodeInstr::GtInt,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().expect("VM execution failed");
        assert!(result.as_bool());
    }

    /// Phase 7.2: Bytecode VM with arrays
    #[test]
    fn phase_7_2_bytecode_vm_arrays() {
        let instrs = vec![
            BytecodeInstr::PushInt(5),   // array length
            BytecodeInstr::ArrayNew,     // create array
            BytecodeInstr::Dup,          // duplicate array ref
            BytecodeInstr::PushInt(2),   // index
            BytecodeInstr::PushInt(777), // value
            BytecodeInstr::ArraySet,     // set element
            BytecodeInstr::PushInt(2),   // index to get
            BytecodeInstr::ArrayGet,     // get element
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().expect("VM execution failed");
        assert_eq!(result.as_int(), 777);
    }

    /// Phase 7.2: Bytecode VM with locals
    #[test]
    fn phase_7_2_bytecode_vm_locals() {
        let instrs = vec![
            BytecodeInstr::PushInt(123),
            BytecodeInstr::SetLocal(0),
            BytecodeInstr::GetLocal(0),
            BytecodeInstr::PushInt(77),
            BytecodeInstr::AddInt,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().expect("VM execution failed");
        assert_eq!(result.as_int(), 200);
    }

    /// Phase 7.3: LLVM Backend Basic IR Generation
    #[test]
    fn phase_7_3_llvm_backend_ir_generation() {
        let config = LLVMBackendConfig::default();
        let codegen = LLVMCodegen::new("test_module".to_string(), config);

        let ir = codegen.generate_ir();
        assert!(ir.contains("test_module"));
        assert!(ir.contains("x86_64-unknown-linux-gnu"));
    }

    /// Phase 7.3: LLVM Configuration Testing
    #[test]
    fn phase_7_3_llvm_backend_config() {
        let config = LLVMBackendConfig {
            opt_level: 3,
            target_triple: "x86_64-apple-darwin".to_string(),
            enable_lto: true,
            enable_simd: true,
        };

        assert_eq!(config.opt_level, 3);
        assert!(config.enable_lto);
        assert!(config.enable_simd);
    }

    /// Cross-backend: Interpreter vs Bytecode consistency
    #[test]
    fn cross_backend_interpreter_vs_bytecode() {
        // Test the same computation on both backends
        let interpreter = InterpreterExecutor::new();

        let interp_result = interpreter.exec_int_binop("add", 42, 8);
        assert_eq!(interp_result, 50);

        let bytecode_instrs = vec![
            BytecodeInstr::PushInt(42),
            BytecodeInstr::PushInt(8),
            BytecodeInstr::AddInt,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(bytecode_instrs);
        let bytecode_result = vm.run().expect("Bytecode failed");

        assert_eq!(interp_result, bytecode_result.as_int());
    }

    /// Cross-backend: Floating point operations
    #[test]
    fn cross_backend_float_operations() {
        let interpreter = InterpreterExecutor::new();

        let interp_result = interpreter.exec_float_binop("add", 3.14, 2.86);
        assert!((interp_result - 6.0).abs() < 0.01);

        let bytecode_instrs = vec![
            BytecodeInstr::PushFloat(3.14),
            BytecodeInstr::PushFloat(2.86),
            BytecodeInstr::AddFloat,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(bytecode_instrs);
        let bytecode_result = vm.run().expect("Bytecode failed");

        assert!((bytecode_result.as_float() - 6.0).abs() < 0.01);
    }

    /// Cross-backend: Type conversions
    #[test]
    fn cross_backend_type_conversions() {
        let interpreter = InterpreterExecutor::new();

        let val = InterpreterValue::Int(42);
        let casted = interpreter.exec_cast(&val, "string");
        assert_eq!(casted.as_string(), "42");

        let bytecode_instrs = vec![
            BytecodeInstr::PushInt(42),
            BytecodeInstr::CastToString,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(bytecode_instrs);
        let bytecode_result = vm.run().expect("Bytecode failed");
        assert_eq!(bytecode_result.as_string(), "42");
    }

    /// Performance: Bytecode VM with step limit
    #[test]
    fn phase_7_2_bytecode_vm_step_limit() {
        let instrs = vec![
            BytecodeInstr::PushInt(1),
            BytecodeInstr::PushInt(1),
            BytecodeInstr::AddInt,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run_steps(100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_int(), 2);
    }

    /// Stress test: Interpreter memory allocation
    #[test]
    fn stress_interpreter_memory() {
        let executor = InterpreterExecutor::new();

        let mut ctx = executor.context_mut();
        for i in 0..1000 {
            ctx.set_value(i, InterpreterValue::Int(i as i64));
        }

        for i in 0..1000 {
            let val = ctx.get_value(i);
            assert_eq!(val, Some(InterpreterValue::Int(i as i64)));
        }
    }

    /// Stress test: Bytecode VM deep stack
    #[test]
    fn stress_bytecode_deep_stack() {
        let mut instrs = vec![];

        // Push 100 values
        for i in 0..100 {
            instrs.push(BytecodeInstr::PushInt(i));
        }

        // Sum them all (using repeated AddInt)
        for _ in 1..100 {
            instrs.push(BytecodeInstr::AddInt);
        }

        instrs.push(BytecodeInstr::Halt);

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run();
        assert!(result.is_ok());

        // Sum of 0..99 = 99*100/2 = 4950
        let expected: i64 = (0..100).sum();
        assert_eq!(result.unwrap().as_int(), expected);
    }

    /// Regression: Exception in catch block
    #[test]
    fn regression_exception_in_catch() {
        let instrs = vec![
            BytecodeInstr::Try,
            BytecodeInstr::PushInt(5),
            BytecodeInstr::Throw("error".to_string()),
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run();
        assert!(result.is_err());
    }

    /// Integration: Multi-type stack operations
    #[test]
    fn integration_multi_type_stack() {
        let instrs = vec![
            BytecodeInstr::PushInt(42),
            BytecodeInstr::CastToFloat,
            BytecodeInstr::PushFloat(0.5),
            BytecodeInstr::MulFloat,
            BytecodeInstr::CastToInt,
            BytecodeInstr::Halt,
        ];

        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().expect("VM execution failed");
        // 42.0 * 0.5 = 21.0, cast to int = 21
        assert_eq!(result.as_int(), 21);
    }

    /// Integration: Nested scopes
    #[test]
    fn integration_nested_scopes() {
        let executor = InterpreterExecutor::new();

        {
            let mut ctx = executor.context_mut();
            ctx.set_value(0, InterpreterValue::Int(1));

            // Push level 1
            ctx.push_scope();
            ctx.set_value(1, InterpreterValue::Int(2));

            // Push level 2
            ctx.push_scope();
            ctx.set_value(2, InterpreterValue::Int(3));

            // Pop level 2
            ctx.pop_scope();

            // Should have level 1 restored
            assert_eq!(ctx.get_value(2), None);
            assert_eq!(ctx.get_value(1), Some(InterpreterValue::Int(2)));

            // Pop level 1
            ctx.pop_scope();

            // Should have base level restored
            assert_eq!(ctx.get_value(1), None);
            assert_eq!(ctx.get_value(0), Some(InterpreterValue::Int(1)));
        }
    }
}
