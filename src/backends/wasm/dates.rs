use crate::parsing::ast::{Expr, ExprKind, Stmt, StmtKind};

pub(super) fn method_op(method: &str) -> Option<i32> {
    Some(match method {
        "now" => 0,
        "getTime" => 1,
        "getFullYear" => 2,
        "getMonth" => 3,
        "getDate" => 4,
        "getHours" => 5,
        "getMinutes" => 6,
        "getSeconds" => 7,
        "getMilliseconds" => 8,
        "toISOString" => 9,
        "getUTCFullYear" => 10,
        "getUTCMonth" => 11,
        "getUTCDate" => 12,
        "getUTCHours" => 13,
        "getUTCMinutes" => 14,
        "getUTCSeconds" => 15,
        "getUTCMilliseconds" => 16,
        _ => return None,
    })
}

fn expr_uses_dates(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name == "Date",
        ExprKind::Call(target, args, _)
        | ExprKind::OptCall(target, args, _)
        | ExprKind::New(target, args) => {
            expr_uses_dates(target) || args.iter().any(expr_uses_dates)
        }
        ExprKind::Binary(l, _, r)
        | ExprKind::Logical(l, _, r)
        | ExprKind::AssignOp(l, _, r)
        | ExprKind::Index(l, r)
        | ExprKind::Range(l, r, _) => expr_uses_dates(l) || expr_uses_dates(r),
        ExprKind::Conditional(a, b, c) => {
            expr_uses_dates(a) || expr_uses_dates(b) || expr_uses_dates(c)
        }
        ExprKind::Get(e, _)
        | ExprKind::OptGet(e, _)
        | ExprKind::Grouping(e)
        | ExprKind::Unary(_, e)
        | ExprKind::NonNull(e)
        | ExprKind::Spread(e)
        | ExprKind::Assign(_, e)
        | ExprKind::Format(e, _)
        | ExprKind::Cast(e, _)
        | ExprKind::Await(e)
        | ExprKind::Spawn(e)
        | ExprKind::Throw(e)
        | ExprKind::Try(e)
        | ExprKind::Update(_, _, e) => expr_uses_dates(e),
        ExprKind::Array(items) | ExprKind::Tuple(items) | ExprKind::SetLiteral(items) => {
            items.iter().any(expr_uses_dates)
        }
        ExprKind::Object(entries) | ExprKind::StructLiteral(_, entries) => {
            entries.iter().any(|(_, e)| expr_uses_dates(e))
        }
        ExprKind::Set(target, _, value) => expr_uses_dates(target) || expr_uses_dates(value),
        ExprKind::Fn(_, body, _) => body.iter().any(uses_dates),
        _ => false,
    }
}

pub(super) fn uses_dates(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Let(_, Some(e), ..) | StmtKind::ExprStmt(e) | StmtKind::Return(Some(e)) => {
            expr_uses_dates(e)
        }
        StmtKind::Block(stmts) => stmts.iter().any(uses_dates),
        StmtKind::Function(f, _) => f.body.iter().any(uses_dates),
        StmtKind::Class(c, _) => c
            .methods
            .iter()
            .chain(&c.static_methods)
            .any(|f| f.body.iter().any(uses_dates)),
        StmtKind::Extend(_, _, _, methods, _) => {
            methods.iter().any(|f| f.body.iter().any(uses_dates))
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            expr_uses_dates(cond)
                || uses_dates(then_branch)
                || else_branch.as_deref().is_some_and(uses_dates)
        }
        StmtKind::While { cond, body } => expr_uses_dates(cond) || uses_dates(body),
        StmtKind::ForIn { iter, body, .. } => expr_uses_dates(iter) || uses_dates(body),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => uses_dates(try_block) || uses_dates(catch_block),
        StmtKind::Defer(body) | StmtKind::UnsafeBlock(body) | StmtKind::Region { body, .. } => {
            uses_dates(body)
        }
        _ => false,
    }
}
