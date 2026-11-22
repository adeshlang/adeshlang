//! Signature Help Provider
//!
//! Provides rich, context-aware parameter hints for:
//! - User-defined functions (in current file and across workspace)
//! - Class and struct constructors (`User(...)`, `new User(...)`)
//! - Instance and static methods on resolved types (`user.save(...)`, `this.foo(...)`)
//! - Built-in functions (`print`, `range`, `map`, `assert`, etc.)
//! - Standard namespaces (`Math.pow`, `time.nowMs`, `JSON.parse`, `IO.readFile`, etc.)
//! - Built-in String and Array methods (`s.replace(...)`, `arr.slice(...)`)
//!
//! Properly tracks nested calls, brackets, strings, and active parameter indices.

use crate::document::Document;
use crate::type_utils::{
    extract_doc_comment, extract_nominal_type_name, extract_trailing_receiver, find_builtin_fn,
    find_namespace_members, get_type_members_workspace, resolve_expression_chain_type,
    ARRAY_METHODS, STRING_METHODS,
};
use crate::workspace::WorkspaceIndex;
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind, SymbolEntry, TypeDefinition};
use adeshlang::typesystem::checker::Ty;
use lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, SignatureHelp,
    SignatureInformation,
};

/// Compute signature help at a 0-based LSP position.
pub fn get_signature_help(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    doc: &Document,
    line: u32,
    col: u32,
) -> Option<SignatureHelp> {
    let offset = doc.position_to_offset(line, col)?;
    let prefix = &doc.content[..offset.min(doc.content.len())];

    let (callee_expr, active_param) = find_enclosing_call(prefix)?;
    if callee_expr.is_empty() {
        return None;
    }

    let line_1based = (line + 1) as usize;
    let mut signatures = Vec::new();

    // 1. Member call: `receiver.method(`
    if let Some((receiver, method_name)) = split_last_member(&callee_expr) {
        // Built-in namespaces (`Math.pow`, `time.nowMs`, `IO.readFile`, ...)
        if let Some(members) = find_namespace_members(receiver) {
            if let Some(spec) = members
                .iter()
                .find(|m| m.name == method_name && !m.is_const)
            {
                signatures.push(build_spec_signature(
                    spec.signature,
                    spec.doc,
                    spec.params,
                    active_param,
                ));
            }
        }

        // Resolved type member/method call
        if signatures.is_empty() {
            if let Some(ty) = resolve_expression_chain_type(index, workspace, receiver, line_1based)
            {
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
                        if let Some(spec) = STRING_METHODS
                            .iter()
                            .find(|m| m.name == method_name && !m.is_const)
                        {
                            signatures.push(build_spec_signature(
                                spec.signature,
                                spec.doc,
                                spec.params,
                                active_param,
                            ));
                        }
                    }
                    Ty::Array(_) => {
                        if let Some(spec) = ARRAY_METHODS
                            .iter()
                            .find(|m| m.name == method_name && !m.is_const)
                        {
                            signatures.push(build_spec_signature(
                                spec.signature,
                                spec.doc,
                                spec.params,
                                active_param,
                            ));
                        }
                    }
                    _ => {
                        if let Some(type_name) = extract_nominal_type_name(unwrapped) {
                            let members = get_type_members_workspace(index, workspace, &type_name);
                            for m in members {
                                if m.name == method_name {
                                    let sig_label = m.signature.clone().unwrap_or_else(|| {
                                        format_fn_signature(
                                            &m.name,
                                            &m.params,
                                            m.return_type.as_deref(),
                                            m.is_async,
                                        )
                                    });
                                    let params = build_param_infos(&m.params);
                                    let doc_text = workspace
                                        .find_type_index(&type_name)
                                        .and_then(|idx| extract_doc_comment(&idx.source, m.line))
                                        .or_else(|| Some(format!("Method on `{}`", type_name)));
                                    signatures.push(make_sig_info(
                                        sig_label,
                                        doc_text,
                                        params,
                                        active_param,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        // 2. Direct call: `foo(`, `User(`, `new User(`
        let clean_name = callee_expr
            .strip_prefix("new ")
            .unwrap_or(&callee_expr)
            .trim();

        // Check constructor for class / struct in current file or workspace
        if let Some(td) = index
            .get_type(clean_name)
            .or_else(|| workspace.find_type_definition(clean_name))
        {
            if let Some(sig) = constructor_signature(td, active_param) {
                signatures.push(sig);
            }
        }

        // Check user function in current file
        if let Some(sym) = index.find_declaration(clean_name) {
            if matches!(
                sym.kind,
                SemanticSymbolKind::Function | SemanticSymbolKind::Method
            ) {
                let doc_text = sym
                    .documentation
                    .clone()
                    .or_else(|| extract_doc_comment(&index.source, sym.line));
                signatures.push(symbol_signature(sym, doc_text, active_param));
            }
        }

        // Check user function across workspace
        if signatures.is_empty() {
            if let Some((uri, sym)) = workspace.find_declaration_workspace(clean_name) {
                if matches!(
                    sym.kind,
                    SemanticSymbolKind::Function | SemanticSymbolKind::Method
                ) {
                    let doc_text = sym.documentation.clone().or_else(|| {
                        workspace
                            .get_index(uri)
                            .and_then(|idx| extract_doc_comment(&idx.source, sym.line))
                    });
                    signatures.push(symbol_signature(sym, doc_text, active_param));
                }
            }
        }

        // Check built-in functions
        if let Some(spec) = find_builtin_fn(clean_name) {
            signatures.push(build_spec_signature(
                spec.signature,
                spec.doc,
                spec.params,
                active_param,
            ));
        }
    }

    if signatures.is_empty() {
        return None;
    }

    let clamped_active = signatures[0]
        .parameters
        .as_ref()
        .map(|p| {
            if p.is_empty() {
                0
            } else {
                active_param.min((p.len() - 1) as u32)
            }
        })
        .unwrap_or(0);

    Some(SignatureHelp {
        signatures,
        active_signature: Some(0),
        active_parameter: Some(clamped_active),
    })
}

/// Scan backwards from the cursor to find the innermost unclosed `(` and count top-level commas.
/// Returns `(callee_expression, active_parameter_index)`.
pub fn find_enclosing_call(prefix: &str) -> Option<(String, u32)> {
    let bytes = prefix.as_bytes();
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut commas = 0u32;

    // Track string literals in forward pass so backward scanning knows what's inside a string
    let in_string = compute_string_mask(prefix);

    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if in_string[i] {
            continue;
        }
        match bytes[i] {
            b')' => paren_depth += 1,
            b'(' => {
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                    let before_paren = prefix[..i].trim_end();
                    let callee = extract_callee_before_paren(before_paren);
                    if callee.is_empty()
                        || matches!(
                            callee.as_str(),
                            "if" | "elif" | "while" | "for" | "match" | "catch" | "return"
                        )
                    {
                        return None;
                    }
                    return Some((callee, commas));
                }
                paren_depth = (paren_depth - 1).max(0);
            }
            b']' => bracket_depth += 1,
            b'[' => bracket_depth = (bracket_depth - 1).max(0),
            b'}' => brace_depth += 1,
            b'{' => {
                if brace_depth == 0 && paren_depth == 0 {
                    // Hit an enclosing block before any '('
                    return None;
                }
                brace_depth = (brace_depth - 1).max(0);
            }
            b',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                commas += 1;
            }
            b';' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            _ => {}
        }
    }
    None
}

fn compute_string_mask(s: &str) -> Vec<bool> {
    let bytes = s.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut i = 0;
    let mut quote: Option<u8> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while i < bytes.len() {
        let b = bytes[i];
        if in_line_comment {
            mask[i] = true;
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            mask[i] = true;
            if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                mask[i + 1] = true;
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            mask[i] = true;
            if b == b'\\' && i + 1 < bytes.len() {
                mask[i + 1] = true;
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                mask[i] = true;
                i += 1;
                continue;
            }
            if bytes[i + 1] == b'*' {
                in_block_comment = true;
                mask[i] = true;
                i += 1;
                continue;
            }
        }
        if b == b'"' || b == b'\'' || b == b'`' {
            quote = Some(b);
            mask[i] = true;
        }
        i += 1;
    }
    mask
}

fn extract_callee_before_paren(before_paren: &str) -> String {
    // Strip generic arguments on callee: `foo<i32>` -> `foo`
    let trimmed = before_paren.trim_end();
    let without_generics = if trimmed.ends_with('>') {
        if let Some(lt) = trimmed.rfind('<') {
            trimmed[..lt].trim_end()
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let receiver = extract_trailing_receiver(without_generics);
    // Check if preceded by `new `
    if let Some(idx) = without_generics.rfind(&receiver) {
        let before = without_generics[..idx].trim_end();
        if before.ends_with("new") {
            let before_new = before.strip_suffix("new").unwrap_or("");
            if before_new.is_empty()
                || before_new
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_alphanumeric() && c != '_')
            {
                return format!("new {}", receiver);
            }
        }
    }
    receiver
}

fn split_last_member(callee: &str) -> Option<(&str, &str)> {
    let dot_idx = callee.rfind('.')?;
    let member = callee[dot_idx + 1..].trim();
    let receiver = callee[..dot_idx].trim_end_matches('?').trim();
    if receiver.is_empty() || member.is_empty() {
        return None;
    }
    Some((receiver, member))
}

fn constructor_signature(td: &TypeDefinition, active_param: u32) -> Option<SignatureInformation> {
    // Prefer explicit `constructor` / `init` method if present
    if let Some(ctor) = td
        .methods
        .iter()
        .find(|m| m.name == "constructor" || m.name == "init")
    {
        let label = format!(
            "{}({}): {}",
            td.name,
            format_param_list(&ctor.params),
            td.name
        );
        let params = build_param_infos(&ctor.params);
        return Some(make_sig_info(
            label,
            Some(format!("Construct a new `{}` instance.", td.name)),
            params,
            active_param,
        ));
    }

    // Fall back to struct/class fields as positional constructor arguments
    if !td.fields.is_empty() {
        let pairs: Vec<(String, Option<String>)> = td
            .fields
            .iter()
            .map(|(n, ty, _)| (n.clone(), Some(ty.clone())))
            .collect();
        let label = format!("{}({}): {}", td.name, format_param_list(&pairs), td.name);
        let params = build_param_infos(&pairs);
        return Some(make_sig_info(
            label,
            Some(format!("Construct `{}`.", td.name)),
            params,
            active_param,
        ));
    }

    None
}

fn symbol_signature(
    sym: &SymbolEntry,
    doc: Option<String>,
    active_param: u32,
) -> SignatureInformation {
    let label = sym.signature.clone().unwrap_or_else(|| {
        format_fn_signature(
            &sym.name,
            &sym.params,
            sym.return_type.as_deref(),
            sym.is_async,
        )
    });
    let params = build_param_infos(&sym.params);
    make_sig_info(label, doc, params, active_param)
}

fn build_spec_signature(
    label: &str,
    doc: &str,
    params: &[(&str, &str)],
    active_param: u32,
) -> SignatureInformation {
    let param_infos: Vec<ParameterInformation> = params
        .iter()
        .map(|(p_label, p_doc)| ParameterInformation {
            label: ParameterLabel::Simple((*p_label).to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: (*p_doc).to_string(),
            })),
        })
        .collect();
    make_sig_info(
        label.to_string(),
        Some(doc.to_string()),
        param_infos,
        active_param,
    )
}

fn make_sig_info(
    label: String,
    doc: Option<String>,
    params: Vec<ParameterInformation>,
    active_param: u32,
) -> SignatureInformation {
    let active = if params.is_empty() {
        None
    } else {
        Some(active_param.min((params.len() - 1) as u32))
    };
    SignatureInformation {
        label,
        documentation: doc.map(|d| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: d,
            })
        }),
        parameters: Some(params),
        active_parameter: active,
    }
}

fn build_param_infos(params: &[(String, Option<String>)]) -> Vec<ParameterInformation> {
    params
        .iter()
        .map(|(name, ty)| {
            let label = match ty {
                Some(t) => format!("{}: {}", name, t),
                None => name.clone(),
            };
            ParameterInformation {
                label: ParameterLabel::Simple(label),
                documentation: None,
            }
        })
        .collect()
}

fn format_param_list(params: &[(String, Option<String>)]) -> String {
    params
        .iter()
        .map(|(n, t)| match t {
            Some(ty) => format!("{}: {}", n, ty),
            None => n.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_fn_signature(
    name: &str,
    params: &[(String, Option<String>)],
    ret: Option<&str>,
    is_async: bool,
) -> String {
    let prefix = if is_async { "async fn " } else { "fn " };
    let ret_part = ret.map(|r| format!(": {}", r)).unwrap_or_default();
    format!(
        "{}{}({}){}",
        prefix,
        name,
        format_param_list(params),
        ret_part
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Url;

    #[test]
    fn test_signature_help_nested_user_function() {
        let src = "fn add(a: i32, b: i32): i32 { return a + b; }\nfn mul(x: i32, y: i32): i32 { return x * y; }\nlet r = add(mul(1, 2), );\n";
        let uri = Url::parse("file:///test.adesh").unwrap();
        let doc = Document::new(uri.clone(), src.to_string(), 1);
        let mut ws = WorkspaceIndex::new();
        ws.index_document(&uri, src);
        let idx = ws.get_index(&uri).unwrap();

        // Cursor right after `add(mul(1, 2), ` on line 2 col 23
        let sig = get_signature_help(idx, &ws, &doc, 2, 23).expect("signature help for add");
        assert!(
            sig.signatures[0].label.contains("add"),
            "label: {}",
            sig.signatures[0].label
        );
        assert_eq!(sig.active_parameter, Some(1));
    }

    #[test]
    fn test_signature_help_constructor_and_method() {
        let src = "class User {\n    id: i32;\n    name: string;\n    fn rename(new_name: string, notify: bool): void {}\n}\nlet u = User(1, );\nu.rename(\"a\", );\n";
        let uri = Url::parse("file:///user.adesh").unwrap();
        let doc = Document::new(uri.clone(), src.to_string(), 1);
        let mut ws = WorkspaceIndex::new();
        ws.index_document(&uri, src);
        let idx = ws.get_index(&uri).unwrap();

        let ctor = get_signature_help(idx, &ws, &doc, 5, 16).expect("constructor signature");
        assert!(ctor.signatures[0].label.contains("User("));
        assert_eq!(ctor.active_parameter, Some(1));

        let meth = get_signature_help(idx, &ws, &doc, 6, 14).expect("method signature");
        assert!(meth.signatures[0].label.contains("rename"));
        assert_eq!(meth.active_parameter, Some(1));
    }
}
