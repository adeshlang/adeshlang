//! Expression evaluation module coordinator
//!
//! This module provides the main `eval_expr()` dispatcher and coordinates
//! all expression evaluation sub-modules.

mod binary;
mod builtin_methods;
mod calls;
mod control;
mod functions;
mod helpers;
mod literals;
mod member_access;
mod unary;
mod variables;

pub(crate) use helpers::is_pure_user_fn;

use crate::parsing::ast::{Expr, ExprKind, Value};

use super::core::Exec;

impl Exec {
    /// Main expression evaluation dispatcher
    pub(in crate::execution::runtime_core) fn eval_expr(
        &mut self,
        e: &Expr,
    ) -> Result<Value, String> {
        match &e.kind {
            // Literals
            ExprKind::Literal(v) => self.eval_literal(v),
            ExprKind::Array(es) => self.eval_array(es),
            ExprKind::Tuple(es) => self.eval_tuple(es),
            ExprKind::SetLiteral(es) => self.eval_set_literal(es),
            ExprKind::Object(kv) => self.eval_object(kv),
            ExprKind::StructLiteral(name, kv) => self.eval_struct_literal(name, kv, &e.span),
            ExprKind::Range(start, end, inclusive) => self.eval_range(start, end, *inclusive),

            // Variables
            ExprKind::Variable(name) => self.eval_variable(name, &e.span),
            ExprKind::Assign(name, rhs) => self.eval_assign(name, rhs, &e.span),
            ExprKind::AssignOp(target, op, rhs) => self.eval_assign_op(target, op, rhs),
            ExprKind::AssignTuple(names, rhs) => self.eval_assign_tuple(names, rhs, &e.span),
            ExprKind::AssignObject(bindings, rhs) => {
                self.eval_assign_object(bindings, rhs, &e.span)
            }

            // Binary and Logical
            ExprKind::Binary(l, op, r) => self.eval_binary(l, op, r),
            ExprKind::Logical(l, op, r) => self.eval_logical(l, op, r),

            // Unary
            ExprKind::Unary(op, r) => self.eval_unary(op, r),

            // Calls
            ExprKind::Call(callee, args, _type_args) => self.eval_call(callee, args),
            ExprKind::New(ctor, args) => self.eval_new(ctor, args),

            // Member Access
            ExprKind::Get(obj, key) => self.eval_get(obj, key),
            ExprKind::Set(obj, key, v) => self.eval_set(obj, key, v),
            ExprKind::Index(target, index) => self.eval_index(target, index),

            // Control
            ExprKind::Throw(v) => self.eval_throw(v),
            ExprKind::Match(value, arms) => self.eval_match(value, arms),
            ExprKind::Format(inner, spec) => self.eval_format(inner, spec),
            ExprKind::Grouping(g) => self.eval_grouping(g),
            ExprKind::Spread(expr) => self.eval_spread(expr),
            ExprKind::Await(r) => self.eval_await(r),
            ExprKind::Spawn(r) => self.eval_spawn(r),

            // Functions
            ExprKind::Fn(params, body, is_async) => self.eval_fn(params, body, *is_async),

            _ => Ok(Value::Null),
        }
    }
}
