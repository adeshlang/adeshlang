use crate::parsing::hir::*;
/// Compile-time borrow checking pass for HIR
/// Enforces:
/// - free-while-borrowed prevention
/// - move-while-borrowed prevention  
/// - borrow state propagation through control flow
/// - function boundary borrow validation
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum BorrowState {
    /// Variable is owned and not borrowed
    Owned,
    /// Variable is borrowed (immutable borrow)
    Borrowed {
        borrowed_at: usize, // source span
        borrow_count: usize,
    },
    /// Variable is mutably borrowed
    MutBorrowed {
        borrowed_at: usize, // source span
    },
    /// Variable has been moved
    Moved,
    /// Variable has been freed
    Freed,
}

#[derive(Debug)]
pub enum BorrowCheckError {
    /// Attempted to free a borrowed value
    FreeWhileBorrowed {
        variable: String,
        borrowed_at: usize,
        freed_at: usize,
    },
    /// Attempted to move a borrowed value
    MoveWhileBorrowed {
        variable: String,
        borrowed_at: usize,
        move_at: usize,
    },
    /// Attempted to use a freed value
    UseAfterFree {
        variable: String,
        freed_at: usize,
        use_at: usize,
    },
    /// Attempted to create mutable borrow while immutable borrow exists
    MutableBorrowWhileBorrowed {
        variable: String,
        previous_borrow: usize,
        mutable_borrow_at: usize,
    },
    /// Mismatched borrow state at control flow join
    BorrowStateMismatch {
        variable: String,
        paths: Vec<BorrowState>,
    },
}

pub struct BorrowChecker {
    state_map: HashMap<String, BorrowState>,
    errors: Vec<BorrowCheckError>,
    /// Use CFG-based analysis for functions with control flow
    use_cfg_analysis: bool,
}

impl BorrowChecker {
    pub fn new() -> Self {
        BorrowChecker {
            state_map: HashMap::new(),
            errors: Vec::new(),
            use_cfg_analysis: true,
        }
    }

    /// Create a checker with CFG analysis disabled (for testing/compatibility)
    pub fn new_linear() -> Self {
        BorrowChecker {
            state_map: HashMap::new(),
            errors: Vec::new(),
            use_cfg_analysis: false,
        }
    }

    /// Check a HIR module for borrow violations
    /// Uses CFG-based analysis for functions when enabled
    pub fn check_module(&mut self, module: &HirModule) -> Result<(), Vec<BorrowCheckError>> {
        for func in &module.functions {
            if self.use_cfg_analysis && Self::has_control_flow(func) {
                // Use CFG-based analysis for functions with branches/loops
                self.check_function_cfg(func)?;
            } else {
                // Use linear analysis for simple functions
                self.check_function(func)?;
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Check if a function has non-trivial control flow
    fn has_control_flow(func: &HirFunction) -> bool {
        fn has_cf_stmt(stmt: &HirStmt) -> bool {
            match stmt {
                HirStmt::If { .. } | HirStmt::While { .. } | HirStmt::ForIn { .. } => true,
                HirStmt::Block(stmts) => stmts.iter().any(has_cf_stmt),
                HirStmt::TryCatch { .. } => true,
                _ => false,
            }
        }
        func.body.iter().any(has_cf_stmt)
    }

    /// Check a function using CFG-based analysis
    fn check_function_cfg(&mut self, func: &HirFunction) -> Result<(), Vec<BorrowCheckError>> {
        use super::cfg_borrow::{CfgBorrowChecker, CfgBuilder};

        // Build CFG
        let mut cfg = CfgBuilder::build(func);

        // Run CFG-based analysis
        let mut cfg_checker = CfgBorrowChecker::new();

        match cfg_checker.analyze(&mut cfg) {
            Ok(()) => Ok(()),
            Err(cfg_errors) => {
                // Convert CFG errors to standard BorrowCheckError
                for err in cfg_errors {
                    self.errors.push(err.to_borrow_check_error());
                }
                Ok(()) // Errors collected, don't short-circuit
            }
        }
    }

    fn check_function(&mut self, func: &HirFunction) -> Result<(), Vec<BorrowCheckError>> {
        self.state_map.clear();

        // Initialize parameters as owned
        // Params are (name, type, default_expr)
        for (name, _ty, _default) in &func.params {
            self.state_map.insert(name.clone(), BorrowState::Owned);
        }

        // Check function body (body is Arc<Vec<HirStmt>>)
        for stmt in func.body.iter() {
            self.check_stmt(stmt)?;
        }

        // At function end, all borrowed values must be released
        for (var, state) in &self.state_map {
            if matches!(
                state,
                BorrowState::Borrowed { .. } | BorrowState::MutBorrowed { .. }
            ) {
                eprintln!("warning: {} is still borrowed at function end", var);
            }
        }

        Ok(())
    }

    fn check_stmt(&mut self, stmt: &HirStmt) -> Result<(), Vec<BorrowCheckError>> {
        match stmt {
            HirStmt::Let {
                name,
                init,
                is_borrowed: _,
                ..
            } => {
                if let Some(init_expr) = init {
                    self.check_expr(init_expr)?;
                }
                self.state_map.insert(name.clone(), BorrowState::Owned);
                Ok(())
            }
            HirStmt::LetTuple { names, init, .. } => {
                if let Some(init_expr) = init {
                    self.check_expr(init_expr)?;
                }
                for name in names {
                    self.state_map.insert(name.clone(), BorrowState::Owned);
                }
                Ok(())
            }
            HirStmt::Assign {
                target: _,
                value,
                is_move: _,
            } => {
                // Check RHS doesn't use invalid references
                self.check_expr(value)?;
                Ok(())
            }
            HirStmt::Expr(expr) => self.check_expr(expr),
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.check_expr(cond)?;

                let then_state = self.state_map.clone();
                self.check_stmt(then_branch)?;
                let then_end_state = self.state_map.clone();

                self.state_map = then_state;
                if let Some(else_branch) = else_branch {
                    self.check_stmt(else_branch)?;
                }
                let else_end_state = self.state_map.clone();

                // Merge borrow states from both branches
                self.merge_borrow_states(&then_end_state, &else_end_state)?;
                Ok(())
            }
            HirStmt::While { cond, body } => {
                // Check condition for use of moved/freed variables
                self.check_expr(cond)?;

                // Save state before loop body
                let pre_state = self.state_map.clone();

                // Analyze loop body - borrow states must be valid for all iterations
                self.check_stmt(body)?;

                // After loop body, merge with pre-state:
                // Variables that were moved in the body are conservatively moved
                // (they might not be reinitialized on the first/last iteration)
                let mut updates = Vec::new();
                for (var, post_state) in &self.state_map {
                    if let Some(pre_st) = pre_state.get(var) {
                        // If moved in body but not moved before, keep as moved (conservative)
                        if matches!(post_state, BorrowState::Moved)
                            && !matches!(pre_st, BorrowState::Moved)
                        {
                            // Keep moved state - variable may be used after loop
                        } else if !matches!(post_state, BorrowState::Moved) {
                            // Restore to pre-state if not moved (borrows end at scope exit)
                            updates.push((var.clone(), pre_st.clone()));
                        }
                    }
                }
                for (var, state) in updates {
                    self.state_map.insert(var, state);
                }
                Ok(())
            }
            HirStmt::ForIn { iter, body, .. } => {
                // Check iterator for use of moved/freed variables
                self.check_expr(iter)?;

                // Save state before loop body
                let pre_state = self.state_map.clone();

                // Analyze loop body
                self.check_stmt(body)?;

                // Merge conservatively (same as while loop)
                let mut updates = Vec::new();
                for (var, post_state) in &self.state_map {
                    if let Some(pre_st) = pre_state.get(var) {
                        if matches!(post_state, BorrowState::Moved)
                            && !matches!(pre_st, BorrowState::Moved)
                        {
                            // Keep moved state
                        } else if !matches!(post_state, BorrowState::Moved) {
                            updates.push((var.clone(), pre_st.clone()));
                        }
                    }
                }
                for (var, state) in updates {
                    self.state_map.insert(var, state);
                }
                Ok(())
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                // Save state before try block
                let pre_state = self.state_map.clone();

                // Check try block
                self.check_stmt(try_block)?;

                // For catch block: restore to pre-try state (try may have failed at any point)
                let after_try_state = self.state_map.clone();
                self.state_map = pre_state.clone();
                self.check_stmt(catch_block)?;

                // Merge: if moved in either path, keep moved
                for (var, try_st) in &after_try_state {
                    if matches!(try_st, BorrowState::Moved) {
                        self.state_map.insert(var.clone(), BorrowState::Moved);
                    }
                }
                Ok(())
            }
            HirStmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.check_expr(expr)?;
                }
                Ok(())
            }
            HirStmt::Block(stmts) => {
                for stmt in stmts {
                    self.check_stmt(stmt)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn check_expr(&mut self, expr: &HirExpr) -> Result<(), Vec<BorrowCheckError>> {
        match expr {
            HirExpr::LoadVar(name) => {
                // Check that variable is in valid state
                match self
                    .state_map
                    .get(name)
                    .cloned()
                    .unwrap_or(BorrowState::Owned)
                {
                    BorrowState::Moved | BorrowState::Freed => {
                        self.errors.push(BorrowCheckError::UseAfterFree {
                            variable: name.clone(),
                            freed_at: 0,
                            use_at: 0,
                        });
                    }
                    _ => {}
                }
                Ok(())
            }
            HirExpr::Borrow(inner, is_exclusive) => {
                // Process borrow: check for conflicts and update state
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    self.process_borrow(var_name, *is_exclusive, 0)?;
                }
                self.check_expr(inner)
            }
            HirExpr::BorrowImmut(inner) => {
                // Immutable borrow
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    self.process_borrow(var_name, false, 0)?;
                }
                self.check_expr(inner)
            }
            HirExpr::BorrowMut(inner) => {
                // Mutable borrow
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    self.process_borrow(var_name, true, 0)?;
                }
                self.check_expr(inner)
            }
            HirExpr::Free(target) => {
                // Validate that free doesn't happen while borrowed
                if let HirExpr::LoadVar(var_name) = target.as_ref() {
                    self.validate_free(var_name, 0)?;
                }
                self.check_expr(target)
            }
            HirExpr::Call(func, args, _) => {
                self.check_expr(func)?;
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(())
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.check_expr(left)?;
                self.check_expr(right)?;
                Ok(())
            }
            HirExpr::UnaryOp(_, operand) => {
                self.check_expr(operand)?;
                Ok(())
            }
            HirExpr::MethodCall(object, _method, args) => {
                self.check_expr(object)?;
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(())
            }
            HirExpr::ArrayLiteral(items)
            | HirExpr::SetLiteral(items)
            | HirExpr::TupleLiteral(items) => {
                for item in items {
                    self.check_expr(item)?;
                }
                Ok(())
            }
            HirExpr::DictLiteral(pairs) => {
                for (k, v) in pairs {
                    self.check_expr(k)?;
                    self.check_expr(v)?;
                }
                Ok(())
            }
            HirExpr::ObjectLiteral(fields) => {
                for (_, v) in fields {
                    self.check_expr(v)?;
                }
                Ok(())
            }
            HirExpr::Cast(inner, _) => {
                self.check_expr(inner)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn merge_borrow_states(
        &mut self,
        then_state: &HashMap<String, BorrowState>,
        else_state: &HashMap<String, BorrowState>,
    ) -> Result<(), Vec<BorrowCheckError>> {
        let mut merged = HashMap::new();

        // Collect all variables
        let mut all_vars = then_state.keys().collect::<std::collections::HashSet<_>>();
        all_vars.extend(else_state.keys());

        for var in all_vars {
            let then_st = then_state.get(var).cloned().unwrap_or(BorrowState::Owned);
            let else_st = else_state.get(var).cloned().unwrap_or(BorrowState::Owned);

            if then_st == else_st {
                merged.insert(var.clone(), then_st);
            } else {
                // Borrow state mismatch between branches
                self.errors.push(BorrowCheckError::BorrowStateMismatch {
                    variable: var.clone(),
                    paths: vec![then_st, else_st],
                });
                // Conservative: mark as moved to catch subsequent uses
                merged.insert(var.clone(), BorrowState::Moved);
            }
        }

        self.state_map = merged;
        Ok(())
    }
}

impl Default for BorrowChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl BorrowChecker {
    /// Check if a variable is currently borrowed (shared or exclusive)
    #[allow(dead_code)]
    fn is_borrowed(&self, name: &str) -> bool {
        matches!(
            self.state_map.get(name),
            Some(BorrowState::Borrowed { .. }) | Some(BorrowState::MutBorrowed { .. })
        )
    }

    /// Check if a variable has been moved
    #[allow(dead_code)]
    fn is_moved(&self, name: &str) -> bool {
        matches!(self.state_map.get(name), Some(BorrowState::Moved))
    }

    /// Check if a variable has been freed
    #[allow(dead_code)]
    fn is_freed(&self, name: &str) -> bool {
        matches!(self.state_map.get(name), Some(BorrowState::Freed))
    }

    /// Validate that assignment doesn't violate borrow rules
    #[allow(dead_code)]
    fn validate_assignment(
        &mut self,
        target: &str,
        assign_at: usize,
    ) -> Result<(), Vec<BorrowCheckError>> {
        if let Some(state) = self.state_map.get(target).cloned() {
            match state {
                BorrowState::Borrowed { borrowed_at, .. } => {
                    self.errors.push(BorrowCheckError::MoveWhileBorrowed {
                        variable: target.to_string(),
                        borrowed_at,
                        move_at: assign_at,
                    });
                }
                BorrowState::MutBorrowed { borrowed_at } => {
                    self.errors.push(BorrowCheckError::MoveWhileBorrowed {
                        variable: target.to_string(),
                        borrowed_at,
                        move_at: assign_at,
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Validate function call arguments for borrow conflicts
    #[allow(dead_code)]
    fn validate_call_args(
        &mut self,
        args: &[HirExpr],
        call_at: usize,
    ) -> Result<(), Vec<BorrowCheckError>> {
        let mut borrowed_vars: Vec<(String, bool)> = Vec::new(); // (name, is_exclusive)

        for arg in args {
            if let HirExpr::Borrow(inner, is_exclusive) = arg {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    // Check for conflicting borrows in same call
                    for (existing_var, existing_exclusive) in &borrowed_vars {
                        if existing_var == name {
                            if *is_exclusive || *existing_exclusive {
                                self.errors
                                    .push(BorrowCheckError::MutableBorrowWhileBorrowed {
                                        variable: name.clone(),
                                        previous_borrow: call_at,
                                        mutable_borrow_at: call_at,
                                    });
                            }
                        }
                    }
                    borrowed_vars.push((name.clone(), *is_exclusive));
                }
            }
        }
        Ok(())
    }

    /// Validate that a free operation doesn't happen while borrowed
    fn validate_free(
        &mut self,
        var_name: &str,
        free_at: usize,
    ) -> Result<(), Vec<BorrowCheckError>> {
        if let Some(state) = self.state_map.get(var_name).cloned() {
            match state {
                BorrowState::Borrowed { borrowed_at, .. } => {
                    self.errors.push(BorrowCheckError::FreeWhileBorrowed {
                        variable: var_name.to_string(),
                        borrowed_at,
                        freed_at: free_at,
                    });
                }
                BorrowState::MutBorrowed { borrowed_at } => {
                    self.errors.push(BorrowCheckError::FreeWhileBorrowed {
                        variable: var_name.to_string(),
                        borrowed_at,
                        freed_at: free_at,
                    });
                }
                BorrowState::Freed => {
                    // Double free - already freed
                    self.errors.push(BorrowCheckError::UseAfterFree {
                        variable: var_name.to_string(),
                        freed_at: free_at,
                        use_at: free_at,
                    });
                }
                _ => {
                    // Mark as freed
                    self.state_map
                        .insert(var_name.to_string(), BorrowState::Freed);
                }
            }
        }
        Ok(())
    }

    /// Process a borrow expression, updating state and checking for conflicts
    fn process_borrow(
        &mut self,
        var_name: &str,
        is_exclusive: bool,
        borrow_at: usize,
    ) -> Result<(), Vec<BorrowCheckError>> {
        match self.state_map.get(var_name).cloned() {
            Some(BorrowState::Owned) => {
                if is_exclusive {
                    self.state_map.insert(
                        var_name.to_string(),
                        BorrowState::MutBorrowed {
                            borrowed_at: borrow_at,
                        },
                    );
                } else {
                    self.state_map.insert(
                        var_name.to_string(),
                        BorrowState::Borrowed {
                            borrowed_at: borrow_at,
                            borrow_count: 1,
                        },
                    );
                }
            }
            Some(BorrowState::Borrowed {
                borrowed_at,
                borrow_count,
            }) => {
                if is_exclusive {
                    // Cannot get exclusive access while shared borrow exists
                    self.errors
                        .push(BorrowCheckError::MutableBorrowWhileBorrowed {
                            variable: var_name.to_string(),
                            previous_borrow: borrowed_at,
                            mutable_borrow_at: borrow_at,
                        });
                } else {
                    // Add another shared borrow
                    self.state_map.insert(
                        var_name.to_string(),
                        BorrowState::Borrowed {
                            borrowed_at,
                            borrow_count: borrow_count + 1,
                        },
                    );
                }
            }
            Some(BorrowState::MutBorrowed { borrowed_at }) => {
                // Cannot borrow while exclusive borrow exists
                self.errors
                    .push(BorrowCheckError::MutableBorrowWhileBorrowed {
                        variable: var_name.to_string(),
                        previous_borrow: borrowed_at,
                        mutable_borrow_at: borrow_at,
                    });
            }
            Some(BorrowState::Moved) => {
                self.errors.push(BorrowCheckError::UseAfterFree {
                    variable: var_name.to_string(),
                    freed_at: 0,
                    use_at: borrow_at,
                });
            }
            Some(BorrowState::Freed) => {
                self.errors.push(BorrowCheckError::UseAfterFree {
                    variable: var_name.to_string(),
                    freed_at: 0,
                    use_at: borrow_at,
                });
            }
            None => {
                // Unknown variable - register as owned then borrow
                if is_exclusive {
                    self.state_map.insert(
                        var_name.to_string(),
                        BorrowState::MutBorrowed {
                            borrowed_at: borrow_at,
                        },
                    );
                } else {
                    self.state_map.insert(
                        var_name.to_string(),
                        BorrowState::Borrowed {
                            borrowed_at: borrow_at,
                            borrow_count: 1,
                        },
                    );
                }
            }
        }
        Ok(())
    }

    /// Release all borrows for a variable
    #[allow(dead_code)]
    fn release_borrow(&mut self, var_name: &str) {
        if let Some(state) = self.state_map.get(var_name).cloned() {
            match state {
                BorrowState::Borrowed { borrow_count, .. } if borrow_count > 1 => {
                    self.state_map.insert(
                        var_name.to_string(),
                        BorrowState::Borrowed {
                            borrowed_at: 0,
                            borrow_count: borrow_count - 1,
                        },
                    );
                }
                BorrowState::Borrowed { .. } | BorrowState::MutBorrowed { .. } => {
                    self.state_map
                        .insert(var_name.to_string(), BorrowState::Owned);
                }
                _ => {}
            }
        }
    }

    /// Get all currently borrowed variables
    pub fn get_borrowed_vars(&self) -> Vec<String> {
        self.state_map
            .iter()
            .filter(|(_, state)| {
                matches!(
                    state,
                    BorrowState::Borrowed { .. } | BorrowState::MutBorrowed { .. }
                )
            })
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get all errors
    pub fn get_errors(&self) -> &[BorrowCheckError] {
        &self.errors
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrow_checker_creation() {
        let checker = BorrowChecker::new();
        assert!(checker.errors.is_empty());
    }

    #[test]
    fn test_shared_borrow_allowed() {
        let mut checker = BorrowChecker::new();
        checker
            .state_map
            .insert("x".to_string(), BorrowState::Owned);

        // First shared borrow
        assert!(checker.process_borrow("x", false, 1).is_ok());
        assert!(!checker.has_errors());

        // Second shared borrow should be allowed
        assert!(checker.process_borrow("x", false, 2).is_ok());
        assert!(!checker.has_errors());
    }

    #[test]
    fn test_exclusive_borrow_conflict() {
        let mut checker = BorrowChecker::new();
        checker
            .state_map
            .insert("x".to_string(), BorrowState::Owned);

        // First shared borrow
        assert!(checker.process_borrow("x", false, 1).is_ok());

        // Exclusive borrow should fail
        assert!(checker.process_borrow("x", true, 2).is_ok());
        assert!(checker.has_errors());
    }

    #[test]
    fn test_free_while_borrowed() {
        let mut checker = BorrowChecker::new();
        checker.state_map.insert(
            "x".to_string(),
            BorrowState::Borrowed {
                borrowed_at: 1,
                borrow_count: 1,
            },
        );

        // Free should fail
        assert!(checker.validate_free("x", 2).is_ok());
        assert!(checker.has_errors());
    }
}
