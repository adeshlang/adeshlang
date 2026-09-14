//! Unsafe Pointer Tracking - Adapted for existing HIR
//!
//! Tracks raw pointers and enforces unsafe block requirements.
//! Works with the actual HIR structure in AdeshLang.

use crate::parsing::hir::*;
use crate::parsing::safety_hir_adapter::*;
use std::collections::HashMap;

/// Tracker for unsafe raw pointers
pub struct UnsafePointerTracker {
    place_tracker: PlaceTracker,
    raw_pointers: HashMap<PlaceId, RawPointerInfo>,
    errors: Vec<UnsafePointerError>,
}

#[derive(Debug, Clone)]
pub struct RawPointerInfo {
    pub var_name: String,
    pub used_in_safe_context: bool,
}

#[derive(Debug, Clone)]
pub enum UnsafePointerError {
    RawPointerOutsideUnsafe { var_name: String, operation: String },
    EscapeToSafeContext { var_name: String },
}

impl UnsafePointerTracker {
    pub fn new() -> Self {
        Self {
            place_tracker: PlaceTracker::new(),
            raw_pointers: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Analyze a function for unsafe pointer usage
    pub fn analyze_function(&mut self, function: &HirFunction) {
        for stmt in function.body.as_ref() {
            self.analyze_stmt(stmt, false);
        }
        // Track all discovered raw pointers in place_tracker
        for (place_id, ptr_info) in &self.raw_pointers {
            if ptr_info.used_in_safe_context {
                if let Some(var_name) = self.place_tracker.get_var_name(*place_id) {
                    self.errors.push(UnsafePointerError::EscapeToSafeContext {
                        var_name: var_name.to_string(),
                    });
                }
            }
        }
    }

    /// Analyze a statement for unsafe pointer usage
    fn analyze_stmt(&mut self, stmt: &HirStmt, in_unsafe: bool) {
        match stmt {
            HirStmt::Unsafe(body) => {
                // Inside unsafe block
                self.analyze_stmt(body, true);
            }
            HirStmt::Block(stmts) => {
                for stmt in stmts {
                    self.analyze_stmt(stmt, in_unsafe);
                }
            }
            HirStmt::Expr(expr) => {
                self.analyze_expr(expr, in_unsafe);
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.analyze_expr(cond, in_unsafe);
                self.analyze_stmt(then_branch, in_unsafe);
                if let Some(else_br) = else_branch {
                    self.analyze_stmt(else_br, in_unsafe);
                }
            }
            HirStmt::While { cond, body } => {
                self.analyze_expr(cond, in_unsafe);
                self.analyze_stmt(body, in_unsafe);
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.analyze_expr(iter, in_unsafe);
                self.analyze_stmt(body, in_unsafe);
            }
            HirStmt::Return(Some(expr)) | HirStmt::Throw(expr) => {
                self.analyze_expr(expr, in_unsafe);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.analyze_stmt(try_block, in_unsafe);
                self.analyze_stmt(catch_block, in_unsafe);
            }
            HirStmt::Region { body, .. } => {
                self.analyze_stmt(body, in_unsafe);
            }
            HirStmt::Let {
                init: Some(init), ..
            } => {
                self.analyze_expr(init, in_unsafe);
            }
            HirStmt::Assign { value, .. } => {
                self.analyze_expr(value, in_unsafe);
            }
            _ => {}
        }
    }

    /// Analyze an expression for unsafe pointer usage
    fn analyze_expr(&mut self, expr: &HirExpr, in_unsafe: bool) {
        match expr {
            HirExpr::Alloc(_, size) => {
                if !in_unsafe {
                    self.errors
                        .push(UnsafePointerError::RawPointerOutsideUnsafe {
                            var_name: "alloc".to_string(),
                            operation: "allocation".to_string(),
                        });
                }
                self.analyze_expr(size, in_unsafe);
            }
            HirExpr::Free(ptr) => {
                if !in_unsafe {
                    self.errors
                        .push(UnsafePointerError::RawPointerOutsideUnsafe {
                            var_name: "free".to_string(),
                            operation: "deallocation".to_string(),
                        });
                }
                self.analyze_expr(ptr, in_unsafe);
            }
            HirExpr::Deref(inner) => {
                // Dereferencing raw pointers requires unsafe
                if !in_unsafe {
                    // Check if the inner expression is a raw pointer deref
                    // (LoadVar pointing to a pointer type, or result of alloc)
                    let is_raw_ptr = match inner.as_ref() {
                        HirExpr::LoadVar(_) => true, // Conservative: assume any deref of a var is raw
                        HirExpr::Alloc(_, _) => true,
                        HirExpr::Free(_) => true,
                        _ => false,
                    };
                    if is_raw_ptr {
                        self.errors
                            .push(UnsafePointerError::RawPointerOutsideUnsafe {
                                var_name: "deref".to_string(),
                                operation: "pointer dereference".to_string(),
                            });
                    }
                }
                self.analyze_expr(inner, in_unsafe);
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.analyze_expr(left, in_unsafe);
                self.analyze_expr(right, in_unsafe);
            }
            HirExpr::UnaryOp(_, operand) => {
                self.analyze_expr(operand, in_unsafe);
            }
            HirExpr::Call(func, args, _) => {
                self.analyze_expr(func, in_unsafe);
                for arg in args {
                    self.analyze_expr(arg, in_unsafe);
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.analyze_expr(obj, in_unsafe);
                for arg in args {
                    self.analyze_expr(arg, in_unsafe);
                }
            }
            HirExpr::Conditional(cond, then_expr, else_expr) => {
                self.analyze_expr(cond, in_unsafe);
                self.analyze_expr(then_expr, in_unsafe);
                self.analyze_expr(else_expr, in_unsafe);
            }
            HirExpr::ArrayLiteral(elements) => {
                for elem in elements {
                    self.analyze_expr(elem, in_unsafe);
                }
            }
            HirExpr::Match(expr, arms) => {
                self.analyze_expr(expr, in_unsafe);
                for (_, arm_expr) in arms {
                    self.analyze_expr(arm_expr, in_unsafe);
                }
            }
            _ => {}
        }
    }

    /// Get all errors found
    pub fn get_errors(&self) -> &[UnsafePointerError] {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_requires_unsafe() {
        let mut tracker = UnsafePointerTracker::new();

        // Alloc outside unsafe should error
        let alloc_expr = HirExpr::Alloc(
            Box::new(HirType::U8),
            Box::new(HirExpr::Literal(HirLiteral::Int(128))),
        );
        tracker.analyze_expr(&alloc_expr, false);

        assert_eq!(tracker.get_errors().len(), 1);
    }

    #[test]
    fn test_alloc_in_unsafe_allowed() {
        let mut tracker = UnsafePointerTracker::new();

        // Alloc inside unsafe should be allowed
        let alloc_expr = HirExpr::Alloc(
            Box::new(HirType::U8),
            Box::new(HirExpr::Literal(HirLiteral::Int(128))),
        );
        tracker.analyze_expr(&alloc_expr, true);

        assert_eq!(tracker.get_errors().len(), 0);
    }

    #[test]
    fn test_free_requires_unsafe() {
        let mut tracker = UnsafePointerTracker::new();

        // Free outside unsafe should error
        let free_expr = HirExpr::Free(Box::new(HirExpr::LoadVar("ptr".to_string())));
        tracker.analyze_expr(&free_expr, false);

        assert_eq!(tracker.get_errors().len(), 1);
    }

    #[test]
    fn test_deref_requires_unsafe() {
        let mut tracker = UnsafePointerTracker::new();

        // Deref of a variable outside unsafe should error
        let deref_expr = HirExpr::Deref(Box::new(HirExpr::LoadVar("ptr".to_string())));
        tracker.analyze_expr(&deref_expr, false);

        assert_eq!(tracker.get_errors().len(), 1);
    }

    #[test]
    fn test_deref_in_unsafe_allowed() {
        let mut tracker = UnsafePointerTracker::new();

        // Deref inside unsafe should be allowed
        let deref_expr = HirExpr::Deref(Box::new(HirExpr::LoadVar("ptr".to_string())));
        tracker.analyze_expr(&deref_expr, true);

        assert_eq!(tracker.get_errors().len(), 0);
    }
}
