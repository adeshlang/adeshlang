//! Phase 10 LSP & Compiler Server Foundation E2E Test Suite.
//!
//! Validates:
//! - LSP hover type query.
//! - Diagnostic emission on syntax errors.

#![allow(dead_code, unused_imports)]

use adesh_codegen::compiler_server::CompilerServer;

#[test]
fn test_lsp_server_hover_and_diagnostics() {
    let mut server = CompilerServer::new();

    let res = server.compile_module("math", "fn compute() -> void {}");
    assert!(res.is_ok());

    let hover = server.query_hover("math", "compute");
    assert!(hover.is_some());
    assert!(hover.unwrap().contains("compute"));

    let err_res = server.compile_module("bad", "syntax_error_intentional");
    assert!(err_res.is_err());
    let diags = err_res.unwrap_err();
    assert_eq!(diags[0].severity, "Error");
}
