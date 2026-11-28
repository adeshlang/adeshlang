//! Structural & Navigation Providers:
//! - Call Hierarchy (`prepareCallHierarchy`, `incomingCalls`, `outgoingCalls`)
//! - Type Hierarchy (`prepareTypeHierarchy`, `supertypes`, `subtypes`)
//! - Accurate Folding Ranges (braces, comments, contiguous imports, regions)
//! - Smart Selection Ranges (identifier -> call/member -> statement -> block -> decl -> document)
//! - Document Links (clickable `import ... from "..."` paths in `.adesh` files)
//! - Linked Editing Ranges (synchronized local identifier edits)

use crate::document::Document;
use crate::type_utils::{
    enclosing_function_span, find_best_declaration_for_line, find_block_end_line,
    visit_exprs_in_stmts,
};
use crate::workspace::WorkspaceIndex;
use adeshlang::parsing::ast::{ExprKind, Function, Stmt, StmtKind};
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind, SymbolEntry, TypeDefinition};
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, DocumentLink,
    FoldingRange, FoldingRangeKind, LinkedEditingRanges, Position, Range, SelectionRange,
    SymbolKind, TypeHierarchyItem, Url,
};
use std::collections::HashMap;

// ============================================================================
// Call Hierarchy
// ============================================================================

pub fn prepare_call_hierarchy(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Vec<CallHierarchyItem> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;
    let Some(name) = index.identifier_at(internal_line, internal_col) else {
        return Vec::new();
    };

    if let Some(sym) = index.find_declaration(name) {
        if is_callable_kind(sym.kind) {
            return vec![symbol_to_call_item(sym, &doc.uri, &index.source)];
        }
    }
    // Check methods in current file
    for sym in &index.symbols {
        if sym.name == name && is_callable_kind(sym.kind) {
            return vec![symbol_to_call_item(sym, &doc.uri, &index.source)];
        }
    }
    // Cross-file lookup
    if let Some((uri, sym)) = workspace.find_declaration_workspace(name) {
        if is_callable_kind(sym.kind) {
            let src = workspace
                .get_index(uri)
                .map(|i| i.source.as_str())
                .unwrap_or("");
            return vec![symbol_to_call_item(sym, uri, src)];
        }
    }

    Vec::new()
}

type CallableBody<'a> = (String, Option<String>, usize, usize, &'a [Stmt]);

pub fn call_hierarchy_incoming(
    workspace: &WorkspaceIndex,
    item: &CallHierarchyItem,
) -> Vec<CallHierarchyIncomingCall> {
    let target_name = &item.name;
    let mut incoming: Vec<CallHierarchyIncomingCall> = Vec::new();

    for (uri, index) in workspace.iter_indices() {
        let mut funcs: Vec<CallableBody<'_>> = Vec::new();
        collect_callable_bodies(&index.statements, &index.source, &mut funcs);

        for (fn_name, parent, start_line, end_line, body) in funcs {
            let mut from_ranges = Vec::new();
            visit_exprs_in_stmts(body, &mut |expr| {
                let called = match &expr.kind {
                    ExprKind::Call(callee, _, _) | ExprKind::OptCall(callee, _, _) => {
                        extract_callee_name(callee)
                    }
                    _ => None,
                };
                if called.as_deref() == Some(target_name.as_str()) {
                    let l = expr.span.line.saturating_sub(1) as u32;
                    let c = expr.span.col.saturating_sub(1) as u32;
                    from_ranges.push(Range::new(
                        Position::new(l, c),
                        Position::new(l, c + target_name.len() as u32),
                    ));
                }
            });

            if !from_ranges.is_empty() {
                let start_0 = start_line.saturating_sub(1) as u32;
                let end_0 = end_line.saturating_sub(1).max(start_0 as usize) as u32;
                let caller_item = CallHierarchyItem {
                    name: fn_name.clone(),
                    kind: if parent.is_some() {
                        SymbolKind::METHOD
                    } else {
                        SymbolKind::FUNCTION
                    },
                    tags: None,
                    detail: parent.map(|p| format!("in {}", p)),
                    uri: uri.clone(),
                    range: Range::new(Position::new(start_0, 0), Position::new(end_0, 1)),
                    selection_range: Range::new(
                        Position::new(start_0, 0),
                        Position::new(start_0, fn_name.len() as u32),
                    ),
                    data: None,
                };
                incoming.push(CallHierarchyIncomingCall {
                    from: caller_item,
                    from_ranges,
                });
            }
        }
    }

    incoming
}

pub fn call_hierarchy_outgoing(
    workspace: &WorkspaceIndex,
    item: &CallHierarchyItem,
) -> Vec<CallHierarchyOutgoingCall> {
    let Some(index) = workspace.get_index(&item.uri) else {
        return Vec::new();
    };

    let mut funcs: Vec<CallableBody<'_>> = Vec::new();
    collect_callable_bodies(&index.statements, &index.source, &mut funcs);

    let item_line = (item.selection_range.start.line + 1) as usize;
    let Some((_, _, _, _, body)) = funcs.into_iter().find(|(name, _, start, end, _)| {
        name == &item.name && item_line >= start.saturating_sub(1) && item_line <= end + 1
    }) else {
        return Vec::new();
    };

    let mut grouped: HashMap<String, Vec<Range>> = HashMap::new();
    visit_exprs_in_stmts(body, &mut |expr| {
        let called = match &expr.kind {
            ExprKind::Call(callee, _, _) | ExprKind::OptCall(callee, _, _) => {
                extract_callee_name(callee)
            }
            _ => None,
        };
        if let Some(callee_name) = called {
            let l = expr.span.line.saturating_sub(1) as u32;
            let c = expr.span.col.saturating_sub(1) as u32;
            grouped
                .entry(callee_name.clone())
                .or_default()
                .push(Range::new(
                    Position::new(l, c),
                    Position::new(l, c + callee_name.len() as u32),
                ));
        }
    });

    let mut outgoing = Vec::new();
    for (callee_name, from_ranges) in grouped {
        if let Some((target_uri, sym)) = index
            .find_declaration(&callee_name)
            .map(|s| (&item.uri, s))
            .or_else(|| workspace.find_declaration_workspace(&callee_name))
        {
            if is_callable_kind(sym.kind) {
                let src = workspace
                    .get_index(target_uri)
                    .map(|i| i.source.as_str())
                    .unwrap_or("");
                outgoing.push(CallHierarchyOutgoingCall {
                    to: symbol_to_call_item(sym, target_uri, src),
                    from_ranges,
                });
            }
        }
    }

    outgoing
}

fn is_callable_kind(kind: SemanticSymbolKind) -> bool {
    matches!(
        kind,
        SemanticSymbolKind::Function
            | SemanticSymbolKind::Method
            | SemanticSymbolKind::StaticMethod
    )
}

fn symbol_to_call_item(sym: &SymbolEntry, uri: &Url, source: &str) -> CallHierarchyItem {
    let start_0 = sym.line.saturating_sub(1) as u32;
    let col_0 = sym.col.saturating_sub(1) as u32;
    let end_0 = find_block_end_line(source, sym.line)
        .map(|l| l.saturating_sub(1) as u32)
        .unwrap_or(start_0);
    let sel = Range::new(
        Position::new(start_0, col_0),
        Position::new(start_0, col_0 + sym.name.len() as u32),
    );
    CallHierarchyItem {
        name: sym.name.clone(),
        kind: if sym.parent_type.is_some() {
            SymbolKind::METHOD
        } else {
            SymbolKind::FUNCTION
        },
        tags: None,
        detail: sym.signature.clone().or_else(|| sym.parent_type.clone()),
        uri: uri.clone(),
        range: Range::new(Position::new(start_0, 0), Position::new(end_0, 1)),
        selection_range: sel,
        data: None,
    }
}

fn collect_callable_bodies<'a>(stmts: &'a [Stmt], source: &str, out: &mut Vec<CallableBody<'a>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f, _) | StmtKind::ExportDefaultFunction(f) => {
                let start = stmt.span.line;
                let end = find_block_end_line(source, start)
                    .or_else(|| f.body.last().map(|s| s.span.line + 1))
                    .unwrap_or(start);
                out.push((f.name.clone(), None, start, end, &f.body));
                collect_callable_bodies(&f.body, source, out);
            }
            StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
                for m in c.methods.iter().chain(c.static_methods.iter()) {
                    push_method_body(m, &c.name, source, out);
                }
            }
            StmtKind::Extend(_, target, _, methods, _) => {
                for m in methods {
                    push_method_body(m, target, source, out);
                }
            }
            StmtKind::Block(inner) => collect_callable_bodies(inner, source, out),
            _ => {}
        }
    }
}

fn push_method_body<'a>(
    m: &'a Function,
    owner: &str,
    source: &str,
    out: &mut Vec<CallableBody<'a>>,
) {
    let start = m
        .body
        .first()
        .map(|s| s.span.line.saturating_sub(1).max(1))
        .unwrap_or(1);
    let end = find_block_end_line(source, start)
        .or_else(|| m.body.last().map(|s| s.span.line + 1))
        .unwrap_or(start);
    out.push((m.name.clone(), Some(owner.to_string()), start, end, &m.body));
}

fn extract_callee_name(callee: &adeshlang::parsing::ast::Expr) -> Option<String> {
    match &callee.kind {
        ExprKind::Variable(name) => Some(name.clone()),
        ExprKind::Get(_, member) | ExprKind::OptGet(_, member) => Some(member.clone()),
        _ => None,
    }
}

// ============================================================================
// Type Hierarchy
// ============================================================================

pub fn prepare_type_hierarchy(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Vec<TypeHierarchyItem> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;
    let Some(name) = index.identifier_at(internal_line, internal_col) else {
        return Vec::new();
    };

    if let Some(td) = index.get_type(name) {
        return vec![type_def_to_hierarchy_item(td, &doc.uri, &index.source)];
    }
    if let Some((uri, td)) = workspace.find_type_location(name) {
        let src = workspace
            .get_index(uri)
            .map(|i| i.source.as_str())
            .unwrap_or("");
        return vec![type_def_to_hierarchy_item(td, uri, src)];
    }
    Vec::new()
}

pub fn type_hierarchy_supertypes(
    workspace: &WorkspaceIndex,
    item: &TypeHierarchyItem,
) -> Vec<TypeHierarchyItem> {
    let Some(td) = workspace.find_type_definition(&item.name) else {
        return Vec::new();
    };

    let mut supers = Vec::new();
    if let Some(parent) = &td.extends {
        if let Some((uri, ptd)) = workspace.find_type_location(parent) {
            let src = workspace
                .get_index(uri)
                .map(|i| i.source.as_str())
                .unwrap_or("");
            supers.push(type_def_to_hierarchy_item(ptd, uri, src));
        }
    }
    for iface in &td.implements {
        if let Some((uri, itd)) = workspace.find_type_location(iface) {
            let src = workspace
                .get_index(uri)
                .map(|i| i.source.as_str())
                .unwrap_or("");
            supers.push(type_def_to_hierarchy_item(itd, uri, src));
        }
    }
    supers
}

pub fn type_hierarchy_subtypes(
    workspace: &WorkspaceIndex,
    item: &TypeHierarchyItem,
) -> Vec<TypeHierarchyItem> {
    let mut subs = Vec::new();
    for (uri, index) in workspace.iter_indices() {
        for td in index.types.values() {
            if td.extends.as_deref() == Some(item.name.as_str())
                || td.implements.iter().any(|i| i == &item.name)
            {
                subs.push(type_def_to_hierarchy_item(td, uri, &index.source));
            }
        }
    }
    subs
}

fn type_def_to_hierarchy_item(td: &TypeDefinition, uri: &Url, source: &str) -> TypeHierarchyItem {
    let start_0 = td.line.saturating_sub(1) as u32;
    let col_0 = td.col.saturating_sub(1) as u32;
    let end_0 = find_block_end_line(source, td.line)
        .map(|l| l.saturating_sub(1) as u32)
        .unwrap_or(start_0);
    let kind = match td.kind {
        SemanticSymbolKind::Class => SymbolKind::CLASS,
        SemanticSymbolKind::Struct => SymbolKind::STRUCT,
        SemanticSymbolKind::Enum => SymbolKind::ENUM,
        SemanticSymbolKind::Interface => SymbolKind::INTERFACE,
        _ => SymbolKind::CLASS,
    };
    let mut detail_parts = Vec::new();
    if let Some(ext) = &td.extends {
        detail_parts.push(format!("extends {}", ext));
    }
    if !td.implements.is_empty() {
        detail_parts.push(format!("implements {}", td.implements.join(", ")));
    }
    TypeHierarchyItem {
        name: td.name.clone(),
        kind,
        tags: None,
        detail: if detail_parts.is_empty() {
            None
        } else {
            Some(detail_parts.join(" "))
        },
        uri: uri.clone(),
        range: Range::new(Position::new(start_0, 0), Position::new(end_0, 1)),
        selection_range: Range::new(
            Position::new(start_0, col_0),
            Position::new(start_0, col_0 + td.name.len() as u32),
        ),
        data: None,
    }
}

// ============================================================================
// Accurate Folding Ranges
// ============================================================================

/// Compute accurate folding ranges for an Adesh source file:
/// - All `{ ... }`, `[ ... ]`, `( ... )` multi-line blocks (ignoring strings/comments)
/// - Multi-line `/* ... */` block comments and contiguous `//` line comment blocks
/// - Contiguous `import` blocks (`FoldingRangeKind::Imports`)
/// - `// #region ... // #endregion` explicit regions
pub fn get_folding_ranges(content: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = content.lines().collect();
    let mut ranges = Vec::new();

    // 1. Delimiter-based folding (`{ }`, `[ ]`, `( )`) + block comments
    let mut brace_stack: Vec<(u32, char)> = Vec::new();
    let mut region_stack: Vec<u32> = Vec::new();
    let mut block_comment_start: Option<u32> = None;

    // Contiguous line comments and imports tracking
    let mut line_comment_start: Option<u32> = None;
    let mut line_comment_end: u32 = 0;
    let mut import_start: Option<u32> = None;
    let mut import_end: u32 = 0;

    for (line_idx, raw_line) in lines.iter().enumerate() {
        let line_u32 = line_idx as u32;
        let trimmed = raw_line.trim();

        // Contiguous `//` comments
        if trimmed.starts_with("//") {
            if trimmed.starts_with("// #region") || trimmed.starts_with("//#region") {
                region_stack.push(line_u32);
            } else if trimmed.starts_with("// #endregion") || trimmed.starts_with("//#endregion") {
                if let Some(start) = region_stack.pop() {
                    if line_u32 > start {
                        ranges.push(FoldingRange {
                            start_line: start,
                            end_line: line_u32,
                            start_character: None,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Region),
                            collapsed_text: None,
                        });
                    }
                }
            }
            if line_comment_start.is_none() {
                line_comment_start = Some(line_u32);
            }
            line_comment_end = line_u32;
        } else if let Some(start) = line_comment_start.take() {
            if line_comment_end > start {
                ranges.push(FoldingRange {
                    start_line: start,
                    end_line: line_comment_end,
                    start_character: None,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Comment),
                    collapsed_text: Some("// ...".to_string()),
                });
            }
        }

        // Contiguous `import` statements
        if trimmed.starts_with("import ") {
            if import_start.is_none() {
                import_start = Some(line_u32);
            }
            import_end = line_u32;
        } else if !trimmed.is_empty() {
            if let Some(start) = import_start.take() {
                if import_end > start {
                    ranges.push(FoldingRange {
                        start_line: start,
                        end_line: import_end,
                        start_character: None,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Imports),
                        collapsed_text: Some("import ...".to_string()),
                    });
                }
            }
        }

        // Character scan for `{ }`, `[ ]`, `( )`, `/* */`
        let bytes = raw_line.as_bytes();
        let mut i = 0;
        let mut in_string: Option<u8> = None;

        while i < bytes.len() {
            let b = bytes[i];
            if let Some(start_c) = block_comment_start {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    if line_u32 > start_c {
                        ranges.push(FoldingRange {
                            start_line: start_c,
                            end_line: line_u32,
                            start_character: None,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Comment),
                            collapsed_text: Some("/* ... */".to_string()),
                        });
                    }
                    block_comment_start = None;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if let Some(q) = in_string {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == q {
                    in_string = None;
                }
                i += 1;
                continue;
            }
            if b == b'/' && i + 1 < bytes.len() {
                if bytes[i + 1] == b'/' {
                    break;
                }
                if bytes[i + 1] == b'*' {
                    block_comment_start = Some(line_u32);
                    i += 2;
                    continue;
                }
            }
            if b == b'"' || b == b'\'' || b == b'`' {
                in_string = Some(b);
                i += 1;
                continue;
            }
            match b {
                b'{' | b'[' | b'(' => brace_stack.push((line_u32, b as char)),
                b'}' | b']' | b')' => {
                    let expected = match b {
                        b'}' => '{',
                        b']' => '[',
                        _ => '(',
                    };
                    if let Some(pos) = brace_stack.iter().rposition(|&(_, c)| c == expected) {
                        let (start_line, _) = brace_stack.remove(pos);
                        // Report the actual delimiter span. Clients decide how to
                        // display the closing delimiter when folding.
                        if line_u32 > start_line {
                            let end_line = line_u32;
                            ranges.push(FoldingRange {
                                start_line,
                                end_line,
                                start_character: None,
                                end_character: None,
                                kind: Some(FoldingRangeKind::Region),
                                collapsed_text: None,
                            });
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    // Flush trailing comment / import blocks
    if let Some(start) = line_comment_start {
        if line_comment_end > start {
            ranges.push(FoldingRange {
                start_line: start,
                end_line: line_comment_end,
                start_character: None,
                end_character: None,
                kind: Some(FoldingRangeKind::Comment),
                collapsed_text: Some("// ...".to_string()),
            });
        }
    }
    if let Some(start) = import_start {
        if import_end > start {
            ranges.push(FoldingRange {
                start_line: start,
                end_line: import_end,
                start_character: None,
                end_character: None,
                kind: Some(FoldingRangeKind::Imports),
                collapsed_text: Some("import ...".to_string()),
            });
        }
    }

    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
    ranges
}

// ============================================================================
// Smart Selection Ranges
// ============================================================================

/// Compute smart-expand selection ranges for each requested cursor position in an Adesh document.
pub fn get_selection_ranges(doc: &Document, positions: &[Position]) -> Vec<SelectionRange> {
    let pairs = find_enclosing_delimiter_pairs(&doc.content);
    let doc_end = doc.offset_to_position(doc.content.len());
    let doc_range = Range::new(Position::new(0, 0), Position::new(doc_end.0, doc_end.1));

    positions
        .iter()
        .map(|pos| build_selection_chain(doc, *pos, &pairs, doc_range))
        .collect()
}

fn build_selection_chain(
    doc: &Document,
    pos: Position,
    pairs: &[(usize, usize)],
    doc_range: Range,
) -> SelectionRange {
    let mut chain: Vec<Range> = Vec::new();
    let offset = doc.position_to_offset(pos.line, pos.character).unwrap_or(0);

    // 1. Word/identifier at cursor
    if let Some(line_text) = doc.get_line(pos.line as usize) {
        let byte_col = Document::utf16_col_to_byte_offset(line_text, pos.character);
        let start_b = line_text[..byte_col]
            .char_indices()
            .rev()
            .find(|&(_, c)| !c.is_alphanumeric() && c != '_')
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let end_b = line_text[byte_col..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| byte_col + i)
            .unwrap_or(line_text.len());
        if start_b < end_b {
            let start_u16 = line_text[..start_b].encode_utf16().count() as u32;
            let end_u16 = line_text[..end_b].encode_utf16().count() as u32;
            chain.push(Range::new(
                Position::new(pos.line, start_u16),
                Position::new(pos.line, end_u16),
            ));
        }

        // 2. Dotted/call chain on the line (e.g. `user.profile.name`)
        let chain_start_b = line_text[..byte_col]
            .char_indices()
            .rev()
            .find(|&(_, c)| !c.is_alphanumeric() && c != '_' && c != '.')
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let chain_end_b = line_text[byte_col..]
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .map(|i| byte_col + i)
            .unwrap_or(line_text.len());
        if chain_start_b < chain_end_b && (chain_start_b != start_b || chain_end_b != end_b) {
            let s_u16 = line_text[..chain_start_b].encode_utf16().count() as u32;
            let e_u16 = line_text[..chain_end_b].encode_utf16().count() as u32;
            chain.push(Range::new(
                Position::new(pos.line, s_u16),
                Position::new(pos.line, e_u16),
            ));
        }

        // 3. Trimmed line (statement)
        let trim_start = line_text.len() - line_text.trim_start().len();
        let trim_end = line_text.trim_end().len();
        if trim_start < trim_end {
            let s_u16 = line_text[..trim_start].encode_utf16().count() as u32;
            let e_u16 = line_text[..trim_end].encode_utf16().count() as u32;
            chain.push(Range::new(
                Position::new(pos.line, s_u16),
                Position::new(pos.line, e_u16),
            ));
        }
    }

    // 4. All enclosing delimiter pairs (innermost first -> outermost)
    let mut enclosing: Vec<(usize, usize)> = pairs
        .iter()
        .copied()
        .filter(|&(s, e)| offset >= s && offset <= e)
        .collect();
    enclosing.sort_by_key(|&(s, e)| e - s);

    for (s, e) in enclosing {
        if s + 1 < e {
            let (il, ic) = doc.offset_to_position(s + 1);
            let (el, ec) = doc.offset_to_position(e);
            chain.push(Range::new(Position::new(il, ic), Position::new(el, ec)));
        }
        let (sl, sc) = doc.offset_to_position(s);
        let (el, ec) = doc.offset_to_position((e + 1).min(doc.content.len()));
        chain.push(Range::new(Position::new(sl, sc), Position::new(el, ec)));
    }

    // 5. Full document
    chain.push(doc_range);

    // Filter to strictly growing ranges
    let mut unique: Vec<Range> = Vec::new();
    for r in chain {
        if let Some(last) = unique.last() {
            if r.start <= last.start && r.end >= last.end && r != *last {
                unique.push(r);
            }
        } else {
            unique.push(r);
        }
    }

    let mut parent: Option<Box<SelectionRange>> = None;
    for r in unique.into_iter().rev() {
        parent = Some(Box::new(SelectionRange { range: r, parent }));
    }
    *parent.unwrap_or_else(|| {
        Box::new(SelectionRange {
            range: doc_range,
            parent: None,
        })
    })
}

fn find_enclosing_delimiter_pairs(content: &str) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut stack: Vec<(usize, u8)> = Vec::new();
    let mut pairs = Vec::new();
    let mut i = 0;
    let mut in_str: Option<(u8, usize)> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while i < bytes.len() {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some((q, start_idx)) = in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                pairs.push((start_idx, i));
                in_str = None;
            }
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                in_block_comment = true;
                i += 2;
                continue;
            }
        }
        if b == b'"' || b == b'\'' || b == b'`' {
            in_str = Some((b, i));
            i += 1;
            continue;
        }
        match b {
            b'{' | b'[' | b'(' => stack.push((i, b)),
            b'}' | b']' | b')' => {
                let open = match b {
                    b'}' => b'{',
                    b']' => b'[',
                    _ => b'(',
                };
                if let Some(pos) = stack.iter().rposition(|&(_, c)| c == open) {
                    let (start_idx, _) = stack.remove(pos);
                    pairs.push((start_idx, i));
                }
            }
            _ => {}
        }
        i += 1;
    }

    pairs
}

// ============================================================================
// Document Links for `.adesh` Imports
// ============================================================================

/// Compute clickable document links for `import ... from "..."` and `import "..."` in `.adesh` files.
pub fn get_adesh_document_links(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
) -> Vec<DocumentLink> {
    let mut links = Vec::new();

    for imp in &index.imports {
        let line_0 = imp.line.saturating_sub(1);
        let Some(line_text) = doc.get_line(line_0) else {
            continue;
        };
        let quoted = format!("\"{}\"", imp.path);
        let single_quoted = format!("'{}'", imp.path);
        let found = line_text
            .find(&quoted)
            .or_else(|| line_text.find(&single_quoted));
        let Some(byte_start) = found else {
            continue;
        };
        // Highlight inner path (inside quotes)
        let inner_start = byte_start + 1;
        let inner_end = inner_start + imp.path.len();
        let start_col = line_text[..inner_start].encode_utf16().count() as u32;
        let end_col = line_text[..inner_end].encode_utf16().count() as u32;

        if let Some(target_uri) = workspace.resolve_module(&doc.uri, &imp.path) {
            links.push(DocumentLink {
                range: Range::new(
                    Position::new(line_0 as u32, start_col),
                    Position::new(line_0 as u32, end_col),
                ),
                tooltip: Some(format!("Open {}", imp.path)),
                target: Some(target_uri),
                data: None,
            });
        }
    }

    links
}

// ============================================================================
// Linked Editing Ranges (synchronized local edits)
// ============================================================================

/// Provide linked editing ranges for local variables/parameters within the same enclosing function.
pub fn get_linked_editing_ranges(
    index: &SemanticIndex,
    line: u32,
    col: u32,
) -> Option<LinkedEditingRanges> {
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;
    let name = index.identifier_at(internal_line, internal_col)?;

    let decl = find_best_declaration_for_line(index, name, internal_line)?;
    if !matches!(
        decl.kind,
        SemanticSymbolKind::Variable | SemanticSymbolKind::Constant | SemanticSymbolKind::Parameter
    ) {
        return None;
    }

    // Only enable synchronized linked editing for locals/params inside a function body
    let (fn_start, fn_end) = enclosing_function_span(index, internal_line)?;
    if decl.line < fn_start || decl.line > fn_end {
        return None;
    }

    let ranges: Vec<Range> = index
        .find_references_by_name(name)
        .into_iter()
        .filter(|&(l, _, _)| l >= fn_start && l <= fn_end)
        .map(|(l, c, end_c)| {
            Range::new(
                Position::new(l.saturating_sub(1) as u32, c.saturating_sub(1) as u32),
                Position::new(l.saturating_sub(1) as u32, end_c.saturating_sub(1) as u32),
            )
        })
        .collect();

    if ranges.len() < 2 {
        return None;
    }

    Some(LinkedEditingRanges {
        ranges,
        word_pattern: Some("[a-zA-Z_][a-zA-Z0-9_]*".to_string()),
    })
}
