//! Expression Visitor Pattern
//!
//! Defines the ExpressionVisitor trait for traversing and evaluating expressions
//! using the visitor pattern, reducing direct recursion and improving modularity.

use crate::parsing::ast::{Expr, ExprKind, Pattern, Stmt, TokenKind, Value};
use std::sync::Arc;

/// Visitor trait for expression evaluation.
///
/// Implementations of this trait can traverse and process expression AST nodes
/// without deep recursion. The visitor pattern separates traversal logic from
/// evaluation logic, making it easier to:
/// - Add new expression types
/// - Optimize hot paths with iterative evaluation
/// - Test evaluation logic independently
/// - Implement multiple evaluation strategies (interpreter, compiler, analyzer)
pub trait ExpressionVisitor {
    /// The output type produced by visiting expressions
    type Output;

    /// Visit a literal value expression
    fn visit_literal(&mut self, value: &Value) -> Self::Output;

    /// Visit a variable reference
    fn visit_variable(&mut self, name: &str) -> Self::Output;

    /// Visit a variable assignment
    fn visit_assign(&mut self, name: &str, value: &Expr) -> Self::Output;

    /// Visit an assignment with operator (+=, -=, etc.)
    fn visit_assign_op(&mut self, target: &Expr, op: TokenKind, value: &Expr) -> Self::Output;

    /// Visit tuple/array destructuring assignment ([a, b] = expr)
    fn visit_assign_tuple(&mut self, names: &[String], value: &Expr) -> Self::Output;

    /// Visit object destructuring assignment ({x, y} = expr)
    fn visit_assign_object(
        &mut self,
        bindings: &[(String, Option<String>)],
        value: &Expr,
    ) -> Self::Output;

    /// Visit a unary operation (!, -, typeof, etc.)
    fn visit_unary(&mut self, op: TokenKind, expr: &Expr) -> Self::Output;

    /// Visit a binary operation (+, -, *, /, etc.)
    fn visit_binary(&mut self, left: &Expr, op: TokenKind, right: &Expr) -> Self::Output;

    /// Visit a logical operation (&&, ||)
    fn visit_logical(&mut self, left: &Expr, op: TokenKind, right: &Expr) -> Self::Output;

    /// Visit a grouping (parenthesized) expression
    fn visit_grouping(&mut self, expr: &Expr) -> Self::Output;

    /// Visit a function call
    fn visit_call(&mut self, callee: &Expr, args: &[Expr], named_args: &[String]) -> Self::Output;

    /// Visit an array literal
    fn visit_array(&mut self, elements: &[Expr]) -> Self::Output;

    /// Visit a tuple literal
    fn visit_tuple(&mut self, elements: &[Expr]) -> Self::Output;

    /// Visit an object literal
    fn visit_object(&mut self, fields: &[(String, Expr)]) -> Self::Output;

    /// Visit a struct literal
    fn visit_struct_literal(&mut self, name: &str, fields: &[(String, Expr)]) -> Self::Output;

    /// Visit a set literal
    fn visit_set_literal(&mut self, elements: &[Expr]) -> Self::Output;

    /// Visit property access (obj.prop)
    fn visit_get(&mut self, object: &Expr, property: &str) -> Self::Output;

    /// Visit property assignment (obj.prop = value)
    fn visit_set(&mut self, object: &Expr, property: &str, value: &Expr) -> Self::Output;

    /// Visit a function/lambda literal
    fn visit_fn(
        &mut self,
        params: &[(String, Option<Expr>, Option<String>)],
        body: &Arc<Vec<Stmt>>,
        is_async: bool,
    ) -> Self::Output;

    /// Visit a new expression (constructor call)
    fn visit_new(&mut self, callee: &Expr, args: &[Expr]) -> Self::Output;

    /// Visit an await expression
    fn visit_await(&mut self, expr: &Expr) -> Self::Output;

    /// Visit a spawn expression (async spawn)
    fn visit_spawn(&mut self, expr: &Expr) -> Self::Output;

    /// Visit a throw expression
    fn visit_throw(&mut self, expr: &Expr) -> Self::Output;

    /// Visit a spread operator (...expr)
    fn visit_spread(&mut self, expr: &Expr) -> Self::Output;

    /// Visit a range expression (start..end or start..=end)
    fn visit_range(&mut self, start: &Expr, end: &Expr, inclusive: bool) -> Self::Output;

    /// Visit an index expression (array[index])
    fn visit_index(&mut self, target: &Expr, index: &Expr) -> Self::Output;

    /// Visit a non-null assertion (expr!)
    fn visit_non_null(&mut self, expr: &Expr) -> Self::Output;

    /// Visit an optional property access (obj?.prop)
    fn visit_opt_get(&mut self, object: &Expr, property: &str) -> Self::Output;

    /// Visit an optional call (fn?.(args))
    fn visit_opt_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        named_args: &[String],
    ) -> Self::Output;

    /// Visit a try expression (expr?)
    fn visit_try(&mut self, expr: &Expr) -> Self::Output;

    /// Visit a conditional/ternary expression (cond ? then : else)
    fn visit_conditional(
        &mut self,
        cond: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
    ) -> Self::Output;

    /// Visit an update expression (++i, i++, --i, i--)
    fn visit_update(&mut self, is_prefix: bool, is_increment: bool, expr: &Expr) -> Self::Output;

    /// Visit a match expression
    fn visit_match(&mut self, expr: &Expr, arms: &[(Pattern, Expr)]) -> Self::Output;

    /// Visit a format expression (${expr:format})
    fn visit_format(&mut self, expr: &Expr, format_spec: &str) -> Self::Output;

    /// Visit a cast expression (expr as Type)
    fn visit_cast(&mut self, expr: &Expr, target_ty: &str) -> Self::Output;

    /// Main dispatch method that routes to the appropriate visit method
    ///
    /// This is the entry point for evaluating an expression using the visitor.
    /// It examines the expression kind and calls the corresponding visit method.
    fn visit_expr(&mut self, expr: &Expr) -> Self::Output {
        match &expr.kind {
            ExprKind::Literal(v) => self.visit_literal(v),
            ExprKind::Variable(name) => self.visit_variable(name),
            ExprKind::Assign(name, value) => self.visit_assign(name, value),
            ExprKind::AssignOp(target, op, value) => self.visit_assign_op(target, *op, value),
            ExprKind::AssignTuple(names, value) => self.visit_assign_tuple(names, value),
            ExprKind::AssignObject(bindings, value) => self.visit_assign_object(bindings, value),
            ExprKind::Unary(op, expr) => self.visit_unary(*op, expr),
            ExprKind::Binary(left, op, right) => self.visit_binary(left, *op, right),
            ExprKind::Logical(left, op, right) => self.visit_logical(left, *op, right),
            ExprKind::Grouping(expr) => self.visit_grouping(expr),
            ExprKind::Call(callee, args, named) => self.visit_call(callee, args, named),
            ExprKind::Array(elements) => self.visit_array(elements),
            ExprKind::Tuple(elements) => self.visit_tuple(elements),
            ExprKind::Object(fields) => self.visit_object(fields),
            ExprKind::StructLiteral(name, fields) => self.visit_struct_literal(name, fields),
            ExprKind::SetLiteral(elements) => self.visit_set_literal(elements),
            ExprKind::Get(object, prop) => self.visit_get(object, prop),
            ExprKind::Set(object, prop, value) => self.visit_set(object, prop, value),
            ExprKind::Fn(params, body, is_async) => self.visit_fn(params, body, *is_async),
            ExprKind::New(callee, args) => self.visit_new(callee, args),
            ExprKind::Await(expr) => self.visit_await(expr),
            ExprKind::Spawn(expr) => self.visit_spawn(expr),
            ExprKind::Throw(expr) => self.visit_throw(expr),
            ExprKind::Spread(expr) => self.visit_spread(expr),
            ExprKind::Range(start, end, inclusive) => self.visit_range(start, end, *inclusive),
            ExprKind::Index(target, index) => self.visit_index(target, index),
            ExprKind::NonNull(expr) => self.visit_non_null(expr),
            ExprKind::OptGet(object, prop) => self.visit_opt_get(object, prop),
            ExprKind::OptCall(callee, args, named) => self.visit_opt_call(callee, args, named),
            ExprKind::Try(expr) => self.visit_try(expr),
            ExprKind::Conditional(cond, then_expr, else_expr) => {
                self.visit_conditional(cond, then_expr, else_expr)
            }
            ExprKind::Update(is_prefix, is_incr, expr) => {
                self.visit_update(*is_prefix, *is_incr, expr)
            }
            ExprKind::Match(expr, arms) => self.visit_match(expr, arms),
            ExprKind::Format(expr, spec) => self.visit_format(expr, spec),
            ExprKind::Cast(expr, target_ty) => self.visit_cast(expr, target_ty),
        }
    }
}
