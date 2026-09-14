//! Source code parsing module
//!
//! This module provides parsing with ownership and borrow checking.

use crate::cli::RuntimeConfig;
use crate::parsing::ast::Stmt;
use crate::parsing::hir::HirModule;

/// Run ownership and borrow checking on source code
/// Returns (AST, HIR) if successful
pub fn check_ownership_and_parse(
    src: &str,
    config: &RuntimeConfig,
) -> Result<(Vec<Stmt>, HirModule), String> {
    check_ownership_and_parse_in(src, config, None)
}

/// Parse + memory-safety analysis with a source file path for clickable diagnostics.
pub fn check_ownership_and_parse_in(
    src: &str,
    config: &RuntimeConfig,
    file: Option<&str>,
) -> Result<(Vec<Stmt>, HirModule), String> {
    use crate::parsing::compile_time_memory_safety::check_memory_safety_compile_time_in;
    use crate::parsing::decorator_compile::execute_compile_time_phases;
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::hir_passes::run_safety_passes_with_location;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    let clean_src = super::directives::strip_compile_directive(src);
    let file_owned = file.map(|f| f.to_string());

    // Parse source code (with file path for clickable errors)
    let mut lexer = Lexer::new(&clean_src);
    if let Some(ref f) = file_owned {
        lexer.set_file(f.clone());
    }
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, file_owned.clone());
    let mut ast = parser.parse_program().map_err(|e| e.to_string())?;

    // Execute decorator compile-time phases (typecheck, compile)
    if config.verbose {
        eprintln!("🎨 Executing decorator compile-time phases...");
    }
    execute_compile_time_phases(&mut ast)?;

    // Enforce embedded-mode heap prohibition at compile-time
    if config.embedded && super::validation::ast_contains_heap_alloc(&ast) {
        return Err(
            "Embedded mode forbids heap allocation via alloc(); use stack/arena/region instead."
                .to_string(),
        );
    }

    // Apply RAII transformation for automatic memory cleanup
    if config.verbose {
        eprintln!("🔄 Applying RAII memory management transformations...");
    }
    ast = crate::memory::raii::transform_ast_with_raii(ast);

    // Lower to HIR
    let hir = ast_to_hir(&ast, false)?;

    // ============================================
    // COMPILE-TIME MEMORY SAFETY CHECKS
    // ============================================
    // These checks happen BEFORE any backend execution
    // Once these pass, all backends are guaranteed memory-safe

    if config.verbose {
        eprintln!("🔒 Running comprehensive compile-time memory safety analysis...");
    }

    check_memory_safety_compile_time_in(&hir, file)?;

    if config.verbose {
        eprintln!("✅ Compile-time memory safety validation passed!");
    }

    // Run unused warnings check with line and column precision
    let unused_warnings = crate::parsing::unused_warnings::UnusedWarningPass::check_ast(
        &ast, file, &clean_src, config,
    );
    for warning in unused_warnings {
        eprintln!("{}", warning);
    }

    // Run ownership and borrow checking if enabled (legacy checks)
    if config.check_ownership || config.check_moves {
        let pass_results = run_safety_passes_with_location(&hir, config.check_ownership, config.check_moves, file, Some(&clean_src))?;

        if config.verbose {
            eprintln!("✅ Ownership and borrow checking passed");
            if let Some(ref analysis) = pass_results.ownership {
                if !analysis.needs_arc.is_empty() {
                    eprintln!("   📦 Variables needing ARC: {:?}", analysis.needs_arc);
                }
                if !analysis.escaping_vars.is_empty() {
                    eprintln!(
                        "   🚀 Variables escaping scope: {:?}",
                        analysis.escaping_vars
                    );
                }
            }
        }
    } else if config.verbose {
        eprintln!("⚠️  Legacy ownership checking disabled (compile-time checks still active)");
    }

    Ok((ast, hir))
}
