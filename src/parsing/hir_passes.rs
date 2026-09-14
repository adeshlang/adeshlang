//! HIR Passes - Analysis and Optimization
//!
//! This module provides HIR-level analysis and optimization passes:
//! - Ownership checking: enforces Rust-like ownership rules (move semantics, single owner)
//! - Borrow checking: enforces borrowing rules (multiple immutable OR one mutable)
//! - Lifetime validation: ensures references don't escape their owner scope
//! - Escape analysis: determines if values need heap allocation
//! - Constant folding: evaluates constant expressions at compile time
//! - Dead code elimination: removes unreachable code
//!
//! These passes prepare the HIR for efficient LIR lowering and provide
//! memory safety without garbage collection.

use super::hir::{
    BinOp, HirClass, HirExpr, HirFunction, HirLiteral, HirModule, HirStmt, HirType, UnaryOp,
};
use super::ownership::{OwnershipAnalysis, OwnershipError, check_move_semantics, check_ownership};
use crate::parsing::borrow_inference::{BorrowMode, infer_borrow_modes};
use crate::parsing::drop_insertion::{DropPlan, DropPlanner};
use crate::parsing::error::{ErrorKind, LangError, RelatedLocation};
use crate::parsing::variance::{Variance, summarize_variance};
use crate::utils::memory::LifetimeTracker;
use std::collections::{HashMap, HashSet};

// ============================================
// HIR PASS RESULTS
// ============================================

/// Complete result of all HIR passes
#[derive(Debug, Clone)]
pub struct HirPassResults {
    /// Ownership analysis results
    pub ownership: Option<OwnershipAnalysis>,
    /// Whether ownership checking passed
    pub ownership_ok: bool,
    /// Whether lifetime validation passed
    pub lifetimes_ok: bool,
    /// Errors from ownership checking
    pub ownership_errors: Vec<OwnershipError>,
    /// Errors from lifetime validation
    pub lifetime_errors: Vec<LifetimeError>,
}

/// Helper to find a whole-word identifier column in a source line
fn find_variable_token_in_line(line: &str, var: &str) -> Option<usize> {
    let mut search_start = 0;
    while let Some(idx) = line[search_start..].find(var) {
        let actual_idx = search_start + idx;
        let before_ok = if actual_idx == 0 {
            true
        } else {
            let ch = line[..actual_idx].chars().last().unwrap();
            !ch.is_alphanumeric() && ch != '_'
        };
        let after_idx = actual_idx + var.len();
        let after_ok = if after_idx >= line.len() {
            true
        } else {
            let ch = line[after_idx..].chars().next().unwrap();
            !ch.is_alphanumeric() && ch != '_'
        };
        if before_ok && after_ok {
            return Some(actual_idx + 1);
        }
        search_start = actual_idx + 1;
        if search_start >= line.len() {
            break;
        }
    }
    None
}

/// Helper to find all occurrences of a variable in the source code
fn find_var_occurrences(src: &str, var: &str) -> Vec<(usize, usize, String)> {
    let mut results = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        if let Some(col) = find_variable_token_in_line(line, var) {
            results.push((idx + 1, col, line.to_string()));
        }
    }
    results
}

/// Format HIR safety errors using rich, colored LangError diagnostics
fn format_hir_safety_errors(
    results: &HirPassResults,
    file: Option<&str>,
    src: Option<&str>,
) -> String {
    let source_content = src
        .map(|s| s.to_string())
        .or_else(|| file.and_then(|f| std::fs::read_to_string(f).ok()))
        .unwrap_or_default();

    let mut formatted_errors = Vec::new();
    let mut seen_messages = HashSet::new();

    for err in &results.ownership_errors {
        let var_name = match err {
            OwnershipError::UseAfterMove { variable, .. } => variable,
            OwnershipError::MoveWhileBorrowed { variable, .. } => variable,
            OwnershipError::BorrowAfterMove { variable, .. } => variable,
            OwnershipError::BorrowWhileMutablyBorrowed { variable, .. } => variable,
            OwnershipError::MutableBorrowWhileBorrowed { variable, .. } => variable,
            OwnershipError::MutableBorrowWhileMutablyBorrowed { variable, .. } => variable,
            OwnershipError::UnknownVariable { variable } => variable,
            OwnershipError::MultipleMutableBorrows { variable, .. } => variable,
        };

        let occurrences = find_var_occurrences(&source_content, var_name);

        let lang_err = match err {
            OwnershipError::UseAfterMove { variable, .. } => {
                // Find move site (e.g. `prev = curr`) and use site (in expression / return / loop)
                let mut move_occ = None;
                let mut use_occ = None;

                for occ in &occurrences {
                    let line_t = &occ.2;
                    let is_let = line_t.trim_start().starts_with("let ");
                    let is_move_assign = (line_t.contains(&format!("= {}", variable))
                        || line_t.contains(&format!("={}", variable)))
                        && !line_t.contains(&format!("let {} =", variable))
                        && !line_t.contains(&format!("let {}=", variable));
                    if is_move_assign && move_occ.is_none() {
                        move_occ = Some(occ.clone());
                    } else if !is_let && move_occ.is_some() && use_occ.is_none() {
                        use_occ = Some(occ.clone());
                    }
                }

                // If no use found after move, look for any expression use
                if use_occ.is_none() {
                    for occ in &occurrences {
                        if Some(occ.0) != move_occ.as_ref().map(|o| o.0) {
                            use_occ = Some(occ.clone());
                            break;
                        }
                    }
                }

                let primary = use_occ.or_else(|| occurrences.last().cloned()).unwrap_or((
                    1,
                    1,
                    String::new(),
                ));
                let related = move_occ.or_else(|| occurrences.first().cloned());

                let mut le = LangError::located(
                    ErrorKind::Ownership,
                    format!("use of moved value `{}`", variable),
                    file.map(String::from),
                    primary.0,
                    primary.1,
                    primary.2,
                )
                .with_code("E0382");
                le.end_col = primary.1 + variable.len();

                if let Some(rel) = related {
                    if rel.0 != primary.0 || rel.1 != primary.1 {
                        le = le.with_related(RelatedLocation::new(
                            file.map(String::from),
                            rel.0,
                            rel.1,
                            rel.2,
                            format!("value `{}` moved here", variable),
                        ));
                    }
                }

                le.push_note("ownership is exclusive: assigning a variable transfers ownership (move semantics)")
                    .with_help(format!(
                        "AdeshLang variables transfer ownership on assignment. To avoid moving `{}`, use shared reference (e.g. `share`), clone, or update variables together with tuple assignment (e.g. `prev, curr = curr, next`).",
                        variable
                    ))
            }

            OwnershipError::MoveWhileBorrowed { variable, .. } => {
                let primary = occurrences.last().cloned().unwrap_or((1, 1, String::new()));
                let mut le = LangError::located(
                    ErrorKind::Ownership,
                    format!("cannot move `{}` while it is borrowed", variable),
                    file.map(String::from),
                    primary.0,
                    primary.1,
                    primary.2,
                )
                .with_code("E0505");
                le.end_col = primary.1 + variable.len();
                le.push_note(format!("variable `{}` is currently borrowed", variable))
                    .with_help(format!(
                        "ensure all borrows of `{}` are finished before moving it",
                        variable
                    ))
            }

            OwnershipError::BorrowAfterMove { variable, .. } => {
                let primary = occurrences.last().cloned().unwrap_or((1, 1, String::new()));
                let mut le = LangError::located(
                    ErrorKind::Ownership,
                    format!("cannot borrow `{}` after it has been moved", variable),
                    file.map(String::from),
                    primary.0,
                    primary.1,
                    primary.2,
                )
                .with_code("E0382");
                le.end_col = primary.1 + variable.len();
                le.push_note(format!("value `{}` was moved here", variable))
                    .with_help(format!("borrow `{}` before transferring its ownership, or use shared references (`share`)", variable))
            }

            OwnershipError::BorrowWhileMutablyBorrowed { variable, .. }
            | OwnershipError::MutableBorrowWhileBorrowed { variable, .. } => {
                let primary = occurrences.last().cloned().unwrap_or((1, 1, String::new()));
                let mut le = LangError::located(
                    ErrorKind::Ownership,
                    format!(
                        "cannot borrow `{}` because it is already borrowed",
                        variable
                    ),
                    file.map(String::from),
                    primary.0,
                    primary.1,
                    primary.2,
                )
                .with_code("E0502");
                le.end_col = primary.1 + variable.len();
                le.push_note("mutable and immutable borrows cannot coexist simultaneously")
                    .with_help("restructure your code so that borrows do not overlap")
            }

            OwnershipError::MutableBorrowWhileMutablyBorrowed { variable, .. }
            | OwnershipError::MultipleMutableBorrows { variable, .. } => {
                let primary = occurrences.last().cloned().unwrap_or((1, 1, String::new()));
                let mut le = LangError::located(
                    ErrorKind::Ownership,
                    format!(
                        "cannot borrow `{}` as mutable more than once at a time",
                        variable
                    ),
                    file.map(String::from),
                    primary.0,
                    primary.1,
                    primary.2,
                )
                .with_code("E0499");
                le.end_col = primary.1 + variable.len();
                le.push_note(format!(
                    "first mutable borrow of `{}` is still active",
                    variable
                ))
                .with_help("only one mutable borrow is permitted in any scope at a time")
            }

            OwnershipError::UnknownVariable { variable } => {
                let primary = occurrences
                    .first()
                    .cloned()
                    .unwrap_or((1, 1, String::new()));
                let mut le = LangError::located(
                    ErrorKind::Ownership,
                    format!("cannot find value `{}` in this scope", variable),
                    file.map(String::from),
                    primary.0,
                    primary.1,
                    primary.2,
                )
                .with_code("E0425");
                le.end_col = primary.1 + variable.len();
                le.with_help(format!("declare `{}` with `let` before using it", variable))
            }
        };

        let formatted = format!("{}", lang_err);
        if seen_messages.insert(formatted.clone()) {
            formatted_errors.push(formatted);
        }
    }

    for err in &results.lifetime_errors {
        let var_name = &err.variable;
        let occurrences = find_var_occurrences(&source_content, var_name);

        let mut assign_occ = None;
        let mut borrow_occ = None;

        for occ in &occurrences {
            let line_t = &occ.2;
            let is_let = line_t.trim_start().starts_with("let ");
            if (line_t.contains(&format!("{} =", var_name))
                || line_t.contains(&format!("{}=", var_name)))
                && !is_let
                && assign_occ.is_none()
            {
                assign_occ = Some(occ.clone());
            }
            if (line_t.contains(&format!("&{}", var_name))
                || line_t.contains(&format!("&mut {}", var_name)))
                && borrow_occ.is_none()
            {
                borrow_occ = Some(occ.clone());
            }
        }

        let primary = assign_occ
            .or_else(|| occurrences.last().cloned())
            .unwrap_or((1, 1, String::new()));
        let related = borrow_occ.or_else(|| occurrences.first().cloned());

        let mut le = LangError::located(
            ErrorKind::Ownership,
            format!("cannot assign to `{}` because it is borrowed", var_name),
            file.map(String::from),
            primary.0,
            primary.1,
            primary.2,
        )
        .with_code("E0506");
        le.end_col = primary.1 + var_name.len();

        if let Some(rel) = related {
            if rel.0 != primary.0 || rel.1 != primary.1 {
                le = le.with_related(RelatedLocation::new(
                    file.map(String::from),
                    rel.0,
                    rel.1,
                    rel.2,
                    format!("borrow of `{}` occurs here", var_name),
                ));
            }
        }

        le = le
            .push_note(format!("variable `{}` is borrowed and cannot be mutated while active references exist", var_name))
            .with_help(format!(
                "ensure all references to `{}` are no longer in use before assigning a new value, or remove the reference `&{}`",
                var_name, var_name
            ));

        let formatted = format!("{}", le);
        if seen_messages.insert(formatted.clone()) {
            formatted_errors.push(formatted);
        }
    }

    formatted_errors.join("\n\n")
}

/// Run all HIR safety passes with source location tracking for diagnostics
pub fn run_safety_passes_with_location(
    module: &HirModule,
    check_ownership_enabled: bool,
    check_moves_enabled: bool,
    file: Option<&str>,
    src: Option<&str>,
) -> Result<HirPassResults, String> {
    let mut results = HirPassResults {
        ownership: None,
        ownership_ok: true,
        lifetimes_ok: true,
        ownership_errors: Vec::new(),
        lifetime_errors: Vec::new(),
    };

    // Run ownership checking
    if check_ownership_enabled {
        match check_ownership(module) {
            Ok(analysis) => {
                results.ownership = Some(analysis);
            }
            Err(errors) => {
                results.ownership_ok = false;
                results.ownership_errors = errors;
            }
        }
    }

    // Run move semantics checking
    if check_moves_enabled {
        if let Err(errors) = check_move_semantics(module) {
            results.ownership_ok = false;
            results.ownership_errors.extend(errors);
        }
    }

    // Run borrow checking
    if let Err(errors) = check_borrows(module) {
        results.lifetimes_ok = false;
        results.lifetime_errors = errors;
    }

    // Run lifetime validation
    if let Err(errors) = validate_lifetimes(module) {
        results.lifetimes_ok = false;
        results.lifetime_errors.extend(errors);
    }

    // Return results or formatted errors
    if !results.ownership_ok || !results.lifetimes_ok {
        let formatted = format_hir_safety_errors(&results, file, src);
        if formatted.is_empty() {
            Err("HIR safety checks failed".to_string())
        } else {
            Err(formatted)
        }
    } else {
        Ok(results)
    }
}

/// Run all HIR safety passes (backwards-compatible overload)
pub fn run_safety_passes(
    module: &HirModule,
    check_ownership_enabled: bool,
    check_moves_enabled: bool,
) -> Result<HirPassResults, String> {
    run_safety_passes_with_location(
        module,
        check_ownership_enabled,
        check_moves_enabled,
        None,
        None,
    )
}

// ============================================
// BORROW CHECKING PASS
// ============================================

/// Track borrow state for each variable
#[derive(Debug, Clone, PartialEq)]
enum BorrowState {
    Owned,
    BorrowedImmut { count: usize, escapes: bool },
    BorrowedMut { borrowed_at_line: usize },
}

/// Borrow checking context
#[derive(Debug, Clone)]
struct BorrowContext {
    borrows: HashMap<String, BorrowState>,
    scope_depth: usize,
    scopes: Vec<HashMap<String, BorrowState>>,
}

impl BorrowContext {
    fn new() -> Self {
        BorrowContext {
            borrows: HashMap::new(),
            scope_depth: 0,
            scopes: Vec::new(),
        }
    }

    fn enter_scope(&mut self) {
        self.scope_depth += 1;
        self.scopes.push(self.borrows.clone());
    }

    fn exit_scope(&mut self) {
        if self.scope_depth > 0 {
            self.scope_depth -= 1;
            if let Some(saved) = self.scopes.pop() {
                self.borrows = saved;
            }
        }
    }
}

/// Borrow check an entire module
fn check_borrows(module: &HirModule) -> Result<(), Vec<LifetimeError>> {
    let mut ctx = BorrowContext::new();
    let mut errors = Vec::new();

    for stmt in &module.statements {
        check_stmt_borrows(stmt, &mut ctx, &mut errors);
    }

    for func in &module.functions {
        let mut func_ctx = BorrowContext::new();
        for param in &func.params {
            func_ctx.borrows.insert(param.0.clone(), BorrowState::Owned);
        }
        for stmt in func.body.iter() {
            check_stmt_borrows(stmt, &mut func_ctx, &mut errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check borrows in a statement
fn check_stmt_borrows(stmt: &HirStmt, ctx: &mut BorrowContext, errors: &mut Vec<LifetimeError>) {
    match stmt {
        HirStmt::Let { name, init, .. } => {
            ctx.borrows.insert(name.clone(), BorrowState::Owned);
            if let Some(expr) = init {
                check_expr_borrows(expr, ctx, errors);
            }
        }
        HirStmt::Assign { target, value, .. } => {
            // Check that target isn't currently borrowed
            if let HirExpr::LoadVar(var_name) = target {
                if let Some(state) = ctx.borrows.get(var_name) {
                    match state {
                        BorrowState::BorrowedImmut { .. } | BorrowState::BorrowedMut { .. } => {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Cannot assign to borrowed variable '{}'",
                                    var_name
                                ),
                                variable: var_name.clone(),
                                scope: ctx.scope_depth as u32,
                            });
                        }
                        BorrowState::Owned => {}
                    }
                }
            }
            check_expr_borrows(value, ctx, errors);
        }
        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            check_expr_borrows(cond, ctx, errors);
            ctx.enter_scope();
            check_stmt_borrows(then_branch, ctx, errors);
            ctx.exit_scope();

            if let Some(else_b) = else_branch {
                ctx.enter_scope();
                check_stmt_borrows(else_b, ctx, errors);
                ctx.exit_scope();
            }
        }
        HirStmt::While { cond, body } => {
            check_expr_borrows(cond, ctx, errors);
            ctx.enter_scope();
            check_stmt_borrows(body, ctx, errors);
            ctx.exit_scope();
        }
        HirStmt::Block(stmts) => {
            ctx.enter_scope();
            for s in stmts {
                check_stmt_borrows(s, ctx, errors);
            }
            ctx.exit_scope();
        }
        HirStmt::Region { body, .. } => {
            ctx.enter_scope();
            check_stmt_borrows(body, ctx, errors);
            ctx.exit_scope();
        }
        HirStmt::Unsafe(body) => {
            check_stmt_borrows(body, ctx, errors);
        }
        HirStmt::Expr(expr) => {
            check_expr_borrows(expr, ctx, errors);
        }
        _ => {}
    }
}

/// Check borrows in an expression
fn check_expr_borrows(expr: &HirExpr, ctx: &mut BorrowContext, errors: &mut Vec<LifetimeError>) {
    match expr {
        // Handle unified Borrow expression with auto-inference
        HirExpr::Borrow(inner, is_exclusive) => {
            check_expr_borrows(inner, ctx, errors);
            if let HirExpr::LoadVar(var_name) = &**inner {
                if let Some(current_state) = ctx.borrows.get(var_name).cloned() {
                    match current_state {
                        BorrowState::Owned => {
                            if *is_exclusive {
                                ctx.borrows.insert(
                                    var_name.clone(),
                                    BorrowState::BorrowedMut {
                                        borrowed_at_line: 0,
                                    },
                                );
                            } else {
                                ctx.borrows.insert(
                                    var_name.clone(),
                                    BorrowState::BorrowedImmut {
                                        count: 1,
                                        escapes: false,
                                    },
                                );
                            }
                        }
                        BorrowState::BorrowedImmut { count, escapes } => {
                            if *is_exclusive {
                                errors.push(LifetimeError {
                                    message: format!("Cannot get exclusive access to '{}' while shared borrow exists", var_name),
                                    variable: var_name.clone(),
                                    scope: ctx.scope_depth as u32,
                                });
                            } else {
                                ctx.borrows.insert(
                                    var_name.clone(),
                                    BorrowState::BorrowedImmut {
                                        count: count + 1,
                                        escapes,
                                    },
                                );
                            }
                        }
                        BorrowState::BorrowedMut { .. } => {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Cannot borrow '{}' while exclusive borrow exists",
                                    var_name
                                ),
                                variable: var_name.clone(),
                                scope: ctx.scope_depth as u32,
                            });
                        }
                    }
                } else {
                    errors.push(LifetimeError {
                        message: format!("Variable '{}' not found in borrow context", var_name),
                        variable: var_name.clone(),
                        scope: ctx.scope_depth as u32,
                    });
                }
            }
        }
        // Legacy: handle BorrowImmut
        HirExpr::BorrowImmut(inner) => {
            check_expr_borrows(inner, ctx, errors);
            if let HirExpr::LoadVar(var_name) = &**inner {
                if let Some(current_state) = ctx.borrows.get(var_name).cloned() {
                    match current_state {
                        BorrowState::Owned => {
                            ctx.borrows.insert(
                                var_name.clone(),
                                BorrowState::BorrowedImmut {
                                    count: 1,
                                    escapes: false,
                                },
                            );
                        }
                        BorrowState::BorrowedImmut { count, escapes } => {
                            ctx.borrows.insert(
                                var_name.clone(),
                                BorrowState::BorrowedImmut {
                                    count: count + 1,
                                    escapes,
                                },
                            );
                        }
                        BorrowState::BorrowedMut { .. } => {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Cannot borrow '{}' immutably while mutably borrowed",
                                    var_name
                                ),
                                variable: var_name.clone(),
                                scope: ctx.scope_depth as u32,
                            });
                        }
                    }
                } else {
                    errors.push(LifetimeError {
                        message: format!("Variable '{}' not found in borrow context", var_name),
                        variable: var_name.clone(),
                        scope: ctx.scope_depth as u32,
                    });
                }
            }
        }
        HirExpr::BorrowMut(inner) => {
            check_expr_borrows(inner, ctx, errors);
            if let HirExpr::LoadVar(var_name) = &**inner {
                if let Some(current_state) = ctx.borrows.get(var_name).cloned() {
                    match current_state {
                        BorrowState::Owned => {
                            ctx.borrows.insert(
                                var_name.clone(),
                                BorrowState::BorrowedMut {
                                    borrowed_at_line: 0,
                                },
                            );
                        }
                        BorrowState::BorrowedImmut { .. } => {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Cannot borrow '{}' mutably: immutably borrowed",
                                    var_name
                                ),
                                variable: var_name.clone(),
                                scope: ctx.scope_depth as u32,
                            });
                        }
                        BorrowState::BorrowedMut { .. } => {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Cannot have multiple mutable borrows of '{}'",
                                    var_name
                                ),
                                variable: var_name.clone(),
                                scope: ctx.scope_depth as u32,
                            });
                        }
                    }
                } else {
                    errors.push(LifetimeError {
                        message: format!("Variable '{}' not found in borrow context", var_name),
                        variable: var_name.clone(),
                        scope: ctx.scope_depth as u32,
                    });
                }
            }
        }
        HirExpr::Deref(inner) => {
            check_expr_borrows(inner, ctx, errors);
        }
        HirExpr::BinaryOp(left, _, right) => {
            check_expr_borrows(left, ctx, errors);
            check_expr_borrows(right, ctx, errors);
        }
        HirExpr::Call(func, args, _) => {
            check_expr_borrows(func, ctx, errors);
            for arg in args {
                check_expr_borrows(arg, ctx, errors);
            }
        }
        HirExpr::MemberAccess(obj, _) => {
            check_expr_borrows(obj, ctx, errors);
        }
        HirExpr::Cast(inner, _) => {
            check_expr_borrows(inner, ctx, errors);
        }
        _ => {}
    }
}

// ============================================
// LIFETIME VALIDATION PASS
// ============================================

/// Errors from lifetime validation
#[derive(Debug, Clone)]
pub struct LifetimeError {
    pub message: String,
    pub variable: String,
    pub scope: u32,
}

// ============================================
// PHASE 3 PASSES (Variance, Borrow Inference, Drop Planning)
// ============================================

/// Compute a variance summary for each function's return type.
/// Keyed by function name. Missing return types are treated as bivariant.
pub fn summarize_function_return_variance(module: &HirModule) -> HashMap<String, Variance> {
    let mut out = HashMap::new();
    for f in &module.functions {
        let v = match &f.ret_type {
            Some(t) => summarize_variance(t),
            None => Variance::Bivariant,
        };
        out.insert(f.name.clone(), v);
    }
    out
}

/// Infer borrow modes for each function's parameters.
/// Keyed by function name. Unknown/missing parameter types default to Move.
pub fn infer_function_borrow_modes(module: &HirModule) -> HashMap<String, Vec<BorrowMode>> {
    let mut out = HashMap::new();
    for f in &module.functions {
        let mut tys = Vec::with_capacity(f.params.len());
        for (_, opt_ty, _) in &f.params {
            tys.push(opt_ty.clone().unwrap_or(super::hir::HirType::Unknown));
        }
        out.insert(f.name.clone(), infer_borrow_modes(&tys));
    }
    out
}

/// Produce a basic drop plan for each function using the `DropPlanner`.
/// Keyed by function name. Current planner returns an empty plan.
pub fn plan_function_drops(module: &HirModule) -> HashMap<String, DropPlan> {
    let mut out = HashMap::new();
    // Provide the drop planner with the set of module-level function names
    let mut fn_names: HashSet<String> = module.functions.iter().map(|f| f.name.clone()).collect();
    fn_names.insert("__top_level_wrapper".to_string());
    let planner = DropPlanner::new_with_fn_names(fn_names);
    for f in &module.functions {
        let plan = planner.plan(f);
        out.insert(f.name.clone(), plan);
    }

    // Also plan drops for top-level statements using a dummy function representation
    let top_level_func = HirFunction {
        name: "__top_level_wrapper".to_string(),
        params: Vec::new(),
        body: std::sync::Arc::new(module.statements.clone()),
        ret_type: None,
        decorators: Vec::new(),
        is_async: false,
        is_exported: false,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };
    let top_plan = planner.plan(&top_level_func);
    out.insert("__top_level_wrapper".to_string(), top_plan);

    out
}

/// Aggregated Phase 3 results for convenience.
#[derive(Debug, Clone)]
pub struct Phase3Results {
    pub return_variance: HashMap<String, Variance>,
    pub borrow_modes: HashMap<String, Vec<BorrowMode>>,
    pub drop_plans: HashMap<String, DropPlan>,
}

impl Phase3Results {
    /// Generate a comprehensive diagnostic report of Phase 3 analysis results
    pub fn diagnostic_report(&self) -> String {
        let mut report = String::from("=== Phase 3 Analyses Summary ===\n\n");

        report.push_str(&format!(
            "Functions analyzed: {}\n",
            self.return_variance.len()
        ));
        report.push_str("─────────────────────────────\n\n");

        // Borrow modes report
        report.push_str("Borrow Mode Inference:\n");
        for (func, modes) in &self.borrow_modes {
            report.push_str(&format!("  {}: ", func));
            let mode_strs: Vec<_> = modes.iter().map(|m| format!("{:?}", m)).collect();
            report.push_str(&mode_strs.join(", "));
            report.push('\n');
        }
        report.push('\n');

        // Variance report
        report.push_str("Return Variance Summary:\n");
        for (func, var) in &self.return_variance {
            report.push_str(&format!("  {}: {:?}\n", func, var));
        }
        report.push('\n');

        // Drop plans report
        report.push_str("Drop Plans:\n");
        for (func, plan) in &self.drop_plans {
            report.push_str(&format!("  {}: {} drops planned\n", func, plan.planned));
            for event in &plan.events {
                report.push_str(&format!(
                    "    - {} at {} ({})\n",
                    event.var,
                    event.location,
                    event.reason.as_str()
                ));
            }
        }

        report
    }

    /// Get summary statistics about Phase 3 results
    pub fn summary(&self) -> String {
        let total_funcs = self.return_variance.len();
        let total_drops: usize = self.drop_plans.values().map(|p| p.planned).sum();

        format!(
            "Phase 3 Summary: {} functions, {} total drops planned",
            total_funcs, total_drops
        )
    }
}

/// Run all Phase 3 analyses and return their results.
pub fn run_phase3_passes(module: &HirModule) -> Phase3Results {
    Phase3Results {
        return_variance: summarize_function_return_variance(module),
        borrow_modes: infer_function_borrow_modes(module),
        drop_plans: plan_function_drops(module),
    }
}

/// Result of lifetime validation
pub type LifetimeResult = Result<(), Vec<LifetimeError>>;

/// Validate lifetimes in an HIR module
///
/// Ensures that:
/// - No reference escapes its owner scope (unless explicitly escapable)
/// - Closures capture variables correctly
/// - Return values don't reference local variables
pub fn validate_lifetimes(module: &HirModule) -> LifetimeResult {
    let mut tracker = LifetimeTracker::new();
    let mut errors = Vec::new();

    // Validate all statements
    for stmt in &module.statements {
        validate_stmt_lifetimes(stmt, &mut tracker, &mut errors);
    }

    // Validate functions
    for func in &module.functions {
        validate_function_lifetimes(func, &mut tracker, &mut errors);
    }

    // Validate classes
    for class in &module.classes {
        validate_class_lifetimes(class, &mut tracker, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_stmt_lifetimes(
    stmt: &HirStmt,
    tracker: &mut LifetimeTracker,
    errors: &mut Vec<LifetimeError>,
) {
    match stmt {
        HirStmt::Let { name, init, .. } => {
            // Register variable in current scope
            tracker.register_var(name.clone(), false);

            // Validate initializer
            if let Some(expr) = init {
                validate_expr_lifetimes(expr, tracker, errors);
            }
        }

        HirStmt::LetTuple { names, init, .. } => {
            // Register all variables
            for name in names {
                tracker.register_var(name.clone(), false);
            }

            if let Some(expr) = init {
                validate_expr_lifetimes(expr, tracker, errors);
            }
        }

        HirStmt::Assign {
            target,
            value,
            is_move: _,
        } => {
            validate_expr_lifetimes(target, tracker, errors);
            validate_expr_lifetimes(value, tracker, errors);
        }

        HirStmt::Expr(expr) => {
            validate_expr_lifetimes(expr, tracker, errors);
        }

        HirStmt::Return(expr_opt) => {
            if let Some(expr) = expr_opt {
                // Check that returned value doesn't reference local scope
                validate_return_lifetime(expr, tracker, errors);
            }
        }

        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            validate_expr_lifetimes(cond, tracker, errors);

            // Enter then scope
            tracker.enter_scope();
            validate_stmt_lifetimes(then_branch, tracker, errors);
            tracker.exit_scope();

            // Enter else scope if present
            if let Some(else_stmt) = else_branch {
                tracker.enter_scope();
                validate_stmt_lifetimes(else_stmt, tracker, errors);
                tracker.exit_scope();
            }
        }

        HirStmt::While { cond, body } => {
            validate_expr_lifetimes(cond, tracker, errors);

            tracker.enter_scope();
            validate_stmt_lifetimes(body, tracker, errors);
            tracker.exit_scope();
        }

        HirStmt::ForIn { var, iter, body } => {
            validate_expr_lifetimes(iter, tracker, errors);

            tracker.enter_scope();
            tracker.register_var(var.clone(), false);
            validate_stmt_lifetimes(body, tracker, errors);
            tracker.exit_scope();
        }

        HirStmt::Block(stmts) => {
            tracker.enter_scope();
            for s in stmts {
                validate_stmt_lifetimes(s, tracker, errors);
            }
            tracker.exit_scope();
        }

        HirStmt::FunctionDef { name, body, .. } => {
            // Functions create escapable bindings (closures can capture them)
            tracker.register_var(name.clone(), true);

            tracker.enter_scope();
            // Note: params would be registered here in a fuller implementation
            for stmt in body.iter() {
                validate_stmt_lifetimes(stmt, tracker, errors);
            }
            tracker.exit_scope();
        }

        HirStmt::ClassDef(class) => {
            // Classes are always escapable
            tracker.register_var(class.name.clone(), true);
            // Class methods are validated separately
        }

        HirStmt::TryCatch {
            try_block,
            error_name,
            catch_block,
        } => {
            tracker.enter_scope();
            validate_stmt_lifetimes(try_block, tracker, errors);
            tracker.exit_scope();

            tracker.enter_scope();
            tracker.register_var(error_name.clone(), false);
            validate_stmt_lifetimes(catch_block, tracker, errors);
            tracker.exit_scope();
        }

        HirStmt::Throw(expr) => {
            validate_expr_lifetimes(expr, tracker, errors);
        }

        HirStmt::Extend { methods, .. } => {
            for method in methods {
                validate_function_lifetimes(method, tracker, errors);
            }
        }

        HirStmt::Import { alias, .. } | HirStmt::ImportDefault { alias, .. } => {
            tracker.register_var(alias.clone(), true);
        }

        HirStmt::ImportNames { names, .. } => {
            for name in names {
                tracker.register_var(name.clone(), true);
            }
        }

        HirStmt::Region { name, body } => {
            // Region creates its own scope
            tracker.register_var(name.clone(), false);
            tracker.enter_scope();
            validate_stmt_lifetimes(body, tracker, errors);
            tracker.exit_scope();
        }

        HirStmt::Unsafe(stmt) => {
            // Unsafe block - still validate lifetimes but disable borrow checking
            validate_stmt_lifetimes(stmt, tracker, errors);
        }

        HirStmt::Defer(block) => {
            // Defer block - validate lifetimes of captured variables
            // Variables captured in defer must be valid at scope exit
            tracker.enter_scope();
            validate_stmt_lifetimes(block, tracker, errors);
            tracker.exit_scope();
        }

        HirStmt::Break | HirStmt::Continue => {}
    }
}

fn validate_expr_lifetimes(
    expr: &HirExpr,
    tracker: &mut LifetimeTracker,
    errors: &mut Vec<LifetimeError>,
) {
    match expr {
        HirExpr::Literal(_) => {}

        HirExpr::LoadVar(name) => {
            // Check variable exists and has valid lifetime
            if tracker.get_lifetime(name).is_none() {
                // Unknown variable - might be a builtin, which is OK
            }
        }

        HirExpr::BinaryOp(left, _, right) => {
            validate_expr_lifetimes(left, tracker, errors);
            validate_expr_lifetimes(right, tracker, errors);
        }

        HirExpr::UnaryOp(_, expr) => {
            validate_expr_lifetimes(expr, tracker, errors);
        }

        HirExpr::Call(callee, args, _) => {
            validate_expr_lifetimes(callee, tracker, errors);
            for arg in args {
                validate_expr_lifetimes(arg, tracker, errors);
            }
        }

        HirExpr::MethodCall(obj, _, args) => {
            validate_expr_lifetimes(obj, tracker, errors);
            for arg in args {
                validate_expr_lifetimes(arg, tracker, errors);
            }
        }

        HirExpr::ArrayLiteral(elements) => {
            for elem in elements {
                validate_expr_lifetimes(elem, tracker, errors);
            }
        }

        HirExpr::ObjectLiteral(fields) => {
            for (_, value) in fields {
                validate_expr_lifetimes(value, tracker, errors);
            }
        }

        HirExpr::StructLiteral(_, fields) => {
            for (_, value) in fields {
                validate_expr_lifetimes(value, tracker, errors);
            }
        }

        HirExpr::Index(obj, index) => {
            validate_expr_lifetimes(obj, tracker, errors);
            validate_expr_lifetimes(index, tracker, errors);
        }

        HirExpr::MemberAccess(obj, _) => {
            validate_expr_lifetimes(obj, tracker, errors);
        }

        HirExpr::SetMember(obj, _, value) => {
            validate_expr_lifetimes(obj, tracker, errors);
            validate_expr_lifetimes(value, tracker, errors);
        }

        HirExpr::Conditional(cond, then_expr, else_expr) => {
            validate_expr_lifetimes(cond, tracker, errors);
            validate_expr_lifetimes(then_expr, tracker, errors);
            validate_expr_lifetimes(else_expr, tracker, errors);
        }

        HirExpr::StoreVar(_, value) => {
            validate_expr_lifetimes(value, tracker, errors);
        }

        HirExpr::Lambda(params, body, _) => {
            // Lambda creates a new scope
            tracker.enter_scope();
            for (param_name, _) in params {
                tracker.register_var(param_name.clone(), false);
            }
            for stmt in body.iter() {
                validate_stmt_lifetimes(stmt, tracker, errors);
            }
            tracker.exit_scope();
        }

        HirExpr::NewInstance(_, args) => {
            for arg in args {
                validate_expr_lifetimes(arg, tracker, errors);
            }
        }

        HirExpr::Await(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Spawn(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Range(start, end, _) => {
            validate_expr_lifetimes(start, tracker, errors);
            validate_expr_lifetimes(end, tracker, errors);
        }

        HirExpr::Spread(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::OptionalGet(obj, _) => {
            validate_expr_lifetimes(obj, tracker, errors);
        }

        HirExpr::NonNull(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Update(inner, _, _) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Match(scrutinee, arms) => {
            validate_expr_lifetimes(scrutinee, tracker, errors);
            for (_, arm_expr) in arms {
                validate_expr_lifetimes(arm_expr, tracker, errors);
            }
        }

        HirExpr::Format(inner, _) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::This | HirExpr::Super => {}

        HirExpr::DictLiteral(entries) => {
            for (k, v) in entries {
                validate_expr_lifetimes(k, tracker, errors);
                validate_expr_lifetimes(v, tracker, errors);
            }
        }

        HirExpr::SetLiteral(elements) => {
            for elem in elements {
                validate_expr_lifetimes(elem, tracker, errors);
            }
        }

        HirExpr::TupleLiteral(elements) => {
            for elem in elements {
                validate_expr_lifetimes(elem, tracker, errors);
            }
        }

        HirExpr::Borrow(inner, _is_exclusive) => {
            validate_expr_lifetimes(inner, tracker, errors);
            // Check that borrowed value has sufficient lifetime
            if let HirExpr::LoadVar(name) = inner.as_ref() {
                if let Some(lifetime) = tracker.get_lifetime(name) {
                    // Borrows must not escape their owner's scope
                    if let Some(current) = tracker.current_scope() {
                        if lifetime.scope.0 > current.0 {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Borrowed reference to '{}' may escape its scope",
                                    name
                                ),
                                variable: name.clone(),
                                scope: current.0,
                            });
                        }
                    }
                }
            }
        }

        HirExpr::BorrowImmut(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::BorrowMut(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Deref(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Share(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Downgrade(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }

        HirExpr::Alloc(_, size) => {
            validate_expr_lifetimes(size, tracker, errors);
        }

        HirExpr::Free(ptr) => {
            validate_expr_lifetimes(ptr, tracker, errors);
        }

        HirExpr::Move(inner) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }
        HirExpr::Cast(inner, _) => {
            validate_expr_lifetimes(inner, tracker, errors);
        }
        HirExpr::AssignTuple(_, target) => {
            validate_expr_lifetimes(target, tracker, errors);
        }
        HirExpr::AssignObject(_, target) => {
            validate_expr_lifetimes(target, tracker, errors);
        }
    }
}

fn validate_return_lifetime(
    expr: &HirExpr,
    tracker: &LifetimeTracker,
    errors: &mut Vec<LifetimeError>,
) {
    // Check if the returned expression references a local variable
    // that cannot escape
    fn check_escapable(expr: &HirExpr, tracker: &LifetimeTracker, errors: &mut Vec<LifetimeError>) {
        match expr {
            // Returning a plain variable (move) is allowed; only borrows may be invalid.
            // Borrows of local variables cannot escape
            HirExpr::Borrow(inner, _is_exclusive) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    if let Some(lt) = tracker.get_lifetime(name) {
                        // Local variable borrows cannot be returned
                        if lt.scope.0 > 0 && !lt.can_escape {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Cannot return borrow of local variable '{}' - reference would outlive owner",
                                    name
                                ),
                                variable: name.clone(),
                                scope: lt.scope.0,
                            });
                        }
                    }
                }
            }
            HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    if let Some(lt) = tracker.get_lifetime(name) {
                        if lt.scope.0 > 0 && !lt.can_escape {
                            errors.push(LifetimeError {
                                message: format!(
                                    "Cannot return borrow of local variable '{}' - reference would outlive owner",
                                    name
                                ),
                                variable: name.clone(),
                                scope: lt.scope.0,
                            });
                        }
                    }
                }
            }
            // For other expressions, recursively check
            HirExpr::BinaryOp(l, _, r) => {
                check_escapable(l, tracker, errors);
                check_escapable(r, tracker, errors);
            }
            HirExpr::ArrayLiteral(elems) => {
                for e in elems {
                    check_escapable(e, tracker, errors);
                }
            }
            HirExpr::ObjectLiteral(fields) => {
                for (_, v) in fields {
                    check_escapable(v, tracker, errors);
                }
            }
            HirExpr::StructLiteral(_, fields) => {
                for (_, v) in fields {
                    check_escapable(v, tracker, errors);
                }
            }
            HirExpr::Conditional(_, then_expr, else_expr) => {
                check_escapable(then_expr, tracker, errors);
                check_escapable(else_expr, tracker, errors);
            }
            HirExpr::Cast(inner, _) => {
                check_escapable(inner, tracker, errors);
            }
            _ => {}
        }
    }

    check_escapable(expr, tracker, errors);
}

fn validate_function_lifetimes(
    func: &HirFunction,
    tracker: &mut LifetimeTracker,
    errors: &mut Vec<LifetimeError>,
) {
    tracker.enter_scope();

    // Register parameters
    for (param_name, _, _) in &func.params {
        tracker.register_var(param_name.clone(), false);
    }

    // Validate body
    for stmt in func.body.iter() {
        validate_stmt_lifetimes(stmt, tracker, errors);
    }

    tracker.exit_scope();
}

fn validate_class_lifetimes(
    class: &HirClass,
    tracker: &mut LifetimeTracker,
    errors: &mut Vec<LifetimeError>,
) {
    // Validate each method
    for method in &class.methods {
        validate_method_lifetimes(method, tracker, errors);
    }

    for method in &class.static_methods {
        validate_method_lifetimes(method, tracker, errors);
    }
}

fn validate_method_lifetimes(
    method: &super::hir::HirMethod,
    tracker: &mut LifetimeTracker,
    errors: &mut Vec<LifetimeError>,
) {
    tracker.enter_scope();

    // Register 'this' for instance methods
    if !method.is_static {
        tracker.register_var("this".to_string(), true);
    }

    // Register parameters
    for (param_name, _) in &method.params {
        tracker.register_var(param_name.clone(), false);
    }

    // Validate body
    for stmt in method.body.iter() {
        validate_stmt_lifetimes(stmt, tracker, errors);
    }

    tracker.exit_scope();
}

// ============================================
// ESCAPE ANALYSIS
// ============================================

/// Result of escape analysis for a variable
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeState {
    /// Value never escapes, can be stack-allocated
    NoEscape,
    /// Value escapes through return
    EscapesReturn,
    /// Value escapes through closure capture
    EscapesClosure,
    /// Value escapes through heap allocation (stored in object/array)
    EscapesHeap,
    /// Unknown - conservative assumption
    Unknown,
}

/// Analyze escape behavior of variables in a function
pub fn analyze_escapes(func: &HirFunction) -> HashMap<String, EscapeState> {
    let mut escapes = HashMap::new();

    // Initialize all parameters and locals as NoEscape
    for (param_name, _, _) in &func.params {
        escapes.insert(param_name.clone(), EscapeState::NoEscape);
    }

    // Analyze the function body
    for stmt in func.body.iter() {
        analyze_stmt_escapes(stmt, &mut escapes);
    }

    escapes
}

fn analyze_stmt_escapes(stmt: &HirStmt, escapes: &mut HashMap<String, EscapeState>) {
    match stmt {
        HirStmt::Let { name, init, .. } => {
            escapes.insert(name.clone(), EscapeState::NoEscape);
            if let Some(expr) = init {
                analyze_expr_escapes(expr, escapes, None);
            }
        }

        HirStmt::Return(Some(expr)) => {
            // Mark variables in return expression as escaping
            mark_escaping(expr, escapes, EscapeState::EscapesReturn);
        }

        HirStmt::Assign { value, .. } => {
            analyze_expr_escapes(value, escapes, None);
        }

        HirStmt::Expr(expr) => {
            analyze_expr_escapes(expr, escapes, None);
        }

        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            analyze_expr_escapes(cond, escapes, None);
            analyze_stmt_escapes(then_branch, escapes);
            if let Some(else_stmt) = else_branch {
                analyze_stmt_escapes(else_stmt, escapes);
            }
        }

        HirStmt::While { cond, body } => {
            analyze_expr_escapes(cond, escapes, None);
            analyze_stmt_escapes(body, escapes);
        }

        HirStmt::ForIn { iter, body, .. } => {
            analyze_expr_escapes(iter, escapes, None);
            analyze_stmt_escapes(body, escapes);
        }

        HirStmt::Block(stmts) => {
            for s in stmts {
                analyze_stmt_escapes(s, escapes);
            }
        }

        HirStmt::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            analyze_stmt_escapes(try_block, escapes);
            analyze_stmt_escapes(catch_block, escapes);
        }

        _ => {}
    }
}

fn analyze_expr_escapes(
    expr: &HirExpr,
    escapes: &mut HashMap<String, EscapeState>,
    _context: Option<&str>,
) {
    match expr {
        HirExpr::Lambda(_, body, _) => {
            // Lambda captures can cause escapes
            let captured = find_captured_vars(body);
            for var in captured {
                if escapes.contains_key(&var) {
                    update_escape_state(escapes, &var, EscapeState::EscapesClosure);
                }
            }
        }

        HirExpr::ArrayLiteral(elems) => {
            for elem in elems {
                mark_escaping(elem, escapes, EscapeState::EscapesHeap);
            }
        }

        HirExpr::ObjectLiteral(fields) => {
            for (_, value) in fields {
                mark_escaping(value, escapes, EscapeState::EscapesHeap);
            }
        }

        HirExpr::StructLiteral(_, fields) => {
            for (_, value) in fields {
                mark_escaping(value, escapes, EscapeState::EscapesHeap);
            }
        }

        HirExpr::Call(callee, args, _) => {
            analyze_expr_escapes(callee, escapes, None);
            for arg in args {
                // Arguments might escape through the call
                analyze_expr_escapes(arg, escapes, None);
            }
        }

        HirExpr::BinaryOp(left, _, right) => {
            analyze_expr_escapes(left, escapes, None);
            analyze_expr_escapes(right, escapes, None);
        }

        HirExpr::Cast(inner, _) => {
            analyze_expr_escapes(inner, escapes, None);
        }

        _ => {}
    }
}

fn mark_escaping(expr: &HirExpr, escapes: &mut HashMap<String, EscapeState>, state: EscapeState) {
    match expr {
        HirExpr::LoadVar(name) => {
            update_escape_state(escapes, name, state);
        }
        HirExpr::BinaryOp(l, _, r) => {
            mark_escaping(l, escapes, state);
            mark_escaping(r, escapes, state);
        }
        HirExpr::ArrayLiteral(elems) => {
            for e in elems {
                mark_escaping(e, escapes, state);
            }
        }
        HirExpr::Cast(inner, _) => {
            mark_escaping(inner, escapes, state);
        }
        _ => {}
    }
}

fn update_escape_state(
    escapes: &mut HashMap<String, EscapeState>,
    name: &str,
    new_state: EscapeState,
) {
    if let Some(current) = escapes.get_mut(name) {
        // Escalate escape state
        *current = match (*current, new_state) {
            (EscapeState::NoEscape, s) => s,
            (s, EscapeState::NoEscape) => s,
            (EscapeState::Unknown, _) | (_, EscapeState::Unknown) => EscapeState::Unknown,
            (s, _) => s, // Keep current if already escaping
        };
    }
}

fn find_captured_vars(body: &[HirStmt]) -> HashSet<String> {
    let mut captured = HashSet::new();
    let mut local_vars = HashSet::new();

    for stmt in body {
        find_captured_in_stmt(stmt, &mut captured, &mut local_vars);
    }

    captured
}

fn find_captured_in_stmt(
    stmt: &HirStmt,
    captured: &mut HashSet<String>,
    local_vars: &mut HashSet<String>,
) {
    match stmt {
        HirStmt::Let { name, init, .. } => {
            local_vars.insert(name.clone());
            if let Some(expr) = init {
                find_captured_in_expr(expr, captured, local_vars);
            }
        }
        HirStmt::Expr(expr) => {
            find_captured_in_expr(expr, captured, local_vars);
        }
        HirStmt::Block(stmts) => {
            for s in stmts {
                find_captured_in_stmt(s, captured, local_vars);
            }
        }
        _ => {}
    }
}

fn find_captured_in_expr(
    expr: &HirExpr,
    captured: &mut HashSet<String>,
    local_vars: &HashSet<String>,
) {
    match expr {
        HirExpr::LoadVar(name) => {
            if !local_vars.contains(name) {
                captured.insert(name.clone());
            }
        }
        HirExpr::BinaryOp(l, _, r) => {
            find_captured_in_expr(l, captured, local_vars);
            find_captured_in_expr(r, captured, local_vars);
        }
        HirExpr::Call(callee, args, _) => {
            find_captured_in_expr(callee, captured, local_vars);
            for arg in args {
                find_captured_in_expr(arg, captured, local_vars);
            }
        }
        HirExpr::Cast(inner, _) => {
            find_captured_in_expr(inner, captured, local_vars);
        }
        _ => {}
    }
}

// ============================================
// CONSTANT FOLDING
// ============================================

/// Perform constant folding on an HIR expression
/// Returns Some(literal) if the expression can be evaluated at compile time
pub fn fold_constants(expr: &HirExpr) -> Option<HirLiteral> {
    match expr {
        HirExpr::Literal(lit) => Some(lit.clone()),

        HirExpr::BinaryOp(left, op, right) => {
            let l = fold_constants(left)?;
            let r = fold_constants(right)?;

            match (l, op, r) {
                // Integer arithmetic
                (HirLiteral::Int(a), BinOp::Add, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Int(a + b))
                }
                (HirLiteral::Int(a), BinOp::Sub, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Int(a - b))
                }
                (HirLiteral::Int(a), BinOp::Mul, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Int(a * b))
                }
                (HirLiteral::Int(a), BinOp::Div, HirLiteral::Int(b)) if b != 0 => {
                    Some(HirLiteral::Int(a / b))
                }
                (HirLiteral::Int(a), BinOp::Mod, HirLiteral::Int(b)) if b != 0 => {
                    Some(HirLiteral::Int(a % b))
                }

                // Float arithmetic
                (HirLiteral::Float(a), BinOp::Add, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Float(a + b))
                }
                (HirLiteral::Float(a), BinOp::Sub, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Float(a - b))
                }
                (HirLiteral::Float(a), BinOp::Mul, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Float(a * b))
                }
                (HirLiteral::Float(a), BinOp::Div, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Float(a / b))
                }

                // Comparisons
                (HirLiteral::Int(a), BinOp::Lt, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Bool(a < b))
                }
                (HirLiteral::Int(a), BinOp::Le, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Bool(a <= b))
                }
                (HirLiteral::Int(a), BinOp::Gt, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Bool(a > b))
                }
                (HirLiteral::Int(a), BinOp::Ge, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Bool(a >= b))
                }
                (HirLiteral::Int(a), BinOp::Eq, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Bool(a == b))
                }
                (HirLiteral::Int(a), BinOp::Ne, HirLiteral::Int(b)) => {
                    Some(HirLiteral::Bool(a != b))
                }

                (HirLiteral::Float(a), BinOp::Lt, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Bool(a < b))
                }
                (HirLiteral::Float(a), BinOp::Le, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Bool(a <= b))
                }
                (HirLiteral::Float(a), BinOp::Gt, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Bool(a > b))
                }
                (HirLiteral::Float(a), BinOp::Ge, HirLiteral::Float(b)) => {
                    Some(HirLiteral::Bool(a >= b))
                }

                // Boolean operations
                (HirLiteral::Bool(a), BinOp::And, HirLiteral::Bool(b)) => {
                    Some(HirLiteral::Bool(a && b))
                }
                (HirLiteral::Bool(a), BinOp::Or, HirLiteral::Bool(b)) => {
                    Some(HirLiteral::Bool(a || b))
                }

                // String concatenation
                (HirLiteral::String(a), BinOp::Add, HirLiteral::String(b)) => {
                    Some(HirLiteral::String(format!("{}{}", a, b)))
                }

                _ => None,
            }
        }

        HirExpr::UnaryOp(op, inner) => {
            let lit = fold_constants(inner)?;

            match (op, lit) {
                (UnaryOp::Neg, HirLiteral::Int(n)) => Some(HirLiteral::Int(-n)),
                (UnaryOp::Neg, HirLiteral::Float(n)) => Some(HirLiteral::Float(-n)),
                (UnaryOp::Not, HirLiteral::Bool(b)) => Some(HirLiteral::Bool(!b)),
                (UnaryOp::BitNot, HirLiteral::Int(n)) => Some(HirLiteral::Int(!n)),
                _ => None,
            }
        }

        HirExpr::Conditional(cond, then_expr, else_expr) => {
            let cond_lit = fold_constants(cond)?;

            match cond_lit {
                HirLiteral::Bool(true) => fold_constants(then_expr),
                HirLiteral::Bool(false) => fold_constants(else_expr),
                _ => None,
            }
        }

        HirExpr::Cast(inner, target_ty) => {
            let inner_lit = fold_constants(inner)?;
            let num_val = match inner_lit {
                HirLiteral::Int(n) => n as f64,
                HirLiteral::Float(n) => n,
                HirLiteral::U8(n) => n as f64,
                HirLiteral::U16(n) => n as f64,
                HirLiteral::U32(n) => n as f64,
                HirLiteral::U64(n) => n as f64,
                HirLiteral::U128(n) => n as f64,
                HirLiteral::I8(n) => n as f64,
                HirLiteral::I16(n) => n as f64,
                HirLiteral::I32(n) => n as f64,
                HirLiteral::I64(n) => n as f64,
                HirLiteral::I128(n) => n as f64,
                HirLiteral::F32(n) => n as f64,
                HirLiteral::F64(n) => n,
                _ => return None,
            };
            match target_ty {
                HirType::U8 => Some(HirLiteral::U8(num_val as u8)),
                HirType::U16 => Some(HirLiteral::U16(num_val as u16)),
                HirType::U32 => Some(HirLiteral::U32(num_val as u32)),
                HirType::U64 => Some(HirLiteral::U64(num_val as u64)),
                HirType::U128 => Some(HirLiteral::U128(num_val as u128)),
                HirType::I8 => Some(HirLiteral::I8(num_val as i8)),
                HirType::I16 => Some(HirLiteral::I16(num_val as i16)),
                HirType::I32 => Some(HirLiteral::I32(num_val as i32)),
                HirType::I64 | HirType::Int => Some(HirLiteral::I64(num_val as i64)),
                HirType::I128 => Some(HirLiteral::I128(num_val as i128)),
                HirType::F32 => Some(HirLiteral::F32(num_val as f32)),
                HirType::F64 | HirType::Float => Some(HirLiteral::F64(num_val)),
                _ => None,
            }
        }

        _ => None,
    }
}

/// Apply constant folding to an entire HIR module
pub fn fold_module_constants(module: &mut HirModule) {
    for stmt in &mut module.statements {
        fold_stmt_constants(stmt);
    }

    for func in &mut module.functions {
        // Only clone if there are multiple references to the Arc
        // This avoids unnecessary cloning when we're the only owner
        if std::sync::Arc::strong_count(&func.body) == 1 {
            // Safe to modify in place since we're the only owner
            for stmt in std::sync::Arc::make_mut(&mut func.body).iter_mut() {
                fold_stmt_constants(stmt);
            }
        } else {
            // Multiple owners - need to clone for mutation
            let mut body = (*func.body).clone();
            for stmt in body.iter_mut() {
                fold_stmt_constants(stmt);
            }
            func.body = std::sync::Arc::new(body);
        }
    }
}

fn fold_stmt_constants(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Let {
            init: Some(expr), ..
        } => {
            if let Some(lit) = fold_constants(expr) {
                *expr = HirExpr::Literal(lit);
            }
        }
        HirStmt::Assign { value, .. } => {
            if let Some(lit) = fold_constants(value) {
                *value = HirExpr::Literal(lit);
            }
        }
        HirStmt::Expr(expr) => {
            if let Some(lit) = fold_constants(expr) {
                *expr = HirExpr::Literal(lit);
            }
        }
        HirStmt::Return(Some(expr)) => {
            if let Some(lit) = fold_constants(expr) {
                *expr = HirExpr::Literal(lit);
            }
        }
        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            if let Some(lit) = fold_constants(cond) {
                *cond = HirExpr::Literal(lit);
            }
            fold_stmt_constants(then_branch);
            if let Some(else_stmt) = else_branch {
                fold_stmt_constants(else_stmt);
            }
        }
        HirStmt::While { cond, body } => {
            if let Some(lit) = fold_constants(cond) {
                *cond = HirExpr::Literal(lit);
            }
            fold_stmt_constants(body);
        }
        HirStmt::Block(stmts) => {
            for s in stmts {
                fold_stmt_constants(s);
            }
        }
        _ => {}
    }
}

// ============================================
// DEAD CODE ELIMINATION
// ============================================

/// Check if a statement is dead (unreachable)
pub fn is_dead_code(stmt: &HirStmt, _after_return: bool) -> bool {
    match stmt {
        // After return, everything is dead
        HirStmt::Return(_) => false, // Return itself is not dead
        _ if _after_return => true,

        // Constant false condition means then branch is dead
        HirStmt::If {
            cond: HirExpr::Literal(HirLiteral::Bool(false)),
            ..
        } => true,

        // Constant false while condition means body is dead
        HirStmt::While {
            cond: HirExpr::Literal(HirLiteral::Bool(false)),
            ..
        } => true,

        _ => false,
    }
}

/// Eliminate dead code from an HIR module
pub fn eliminate_dead_code(module: &mut HirModule) {
    module.statements = eliminate_dead_in_stmts(std::mem::take(&mut module.statements));

    for func in &mut module.functions {
        // Only clone if there are multiple references to the Arc
        if std::sync::Arc::strong_count(&func.body) == 1 {
            let body = std::sync::Arc::make_mut(&mut func.body);
            *body = eliminate_dead_in_stmts(std::mem::take(body));
        } else {
            // Multiple owners - need to clone for mutation
            let body = eliminate_dead_in_stmts((*func.body).clone());
            func.body = std::sync::Arc::new(body);
        }
    }
}

fn eliminate_dead_in_stmts(stmts: Vec<HirStmt>) -> Vec<HirStmt> {
    let mut result = Vec::new();
    let mut after_return = false;

    for mut stmt in stmts {
        if after_return {
            // Skip dead code after return
            continue;
        }

        // Recursively eliminate dead code in nested structures
        match &mut stmt {
            HirStmt::If {
                then_branch,
                else_branch,
                cond,
            } => {
                // Check if condition is constant true/false
                if let HirExpr::Literal(HirLiteral::Bool(true)) = cond {
                    // Only then branch executes
                    let then_stmts = match then_branch.as_mut() {
                        HirStmt::Block(stmts) => eliminate_dead_in_stmts(std::mem::take(stmts)),
                        other => vec![std::mem::replace(other, HirStmt::Break)],
                    };
                    result.extend(then_stmts);
                    continue;
                } else if let HirExpr::Literal(HirLiteral::Bool(false)) = cond {
                    // Only else branch executes (if present)
                    if let Some(else_stmt) = else_branch {
                        let else_stmts = match else_stmt.as_mut() {
                            HirStmt::Block(stmts) => eliminate_dead_in_stmts(std::mem::take(stmts)),
                            other => vec![std::mem::replace(other, HirStmt::Break)],
                        };
                        result.extend(else_stmts);
                    }
                    continue;
                }

                // Normal if - eliminate dead code in branches
                if let HirStmt::Block(stmts) = then_branch.as_mut() {
                    *stmts = eliminate_dead_in_stmts(std::mem::take(stmts));
                }
                if let Some(else_stmt) = else_branch {
                    if let HirStmt::Block(stmts) = else_stmt.as_mut() {
                        *stmts = eliminate_dead_in_stmts(std::mem::take(stmts));
                    }
                }
            }

            HirStmt::While { body, cond } => {
                // Check for constant false condition
                if let HirExpr::Literal(HirLiteral::Bool(false)) = cond {
                    continue; // Skip entire while
                }

                if let HirStmt::Block(stmts) = body.as_mut() {
                    *stmts = eliminate_dead_in_stmts(std::mem::take(stmts));
                }
            }

            HirStmt::Block(stmts) => {
                *stmts = eliminate_dead_in_stmts(std::mem::take(stmts));
            }

            HirStmt::Return(_) => {
                after_return = true;
            }

            _ => {}
        }

        result.push(stmt);
    }

    result
}

// ============================================
// TESTS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::hir::HirType;

    #[test]
    fn test_constant_folding() {
        // 2 + 3 -> 5
        let expr = HirExpr::BinaryOp(
            Box::new(HirExpr::Literal(HirLiteral::Int(2))),
            BinOp::Add,
            Box::new(HirExpr::Literal(HirLiteral::Int(3))),
        );

        let result = fold_constants(&expr);
        assert_eq!(result, Some(HirLiteral::Int(5)));
    }

    #[test]
    fn test_constant_folding_nested() {
        // (2 + 3) * 4 -> 20
        let inner = HirExpr::BinaryOp(
            Box::new(HirExpr::Literal(HirLiteral::Int(2))),
            BinOp::Add,
            Box::new(HirExpr::Literal(HirLiteral::Int(3))),
        );
        let expr = HirExpr::BinaryOp(
            Box::new(inner),
            BinOp::Mul,
            Box::new(HirExpr::Literal(HirLiteral::Int(4))),
        );

        let result = fold_constants(&expr);
        assert_eq!(result, Some(HirLiteral::Int(20)));
    }

    #[test]
    fn test_constant_folding_comparison() {
        // 5 > 3 -> true
        let expr = HirExpr::BinaryOp(
            Box::new(HirExpr::Literal(HirLiteral::Int(5))),
            BinOp::Gt,
            Box::new(HirExpr::Literal(HirLiteral::Int(3))),
        );

        let result = fold_constants(&expr);
        assert_eq!(result, Some(HirLiteral::Bool(true)));
    }

    #[test]
    fn test_escape_analysis() {
        use std::sync::Arc;

        // Simple function with no escaping
        let func = HirFunction {
            is_exported: false,
            name: "test".to_string(),
            params: vec![("x".to_string(), None, None)],
            body: Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("x".to_string())),
                BinOp::Add,
                Box::new(HirExpr::Literal(HirLiteral::Int(1))),
            )))]),
            ret_type: None,
            is_async: false,
            decorators: vec![],
            move_params: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };

        let escapes = analyze_escapes(&func);
        // x escapes through return
        assert_eq!(escapes.get("x"), Some(&EscapeState::EscapesReturn));
    }

    #[test]
    fn phase3_integration_smoke() {
        use std::sync::Arc;

        // Build a tiny module with a function that owns a local and returns.
        let body = Arc::new(vec![
            HirStmt::Let {
                name: "x".into(),
                ty: None,
                init: None,
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(None),
        ]);
        let f = HirFunction {
            name: "f".into(),
            params: vec![
                (
                    "a".into(),
                    Some(HirType::BorrowImmut(Box::new(HirType::I32))),
                    None,
                ),
                (
                    "b".into(),
                    Some(HirType::BorrowMut(Box::new(HirType::I32))),
                    None,
                ),
                ("c".into(), Some(HirType::I32), None),
            ],
            body,
            ret_type: Some(HirType::I32),
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
        let mut m = HirModule::new();
        m.functions.push(f);

        let results = run_phase3_passes(&m);
        // Borrow modes for f should be present and of length 3
        let modes = results.borrow_modes.get("f").expect("borrow modes");
        assert_eq!(modes.len(), 3);
        assert!(matches!(modes[0], BorrowMode::SharedBorrow));
        assert!(matches!(modes[1], BorrowMode::MutBorrow));
        assert!(matches!(modes[2], BorrowMode::Move));

        // Variance summary should include function f
        assert!(results.return_variance.get("f").is_some());

        // Drop plan should include at least one event for x on return
        let plan = results.drop_plans.get("f").expect("drop plan");
        assert!(plan.planned >= 1);
        assert!(plan.events.iter().any(|e| e.var == "x"));
    }
}
