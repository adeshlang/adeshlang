//! Borrow checking and reference validation
//!
//! This module implements borrow checking rules:
//! - Multiple immutable borrows OR single mutable borrow
//! - No aliasing mutable references
//! - Borrow scope validation
//! - Free-while-borrowed prevention

use super::error::{BorrowLocation, CompileTimeMemoryError};
use super::state::{BorrowInfo, OwnershipState};
use crate::parsing::hir::{HirExpr, HirFunction, HirModule, HirStmt, HirType};
use std::collections::HashMap;

impl super::CompileTimeMemorySafety {
    /// Validate borrowing rules across module
    pub(super) fn validate_borrowing(
        &mut self,
        module: &HirModule,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        for func in &module.functions {
            self.check_function_borrowing(func)?;
        }
        Ok(())
    }

    /// Check function body for borrow violations
    pub(super) fn check_function_borrowing(
        &mut self,
        func: &HirFunction,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        // Set function context for better error messages
        self.set_current_function(Some(func.name.clone()));

        // Track active borrows per variable
        let mut active_borrows: HashMap<String, Vec<BorrowInfo>> = HashMap::new();

        for (idx, stmt) in func.body.iter().enumerate() {
            self.set_current_line(idx + 1);
            self.check_stmt_borrowing(stmt, &mut active_borrows)?;
        }

        self.set_current_function(None);
        Ok(())
    }

    /// Check statement for borrow violations
    pub(super) fn check_stmt_borrowing(
        &mut self,
        stmt: &HirStmt,
        active_borrows: &mut HashMap<String, Vec<BorrowInfo>>,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match stmt {
            HirStmt::Expr(expr) => {
                self.check_expr_borrowing(expr, active_borrows)?;
            }

            HirStmt::Let {
                init: Some(expr), ..
            } => {
                self.check_expr_borrowing(expr, active_borrows)?;
            }

            HirStmt::Assign { value, .. } => {
                self.check_expr_borrowing(value, active_borrows)?;
            }

            HirStmt::Block(stmts) => {
                let saved_borrows = active_borrows.clone();
                for s in stmts {
                    self.check_stmt_borrowing(s, active_borrows)?;
                }
                *active_borrows = saved_borrows;
            }

            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.check_expr_borrowing(cond, active_borrows)?;
                let mut then_borrows = active_borrows.clone();
                self.check_stmt_borrowing(then_branch, &mut then_borrows)?;
                if let Some(else_stmt) = else_branch {
                    let mut else_borrows = active_borrows.clone();
                    self.check_stmt_borrowing(else_stmt, &mut else_borrows)?;
                }
            }

            HirStmt::While { cond, body } => {
                self.check_expr_borrowing(cond, active_borrows)?;
                self.check_stmt_borrowing(body, active_borrows)?;
            }

            HirStmt::ForIn { iter, body, .. } => {
                self.check_expr_borrowing(iter, active_borrows)?;
                self.check_stmt_borrowing(body, active_borrows)?;
            }

            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                // Check try block
                let saved_borrows = active_borrows.clone();
                self.check_stmt_borrowing(try_block, active_borrows)?;
                // Restore borrows for catch block (try may have failed before modifying)
                *active_borrows = saved_borrows;
                self.check_stmt_borrowing(catch_block, active_borrows)?;
            }

            HirStmt::Return(Some(expr)) => {
                self.check_expr_borrowing(expr, active_borrows)?;
            }

            _ => {}
        }

        Ok(())
    }

    /// Recursively check expressions for borrowing rules (including method calls)
    pub(super) fn check_expr_borrowing(
        &mut self,
        expr: &HirExpr,
        active_borrows: &mut HashMap<String, Vec<BorrowInfo>>,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match expr {
            HirExpr::Borrow(target_box, is_mut) => {
                if let HirExpr::LoadVar(var_name) = &**target_box {
                    // Check for borrow conflicts
                    if let Some(borrows) = active_borrows.get(var_name) {
                        for existing_borrow in borrows {
                            if *is_mut || existing_borrow.is_mutable {
                                // Conflict
                                self.errors.push(CompileTimeMemoryError::BorrowConflict {
                                    variable: var_name.clone(),
                                    existing_borrow: BorrowLocation {
                                        location: existing_borrow.location.clone(),
                                        is_mutable: existing_borrow.is_mutable,
                                        borrow_count: 1,
                                    },
                                    conflicting_borrow: BorrowLocation {
                                        location: self.get_source_location(0),
                                        is_mutable: *is_mut,
                                        borrow_count: 1,
                                    },
                                    suggestion: if *is_mut {
                                        "mutable borrows cannot coexist with other borrows"
                                            .to_string()
                                    } else {
                                        "cannot borrow immutably while mutable borrow exists"
                                            .to_string()
                                    },
                                });
                            }
                        }
                    }

                    // Add new borrow
                    active_borrows
                        .entry(var_name.clone())
                        .or_insert_with(Vec::new)
                        .push(BorrowInfo {
                            borrower: var_name.clone(),
                            is_mutable: *is_mut,
                            scope_id: 0,
                            location: self.get_source_location(0),
                        });
                }
            }

            HirExpr::Free(target) => {
                if let HirExpr::LoadVar(var_name) = &**target {
                    // Check if variable is borrowed
                    if let Some(borrows) = active_borrows.get(var_name) {
                        if !borrows.is_empty() {
                            self.errors.push(CompileTimeMemoryError::FreeWhileBorrowed {
                                variable: var_name.clone(),
                                borrowed_at: borrows.iter().map(|b| b.location.clone()).collect(),
                                freed_at: self.get_source_location(0),
                            });
                        }
                    }

                    // Check for double free
                    if let Some(node) = self.ownership_graph.get(var_name) {
                        if node.state == OwnershipState::Freed {
                            self.errors.push(CompileTimeMemoryError::DoubleFree {
                                variable: var_name.clone(),
                                first_free: node.location.clone(),
                                second_free: self.get_source_location(0),
                            });
                        }
                    }
                }
            }

            HirExpr::MethodCall(obj, method_name, args) => {
                // Check arguments
                for arg in args {
                    self.check_expr_borrowing(arg, active_borrows)?;
                }

                // Check object receiver "this" borrowing rules
                if let HirExpr::LoadVar(var_name) = &**obj {
                    if let Some(class_type) = self.var_types.get(var_name) {
                        if let HirType::Class(class_name) = class_type {
                            if let Some(class) = self.class_registry.get(class_name) {
                                // Find method
                                let method = class
                                    .methods
                                    .iter()
                                    .find(|m| &m.name == method_name)
                                    .or_else(|| {
                                        class.static_methods.iter().find(|m| &m.name == method_name)
                                    });

                                if let Some(m) = method {
                                    // Check "this" parameter
                                    if let Some((_, Some(ty))) =
                                        m.params.iter().find(|(pname, _)| pname == "this")
                                    {
                                        match ty {
                                            HirType::Borrow(_, true) => {
                                                // mut ref: requires mutable access
                                                if let Some(borrows) = active_borrows.get(var_name)
                                                {
                                                    for b in borrows {
                                                        self.errors.push(CompileTimeMemoryError::BorrowConflict {
                                                                variable: var_name.clone(),
                                                                existing_borrow: BorrowLocation {
                                                                    location: b.location.clone(),
                                                                    is_mutable: b.is_mutable,
                                                                    borrow_count: 1,
                                                                },
                                                                conflicting_borrow: BorrowLocation {
                                                                    location: self.get_source_location(0),
                                                                    is_mutable: true,
                                                                    borrow_count: 1,
                                                                },
                                                                suggestion: format!("method '{}' requires mutable access to '{}', but it is already borrowed", method_name, var_name),
                                                            });
                                                    }
                                                }
                                            }
                                            HirType::Borrow(_, false) => {
                                                // ref: requires immutable access
                                                if let Some(borrows) = active_borrows.get(var_name)
                                                {
                                                    for b in borrows {
                                                        if b.is_mutable {
                                                            self.errors.push(CompileTimeMemoryError::BorrowConflict {
                                                                variable: var_name.clone(),
                                                                existing_borrow: BorrowLocation {
                                                                    location: b.location.clone(),
                                                                    is_mutable: true,
                                                                    borrow_count: 1,
                                                                },
                                                                conflicting_borrow: BorrowLocation {
                                                                    location: self.get_source_location(0),
                                                                    is_mutable: false,
                                                                    borrow_count: 1,
                                                                },
                                                                suggestion: format!("method '{}' requires shared access to '{}', but it is mutably borrowed", method_name, var_name),
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {} // By value or other
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                self.check_expr_borrowing(obj, active_borrows)?;
            }

            HirExpr::Call(func, args, _) => {
                self.check_expr_borrowing(func, active_borrows)?;
                for arg in args {
                    self.check_expr_borrowing(arg, active_borrows)?;
                }
            }

            HirExpr::BinaryOp(left, _, right) => {
                self.check_expr_borrowing(left, active_borrows)?;
                self.check_expr_borrowing(right, active_borrows)?;
            }

            // ... Handle other expressions recursively ...
            _ => {}
        }
        Ok(())
    }
}
