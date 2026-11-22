//! Code Lens Provider
//!
//! Displays actionable inline lenses above declarations (like rust-analyzer / TypeScript):
//! - `N references` above functions, classes, structs, enums, and interfaces
//! - `N implementations` above interfaces and base classes
//! - `▶ Run` above `fn main()`
//! - `▶ Run Test` above `test` / `@test` functions

use crate::config::AlsConfig;
use crate::document::Document;
use crate::workspace::WorkspaceIndex;
use adeshlang::parsing::ast::StmtKind;
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind};
use lsp_types::{CodeLens, Command, Position, Range};

/// Compute code lenses for a document.
pub fn get_code_lenses(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    config: &AlsConfig,
) -> Vec<CodeLens> {
    if !config.code_lens.enabled {
        return Vec::new();
    }

    let mut lenses = Vec::new();

    // 1. Run / Run Test lenses from AST statements
    if config.code_lens.run_tests {
        for stmt in &index.statements {
            if let StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) = &stmt.kind
            {
                let line = stmt.span.line.saturating_sub(1) as u32;
                let range = Range::new(Position::new(line, 0), Position::new(line, 0));

                if func.name == "main" {
                    lenses.push(CodeLens {
                        range,
                        command: Some(Command {
                            title: "▶ Run".to_string(),
                            command: "adesh.runFile".to_string(),
                            arguments: Some(vec![serde_json::json!(doc.uri.to_string())]),
                        }),
                        data: None,
                    });
                }

                let has_test_decorator = func.decorators.iter().any(|d| {
                    matches!(&d.kind, adeshlang::parsing::ast::ExprKind::Variable(n) if n == "test")
                });
                if func.is_test || has_test_decorator {
                    lenses.push(CodeLens {
                        range,
                        command: Some(Command {
                            title: format!("▶ Run Test `{}`", func.name),
                            command: "adesh.runTest".to_string(),
                            arguments: Some(vec![
                                serde_json::json!(doc.uri.to_string()),
                                serde_json::json!(func.name),
                            ]),
                        }),
                        data: None,
                    });
                }
            }
        }
    }

    // 2. Reference count and implementation count lenses on top-level declarations
    for sym in &index.symbols {
        if sym.parent_type.is_some() || sym.line == 0 {
            continue;
        }
        let eligible = matches!(
            sym.kind,
            SemanticSymbolKind::Function
                | SemanticSymbolKind::Class
                | SemanticSymbolKind::Struct
                | SemanticSymbolKind::Enum
                | SemanticSymbolKind::Interface
                | SemanticSymbolKind::TypeAlias
        );
        if !eligible {
            continue;
        }

        let line = sym.line.saturating_sub(1) as u32;
        let col = sym.col.saturating_sub(1) as u32;
        let range = Range::new(
            Position::new(line, col),
            Position::new(line, col + sym.name.len() as u32),
        );

        if config.code_lens.references {
            let all_refs = workspace.find_references_workspace(&sym.name);
            // Exclude the declaration itself
            let ref_count = all_refs
                .iter()
                .filter(|(u, l, c, _)| !(u == &doc.uri && *l == sym.line && *c == sym.col))
                .count();
            let title = if ref_count == 1 {
                "1 reference".to_string()
            } else {
                format!("{} references", ref_count)
            };
            lenses.push(CodeLens {
                range,
                command: Some(Command {
                    title,
                    command: "adesh.showReferences".to_string(),
                    arguments: Some(vec![
                        serde_json::json!(doc.uri.to_string()),
                        serde_json::json!({ "line": line, "character": col }),
                    ]),
                }),
                data: None,
            });
        }

        if config.code_lens.implementations
            && matches!(
                sym.kind,
                SemanticSymbolKind::Interface
                    | SemanticSymbolKind::Class
                    | SemanticSymbolKind::Struct
            )
        {
            let impls = workspace.find_implementations(&sym.name);
            if !impls.is_empty() || sym.kind == SemanticSymbolKind::Interface {
                let count = impls.len();
                let title = if count == 1 {
                    "1 implementation".to_string()
                } else {
                    format!("{} implementations", count)
                };
                lenses.push(CodeLens {
                    range,
                    command: Some(Command {
                        title,
                        command: "adesh.showImplementations".to_string(),
                        arguments: Some(vec![
                            serde_json::json!(doc.uri.to_string()),
                            serde_json::json!({ "line": line, "character": col }),
                        ]),
                    }),
                    data: None,
                });
            }
        }
    }

    lenses
}
