//! Public API for JIT execution

use crate::backends::jit::builtins::RuntimeValue;
use crate::backends::jit::cranelift::cranelift_impl::expand_header_imports;
use crate::backends::jit::cranelift::{JitContext, JitMemoryStats};
use crate::backends::jit::recursion_opt::RecursionOptConfig;

/// Compile and execute source code using the JIT, returning memory stats
pub fn jit_run_with_stats(src: &str) -> Result<(RuntimeValue, JitMemoryStats), String> {
    jit_run_with_stats_and_config(src, RecursionOptConfig::default())
}

/// Compile and execute source code using the JIT with custom recursion config
///
/// This function supports two compilation paths:
/// 1. **VIR path (new)**: AST → HIR → MIR → VIR → Cranelift (unified backend)
/// 2. **LIR path (legacy)**: AST → HIR → LIR → Cranelift (for backward compatibility)
///
/// The VIR path includes full memory safety analysis and is the recommended approach.
pub fn jit_run_with_stats_and_config(
    src: &str,
    recursion_config: RecursionOptConfig,
) -> Result<(RuntimeValue, JitMemoryStats), String> {
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    // Parse
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, None);
    let mut ast = parser.parse_program().map_err(|e| e.to_string())?;

    // Pre-process @cImport directives and expand them into ExternFunction declarations
    ast = expand_header_imports(ast)?;

    // Mandatory checks on the exact program being lowered: decorator
    // compile-time phases generate code that must run like on every other
    // backend, and compile-time memory safety must validate the exact HIR.
    // The RAII transform is deliberately NOT applied: the JIT runs a
    // managed handle heap whose cleanup the JIT runtime owns; injecting
    // raw frees would target a memory model this backend does not
    // implement.
    crate::parsing::decorator_compile::execute_compile_time_phases(&mut ast)?;

    // Lower to HIR
    let hir = ast_to_hir(&ast, false)?;
    crate::parsing::compile_time_memory_safety::check_memory_safety_compile_time_in(&hir, None)?;

    // Choose compilation path — LIR is the default (VIR drops ARC ops)
    let use_vir = std::env::var("ADESH_USE_VIR")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let mut ctx = JitContext::with_recursion_opt(recursion_config);

    if use_vir {
        // VIR path: HIR → MIR → VIR → Cranelift
        use crate::ir::mir::lower::lower_hir_to_mir;
        use crate::ir::vir::lower::lower_mir_to_vir;

        // Lower HIR to MIR (includes memory safety analysis)
        let mir = lower_hir_to_mir(&hir)?;

        // Lower MIR to VIR
        let vir = lower_mir_to_vir(&mir)?;

        // Load VIR module into JIT context
        ctx.load_vir_module(&vir)?;
    } else {
        // LIR path (legacy): HIR → LIR → Cranelift
        use crate::backends::jit::lir_lower::hir_to_lir;
        let lir = hir_to_lir(&hir)?;
        ctx.load_module(&lir);
    }

    // Execute main function
    let result = ctx.run_main()?;
    let stats = ctx.get_memory_stats();

    // Flush output streams to ensure all output is written
    use std::io::Write;
    std::io::stdout().flush().ok();
    std::io::stderr().flush().ok();

    // Give background threads a moment to complete
    std::thread::sleep(std::time::Duration::from_millis(10));

    Ok((result, stats))
}

/// Compile and execute source code using the JIT
pub fn jit_run(src: &str) -> Result<RuntimeValue, String> {
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    // Parse
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, None);
    let mut ast = parser.parse_program().map_err(|e| e.to_string())?;

    // Pre-process @cImport directives
    ast = expand_header_imports(ast)?;

    // Mandatory checks, mirroring jit_run_with_stats_and_config: decorator
    // compile-time phases and compile-time memory safety run on the exact
    // program being lowered. RAII is intentionally not applied (managed
    // handle heap; see jit_run_with_stats_and_config).
    crate::parsing::decorator_compile::execute_compile_time_phases(&mut ast)?;

    // Lower to HIR
    let hir = ast_to_hir(&ast, false)?;
    crate::parsing::compile_time_memory_safety::check_memory_safety_compile_time_in(&hir, None)?;

    // Choose compilation path — LIR is the default (VIR drops ARC ops)
    let use_vir = std::env::var("ADESH_USE_VIR")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let mut ctx = JitContext::new();

    if use_vir {
        // VIR path
        use crate::ir::mir::lower::lower_hir_to_mir;
        use crate::ir::vir::lower::lower_mir_to_vir;

        let mir = lower_hir_to_mir(&hir)?;
        let vir = lower_mir_to_vir(&mir)?;
        ctx.load_vir_module(&vir)?;
    } else {
        // LIR path (legacy)
        use crate::backends::jit::lir_lower::hir_to_lir;
        let lir = hir_to_lir(&hir)?;
        ctx.load_module(&lir);
    }

    ctx.run_main()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jit_simple_arithmetic() {
        let result = jit_run("let x = 1 + 2;").unwrap();
        // The result is Null since main doesn't return anything
        assert!(matches!(result, RuntimeValue::Null));
    }

    #[test]
    fn test_jit_function_call() {
        let result = jit_run(
            r#"
            fn add(a, b) { return a + b; }
            let x = add(2, 3);
        "#,
        )
        .unwrap();
        assert!(matches!(result, RuntimeValue::Null));
    }

    #[test]
    fn test_jit_if_statement() {
        let result = jit_run(
            r#"
            let x = 5;
            if (x > 0) { let y = 1; }
        "#,
        )
        .unwrap();
        assert!(matches!(result, RuntimeValue::Null));
    }

    #[test]
    fn test_jit_while_loop() {
        unsafe {
            std::env::set_var("ADESH_USE_VIR", "0");
        }
        let result = jit_run(
            r#"
            let i = 0;
            while (i < 10) { i = i + 1; }
        "#,
        )
        .unwrap();
        assert!(matches!(result, RuntimeValue::Null));
    }

    #[test]
    fn test_jit_context_creation() {
        let ctx = JitContext::new();
        assert!(ctx.functions.is_empty());
    }
}
