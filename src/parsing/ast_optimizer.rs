//! AST Optimizer
//!
//! Performs local optimizations on the AST prior to lowering or emission:
//! - Constant folding for arithmetic, comparisons, and booleans
//! - Algebraic simplifications (identity/zero elements)
//! - Strength reduction for select patterns (e.g., x*2 → x+x)
//! - Dead-code elimination for constant conditions
use super::ast::{
    Expr, ExprKind, ShareDecl, Stmt, StmtKind, StrongDecl, TokenKind, Value, WeakDecl,
};

/// Optimized AST folder with constant folding, algebraic simplifications,
/// and strength reduction transformations.
fn fold_expr(e: &Expr) -> Expr {
    let kind = match &e.kind {
        ExprKind::Binary(l, op, r) => {
            let fl = fold_expr(l);
            let fr = fold_expr(r);

            // First, try constant folding
            match (&fl.kind, op, &fr.kind) {
                // Constant folding for arithmetic
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::Plus,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(a + b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::Minus,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(a - b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::Star,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(a * b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::Slash,
                    ExprKind::Literal(Value::Number(b)),
                ) if *b != 0.0 => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(a / b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::Percent,
                    ExprKind::Literal(Value::Number(b)),
                ) if *b != 0.0 => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(a % b)),
                        span: e.span.clone(),
                    };
                }

                // Constant folding for comparisons
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::Less,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(a < b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::LessEqual,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(a <= b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::Greater,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(a > b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::GreaterEqual,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(a >= b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::EqualEqual,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool((a - b).abs() < f64::EPSILON)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Number(a)),
                    TokenKind::BangEqual,
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool((a - b).abs() >= f64::EPSILON)),
                        span: e.span.clone(),
                    };
                }

                // Constant folding for booleans
                (
                    ExprKind::Literal(Value::Bool(a)),
                    TokenKind::EqualEqual,
                    ExprKind::Literal(Value::Bool(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(a == b)),
                        span: e.span.clone(),
                    };
                }
                (
                    ExprKind::Literal(Value::Bool(a)),
                    TokenKind::BangEqual,
                    ExprKind::Literal(Value::Bool(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(a != b)),
                        span: e.span.clone(),
                    };
                }

                // String constant folding
                (
                    ExprKind::Literal(Value::Str(a)),
                    TokenKind::Plus,
                    ExprKind::Literal(Value::Str(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Str(format!("{}{}", a, b))),
                        span: e.span.clone(),
                    };
                }

                _ => {}
            }

            // Algebraic simplifications - identity and zero element rules
            match (&fl.kind, op, &fr.kind) {
                // x + 0 = x, 0 + x = x
                (_, TokenKind::Plus, ExprKind::Literal(Value::Number(n))) if *n == 0.0 => {
                    return fl;
                }
                (ExprKind::Literal(Value::Number(n)), TokenKind::Plus, _) if *n == 0.0 => {
                    return fr;
                }

                // x - 0 = x
                (_, TokenKind::Minus, ExprKind::Literal(Value::Number(n))) if *n == 0.0 => {
                    return fl;
                }

                // x * 0 = 0, 0 * x = 0
                (_, TokenKind::Star, ExprKind::Literal(Value::Number(n))) if *n == 0.0 => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(0.0)),
                        span: e.span.clone(),
                    };
                }
                (ExprKind::Literal(Value::Number(n)), TokenKind::Star, _) if *n == 0.0 => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(0.0)),
                        span: e.span.clone(),
                    };
                }

                // x * 1 = x, 1 * x = x
                (_, TokenKind::Star, ExprKind::Literal(Value::Number(n))) if *n == 1.0 => {
                    return fl;
                }
                (ExprKind::Literal(Value::Number(n)), TokenKind::Star, _) if *n == 1.0 => {
                    return fr;
                }

                // x / 1 = x
                (_, TokenKind::Slash, ExprKind::Literal(Value::Number(n))) if *n == 1.0 => {
                    return fl;
                }

                // Strength reduction: x * 2 = x + x (cheaper on some architectures)
                (_, TokenKind::Star, ExprKind::Literal(Value::Number(n))) if *n == 2.0 => {
                    ExprKind::Binary(Box::new(fl.clone()), TokenKind::Plus, Box::new(fl))
                }
                (ExprKind::Literal(Value::Number(n)), TokenKind::Star, _) if *n == 2.0 => {
                    ExprKind::Binary(Box::new(fr.clone()), TokenKind::Plus, Box::new(fr))
                }
                _ => ExprKind::Binary(Box::new(fl), *op, Box::new(fr)),
            }
        }
        ExprKind::Grouping(g) => return fold_expr(g),
        ExprKind::Array(xs) => ExprKind::Array(xs.iter().map(fold_expr).collect()),
        ExprKind::Tuple(xs) => ExprKind::Tuple(xs.iter().map(fold_expr).collect()),
        ExprKind::SetLiteral(xs) => ExprKind::SetLiteral(xs.iter().map(fold_expr).collect()),
        ExprKind::Object(kv) => {
            ExprKind::Object(kv.iter().map(|(k, v)| (k.clone(), fold_expr(v))).collect())
        }
        ExprKind::StructLiteral(name, kv) => ExprKind::StructLiteral(
            name.clone(),
            kv.iter().map(|(k, v)| (k.clone(), fold_expr(v))).collect(),
        ),
        ExprKind::Call(c, args, _type_args) => ExprKind::Call(
            Box::new(fold_expr(c)),
            args.iter().map(fold_expr).collect(),
            _type_args.clone(),
        ),
        ExprKind::Unary(op, r) => {
            let fr = fold_expr(r);
            match (op, &fr.kind) {
                // Constant folding for unary minus
                (TokenKind::Minus, ExprKind::Literal(Value::Number(n))) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(-n)),
                        span: e.span.clone(),
                    };
                }
                // Double negation elimination: --x = x
                (TokenKind::Minus, ExprKind::Unary(TokenKind::Minus, inner)) => {
                    return (**inner).clone();
                }
                // Constant folding for unary not
                (TokenKind::Bang, ExprKind::Literal(Value::Bool(b))) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(!b)),
                        span: e.span.clone(),
                    };
                }
                // Double not elimination: !!x = x (when x is already bool)
                (TokenKind::Bang, ExprKind::Unary(TokenKind::Bang, inner)) => {
                    return (**inner).clone();
                }
                // Unary plus is identity for numbers
                (TokenKind::Plus, ExprKind::Literal(Value::Number(n))) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Number(*n)),
                        span: e.span.clone(),
                    };
                }
                _ => ExprKind::Unary(*op, Box::new(fr)),
            }
        }
        ExprKind::Logical(l, op, r) => {
            let fl = fold_expr(l);
            let fr = fold_expr(r);
            match (op, &fl.kind, &fr.kind) {
                // constant boolean folding
                (
                    TokenKind::And,
                    ExprKind::Literal(Value::Bool(a)),
                    ExprKind::Literal(Value::Bool(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(*a && *b)),
                        span: e.span.clone(),
                    };
                }
                (
                    TokenKind::Or,
                    ExprKind::Literal(Value::Bool(a)),
                    ExprKind::Literal(Value::Bool(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(*a || *b)),
                        span: e.span.clone(),
                    };
                }
                // short-circuit when left is constant
                (TokenKind::And, ExprKind::Literal(Value::Bool(false)), _) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(false)),
                        span: e.span.clone(),
                    };
                }
                (TokenKind::And, ExprKind::Literal(Value::Bool(true)), _) => return fr,
                (TokenKind::Or, ExprKind::Literal(Value::Bool(true)), _) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool(true)),
                        span: e.span.clone(),
                    };
                }
                (TokenKind::Or, ExprKind::Literal(Value::Bool(false)), _) => return fr,
                // numeric truthiness for simple cases
                (
                    TokenKind::And,
                    ExprKind::Literal(Value::Number(a)),
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool((*a != 0.0) && (*b != 0.0))),
                        span: e.span.clone(),
                    };
                }
                (
                    TokenKind::Or,
                    ExprKind::Literal(Value::Number(a)),
                    ExprKind::Literal(Value::Number(b)),
                ) => {
                    return Expr {
                        kind: ExprKind::Literal(Value::Bool((*a != 0.0) || (*b != 0.0))),
                        span: e.span.clone(),
                    };
                }
                (TokenKind::And, ExprKind::Literal(Value::Number(a)), _) => {
                    if *a == 0.0 {
                        return Expr {
                            kind: ExprKind::Literal(Value::Bool(false)),
                            span: e.span.clone(),
                        };
                    } else {
                        ExprKind::Logical(
                            Box::new(Expr {
                                kind: ExprKind::Literal(Value::Number(*a)),
                                span: e.span.clone(),
                            }),
                            *op,
                            Box::new(fr),
                        )
                    }
                }
                (TokenKind::Or, ExprKind::Literal(Value::Number(a)), _) => {
                    if *a != 0.0 {
                        return Expr {
                            kind: ExprKind::Literal(Value::Bool(true)),
                            span: e.span.clone(),
                        };
                    } else {
                        return fr;
                    }
                }
                _ => ExprKind::Logical(Box::new(fl), *op, Box::new(fr)),
            }
        }
        ExprKind::Assign(n, rhs) => ExprKind::Assign(n.clone(), Box::new(fold_expr(rhs))),
        ExprKind::Get(obj, key) => ExprKind::Get(Box::new(fold_expr(obj)), key.clone()),
        ExprKind::Set(obj, key, v) => ExprKind::Set(
            Box::new(fold_expr(obj)),
            key.clone(),
            Box::new(fold_expr(v)),
        ),
        ExprKind::Fn(params, body, is_async) => {
            let nb: Vec<Stmt> = body.iter().map(fold_stmt).collect();
            // Optimize: wrap in Arc
            ExprKind::Fn(params.clone(), std::sync::Arc::new(nb), *is_async)
        }
        ExprKind::New(ctor, args) => ExprKind::New(
            Box::new(fold_expr(ctor)),
            args.iter().map(fold_expr).collect(),
        ),
        ExprKind::Await(x) => ExprKind::Await(Box::new(fold_expr(x))),
        ExprKind::Spawn(x) => ExprKind::Spawn(Box::new(fold_expr(x))),
        ExprKind::Throw(x) => ExprKind::Throw(Box::new(fold_expr(x))),
        _ => e.kind.clone(),
    };
    Expr {
        kind,
        span: e.span.clone(),
    }
}

fn fold_stmt(s: &Stmt) -> Stmt {
    let kind = match &s.kind {
        StmtKind::ShareDeclaration(decl, exp) => StmtKind::ShareDeclaration(
            ShareDecl {
                name: decl.name.clone(),
                expr: fold_expr(&decl.expr),
                type_ann: decl.type_ann.clone(),
            },
            *exp,
        ),
        StmtKind::StrongDeclaration(decl, exp) => StmtKind::StrongDeclaration(
            StrongDecl {
                name: decl.name.clone(),
                expr: fold_expr(&decl.expr),
                type_ann: decl.type_ann.clone(),
            },
            *exp,
        ),
        StmtKind::WeakDeclaration(decl, exp) => StmtKind::WeakDeclaration(
            WeakDecl {
                name: decl.name.clone(),
                expr: fold_expr(&decl.expr),
                type_ann: decl.type_ann.clone(),
            },
            *exp,
        ),
        StmtKind::ExprStmt(e) => StmtKind::ExprStmt(fold_expr(e)),
        StmtKind::Let(name, init, ann, exp, cst, readonly) => {
            let ni = init.as_ref().map(|e| fold_expr(e));
            StmtKind::Let(name.clone(), ni, ann.clone(), *exp, *cst, *readonly)
        }
        StmtKind::Block(bs) => StmtKind::Block(bs.iter().map(fold_stmt).collect()),
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let fc = fold_expr(cond);
            // dead code elimination for constant conditions
            match &fc.kind {
                ExprKind::Literal(Value::Bool(true)) => return fold_stmt(then_branch),
                ExprKind::Literal(Value::Bool(false)) => {
                    if let Some(el) = else_branch {
                        return fold_stmt(el);
                    } else {
                        return Stmt {
                            kind: StmtKind::Block(vec![]),
                            span: s.span.clone(),
                        };
                    }
                }
                ExprKind::Literal(Value::Number(n)) => {
                    if *n != 0.0 {
                        return fold_stmt(then_branch);
                    } else {
                        if let Some(el) = else_branch {
                            return fold_stmt(el);
                        } else {
                            return Stmt {
                                kind: StmtKind::Block(vec![]),
                                span: s.span.clone(),
                            };
                        }
                    }
                }
                _ => StmtKind::If {
                    cond: fc,
                    then_branch: Box::new(fold_stmt(then_branch)),
                    else_branch: else_branch.as_ref().map(|b| Box::new(fold_stmt(b))),
                },
            }
        }
        StmtKind::While { cond, body } => {
            let fc = fold_expr(cond);
            match &fc.kind {
                ExprKind::Literal(Value::Bool(false)) => StmtKind::Block(vec![]),
                ExprKind::Literal(Value::Number(n)) if *n == 0.0 => StmtKind::Block(vec![]),
                _ => StmtKind::While {
                    cond: fc,
                    body: Box::new(fold_stmt(body)),
                },
            }
        }
        StmtKind::ForIn { name, iter, body } => StmtKind::ForIn {
            name: name.clone(),
            iter: fold_expr(iter),
            body: Box::new(fold_stmt(body)),
        },
        StmtKind::Function(f, exp) => StmtKind::Function(f.clone(), *exp),
        StmtKind::Class(c, exp) => StmtKind::Class(c.clone(), *exp),
        StmtKind::Struct(st, exp) => StmtKind::Struct(st.clone(), *exp),
        StmtKind::Enum(en, exp) => StmtKind::Enum(en.clone(), *exp),
        StmtKind::Interface(i, exp) => StmtKind::Interface(i.clone(), *exp),
        StmtKind::TypeAlias(ta, exp) => StmtKind::TypeAlias(ta.clone(), *exp),
        StmtKind::Return(e) => StmtKind::Return(e.as_ref().map(|x| fold_expr(x))),
        StmtKind::Import { path, alias } => StmtKind::Import {
            path: path.clone(),
            alias: alias.clone(),
        },
        StmtKind::ImportDefault { path, alias } => StmtKind::ImportDefault {
            path: path.clone(),
            alias: alias.clone(),
        },
        StmtKind::ImportNames { path, names } => StmtKind::ImportNames {
            path: path.clone(),
            names: names.clone(),
        },
        StmtKind::ExportDefaultFunction(f) => StmtKind::ExportDefaultFunction(f.clone()),
        StmtKind::ExportDefaultClass(c) => StmtKind::ExportDefaultClass(c.clone()),
        StmtKind::ExportDefault(name) => StmtKind::ExportDefault(name.clone()),
        StmtKind::Extend(n, t, m, exp) => StmtKind::Extend(n.clone(), t.clone(), m.clone(), *exp),
        StmtKind::TryCatch {
            try_block,
            err_name,
            catch_block,
        } => StmtKind::TryCatch {
            try_block: Box::new(fold_stmt(try_block)),
            err_name: err_name.clone(),
            catch_block: Box::new(fold_stmt(catch_block)),
        },
        StmtKind::Break => StmtKind::Break,
        StmtKind::Continue => StmtKind::Continue,
        StmtKind::Jump(e) => StmtKind::Jump(fold_expr(e)),
        StmtKind::LetTuple(names, type_anns, init, exp, is_const, is_readonly) => {
            StmtKind::LetTuple(
                names.clone(),
                type_anns.clone(),
                init.as_ref().map(fold_expr),
                *exp,
                *is_const,
                *is_readonly,
            )
        }
        StmtKind::LetObject(bindings, init, exp, is_const, is_readonly) => StmtKind::LetObject(
            bindings.clone(),
            init.as_ref().map(fold_expr),
            *exp,
            *is_const,
            *is_readonly,
        ),
        // FFI: pass-through
        StmtKind::HeaderImport { path } => StmtKind::HeaderImport { path: path.clone() },
        StmtKind::ExternFunction(decl) => StmtKind::ExternFunction(decl.clone()),
        StmtKind::ExternBlock { abi, functions } => StmtKind::ExternBlock {
            abi: abi.clone(),
            functions: functions.clone(),
        },
        StmtKind::Region { name, body } => StmtKind::Region {
            name: name.clone(),
            body: Box::new(fold_stmt(body)),
        },
        StmtKind::UnsafeBlock(body) => StmtKind::UnsafeBlock(Box::new(fold_stmt(body))),
        StmtKind::Defer(body) => StmtKind::Defer(Box::new(fold_stmt(body))),
        StmtKind::Decorator(def, is_export) => StmtKind::Decorator(def.clone(), *is_export),
    };
    Stmt {
        kind,
        span: s.span.clone(),
    }
}

pub fn optimize_program(stmts: &Vec<Stmt>) -> Vec<Stmt> {
    stmts.iter().map(fold_stmt).collect()
}
