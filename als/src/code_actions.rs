//! Code Actions, Quick Fixes & Refactorings Provider
//!
//! Provides IDE-grade quick fixes, refactorings, and source actions:
//! - Quick Fixes:
//!   - Fix typo ("Did you mean `...`?")
//!   - Smart workspace & stdlib Auto-Import (`import { Sym } from "./path";`)
//!   - Prefix unused variable/parameter with `_`
//!   - Remove entire unused declaration statement safely
//!   - Remove unused import
//!   - Remove unreachable code
//!   - Implement missing interface methods on `class Foo implements Bar`
//!   - Convert `const` to `let` on immutable reassignment error
//!   - Add `async` keyword to function using `await`
//!   - Insert missing semicolon `;`
//! - Refactorings:
//!   - Add explicit type annotation to `let`/`const` variable
//!   - Add explicit return type annotation to function
//!   - Convert `let` <-> `const`
//!   - Generate doc comment (`/// ...`) for function/class/struct/interface/enum
//!   - Extract selected expression into a local variable
//!   - Wrap selection in `try { ... } catch (err) { ... }` or `if (cond) { ... }`
//! - Source Actions:
//!   - Organize Imports (`source.organizeImports`)

use crate::document::Document;
use crate::type_utils::{
    find_block_end_line, format_ty, infer_function_return_type, is_concrete_ty, STDLIB_MODULES,
};
use crate::workspace::WorkspaceIndex;
use adeshlang::parsing::ast::StmtKind;
use adeshlang::semantics::SemanticIndex;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, Position, Range, TextEdit, Url,
    WorkspaceEdit,
};
use std::collections::HashMap;

/// Compute all applicable code actions (quick fixes, refactorings, source actions) for a range.
pub fn get_code_actions(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    range: Range,
    diagnostics: &[Diagnostic],
    only: Option<&[CodeActionKind]>,
) -> Vec<CodeActionOrCommand> {
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    let wants_kind = |kind: &CodeActionKind| -> bool {
        match only {
            None => true,
            Some(filter) if filter.is_empty() => true,
            Some(filter) => filter
                .iter()
                .any(|f| kind.as_str().starts_with(f.as_str()) || f.as_str() == kind.as_str()),
        }
    };

    // 1. Diagnostic-driven Quick Fixes
    if wants_kind(&CodeActionKind::QUICKFIX) {
        for diag in diagnostics {
            generate_diagnostic_quick_fixes(index, workspace, doc, diag, &mut actions);
        }
    }

    // 2. Context-sensitive Refactorings
    if wants_kind(&CodeActionKind::REFACTOR)
        || wants_kind(&CodeActionKind::REFACTOR_REWRITE)
        || wants_kind(&CodeActionKind::REFACTOR_EXTRACT)
    {
        generate_refactor_actions(index, doc, range, &mut actions, &wants_kind);
    }

    // 3. Source Actions (Organize Imports)
    if wants_kind(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS) || wants_kind(&CodeActionKind::SOURCE) {
        if let Some(action) = create_organize_imports_action(index, doc) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
    }

    actions
}

// ============================================================================
// Quick Fixes
// ============================================================================

fn generate_diagnostic_quick_fixes(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    diag: &Diagnostic,
    actions: &mut Vec<CodeActionOrCommand>,
) {
    let msg = diag.message.as_str();
    let code_str = diag.code.as_ref().map(|c| match c {
        lsp_types::NumberOrString::String(s) => s.as_str(),
        lsp_types::NumberOrString::Number(_) => "",
    });

    // A. Structured diagnostic data payload (from our enhanced diagnostics)
    if let Some(data) = &diag.data {
        // Typo suggestion: `{ "didYouMean": "actualName" }`
        if let Some(suggestion) = data.get("didYouMean").and_then(|v| v.as_str()) {
            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                format!("Replace with `{}`", suggestion),
                CodeActionKind::QUICKFIX,
                Some(vec![diag.clone()]),
                &doc.uri,
                diag.range,
                suggestion.to_string(),
                true,
            )));
        }

        // Missing interface methods: `{ "missingMethods": [...], "classLine": ..., "interface": "..." }`
        if let (Some(iface), Some(methods_val), Some(class_line)) = (
            data.get("interface").and_then(|v| v.as_str()),
            data.get("missingMethods").and_then(|v| v.as_array()),
            data.get("classLine").and_then(|v| v.as_u64()),
        ) {
            let stubs: Vec<String> = methods_val
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !stubs.is_empty() {
                if let Some(action) =
                    create_implement_methods_action(doc, diag, iface, class_line as usize, &stubs)
                {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
        }
    }

    // Also parse inline "Did you mean `foo`?" from any diagnostic message
    if let Some(suggestion) = extract_did_you_mean(msg) {
        let already = actions.iter().any(|a| match a {
            CodeActionOrCommand::CodeAction(ca) => ca.title.contains(&suggestion),
            _ => false,
        });
        if !already {
            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                format!("Replace with `{}`", suggestion),
                CodeActionKind::QUICKFIX,
                Some(vec![diag.clone()]),
                &doc.uri,
                diag.range,
                suggestion,
                true,
            )));
        }
    }

    // B. Undefined / not found identifier -> Smart Auto-Import from workspace or stdlib
    if msg.contains("not found")
        || msg.contains("undefined")
        || msg.contains("Undefined")
        || code_str == Some("undefined-symbol")
    {
        if let Some(ident) = extract_backtick_ident(msg) {
            add_smart_import_actions(index, workspace, doc, diag, &ident, actions);
        }
    }

    // C. Unused variable / parameter
    if code_str == Some("unused")
        || code_str == Some("unused-param")
        || msg.contains("unused variable")
        || msg.contains("unused parameter")
        || msg.contains("never used")
    {
        if let Some(var_name) = extract_backtick_ident(msg) {
            // Option 1 (Preferred): prefix with `_`
            if !var_name.starts_with('_') {
                actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                    format!("Prefix `{}` with underscore (`_{}`)", var_name, var_name),
                    CodeActionKind::QUICKFIX,
                    Some(vec![diag.clone()]),
                    &doc.uri,
                    diag.range,
                    format!("_{}", var_name),
                    true,
                )));
            }

            // Option 2: safely remove entire declaration line (only for standalone let/const line)
            if code_str != Some("unused-param") {
                if let Some(line_range) = full_line_delete_range(doc, diag.range.start.line) {
                    if let Some(line_text) = doc.get_line(diag.range.start.line as usize) {
                        let trimmed = line_text.trim();
                        if (trimmed.starts_with("let ")
                            || trimmed.starts_with("const ")
                            || trimmed.starts_with("readonly "))
                            && trimmed.ends_with(';')
                        {
                            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                                format!("Remove unused declaration `{}`", var_name),
                                CodeActionKind::QUICKFIX,
                                Some(vec![diag.clone()]),
                                &doc.uri,
                                line_range,
                                String::new(),
                                false,
                            )));
                        }
                    }
                }
            }
        }
    }

    // D. Unused import -> remove import line
    if code_str == Some("unused-import") || msg.contains("unused import") {
        if let Some((range, new_text)) = unused_import_edit(doc, diag) {
            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                "Remove unused import".to_string(),
                CodeActionKind::QUICKFIX,
                Some(vec![diag.clone()]),
                &doc.uri,
                range,
                new_text,
                true,
            )));
        }
    }

    // E. Unreachable code -> remove unreachable line/statement
    if code_str == Some("unreachable") || msg.contains("unreachable code") {
        if let Some(line_range) = full_line_delete_range(doc, diag.range.start.line) {
            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                "Remove unreachable code".to_string(),
                CodeActionKind::QUICKFIX,
                Some(vec![diag.clone()]),
                &doc.uri,
                line_range,
                String::new(),
                true,
            )));
        }
    }

    // F. Reassignment to `const` / `readonly` -> convert declaration to `let`
    if code_str == Some("const-reassign") || msg.contains("Cannot reassign constant") {
        if let Some(var_name) = extract_backtick_ident(msg) {
            if let Some(sym) = index.find_declaration(&var_name) {
                let decl_line = sym.line.saturating_sub(1);
                if let Some(line_text) = doc.get_line(decl_line) {
                    for kw in ["const", "readonly"] {
                        if let Some(idx) = line_text.find(kw) {
                            let start_col = line_text[..idx].encode_utf16().count() as u32;
                            let end_col = start_col + kw.len() as u32;
                            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                                format!("Change `{}` to `let` for `{}`", kw, var_name),
                                CodeActionKind::QUICKFIX,
                                Some(vec![diag.clone()]),
                                &doc.uri,
                                Range::new(
                                    Position::new(decl_line as u32, start_col),
                                    Position::new(decl_line as u32, end_col),
                                ),
                                "let".to_string(),
                                true,
                            )));
                            break;
                        }
                    }
                }
            }
        }
    }

    // G. `await` outside `async fn` -> add `async` keyword to enclosing function
    if code_str == Some("await-in-sync")
        || msg.contains("`await` can only be used inside an `async`")
    {
        if let Some(fn_line) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("fnLine"))
            .and_then(|v| v.as_u64())
        {
            let line_0 = fn_line.saturating_sub(1) as usize;
            if let Some(line_text) = doc.get_line(line_0) {
                if let Some(fn_idx) = line_text.find("fn ") {
                    let col = line_text[..fn_idx].encode_utf16().count() as u32;
                    let pos = Position::new(line_0 as u32, col);
                    actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                        "Make enclosing function `async`".to_string(),
                        CodeActionKind::QUICKFIX,
                        Some(vec![diag.clone()]),
                        &doc.uri,
                        Range::new(pos, pos),
                        "async ".to_string(),
                        true,
                    )));
                }
            }
        }
    }

    // H. Missing semicolon
    if msg.contains("Expected ';'") || msg.contains("expected ';'") {
        let line_0 = diag.range.start.line as usize;
        if let Some(line_text) = doc.get_line(line_0) {
            let trimmed_end = line_text.trim_end();
            if !trimmed_end.ends_with(';')
                && !trimmed_end.ends_with('{')
                && !trimmed_end.ends_with('}')
            {
                let col = trimmed_end.encode_utf16().count() as u32;
                let pos = Position::new(line_0 as u32, col);
                actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                    "Insert missing `;`".to_string(),
                    CodeActionKind::QUICKFIX,
                    Some(vec![diag.clone()]),
                    &doc.uri,
                    Range::new(pos, pos),
                    ";".to_string(),
                    true,
                )));
            }
        }
    }
}

fn add_smart_import_actions(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    diag: &Diagnostic,
    ident: &str,
    actions: &mut Vec<CodeActionOrCommand>,
) {
    let insert_line = compute_import_insert_line(index);
    let insert_pos = Position::new(insert_line, 0);

    // 1. Check workspace files exporting/declaring `ident`
    let candidates = workspace.find_export_candidates(ident, &doc.uri);
    for (target_uri, _sym) in candidates {
        if let Some(rel_path) = workspace.relative_import_path(&doc.uri, target_uri) {
            let import_stmt = format!("import {{ {} }} from \"{}\";\n", ident, rel_path);
            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                format!("Import `{}` from \"{}\"", ident, rel_path),
                CodeActionKind::QUICKFIX,
                Some(vec![diag.clone()]),
                &doc.uri,
                Range::new(insert_pos, insert_pos),
                import_stmt,
                true,
            )));
        }
    }

    // 2. Check Adesh stdlib modules (`IO`, `HTTP`, `Crypto`, ...)
    for std_mod in STDLIB_MODULES {
        if std_mod.name == ident {
            let import_stmt = format!("import \"{}\";\n", std_mod.name);
            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                format!("Import stdlib module \"{}\"", std_mod.name),
                CodeActionKind::QUICKFIX,
                Some(vec![diag.clone()]),
                &doc.uri,
                Range::new(insert_pos, insert_pos),
                import_stmt,
                true,
            )));
        } else if std_mod.exports.iter().any(|e| e.name == ident) {
            let import_stmt = format!("import {{ {} }} from \"{}\";\n", ident, std_mod.name);
            actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                format!("Import `{}` from stdlib \"{}\"", ident, std_mod.name),
                CodeActionKind::QUICKFIX,
                Some(vec![diag.clone()]),
                &doc.uri,
                Range::new(insert_pos, insert_pos),
                import_stmt,
                true,
            )));
        }
    }
}

fn create_implement_methods_action(
    doc: &Document,
    diag: &Diagnostic,
    iface_name: &str,
    class_line_1based: usize,
    method_stubs: &[String],
) -> Option<CodeAction> {
    let end_line_1based = find_block_end_line(&doc.content, class_line_1based)?;
    let insert_line = end_line_1based.saturating_sub(1) as u32;
    let insert_pos = Position::new(insert_line, 0);

    let mut body = String::new();
    for stub in method_stubs {
        body.push_str(&format!(
            "    {} {{\n        // TODO: implement\n    }}\n",
            stub
        ));
    }

    Some(make_single_edit_action(
        format!(
            "Implement missing methods from `{}` ({})",
            iface_name,
            method_stubs.len()
        ),
        CodeActionKind::QUICKFIX,
        Some(vec![diag.clone()]),
        &doc.uri,
        Range::new(insert_pos, insert_pos),
        body,
        true,
    ))
}

// ============================================================================
// Refactorings
// ============================================================================

fn generate_refactor_actions<F: Fn(&CodeActionKind) -> bool>(
    index: &SemanticIndex,
    doc: &Document,
    range: Range,
    actions: &mut Vec<CodeActionOrCommand>,
    wants_kind: &F,
) {
    let cursor_line_0 = range.start.line as usize;
    let cursor_line_1 = cursor_line_0 + 1;

    if wants_kind(&CodeActionKind::REFACTOR_REWRITE) {
        // 1. Add explicit type annotation to `let x = ...` or `const x = ...`
        for stmt in &index.statements {
            if stmt.span.line == cursor_line_1 {
                if let StmtKind::Let(name, Some(init), None, _, _, _) = &stmt.kind {
                    let inferred = index.infer_type(init);
                    if is_concrete_ty(&inferred) {
                        let ty_str = format_ty(&inferred);
                        if let Some(line_text) = doc.get_line(cursor_line_0) {
                            if let Some(name_pos) = find_decl_name_end(line_text, name) {
                                let pos = Position::new(range.start.line, name_pos);
                                actions.push(CodeActionOrCommand::CodeAction(
                                    make_single_edit_action(
                                        format!("Add explicit type annotation `: {}`", ty_str),
                                        CodeActionKind::REFACTOR_REWRITE,
                                        None,
                                        &doc.uri,
                                        Range::new(pos, pos),
                                        format!(": {}", ty_str),
                                        false,
                                    ),
                                ));
                            }
                        }
                    }
                }

                // 2. Add explicit return type to `fn foo(...) { ... }`
                if let StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) =
                    &stmt.kind
                {
                    if func.ret_type.is_none() {
                        if let Some(inferred_ret) = infer_function_return_type(index, func) {
                            let ty_str = format_ty(&inferred_ret);
                            if let Some(line_text) = doc.get_line(cursor_line_0) {
                                if let Some(close_paren) =
                                    signature_close_paren(index, cursor_line_1)
                                {
                                    let col =
                                        line_text[..close_paren + 1].encode_utf16().count() as u32;
                                    let pos = Position::new(range.start.line, col);
                                    actions.push(CodeActionOrCommand::CodeAction(
                                        make_single_edit_action(
                                            format!("Add explicit return type `: {}`", ty_str),
                                            CodeActionKind::REFACTOR_REWRITE,
                                            None,
                                            &doc.uri,
                                            Range::new(pos, pos),
                                            format!(": {}", ty_str),
                                            false,
                                        ),
                                    ));
                                }
                            }
                        }
                    }

                    // 3. Generate doc comment for function
                    if !has_preceding_comment(doc, cursor_line_0) {
                        let mut comment = format!("/// {}\n", func.name);
                        for (pname, _, ptype) in &func.params {
                            match ptype {
                                Some(t) => {
                                    comment.push_str(&format!("/// @param {} ({})\n", pname, t))
                                }
                                None => comment.push_str(&format!("/// @param {}\n", pname)),
                            }
                        }
                        if let Some(ret) = &func.ret_type {
                            comment.push_str(&format!("/// @returns {}\n", ret));
                        }
                        let indent = leading_whitespace(doc.get_line(cursor_line_0).unwrap_or(""));
                        let indented = comment
                            .lines()
                            .map(|l| format!("{}{}\n", indent, l))
                            .collect::<String>();
                        let pos = Position::new(range.start.line, 0);
                        actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                            format!("Generate doc comment for `{}`", func.name),
                            CodeActionKind::REFACTOR_REWRITE,
                            None,
                            &doc.uri,
                            Range::new(pos, pos),
                            indented,
                            false,
                        )));
                    }
                }
            }
        }

        // 4. Convert `let` <-> `const` on current line
        if let Some(line_text) = doc.get_line(cursor_line_0) {
            let trimmed = line_text.trim_start();
            let indent_u16 = (line_text.len() - trimmed.len()) as u32;
            if trimmed.starts_with("let ") {
                actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                    "Convert `let` to `const`".to_string(),
                    CodeActionKind::REFACTOR_REWRITE,
                    None,
                    &doc.uri,
                    Range::new(
                        Position::new(range.start.line, indent_u16),
                        Position::new(range.start.line, indent_u16 + 3),
                    ),
                    "const".to_string(),
                    false,
                )));
            } else if trimmed.starts_with("const ") {
                actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                    "Convert `const` to `let`".to_string(),
                    CodeActionKind::REFACTOR_REWRITE,
                    None,
                    &doc.uri,
                    Range::new(
                        Position::new(range.start.line, indent_u16),
                        Position::new(range.start.line, indent_u16 + 5),
                    ),
                    "let".to_string(),
                    false,
                )));
            }
        }
    }

    // 5. Selection-based refactorings (Extract Variable, Wrap in try/catch, Wrap in if)
    if range.start != range.end {
        if let (Some(start_off), Some(end_off)) = (
            doc.position_to_offset(range.start.line, range.start.character),
            doc.position_to_offset(range.end.line, range.end.character),
        ) {
            if start_off < end_off && end_off <= doc.content.len() {
                let selected = doc.content[start_off..end_off].trim();
                let indent =
                    leading_whitespace(doc.get_line(range.start.line as usize).unwrap_or(""));

                // Extract single-line expression to local variable
                if wants_kind(&CodeActionKind::REFACTOR_EXTRACT)
                    && !selected.is_empty()
                    && !selected.contains('\n')
                    && !selected.ends_with(';')
                {
                    let line_start = Position::new(range.start.line, 0);
                    let mut changes = HashMap::new();
                    changes.insert(
                        doc.uri.clone(),
                        vec![
                            TextEdit {
                                range: Range::new(line_start, line_start),
                                new_text: format!("{}let extracted = {};\n", indent, selected),
                            },
                            TextEdit {
                                range,
                                new_text: "extracted".to_string(),
                            },
                        ],
                    );
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: "Extract to local variable `extracted`".to_string(),
                        kind: Some(CodeActionKind::REFACTOR_EXTRACT),
                        diagnostics: None,
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            document_changes: None,
                            change_annotations: None,
                        }),
                        command: None,
                        is_preferred: Some(false),
                        disabled: None,
                        data: None,
                    }));
                }

                // Wrap selection in try / catch
                if wants_kind(&CodeActionKind::REFACTOR_REWRITE) && !selected.is_empty() {
                    let body_indented = selected
                        .lines()
                        .map(|l| format!("{}    {}\n", indent, l.trim()))
                        .collect::<String>();
                    let wrapped = format!(
                        "try {{\n{}{}}} catch (err) {{\n{}    print(\"Error:\", err);\n{}}}",
                        body_indented, indent, indent, indent
                    );
                    actions.push(CodeActionOrCommand::CodeAction(make_single_edit_action(
                        "Wrap selection in `try / catch`".to_string(),
                        CodeActionKind::REFACTOR_REWRITE,
                        None,
                        &doc.uri,
                        range,
                        wrapped,
                        false,
                    )));
                }
            }
        }
    }
}

// ============================================================================
// Organize Imports (`source.organizeImports`)
// ============================================================================

/// Create an "Organize Imports" source action (sorts stdlib before relative imports & deduplicates).
pub fn create_organize_imports_action(index: &SemanticIndex, doc: &Document) -> Option<CodeAction> {
    let mut import_lines: Vec<(usize, String)> = Vec::new();
    for (idx, line) in doc.content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") && trimmed.ends_with(';') {
            import_lines.push((idx, trimmed.to_string()));
        } else if !trimmed.is_empty() && !import_lines.is_empty() {
            break;
        }
    }

    if import_lines.len() < 2 {
        let _ = index;
        return None;
    }

    let first_line = import_lines.first()?.0 as u32;
    let last_line = import_lines.last()?.0 as u32;

    let mut stdlib_imports = Vec::new();
    let mut local_imports = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (_, line) in &import_lines {
        if !seen.insert(line.clone()) {
            continue;
        }
        if line.contains("\"./")
            || line.contains("\"../")
            || line.contains("'./")
            || line.contains("'../")
        {
            local_imports.push(line.clone());
        } else {
            stdlib_imports.push(line.clone());
        }
    }

    stdlib_imports.sort();
    local_imports.sort();

    let mut organized = stdlib_imports;
    organized.extend(local_imports);

    let new_text = format!("{}\n", organized.join("\n"));
    let old_text = (first_line..=last_line)
        .filter_map(|l| doc.get_line(l as usize))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    if new_text == old_text {
        return None;
    }

    Some(make_single_edit_action(
        "Organize imports".to_string(),
        CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
        None,
        &doc.uri,
        Range::new(
            Position::new(first_line, 0),
            Position::new(last_line + 1, 0),
        ),
        new_text,
        false,
    ))
}

// ============================================================================
// Helpers
// ============================================================================

fn make_single_edit_action(
    title: String,
    kind: CodeActionKind,
    diagnostics: Option<Vec<Diagnostic>>,
    uri: &Url,
    range: Range,
    new_text: String,
    is_preferred: bool,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);
    CodeAction {
        title,
        kind: Some(kind),
        diagnostics,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(is_preferred),
        disabled: None,
        data: None,
    }
}

fn compute_import_insert_line(index: &SemanticIndex) -> u32 {
    index
        .imports
        .iter()
        .map(|i| i.line as u32)
        .max()
        .unwrap_or(0)
}

fn full_line_delete_range(doc: &Document, line: u32) -> Option<Range> {
    let line_usize = line as usize;
    if line_usize >= doc.line_count() {
        return None;
    }
    if line_usize + 1 < doc.line_count() {
        Some(Range::new(
            Position::new(line, 0),
            Position::new(line + 1, 0),
        ))
    } else {
        let len = doc.get_line(line_usize)?.encode_utf16().count() as u32;
        Some(Range::new(Position::new(line, 0), Position::new(line, len)))
    }
}

fn signature_close_paren(index: &SemanticIndex, line: usize) -> Option<usize> {
    let mut depth = 0usize;
    for token in index.tokens.iter().filter(|t| t.line == line) {
        match token.lexeme.as_str() {
            "(" => depth += 1,
            ")" if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return Some(token.col.saturating_sub(1));
                }
            }
            _ => {}
        }
    }
    None
}

fn unused_import_edit(doc: &Document, diag: &Diagnostic) -> Option<(Range, String)> {
    let line = diag.range.start.line;
    let text = doc.get_line(line as usize)?;
    if !text.trim_start().starts_with("import ")
        || text.matches(';').count() != 1
        || !text.trim_end().ends_with(';')
    {
        return None;
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.find('}')) {
        let name = extract_backtick_ident(&diag.message)?;
        let parts: Vec<_> = text[start + 1..end]
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if !parts.contains(&name.as_str()) {
            return None;
        }
        let remaining: Vec<_> = parts.into_iter().filter(|p| *p != name).collect();
        if !remaining.is_empty() {
            return Some((
                Range::new(
                    Position::new(line, text[..start + 1].encode_utf16().count() as u32),
                    Position::new(line, text[..end].encode_utf16().count() as u32),
                ),
                format!(" {} ", remaining.join(", ")),
            ));
        }
    }
    Some((full_line_delete_range(doc, line)?, String::new()))
}

fn extract_backtick_ident(message: &str) -> Option<String> {
    let start = message.find('`')?;
    let rest = &message[start + 1..];
    let end = rest.find('`')?;
    let ident = rest[..end].trim();
    if ident.is_empty() {
        None
    } else {
        Some(ident.to_string())
    }
}

fn extract_did_you_mean(message: &str) -> Option<String> {
    let idx = message.find("Did you mean `")?;
    let rest = &message[idx + "Did you mean `".len()..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

fn find_decl_name_end(line_text: &str, name: &str) -> Option<u32> {
    let idx = line_text.find(name)?;
    let end_byte = idx + name.len();
    Some(line_text[..end_byte].encode_utf16().count() as u32)
}

fn has_preceding_comment(doc: &Document, line_0: usize) -> bool {
    if line_0 == 0 {
        return false;
    }
    doc.get_line(line_0 - 1)
        .is_some_and(|l| l.trim().starts_with("//") || l.trim().ends_with("*/"))
}

fn leading_whitespace(line: &str) -> &str {
    let non_ws = line.len() - line.trim_start().len();
    &line[..non_ws]
}
