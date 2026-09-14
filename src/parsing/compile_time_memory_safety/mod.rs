//! Comprehensive Compile-Time Memory Safety Analysis
//!
//! This module implements Rust-like compile-time memory safety guarantees that execute
//! during the CFG/HIR phase of compilation, before any backend execution. Once these checks
//! pass, all backends (Interpreter, VM, JIT, WASM, AOT, REPL) are guaranteed to be memory-safe
//! without any runtime overhead.
//!
//! # Architecture
//!
//! ```text
//! Source Code → Lexer → Parser → AST → HIR → CFG → [COMPILE-TIME MEMORY SAFETY CHECKS]
//!                                                    ↓
//!                                                    LIR → IR → Backends (All Safe!)
//! ```
//!
//! # Checks Performed
//!
//! ## 1. Ownership Analysis
//! - Every value has exactly one owner
//! - Ownership transfer on assignment (move semantics)
//! - Use-after-move detection
//! - Double-free prevention
//!
//! ## 2. Borrow Checking
//! - Multiple immutable borrows OR single mutable borrow
//! - No aliasing mutable references
//! - Borrow scope validation
//! - Free-while-borrowed prevention
//!
//! ## 3. Lifetime Analysis
//! - References don't outlive their referents
//! - Return value lifetime validation
//! - Closure capture lifetime checking
//!
//! ## 4. Data Race Prevention
//! - Thread-safe ownership transfer
//! - Send/Sync trait enforcement
//! - Atomic operation validation
//!
//! ## 5. Memory Leak Detection
//! - Cycle detection in reference graphs
//! - Weak reference validation
//! - Resource cleanup verification

// Module structure
mod borrow;
mod detection;
mod error;
mod lifetime;
mod ownership;
mod state;
mod utils;

// Re-export public types
pub use error::{BorrowLocation, CompileTimeMemoryError, SourceLocation};

#[cfg(test)]
use crate::parsing::hir::{HirClass, HirExpr, HirLiteral, HirModule, HirStmt, HirType};
#[cfg(not(test))]
use crate::parsing::hir::{HirClass, HirModule, HirType};
use crate::types::traits::TraitChecker;
use crate::utils::collections::{FastMap, FastSet};
use state::{BorrowInfo, LifetimeScope, OwnershipNode, ThreadContext};

/// Compile-time memory safety analyzer
pub struct CompileTimeMemorySafety {
    errors: Vec<CompileTimeMemoryError>,
    warnings: Vec<String>,

    // Ownership tracking
    ownership_graph: FastMap<String, OwnershipNode>,

    // Borrow tracking
    #[allow(dead_code)]
    borrow_map: FastMap<String, Vec<BorrowInfo>>,

    // Lifetime tracking
    #[allow(dead_code)]
    lifetime_scopes: Vec<LifetimeScope>,

    // Thread safety tracking
    #[allow(dead_code)]
    thread_contexts: FastMap<String, ThreadContext>,

    // Memory leak detection
    reference_graph: FastMap<String, Vec<String>>,

    // Trait checker for Send/Sync validation
    trait_checker: TraitChecker,

    // Class registry for method lookup
    class_registry: FastMap<String, HirClass>,

    // Variable type tracking
    var_types: FastMap<String, HirType>,

    // Source location tracking
    current_file: String,
    current_line: usize,
    current_function: Option<String>,
    stmt_line_map: FastMap<usize, usize>, // stmt index -> line number
}

impl CompileTimeMemorySafety {
    /// Create a new memory safety analyzer
    pub fn new() -> Self {
        Self::with_file(None)
    }

    /// Create analyzer with an optional source file path (for clickable diagnostics)
    pub fn with_file(file: Option<&str>) -> Self {
        let file_name = file
            .filter(|f| !f.is_empty())
            .unwrap_or("main.adesh")
            .to_string();
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            ownership_graph: FastMap::default(),
            borrow_map: FastMap::default(),
            lifetime_scopes: vec![LifetimeScope {
                id: 0,
                parent: None,
                variables: FastSet::default(),
                start_location: SourceLocation {
                    file: file_name.clone(),
                    line: 1,
                    column: 1,
                    context: "".to_string(),
                },
                end_location: None,
            }],
            thread_contexts: FastMap::default(),
            reference_graph: FastMap::default(),
            trait_checker: TraitChecker::new(),
            class_registry: FastMap::default(),
            var_types: FastMap::default(),
            current_file: file_name,
            current_line: 1,
            current_function: None,
            stmt_line_map: FastMap::default(),
        }
    }

    /// Override the file path used in diagnostics
    pub fn set_file(&mut self, file: impl Into<String>) {
        let f = file.into();
        self.current_file = f.clone();
        if let Some(scope) = self.lifetime_scopes.first_mut() {
            scope.start_location.file = f;
        }
    }

    /// Perform comprehensive compile-time memory safety analysis
    pub fn analyze(&mut self, module: &HirModule) -> Result<(), Vec<CompileTimeMemoryError>> {
        // Register all type definitions with trait checker
        self.trait_checker.analyze_module(module);

        // Phase 1: Build ownership graph
        // First populate class registry
        for class in &module.classes {
            self.class_registry
                .insert(class.name.clone(), class.clone());
        }

        self.build_ownership_graph(module)?;

        // Phase 2: Validate borrowing rules
        self.validate_borrowing(module)?;

        // Phase 3: Check lifetimes
        self.check_lifetimes(module)?;

        // Phase 4: Detect data races (ENHANCED with Send/Sync)
        self.detect_data_races(module)?;

        // Phase 5: Find memory leaks
        self.detect_memory_leaks(module)?;

        // Phase 6: Run CFG-based borrow checker for control flow
        self.run_cfg_borrow_checker(module)?;

        // Phase 7: NEW - Concurrency safety validation
        self.check_concurrency_safety(module)?;

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }
}

/// Perform complete compile-time memory safety analysis on HIR module
/// This must pass before any backend execution
pub fn check_memory_safety_compile_time(module: &HirModule) -> Result<(), String> {
    check_memory_safety_compile_time_in(module, None)
}

/// Same as [`check_memory_safety_compile_time`] but attaches a source file path
/// so diagnostics use clickable `file:line:col` locations.
pub fn check_memory_safety_compile_time_in(
    module: &HirModule,
    file: Option<&str>,
) -> Result<(), String> {
    let mut analyzer = CompileTimeMemorySafety::with_file(file);

    match analyzer.analyze(module) {
        Ok(()) => {
            for warning in analyzer.get_warnings() {
                eprintln!("warning: {}", warning);
            }
            Ok(())
        }
        Err(errors) => {
            let mut unique_errors = Vec::new();
            let mut seen = FastSet::default();
            for err in errors {
                let msg = err.format_error();
                if seen.insert(msg.clone()) {
                    unique_errors.push(msg);
                }
            }

            let mut error_msg = String::new();
            error_msg.push_str(&format!(
                "Compile-time memory safety check failed with {} error(s):\n\n",
                unique_errors.len()
            ));

            for (i, msg) in unique_errors.iter().enumerate() {
                error_msg.push_str(&format!("── Error {} ──\n{}\n\n", i + 1, msg));
            }

            error_msg
                .push_str("Locations use file:line:col (clickable in most editors/terminals).\n");
            error_msg.push_str(
                "AdeshLang uses ARC for shared ownership — use `share`, `strong`, or `weak` \
                 to share values instead of moving them.\n",
            );
            error_msg.push_str("Docs: https://adeshlang.dev/docs/memory-safety\n");

            Err(error_msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Helper to create a simple HirModule for testing
    fn create_test_module(
        statements: Vec<HirStmt>,
        functions: Vec<crate::parsing::hir::HirFunction>,
    ) -> HirModule {
        HirModule {
            functions,
            classes: Vec::new(),
            statements,
        }
    }

    /// Helper to create a simple function
    fn create_test_function(name: &str, body: Vec<HirStmt>) -> crate::parsing::hir::HirFunction {
        crate::parsing::hir::HirFunction {
            name: name.to_string(),
            params: Vec::new(),
            body: Arc::new(body),
            ret_type: None,
            is_async: false,
            decorators: Vec::new(),
            is_exported: false,
            move_params: Vec::new(),
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        }
    }

    #[test]
    fn test_use_after_move_detection() {
        let mut analyzer = CompileTimeMemorySafety::new();

        // Create: let x = "hello"; let y = x; print(x);  // x is moved!
        let stmt_hir_stmts = vec![
            HirStmt::Let {
                name: "x".to_string(),
                ty: Some(HirType::String),
                init: Some(HirExpr::Literal(HirLiteral::String("hello".to_string()))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "y".to_string(),
                ty: None,
                init: Some(HirExpr::LoadVar("x".to_string())),
                is_const: false,
                is_borrowed: None,
            },
            // Use x after move
            HirStmt::Expr(HirExpr::Call(
                Box::new(HirExpr::LoadVar("print".to_string())),
                vec![HirExpr::LoadVar("x".to_string())],
                vec![],
            )),
        ];

        let func = create_test_function("test_fn", stmt_hir_stmts);
        let module = create_test_module(Vec::new(), vec![func]);

        let result = analyzer.analyze(&module);

        // Should detect use-after-move
        assert!(
            result.is_err() || !analyzer.errors.is_empty(),
            "Should detect use after move"
        );
    }

    #[test]
    fn test_copy_types_dont_move() {
        let mut analyzer = CompileTimeMemorySafety::new();

        // Create: let x = 42; let y = x; print(x);  // x is Copy, no move!
        let stmts = vec![
            HirStmt::Let {
                name: "x".to_string(),
                ty: Some(HirType::Int),
                init: Some(HirExpr::Literal(HirLiteral::Int(42))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "y".to_string(),
                ty: None,
                init: Some(HirExpr::LoadVar("x".to_string())),
                is_const: false,
                is_borrowed: None,
            },
            // Use x - should be fine since Int is Copy
            HirStmt::Expr(HirExpr::Call(
                Box::new(HirExpr::LoadVar("print".to_string())),
                vec![HirExpr::LoadVar("x".to_string())],
                vec![],
            )),
        ];

        let func = create_test_function("test_fn", stmts);
        let module = create_test_module(Vec::new(), vec![func]);

        let result = analyzer.analyze(&module);

        // Should NOT detect any errors (Copy types don't move)
        assert!(result.is_ok(), "Copy types should not move");
    }

    #[test]
    fn test_analyzer_initialization() {
        let analyzer = CompileTimeMemorySafety::new();

        assert!(
            analyzer.errors.is_empty(),
            "New analyzer should have no errors"
        );
        assert!(
            analyzer.warnings.is_empty(),
            "New analyzer should have no warnings"
        );
        assert!(
            analyzer.ownership_graph.is_empty(),
            "New analyzer should have empty ownership graph"
        );
        assert!(
            analyzer.class_registry.is_empty(),
            "New analyzer should have empty class registry"
        );
        assert!(
            analyzer.var_types.is_empty(),
            "New analyzer should have empty var types"
        );
    }

    #[test]
    fn test_empty_module_passes() {
        let mut analyzer = CompileTimeMemorySafety::new();
        let module = HirModule::new();

        let result = analyzer.analyze(&module);

        assert!(result.is_ok(), "Empty module should pass analysis");
    }
}
