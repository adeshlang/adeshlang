//! Rich Inlay Hints Provider
//!
//! Powered by the shared `SemanticIndex` and `WorkspaceIndex`:
//! - Inferred type hints on `let`/`const` bindings without explicit type annotations
//!   (with clickable type locations and double-click `text_edits` to materialize!)
//! - Parameter name hints on function, method, and constructor calls
//! - Inferred return type hints on functions without explicit return annotations
//! - Ownership / ARC hints (`share`, `strong`, `weak`, `readonly`)

use crate::analysis::SymbolInfo;
use crate::config::AlsConfig;
use crate::document::Document;
use crate::type_utils::{
    extract_nominal_type_name, find_builtin_fn, format_ty, infer_function_return_type,
    is_concrete_ty, resolve_expression_chain_type, visit_exprs_in_stmts,
};
use crate::workspace::WorkspaceIndex;
use adeshlang::parsing::ast::{Expr, ExprKind, Stmt, StmtKind};
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind};
use lsp_types::*;

/// Generate rich semantic inlay hints for a document range.
pub fn get_inlay_hints_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    range: Range,
    config: &AlsConfig,
) -> Vec<InlayHint> {
    if !config.inlay_hints.enabled {
        return Vec::new();
    }

    let mut hints = Vec::new();

    // 1. Variable / constant / ARC inferred type & ownership hints
    collect_stmt_inlay_hints(
        index,
        workspace,
        doc,
        &index.statements,
        range,
        config,
        &mut hints,
    );

    // 2. Parameter name hints at call sites
    if config.inlay_hints.show_parameter_names {
        collect_call_parameter_hints(index, workspace, doc, range, config, &mut hints);
    }

    // Deduplicate hints at the exact same position
    hints.sort_by_key(|h| (h.position.line, h.position.character));
    hints.dedup_by(|a, b| a.position == b.position && a.kind == b.kind);

    hints
}

fn collect_stmt_inlay_hints(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    stmts: &[Stmt],
    range: Range,
    config: &AlsConfig,
    hints: &mut Vec<InlayHint>,
) {
    for stmt in stmts {
        let line_0 = stmt.span.line.saturating_sub(1) as u32;
        let in_range = line_0 >= range.start.line && line_0 <= range.end.line;

        match &stmt.kind {
            StmtKind::Let(name, Some(init), None, _, _, is_readonly) => {
                if in_range
                    && config.inlay_hints.show_types
                    && config.type_assistance.show_inferred_types
                {
                    let inferred = index.infer_type(init);
                    if is_concrete_ty(&inferred) {
                        let ty_str =
                            truncate_hint(&format_ty(&inferred), config.inlay_hints.max_length);
                        if let Some(pos) = find_binding_name_end(doc, line_0, name) {
                            let type_loc = extract_nominal_type_name(&inferred)
                                .and_then(|tname| {
                                    index
                                        .get_type(&tname)
                                        .map(|td| (&doc.uri, td))
                                        .or_else(|| workspace.find_type_location(&tname))
                                })
                                .map(|(u, td)| {
                                    let l = td.line.saturating_sub(1) as u32;
                                    let c = td.col.saturating_sub(1) as u32;
                                    Location::new(
                                        u.clone(),
                                        Range::new(
                                            Position::new(l, c),
                                            Position::new(l, c + td.name.len() as u32),
                                        ),
                                    )
                                });

                            let label = if let Some(loc) = type_loc {
                                InlayHintLabel::LabelParts(vec![
                                    InlayHintLabelPart {
                                        value: ": ".to_string(),
                                        ..Default::default()
                                    },
                                    InlayHintLabelPart {
                                        value: ty_str.clone(),
                                        tooltip: Some(InlayHintLabelPartTooltip::String(format!(
                                            "Go to type `{}`",
                                            ty_str
                                        ))),
                                        location: Some(loc),
                                        command: None,
                                    },
                                ])
                            } else {
                                InlayHintLabel::String(format!(": {}", ty_str))
                            };

                            hints.push(InlayHint {
                                position: pos,
                                label,
                                kind: Some(InlayHintKind::TYPE),
                                text_edits: Some(vec![TextEdit {
                                    range: Range::new(pos, pos),
                                    new_text: format!(": {}", ty_str),
                                }]),
                                tooltip: Some(InlayHintTooltip::String(format!(
                                    "Inferred type `{}` (double-click to insert)",
                                    ty_str
                                ))),
                                padding_left: Some(false),
                                padding_right: Some(true),
                                data: None,
                            });
                        }
                    }
                }
                if in_range && *is_readonly && config.inlay_hints.show_ownership {
                    if let Some(pos) = find_binding_name_end(doc, line_0, name) {
                        hints.push(make_ownership_hint(
                            pos,
                            "readonly",
                            "Immutable read-only binding",
                        ));
                    }
                }
            }

            StmtKind::ShareDeclaration(decl, _) => {
                if in_range && config.inlay_hints.show_ownership {
                    if let Some(pos) = find_binding_name_end(doc, line_0, &decl.name) {
                        let ty = decl
                            .type_ann
                            .clone()
                            .unwrap_or_else(|| format_ty(&index.infer_type(&decl.expr)));
                        hints.push(make_ownership_hint(
                            pos,
                            &format!("ARC<share {}>", ty),
                            "Shared ARC read-only reference",
                        ));
                    }
                }
            }
            StmtKind::StrongDeclaration(decl, _) => {
                if in_range && config.inlay_hints.show_ownership {
                    if let Some(pos) = find_binding_name_end(doc, line_0, &decl.name) {
                        let ty = decl
                            .type_ann
                            .clone()
                            .unwrap_or_else(|| format_ty(&index.infer_type(&decl.expr)));
                        hints.push(make_ownership_hint(
                            pos,
                            &format!("ARC<strong {}>", ty),
                            "Strong owning ARC reference",
                        ));
                    }
                }
            }
            StmtKind::WeakDeclaration(decl, _) => {
                if in_range && config.inlay_hints.show_ownership {
                    if let Some(pos) = find_binding_name_end(doc, line_0, &decl.name) {
                        let ty = decl
                            .type_ann
                            .clone()
                            .unwrap_or_else(|| format_ty(&index.infer_type(&decl.expr)));
                        hints.push(make_ownership_hint(
                            pos,
                            &format!("ARC<weak {}>", ty),
                            "Non-owning weak ARC reference (prevents cycles)",
                        ));
                    }
                }
            }

            StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) => {
                if in_range && config.inlay_hints.show_return_types && func.ret_type.is_none() {
                    if let Some(ret_ty) = infer_function_return_type(index, func) {
                        let ret_str =
                            truncate_hint(&format_ty(&ret_ty), config.inlay_hints.max_length);
                        if let Some(line_text) = doc.get_line(line_0 as usize) {
                            if let Some(close_paren) = line_text.rfind(')') {
                                let col =
                                    line_text[..close_paren + 1].encode_utf16().count() as u32;
                                let pos = Position::new(line_0, col);
                                hints.push(InlayHint {
                                    position: pos,
                                    label: InlayHintLabel::String(format!(": {}", ret_str)),
                                    kind: Some(InlayHintKind::TYPE),
                                    text_edits: Some(vec![TextEdit {
                                        range: Range::new(pos, pos),
                                        new_text: format!(": {}", ret_str),
                                    }]),
                                    tooltip: Some(InlayHintTooltip::String(format!(
                                        "Inferred return type `{}` (double-click to insert)",
                                        ret_str
                                    ))),
                                    padding_left: Some(false),
                                    padding_right: Some(true),
                                    data: None,
                                });
                            }
                        }
                    }
                }
                collect_stmt_inlay_hints(index, workspace, doc, &func.body, range, config, hints);
            }

            StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
                for m in c.methods.iter().chain(c.static_methods.iter()) {
                    collect_stmt_inlay_hints(index, workspace, doc, &m.body, range, config, hints);
                }
            }
            StmtKind::Extend(_, _, _, methods, _) => {
                for m in methods {
                    collect_stmt_inlay_hints(index, workspace, doc, &m.body, range, config, hints);
                }
            }
            StmtKind::Block(inner) => {
                collect_stmt_inlay_hints(index, workspace, doc, inner, range, config, hints);
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_stmt_inlay_hints(
                    index,
                    workspace,
                    doc,
                    std::slice::from_ref(then_branch.as_ref()),
                    range,
                    config,
                    hints,
                );
                if let Some(e) = else_branch {
                    collect_stmt_inlay_hints(
                        index,
                        workspace,
                        doc,
                        std::slice::from_ref(e.as_ref()),
                        range,
                        config,
                        hints,
                    );
                }
            }
            StmtKind::While { body, .. } | StmtKind::ForIn { body, .. } => {
                collect_stmt_inlay_hints(
                    index,
                    workspace,
                    doc,
                    std::slice::from_ref(body.as_ref()),
                    range,
                    config,
                    hints,
                );
            }
            StmtKind::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                collect_stmt_inlay_hints(
                    index,
                    workspace,
                    doc,
                    std::slice::from_ref(try_block.as_ref()),
                    range,
                    config,
                    hints,
                );
                collect_stmt_inlay_hints(
                    index,
                    workspace,
                    doc,
                    std::slice::from_ref(catch_block.as_ref()),
                    range,
                    config,
                    hints,
                );
            }
            _ => {}
        }
    }
}

fn collect_call_parameter_hints(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    range: Range,
    _config: &AlsConfig,
    hints: &mut Vec<InlayHint>,
) {
    visit_exprs_in_stmts(&index.statements, &mut |expr| {
        let (callee, args) = match &expr.kind {
            ExprKind::Call(c, args, _) | ExprKind::OptCall(c, args, _) | ExprKind::New(c, args) => {
                (c.as_ref(), args.as_slice())
            }
            _ => return,
        };
        if args.is_empty() {
            return;
        }
        let call_line_0 = expr.span.line.saturating_sub(1) as u32;
        if call_line_0 < range.start.line || call_line_0 > range.end.line {
            return;
        }

        let param_names = resolve_call_param_names(index, workspace, callee, expr.span.line);
        if param_names.is_empty() {
            return;
        }

        for (arg, param_name) in args.iter().zip(param_names.iter()) {
            // Skip variadic params (`...args`) or when argument variable already matches param name
            if param_name.starts_with("...") || param_name.is_empty() {
                continue;
            }
            if let ExprKind::Variable(v) = &arg.kind {
                if v == param_name {
                    continue;
                }
            }
            let arg_line_0 = arg.span.line.saturating_sub(1) as u32;
            let arg_col_0 = arg.span.col.saturating_sub(1) as u32;
            if arg_line_0 < range.start.line || arg_line_0 > range.end.line {
                continue;
            }
            // Validate col is within line bounds
            if let Some(lt) = doc.get_line(arg_line_0 as usize) {
                if (arg_col_0 as usize) > lt.encode_utf16().count() {
                    continue;
                }
            }
            let clean_param = param_name.trim_end_matches('?');
            hints.push(InlayHint {
                position: Position::new(arg_line_0, arg_col_0),
                label: InlayHintLabel::String(format!("{}:", clean_param)),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(format!(
                    "Parameter `{}`",
                    clean_param
                ))),
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }
    });
}

fn resolve_call_param_names(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    callee: &Expr,
    line_1based: usize,
) -> Vec<String> {
    match &callee.kind {
        ExprKind::Variable(name) => {
            // Constructor call
            if let Some(td) = index
                .get_type(name)
                .or_else(|| workspace.find_type_definition(name))
            {
                if let Some(ctor) = td
                    .methods
                    .iter()
                    .find(|m| m.name == "constructor" || m.name == "init")
                {
                    return ctor.params.iter().map(|(n, _)| n.clone()).collect();
                }
                return td.fields.iter().map(|(n, _, _)| n.clone()).collect();
            }
            // Function in file or workspace
            if let Some(sym) = index
                .find_declaration(name)
                .or_else(|| workspace.find_declaration_workspace(name).map(|(_, s)| s))
            {
                if matches!(
                    sym.kind,
                    SemanticSymbolKind::Function | SemanticSymbolKind::Method
                ) {
                    return sym.params.iter().map(|(n, _)| n.clone()).collect();
                }
            }
            // Builtin function (skip print/println to avoid noise)
            if !matches!(name.as_str(), "print" | "println" | "eprint") {
                if let Some(spec) = find_builtin_fn(name) {
                    return spec
                        .params
                        .iter()
                        .map(|(p, _)| p.split(':').next().unwrap_or(p).trim().to_string())
                        .collect();
                }
            }
            Vec::new()
        }
        ExprKind::Get(obj, method_name) | ExprKind::OptGet(obj, method_name) => {
            if let ExprKind::Variable(recv_name) = &obj.kind {
                if let Some(ty) =
                    resolve_expression_chain_type(index, workspace, recv_name, line_1based)
                {
                    if let Some(tname) = extract_nominal_type_name(&ty) {
                        let members =
                            crate::type_utils::get_type_members_workspace(index, workspace, &tname);
                        if let Some(m) = members.into_iter().find(|m| m.name == *method_name) {
                            return m.params.into_iter().map(|(n, _)| n).collect();
                        }
                    }
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn find_binding_name_end(doc: &Document, line_0: u32, name: &str) -> Option<Position> {
    let line_text = doc.get_line(line_0 as usize)?;
    // Skip leading keyword (`let`, `const`, `readonly`, `export`, `share`, `strong`, `weak`)
    let mut search_from = 0;
    for kw in [
        "export ",
        "let ",
        "const ",
        "readonly ",
        "share ",
        "strong ",
        "weak ",
    ] {
        if let Some(pos) = line_text[search_from..].find(kw) {
            search_from += pos + kw.len();
        }
    }
    let idx = line_text[search_from..].find(name)? + search_from;
    let end_byte = idx + name.len();
    let end_u16 = line_text[..end_byte].encode_utf16().count() as u32;
    Some(Position::new(line_0, end_u16))
}

fn make_ownership_hint(pos: Position, label: &str, tooltip: &str) -> InlayHint {
    InlayHint {
        position: pos,
        label: InlayHintLabel::String(format!(" «{}»", label)),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: Some(InlayHintTooltip::String(tooltip.to_string())),
        padding_left: Some(true),
        padding_right: Some(false),
        data: None,
    }
}

fn truncate_hint(s: &str, max_len: usize) -> String {
    if max_len == 0 || s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Legacy compatibility entry point kept for existing callers/tests.
#[allow(dead_code)]
pub fn get_inlay_hints(doc: &Document, _symbols: &[SymbolInfo], range: Range) -> Vec<InlayHint> {
    let index = adeshlang::semantics::index_source(&doc.content);
    let ws = WorkspaceIndex::new();
    get_inlay_hints_semantic(&index, &ws, doc, range, &AlsConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inlay_hints_types_params_and_return() {
        let src = "fn add(a: i32, b: i32) {\n    return a + b;\n}\nlet x = 42;\nlet y: i32 = 10;\nlet sum = add(1, 2);\n";
        let uri = Url::parse("file:///hints.adesh").unwrap();
        let doc = Document::new(uri.clone(), src.to_string(), 1);
        let mut ws = WorkspaceIndex::new();
        ws.index_document(&uri, src);
        let idx = ws.get_index(&uri).unwrap();

        let hints = get_inlay_hints_semantic(
            idx,
            &ws,
            &doc,
            Range::new(Position::new(0, 0), Position::new(10, 0)),
            &AlsConfig::default(),
        );

        // `let x = 42` gets a type hint; `let y: i32 = 10` does NOT (already annotated)
        assert!(
            hints
                .iter()
                .any(|h| h.position.line == 3 && h.kind == Some(InlayHintKind::TYPE)),
            "Expected type hint on line 3 (`let x = 42`): {:?}",
            hints
        );
        assert!(
            !hints
                .iter()
                .any(|h| h.position.line == 4 && h.kind == Some(InlayHintKind::TYPE)),
            "Should NOT emit type hint on already-annotated line 4: {:?}",
            hints
        );
        // `add(1, 2)` gets parameter hints `a:` and `b:`
        let param_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.position.line == 5 && h.kind == Some(InlayHintKind::PARAMETER))
            .collect();
        assert_eq!(param_hints.len(), 2, "Expected 2 param hints: {:?}", hints);
    }
}
