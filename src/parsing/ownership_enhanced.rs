//! Enhanced Ownership Tracking - Adapted for existing HIR
//!
//! Provides improved move tracking using the existing HIR and CFG infrastructure.
//! Works with the actual AdeshLang structures.

use crate::parsing::hir::*;
use crate::parsing::safety_hir_adapter::*;
use std::collections::HashSet;

/// Enhanced move tracker
pub struct CfgMoveTracker {
    place_tracker: PlaceTracker,
    moved_vars: HashSet<String>,
    errors: Vec<OwnershipError>,
}

#[derive(Debug, Clone)]
pub enum OwnershipError {
    UseAfterMove { var_name: String },
    DoubleMove { var_name: String },
}

impl CfgMoveTracker {
    pub fn new() -> Self {
        Self {
            place_tracker: PlaceTracker::new(),
            moved_vars: HashSet::new(),
            errors: Vec::new(),
        }
    }

    /// Analyze a function for moves
    pub fn analyze_function(&mut self, function: &HirFunction) {
        for stmt in function.body.as_ref() {
            self.analyze_stmt(stmt);
        }
    }

    /// Analyze a statement for moves
    fn analyze_stmt(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Assign {
                target: _,
                value,
                is_move,
            } => {
                if *is_move {
                    // Track moves in the value expression
                    let mut moves = Vec::new();
                    extract_moves_from_expr(value, &mut self.place_tracker, &mut moves);

                    for place in moves {
                        if let Some(var_name) = self.place_tracker.get_var_name(place) {
                            if self.moved_vars.contains(var_name) {
                                self.errors.push(OwnershipError::UseAfterMove {
                                    var_name: var_name.to_string(),
                                });
                            } else {
                                self.moved_vars.insert(var_name.to_string());
                            }
                        }
                    }
                }
            }
            HirStmt::Let {
                init: Some(init), ..
            } => {
                let mut moves = Vec::new();
                extract_moves_from_expr(init, &mut self.place_tracker, &mut moves);

                for place in moves {
                    if let Some(var_name) = self.place_tracker.get_var_name(place) {
                        if self.moved_vars.contains(var_name) {
                            self.errors.push(OwnershipError::UseAfterMove {
                                var_name: var_name.to_string(),
                            });
                        } else {
                            self.moved_vars.insert(var_name.to_string());
                        }
                    }
                }
            }
            HirStmt::Block(stmts) => {
                for stmt in stmts {
                    self.analyze_stmt(stmt);
                }
            }
            HirStmt::If {
                cond: _,
                then_branch,
                else_branch,
            } => {
                // Save state before branches
                let saved_moved = self.moved_vars.clone();

                // Analyze then branch
                self.analyze_stmt(then_branch);
                let then_moved = self.moved_vars.clone();

                // Restore and analyze else branch
                self.moved_vars = saved_moved.clone();
                if let Some(else_br) = else_branch {
                    self.analyze_stmt(else_br);
                }
                let else_moved = self.moved_vars.clone();

                // Merge: only keep definitely moved (in both branches)
                if else_branch.is_some() {
                    self.moved_vars = then_moved.intersection(&else_moved).cloned().collect();
                } else {
                    // If no else, restore original state
                    self.moved_vars = saved_moved;
                }
            }
            HirStmt::While { body, .. } => {
                // Conservative: don't track moves in loops
                self.analyze_stmt(body);
            }
            HirStmt::ForIn { body, .. } => {
                self.analyze_stmt(body);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.analyze_stmt(try_block);
                self.analyze_stmt(catch_block);
            }
            HirStmt::Region { body, .. } | HirStmt::Unsafe(body) => {
                self.analyze_stmt(body);
            }
            _ => {}
        }
    }

    /// Get all errors found
    pub fn get_errors(&self) -> &[OwnershipError] {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::sync::Arc;

    #[test]
    fn test_simple_move() {
        let mut tracker = CfgMoveTracker::new();

        // Create a simple assignment that moves
        let assign_stmt = HirStmt::Assign {
            target: HirExpr::LoadVar("x".to_string()),
            value: HirExpr::LoadVar("y".to_string()),
            is_move: true,
        };

        tracker.analyze_stmt(&assign_stmt);

        // y should be moved
        assert!(tracker.moved_vars.contains("y"));
    }

    #[test]
    fn test_use_after_move() {
        let mut tracker = CfgMoveTracker::new();

        // First move
        let assign1 = HirStmt::Assign {
            target: HirExpr::LoadVar("x".to_string()),
            value: HirExpr::LoadVar("y".to_string()),
            is_move: true,
        };
        tracker.analyze_stmt(&assign1);

        // Second move of same variable (should error)
        let assign2 = HirStmt::Assign {
            target: HirExpr::LoadVar("z".to_string()),
            value: HirExpr::LoadVar("y".to_string()),
            is_move: true,
        };
        tracker.analyze_stmt(&assign2);

        assert_eq!(tracker.get_errors().len(), 1);
    }

    #[test]
    fn test_conditional_move() {
        let mut tracker = CfgMoveTracker::new();

        // Move in if branch only
        let if_stmt = HirStmt::If {
            cond: HirExpr::Literal(HirLiteral::Bool(true)),
            then_branch: Box::new(HirStmt::Assign {
                target: HirExpr::LoadVar("x".to_string()),
                value: HirExpr::LoadVar("y".to_string()),
                is_move: true,
            }),
            else_branch: None,
        };

        tracker.analyze_stmt(&if_stmt);

        // y should NOT be definitely moved (no else branch)
        assert!(!tracker.moved_vars.contains("y"));
    }
}
