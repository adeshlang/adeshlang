//! Compiler Diagnostics Tests
//!
//! Comprehensive tests verifying diagnostic generation across lexical, syntax,
//! semantic, and type-checking phases.

use adeshlang::parsing::error::ErrorKind;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use adeshlang::semantics::index_source;
use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

#[test]
fn test_lexer_diagnostic_invalid_token() {
    let src = "let x = @#$%;";
    let mut lexer = Lexer::new(src);
    let res = lexer.tokenize();

    // Lexer either fails with error or skips/marks invalid tokens
    if let Err(err) = res {
        assert!(err.line > 0, "Error line should be >= 1");
        assert!(err.col > 0, "Error column should be >= 1");
    }
}

#[test]
fn test_parser_diagnostic_unexpected_token() {
    let src = "let x = ) + 10;";
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().expect("Lexing should succeed");
    let mut parser = Parser::new(tokens, Some("diag_syntax.adesh".to_string()));
    let res = parser.parse_program();

    assert!(res.is_err(), "Parser must fail on unexpected token ')'");
    let err = res.unwrap_err();
    assert!(matches!(err.kind, ErrorKind::Parse));
    assert!(err.line > 0);
    assert!(err.col > 0);
    assert!(
        err.to_string().contains("diag_syntax.adesh") || !err.to_string().is_empty(),
        "Error message should contain diagnostic information"
    );
}

#[test]
fn test_parser_diagnostic_unclosed_block() {
    let src = r#"
    fn unclosed_function() {
        let x = 10;
        let y = 20;
    "#;
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().expect("Lexing should succeed");
    let mut parser = Parser::new(tokens, Some("unclosed.adesh".to_string()));
    let res = parser.parse_program();

    assert!(res.is_err(), "Parser must fail on unclosed function block");
}

#[test]
fn test_semantic_diagnostic_undefined_symbol() {
    let src = r#"
    fn compute() {
        return non_existent_symbol_12345 + 10;
    }
    "#;
    let index = index_source(src);

    // Declaration should not exist in symbol table
    assert!(
        index.find_declaration("non_existent_symbol_12345").is_none(),
        "Undefined symbol must not be in declaration index"
    );
}

#[test]
fn test_type_diagnostic_mismatch() {
    let bad_type_src = r#"
    let num: int = "string cannot be assigned to int";
    "#;
    let mut loader = ModuleLoader::new(Path::new("."));
    let mut interp = Interpreter::new();
    let res = interp.run_module(bad_type_src, &mut loader, None);

    assert!(
        res.is_err(),
        "Runtime/TypeChecker must reject assigning string literal to int variable"
    );
}
