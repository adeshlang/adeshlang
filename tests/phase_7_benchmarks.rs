//! Backend Performance Benchmarks - Phase 7
//!
//! Real-world performance testing across execution backends

#[cfg(test)]
mod backend_performance_benchmarks {
    use adeshlang::backends::lowering::*;
    use std::time::{Duration, Instant};

    /// Results collector for benchmarks
    #[derive(Debug, Clone)]
    struct BenchResult {
        name: String,
        duration_ms: f64,
        throughput_ops_per_sec: f64,
    }

    impl BenchResult {
        fn new(name: &str, duration: Duration, ops: usize) -> Self {
            let duration_ms = duration.as_secs_f64() * 1000.0;
            let throughput = ops as f64 / duration.as_secs_f64();
            Self {
                name: name.to_string(),
                duration_ms,
                throughput_ops_per_sec: throughput,
            }
        }

        fn print(&self) {
            println!(
                "  {:<40} {:8.2} ms   {:12.0} ops/sec",
                self.name, self.duration_ms, self.throughput_ops_per_sec
            );
        }
    }

    /// Benchmark 1: Interpreter basic arithmetic
    #[test]
    fn bench_interpreter_arithmetic() {
        println!("\n=== Interpreter Arithmetic Performance ===");

        let executor = InterpreterExecutor::new();

        let start = Instant::now();
        for _ in 0..100000 {
            let _ = executor.exec_int_binop("add", 42, 8);
        }
        let result = BenchResult::new("Integer Add (100k ops)", start.elapsed(), 100000);
        result.print();

        let start = Instant::now();
        for _ in 0..100000 {
            let _ = executor.exec_int_binop("mul", 7, 13);
        }
        let result = BenchResult::new("Integer Mul (100k ops)", start.elapsed(), 100000);
        result.print();

        let start = Instant::now();
        for _ in 0..100000 {
            let _ = executor.exec_int_binop("sub", 100, 50);
        }
        let result = BenchResult::new("Integer Sub (100k ops)", start.elapsed(), 100000);
        result.print();
    }

    /// Benchmark 2: Bytecode VM arithmetic
    #[test]
    fn bench_bytecode_arithmetic() {
        println!("\n=== Bytecode VM Arithmetic Performance ===");

        let start = Instant::now();
        for _ in 0..100000 {
            let instrs = vec![
                BytecodeInstr::PushInt(42),
                BytecodeInstr::PushInt(8),
                BytecodeInstr::AddInt,
                BytecodeInstr::Halt,
            ];
            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("VM Add (100k ops)", start.elapsed(), 100000);
        result.print();

        let start = Instant::now();
        for _ in 0..100000 {
            let instrs = vec![
                BytecodeInstr::PushInt(7),
                BytecodeInstr::PushInt(13),
                BytecodeInstr::MulInt,
                BytecodeInstr::Halt,
            ];
            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("VM Mul (100k ops)", start.elapsed(), 100000);
        result.print();
    }

    /// Benchmark 3: Complex arithmetic expression
    #[test]
    fn bench_complex_expressions() {
        println!("\n=== Complex Expression Evaluation ===");

        // (a + b) * (c - d) / e
        let start = Instant::now();
        for _ in 0..50000 {
            let instrs = vec![
                BytecodeInstr::PushInt(10),
                BytecodeInstr::PushInt(5),
                BytecodeInstr::AddInt,
                BytecodeInstr::PushInt(20),
                BytecodeInstr::PushInt(7),
                BytecodeInstr::SubInt,
                BytecodeInstr::MulInt,
                BytecodeInstr::PushInt(3),
                BytecodeInstr::DivInt,
                BytecodeInstr::Halt,
            ];
            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("Complex Expr (50k ops)", start.elapsed(), 50000);
        result.print();
    }

    /// Benchmark 4: Array operations
    #[test]
    fn bench_array_operations() {
        println!("\n=== Array Operation Performance ===");

        let start = Instant::now();
        for _ in 0..10000 {
            let instrs = vec![
                BytecodeInstr::PushInt(100),
                BytecodeInstr::ArrayNew,
                BytecodeInstr::Dup,
                BytecodeInstr::PushInt(50),
                BytecodeInstr::PushInt(777),
                BytecodeInstr::ArraySet,
                BytecodeInstr::PushInt(50),
                BytecodeInstr::ArrayGet,
                BytecodeInstr::Halt,
            ];
            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("Array Set/Get (10k ops)", start.elapsed(), 10000);
        result.print();
    }

    /// Benchmark 5: Local variable operations
    #[test]
    fn bench_local_variables() {
        println!("\n=== Local Variable Performance ===");

        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for _ in 0..50000 {
            let mut ctx = executor.context_mut();
            ctx.push_scope();
            ctx.set_value(0, InterpreterValue::Int(42));
            ctx.set_value(1, InterpreterValue::Int(99));
            ctx.pop_scope();
        }
        let result = BenchResult::new("Scope Push/Pop (50k ops)", start.elapsed(), 50000);
        result.print();
    }

    /// Benchmark 6: Comparisons
    #[test]
    fn bench_comparisons() {
        println!("\n=== Comparison Operations ===");

        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for _ in 0..100000 {
            let _ = executor.exec_int_binop("cmp_lt", 10, 20);
        }
        let result = BenchResult::new("Integer Compare (100k)", start.elapsed(), 100000);
        result.print();

        let start = Instant::now();
        for _ in 0..100000 {
            let instrs = vec![
                BytecodeInstr::PushInt(10),
                BytecodeInstr::PushInt(20),
                BytecodeInstr::LtInt,
                BytecodeInstr::Halt,
            ];
            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("VM Compare (100k)", start.elapsed(), 100000);
        result.print();
    }

    /// Benchmark 7: Type casting
    #[test]
    fn bench_type_casts() {
        println!("\n=== Type Casting Performance ===");

        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for _ in 0..100000 {
            let val = InterpreterValue::Int(42);
            let _ = executor.exec_cast(&val, "float");
        }
        let result = BenchResult::new("Int→Float Cast (100k)", start.elapsed(), 100000);
        result.print();

        let start = Instant::now();
        for _ in 0..100000 {
            let instrs = vec![
                BytecodeInstr::PushInt(42),
                BytecodeInstr::CastToFloat,
                BytecodeInstr::Halt,
            ];
            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("VM Int→Float (100k)", start.elapsed(), 100000);
        result.print();
    }

    /// Benchmark 8: Nested operations
    #[test]
    fn bench_nested_operations() {
        println!("\n=== Nested Operation Performance ===");

        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for _ in 0..10000 {
            let mut ctx = executor.context_mut();
            for depth in 0..10 {
                ctx.push_scope();
                ctx.set_value(depth, InterpreterValue::Int(depth as i64));
            }
            for _ in 0..10 {
                ctx.pop_scope();
            }
        }
        let result = BenchResult::new("Deep Nesting (100k scopes)", start.elapsed(), 100000);
        result.print();
    }

    /// Benchmark 9: Memory allocation
    #[test]
    fn bench_memory_allocation() {
        println!("\n=== Memory Allocation Performance ===");

        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for i in 0..10000 {
            let mut ctx = executor.context_mut();
            for j in 0..10 {
                ctx.set_value(
                    i as u32 * 10 + j,
                    InterpreterValue::Array(vec![
                        InterpreterValue::Int(1),
                        InterpreterValue::Int(2),
                    ]),
                );
            }
        }
        let result = BenchResult::new("Allocate Arrays (100k)", start.elapsed(), 100000);
        result.print();
    }

    /// Benchmark 10: ARC operations
    #[test]
    fn bench_arc_operations() {
        println!("\n=== ARC Operations Performance ===");

        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for _ in 0..50000 {
            let mut ctx = executor.context_mut();
            let val =
                InterpreterValue::Array(vec![InterpreterValue::Int(1), InterpreterValue::Int(2)]);
            ctx.set_value(0, val.clone());
            // Simulating reference operations
            ctx.set_value(1, val);
        }
        let result = BenchResult::new("ARC Clone/Set (50k ops)", start.elapsed(), 50000);
        result.print();
    }

    /// Benchmark 11: Stack depth stress
    #[test]
    fn bench_stack_depth() {
        println!("\n=== Stack Depth Stress Test ===");

        let start = Instant::now();
        for _ in 0..1000 {
            let mut instrs = vec![];
            // Push 100 values
            for i in 0..100 {
                instrs.push(BytecodeInstr::PushInt(i));
            }
            // Pop all but one
            for _ in 1..100 {
                instrs.push(BytecodeInstr::AddInt);
            }
            instrs.push(BytecodeInstr::Halt);

            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("Stack Depth (100k items)", start.elapsed(), 100000);
        result.print();
    }

    /// Benchmark 12: Mixed operations workload
    #[test]
    fn bench_mixed_workload() {
        println!("\n=== Mixed Workload Performance ===");

        let start = Instant::now();
        for _ in 0..10000 {
            let instrs = vec![
                // Arithmetic
                BytecodeInstr::PushInt(10),
                BytecodeInstr::PushInt(20),
                BytecodeInstr::AddInt,
                // Comparison
                BytecodeInstr::PushInt(30),
                BytecodeInstr::LtInt,
                // Array
                BytecodeInstr::PushInt(5),
                BytecodeInstr::ArrayNew,
                // Type cast
                BytecodeInstr::PushInt(42),
                BytecodeInstr::CastToFloat,
                BytecodeInstr::Halt,
            ];
            let mut vm = BytecodeVM::new(instrs);
            let _ = vm.run();
        }
        let result = BenchResult::new("Mixed Workload (10k cycles)", start.elapsed(), 10000);
        result.print();
    }

    /// Benchmark 13: Branch prediction
    #[test]
    fn bench_branch_performance() {
        println!("\n=== Branch Prediction Performance ===");

        // Predictable branches
        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for _ in 0..50000 {
            let _ = executor.exec_int_binop("cmp_eq", 1, 1);
            let _ = executor.exec_int_binop("cmp_eq", 1, 1);
            let _ = executor.exec_int_binop("cmp_eq", 1, 1);
        }
        let result = BenchResult::new("Predictable Compare (150k)", start.elapsed(), 150000);
        result.print();

        // Less predictable
        let start = Instant::now();
        for i in 0..50000 {
            let cmp = if i % 2 == 0 { 1 } else { 2 };
            let _ = executor.exec_int_binop("cmp_eq", cmp, 1);
        }
        let result = BenchResult::new("Variable Compare (50k)", start.elapsed(), 50000);
        result.print();
    }

    /// Benchmark 14: Floating point operations
    #[test]
    fn bench_float_performance() {
        println!("\n=== Floating Point Performance ===");

        let start = Instant::now();
        let executor = InterpreterExecutor::new();
        for _ in 0..50000 {
            let _ = executor.exec_float_binop("add", 3.14, 2.86);
        }
        let result = BenchResult::new("Float Add (50k)", start.elapsed(), 50000);
        result.print();

        let start = Instant::now();
        for _ in 0..50000 {
            let _ = executor.exec_float_binop("mul", 1.5, 2.5);
        }
        let result = BenchResult::new("Float Mul (50k)", start.elapsed(), 50000);
        result.print();

        let start = Instant::now();
        for _ in 0..50000 {
            let _ = executor.exec_float_binop("div", 100.0, 3.0);
        }
        let result = BenchResult::new("Float Div (50k)", start.elapsed(), 50000);
        result.print();
    }

    /// Benchmark Summary
    #[test]
    #[ignore]
    fn bench_all_summaries() {
        println!(
            "\n╔════════════════════════════════════════════════════════════════════════════════╗"
        );
        println!(
            "║                    Phase 7 Backend Performance Benchmarks                      ║"
        );
        println!(
            "║                           AdeshLang v0.3.0 - Feb 2026                          ║"
        );
        println!(
            "╚════════════════════════════════════════════════════════════════════════════════╝"
        );

        bench_interpreter_arithmetic();
        bench_bytecode_arithmetic();
        bench_complex_expressions();
        bench_array_operations();
        bench_local_variables();
        bench_comparisons();
        bench_type_casts();
        bench_nested_operations();
        bench_memory_allocation();
        bench_arc_operations();
        bench_stack_depth();
        bench_mixed_workload();
        bench_branch_performance();
        bench_float_performance();

        println!(
            "\n╔════════════════════════════════════════════════════════════════════════════════╗"
        );
        println!(
            "║                           Benchmark Suite Complete                            ║"
        );
        println!(
            "╚════════════════════════════════════════════════════════════════════════════════╝\n"
        );
    }
}
