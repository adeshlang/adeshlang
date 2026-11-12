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
pub mod loader;
pub mod resolver;
pub mod runtime;
pub mod runtime_bridge;

pub use compiler::NativeJitCompiler;
pub use context::NativeJitContext;
pub use loader::{
    HostSymbolResolver, JitLoader, JitLoaderError, LoadedJitModule, StandardHostSymbolResolver,
};
pub use runtime::execute_native_jit;

// Removed unused import: crate::backends::common::lir::LirModule
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct NativeJitProfileStats {
    pub compile_time: std::time::Duration,
    pub exec_time: std::time::Duration,
}

/// Run a program using the Native JIT compiler
pub fn native_jit_run(source: &str) -> Result<(), String> {
    native_jit_run_with_stats(source, Path::new("."), Default::default()).map(|_| ())
}

/// Run a program using the Native JIT compiler and return profile statistics
///
/// `root_dir` is the directory imports are resolved against (typically the
/// directory of the file being run). Imported modules are loaded, gated, and
/// merged into the main module so `main` can call imported functions and read
/// imported constants natively.
pub fn native_jit_run_with_stats(
    source: &str,
    root_dir: &Path,
    prebound_globals: std::collections::HashMap<String, crate::parsing::ast::Value>,
) -> Result<NativeJitProfileStats, String> {
    let compile_start = std::time::Instant::now();

    // Parse source to HIR
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, None);
    let mut ast = parser.parse_program().map_err(|e| e.to_string())?;

    // Mandatory checks on the exact program being lowered: decorator
    // compile-time phases generate code, and compile-time memory safety
    // validates the exact HIR. The RAII transform is deliberately not
    // applied — the native JIT runs a managed handle heap whose cleanup
    // its runtime bridge owns.
    crate::parsing::decorator_compile::execute_compile_time_phases(&mut ast)?;

    let hir = ast_to_hir(&ast, true)?;
    crate::parsing::compile_time_memory_safety::check_memory_safety_compile_time_in(&hir, None)?;
    let ast = crate::cli::static_imports::link_ast(ast, root_dir)?;
    let hir = ast_to_hir(&ast, true)?;

    // Choose compilation path
    let use_adesh_jit = std::env::var("ADESH_JIT_BACKEND")
        .map(|v| v.to_lowercase() == "adesh" || v == "1")
        .unwrap_or(false);

    if use_adesh_jit {
        return execute_with_adesh_native_jit(&hir, compile_start);
    }

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
        // LIR path: HIR → LIR → Native Code
        super::super::lir_lower::hir_to_lir(&hir)?
    };

    // Ensure clean exception state before executing
    crate::backends::common::builtins::CURRENT_EXCEPTION.with(|exc| *exc.borrow_mut() = None);

    // Compile to native code
    let mut compiler = NativeJitCompiler::new()?;
    for (name, value) in &prebound_globals {
        compiler.bind_prebound_global(name.clone(), value.clone());
    }
    let context = compiler.compile_module(&lir_module)?;
    let compile_time = compile_start.elapsed();

    // Execute the main function on CPU with high precision measurement
    let exec_time = runtime::execute_native_jit_with_timing(context)?;

    // The shared LIR lowers every builtin call with an `__has_exception`
    // check whose unhandled path simply returns. If a runtime violation
    // (out-of-bounds pointer store, double free, non-numeric operand, ...)
    // left a pending exception behind, surface it loudly instead of
    // exiting 0 with no output — silent success on an error is wrong-code.
    let pending =
        crate::backends::common::builtins::CURRENT_EXCEPTION.with(|exc| exc.borrow_mut().take());
    if let Some(err) = pending {
        let msg = match err {
            crate::backends::common::builtins::RuntimeValue::String(s) => s,
            other => format!("{:?}", other),
        };
        return Err(format!("unhandled runtime exception: {}", msg));
    }

    Ok(NativeJitProfileStats {
        compile_time,
        exec_time,
    })
}

/// Execute HIR program directly via the pure in-house Adesh Native JIT engine.
fn execute_with_adesh_native_jit(
    hir: &crate::parsing::hir::HirModule,
    compile_start: std::time::Instant,
) -> Result<NativeJitProfileStats, String> {
    use adesh_object::TargetDescriptor;

    let triple = if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    };

    let target = TargetDescriptor::from_triple(triple).map_err(|e| format!("{:?}", e))?;

    // 1. Lower HIR directly to Adesh Machine IR
    let native_module = crate::backends::native::lower::lower_hir_module(hir, &target)
        .map_err(|e| format!("Native HIR lowering error: {:?}", e))?;

    // 2. Load and relocate in-memory with strict W^X safety
    let resolver = StandardHostSymbolResolver::new();
    let loader = JitLoader::new(&target, &resolver);
    let loaded = loader
        .load_module(&native_module)
        .map_err(|e| format!("Native JIT loading error: {:?}", e))?;

    let compile_time = compile_start.elapsed();
    let exec_start = std::time::Instant::now();

    // 3. Obtain function pointer to main or start
    unsafe {
        let entry_fn = loaded
            .get_function::<extern "C" fn() -> i64>("main")
            .or_else(|_| loaded.get_function::<extern "C" fn() -> i64>("entry"))
            .map_err(|e| format!("JIT entry point error: {:?}", e))?;

        entry_fn();
    }

    let exec_time = exec_start.elapsed();

    Ok(NativeJitProfileStats {
        compile_time,
        exec_time,
    })
}
