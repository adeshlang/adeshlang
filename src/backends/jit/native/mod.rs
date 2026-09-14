//! Native JIT Backend for AdeshLang
//!
//! This module provides a TRUE native JIT compiler using cranelift-jit.
//! Unlike the existing JIT backend which interprets LIR, this backend
//! compiles LIR to native machine code at runtime and executes it via
//! function pointers.
//!
//! ## Architecture
//! - Uses cranelift-jit::JITModule for runtime code generation
//! - Reuses lowering logic from AOT backend where possible
//! - Maintains 100% semantic parity with other backends
//! - Executes via function pointers (no interpretation loop)
//!
//! ## Design Goals
//! - Native machine code generation (x86_64/AArch64)
//! - Zero backend-specific semantic divergence
//! - Shared lowering with AOT backend
//! - Modular and non-destructive evolution

pub mod compiler;
pub mod context;
pub mod runtime;
pub mod runtime_bridge;

pub use compiler::NativeJitCompiler;
pub use context::NativeJitContext;
pub use runtime::execute_native_jit;

// Removed unused import: crate::backends::common::lir::LirModule

/// Run a program using the Native JIT compiler
///
/// Supports two compilation paths:
/// 1. **VIR path (new)**: AST → HIR → MIR → VIR → LIR → Native Code (unified backend)
/// 2. **LIR path (legacy)**: AST → HIR → LIR → Native Code (for backward compatibility)
///
/// Environment variable `ADESH_USE_VIR=1` enables VIR path (on by default, `ADESH_USE_VIR=0` to use LIR)
pub fn native_jit_run(source: &str) -> Result<(), String> {
    // Parse source to HIR
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().map_err(|e| e.to_string())?;
    let hir = ast_to_hir(&ast, false)?;

    // Choose compilation path
    // LIR path is the default — VIR path silently drops ARC, match, and other
    // expressions via its catch-all `_ => 0` fallback in MIR lowering.
    let use_vir = std::env::var("ADESH_USE_VIR")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let lir_module = if use_vir {
        // VIR path: HIR → MIR → VIR → LIR → Native Code
        use crate::backends::common::vir_lir_bridge::VirToLirBridge;
        use crate::ir::mir::lower::lower_hir_to_mir;
        use crate::ir::vir::lower::lower_mir_to_vir;

        let mir = lower_hir_to_mir(&hir)?;
        let vir = lower_mir_to_vir(&mir)?;
        let mut bridge = VirToLirBridge::new();
        bridge.convert_module(&vir)?
    } else {
        // LIR path (legacy): HIR → LIR → Native Code
        super::super::lir_lower::hir_to_lir(&hir)?
    };

    // Compile to native code
    let mut compiler = NativeJitCompiler::new()?;
    let context = compiler.compile_module(&lir_module)?;

    // Execute the main function
    runtime::execute_native_jit(context)
}
