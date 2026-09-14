//! AST validation module
//!
//! This module provides validation functions for AST nodes,
//! including heap allocation detection for embedded mode.

use crate::cli::RuntimeConfig;
use crate::parsing::ast::{ClassDecl, DecoratorPhase, Expr, ExprKind, Function, Stmt, StmtKind};

pub fn expr_contains_heap_alloc(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Call(callee, args, _) => {
            matches!(callee.kind, ExprKind::Variable(ref name) if name == "alloc")
                || expr_contains_heap_alloc(callee)
                || args.iter().any(expr_contains_heap_alloc)
        }
        ExprKind::Assign(_, rhs)
        | ExprKind::AssignTuple(_, rhs)
        | ExprKind::AssignObject(_, rhs)
        | ExprKind::Unary(_, rhs)
        | ExprKind::Grouping(rhs)
        | ExprKind::Await(rhs)
        | ExprKind::Spawn(rhs)
        | ExprKind::Throw(rhs)
        | ExprKind::Spread(rhs)
        | ExprKind::Try(rhs)
        | ExprKind::NonNull(rhs)
        | ExprKind::Format(rhs, _)
        | ExprKind::Cast(rhs, _) => expr_contains_heap_alloc(rhs),

        ExprKind::AssignOp(lhs, _, rhs)
        | ExprKind::Binary(lhs, _, rhs)
        | ExprKind::Logical(lhs, _, rhs)
        | ExprKind::Range(lhs, rhs, _)
        | ExprKind::Index(lhs, rhs) => {
            expr_contains_heap_alloc(lhs) || expr_contains_heap_alloc(rhs)
        }

        ExprKind::Conditional(cond, then_expr, else_expr) => {
            expr_contains_heap_alloc(cond)
                || expr_contains_heap_alloc(then_expr)
                || expr_contains_heap_alloc(else_expr)
        }
        ExprKind::Update(_, _, value) => expr_contains_heap_alloc(value),

        ExprKind::Array(items) | ExprKind::Tuple(items) | ExprKind::SetLiteral(items) => {
            items.iter().any(expr_contains_heap_alloc)
        }
        ExprKind::Object(kvs) | ExprKind::StructLiteral(_, kvs) => {
            kvs.iter().any(|(_, v)| expr_contains_heap_alloc(v))
        }
        ExprKind::Get(target, _) | ExprKind::OptGet(target, _) => expr_contains_heap_alloc(target),
        ExprKind::Set(target, _, value) => {
            expr_contains_heap_alloc(target) || expr_contains_heap_alloc(value)
        }
        ExprKind::Fn(params, body, _) => {
            params
                .iter()
                .filter_map(|(_, default, _)| default.as_ref())
                .any(expr_contains_heap_alloc)
                || body.iter().any(stmt_contains_heap_alloc)
        }
        ExprKind::New(target, args) => {
            expr_contains_heap_alloc(target) || args.iter().any(expr_contains_heap_alloc)
        }
        ExprKind::OptCall(target, args, _) => {
            expr_contains_heap_alloc(target) || args.iter().any(expr_contains_heap_alloc)
        }
        ExprKind::Match(scrut, arms) => {
            expr_contains_heap_alloc(scrut)
                || arms
                    .iter()
                    .any(|(_, arm_expr)| expr_contains_heap_alloc(arm_expr))
        }
        ExprKind::Literal(_) | ExprKind::Variable(_) => false,
    }
}

pub fn function_contains_heap_alloc(fun: &Function) -> bool {
    fun.params
        .iter()
        .filter_map(|(_, default, _)| default.as_ref())
        .any(expr_contains_heap_alloc)
        || fun.body.iter().any(stmt_contains_heap_alloc)
        || fun.decorators.iter().any(expr_contains_heap_alloc)
}

pub fn class_contains_heap_alloc(class: &ClassDecl) -> bool {
    class.decorators.iter().any(expr_contains_heap_alloc)
        || class.methods.iter().any(function_contains_heap_alloc)
        || class
            .static_methods
            .iter()
            .any(function_contains_heap_alloc)
        || class.static_properties.iter().any(|(_, expr, args)| {
            expr_contains_heap_alloc(expr) || args.iter().any(expr_contains_heap_alloc)
        })
}

pub fn stmt_contains_heap_alloc(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Let(_, init, _, _, _, _) => init.as_ref().is_some_and(expr_contains_heap_alloc),
        StmtKind::ShareDeclaration(decl, _) => expr_contains_heap_alloc(&decl.expr),
        StmtKind::StrongDeclaration(decl, _) => expr_contains_heap_alloc(&decl.expr),
        StmtKind::WeakDeclaration(decl, _) => expr_contains_heap_alloc(&decl.expr),
        StmtKind::LetTuple(_, _, init, _, _, _) => {
            init.as_ref().is_some_and(expr_contains_heap_alloc)
        }
        StmtKind::LetObject(_, init, _, _, _) => {
            init.as_ref().is_some_and(expr_contains_heap_alloc)
        }
        StmtKind::ExprStmt(expr) => expr_contains_heap_alloc(expr),
        StmtKind::Block(stmts) => stmts.iter().any(stmt_contains_heap_alloc),
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            expr_contains_heap_alloc(cond)
                || stmt_contains_heap_alloc(then_branch)
                || else_branch.as_deref().is_some_and(stmt_contains_heap_alloc)
        }
        StmtKind::While { cond, body } => {
            expr_contains_heap_alloc(cond) || stmt_contains_heap_alloc(body)
        }
        StmtKind::ForIn { iter, body, .. } => {
            expr_contains_heap_alloc(iter) || stmt_contains_heap_alloc(body)
        }
        StmtKind::Jump(expr) => expr_contains_heap_alloc(expr),
        StmtKind::Function(fun, _) | StmtKind::ExportDefaultFunction(fun) => {
            function_contains_heap_alloc(fun)
        }
        StmtKind::Class(class, _) | StmtKind::ExportDefaultClass(class) => {
            class_contains_heap_alloc(class)
        }
        StmtKind::Return(expr_opt) => expr_opt.as_ref().is_some_and(expr_contains_heap_alloc),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => stmt_contains_heap_alloc(try_block) || stmt_contains_heap_alloc(catch_block),
        StmtKind::Region { body, .. } | StmtKind::UnsafeBlock(body) | StmtKind::Defer(body) => {
            stmt_contains_heap_alloc(body)
        }
        StmtKind::Extend(_, _, methods, _) => methods.iter().any(function_contains_heap_alloc),
        StmtKind::TypeAlias(_, _)
        | StmtKind::Struct(_, _)
        | StmtKind::Enum(_, _)
        | StmtKind::Interface(_, _) => false,
        StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Import { .. }
        | StmtKind::ImportDefault { .. }
        | StmtKind::ImportNames { .. }
        | StmtKind::HeaderImport { .. }
        | StmtKind::ExternFunction(_)
        | StmtKind::ExternBlock { .. }
        | StmtKind::ExportDefault(_) => false,
        StmtKind::Decorator(def, _) => {
            // Check if decorator phases contain heap allocations
            def.phases.iter().any(|phase| match phase {
                DecoratorPhase::Compile(body)
                | DecoratorPhase::Runtime(body)
                | DecoratorPhase::Typecheck(body)
                | DecoratorPhase::Emit(body) => body.iter().any(stmt_contains_heap_alloc),
            })
        }
    }
}

pub fn ast_contains_heap_alloc(ast: &[Stmt]) -> bool {
    ast.iter().any(stmt_contains_heap_alloc)
}

pub fn ensure_embedded_no_heap(src: &str, config: &RuntimeConfig) -> Result<(), String> {
    if !config.embedded {
        return Ok(());
    }

    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    let body = super::directives::strip_compile_directive(src);
    let mut lexer = Lexer::new(&body);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Lexer error: {}", e))?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser
        .parse_program()
        .map_err(|e| format!("Parser error: {}", e))?;

    if ast_contains_heap_alloc(&ast) {
        return Err(
            "Embedded mode forbids heap allocation via alloc(); use stack/arena/region instead."
                .to_string(),
        );
    }

    Ok(())
}
