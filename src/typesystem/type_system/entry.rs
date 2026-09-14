//! Entry point for type checking and signature collection.

use super::resolution::{resolve_type_name, type_from_name};
use super::statement_checking::check_stmt_types;
use crate::parsing::ast::*;
use crate::parsing::error::{ErrorKind, LangError};
use crate::parsing::lexer::Lexer;
use crate::parsing::parser::Parser;
use crate::typesystem::checker::Ty;
use crate::utils::collections::FastMap;

pub fn check_module(src: &str) -> Result<(), LangError> {
    check_module_in(src, None)
}

/// Type-check a module, attaching `file` to diagnostics for clickable locations.
pub fn check_module_in(src: &str, file: Option<&str>) -> Result<(), LangError> {
    // Lex & parse
    let mut lx = Lexer::new(src);
    if let Some(f) = file {
        lx.set_file(f);
    }
    let toks = lx.tokenize()?;
    let mut p = Parser::new(toks, file.map(|f| f.to_string()));
    let prog = p.parse_program()?;

    // collect signatures: map name -> parameter annotation strings (Option<String> per param)
    let mut fns: FastMap<String, Vec<Option<String>>> = FastMap::default();
    // function generic type parameter names: name -> Vec<type_param_names>
    let mut fns_type_params: FastMap<String, Vec<String>> = FastMap::default();
    // function/method return type annotations: name -> Option<String>
    let mut fns_ret_types: FastMap<String, Option<String>> = FastMap::default();

    for s in &prog {
        collect_sigs(s, &mut fns, &mut fns_type_params, &mut fns_ret_types);
    }

    // collect type aliases (simple record shapes)
    let mut aliases: FastMap<String, Ty> = FastMap::default();

    // collect class, interface, and enum names as Ty::Any first
    for s in &prog {
        collect_custom_types_names(s, &mut aliases);
    }

    // generic aliases (keep AST so we can instantiate with concrete type args)
    let mut generic_aliases: FastMap<String, TypeAliasDecl> = FastMap::default();
    for s in &prog {
        if let StmtKind::TypeAlias(ta, _exp) = &s.kind {
            if ta.type_params.is_empty() {
                let mut req: Vec<(String, Ty)> = Vec::new();
                let mut opt: Vec<(String, Ty)> = Vec::new();
                for (fname, optional, fty) in &ta.fields {
                    if let Some(ft) = type_from_name(fty) {
                        if *optional {
                            opt.push((fname.clone(), ft));
                        } else {
                            req.push((fname.clone(), ft));
                        }
                    } else {
                        return Err(LangError::new(
                            ErrorKind::Type,
                            format!("unknown type '{}' in alias {}", fty, ta.name),
                            s.span.line,
                            s.span.col,
                            s.span.line_text.clone(),
                        )
                        .with_file_opt(file.map(|f| f.to_string()))
                        .with_auto_hints());
                    }
                }
                aliases.insert(
                    ta.name.clone(),
                    Ty::Record {
                        required: req,
                        optional: opt,
                    },
                );
            } else {
                generic_aliases.insert(ta.name.clone(), ta.clone());
            }
        }
    }

    // collect struct declarations (including nested in functions/blocks) as closed record types
    fn collect_structs_recursive<'a>(stmts: &'a [Stmt], out: &mut Vec<(&'a StructDecl, &'a Span)>) {
        for s in stmts {
            match &s.kind {
                StmtKind::Struct(sd, _) => out.push((sd, &s.span)),
                StmtKind::Function(f, _) => collect_structs_recursive(&f.body, out),
                StmtKind::Block(b) => collect_structs_recursive(b, out),
                StmtKind::UnsafeBlock(body) | StmtKind::Region { body, .. } => {
                    collect_structs_recursive(std::slice::from_ref(body), out);
                }
                StmtKind::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_structs_recursive(std::slice::from_ref(then_branch), out);
                    if let Some(eb) = else_branch {
                        collect_structs_recursive(std::slice::from_ref(eb), out);
                    }
                }
                StmtKind::TryCatch {
                    try_block,
                    catch_block,
                    ..
                } => {
                    collect_structs_recursive(std::slice::from_ref(try_block), out);
                    collect_structs_recursive(std::slice::from_ref(catch_block), out);
                }
                StmtKind::While { body, .. } | StmtKind::ForIn { body, .. } => {
                    collect_structs_recursive(std::slice::from_ref(body), out);
                }
                _ => {}
            }
        }
    }

    let empty_type_params: FastMap<String, Ty> = FastMap::default();
    let mut all_structs = Vec::new();
    collect_structs_recursive(&prog, &mut all_structs);
    for (sd, span) in all_structs {
        let mut req: Vec<(String, Ty)> = Vec::new();
        for (fname, fty) in &sd.fields {
            let resolved =
                resolve_type_name(fty, &aliases, &generic_aliases, &empty_type_params)
                    .or_else(|| type_from_name(fty));
            if let Some(ft) = resolved {
                req.push((fname.clone(), ft));
            } else {
                return Err(LangError::new(
                    ErrorKind::Type,
                    format!("unknown type '{}' in struct {}", fty, sd.name),
                    span.line,
                    span.col,
                    span.line_text.clone(),
                )
                .with_file_opt(file.map(|f| f.to_string()))
                .with_auto_hints());
            }
        }
        aliases.insert(
            sd.name.clone(),
            Ty::Record {
                required: req,
                optional: Vec::new(),
            },
        );
    }

    let mut env: Vec<FastMap<String, Ty>> = vec![FastMap::default()];
    for s in &prog {
        check_stmt_types(
            s,
            &mut env,
            &fns,
            &fns_type_params,
            &fns_ret_types,
            false,
            None,
            &aliases,
            &generic_aliases,
            &empty_type_params,
        )
        .map_err(|msg| {
            let (clean_msg, line, col, line_text) = if let Some(idx) = msg.find("@@span:") {
                let (m, rest) = msg.split_at(idx);
                let span_info = &rest["@@span:".len()..];
                let parts: Vec<&str> = span_info.splitn(3, ':').collect();
                let l = parts.get(0).and_then(|x| x.parse().ok()).unwrap_or(s.span.line);
                let c = parts.get(1).and_then(|x| x.parse().ok()).unwrap_or(s.span.col);
                let text = parts.get(2).map(|x| x.to_string()).unwrap_or_else(|| s.span.line_text.clone());
                (m.to_string(), l, c, text)
            } else {
                (msg.clone(), s.span.line, s.span.col, s.span.line_text.clone())
            };
            let code = if clean_msg.contains("await is only valid inside async functions") {
                "E_ASYNC_CONTEXT"
            } else if clean_msg.contains("cannot await value of type") {
                "E_ASYNC_AWAIT_TYPE"
            } else {
                "E0308"
            };
            LangError::new(
                ErrorKind::Type,
                enhance_type_message(&clean_msg),
                line,
                col,
                line_text,
            )
            .with_file_opt(file.map(|f| f.to_string()))
            .with_auto_hints()
            .with_code(code)
        })?;
    }

    Ok(())
}

/// Make type-checker messages more descriptive for end users
fn enhance_type_message(msg: &str) -> String {
    let lower = msg.to_lowercase();
    if lower.contains("expected") && lower.contains("found") {
        if lower.contains("cannot await value of type") {
            return msg.to_string();
        }
        if lower.contains("return type mismatch") {
            format!(
                "{}\n  = hint: if multiple types can be returned, use union syntax (e.g. `[int] | null` or `T1 | T2`)",
                msg
            )
        } else if lower.contains("type mismatch for") {
            format!(
                "{}\n  = hint: containers like `set`, `dict`, `map`, `array` can be initialized with constructors `Set()`, `Dict()`, `Array()` or default declaration `let sa: set;`",
                msg
            )
        } else {
            format!("type mismatch: {}", msg)
        }
    } else if lower.contains("unknown type") {
        format!(
            "{} — check the spelling, or declare the type (class/struct/type alias) first",
            msg
        )
    } else if lower.contains("outside of loop") {
        format!(
            "{} — `break`/`continue`/`jump` are only valid inside loops",
            msg
        )
    } else {
        msg.to_string()
    }
}

pub(crate) fn collect_sigs(
    s: &Stmt,
    fns: &mut FastMap<String, Vec<Option<String>>>,
    fns_type_params: &mut FastMap<String, Vec<String>>,
    fns_ret_types: &mut FastMap<String, Option<String>>,
) {
    match &s.kind {
        StmtKind::Function(f, _export) => {
            let mut v: Vec<Option<String>> = Vec::new();
            // store raw annotation strings; resolution happens at call/check time
            for p in &f.params {
                if let Some(tn) = &p.2 {
                    v.push(Some(tn.clone()));
                } else {
                    v.push(None);
                }
            }
            fns.insert(f.name.clone(), v);
            if !f.type_params.is_empty() {
                fns_type_params.insert(f.name.clone(), f.type_params.clone());
            }
            fns_ret_types.insert(f.name.clone(), f.ret_type.clone());
        }
        StmtKind::Struct(_s, _export) => {}
        StmtKind::Interface(_i, _export) => {}
        StmtKind::Enum(_e, _export) => {}
        StmtKind::Class(c, _export) => {
            for m in &c.methods {
                let key = format!("{}::{}", c.name, m.name);
                let mut v: Vec<Option<String>> = Vec::new();
                for p in &m.params {
                    if let Some(tn) = &p.2 {
                        v.push(Some(tn.clone()));
                    } else {
                        v.push(None);
                    }
                }
                fns.insert(key.clone(), v);
                if !m.type_params.is_empty() {
                    fns_type_params.insert(key.clone(), m.type_params.clone());
                }
                fns_ret_types.insert(key.clone(), m.ret_type.clone());
            }
        }
        StmtKind::Extend(_name, target, methods, _export) => {
            for m in methods {
                let key = format!("{}::{}", target, m.name);
                let mut v: Vec<Option<String>> = Vec::new();
                for p in &m.params {
                    if let Some(tn) = &p.2 {
                        v.push(Some(tn.clone()));
                    } else {
                        v.push(None);
                    }
                }
                fns.insert(key.clone(), v);
                if !m.type_params.is_empty() {
                    fns_type_params.insert(key.clone(), m.type_params.clone());
                }
                fns_ret_types.insert(key.clone(), m.ret_type.clone());
            }
        }
        StmtKind::Block(bs) => {
            for b in bs {
                collect_sigs(b, fns, fns_type_params, fns_ret_types);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_sigs(then_branch, fns, fns_type_params, fns_ret_types);
            if let Some(e) = else_branch {
                collect_sigs(e, fns, fns_type_params, fns_ret_types);
            }
        }
        StmtKind::While { body, .. } => collect_sigs(body, fns, fns_type_params, fns_ret_types),
        StmtKind::ForIn { body, .. } => collect_sigs(body, fns, fns_type_params, fns_ret_types),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            collect_sigs(try_block, fns, fns_type_params, fns_ret_types);
            collect_sigs(catch_block, fns, fns_type_params, fns_ret_types);
        }
        _ => {}
    }
}

fn collect_custom_types_names(s: &Stmt, aliases: &mut FastMap<String, Ty>) {
    match &s.kind {
        StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
            aliases.insert(c.name.clone(), Ty::Class(c.name.clone()));
        }
        StmtKind::Interface(i, _) => {
            aliases.insert(i.name.clone(), Ty::Interface(i.name.clone()));
        }
        StmtKind::Enum(e, _) => {
            aliases.insert(e.name.clone(), Ty::Enum(e.name.clone()));
        }
        StmtKind::Block(bs) => {
            for b in bs {
                collect_custom_types_names(b, aliases);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_custom_types_names(then_branch, aliases);
            if let Some(e) = else_branch {
                collect_custom_types_names(e, aliases);
            }
        }
        StmtKind::While { body, .. } => {
            collect_custom_types_names(body, aliases);
        }
        StmtKind::ForIn { body, .. } => {
            collect_custom_types_names(body, aliases);
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            collect_custom_types_names(try_block, aliases);
            collect_custom_types_names(catch_block, aliases);
        }
        StmtKind::Region { body, .. } => {
            collect_custom_types_names(body, aliases);
        }
        StmtKind::UnsafeBlock(body) => {
            collect_custom_types_names(body, aliases);
        }
        StmtKind::Defer(body) => {
            collect_custom_types_names(body, aliases);
        }
        _ => {}
    }
}
