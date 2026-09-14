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
            _ => is_subtype(inf, exp),
        }
    }

    if is_compatible(inferred, expected, &is_concrete_int, &is_ptr) {
        return true;
    }

    if let Some(fits) = literal_numeric_fits_expected(expr, expected) {
        if fits {
            return true;
        }
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

    false
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
    check_stmt_types_inner(
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
    )
    .map_err(|err| {
        if err.contains("@@span:") {
            err
        } else {
            format!(
                "{}@@span:{}:{}:{}",
                err, s.span.line, s.span.col, s.span.line_text
            )
        }
    })
}

fn check_stmt_types_inner(
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
    match &s.kind {
        StmtKind::ExprStmt(e) => infer_expr_type(
            e,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            aliases,
            generic_aliases,
            type_params,
        )
        .map(|_| ()),
        StmtKind::ShareDeclaration(decl, _) => {
            let expr_ty = infer_expr_type(
                &decl.expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let env_idx = env.len() - 1;
            env[env_idx].insert(decl.name.clone(), expr_ty);
            Ok(())
        }
        StmtKind::StrongDeclaration(decl, _) => {
            let expr_ty = infer_expr_type(
                &decl.expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let env_idx = env.len() - 1;
            env[env_idx].insert(decl.name.clone(), expr_ty);
            Ok(())
        }
        StmtKind::WeakDeclaration(decl, _) => {
            let expr_ty = infer_expr_type(
                &decl.expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let env_idx = env.len() - 1;
            env[env_idx].insert(decl.name.clone(), expr_ty);
            Ok(())
        }
        StmtKind::Let(name, init, type_ann, _export, _is_const, _is_readonly) => {
            // If a type annotation is present, enforce it.
            let declared = if let Some(tn) = type_ann {
                if let Some(dt) = resolve_type_name(tn, aliases, generic_aliases, type_params) {
                    Some(dt)
                } else {
                    return Err(format!("unknown type '{}' for '{}'", tn, name));
                }
            } else {
                None
            };

            if let Some(e) = init {
                let inferred = infer_expr_type(
                    e,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
                if let Some(d) = declared {
                    if d != Ty::Any && !expr_matches_expected_type(e, &inferred, &d) {
                        return Err(format!(
                            "type mismatch for '{}': declared {}, found {}",
                            name, d, inferred
                        ));
                    }
                    env.last_mut()
                        .ok_or("Internal error: scope stack empty")?
                        .insert(name.clone(), d);
                } else {
                    env.last_mut()
                        .ok_or("Internal error: scope stack empty")?
                        .insert(name.clone(), inferred);
                }
            } else {
                // no initializer: if declared put declared type, else Null
                if let Some(d) = declared {
                    env.last_mut()
                        .ok_or("Internal error: scope stack empty")?
                        .insert(name.clone(), d);
                } else {
                    env.last_mut()
                        .ok_or("Internal error: scope stack empty")?
                        .insert(name.clone(), Ty::Null);
                }
            }
            Ok(())
        }
        StmtKind::Extend(_name, _target, methods, _export) => {
            // treat extension methods like class methods for checking
            for m in methods {
                let mut m_scope = FastMap::default();
                if m.is_async {
                    m_scope.insert("__in_async__".to_string(), Ty::Bool);
                } else {
                    m_scope.insert("__in_async__".to_string(), Ty::Void);
                }
                // params are (name, default_expr, Option<type_name>)
                for p in &m.params {
                    let name = p.0.clone();
                    if let Some(tn) = &p.2 {
                        if let Some(tt) =
                            resolve_type_name(tn, aliases, generic_aliases, type_params)
                        {
                            m_scope.insert(name, tt);
                        } else {
                            return Err(format!("unknown type '{}' for parameter '{}'", tn, name));
                        }
                    } else {
                        m_scope.insert(name, Ty::Any);
                    }
                }
                // method-level expected return: none (we don't have ret annotations on methods in this AST site)
                env.push(m_scope);
                for st in m.body.iter() {
                    check_stmt_types(
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
                    )?;
                }
                env.pop();
            }
            Ok(())
        }
        StmtKind::Struct(_s, _export) => Ok(()),
        StmtKind::Enum(_e, _export) => Ok(()),
        StmtKind::Interface(i, _export) => {
            // prepare a local type-parameter scope for the interface
            let mut local_type_params = type_params.clone();
            if !i.type_params.is_empty() {
                for tp in &i.type_params {
                    local_type_params.insert(tp.clone(), Ty::GenericParam(tp.clone()));
                }
            }
            // check interface method bodies (they should be signatures only, but parse allows empty/placeholder)
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
                            return Err(format!("unknown type '{}' for parameter '{}'", tn, name));
                        }
                    } else {
                        m_scope.insert(name, Ty::Any);
                    }
                }
                env.push(m_scope);
                for st in m.body.iter() {
                    check_stmt_types(
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
                    )?;
                }
                env.pop();
            }
            Ok(())
        }
        StmtKind::Function(f, _export) => {
            // compute expected return type for this function (if annotated)
            // prepare type-parameter scope for this function
            let mut local_type_params = type_params.clone();
            if !f.type_params.is_empty() {
                for tp in &f.type_params {
                    local_type_params.insert(tp.clone(), Ty::GenericParam(tp.clone()));
                }
            }
            // new scope for function parameters & body
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
                        return Err(format!("unknown type '{}' for parameter '{}'", tn, name));
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
                check_stmt_types(
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
                )?;
            }
            // ensure return across control paths when expected is present
            if let Some(exp) = &expected {
                if *exp != Ty::Any {
                    let has_ret = f.body.iter().any(stmt_has_return);
                    if !has_ret {
                        if let Some(last) = f.body.as_ref().last() {
                            if let StmtKind::ExprStmt(e) = &last.kind {
                                let lt = infer_expr_type(
                                    e,
                                    env,
                                    fns,
                                    fns_type_params,
                                    fns_ret_types,
                                    aliases,
                                    generic_aliases,
                                    &local_type_params,
                                )?;
                                if !is_subtype(&lt, exp) {
                                    return Err(format!(
                                        "missing or mismatched return: expected {}, found {}",
                                        exp, lt
                                    ));
                                }
                            } else {
                                return Err(
                                    "missing return in function with declared return type".into()
                                );
                            }
                        } else {
                            return Err(
                                "missing return in function with declared return type".into()
                            );
                        }
                    }
                }
            }
            env.pop();
            Ok(())
        }
        StmtKind::Class(c, _export) => {
            // prepare a local type-parameter scope for the class
            let mut local_type_params = type_params.clone();
            if !c.type_params.is_empty() {
                for tp in &c.type_params {
                    local_type_params.insert(tp.clone(), Ty::GenericParam(tp.clone()));
                }
            }
            // methods checked in their own scope
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
                            return Err(format!("unknown type '{}' for parameter '{}'", tn, name));
                        }
                    } else {
                        m_scope.insert(name, Ty::Any);
                    }
                }
                // method return annotation supported via m.ret_type if present
                let expected = if let Some(rt_name) = &m.ret_type {
                    resolve_type_name(rt_name, aliases, generic_aliases, &method_type_params)
                } else {
                    None
                };
                env.push(m_scope);
                for st in m.body.iter() {
                    check_stmt_types(
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
                    )?;
                }
                env.pop();
            }
            Ok(())
        }
        StmtKind::Block(bs) => {
            env.push(FastMap::default());
            for b in bs {
                check_stmt_types(
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
                )?;
            }
            env.pop();
            Ok(())
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let ct = infer_expr_type(
                cond,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            if !(ct == Ty::Any || is_subtype(&ct, &Ty::Bool)) {
                return Err(format!("condition expected Bool, found {}", ct));
            }

            // Flow-sensitive narrowing: create cloned envs for then/else branches
            let mut then_env = env.clone();
            apply_narrowing_to_env(cond, &mut then_env, true);
            check_stmt_types(
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
            )?;

            if let Some(e) = else_branch {
                let mut else_env = env.clone();
                apply_narrowing_to_env(cond, &mut else_env, false);
                check_stmt_types(
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
                )?;
            }
            Ok(())
        }
        StmtKind::While { cond, body } => {
            let ct = infer_expr_type(
                cond,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            if !(ct == Ty::Any || is_subtype(&ct, &Ty::Bool)) {
                return Err(format!("while condition expected Bool, found {}", ct));
            }
            // body is in-loop
            check_stmt_types(
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
            )
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
            )?;
            let loop_var_ty = match &it {
                Ty::Array(elem) => (**elem).clone(),
                Ty::Ptr(elem) | Ty::PtrOwning(elem) | Ty::PtrShared(elem) | Ty::PtrMut(elem) => {
                    (**elem).clone()
                }
                Ty::Str => Ty::Str,
                Ty::Map(k, _) => (**k).clone(),
                Ty::GenericInstance { name: gname, args }
                    if (gname == "Set" || gname == "set") && !args.is_empty() =>
                {
                    args[0].clone()
                }
                _ => match &iter.kind {
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
            };
            env.push(FastMap::default());
            env.last_mut()
                .ok_or("Internal error: scope stack empty")?
                .insert(name.clone(), loop_var_ty);
            // body is in-loop
            check_stmt_types(
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
            )?;
            env.pop();
            Ok(())
        }
        StmtKind::Break => {
            if !in_loop {
                return Err("'break' used outside of loop".into());
            }
            Ok(())
        }
        StmtKind::Continue => {
            if !in_loop {
                return Err("'continue' used outside of loop".into());
            }
            Ok(())
        }
        StmtKind::Jump(expr) => {
            if !in_loop {
                return Err("'jump' used outside of loop".into());
            }
            let jt = infer_expr_type(
                expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            if !is_numeric(&jt) {
                return Err("'jump' expects numeric expression".into());
            }
            Ok(())
        }
        StmtKind::Return(opt) => {
            if let Some(e) = opt {
                let rt = infer_expr_type(
                    e,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
                if let Some(expected) = &expected_ret {
                    let type_ok = expr_matches_expected_type(e, &rt, expected);

                    if !type_ok && *expected != Ty::Any {
                        return Err(format!(
                            "return type mismatch: expected {}, found {}",
                            expected, rt
                        ));
                    }
                }
                Ok(())
            } else {
                // returning nothing (implicit null)
                if let Some(expected) = &expected_ret {
                    if *expected != Ty::Any && !is_subtype(&Ty::Null, expected) {
                        return Err(format!(
                            "return type mismatch: expected {}, found null",
                            expected
                        ));
                    }
                }
                Ok(())
            }
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            check_stmt_types(
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
            )?;
            check_stmt_types(
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
            )
        }
        StmtKind::Import { .. } => Ok(()),
        StmtKind::ImportDefault { .. } => Ok(()),
        StmtKind::ImportNames { .. } => Ok(()),
        StmtKind::ExportDefault(_name) => Ok(()),
        StmtKind::ExportDefaultFunction(_f) => Ok(()),
        StmtKind::ExportDefaultClass(_c) => Ok(()),
        StmtKind::TypeAlias(_, _) => Ok(()),
        StmtKind::LetTuple(names, type_anns, init, _, _, _is_readonly) => {
            // Init must be present for tuple destructuring
            let init_expr = match init {
                Some(expr) => expr,
                None => {
                    return Err("tuple destructuring requires an initializer".to_string());
                }
            };

            // Type check the initializer expression
            let init_type = infer_expr_type(
                init_expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;

            // Verify the initializer is a tuple type with matching arity
            match &init_type {
                Ty::Tuple(elem_types) => {
                    if elem_types.len() != names.len() {
                        return Err(format!(
                            "tuple destructuring: expected {} elements, found {}",
                            names.len(),
                            elem_types.len()
                        ));
                    }

                    // Add variables to scope with their types
                    if let Some(scope) = env.last_mut() {
                        for (i, name) in names.iter().enumerate() {
                            let elem_ty = &elem_types[i];

                            // Check if type annotation exists and matches
                            // type_anns is Option<Vec<String>>, so we need to access it properly
                            let ann = type_anns.as_ref().and_then(|anns| anns.get(i));
                            if let Some(ann_str) = ann {
                                if let Some(anno_ty) = resolve_type_name(
                                    ann_str,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                ) {
                                    if !is_subtype(elem_ty, &anno_ty) && anno_ty != Ty::Any {
                                        return Err(format!(
                                            "tuple element '{}': expected type {}, found {}",
                                            name, anno_ty, elem_ty
                                        ));
                                    }
                                    scope.insert(name.clone(), anno_ty);
                                } else {
                                    return Err(format!(
                                        "unknown type annotation '{}' for '{}'",
                                        ann_str, name
                                    ));
                                }
                            } else {
                                // No annotation - use inferred type
                                scope.insert(name.clone(), elem_ty.clone());
                            }
                        }
                    }
                }
                Ty::Array(elem_ty) => {
                    // Array destructuring - all elements have the same type
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
                                        return Err(format!(
                                            "array element '{}': expected type {}, found {}",
                                            name, anno_ty, elem_ty
                                        ));
                                    }
                                    scope.insert(name.clone(), anno_ty);
                                } else {
                                    return Err(format!(
                                        "unknown type annotation '{}' for '{}'",
                                        ann_str, name
                                    ));
                                }
                            } else {
                                scope.insert(name.clone(), (**elem_ty).clone());
                            }
                        }
                    }
                }
                Ty::Any => {
                    // Allow any for dynamic typing
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
                                    return Err(format!(
                                        "unknown type annotation '{}' for '{}'",
                                        ann_str, name
                                    ));
                                }
                            } else {
                                scope.insert(name.clone(), Ty::Any);
                            }
                        }
                    }
                }
                _ => {
                    return Err(format!(
                        "cannot destructure non-tuple/array type: {}",
                        init_type
                    ));
                }
            }

            Ok(())
        }
        StmtKind::LetObject(bindings, init, _, _, _is_readonly) => {
            // Init must be present for object destructuring
            let init_expr = match init {
                Some(expr) => expr,
                None => {
                    return Err("object destructuring requires an initializer".to_string());
                }
            };

            // Type check the initializer expression
            let _init_type = infer_expr_type(
                init_expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;

            // For object types, verify keys exist; otherwise allow Any
            // Add destructured variables to scope with Any type (we don't track object field types yet)
            if let Some(scope) = env.last_mut() {
                for (key, alias) in bindings.iter() {
                    let var_name = alias.as_ref().unwrap_or(key);
                    scope.insert(var_name.clone(), Ty::Any);
                }
            }

            Ok(())
        }
        // FFI: pass-through (type checking happens at exec time)
        StmtKind::HeaderImport { .. } => Ok(()),
        StmtKind::ExternFunction(_) => Ok(()),
        StmtKind::ExternBlock { .. } => Ok(()),
        StmtKind::Region { name: _, body } => check_stmt_types(
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
        ),
        StmtKind::UnsafeBlock(body) => check_stmt_types(
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
        ),
        StmtKind::Defer(body) => check_stmt_types(
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
        ),
        StmtKind::Decorator(_def, _is_export) => {
            // Decorator definitions don't need type checking here
            // They will be checked when applied
            Ok(())
        }
    }
}
