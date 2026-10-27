//! Integration tests for Adesh diagnostics system:
//! - Explanations (`--explain <CODE>` and `--explain list`)
//! - JSON serialization (`schema_version = "1.0.0"`)
//! - Terminal rendering with/without ANSI colors
//! - Source map visual column calculation
//! - LangError bridge

use adeshlang::diagnostics::{
    Diagnostic, DiagnosticJsonReport, DiagnosticPhase, DiagnosticSeverity, Label, SourceMap, Span,
    TerminalRenderer, explain_code, list_diagnostics,
};
use adeshlang::parsing::error::{ErrorKind, LangError};

#[test]
fn test_explain_rust_compatible_code() {
    let result = explain_code("E0308");
    assert!(result.is_ok(), "E0308 should be a registered code");
    let expl = result.unwrap();
    assert!(
        expl.contains("E0308 — Mismatched types"),
        "Explanation must have title"
    );
    assert!(expl.contains("Rust-equivalent (verified semantic mapping)"));
    assert!(expl.contains("Example of Erroneous Code:"));
    assert!(expl.contains("Suggested Fixes:"));
}

#[test]
fn test_explain_adesh_specific_code() {
    let result = explain_code("AD0001");
    assert!(result.is_ok(), "AD0001 should be a registered code");
    let expl = result.unwrap();
    assert!(expl.contains("AD0001 — Unrecognized character"));
    assert!(expl.contains("Adesh-specific") || expl.contains("AdeshLang-specific"));
    assert!(expl.contains("lexer"));
}

#[test]
fn test_explain_unknown_code_fails() {
    let result = explain_code("E9999");
    assert!(result.is_err(), "E9999 must return an error");
    let err = result.unwrap_err();
    assert!(err.contains("unknown diagnostic code `E9999`"));
    assert!(err.contains("adesh --explain list"));
}

#[test]
fn test_explain_list_contains_all_codes() {
    let list = list_diagnostics();
    assert!(
        list.contains("Adesh Registered Diagnostic Codes:")
            || list.contains("AdeshLang Registered Diagnostic Codes:")
    );
    assert!(list.contains("E0308"));
    assert!(list.contains("E0382"));
    assert!(list.contains("E0425"));
    assert!(list.contains("AD0001"));
    assert!(list.contains("AD0002"));
    assert!(list.contains("AD0003"));
    assert!(list.contains("AD0004"));
    assert!(list.contains("AD0005"));
    assert!(list.contains("AD0006"));
    assert!(list.contains("AD0007"));
}

#[test]
fn test_json_report_format() {
    let diag = Diagnostic::new(
        DiagnosticSeverity::Error,
        DiagnosticPhase::TypeCheck,
        "mismatched types: expected i32, found string",
    )
    .with_code("E0308")
    .with_file("test.adesh")
    .with_primary_span(Span::new(4, 18, 4, 24, "let count: i32 = \"seven\";"))
    .with_note("this declaration requires an integer value")
    .with_help("provide an integer or change the declared type");

    let report = DiagnosticJsonReport::new(vec![diag]);
    let json = report
        .to_json()
        .expect("Serialization to JSON must succeed");

    assert!(json.contains(r#""schema_version":"1.0.0""#));
    assert!(json.contains(r#""error_count":1"#));
    assert!(json.contains(r#""code":"E0308""#));
    assert!(json.contains(r#""phase":"typecheck""#));
    assert!(json.contains(r#""line_text":"let count: i32 = \"seven\";""#));
}

#[test]
fn test_terminal_renderer_rust_style() {
    let diag = Diagnostic::new(
        DiagnosticSeverity::Error,
        DiagnosticPhase::TypeCheck,
        "mismatched types",
    )
    .with_code("E0308")
    .with_file("examples/types.adesh")
    .with_primary_span(Span::new(4, 18, 4, 24, "let count: i32 = \"seven\";"))
    .with_label(Label::primary(
        Span::new(4, 18, 4, 24, "let count: i32 = \"seven\";"),
        "expected `i32`, found string",
    ))
    .with_note("this declaration requires an integer value")
    .with_help("provide an integer or change the declared type");

    let renderer = TerminalRenderer::plain();
    let rendered = renderer.render(&diag);

    assert!(rendered.contains("error[E0308]: mismatched types"));
    assert!(rendered.contains("--> examples/types.adesh:4:18"));
    assert!(rendered.contains("4 | let count: i32 = \"seven\";"));
    assert!(rendered.contains("^^^^^^^ expected `i32`, found string"));
    assert!(rendered.contains("= note: this declaration requires an integer value"));
    assert!(rendered.contains("= help: provide an integer or change the declared type"));
    assert!(rendered.contains("= help: run `adesh --explain E0308` for more information"));
}

#[test]
fn test_sourcemap_multiline_and_tab_stops() {
    let source = "fn main() {\n\tlet x = 42;\n\tlet y = \"hello\";\n}";
    let sm = SourceMap::new(source.to_string(), Some("main.adesh".to_string()));

    assert_eq!(sm.line_count(), 4);
    let line2 = sm.get_line(2).expect("line 2 should exist");
    assert_eq!(line2, "\tlet x = 42;");

    // \t with tab width 4 expands to col 5
    let visual_col = SourceMap::visual_column(line2, 2, 4);
    assert_eq!(visual_col, 5); // 4 spaces for \t, so col 2 starts at visual col 5
}

#[test]
fn test_lang_error_to_diagnostic_conversion() {
    let err = LangError::new(
        ErrorKind::Type,
        "mismatched types".to_string(),
        10,
        5,
        "let x: int = \"hello\";".to_string(),
    )
    .with_code("E0308")
    .with_file("types.adesh".to_string())
    .with_hint("cast or provide an integer");

    let diag = err.to_diagnostic();
    assert_eq!(diag.code, Some("E0308".to_string()));
    assert_eq!(diag.severity, DiagnosticSeverity::Error);
    assert_eq!(diag.phase, DiagnosticPhase::TypeCheck);
    assert_eq!(diag.file, Some("types.adesh".to_string()));
    assert!(diag.primary_span.is_some());
    let span = diag.primary_span.unwrap();
    assert_eq!(span.start_line, 10);
    assert_eq!(span.start_col, 5);
}
