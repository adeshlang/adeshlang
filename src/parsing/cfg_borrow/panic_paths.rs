//! Panic Path, Cleanup, and Control-Exit Tracking for HIR CFGs
//!
//! Coordinates with `ControlFlowGraph`, `validate_cfg`, `Reachability`, and
//! `DominatorTree` / `LoopAnalysis` to determine which owned variables, tracked
//! allocations, ARC handles, and deferred cleanup actions are active at every
//! control-flow exit (`Return`, `EarlyReturn`, `Break`, `Continue`, `Throw`,
//! `Panic`, and `NormalExit`) in deterministic LIFO drop order.

use super::cfg::{validate_cfg, BlockId, BlockKind, ControlFlowGraph};
use super::dominators::{DominatorTree, LoopAnalysis, Reachability};
use crate::parsing::hir::{HirExpr, HirStmt};
use std::collections::{HashMap, HashSet, VecDeque};

/// Control-flow exit path classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitPath {
    /// Normal function fallthrough to Exit
    NormalExit,
    /// Return at end of function
    Return,
    /// Early return from inside a branch or loop
    EarlyReturn,
    /// Break out of a loop
    Break,
    /// Continue (or jump) to loop header
    Continue,
    /// Explicit `throw` statement
    Throw,
    /// Potentially panicking call (`panic`, `unwrap`, `expect`, `abort`, etc.)
    Panic,
}

/// Cleanup obligations along a specific exit path
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupInfo {
    /// Owned variable names live at this exit point
    pub to_drop: Vec<String>,
    /// Deterministic LIFO drop order (reverse declaration/allocation order)
    pub drop_order: Vec<String>,
    /// Exit path kind
    pub exit_type: ExitPath,
    /// Block where the exit occurs
    pub block_id: BlockId,
    /// Whether this exit passes through a `Defer` block
    pub has_defer: bool,
}

/// Tracks active owned bindings and cleanup obligations across all CFG exits.
pub struct PanicPathTracker {
    /// Active owned variables at block entry (`in`) and exit (`out`)
    active_in: HashMap<BlockId, HashSet<String>>,
    active_out: HashMap<BlockId, HashSet<String>>,
    /// Global declaration/allocation order for deterministic LIFO drop ordering
    allocation_order: Vec<String>,
    /// Cleanup records collected for every exit point
    cleanup_info: Vec<CleanupInfo>,
}

impl PanicPathTracker {
    pub fn new() -> Self {
        Self {
            active_in: HashMap::new(),
            active_out: HashMap::new(),
            allocation_order: Vec::new(),
            cleanup_info: Vec::new(),
        }
    }

    /// Analyze a validated CFG for owned-value liveness and cleanup paths.
    pub fn analyze(&mut self, cfg: &ControlFlowGraph) -> Result<(), String> {
        self.active_in.clear();
        self.active_out.clear();
        self.allocation_order.clear();
        self.cleanup_info.clear();

        if let Err(errs) = validate_cfg(cfg) {
            return Err(format!(
                "Cannot analyze panic paths on invalid CFG `{}`: {}",
                cfg.function_name,
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }

        let reach = Reachability::compute(cfg);
        self.record_allocation_order(cfg, &reach);
        self.compute_active_owned_fixpoint(cfg, &reach);
        self.collect_exit_cleanups(cfg, &reach);

        Ok(())
    }

    fn record_allocation_order(&mut self, cfg: &ControlFlowGraph, reach: &Reachability) {
        let mut seen = HashSet::new();
        for &block_id in &reach.rpo {
            let Some(block) = cfg.get_block(block_id) else {
                continue;
            };
            for stmt in &block.statements {
                match stmt {
                    HirStmt::Let { name, .. } => {
                        if seen.insert(name.clone()) {
                            self.allocation_order.push(name.clone());
                        }
                    }
                    HirStmt::LetTuple { names, .. } => {
                        for n in names {
                            if seen.insert(n.clone()) {
                                self.allocation_order.push(n.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // Any remaining bindings (e.g., function parameters) precede local allocations
        // in declaration order, so they are dropped last (inserted at front in sorted order).
        let mut remaining: Vec<String> = cfg
            .variables
            .keys()
            .filter(|v| !seen.contains(*v))
            .cloned()
            .collect();
        remaining.sort();
        if !remaining.is_empty() {
            remaining.extend(std::mem::take(&mut self.allocation_order));
            self.allocation_order = remaining;
        }
    }

    fn compute_active_owned_fixpoint(&mut self, cfg: &ControlFlowGraph, reach: &Reachability) {
        let rpo = &reach.rpo;
        let initial: HashSet<String> = cfg.variables.keys().cloned().collect();
        self.active_in.insert(cfg.entry, initial);

        let mut worklist: VecDeque<BlockId> = rpo.iter().copied().collect();
        let mut in_worklist: HashSet<BlockId> = rpo.iter().copied().collect();
        let max_steps = (cfg.blocks.len().max(1) * 16).max(256);
        let mut steps = 0;

        while let Some(block_id) = worklist.pop_front() {
            in_worklist.remove(&block_id);
            if !reach.is_reachable(block_id) {
                continue;
            }
            steps += 1;
            if steps > max_steps {
                break;
            }

            let Some(block) = cfg.get_block(block_id) else {
                continue;
            };

            let mut state = if block_id == cfg.entry {
                self.active_in.get(&cfg.entry).cloned().unwrap_or_default()
            } else {
                // Conservative union across reachable predecessors: any variable that could
                // still be owned on some incoming path must be considered for cleanup
                // (guarded by initialization/move flags at runtime when conditional).
                let mut merged = HashSet::new();
                for &pred in &block.predecessors {
                    if reach.is_reachable(pred) {
                        if let Some(pred_out) = self.active_out.get(&pred) {
                            merged.extend(pred_out.iter().cloned());
                        }
                    }
                }
                merged
            };

            self.active_in.insert(block_id, state.clone());

            for stmt in &block.statements {
                Self::transfer_stmt(stmt, &mut state);
            }

            let changed = self.active_out.get(&block_id) != Some(&state);
            if changed {
                self.active_out.insert(block_id, state);
                for &succ in &block.successors {
                    if reach.is_reachable(succ) && in_worklist.insert(succ) {
                        worklist.push_back(succ);
                    }
                }
            }
        }
    }

    fn transfer_stmt(stmt: &HirStmt, state: &mut HashSet<String>) {
        match stmt {
            HirStmt::Let {
                name,
                init,
                is_borrowed,
                ..
            } => {
                if let Some(expr) = init {
                    Self::transfer_expr(expr, state);
                }
                // Only owned bindings (not pure borrow aliases) need ownership cleanup
                if is_borrowed.is_none() {
                    state.insert(name.clone());
                }
            }
            HirStmt::LetTuple { names, init, .. } => {
                if let Some(expr) = init {
                    Self::transfer_expr(expr, state);
                }
                for n in names {
                    state.insert(n.clone());
                }
            }
            HirStmt::Assign { target, value, .. } => {
                Self::transfer_expr(value, state);
                if let HirExpr::LoadVar(name) = target {
                    state.insert(name.clone());
                }
            }
            HirStmt::Expr(expr) | HirStmt::Throw(expr) | HirStmt::Jump(expr) => {
                Self::transfer_expr(expr, state);
            }
            HirStmt::Return(Some(expr)) => {
                Self::transfer_expr(expr, state);
                // Moving/returning a local transfers its ownership to caller
                if let HirExpr::LoadVar(name) = expr {
                    state.remove(name);
                }
            }
            HirStmt::Unsafe(body) | HirStmt::Defer(body) => {
                Self::transfer_stmt(body, state);
            }
            _ => {}
        }
    }

    fn transfer_expr(expr: &HirExpr, state: &mut HashSet<String>) {
        match expr {
            HirExpr::Move(inner) | HirExpr::Free(inner) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    state.remove(name);
                } else {
                    Self::transfer_expr(inner, state);
                }
            }
            HirExpr::BinaryOp(a, _, b)
            | HirExpr::Index(a, b)
            | HirExpr::Range(a, b, _)
            | HirExpr::SetMember(a, _, b) => {
                Self::transfer_expr(a, state);
                Self::transfer_expr(b, state);
            }
            HirExpr::Call(f, args, _) => {
                Self::transfer_expr(f, state);
                for arg in args {
                    Self::transfer_expr(arg, state);
                }
            }
            HirExpr::MethodCall(o, _, args) => {
                Self::transfer_expr(o, state);
                for arg in args {
                    Self::transfer_expr(arg, state);
                }
            }
            HirExpr::UnaryOp(_, inner)
            | HirExpr::Deref(inner)
            | HirExpr::Share(inner)
            | HirExpr::Downgrade(inner)
            | HirExpr::Await(inner)
            | HirExpr::Spawn(inner)
            | HirExpr::NonNull(inner)
            | HirExpr::Spread(inner)
            | HirExpr::Update(inner, _, _)
            | HirExpr::Cast(inner, _)
            | HirExpr::Format(inner, _)
            | HirExpr::Alloc(_, inner)
            | HirExpr::MemberAccess(inner, _)
            | HirExpr::OptionalGet(inner, _)
            | HirExpr::StoreVar(_, inner) => Self::transfer_expr(inner, state),
            HirExpr::ArrayLiteral(elems)
            | HirExpr::SetLiteral(elems)
            | HirExpr::TupleLiteral(elems)
            | HirExpr::NewInstance(_, elems) => {
                for e in elems {
                    Self::transfer_expr(e, state);
                }
            }
            HirExpr::DictLiteral(pairs) => {
                for (k, v) in pairs {
                    Self::transfer_expr(k, state);
                    Self::transfer_expr(v, state);
                }
            }
            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                for (_, v) in fields {
                    Self::transfer_expr(v, state);
                }
            }
            _ => {}
        }
    }

    fn collect_exit_cleanups(&mut self, cfg: &ControlFlowGraph, reach: &Reachability) {
        for &block_id in &reach.rpo {
            let Some(block) = cfg.get_block(block_id) else {
                continue;
            };
            let owned = self.active_out.get(&block_id).cloned().unwrap_or_default();
            let has_defer = block.kind == BlockKind::Defer
                || block
                    .predecessors
                    .iter()
                    .any(|&p| cfg.get_block(p).is_some_and(|pb| pb.kind == BlockKind::Defer));

            match block.kind {
                BlockKind::Return => {
                    let is_early = block
                        .successors
                        .iter()
                        .all(|&s| cfg.exits.contains(&s))
                        && cfg
                            .blocks
                            .iter()
                            .any(|b| b.kind == BlockKind::Exit && b.predecessors.len() > 1);
                    self.record_cleanup(
                        &owned,
                        if is_early {
                            ExitPath::EarlyReturn
                        } else {
                            ExitPath::Return
                        },
                        block_id,
                        has_defer,
                    );
                }
                BlockKind::Break => {
                    self.record_cleanup(&owned, ExitPath::Break, block_id, has_defer);
                }
                BlockKind::Continue => {
                    self.record_cleanup(&owned, ExitPath::Continue, block_id, has_defer);
                }
                BlockKind::Throw => {
                    self.record_cleanup(&owned, ExitPath::Throw, block_id, has_defer);
                }
                BlockKind::Exit => {
                    // Normal fallthrough predecessors (non-Return/non-Throw)
                    for &pred in &block.predecessors {
                        if let Some(pb) = cfg.get_block(pred) {
                            if reach.is_reachable(pred)
                                && !matches!(pb.kind, BlockKind::Return | BlockKind::Throw)
                            {
                                let pred_owned =
                                    self.active_out.get(&pred).cloned().unwrap_or_default();
                                self.record_cleanup(
                                    &pred_owned,
                                    ExitPath::NormalExit,
                                    pred,
                                    pb.kind == BlockKind::Defer,
                                );
                            }
                        }
                    }
                }
                _ => {}
            }

            // Detect potentially panicking expressions in any reachable block
            if block.statements.iter().any(Self::stmt_may_panic) {
                let state_before = self.active_in.get(&block_id).cloned().unwrap_or_default();
                let mut combined = state_before;
                combined.extend(owned);
                self.record_cleanup(&combined, ExitPath::Panic, block_id, has_defer);
            }
        }
    }

    fn stmt_may_panic(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Throw(_) => true,
            HirStmt::Expr(e) | HirStmt::Return(Some(e)) => Self::expr_may_panic(e),
            HirStmt::Let { init: Some(e), .. } | HirStmt::Assign { value: e, .. } => {
                Self::expr_may_panic(e)
            }
            HirStmt::Unsafe(b) | HirStmt::Defer(b) => Self::stmt_may_panic(b),
            _ => false,
        }
    }

    fn expr_may_panic(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call(callee, args, _) => {
                let is_panic_name = matches!(
                    callee.as_ref(),
                    HirExpr::LoadVar(name)
                        if matches!(name.as_str(), "panic" | "abort" | "assert" | "unreachable")
                );
                is_panic_name
                    || Self::expr_may_panic(callee)
                    || args.iter().any(Self::expr_may_panic)
            }
            HirExpr::MethodCall(recv, method, args) => {
                matches!(method.as_str(), "unwrap" | "expect")
                    || Self::expr_may_panic(recv)
                    || args.iter().any(Self::expr_may_panic)
            }
            HirExpr::NonNull(_) => true,
            HirExpr::BinaryOp(a, _, b) | HirExpr::Index(a, b) => {
                Self::expr_may_panic(a) || Self::expr_may_panic(b)
            }
            _ => false,
        }
    }

    fn record_cleanup(
        &mut self,
        owned: &HashSet<String>,
        exit_type: ExitPath,
        block_id: BlockId,
        has_defer: bool,
    ) {
        let mut to_drop: Vec<String> = owned.iter().cloned().collect();
        to_drop.sort();
        let drop_order = self.compute_drop_order(owned);
        self.cleanup_info.push(CleanupInfo {
            to_drop,
            drop_order,
            exit_type,
            block_id,
            has_defer,
        });
    }

    /// Compute LIFO drop order (reverse of declaration/allocation order)
    fn compute_drop_order(&self, owned: &HashSet<String>) -> Vec<String> {
        let mut order: Vec<String> = owned.iter().cloned().collect();
        order.sort_by_key(|name| {
            self.allocation_order
                .iter()
                .position(|n| n == name)
                .unwrap_or(usize::MAX)
        });
        order.reverse();
        order
    }

    /// Get cleanup information for all exit paths
    pub fn get_cleanup_info(&self) -> &[CleanupInfo] {
        &self.cleanup_info
    }

    /// Active owned variables at exit of `block_id`
    pub fn active_owned_at_exit(&self, block_id: BlockId) -> Option<&HashSet<String>> {
        self.active_out.get(&block_id)
    }

    /// Validate that every exit path has complete, duplicate-free drop ordering
    pub fn validate_complete_cleanup(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        for cleanup in &self.cleanup_info {
            let unique_drops: HashSet<&String> = cleanup.drop_order.iter().collect();
            if unique_drops.len() != cleanup.drop_order.len() {
                errors.push(format!(
                    "Duplicate drop in {:?} at block {}",
                    cleanup.exit_type, cleanup.block_id
                ));
            }
            if cleanup.to_drop.len() != cleanup.drop_order.len() {
                errors.push(format!(
                    "Incomplete cleanup for {:?} at block {}: {} to drop vs {} in order",
                    cleanup.exit_type,
                    cleanup.block_id,
                    cleanup.to_drop.len(),
                    cleanup.drop_order.len()
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for PanicPathTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates loop structural consistency with respect to dominator-based natural loops.
pub struct LoopBorrowValidator {
    loop_headers: HashSet<BlockId>,
    errors: Vec<String>,
}

impl LoopBorrowValidator {
    pub fn new() -> Self {
        Self {
            loop_headers: HashSet::new(),
            errors: Vec::new(),
        }
    }

    pub fn analyze(&mut self, cfg: &ControlFlowGraph) -> Result<(), Vec<String>> {
        self.loop_headers.clear();
        self.errors.clear();

        if let Err(errs) = validate_cfg(cfg) {
            for e in errs {
                self.errors.push(e.to_string());
            }
            return Err(std::mem::take(&mut self.errors));
        }

        let dom = DominatorTree::compute(cfg);
        let loops = LoopAnalysis::compute(cfg, &dom);
        for (&header, lp) in &loops.loops {
            self.loop_headers.insert(header);
            // Every loop header must dominate all nodes in its natural loop body
            for &node in &lp.body {
                if !dom.dominates(header, node) {
                    self.errors.push(format!(
                        "Loop header {} does not dominate loop body block {}",
                        header, node
                    ));
                }
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    pub fn loop_headers(&self) -> &HashSet<BlockId> {
        &self.loop_headers
    }

    pub fn get_errors(&self) -> &[String] {
        &self.errors
    }
}

impl Default for LoopBorrowValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::cfg_borrow::cfg::CfgBuilder;
    use crate::parsing::hir::{HirFunction, HirLiteral, HirType};
    use std::sync::Arc;

    fn make_fn(body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "test_panic".to_string(),
            params: vec![("cond".to_string(), Some(HirType::Bool), None)],
            body: Arc::new(body),
            ret_type: None,
            is_async: false,
            decorators: Vec::new(),
            is_exported: false,
            move_params: Vec::new(),
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        }
    }

    #[test]
    fn test_panic_path_tracker_early_return_and_lifo_order() {
        let func = make_fn(vec![
            HirStmt::Let {
                name: "first".to_string(),
                ty: Some(HirType::String),
                init: Some(HirExpr::Literal(HirLiteral::String("a".to_string()))),
                is_const: false,
                is_borrowed: None,
                is_atomic: false,
            },
            HirStmt::Let {
                name: "second".to_string(),
                ty: Some(HirType::String),
                init: Some(HirExpr::Literal(HirLiteral::String("b".to_string()))),
                is_const: false,
                is_borrowed: None,
                is_atomic: false,
            },
            HirStmt::If {
                cond: HirExpr::LoadVar("cond".to_string()),
                then_branch: Box::new(HirStmt::Return(None)),
                else_branch: None,
            },
        ]);

        let cfg = CfgBuilder::build(&func);
        let mut tracker = PanicPathTracker::new();
        assert!(tracker.analyze(&cfg).is_ok());
        assert!(tracker.validate_complete_cleanup().is_ok());

        let ret_cleanup = tracker
            .get_cleanup_info()
            .iter()
            .find(|c| matches!(c.exit_type, ExitPath::Return | ExitPath::EarlyReturn))
            .expect("expected return cleanup record");
        // LIFO order: `second` must be dropped before `first`
        let pos_second = ret_cleanup.drop_order.iter().position(|n| n == "second");
        let pos_first = ret_cleanup.drop_order.iter().position(|n| n == "first");
        assert!(pos_second < pos_first);
    }

    #[test]
    fn test_panic_path_tracker_throw_and_panic_call() {
        let func = make_fn(vec![
            HirStmt::Let {
                name: "res".to_string(),
                ty: Some(HirType::String),
                init: Some(HirExpr::Literal(HirLiteral::String("x".to_string()))),
                is_const: false,
                is_borrowed: None,
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::Call(
                Box::new(HirExpr::LoadVar("panic".to_string())),
                vec![HirExpr::Literal(HirLiteral::String("boom".to_string()))],
                vec![],
            )),
            HirStmt::Throw(HirExpr::Literal(HirLiteral::String("err".to_string()))),
        ]);

        let cfg = CfgBuilder::build(&func);
        let mut tracker = PanicPathTracker::new();
        assert!(tracker.analyze(&cfg).is_ok());
        assert!(tracker
            .get_cleanup_info()
            .iter()
            .any(|c| c.exit_type == ExitPath::Panic && c.drop_order.contains(&"res".to_string())));
        assert!(tracker
            .get_cleanup_info()
            .iter()
            .any(|c| c.exit_type == ExitPath::Throw && c.drop_order.contains(&"res".to_string())));
    }

    #[test]
    fn test_loop_borrow_validator_on_while_loop() {
        let func = make_fn(vec![HirStmt::While {
            cond: HirExpr::LoadVar("cond".to_string()),
            body: Box::new(HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(1)))),
        }]);
        let cfg = CfgBuilder::build(&func);
        let mut validator = LoopBorrowValidator::new();
        assert!(validator.analyze(&cfg).is_ok());
        assert!(!validator.loop_headers().is_empty());
    }
}
