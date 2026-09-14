//! Symbol Navigation: Go-to-Definition, Find References, Rename, Document Symbols
//!
//! Uses the semantic engine's symbol table for semantic resolution,
//! not text matching. Supports cross-file navigation via the workspace index.

#![allow(deprecated)]

use crate::document::Document;
use crate::workspace::WorkspaceIndex;
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind, SymbolEntry};
use lsp_types::{
    DocumentSymbol, Location, Position, Range, SymbolKind, TextEdit, Url, WorkspaceEdit,
};

/// Find the definition of a symbol at a position using semantic resolution.
pub fn find_definition_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Option<Location> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    let identifier = index.identifier_at(internal_line, internal_col)?;

    // Find the declaration in the current file's index
    if let Some(sym) = index.find_declaration(identifier) {
        return Some(symbol_to_location(sym, &doc.uri));
    }

    // Search workspace for cross-file definitions
    if let Some((uri, sym)) = workspace.find_declaration_workspace(identifier) {
        return Some(symbol_to_location(sym, uri));
    }

    // Try to resolve as a type name
    if let Some(td) = index.get_type(identifier) {
        let start = Position {
            line: (td.line.saturating_sub(1)) as u32,
            character: (td.col.saturating_sub(1)) as u32,
        };
        let end = Position {
            line: start.line,
            character: start.character + identifier.len() as u32,
        };
        return Some(Location {
            uri: doc.uri.clone(),
            range: Range { start, end },
        });
    }

    None
}

/// Find all references to a symbol using semantic resolution.
pub fn find_references_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    _doc: &Document,
    line: u32,
    col: u32,
    include_declaration: bool,
) -> Vec<Location> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    let identifier = match index.identifier_at(internal_line, internal_col) {
        Some(id) => id,
        None => return vec![],
    };

    let mut locations = Vec::new();

    let refs = workspace.find_references_workspace(identifier);

    for (uri, ref_line, ref_col, ref_end_col) in refs {
        if !include_declaration {
            if let Some(decl) = index.find_declaration(identifier) {
                if decl.line == ref_line && decl.col == ref_col {
                    continue;
                }
            }
        }

        let start = Position {
            line: (ref_line.saturating_sub(1)) as u32,
            character: (ref_col.saturating_sub(1)) as u32,
        };
        let end = Position {
            line: start.line,
            character: (ref_end_col.saturating_sub(1)) as u32,
        };

        locations.push(Location {
            uri: uri.clone(),
            range: Range { start, end },
        });
    }

    locations
}

/// Prepare for a rename operation.
pub fn prepare_rename_semantic(
    index: &SemanticIndex,
    _doc: &Document,
    line: u32,
    col: u32,
) -> Option<Range> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    let token = index.token_at(internal_line, internal_col)?;
    if !matches!(token.kind, adeshlang::parsing::ast::TokenKind::Identifier) {
        return None;
    }

    let start = Position {
        line: (token.line.saturating_sub(1)) as u32,
        character: (token.col.saturating_sub(1)) as u32,
    };
    let end = Position {
        line: start.line,
        character: start.character + token.lexeme.len() as u32,
    };

    Some(Range { start, end })
}

/// Perform a semantic rename across the workspace.
pub fn do_rename_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    _doc: &Document,
    line: u32,
    col: u32,
    new_name: &str,
) -> WorkspaceEdit {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    let identifier = match index.identifier_at(internal_line, internal_col) {
        Some(id) => id.to_string(),
        None => {
            return WorkspaceEdit {
                changes: None,
                document_changes: None,
                change_annotations: None,
            }
        }
    };

    let refs = workspace.find_references_workspace(&identifier);

    let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
        std::collections::HashMap::new();

    for (uri, ref_line, ref_col, ref_end_col) in refs {
        let start = Position {
            line: (ref_line.saturating_sub(1)) as u32,
            character: (ref_col.saturating_sub(1)) as u32,
        };
        let end = Position {
            line: start.line,
            character: (ref_end_col.saturating_sub(1)) as u32,
        };

        changes.entry(uri.clone()).or_default().push(TextEdit {
            range: Range { start, end },
            new_text: new_name.to_string(),
        });
    }

    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

/// Get document symbols using the semantic engine.
pub fn get_document_symbols_semantic(index: &SemanticIndex) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    for sym in &index.symbols {
        if sym.parent_type.is_some() {
            continue;
        }
        if sym.kind == SemanticSymbolKind::Parameter || sym.kind == SemanticSymbolKind::Import {
            continue;
        }

        let kind = semantic_kind_to_lsp_kind(sym.kind);
        let start = Position {
            line: (sym.line.saturating_sub(1)) as u32,
            character: (sym.col.saturating_sub(1)) as u32,
        };
        let end = Position {
            line: start.line,
            character: start.character + sym.name.len() as u32,
        };

        let children = if sym.kind.is_type() {
            get_type_children(index, &sym.name)
        } else {
            vec![]
        };

        let detail = sym.signature.clone().or_else(|| {
            sym.type_annotation
                .as_ref()
                .map(|t| format!("{}: {}", sym.name, t))
        });

        symbols.push(DocumentSymbol {
            name: sym.name.clone(),
            detail,
            kind,
            tags: None,
            deprecated: None,
            range: Range { start, end },
            selection_range: Range { start, end },
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        });
    }

    symbols
}

/// Get child symbols for a type (fields, methods, variants).
fn get_type_children(index: &SemanticIndex, type_name: &str) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();

    for sym in &index.symbols {
        if sym.parent_type.as_deref() == Some(type_name) {
            let kind = semantic_kind_to_lsp_kind(sym.kind);
            let start = Position {
                line: (sym.line.saturating_sub(1)) as u32,
                character: (sym.col.saturating_sub(1)) as u32,
            };
            let end = Position {
                line: start.line,
                character: start.character + sym.name.len() as u32,
            };

            children.push(DocumentSymbol {
                name: sym.name.clone(),
                detail: sym.signature.clone().or_else(|| {
                    sym.type_annotation
                        .as_ref()
                        .map(|t| format!("{}: {}", sym.name, t))
                }),
                kind,
                tags: None,
                deprecated: None,
                range: Range { start, end },
                selection_range: Range { start, end },
                children: None,
            });
        }
    }

    if let Some(td) = index.get_type(type_name) {
        for (fname, ftype, _) in &td.fields {
            if children.iter().any(|c| c.name == *fname) {
                continue;
            }
            children.push(DocumentSymbol {
                name: fname.clone(),
                detail: Some(format!("{}: {}", fname, ftype)),
                kind: SymbolKind::FIELD,
                tags: None,
                deprecated: None,
                range: Range::new(Position::new(0, 0), Position::new(0, fname.len() as u32)),
                selection_range: Range::new(
                    Position::new(0, 0),
                    Position::new(0, fname.len() as u32),
                ),
                children: None,
            });
        }

        for m in &td.methods {
            if children.iter().any(|c| c.name == m.name) {
                continue;
            }
            children.push(DocumentSymbol {
                name: m.name.clone(),
                detail: m.signature.clone(),
                kind: SymbolKind::METHOD,
                tags: None,
                deprecated: None,
                range: Range::new(Position::new(0, 0), Position::new(0, m.name.len() as u32)),
                selection_range: Range::new(
                    Position::new(0, 0),
                    Position::new(0, m.name.len() as u32),
                ),
                children: None,
            });
        }

        for (vname, _) in &td.variants {
            if children.iter().any(|c| c.name == *vname) {
                continue;
            }
            children.push(DocumentSymbol {
                name: vname.clone(),
                detail: None,
                kind: SymbolKind::ENUM_MEMBER,
                tags: None,
                deprecated: None,
                range: Range::new(Position::new(0, 0), Position::new(0, vname.len() as u32)),
                selection_range: Range::new(
                    Position::new(0, 0),
                    Position::new(0, vname.len() as u32),
                ),
                children: None,
            });
        }
    }

    children
}

fn semantic_kind_to_lsp_kind(kind: SemanticSymbolKind) -> SymbolKind {
    match kind {
        SemanticSymbolKind::Variable => SymbolKind::VARIABLE,
        SemanticSymbolKind::Constant => SymbolKind::CONSTANT,
        SemanticSymbolKind::Parameter => SymbolKind::VARIABLE,
        SemanticSymbolKind::Function => SymbolKind::FUNCTION,
        SemanticSymbolKind::Method => SymbolKind::METHOD,
        SemanticSymbolKind::StaticMethod => SymbolKind::METHOD,
        SemanticSymbolKind::Field => SymbolKind::FIELD,
        SemanticSymbolKind::Property => SymbolKind::PROPERTY,
        SemanticSymbolKind::Class => SymbolKind::CLASS,
        SemanticSymbolKind::Struct => SymbolKind::STRUCT,
        SemanticSymbolKind::Enum => SymbolKind::ENUM,
        SemanticSymbolKind::EnumVariant => SymbolKind::ENUM_MEMBER,
        SemanticSymbolKind::Interface => SymbolKind::INTERFACE,
        SemanticSymbolKind::TypeAlias => SymbolKind::TYPE_PARAMETER,
        SemanticSymbolKind::Module => SymbolKind::MODULE,
        SemanticSymbolKind::Import => SymbolKind::MODULE,
    }
}

fn symbol_to_location(sym: &SymbolEntry, uri: &Url) -> Location {
    let start = Position {
        line: (sym.line.saturating_sub(1)) as u32,
        character: (sym.col.saturating_sub(1)) as u32,
    };
    let end = Position {
        line: start.line,
        character: start.character + sym.name.len() as u32,
    };
    Location {
        uri: uri.clone(),
        range: Range { start, end },
    }
}

// ===== Legacy API (for backward compatibility) =====

use crate::analysis::{analyze, extract_symbols_with_positions};

/// Find definition (legacy API)
#[allow(dead_code)]
pub fn find_definition(doc: &Document, line: u32, col: u32) -> Option<Location> {
    let word = doc.get_word_at(line, col)?;
    let result = analyze(&doc.content);
    let symbols = extract_symbols_with_positions(&result.statements, &result.tokens);

    for sym in &symbols {
        if sym.name == word {
            let start = Position {
                line: (sym.line.saturating_sub(1)) as u32,
                character: (sym.col.saturating_sub(1)) as u32,
            };
            let end = Position {
                line: (sym.end_line.saturating_sub(1)) as u32,
                character: (sym.end_col.saturating_sub(1)) as u32,
            };
            return Some(Location {
                uri: doc.uri.clone(),
                range: Range { start, end },
            });
        }
    }
    None
}

/// Find references (legacy API)
#[allow(dead_code)]
pub fn find_references(
    doc: &Document,
    line: u32,
    col: u32,
    _include_declaration: bool,
) -> Vec<Location> {
    let word = match doc.get_word_at(line, col) {
        Some(w) => w,
        None => return vec![],
    };

    let result = analyze(&doc.content);
    let occurrences = crate::analysis::find_all_identifier_occurrences(&result.tokens, &word);

    occurrences
        .into_iter()
        .map(|(l, c)| {
            let start = Position {
                line: (l.saturating_sub(1)) as u32,
                character: (c.saturating_sub(1)) as u32,
            };
            let end = Position {
                line: start.line,
                character: start.character + word.len() as u32,
            };
            Location {
                uri: doc.uri.clone(),
                range: Range { start, end },
            }
        })
        .collect()
}

/// Get document symbols (legacy API)
#[allow(dead_code)]
pub fn get_document_symbols(doc: &Document) -> Vec<DocumentSymbol> {
    let result = analyze(&doc.content);
    let symbols = extract_symbols_with_positions(&result.statements, &result.tokens);

    symbols
        .iter()
        .filter_map(|sym| {
            if sym.line == 0 {
                return None;
            }

            let kind = match sym.kind {
                crate::analysis::SymbolKind::Function => SymbolKind::FUNCTION,
                crate::analysis::SymbolKind::Class => SymbolKind::CLASS,
                crate::analysis::SymbolKind::Variable => SymbolKind::VARIABLE,
                crate::analysis::SymbolKind::Constant => SymbolKind::CONSTANT,
                crate::analysis::SymbolKind::Parameter => SymbolKind::VARIABLE,
                crate::analysis::SymbolKind::Method => SymbolKind::METHOD,
                crate::analysis::SymbolKind::Property => SymbolKind::PROPERTY,
                crate::analysis::SymbolKind::Enum => SymbolKind::ENUM,
                crate::analysis::SymbolKind::Interface => SymbolKind::INTERFACE,
                crate::analysis::SymbolKind::Module => SymbolKind::MODULE,
            };

            let start = Position {
                line: (sym.line.saturating_sub(1)) as u32,
                character: (sym.col.saturating_sub(1)) as u32,
            };
            let end = Position {
                line: (sym.end_line.saturating_sub(1)) as u32,
                character: (sym.end_col.saturating_sub(1)) as u32,
            };

            Some(DocumentSymbol {
                name: sym.name.clone(),
                detail: sym.signature.clone(),
                kind,
                tags: None,
                deprecated: None,
                range: Range { start, end },
                selection_range: Range { start, end },
                children: None,
            })
        })
        .collect()
}

/// Prepare rename (legacy API)
#[allow(dead_code)]
pub fn prepare_rename(doc: &Document, line: u32, col: u32) -> Option<Range> {
    let word = doc.get_word_at(line, col)?;
    let _line_text = doc.get_line(line as usize)?;

    let start = Position {
        line,
        character: col.saturating_sub(1),
    };
    let end = Position {
        line,
        character: start.character + word.len() as u32,
    };
    Some(Range { start, end })
}

/// Do rename (legacy API)
#[allow(dead_code)]
pub fn do_rename(doc: &Document, line: u32, col: u32, new_name: &str) -> Vec<(Range, String)> {
    let word = match doc.get_word_at(line, col) {
        Some(w) => w,
        None => return vec![],
    };

    let result = analyze(&doc.content);
    let occurrences = crate::analysis::find_all_identifier_occurrences(&result.tokens, &word);

    occurrences
        .into_iter()
        .map(|(l, c)| {
            let start = Position {
                line: (l.saturating_sub(1)) as u32,
                character: (c.saturating_sub(1)) as u32,
            };
            let end = Position {
                line: start.line,
                character: start.character + word.len() as u32,
            };
            (Range { start, end }, new_name.to_string())
        })
        .collect()
}
