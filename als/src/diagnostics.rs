//! Diagnostics Provider
//!
//! Provides compiler-quality diagnostics using the compiler's type checker,
//! the shared semantic engine, and rich static analysis passes:
//! - Syntax/parse errors (from the recovering parser)
//! - Type checking errors (from the compiler's type checker) with "Did you mean...?" hints
//! - Function, method, and builtin call arity checks
//! - Unknown member access checks with Levenshtein "Did you mean...?" suggestions
//! - Interface conformance checks (missing methods with structured quick-fix data)
//! - Immutable (`const`/`readonly`) reassignment checks
//! - `await` outside `async` function checks (with quick-fix data)
//! - Unused variables, constants, parameters, and imports (respecting `_` prefix & templates)
//! - Unreachable code detection
//! - Optional `unsafe` block warnings and strict-mode missing type annotation hints

use crate::analysis::{analyze, error_to_diagnostic, AnalysisResult};
use crate::config::AlsConfig;
use crate::document::Document;
use crate::type_utils::{
    extract_nominal_type_name, find_builtin_fn, find_closest_match, find_namespace_members,
    get_type_members_workspace, resolve_expression_chain_type, visit_exprs_in_stmts, ARRAY_METHODS,
    STRING_METHODS,
};
use crate::workspace::WorkspaceIndex;
use adeshlang::parsing::ast::{Expr, ExprKind, Function, Stmt, StmtKind, TokenKind};
use adeshlang::parsing::lexer::extract_identifiers_from_template;
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind, TypeDefinition};
use adeshlang::typesystem::checker::Ty;
use adeshlang::typesystem::type_system::check_module_in;
use lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag, Location,
    NumberOrString, Position, Range,
};
use std::collections::{HashMap, HashSet};

/// Compute diagnostics with workspace awareness and ALS configuration.
pub fn compute_diagnostics_semantic_with_config(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    config: &AlsConfig,
) -> Vec<Diagnostic> {
    if !config.diagnostics.enabled {
        return Vec::new();
    }

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
        let mut diag = error_to_diagnostic_semantic(&type_error, doc);
        enrich_undefined_diagnostic(index, workspace, &mut diag);
        diagnostics.push(diag);
    }

    // 3. Semantic static analysis passes
    if config.diagnostics.unused_warnings {
        diagnostics.extend(check_unused_variables_semantic(index, doc));
        diagnostics.extend(check_unused_imports_semantic(index, doc));
    }
    if config.diagnostics.unreachable_warnings {
        diagnostics.extend(check_unreachable_code_semantic(index, doc));
    }
    if config.diagnostics.arity_checks {
        diagnostics.extend(check_call_arities(index, workspace, doc));
    }
    if config.diagnostics.member_checks {
        diagnostics.extend(check_unknown_members(index, workspace, doc));
    }

    diagnostics.extend(check_missing_interface_methods(index, workspace, doc));
    diagnostics.extend(check_const_reassignments(index, doc));
    diagnostics.extend(check_await_outside_async(index, doc));

    if config.unsafe_cfg.warn_on_usage {
        diagnostics.extend(check_unsafe_blocks(index, doc));
    }
    if config.type_assistance.strict_mode {
        diagnostics.extend(check_strict_type_annotations(index, doc));
    }

    // Sort & deduplicate by (line, col, message)
    diagnostics.sort_by(|a, b| {
        (a.range.start.line, a.range.start.character, &a.message).cmp(&(
            b.range.start.line,
            b.range.start.character,
            &b.message,
        ))
    });
    diagnostics.dedup_by(|a, b| {
        a.range.start.line == b.range.start.line
            && a.range.start.character == b.range.start.character
            && a.message == b.message
    });

    diagnostics
}

/// Backward-compatible diagnostics entry point with default workspace and config.
pub fn compute_diagnostics_semantic(index: &SemanticIndex, doc: &Document) -> Vec<Diagnostic> {
    let empty_ws = WorkspaceIndex::new();
    compute_diagnostics_semantic_with_config(index, &empty_ws, doc, &AlsConfig::default())
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

    let mut end = Position {
        line: error.end_line.saturating_sub(1) as u32,
        character: error.end_col.saturating_sub(1) as u32,
    };
    if end <= start {
        end = Position {
            line: start.line,
            character: start.character + 1,
        };
    }

    let severity = match error.kind {
        adeshlang::parsing::error::ErrorKind::Lexical
        | adeshlang::parsing::error::ErrorKind::Parse
        | adeshlang::parsing::error::ErrorKind::Type
        | adeshlang::parsing::error::ErrorKind::Compile => DiagnosticSeverity::ERROR,
        adeshlang::parsing::error::ErrorKind::Runtime
        | adeshlang::parsing::error::ErrorKind::Jit => DiagnosticSeverity::WARNING,
        _ => DiagnosticSeverity::INFORMATION,
    };

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

/// If a compiler diagnostic mentions an undefined symbol, attach a "Did you mean `...`?" suggestion.
fn enrich_undefined_diagnostic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    diag: &mut Diagnostic,
) {
    let msg = diag.message.as_str();
    if !(msg.contains("Undefined") || msg.contains("undefined") || msg.contains("not found")) {
        return;
    }
    if msg.contains("Did you mean") {
        return;
    }

    let ident = extract_quoted_ident(msg);
    let Some(name) = ident else {
        return;
    };

    let mut candidates: Vec<&str> = index.symbols.iter().map(|s| s.name.as_str()).collect();
    for tname in index.types.keys() {
        candidates.push(tname.as_str());
    }
    for (_, ws_idx) in workspace.iter_indices() {
        for s in &ws_idx.symbols {
            if s.parent_type.is_none() {
                candidates.push(s.name.as_str());
            }
        }
    }
    for b in crate::type_utils::BUILTIN_FUNCTIONS {
        candidates.push(b.name);
    }

    if let Some(suggestion) = find_closest_match(&name, candidates.into_iter()) {
        diag.message = format!(
            "{}. Did you mean `{}`?",
            diag.message.trim_end_matches('.'),
            suggestion
        );
        diag.data = Some(serde_json::json!({ "didYouMean": suggestion }));
    }
}

fn extract_quoted_ident(msg: &str) -> Option<String> {
    for quote in ['`', '\'', '"'] {
        if let Some(start) = msg.find(quote) {
            let rest = &msg[start + 1..];
            if let Some(end) = rest.find(quote) {
                let candidate = rest[..end].trim();
                if !candidate.is_empty()
                    && candidate.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

/// Count all occurrences of an identifier name across tokens, including
/// inside `${...}` interpolations of `TokenKind::Template` tokens.
fn count_name_occurrences(tokens: &[adeshlang::parsing::lexer::Token], name: &str) -> usize {
    let mut count = 0usize;
    for tok in tokens {
        match tok.kind {
            TokenKind::Identifier if tok.lexeme == name => count += 1,
            TokenKind::Template => {
                for id in extract_identifiers_from_template(&tok.lexeme) {
                    if id == name {
                        count += 1;
                    }
                }
            }
            _ => {}
        }
    }
    count
}

fn check_unused_variables_semantic(index: &SemanticIndex, _doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Collect exported names so we never flag exported constants/variables as unused
    let mut exported_names: HashSet<&str> = HashSet::new();
    for stmt in &index.statements {
        match &stmt.kind {
            StmtKind::Let(name, _, _, true, _, _) => {
                exported_names.insert(name.as_str());
            }
            StmtKind::ShareDeclaration(d, true) => {
                exported_names.insert(d.name.as_str());
            }
            StmtKind::StrongDeclaration(d, true) => {
                exported_names.insert(d.name.as_str());
            }
            StmtKind::WeakDeclaration(d, true) => {
                exported_names.insert(d.name.as_str());
            }
            StmtKind::ExportDefault(name) => {
                exported_names.insert(name.as_str());
            }
            _ => {}
        }
    }

    for sym in &index.symbols {
        let is_var_or_const = matches!(
            sym.kind,
            SemanticSymbolKind::Variable | SemanticSymbolKind::Constant
        );
        let is_param = sym.kind == SemanticSymbolKind::Parameter;
        if !is_var_or_const && !is_param {
            continue;
        }
        if sym.name.starts_with('_')
            || sym.name == "this"
            || sym.name == "self"
            || exported_names.contains(sym.name.as_str())
        {
            continue;
        }

        let occurrences = count_name_occurrences(&index.tokens, &sym.name);
        if occurrences <= 1 && sym.line > 0 {
            let start = Position {
                line: sym.line.saturating_sub(1) as u32,
                character: sym.col.saturating_sub(1) as u32,
            };
            let end = Position {
                line: start.line,
                character: start.character + sym.name.encode_utf16().count() as u32,
            };

            let (code, message) = if is_param {
                ("unused-param", format!("unused parameter: `{}`", sym.name))
            } else {
                ("unused", format!("unused variable: `{}`", sym.name))
            };

            diagnostics.push(Diagnostic {
                range: Range { start, end },
                severity: Some(DiagnosticSeverity::HINT),
                code: Some(NumberOrString::String(code.to_string())),
                code_description: None,
                source: Some("als".to_string()),
                message,
                related_information: None,
                tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                data: None,
            });
        }
    }

    diagnostics
}

fn check_unused_imports_semantic(index: &SemanticIndex, doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for stmt in &index.statements {
        let line_0 = stmt.span.line.saturating_sub(1) as u32;
        let line_text = doc.get_line(line_0 as usize).unwrap_or("");
        match &stmt.kind {
            StmtKind::Import { alias, .. } | StmtKind::ImportDefault { alias, .. } => {
                if !alias.is_empty() && !alias.starts_with('_') {
                    let count = count_name_occurrences(&index.tokens, alias);
                    if count <= 1 {
                        let range = find_name_range_on_line(line_0, line_text, alias);
                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::HINT),
                            code: Some(NumberOrString::String("unused-import".to_string())),
                            code_description: None,
                            source: Some("als".to_string()),
                            message: format!("unused import: `{}`", alias),
                            related_information: None,
                            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                            data: None,
                        });
                    }
                }
            }
            StmtKind::ImportNames { names, .. } => {
                for name in names {
                    if name.starts_with('_') || name == "*" {
                        continue;
                    }
                    let count = count_name_occurrences(&index.tokens, name);
                    if count <= 1 {
                        let range = find_name_range_on_line(line_0, line_text, name);
                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::HINT),
                            code: Some(NumberOrString::String("unused-import".to_string())),
                            code_description: None,
                            source: Some("als".to_string()),
                            message: format!("unused import: `{}`", name),
                            related_information: None,
                            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                            data: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    diagnostics
}

fn find_name_range_on_line(line_0: u32, line_text: &str, name: &str) -> Range {
    if let Some(idx) = line_text.find(name) {
        let start_col = line_text[..idx].encode_utf16().count() as u32;
        let end_col = start_col + name.encode_utf16().count() as u32;
        Range::new(
            Position::new(line_0, start_col),
            Position::new(line_0, end_col),
        )
    } else {
        Range::new(
            Position::new(line_0, 0),
            Position::new(line_0, line_text.encode_utf16().count() as u32),
        )
    }
}

/// Check for unreachable code using the semantic index.
fn check_unreachable_code_semantic(index: &SemanticIndex, doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_unreachable_in_stmts(&index.statements, doc, &mut diagnostics);
    diagnostics
}

fn check_unreachable_in_stmts(stmts: &[Stmt], doc: &Document, diagnostics: &mut Vec<Diagnostic>) {
    let mut found_terminator = false;
    for s in stmts {
        if found_terminator {
            let line_0 = s.span.line.saturating_sub(1) as u32;
            let start_col = s.span.col.saturating_sub(1) as u32;
            let end_col = doc
                .get_line(line_0 as usize)
                .map(|l| l.trim_end().encode_utf16().count() as u32)
                .unwrap_or(start_col + 20)
                .max(start_col + 1);
            diagnostics.push(Diagnostic {
                range: Range::new(
                    Position::new(line_0, start_col),
                    Position::new(line_0, end_col),
                ),
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
        check_unreachable_in_stmt(s, doc, diagnostics);
    }
}

fn check_unreachable_in_stmt(stmt: &Stmt, doc: &Document, diagnostics: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Block(stmts) => {
            check_unreachable_in_stmts(stmts, doc, diagnostics);
        }
        StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) => {
            check_unreachable_in_stmts(&func.body, doc, diagnostics);
        }
        StmtKind::Class(cls, _) | StmtKind::ExportDefaultClass(cls) => {
            for m in cls.methods.iter().chain(cls.static_methods.iter()) {
                check_unreachable_in_stmts(&m.body, doc, diagnostics);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            check_unreachable_in_stmt(then_branch, doc, diagnostics);
            if let Some(e) = else_branch {
                check_unreachable_in_stmt(e, doc, diagnostics);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::ForIn { body, .. }
        | StmtKind::Region { body, .. }
        | StmtKind::UnsafeBlock(body)
        | StmtKind::Defer(body) => check_unreachable_in_stmt(body, doc, diagnostics),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            check_unreachable_in_stmt(try_block, doc, diagnostics);
            check_unreachable_in_stmt(catch_block, doc, diagnostics);
        }
        _ => {}
    }
}

fn is_terminator(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => true,
        StmtKind::ExprStmt(e) => matches!(e.kind, ExprKind::Throw(_)),
        _ => false,
    }
}

/// Check function, builtin, and method call argument counts.
fn check_call_arities(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    _doc: &Document,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Collect local AST function arity bounds (min_required, max_total)
    let mut fn_arities: HashMap<&str, (usize, usize)> = HashMap::new();
    for stmt in &index.statements {
        if let StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) = &stmt.kind {
            let min = func
                .params
                .iter()
                .filter(|(_, default, _)| default.is_none())
                .count();
            let max = func.params.len();
            fn_arities.insert(func.name.as_str(), (min, max));
        }
    }

    visit_exprs_in_stmts(&index.statements, &mut |expr| {
        let (callee, args) = match &expr.kind {
            ExprKind::Call(c, a, _) | ExprKind::OptCall(c, a, _) => (c.as_ref(), a.as_slice()),
            _ => return,
        };

        // Ignore calls with spread arguments (`...args`)
        if args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_))) {
            return;
        }
        let arg_count = args.len();
        let line_0 = expr.span.line.saturating_sub(1) as u32;
        let col_0 = expr.span.col.saturating_sub(1) as u32;

        match &callee.kind {
            ExprKind::Variable(name) => {
                // Skip constructors (classes/structs can have default field values or custom constructors)
                if index.get_type(name).is_some() || workspace.find_type_definition(name).is_some()
                {
                    return;
                }

                // 1. Local AST function
                if let Some(&(min, max)) = fn_arities.get(name.as_str()) {
                    if arg_count < min || arg_count > max {
                        let expected = if min == max {
                            format!("{}", max)
                        } else {
                            format!("{}..={}", min, max)
                        };
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(line_0, col_0),
                                Position::new(line_0, col_0 + name.encode_utf16().count() as u32),
                            ),
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("arity-mismatch".to_string())),
                            code_description: None,
                            source: Some("als".to_string()),
                            message: format!(
                                "Function `{}` expects {} argument(s), but got {}",
                                name, expected, arg_count
                            ),
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                    return;
                }

                // 2. Builtin function
                if let Some(spec) = find_builtin_fn(name) {
                    let too_few = arg_count < spec.min_args;
                    let too_many = spec.max_args.is_some_and(|m| arg_count > m);
                    if too_few || too_many {
                        let expected = match spec.max_args {
                            Some(m) if m == spec.min_args => format!("{}", m),
                            Some(m) => format!("{}..={}", spec.min_args, m),
                            None => format!("at least {}", spec.min_args),
                        };
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(line_0, col_0),
                                Position::new(line_0, col_0 + name.encode_utf16().count() as u32),
                            ),
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("arity-mismatch".to_string())),
                            code_description: None,
                            source: Some("als".to_string()),
                            message: format!(
                                "Builtin `{}` expects {} argument(s), but got {}",
                                name, expected, arg_count
                            ),
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
            }
            ExprKind::Get(obj, method) => {
                // Check builtin namespace calls (e.g. `Math.sqrt(1, 2)`)
                if let ExprKind::Variable(ns) = &obj.kind {
                    if let Some(members) = find_namespace_members(ns) {
                        if let Some(spec) = members.iter().find(|m| m.name == method && !m.is_const)
                        {
                            let variadic = spec.params.iter().any(|(p, _)| p.starts_with("..."));
                            if !variadic {
                                let min =
                                    spec.params.iter().filter(|(p, _)| !p.contains('?')).count();
                                let max = spec.params.len();
                                if arg_count < min || arg_count > max {
                                    diagnostics.push(Diagnostic {
                                        range: Range::new(
                                            Position::new(line_0, col_0),
                                            Position::new(
                                                line_0,
                                                col_0 + method.encode_utf16().count() as u32,
                                            ),
                                        ),
                                        severity: Some(DiagnosticSeverity::ERROR),
                                        code: Some(NumberOrString::String(
                                            "arity-mismatch".to_string(),
                                        )),
                                        code_description: None,
                                        source: Some("als".to_string()),
                                        message: format!(
                                            "`{}.{}` expects {} argument(s), but got {}",
                                            ns,
                                            method,
                                            if min == max {
                                                format!("{}", max)
                                            } else {
                                                format!("{}..={}", min, max)
                                            },
                                            arg_count
                                        ),
                                        related_information: None,
                                        tags: None,
                                        data: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    });

    diagnostics
}

/// Check member access (`obj.member`) against known user types and builtin namespaces.
fn check_unknown_members(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    visit_exprs_in_stmts(&index.statements, &mut |expr| {
        let (obj, member) = match &expr.kind {
            ExprKind::Get(o, m) | ExprKind::OptGet(o, m) => (o.as_ref(), m.as_str()),
            _ => return,
        };
        if member.is_empty() || expr.span.line == 0 {
            return;
        }

        let line_1based = expr.span.line;
        let line_0 = line_1based.saturating_sub(1) as u32;
        let line_text = doc.get_line(line_0 as usize).unwrap_or("");

        // 1. Builtin namespace (`Math.foo`, `time.foo`, `JSON.foo`, ...)
        if let ExprKind::Variable(ns) = &obj.kind {
            if let Some(members) = find_namespace_members(ns) {
                if !members.iter().any(|m| m.name == member) {
                    let suggestion = find_closest_match(member, members.iter().map(|m| m.name));
                    diagnostics.push(build_unknown_member_diag(
                        line_0, line_text, ns, member, suggestion,
                    ));
                }
                return;
            }

            // Static access on a known type name (e.g. `Color.Purple` or `User.staticFn`)
            if let Some(td) = index
                .get_type(ns)
                .or_else(|| workspace.find_type_definition(ns))
            {
                let mut valid: Vec<String> = Vec::new();
                for (v, _) in &td.variants {
                    valid.push(v.clone());
                }
                for m in &td.static_methods {
                    valid.push(m.name.clone());
                }
                for (p, _) in &td.static_properties {
                    valid.push(p.clone());
                }
                // Only flag if type actually declares static members or enum variants
                if !valid.is_empty() && !valid.iter().any(|v| v == member) {
                    let suggestion = find_closest_match(member, valid.iter().map(|s| s.as_str()));
                    diagnostics.push(build_unknown_member_diag(
                        line_0, line_text, ns, member, suggestion,
                    ));
                }
                return;
            }
        }

        // 2. Resolved receiver expression type
        let receiver_text = expr_to_chain_str(obj);
        let receiver_ty = receiver_text
            .as_deref()
            .and_then(|chain| resolve_expression_chain_type(index, workspace, chain, line_1based))
            .or_else(|| {
                let t = index.infer_type(obj);
                if matches!(t, Ty::Any | Ty::Unknown) {
                    None
                } else {
                    Some(t)
                }
            });

        let Some(ty) = receiver_ty else {
            return;
        };

        let unwrapped = match &ty {
            Ty::Nullable(inner)
            | Ty::OptionTy(inner)
            | Ty::Ptr(inner)
            | Ty::PtrOwning(inner)
            | Ty::PtrShared(inner)
            | Ty::PtrMut(inner) => inner.as_ref(),
            other => other,
        };

        match unwrapped {
            Ty::Str => {
                if !STRING_METHODS.iter().any(|m| m.name == member) {
                    let suggestion =
                        find_closest_match(member, STRING_METHODS.iter().map(|m| m.name));
                    diagnostics.push(build_unknown_member_diag(
                        line_0, line_text, "string", member, suggestion,
                    ));
                }
            }
            Ty::Array(_) => {
                if !ARRAY_METHODS.iter().any(|m| m.name == member) {
                    let suggestion =
                        find_closest_match(member, ARRAY_METHODS.iter().map(|m| m.name));
                    diagnostics.push(build_unknown_member_diag(
                        line_0, line_text, "Array", member, suggestion,
                    ));
                }
            }
            other => {
                if let Some(tname) = extract_nominal_type_name(other) {
                    // Only check if the type is explicitly defined in current file or workspace
                    if index.get_type(&tname).is_some()
                        || workspace.find_type_definition(&tname).is_some()
                    {
                        let members = get_type_members_workspace(index, workspace, &tname);
                        if !members.is_empty() && !members.iter().any(|m| m.name == member) {
                            let suggestion =
                                find_closest_match(member, members.iter().map(|m| m.name.as_str()));
                            diagnostics.push(build_unknown_member_diag(
                                line_0, line_text, &tname, member, suggestion,
                            ));
                        }
                    }
                }
            }
        }
    });

    diagnostics
}

fn build_unknown_member_diag(
    line_0: u32,
    line_text: &str,
    owner_name: &str,
    member: &str,
    suggestion: Option<String>,
) -> Diagnostic {
    let pattern = format!(".{}", member);
    let range = if let Some(idx) = line_text.find(&pattern) {
        let start_col = line_text[..idx + 1].encode_utf16().count() as u32;
        let end_col = start_col + member.encode_utf16().count() as u32;
        Range::new(
            Position::new(line_0, start_col),
            Position::new(line_0, end_col),
        )
    } else {
        find_name_range_on_line(line_0, line_text, member)
    };

    let (message, data) = if let Some(sugg) = suggestion {
        (
            format!(
                "Property or method `{}` does not exist on `{}`. Did you mean `{}`?",
                member, owner_name, sugg
            ),
            Some(serde_json::json!({ "didYouMean": sugg })),
        )
    } else {
        (
            format!(
                "Property or method `{}` does not exist on `{}`",
                member, owner_name
            ),
            None,
        )
    };

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("unknown-member".to_string())),
        code_description: None,
        source: Some("als".to_string()),
        message,
        related_information: None,
        tags: None,
        data,
    }
}

fn expr_to_chain_str(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name.clone()),
        ExprKind::Get(obj, field) | ExprKind::OptGet(obj, field) => {
            Some(format!("{}.{}", expr_to_chain_str(obj)?, field))
        }
        ExprKind::Call(callee, _, _) => Some(format!("{}()", expr_to_chain_str(callee)?)),
        ExprKind::Index(obj, _) => Some(format!("{}[0]", expr_to_chain_str(obj)?)),
        ExprKind::NonNull(inner) | ExprKind::Grouping(inner) => expr_to_chain_str(inner),
        _ => None,
    }
}

/// Check that non-abstract classes implement all methods required by their `implements` interfaces.
fn check_missing_interface_methods(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for stmt in &index.statements {
        let cls = match &stmt.kind {
            StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => c,
            _ => continue,
        };
        if cls.is_abstract || cls.implements.is_empty() {
            continue;
        }

        // Collect all methods on this class and its inheritance chain
        let mut implemented: HashSet<String> = cls
            .methods
            .iter()
            .chain(cls.static_methods.iter())
            .map(|m| m.name.clone())
            .collect();

        // Also include inherited methods from `extends`
        let mut parent_cursor = cls.extends.clone();
        let mut depth = 0;
        while let Some(parent_name) = parent_cursor {
            if depth > 8 {
                break;
            }
            depth += 1;
            if let Some(parent_td) = index
                .get_type(&parent_name)
                .or_else(|| workspace.find_type_definition(&parent_name))
            {
                for m in &parent_td.methods {
                    implemented.insert(m.name.clone());
                }
                parent_cursor = parent_td.extends.clone();
            } else {
                break;
            }
        }

        // Also include `extend on ClassName` methods in current file
        for s in &index.statements {
            if let StmtKind::Extend(_, target, _, methods, _) = &s.kind {
                if target == &cls.name {
                    for m in methods {
                        implemented.insert(m.name.clone());
                    }
                }
            }
        }

        let line_1based = stmt.span.line;
        let line_0 = line_1based.saturating_sub(1) as u32;
        let line_text = doc.get_line(line_0 as usize).unwrap_or("");

        for iface_name in &cls.implements {
            let Some(iface_td) = index
                .get_type(iface_name)
                .or_else(|| workspace.find_type_definition(iface_name))
            else {
                continue;
            };
            if iface_td.kind != SemanticSymbolKind::Interface {
                continue;
            }

            let missing_stubs = collect_missing_interface_stubs(iface_td, &implemented);
            if !missing_stubs.is_empty() {
                let missing_names: Vec<String> = iface_td
                    .methods
                    .iter()
                    .filter(|m| !implemented.contains(&m.name))
                    .map(|m| format!("`{}`", m.name))
                    .collect();

                diagnostics.push(Diagnostic {
                    range: find_name_range_on_line(line_0, line_text, &cls.name),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String(
                        "missing-interface-methods".to_string(),
                    )),
                    code_description: None,
                    source: Some("als".to_string()),
                    message: format!(
                        "Class `{}` does not implement interface `{}`: missing method(s) {}",
                        cls.name,
                        iface_name,
                        missing_names.join(", ")
                    ),
                    related_information: None,
                    tags: None,
                    data: Some(serde_json::json!({
                        "interface": iface_name,
                        "missingMethods": missing_stubs,
                        "classLine": line_1based,
                    })),
                });
            }
        }
    }

    diagnostics
}

fn collect_missing_interface_stubs(
    iface: &TypeDefinition,
    implemented: &HashSet<String>,
) -> Vec<String> {
    let mut stubs = Vec::new();
    for m in &iface.methods {
        if !implemented.contains(&m.name) {
            let stub = m
                .signature
                .clone()
                .map(|s| s.trim_end_matches(';').trim().to_string())
                .unwrap_or_else(|| format!("fn {}()", m.name));
            stubs.push(stub);
        }
    }
    stubs
}

/// Detect reassignments to `const` or `readonly` bindings.
fn check_const_reassignments(index: &SemanticIndex, _doc: &Document) -> Vec<Diagnostic> {
    let mut immutables: HashSet<&str> = HashSet::new();
    for sym in &index.symbols {
        if sym.parent_type.is_none()
            && !sym.mutable
            && matches!(
                sym.kind,
                SemanticSymbolKind::Constant | SemanticSymbolKind::Variable
            )
        {
            immutables.insert(sym.name.as_str());
        }
    }
    if immutables.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    visit_exprs_in_stmts(&index.statements, &mut |expr| {
        let target_name = match &expr.kind {
            ExprKind::Assign(name, _) => Some(name.as_str()),
            ExprKind::AssignOp(lhs, _, _) => match &lhs.kind {
                ExprKind::Variable(name) => Some(name.as_str()),
                _ => None,
            },
            ExprKind::Update(_, _, target) => match &target.kind {
                ExprKind::Variable(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        };

        if let Some(name) = target_name {
            if immutables.contains(name) && expr.span.line > 0 {
                let line_0 = expr.span.line.saturating_sub(1) as u32;
                let col_0 = expr.span.col.saturating_sub(1) as u32;
                diagnostics.push(Diagnostic {
                    range: Range::new(
                        Position::new(line_0, col_0),
                        Position::new(line_0, col_0 + name.encode_utf16().count() as u32),
                    ),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("const-reassign".to_string())),
                    code_description: None,
                    source: Some("als".to_string()),
                    message: format!("Cannot reassign constant `{}`", name),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }
        }
    });

    diagnostics
}

/// Detect `await` expressions used inside non-async functions.
fn check_await_outside_async(index: &SemanticIndex, _doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for stmt in &index.statements {
        check_await_in_stmt(stmt, None, &mut diagnostics);
    }

    diagnostics
}

fn check_await_in_stmt(
    stmt: &Stmt,
    enclosing_fn: Option<(bool, usize)>, // (is_async, 1-based fn line)
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &stmt.kind {
        StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) => {
            let fn_ctx = Some((func.is_async, stmt.span.line));
            for s in func.body.iter() {
                check_await_in_stmt(s, fn_ctx, diagnostics);
            }
        }
        StmtKind::Class(cls, _) | StmtKind::ExportDefaultClass(cls) => {
            for m in cls.methods.iter().chain(cls.static_methods.iter()) {
                let m_line = m
                    .body
                    .first()
                    .map(|s| s.span.line.saturating_sub(1))
                    .unwrap_or(stmt.span.line);
                let fn_ctx = Some((m.is_async, m_line));
                for s in m.body.iter() {
                    check_await_in_stmt(s, fn_ctx, diagnostics);
                }
            }
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                check_await_in_stmt(s, enclosing_fn, diagnostics);
            }
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            check_await_in_expr(cond, enclosing_fn, diagnostics);
            check_await_in_stmt(then_branch, enclosing_fn, diagnostics);
            if let Some(e) = else_branch {
                check_await_in_stmt(e, enclosing_fn, diagnostics);
            }
        }
        StmtKind::While { cond, body } => {
            check_await_in_expr(cond, enclosing_fn, diagnostics);
            check_await_in_stmt(body, enclosing_fn, diagnostics);
        }
        StmtKind::ForIn { iter, body, .. } => {
            check_await_in_expr(iter, enclosing_fn, diagnostics);
            check_await_in_stmt(body, enclosing_fn, diagnostics);
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            check_await_in_stmt(try_block, enclosing_fn, diagnostics);
            check_await_in_stmt(catch_block, enclosing_fn, diagnostics);
        }
        StmtKind::Region { body, .. } | StmtKind::UnsafeBlock(body) | StmtKind::Defer(body) => {
            check_await_in_stmt(body, enclosing_fn, diagnostics);
        }
        StmtKind::Let(_, Some(init), _, _, _, _)
        | StmtKind::LetTuple(_, _, Some(init), _, _, _)
        | StmtKind::LetObject(_, Some(init), _, _, _)
        | StmtKind::ThreadLocalLet(_, Some(init), _, _)
        | StmtKind::AtomicLet(_, Some(init), _, _) => {
            check_await_in_expr(init, enclosing_fn, diagnostics);
        }
        StmtKind::ExprStmt(e) | StmtKind::Return(Some(e)) | StmtKind::Jump(e) => {
            check_await_in_expr(e, enclosing_fn, diagnostics);
        }
        _ => {}
    }
}

fn check_await_in_expr(
    expr: &Expr,
    enclosing_fn: Option<(bool, usize)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &expr.kind {
        ExprKind::Await(inner) => {
            if let Some((false, fn_line)) = enclosing_fn {
                let line_0 = expr.span.line.saturating_sub(1) as u32;
                let col_0 = expr.span.col.saturating_sub(1) as u32;
                diagnostics.push(Diagnostic {
                    range: Range::new(
                        Position::new(line_0, col_0),
                        Position::new(line_0, col_0 + 5),
                    ),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("await-in-sync".to_string())),
                    code_description: None,
                    source: Some("als".to_string()),
                    message: "`await` can only be used inside an `async` function".to_string(),
                    related_information: None,
                    tags: None,
                    data: Some(serde_json::json!({ "fnLine": fn_line })),
                });
            }
            check_await_in_expr(inner, enclosing_fn, diagnostics);
        }
        ExprKind::Fn(_, body, is_async) => {
            let nested = Some((*is_async, expr.span.line));
            for s in body.iter() {
                check_await_in_stmt(s, nested, diagnostics);
            }
        }
        ExprKind::Assign(_, rhs)
        | ExprKind::AssignTuple(_, rhs)
        | ExprKind::AssignObject(_, rhs)
        | ExprKind::Unary(_, rhs)
        | ExprKind::Grouping(rhs)
        | ExprKind::Spawn(rhs)
        | ExprKind::Throw(rhs)
        | ExprKind::Spread(rhs)
        | ExprKind::NonNull(rhs)
        | ExprKind::Try(rhs)
        | ExprKind::Update(_, _, rhs)
        | ExprKind::Format(rhs, _)
        | ExprKind::Cast(rhs, _)
        | ExprKind::Get(rhs, _)
        | ExprKind::OptGet(rhs, _) => {
            check_await_in_expr(rhs, enclosing_fn, diagnostics);
        }
        ExprKind::Binary(l, _, r)
        | ExprKind::Logical(l, _, r)
        | ExprKind::AssignOp(l, _, r)
        | ExprKind::Range(l, r, _)
        | ExprKind::Index(l, r)
        | ExprKind::Set(l, _, r) => {
            check_await_in_expr(l, enclosing_fn, diagnostics);
            check_await_in_expr(r, enclosing_fn, diagnostics);
        }
        ExprKind::Conditional(c, t, e) => {
            check_await_in_expr(c, enclosing_fn, diagnostics);
            check_await_in_expr(t, enclosing_fn, diagnostics);
            check_await_in_expr(e, enclosing_fn, diagnostics);
        }
        ExprKind::Call(callee, args, _)
        | ExprKind::OptCall(callee, args, _)
        | ExprKind::New(callee, args) => {
            check_await_in_expr(callee, enclosing_fn, diagnostics);
            for a in args {
                check_await_in_expr(a, enclosing_fn, diagnostics);
            }
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) | ExprKind::SetLiteral(items) => {
            for item in items {
                check_await_in_expr(item, enclosing_fn, diagnostics);
            }
        }
        ExprKind::Object(fields) | ExprKind::StructLiteral(_, fields) => {
            for (_, v) in fields {
                check_await_in_expr(v, enclosing_fn, diagnostics);
            }
        }
        ExprKind::Match(scrutinee, arms) => {
            check_await_in_expr(scrutinee, enclosing_fn, diagnostics);
            for (_, arm) in arms {
                check_await_in_expr(arm, enclosing_fn, diagnostics);
            }
        }
        _ => {}
    }
}

/// Warn on `unsafe { ... }` blocks when `adesh.unsafe.warnOnUsage` is enabled.
fn check_unsafe_blocks(index: &SemanticIndex, _doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    collect_unsafe_in_stmts(&index.statements, &mut diagnostics);
    diagnostics
}

fn collect_unsafe_in_stmts(stmts: &[Stmt], diagnostics: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::UnsafeBlock(body) => {
                let line_0 = stmt.span.line.saturating_sub(1) as u32;
                let col_0 = stmt.span.col.saturating_sub(1) as u32;
                diagnostics.push(Diagnostic {
                    range: Range::new(
                        Position::new(line_0, col_0),
                        Position::new(line_0, col_0 + 6),
                    ),
                    severity: Some(DiagnosticSeverity::INFORMATION),
                    code: Some(NumberOrString::String("unsafe-block".to_string())),
                    code_description: None,
                    source: Some("als".to_string()),
                    message: "Unsafe block: borrow and memory safety checks are disabled inside this block".to_string(),
                    related_information: None,
                    tags: None,
                    data: None,
                });
                collect_unsafe_in_stmts(std::slice::from_ref(body.as_ref()), diagnostics);
            }
            StmtKind::Block(inner) => collect_unsafe_in_stmts(inner, diagnostics),
            StmtKind::Function(f, _) | StmtKind::ExportDefaultFunction(f) => {
                collect_unsafe_in_stmts(&f.body, diagnostics);
            }
            StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
                for m in c.methods.iter().chain(c.static_methods.iter()) {
                    collect_unsafe_in_stmts(&m.body, diagnostics);
                }
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_unsafe_in_stmts(std::slice::from_ref(then_branch.as_ref()), diagnostics);
                if let Some(e) = else_branch {
                    collect_unsafe_in_stmts(std::slice::from_ref(e.as_ref()), diagnostics);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::ForIn { body, .. }
            | StmtKind::Region { body, .. }
            | StmtKind::Defer(body) => {
                collect_unsafe_in_stmts(std::slice::from_ref(body.as_ref()), diagnostics);
            }
            StmtKind::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                collect_unsafe_in_stmts(std::slice::from_ref(try_block.as_ref()), diagnostics);
                collect_unsafe_in_stmts(std::slice::from_ref(catch_block.as_ref()), diagnostics);
            }
            _ => {}
        }
    }
}

/// Strict type assistance: report missing explicit type annotations when enabled.
fn check_strict_type_annotations(index: &SemanticIndex, doc: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &index.statements {
        collect_strict_in_stmt(stmt, doc, &mut diagnostics);
    }
    diagnostics
}

fn collect_strict_in_stmt(stmt: &Stmt, doc: &Document, diagnostics: &mut Vec<Diagnostic>) {
    let line_0 = stmt.span.line.saturating_sub(1) as u32;
    let line_text = doc.get_line(line_0 as usize).unwrap_or("");
    match &stmt.kind {
        StmtKind::Let(name, Some(_), None, _, _, _) => {
            diagnostics.push(Diagnostic {
                range: find_name_range_on_line(line_0, line_text, name),
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String(
                    "missing-type-annotation".to_string(),
                )),
                code_description: None,
                source: Some("als".to_string()),
                message: format!(
                    "Strict mode: missing explicit type annotation on `{}`",
                    name
                ),
                related_information: None,
                tags: None,
                data: None,
            });
        }
        StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) => {
            check_strict_fn(func, line_0, line_text, diagnostics);
            for s in func.body.iter() {
                collect_strict_in_stmt(s, doc, diagnostics);
            }
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                collect_strict_in_stmt(s, doc, diagnostics);
            }
        }
        _ => {}
    }
}

fn check_strict_fn(
    func: &Function,
    line_0: u32,
    line_text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (pname, _, ptype) in &func.params {
        if ptype.is_none() && !pname.starts_with('_') {
            diagnostics.push(Diagnostic {
                range: find_name_range_on_line(line_0, line_text, pname),
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String(
                    "missing-type-annotation".to_string(),
                )),
                code_description: None,
                source: Some("als".to_string()),
                message: format!(
                    "Strict mode: parameter `{}` of `{}` is missing a type annotation",
                    pname, func.name
                ),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
    if func.ret_type.is_none() && func.name != "main" && !func.is_test {
        diagnostics.push(Diagnostic {
            range: find_name_range_on_line(line_0, line_text, &func.name),
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String(
                "missing-type-annotation".to_string(),
            )),
            code_description: None,
            source: Some("als".to_string()),
            message: format!(
                "Strict mode: function `{}` is missing an explicit return type",
                func.name
            ),
            related_information: None,
            tags: None,
            data: None,
        });
    }
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

        let occurrences = count_name_occurrences(&result.tokens, &sym.name);

        if occurrences <= 1 && sym.line > 0 {
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
    check_unreachable_in_stmts(&result.statements, doc, &mut diagnostics);
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

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Url;

    fn run_diags(src: &str, cfg: AlsConfig) -> Vec<Diagnostic> {
        let uri = Url::parse("file:///diag_test.adesh").unwrap();
        let doc = Document::new(uri.clone(), src.to_string(), 1);
        let mut ws = WorkspaceIndex::new();
        ws.index_document(&uri, src);
        let idx = ws.get_index(&uri).unwrap();
        compute_diagnostics_semantic_with_config(idx, &ws, &doc, &cfg)
    }

    #[test]
    fn test_arity_mismatch_diagnostic() {
        let src = "fn add(a: i32, b: i32): i32 { return a + b; }\nlet r = add(1);\nprint(r);\n";
        let diags = run_diags(src, AlsConfig::default());
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("expects 2 argument")),
            "diags: {:?}",
            diags
        );
    }

    #[test]
    fn test_unknown_member_with_did_you_mean() {
        let src = "class User {\n    name: string;\n}\nlet u = User(\"Ajay\");\nprint(u.nam);\n";
        let diags = run_diags(src, AlsConfig::default());
        assert!(
            diags.iter().any(|d| d.message.contains("does not exist")
                && d.message.contains("Did you mean `name`")),
            "diags: {:?}",
            diags
        );
    }

    #[test]
    fn test_missing_interface_methods_diagnostic() {
        let src = "interface Drawable {\n    fn draw();\n    fn resize(w: f64, h: f64);\n}\nclass Box implements Drawable {\n    fn draw() { print(\"ok\"); }\n}\n";
        let diags = run_diags(src, AlsConfig::default());
        let missing = diags
            .iter()
            .find(|d| d.message.contains("missing method"))
            .expect("missing interface method diagnostic");
        assert!(
            missing.message.contains("resize"),
            "msg: {}",
            missing.message
        );
        assert!(missing.data.is_some());
    }

    #[test]
    fn test_const_reassignment_and_await_in_sync() {
        let src =
            "const X = 10;\nX = 20;\nfn load() {\n    let d = await fetch();\n    print(d);\n}\n";
        let diags = run_diags(src, AlsConfig::default());
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("Cannot reassign constant `X`")),
            "diags: {:?}",
            diags
        );
        assert!(
            diags.iter().any(|d| d
                .message
                .contains("`await` can only be used inside an `async`")),
            "diags: {:?}",
            diags
        );
    }

    #[test]
    fn test_strict_mode_missing_type_annotations() {
        let src = "let x = 42;\nprint(x);\n";
        let mut cfg = AlsConfig::default();
        cfg.type_assistance.strict_mode = true;
        let diags = run_diags(src, cfg);
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("missing explicit type annotation")),
            "diags: {:?}",
            diags
        );
    }
}
