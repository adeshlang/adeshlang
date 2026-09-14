//! Utility functions for memory safety checking
//!
//! This module provides:
//! - CFG-based borrow checker integration
//! - Source location tracking
//! - Helper functions for error conversion

use super::error::{BorrowLocation, CompileTimeMemoryError, SourceLocation};
use crate::parsing::cfg_borrow::{CfgBorrowChecker, CfgBuilder};
use crate::parsing::hir::HirModule;

impl super::CompileTimeMemorySafety {
    /// Run CFG-based borrow checker for control flow analysis
    pub(super) fn run_cfg_borrow_checker(
        &mut self,
        module: &HirModule,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        for func in &module.functions {
            self.set_current_function(Some(func.name.clone()));

            let mut cfg = CfgBuilder::build(func);
            let mut checker = CfgBorrowChecker::new();

            if let Err(cfg_errors) = checker.analyze(&mut cfg) {
                // Convert CFG errors to CompileTimeMemoryErrors
                for err in cfg_errors {
                    let converted_error = self.convert_cfg_error(&err);
                    self.errors.push(converted_error);
                }
            }

            self.set_current_function(None);
        }

        Ok(())
    }

    /// Convert a CFG borrow error to our CompileTimeMemoryError type
    pub(super) fn convert_cfg_error(
        &self,
        err: &crate::parsing::cfg_borrow::CfgBorrowError,
    ) -> CompileTimeMemoryError {
        use crate::parsing::cfg_borrow::CfgBorrowError;

        match err {
            CfgBorrowError::UseAfterMaybeMoved {
                variable,
                moved_in_branch,
                use_location,
            } => CompileTimeMemoryError::UseAfterMove {
                variable: variable.clone(),
                moved_at: SourceLocation {
                    file: self.current_file.clone(),
                    line: moved_in_branch.span.start,
                    column: 1,
                    context: moved_in_branch.description.clone(),
                },
                used_at: SourceLocation {
                    file: self.current_file.clone(),
                    line: use_location.start,
                    column: 1,
                    context: self.current_function.clone().unwrap_or_default(),
                },
                suggestion: crate::parsing::error::ownership_help::branch_move(variable),
            },
            CfgBorrowError::UseAfterMaybeFreed {
                variable,
                freed_in_branch,
                use_location,
            } => CompileTimeMemoryError::UseAfterFree {
                variable: variable.clone(),
                freed_at: SourceLocation {
                    file: self.current_file.clone(),
                    line: freed_in_branch.span.start,
                    column: 1,
                    context: freed_in_branch.description.clone(),
                },
                used_at: SourceLocation {
                    file: self.current_file.clone(),
                    line: use_location.start,
                    column: 1,
                    context: self.current_function.clone().unwrap_or_default(),
                },
            },
            CfgBorrowError::BorrowConflict {
                variable,
                existing_borrow,
                new_borrow,
                existing_is_exclusive,
                new_is_exclusive,
            } => CompileTimeMemoryError::BorrowConflict {
                variable: variable.clone(),
                existing_borrow: BorrowLocation {
                    location: SourceLocation {
                        file: self.current_file.clone(),
                        line: existing_borrow.span.start,
                        column: 1,
                        context: existing_borrow.description.clone(),
                    },
                    is_mutable: *existing_is_exclusive,
                    borrow_count: 1,
                },
                conflicting_borrow: BorrowLocation {
                    location: SourceLocation {
                        file: self.current_file.clone(),
                        line: new_borrow.start,
                        column: 1,
                        context: self.current_function.clone().unwrap_or_default(),
                    },
                    is_mutable: *new_is_exclusive,
                    borrow_count: 1,
                },
                suggestion: if *new_is_exclusive {
                    "cannot mutably borrow while already borrowed; limit the scope of the existing borrow".to_string()
                } else {
                    "cannot borrow while mutably borrowed; consider restructuring".to_string()
                },
            },
            CfgBorrowError::FreeWhileBorrowed {
                variable,
                borrowed_at,
                freed_at,
            } => CompileTimeMemoryError::FreeWhileBorrowed {
                variable: variable.clone(),
                borrowed_at: vec![SourceLocation {
                    file: self.current_file.clone(),
                    line: borrowed_at.start,
                    column: 1,
                    context: "borrow location".to_string(),
                }],
                freed_at: SourceLocation {
                    file: self.current_file.clone(),
                    line: freed_at.start,
                    column: 1,
                    context: self.current_function.clone().unwrap_or_default(),
                },
            },
            CfgBorrowError::MoveWhileBorrowed {
                variable,
                borrowed_at,
                move_at,
            } => CompileTimeMemoryError::UseAfterMove {
                variable: variable.clone(),
                moved_at: SourceLocation {
                    file: self.current_file.clone(),
                    line: move_at.start,
                    column: 1,
                    context: "move location".to_string(),
                },
                used_at: SourceLocation {
                    file: self.current_file.clone(),
                    line: borrowed_at.start,
                    column: 1,
                    context: "still borrowed here".to_string(),
                },
                suggestion:
                    "cannot move value while it is borrowed; drop or limit the borrow first"
                        .to_string(),
            },
            CfgBorrowError::MergeError(merge_err) => CompileTimeMemoryError::BorrowConflict {
                variable: merge_err.variable.clone(),
                existing_borrow: BorrowLocation {
                    location: SourceLocation {
                        file: self.current_file.clone(),
                        line: merge_err.join_location.start,
                        column: 1,
                        context: "conflicting borrow states at join point".to_string(),
                    },
                    is_mutable: false,
                    borrow_count: 1,
                },
                conflicting_borrow: BorrowLocation {
                    location: SourceLocation {
                        file: self.current_file.clone(),
                        line: merge_err.join_location.start,
                        column: 1,
                        context: merge_err.message(),
                    },
                    is_mutable: false,
                    borrow_count: 1,
                },
                suggestion: "borrow states must be consistent across all control flow paths"
                    .to_string(),
            },
            CfgBorrowError::FixpointNotReached {
                function_name,
                iterations,
            } => {
                // Internal error - shouldn't happen in normal usage
                self.warnings.iter().for_each(|_| {});
                CompileTimeMemoryError::BorrowConflict {
                    variable: format!("internal::{}", function_name),
                    existing_borrow: BorrowLocation {
                        location: self.get_source_location(0),
                        is_mutable: false,
                        borrow_count: 1,
                    },
                    conflicting_borrow: BorrowLocation {
                        location: self.get_source_location(0),
                        is_mutable: false,
                        borrow_count: 1,
                    },
                    suggestion: format!(
                        "internal error: fixpoint not reached after {} iterations",
                        iterations
                    ),
                }
            }
        }
    }

    /// Get source location using current tracking state
    pub(super) fn get_source_location(&self, stmt_index: usize) -> SourceLocation {
        let line = self
            .stmt_line_map
            .get(&stmt_index)
            .copied()
            .unwrap_or(self.current_line);

        let context = if let Some(ref func) = self.current_function {
            format!("in function '{}'", func)
        } else {
            String::new()
        };

        SourceLocation {
            file: self.current_file.clone(),
            line,
            column: 1, // Column tracking would require token-level info
            context,
        }
    }

    /// Set current file being analyzed
    pub fn set_current_file(&mut self, file: &str) {
        self.current_file = file.to_string();
    }

    /// Set current line number
    pub(super) fn set_current_line(&mut self, line: usize) {
        self.current_line = line;
    }

    /// Set current function context
    pub(super) fn set_current_function(&mut self, func: Option<String>) {
        self.current_function = func;
    }

    /// Map statement index to source line
    pub(super) fn map_stmt_line(&mut self, stmt_index: usize, line: usize) {
        self.stmt_line_map.insert(stmt_index, line);
    }

    /// Get all errors found during analysis
    pub fn get_errors(&self) -> &[CompileTimeMemoryError] {
        &self.errors
    }

    /// Get all warnings found during analysis
    pub fn get_warnings(&self) -> &[String] {
        &self.warnings
    }
}
