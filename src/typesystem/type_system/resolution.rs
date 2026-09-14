//! Type name resolution and parsing utilities.

use crate::parsing::ast::TypeAliasDecl;
use crate::typesystem::checker::Ty;
use crate::utils::collections::{FastMap, FastSet};
use std::cell::RefCell;

thread_local! {
    static ACTIVE_EXPANSIONS: RefCell<FastSet<String>> = RefCell::new(FastSet::default());
}

pub fn resolve_type(
    ty: Ty,
    aliases: &FastMap<String, Ty>,
    generic_aliases: &FastMap<String, TypeAliasDecl>,
    type_params: &FastMap<String, Ty>,
) -> Ty {
    match ty {
        Ty::GenericParam(ref name) => {
            if let Some(resolved) = type_params.get(name) {
                resolved.clone()
            } else if let Some(resolved) = aliases.get(name) {
                resolved.clone()
            } else {
                ty
            }
        }
        Ty::GenericInstance { name, args } => {
            let resolved_args: Vec<Ty> = args
                .into_iter()
                .map(|arg| resolve_type(arg, aliases, generic_aliases, type_params))
                .collect();
            if (name.eq_ignore_ascii_case("Array") || name.eq_ignore_ascii_case("List")) && resolved_args.len() == 1 {
                return Ty::Array(Box::new(resolved_args[0].clone()));
            }
            if (name.eq_ignore_ascii_case("Map") || name.eq_ignore_ascii_case("Dict")) && resolved_args.len() == 2 {
                return Ty::Map(
                    Box::new(resolved_args[0].clone()),
                    Box::new(resolved_args[1].clone()),
                );
            }
            if name.eq_ignore_ascii_case("Set") && resolved_args.len() == 1 {
                return Ty::GenericInstance {
                    name: "Set".to_string(),
                    args: resolved_args,
                };
            }
            if name == "Option" && resolved_args.len() == 1 {
                return Ty::OptionTy(Box::new(resolved_args[0].clone()));
            }
            if name == "Result" && resolved_args.len() == 2 {
                return Ty::ResultTy(
                    Box::new(resolved_args[0].clone()),
                    Box::new(resolved_args[1].clone()),
                );
            }
            if let Some(ta) = generic_aliases.get(&name) {
                if ta.type_params.len() == resolved_args.len() {
                    let has_cycle = ACTIVE_EXPANSIONS.with(|active| {
                        let mut set = active.borrow_mut();
                        if set.contains(&name) {
                            true
                        } else {
                            set.insert(name.clone());
                            false
                        }
                    });
                    if has_cycle {
                        return Ty::Any;
                    }
                    let mut subs: FastMap<String, Ty> = FastMap::default();
                    for (i, pname) in ta.type_params.iter().enumerate() {
                        subs.insert(pname.clone(), resolved_args[i].clone());
                    }
                    let mut combined_type_params = type_params.clone();
                    for (k, v) in &subs {
                        combined_type_params.insert(k.clone(), v.clone());
                    }
                    let mut req: Vec<(String, Ty)> = Vec::new();
                    let mut opt: Vec<(String, Ty)> = Vec::new();
                    for (fname, optional, fty) in &ta.fields {
                        let trimmed = fty.trim();
                        if let Some(subt) = subs.get(trimmed) {
                            if *optional {
                                opt.push((fname.clone(), subt.clone()));
                            } else {
                                req.push((fname.clone(), subt.clone()));
                            }
                        } else if let Some(rt) = resolve_type_name(
                            trimmed,
                            aliases,
                            generic_aliases,
                            &combined_type_params,
                        ) {
                            if *optional {
                                opt.push((fname.clone(), rt));
                            } else {
                                req.push((fname.clone(), rt));
                            }
                        }
                    }
                    ACTIVE_EXPANSIONS.with(|active| {
                        active.borrow_mut().remove(&name);
                    });
                    return Ty::Record {
                        required: req,
                        optional: opt,
                    };
                }
            }
            Ty::GenericInstance {
                name,
                args: resolved_args,
            }
        }
        Ty::Array(inner) => Ty::Array(Box::new(resolve_type(
            *inner,
            aliases,
            generic_aliases,
            type_params,
        ))),
        Ty::Ptr(inner) => Ty::Ptr(Box::new(resolve_type(
            *inner,
            aliases,
            generic_aliases,
            type_params,
        ))),
        Ty::PtrOwning(inner) => Ty::PtrOwning(Box::new(resolve_type(
            *inner,
            aliases,
            generic_aliases,
            type_params,
        ))),
        Ty::PtrShared(inner) => Ty::PtrShared(Box::new(resolve_type(
            *inner,
            aliases,
            generic_aliases,
            type_params,
        ))),
        Ty::PtrMut(inner) => Ty::PtrMut(Box::new(resolve_type(
            *inner,
            aliases,
            generic_aliases,
            type_params,
        ))),
        Ty::Nullable(inner) => Ty::Nullable(Box::new(resolve_type(
            *inner,
            aliases,
            generic_aliases,
            type_params,
        ))),
        Ty::Tuple(inners) => Ty::Tuple(
            inners
                .into_iter()
                .map(|t| resolve_type(t, aliases, generic_aliases, type_params))
                .collect(),
        ),
        Ty::Union(inners) => {
            let mut resolved_inners: Vec<Ty> = Vec::new();
            for t in inners {
                let r = resolve_type(t, aliases, generic_aliases, type_params);
                match r {
                    Ty::Union(subs) => {
                        for s in subs {
                            if !resolved_inners.contains(&s) {
                                resolved_inners.push(s);
                            }
                        }
                    }
                    other => {
                        if !resolved_inners.contains(&other) {
                            resolved_inners.push(other);
                        }
                    }
                }
            }
            if resolved_inners.len() == 1 {
                resolved_inners.into_iter().next().unwrap()
            } else {
                Ty::Union(resolved_inners)
            }
        }
        Ty::Record { required, optional } => {
            let req = required
                .into_iter()
                .map(|(n, t)| (n, resolve_type(t, aliases, generic_aliases, type_params)))
                .collect();
            let opt = optional
                .into_iter()
                .map(|(n, t)| (n, resolve_type(t, aliases, generic_aliases, type_params)))
                .collect();
            Ty::Record {
                required: req,
                optional: opt,
            }
        }
        other => other,
    }
}

pub(crate) fn resolve_type_name(
    s: &str,
    aliases: &FastMap<String, Ty>,
    generic_aliases: &FastMap<String, TypeAliasDecl>,
    type_params: &FastMap<String, Ty>,
) -> Option<Ty> {
    let ls = s.trim();
    if ls.ends_with('?') {
        let base = &ls[..ls.len() - 1];
        return resolve_type_name(base, aliases, generic_aliases, type_params)
            .map(|t| Ty::Nullable(Box::new(t)));
    }
    // check type parameter scope first
    if let Some(tp) = type_params.get(ls) {
        return Some(tp.clone());
    }
    if let Some(t) = aliases.get(ls) {
        return Some(t.clone());
    }
    // try to parse as name with type args or union, e.g. Foo<int,string> or int | string
    if let Some(parsed) = type_from_name(ls) {
        let resolved = resolve_type(parsed, aliases, generic_aliases, type_params);
        match resolved {
            Ty::GenericInstance { name, args } => {
                if name.eq_ignore_ascii_case("Option") && args.len() == 1 {
                    return Some(Ty::Nullable(Box::new(args[0].clone())));
                }
                Some(Ty::GenericInstance { name, args })
            }
            other => Some(other),
        }
    } else {
        None
    }
}

pub fn type_from_name(s: &str) -> Option<Ty> {
    let ls = s.trim().to_string();
    if ls.is_empty() {
        return None;
    }

    // Top-level union parsing: T1 | T2 | T3
    let mut depth_bracket = 0;
    let mut depth_paren = 0;
    let mut depth_angle = 0;
    let mut depth_brace = 0;
    let mut union_parts: Vec<&str> = Vec::new();
    let mut last_idx = 0;

    for (i, ch) in ls.char_indices() {
        match ch {
            '[' => depth_bracket += 1,
            ']' => depth_bracket = (depth_bracket - 1).max(0),
            '(' => depth_paren += 1,
            ')' => depth_paren = (depth_paren - 1).max(0),
            '<' => depth_angle += 1,
            '>' => depth_angle = (depth_angle - 1).max(0),
            '{' => depth_brace += 1,
            '}' => depth_brace = (depth_brace - 1).max(0),
            '|' if depth_bracket == 0 && depth_paren == 0 && depth_angle == 0 && depth_brace == 0 => {
                union_parts.push(&ls[last_idx..i]);
                last_idx = i + 1;
            }
            _ => {}
        }
    }
    if !union_parts.is_empty() {
        union_parts.push(&ls[last_idx..]);
        let mut resolved_members: Vec<Ty> = Vec::new();
        for part in union_parts {
            let part_trimmed = part.trim();
            if part_trimmed.is_empty() {
                continue;
            }
            if let Some(ty) = type_from_name(part_trimmed) {
                match ty {
                    Ty::Union(subs) => {
                        for sub in subs {
                            if !resolved_members.contains(&sub) {
                                resolved_members.push(sub);
                            }
                        }
                    }
                    other => {
                        if !resolved_members.contains(&other) {
                            resolved_members.push(other);
                        }
                    }
                }
            } else {
                return None;
            }
        }
        if resolved_members.len() == 1 {
            return resolved_members.into_iter().next();
        }
        return Some(Ty::Union(resolved_members));
    }

    // support nullable suffix: e.g., "int?" -> Nullable(Int)
    if ls.ends_with('?') {
        let base = &ls[..ls.len() - 1];
        return type_from_name(base).map(|t| Ty::Nullable(Box::new(t)));
    }

    // Raw pointer syntax: *T
    if let Some(stripped) = ls.strip_prefix('*') {
        if let Some(inner) = type_from_name(stripped.trim()) {
            return Some(Ty::Ptr(Box::new(inner)));
        }
    }

    // Empty array type `[]`
    if ls == "[]" {
        return Some(Ty::Array(Box::new(Ty::Unknown)));
    }

    // Empty record/object `{}`
    if ls == "{}" {
        return Some(Ty::Record {
            required: Vec::new(),
            optional: Vec::new(),
        });
    }

    // Postfix array syntax: e.g., `int[]` or `string[][]`
    if ls.ends_with("[]") && ls.len() > 2 && !ls.starts_with('[') {
        let base = &ls[..ls.len() - 2];
        if let Some(inner) = type_from_name(base) {
            return Some(Ty::Array(Box::new(inner)));
        }
    }

    let arr_str = if ls.starts_with("&[") { &ls[1..] } else { &ls };
    if arr_str.starts_with('[') {
        let mut depth: i32 = 0;
        let mut end_idx: Option<usize> = None;
        for (i, ch) in arr_str.char_indices() {
            if ch == '[' {
                depth += 1;
            } else if ch == ']' {
                depth -= 1;
                if depth == 0 {
                    end_idx = Some(i);
                    break;
                }
            }
        }
        if let Some(end) = end_idx {
            let inner = arr_str[1..end].trim();
            let mut depth_inner: i32 = 0;
            let mut angle_depth: i32 = 0;
            let mut paren_depth: i32 = 0;
            let mut semi_idx: Option<usize> = None;
            for (i, ch) in inner.char_indices() {
                if ch == '[' {
                    depth_inner += 1;
                } else if ch == ']' {
                    depth_inner -= 1;
                } else if ch == '<' {
                    angle_depth += 1;
                } else if ch == '>' {
                    angle_depth -= 1;
                } else if ch == '(' {
                    paren_depth += 1;
                } else if ch == ')' {
                    paren_depth -= 1;
                } else if ch == ';' && depth_inner == 0 && angle_depth == 0 && paren_depth == 0 {
                    semi_idx = Some(i);
                    break;
                }
            }
            let elem_ty_str = if let Some(idx) = semi_idx {
                inner[..idx].trim()
            } else {
                inner.trim()
            };
            if !elem_ty_str.is_empty() {
                if let Some(elem_ty) = type_from_name(elem_ty_str) {
                    return Some(Ty::Array(Box::new(elem_ty)));
                } else {
                    return None;
                }
            }
        }
    }
    if ls.starts_with('(') && ls.ends_with(')') && !ls.contains("->") {
        let inner = &ls[1..ls.len() - 1];
        let mut items: Vec<Ty> = Vec::new();
        for part in inner.split(',') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            if let Some(pt) = type_from_name(p) {
                items.push(pt);
            } else {
                return None;
            }
        }
        return Some(Ty::Tuple(items));
    }
    // support function type syntax: "(t1,t2)->ret" or "fn(t1,t2)->ret"
    let lowered = ls.to_lowercase();
    if lowered.starts_with("fn(") || lowered.contains(")->") {
        // normalize to form '(params)->ret'
        let working = if lowered.starts_with("fn(") {
            lowered.replacen("fn", "", 1)
        } else {
            lowered.clone()
        };
        if let Some(idx) = working.find(")->") {
            let params_part = working.split_once(')').map(|(a, _)| a).unwrap_or("");
            // remove leading '(' if present
            let params_str = params_part.trim_start_matches('(').trim();
            let ret_part = &working[idx + 3..];
            let ret_ty = type_from_name(ret_part.trim())?;
            let mut params: Vec<Ty> = Vec::new();
            if !params_str.is_empty() {
                for p in params_str.split(',') {
                    let ptrim = p.trim();
                    if ptrim.is_empty() {
                        continue;
                    }
                    if let Some(pt) = type_from_name(ptrim) {
                        params.push(pt);
                    } else {
                        return None;
                    }
                }
            }
            return Some(Ty::Func {
                params,
                ret: Box::new(ret_ty),
            });
        }
    }
    // support generic instance syntax: Name<T1, T2>
    if let Some(lt_idx) = ls.find('<') {
        // find matching '>' for the first '<'
        let mut depth: i32 = 0;
        let mut end_idx: Option<usize> = None;
        for (i, ch) in ls.char_indices().skip(lt_idx) {
            if ch == '<' {
                depth += 1;
            } else if ch == '>' {
                depth -= 1;
                if depth == 0 {
                    end_idx = Some(i);
                    break;
                }
            }
        }
        if let Some(end) = end_idx {
            // allow only whitespace after the closing '>' for now
            if ls[end + 1..].trim().is_empty() {
                let name = ls[..lt_idx].trim();
                let args_str = &ls[lt_idx + 1..end];
                let mut args: Vec<Ty> = Vec::new();
                let mut cur = String::new();
                let mut inner_depth: i32 = 0;
                for ch in args_str.chars() {
                    if ch == '<' {
                        inner_depth += 1;
                        cur.push(ch);
                    } else if ch == '>' {
                        inner_depth -= 1;
                        cur.push(ch);
                    } else if ch == ',' && inner_depth == 0 {
                        let part = cur.trim();
                        if !part.is_empty() {
                            if let Some(at) = type_from_name(part) {
                                args.push(at);
                            } else {
                                return None;
                            }
                        }
                        cur.clear();
                    } else {
                        cur.push(ch);
                    }
                }
                if !cur.trim().is_empty() {
                    let part = cur.trim();
                    if let Some(at) = type_from_name(part) {
                        args.push(at);
                    } else {
                        return None;
                    }
                }
                return Some(Ty::GenericInstance {
                    name: name.to_string(),
                    args,
                });
            }
        }
    }
    match lowered.as_str() {
        "number" | "float" => Some(Ty::Float),
        "int" | "bigint" | "uint" => Some(Ty::Int),
        "char" => Some(Ty::Char),
        "string" | "str" => Some(Ty::Str),
        "complex" => Some(Ty::GenericParam("complex".to_string())),
        "bool" | "boolean" => Some(Ty::Bool),
        "null" | "undefined" => Some(Ty::Null),
        "void" => Some(Ty::Void),
        "never" => Some(Ty::Never),
        "any" => Some(Ty::Any),
        "unknown" => Some(Ty::Unknown),
        "set" => Some(Ty::GenericInstance {
            name: "Set".to_string(),
            args: vec![Ty::Any],
        }),
        "dict" => Some(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Any))),
        "map" => Some(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Any))),
        "array" | "list" => Some(Ty::Array(Box::new(Ty::Any))),
        "tuple" => Some(Ty::Tuple(Vec::new())),
        "object" => Some(Ty::Record {
            required: Vec::new(),
            optional: Vec::new(),
        }),
        // Fixed-width integer types (unsigned)
        "u8" => Some(Ty::U8),
        "u16" => Some(Ty::U16),
        "u32" => Some(Ty::U32),
        "u64" => Some(Ty::U64),
        "u128" => Some(Ty::U128),
        // Fixed-width integer types (signed)
        "i8" => Some(Ty::I8),
        "i16" => Some(Ty::I16),
        "i32" => Some(Ty::I32),
        "i64" => Some(Ty::I64),
        "i128" => Some(Ty::I128),
        // Fixed-width float types
        "f32" => Some(Ty::F32),
        "f64" => Some(Ty::F64Ty),
        _ => {
            let trimmed = ls.trim();
            if !trimmed.is_empty()
                && trimmed
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_alphabetic() || c == '_')
            {
                Some(Ty::GenericParam(trimmed.to_string()))
            } else {
                None
            }
        }
    }
}
