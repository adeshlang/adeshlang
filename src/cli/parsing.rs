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

    let session_id = std::env::var("ADESH_OBSERVATORY_SESSION").ok();
    let tele_client = session_id.map(adesh_monitor_core::ipc::TelemetryClient::new);

    // Stage: Lex & Parse
    let t_parse_start = std::time::Instant::now();
    let mut lexer = Lexer::new(&clean_src);
    if let Some(ref f) = file_owned {
        lexer.set_file(f.clone());
    }
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, file_owned.clone());
    let mut ast = parser.parse_program().map_err(|e| e.to_string())?;
    if let Some(ref tc) = tele_client {
        tc.emit(adesh_monitor_protocol::EventPayload::CompilerStage(
            adesh_monitor_protocol::CompilerStageEvent {
                stage_name: "parse".to_string(),
                input_file: file_owned.clone(),
                duration_micros: t_parse_start.elapsed().as_micros() as u64,
                success: true,
                error_message: None,
                metadata: std::collections::HashMap::new(),
            },
        ));
        // Static source-level inventory complements OS RSS samples. These are
        // explicitly estimates/counts, not fabricated runtime byte totals.
        let mut memory_metadata = std::collections::HashMap::new();
        memory_metadata.insert(
            "static_source_bytes".to_string(),
            clean_src.len().to_string(),
        );
        memory_metadata.insert(
            "dynamic_allocation_sites".to_string(),
            clean_src.matches("alloc(").count().to_string(),
        );
        memory_metadata.insert(
            "borrow_operations".to_string(),
            clean_src.matches("borrow").count().to_string(),
        );
        memory_metadata.insert("raii_transform_applied".to_string(), "true".to_string());
        tc.emit(adesh_monitor_protocol::EventPayload::CompilerStage(
            adesh_monitor_protocol::CompilerStageEvent {
                stage_name: "memory_efficiency_inventory".to_string(),
                input_file: file_owned.clone(),
                duration_micros: 0,
                success: true,
                error_message: None,
                metadata: memory_metadata,
            },
        ));
    }

    // Execute decorator compile-time phases (typecheck, compile)
    if config.verbose {
        eprintln!("🎨 Executing decorator compile-time phases...");
    }
    let t_dec_start = std::time::Instant::now();
    execute_compile_time_phases(&mut ast)?;
    if let Some(ref tc) = tele_client {
        tc.emit(adesh_monitor_protocol::EventPayload::CompilerStage(
            adesh_monitor_protocol::CompilerStageEvent {
                stage_name: "decorator_phases".to_string(),
                input_file: file_owned.clone(),
                duration_micros: t_dec_start.elapsed().as_micros() as u64,
                success: true,
                error_message: None,
                metadata: std::collections::HashMap::new(),
            },
        ));
    }

    // Enforce embedded-mode heap prohibition at compile-time
    if config.embedded && super::validation::ast_contains_heap_alloc(&ast) {
        let err = crate::parsing::error::LangError::located(
            crate::parsing::error::ErrorKind::Ownership,
            "embedded mode forbids heap allocation via alloc(); use stack/arena/region instead",
            file_owned,
            1,
            1,
            "",
        )
        .with_code("AD0004")
        .with_help("use stack buffers or region arena allocation instead of alloc()")
        .with_explain_hint();
        return Err(err.to_string());
    }

    // Apply RAII transformation for automatic memory cleanup
    if config.verbose {
        eprintln!("🔄 Applying RAII memory management transformations...");
    }
    let t_raii_start = std::time::Instant::now();
    ast = crate::memory::raii::transform_ast_with_raii(ast);
    if let Some(ref tc) = tele_client {
        tc.emit(adesh_monitor_protocol::EventPayload::CompilerStage(
            adesh_monitor_protocol::CompilerStageEvent {
                stage_name: "raii_transformation".to_string(),
                input_file: file_owned.clone(),
                duration_micros: t_raii_start.elapsed().as_micros() as u64,
                success: true,
                error_message: None,
                metadata: std::collections::HashMap::new(),
            },
        ));
    }

    // Lower to HIR (include tests so test functions called by user code or test runners are retained)
    let t_hir_start = std::time::Instant::now();
    let hir = ast_to_hir(&ast, true)?;
    if let Some(ref tc) = tele_client {
        tc.emit(adesh_monitor_protocol::EventPayload::CompilerStage(
            adesh_monitor_protocol::CompilerStageEvent {
                stage_name: "hir_lower".to_string(),
                input_file: file_owned.clone(),
                duration_micros: t_hir_start.elapsed().as_micros() as u64,
                success: true,
                error_message: None,
                metadata: std::collections::HashMap::new(),
            },
        ));
    }

    // ============================================
    // COMPILE-TIME MEMORY SAFETY CHECKS
    // ============================================
    // These checks happen BEFORE any backend execution
    // Once these pass, all backends are guaranteed memory-safe

    if config.verbose {
        eprintln!("🔒 Running comprehensive compile-time memory safety analysis...");
    }

    let t_safety_start = std::time::Instant::now();
    check_memory_safety_compile_time_in(&hir, file)?;
    if let Some(ref tc) = tele_client {
        tc.emit(adesh_monitor_protocol::EventPayload::CompilerStage(
            adesh_monitor_protocol::CompilerStageEvent {
                stage_name: "compile_time_memory_safety".to_string(),
                input_file: file_owned.clone(),
                duration_micros: t_safety_start.elapsed().as_micros() as u64,
                success: true,
                error_message: None,
                metadata: std::collections::HashMap::new(),
            },
        ));
    }

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
        let pass_results = run_safety_passes_with_location(
            &hir,
            config.check_ownership,
            config.check_moves,
            file,
            Some(&clean_src),
        )?;

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
