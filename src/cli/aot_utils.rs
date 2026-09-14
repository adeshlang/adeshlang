//! AOT compilation utilities
//!
//! This module provides utilities for AOT compilation, including
//! header file generation.

use crate::cli::RuntimeConfig;
use std::path::Path;

/// Generate a C header file for exported functions in an AOT-compiled module
pub fn generate_aot_header(src: &str, output_path: &Path) -> Result<(), String> {
    use crate::backends::cranelift_aot::CraneliftAotCompiler;
    use crate::backends::lir_lower::hir_to_lir;
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::hir_passes::run_safety_passes;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().map_err(|e| e.to_string())?;

    let hir = ast_to_hir(&ast, false)?;

    // Run ownership and borrow checking
    let config = RuntimeConfig::default();
    let pass_results = run_safety_passes(&hir, config.check_ownership, config.check_moves)?;

    if config.verbose {
        eprintln!("✅ Ownership and borrow checking passed");
        if let Some(ref analysis) = pass_results.ownership {
            if !analysis.needs_arc.is_empty() {
                eprintln!("   Variables needing ARC: {:?}", analysis.needs_arc);
            }
            if !analysis.escaping_vars.is_empty() {
                eprintln!("   Variables escaping scope: {:?}", analysis.escaping_vars);
            }
        }
    }

    let lir = hir_to_lir(&hir)?;

    let compiler = CraneliftAotCompiler::new();
    let header = compiler.generate_header(&lir);

    std::fs::write(output_path, header).map_err(|e| format!("Failed to write header file: {}", e))
}
