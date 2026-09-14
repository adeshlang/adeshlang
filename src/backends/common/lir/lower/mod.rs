//! HIR to LIR lowering pass
//!
//! This module provides the transformation from HIR to LIR representation.
//! It has been modularized into focused submodules for maintainability:
//!
//! - `core`: Main entry point, context structures, and initialization
//! - `functions`: Function and decorator lowering
//! - `statements`: Statement lowering
//! - `expressions`: Expression lowering
//! - `types`: Type conversion and manipulation
//! - `memory`: Memory management and drop handling

mod core;
mod expressions;
mod functions;
mod memory;
mod statements;
mod types;

// Re-export the main public API
pub use core::hir_to_lir;

#[cfg(test)]
mod tests {
    use super::super::{LirInst, LirModule};
    use super::*;
    use crate::parsing::hir::{HirExpr, HirFunction, HirLiteral, HirModule, HirStmt, HirType};
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    fn parse_to_lir(src: &str) -> Result<LirModule, String> {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
        let mut parser = Parser::new(tokens, None);
        let ast = parser.parse_program().map_err(|e| e.to_string())?;
        let hir = ast_to_hir(&ast, false)?;
        hir_to_lir(&hir)
    }

    #[test]
    fn test_simple_let_lir() {
        let lir = parse_to_lir("let x = 42;").unwrap();
        assert!(!lir.functions.is_empty());
    }

    #[test]
    fn test_arithmetic_lir() {
        let lir = parse_to_lir("let x = 1 + 2 * 3;").unwrap();
        assert!(!lir.functions.is_empty());
    }

    #[test]
    fn drop_plan_emits_arc_drop_in_lir() {
        use std::sync::Arc;

        let mut module = HirModule::new();
        let body = Arc::new(vec![
            HirStmt::Let {
                name: "y".into(),
                ty: Some(HirType::Shared(Box::new(HirType::I32))),
                init: Some(HirExpr::Literal(HirLiteral::Int(1))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(None),
        ]);

        let func = HirFunction {
            name: "f".into(),
            params: vec![],
            body,
            ret_type: Some(HirType::I32),
            is_async: false,
            decorators: vec![],
            is_exported: false,
            move_params: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };
        module.functions.push(func);

        let lir = hir_to_lir(&module).expect("hir_to_lir should succeed");
        let f = lir
            .functions
            .iter()
            .find(|f| f.name == "f")
            .expect("function f present");
        let has_arc_drop = f.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|inst| matches!(inst, LirInst::ArcDrop(_)))
        });

        assert!(
            has_arc_drop,
            "expected ArcDrop inserted from drop plan lowering"
        );
    }

    #[test]
    fn test_function_lir() {
        let lir = parse_to_lir("fn add(a, b) { return a + b; }").unwrap();
        assert!(lir.functions.iter().any(|f| f.name == "add"));
    }

    #[test]
    fn test_if_lir() {
        let lir = parse_to_lir("if (x > 0) { print(x); }").unwrap();
        assert!(!lir.functions.is_empty());
        assert!(lir.functions[0].blocks.len() > 1);
    }

    #[test]
    fn test_while_lir() {
        let lir = parse_to_lir("while (i < 10) { i = i + 1; }").unwrap();
        assert!(!lir.functions.is_empty());
        assert!(lir.functions[0].blocks.len() >= 3);
    }
}
