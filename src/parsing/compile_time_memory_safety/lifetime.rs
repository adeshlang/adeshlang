//! Lifetime analysis and validation
//!
//! This module ensures that references don't outlive their referents:
//! - Return value lifetime validation
//! - Local variable lifetime tracking
//! - Closure capture lifetime checking

use super::error::CompileTimeMemoryError;
use crate::parsing::hir::{HirExpr, HirFunction, HirModule, HirStmt, HirType};
use crate::utils::collections::FastSet;

impl super::CompileTimeMemorySafety {
    /// Check lifetime validity across module
    pub(super) fn check_lifetimes(
        &mut self,
        module: &HirModule,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        // Check each function for lifetime violations
        for func in &module.functions {
            self.check_function_lifetimes(func)?;
        }

        // Check class methods
        for class in &module.classes {
            for method in &class.methods {
                self.check_method_lifetimes(method)?;
            }
        }

        Ok(())
    }

    /// Check function for lifetime violations
    pub(super) fn check_function_lifetimes(
        &mut self,
        func: &HirFunction,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        // Track which local variables are borrowed and returned
        let mut local_vars: FastSet<String> = FastSet::default();
        let mut borrowed_returns: Vec<(String, String)> = Vec::new(); // (ref_name, source_var)

        // Collect all local variables declared in the function
        for stmt in func.body.iter() {
            self.collect_local_vars(stmt, &mut local_vars);
        }

        // Check for returning references to local variables
        for stmt in func.body.iter() {
            self.check_stmt_lifetimes(stmt, &local_vars, &mut borrowed_returns)?;
        }

        // Report any lifetime violations where local refs are returned
        for (ref_name, source_var) in borrowed_returns {
            if local_vars.contains(&source_var) {
                self.errors.push(CompileTimeMemoryError::LifetimeViolation {
                    reference: ref_name,
                    owner: source_var.clone(),
                    return_location: self.get_source_location(0),
                    owner_scope_ends: self.get_source_location(0),
                });
            }
        }

        Ok(())
    }

    /// Check method for lifetime violations
    pub(super) fn check_method_lifetimes(
        &mut self,
        method: &crate::parsing::hir::HirMethod,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        let mut local_vars: FastSet<String> = FastSet::default();
        let mut borrowed_returns: Vec<(String, String)> = Vec::new();

        for stmt in method.body.iter() {
            self.collect_local_vars(stmt, &mut local_vars);
        }

        for stmt in method.body.iter() {
            self.check_stmt_lifetimes(stmt, &local_vars, &mut borrowed_returns)?;
        }

        for (ref_name, source_var) in borrowed_returns {
            if local_vars.contains(&source_var) {
                self.errors.push(CompileTimeMemoryError::LifetimeViolation {
                    reference: ref_name,
                    owner: source_var.clone(),
                    return_location: self.get_source_location(0),
                    owner_scope_ends: self.get_source_location(0),
                });
            }
        }

        Ok(())
    }

    /// Collect local variable names from statements
    pub(super) fn collect_local_vars(&self, stmt: &HirStmt, local_vars: &mut FastSet<String>) {
        match stmt {
            HirStmt::Let { name, .. } => {
                local_vars.insert(name.clone());
            }
            HirStmt::LetTuple { names, .. } => {
                for name in names {
                    local_vars.insert(name.clone());
                }
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_local_vars(s, local_vars);
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_local_vars(then_branch, local_vars);
                if let Some(else_stmt) = else_branch {
                    self.collect_local_vars(else_stmt, local_vars);
                }
            }
            HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
                self.collect_local_vars(body, local_vars);
            }
            _ => {}
        }
    }

    /// Check statement for lifetime violations
    pub(super) fn check_stmt_lifetimes(
        &mut self,
        stmt: &HirStmt,
        local_vars: &FastSet<String>,
        borrowed_returns: &mut Vec<(String, String)>,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match stmt {
            HirStmt::Return(Some(expr)) => {
                // Check if returning a borrow of a local variable
                if let Some((ref_name, source_var)) = self.extract_borrow_source(expr) {
                    if local_vars.contains(&source_var) {
                        borrowed_returns.push((ref_name, source_var));
                    }
                }
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.check_stmt_lifetimes(s, local_vars, borrowed_returns)?;
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.check_stmt_lifetimes(then_branch, local_vars, borrowed_returns)?;
                if let Some(else_stmt) = else_branch {
                    self.check_stmt_lifetimes(else_stmt, local_vars, borrowed_returns)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Extract borrow source from an expression (returns (ref_name, source_var) if applicable)
    pub(super) fn extract_borrow_source(&self, expr: &HirExpr) -> Option<(String, String)> {
        match expr {
            HirExpr::Borrow(inner, _) | HirExpr::BorrowMut(inner) | HirExpr::BorrowImmut(inner) => {
                if let HirExpr::LoadVar(var_name) = &**inner {
                    Some((var_name.clone(), var_name.clone()))
                } else {
                    None
                }
            }
            HirExpr::LoadVar(var_name) => {
                // Check if this variable itself is a borrow type
                if let Some(ty) = self.var_types.get(var_name) {
                    if matches!(
                        ty,
                        HirType::Borrow(_, _) | HirType::BorrowMut(_) | HirType::BorrowImmut(_)
                    ) {
                        Some((var_name.clone(), var_name.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
