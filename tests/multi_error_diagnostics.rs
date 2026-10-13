//! Comprehensive integration tests for multi-error diagnostics and compiler error recovery.

use adeshlang::parsing::error::ErrorKind;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use adeshlang::typesystem::type_system::check_module_in;

// 1. Multiple independent lexer errors
#[test]
fn test_multiple_lexer_errors() {
    let src = "let a = \"\\xZZ\";\nlet b = 10;\nlet c = \"\\xWW\";";
    let mut lexer = Lexer::new(src);
    let (tokens, errors) = lexer.tokenize_recovering();

    assert!(
        errors.len() >= 2,
        "Expected at least 2 lexer errors, got {}",
        errors.len()
    );
    assert!(
        !tokens.is_empty(),
        "Tokens should still be produced after recovery"
    );
}

// 2. Multiple syntax errors in one function
#[test]
fn test_multiple_syntax_errors_in_one_function() {
    let src = r#"
fn test_fn() {
    let a: int = ;
    let b: int = 10;
    let c: int = ;
    return b;
}
"#;
    let mut lexer = Lexer::new(src);
    let (tokens, lex_errs) = lexer.tokenize_recovering();
    assert!(lex_errs.is_empty());

    let mut parser = Parser::new(tokens, Some("test.adesh".to_string()));
    let (_ast, parse_errs) = parser.parse_program_recovering();

    assert!(
        parse_errs.len() >= 2,
        "Expected multiple parser errors in one function, got {}",
        parse_errs.len()
    );
}

// 3. Errors across multiple functions
#[test]
fn test_errors_across_multiple_functions() {
    let src = r#"
fn fn_one() {
    let x: int = ;
}

fn fn_two() {
    let y: int = ;
}
"#;
    let mut lexer = Lexer::new(src);
    let (tokens, _) = lexer.tokenize_recovering();
    let mut parser = Parser::new(tokens, Some("test.adesh".to_string()));
    let (_ast, parse_errs) = parser.parse_program_recovering();

    assert!(
        parse_errs.len() >= 2,
        "Expected errors across both functions, got {}",
        parse_errs.len()
    );
}

// 4. Multiple unresolved identifiers / unknown types
#[test]
fn test_multiple_unknown_types() {
    let src = r#"
let a: NonExistentTypeA = 1;
let b: NonExistentTypeB = 2;
"#;
    let res = check_module_in(src, Some("unknown_types.adesh"));
    assert!(res.is_err(), "Expected type check to fail");
    let err = res.unwrap_err();

    let err_str = err.to_string();
    assert!(
        err_str.contains("NonExistentTypeA"),
        "Should mention NonExistentTypeA in: {}",
        err_str
    );
    assert!(
        err_str.contains("NonExistentTypeB"),
        "Should mention NonExistentTypeB in: {}",
        err_str
    );
}

// 5. Multiple independent type mismatches
#[test]
fn test_multiple_independent_type_mismatches() {
    let src = r#"
let a: int = "hello";
let b: string = 123;
let c: bool = 456;
"#;
    let res = check_module_in(src, Some("mismatches.adesh"));
    assert!(res.is_err());
    let err = res.unwrap_err();

    if let ErrorKind::Multiple(errs) = &err.kind {
        assert!(
            errs.len() >= 3,
            "Expected at least 3 type mismatch errors, got {}",
            errs.len()
        );
    } else {
        panic!("Expected ErrorKind::Multiple, got {:?}", err.kind);
    }
}

// 6. Errors spanning different source files
#[test]
fn test_errors_with_file_spans() {
    let src = "let a: int = \"first\";\nlet b: int = \"second\";";
    let res = check_module_in(src, Some("module_a.adesh"));
    assert!(res.is_err());
    let err = res.unwrap_err();
    let err_str = err.to_string();

    assert!(
        err_str.contains("module_a.adesh"),
        "Error should contain file name"
    );
}

// 7. Syntax errors followed by valid declarations
#[test]
fn test_syntax_errors_followed_by_valid_declarations() {
    let src = r#"
fn broken() {
    let invalid = ;
}

fn valid_one() -> int {
    return 42;
}
"#;
    let mut lexer = Lexer::new(src);
    let (tokens, _) = lexer.tokenize_recovering();
    let mut parser = Parser::new(tokens, None);
    let (ast, errors) = parser.parse_program_recovering();

    assert!(!errors.is_empty(), "Expected error in broken function");
    assert!(
        ast.iter().any(|s| match &s.kind {
            adeshlang::parsing::ast::StmtKind::Function(f, _) => f.name == "valid_one",
            _ => false,
        }),
        "AST should contain valid_one function after recovery"
    );
}

// 8. Cascading-error suppression (declared type preserved)
#[test]
fn test_cascading_error_suppression() {
    // When `x: int = "bad"` has a type mismatch, `x` is still registered as `int`.
    // Therefore `let y: int = x + 1` should NOT trigger a secondary unknown variable error.
    let src = r#"
let x: int = "bad";
let y: int = x + 1;
let z: bool = "also_bad";
"#;
    let res = check_module_in(src, None);
    assert!(res.is_err());
    let err = res.unwrap_err();

    if let ErrorKind::Multiple(errs) = &err.kind {
        // We should have errors for x and z, but NOT a cascade error for y
        assert_eq!(
            errs.len(),
            2,
            "Expected exactly 2 errors (for x and z), got {}: {:?}",
            errs.len(),
            errs
        );
    } else {
        panic!("Expected ErrorKind::Multiple, got {:?}", err.kind);
    }
}

// 9. Diagnostic-count limits
#[test]
fn test_diagnostic_count_limit() {
    // Set custom limit via env var
    unsafe {
        std::env::set_var("ADESH_MAX_ERRORS", "3");
    }
    let src = r#"
let a: int = "err1";
let b: int = "err2";
let c: int = "err3";
let d: int = "err4";
let e: int = "err5";
"#;
    let res = check_module_in(src, None);
    unsafe {
        std::env::remove_var("ADESH_MAX_ERRORS");
    }
    assert!(res.is_err());
    let err = res.unwrap_err();

    if let ErrorKind::Multiple(errs) = &err.kind {
        assert!(
            errs.len() <= 3,
            "Errors should be capped at 3, got {}",
            errs.len()
        );
        assert!(
            err.note
                .as_ref()
                .map(|n| n.contains("limit reached"))
                .unwrap_or(false),
            "Should include note about diagnostic limit reached"
        );
    } else {
        panic!("Expected ErrorKind::Multiple, got {:?}", err.kind);
    }
}

// 10. Mixed error integration test
#[test]
fn test_mixed_error_integration() {
    let src = r#"
let a: UnknownType = 123;
let b: int = "mismatch";
let c: float = "another_mismatch";
"#;
    let res = check_module_in(src, Some("mixed.adesh"));
    assert!(res.is_err());
    let err = res.unwrap_err();

    let err_str = err.to_string();
    assert!(err_str.contains("UnknownType"));
    assert!(err_str.contains("mismatch"));
    assert!(err_str.contains("another_mismatch"));
}

// 11. Valid programs compile cleanly without errors
#[test]
fn test_valid_program_succeeds() {
    let src = r#"
fn add(a: int, b: int) -> int {
    return a + b;
}

let sum: int = add(10, 20);
"#;
    let res = check_module_in(src, Some("valid.adesh"));
    assert!(res.is_ok(), "Valid program should succeed: {:?}", res.err());
}

// 12. Replicating the user's main.adesh scenario with 3 errors
#[test]
fn test_user_reported_three_syntax_errors() {
    let src = r#"let a = 10;
let b = 20;
if(a==b){
    print("true")
}else{
    print("false"
}

print("The value of a is: " + a)
"#;
    let mut lexer = Lexer::new(src);
    let (tokens, _) = lexer.tokenize_recovering();
    let mut parser = Parser::new(tokens, Some("main.adesh".to_string()));
    let (_, errors) = parser.parse_program_recovering();

    assert_eq!(
        errors.len(),
        3,
        "Expected exactly 3 syntax errors in user program, got {}: {:?}",
        errors.len(),
        errors
    );
}
