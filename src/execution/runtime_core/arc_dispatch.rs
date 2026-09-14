//! ARC method dispatch without cloning receiver ownership for introspection.

use crate::execution::runtime_core::interpreter::env::Env;
use crate::parsing::ast::{Expr, ExprKind, Value};

/// Invoke an ARC method on a variable binding without cloning `StrongRef` / `WeakRef`.
///
/// Variable reads clone `Value::Share`, which increments the strong count. For
/// introspection methods that must reflect language-level ownership, borrow the
/// live binding instead.
pub fn try_invoke_arc_method_on_receiver_expr(
    envs: &[Env],
    start_env: usize,
    obj_expr: &Expr,
    method: &str,
) -> Option<Result<Value, String>> {
    if let ExprKind::Variable(name) = &obj_expr.kind {
        return try_invoke_arc_method_on_variable(envs, start_env, name, method);
    }
    None
}

fn try_invoke_arc_method_on_variable(
    envs: &[Env],
    start_env: usize,
    name: &str,
    method: &str,
) -> Option<Result<Value, String>> {
    let mut c = Some(start_env);
    while let Some(id) = c {
        let env = envs.get(id)?;
        if let Some(v) = env.values.get(name) {
            return match v {
                Value::Share(sr) => Some(sr.invoke_method(method)),
                Value::Weak(wr) => Some(wr.invoke_method(method)),
                _ => None,
            };
        }
        c = env.enclosing;
    }
    None
}
