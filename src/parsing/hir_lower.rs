//! AST to HIR transformation pass
//!
//! This module provides the `ast_to_hir` function that transforms
//! the AST into HIR representation.

use std::sync::Arc;

use super::ast::{ClassDecl, Expr, ExprKind, Function, Pattern, Stmt, StmtKind, TokenKind, Value};
use super::hir::{
    ArrayKind, BinOp, HirClass, HirExpr, HirFunction, HirLiteral, HirMethod, HirModule, HirPattern,
    HirStmt, HirType, UnaryOp,
};

/// Transform an AST program into an HIR module
///
/// # Parameters
/// * `stmts` - The AST statements to transform
/// * `include_tests` - Whether to include test functions (false for production builds)
pub fn ast_to_hir(stmts: &[Stmt], include_tests: bool) -> Result<HirModule, String> {
    let mut module = HirModule::new();

    for stmt in stmts {
        match lower_stmt(stmt, include_tests)? {
            LoweredStmt::Stmt(s) => module.statements.push(s),
            LoweredStmt::Function(f) => module.functions.push(f),
            LoweredStmt::Class(c) => module.classes.push(c),
            LoweredStmt::Skip => {} // Skip test functions in production builds
        }
    }

    Ok(module)
}

enum LoweredStmt {
    Stmt(HirStmt),
    Function(HirFunction),
    Class(HirClass),
    Skip, // For test functions that should be excluded
}

fn lower_stmt(stmt: &Stmt, include_tests: bool) -> Result<LoweredStmt, String> {
    match &stmt.kind {
        StmtKind::ShareDeclaration(decl, _) => {
            let inner_expr = lower_expr(&decl.expr)?;
            // Wrap in Share so the ownership checker knows this is an ARC
            // initialisation — the source value is NOT moved.
            let init_expr = HirExpr::Share(Box::new(inner_expr));
            let ty = decl.type_ann.as_ref().map(|t| type_from_annotation(t));
            Ok(LoweredStmt::Stmt(HirStmt::Let {
                name: decl.name.clone(),
                ty,
                init: Some(init_expr),
                is_const: false,
                is_borrowed: None,
            }))
        }
        StmtKind::StrongDeclaration(decl, _) => {
            let inner_expr = lower_expr(&decl.expr)?;
            // Creating a strong ARC ref clones/retains the ARC handle
            let init_expr = HirExpr::Share(Box::new(inner_expr));
            let ty = decl.type_ann.as_ref().map(|t| type_from_annotation(t));
            Ok(LoweredStmt::Stmt(HirStmt::Let {
                name: decl.name.clone(),
                ty,
                init: Some(init_expr),
                is_const: false,
                is_borrowed: Some(false),
            }))
        }
        StmtKind::WeakDeclaration(decl, _) => {
            let inner_expr = lower_expr(&decl.expr)?;
            // Creating a weak ARC ref downgrades the strong reference to a weak handle
            let init_expr = HirExpr::Downgrade(Box::new(inner_expr));
            let ty = decl.type_ann.as_ref().map(|t| type_from_annotation(t));
            Ok(LoweredStmt::Stmt(HirStmt::Let {
                name: decl.name.clone(),
                ty,
                init: Some(init_expr),
                is_const: false,
                is_borrowed: Some(false),
            }))
        }
        StmtKind::Let(name, init, ty_ann, _export, is_const, _is_readonly) => {
            let init_expr = init.as_ref().map(|e| lower_expr(e)).transpose()?;
            let ty = ty_ann.as_ref().map(|t| type_from_annotation(t));
            Ok(LoweredStmt::Stmt(HirStmt::Let {
                name: name.clone(),
                ty,
                init: init_expr,
                is_const: *is_const,
                is_borrowed: None,
            }))
        }

        StmtKind::LetTuple(names, _type_anns, init, _export, is_const, _is_readonly) => {
            let init_expr = init.as_ref().map(|e| lower_expr(e)).transpose()?;
            Ok(LoweredStmt::Stmt(HirStmt::LetTuple {
                names: names.clone(),
                init: init_expr,
                is_const: *is_const,
            }))
        }

        StmtKind::ExprStmt(expr) => {
            let hir_expr = lower_expr(expr)?;
            // Check if it's a throw expression - convert to Throw statement
            if let ExprKind::Throw(inner) = &expr.kind {
                let hir_inner = lower_expr(inner)?;
                Ok(LoweredStmt::Stmt(HirStmt::Throw(hir_inner)))
            // Check if it's an assignment expression
            } else if let ExprKind::Assign(name, value) = &expr.kind {
                let hir_value = lower_expr(value)?;
                Ok(LoweredStmt::Stmt(HirStmt::Assign {
                    target: HirExpr::LoadVar(name.clone()),
                    value: hir_value,
                    is_move: true,
                }))
            } else if let ExprKind::AssignOp(target, op, value) = &expr.kind {
                // Handle assignment expressions.
                // For ??= (nullish coalescing assignment), we need conditional logic.
                // For other compound assignments like `i += 1`, they are expanded to `i = i + 1`.
                let hir_value = lower_expr(value)?;

                // Handle ??= specially - only assign if target is null
                if matches!(op, TokenKind::NullCoalesceEqual) {
                    if let ExprKind::Variable(name) = &target.kind {
                        // Generate: if target == null { target = value }
                        let load_target = HirExpr::LoadVar(name.clone());
                        let null_check = HirExpr::BinaryOp(
                            Box::new(load_target),
                            BinOp::Eq,
                            Box::new(HirExpr::Literal(HirLiteral::Null)),
                        );
                        let assign_stmt = HirStmt::Assign {
                            target: HirExpr::LoadVar(name.clone()),
                            value: hir_value,
                            is_move: true,
                        };
                        Ok(LoweredStmt::Stmt(HirStmt::If {
                            cond: null_check,
                            then_branch: Box::new(HirStmt::Block(vec![assign_stmt])),
                            else_branch: None,
                        }))
                    } else if let ExprKind::Get(obj, field) = &target.kind {
                        // Generate: if obj.field == null { obj.field = value }
                        let hir_obj = lower_expr(obj)?;
                        let load_member =
                            HirExpr::MemberAccess(Box::new(hir_obj.clone()), field.clone());
                        let null_check = HirExpr::BinaryOp(
                            Box::new(load_member),
                            BinOp::Eq,
                            Box::new(HirExpr::Literal(HirLiteral::Null)),
                        );
                        let set_member = HirStmt::Expr(HirExpr::SetMember(
                            Box::new(hir_obj),
                            field.clone(),
                            Box::new(hir_value),
                        ));
                        Ok(LoweredStmt::Stmt(HirStmt::If {
                            cond: null_check,
                            then_branch: Box::new(HirStmt::Block(vec![set_member])),
                            else_branch: None,
                        }))
                    } else {
                        Ok(LoweredStmt::Stmt(HirStmt::Expr(hir_expr)))
                    }
                } else {
                    // Normal compound assignment - just store the value (already expanded by parser)
                    if let ExprKind::Variable(name) = &target.kind {
                        Ok(LoweredStmt::Stmt(HirStmt::Assign {
                            target: HirExpr::LoadVar(name.clone()),
                            value: hir_value,
                            is_move: true,
                        }))
                    } else if let ExprKind::Get(obj, field) = &target.kind {
                        // Handle property assignment: obj.field = value
                        let hir_obj = lower_expr(obj)?;
                        Ok(LoweredStmt::Stmt(HirStmt::Expr(HirExpr::SetMember(
                            Box::new(hir_obj),
                            field.clone(),
                            Box::new(hir_value),
                        ))))
                    } else if let ExprKind::Index(obj, idx) = &target.kind {
                        // Handle index assignment: obj[idx] = value
                        let hir_obj = lower_expr(obj)?;
                        let hir_idx = lower_expr(idx)?;
                        Ok(LoweredStmt::Stmt(HirStmt::Assign {
                            target: HirExpr::Index(Box::new(hir_obj), Box::new(hir_idx)),
                            value: hir_value,
                            is_move: true,
                        }))
                    } else {
                        Ok(LoweredStmt::Stmt(HirStmt::Expr(hir_expr)))
                    }
                }
            } else {
                Ok(LoweredStmt::Stmt(HirStmt::Expr(hir_expr)))
            }
        }

        StmtKind::Block(stmts) => {
            let mut hir_stmts = Vec::new();
            for s in stmts {
                match lower_stmt(s, include_tests)? {
                    LoweredStmt::Stmt(hs) => hir_stmts.push(hs),
                    LoweredStmt::Function(f) => {
                        // Inline function definition as a statement
                        hir_stmts.push(HirStmt::FunctionDef {
                            name: f.name,
                            params: f.params,
                            body: f.body,
                            ret_type: f.ret_type,
                            is_async: f.is_async,
                            decorators: f.decorators,
                            move_params: f.move_params,
                            is_unsafe: f.is_unsafe,
                        });
                    }
                    LoweredStmt::Class(c) => {
                        hir_stmts.push(HirStmt::ClassDef(c));
                    }
                    LoweredStmt::Skip => {} // Skip test functions
                }
            }
            Ok(LoweredStmt::Stmt(HirStmt::Block(hir_stmts)))
        }

        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let hir_cond = lower_expr(cond)?;
            let hir_then = match lower_stmt(then_branch, include_tests)? {
                LoweredStmt::Stmt(s) => s,
                LoweredStmt::Function(_) => return Err("Function not allowed as if branch".into()),
                LoweredStmt::Class(_) => return Err("Class not allowed as if branch".into()),
                LoweredStmt::Skip => return Err("Test function not allowed as if branch".into()),
            };
            let hir_else = else_branch
                .as_ref()
                .map(|e| lower_stmt(e, include_tests))
                .transpose()?
                .map(|ls| match ls {
                    LoweredStmt::Stmt(s) => Ok(s),
                    LoweredStmt::Function(_) => Err("Function not allowed as else branch"),
                    LoweredStmt::Class(_) => Err("Class not allowed as else branch"),
                    LoweredStmt::Skip => Err("Test function not allowed as else branch"),
                })
                .transpose()?;

            Ok(LoweredStmt::Stmt(HirStmt::If {
                cond: hir_cond,
                then_branch: Box::new(hir_then),
                else_branch: hir_else.map(Box::new),
            }))
        }

        StmtKind::While { cond, body } => {
            let hir_cond = lower_expr(cond)?;
            let hir_body = match lower_stmt(body, include_tests)? {
                LoweredStmt::Stmt(s) => s,
                LoweredStmt::Function(_) => return Err("Function not allowed as while body".into()),
                LoweredStmt::Class(_) => return Err("Class not allowed as while body".into()),
                LoweredStmt::Skip => return Err("Test function not allowed as while body".into()),
            };
            Ok(LoweredStmt::Stmt(HirStmt::While {
                cond: hir_cond,
                body: Box::new(hir_body),
            }))
        }

        StmtKind::ForIn { name, iter, body } => {
            let hir_iter = lower_expr(iter)?;
            let hir_body = match lower_stmt(body, include_tests)? {
                LoweredStmt::Stmt(s) => s,
                LoweredStmt::Function(_) => return Err("Function not allowed as for body".into()),
                LoweredStmt::Class(_) => return Err("Class not allowed as for body".into()),
                LoweredStmt::Skip => return Err("Test function not allowed as for body".into()),
            };
            Ok(LoweredStmt::Stmt(HirStmt::ForIn {
                var: name.clone(),
                iter: hir_iter,
                body: Box::new(hir_body),
            }))
        }

        StmtKind::Function(func, export) => {
            // Skip test functions in production builds (like Rust)
            if func.is_test && !include_tests {
                return Ok(LoweredStmt::Skip);
            }
            lower_function_with_export(func, *export).map(LoweredStmt::Function)
        }

        StmtKind::Return(expr) => {
            let hir_expr = expr.as_ref().map(|e| lower_expr(e)).transpose()?;
            Ok(LoweredStmt::Stmt(HirStmt::Return(hir_expr)))
        }

        StmtKind::Break => Ok(LoweredStmt::Stmt(HirStmt::Break)),
        StmtKind::Continue => Ok(LoweredStmt::Stmt(HirStmt::Continue)),

        // Handle class declarations
        StmtKind::Class(class_decl, _export) => lower_class(class_decl).map(LoweredStmt::Class),

        // Handle try-catch
        StmtKind::TryCatch {
            try_block,
            err_name,
            catch_block,
        } => {
            let hir_try = match lower_stmt(try_block, include_tests)? {
                LoweredStmt::Stmt(s) => s,
                _ => return Err("Invalid try block".into()),
            };
            let hir_catch = match lower_stmt(catch_block, include_tests)? {
                LoweredStmt::Stmt(s) => s,
                _ => return Err("Invalid catch block".into()),
            };
            Ok(LoweredStmt::Stmt(HirStmt::TryCatch {
                try_block: Box::new(hir_try),
                error_name: err_name.clone(),
                catch_block: Box::new(hir_catch),
            }))
        }

        // Handle throw (throw is an expression in AST, wrapped in ExprStmt)
        // No separate StmtKind::Throw - it's handled through ExprKind::Throw

        // Handle imports - now properly lowered to HIR
        StmtKind::Import { path, alias } => Ok(LoweredStmt::Stmt(HirStmt::Import {
            path: path.clone(),
            alias: alias.clone(),
        })),
        StmtKind::ImportDefault { path, alias } => Ok(LoweredStmt::Stmt(HirStmt::ImportDefault {
            path: path.clone(),
            alias: alias.clone(),
        })),
        StmtKind::ImportNames { path, names } => Ok(LoweredStmt::Stmt(HirStmt::ImportNames {
            path: path.clone(),
            names: names.clone(),
        })),

        // Handle extend statement
        StmtKind::Extend(_name, target, methods, _export) => {
            let hir_methods: Result<Vec<HirFunction>, String> =
                methods.iter().map(|m| lower_function(m)).collect();
            Ok(LoweredStmt::Stmt(HirStmt::Extend {
                target: target.clone(),
                methods: hir_methods?,
            }))
        }

        // Unsafe block: lower inner statement and wrap in HirStmt::Unsafe
        StmtKind::UnsafeBlock(body) => match lower_stmt(body, include_tests)? {
            LoweredStmt::Stmt(inner) => Ok(LoweredStmt::Stmt(HirStmt::Unsafe(Box::new(inner)))),
            LoweredStmt::Function(_) => {
                Err("Function not allowed directly inside unsafe block".into())
            }
            LoweredStmt::Class(_) => Err("Class not allowed directly inside unsafe block".into()),
            LoweredStmt::Skip => {
                Err("Test function not allowed directly inside unsafe block".into())
            }
        },

        // Defer block: lower inner statement and wrap in HirStmt::Defer
        StmtKind::Defer(block) => match lower_stmt(block, include_tests)? {
            LoweredStmt::Stmt(inner) => Ok(LoweredStmt::Stmt(HirStmt::Defer(Box::new(inner)))),
            LoweredStmt::Function(_) => Err("Function not allowed inside defer block".into()),
            LoweredStmt::Class(_) => Err("Class not allowed inside defer block".into()),
            LoweredStmt::Skip => Err("Test function not allowed inside defer block".into()),
        },

        _ => Ok(LoweredStmt::Stmt(HirStmt::Block(Vec::new()))),
    }
}

fn lower_class(class_decl: &ClassDecl) -> Result<HirClass, String> {
    let mut methods = Vec::new();
    let mut static_methods = Vec::new();

    // Lower instance methods
    for method in &class_decl.methods {
        let hir_method = lower_method(method, false)?;
        methods.push(hir_method);
    }

    // Lower static methods
    for method in &class_decl.static_methods {
        let hir_method = lower_method(method, true)?;
        static_methods.push(hir_method);
    }

    // Lower decorators
    let decorators: Result<Vec<_>, _> = class_decl.decorators.iter().map(lower_expr).collect();

    Ok(HirClass {
        name: class_decl.name.clone(),
        extends: class_decl.extends.clone(),
        methods,
        static_methods,
        decorators: decorators?,
    })
}

fn lower_method(func: &Function, is_static: bool) -> Result<HirMethod, String> {
    let params: Vec<(String, Option<HirType>)> = func
        .params
        .iter()
        .map(|(name, _default, ty)| (name.clone(), ty.as_ref().map(|t| type_from_annotation(t))))
        .collect();

    let mut body_stmts = Vec::new();
    // For methods, always include all code (not top-level)
    let include_tests = true;
    for s in func.body.iter() {
        match lower_stmt(s, include_tests)? {
            LoweredStmt::Stmt(hs) => body_stmts.push(hs),
            LoweredStmt::Function(f) => {
                body_stmts.push(HirStmt::FunctionDef {
                    name: f.name,
                    params: f.params,
                    body: f.body,
                    ret_type: f.ret_type,
                    is_async: f.is_async,
                    decorators: f.decorators,
                    move_params: f.move_params,
                    is_unsafe: f.is_unsafe,
                });
            }
            LoweredStmt::Class(c) => {
                body_stmts.push(HirStmt::ClassDef(c));
            }
            LoweredStmt::Skip => {} // Won't happen since include_tests=true
        }
    }

    Ok(HirMethod {
        name: func.name.clone(),
        params,
        body: Arc::new(body_stmts),
        ret_type: func.ret_type.as_ref().map(|t| type_from_annotation(t)),
        is_static,
        is_async: func.is_async,
        is_unsafe: func.is_unsafe,
    })
}

fn lower_function(func: &Function) -> Result<HirFunction, String> {
    lower_function_with_export(func, false)
}

fn lower_function_with_export(func: &Function, is_exported: bool) -> Result<HirFunction, String> {
    // For function bodies, always include all code (not top-level)
    let include_tests = true;
    // FFI: Exported functions can optionally have type annotations.
    // If omitted, parameters default to Any (dynamic) and return type defaults to Any.
    // Type annotations are only required when generating C FFI headers (--emit-header).

    let params: Result<Vec<(String, Option<HirType>, Option<HirExpr>)>, String> = func
        .params
        .iter()
        .map(|(name, default, ty)| {
            let hir_default = default.as_ref().map(|d| lower_expr(d)).transpose()?;
            Ok((
                name.clone(),
                ty.as_ref().map(|t| type_from_annotation(t)),
                hir_default,
            ))
        })
        .collect();
    let params = params?;

    let mut body_stmts = Vec::new();
    for s in func.body.iter() {
        match lower_stmt(s, include_tests)? {
            LoweredStmt::Stmt(hs) => body_stmts.push(hs),
            LoweredStmt::Function(f) => {
                body_stmts.push(HirStmt::FunctionDef {
                    name: f.name,
                    params: f.params,
                    body: f.body,
                    ret_type: f.ret_type,
                    is_async: f.is_async,
                    decorators: f.decorators,
                    move_params: f.move_params,
                    is_unsafe: f.is_unsafe,
                });
            }
            LoweredStmt::Class(c) => {
                body_stmts.push(HirStmt::ClassDef(c));
            }
            LoweredStmt::Skip => {} // Won't happen since include_tests=true
        }
    }

    // Lower decorators
    let decorators: Result<Vec<_>, _> = func.decorators.iter().map(lower_expr).collect();

    Ok(HirFunction {
        name: func.name.clone(),
        params,
        body: Arc::new(body_stmts),
        ret_type: func.ret_type.as_ref().map(|t| type_from_annotation(t)),
        is_async: func.is_async,
        decorators: decorators?,
        is_exported,
        move_params: Vec::new(),
        is_test: func.is_test,
        test_ignore: func.test_ignore,
        test_expect_fail: func.test_expect_fail,
        test_timeout: func.test_timeout,
        is_unsafe: func.is_unsafe,
    })
}

fn lower_expr(expr: &Expr) -> Result<HirExpr, String> {
    match &expr.kind {
        ExprKind::Literal(val) => Ok(HirExpr::Literal(value_to_hir_literal(val))),

        ExprKind::Variable(name) => Ok(HirExpr::LoadVar(name.clone())),

        ExprKind::Assign(name, value) => {
            let hir_value = lower_expr(value)?;
            Ok(HirExpr::StoreVar(name.clone(), Box::new(hir_value)))
        }

        ExprKind::AssignOp(target, _op, value) => {
            // AssignOp represents `target = value` where value already includes any
            // compound operation (the AST parser expands `i += 1` to `AssignOp(i, =, i + 1)`).
            // For simple variable targets, treat it as a store.
            if let ExprKind::Variable(name) = &target.kind {
                let hir_value = lower_expr(value)?;
                Ok(HirExpr::StoreVar(name.clone(), Box::new(hir_value)))
            } else if let ExprKind::Get(obj, field) = &target.kind {
                // Handle property assignment: obj.field = value
                let hir_obj = lower_expr(obj)?;
                let hir_value = lower_expr(value)?;
                Ok(HirExpr::SetMember(
                    Box::new(hir_obj),
                    field.clone(),
                    Box::new(hir_value),
                ))
            } else if let ExprKind::Index(container, idx_expr) = &target.kind {
                // Lower index assignment to builtin set_index(container, index, value)
                let hir_container = lower_expr(container)?;
                let hir_index = lower_expr(idx_expr)?;
                let hir_value = lower_expr(value)?;
                Ok(HirExpr::Call(
                    Box::new(HirExpr::LoadVar("set_index".to_string())),
                    vec![hir_container, hir_index, hir_value],
                    vec![],
                ))
            } else {
                // For complex targets (array index, etc.), just evaluate the value
                lower_expr(value)
            }
        }

        ExprKind::Binary(left, op, right) => {
            let hir_left = lower_expr(left)?;
            let hir_right = lower_expr(right)?;
            let bin_op = token_to_binop(op)?;
            Ok(HirExpr::BinaryOp(
                Box::new(hir_left),
                bin_op,
                Box::new(hir_right),
            ))
        }

        ExprKind::Unary(op, operand) => {
            // Special case: fold unary minus on numeric literals into signed literals
            if *op == TokenKind::Minus {
                if let ExprKind::Literal(val) = &operand.kind {
                    match val {
                        Value::Number(n) => {
                            // Convert to negative and infer signed type
                            let neg = -(*n);
                            let is_integer = neg.fract().abs() < 1e-12;

                            if is_integer {
                                let i = neg as i128;
                                // Negative integer: choose smallest signed type
                                let lit = if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                                    HirLiteral::I8(i as i8)
                                } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                                    HirLiteral::I16(i as i16)
                                } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                                    HirLiteral::I32(i as i32)
                                } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                                    HirLiteral::I64(i as i64)
                                } else {
                                    HirLiteral::I128(i)
                                };
                                return Ok(HirExpr::Literal(lit));
                            } else {
                                return Ok(HirExpr::Literal(HirLiteral::F64(neg)));
                            }
                        }
                        Value::U8(n) => return Ok(HirExpr::Literal(HirLiteral::I8(-(*n as i8)))),
                        Value::U16(n) => {
                            return Ok(HirExpr::Literal(HirLiteral::I16(-(*n as i16))));
                        }
                        Value::U32(n) => {
                            return Ok(HirExpr::Literal(HirLiteral::I32(-(*n as i32))));
                        }
                        Value::U64(n) => {
                            return Ok(HirExpr::Literal(HirLiteral::I64(-(*n as i64))));
                        }
                        Value::I8(n) => return Ok(HirExpr::Literal(HirLiteral::I8(-*n))),
                        Value::I16(n) => return Ok(HirExpr::Literal(HirLiteral::I16(-*n))),
                        Value::I32(n) => return Ok(HirExpr::Literal(HirLiteral::I32(-*n))),
                        Value::I64(n) => return Ok(HirExpr::Literal(HirLiteral::I64(-*n))),
                        Value::I128(n) => return Ok(HirExpr::Literal(HirLiteral::I128(-*n))),
                        Value::F32(n) => return Ok(HirExpr::Literal(HirLiteral::F32(-*n))),
                        Value::F64(n) => return Ok(HirExpr::Literal(HirLiteral::F64(-*n))),
                        _ => {}
                    }
                }
            }

            // Handle borrow/unary operators that map directly to HIR borrow/deref
            if *op == TokenKind::Ampersand {
                let hir_operand = lower_expr(operand)?;
                return Ok(HirExpr::Borrow(Box::new(hir_operand), false));
            }
            if *op == TokenKind::BangEqual {
                // Parser uses BangEqual as a marker for `&mut` (legacy); lower to BorrowMut
                let hir_operand = lower_expr(operand)?;
                return Ok(HirExpr::BorrowMut(Box::new(hir_operand)));
            }
            if *op == TokenKind::Star {
                let hir_operand = lower_expr(operand)?;
                return Ok(HirExpr::Deref(Box::new(hir_operand)));
            }

            let hir_operand = lower_expr(operand)?;
            let unary_op = token_to_unaryop(op)?;
            Ok(HirExpr::UnaryOp(unary_op, Box::new(hir_operand)))
        }

        ExprKind::Logical(left, op, right) => {
            let hir_left = lower_expr(left)?;
            let hir_right = lower_expr(right)?;
            let bin_op = match op {
                TokenKind::And | TokenKind::AndAnd => BinOp::And,
                TokenKind::Or | TokenKind::OrOr => BinOp::Or,
                _ => return Err(format!("Unknown logical operator: {:?}", op)),
            };
            Ok(HirExpr::BinaryOp(
                Box::new(hir_left),
                bin_op,
                Box::new(hir_right),
            ))
        }

        ExprKind::Grouping(inner) => lower_expr(inner),

        ExprKind::Call(callee, args, type_args) => {
            // Check if this is a method call
            if let ExprKind::Get(obj, method_name) = &callee.kind {
                let hir_obj = lower_expr(obj)?;
                let hir_args: Result<Vec<_>, _> = args.iter().map(lower_expr).collect();
                Ok(HirExpr::MethodCall(
                    Box::new(hir_obj),
                    method_name.clone(),
                    hir_args?,
                ))
            } else {
                let hir_callee = lower_expr(callee)?;
                let hir_args: Result<Vec<_>, _> = args.iter().map(lower_expr).collect();
                Ok(HirExpr::Call(
                    Box::new(hir_callee),
                    hir_args?,
                    type_args.clone(),
                ))
            }
        }

        ExprKind::Array(elements) => {
            let hir_elements: Result<Vec<_>, _> = elements.iter().map(lower_expr).collect();
            Ok(HirExpr::ArrayLiteral(hir_elements?))
        }

        ExprKind::Tuple(elements) => {
            let hir_elements: Result<Vec<_>, _> = elements.iter().map(lower_expr).collect();
            Ok(HirExpr::TupleLiteral(hir_elements?))
        }

        ExprKind::SetLiteral(elements) => {
            let hir_elements: Result<Vec<_>, _> = elements.iter().map(lower_expr).collect();
            Ok(HirExpr::SetLiteral(hir_elements?))
        }

        ExprKind::Object(fields) => {
            let hir_fields: Result<Vec<_>, _> = fields
                .iter()
                .map(|(k, v)| lower_expr(v).map(|hv| (k.clone(), hv)))
                .collect();
            Ok(HirExpr::ObjectLiteral(hir_fields?))
        }

        ExprKind::StructLiteral(name, fields) => {
            let hir_fields: Result<Vec<_>, _> = fields
                .iter()
                .map(|(k, v)| lower_expr(v).map(|hv| (k.clone(), hv)))
                .collect();
            Ok(HirExpr::StructLiteral(name.clone(), hir_fields?))
        }

        ExprKind::Index(target, index) => {
            let hir_target = lower_expr(target)?;
            let hir_index = lower_expr(index)?;
            Ok(HirExpr::Index(Box::new(hir_target), Box::new(hir_index)))
        }

        ExprKind::Get(target, field) => {
            let hir_target = lower_expr(target)?;
            Ok(HirExpr::MemberAccess(Box::new(hir_target), field.clone()))
        }

        ExprKind::Set(target, field, value) => {
            let hir_target = lower_expr(target)?;
            let hir_value = lower_expr(value)?;
            Ok(HirExpr::SetMember(
                Box::new(hir_target),
                field.clone(),
                Box::new(hir_value),
            ))
        }

        ExprKind::Conditional(cond, then_expr, else_expr) => {
            let hir_cond = lower_expr(cond)?;
            let hir_then = lower_expr(then_expr)?;
            let hir_else = lower_expr(else_expr)?;
            Ok(HirExpr::Conditional(
                Box::new(hir_cond),
                Box::new(hir_then),
                Box::new(hir_else),
            ))
        }

        ExprKind::Fn(params, body, is_async) => {
            let hir_params: Vec<(String, Option<HirType>)> = params
                .iter()
                .map(|(name, _default, ty)| {
                    (name.clone(), ty.as_ref().map(|t| type_from_annotation(t)))
                })
                .collect();

            let mut hir_body = Vec::new();
            // For lambda/closure bodies, always include all code
            let include_tests = true;
            // Dereference Arc to iterate
            for s in body.as_ref() {
                match lower_stmt(s, include_tests)? {
                    LoweredStmt::Stmt(hs) => hir_body.push(hs),
                    LoweredStmt::Function(f) => {
                        hir_body.push(HirStmt::FunctionDef {
                            name: f.name,
                            params: f.params,
                            body: f.body,
                            ret_type: f.ret_type,
                            is_async: f.is_async,
                            decorators: f.decorators,
                            move_params: f.move_params,
                            is_unsafe: f.is_unsafe,
                        });
                    }
                    LoweredStmt::Class(c) => {
                        hir_body.push(HirStmt::ClassDef(c));
                    }
                    LoweredStmt::Skip => {} // Won't happen in lambda body
                }
            }
            Ok(HirExpr::Lambda(hir_params, Arc::new(hir_body), *is_async))
        }

        ExprKind::New(class_expr, args) => {
            // Extract class name from expression if it's a variable
            let class_name = if let ExprKind::Variable(name) = &class_expr.kind {
                name.clone()
            } else {
                // For complex expressions, use a placeholder
                // Dynamic class instantiation is limited in JIT
                "__dynamic_class__".to_string()
            };
            let hir_args: Result<Vec<_>, _> = args.iter().map(lower_expr).collect();
            Ok(HirExpr::NewInstance(class_name, hir_args?))
        }

        ExprKind::Await(inner) => {
            let hir_inner = lower_expr(inner)?;
            Ok(HirExpr::Await(Box::new(hir_inner)))
        }

        ExprKind::Spawn(inner) => {
            let hir_inner = lower_expr(inner)?;
            Ok(HirExpr::Spawn(Box::new(hir_inner)))
        }

        ExprKind::Throw(inner) => {
            // Throw as an expression - wrap in a throw HIR stmt pattern
            let hir_inner = lower_expr(inner)?;
            // For now, just evaluate the expression (throw is usually statement-level)
            Ok(hir_inner)
        }

        ExprKind::Range(start, end, inclusive) => {
            let hir_start = lower_expr(start)?;
            let hir_end = lower_expr(end)?;
            Ok(HirExpr::Range(
                Box::new(hir_start),
                Box::new(hir_end),
                *inclusive,
            ))
        }

        ExprKind::Spread(inner) => {
            let hir_inner = lower_expr(inner)?;
            Ok(HirExpr::Spread(Box::new(hir_inner)))
        }

        ExprKind::OptGet(inner, member) => {
            let hir_inner = lower_expr(inner)?;
            Ok(HirExpr::OptionalGet(Box::new(hir_inner), member.clone()))
        }

        ExprKind::NonNull(inner) => {
            let hir_inner = lower_expr(inner)?;
            Ok(HirExpr::NonNull(Box::new(hir_inner)))
        }

        ExprKind::Try(inner) => lower_expr(inner),

        ExprKind::Update(is_increment, is_prefix, target) => {
            let hir_target = lower_expr(target)?;
            Ok(HirExpr::Update(
                Box::new(hir_target),
                *is_increment,
                *is_prefix,
            ))
        }

        ExprKind::Match(scrutinee, arms) => {
            let hir_scrutinee = lower_expr(scrutinee)?;
            let hir_arms: Result<Vec<(HirPattern, HirExpr)>, String> = arms
                .iter()
                .map(|(pat, expr)| {
                    let hir_pattern = lower_pattern(pat)?;
                    let hir_expr = lower_expr(expr)?;
                    Ok((hir_pattern, hir_expr))
                })
                .collect();
            Ok(HirExpr::Match(Box::new(hir_scrutinee), hir_arms?))
        }

        ExprKind::Format(inner, spec) => {
            let hir_inner = lower_expr(inner)?;
            Ok(HirExpr::Format(Box::new(hir_inner), spec.clone()))
        }

        ExprKind::Cast(inner, target_ty_ann) => {
            let hir_inner = lower_expr(inner)?;
            let target_ty = type_from_annotation(target_ty_ann);
            Ok(HirExpr::Cast(Box::new(hir_inner), target_ty))
        }

        ExprKind::AssignTuple(names, value) => {
            let hir_value = lower_expr(value)?;
            let wrapped_value = match hir_value {
                HirExpr::LoadVar(name) => HirExpr::Move(Box::new(HirExpr::LoadVar(name))),
                other => other,
            };
            Ok(HirExpr::AssignTuple(names.clone(), Box::new(wrapped_value)))
        }

        ExprKind::AssignObject(properties, value) => {
            let hir_value = lower_expr(value)?;
            let wrapped_value = match hir_value {
                HirExpr::LoadVar(name) => HirExpr::Move(Box::new(HirExpr::LoadVar(name))),
                other => other,
            };
            Ok(HirExpr::AssignObject(
                properties.clone(),
                Box::new(wrapped_value),
            ))
        }

        // Handle other expression types with fallback
        _ => Ok(HirExpr::Literal(HirLiteral::Null)),
    }
}

fn value_to_hir_literal(val: &Value) -> HirLiteral {
    match val {
        Value::Number(n) => {
            // Smart type inference: choose smallest type based on value
            let is_integer = n.fract().abs() < 1e-12;

            if is_integer {
                let i = *n as i128;

                if i >= 0 {
                    // Positive integer: choose smallest unsigned type
                    if i <= u8::MAX as i128 {
                        HirLiteral::U8(i as u8)
                    } else if i <= u16::MAX as i128 {
                        HirLiteral::U16(i as u16)
                    } else if i <= u32::MAX as i128 {
                        HirLiteral::U32(i as u32)
                    } else if i <= u64::MAX as i128 {
                        HirLiteral::U64(i as u64)
                    } else {
                        HirLiteral::U128(i as u128)
                    }
                } else {
                    // Negative integer: choose smallest signed type
                    if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                        HirLiteral::I8(i as i8)
                    } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                        HirLiteral::I16(i as i16)
                    } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                        HirLiteral::I32(i as i32)
                    } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                        HirLiteral::I64(i as i64)
                    } else {
                        HirLiteral::I128(i)
                    }
                }
            } else {
                // Floating point: use f64 for precision
                HirLiteral::F64(*n)
            }
        }
        Value::Bool(b) => HirLiteral::Bool(*b),
        Value::Str(s) => HirLiteral::String(s.clone()),
        Value::Char(c) => HirLiteral::Char(*c),
        Value::BigInt(bi) => HirLiteral::BigInt((*bi).clone()),
        Value::Null => HirLiteral::Null,
        // Fixed-width integer types (unsigned)
        Value::U8(n) => HirLiteral::U8(*n),
        Value::U16(n) => HirLiteral::U16(*n),
        Value::U32(n) => HirLiteral::U32(*n),
        Value::U64(n) => HirLiteral::U64(*n),
        Value::U128(n) => HirLiteral::U128(*n),
        // Fixed-width integer types (signed)
        Value::I8(n) => HirLiteral::I8(*n),
        Value::I16(n) => HirLiteral::I16(*n),
        Value::I32(n) => HirLiteral::I32(*n),
        Value::I64(n) => HirLiteral::I64(*n),
        Value::I128(n) => HirLiteral::I128(*n),
        // Fixed-width float types
        Value::F32(n) => HirLiteral::F32(*n),
        Value::F64(n) => HirLiteral::F64(*n),
        _ => HirLiteral::Null,
    }
}

fn token_to_binop(tok: &TokenKind) -> Result<BinOp, String> {
    match tok {
        TokenKind::Plus => Ok(BinOp::Add),
        TokenKind::Minus => Ok(BinOp::Sub),
        TokenKind::Star => Ok(BinOp::Mul),
        TokenKind::Slash => Ok(BinOp::Div),
        TokenKind::Percent => Ok(BinOp::Mod),
        TokenKind::StarStar => Ok(BinOp::Pow),
        TokenKind::TildeSlash => Ok(BinOp::IntDiv),
        TokenKind::Less => Ok(BinOp::Lt),
        TokenKind::LessEqual => Ok(BinOp::Le),
        TokenKind::Greater => Ok(BinOp::Gt),
        TokenKind::GreaterEqual => Ok(BinOp::Ge),
        TokenKind::EqualEqual => Ok(BinOp::Eq),
        TokenKind::BangEqual => Ok(BinOp::Ne),
        TokenKind::StrictEqual => Ok(BinOp::StrictEq),
        TokenKind::StrictNotEqual => Ok(BinOp::StrictNe),
        TokenKind::Ampersand => Ok(BinOp::BitAnd),
        TokenKind::Pipe => Ok(BinOp::BitOr),
        TokenKind::Caret => Ok(BinOp::BitXor),
        TokenKind::ShiftLeft => Ok(BinOp::ShiftLeft),
        TokenKind::ShiftRight => Ok(BinOp::ShiftRight),
        TokenKind::NullCoalesce => Ok(BinOp::NullCoalesce),
        TokenKind::In => Ok(BinOp::In),
        TokenKind::Instanceof => Ok(BinOp::Instanceof),
        _ => Err(format!("Unknown binary operator: {:?}", tok)),
    }
}

fn token_to_unaryop(tok: &TokenKind) -> Result<UnaryOp, String> {
    match tok {
        TokenKind::Minus => Ok(UnaryOp::Neg),
        TokenKind::Bang => Ok(UnaryOp::Not),
        TokenKind::Tilde => Ok(UnaryOp::BitNot),
        TokenKind::Typeof => Ok(UnaryOp::Typeof),
        _ => Err(format!("Unknown unary operator: {:?}", tok)),
    }
}

fn type_from_annotation(ann: &str) -> HirType {
    let mut ann = ann.trim();
    if let Some(stripped) = ann.strip_suffix("readonly") {
        ann = stripped.trim();
    }
    if let Some(stripped) = ann.strip_prefix("readonly") {
        ann = stripped.trim();
    }
    let ann_lower = ann.to_ascii_lowercase();

    match ann_lower.as_str() {
        "int" | "number" | "float" | "array" => HirType::Unknown,
        "f32" => HirType::F32,
        "f64" => HirType::F64,
        "u8" => HirType::U8,
        "u16" => HirType::U16,
        "u32" => HirType::U32,
        "u64" => HirType::U64,
        "u128" => HirType::U128,
        "i8" => HirType::I8,
        "i16" => HirType::I16,
        "i32" => HirType::I32,
        "i64" => HirType::I64,
        "i128" => HirType::I128,
        "bool" | "boolean" => HirType::Bool,
        "string" | "str" => HirType::String,
        "char" => HirType::Char,
        "null" | "void" => HirType::Null,
        "any" => HirType::Any,
        _ if ann.contains('|') => {
            let mut depth_sq = 0;
            let mut depth_angle = 0;
            let mut depth_paren = 0;
            let mut depth_brace = 0;
            let mut parts = Vec::new();
            let mut last_idx = 0;
            for (i, ch) in ann.char_indices() {
                match ch {
                    '[' => depth_sq += 1,
                    ']' => depth_sq -= 1,
                    '<' => depth_angle += 1,
                    '>' => depth_angle -= 1,
                    '(' => depth_paren += 1,
                    ')' => depth_paren -= 1,
                    '{' => depth_brace += 1,
                    '}' => depth_brace -= 1,
                    '|' if depth_sq == 0
                        && depth_angle == 0
                        && depth_paren == 0
                        && depth_brace == 0 =>
                    {
                        parts.push(ann[last_idx..i].trim());
                        last_idx = i + 1;
                    }
                    _ => {}
                }
            }
            if last_idx < ann.len() {
                parts.push(ann[last_idx..].trim());
            }
            if parts.len() > 1 {
                if let Some(primary) = parts.iter().find(|p| {
                    let p_lower = p.to_ascii_lowercase();
                    p_lower != "null" && p_lower != "void" && p_lower != "undefined"
                }) {
                    type_from_annotation(primary)
                } else {
                    HirType::Unknown
                }
            } else {
                HirType::Class(ann.to_string())
            }
        }
        _ if ann.starts_with('[') && ann.ends_with(']') => {
            // Parse [T], [T;N], [T;raw], [T;N;raw] and nested array/matrix forms
            let inner = &ann[1..ann.len() - 1];
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
            if let Some(semicolon_pos) = semi_idx {
                let elem_type_str = &inner[..semicolon_pos];
                let rest = &inner[semicolon_pos + 1..];
                let elem_type = type_from_annotation(elem_type_str);
                if rest == "raw" {
                    HirType::Array(Box::new(elem_type), ArrayKind::Raw)
                } else if let Ok(size) = rest.parse::<usize>() {
                    HirType::Array(Box::new(elem_type), ArrayKind::Fixed(size))
                } else if let Some(second_semicolon) = rest.find(';') {
                    let size_str = &rest[..second_semicolon];
                    let after = &rest[second_semicolon + 1..];
                    if after == "raw" {
                        if let Ok(size) = size_str.parse::<usize>() {
                            HirType::Array(Box::new(elem_type), ArrayKind::FixedRaw(size))
                        } else {
                            HirType::Unknown
                        }
                    } else {
                        HirType::Unknown
                    }
                } else {
                    HirType::Unknown
                }
            } else {
                // [T] - dynamic array
                let elem_type = type_from_annotation(inner);
                HirType::Array(Box::new(elem_type), ArrayKind::Dynamic)
            }
        }
        _ if (ann_lower.starts_with("dict<") || ann_lower.starts_with("map<"))
            && ann.ends_with('>') =>
        {
            // Simple parsing for Dict<K, V>
            HirType::Object
        }
        _ if ann_lower.starts_with("set<") && ann.ends_with('>') => {
            let inner = &ann[4..ann.len() - 1];
            HirType::Set(Box::new(type_from_annotation(inner)))
        }
        _ if ann_lower.starts_with("promise<") && ann.ends_with('>') => {
            let inner = &ann[8..ann.len() - 1];
            HirType::Promise(Box::new(type_from_annotation(inner)))
        }
        _ if ann_lower.starts_with("array<") && ann.ends_with('>') => HirType::Unknown,
        _ => {
            // Preserve named user types (struct/class/interface) as class-like references.
            // Keeping this as Unknown loses type intent and degrades backend lowering.
            HirType::Class(ann.to_string())
        }
    }
}

fn lower_pattern(pat: &Pattern) -> Result<HirPattern, String> {
    match pat {
        Pattern::Literal(val) => Ok(HirPattern::Literal(value_to_hir_literal(val))),
        Pattern::Variable(name) => Ok(HirPattern::Variable(name.clone())),
        Pattern::Wildcard => Ok(HirPattern::Wildcard),
        Pattern::Or(left, right) => {
            let hir_left = lower_pattern(left)?;
            let hir_right = lower_pattern(right)?;
            Ok(HirPattern::Or(Box::new(hir_left), Box::new(hir_right)))
        }
        Pattern::EnumVariant(name, subpats) => {
            let hir_subpats: Result<Vec<HirPattern>, String> =
                subpats.iter().map(lower_pattern).collect();
            Ok(HirPattern::EnumVariant(name.clone(), hir_subpats?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    fn parse_to_hir(src: &str) -> Result<HirModule, String> {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
        let mut parser = Parser::new(tokens, None);
        let ast = parser.parse_program().map_err(|e| e.to_string())?;
        ast_to_hir(&ast, true)
    }

    #[test]
    fn test_simple_let() {
        let hir = parse_to_hir("let x = 42;").unwrap();
        assert_eq!(hir.statements.len(), 1);
        match &hir.statements[0] {
            HirStmt::Let { name, init, .. } => {
                assert_eq!(name, "x");
                assert!(init.is_some());
            }
            _ => panic!("Expected Let statement"),
        }
    }

    #[test]
    fn test_function_def() {
        let hir = parse_to_hir("fn add(a, b) { return a + b; }").unwrap();
        assert_eq!(hir.functions.len(), 1);
        assert_eq!(hir.functions[0].name, "add");
        assert_eq!(hir.functions[0].params.len(), 2);
    }

    #[test]
    fn test_if_statement() {
        let hir = parse_to_hir("if (x > 0) { print(x); }").unwrap();
        assert_eq!(hir.statements.len(), 1);
        match &hir.statements[0] {
            HirStmt::If { cond, .. } => match cond {
                HirExpr::BinaryOp(_, BinOp::Gt, _) => {}
                _ => panic!("Expected > comparison"),
            },
            _ => panic!("Expected If statement"),
        }
    }

    #[test]
    fn test_while_loop() {
        let hir = parse_to_hir("while (i < 10) { i = i + 1; }").unwrap();
        assert_eq!(hir.statements.len(), 1);
        match &hir.statements[0] {
            HirStmt::While { .. } => {}
            _ => panic!("Expected While statement"),
        }
    }

    #[test]
    fn test_type_annotation_no_legacy_aliases() {
        assert_eq!(type_from_annotation("u8"), HirType::U8);
        assert_eq!(type_from_annotation("i32"), HirType::I32);
        assert_eq!(type_from_annotation("f32"), HirType::F32);
        assert_eq!(
            type_from_annotation("[u8]"),
            HirType::Array(Box::new(HirType::U8), ArrayKind::Dynamic)
        );
        assert_eq!(type_from_annotation("int"), HirType::Unknown);
        assert_eq!(type_from_annotation("float"), HirType::Unknown);
        assert_eq!(type_from_annotation("array<u8>"), HirType::Unknown);
    }
}
