//! Public API for the tiered JIT compiler.
//!
//! Provides convenience functions for compiling and executing AdeshLang source code
//! using the tiered JIT system.

use crate::backends::jit::builtins::RuntimeValue;
use crate::backends::jit::tiered::{OptLevel, TieredJitContext};

/// Compile and execute source code using the tiered JIT with O3 optimization.
///
/// This is the recommended entry point for executing AdeshLang code with maximum
/// performance. Uses aggressive optimization and tier promotion.
pub fn tiered_jit_run(src: &str) -> Result<RuntimeValue, String> {
    tiered_jit_run_with_opt(src, OptLevel::O3)
}

/// Compile and execute source code with a specific optimization level.
///
/// Allows fine-grained control over the optimization strategy:
/// - O0: No optimization (debug mode)
/// - O1: Basic optimizations
/// - O2: Moderate optimizations (balanced)
/// - O3: Aggressive optimizations (maximum performance)
///
/// Supports two compilation paths:
/// 1. **VIR path (new)**: AST → HIR → MIR → VIR → Tiered JIT (unified backend)
/// 2. **LIR path (legacy)**: AST → HIR → LIR → Tiered JIT (for backward compatibility)
///
/// Environment variable `ADESH_USE_VIR=1` enables VIR path (on by default, `ADESH_USE_VIR=0` to use LIR)
pub fn tiered_jit_run_with_opt(src: &str, opt_level: OptLevel) -> Result<RuntimeValue, String> {
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    // Parse
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().map_err(|e| e.to_string())?;

    // Lower to HIR
    let hir = ast_to_hir(&ast, false)?;

    // Choose compilation path — LIR is the default (VIR drops ARC ops)
    let use_vir = std::env::var("ADESH_USE_VIR")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let mut ctx = TieredJitContext::new();
    ctx.set_opt_level(opt_level);

    if use_vir {
        // VIR path: HIR → MIR → VIR → Tiered JIT
        use crate::ir::mir::lower::lower_hir_to_mir;
        use crate::ir::vir::lower::lower_mir_to_vir;

        let mir = lower_hir_to_mir(&hir)?;
        let vir = lower_mir_to_vir(&mir)?;
        ctx.load_vir_module(&vir)?;
    } else {
        // LIR path (legacy): HIR → LIR → Tiered JIT
        use crate::backends::jit::lir_lower::hir_to_lir;
        let lir = hir_to_lir(&hir)?;
        ctx.load_module(&lir);
    }

    ctx.run_main()
}
