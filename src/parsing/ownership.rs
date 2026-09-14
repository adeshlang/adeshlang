//! Ownership and Borrow Checking System
//!
//! This module implements Rust-like ownership rules for AdeshLang:
//! - Each value has exactly one owner
//! - Values are moved by default (no implicit copies)
//! - Borrowing rules: multiple immutable OR one mutable borrow
//! - Lifetime tracking to prevent use-after-free
//! - No implicit cloning or reference counting

use super::hir::{HirExpr, HirFunction, HirId, HirModule, HirStmt, HirType};
use std::collections::{HashMap, HashSet};

// ============================================
// COPY TYPE CHECK
// ============================================

/// Check if a HirType is a Copy type (primitive types that are bitwise-copied, not moved).
/// Copy types: Int, Float, Bool, Char, Null, all fixed-width integers/floats.
/// Non-Copy types: String, Array, Dict, Set, Tuple, Object, Class, Instance, etc.
fn is_copy_type(ty: &HirType) -> bool {
    match ty {
        HirType::Int
        | HirType::Float
        | HirType::Bool
        | HirType::Char
        | HirType::Null
        | HirType::U8
        | HirType::U16
        | HirType::U32
        | HirType::U64
        | HirType::U128
        | HirType::I8
        | HirType::I16
        | HirType::I32
        | HirType::I64
        | HirType::I128
        | HirType::F32
        | HirType::F64 => true,
        // Simd vectors are Copy (fixed-size, bitwise-copyable)
        HirType::Simd(..) => true,
        // Everything else is non-Copy (moved on assignment)
        _ => false,
    }
}

/// Check if a variable should be moved when used in an expression.
/// Returns true if the variable is known to be non-Copy, false if Copy or unknown.
/// Being conservative: when type is unknown, don't move (avoid false positives).
fn should_move_on_use(ty: Option<&HirType>) -> bool {
    match ty {
        Some(t) => !is_copy_type(t),
        None => false, // Conservative: don't move if type is unknown
    }
}

// ============================================
// OWNERSHIP STATE
// ============================================

/// Represents the ownership state of a variable
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipState {
    /// Variable is owned and can be moved or borrowed
    Owned,
    /// Variable has been moved and is no longer accessible
    Moved { moved_at: HirId },
    /// Variable is borrowed immutably (can have multiple immutable borrows)
    BorrowedImmut { borrow_count: usize },
    /// Variable is borrowed mutably (exclusive borrow)
    BorrowedMut { borrowed_at: HirId },
}

/// Represents a borrow (reference)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Borrow {
    pub variable: String,
    pub is_mutable: bool,
    pub borrow_id: HirId,
    pub scope_depth: usize,
}

/// Ownership and borrow checker context
#[derive(Debug)]
pub struct OwnershipContext {
    /// Maps variable names to their ownership state
    ownership: HashMap<String, OwnershipState>,
    /// Active borrows (references) currently in scope
    active_borrows: Vec<Borrow>,
    /// Current scope depth (for tracking nested scopes)
    scope_depth: usize,
    /// Variables that need explicit ARC (shared ownership)
    needs_arc: HashSet<String>,
    /// Variables that escape their scope (need heap allocation)
    escaping_vars: HashSet<String>,
}

impl OwnershipContext {
    pub fn new() -> Self {
        Self {
            ownership: HashMap::new(),
            active_borrows: Vec::new(),
            scope_depth: 0,
            needs_arc: HashSet::new(),
            escaping_vars: HashSet::new(),
        }
    }

    /// Register a new owned variable
    pub fn register_owned(&mut self, name: String) {
        self.ownership.insert(name, OwnershipState::Owned);
    }

    /// Move a variable (transfers ownership)
    pub fn move_variable(&mut self, name: &str, move_id: HirId) -> Result<(), OwnershipError> {
        match self.ownership.get(name) {
            Some(OwnershipState::Owned) => {
                self.ownership.insert(
                    name.to_string(),
                    OwnershipState::Moved { moved_at: move_id },
                );
                Ok(())
            }
            Some(OwnershipState::Moved { moved_at }) => Err(OwnershipError::UseAfterMove {
                variable: name.to_string(),
                moved_at: *moved_at,
                used_at: move_id,
            }),
            Some(OwnershipState::BorrowedImmut { .. }) => Err(OwnershipError::MoveWhileBorrowed {
                variable: name.to_string(),
                move_id,
            }),
            Some(OwnershipState::BorrowedMut { borrowed_at: _ }) => {
                Err(OwnershipError::MoveWhileBorrowed {
                    variable: name.to_string(),
                    move_id,
                })
            }
            None => Err(OwnershipError::UnknownVariable {
                variable: name.to_string(),
            }),
        }
    }

    /// Borrow a variable immutably
    pub fn borrow_immut(&mut self, name: &str, borrow_id: HirId) -> Result<(), OwnershipError> {
        match self.ownership.get_mut(name) {
            Some(OwnershipState::Owned) => {
                // Can borrow owned value
                *self.ownership.get_mut(name).unwrap() =
                    OwnershipState::BorrowedImmut { borrow_count: 1 };
                self.active_borrows.push(Borrow {
                    variable: name.to_string(),
                    is_mutable: false,
                    borrow_id,
                    scope_depth: self.scope_depth,
                });
                Ok(())
            }
            Some(OwnershipState::BorrowedImmut { borrow_count }) => {
                // Can have multiple immutable borrows
                *borrow_count += 1;
                self.active_borrows.push(Borrow {
                    variable: name.to_string(),
                    is_mutable: false,
                    borrow_id,
                    scope_depth: self.scope_depth,
                });
                Ok(())
            }
            Some(OwnershipState::BorrowedMut { borrowed_at }) => {
                Err(OwnershipError::BorrowWhileMutablyBorrowed {
                    variable: name.to_string(),
                    mutable_borrow_at: *borrowed_at,
                    new_borrow_at: borrow_id,
                })
            }
            Some(OwnershipState::Moved { moved_at }) => Err(OwnershipError::BorrowAfterMove {
                variable: name.to_string(),
                moved_at: *moved_at,
                borrow_at: borrow_id,
            }),
            None => Err(OwnershipError::UnknownVariable {
                variable: name.to_string(),
            }),
        }
    }

    /// Borrow a variable mutably
    pub fn borrow_mut(&mut self, name: &str, borrow_id: HirId) -> Result<(), OwnershipError> {
        match self.ownership.get(name) {
            Some(OwnershipState::Owned) => {
                // Can borrow owned value mutably
                self.ownership.insert(
                    name.to_string(),
                    OwnershipState::BorrowedMut {
                        borrowed_at: borrow_id,
                    },
                );
                self.active_borrows.push(Borrow {
                    variable: name.to_string(),
                    is_mutable: true,
                    borrow_id,
                    scope_depth: self.scope_depth,
                });
                Ok(())
            }
            Some(OwnershipState::BorrowedImmut { .. }) => {
                Err(OwnershipError::MutableBorrowWhileBorrowed {
                    variable: name.to_string(),
                    borrow_at: borrow_id,
                })
            }
            Some(OwnershipState::BorrowedMut { borrowed_at }) => {
                Err(OwnershipError::MutableBorrowWhileMutablyBorrowed {
                    variable: name.to_string(),
                    existing_borrow: *borrowed_at,
                    new_borrow: borrow_id,
                })
            }
            Some(OwnershipState::Moved { moved_at }) => Err(OwnershipError::BorrowAfterMove {
                variable: name.to_string(),
                moved_at: *moved_at,
                borrow_at: borrow_id,
            }),
            None => Err(OwnershipError::UnknownVariable {
                variable: name.to_string(),
            }),
        }
    }

    /// End a borrow (reference goes out of scope)
    pub fn end_borrow(&mut self, variable: &str) {
        match self.ownership.get_mut(variable) {
            Some(OwnershipState::BorrowedImmut { borrow_count }) => {
                *borrow_count -= 1;
                if *borrow_count == 0 {
                    self.ownership
                        .insert(variable.to_string(), OwnershipState::Owned);
                }
            }
            Some(OwnershipState::BorrowedMut { .. }) => {
                self.ownership
                    .insert(variable.to_string(), OwnershipState::Owned);
            }
            _ => {}
        }
    }

    /// Enter a new scope
    pub fn enter_scope(&mut self) {
        self.scope_depth += 1;
    }

    /// Exit a scope (end all borrows in this scope)
    pub fn exit_scope(&mut self) {
        // End all borrows at current scope depth
        let scope = self.scope_depth;
        let to_end: Vec<String> = self
            .active_borrows
            .iter()
            .filter(|b| b.scope_depth == scope)
            .map(|b| b.variable.clone())
            .collect();

        for var in to_end {
            self.end_borrow(&var);
        }

        // Remove ended borrows
        self.active_borrows.retain(|b| b.scope_depth < scope);
        self.scope_depth -= 1;
    }

    /// Mark a variable as needing ARC (shared ownership)
    pub fn mark_needs_arc(&mut self, name: String) {
        self.needs_arc.insert(name);
    }

    /// Mark a variable as escaping its scope
    pub fn mark_escaping(&mut self, name: String) {
        self.escaping_vars.insert(name);
    }

    /// Check if a variable needs ARC
    pub fn needs_arc(&self, name: &str) -> bool {
        self.needs_arc.contains(name)
    }

    /// Check if a variable escapes
    pub fn escapes(&self, name: &str) -> bool {
        self.escaping_vars.contains(name)
    }
}

// ============================================
// OWNERSHIP ERRORS
// ============================================

#[derive(Debug, Clone)]
pub enum OwnershipError {
    /// Variable used after being moved
    UseAfterMove {
        variable: String,
        moved_at: HirId,
        used_at: HirId,
    },
    /// Trying to move while borrowed
    MoveWhileBorrowed { variable: String, move_id: HirId },
    /// Trying to borrow after move
    BorrowAfterMove {
        variable: String,
        moved_at: HirId,
        borrow_at: HirId,
    },
    /// Trying to borrow while mutably borrowed
    BorrowWhileMutablyBorrowed {
        variable: String,
        mutable_borrow_at: HirId,
        new_borrow_at: HirId,
    },
    /// Trying to mutably borrow while borrowed
    MutableBorrowWhileBorrowed { variable: String, borrow_at: HirId },
    /// Trying to mutably borrow while already mutably borrowed
    MutableBorrowWhileMutablyBorrowed {
        variable: String,
        existing_borrow: HirId,
        new_borrow: HirId,
    },
    /// Variable not found
    UnknownVariable { variable: String },
    /// Multiple mutable borrows
    MultipleMutableBorrows {
        variable: String,
        first_borrow: HirId,
        second_borrow: HirId,
    },
}

impl OwnershipError {
    pub fn message(&self) -> String {
        match self {
            Self::UseAfterMove { variable, .. } => {
                format!("use of moved value `{}`", variable)
            }
            Self::MoveWhileBorrowed { variable, .. } => {
                format!("cannot move `{}` while it is borrowed", variable)
            }
            Self::BorrowAfterMove { variable, .. } => {
                format!("cannot borrow `{}` after it has been moved", variable)
            }
            Self::BorrowWhileMutablyBorrowed { variable, .. } => {
                format!("cannot borrow `{}` while it is mutably borrowed", variable)
            }
            Self::MutableBorrowWhileBorrowed { variable, .. } => {
                format!(
                    "cannot borrow `{}` as mutable because it is already borrowed",
                    variable
                )
            }
            Self::MutableBorrowWhileMutablyBorrowed { variable, .. } => {
                format!("cannot borrow `{}` as mutable more than once", variable)
            }
            Self::UnknownVariable { variable } => {
                format!("unknown variable `{}`", variable)
            }
            Self::MultipleMutableBorrows { variable, .. } => {
                format!("cannot have multiple mutable borrows of `{}`", variable)
            }
        }
    }
}

// ============================================
// OWNERSHIP CHECKER
// ============================================

/// Check ownership rules in an HIR module
pub fn check_ownership(module: &HirModule) -> Result<OwnershipAnalysis, Vec<OwnershipError>> {
    let mut ctx = OwnershipContext::new();
    let mut errors = Vec::new();

    // Check all statements
    for stmt in &module.statements {
        check_stmt_ownership(stmt, &mut ctx, &mut errors);
    }

    // Check functions
    for func in &module.functions {
        check_function_ownership(func, &mut ctx, &mut errors);
    }

    // Check classes
    for class in &module.classes {
        check_class_ownership(class, &mut ctx, &mut errors);
    }

    if errors.is_empty() {
        Ok(OwnershipAnalysis {
            needs_arc: ctx.needs_arc.clone(),
            escaping_vars: ctx.escaping_vars.clone(),
        })
    } else {
        Err(errors)
    }
}

/// Result of ownership analysis
#[derive(Debug, Clone)]
pub struct OwnershipAnalysis {
    /// Variables that need explicit ARC
    pub needs_arc: HashSet<String>,
    /// Variables that escape their scope
    pub escaping_vars: HashSet<String>,
}

fn check_stmt_ownership(
    stmt: &HirStmt,
    ctx: &mut OwnershipContext,
    errors: &mut Vec<OwnershipError>,
) {
    match stmt {
        HirStmt::Let { name, init, .. } => {
            // Register new variable as owned
            ctx.register_owned(name.clone());

            // Check initializer
            if let Some(expr) = init {
                check_expr_ownership(expr, ctx, errors, false);
            }
        }

        HirStmt::LetTuple { names, init, .. } => {
            // Register all variables
            for name in names {
                ctx.register_owned(name.clone());
            }

            if let Some(expr) = init {
                check_expr_ownership(expr, ctx, errors, false);
            }
        }

        HirStmt::Assign {
            target,
            value,
            is_move: _,
        } => {
            // Assignment is a move operation
            check_expr_ownership(target, ctx, errors, true);
            check_expr_ownership(value, ctx, errors, false);
        }

        HirStmt::Expr(expr) => {
            check_expr_ownership(expr, ctx, errors, false);
        }

        HirStmt::Return(expr_opt) => {
            if let Some(expr) = expr_opt {
                // Return moves the value
                check_expr_ownership(expr, ctx, errors, false);
            }
        }

        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            check_expr_ownership(cond, ctx, errors, false);

            ctx.enter_scope();
            check_stmt_ownership(then_branch, ctx, errors);
            ctx.exit_scope();

            if let Some(else_stmt) = else_branch {
                ctx.enter_scope();
                check_stmt_ownership(else_stmt, ctx, errors);
                ctx.exit_scope();
            }
        }

        HirStmt::While { cond, body } => {
            check_expr_ownership(cond, ctx, errors, false);

            ctx.enter_scope();
            check_stmt_ownership(body, ctx, errors);
            ctx.exit_scope();
        }

        HirStmt::ForIn { var, iter, body } => {
            check_expr_ownership(iter, ctx, errors, false);

            ctx.enter_scope();
            ctx.register_owned(var.clone());
            check_stmt_ownership(body, ctx, errors);
            ctx.exit_scope();
        }

        HirStmt::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            ctx.enter_scope();
            check_stmt_ownership(try_block, ctx, errors);
            ctx.exit_scope();

            ctx.enter_scope();
            check_stmt_ownership(catch_block, ctx, errors);
            ctx.exit_scope();
        }

        HirStmt::Block(stmts) => {
            ctx.enter_scope();
            for s in stmts {
                check_stmt_ownership(s, ctx, errors);
            }
            ctx.exit_scope();
        }

        HirStmt::FunctionDef {
            name, params, body, ..
        } => {
            ctx.register_owned(name.clone());

            // Function body gets new scope
            ctx.enter_scope();
            for (param_name, _, _) in params {
                ctx.register_owned(param_name.clone());
            }
            for s in body.iter() {
                check_stmt_ownership(s, ctx, errors);
            }
            ctx.exit_scope();
        }

        HirStmt::ClassDef { .. } => {
            // Classes handled separately
        }

        _ => {}
    }
}

fn check_expr_ownership(
    expr: &HirExpr,
    ctx: &mut OwnershipContext,
    _errors: &mut Vec<OwnershipError>,
    _is_lvalue: bool,
) {
    match expr {
        HirExpr::LoadVar(name) => {
            // Simple read - treat as immutable borrow for now
            // Full implementation would track move vs copy semantics
            let _ = ctx.borrow_immut(name, 0); // Using 0 as placeholder ID
        }

        HirExpr::BinaryOp(left, _, right) => {
            check_expr_ownership(left, ctx, _errors, false);
            check_expr_ownership(right, ctx, _errors, false);
        }

        HirExpr::UnaryOp(_, operand) => {
            check_expr_ownership(operand, ctx, _errors, false);
        }

        HirExpr::Call(func, args, _) => {
            check_expr_ownership(func, ctx, _errors, false);
            for arg in args {
                check_expr_ownership(arg, ctx, _errors, false);
            }
        }

        HirExpr::Index(object, index) => {
            check_expr_ownership(object, ctx, _errors, false);
            check_expr_ownership(index, ctx, _errors, false);
        }

        HirExpr::MemberAccess(object, _) => {
            check_expr_ownership(object, ctx, _errors, false);
        }

        HirExpr::ArrayLiteral(elements) => {
            for elem in elements {
                check_expr_ownership(elem, ctx, _errors, false);
            }
        }

        HirExpr::ObjectLiteral(properties) => {
            for (_, value) in properties {
                check_expr_ownership(value, ctx, _errors, false);
            }
        }

        HirExpr::Lambda(params, body, _is_async) => {
            // Lambda creates new scope
            ctx.enter_scope();
            for (param_name, _) in params {
                ctx.register_owned(param_name.clone());
            }
            for s in body.iter() {
                check_stmt_ownership(s, ctx, _errors);
            }
            ctx.exit_scope();
        }

        HirExpr::Cast(inner, _) => {
            check_expr_ownership(inner, ctx, _errors, false);
        }

        _ => {
            // Literals, etc. - no ownership concerns
        }
    }
}

fn check_function_ownership(
    func: &HirFunction,
    ctx: &mut OwnershipContext,
    errors: &mut Vec<OwnershipError>,
) {
    ctx.enter_scope();

    // Register parameters as owned (extract name from tuple)
    for (param_name, _, _) in &func.params {
        ctx.register_owned(param_name.clone());
    }

    // Check function body
    for stmt in func.body.iter() {
        check_stmt_ownership(stmt, ctx, errors);
    }

    ctx.exit_scope();
}

fn check_class_ownership(
    class: &super::hir::HirClass,
    ctx: &mut OwnershipContext,
    _errors: &mut Vec<OwnershipError>,
) {
    // Register class name
    ctx.register_owned(class.name.clone());

    // Methods are checked separately - we could check them here
    // but for now just register the class
}

// ============================================
// MOVE SEMANTICS CHECKER
// ============================================

/// Check that moves are valid and track moved values
/// This pass specifically focuses on detecting use-after-move patterns
/// and partial moves (moving individual fields of a struct/object)
pub fn check_move_semantics(module: &HirModule) -> Result<(), Vec<OwnershipError>> {
    let mut ctx = OwnershipContext::new();
    let mut errors = Vec::new();

    // Track all moves and ensure no use-after-move
    for stmt in &module.statements {
        check_stmt_moves(stmt, &mut ctx, &mut errors);
    }

    // Also check function bodies
    for func in &module.functions {
        check_function_moves(func, &mut ctx, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check a function body for move violations
fn check_function_moves(
    func: &HirFunction,
    ctx: &mut OwnershipContext,
    errors: &mut Vec<OwnershipError>,
) {
    ctx.enter_scope();

    // Register parameters as owned
    for (param_name, _, _) in &func.params {
        ctx.register_owned(param_name.clone());
    }

    // Check function body
    for stmt in func.body.iter() {
        check_stmt_moves(stmt, ctx, errors);
    }

    ctx.exit_scope();
}

/// Check a statement for move violations
/// Tracks moves, partial moves (field-level), and use-after-move
fn check_stmt_moves(
    stmt: &HirStmt,
    ctx: &mut OwnershipContext,
    errors: &mut Vec<OwnershipError>,
) {
    match stmt {
        HirStmt::Let { name, ty, init, .. } => {
            // Register new variable as owned
            ctx.register_owned(name.clone());

            // Check initializer for use of moved variables
            if let Some(expr) = init {
                check_expr_moves(expr, ctx, errors);

                // If init is a LoadVar, it's a move — but ONLY for non-Copy types.
                // Copy types (numbers, bools, chars, SIMD) are bitwise-copied, not moved.
                if let HirExpr::LoadVar(src) = expr {
                    if should_move_on_use(ty.as_ref()) {
                        if let Err(e) = ctx.move_variable(src, 0) {
                            errors.push(e);
                        }
                    }
                }
            }
        }

        HirStmt::LetTuple { names, init, .. } => {
            for name in names {
                ctx.register_owned(name.clone());
            }
            if let Some(expr) = init {
                check_expr_moves(expr, ctx, errors);
            }
        }

        HirStmt::Assign {
            target,
            value,
            is_move,
        } => {
            // Check RHS for moved variable usage
            check_expr_moves(value, ctx, errors);

            // If this is a move assignment, mark source as moved
            if *is_move {
                if let HirExpr::LoadVar(src) = value {
                    if let Err(e) = ctx.move_variable(src, 0) {
                        errors.push(e);
                    }
                }
            }

            // Check target for partial move (field assignment)
            check_partial_move_target(target, ctx, errors);
        }

        HirStmt::Expr(expr) => {
            check_expr_moves(expr, ctx, errors);

            // Function calls: check arguments for use of moved variables,
            // but do NOT mark arguments as moved. In AdeshLang, the borrow
            // inference system determines whether arguments are borrowed or
            // moved based on type information. The move checker should only
            // detect use-of-already-moved values, not create new moves.
            if let HirExpr::Call(func, args, _) = expr {
                check_expr_moves(func, ctx, errors);
                for arg in args {
                    check_expr_moves(arg, ctx, errors);
                }
            }
        }

        HirStmt::Return(expr_opt) => {
            if let Some(expr) = expr_opt {
                check_expr_moves(expr, ctx, errors);
                // Return moves the value — but only for non-Copy types.
                // For Copy types (numbers, bools, etc.), the value is copied, not moved.
                // Since we don't have the return type here, be conservative and don't mark.
                // The borrow checker and other passes handle use-after-free for non-Copy types.
            }
        }

        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            check_expr_moves(cond, ctx, errors);
            ctx.enter_scope();
            check_stmt_moves(then_branch, ctx, errors);
            ctx.exit_scope();
            if let Some(else_stmt) = else_branch {
                ctx.enter_scope();
                check_stmt_moves(else_stmt, ctx, errors);
                ctx.exit_scope();
            }
        }

        HirStmt::While { cond, body } => {
            check_expr_moves(cond, ctx, errors);
            ctx.enter_scope();
            check_stmt_moves(body, ctx, errors);
            ctx.exit_scope();
        }

        HirStmt::ForIn { var, iter, body } => {
            check_expr_moves(iter, ctx, errors);
            ctx.enter_scope();
            ctx.register_owned(var.clone());
            check_stmt_moves(body, ctx, errors);
            ctx.exit_scope();
        }

        HirStmt::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            ctx.enter_scope();
            check_stmt_moves(try_block, ctx, errors);
            ctx.exit_scope();
            ctx.enter_scope();
            check_stmt_moves(catch_block, ctx, errors);
            ctx.exit_scope();
        }

        HirStmt::Block(stmts) => {
            ctx.enter_scope();
            for s in stmts {
                check_stmt_moves(s, ctx, errors);
            }
            ctx.exit_scope();
        }

        HirStmt::FunctionDef {
            name, params, body, ..
        } => {
            ctx.register_owned(name.clone());
            ctx.enter_scope();
            for (param_name, _, _) in params {
                ctx.register_owned(param_name.clone());
            }
            for s in body.iter() {
                check_stmt_moves(s, ctx, errors);
            }
            ctx.exit_scope();
        }

        _ => {}
    }
}

/// Check an expression for use of moved variables
fn check_expr_moves(
    expr: &HirExpr,
    ctx: &mut OwnershipContext,
    errors: &mut Vec<OwnershipError>,
) {
    match expr {
        HirExpr::LoadVar(name) => {
            // Check if variable has been moved
            if let Some(state) = ctx.ownership.get(name) {
                if let OwnershipState::Moved { moved_at } = state {
                    errors.push(OwnershipError::UseAfterMove {
                        variable: name.clone(),
                        moved_at: *moved_at,
                        used_at: 0,
                    });
                }
            }
        }

        HirExpr::BinaryOp(left, _, right) => {
            check_expr_moves(left, ctx, errors);
            check_expr_moves(right, ctx, errors);
        }

        HirExpr::UnaryOp(_, operand) => {
            check_expr_moves(operand, ctx, errors);
        }

        HirExpr::Call(func, args, _) => {
            check_expr_moves(func, ctx, errors);
            for arg in args {
                check_expr_moves(arg, ctx, errors);
            }
        }

        HirExpr::MethodCall(obj, _, args) => {
            check_expr_moves(obj, ctx, errors);
            for arg in args {
                check_expr_moves(arg, ctx, errors);
            }
        }

        HirExpr::Index(obj, idx) => {
            check_expr_moves(obj, ctx, errors);
            check_expr_moves(idx, ctx, errors);
        }

        HirExpr::MemberAccess(obj, _field) => {
            check_expr_moves(obj, ctx, errors);
            // Check for partial move: accessing a field of a moved struct
            if let HirExpr::LoadVar(src) = obj.as_ref() {
                if let Some(state) = ctx.ownership.get(src) {
                    if let OwnershipState::Moved { moved_at } = state {
                        errors.push(OwnershipError::UseAfterMove {
                            variable: src.clone(),
                            moved_at: *moved_at,
                            used_at: 0,
                        });
                    }
                }
            }
        }

        HirExpr::ArrayLiteral(elements) => {
            for elem in elements {
                check_expr_moves(elem, ctx, errors);
            }
        }

        HirExpr::ObjectLiteral(properties) => {
            for (_, value) in properties {
                check_expr_moves(value, ctx, errors);
            }
        }

        HirExpr::Lambda(params, body, _) => {
            ctx.enter_scope();
            for (param_name, _) in params {
                ctx.register_owned(param_name.clone());
            }
            for s in body.iter() {
                check_stmt_moves(s, ctx, errors);
            }
            ctx.exit_scope();
        }

        HirExpr::Cast(inner, _) => {
            check_expr_moves(inner, ctx, errors);
        }

        HirExpr::Borrow(inner, _) | HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
            check_expr_moves(inner, ctx, errors);
        }

        HirExpr::Deref(inner) => {
            check_expr_moves(inner, ctx, errors);
        }

        HirExpr::Move(inner) => {
            // Explicit move
            if let HirExpr::LoadVar(src) = inner.as_ref() {
                if let Err(e) = ctx.move_variable(src, 0) {
                    errors.push(e);
                }
            } else {
                check_expr_moves(inner, ctx, errors);
            }
        }

        HirExpr::Conditional(cond, then_expr, else_expr) => {
            check_expr_moves(cond, ctx, errors);
            check_expr_moves(then_expr, ctx, errors);
            check_expr_moves(else_expr, ctx, errors);
        }

        HirExpr::Match(expr, arms) => {
            check_expr_moves(expr, ctx, errors);
            for (_, arm_expr) in arms {
                check_expr_moves(arm_expr, ctx, errors);
            }
        }

        _ => {}
    }
}

/// Check for partial move in assignment target (e.g., `obj.field = value`)
/// Moving a field of an object is a partial move; the rest of the object is still usable
fn check_partial_move_target(
    target: &HirExpr,
    ctx: &mut OwnershipContext,
    errors: &mut Vec<OwnershipError>,
) {
    match target {
        HirExpr::MemberAccess(obj, _field) => {
            // Partial move: moving a field out of an object
            if let HirExpr::LoadVar(src) = obj.as_ref() {
                // Check if the whole object has been moved
                if let Some(state) = ctx.ownership.get(src) {
                    if let OwnershipState::Moved { moved_at } = state {
                        errors.push(OwnershipError::UseAfterMove {
                            variable: src.clone(),
                            moved_at: *moved_at,
                            used_at: 0,
                        });
                    }
                }
                // Note: partial moves of individual fields are allowed
                // The object itself remains valid, only the moved field is invalid
                // Full partial-move tracking would require per-field state tracking
            }
        }
        HirExpr::LoadVar(name) => {
            // Full assignment to a variable - check if target was borrowed
            if let Some(state) = ctx.ownership.get(name) {
                match state {
                    OwnershipState::BorrowedImmut { .. } | OwnershipState::BorrowedMut { .. } => {
                        // Assignment to a borrowed variable is a move-while-borrowed
                        errors.push(OwnershipError::MoveWhileBorrowed {
                            variable: name.clone(),
                            move_id: 0,
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {
            check_expr_moves(target, ctx, errors);
        }
    }
}
