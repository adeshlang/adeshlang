/// RAII memory management transformation pass.
/// Automatically injects `free(ptr)` calls at scope boundaries for allocated pointers.
/// Handles all control flow exits: return, break, continue, scope end.
use crate::parsing::ast::{Expr, ExprKind, Span, Stmt, StmtKind};

/// Track allocated pointers in a scope for automatic deallocation
#[derive(Debug, Clone)]
struct AllocatedPointer {
    var_name: String,
    #[allow(dead_code)]
    ty: String, // e.g., "*u8", "*i32"
}

/// Generate free() calls for a list of allocated pointers
/// Uses LIFO (reverse) order for correct destructor semantics
fn generate_free_calls(allocated_ptrs: &[AllocatedPointer]) -> Vec<Stmt> {
    allocated_ptrs
        .iter()
        .rev() // LIFO: reverse declaration order
        .map(|alloc_ptr| {
            let span = Span::default();
            Stmt {
                kind: StmtKind::ExprStmt(Expr {
                    kind: ExprKind::Call(
                        Box::new(Expr {
                            kind: ExprKind::Variable("free".to_string()),
                            span: span.clone(),
                        }),
                        vec![Expr {
                            kind: ExprKind::Variable(alloc_ptr.var_name.clone()),
                            span: span.clone(),
                        }],
                        vec![],
                    ),
                    span: span.clone(),
                }),
                span,
            }
        })
        .collect()
}

use std::collections::HashSet;

fn find_explicit_frees_in_expr(expr: &Expr, freed: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Call(callee, args, _) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if name == "free" && !args.is_empty() {
                    if let ExprKind::Variable(var) = &args[0].kind {
                        freed.insert(var.clone());
                    }
                }
            }
            find_explicit_frees_in_expr(callee, freed);
            for a in args {
                find_explicit_frees_in_expr(a, freed);
            }
        }
        ExprKind::Binary(l, _, r) => {
            find_explicit_frees_in_expr(l, freed);
            find_explicit_frees_in_expr(r, freed);
        }
        ExprKind::Assign(_name, val) => {
            find_explicit_frees_in_expr(val, freed);
        }
        _ => {}
    }
}

fn find_explicit_frees_in_stmt(stmt: &Stmt, freed: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::ExprStmt(expr) => find_explicit_frees_in_expr(expr, freed),
        StmtKind::Block(stmts) => {
            for s in stmts {
                find_explicit_frees_in_stmt(s, freed);
            }
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            find_explicit_frees_in_expr(cond, freed);
            find_explicit_frees_in_stmt(then_branch, freed);
            if let Some(eb) = else_branch {
                find_explicit_frees_in_stmt(eb, freed);
            }
        }
        StmtKind::While { cond, body } => {
            find_explicit_frees_in_expr(cond, freed);
            find_explicit_frees_in_stmt(body, freed);
        }
        StmtKind::Let(_name, Some(init), ..) => find_explicit_frees_in_expr(init, freed),
        StmtKind::UnsafeBlock(body) => find_explicit_frees_in_stmt(body, freed),
        _ => {}
    }
}

/// Transform a block to inject free() calls for allocated pointers
/// Handles early exits (return, break, continue)
/// `parent_ptrs` are pointers from enclosing scopes that must also be freed on early exit
fn transform_block_with_raii_internal(
    stmts: Vec<Stmt>,
    parent_ptrs: &[AllocatedPointer],
) -> Vec<Stmt> {
    let mut explicitly_freed = HashSet::default();
    for s in &stmts {
        find_explicit_frees_in_stmt(s, &mut explicitly_freed);
    }

    // Collect all allocated pointers in this block
    let mut allocated_ptrs = Vec::new();
    let mut transformed_stmts = Vec::new();

    // Combined list for early exit cleanup: parent pointers first, then local
    // (freed in LIFO order: locals first, then parents)
    let make_cleanup_ptrs = |locals: &[AllocatedPointer]| -> Vec<AllocatedPointer> {
        let mut all = Vec::new();
        all.extend_from_slice(locals);
        all.extend_from_slice(parent_ptrs);
        all
    };

    for stmt in stmts {
        // Check if this is an allocating let statement
        if let StmtKind::Let(name, init, type_ann, ..) = &stmt.kind {
            let is_ptr_ann = type_ann.as_ref().map_or(false, |ty| ty.starts_with('*'));
            let is_alloc_call = if let Some(init_expr) = init {
                if let ExprKind::Call(callee, _args, _) = &init_expr.kind {
                    if let ExprKind::Variable(func_name) = &callee.kind {
                        func_name == "alloc" || func_name == "alloc_typed"
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if is_alloc_call || (is_ptr_ann && is_alloc_call) {
                allocated_ptrs.push(AllocatedPointer {
                    var_name: name.clone(),
                    ty: type_ann.clone().unwrap_or_else(|| "*u8".to_string()),
                });
            }
        }

        // Handle early exit statements - inject cleanup BEFORE the exit
        // Free BOTH local and parent scope pointers
        let span = stmt.span.clone();
        let stmt_with_cleanup = match stmt.kind {
            // Return statement: inject frees before returning (preserving returned pointer)
            StmtKind::Return(expr) => {
                let ret_var = if let Some(ref e) = expr {
                    if let ExprKind::Variable(ref vname) = e.kind {
                        Some(vname.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                let cleanup_ptrs = make_cleanup_ptrs(&allocated_ptrs);
                let cleanup_filtered: Vec<AllocatedPointer> = cleanup_ptrs
                    .into_iter()
                    .filter(|p| {
                        ret_var.as_ref().map_or(true, |rv| &p.var_name != rv)
                            && !explicitly_freed.contains(&p.var_name)
                    })
                    .collect();
                let mut result_stmts = generate_free_calls(&cleanup_filtered);
                result_stmts.push(Stmt {
                    kind: StmtKind::Return(expr),
                    span: span.clone(),
                });
                if result_stmts.len() == 1 {
                    result_stmts.into_iter().next().unwrap()
                } else {
                    Stmt {
                        kind: StmtKind::Block(result_stmts),
                        span,
                    }
                }
            }

            // Break statement: inject frees before breaking
            StmtKind::Break => {
                let cleanup_ptrs = make_cleanup_ptrs(&allocated_ptrs);
                let cleanup_filtered: Vec<AllocatedPointer> = cleanup_ptrs
                    .into_iter()
                    .filter(|p| !explicitly_freed.contains(&p.var_name))
                    .collect();
                let mut result_stmts = generate_free_calls(&cleanup_filtered);
                result_stmts.push(Stmt {
                    kind: StmtKind::Break,
                    span: span.clone(),
                });
                if result_stmts.len() == 1 {
                    result_stmts.into_iter().next().unwrap()
                } else {
                    Stmt {
                        kind: StmtKind::Block(result_stmts),
                        span,
                    }
                }
            }

            // Continue statement: inject frees before continuing
            StmtKind::Continue => {
                let cleanup_ptrs = make_cleanup_ptrs(&allocated_ptrs);
                let cleanup_filtered: Vec<AllocatedPointer> = cleanup_ptrs
                    .into_iter()
                    .filter(|p| !explicitly_freed.contains(&p.var_name))
                    .collect();
                let mut result_stmts = generate_free_calls(&cleanup_filtered);
                result_stmts.push(Stmt {
                    kind: StmtKind::Continue,
                    span: span.clone(),
                });
                if result_stmts.len() == 1 {
                    result_stmts.into_iter().next().unwrap()
                } else {
                    Stmt {
                        kind: StmtKind::Block(result_stmts),
                        span,
                    }
                }
            }

            // Recursively transform nested blocks, passing current + parent ptrs
            other => {
                // Combine current allocated ptrs with parent ptrs for nested scopes
                let combined_ptrs: Vec<AllocatedPointer> = {
                    let mut v = Vec::new();
                    v.extend_from_slice(&allocated_ptrs);
                    v.extend_from_slice(parent_ptrs);
                    v
                };
                let transformed = transform_stmt_with_raii_internal(
                    Stmt {
                        kind: other,
                        span: span.clone(),
                    },
                    &combined_ptrs,
                );
                transformed
            }
        };

        transformed_stmts.push(stmt_with_cleanup);
    }

    // Inject free() calls for allocated pointers at the end of the block (scope exit)
    // Only local pointers not already explicitly freed
    let unreleased_ptrs: Vec<AllocatedPointer> = allocated_ptrs
        .into_iter()
        .filter(|p| !explicitly_freed.contains(&p.var_name))
        .collect();
    if !unreleased_ptrs.is_empty() {
        transformed_stmts.extend(generate_free_calls(&unreleased_ptrs));
    }

    transformed_stmts
}

/// Public API: transform a block with RAII cleanup (no parent pointers)
pub fn transform_block_with_raii(stmts: Vec<Stmt>) -> Vec<Stmt> {
    transform_block_with_raii_internal(stmts, &[])
}

/// Transform the entire AST to inject RAII cleanup for all blocks
pub fn transform_ast_with_raii(stmts: Vec<Stmt>) -> Vec<Stmt> {
    stmts
        .into_iter()
        .map(|stmt| transform_stmt_with_raii(stmt))
        .collect()
}

fn transform_stmt_with_raii(stmt: Stmt) -> Stmt {
    transform_stmt_with_raii_internal(stmt, &[])
}

/// Internal recursive transformation with parent scope tracking
fn transform_stmt_with_raii_internal(stmt: Stmt, parent_ptrs: &[AllocatedPointer]) -> Stmt {
    let span = stmt.span.clone();
    match stmt.kind {
        StmtKind::Block(stmts) => Stmt {
            kind: StmtKind::Block(transform_block_with_raii_internal(
                stmts
                    .into_iter()
                    .map(|s| transform_stmt_with_raii_internal(s, parent_ptrs))
                    .collect(),
                parent_ptrs,
            )),
            span,
        },
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let then_transformed = {
                let then_stmt = *then_branch;
                let then_span = then_stmt.span.clone();
                match then_stmt.kind {
                    StmtKind::Block(stmts) => Box::new(Stmt {
                        kind: StmtKind::Block(transform_block_with_raii_internal(
                            stmts
                                .into_iter()
                                .map(|s| transform_stmt_with_raii_internal(s, parent_ptrs))
                                .collect(),
                            parent_ptrs,
                        )),
                        span: then_span,
                    }),
                    other => Box::new(transform_stmt_with_raii_internal(
                        Stmt {
                            kind: other,
                            span: then_span,
                        },
                        parent_ptrs,
                    )),
                }
            };

            let else_transformed = else_branch.map(|b| {
                let else_stmt = *b;
                let else_span = else_stmt.span.clone();
                match else_stmt.kind {
                    StmtKind::Block(stmts) => Box::new(Stmt {
                        kind: StmtKind::Block(transform_block_with_raii_internal(
                            stmts
                                .into_iter()
                                .map(|s| transform_stmt_with_raii_internal(s, parent_ptrs))
                                .collect(),
                            parent_ptrs,
                        )),
                        span: else_span,
                    }),
                    other => Box::new(transform_stmt_with_raii_internal(
                        Stmt {
                            kind: other,
                            span: else_span,
                        },
                        parent_ptrs,
                    )),
                }
            });

            Stmt {
                kind: StmtKind::If {
                    cond,
                    then_branch: then_transformed,
                    else_branch: else_transformed,
                },
                span,
            }
        }
        StmtKind::While { cond, body } => {
            let body_stmt = *body;
            let body_span = body_stmt.span.clone();
            let body_transformed = match body_stmt.kind {
                StmtKind::Block(stmts) => Box::new(Stmt {
                    kind: StmtKind::Block(transform_block_with_raii_internal(
                        stmts
                            .into_iter()
                            .map(|s| transform_stmt_with_raii_internal(s, parent_ptrs))
                            .collect(),
                        parent_ptrs,
                    )),
                    span: body_span,
                }),
                other => Box::new(transform_stmt_with_raii_internal(
                    Stmt {
                        kind: other,
                        span: body_span,
                    },
                    parent_ptrs,
                )),
            };
            Stmt {
                kind: StmtKind::While {
                    cond,
                    body: body_transformed,
                },
                span,
            }
        }
        StmtKind::ForIn { name, iter, body } => {
            let body_stmt = *body;
            let body_span = body_stmt.span.clone();
            let body_transformed = match body_stmt.kind {
                StmtKind::Block(stmts) => Box::new(Stmt {
                    kind: StmtKind::Block(transform_block_with_raii_internal(
                        stmts
                            .into_iter()
                            .map(|s| transform_stmt_with_raii_internal(s, parent_ptrs))
                            .collect(),
                        parent_ptrs,
                    )),
                    span: body_span,
                }),
                other => Box::new(transform_stmt_with_raii_internal(
                    Stmt {
                        kind: other,
                        span: body_span,
                    },
                    parent_ptrs,
                )),
            };
            Stmt {
                kind: StmtKind::ForIn {
                    name,
                    iter,
                    body: body_transformed,
                },
                span,
            }
        }
        StmtKind::UnsafeBlock(body) => {
            let body_stmt = *body;
            let body_span = body_stmt.span.clone();
            let body_transformed = match body_stmt.kind {
                StmtKind::Block(stmts) => Box::new(Stmt {
                    kind: StmtKind::Block(transform_block_with_raii_internal(
                        stmts
                            .into_iter()
                            .map(|s| transform_stmt_with_raii_internal(s, parent_ptrs))
                            .collect(),
                        parent_ptrs,
                    )),
                    span: body_span,
                }),
                other => Box::new(transform_stmt_with_raii_internal(
                    Stmt {
                        kind: other,
                        span: body_span,
                    },
                    parent_ptrs,
                )),
            };
            Stmt {
                kind: StmtKind::UnsafeBlock(body_transformed),
                span,
            }
        }
        other => Stmt { kind: other, span },
    }
}
