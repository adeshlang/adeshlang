//! Drop Insertion Pass (Phase 3)
//!
//! Plans drop operations at lifetime boundaries based on ownership and
//! borrowing information. This provides a conservative plan which can be
//! enriched with finer-grained scope/lifetime info in follow-ups.

use crate::parsing::hir::{HirExpr, HirFunction, HirStmt};
use crate::stdlib::create_stdlib;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    ScopeExit,
    Return,
    RegionExit,
}

impl DropReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ScopeExit => "scope_exit",
            Self::Return => "return",
            Self::RegionExit => "region_exit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropEvent {
    pub var: String,
    pub location: String,
    pub reason: DropReason,
}

/// Drop order per scope level (LIFO within each scope)
#[derive(Debug, Clone, Default)]
pub struct ScopedDrops {
    pub drops_by_scope: Vec<Vec<String>>, // Stack of scope drop lists (LIFO within scope)
}

#[derive(Debug, Default, Clone)]
pub struct DropPlan {
    pub planned: usize,
    pub events: Vec<DropEvent>,
}

impl DropPlan {
    /// Generate a debug report showing drop order and reasons
    pub fn debug_report(&self) -> String {
        let mut report = String::from("=== Drop Plan Report ===\n");
        report.push_str(&format!("Total drops planned: {}\n\n", self.planned));

        for (idx, event) in self.events.iter().enumerate() {
            report.push_str(&format!(
                "[{}] {} at {} ({})\n",
                idx + 1,
                event.var,
                event.location,
                event.reason.as_str()
            ));
        }
        report
    }

    /// Check if a variable has a drop event
    pub fn has_drop(&self, var: &str) -> bool {
        self.events.iter().any(|e| e.var == var)
    }

    /// Get all drop events for a specific variable
    pub fn drops_for(&self, var: &str) -> Vec<&DropEvent> {
        self.events.iter().filter(|e| e.var == var).collect()
    }

    /// Get drops at a specific location
    pub fn drops_at_location(&self, loc: &str) -> Vec<&DropEvent> {
        self.events.iter().filter(|e| e.location == loc).collect()
    }
}

/// Drop planner: orchestrates drop event planning
#[derive(Debug, Default)]
pub struct DropPlanner {
    builtin_names: HashSet<String>,
    module_fn_names: HashSet<String>,
}

impl DropPlanner {
    pub fn new() -> Self {
        let registry = create_stdlib();
        let builtin_names: HashSet<String> = registry
            .all_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            builtin_names,
            module_fn_names: HashSet::new(),
        }
    }

    pub fn new_with_fn_names(fn_names: HashSet<String>) -> Self {
        let registry = create_stdlib();
        let builtin_names: HashSet<String> = registry
            .all_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            builtin_names,
            module_fn_names: fn_names,
        }
    }

    /// Compute a drop plan for a given HIR function.
    ///
    /// This scan tracks owned locals introduced via `let` (when not
    /// marked as borrowed) and emits drop events when leaving scopes or
    /// returning. Moves detected in simple assignments discount the owner.
    pub fn plan(&self, func: &HirFunction) -> DropPlan {
        let mut plan = DropPlan::default();
        let mut scopes = ScopedDrops {
            drops_by_scope: vec![Vec::new()],
        };

        self.plan_block("fn".to_string(), &func.body, &mut scopes, &mut plan);

        plan.planned = plan.events.len();
        plan
    }

    fn plan_block(
        &self,
        loc: String,
        block: &std::sync::Arc<Vec<HirStmt>>,
        scopes: &mut ScopedDrops,
        plan: &mut DropPlan,
    ) {
        for (idx, stmt) in block.iter().enumerate() {
            let this_loc = format!("{loc}.{idx}");
            self.plan_stmt(this_loc, stmt, scopes, plan);
        }
    }

    fn plan_stmt(
        &self,
        loc: String,
        stmt: &HirStmt,
        scopes: &mut ScopedDrops,
        plan: &mut DropPlan,
    ) {
        match stmt {
            HirStmt::Let {
                name,
                init,
                is_borrowed,
                ..
            } => {
                let needs_drop = is_borrowed.is_none()
                    || init.as_ref().map_or(false, |e| {
                        matches!(e, HirExpr::Share(_) | HirExpr::Downgrade(_))
                    });
                if needs_drop {
                    if let Some(top) = scopes.drops_by_scope.last_mut() {
                        top.push(name.clone());
                    }
                }
                if let Some(expr) = init {
                    self.visit_expr_moves(expr, scopes);
                }
            }
            HirStmt::Assign {
                target,
                value,
                is_move,
            } => {
                // If moving from a local variable, discount its ownership so it won't be dropped here
                if *is_move {
                    if let HirExpr::LoadVar(src) = value {
                        self.remove_from_scopes(src, scopes);
                    }
                }
                // Visiting both sides for nested moves/borrows indicator
                self.visit_expr_moves(target, scopes);
                self.visit_expr_moves(value, scopes);
            }
            HirStmt::Block(stmts) => {
                scopes.drops_by_scope.push(Vec::new());
                for (i, s) in stmts.iter().enumerate() {
                    let inner_loc = format!("{loc}.b{i}");
                    self.plan_stmt(inner_loc, s, scopes, plan);
                }
                // Scope exit: drop locals from this scope
                if let Some(locals) = scopes.drops_by_scope.pop() {
                    for v in locals {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::ScopeExit,
                        });
                    }
                }
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.visit_expr_moves(cond, scopes);
                scopes.drops_by_scope.push(Vec::new());
                self.plan_stmt(format!("{loc}.then"), then_branch, scopes, plan);
                if let Some(locals) = scopes.drops_by_scope.pop() {
                    for v in locals {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::ScopeExit,
                        });
                    }
                }
                if let Some(else_b) = else_branch {
                    scopes.drops_by_scope.push(Vec::new());
                    self.plan_stmt(format!("{loc}.else"), else_b, scopes, plan);
                    if let Some(locals) = scopes.drops_by_scope.pop() {
                        for v in locals {
                            plan.events.push(DropEvent {
                                var: v,
                                location: loc.clone(),
                                reason: DropReason::ScopeExit,
                            });
                        }
                    }
                }
            }
            HirStmt::While { cond, body } => {
                self.visit_expr_moves(cond, scopes);
                scopes.drops_by_scope.push(Vec::new());
                self.plan_stmt(format!("{loc}.loop"), body, scopes, plan);
                if let Some(locals) = scopes.drops_by_scope.pop() {
                    for v in locals {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::ScopeExit,
                        });
                    }
                }
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.visit_expr_moves(iter, scopes);
                scopes.drops_by_scope.push(Vec::new());
                self.plan_stmt(format!("{loc}.forin"), body, scopes, plan);
                if let Some(locals) = scopes.drops_by_scope.pop() {
                    for v in locals {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::ScopeExit,
                        });
                    }
                }
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                // Handle try block
                scopes.drops_by_scope.push(Vec::new());
                self.plan_stmt(format!("{loc}.try"), try_block, scopes, plan);
                if let Some(locals) = scopes.drops_by_scope.pop() {
                    for v in locals {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::ScopeExit,
                        });
                    }
                }
                // Handle catch block
                scopes.drops_by_scope.push(Vec::new());
                self.plan_stmt(format!("{loc}.catch"), catch_block, scopes, plan);
                if let Some(locals) = scopes.drops_by_scope.pop() {
                    for v in locals {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::ScopeExit,
                        });
                    }
                }
            }
            HirStmt::Region { body, .. } => {
                scopes.drops_by_scope.push(Vec::new());
                self.plan_stmt(format!("{loc}.region"), body, scopes, plan);
                if let Some(locals) = scopes.drops_by_scope.pop() {
                    for v in locals {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::RegionExit,
                        });
                    }
                }
            }
            HirStmt::Unsafe(inner) => {
                self.plan_stmt(format!("{loc}.unsafe"), inner, scopes, plan);
            }
            HirStmt::Return(_) => {
                // On return: drop everything still owned in all open scopes
                for scope_locals in scopes.drops_by_scope.iter_mut().rev() {
                    for v in std::mem::take(scope_locals).into_iter() {
                        plan.events.push(DropEvent {
                            var: v,
                            location: loc.clone(),
                            reason: DropReason::Return,
                        });
                    }
                }
            }
            HirStmt::Expr(e) => {
                self.visit_expr_moves(e, scopes);
            }
            _ => {}
        }
    }

    /// Track moves and usage through expressions
    fn visit_expr_moves(&self, expr: &HirExpr, scopes: &mut ScopedDrops) {
        match expr {
            HirExpr::Call(func, args, _) => {
                // If the function expression is a simple variable load (a function name),
                // treat it as a non-move use here (no warning) and don't traverse into it.
                if let HirExpr::LoadVar(_) = func.as_ref() {
                    // skip visiting the function identifier to avoid false-positive move warnings
                } else {
                    self.visit_expr_moves(func, scopes);
                }
                // Arguments: detect if any are moved
                for arg in args {
                    if let HirExpr::LoadVar(var) = arg {
                        // Conservative: assume function consumes argument (move)
                        self.remove_from_scopes(var, scopes);
                    } else {
                        self.visit_expr_moves(arg, scopes);
                    }
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.visit_expr_moves(obj, scopes);
                for arg in args {
                    if let HirExpr::LoadVar(var) = arg {
                        self.remove_from_scopes(var, scopes);
                    } else {
                        self.visit_expr_moves(arg, scopes);
                    }
                }
            }
            HirExpr::LoadVar(name) => {
                // Don't warn for builtin functions, module names, or class/type names (uppercase)
                if self.builtin_names.contains(name)
                    || self.module_fn_names.contains(name)
                    || name.chars().next().map_or(false, |c| c.is_uppercase())
                {
                    return;
                }
                if !self.is_in_scopes(name, scopes) {
                    eprintln!("Warning: possible use-after-move of '{name}'");
                }
            }
            HirExpr::Lambda(_, body, _) => {
                // Treat closures as capturing by reference (bodies don't drop captured vars here)
                let mut nested = ScopedDrops {
                    drops_by_scope: vec![Vec::new()],
                };
                for s in body.iter() {
                    self.plan_stmt("closure".into(), s, &mut nested, &mut DropPlan::default());
                }
            }
            _ => {
                // Fall through to general expression visitation
                self.visit_expr_generic(expr, scopes);
            }
        }
    }

    /// General expression visitation for nested structure traversal
    fn visit_expr_generic(&self, expr: &HirExpr, scopes: &mut ScopedDrops) {
        match expr {
            HirExpr::BinaryOp(l, _, r) => {
                self.visit_expr_generic(l, scopes);
                self.visit_expr_generic(r, scopes);
            }
            HirExpr::UnaryOp(_, e)
            | HirExpr::Await(e)
            | HirExpr::Spawn(e)
            | HirExpr::BorrowImmut(e)
            | HirExpr::BorrowMut(e)
            | HirExpr::Deref(e)
            | HirExpr::NonNull(e)
            | HirExpr::Share(e)
            | HirExpr::Downgrade(e) => {
                self.visit_expr_generic(e, scopes);
            }
            HirExpr::ArrayLiteral(items)
            | HirExpr::SetLiteral(items)
            | HirExpr::TupleLiteral(items) => {
                for it in items {
                    self.visit_expr_generic(it, scopes);
                }
            }
            HirExpr::DictLiteral(kvs) => {
                for (k, v) in kvs {
                    self.visit_expr_generic(k, scopes);
                    self.visit_expr_generic(v, scopes);
                }
            }
            HirExpr::Index(a, b) => {
                self.visit_expr_generic(a, scopes);
                self.visit_expr_generic(b, scopes);
            }
            HirExpr::MemberAccess(a, _) => {
                self.visit_expr_generic(a, scopes);
            }
            HirExpr::SetMember(a, _, b) => {
                self.visit_expr_generic(a, scopes);
                self.visit_expr_generic(b, scopes);
            }
            HirExpr::Conditional(a, b, c) => {
                self.visit_expr_generic(a, scopes);
                self.visit_expr_generic(b, scopes);
                self.visit_expr_generic(c, scopes);
            }
            HirExpr::Range(a, b, _) => {
                self.visit_expr_generic(a, scopes);
                self.visit_expr_generic(b, scopes);
            }
            HirExpr::Spread(a) => {
                self.visit_expr_generic(a, scopes);
            }
            HirExpr::OptionalGet(a, _) => {
                self.visit_expr_generic(a, scopes);
            }
            HirExpr::Format(a, _) => {
                self.visit_expr_generic(a, scopes);
            }
            _ => {}
        }
    }

    /// Remove a variable from the current scope's owned set
    fn remove_from_scopes(&self, var: &str, scopes: &mut ScopedDrops) {
        for scope_locals in scopes.drops_by_scope.iter_mut().rev() {
            if let Some(pos) = scope_locals.iter().position(|v| v == var) {
                scope_locals.remove(pos);
                return;
            }
        }
    }

    /// Check if a variable is still owned in any active scope
    fn is_in_scopes(&self, var: &str, scopes: &ScopedDrops) -> bool {
        scopes
            .drops_by_scope
            .iter()
            .any(|scope| scope.iter().any(|v| v == var))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::hir::{HirExpr, HirFunction, HirStmt};

    #[test]
    fn planner_smoke() {
        // Empty function → no drops
        let f = HirFunction {
            name: "f".into(),
            params: vec![],
            body: std::sync::Arc::new(vec![] as Vec<HirStmt>),
            ret_type: None,
            is_async: false,
            decorators: vec![],
            is_exported: false,
            move_params: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };
        let planner = DropPlanner::new();
        let plan = planner.plan(&f);
        assert_eq!(plan.planned, 0);
        assert!(plan.events.is_empty());
    }

    #[test]
    fn drop_simple_let_on_return() {
        // let x; return; → x should be dropped at return
        let body = vec![
            HirStmt::Let {
                name: "x".into(),
                ty: None,
                init: None,
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(None),
        ];
        let f = HirFunction {
            name: "g".into(),
            params: vec![],
            body: std::sync::Arc::new(body),
            ret_type: None,
            is_async: false,
            decorators: vec![],
            is_exported: false,
            move_params: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };
        let planner = DropPlanner::new();
        let plan = planner.plan(&f);
        assert!(plan.planned >= 1);
        assert!(
            plan.events
                .iter()
                .any(|e| e.var == "x" && matches!(e.reason, DropReason::Return))
        );
    }

    #[test]
    fn drop_move_excludes_variable() {
        // let x; let y = x (move); return; → only y should be dropped, not x
        let body = vec![
            HirStmt::Let {
                name: "x".into(),
                ty: None,
                init: None,
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Assign {
                target: HirExpr::LoadVar("y".to_string()),
                value: HirExpr::LoadVar("x".to_string()),
                is_move: true,
            },
            HirStmt::Return(None),
        ];
        let f = HirFunction {
            name: "h".into(),
            params: vec![],
            body: std::sync::Arc::new(body),
            ret_type: None,
            is_async: false,
            decorators: vec![],
            is_exported: false,
            move_params: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };
        let planner = DropPlanner::new();
        let plan = planner.plan(&f);
        // x should NOT have a drop (ownership moved to y)
        assert!(!plan.events.iter().any(|e| e.var == "x"));
    }

    #[test]
    fn drop_plan_diagnostics() {
        // Test diagnostic helpers
        let plan = DropPlan {
            planned: 2,
            events: vec![
                DropEvent {
                    var: "a".into(),
                    location: "fn.0".into(),
                    reason: DropReason::ScopeExit,
                },
                DropEvent {
                    var: "b".into(),
                    location: "fn.1".into(),
                    reason: DropReason::Return,
                },
            ],
        };

        assert_eq!(plan.planned, 2);
        assert!(plan.has_drop("a"));
        assert!(!plan.has_drop("c"));
        assert_eq!(plan.drops_for("a").len(), 1);
        assert_eq!(plan.drops_at_location("fn.0").len(), 1);

        let report = plan.debug_report();
        assert!(report.contains("Total drops planned: 2"));
        assert!(report.contains("scope_exit"));
    }
}
