//! Statement type checking logic.

use super::expression_inference::infer_expr_type;
use super::narrowing::{apply_narrowing_to_env, is_numeric};
use super::resolution::resolve_type_name;
use crate::parsing::ast::*;
use crate::typesystem::checker::{Ty, is_subtype};
use crate::utils::collections::FastMap;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

pub(super) fn numeric_literal_fits_expected(n: f64, expected: &Ty) -> bool {
    if !n.is_finite() {
        return false;
    }

    let is_integral = n.fract() == 0.0;

    match expected {
        // An untyped/undetermined target accepts any literal: the general
        // compatibility check would accept it (`*exp == Ty::Any`), so the
        // literal fast-path must not reject it first.
        Ty::Any | Ty::Unknown => true,
        Ty::F32 => {
            // Accept unsuffixed float literals for f32 when finite and representable in f32 range.
            // (Precision narrowing is allowed by design for literal assignment.)
            let f = n as f32;
            f.is_finite()
        }
        Ty::F64Ty | Ty::Float => true,
        Ty::Int => is_integral && n >= i64::MIN as f64 && n <= i64::MAX as f64,
        Ty::U8 => is_integral && n >= u8::MIN as f64 && n <= u8::MAX as f64,
        Ty::U16 => is_integral && n >= u16::MIN as f64 && n <= u16::MAX as f64,
        Ty::U32 => is_integral && n >= u32::MIN as f64 && n <= u32::MAX as f64,
        Ty::U64 => is_integral && n >= 0.0 && n <= u64::MAX as f64,
        Ty::U128 => is_integral && n >= 0.0 && n <= u128::MAX as f64,
        Ty::I8 => is_integral && n >= i8::MIN as f64 && n <= i8::MAX as f64,
        Ty::I16 => is_integral && n >= i16::MIN as f64 && n <= i16::MAX as f64,
        Ty::I32 => is_integral && n >= i32::MIN as f64 && n <= i32::MAX as f64,
        Ty::I64 => is_integral && n >= i64::MIN as f64 && n <= i64::MAX as f64,
        Ty::I128 => is_integral && n >= i128::MIN as f64 && n <= i128::MAX as f64,
        _ => false,
    }
}

pub(super) fn stmt_has_return(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) => true,
        StmtKind::Block(stmts) => stmts.iter().any(stmt_has_return),
        StmtKind::UnsafeBlock(body) | StmtKind::Region { body, .. } => stmt_has_return(body),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            stmt_has_return(then_branch)
                || else_branch.as_ref().map_or(false, |b| stmt_has_return(b))
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => stmt_has_return(try_block) || stmt_has_return(catch_block),
        StmtKind::While { body, .. } | StmtKind::ForIn { body, .. } => stmt_has_return(body),
        _ => false,
    }
}

fn bigint_literal_fits_expected(n: &BigInt, expected: &Ty) -> bool {
    match expected {
        // Untyped/undetermined targets accept any literal (see
        // `numeric_literal_fits_expected`).
        Ty::Any | Ty::Unknown => true,
        Ty::F32 | Ty::F64Ty | Ty::Float => n.to_f64().is_some_and(|value| value.is_finite()),
        Ty::Int => true,
        Ty::U8 => n.to_u8().is_some(),
        Ty::U16 => n.to_u16().is_some(),
        Ty::U32 => n.to_u32().is_some(),
        Ty::U64 => n.to_u64().is_some(),
        Ty::U128 => n.to_u128().is_some(),
        Ty::I8 => n.to_i8().is_some(),
        Ty::I16 => n.to_i16().is_some(),
        Ty::I32 => n.to_i32().is_some(),
        Ty::I64 => n.to_i64().is_some(),
        Ty::I128 => n.to_i128().is_some(),
        _ => false,
    }
}

fn literal_numeric_fits_expected(expr: &Expr, expected: &Ty) -> Option<bool> {
    match &expr.kind {
        ExprKind::Literal(Value::Number(n)) => Some(numeric_literal_fits_expected(*n, expected)),
        ExprKind::Literal(Value::BigInt(n)) => Some(bigint_literal_fits_expected(n, expected)),
        _ => None,
    }
}

pub(crate) fn expr_matches_expected_type(expr: &Expr, inferred: &Ty, expected: &Ty) -> bool {
    let is_concrete_int = |t: &Ty| {
        matches!(
            t,
            Ty::Int
                | Ty::U8
                | Ty::U16
                | Ty::U32
                | Ty::U64
                | Ty::U128
                | Ty::I8
                | Ty::I16
                | Ty::I32
                | Ty::I64
                | Ty::I128
        )
    };

    let is_ptr = |t: &Ty| {
        matches!(
            t,
            Ty::Ptr(_) | Ty::PtrOwning(_) | Ty::PtrShared(_) | Ty::PtrMut(_)
        )
    };

    fn is_compatible(
        inf: &Ty,
        exp: &Ty,
        is_concrete_int: &dyn Fn(&Ty) -> bool,
        is_ptr: &dyn Fn(&Ty) -> bool,
    ) -> bool {
        if inf == exp || *inf == Ty::Any || *exp == Ty::Any {
            return true;
        }
        match (inf, exp) {
            (a, b) if (is_concrete_int(a) || *a == Ty::Null) && is_ptr(b) => true,
            (a, b) if is_concrete_int(a) && is_concrete_int(b) => true,
            (a, b)
                if (a == &Ty::Float || *a == Ty::F32 || *a == Ty::F64Ty)
                    && (b == &Ty::Float || *b == Ty::F32 || *b == Ty::F64Ty) =>
            {
                true
            }
            (Ty::Array(a), Ty::Array(b)) => {
                matches!(**a, Ty::Unknown | Ty::Never | Ty::Any)
                    || is_compatible(a, b, is_concrete_int, is_ptr)
            }
            (Ty::Tuple(a), Ty::Tuple(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter()
                    .zip(b.iter())
                    .all(|(x, y)| is_compatible(x, y, is_concrete_int, is_ptr))
            }
            (Ty::Nullable(a), Ty::Nullable(b)) => is_compatible(a, b, is_concrete_int, is_ptr),
            (Ty::Nullable(a), _b) if **a == Ty::Any => true,
            (Ty::Nullable(a), b) => is_compatible(a, b, is_concrete_int, is_ptr) && *b == Ty::Null,
            (a, Ty::Nullable(b)) => is_compatible(a, b, is_concrete_int, is_ptr) || *a == Ty::Null,
            (Ty::Union(sources), target) => sources
                .iter()
                .all(|s| is_compatible(s, target, is_concrete_int, is_ptr)),
            (source, Ty::Union(targets)) => targets
                .iter()
                .any(|t| is_compatible(source, t, is_concrete_int, is_ptr)),
            (
                Ty::Record {
                    required: sreq,
                    optional: sopt,
                },
                Ty::Record {
                    required: treq,
                    optional: topt,
                },
            ) => {
                for (k, t_ty) in treq {
                    let mut found = None;
                    for (sk, st) in sreq {
                        if sk == k {
                            found = Some(st);
                            break;
                        }
                    }
                    if found.is_none() {
                        for (sk, st) in sopt {
                            if sk == k {
                                found = Some(st);
                                break;
                            }
                        }
                    }
                    if let Some(st) = found {
                        if !is_compatible(st, t_ty, is_concrete_int, is_ptr) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                for (k, t_ty) in topt {
                    if let Some((_, st)) = sreq.iter().chain(sopt.iter()).find(|(sk, _)| sk == k) {
                        if !is_compatible(st, t_ty, is_concrete_int, is_ptr) {
                            return false;
                        }
                    }
                }
                true
            }
            _ => is_subtype(inf, exp),
        }
    }

    if let Some(fits) = literal_numeric_fits_expected(expr, expected) {
        return fits;
    }

    // When assigning an object literal to a record/struct type, validate each field expression
    // against the expected field type (e.g. checking numeric literal bounds like u8 range 0..255).
    if let Ty::Record { required, optional } = expected {
        if let ExprKind::Object(kv) = &expr.kind {
            for (req_name, req_ty) in required {
                if let Some((_, field_expr)) = kv.iter().find(|(k, _)| k == req_name) {
                    if let Some(fits) = literal_numeric_fits_expected(field_expr, req_ty) {
                        if !fits {
                            return false;
                        }
                    }
                }
            }
            for (opt_name, opt_ty) in optional {
                if let Some((_, field_expr)) = kv.iter().find(|(k, _)| k == opt_name) {
                    if let Some(fits) = literal_numeric_fits_expected(field_expr, opt_ty) {
                        if !fits {
                            return false;
                        }
                    }
                }
            }
        }
    }

    if is_compatible(inferred, expected, &is_concrete_int, &is_ptr) {
        return true;
    }

    if *inferred == Ty::Int {
        if is_concrete_int(expected) {
            return true;
        }
        if let Ty::Nullable(inner) = expected {
            if is_concrete_int(inner) {
                return true;
            }
        }
    }

    if let Ty::Union(choices) = inferred {
        let expected_is_int = is_concrete_int(expected)
            || matches!(expected, Ty::Nullable(inner) if is_concrete_int(inner));

        if expected_is_int
            && choices.iter().all(|choice| {
                is_subtype(choice, expected)
                    || *choice == Ty::Int
                    || matches!(choice, Ty::Nullable(inner) if **inner == Ty::Int)
            })
        {
            return true;
        }
    }

    if let (Ty::Nullable(inner_inferred), Ty::Nullable(inner_expected)) = (inferred, expected) {
        if **inner_inferred == Ty::Int && is_concrete_int(inner_expected) {
            return true;
        }
    }

    if let Ty::Nullable(inner_expected) = expected {
        if let ExprKind::Call(callee, args, _) = &expr.kind {
            if let ExprKind::Variable(name) = &callee.kind {
                if name == "Some" && args.len() == 1 {
                    if let ExprKind::Literal(Value::Number(n)) = &args[0].kind {
                        if numeric_literal_fits_expected(*n, inner_expected) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    if *expected == Ty::Any || is_subtype(inferred, expected) {
        return true;
    }

    if let Some(fits) = literal_numeric_fits_expected(expr, expected) {
        return fits;
    }

    // When assigning an object literal to a record/struct type, validate each field expression
    // against the expected field type (e.g. checking numeric literal bounds like u8 range 0..255).
    if let Ty::Record { required, optional } = expected {
        if let ExprKind::Object(kv) = &expr.kind {
            for (req_name, req_ty) in required {
                if let Some((_, field_expr)) = kv.iter().find(|(k, _)| k == req_name) {
                    if let Some(fits) = literal_numeric_fits_expected(field_expr, req_ty) {
                        if !fits {
                            return false;
                        }
                    }
                }
            }
            for (opt_name, opt_ty) in optional {
                if let Some((_, field_expr)) = kv.iter().find(|(k, _)| k == opt_name) {
                    if let Some(fits) = literal_numeric_fits_expected(field_expr, opt_ty) {
                        if !fits {
                            return false;
                        }
                    }
                }
            }
        }
    }

    false
}

#[inline]
fn record_stmt_error(errors: &mut Vec<String>, s: &Stmt, err: String) {
    if err.contains("@@span:") {
        errors.push(err);
    } else {
        errors.push(format!(
            "{}@@span:{}:{}:{}",
            err, s.span.line, s.span.col, s.span.line_text
        ));
    }
}

pub(crate) fn check_stmt_types(
    s: &Stmt,
    env: &mut Vec<FastMap<String, Ty>>,
    fns: &FastMap<String, Vec<Option<String>>>,
    fns_type_params: &FastMap<String, Vec<String>>,
    fns_ret_types: &FastMap<String, Option<String>>,
    in_loop: bool,
    expected_ret: Option<Ty>,
    aliases: &FastMap<String, Ty>,
    generic_aliases: &FastMap<String, TypeAliasDecl>,
    type_params: &FastMap<String, Ty>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    check_stmt_types_collecting(
        s,
        env,
        fns,
        fns_type_params,
        fns_ret_types,
        in_loop,
        expected_ret,
        aliases,
        generic_aliases,
        type_params,
        &mut errors,
    );
    if let Some(first) = errors.into_iter().next() {
        Err(first)
    } else {
        Ok(())
    }
}

pub(crate) fn check_stmt_types_collecting(
    s: &Stmt,
    env: &mut Vec<FastMap<String, Ty>>,
    fns: &FastMap<String, Vec<Option<String>>>,
    fns_type_params: &FastMap<String, Vec<String>>,
    fns_ret_types: &FastMap<String, Option<String>>,
    in_loop: bool,
    expected_ret: Option<Ty>,
    aliases: &FastMap<String, Ty>,
    generic_aliases: &FastMap<String, TypeAliasDecl>,
    type_params: &FastMap<String, Ty>,
    errors: &mut Vec<String>,
) {
    match &s.kind {
        StmtKind::ExprStmt(e) => {
            if let Err(err) = infer_expr_type(
                e,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                record_stmt_error(errors, s, err);
            }
        }
        StmtKind::ShareDeclaration(decl, _) => {
            match infer_expr_type(
                &decl.expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                Ok(expr_ty) => {
                    let env_idx = env.len() - 1;
                    env[env_idx].insert(decl.name.clone(), expr_ty);
                }
                Err(err) => {
                    record_stmt_error(errors, s, err);
                    let env_idx = env.len() - 1;
                    env[env_idx].insert(decl.name.clone(), Ty::Any);
                }
            }
        }
        StmtKind::StrongDeclaration(decl, _) => {
            match infer_expr_type(
                &decl.expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                Ok(expr_ty) => {
                    let env_idx = env.len() - 1;
                    env[env_idx].insert(decl.name.clone(), expr_ty);
                }
                Err(err) => {
                    record_stmt_error(errors, s, err);
                    let env_idx = env.len() - 1;
                    env[env_idx].insert(decl.name.clone(), Ty::Any);
                }
            }
        }
        StmtKind::WeakDeclaration(decl, _) => {
            match infer_expr_type(
                &decl.expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                Ok(expr_ty) => {
                    let env_idx = env.len() - 1;
                    env[env_idx].insert(decl.name.clone(), expr_ty);
                }
                Err(err) => {
                    record_stmt_error(errors, s, err);
                    let env_idx = env.len() - 1;
                    env[env_idx].insert(decl.name.clone(), Ty::Any);
                }
            }
        }
        StmtKind::Let(name, init, type_ann, _export, _is_const, _is_readonly) => {
            let declared = if let Some(tn) = type_ann {
                if let Some(dt) = resolve_type_name(tn, aliases, generic_aliases, type_params) {
                    Some(dt)
                } else {
                    record_stmt_error(errors, s, format!("unknown type '{}' for '{}'", tn, name));
                    None
                }
            } else {
                None
            };

            if let Some(e) = init {
                match infer_expr_type(
                    e,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                ) {
                    Ok(inferred) => {
                        if let Some(d) = &declared {
                            if *d != Ty::Any && !expr_matches_expected_type(e, &inferred, d) {
                                let mut field_range_err: Option<String> = None;
                                if let Ty::Record { required, optional } = d {
                                    if let ExprKind::Object(kv) = &e.kind {
                                        for (req_name, req_ty) in required.iter().chain(optional.iter()) {
                                            if let Some((_, field_expr)) = kv.iter().find(|(k, _)| k == req_name) {
                                                if let Some(false) = literal_numeric_fits_expected(field_expr, req_ty) {
                                                    let val_str = match &field_expr.kind {
                                                        ExprKind::Literal(Value::Number(n)) => format!("{}", n),
                                                        ExprKind::Literal(Value::BigInt(bi)) => format!("{}", bi),
                                                        _ => "value".to_string(),
                                                    };
                                                    field_range_err = Some(format!(
                                                        "type mismatch for '{}': field '{}' value {} is out of range for {}",
                                                        name, req_name, val_str, req_ty
                                                    ));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                let err_msg = field_range_err.unwrap_or_else(|| {
                                    format!(
                                        "type mismatch for '{}': declared {}, found {}",
                                        name, d, inferred
                                    )
                                });
                                record_stmt_error(errors, s, err_msg);
                            }
                            if let Some(scope) = env.last_mut() {
                                scope.insert(name.clone(), d.clone());
                            }
                        } else {
                            if let Some(scope) = env.last_mut() {
                                scope.insert(name.clone(), inferred);
                            }
                        }
                    }
                    Err(err) => {
                        record_stmt_error(errors, s, err);
                        let fallback = declared.unwrap_or(Ty::Any);
                        if let Some(scope) = env.last_mut() {
                            scope.insert(name.clone(), fallback);
                        }
                    }
                }
            } else {
                let d = declared.unwrap_or(Ty::Null);
                if let Some(scope) = env.last_mut() {
                    scope.insert(name.clone(), d);
                }
            }
        }
        StmtKind::Extend(_name, _target, _implements, methods, _export) => {
            for m in methods {
                let mut m_scope = FastMap::default();
                if m.is_async {
                    m_scope.insert("__in_async__".to_string(), Ty::Bool);
                } else {
                    m_scope.insert("__in_async__".to_string(), Ty::Void);
                }
                for p in &m.params {
                    let name = p.0.clone();
                    if let Some(tn) = &p.2 {
                        if let Some(tt) =
                            resolve_type_name(tn, aliases, generic_aliases, type_params)
                        {
                            m_scope.insert(name, tt);
                        } else {
                            record_stmt_error(
                                errors,
                                s,
                                format!("unknown type '{}' for parameter '{}'", tn, name),
                            );
                            m_scope.insert(name, Ty::Any);
                        }
                    } else {
                        m_scope.insert(name, Ty::Any);
                    }
                }
                env.push(m_scope);
                for st in m.body.iter() {
                    check_stmt_types_collecting(
                        st,
                        env,
                        fns,
                        fns_type_params,
                        fns_ret_types,
                        false,
                        None,
                        aliases,
                        generic_aliases,
                        type_params,
                        errors,
                    );
                }
                env.pop();
            }
        }
        StmtKind::Struct(_s, _export) => {}
        StmtKind::Enum(_e, _export) => {}
        StmtKind::Interface(i, _export) => {
            let mut local_type_params = type_params.clone();
            if !i.type_params.is_empty() {
                for tp in &i.type_params {
                    local_type_params.insert(tp.clone(), Ty::GenericParam(tp.clone()));
                }
            }
            for m in &i.methods {
                let mut m_scope = FastMap::default();
                for p in &m.params {
                    let name = p.0.clone();
                    if let Some(tn) = &p.2 {
                        if let Some(tt) =
                            resolve_type_name(tn, aliases, generic_aliases, &local_type_params)
                        {
                            m_scope.insert(name, tt);
                        } else {
                            record_stmt_error(
                                errors,
                                s,
                                format!("unknown type '{}' for parameter '{}'", tn, name),
                            );
                            m_scope.insert(name, Ty::Any);
                        }
                    } else {
                        m_scope.insert(name, Ty::Any);
                    }
                }
                env.push(m_scope);
                for st in m.body.iter() {
                    check_stmt_types_collecting(
                        st,
                        env,
                        fns,
                        fns_type_params,
                        fns_ret_types,
                        false,
                        None,
                        aliases,
                        generic_aliases,
                        &local_type_params,
                        errors,
                    );
                }
                env.pop();
            }
        }
        StmtKind::Function(f, _export) => {
            let mut local_type_params = type_params.clone();
            if !f.type_params.is_empty() {
                for tp in &f.type_params {
                    local_type_params.insert(tp.clone(), Ty::GenericParam(tp.clone()));
                }
            }
            let mut fn_scope = FastMap::default();
            if f.is_async {
                fn_scope.insert("__in_async__".to_string(), Ty::Bool);
            } else {
                fn_scope.insert("__in_async__".to_string(), Ty::Void);
            }
            for p in &f.params {
                let name = p.0.clone();
                if let Some(tn) = &p.2 {
                    if let Some(tt) =
                        resolve_type_name(tn, aliases, generic_aliases, &local_type_params)
                    {
                        fn_scope.insert(name, tt);
                    } else {
                        record_stmt_error(
                            errors,
                            s,
                            format!("unknown type '{}' for parameter '{}'", tn, name),
                        );
                        fn_scope.insert(name, Ty::Any);
                    }
                } else {
                    fn_scope.insert(name, Ty::Any);
                }
            }
            let expected = if let Some(rt_name) = &f.ret_type {
                resolve_type_name(rt_name, aliases, generic_aliases, &local_type_params)
            } else {
                None
            };
            env.push(fn_scope);
            for st in f.body.iter() {
                check_stmt_types_collecting(
                    st,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    false,
                    expected.clone(),
                    aliases,
                    generic_aliases,
                    &local_type_params,
                    errors,
                );
            }
            if let Some(exp) = &expected {
                if *exp != Ty::Any {
                    let has_ret = f.body.iter().any(stmt_has_return);
                    if !has_ret {
                        if let Some(last) = f.body.as_ref().last() {
                            if let StmtKind::ExprStmt(e) = &last.kind {
                                match infer_expr_type(
                                    e,
                                    env,
                                    fns,
                                    fns_type_params,
                                    fns_ret_types,
                                    aliases,
                                    generic_aliases,
                                    &local_type_params,
                                ) {
                                    Ok(lt) => {
                                        if !is_subtype(&lt, exp) {
                                            record_stmt_error(
                                                errors,
                                                s,
                                                format!(
                                                    "missing or mismatched return: expected {}, found {}",
                                                    exp, lt
                                                ),
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        record_stmt_error(errors, s, err);
                                    }
                                }
                            } else {
                                record_stmt_error(
                                    errors,
                                    s,
                                    "missing return in function with declared return type".into(),
                                );
                            }
                        } else {
                            record_stmt_error(
                                errors,
                                s,
                                "missing return in function with declared return type".into(),
                            );
                        }
                    }
                }
            }
            env.pop();
        }
        StmtKind::Class(c, _export) => {
            let mut local_type_params = type_params.clone();
            if !c.type_params.is_empty() {
                for tp in &c.type_params {
                    local_type_params.insert(tp.clone(), Ty::GenericParam(tp.clone()));
                }
            }
            for m in &c.methods {
                let mut method_type_params = local_type_params.clone();
                if !m.type_params.is_empty() {
                    for tp in &m.type_params {
                        method_type_params.insert(tp.clone(), Ty::GenericParam(tp.clone()));
                    }
                }
                let mut m_scope = FastMap::default();
                if m.is_async {
                    m_scope.insert("__in_async__".to_string(), Ty::Bool);
                } else {
                    m_scope.insert("__in_async__".to_string(), Ty::Void);
                }
                for p in &m.params {
                    let name = p.0.clone();
                    if let Some(tn) = &p.2 {
                        if let Some(tt) =
                            resolve_type_name(tn, aliases, generic_aliases, &method_type_params)
                        {
                            m_scope.insert(name, tt);
                        } else {
                            record_stmt_error(
                                errors,
                                s,
                                format!("unknown type '{}' for parameter '{}'", tn, name),
                            );
                            m_scope.insert(name, Ty::Any);
                        }
                    } else {
                        m_scope.insert(name, Ty::Any);
                    }
                }
                let expected = if let Some(rt_name) = &m.ret_type {
                    resolve_type_name(rt_name, aliases, generic_aliases, &method_type_params)
                } else {
                    None
                };
                env.push(m_scope);
                for st in m.body.iter() {
                    check_stmt_types_collecting(
                        st,
                        env,
                        fns,
                        fns_type_params,
                        fns_ret_types,
                        false,
                        expected.clone(),
                        aliases,
                        generic_aliases,
                        &method_type_params,
                        errors,
                    );
                }
                env.pop();
            }
        }
        StmtKind::Block(bs) => {
            env.push(FastMap::default());
            for b in bs {
                check_stmt_types_collecting(
                    b,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    in_loop,
                    expected_ret.clone(),
                    aliases,
                    generic_aliases,
                    type_params,
                    errors,
                );
            }
            env.pop();
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            match infer_expr_type(
                cond,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                Ok(ct) => {
                    if !(ct == Ty::Any || is_subtype(&ct, &Ty::Bool)) {
                        record_stmt_error(
                            errors,
                            s,
                            format!("condition expected Bool, found {}", ct),
                        );
                    }
                }
                Err(err) => {
                    record_stmt_error(errors, s, err);
                }
            }

            let mut then_env = env.clone();
            apply_narrowing_to_env(cond, &mut then_env, true);
            check_stmt_types_collecting(
                then_branch,
                &mut then_env,
                fns,
                fns_type_params,
                fns_ret_types,
                in_loop,
                expected_ret.clone(),
                aliases,
                generic_aliases,
                type_params,
                errors,
            );

            if let Some(e) = else_branch {
                let mut else_env = env.clone();
                apply_narrowing_to_env(cond, &mut else_env, false);
                check_stmt_types_collecting(
                    e,
                    &mut else_env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    in_loop,
                    expected_ret.clone(),
                    aliases,
                    generic_aliases,
                    type_params,
                    errors,
                );
            }
        }
        StmtKind::While { cond, body } => {
            match infer_expr_type(
                cond,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                Ok(ct) => {
                    if !(ct == Ty::Any || is_subtype(&ct, &Ty::Bool)) {
                        record_stmt_error(
                            errors,
                            s,
                            format!("while condition expected Bool, found {}", ct),
                        );
                    }
                }
                Err(err) => {
                    record_stmt_error(errors, s, err);
                }
            }
            check_stmt_types_collecting(
                body,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                true,
                expected_ret.clone(),
                aliases,
                generic_aliases,
                type_params,
                errors,
            );
        }
        StmtKind::ForIn { name, iter, body } => {
            let it = infer_expr_type(
                iter,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            );
            let loop_var_ty = match &it {
                Ok(Ty::Array(elem)) => (**elem).clone(),
                Ok(
                    Ty::Ptr(elem) | Ty::PtrOwning(elem) | Ty::PtrShared(elem) | Ty::PtrMut(elem),
                ) => (**elem).clone(),
                Ok(Ty::Str) => Ty::Str,
                Ok(Ty::Map(k, _)) => (**k).clone(),
                Ok(Ty::GenericInstance { name: gname, args })
                    if (gname == "Set" || gname == "set") && !args.is_empty() =>
                {
                    args[0].clone()
                }
                Ok(_) => match &iter.kind {
                    ExprKind::Range(_, _, _) => Ty::Int,
                    ExprKind::Call(callee, _, _) => {
                        if let ExprKind::Variable(fname) = &callee.kind {
                            if fname == "range" { Ty::Int } else { Ty::Any }
                        } else {
                            Ty::Any
                        }
                    }
                    _ => Ty::Any,
                },
                Err(err) => {
                    record_stmt_error(errors, s, err.clone());
                    Ty::Any
                }
            };
            env.push(FastMap::default());
            if let Some(scope) = env.last_mut() {
                scope.insert(name.clone(), loop_var_ty);
            }
            check_stmt_types_collecting(
                body,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                true,
                expected_ret.clone(),
                aliases,
                generic_aliases,
                type_params,
                errors,
            );
            env.pop();
        }
        StmtKind::Break => {
            if !in_loop {
                record_stmt_error(errors, s, "'break' used outside of loop".into());
            }
        }
        StmtKind::Continue => {
            if !in_loop {
                record_stmt_error(errors, s, "'continue' used outside of loop".into());
            }
        }
        StmtKind::Jump(expr) => {
            if !in_loop {
                record_stmt_error(errors, s, "'jump' used outside of loop".into());
            }
            match infer_expr_type(
                expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                Ok(jt) => {
                    if !is_numeric(&jt) {
                        record_stmt_error(errors, s, "'jump' expects numeric expression".into());
                    }
                }
                Err(err) => {
                    record_stmt_error(errors, s, err);
                }
            }
        }
        StmtKind::Return(opt) => {
            if let Some(e) = opt {
                match infer_expr_type(
                    e,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                ) {
                    Ok(rt) => {
                        if let Some(expected) = &expected_ret {
                            let type_ok = expr_matches_expected_type(e, &rt, expected);
                            if !type_ok && *expected != Ty::Any {
                                record_stmt_error(
                                    errors,
                                    s,
                                    format!(
                                        "return type mismatch: expected {}, found {}",
                                        expected, rt
                                    ),
                                );
                            }
                        }
                    }
                    Err(err) => {
                        record_stmt_error(errors, s, err);
                    }
                }
            } else {
                if let Some(expected) = &expected_ret {
                    if *expected != Ty::Any && !is_subtype(&Ty::Null, expected) {
                        record_stmt_error(
                            errors,
                            s,
                            format!("return type mismatch: expected {}, found null", expected),
                        );
                    }
                }
            }
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            check_stmt_types_collecting(
                try_block,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                in_loop,
                expected_ret.clone(),
                aliases,
                generic_aliases,
                type_params,
                errors,
            );
            check_stmt_types_collecting(
                catch_block,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                in_loop,
                expected_ret.clone(),
                aliases,
                generic_aliases,
                type_params,
                errors,
            );
        }
        StmtKind::Import { alias, .. } | StmtKind::ImportDefault { alias, .. } => {
            if let Some(scope) = env.last_mut() {
                scope.insert(alias.clone(), Ty::Any);
            }
        }
        StmtKind::ImportNames { names, .. } => {
            if let Some(scope) = env.last_mut() {
                for n in names {
                    scope.insert(n.clone(), Ty::Any);
                }
            }
        }
        StmtKind::ExportDefault(_name) => {}
        StmtKind::ExportDefaultFunction(_f) => {}
        StmtKind::ExportDefaultClass(_c) => {}
        StmtKind::TypeAlias(_, _) => {}
        StmtKind::LetTuple(names, type_anns, init, _, _, _is_readonly) => {
            let init_expr = match init {
                Some(expr) => expr,
                None => {
                    record_stmt_error(
                        errors,
                        s,
                        "tuple destructuring requires an initializer".to_string(),
                    );
                    if let Some(scope) = env.last_mut() {
                        for name in names {
                            scope.insert(name.clone(), Ty::Any);
                        }
                    }
                    return;
                }
            };

            let init_type = match infer_expr_type(
                init_expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                Ok(ty) => ty,
                Err(err) => {
                    record_stmt_error(errors, s, err);
                    if let Some(scope) = env.last_mut() {
                        for name in names {
                            scope.insert(name.clone(), Ty::Any);
                        }
                    }
                    return;
                }
            };

            match &init_type {
                Ty::Tuple(elem_types) => {
                    if elem_types.len() != names.len() {
                        record_stmt_error(
                            errors,
                            s,
                            format!(
                                "tuple destructuring: expected {} elements, found {}",
                                names.len(),
                                elem_types.len()
                            ),
                        );
                    }
                    if let Some(scope) = env.last_mut() {
                        for (i, name) in names.iter().enumerate() {
                            let elem_ty = elem_types.get(i).unwrap_or(&Ty::Any);
                            let ann = type_anns.as_ref().and_then(|anns| anns.get(i));
                            if let Some(ann_str) = ann {
                                if let Some(anno_ty) = resolve_type_name(
                                    ann_str,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                ) {
                                    if !is_subtype(elem_ty, &anno_ty) && anno_ty != Ty::Any {
                                        record_stmt_error(
                                            errors,
                                            s,
                                            format!(
                                                "tuple element '{}': expected type {}, found {}",
                                                name, anno_ty, elem_ty
                                            ),
                                        );
                                    }
                                    scope.insert(name.clone(), anno_ty);
                                } else {
                                    record_stmt_error(
                                        errors,
                                        s,
                                        format!(
                                            "unknown type annotation '{}' for '{}'",
                                            ann_str, name
                                        ),
                                    );
                                    scope.insert(name.clone(), elem_ty.clone());
                                }
                            } else {
                                scope.insert(name.clone(), elem_ty.clone());
                            }
                        }
                    }
                }
                Ty::Array(elem_ty) => {
                    if let Some(scope) = env.last_mut() {
                        for (i, name) in names.iter().enumerate() {
                            let ann = type_anns.as_ref().and_then(|anns| anns.get(i));
                            if let Some(ann_str) = ann {
                                if let Some(anno_ty) = resolve_type_name(
                                    ann_str,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                ) {
                                    if !is_subtype(elem_ty, &anno_ty) && anno_ty != Ty::Any {
                                        record_stmt_error(
                                            errors,
                                            s,
                                            format!(
                                                "array element '{}': expected type {}, found {}",
                                                name, anno_ty, elem_ty
                                            ),
                                        );
                                    }
                                    scope.insert(name.clone(), anno_ty);
                                } else {
                                    record_stmt_error(
                                        errors,
                                        s,
                                        format!(
                                            "unknown type annotation '{}' for '{}'",
                                            ann_str, name
                                        ),
                                    );
                                    scope.insert(name.clone(), (**elem_ty).clone());
                                }
                            } else {
                                scope.insert(name.clone(), (**elem_ty).clone());
                            }
                        }
                    }
                }
                Ty::Any => {
                    if let Some(scope) = env.last_mut() {
                        for (i, name) in names.iter().enumerate() {
                            let ann = type_anns.as_ref().and_then(|anns| anns.get(i));
                            if let Some(ann_str) = ann {
                                if let Some(anno_ty) = resolve_type_name(
                                    ann_str,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                ) {
                                    scope.insert(name.clone(), anno_ty);
                                } else {
                                    record_stmt_error(
                                        errors,
                                        s,
                                        format!(
                                            "unknown type annotation '{}' for '{}'",
                                            ann_str, name
                                        ),
                                    );
                                    scope.insert(name.clone(), Ty::Any);
                                }
                            } else {
                                scope.insert(name.clone(), Ty::Any);
                            }
                        }
                    }
                }
                _ => {
                    record_stmt_error(
                        errors,
                        s,
                        format!("cannot destructure non-tuple/array type: {}", init_type),
                    );
                    if let Some(scope) = env.last_mut() {
                        for name in names {
                            scope.insert(name.clone(), Ty::Any);
                        }
                    }
                }
            }
        }
        StmtKind::LetObject(bindings, init, _, _, _is_readonly) => {
            let init_expr = match init {
                Some(expr) => expr,
                None => {
                    record_stmt_error(
                        errors,
                        s,
                        "object destructuring requires an initializer".to_string(),
                    );
                    if let Some(scope) = env.last_mut() {
                        for (key, alias) in bindings.iter() {
                            let var_name = alias.as_ref().unwrap_or(key);
                            scope.insert(var_name.clone(), Ty::Any);
                        }
                    }
                    return;
                }
            };

            if let Err(err) = infer_expr_type(
                init_expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                record_stmt_error(errors, s, err);
            }

            if let Some(scope) = env.last_mut() {
                for (key, alias) in bindings.iter() {
                    let var_name = alias.as_ref().unwrap_or(key);
                    scope.insert(var_name.clone(), Ty::Any);
                }
            }
        }
        StmtKind::HeaderImport { .. } => {}
        StmtKind::ExternFunction(_) => {}
        StmtKind::ExternBlock { .. } => {}
        StmtKind::Region { name: _, body } => check_stmt_types_collecting(
            body,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            in_loop,
            expected_ret.clone(),
            aliases,
            generic_aliases,
            type_params,
            errors,
        ),
        StmtKind::UnsafeBlock(body) => check_stmt_types_collecting(
            body,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            in_loop,
            expected_ret.clone(),
            aliases,
            generic_aliases,
            type_params,
            errors,
        ),
        StmtKind::Defer(body) => check_stmt_types_collecting(
            body,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            in_loop,
            expected_ret.clone(),
            aliases,
            generic_aliases,
            type_params,
            errors,
        ),
        StmtKind::Decorator(_def, _is_export) => {}
    }
}
