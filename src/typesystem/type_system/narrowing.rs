//! Type narrowing utilities for flow-sensitive type refinement.

use super::environment::{lookup_var, set_var};
use crate::parsing::ast::*;
use crate::typesystem::checker::{Ty, narrow_on_null};
use crate::utils::collections::FastMap;

pub(crate) fn remove_choice(var_ty: &Ty, to_remove: &Ty) -> Ty {
    use Ty::*;
    match var_ty {
        Union(choices) => {
            let filtered: Vec<Ty> = choices.iter().cloned().filter(|c| c != to_remove).collect();
            if filtered.is_empty() {
                Ty::Any
            } else if filtered.len() == 1 {
                filtered.into_iter().next().unwrap_or(Ty::Any)
            } else {
                Union(filtered)
            }
        }
        other => {
            if other == to_remove {
                Ty::Any
            } else {
                other.clone()
            }
        }
    }
}

/// Apply simple narrowing rules to the provided environment based on the condition expression.
/// `positive=true` means the branch where the condition holds; `false` is the negated branch.
pub(crate) fn apply_narrowing_to_env(
    cond: &Expr,
    env: &mut Vec<FastMap<String, Ty>>,
    positive: bool,
) {
    use Value::*;
    match &cond.kind {
        ExprKind::Binary(left, op, right) => {
            match op {
                TokenKind::EqualEqual | TokenKind::BangEqual => {
                    let is_eq = matches!(*op, TokenKind::EqualEqual);
                    // target branch polarity: when positive==true and op==EqualEqual, condition holds
                    let branch_true = positive == is_eq;

                    // null comparisons: var == null
                    if let (ExprKind::Variable(name), ExprKind::Literal(Null)) =
                        (&left.kind, &right.kind)
                    {
                        // use narrow_on_null helper to update variable type
                        if branch_true {
                            let cur = lookup_var(env, name).unwrap_or(Ty::Any);
                            let narrowed = narrow_on_null(&cur, true);
                            set_var(env, name, narrowed);
                        } else if let Some(cur) = lookup_var(env, name) {
                            let narrowed = narrow_on_null(&cur, false);
                            set_var(env, name, narrowed);
                        }
                        return;
                    }

                    // typeof(x) == "string" pattern: left is Call(Variable("typeof"), [arg]) or reversed
                    let maybe_call = match (&left.kind, &right.kind) {
                        (ExprKind::Call(callee, args, _), ExprKind::Literal(Value::Str(s))) => {
                            Some((callee, args, s.clone()))
                        }
                        (ExprKind::Literal(Value::Str(s)), ExprKind::Call(callee, args, _)) => {
                            Some((callee, args, s.clone()))
                        }
                        _ => None,
                    };
                    if let Some((callee, args, s)) = maybe_call {
                        if let ExprKind::Variable(fn_name) = &callee.kind {
                            if fn_name == "typeof" && args.len() == 1 {
                                if let ExprKind::Variable(varname) = &args[0].kind {
                                    // map string to Ty
                                    let target_ty = match s.as_str() {
                                        "string" => Ty::Str,
                                        "number" => Ty::Float,
                                        "int" => Ty::Int,
                                        "bool" | "boolean" => Ty::Bool,
                                        "null" => Ty::Null,
                                        _ => Ty::Any,
                                    };
                                    if branch_true {
                                        set_var(env, varname, target_ty);
                                    } else {
                                        if let Some(cur) = lookup_var(env, varname) {
                                            let newt = remove_choice(&cur, &target_ty);
                                            set_var(env, varname, newt);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // instanceof operator for type narrowing (x instanceof Type)
                TokenKind::Instanceof => {
                    if let ExprKind::Variable(var_name) = &left.kind {
                        if let ExprKind::Variable(type_name) = &right.kind {
                            // Map type name to Ty
                            let target_ty = match type_name.as_str() {
                                "String" | "string" => Ty::Str,
                                "Number" | "number" => Ty::Float,
                                "Int" | "int" => Ty::Int,
                                "Bool" | "bool" | "boolean" => Ty::Bool,
                                "Null" | "null" => Ty::Null,
                                "Array" => Ty::Array(Box::new(Ty::Any)),
                                _ => Ty::Any,
                            };

                            if positive {
                                // In true branch: variable has the target type
                                set_var(env, var_name, target_ty);
                            } else {
                                // In false branch: remove target type from variable's type
                                if let Some(cur) = lookup_var(env, var_name) {
                                    let narrowed = remove_choice(&cur, &target_ty);
                                    set_var(env, var_name, narrowed);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // Logical AND: narrow to intersection of both conditions
        ExprKind::Logical(left, op, right) if matches!(*op, TokenKind::AndAnd) => {
            if positive {
                // Both conditions must hold: apply both narrowings
                apply_narrowing_to_env(left, env, true);
                apply_narrowing_to_env(right, env, true);
            } else {
                // At least one is false: create union of negated branches
                // For simplicity, we don't narrow in the negative case for AND
                // (full implementation would need path splitting)
            }
        }
        // Logical OR: narrow to union of conditions
        ExprKind::Logical(left, op, right) if matches!(*op, TokenKind::OrOr) => {
            if positive {
                // At least one is true: create union of both branches
                // For simplicity, we don't narrow in the positive case for OR
                // (full implementation would need path splitting)
            } else {
                // Both must be false: apply both negated narrowings
                apply_narrowing_to_env(left, env, false);
                apply_narrowing_to_env(right, env, false);
            }
        }
        _ => {}
    }
}

pub(crate) fn is_numeric(t: &Ty) -> bool {
    matches!(
        t,
        Ty::Int | Ty::Float | Ty::Any |
        // Fixed-width integer types
        Ty::U8 | Ty::U16 | Ty::U32 | Ty::U64 | Ty::U128 |
        Ty::I8 | Ty::I16 | Ty::I32 | Ty::I64 | Ty::I128 |
        // Fixed-width float types
        Ty::F32 | Ty::F64Ty
    )
}
