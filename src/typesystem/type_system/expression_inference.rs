//! Expression type inference logic.

use super::environment::{lookup_var, set_var};
use super::narrowing::is_numeric;
use super::resolution::{resolve_type_name, type_from_name};
use super::statement_checking::{check_stmt_types, expr_matches_expected_type};
use crate::parsing::ast::*;
use crate::typesystem::checker::{Ty, union_merge};
use crate::utils::collections::FastMap;

fn numeric_result_type(left: &Ty, right: &Ty) -> Option<Ty> {
    use Ty::*;

    if left == right {
        return Some(left.clone());
    }

    let left_is_numeric = matches!(
        left,
        Int | Float | U8 | U16 | U32 | U64 | U128 | I8 | I16 | I32 | I64 | I128 | F32 | F64Ty
    );
    let right_is_numeric = matches!(
        right,
        Int | Float | U8 | U16 | U32 | U64 | U128 | I8 | I16 | I32 | I64 | I128 | F32 | F64Ty
    );

    if !left_is_numeric || !right_is_numeric {
        return None;
    }

    match (left, right) {
        (Int, other)
            if matches!(
                other,
                Int | Float
                    | U8
                    | U16
                    | U32
                    | U64
                    | U128
                    | I8
                    | I16
                    | I32
                    | I64
                    | I128
                    | F32
                    | F64Ty
            ) =>
        {
            Some(other.clone())
        }
        (other, Int)
            if matches!(
                other,
                Int | Float
                    | U8
                    | U16
                    | U32
                    | U64
                    | U128
                    | I8
                    | I16
                    | I32
                    | I64
                    | I128
                    | F32
                    | F64Ty
            ) =>
        {
            Some(other.clone())
        }
        (F32, _) | (_, F32) => Some(F64Ty),
        (F64Ty, _) | (_, F64Ty) | (Float, _) | (_, Float) => Some(F64Ty),
        _ => Some(Int),
    }
}

/// Infer result type for element-wise / broadcast array arithmetic.
fn array_broadcast_binary(lt: &Ty, rt: &Ty) -> Option<Ty> {
    match (lt, rt) {
        (Ty::Array(le), Ty::Array(re)) => Some(Ty::Array(Box::new(
            numeric_result_type(le, re).unwrap_or(Ty::Any),
        ))),
        (Ty::Array(elem), scalar) if is_numeric(scalar) => Some(Ty::Array(Box::new(
            numeric_result_type(elem, scalar).unwrap_or_else(|| (**elem).clone()),
        ))),
        (scalar, Ty::Array(elem)) if is_numeric(scalar) => Some(Ty::Array(Box::new(
            numeric_result_type(scalar, elem).unwrap_or_else(|| (**elem).clone()),
        ))),
        (Ty::Array(elem), Ty::Unknown) | (Ty::Unknown, Ty::Array(elem)) => {
            Some(Ty::Array(Box::new((**elem).clone())))
        }
        (Ty::Unknown, Ty::Unknown) => Some(Ty::Array(Box::new(Ty::Any))),
        _ => None,
    }
}

pub(crate) fn infer_expr_type(
    e: &Expr,
    env: &mut Vec<FastMap<String, Ty>>,
    fns: &FastMap<String, Vec<Option<String>>>,
    fns_type_params: &FastMap<String, Vec<String>>,
    fns_ret_types: &FastMap<String, Option<String>>,
    aliases: &FastMap<String, Ty>,
    generic_aliases: &FastMap<String, TypeAliasDecl>,
    type_params: &FastMap<String, Ty>,
) -> Result<Ty, String> {
    match &e.kind {
        ExprKind::Literal(v) => match v {
            Value::Number(n) => {
                if n.fract() == 0.0 {
                    Ok(Ty::Int)
                } else {
                    // Rust-like default: unsuffixed decimal literals are f64.
                    Ok(Ty::F64Ty)
                }
            }
            Value::BigInt(_) => Ok(Ty::Int),
            Value::Str(_) => Ok(Ty::Str),
            Value::Bool(_) => Ok(Ty::Bool),
            Value::Null => Ok(Ty::Null),
            // Fixed-width integer types
            Value::U8(_) => Ok(Ty::U8),
            Value::U16(_) => Ok(Ty::U16),
            Value::U32(_) => Ok(Ty::U32),
            Value::U64(_) => Ok(Ty::U64),
            Value::U128(_) => Ok(Ty::U128),
            Value::I8(_) => Ok(Ty::I8),
            Value::I16(_) => Ok(Ty::I16),
            Value::I32(_) => Ok(Ty::I32),
            Value::I64(_) => Ok(Ty::I64),
            Value::I128(_) => Ok(Ty::I128),
            // Fixed-width float types
            Value::F32(_) => Ok(Ty::F32),
            Value::F64(_) => Ok(Ty::F64Ty),
            _ => Ok(Ty::Any),
        },
        ExprKind::Variable(name) => {
            let res = if name == "None" {
                Ok(Ty::Null)
            } else if let Some(local_ty) = lookup_var(env, name) {
                Ok(local_ty)
            } else if let Some(param_vec) = fns.get(name) {
                let mut params: Vec<Ty> = Vec::new();
                for p_opt in param_vec {
                    let p_ty = if let Some(p_str) = p_opt {
                        resolve_type_name(p_str, aliases, generic_aliases, type_params)
                            .unwrap_or(Ty::Any)
                    } else {
                        Ty::Any
                    };
                    params.push(p_ty);
                }
                let ret_ty = if let Some(Some(ret_str)) = fns_ret_types.get(name) {
                    resolve_type_name(ret_str, aliases, generic_aliases, type_params)
                        .unwrap_or(Ty::Any)
                } else {
                    Ty::Any
                };
                Ok(Ty::Func {
                    params,
                    ret: Box::new(ret_ty),
                })
            } else {
                Ok(Ty::Any)
            };
            res
        }
        ExprKind::Assign(name, rhs) => {
            let rt = infer_expr_type(
                rhs,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let mut final_ty = rt;
            if let Some(dt) = lookup_var(env, name.as_str()) {
                if dt != Ty::Any && dt != Ty::Unknown {
                    if !expr_matches_expected_type(rhs, &final_ty, &dt) {
                        return Err(format!(
                            "type mismatch assigning to '{}': expected {}, found {}",
                            name, dt, final_ty
                        ));
                    }
                    final_ty = dt;
                }
            }
            set_var(env, name.as_str(), final_ty.clone());
            Ok(final_ty)
        }
        ExprKind::AssignOp(target, _op, rhs) => {
            let rt = infer_expr_type(
                rhs,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            match &target.kind {
                ExprKind::Variable(name) => {
                    let mut final_ty = rt.clone();
                    if let Some(dt) = lookup_var(env, name) {
                        if dt != Ty::Any && dt != Ty::Unknown {
                            if !expr_matches_expected_type(rhs, &rt, &dt) {
                                return Err(format!(
                                    "type mismatch assigning to '{}': expected {}, found {}",
                                    name, dt, rt
                                ));
                            }
                            final_ty = dt;
                        }
                    }
                    set_var(env, name, final_ty.clone());
                    Ok(final_ty)
                }
                _ => Ok(rt),
            }
        }
        ExprKind::AssignTuple(names, rhs) => {
            let rt = infer_expr_type(
                rhs,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            // For tuple destructuring, infer element types if possible
            if let Ty::Tuple(elem_types) = &rt {
                for (i, name) in names.iter().enumerate() {
                    if let Some(elem_ty) = elem_types.get(i) {
                        set_var(env, name, elem_ty.clone());
                    } else {
                        set_var(env, name, Ty::Any);
                    }
                }
            } else {
                // Otherwise assign Any to all variables
                for name in names {
                    set_var(env, name, Ty::Any);
                }
            }
            Ok(rt)
        }
        ExprKind::AssignObject(bindings, rhs) => {
            let rt = infer_expr_type(
                rhs,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            // For object destructuring, assign Any type to all variables
            // (we don't track object field types in the type system yet)
            for (_key, alias) in bindings {
                let var_name = alias.as_ref().unwrap_or(_key);
                set_var(env, var_name, Ty::Any);
            }
            Ok(rt)
        }
        ExprKind::Await(r) => {
            let mut in_async = false;
            for scope in env.iter().rev() {
                if let Some(ty) = scope.get("__in_async__") {
                    if *ty == Ty::Bool {
                        in_async = true;
                    }
                    break;
                }
            }
            if !in_async {
                return Err("await is only valid inside async functions".to_string());
            }
            let inner_ty = infer_expr_type(
                r,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            match inner_ty {
                Ty::GenericInstance { name, args }
                    if (name == "Future" || name == "Task") && args.len() == 1 =>
                {
                    Ok(args[0].clone())
                }
                Ty::Any => Ok(Ty::Any),
                other => Err(format!(
                    "cannot await value of type `{}`\nexpected an awaitable asynchronous value such as:\n  Future<T>\nfound:\n  {}",
                    other, other
                )),
            }
        }
        ExprKind::Spawn(r) => {
            let inner_ty = infer_expr_type(
                r,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let t = match inner_ty {
                Ty::GenericInstance { name, args } if name == "Future" && args.len() == 1 => {
                    args[0].clone()
                }
                other => other,
            };
            Ok(Ty::GenericInstance {
                name: "Task".to_string(),
                args: vec![t],
            })
        }
        ExprKind::Unary(_op, r) => infer_expr_type(
            r,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            aliases,
            generic_aliases,
            type_params,
        ),
        ExprKind::Binary(l, op, r) => {
            let lt = infer_expr_type(
                l,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let rt = infer_expr_type(
                r,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let l_base = match &lt {
                Ty::Nullable(inner) => &**inner,
                other => other,
            };
            let r_base = match &rt {
                Ty::Nullable(inner) => &**inner,
                other => other,
            };

            match op {
                TokenKind::Plus => {
                    if *l_base == Ty::Any || *r_base == Ty::Any {
                        Ok(Ty::Any)
                    } else if let Some(t) = array_broadcast_binary(&lt, &rt) {
                        // Element-wise array addition / scalar broadcast
                        Ok(t)
                    } else if is_numeric(l_base) && is_numeric(r_base) {
                        Ok(numeric_result_type(l_base, r_base).unwrap_or(Ty::Float))
                    } else if *l_base == Ty::Str || *r_base == Ty::Str {
                        Ok(Ty::Str)
                    } else {
                        Err(format!("invalid operands to +: {} and {}", lt, rt))
                    }
                }
                TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::Ampersand
                | TokenKind::Pipe
                | TokenKind::Caret
                | TokenKind::ShiftLeft
                | TokenKind::ShiftRight => {
                    if *l_base == Ty::Any || *r_base == Ty::Any {
                        Ok(Ty::Any)
                    } else if let Some(t) = array_broadcast_binary(&lt, &rt) {
                        Ok(t)
                    } else if is_numeric(l_base) && is_numeric(r_base) {
                        Ok(numeric_result_type(l_base, r_base).unwrap_or(Ty::Float))
                    } else {
                        Err(format!("invalid numeric operands: {} and {}", lt, rt))
                    }
                }
                TokenKind::EqualEqual
                | TokenKind::BangEqual
                | TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::Less
                | TokenKind::LessEqual => Ok(Ty::Bool),
                _ => Ok(Ty::Any),
            }
        }
        ExprKind::Logical(l, _op, r) => {
            let _ = infer_expr_type(
                l,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let _ = infer_expr_type(
                r,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            Ok(Ty::Bool)
        }
        ExprKind::Grouping(g) => infer_expr_type(
            g,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            aliases,
            generic_aliases,
            type_params,
        ),
        ExprKind::NonNull(expr) => infer_expr_type(
            expr,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            aliases,
            generic_aliases,
            type_params,
        ),
        ExprKind::Array(es) => {
            if es.is_empty() {
                return Ok(Ty::Array(Box::new(Ty::Unknown)));
            }
            let mut acc: Option<Ty> = None;
            for x in es {
                let t = infer_expr_type(
                    x,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
                acc = Some(if let Some(cur) = acc {
                    union_merge(cur, t)
                } else {
                    t
                });
            }
            let elem_ty = match acc.unwrap_or(Ty::Unknown) {
                Ty::Int => Ty::I32,
                other => other,
            };
            Ok(Ty::Array(Box::new(elem_ty)))
        }
        ExprKind::Tuple(es) => {
            let mut types: Vec<Ty> = Vec::new();
            for x in es {
                types.push(infer_expr_type(
                    x,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?);
            }
            Ok(Ty::Tuple(types))
        }
        ExprKind::Object(kv) => {
            let mut fields = Vec::new();
            for (k, v) in kv {
                let vt = infer_expr_type(
                    v,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
                fields.push((k.clone(), vt));
            }
            Ok(Ty::Record {
                required: fields,
                optional: Vec::new(),
            })
        }
        ExprKind::StructLiteral(name, kv) => {
            let struct_ty = resolve_type_name(name, aliases, generic_aliases, type_params)
                .ok_or_else(|| format!("unknown struct type '{}'", name))?;

            let (required, optional) = match &struct_ty {
                Ty::Record { required, optional } => (required, optional),
                other => {
                    return Err(format!(
                        "'{}' is not a struct-like type (resolved as {})",
                        name, other
                    ));
                }
            };

            let mut expected_fields: FastMap<String, Ty> = FastMap::default();
            for (fname, fty) in required {
                expected_fields.insert(fname.clone(), fty.clone());
            }
            for (fname, fty) in optional {
                expected_fields.insert(fname.clone(), fty.clone());
            }

            let mut provided_fields: FastMap<String, bool> = FastMap::default();
            for (field_name, field_expr) in kv {
                let expected = expected_fields.get(field_name).ok_or_else(|| {
                    format!("unknown field '{}' for struct '{}'", field_name, name)
                })?;

                let inferred = infer_expr_type(
                    field_expr,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;

                if !expr_matches_expected_type(field_expr, &inferred, expected) {
                    return Err(format!(
                        "struct field '{}.{}' expected {}, found {}",
                        name, field_name, expected, inferred
                    ));
                }

                provided_fields.insert(field_name.clone(), true);
            }

            for (field_name, _field_ty) in required {
                if !provided_fields.contains_key(field_name) {
                    return Err(format!(
                        "missing required field '{}.{}' in struct literal",
                        name, field_name
                    ));
                }
            }

            Ok(struct_ty)
        }
        ExprKind::SetLiteral(es) => {
            if es.is_empty() {
                return Ok(Ty::Array(Box::new(Ty::Unknown)));
            }
            let mut acc: Option<Ty> = None;
            for x in es {
                let t = infer_expr_type(
                    x,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
                acc = Some(if let Some(cur) = acc {
                    union_merge(cur, t)
                } else {
                    t
                });
            }
            // represent set as array-of-T for now
            Ok(Ty::Array(Box::new(acc.unwrap_or(Ty::Unknown))))
        }
        ExprKind::Get(obj, _key) => {
            let _ot = infer_expr_type(
                obj,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            // Property access is dynamically typed for now
            Ok(Ty::Any)
        }
        ExprKind::OptGet(obj, _key) => {
            let _ot = infer_expr_type(
                obj,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            Ok(Ty::Any)
        }
        ExprKind::Index(target, index) => {
            let tt = infer_expr_type(
                target,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let _ = infer_expr_type(
                index,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            match tt {
                Ty::Array(elem)
                | Ty::Ptr(elem)
                | Ty::PtrOwning(elem)
                | Ty::PtrShared(elem)
                | Ty::PtrMut(elem) => Ok((*elem).clone()),
                Ty::Map(_k, v) => Ok(Ty::Nullable(v)),
                Ty::Str => Ok(Ty::Str),
                _ => Ok(Ty::Any),
            }
        }
        ExprKind::Set(obj, _key, val) => {
            let _ = infer_expr_type(
                obj,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let vt = infer_expr_type(
                val,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            Ok(vt)
        }
        ExprKind::Fn(params, body, _is_async) => {
            // Build a function-local scope and run body checks there. Also attempt
            // to infer the function's return type by collecting all `return` sites.
            let mut fn_scope = FastMap::default();
            if *_is_async {
                fn_scope.insert("__in_async__".to_string(), Ty::Bool);
            } else {
                fn_scope.insert("__in_async__".to_string(), Ty::Void);
            }
            for p in params {
                let name = p.0.clone();
                if let Some(tn) = &p.2 {
                    if let Some(tt) = resolve_type_name(tn, aliases, generic_aliases, type_params) {
                        fn_scope.insert(name, tt);
                    } else {
                        fn_scope.insert(name, Ty::Any);
                    }
                } else {
                    fn_scope.insert(name, Ty::Any);
                }
            }
            // create a cloned env for function-local checking
            let mut fn_env = env.clone();
            fn_env.push(fn_scope);
            // run checks on the body (no expected return provided here)
            // Dereference Arc to iterate
            for s in body.as_ref() {
                check_stmt_types(
                    s,
                    &mut fn_env,
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

            // helper to recursively collect return expression types from a statement list
            fn collect_return_types(
                stmts: &Vec<Stmt>,
                env: &mut Vec<FastMap<String, Ty>>,
                fns: &FastMap<String, Vec<Option<String>>>,
                fns_type_params: &FastMap<String, Vec<String>>,
                fns_ret_types: &FastMap<String, Option<String>>,
                acc: &mut Option<Ty>,
                aliases: &FastMap<String, Ty>,
                generic_aliases: &FastMap<String, TypeAliasDecl>,
                type_params: &FastMap<String, Ty>,
            ) -> Result<(), String> {
                for s in stmts {
                    match &s.kind {
                        StmtKind::Return(opt) => {
                            if let Some(e) = opt {
                                let t = infer_expr_type(
                                    e,
                                    env,
                                    fns,
                                    fns_type_params,
                                    fns_ret_types,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                )?;
                                *acc = Some(if let Some(cur) = acc.take() {
                                    union_merge(cur, t)
                                } else {
                                    t
                                });
                            } else {
                                let t = Ty::Null;
                                *acc = Some(if let Some(cur) = acc.take() {
                                    union_merge(cur, t)
                                } else {
                                    t
                                });
                            }
                        }
                        StmtKind::Block(bs) => {
                            collect_return_types(
                                bs,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                acc,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        StmtKind::If {
                            then_branch,
                            else_branch,
                            ..
                        } => {
                            collect_return_types(
                                &vec![*then_branch.clone()],
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                acc,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                            if let Some(e) = else_branch {
                                collect_return_types(
                                    &vec![*e.clone()],
                                    env,
                                    fns,
                                    fns_type_params,
                                    fns_ret_types,
                                    acc,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                )?;
                            }
                        }
                        StmtKind::While { body, .. } => {
                            collect_return_types(
                                &vec![*body.clone()],
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                acc,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        StmtKind::ForIn { body, .. } => {
                            collect_return_types(
                                &vec![*body.clone()],
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                acc,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        StmtKind::TryCatch {
                            try_block,
                            catch_block,
                            ..
                        } => {
                            collect_return_types(
                                &vec![*try_block.clone()],
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                acc,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                            collect_return_types(
                                &vec![*catch_block.clone()],
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                acc,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        // other statements don't contain returns we care about for now
                        _ => {}
                    }
                }
                Ok(())
            }

            let mut ret_acc: Option<Ty> = None;
            collect_return_types(
                body,
                &mut fn_env,
                fns,
                fns_type_params,
                fns_ret_types,
                &mut ret_acc,
                aliases,
                generic_aliases,
                type_params,
            )?;

            let ret_ty = ret_acc.unwrap_or(Ty::Any);
            // Build param types vector
            let mut param_tys: Vec<Ty> = Vec::new();
            for p in params {
                if let Some(tn) = &p.2 {
                    if let Some(tt) = resolve_type_name(tn, aliases, generic_aliases, type_params) {
                        param_tys.push(tt);
                    } else {
                        param_tys.push(Ty::Any);
                    }
                } else {
                    param_tys.push(Ty::Any);
                }
            }
            let final_ret_ty = if *_is_async {
                Ty::GenericInstance {
                    name: "Future".to_string(),
                    args: vec![ret_ty],
                }
            } else {
                ret_ty
            };
            Ok(Ty::Func {
                params: param_tys,
                ret: Box::new(final_ret_ty),
            })
        }
        ExprKind::Call(callee, args, call_type_args) => {
            // First check if the callee expression itself has a function type in the current context
            if let Ok(Ty::Func { params, ret }) = infer_expr_type(
                callee,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            ) {
                for (i, a) in args.iter().enumerate() {
                    let at = infer_expr_type(
                        a,
                        env,
                        fns,
                        fns_type_params,
                        fns_ret_types,
                        aliases,
                        generic_aliases,
                        type_params,
                    )?;
                    if let Some(expected_ty) = params.get(i) {
                        let type_ok = expr_matches_expected_type(a, &at, expected_ty);
                        if !type_ok {
                            return Err(format!(
                                "argument {} expects {}, found {}",
                                i + 1,
                                expected_ty,
                                at
                            ));
                        }
                    }
                }
                return Ok(*ret);
            }

            // For simple identifier callees, check param annotations when available
            match &callee.kind {
                ExprKind::Variable(name) => {
                    if name == "Some" {
                        if args.len() != 1 {
                            return Err(format!(
                                "arity mismatch: function '{}' expects {} args, found {}",
                                name,
                                1,
                                args.len()
                            ));
                        }
                        let inner_ty = infer_expr_type(
                            &args[0],
                            env,
                            fns,
                            fns_type_params,
                            fns_ret_types,
                            aliases,
                            generic_aliases,
                            type_params,
                        )?;
                        return Ok(Ty::Nullable(Box::new(inner_ty)));
                    }

                    if name == "input" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        if !call_type_args.is_empty() {
                            if let Some(resolved) = resolve_type_name(
                                &call_type_args[0],
                                aliases,
                                generic_aliases,
                                type_params,
                            ) {
                                return Ok(resolved);
                            }
                        }
                        return Ok(Ty::Str);
                    }

                    if name == "len" || name == "length" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Int);
                    }

                    if name == "range" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Array(Box::new(Ty::Int)));
                    }

                    if name == "clock" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::F64Ty);
                    }

                    if name == "print" || name == "println" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Void);
                    }

                    if name == "Set" || name == "set" || name == "makeSet" {
                        let mut elem_ty = Ty::Any;
                        if !call_type_args.is_empty() {
                            if let Some(resolved) = resolve_type_name(
                                &call_type_args[0],
                                aliases,
                                generic_aliases,
                                type_params,
                            ) {
                                elem_ty = resolved;
                            }
                        } else if !args.is_empty() {
                            let at = infer_expr_type(
                                &args[0],
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                            if let Ty::Array(inner) = at {
                                elem_ty = *inner;
                            }
                        }
                        return Ok(Ty::GenericInstance {
                            name: "Set".to_string(),
                            args: vec![elem_ty],
                        });
                    }

                    if name == "Dict" || name == "dict" || name == "Map" {
                        let mut k_ty = Ty::Str;
                        let mut v_ty = Ty::Any;
                        if call_type_args.len() >= 2 {
                            if let Some(rk) = resolve_type_name(
                                &call_type_args[0],
                                aliases,
                                generic_aliases,
                                type_params,
                            ) {
                                k_ty = rk;
                            }
                            if let Some(rv) = resolve_type_name(
                                &call_type_args[1],
                                aliases,
                                generic_aliases,
                                type_params,
                            ) {
                                v_ty = rv;
                            }
                        }
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Map(Box::new(k_ty), Box::new(v_ty)));
                    }

                    if name == "Array" || name == "List" || name == "array" || name == "list" {
                        let mut elem_ty = Ty::Any;
                        if !call_type_args.is_empty() {
                            if let Some(resolved) = resolve_type_name(
                                &call_type_args[0],
                                aliases,
                                generic_aliases,
                                type_params,
                            ) {
                                elem_ty = resolved;
                            }
                        } else if !args.is_empty() {
                            let mut acc: Option<Ty> = None;
                            for a in args {
                                let at = infer_expr_type(
                                    a,
                                    env,
                                    fns,
                                    fns_type_params,
                                    fns_ret_types,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                )?;
                                acc = Some(if let Some(cur) = acc {
                                    union_merge(cur, at)
                                } else {
                                    at
                                });
                            }
                            if let Some(t) = acc {
                                elem_ty = t;
                            }
                        } else {
                            elem_ty = Ty::Unknown;
                        }
                        return Ok(Ty::Array(Box::new(elem_ty)));
                    }

                    if name == "Tuple" || name == "tuple" {
                        let mut tuple_types = Vec::new();
                        for a in args {
                            let at = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                            tuple_types.push(at);
                        }
                        return Ok(Ty::Tuple(tuple_types));
                    }

                    if name == "int" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Int);
                    }

                    if name == "float" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::F64Ty);
                    }

                    if name == "str" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Str);
                    }

                    if name == "bool" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Bool);
                    }

                    if name == "None" {
                        if !args.is_empty() {
                            return Err(format!(
                                "arity mismatch: function '{}' expects {} args, found {}",
                                name,
                                0,
                                args.len()
                            ));
                        }
                        return Ok(Ty::Null);
                    }

                    if let Some(param_vec) = fns.get(name) {
                        if param_vec.len() != args.len() {
                            return Err(format!(
                                "arity mismatch: function '{}' expects {} args, found {}",
                                name,
                                param_vec.len(),
                                args.len()
                            ));
                        }

                        // build local type-parameter substitution if call-site type args provided
                        let mut local_type_params: FastMap<String, Ty> = type_params.clone();
                        if !call_type_args.is_empty() {
                            if let Some(tp_names) = fns_type_params.get(name) {
                                if tp_names.len() == call_type_args.len() {
                                    for (i, pname) in tp_names.iter().enumerate() {
                                        if let Some(resolved) = resolve_type_name(
                                            &call_type_args[i],
                                            aliases,
                                            generic_aliases,
                                            type_params,
                                        ) {
                                            local_type_params.insert(pname.clone(), resolved);
                                        }
                                    }
                                }
                            }
                        }

                        for (i, a) in args.iter().enumerate() {
                            let at = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                            if let Some(Some(expected_str)) = param_vec.get(i) {
                                if let Some(expected_ty) = resolve_type_name(
                                    expected_str,
                                    aliases,
                                    generic_aliases,
                                    &local_type_params,
                                ) {
                                    let type_ok = expr_matches_expected_type(a, &at, &expected_ty);

                                    if !type_ok {
                                        return Err(format!(
                                            "argument {} to '{}' expected {}, found {}",
                                            i + 1,
                                            name,
                                            expected_ty,
                                            at
                                        ));
                                    }
                                }
                            }
                        }

                        if let Some(ret_ann) = fns_ret_types.get(name) {
                            if let Some(rt_name) = ret_ann {
                                if let Some(rty) = resolve_type_name(
                                    rt_name,
                                    aliases,
                                    generic_aliases,
                                    &local_type_params,
                                ) {
                                    return Ok(rty);
                                }
                            }
                        }
                    }
                }
                ExprKind::Get(obj, key) => {
                    // Check if obj is Collections namespace and key is a constructor
                    if let ExprKind::Variable(mod_name) = &obj.kind {
                        if mod_name == "Collections"
                            || mod_name == "std:Collections"
                            || mod_name == "collections"
                        {
                            let mut gargs = Vec::new();
                            for tname in call_type_args {
                                if let Some(resolved) =
                                    resolve_type_name(tname, aliases, generic_aliases, type_params)
                                {
                                    gargs.push(resolved);
                                }
                            }
                            return Ok(Ty::GenericInstance {
                                name: key.clone(),
                                args: gargs,
                            });
                        }
                    }

                    // Infer obj type to check method calls on generic collection instances
                    if let Ok(obj_ty) = infer_expr_type(
                        obj,
                        env,
                        fns,
                        fns_type_params,
                        fns_ret_types,
                        aliases,
                        generic_aliases,
                        type_params,
                    ) {
                        if let Ty::GenericInstance {
                            name: _,
                            args: gen_args,
                        } = &obj_ty
                        {
                            if !gen_args.is_empty() {
                                let expected_ty = &gen_args[0];
                                if key == "push"
                                    || key == "insert"
                                    || key == "set"
                                    || key == "enqueue"
                                    || key == "push_back"
                                    || key == "push_front"
                                {
                                    let arg_to_check = if (key == "insert" || key == "set")
                                        && args.len() >= 2
                                    {
                                        &args[1]
                                    } else if !args.is_empty() {
                                        &args[0]
                                    } else {
                                        return Err(format!("arity mismatch for method '{}'", key));
                                    };

                                    let at = infer_expr_type(
                                        arg_to_check,
                                        env,
                                        fns,
                                        fns_type_params,
                                        fns_ret_types,
                                        aliases,
                                        generic_aliases,
                                        type_params,
                                    )?;

                                    if expected_ty != &Ty::Any
                                        && !expr_matches_expected_type(
                                            arg_to_check,
                                            &at,
                                            expected_ty,
                                        )
                                    {
                                        return Err(format!(
                                            "type mismatch for method '{}': expected {}, found {}",
                                            key, expected_ty, at
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    // --- FIX: Array methods that return a new array (append/push/pop/etc.) ---
                    // Previously these fell through to `Ok(Ty::Any)` at the end of the Call
                    // branch, causing `let a:[i32;6] = ...; a = a.append(x)` to fail with
                    // `expected [i32], found Any` in strict release builds.
                    // Handle both Ty::Array and GenericInstance (e.g. List<T>) uniformly.
                    if let Ok(obj_ty_for_arr) = infer_expr_type(
                        obj,
                        env,
                        fns,
                        fns_type_params,
                        fns_ret_types,
                        aliases,
                        generic_aliases,
                        type_params,
                    ) {
                        let elem_opt: Option<Ty> = match &obj_ty_for_arr {
                            Ty::Array(elem) => Some((**elem).clone()),
                            Ty::GenericInstance { args: gen_args, .. } if !gen_args.is_empty() => {
                                Some(gen_args[0].clone())
                            }
                            _ => None,
                        };
                        if let Some(elem_ty) = elem_opt {
                            // Methods that produce a new array of the same element type
                            // Note: `insert`/`remove`/`delete` are handled separately below
                            // for non-array collections (Set/Map) where they return Void/Bool.
                            // For Ty::Array we want them to return Array as well, but we keep
                            // the generic handling below to avoid breaking Set/Map.  Array-specific
                            // insert/remove returning Array is covered by the dedicated check after
                            // the contains/insert branches if needed.
                            if matches!(
                                key.as_str(),
                                "append"
                                    | "push"
                                    | "push_back"
                                    | "push_front"
                                    | "unshift"
                                    | "pop"
                                    | "shift"
                                    | "set_index"
                                    | "removeAt"
                                    | "extend"
                                    | "concat"
                                    | "slice"
                                    | "reverse"
                                    | "sort"
                                    | "clear"
                                    | "filter"
                                    | "map"
                                    | "flat"
                                    | "flatMap"
                            ) {
                                for a in args {
                                    let _ = infer_expr_type(
                                        a,
                                        env,
                                        fns,
                                        fns_type_params,
                                        fns_ret_types,
                                        aliases,
                                        generic_aliases,
                                        type_params,
                                    )?;
                                }
                                return Ok(Ty::Array(Box::new(elem_ty)));
                            }
                        }
                    }

                    if key == "contains" || key == "has" || key == "hasKey" || key == "includes" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Bool);
                    }

                    if key == "insert" || key == "add" || key == "set" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Void);
                    }

                    if key == "remove" || key == "delete" {
                        for a in args {
                            let _ = infer_expr_type(
                                a,
                                env,
                                fns,
                                fns_type_params,
                                fns_ret_types,
                                aliases,
                                generic_aliases,
                                type_params,
                            )?;
                        }
                        return Ok(Ty::Bool);
                    }

                    if key == "size" || key == "length" || key == "len" || key == "count" {
                        let ot = infer_expr_type(
                            obj,
                            env,
                            fns,
                            fns_type_params,
                            fns_ret_types,
                            aliases,
                            generic_aliases,
                            type_params,
                        )?;
                        if matches!(
                            ot,
                            Ty::Array(_)
                                | Ty::Str
                                | Ty::Map(_, _)
                                | Ty::Any
                                | Ty::Record { .. }
                                | Ty::Tuple(_)
                                | Ty::Unknown
                                | Ty::GenericInstance { .. }
                        ) {
                            return Ok(Ty::Int);
                        }
                    }
                    // Treat ClassName.method(...) as a static method call for type checking
                    if let ExprKind::Variable(class_name) = &obj.kind {
                        let mkey = format!("{}::{}", class_name, key);
                        if let Some(param_vec) = fns.get(&mkey) {
                            if param_vec.len() != args.len() {
                                return Err(format!(
                                    "arity mismatch: method '{}' expects {} args, found {}",
                                    mkey,
                                    param_vec.len(),
                                    args.len()
                                ));
                            }

                            // build local type-parameter substitution if call-site type args provided
                            let mut local_type_params: FastMap<String, Ty> = type_params.clone();
                            if !call_type_args.is_empty() {
                                if let Some(tp_names) = fns_type_params.get(&mkey) {
                                    if tp_names.len() == call_type_args.len() {
                                        for (i, pname) in tp_names.iter().enumerate() {
                                            if let Some(resolved) = resolve_type_name(
                                                &call_type_args[i],
                                                aliases,
                                                generic_aliases,
                                                type_params,
                                            ) {
                                                local_type_params.insert(pname.clone(), resolved);
                                            }
                                        }
                                    }
                                }
                            }

                            for (i, a) in args.iter().enumerate() {
                                let at = infer_expr_type(
                                    a,
                                    env,
                                    fns,
                                    fns_type_params,
                                    fns_ret_types,
                                    aliases,
                                    generic_aliases,
                                    type_params,
                                )?;
                                if let Some(Some(expected_str)) = param_vec.get(i) {
                                    if let Some(expected_ty) = resolve_type_name(
                                        expected_str,
                                        aliases,
                                        generic_aliases,
                                        &local_type_params,
                                    ) {
                                        let type_ok =
                                            expr_matches_expected_type(a, &at, &expected_ty);

                                        if !type_ok {
                                            return Err(format!(
                                                "argument {} to '{}' expected {}, found {}",
                                                i + 1,
                                                mkey,
                                                expected_ty,
                                                at
                                            ));
                                        }
                                    }
                                }
                            }

                            if let Some(ret_ann) = fns_ret_types.get(&mkey) {
                                if let Some(rt_name) = ret_ann {
                                    if let Some(rty) = resolve_type_name(
                                        rt_name,
                                        aliases,
                                        generic_aliases,
                                        &local_type_params,
                                    ) {
                                        return Ok(rty);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            for a in args {
                let _ = infer_expr_type(
                    a,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
            }
            Ok(Ty::Any)
        }
        ExprKind::OptCall(callee, args, _type_args) => {
            let _ = infer_expr_type(
                callee,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            for a in args {
                let _ = infer_expr_type(
                    a,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
            }
            Ok(Ty::Any)
        }
        ExprKind::New(ctor, args) => {
            let _ = infer_expr_type(
                ctor,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            for a in args {
                let _ = infer_expr_type(
                    a,
                    env,
                    fns,
                    fns_type_params,
                    fns_ret_types,
                    aliases,
                    generic_aliases,
                    type_params,
                )?;
            }
            Ok(Ty::Any)
        }
        ExprKind::Throw(v) => infer_expr_type(
            v,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            aliases,
            generic_aliases,
            type_params,
        ),
        ExprKind::Spread(expr) => {
            // Spread operator: infer the inner expression type
            let inner_ty = infer_expr_type(
                expr,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            match inner_ty {
                Ty::Array(elem_ty) => Ok(Ty::Array(elem_ty)),
                Ty::Tuple(elem_types) => Ok(Ty::Tuple(elem_types)),
                Ty::Any => Ok(Ty::Any),
                _ => Err(format!("Cannot spread type {}", inner_ty)),
            }
        }
        ExprKind::Range(start, end, _inclusive) => {
            // Range operator: both operands should be numeric
            let start_ty = infer_expr_type(
                start,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let end_ty = infer_expr_type(
                end,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;

            if !is_numeric(&start_ty) {
                return Err(format!("Range start must be numeric, found {}", start_ty));
            }
            if !is_numeric(&end_ty) {
                return Err(format!("Range end must be numeric, found {}", end_ty));
            }

            // Range produces an array of numbers
            Ok(Ty::Array(Box::new(Ty::Float)))
        }
        ExprKind::Try(expr) => infer_expr_type(
            expr,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            aliases,
            generic_aliases,
            type_params,
        ),
        ExprKind::Conditional(c, t, e2) => {
            let _ = infer_expr_type(
                c,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let tt = infer_expr_type(
                t,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            let et = infer_expr_type(
                e2,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            Ok(union_merge(tt, et))
        }
        ExprKind::Update(_is_inc, _is_post, target) => infer_expr_type(
            target,
            env,
            fns,
            fns_type_params,
            fns_ret_types,
            aliases,
            generic_aliases,
            type_params,
        ),
        ExprKind::Match(value, arms) => {
            let val_ty = infer_expr_type(
                value,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            if arms.is_empty() {
                Ok(Ty::Void)
            } else {
                let mut arm_tys = Vec::new();
                for (pat, expr) in arms {
                    env.push(FastMap::default());

                    // Bind pattern variables with narrowed types for Option-like matches.
                    if let Some(scope) = env.last_mut() {
                        match pat {
                            Pattern::Variable(name) => {
                                scope.insert(name.clone(), val_ty.clone());
                            }
                            Pattern::EnumVariant(variant, inner_patterns) => {
                                if variant == "Some" {
                                    if let Ty::Nullable(inner) = &val_ty {
                                        if inner_patterns.len() == 1 {
                                            if let Pattern::Variable(name) = &inner_patterns[0] {
                                                scope.insert(name.clone(), (**inner).clone());
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    arm_tys.push(infer_expr_type(
                        expr,
                        env,
                        fns,
                        fns_type_params,
                        fns_ret_types,
                        aliases,
                        generic_aliases,
                        type_params,
                    )?);

                    env.pop();
                }
                let mut acc = arm_tys[0].clone();
                for t in arm_tys.iter().skip(1) {
                    acc = union_merge(acc, t.clone());
                }
                Ok(acc)
            }
        }
        ExprKind::Format(inner, _spec) => {
            // Format expressions always return a string
            let _ = infer_expr_type(
                inner,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            Ok(Ty::Str)
        }
        ExprKind::Cast(inner, target_ty_ann) => {
            let _ = infer_expr_type(
                inner,
                env,
                fns,
                fns_type_params,
                fns_ret_types,
                aliases,
                generic_aliases,
                type_params,
            )?;
            Ok(
                resolve_type_name(target_ty_ann, aliases, generic_aliases, type_params)
                    .or_else(|| type_from_name(target_ty_ann))
                    .unwrap_or(Ty::Unknown),
            )
        }
    }
}
