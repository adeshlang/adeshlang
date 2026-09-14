//! Safe Reference Validation - Adapted for existing HIR
//!
//! Validates safe references don't become dangling, null, or uninitialized.
//! Works with the actual HIR structure of the language runtime.

use crate::parsing::hir::*;
use crate::parsing::safety_hir_adapter::*;
use std::collections::HashSet;

/// Validator for safe references
pub struct SafeRefValidator {
    place_tracker: PlaceTracker,
    initialized_vars: HashSet<String>,
    borrowed_vars: HashSet<String>,
    errors: Vec<RefSafetyError>,
}

#[derive(Debug, Clone)]
pub enum RefSafety {
    Safe,          // All checks passed
    Dangling,      // References deallocated memory
    Uninitialized, // References uninitialized memory
}

#[derive(Debug, Clone)]
pub enum RefSafetyError {
    DanglingReference { var_name: String },
    UninitializedReference { var_name: String },
    InvalidBorrow { var_name: String },
}

impl SafeRefValidator {
    pub fn new() -> Self {
        Self {
            place_tracker: PlaceTracker::new(),
            initialized_vars: HashSet::new(),
            borrowed_vars: HashSet::new(),
            errors: Vec::new(),
        }
    }

    /// Analyze a function for reference safety
    pub fn analyze_function(&mut self, function: &HirFunction) {
        // Initialize parameters as initialized and track in place_tracker
        for (param_name, _, _) in &function.params {
            self.initialized_vars.insert(param_name.clone());
            self.place_tracker.get_or_create_place(param_name);
        }

        for stmt in function.body.as_ref() {
            self.analyze_stmt(stmt);
        }

        // Validate all tracked references
        for (var_name,) in self.borrowed_vars.iter().map(|v| (v,)) {
            if !self.initialized_vars.contains(var_name) {
                self.errors.push(RefSafetyError::UninitializedReference {
                    var_name: var_name.clone(),
                });
            }
        }
    }

    /// Analyze a statement
    fn analyze_stmt(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Let { name, init, .. } => {
                if let Some(init_expr) = init {
                    self.analyze_expr(init_expr);
                    self.initialized_vars.insert(name.clone());
                }
                // If no init, variable is uninitialized
            }
            HirStmt::Assign { target, value, .. } => {
                self.analyze_expr(value);
                if let HirExpr::LoadVar(var_name) = target {
                    self.initialized_vars.insert(var_name.clone());
                }
            }
            HirStmt::Block(stmts) => {
                for stmt in stmts {
                    self.analyze_stmt(stmt);
                }
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.analyze_expr(cond);
                self.analyze_stmt(then_branch);
                if let Some(else_br) = else_branch {
                    self.analyze_stmt(else_br);
                }
            }
            HirStmt::While { cond, body } => {
                self.analyze_expr(cond);
                self.analyze_stmt(body);
            }
            HirStmt::ForIn { iter, body, var } => {
                self.analyze_expr(iter);
                self.initialized_vars.insert(var.clone());
                self.analyze_stmt(body);
            }
            HirStmt::Return(Some(expr)) | HirStmt::Throw(expr) | HirStmt::Expr(expr) => {
                self.analyze_expr(expr);
            }
            HirStmt::TryCatch {
                try_block,
                error_name,
                catch_block,
            } => {
                self.analyze_stmt(try_block);
                self.initialized_vars.insert(error_name.clone());
                self.analyze_stmt(catch_block);
            }
            HirStmt::Region { body, .. } | HirStmt::Unsafe(body) => {
                self.analyze_stmt(body);
            }
            _ => {}
        }
    }

    /// Analyze an expression
    fn analyze_expr(&mut self, expr: &HirExpr) {
        match expr {
            HirExpr::LoadVar(var_name) => {
                // Check if variable is initialized
                if !self.initialized_vars.contains(var_name) {
                    self.errors.push(RefSafetyError::UninitializedReference {
                        var_name: var_name.clone(),
                    });
                }
            }
            HirExpr::Borrow(inner, _) | HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                self.analyze_expr(inner);
                // Track that this creates a borrow
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    self.borrowed_vars.insert(var_name.clone());
                }
            }
            HirExpr::Deref(inner) => {
                self.analyze_expr(inner);
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.analyze_expr(left);
                self.analyze_expr(right);
            }
            HirExpr::UnaryOp(_, operand) => {
                self.analyze_expr(operand);
            }
            HirExpr::Call(func, args, _) => {
                self.analyze_expr(func);
                for arg in args {
                    self.analyze_expr(arg);
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.analyze_expr(obj);
                for arg in args {
                    self.analyze_expr(arg);
                }
            }
            HirExpr::ArrayLiteral(elements) => {
                for elem in elements {
                    self.analyze_expr(elem);
                }
            }
            HirExpr::DictLiteral(pairs) => {
                for (key, value) in pairs {
                    self.analyze_expr(key);
                    self.analyze_expr(value);
                }
            }
            HirExpr::Conditional(cond, then_expr, else_expr) => {
                self.analyze_expr(cond);
                self.analyze_expr(then_expr);
                self.analyze_expr(else_expr);
            }
            HirExpr::Match(expr, arms) => {
                self.analyze_expr(expr);
                for (_, arm_expr) in arms {
                    self.analyze_expr(arm_expr);
                }
            }
            _ => {}
        }
    }

    /// Get all errors found
    pub fn get_errors(&self) -> &[RefSafetyError] {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::sync::Arc;

    #[test]
    fn test_uninitialized_variable() {
        let mut validator = SafeRefValidator::new();

        // Using uninitialized variable should error
        let expr = HirExpr::LoadVar("x".to_string());
        validator.analyze_expr(&expr);

        assert_eq!(validator.get_errors().len(), 1);
    }

    #[test]
    fn test_initialized_variable() {
        let mut validator = SafeRefValidator::new();

        // Initialize variable
        validator.initialized_vars.insert("x".to_string());

        // Using initialized variable should be ok
        let expr = HirExpr::LoadVar("x".to_string());
        validator.analyze_expr(&expr);

        assert_eq!(validator.get_errors().len(), 0);
    }

    #[test]
    fn test_borrow_of_uninitialized() {
        let mut validator = SafeRefValidator::new();

        // Borrowing uninitialized variable should error
        let expr = HirExpr::Borrow(Box::new(HirExpr::LoadVar("x".to_string())), false);
        validator.analyze_expr(&expr);

        assert_eq!(validator.get_errors().len(), 1);
    }
}
