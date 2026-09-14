//! HIR Adapter for Safety Modules
//!
//! This module adapts the existing HIR structure to work with the new safety modules.
//! It provides a consistent interface across all backends (HIR, IR, LIR).

use crate::parsing::hir::*;
use std::collections::HashMap;

/// Place identifier - represents a memory location (variable or field access)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaceId(pub usize);

/// Block identifier for CFG
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

/// Adapter for tracking variables and memory locations
pub struct PlaceTracker {
    var_to_place: HashMap<String, PlaceId>,
    place_to_var: HashMap<PlaceId, String>,
    next_place_id: usize,
}

impl PlaceTracker {
    pub fn new() -> Self {
        Self {
            var_to_place: HashMap::new(),
            place_to_var: HashMap::new(),
            next_place_id: 0,
        }
    }

    pub fn get_or_create_place(&mut self, var_name: &str) -> PlaceId {
        if let Some(&place_id) = self.var_to_place.get(var_name) {
            place_id
        } else {
            let place_id = PlaceId(self.next_place_id);
            self.next_place_id += 1;
            self.var_to_place.insert(var_name.to_string(), place_id);
            self.place_to_var.insert(place_id, var_name.to_string());
            place_id
        }
    }

    pub fn get_place(&self, var_name: &str) -> Option<PlaceId> {
        self.var_to_place.get(var_name).copied()
    }

    pub fn get_var_name(&self, place_id: PlaceId) -> Option<&str> {
        self.place_to_var.get(&place_id).map(|s| s.as_str())
    }
}

/// Extracts moves from HIR expressions
pub fn extract_moves_from_expr(
    expr: &HirExpr,
    tracker: &mut PlaceTracker,
    moves: &mut Vec<PlaceId>,
) {
    match expr {
        HirExpr::LoadVar(var_name) => {
            // Variable load might be a move if not borrowed
            let place = tracker.get_or_create_place(var_name);
            moves.push(place);
        }
        HirExpr::Move(inner) => {
            // Explicit move
            extract_moves_from_expr(inner, tracker, moves);
        }
        HirExpr::BinaryOp(left, _, right) => {
            extract_moves_from_expr(left, tracker, moves);
            extract_moves_from_expr(right, tracker, moves);
        }
        HirExpr::UnaryOp(_, operand) => {
            extract_moves_from_expr(operand, tracker, moves);
        }
        HirExpr::Call(func, args, _) => {
            extract_moves_from_expr(func, tracker, moves);
            for arg in args {
                extract_moves_from_expr(arg, tracker, moves);
            }
        }
        HirExpr::MethodCall(obj, _, args) => {
            extract_moves_from_expr(obj, tracker, moves);
            for arg in args {
                extract_moves_from_expr(arg, tracker, moves);
            }
        }
        HirExpr::ArrayLiteral(elements) => {
            for elem in elements {
                extract_moves_from_expr(elem, tracker, moves);
            }
        }
        HirExpr::DictLiteral(pairs) => {
            for (key, value) in pairs {
                extract_moves_from_expr(key, tracker, moves);
                extract_moves_from_expr(value, tracker, moves);
            }
        }
        HirExpr::TupleLiteral(elements) | HirExpr::SetLiteral(elements) => {
            for elem in elements {
                extract_moves_from_expr(elem, tracker, moves);
            }
        }
        HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
            for (_, value) in fields {
                extract_moves_from_expr(value, tracker, moves);
            }
        }
        HirExpr::Index(obj, index) => {
            extract_moves_from_expr(obj, tracker, moves);
            extract_moves_from_expr(index, tracker, moves);
        }
        HirExpr::MemberAccess(obj, _) => {
            extract_moves_from_expr(obj, tracker, moves);
        }
        HirExpr::SetMember(obj, _, value) => {
            extract_moves_from_expr(obj, tracker, moves);
            extract_moves_from_expr(value, tracker, moves);
        }
        HirExpr::Conditional(cond, then_expr, else_expr) => {
            extract_moves_from_expr(cond, tracker, moves);
            extract_moves_from_expr(then_expr, tracker, moves);
            extract_moves_from_expr(else_expr, tracker, moves);
        }
        HirExpr::StoreVar(_, value) => {
            extract_moves_from_expr(value, tracker, moves);
        }
        HirExpr::NewInstance(_, args) => {
            for arg in args {
                extract_moves_from_expr(arg, tracker, moves);
            }
        }
        HirExpr::Await(expr) | HirExpr::Spawn(expr) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirExpr::Range(start, end, _) => {
            extract_moves_from_expr(start, tracker, moves);
            extract_moves_from_expr(end, tracker, moves);
        }
        HirExpr::Spread(expr) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirExpr::OptionalGet(expr, _) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirExpr::NonNull(expr) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirExpr::Update(expr, _, _) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirExpr::Match(expr, arms) => {
            extract_moves_from_expr(expr, tracker, moves);
            for (_, arm_expr) in arms {
                extract_moves_from_expr(arm_expr, tracker, moves);
            }
        }
        HirExpr::Format(expr, _) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        // Borrow operations don't move
        HirExpr::Borrow(_, _) | HirExpr::BorrowImmut(_) | HirExpr::BorrowMut(_) => {}
        HirExpr::Deref(expr) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirExpr::Share(expr) | HirExpr::Downgrade(expr) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirExpr::Alloc(_, size) => {
            extract_moves_from_expr(size, tracker, moves);
        }
        HirExpr::Free(ptr) => {
            extract_moves_from_expr(ptr, tracker, moves);
        }
        _ => {}
    }
}

/// Extracts moves from HIR statements
pub fn extract_moves_from_stmt(
    stmt: &HirStmt,
    tracker: &mut PlaceTracker,
    moves: &mut Vec<PlaceId>,
) {
    match stmt {
        HirStmt::Let {
            init: Some(init), ..
        } => {
            extract_moves_from_expr(init, tracker, moves);
        }
        HirStmt::LetTuple {
            init: Some(init), ..
        } => {
            extract_moves_from_expr(init, tracker, moves);
        }
        HirStmt::Assign {
            target,
            value,
            is_move,
        } => {
            if *is_move {
                extract_moves_from_expr(value, tracker, moves);
            }
            // Also track assignments to track ownership transfer
            if let HirExpr::LoadVar(var_name) = target {
                let place = tracker.get_or_create_place(var_name);
                moves.push(place);
            }
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) | HirStmt::Throw(expr) => {
            extract_moves_from_expr(expr, tracker, moves);
        }
        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            extract_moves_from_expr(cond, tracker, moves);
            extract_moves_from_stmt(then_branch, tracker, moves);
            if let Some(else_br) = else_branch {
                extract_moves_from_stmt(else_br, tracker, moves);
            }
        }
        HirStmt::While { cond, body } => {
            extract_moves_from_expr(cond, tracker, moves);
            extract_moves_from_stmt(body, tracker, moves);
        }
        HirStmt::ForIn { iter, body, .. } => {
            extract_moves_from_expr(iter, tracker, moves);
            extract_moves_from_stmt(body, tracker, moves);
        }
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                extract_moves_from_stmt(stmt, tracker, moves);
            }
        }
        HirStmt::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            extract_moves_from_stmt(try_block, tracker, moves);
            extract_moves_from_stmt(catch_block, tracker, moves);
        }
        HirStmt::Region { body, .. } | HirStmt::Unsafe(body) => {
            extract_moves_from_stmt(body, tracker, moves);
        }
        _ => {}
    }
}

/// Check if a variable is in an unsafe block
pub fn is_in_unsafe_context(stmt: &HirStmt) -> bool {
    matches!(stmt, HirStmt::Unsafe(_))
}

/// Extract all variables declared in a statement
pub fn extract_declared_vars(stmt: &HirStmt, vars: &mut Vec<String>) {
    match stmt {
        HirStmt::Let { name, .. } => {
            vars.push(name.clone());
        }
        HirStmt::LetTuple { names, .. } => {
            vars.extend(names.clone());
        }
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                extract_declared_vars(stmt, vars);
            }
        }
        HirStmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            extract_declared_vars(then_branch, vars);
            if let Some(else_br) = else_branch {
                extract_declared_vars(else_br, vars);
            }
        }
        HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
            extract_declared_vars(body, vars);
        }
        HirStmt::TryCatch {
            try_block,
            catch_block,
            error_name,
            ..
        } => {
            extract_declared_vars(try_block, vars);
            vars.push(error_name.clone());
            extract_declared_vars(catch_block, vars);
        }
        HirStmt::Region { body, .. } | HirStmt::Unsafe(body) => {
            extract_declared_vars(body, vars);
        }
        HirStmt::FunctionDef { params, .. } => {
            for (param_name, _, _) in params {
                vars.push(param_name.clone());
            }
        }
        _ => {}
    }
}
