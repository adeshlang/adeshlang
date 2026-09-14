//! Memory management and drop handling
//!
//! This module handles memory safety, drop insertion, defer execution,
//! and reference counting logic during HIR to LIR lowering.

use super::super::{LirFunction, LirInst, LirModule};
use super::core::{DropLoweringKind, LowerCtx};
use super::statements::lower_stmt;
use super::types::hir_type_to_lir_type;
use crate::parsing::drop_insertion::DropPlan;
use crate::parsing::hir::HirType;

pub(super) fn drop_kind_from_hir_type(hir_type: &HirType) -> DropLoweringKind {
    match hir_type {
        HirType::Shared(_) => DropLoweringKind::Shared,
        HirType::Weak(_) => DropLoweringKind::Weak,
        HirType::BorrowImmut(_) | HirType::BorrowMut(_) | HirType::Borrow(_, _) => {
            DropLoweringKind::Borrow
        }
        _ => DropLoweringKind::Unknown,
    }
}

pub(super) fn record_var_kind(ctx: &mut LowerCtx, name: &str, hir_type: &HirType) {
    let lir_type = hir_type_to_lir_type(hir_type);
    ctx.var_types.insert(name.to_string(), lir_type);
    let kind = drop_kind_from_hir_type(hir_type);
    if !matches!(kind, DropLoweringKind::Unknown) {
        ctx.drop_kinds.insert(name.to_string(), kind);
    }
}

/// Emit defers for the current (top) scope in LIFO order, then pop the scope.
/// Called at normal block exit. This is compile-time inlining — zero runtime overhead.
pub(super) fn emit_scope_defers(
    lir: &mut LirModule,
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    loc: &str,
    drop_plan: Option<&DropPlan>,
) -> Result<(), String> {
    let defers = ctx.defer_scopes.pop().unwrap_or_default();
    // LIFO: iterate in reverse (last registered executes first)
    for defer_block in defers.iter().rev() {
        let defer_loc = format!("{loc}.defer");
        lower_stmt(lir, func, ctx, defer_block, &defer_loc, drop_plan)?;
    }
    Ok(())
}

/// Emit ALL defers from ALL active scopes in LIFO order (innermost scope first,
/// within each scope last-registered first). Used at `return` — all scopes unwind.
/// Clones the defer blocks so they remain available for other code paths (e.g.
/// the else branch of an if where one branch returns). Since return terminates
/// the current block, the duplicated defers in dead code are never executed.
pub(super) fn emit_all_defers(
    lir: &mut LirModule,
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    loc: &str,
    drop_plan: Option<&DropPlan>,
) -> Result<(), String> {
    // Iterate from innermost scope to outermost, each scope's defers in LIFO
    for scope_idx in (0..ctx.defer_scopes.len()).rev() {
        let defers = ctx.defer_scopes[scope_idx].clone();
        for defer_block in defers.iter().rev() {
            let defer_loc = format!("{loc}.defer");
            lower_stmt(lir, func, ctx, defer_block, &defer_loc, drop_plan)?;
        }
    }
    Ok(())
}

/// Emit defers from all scopes from the top down to `target_depth` (exclusive),
/// in LIFO order. Used at `break`/`continue` — unwinds scopes up to the loop body.
/// Clones the defer blocks instead of popping, so they remain available for the
/// normal exit path (end of loop body). Since break/continue jumps away, the
/// normal exit code is never reached — no double emission at runtime.
pub(super) fn emit_defers_until_depth(
    lir: &mut LirModule,
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    target_depth: usize,
    loc: &str,
    drop_plan: Option<&DropPlan>,
) -> Result<(), String> {
    for scope_idx in (target_depth..ctx.defer_scopes.len()).rev() {
        let defers = ctx.defer_scopes[scope_idx].clone();
        for defer_block in defers.iter().rev() {
            let defer_loc = format!("{loc}.defer");
            lower_stmt(lir, func, ctx, defer_block, &defer_loc, drop_plan)?;
        }
    }
    Ok(())
}

pub(super) fn emit_drops_for_location(
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    drop_plan: Option<&DropPlan>,
    loc: &str,
) {
    if let Some(plan) = drop_plan {
        for event in plan.events.iter().filter(|e| e.location == loc) {
            if let Some(val) = func.get_var(&event.var) {
                let kind = ctx.drop_kinds.get(&event.var).copied().unwrap_or(DropLoweringKind::Unknown);
                match kind {
                    DropLoweringKind::Shared => {
                        func.push_to_block(ctx.current_block, LirInst::ArcDrop(val))
                    }
                    DropLoweringKind::Weak => {
                        func.push_to_block(ctx.current_block, LirInst::WeakDrop(val))
                    }
                    DropLoweringKind::Borrow => {
                        // Auto-release borrows at scope/drop boundaries
                        let tmp = func.alloc_value();
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltin(tmp, "borrow_release".to_string(), vec![val]),
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}
