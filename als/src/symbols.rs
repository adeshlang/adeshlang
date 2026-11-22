//! Symbol Navigation:
//! - Go-to-Definition (imports, member access, local scope, cross-file symbols & types)
//! - Go-to-Type-Definition (annotated & inferred types, members, generics, aliases, cross-file)
//! - Go-to-Implementation (interfaces -> implementors, base classes -> subclasses, method overrides)
//! - Find References (scope-aware for locals, workspace-wide for top-level symbols)
//! - Document Highlights (distinguishes READ vs WRITE)
//! - Safe Semantic Rename (validates target identifier, rejects keywords/builtins)
//! - Nested Document Symbols with full enclosing ranges (breadcrumbs & sticky scroll)

#![allow(deprecated)]

use crate::document::Document;
use crate::type_utils::{
    base_type_name, enclosing_function_span, enclosing_type_at_line, extract_member_access_at,
    extract_nominal_type_name, find_best_declaration_for_line, find_block_end_line,
    find_builtin_fn, is_reserved_keyword, is_valid_identifier, resolve_expression_chain_type,
    resolve_type_str, symbol_resolved_type,
};
use crate::workspace::WorkspaceIndex;
use adeshlang::parsing::ast::TokenKind;
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind, SymbolEntry};
use lsp_types::{
    DocumentHighlight, DocumentHighlightKind, DocumentSymbol, Location, Position, Range,
    SymbolKind, TextEdit, Url, WorkspaceEdit,
};

// ============================================================================
// Go-to-Definition
// ============================================================================

/// Find the definition of a symbol, member, type, or import path at a 0-based LSP position.
pub fn find_definition_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Option<Location> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    // 1. Import path string literal -> jump to target module file
    if let Some(target_uri) = import_target_at_position(index, workspace, doc, line, col) {
        return Some(Location {
            uri: target_uri,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        });
    }

    // 2. Member access (`user.save()`, `user.name`, `this.id`, `super.init()`, `Color.Red`)
    if let Some((receiver, member)) = extract_member_access_at(doc, line, col) {
        if let Some(ty) = resolve_expression_chain_type(index, workspace, &receiver, internal_line)
        {
            if let Some(type_name) = extract_nominal_type_name(&ty) {
                if let Some((uri, m_line, m_col, m_len)) =
                    workspace.find_member_location(&type_name, &member)
                {
                    return Some(span_to_location(&uri, m_line, m_col, m_len));
                }
            }
        }
        // Static/enum access fallback (`Color.Red`)
        if let Some((uri, m_line, m_col, m_len)) =
            workspace.find_member_location(&receiver, &member)
        {
            return Some(span_to_location(&uri, m_line, m_col, m_len));
        }
    }

    let identifier = index.identifier_at(internal_line, internal_col)?;

    // 3. Imported symbol name (`import { User } from "./models"` or using `User` from an import)
    for imp in &index.imports {
        let matches_import = imp.names.iter().any(|n| n == identifier) || imp.alias == identifier;
        if matches_import {
            if let Some(target_uri) = workspace.resolve_module(&doc.uri, &imp.path) {
                if let Some(target_idx) = workspace.get_index(&target_uri) {
                    if let Some(sym) = target_idx.find_declaration(identifier) {
                        return Some(symbol_to_location(sym, &target_uri));
                    }
                    if let Some(td) = target_idx.get_type(identifier) {
                        return Some(span_to_location(
                            &target_uri,
                            td.line,
                            td.col,
                            td.name.len(),
                        ));
                    }
                }
                if imp.alias == identifier {
                    return Some(Location {
                        uri: target_uri,
                        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                    });
                }
            }
        }
    }

    // 4. Scope-aware declaration in the current file
    if let Some(sym) = find_best_declaration_for_line(index, identifier, internal_line) {
        if sym.kind != SemanticSymbolKind::Import {
            return Some(symbol_to_location(sym, &doc.uri));
        }
    }

    // 5. Type definition in current file
    if let Some(td) = index.get_type(identifier) {
        return Some(span_to_location(&doc.uri, td.line, td.col, td.name.len()));
    }

    // 6. Cross-file symbol declaration across workspace
    if let Some((uri, sym)) = workspace.find_declaration_workspace(identifier) {
        return Some(symbol_to_location(sym, uri));
    }

    // 7. Cross-file type definition across workspace
    if let Some((uri, td)) = workspace.find_type_location(identifier) {
        return Some(span_to_location(uri, td.line, td.col, td.name.len()));
    }

    None
}

// ============================================================================
// Go-to-Type-Definition
// ============================================================================

/// Find the type definition of the symbol or expression under the cursor.
/// Supports explicit annotations, inferred types, member accesses, call returns,
/// generic containers (`Array<User>`, `Option<User>`), type aliases, and cross-file types.
pub fn find_type_definition_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Option<Location> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    // 1. Member access: `user.address` -> jump to `Address`
    if let Some((receiver, member)) = extract_member_access_at(doc, line, col) {
        let full_expr = format!("{}.{}", receiver, member);
        if let Some(ty) = resolve_expression_chain_type(index, workspace, &full_expr, internal_line)
        {
            if let Some(loc) = resolve_ty_to_location(index, workspace, &doc.uri, &ty) {
                return Some(loc);
            }
        }
    }

    let identifier = index.identifier_at(internal_line, internal_col)?;

    // 2. If cursor is already on a type name itself (including type alias -> resolve target or self)
    if let Some(alias_ty) = index.aliases.get(identifier) {
        if let Some(loc) = resolve_ty_to_location(index, workspace, &doc.uri, alias_ty) {
            return Some(loc);
        }
    }
    if let Some(td) = index.get_type(identifier) {
        return Some(span_to_location(&doc.uri, td.line, td.col, td.name.len()));
    }
    if let Some((uri, td)) = workspace.find_type_location(identifier) {
        return Some(span_to_location(uri, td.line, td.col, td.name.len()));
    }

    // 3. `this` / `self` / `super`
    if identifier == "this" || identifier == "self" {
        if let Some(cls) = enclosing_type_at_line(index, internal_line) {
            return lookup_type_location(index, workspace, &doc.uri, &cls);
        }
    }
    if identifier == "super" {
        if let Some(cls) = enclosing_type_at_line(index, internal_line) {
            if let Some(td) = index
                .get_type(&cls)
                .or_else(|| workspace.find_type_definition(&cls))
            {
                if let Some(parent) = &td.extends {
                    return lookup_type_location(index, workspace, &doc.uri, parent);
                }
            }
        }
    }

    // 4. Local/workspace variable, parameter, or function return type
    let decl = find_best_declaration_for_line(index, identifier, internal_line).or_else(|| {
        workspace
            .find_declaration_workspace(identifier)
            .map(|(_, s)| s)
    });

    if let Some(sym) = decl {
        // Explicit type annotation (e.g. `let users: Array<User>`, `let u: User?`)
        if let Some(ann) = &sym.type_annotation {
            let base = base_type_name(ann);
            if let Some(loc) = lookup_type_location(index, workspace, &doc.uri, base) {
                return Some(loc);
            }
            if let Some(ty) = resolve_type_str(index, workspace, ann) {
                if let Some(loc) = resolve_ty_to_location(index, workspace, &doc.uri, &ty) {
                    return Some(loc);
                }
            }
        }
        // Function/method return type (e.g. cursor on `get_user` -> jump to `User`)
        if let Some(ret) = &sym.return_type {
            let base = base_type_name(ret);
            if let Some(loc) = lookup_type_location(index, workspace, &doc.uri, base) {
                return Some(loc);
            }
        }
        // Inferred type (e.g. `let u = User(1, "Ajay")` with no explicit annotation)
        if let Some(ty) = symbol_resolved_type(index, workspace, sym) {
            if let Some(loc) = resolve_ty_to_location(index, workspace, &doc.uri, &ty) {
                return Some(loc);
            }
        }
    }

    // 5. Semantic engine fallback
    if let Some(ty) = index.resolve_symbol_type(identifier) {
        if let Some(loc) = resolve_ty_to_location(index, workspace, &doc.uri, &ty) {
            return Some(loc);
        }
    }

    None
}

fn resolve_ty_to_location(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    current_uri: &Url,
    ty: &adeshlang::typesystem::checker::Ty,
) -> Option<Location> {
    let type_name = extract_nominal_type_name(ty)?;
    lookup_type_location(index, workspace, current_uri, &type_name)
}

fn lookup_type_location(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    current_uri: &Url,
    type_name: &str,
) -> Option<Location> {
    if let Some(td) = index.get_type(type_name) {
        return Some(span_to_location(
            current_uri,
            td.line,
            td.col,
            td.name.len(),
        ));
    }
    if let Some((uri, td)) = workspace.find_type_location(type_name) {
        return Some(span_to_location(uri, td.line, td.col, td.name.len()));
    }
    None
}

// ============================================================================
// Go-to-Implementation
// ============================================================================

/// Find implementations of an interface, subclasses/extensions of a class/struct,
/// or overriding methods across the workspace.
pub fn find_implementations_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Vec<Location> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    // Member call `drawable.draw()` -> find implementations of `draw` on all implementors of `Drawable`
    if let Some((receiver, member)) = extract_member_access_at(doc, line, col) {
        if let Some(ty) = resolve_expression_chain_type(index, workspace, &receiver, internal_line)
        {
            if let Some(owner) = extract_nominal_type_name(&ty) {
                let method_impls = workspace.find_method_implementations(&owner, &member);
                if !method_impls.is_empty() {
                    return method_impls
                        .into_iter()
                        .map(|(uri, l, c, len)| span_to_location(&uri, l, c, len))
                        .collect();
                }
            }
        }
    }

    let Some(identifier) = index.identifier_at(internal_line, internal_col) else {
        return Vec::new();
    };

    // If cursor is on an interface or class/struct method declaration
    if let Some(sym) = index
        .symbols
        .iter()
        .find(|s| s.name == identifier && s.line == internal_line && s.parent_type.is_some())
    {
        if let Some(owner) = &sym.parent_type {
            let method_impls = workspace.find_method_implementations(owner, identifier);
            if !method_impls.is_empty() {
                return method_impls
                    .into_iter()
                    .map(|(uri, l, c, len)| span_to_location(&uri, l, c, len))
                    .collect();
            }
        }
    }

    // Type-level implementations (`implements`, `extends`, `extend on`)
    let impls = workspace.find_implementations(identifier);
    impls
        .into_iter()
        .map(|(uri, l, c, name)| span_to_location(&uri, l, c, name.len()))
        .collect()
}

// ============================================================================
// Find References (scope-aware for locals, workspace-wide for top-level)
// ============================================================================

/// Find all references to a symbol using scope-aware semantic resolution.
pub fn find_references_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
    include_declaration: bool,
) -> Vec<Location> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    let Some(identifier) = index.identifier_at(internal_line, internal_col) else {
        return Vec::new();
    };

    // Check whether this is a local variable / parameter scoped to an enclosing function
    let decl = find_best_declaration_for_line(index, identifier, internal_line);
    if let Some(sym) = decl {
        if matches!(
            sym.kind,
            SemanticSymbolKind::Variable
                | SemanticSymbolKind::Constant
                | SemanticSymbolKind::Parameter
        ) {
            if let Some((fn_start, fn_end)) = enclosing_function_span(index, internal_line) {
                if sym.line >= fn_start && sym.line <= fn_end {
                    return index
                        .find_references_by_name(identifier)
                        .into_iter()
                        .filter(|&(l, c, _)| {
                            l >= fn_start
                                && l <= fn_end
                                && (include_declaration || !(l == sym.line && c == sym.col))
                        })
                        .map(|(l, c, end_c)| {
                            span_to_location(&doc.uri, l, c, end_c.saturating_sub(c))
                        })
                        .collect();
                }
            }
        }
    }

    let decl_loc = decl.map(|s| (&doc.uri, s.line, s.col)).or_else(|| {
        workspace
            .find_declaration_workspace(identifier)
            .map(|(u, s)| (u, s.line, s.col))
    });

    let mut locations = Vec::new();
    for (uri, ref_line, ref_col, ref_end_col) in workspace.find_references_workspace(identifier) {
        if !include_declaration {
            if let Some((d_uri, d_line, d_col)) = decl_loc {
                if &uri == d_uri && ref_line == d_line && ref_col == d_col {
                    continue;
                }
            }
        }
        locations.push(span_to_location(
            &uri,
            ref_line,
            ref_col,
            ref_end_col.saturating_sub(ref_col),
        ));
    }

    locations
}

// ============================================================================
// Document Highlights (READ vs WRITE)
// ============================================================================

/// Highlight all occurrences of the symbol under the cursor in the current document,
/// distinguishing `WRITE` (declarations & assignments) from `READ`.
pub fn find_document_highlights_semantic(
    index: &SemanticIndex,
    line: u32,
    col: u32,
) -> Vec<DocumentHighlight> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    let Some(name) = index.identifier_at(internal_line, internal_col) else {
        return Vec::new();
    };

    // Scope filter for local variables/parameters
    let scope_bounds = find_best_declaration_for_line(index, name, internal_line).and_then(|sym| {
        if matches!(
            sym.kind,
            SemanticSymbolKind::Variable
                | SemanticSymbolKind::Constant
                | SemanticSymbolKind::Parameter
        ) {
            enclosing_function_span(index, internal_line)
                .filter(|&(s, e)| sym.line >= s && sym.line <= e)
        } else {
            None
        }
    });

    let mut highlights = Vec::new();
    for (i, tok) in index.tokens.iter().enumerate() {
        if tok.kind != TokenKind::Identifier || tok.lexeme != name {
            continue;
        }
        if let Some((s, e)) = scope_bounds {
            if tok.line < s || tok.line > e {
                continue;
            }
        }

        let is_write = is_write_token(index, i);
        let start = Position::new(
            tok.line.saturating_sub(1) as u32,
            tok.col.saturating_sub(1) as u32,
        );
        let end = Position::new(start.line, start.character + tok.lexeme.len() as u32);

        highlights.push(DocumentHighlight {
            range: Range { start, end },
            kind: Some(if is_write {
                DocumentHighlightKind::WRITE
            } else {
                DocumentHighlightKind::READ
            }),
        });
    }

    highlights
}

fn is_write_token(index: &SemanticIndex, tok_idx: usize) -> bool {
    let prev = tok_idx.checked_sub(1).and_then(|i| index.tokens.get(i));
    let next = index.tokens.get(tok_idx + 1);

    // Declaration: `let x`, `const x`, `readonly x`, `fn x`, `class x`, `struct x`, `enum x`, `interface x`
    if let Some(p) = prev {
        if matches!(
            p.kind,
            TokenKind::Let
                | TokenKind::Const
                | TokenKind::Fn
                | TokenKind::Class
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Interface
                | TokenKind::Type
                | TokenKind::Share
                | TokenKind::Strong
                | TokenKind::Weak
        ) {
            return true;
        }
    }

    // Assignment or increment/decrement: `x = ...`, `x += ...`, `x++`, `x--`
    if let Some(n) = next {
        if matches!(
            n.kind,
            TokenKind::Equal
                | TokenKind::PlusEqual
                | TokenKind::MinusEqual
                | TokenKind::StarEqual
                | TokenKind::SlashEqual
                | TokenKind::PercentEqual
                | TokenKind::StarStarEqual
                | TokenKind::AmpersandEqual
                | TokenKind::PipeEqual
                | TokenKind::CaretEqual
                | TokenKind::ShiftLeftEqual
                | TokenKind::ShiftRightEqual
                | TokenKind::NullCoalesceEqual
                | TokenKind::PlusPlus
                | TokenKind::MinusMinus
        ) {
            return true;
        }
    }

    false
}

// ============================================================================
// Safe Semantic Rename
// ============================================================================

/// Prepare for a rename operation. Rejects keywords, literals, and built-in globals.
pub fn prepare_rename_semantic(
    index: &SemanticIndex,
    _doc: &Document,
    line: u32,
    col: u32,
) -> Option<Range> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    let token = index.token_at(internal_line, internal_col)?;
    if !matches!(token.kind, TokenKind::Identifier) {
        return None;
    }
    if is_reserved_keyword(&token.lexeme)
        || find_builtin_fn(&token.lexeme).is_some()
        || matches!(token.lexeme.as_str(), "Math" | "time" | "JSON")
    {
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

/// Perform a safe semantic rename (scope-aware for locals, workspace-wide for top-level symbols).
pub fn do_rename_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
    new_name: &str,
) -> Result<WorkspaceEdit, String> {
    if !is_valid_identifier(new_name) {
        return Err(format!("`{}` is not a valid Adesh identifier", new_name));
    }

    let locations = find_references_semantic(index, workspace, doc, line, col, true);
    if locations.is_empty() {
        return Ok(WorkspaceEdit {
            changes: None,
            document_changes: None,
            change_annotations: None,
        });
    }

    let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
        std::collections::HashMap::new();

    for loc in locations {
        changes.entry(loc.uri).or_default().push(TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        });
    }

    Ok(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

// ============================================================================
// Nested Document Symbols with Full Enclosing Ranges
// ============================================================================

/// Get document symbols with full enclosing ranges (so breadcrumbs and sticky scroll work).
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
        let start_line = sym.line.saturating_sub(1) as u32;
        let start_col = sym.col.saturating_sub(1) as u32;
        let sel_start = Position::new(start_line, start_col);
        let sel_end = Position::new(start_line, start_col + sym.name.len() as u32);
        let selection_range = Range::new(sel_start, sel_end);

        let full_range = if sym.kind.is_type() || sym.kind == SemanticSymbolKind::Function {
            if let Some(end_line_1) = find_block_end_line(&index.source, sym.line) {
                let end_line_0 = end_line_1.saturating_sub(1) as u32;
                let end_col = index
                    .source
                    .lines()
                    .nth(end_line_0 as usize)
                    .map(|l| l.encode_utf16().count() as u32)
                    .unwrap_or(1);
                Range::new(
                    Position::new(start_line, 0),
                    Position::new(end_line_0.max(start_line), end_col.max(sel_end.character)),
                )
            } else {
                selection_range
            }
        } else {
            selection_range
        };

        let children = if sym.kind.is_type() {
            get_type_children(index, &sym.name, full_range)
        } else {
            vec![]
        };

        let detail = sym.signature.clone().or_else(|| {
            sym.type_annotation
                .as_ref()
                .map(|t| format!(": {}", t))
                .or_else(|| {
                    sym.ty
                        .as_ref()
                        .map(|t| format!(": {}", crate::type_utils::format_ty(t)))
                })
        });

        symbols.push(DocumentSymbol {
            name: sym.name.clone(),
            detail,
            kind,
            tags: None,
            deprecated: None,
            range: full_range,
            selection_range,
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        });
    }

    symbols
}

fn get_type_children(
    index: &SemanticIndex,
    type_name: &str,
    parent_range: Range,
) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();

    for sym in &index.symbols {
        if sym.parent_type.as_deref() == Some(type_name) {
            let kind = semantic_kind_to_lsp_kind(sym.kind);
            let start_line = sym.line.saturating_sub(1) as u32;
            let start_col = sym.col.saturating_sub(1) as u32;
            let sel = Range::new(
                Position::new(start_line, start_col),
                Position::new(start_line, start_col + sym.name.len() as u32),
            );
            let full = if matches!(
                sym.kind,
                SemanticSymbolKind::Method | SemanticSymbolKind::StaticMethod
            ) {
                find_block_end_line(&index.source, sym.line)
                    .map(|end_l| {
                        let end_0 = end_l.saturating_sub(1) as u32;
                        Range::new(
                            Position::new(start_line, 0),
                            Position::new(end_0.max(start_line), 1),
                        )
                    })
                    .unwrap_or(sel)
            } else {
                sel
            };

            // LSP invariant: child range must fall within parent range
            let clamped_range = clamp_range_inside(full, parent_range);
            let clamped_sel = clamp_range_inside(sel, clamped_range);

            children.push(DocumentSymbol {
                name: sym.name.clone(),
                detail: sym
                    .signature
                    .clone()
                    .or_else(|| sym.type_annotation.as_ref().map(|t| format!(": {}", t))),
                kind,
                tags: None,
                deprecated: None,
                range: clamped_range,
                selection_range: clamped_sel,
                children: None,
            });
        }
    }

    if let Some(td) = index.get_type(type_name) {
        let fallback_range = Range::new(parent_range.start, parent_range.start);

        for (fname, ftype, _) in &td.fields {
            if children.iter().any(|c| c.name == *fname) {
                continue;
            }
            let r = locate_token_in_range(index, fname, parent_range).unwrap_or(fallback_range);
            children.push(DocumentSymbol {
                name: fname.clone(),
                detail: Some(format!(": {}", ftype)),
                kind: SymbolKind::FIELD,
                tags: None,
                deprecated: None,
                range: r,
                selection_range: r,
                children: None,
            });
        }

        for m in &td.methods {
            if children.iter().any(|c| c.name == m.name) {
                continue;
            }
            let r = locate_token_in_range(index, &m.name, parent_range).unwrap_or(fallback_range);
            children.push(DocumentSymbol {
                name: m.name.clone(),
                detail: m.signature.clone(),
                kind: SymbolKind::METHOD,
                tags: None,
                deprecated: None,
                range: r,
                selection_range: r,
                children: None,
            });
        }

        for (vname, vty) in &td.variants {
            if children.iter().any(|c| c.name == *vname) {
                continue;
            }
            let r = locate_token_in_range(index, vname, parent_range).unwrap_or(fallback_range);
            children.push(DocumentSymbol {
                name: vname.clone(),
                detail: vty.as_ref().map(|t| format!("({})", t)),
                kind: SymbolKind::ENUM_MEMBER,
                tags: None,
                deprecated: None,
                range: r,
                selection_range: r,
                children: None,
            });
        }
    }

    children
}

fn locate_token_in_range(index: &SemanticIndex, name: &str, bounds: Range) -> Option<Range> {
    for tok in &index.tokens {
        if tok.lexeme == name {
            let l = tok.line.saturating_sub(1) as u32;
            let c = tok.col.saturating_sub(1) as u32;
            let r = Range::new(Position::new(l, c), Position::new(l, c + name.len() as u32));
            if r.start >= bounds.start && r.end <= bounds.end {
                return Some(r);
            }
        }
    }
    None
}

fn clamp_range_inside(inner: Range, outer: Range) -> Range {
    let start = if inner.start < outer.start {
        outer.start
    } else if inner.start > outer.end {
        outer.end
    } else {
        inner.start
    };
    let end = if inner.end > outer.end {
        outer.end
    } else if inner.end < start {
        start
    } else {
        inner.end
    };
    Range::new(start, end)
}

fn import_target_at_position(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Option<Url> {
    let line_1 = (line + 1) as usize;
    let imp = index.imports.iter().find(|i| i.line == line_1)?;
    let line_text = doc.get_line(line as usize)?;
    let quoted = format!("\"{}\"", imp.path);
    let single = format!("'{}'", imp.path);
    let byte_start = line_text
        .find(&quoted)
        .or_else(|| line_text.find(&single))?;
    let start_u16 = line_text[..byte_start].encode_utf16().count() as u32;
    let end_u16 = start_u16 + (imp.path.len() + 2) as u32;
    if col >= start_u16 && col <= end_u16 {
        return workspace.resolve_module(&doc.uri, &imp.path);
    }
    None
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
    span_to_location(uri, sym.line, sym.col, sym.name.len())
}

fn span_to_location(uri: &Url, line_1based: usize, col_1based: usize, len: usize) -> Location {
    let start = Position {
        line: (line_1based.saturating_sub(1)) as u32,
        character: (col_1based.saturating_sub(1)) as u32,
    };
    let end = Position {
        line: start.line,
        character: start.character + len as u32,
    };
    Location {
        uri: uri.clone(),
        range: Range { start, end },
    }
}
