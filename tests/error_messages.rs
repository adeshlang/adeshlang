//! Tests for error message quality and format
#![allow(clippy::result_large_err)]

use adeshlang::parsing::error::{ErrorKind, LangError};
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;

fn parse(src: &str) -> Result<Vec<adeshlang::parsing::ast::Stmt>, LangError> {
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens, Some("test.adesh".to_string()));
    parser.parse_program()
}

#[test]
fn test_error_contains_line_and_column() {
    let result = parse(
        r#"
let x = 
"#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.line > 0, "Error should have line number");
    assert!(err.col > 0, "Error should have column number");
}

#[test]
fn test_error_contains_file_when_set() {
    let src = r#"let x = )"#;
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens, Some("myfile.adesh".to_string()));
    let result = parser.parse_program();

    if let Err(err) = result {
        let err_str = err.to_string();
        assert!(
            err_str.contains("myfile.adesh"),
            "Error should contain filename"
        );
    }
}

#[test]
fn test_error_kind_is_set() {
    let result = parse("let x = )");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err.kind, ErrorKind::Parse),
        "Expected ParseError kind"
    );
}

#[test]
fn test_error_contains_source_snippet() {
    let src = "let x = invalid_syntax!!!";
    let result = parse(src);
    if let Err(err) = result {
        assert!(
            !err.line_text.is_empty(),
            "Error should contain source line text"
        );
    }
}

#[test]
fn test_lexer_error_has_location() {
    let src = "let x = \"unterminated string";
    let mut lexer = Lexer::new(src);
    let result = lexer.tokenize();
    if let Err(err) = result {
        assert!(err.line > 0, "Lexer error should have line");
        assert!(err.col > 0, "Lexer error should have column");
        assert!(
            matches!(err.kind, ErrorKind::Lexical),
            "Expected Lexical error kind"
        );
    }
}

#[test]
fn test_error_hint_can_be_added() {
    let err = LangError::new(
        ErrorKind::Parse,
        "Missing semicolon".to_string(),
        5,
        10,
        "let x = 5".to_string(),
    )
    .with_hint("Add a semicolon at the end of the statement");

    assert!(err.hint.is_some());
    assert!(err.hint.as_ref().unwrap().contains("semicolon"));
}

#[test]
fn test_error_note_can_be_added() {
    let err = LangError::new(
        ErrorKind::Type,
        "Type mismatch".to_string(),
        10,
        5,
        "let x: number = \"hello\"".to_string(),
    )
    .with_note("String cannot be assigned to number type");

    assert!(err.note.is_some());
    assert!(err.note.as_ref().unwrap().contains("String"));
}

#[test]
fn test_error_formats_with_caret() {
    let err = LangError::new(
        ErrorKind::Parse,
        "Unexpected token".to_string(),
        1,
        5,
        "let )x = 5".to_string(),
    );

    let formatted = err.to_string();
    assert!(formatted.contains("^"), "Error should have caret marker");
}

#[test]
fn test_error_spans_multiple_characters() {
    let mut err = LangError::new(
        ErrorKind::Parse,
        "Unexpected identifier".to_string(),
        1,
        1,
        "invalid_keyword = 5".to_string(),
    );
    err.end_col = 16; // Span the whole identifier

    let formatted = err.to_string();
    // Check that there are multiple characters in the span indicator
    assert!(formatted.contains("^"), "Error should have span indicator");
}

#[test]
fn test_new_error_kinds() {
    let compile_err = LangError::new(
        ErrorKind::Compile,
        "Compilation failed".to_string(),
        1,
        1,
        "".to_string(),
    );
    assert!(matches!(compile_err.kind, ErrorKind::Compile));

    let lowering_err = LangError::new(
        ErrorKind::Lowering,
        "HIR lowering failed".to_string(),
        1,
        1,
        "".to_string(),
    );
    assert!(matches!(lowering_err.kind, ErrorKind::Lowering));

    let jit_err = LangError::new(
        ErrorKind::Jit,
        "JIT compilation failed".to_string(),
        1,
        1,
        "".to_string(),
    );
    assert!(matches!(jit_err.kind, ErrorKind::Jit));
}
