//! Forward Dataflow Analysis for Borrow Checking
//!
//! Implements monotone worklist-based fixpoint iteration over the validated CFG
//! to compute borrow states at each program point.
//!
//! # Abstract Domain (Finite Join Semi-Lattice)
//!
//! For each variable `v`, the state lattice is:
//! `Unborrowed` < `{SharedBorrowed(origins), ExclusiveBorrowed(id)}` < `Moved` < `Freed` < `Conflict`
//!
//! - Bottom element: `Unborrowed`
//! - Top element: `Conflict` (conservative error state propagated on invalid merges)
//! - `SharedBorrowed` tracks a finite subset of CFG statement spans (idempotent on revisit)
//! - NLL release only lowers `{SharedBorrowed, ExclusiveBorrowed} -> Unborrowed` when
//!   no borrower variable is live along any redefinition-free path.
//!
//! # Algorithm
//!
//! 1. Structurally validate the CFG with `validate_cfg`; refuse malformed CFGs immediately.
//! 2. Compute reachability (`Reachability::compute`), borrower bindings, and variable reads/redefs.
//! 3. Run reverse-postorder worklist iteration until all reachable blocks reach a fixed point,
//!    propagating `CfgBorrowState::Conflict` on merge failures (never falling back to a single
//!    predecessor's state).
//! 4. Once converged, execute one deterministic collection pass over reachable blocks in RPO
//!    order to record all merge errors and borrow/move/free diagnostics without duplicates.

use super::cfg::{BlockId, ControlFlowGraph, validate_cfg};
use super::dominators::Reachability;
use super::errors::CfgBorrowError;
use super::merge::merge_states_with_recovery;
use super::{BorrowId, BorrowStateMap, BranchInfo, CfgBorrowState, SourceSpan};
use crate::parsing::hir::{HirExpr, HirStmt};
use std::collections::{HashMap, HashSet, VecDeque};

/// CFG-based borrow checker
pub struct CfgBorrowChecker {
    /// Errors collected during analysis
    errors: Vec<CfgBorrowError>,
}

impl CfgBorrowChecker {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Analyze a CFG for borrow violations.
    ///
    /// Returns `Ok(())` if the CFG is valid and no borrow/ownership/merge errors exist,
    /// or `Err(errors)` otherwise.
    pub fn analyze(&mut self, cfg: &mut ControlFlowGraph) -> Result<(), Vec<CfgBorrowError>> {
        self.errors.clear();

        // Step 1: Reject any malformed CFG before running dataflow.
        if let Err(validation_errs) = validate_cfg(cfg) {
            self.errors.push(CfgBorrowError::InvalidCfg {
                function_name: cfg.function_name.clone(),
                validation_errors: validation_errs.iter().map(|e| e.to_string()).collect(),
            });
            return Err(std::mem::take(&mut self.errors));
        }

        // Step 2: Compute reachability, reads, redefinitions, and borrower bindings.
        let reach = Reachability::compute(cfg);
        let reads_map = find_reads_in_cfg(cfg);
        let redefs_map = find_redefs_in_cfg(cfg);

        let mut borrowers_cache: HashMap<String, HashSet<String>> = HashMap::new();
        for var in cfg.variables.keys() {
            borrowers_cache.insert(var.clone(), find_borrowers_from_cfg(cfg, var));
        }

        // Initialize entry block's in_state and clear stale out_states on all blocks
        let initial_state = cfg.variables.clone();
        for block in &mut cfg.blocks {
            block.in_state.clear();
            block.out_state.clear();
        }
        if let Some(entry) = cfg.get_block_mut(cfg.entry) {
            entry.in_state = initial_state;
        }

        // Step 3: Monotone worklist fixpoint iteration over reachable blocks (seeded in RPO).
        let rpo = reach.rpo.clone();
        let mut worklist: VecDeque<BlockId> = rpo.iter().copied().collect();
        let mut in_worklist: HashSet<BlockId> = rpo.iter().copied().collect();

        // Bound worklist steps proportionally to graph and variable lattice size.
        let max_steps = (cfg.blocks.len().max(1) * (cfg.variables.len().max(1) + 1) * 16).max(2000);
        let mut iterations = 0;

        while let Some(block_id) = worklist.pop_front() {
            in_worklist.remove(&block_id);
            if !reach.is_reachable(block_id) {
                continue;
            }
            iterations += 1;

            if iterations > max_steps {
                self.errors.push(CfgBorrowError::FixpointNotReached {
                    function_name: cfg.function_name.clone(),
                    iterations,
                });
                return Err(std::mem::take(&mut self.errors));
            }

            let (predecessors, block_span) = match cfg.get_block(block_id) {
                Some(b) => (b.predecessors.clone(), b.span),
                None => continue,
            };

            if !predecessors.is_empty() {
                let pred_states: Vec<BorrowStateMap> = predecessors
                    .iter()
                    .filter(|&&p| reach.is_reachable(p))
                    .filter_map(|&p| cfg.get_block(p).map(|b| b.out_state.clone()))
                    .collect();

                if !pred_states.is_empty() {
                    let pred_refs: Vec<&BorrowStateMap> = pred_states.iter().collect();
                    let (mut merged, merge_errs) =
                        merge_states_with_recovery(&pred_refs, block_span);
                    for err in merge_errs {
                        if matches!(
                            err.kind,
                            super::errors::MergeErrorKind::FreedAcrossBranches { .. }
                        ) && !is_var_used_before_redef_at_block_entry(
                            cfg,
                            block_id,
                            &err.variable,
                            &reads_map,
                            &redefs_map,
                        ) {
                            merged.insert(err.variable, CfgBorrowState::Unborrowed);
                        }
                    }
                    if let Some(block) = cfg.get_block_mut(block_id) {
                        block.in_state = merged;
                    }
                }
            }

            // Compute out_state using transfer (silent mode during fixpoint convergence)
            let new_out_state =
                self.transfer_silent(cfg, block_id, &borrowers_cache, &reads_map, &redefs_map);

            let changed = cfg
                .get_block(block_id)
                .is_some_and(|b| new_out_state != b.out_state);

            if changed {
                if let Some(block) = cfg.get_block_mut(block_id) {
                    block.out_state = new_out_state;
                }

                let successors: Vec<BlockId> = cfg
                    .get_block(block_id)
                    .map(|b| b.successors.clone())
                    .unwrap_or_default();

                for succ in successors {
                    if reach.is_reachable(succ) && !in_worklist.contains(&succ) {
                        worklist.push_back(succ);
                        in_worklist.insert(succ);
                    }
                }
            }
        }

        // Step 4: Deterministic single error-collection pass at the converged fixed point.
        for block_id in rpo {
            let (predecessors, block_span) = match cfg.get_block(block_id) {
                Some(b) => (b.predecessors.clone(), b.span),
                None => continue,
            };

            if !predecessors.is_empty() {
                let pred_states: Vec<BorrowStateMap> = predecessors
                    .iter()
                    .filter(|&&p| reach.is_reachable(p))
                    .filter_map(|&p| cfg.get_block(p).map(|b| b.out_state.clone()))
                    .collect();
                if !pred_states.is_empty() {
                    let pred_refs: Vec<&BorrowStateMap> = pred_states.iter().collect();
                    let (_merged, merge_errs) = merge_states_with_recovery(&pred_refs, block_span);
                    for err in merge_errs {
                        if matches!(
                            err.kind,
                            super::errors::MergeErrorKind::FreedAcrossBranches { .. }
                        ) && !is_var_used_before_redef_at_block_entry(
                            cfg,
                            block_id,
                            &err.variable,
                            &reads_map,
                            &redefs_map,
                        ) {
                            continue;
                        }
                        self.errors.push(err.into());
                    }
                }
            }

            let _ = self.transfer(
                cfg,
                block_id,
                &borrowers_cache,
                &reads_map,
                &redefs_map,
                true,
            );
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    fn transfer_silent(
        &mut self,
        cfg: &ControlFlowGraph,
        block_id: BlockId,
        borrowers_cache: &HashMap<String, HashSet<String>>,
        reads_map: &HashMap<String, HashSet<(BlockId, usize)>>,
        redefs_map: &HashMap<String, HashSet<(BlockId, usize)>>,
    ) -> BorrowStateMap {
        self.transfer(cfg, block_id, borrowers_cache, reads_map, redefs_map, false)
    }

    /// Transfer function: compute out_state from in_state for a block
    fn transfer(
        &mut self,
        cfg: &ControlFlowGraph,
        block_id: BlockId,
        borrowers_cache: &HashMap<String, HashSet<String>>,
        reads_map: &HashMap<String, HashSet<(BlockId, usize)>>,
        redefs_map: &HashMap<String, HashSet<(BlockId, usize)>>,
        collect_errors: bool,
    ) -> BorrowStateMap {
        let block = match cfg.get_block(block_id) {
            Some(b) => b,
            None => return HashMap::new(),
        };

        let mut state = block.in_state.clone();
        let saved_errors_len = self.errors.len();

        for (idx, stmt) in block.statements.iter().enumerate() {
            // Release NLL borrows whose borrower variables are no longer live before `idx`
            // (and have no redefinition-free path to a future read).
            let borrowed_vars: Vec<String> = state
                .iter()
                .filter_map(|(k, v)| {
                    if matches!(
                        v,
                        CfgBorrowState::SharedBorrowed { .. }
                            | CfgBorrowState::ExclusiveBorrowed { .. }
                    ) {
                        Some(k.clone())
                    } else {
                        None
                    }
                })
                .collect();

            for var in borrowed_vars {
                if let Some(borrowers) = borrowers_cache.get(&var) {
                    if !is_borrow_used_before_redef(
                        cfg, block_id, idx, stmt, borrowers, reads_map, redefs_map,
                    ) {
                        state.insert(var, CfgBorrowState::Unborrowed);
                    }
                }
            }

            // Deterministic per-statement synthetic span when no AST span is attached,
            // ensuring loop revisits identify the exact same borrow origin cleanly.
            let stmt_span = if block.span != SourceSpan::default() {
                SourceSpan::new(
                    block.span.start + idx,
                    (block.span.start + idx + 1).max(block.span.end),
                )
            } else {
                let pos = block_id * 1024 + idx + 1;
                SourceSpan::new(pos, pos + 1)
            };

            self.process_stmt(stmt, &mut state, stmt_span);
        }

        if !collect_errors {
            self.errors.truncate(saved_errors_len);
        }

        state
    }

    /// Process a statement, updating borrow states
    fn process_stmt(&mut self, stmt: &HirStmt, state: &mut BorrowStateMap, span: SourceSpan) {
        match stmt {
            HirStmt::Let { name, init, .. } => {
                if let Some(expr) = init {
                    self.process_expr(expr, state, span);
                }
                state.insert(name.clone(), CfgBorrowState::Unborrowed);
            }

            HirStmt::LetTuple { names, init, .. } => {
                if let Some(expr) = init {
                    self.process_expr(expr, state, span);
                }
                for name in names {
                    state.insert(name.clone(), CfgBorrowState::Unborrowed);
                }
            }

            HirStmt::Assign {
                target,
                value,
                is_move: _,
            } => {
                if let HirExpr::LoadVar(name) = target {
                    self.check_not_borrowed(name, state, span);
                }
                // Evaluate target sub-expressions if non-trivial, then value
                if !matches!(target, HirExpr::LoadVar(_)) {
                    self.process_expr(target, state, span);
                }
                self.process_expr(value, state, span);
                if let HirExpr::LoadVar(name) = target {
                    // Reassignment re-initializes the target binding unless poisoned by
                    // Conflict or still actively borrowed.
                    if !matches!(
                        state.get(name),
                        Some(
                            CfgBorrowState::Conflict { .. }
                                | CfgBorrowState::SharedBorrowed { .. }
                                | CfgBorrowState::ExclusiveBorrowed { .. }
                        )
                    ) {
                        state.insert(name.clone(), CfgBorrowState::Unborrowed);
                    }
                }
            }

            HirStmt::Expr(expr) => {
                self.process_expr(expr, state, span);
            }

            HirStmt::Return(expr) => {
                if let Some(e) = expr {
                    self.process_expr(e, state, span);
                }
            }

            HirStmt::Throw(expr) | HirStmt::Jump(expr) => {
                self.process_expr(expr, state, span);
            }

            HirStmt::Unsafe(body) | HirStmt::Defer(body) => {
                self.process_stmt(body, state, span);
            }

            _ => {}
        }
    }

    /// Process an expression in strict left-to-right evaluation order,
    /// checking for borrow violations and updating state.
    fn process_expr(&mut self, expr: &HirExpr, state: &mut BorrowStateMap, span: SourceSpan) {
        match expr {
            HirExpr::LoadVar(name) => {
                self.check_accessible(name, state, span);
            }

            HirExpr::Borrow(inner, is_exclusive) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    self.process_borrow(name, *is_exclusive, state, span);
                } else {
                    self.process_expr(inner, state, span);
                }
            }

            HirExpr::BorrowImmut(inner) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    self.process_borrow(name, false, state, span);
                } else {
                    self.process_expr(inner, state, span);
                }
            }

            HirExpr::BorrowMut(inner) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    self.process_borrow(name, true, state, span);
                } else {
                    self.process_expr(inner, state, span);
                }
            }

            HirExpr::Move(inner) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    self.process_move(name, state, span);
                } else {
                    self.process_expr(inner, state, span);
                }
            }

            HirExpr::Free(inner) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    self.process_free(name, state, span);
                } else {
                    self.process_expr(inner, state, span);
                }
            }

            HirExpr::MethodCall(object, method, args) => {
                let is_exclusive = self.method_requires_exclusive(method);

                if let HirExpr::LoadVar(name) = object.as_ref() {
                    self.check_temporary_borrow(name, is_exclusive, state, span);
                } else {
                    self.process_expr(object, state, span);
                }

                for arg in args {
                    self.process_expr(arg, state, span);
                }
            }

            HirExpr::Call(func, args, _) => {
                self.process_expr(func, state, span);
                for arg in args {
                    self.process_expr(arg, state, span);
                }
            }

            HirExpr::BinaryOp(left, _, right) => {
                self.process_expr(left, state, span);
                self.process_expr(right, state, span);
            }

            HirExpr::UnaryOp(_, operand)
            | HirExpr::Share(operand)
            | HirExpr::Downgrade(operand)
            | HirExpr::Spread(operand)
            | HirExpr::NonNull(operand)
            | HirExpr::Format(operand, _)
            | HirExpr::Update(operand, _, _)
            | HirExpr::Alloc(_, operand) => {
                self.process_expr(operand, state, span);
            }

            HirExpr::Index(container, index) | HirExpr::Range(container, index, _) => {
                self.process_expr(container, state, span);
                self.process_expr(index, state, span);
            }

            HirExpr::MemberAccess(object, _) | HirExpr::OptionalGet(object, _) => {
                self.process_expr(object, state, span);
            }

            HirExpr::SetMember(object, _, value) => {
                if let HirExpr::LoadVar(name) = object.as_ref() {
                    self.check_temporary_borrow(name, true, state, span);
                } else {
                    self.process_expr(object, state, span);
                }
                self.process_expr(value, state, span);
            }

            HirExpr::AssignTuple(names, value) => {
                self.process_expr(value, state, span);
                for name in names {
                    state.insert(name.clone(), CfgBorrowState::Unborrowed);
                }
            }

            HirExpr::AssignObject(properties, value) => {
                self.process_expr(value, state, span);
                for (name, alias) in properties {
                    let target_var = alias.as_ref().unwrap_or(name);
                    state.insert(target_var.clone(), CfgBorrowState::Unborrowed);
                }
            }

            HirExpr::ArrayLiteral(elements)
            | HirExpr::SetLiteral(elements)
            | HirExpr::TupleLiteral(elements) => {
                for elem in elements {
                    self.process_expr(elem, state, span);
                }
            }

            HirExpr::DictLiteral(pairs) => {
                for (k, v) in pairs {
                    self.process_expr(k, state, span);
                    self.process_expr(v, state, span);
                }
            }

            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                for (_, v) in fields {
                    self.process_expr(v, state, span);
                }
            }

            HirExpr::Conditional(cond, then_expr, else_expr) => {
                // Fallback if an un-lowered Conditional reaches transfer
                self.process_expr(cond, state, span);
                let mut then_state = state.clone();
                let mut else_state = state.clone();
                self.process_expr(then_expr, &mut then_state, span);
                self.process_expr(else_expr, &mut else_state, span);
                let (merged, errs) = merge_states_with_recovery(&[&then_state, &else_state], span);
                for err in errs {
                    self.errors.push(err.into());
                }
                *state = merged;
            }

            HirExpr::Lambda(params, body, _) => {
                let mut shadowed: HashSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
                for stmt in body.iter() {
                    self.check_lambda_captures(stmt, state, &mut shadowed);
                }
            }

            HirExpr::StoreVar(name, value) => {
                self.process_expr(value, state, span);
                self.check_not_borrowed(name, state, span);
                if !matches!(
                    state.get(name),
                    Some(
                        CfgBorrowState::Conflict { .. }
                            | CfgBorrowState::SharedBorrowed { .. }
                            | CfgBorrowState::ExclusiveBorrowed { .. }
                    )
                ) {
                    state.insert(name.clone(), CfgBorrowState::Unborrowed);
                }
            }

            HirExpr::NewInstance(_, args) => {
                for arg in args {
                    self.process_expr(arg, state, span);
                }
            }

            HirExpr::Deref(inner)
            | HirExpr::Await(inner)
            | HirExpr::Spawn(inner)
            | HirExpr::Cast(inner, _) => {
                self.process_expr(inner, state, span);
            }

            HirExpr::Match(scrutinee, arms) => {
                self.process_expr(scrutinee, state, span);
                if !arms.is_empty() {
                    let mut arm_states = Vec::with_capacity(arms.len());
                    for (_, arm_expr) in arms {
                        let mut arm_state = state.clone();
                        self.process_expr(arm_expr, &mut arm_state, span);
                        arm_states.push(arm_state);
                    }
                    let refs: Vec<&BorrowStateMap> = arm_states.iter().collect();
                    let (merged, errs) = merge_states_with_recovery(&refs, span);
                    for err in errs {
                        self.errors.push(err.into());
                    }
                    *state = merged;
                }
            }

            HirExpr::Literal(_) | HirExpr::This | HirExpr::Super => {}
        }
    }

    /// Check if a variable is accessible (not moved, freed, or in conflict)
    fn check_accessible(&mut self, name: &str, state: &BorrowStateMap, span: SourceSpan) {
        if let Some(borrow_state) = state.get(name) {
            match borrow_state {
                CfgBorrowState::Moved { moved_at } => {
                    self.errors.push(CfgBorrowError::UseAfterMaybeMoved {
                        variable: name.to_string(),
                        moved_in_branch: BranchInfo::new("previous code", *moved_at),
                        use_location: span,
                    });
                }
                CfgBorrowState::Freed { freed_at } => {
                    self.errors.push(CfgBorrowError::UseAfterMaybeFreed {
                        variable: name.to_string(),
                        freed_in_branch: BranchInfo::new("previous code", *freed_at),
                        use_location: span,
                    });
                }
                CfgBorrowState::Conflict { .. } => {
                    // Merge conflict was already recorded at the join point;
                    // preserve the poisoned Conflict state without downgrading.
                }
                _ => {}
            }
        }
    }

    /// Check if a variable is not currently borrowed
    fn check_not_borrowed(&mut self, name: &str, state: &BorrowStateMap, span: SourceSpan) {
        if let Some(borrow_state) = state.get(name) {
            match borrow_state {
                CfgBorrowState::SharedBorrowed { borrow_origins, .. } => {
                    let origin = borrow_origins.first().copied().unwrap_or_default();
                    self.errors.push(CfgBorrowError::MoveWhileBorrowed {
                        variable: name.to_string(),
                        borrowed_at: origin,
                        move_at: span,
                    });
                }
                CfgBorrowState::ExclusiveBorrowed { borrow_origin, .. } => {
                    self.errors.push(CfgBorrowError::MoveWhileBorrowed {
                        variable: name.to_string(),
                        borrowed_at: *borrow_origin,
                        move_at: span,
                    });
                }
                _ => {}
            }
        }
    }

    /// Process a borrow, updating state and checking for conflicts
    fn process_borrow(
        &mut self,
        name: &str,
        is_exclusive: bool,
        state: &mut BorrowStateMap,
        span: SourceSpan,
    ) {
        self.check_accessible(name, state, span);

        let current_state = state
            .get(name)
            .cloned()
            .unwrap_or(CfgBorrowState::Unborrowed);

        match current_state {
            CfgBorrowState::Unborrowed => {
                if is_exclusive {
                    let borrow_id = span.start as BorrowId;
                    state.insert(
                        name.to_string(),
                        CfgBorrowState::ExclusiveBorrowed {
                            borrow_origin: span,
                            borrow_id,
                        },
                    );
                } else {
                    state.insert(
                        name.to_string(),
                        CfgBorrowState::SharedBorrowed {
                            borrow_origins: vec![span],
                            count: 1,
                        },
                    );
                }
            }

            CfgBorrowState::SharedBorrowed {
                mut borrow_origins,
                count,
            } => {
                if is_exclusive {
                    let origin = borrow_origins.first().copied().unwrap_or_default();
                    self.errors.push(CfgBorrowError::BorrowConflict {
                        variable: name.to_string(),
                        existing_borrow: BranchInfo::new("existing shared borrow", origin),
                        new_borrow: span,
                        existing_is_exclusive: false,
                        new_is_exclusive: true,
                    });
                } else if !borrow_origins.contains(&span) {
                    borrow_origins.push(span);
                    state.insert(
                        name.to_string(),
                        CfgBorrowState::SharedBorrowed {
                            borrow_origins,
                            count: count.saturating_add(1),
                        },
                    );
                }
            }

            CfgBorrowState::ExclusiveBorrowed { borrow_origin, .. } => {
                // If the exact same exclusive borrow site is revisited in a loop,
                // keep it idempotent; distinct sites conflict.
                if borrow_origin != span {
                    self.errors.push(CfgBorrowError::BorrowConflict {
                        variable: name.to_string(),
                        existing_borrow: BranchInfo::new(
                            "existing exclusive borrow",
                            borrow_origin,
                        ),
                        new_borrow: span,
                        existing_is_exclusive: true,
                        new_is_exclusive: is_exclusive,
                    });
                }
            }

            CfgBorrowState::Moved { .. }
            | CfgBorrowState::Freed { .. }
            | CfgBorrowState::Conflict { .. } => {}
        }
    }

    /// Check conflicts for a temporary borrow (like a method receiver) without modifying the state.
    fn check_temporary_borrow(
        &mut self,
        name: &str,
        is_exclusive: bool,
        state: &BorrowStateMap,
        span: SourceSpan,
    ) {
        self.check_accessible(name, state, span);

        let current_state = state
            .get(name)
            .cloned()
            .unwrap_or(CfgBorrowState::Unborrowed);

        match current_state {
            CfgBorrowState::Unborrowed => {}

            CfgBorrowState::SharedBorrowed {
                ref borrow_origins, ..
            } => {
                if is_exclusive {
                    let origin = borrow_origins.first().copied().unwrap_or_default();
                    self.errors.push(CfgBorrowError::BorrowConflict {
                        variable: name.to_string(),
                        existing_borrow: BranchInfo::new("existing shared borrow", origin),
                        new_borrow: span,
                        existing_is_exclusive: false,
                        new_is_exclusive: true,
                    });
                }
            }

            CfgBorrowState::ExclusiveBorrowed { borrow_origin, .. } => {
                self.errors.push(CfgBorrowError::BorrowConflict {
                    variable: name.to_string(),
                    existing_borrow: BranchInfo::new("existing exclusive borrow", borrow_origin),
                    new_borrow: span,
                    existing_is_exclusive: true,
                    new_is_exclusive: is_exclusive,
                });
            }

            CfgBorrowState::Moved { .. }
            | CfgBorrowState::Freed { .. }
            | CfgBorrowState::Conflict { .. } => {}
        }
    }

    /// Process a move, updating state
    fn process_move(&mut self, name: &str, state: &mut BorrowStateMap, span: SourceSpan) {
        self.check_not_borrowed(name, state, span);
        self.check_accessible(name, state, span);

        if !matches!(state.get(name), Some(CfgBorrowState::Conflict { .. })) {
            state.insert(name.to_string(), CfgBorrowState::Moved { moved_at: span });
        }
    }

    /// Process a free, updating state
    fn process_free(&mut self, name: &str, state: &mut BorrowStateMap, span: SourceSpan) {
        if let Some(borrow_state) = state.get(name) {
            match borrow_state {
                CfgBorrowState::SharedBorrowed { borrow_origins, .. } => {
                    let origin = borrow_origins.first().copied().unwrap_or_default();
                    self.errors.push(CfgBorrowError::FreeWhileBorrowed {
                        variable: name.to_string(),
                        borrowed_at: origin,
                        freed_at: span,
                    });
                }
                CfgBorrowState::ExclusiveBorrowed { borrow_origin, .. } => {
                    self.errors.push(CfgBorrowError::FreeWhileBorrowed {
                        variable: name.to_string(),
                        borrowed_at: *borrow_origin,
                        freed_at: span,
                    });
                }
                CfgBorrowState::Freed { freed_at } => {
                    self.errors.push(CfgBorrowError::UseAfterMaybeFreed {
                        variable: name.to_string(),
                        freed_in_branch: BranchInfo::new("previous free", *freed_at),
                        use_location: span,
                    });
                    return;
                }
                CfgBorrowState::Conflict { .. } => return,
                _ => {}
            }
        }

        state.insert(name.to_string(), CfgBorrowState::Freed { freed_at: span });
    }

    /// Heuristic to determine if a method requires exclusive access
    fn method_requires_exclusive(&self, method: &str) -> bool {
        let mutating_methods = [
            "push", "pop", "insert", "remove", "clear", "set", "update", "append", "extend", "add",
            "delete", "write", "put", "replace", "swap", "sort", "reverse", "shuffle",
        ];

        mutating_methods
            .iter()
            .any(|m| method.to_lowercase().contains(m))
    }

    /// Check lambda captures for borrow issues
    fn check_lambda_captures(
        &mut self,
        stmt: &HirStmt,
        outer_state: &BorrowStateMap,
        shadowed: &mut HashSet<String>,
    ) {
        match stmt {
            HirStmt::Let { name, init, .. } => {
                if let Some(expr) = init {
                    self.check_expr_captures(expr, outer_state, shadowed);
                }
                shadowed.insert(name.clone());
            }
            HirStmt::LetTuple { names, init, .. } => {
                if let Some(expr) = init {
                    self.check_expr_captures(expr, outer_state, shadowed);
                }
                for name in names {
                    shadowed.insert(name.clone());
                }
            }
            HirStmt::Assign { target, value, .. } => {
                self.check_expr_captures(target, outer_state, shadowed);
                self.check_expr_captures(value, outer_state, shadowed);
            }
            HirStmt::Expr(expr)
            | HirStmt::Throw(expr)
            | HirStmt::Jump(expr)
            | HirStmt::Return(Some(expr)) => {
                self.check_expr_captures(expr, outer_state, shadowed);
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.check_expr_captures(cond, outer_state, shadowed);
                let mut then_shadow = shadowed.clone();
                self.check_lambda_captures(then_branch, outer_state, &mut then_shadow);
                if let Some(eb) = else_branch {
                    let mut else_shadow = shadowed.clone();
                    self.check_lambda_captures(eb, outer_state, &mut else_shadow);
                }
            }
            HirStmt::While { cond, body } => {
                self.check_expr_captures(cond, outer_state, shadowed);
                let mut body_shadow = shadowed.clone();
                self.check_lambda_captures(body, outer_state, &mut body_shadow);
            }
            HirStmt::ForIn { var, iter, body } => {
                self.check_expr_captures(iter, outer_state, shadowed);
                let mut body_shadow = shadowed.clone();
                body_shadow.insert(var.clone());
                self.check_lambda_captures(body, outer_state, &mut body_shadow);
            }
            HirStmt::Block(stmts) => {
                let mut block_shadow = shadowed.clone();
                for s in stmts {
                    self.check_lambda_captures(s, outer_state, &mut block_shadow);
                }
            }
            HirStmt::TryCatch {
                try_block,
                error_name,
                catch_block,
            } => {
                let mut try_shadow = shadowed.clone();
                self.check_lambda_captures(try_block, outer_state, &mut try_shadow);
                let mut catch_shadow = shadowed.clone();
                if !error_name.is_empty() {
                    catch_shadow.insert(error_name.clone());
                }
                self.check_lambda_captures(catch_block, outer_state, &mut catch_shadow);
            }
            HirStmt::Region { body, .. } | HirStmt::Defer(body) | HirStmt::Unsafe(body) => {
                let mut inner_shadow = shadowed.clone();
                self.check_lambda_captures(body, outer_state, &mut inner_shadow);
            }
            _ => {}
        }
    }

    fn check_expr_captures(
        &mut self,
        expr: &HirExpr,
        outer_state: &BorrowStateMap,
        shadowed: &HashSet<String>,
    ) {
        match expr {
            HirExpr::LoadVar(name) => {
                if !shadowed.contains(name)
                    && let Some(state) = outer_state.get(name)
                    && matches!(
                        state,
                        CfgBorrowState::Moved { .. }
                            | CfgBorrowState::Freed { .. }
                            | CfgBorrowState::Conflict { .. }
                    )
                {
                    self.errors.push(CfgBorrowError::UseAfterMaybeMoved {
                        variable: name.clone(),
                        moved_in_branch: BranchInfo::new("outer scope", SourceSpan::default()),
                        use_location: SourceSpan::default(),
                    });
                }
            }
            HirExpr::BinaryOp(l, _, r) | HirExpr::Index(l, r) | HirExpr::Range(l, r, _) => {
                self.check_expr_captures(l, outer_state, shadowed);
                self.check_expr_captures(r, outer_state, shadowed);
            }
            HirExpr::UnaryOp(_, e)
            | HirExpr::Borrow(e, _)
            | HirExpr::BorrowImmut(e)
            | HirExpr::BorrowMut(e)
            | HirExpr::Move(e)
            | HirExpr::Free(e)
            | HirExpr::Deref(e)
            | HirExpr::Await(e)
            | HirExpr::Spawn(e)
            | HirExpr::Share(e)
            | HirExpr::Downgrade(e)
            | HirExpr::Spread(e)
            | HirExpr::NonNull(e)
            | HirExpr::Format(e, _)
            | HirExpr::Cast(e, _)
            | HirExpr::MemberAccess(e, _)
            | HirExpr::OptionalGet(e, _)
            | HirExpr::Update(e, _, _)
            | HirExpr::Alloc(_, e) => {
                self.check_expr_captures(e, outer_state, shadowed);
            }
            HirExpr::StoreVar(name, val) => {
                if !shadowed.contains(name)
                    && let Some(state) = outer_state.get(name)
                    && matches!(
                        state,
                        CfgBorrowState::Moved { .. }
                            | CfgBorrowState::Freed { .. }
                            | CfgBorrowState::Conflict { .. }
                    )
                {
                    self.errors.push(CfgBorrowError::UseAfterMaybeMoved {
                        variable: name.clone(),
                        moved_in_branch: BranchInfo::new("outer scope", SourceSpan::default()),
                        use_location: SourceSpan::default(),
                    });
                }
                self.check_expr_captures(val, outer_state, shadowed);
            }
            HirExpr::SetMember(obj, _, val) => {
                self.check_expr_captures(obj, outer_state, shadowed);
                self.check_expr_captures(val, outer_state, shadowed);
            }
            HirExpr::Call(func, args, _) => {
                self.check_expr_captures(func, outer_state, shadowed);
                for a in args {
                    self.check_expr_captures(a, outer_state, shadowed);
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.check_expr_captures(obj, outer_state, shadowed);
                for a in args {
                    self.check_expr_captures(a, outer_state, shadowed);
                }
            }
            HirExpr::NewInstance(_, args)
            | HirExpr::ArrayLiteral(args)
            | HirExpr::SetLiteral(args)
            | HirExpr::TupleLiteral(args) => {
                for a in args {
                    self.check_expr_captures(a, outer_state, shadowed);
                }
            }
            HirExpr::DictLiteral(pairs) => {
                for (k, v) in pairs {
                    self.check_expr_captures(k, outer_state, shadowed);
                    self.check_expr_captures(v, outer_state, shadowed);
                }
            }
            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                for (_, v) in fields {
                    self.check_expr_captures(v, outer_state, shadowed);
                }
            }
            HirExpr::Conditional(c, t, e) => {
                self.check_expr_captures(c, outer_state, shadowed);
                self.check_expr_captures(t, outer_state, shadowed);
                self.check_expr_captures(e, outer_state, shadowed);
            }
            HirExpr::Match(scrut, arms) => {
                self.check_expr_captures(scrut, outer_state, shadowed);
                for (_, body) in arms {
                    self.check_expr_captures(body, outer_state, shadowed);
                }
            }
            HirExpr::Lambda(params, body, _) => {
                let mut inner_shadow = shadowed.clone();
                for (n, _) in params {
                    inner_shadow.insert(n.clone());
                }
                for s in body.iter() {
                    self.check_lambda_captures(s, outer_state, &mut inner_shadow);
                }
            }
            HirExpr::AssignTuple(_, val) | HirExpr::AssignObject(_, val) => {
                self.check_expr_captures(val, outer_state, shadowed);
            }
            HirExpr::Literal(_) | HirExpr::This | HirExpr::Super => {}
        }
    }

    /// Get collected errors
    pub fn errors(&self) -> &[CfgBorrowError] {
        &self.errors
    }
}

impl Default for CfgBorrowChecker {
    fn default() -> Self {
        Self::new()
    }
}

fn find_borrowers_from_cfg(cfg: &ControlFlowGraph, borrowed_var: &str) -> HashSet<String> {
    let mut borrowers = HashSet::new();

    fn visit_stmt(stmt: &HirStmt, borrowed_var: &str, borrowers: &mut HashSet<String>) {
        match stmt {
            HirStmt::Let {
                name,
                init: Some(init_expr),
                ..
            } => {
                if let HirExpr::Borrow(inner, _)
                | HirExpr::BorrowImmut(inner)
                | HirExpr::BorrowMut(inner) = init_expr
                {
                    if let HirExpr::LoadVar(src) = inner.as_ref() {
                        if src == borrowed_var {
                            borrowers.insert(name.clone());
                        }
                    }
                }
            }
            HirStmt::Assign { target, value, .. } => {
                if let HirExpr::LoadVar(name) = target {
                    if let HirExpr::Borrow(inner, _)
                    | HirExpr::BorrowImmut(inner)
                    | HirExpr::BorrowMut(inner) = value
                    {
                        if let HirExpr::LoadVar(src) = inner.as_ref() {
                            if src == borrowed_var {
                                borrowers.insert(name.clone());
                            }
                        }
                    }
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                visit_stmt(then_branch, borrowed_var, borrowers);
                if let Some(eb) = else_branch {
                    visit_stmt(eb, borrowed_var, borrowers);
                }
            }
            HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
                visit_stmt(body, borrowed_var, borrowers);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    visit_stmt(s, borrowed_var, borrowers);
                }
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                visit_stmt(try_block, borrowed_var, borrowers);
                visit_stmt(catch_block, borrowed_var, borrowers);
            }
            HirStmt::Region { body, .. } | HirStmt::Defer(body) | HirStmt::Unsafe(body) => {
                visit_stmt(body, borrowed_var, borrowers);
            }
            _ => {}
        }
    }

    for block in &cfg.blocks {
        for stmt in &block.statements {
            visit_stmt(stmt, borrowed_var, &mut borrowers);
        }
    }

    borrowers
}

fn find_redefs_in_cfg(cfg: &ControlFlowGraph) -> HashMap<String, HashSet<(BlockId, usize)>> {
    let mut redefs: HashMap<String, HashSet<(BlockId, usize)>> = HashMap::new();
    for block in &cfg.blocks {
        for (idx, stmt) in block.statements.iter().enumerate() {
            match stmt {
                HirStmt::Let { name, .. } => {
                    redefs
                        .entry(name.clone())
                        .or_default()
                        .insert((block.id, idx));
                }
                HirStmt::LetTuple { names, .. } => {
                    for name in names {
                        redefs
                            .entry(name.clone())
                            .or_default()
                            .insert((block.id, idx));
                    }
                }
                HirStmt::Assign {
                    target: HirExpr::LoadVar(name),
                    ..
                } => {
                    redefs
                        .entry(name.clone())
                        .or_default()
                        .insert((block.id, idx));
                }
                HirStmt::Expr(HirExpr::StoreVar(name, _)) => {
                    redefs
                        .entry(name.clone())
                        .or_default()
                        .insert((block.id, idx));
                }
                _ => {}
            }
        }
    }
    redefs
}

fn expr_reads_var(expr: &HirExpr, var: &str) -> bool {
    match expr {
        HirExpr::LoadVar(n) => n == var,
        HirExpr::BinaryOp(a, _, b)
        | HirExpr::Index(a, b)
        | HirExpr::Range(a, b, _)
        | HirExpr::SetMember(a, _, b) => expr_reads_var(a, var) || expr_reads_var(b, var),
        HirExpr::UnaryOp(_, inner)
        | HirExpr::Deref(inner)
        | HirExpr::Share(inner)
        | HirExpr::Downgrade(inner)
        | HirExpr::Move(inner)
        | HirExpr::Borrow(inner, _)
        | HirExpr::BorrowImmut(inner)
        | HirExpr::BorrowMut(inner)
        | HirExpr::Await(inner)
        | HirExpr::Spawn(inner)
        | HirExpr::NonNull(inner)
        | HirExpr::Spread(inner)
        | HirExpr::Update(inner, _, _)
        | HirExpr::Cast(inner, _)
        | HirExpr::Format(inner, _)
        | HirExpr::Free(inner)
        | HirExpr::Alloc(_, inner)
        | HirExpr::MemberAccess(inner, _)
        | HirExpr::OptionalGet(inner, _)
        | HirExpr::StoreVar(_, inner)
        | HirExpr::AssignTuple(_, inner)
        | HirExpr::AssignObject(_, inner) => expr_reads_var(inner, var),
        HirExpr::Call(f, args, _) => {
            expr_reads_var(f, var) || args.iter().any(|a| expr_reads_var(a, var))
        }
        HirExpr::MethodCall(o, _, args) => {
            expr_reads_var(o, var) || args.iter().any(|a| expr_reads_var(a, var))
        }
        HirExpr::NewInstance(_, args)
        | HirExpr::ArrayLiteral(args)
        | HirExpr::SetLiteral(args)
        | HirExpr::TupleLiteral(args) => args.iter().any(|a| expr_reads_var(a, var)),
        HirExpr::DictLiteral(pairs) => pairs
            .iter()
            .any(|(k, v)| expr_reads_var(k, var) || expr_reads_var(v, var)),
        HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
            fields.iter().any(|(_, v)| expr_reads_var(v, var))
        }
        HirExpr::Conditional(c, t, e) => {
            expr_reads_var(c, var) || expr_reads_var(t, var) || expr_reads_var(e, var)
        }
        HirExpr::Match(s, arms) => {
            expr_reads_var(s, var) || arms.iter().any(|(_, e)| expr_reads_var(e, var))
        }
        HirExpr::Literal(_) | HirExpr::Lambda(..) | HirExpr::This | HirExpr::Super => false,
    }
}

fn find_reads_in_cfg(cfg: &ControlFlowGraph) -> HashMap<String, HashSet<(BlockId, usize)>> {
    let mut reads = HashMap::new();

    fn record_read(
        var: String,
        block_id: BlockId,
        stmt_idx: usize,
        reads: &mut HashMap<String, HashSet<(BlockId, usize)>>,
    ) {
        reads
            .entry(var)
            .or_insert_with(HashSet::new)
            .insert((block_id, stmt_idx));
    }

    fn visit_expr(
        expr: &HirExpr,
        block_id: BlockId,
        stmt_idx: usize,
        reads: &mut HashMap<String, HashSet<(BlockId, usize)>>,
    ) {
        match expr {
            HirExpr::LoadVar(name) => {
                record_read(name.clone(), block_id, stmt_idx, reads);
            }
            HirExpr::BinaryOp(left, _, right) => {
                visit_expr(left, block_id, stmt_idx, reads);
                visit_expr(right, block_id, stmt_idx, reads);
            }
            HirExpr::UnaryOp(_, operand) => {
                visit_expr(operand, block_id, stmt_idx, reads);
            }
            HirExpr::Call(func, args, _) => {
                visit_expr(func, block_id, stmt_idx, reads);
                for arg in args {
                    visit_expr(arg, block_id, stmt_idx, reads);
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                visit_expr(obj, block_id, stmt_idx, reads);
                for arg in args {
                    visit_expr(arg, block_id, stmt_idx, reads);
                }
            }
            HirExpr::ArrayLiteral(elements)
            | HirExpr::SetLiteral(elements)
            | HirExpr::TupleLiteral(elements) => {
                for elem in elements {
                    visit_expr(elem, block_id, stmt_idx, reads);
                }
            }
            HirExpr::DictLiteral(pairs) => {
                for (k, v) in pairs {
                    visit_expr(k, block_id, stmt_idx, reads);
                    visit_expr(v, block_id, stmt_idx, reads);
                }
            }
            HirExpr::ObjectLiteral(props) => {
                for (_, v) in props {
                    visit_expr(v, block_id, stmt_idx, reads);
                }
            }
            HirExpr::AssignTuple(_, val) | HirExpr::AssignObject(_, val) => {
                visit_expr(val, block_id, stmt_idx, reads);
            }
            HirExpr::StructLiteral(_, fields) => {
                for (_, v) in fields {
                    visit_expr(v, block_id, stmt_idx, reads);
                }
            }
            HirExpr::Index(arr, idx) => {
                visit_expr(arr, block_id, stmt_idx, reads);
                visit_expr(idx, block_id, stmt_idx, reads);
            }
            HirExpr::MemberAccess(obj, _) => {
                visit_expr(obj, block_id, stmt_idx, reads);
            }
            HirExpr::SetMember(obj, _, val) => {
                visit_expr(obj, block_id, stmt_idx, reads);
                visit_expr(val, block_id, stmt_idx, reads);
            }
            HirExpr::Conditional(cond, then_expr, else_expr) => {
                visit_expr(cond, block_id, stmt_idx, reads);
                visit_expr(then_expr, block_id, stmt_idx, reads);
                visit_expr(else_expr, block_id, stmt_idx, reads);
            }
            HirExpr::StoreVar(name, val) => {
                record_read(name.clone(), block_id, stmt_idx, reads);
                visit_expr(val, block_id, stmt_idx, reads);
            }
            HirExpr::Lambda(_, body, _) => {
                for s in body.iter() {
                    visit_stmt(s, block_id, stmt_idx, reads);
                }
            }
            HirExpr::NewInstance(_, args) => {
                for arg in args {
                    visit_expr(arg, block_id, stmt_idx, reads);
                }
            }
            HirExpr::Await(val)
            | HirExpr::Spawn(val)
            | HirExpr::Spread(val)
            | HirExpr::NonNull(val)
            | HirExpr::Format(val, _)
            | HirExpr::Share(val)
            | HirExpr::Downgrade(val)
            | HirExpr::Deref(val)
            | HirExpr::Cast(val, _)
            | HirExpr::Alloc(_, val) => {
                visit_expr(val, block_id, stmt_idx, reads);
            }
            HirExpr::Range(start, end, _) => {
                visit_expr(start, block_id, stmt_idx, reads);
                visit_expr(end, block_id, stmt_idx, reads);
            }
            HirExpr::OptionalGet(val, _) => {
                visit_expr(val, block_id, stmt_idx, reads);
            }
            HirExpr::Update(val, _, _) => {
                visit_expr(val, block_id, stmt_idx, reads);
            }
            HirExpr::Match(val, arms) => {
                visit_expr(val, block_id, stmt_idx, reads);
                for (_, body) in arms {
                    visit_expr(body, block_id, stmt_idx, reads);
                }
            }
            HirExpr::Borrow(val, _)
            | HirExpr::BorrowImmut(val)
            | HirExpr::BorrowMut(val)
            | HirExpr::Move(val)
            | HirExpr::Free(val) => {
                visit_expr(val, block_id, stmt_idx, reads);
            }
            _ => {}
        }
    }

    fn visit_stmt(
        stmt: &HirStmt,
        block_id: BlockId,
        stmt_idx: usize,
        reads: &mut HashMap<String, HashSet<(BlockId, usize)>>,
    ) {
        match stmt {
            HirStmt::Let {
                init: Some(expr), ..
            }
            | HirStmt::LetTuple {
                init: Some(expr), ..
            } => visit_expr(expr, block_id, stmt_idx, reads),
            HirStmt::Assign { target, value, .. } => {
                visit_expr(target, block_id, stmt_idx, reads);
                visit_expr(value, block_id, stmt_idx, reads);
            }
            HirStmt::Expr(expr)
            | HirStmt::Throw(expr)
            | HirStmt::Jump(expr)
            | HirStmt::Return(Some(expr)) => visit_expr(expr, block_id, stmt_idx, reads),
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                visit_expr(cond, block_id, stmt_idx, reads);
                visit_stmt(then_branch, block_id, stmt_idx, reads);
                if let Some(eb) = else_branch {
                    visit_stmt(eb, block_id, stmt_idx, reads);
                }
            }
            HirStmt::While { cond, body } => {
                visit_expr(cond, block_id, stmt_idx, reads);
                visit_stmt(body, block_id, stmt_idx, reads);
            }
            HirStmt::ForIn { var: _, iter, body } => {
                visit_expr(iter, block_id, stmt_idx, reads);
                visit_stmt(body, block_id, stmt_idx, reads);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    visit_stmt(s, block_id, stmt_idx, reads);
                }
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                visit_stmt(try_block, block_id, stmt_idx, reads);
                visit_stmt(catch_block, block_id, stmt_idx, reads);
            }
            HirStmt::Region { body, .. } | HirStmt::Defer(body) | HirStmt::Unsafe(body) => {
                visit_stmt(body, block_id, stmt_idx, reads);
            }
            _ => {}
        }
    }

    for block in &cfg.blocks {
        for (idx, stmt) in block.statements.iter().enumerate() {
            visit_stmt(stmt, block.id, idx, &mut reads);
        }
    }

    reads
}

/// Returns true if any `borrower` in `borrowers` is read at or after `(block_id, stmt_index)`
/// along a path that does NOT first redefine that borrower.
fn is_borrow_used_before_redef(
    cfg: &ControlFlowGraph,
    block_id: BlockId,
    stmt_index: usize,
    current_stmt: &HirStmt,
    borrowers: &HashSet<String>,
    reads_map: &HashMap<String, HashSet<(BlockId, usize)>>,
    redefs_map: &HashMap<String, HashSet<(BlockId, usize)>>,
) -> bool {
    if borrowers.is_empty() {
        return false;
    }

    for borrower in borrowers {
        let Some(read_locs) = reads_map.get(borrower) else {
            continue;
        };
        if read_locs.is_empty() {
            continue;
        }
        let redef_locs = redefs_map.get(borrower);

        // If the current statement itself redefines `borrower`:
        // - If its RHS/expression reads the old `borrower`, the borrow is still live now.
        // - Otherwise, the redefinition kills the prior borrow before any subsequent statement!
        if redef_locs.is_some_and(|s| s.contains(&(block_id, stmt_index))) {
            let rhs_reads = match current_stmt {
                HirStmt::Let {
                    init: Some(expr), ..
                } => expr_reads_var(expr, borrower),
                HirStmt::Assign { value, .. } => expr_reads_var(value, borrower),
                HirStmt::Expr(HirExpr::StoreVar(_, val)) => expr_reads_var(val, borrower),
                _ => false,
            };
            if rhs_reads {
                return true;
            }
            continue;
        }

        // Check statements in the rest of `block_id` (`stmt_index + 1..`)
        let block_len = cfg
            .get_block(block_id)
            .map(|b| b.statements.len())
            .unwrap_or(0);
        let mut killed_in_block = false;
        for idx in (stmt_index + 1)..block_len {
            if read_locs.contains(&(block_id, idx)) {
                return true;
            }
            if redef_locs.is_some_and(|s| s.contains(&(block_id, idx))) {
                killed_in_block = true;
                break;
            }
        }
        if killed_in_block {
            continue;
        }

        // BFS into successor blocks, stopping along any block path that redefines `borrower`
        // before reading it.
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        if let Some(b) = cfg.get_block(block_id) {
            for &succ in &b.successors {
                queue.push_back(succ);
            }
        }
        while let Some(curr_id) = queue.pop_front() {
            if !visited.insert(curr_id) {
                continue;
            }
            let curr_len = cfg
                .get_block(curr_id)
                .map(|b| b.statements.len())
                .unwrap_or(0);
            let mut killed_here = false;
            for idx in 0..curr_len {
                if read_locs.contains(&(curr_id, idx)) {
                    return true;
                }
                if redef_locs.is_some_and(|s| s.contains(&(curr_id, idx))) {
                    killed_here = true;
                    break;
                }
            }
            if !killed_here {
                if let Some(curr_block) = cfg.get_block(curr_id) {
                    for &succ in &curr_block.successors {
                        queue.push_back(succ);
                    }
                }
            }
        }
    }

    false
}

/// Returns true if `var` is read (or freed) at or after the entry of `block_id`
/// along any path that does not first redefine `var` (`let var = ...` or `var = ...`
/// with a RHS that does not read `var`).
fn is_var_used_before_redef_at_block_entry(
    cfg: &ControlFlowGraph,
    block_id: BlockId,
    var: &str,
    reads_map: &HashMap<String, HashSet<(BlockId, usize)>>,
    redefs_map: &HashMap<String, HashSet<(BlockId, usize)>>,
) -> bool {
    let Some(read_locs) = reads_map.get(var) else {
        return false;
    };
    if read_locs.is_empty() {
        return false;
    }
    let redef_locs = redefs_map.get(var);

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(block_id);

    while let Some(curr_id) = queue.pop_front() {
        if !visited.insert(curr_id) {
            continue;
        }
        let Some(curr_block) = cfg.get_block(curr_id) else {
            continue;
        };
        let mut killed_here = false;
        for (idx, stmt) in curr_block.statements.iter().enumerate() {
            if redef_locs.is_some_and(|s| s.contains(&(curr_id, idx))) {
                let rhs_reads = match stmt {
                    HirStmt::Let {
                        init: Some(expr), ..
                    } => expr_reads_var(expr, var),
                    HirStmt::Assign { value, .. } => expr_reads_var(value, var),
                    HirStmt::Expr(HirExpr::StoreVar(_, val)) => expr_reads_var(val, var),
                    _ => false,
                };
                if rhs_reads {
                    return true;
                }
                killed_here = true;
                break;
            }
            if read_locs.contains(&(curr_id, idx)) {
                return true;
            }
        }
        if !killed_here {
            for &succ in &curr_block.successors {
                queue.push_back(succ);
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::cfg_borrow::cfg::{BasicBlock, BlockKind, CfgBuilder};
    use crate::parsing::hir::{HirFunction, HirLiteral, HirType};
    use std::sync::Arc;

    fn make_test_function(body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "test".to_string(),
            params: vec![("x".to_string(), Some(HirType::Int), None)],
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
    fn test_simple_no_errors() {
        let func = make_test_function(vec![HirStmt::Let {
            name: "y".to_string(),
            ty: Some(HirType::Int),
            init: Some(HirExpr::Literal(HirLiteral::Int(42))),
            is_const: false,
            is_borrowed: None,
            is_atomic: false,
        }]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();

        let result = checker.analyze(&mut cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_use_after_move() {
        let func = make_test_function(vec![
            HirStmt::Let {
                name: "data".to_string(),
                ty: None,
                init: Some(HirExpr::Literal(HirLiteral::String("hello".to_string()))),
                is_const: false,
                is_borrowed: None,
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::Move(Box::new(HirExpr::LoadVar(
                "data".to_string(),
            )))),
            HirStmt::Expr(HirExpr::LoadVar("data".to_string())),
        ]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();

        let result = checker.analyze(&mut cfg);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CfgBorrowError::UseAfterMaybeMoved { .. }))
        );
    }

    #[test]
    fn test_borrow_conflict() {
        let func = make_test_function(vec![
            HirStmt::Let {
                name: "r1".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                )),
                is_const: false,
                is_borrowed: Some(false),
                is_atomic: false,
            },
            HirStmt::Let {
                name: "r2".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    true,
                )),
                is_const: false,
                is_borrowed: Some(true),
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::LoadVar("r1".to_string())),
            HirStmt::Expr(HirExpr::LoadVar("r2".to_string())),
        ]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();

        let result = checker.analyze(&mut cfg);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CfgBorrowError::BorrowConflict { .. }))
        );
    }

    #[test]
    fn test_nll_releases_unused_borrow_before_exclusive_borrow() {
        let func = make_test_function(vec![
            HirStmt::Let {
                name: "r1".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                )),
                is_const: false,
                is_borrowed: Some(false),
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::LoadVar("r1".to_string())),
            // r1 is never used after this point -> NLL allows &mut x!
            HirStmt::Let {
                name: "r2".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    true,
                )),
                is_const: false,
                is_borrowed: Some(true),
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::LoadVar("r2".to_string())),
        ]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();
        assert!(checker.analyze(&mut cfg).is_ok());
    }

    #[test]
    fn test_multiple_shared_ok() {
        let func = make_test_function(vec![
            HirStmt::Let {
                name: "r1".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                )),
                is_const: false,
                is_borrowed: Some(false),
                is_atomic: false,
            },
            HirStmt::Let {
                name: "r2".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                )),
                is_const: false,
                is_borrowed: Some(false),
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::LoadVar("r1".to_string())),
            HirStmt::Expr(HirExpr::LoadVar("r2".to_string())),
        ]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();

        let result = checker.analyze(&mut cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_free_while_borrowed() {
        let func = make_test_function(vec![
            HirStmt::Let {
                name: "r".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                )),
                is_const: false,
                is_borrowed: Some(false),
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::Free(Box::new(HirExpr::LoadVar("x".to_string())))),
            HirStmt::Expr(HirExpr::LoadVar("r".to_string())),
        ]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();

        let result = checker.analyze(&mut cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|e| matches!(e, CfgBorrowError::FreeWhileBorrowed { .. }))
        );
    }

    #[test]
    fn test_if_else_consistent_borrows() {
        let func = make_test_function(vec![
            HirStmt::Let {
                name: "data".to_string(),
                ty: None,
                init: Some(HirExpr::Literal(HirLiteral::Int(1))),
                is_const: false,
                is_borrowed: None,
                is_atomic: false,
            },
            HirStmt::If {
                cond: HirExpr::LoadVar("x".to_string()),
                then_branch: Box::new(HirStmt::Let {
                    name: "r".to_string(),
                    ty: None,
                    init: Some(HirExpr::Borrow(
                        Box::new(HirExpr::LoadVar("data".to_string())),
                        false,
                    )),
                    is_const: false,
                    is_borrowed: Some(false),
                    is_atomic: false,
                }),
                else_branch: Some(Box::new(HirStmt::Let {
                    name: "r".to_string(),
                    ty: None,
                    init: Some(HirExpr::Borrow(
                        Box::new(HirExpr::LoadVar("data".to_string())),
                        false,
                    )),
                    is_const: false,
                    is_borrowed: Some(false),
                    is_atomic: false,
                })),
            },
        ]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();
        assert!(checker.analyze(&mut cfg).is_ok());
    }

    #[test]
    fn test_nested_merge_conflict_cannot_be_bypassed() {
        // Branch 1 takes shared borrow of `data`; Branch 2 takes exclusive borrow of `data`.
        // Even if a subsequent nested branch joins afterwards, the Conflict must propagate
        // and refuse compilation.
        let func = make_test_function(vec![
            HirStmt::Let {
                name: "data".to_string(),
                ty: None,
                init: Some(HirExpr::Literal(HirLiteral::Int(1))),
                is_const: false,
                is_borrowed: None,
                is_atomic: false,
            },
            HirStmt::If {
                cond: HirExpr::LoadVar("x".to_string()),
                then_branch: Box::new(HirStmt::Let {
                    name: "r".to_string(),
                    ty: None,
                    init: Some(HirExpr::Borrow(
                        Box::new(HirExpr::LoadVar("data".to_string())),
                        false,
                    )),
                    is_const: false,
                    is_borrowed: Some(false),
                    is_atomic: false,
                }),
                else_branch: Some(Box::new(HirStmt::Let {
                    name: "r".to_string(),
                    ty: None,
                    init: Some(HirExpr::Borrow(
                        Box::new(HirExpr::LoadVar("data".to_string())),
                        true,
                    )),
                    is_const: false,
                    is_borrowed: Some(true),
                    is_atomic: false,
                })),
            },
            HirStmt::Expr(HirExpr::LoadVar("r".to_string())),
        ]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();
        let res = checker.analyze(&mut cfg);
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .iter()
                .any(|e| matches!(e, CfgBorrowError::MergeError(_)))
        );
    }

    #[test]
    fn test_loop_redeclared_exclusive_borrow_converges_cleanly() {
        let func = make_test_function(vec![HirStmt::While {
            cond: HirExpr::LoadVar("x".to_string()),
            body: Box::new(HirStmt::Block(vec![
                HirStmt::Let {
                    name: "r".to_string(),
                    ty: None,
                    init: Some(HirExpr::Borrow(
                        Box::new(HirExpr::LoadVar("x".to_string())),
                        true,
                    )),
                    is_const: false,
                    is_borrowed: Some(true),
                    is_atomic: false,
                },
                HirStmt::Expr(HirExpr::LoadVar("r".to_string())),
            ])),
        }]);

        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();
        assert!(checker.analyze(&mut cfg).is_ok());
    }

    #[test]
    fn test_malformed_cfg_rejected_by_analyzer() {
        let mut bad = ControlFlowGraph::new_empty("bad".to_string());
        let mut b0 = BasicBlock::new_with_kind(0, BlockKind::Entry);
        let b1 = BasicBlock::new_with_kind(1, BlockKind::Exit);
        b0.successors.push(1); // missing symmetric b1.predecessors.push(0)
        bad.blocks = vec![b0, b1];
        bad.entry = 0;
        bad.exits = vec![1];

        let mut checker = CfgBorrowChecker::new();
        let err = checker.analyze(&mut bad).unwrap_err();
        assert!(
            err.iter()
                .any(|e| matches!(e, CfgBorrowError::InvalidCfg { .. }))
        );
    }

    #[test]
    fn test_method_exclusive_detection() {
        let checker = CfgBorrowChecker::new();
        assert!(checker.method_requires_exclusive("push"));
        assert!(checker.method_requires_exclusive("update"));
        assert!(!checker.method_requires_exclusive("read"));
        assert!(!checker.method_requires_exclusive("get"));
    }

    /// Independent reference implementation on small generated acyclic CFGs:
    /// enumerates all paths from entry to each block and checks whether use-after-move
    /// matches `CfgBorrowChecker`.
    #[test]
    fn test_property_move_reachability_matches_reference_paths() {
        let mut seed: u64 = 0x5EED_C0DE_1913;
        let mut next_u64 = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };

        for trial in 0..25 {
            let n = 5; // blocks: 0=entry, 1..=3=middle, 4=exit
            let mut cfg = ControlFlowGraph::new_empty(format!("prop_{}", trial));
            cfg.variables
                .insert("v".to_string(), CfgBorrowState::Unborrowed);
            let mut blocks = Vec::new();
            blocks.push(BasicBlock::new_with_kind(0, BlockKind::Entry));
            for i in 1..4 {
                blocks.push(BasicBlock::new_with_kind(i, BlockKind::Normal));
            }
            blocks.push(BasicBlock::new_with_kind(4, BlockKind::Exit));

            // Random DAG edges i -> j with i < j, ensuring connected entry -> exit
            for i in 0..4 {
                let mut added = false;
                for j in (i + 1)..n {
                    if (next_u64() % 3) == 0 || (!added && j == 4) {
                        blocks[i].successors.push(j);
                        blocks[j].predecessors.push(i);
                        added = true;
                    }
                }
            }
            // Assign either Move(v) or LoadVar(v) or noop to middle blocks
            let mut has_move_at = vec![false; n];
            let mut has_read_at = vec![false; n];
            for i in 1..4 {
                match next_u64() % 3 {
                    0 => {
                        blocks[i]
                            .statements
                            .push(HirStmt::Expr(HirExpr::Move(Box::new(HirExpr::LoadVar(
                                "v".to_string(),
                            )))));
                        has_move_at[i] = true;
                    }
                    1 => {
                        blocks[i]
                            .statements
                            .push(HirStmt::Expr(HirExpr::LoadVar("v".to_string())));
                        has_read_at[i] = true;
                    }
                    _ => {}
                }
            }

            cfg.blocks = blocks;
            cfg.entry = 0;
            cfg.exits = vec![4];
            assert!(validate_cfg(&cfg).is_ok());

            // Independent path enumeration: is there a path from 0 to a block that moves `v`
            // and later (either in a successor or same block — here single stmt per block) reads `v`?
            fn dfs_find_use_after_move(
                cfg: &ControlFlowGraph,
                curr: BlockId,
                moved_on_path: bool,
                has_move_at: &[bool],
                has_read_at: &[bool],
            ) -> bool {
                let is_violation = moved_on_path && (has_read_at[curr] || has_move_at[curr]);
                if is_violation {
                    return true;
                }
                let next_moved = moved_on_path || has_move_at[curr];
                for &succ in &cfg.blocks[curr].successors {
                    if dfs_find_use_after_move(cfg, succ, next_moved, has_move_at, has_read_at) {
                        return true;
                    }
                }
                false
            }
            let ref_violation = dfs_find_use_after_move(&cfg, 0, false, &has_move_at, &has_read_at);

            let mut checker = CfgBorrowChecker::new();
            let got_err = checker.analyze(&mut cfg).is_err();
            assert_eq!(
                got_err, ref_violation,
                "trial {} mismatch: checker={}, ref={}",
                trial, got_err, ref_violation
            );
        }
    }

    #[test]
    fn test_reassign_while_live_borrow_rejected_and_after_nll_accepted() {
        // Negative: `r` borrows `x`, `x` is reassigned while `r` is still live afterwards.
        let bad_func = make_test_function(vec![
            HirStmt::Let {
                name: "r".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                )),
                is_const: false,
                is_borrowed: Some(false),
                is_atomic: false,
            },
            HirStmt::Assign {
                target: HirExpr::LoadVar("x".to_string()),
                value: HirExpr::Literal(HirLiteral::Int(99)),
                is_move: false,
            },
            HirStmt::Expr(HirExpr::LoadVar("r".to_string())),
        ]);
        let mut bad_cfg = CfgBuilder::build(&bad_func);
        let mut checker = CfgBorrowChecker::new();
        assert!(
            checker.analyze(&mut bad_cfg).is_err(),
            "reassigning `x` while borrow `r` is still live must fail"
        );

        // Positive: `r` last used before `x` is reassigned (NLL releases borrow).
        let ok_func = make_test_function(vec![
            HirStmt::Let {
                name: "r".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                )),
                is_const: false,
                is_borrowed: Some(false),
                is_atomic: false,
            },
            HirStmt::Expr(HirExpr::LoadVar("r".to_string())),
            HirStmt::Assign {
                target: HirExpr::LoadVar("x".to_string()),
                value: HirExpr::Literal(HirLiteral::Int(99)),
                is_move: false,
            },
        ]);
        let mut ok_cfg = CfgBuilder::build(&ok_func);
        let mut checker2 = CfgBorrowChecker::new();
        assert!(
            checker2.analyze(&mut ok_cfg).is_ok(),
            "reassigning `x` after last use of borrow `r` must succeed"
        );
    }

    #[test]
    fn test_lambda_nested_expr_capture_of_freed_or_moved_rejected() {
        let func = make_test_function(vec![
            HirStmt::Expr(HirExpr::Free(Box::new(HirExpr::LoadVar("x".to_string())))),
            HirStmt::Let {
                name: "f".to_string(),
                ty: None,
                init: Some(HirExpr::Lambda(
                    vec![("y".to_string(), None)],
                    Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
                        Box::new(HirExpr::LoadVar("x".to_string())),
                        crate::parsing::hir::BinOp::Add,
                        Box::new(HirExpr::LoadVar("y".to_string())),
                    )))]),
                    false,
                )),
                is_const: false,
                is_borrowed: None,
                is_atomic: false,
            },
        ]);
        let mut cfg = CfgBuilder::build(&func);
        let mut checker = CfgBorrowChecker::new();
        assert!(
            checker.analyze(&mut cfg).is_err(),
            "capturing freed variable inside nested lambda expression must fail"
        );
    }

    #[test]
    fn test_freed_across_branches_with_subsequent_use_or_double_free_rejected() {
        let double_free_func = make_test_function(vec![
            HirStmt::If {
                cond: HirExpr::Literal(HirLiteral::Bool(true)),
                then_branch: Box::new(HirStmt::Expr(HirExpr::Free(Box::new(HirExpr::LoadVar(
                    "x".to_string(),
                ))))),
                else_branch: Some(Box::new(HirStmt::Expr(HirExpr::Free(Box::new(
                    HirExpr::LoadVar("x".to_string()),
                ))))),
            },
            HirStmt::Expr(HirExpr::Free(Box::new(HirExpr::LoadVar("x".to_string())))),
        ]);
        let mut cfg = CfgBuilder::build(&double_free_func);
        let mut checker = CfgBorrowChecker::new();
        assert!(
            checker.analyze(&mut cfg).is_err(),
            "freeing after both branches already freed must fail"
        );
    }

    #[test]
    fn test_deterministic_bounded_fuzz_malformed_cfg_and_hir() {
        let mut seed: u64 = 0xF022_C0DE_1919;
        let mut next_u64 = || -> u64 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };

        for trial in 0..50 {
            let n = 3 + (next_u64() % 4) as usize;
            let mut cfg = ControlFlowGraph::new_empty(format!("fuzz_{trial}"));
            let mut blocks = Vec::with_capacity(n);
            for i in 0..n {
                let kind = match next_u64() % 5 {
                    0 => BlockKind::Entry,
                    1 => BlockKind::Exit,
                    2 => BlockKind::Return,
                    _ => BlockKind::Normal,
                };
                blocks.push(BasicBlock::new_with_kind(i, kind));
            }
            // Deliberately inject asymmetric/dangling/out-of-bounds edges.
            for b in &mut blocks {
                if (next_u64() % 2) == 0 {
                    b.successors.push((next_u64() % 10) as usize);
                }
                if (next_u64() % 3) == 0 {
                    b.predecessors.push((next_u64() % 10) as usize);
                }
            }
            cfg.blocks = blocks;
            cfg.entry = (next_u64() % 8) as usize;
            cfg.exits = if (next_u64() % 4) == 0 {
                vec![]
            } else {
                vec![n - 1]
            };

            let mut checker = CfgBorrowChecker::new();
            let result = checker.analyze(&mut cfg);
            // If `validate_cfg` fails, `analyze` must also fail (never allow malformed CFG).
            if validate_cfg(&cfg).is_err() {
                assert!(
                    result.is_err(),
                    "malformed CFG in trial {trial} must be rejected"
                );
            }
        }
    }
}
