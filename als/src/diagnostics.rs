//! Diagnostics Provider
//!
//! Provides compiler-quality diagnostics using the compiler's type checker
//! and the semantic engine. Diagnostics include:
//! - Syntax/parse errors (from the recovering parser)
//! - Type errors (from the compiler's type checker)
//! - Semantic warnings (unused variables, unreachable code)
//! - Ownership/borrow checking diagnostics

use crate::analysis::{analyze, error_to_diagnostic, AnalysisResult};
use crate::document::Document;
use adeshlang::parsing::ast::{Stmt, StmtKind};
use adeshlang::semantics::SemanticIndex;
use adeshlang::typesystem::type_system::check_module_in;
use lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag, Location,
    NumberOrString, Position, Range,
};

/// Compute diagnostics for a document using both the semantic engine and the compiler's type checker.
pub fn compute_diagnostics_semantic(index: &SemanticIndex, doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // 1. Parse errors from the semantic engine (recovering parser)
    for error in &index.errors {
        diagnostics.push(error_to_diagnostic_semantic(error, doc));
    }

    // 2. Type checking errors from the compiler's type checker
    let file_path = doc
        .uri
        .to_file_path()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));
    if let Err(type_error) = check_module_in(&doc.content, file_path.as_deref()) {
        diagnostics.push(error_to_diagnostic_semantic(&type_error, doc));
    }

    // 3. Semantic warnings from the AST
    diagnostics.extend(check_unused_variables_semantic(index, doc));
    diagnostics.extend(check_unreachable_code_semantic(index, doc));

    // Deduplicate by (line, col, message)
    diagnostics.dedup_by(|a, b| {
        a.range.start.line == b.range.start.line
            && a.range.start.character == b.range.start.character
            && a.message == b.message
    });

    diagnostics
}

/// Convert a LangError to an LSP Diagnostic with enhanced formatting.
fn error_to_diagnostic_semantic(
    error: &adeshlang::parsing::error::LangError,
    doc: &Document,
) -> Diagnostic {
    let start = Position {
        line: error.line.saturating_sub(1) as u32,
        character: error.col.saturating_sub(1) as u32,
    };

    let end = Position {
        line: error.end_line.saturating_sub(1) as u32,
        character: error.end_col.saturating_sub(1) as u32,
    };

    let severity = match error.kind {
        adeshlang::parsing::error::ErrorKind::Lexical
        | adeshlang::parsing::error::ErrorKind::Parse
        | adeshlang::parsing::error::ErrorKind::Type
        | adeshlang::parsing::error::ErrorKind::Compile => DiagnosticSeverity::ERROR,
        adeshlang::parsing::error::ErrorKind::Runtime
        | adeshlang::parsing::error::ErrorKind::Jit => DiagnosticSeverity::WARNING,
        _ => DiagnosticSeverity::INFORMATION,
    };

    // Build related information from error notes
    let related = if !error.notes.is_empty() {
        Some(
            error
                .notes
                .iter()
                .map(|note| DiagnosticRelatedInformation {
                    location: Location {
                        uri: doc.uri.clone(),
                        range: Range { start, end },
                    },
                    message: note.clone(),
                })
                .collect(),
        )
    } else {
        None
    };

    Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        code: error
            .code
            .as_deref()
            .map(|c| NumberOrString::String(c.to_string())),
        code_description: None,
        source: Some("als".to_string()),
        message: error.message.clone(),
        related_information: related,
        tags: None,
        data: None,
    }
}

/// Check for unused variables using the semantic index.
fn check_unused_variables_semantic(index: &SemanticIndex, _doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sym in &index.symbols {
        if sym.kind != adeshlang::semantics::SemanticSymbolKind::Variable
            && sym.kind != adeshlang::semantics::SemanticSymbolKind::Constant
        {
            continue;
        }

        // Count occurrences in tokens
        let occurrences: Vec<_> = index
            .tokens
            .iter()
            .filter(|t| {
                t.kind == adeshlang::parsing::ast::TokenKind::Identifier && t.lexeme == sym.name
            })
            .collect();

        // If only the declaration exists (1 occurrence), it's unused
        if occurrences.len() <= 1 && sym.line > 0 {
            let start = Position {
                line: (sym.line.saturating_sub(1)) as u32,
                character: (sym.col.saturating_sub(1)) as u32,
            };
            let end = Position {
                line: start.line,
                character: start.character + sym.name.len() as u32,
            };

            diagnostics.push(Diagnostic {
                range: Range { start, end },
                severity: Some(DiagnosticSeverity::HINT),
                code: Some(NumberOrString::String("unused".to_string())),
                code_description: None,
                source: Some("als".to_string()),
                message: format!("unused variable: `{}`", sym.name),
                related_information: None,
                tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                data: None,
            });
        }
    }

    diagnostics
}

/// Check for unreachable code using the semantic index.
fn check_unreachable_code_semantic(index: &SemanticIndex, _doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for stmt in &index.statements {
        check_unreachable_in_stmt(stmt, &mut diagnostics, &index.source);
    }

    diagnostics
}

fn check_unreachable_in_stmt(stmt: &Stmt, diagnostics: &mut Vec<Diagnostic>, source: &str) {
    match &stmt.kind {
        StmtKind::Block(stmts) => {
            let mut found_terminator = false;
            for s in stmts {
                if found_terminator {
                    let start = Position {
                        line: (s.span.line.saturating_sub(1)) as u32,
                        character: (s.span.col.saturating_sub(1)) as u32,
                    };
                    let end = Position {
                        line: start.line,
                        character: start.character + 20,
                    };
                    diagnostics.push(Diagnostic {
                        range: Range { start, end },
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("unreachable".to_string())),
                        code_description: None,
                        source: Some("als".to_string()),
                        message: "unreachable code".to_string(),
                        related_information: None,
                        tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                        data: None,
                    });
                    break;
                }
                if is_terminator(s) {
                    found_terminator = true;
                }
                check_unreachable_in_stmt(s, diagnostics, source);
            }
        }
        StmtKind::Function(func, _) => {
            for s in func.body.iter() {
                check_unreachable_in_stmt(s, diagnostics, source);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            check_unreachable_in_stmt(then_branch, diagnostics, source);
            if let Some(e) = else_branch {
                check_unreachable_in_stmt(e, diagnostics, source);
            }
        }
        StmtKind::While { body, .. } => check_unreachable_in_stmt(body, diagnostics, source),
        StmtKind::ForIn { body, .. } => check_unreachable_in_stmt(body, diagnostics, source),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            check_unreachable_in_stmt(try_block, diagnostics, source);
            check_unreachable_in_stmt(catch_block, diagnostics, source);
        }
        _ => {}
    }
}

fn is_terminator(stmt: &Stmt) -> bool {
    matches!(
        stmt.kind,
        StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue
    )
}

// ===== Legacy API (for backward compatibility) =====

/// Compute diagnostics for a document (legacy API)
#[allow(dead_code)]
pub fn compute_diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let result = analyze(&doc.content);

    for error in &result.errors {
        diagnostics.push(error_to_diagnostic(error));
    }

    diagnostics.extend(check_unused_variables(&result, doc));
    diagnostics.extend(check_unreachable_code(&result, doc));

    diagnostics
}

fn check_unused_variables(result: &AnalysisResult, _doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let symbols =
        crate::analysis::extract_symbols_with_positions(&result.statements, &result.tokens);

    for sym in &symbols {
        if sym.kind != crate::analysis::SymbolKind::Variable
            && sym.kind != crate::analysis::SymbolKind::Constant
        {
            continue;
        }

        let occurrences: Vec<_> = result
            .tokens
            .iter()
            .filter(|t| {
                t.kind == adeshlang::parsing::ast::TokenKind::Identifier && t.lexeme == sym.name
            })
            .collect();

        if occurrences.len() <= 1 && sym.line > 0 {
            let start = Position {
                line: (sym.line.saturating_sub(1)) as u32,
                character: (sym.col.saturating_sub(1)) as u32,
            };
            let end = Position {
                line: start.line,
                character: start.character + sym.name.len() as u32,
            };

            diagnostics.push(Diagnostic {
                range: Range { start, end },
                severity: Some(DiagnosticSeverity::HINT),
                code: None,
                code_description: None,
                source: Some("als".to_string()),
                message: format!("unused variable: `{}`", sym.name),
                related_information: None,
                tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                data: None,
            });
        }
    }

    diagnostics
}

fn check_unreachable_code(result: &AnalysisResult, doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for stmt in &result.statements {
        check_unreachable_in_stmt(stmt, &mut diagnostics, &doc.content);
    }

    diagnostics
}

/// Borrow checker error codes (matching CFG v2.2 spec)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowErrorCode {
    E0501,
    E0502,
    E0503,
    E0504,
    E0505,
    E0506,
    E0507,
    E0508,
    E0509,
    E0510,
    E0511,
    E0512,
    E0513,
    E0514,
}
