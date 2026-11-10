//! Specialize calls whose arguments and supported bodies are compile-time values.
//! Unsupported statements, effects, recursion, or exhausted budgets decline folding.
use super::compiler::expr_to_value_env;
use crate::parsing::ast::{Expr, ExprKind, Function, Span, Stmt, StmtKind, TokenKind, Value};
use crate::utils::collections::FastMap;
use std::cell::{Cell, RefCell};

thread_local! {
    static FUNCTIONS: RefCell<FastMap<String, Function>> = RefCell::new(FastMap::default());
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn register(stmts: &[Stmt]) {
    FUNCTIONS.with(|cell| {
        let mut functions = cell.borrow_mut();
        functions.clear();
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Function(f, _) => {
                    functions.insert(f.name.clone(), f.clone());
                }
                StmtKind::Class(class, _) => {
                    for f in &class.methods {
                        functions.insert(format!("{}.{}", class.name, f.name), f.clone());
                    }
                }
                StmtKind::Extend(_, target, _, methods, _) => {
                    for f in methods {
                        functions.insert(format!("{}.{}", target, f.name), f.clone());
                    }
                }
                _ => {}
            }
        }
    });
}

pub(super) fn instance(class: &str) -> Value {
    let mut fields = FastMap::default();
    fields.insert("__class".into(), Value::Str(class.into()));
    Value::Object(std::sync::Arc::new(fields))
}

pub(super) fn construct(class: &str, args: &[Expr], env: &FastMap<String, Value>) -> Option<Value> {
    let receiver = literal(instance(class));
    let constructor = Expr {
        kind: ExprKind::Get(Box::new(receiver), "__ctor__".into()),
        span: Span::default(),
    };
    let has_constructor = FUNCTIONS.with(|f| f.borrow().contains_key(&format!("{class}.__ctor__")));
    if has_constructor {
        let (value, prints) = call(&constructor, args, env)?;
        if !prints.is_empty() {
            return None;
        }
        Some(value)
    } else {
        Some(instance(class))
    }
}

pub(super) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Str(s) => !s.is_empty(),
        v => v.as_f64().map(|n| n != 0.0 && !n.is_nan()).unwrap_or(true),
    }
}

pub(super) fn call(
    callee: &Expr,
    args: &[Expr],
    caller: &FastMap<String, Value>,
) -> Option<(Value, Vec<Stmt>)> {
    if ACTIVE.with(|active| active.get()) {
        return None;
    }
    let (key, receiver) = match &callee.kind {
        ExprKind::Variable(name) => (name.clone(), None),
        ExprKind::Get(target, method) => {
            let value = expr_to_value_env(target, caller)?;
            let Value::Object(fields) = &value else {
                return None;
            };
            let Value::Str(class) = fields.get("__class")? else {
                return None;
            };
            (format!("{class}.{method}"), Some(value))
        }
        _ => return None,
    };
    let function = FUNCTIONS.with(|functions| functions.borrow().get(&key).cloned())?;
    let mut values = Vec::new();
    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            let Value::Array(items) = expr_to_value_env(inner, caller)? else {
                return None;
            };
            values.extend(items);
        } else {
            values.push(expr_to_value_env(arg, caller)?);
        }
    }
    // Only parameters and locals are mutable here. Do not silently fold writes
    // to the caller's globals or execute host operations during compilation.
    let mut env = FastMap::default();
    if let Some(receiver) = receiver {
        env.insert("this".into(), receiver);
    }
    let mut index = 0;
    for (name, default, _) in &function.params {
        if let Some(name) = name.strip_prefix("...") {
            env.insert(name.into(), Value::Array(values[index..].to_vec()));
            index = values.len();
        } else {
            let value = values
                .get(index)
                .cloned()
                .or_else(|| default.as_ref().and_then(|e| expr_to_value_env(e, &env)))
                .unwrap_or(Value::Null);
            env.insert(name.clone(), value);
            index = (index + 1).min(values.len());
        }
    }
    ACTIVE.with(|active| active.set(true));
    let mut prints = Vec::new();
    let result = run(&function.body, &mut env, &mut prints, &mut 10_000);
    ACTIVE.with(|active| active.set(false));
    let value = result?.unwrap_or_else(|| {
        if function.is_constructor
            || key
                .rsplit_once('.')
                .is_some_and(|(class, name)| class == name)
        {
            env.get("this").cloned().unwrap_or(Value::Null)
        } else {
            Value::Null
        }
    });
    Some((value, prints))
}

fn run(
    stmts: &[Stmt],
    env: &mut FastMap<String, Value>,
    prints: &mut Vec<Stmt>,
    budget: &mut usize,
) -> Option<Option<Value>> {
    for stmt in stmts {
        *budget = budget.checked_sub(1)?;
        match &stmt.kind {
            StmtKind::Let(name, init, ..) => {
                let value = match init {
                    Some(e) => expr_to_value_env(e, env)?,
                    None => Value::Null,
                };
                env.insert(name.clone(), value);
            }
            StmtKind::Return(expr) => {
                return Some(Some(match expr {
                    Some(e) => expr_to_value_env(e, env)?,
                    None => Value::Null,
                }));
            }
            StmtKind::Block(body) => {
                if let Some(value) = run(body, env, prints, budget)? {
                    return Some(Some(value));
                }
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let branch = if truthy(&expr_to_value_env(cond, env)?) {
                    Some(then_branch.as_ref())
                } else {
                    else_branch.as_deref()
                };
                if let Some(branch) = branch {
                    if let Some(value) = run(std::slice::from_ref(branch), env, prints, budget)? {
                        return Some(Some(value));
                    }
                }
            }
            StmtKind::While { cond, body } => {
                while truthy(&expr_to_value_env(cond, env)?) {
                    *budget = budget.checked_sub(1)?;
                    if let Some(value) = run(std::slice::from_ref(body), env, prints, budget)? {
                        return Some(Some(value));
                    }
                }
            }
            StmtKind::ExprStmt(e) => match &e.kind {
                ExprKind::Assign(name, rhs) if env.contains_key(name) => {
                    let value = expr_to_value_env(rhs, env)?;
                    env.insert(name.clone(), value);
                }
                ExprKind::AssignOp(lhs, TokenKind::Equal, rhs) => {
                    let value = expr_to_value_env(rhs, env)?;
                    match &lhs.kind {
                        ExprKind::Variable(name) if env.contains_key(name) => {
                            env.insert(name.clone(), value);
                        }
                        ExprKind::Get(target, field) => {
                            let ExprKind::Variable(name) = &target.kind else {
                                return None;
                            };
                            let Value::Object(fields) = env.get_mut(name)? else {
                                return None;
                            };
                            std::sync::Arc::make_mut(fields).insert(field.clone(), value);
                        }
                        _ => return None,
                    }
                }
                ExprKind::Set(target, field, rhs) => {
                    let value = expr_to_value_env(rhs, env)?;
                    let ExprKind::Variable(name) = &target.kind else {
                        return None;
                    };
                    let Value::Object(fields) = env.get_mut(name)? else {
                        return None;
                    };
                    std::sync::Arc::make_mut(fields).insert(field.clone(), value);
                }
                ExprKind::Call(callee, args, _) => {
                    let ExprKind::Variable(name) = &callee.kind else {
                        return None;
                    };
                    if name != "print" && name != "println" {
                        return None;
                    }
                    let mut literals = Vec::new();
                    for arg in args {
                        // Keep option maps as AST maps so the normal print
                        // emitter still applies separators, styles and endings.
                        let kind = if let ExprKind::Object(entries) = &arg.kind {
                            if entries.iter().any(|(k, _)| k == "file") {
                                return None;
                            }
                            ExprKind::Object(
                                entries
                                    .iter()
                                    .map(|(k, v)| {
                                        Some((k.clone(), literal(expr_to_value_env(v, env)?)))
                                    })
                                    .collect::<Option<Vec<_>>>()?,
                            )
                        } else {
                            ExprKind::Literal(expr_to_value_env(arg, env)?)
                        };
                        literals.push(Expr {
                            kind,
                            span: arg.span.clone(),
                        });
                    }
                    prints.push(Stmt {
                        kind: StmtKind::ExprStmt(Expr {
                            kind: ExprKind::Call(callee.clone(), literals, vec![]),
                            span: e.span.clone(),
                        }),
                        span: stmt.span.clone(),
                    });
                }
                _ => return None,
            },
            _ => return None,
        }
    }
    Some(None)
}

fn literal(value: Value) -> Expr {
    Expr {
        kind: ExprKind::Literal(value),
        span: Span::default(),
    }
}
