//! Ownership tracking and move semantics analysis
//!
//! This module implements Rust-like ownership tracking:
//! - Every value has exactly one owner
//! - Ownership transfer on assignment (move semantics)
//! - Use-after-move detection
//! - Copy types don't move

use super::error::CompileTimeMemoryError;
use super::state::{OwnershipNode, OwnershipState};
use crate::parsing::hir::{
    HirClass, HirExpr, HirFunction, HirLiteral, HirModule, HirStmt, HirType,
};
use crate::utils::collections::FastSet;

impl super::CompileTimeMemorySafety {
    /// Build ownership graph from HIR module
    pub(super) fn build_ownership_graph(
        &mut self,
        module: &HirModule,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        for stmt in &module.statements {
            self.process_stmt_ownership(stmt, &mut 0)?;
        }

        for func in &module.functions {
            self.process_function_ownership(func)?;
        }

        for class in &module.classes {
            self.process_class_ownership(class)?;
        }

        Ok(())
    }

    /// Process statement for ownership tracking
    pub(super) fn process_stmt_ownership(
        &mut self,
        stmt: &HirStmt,
        scope_id: &mut usize,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match stmt {
            HirStmt::Let {
                name,
                ty,
                init,
                is_borrowed,
                ..
            } => {
                // ── Determine if this is an ARC declaration ─────────────────
                // `share x = v` lowers to Let { init: Share(LoadVar("v")), .. }
                // `strong x = v` / `weak x = v` lower to Let { init: Borrow(LoadVar("v"), false), is_borrowed: Some(false), .. }
                // In both cases the source is NOT moved — ARC is a refcount bump.
                let is_arc_decl = match init.as_ref() {
                    Some(HirExpr::Share(_)) => true,
                    Some(HirExpr::Borrow(_, false)) if is_borrowed.is_some() => true,
                    Some(HirExpr::BorrowImmut(_)) if is_borrowed.is_some() => true,
                    _ => false,
                };

                // Check if initializing with a Copy type (literal or from another Copy/ARC var)
                let is_copy = is_arc_decl
                    || init
                        .as_ref()
                        .map(|expr| {
                            // Direct Copy literal
                            if Self::is_literal_copy_type(expr) {
                                return true;
                            }
                            // Loading from another Copy variable
                            if let HirExpr::LoadVar(var_name) = expr {
                                return self
                                    .ownership_graph
                                    .get(var_name)
                                    .map(|n| n.is_copy_type)
                                    .unwrap_or(false);
                            }
                            // Check if the inferred type is a primitive Copy type!
                            if let Some(inferred_ty) = self.infer_type(expr) {
                                match inferred_ty {
                                    HirType::Int
                                    | HirType::Float
                                    | HirType::Bool
                                    | HirType::Char
                                    | HirType::I8
                                    | HirType::I16
                                    | HirType::I32
                                    | HirType::I64
                                    | HirType::I128
                                    | HirType::U8
                                    | HirType::U16
                                    | HirType::U32
                                    | HirType::U64
                                    | HirType::U128
                                    | HirType::F32
                                    | HirType::F64 => return true,
                                    _ => {}
                                }
                            }
                            false
                        })
                        .unwrap_or(false);

                // New variable is owned
                self.ownership_graph.insert(
                    name.clone(),
                    OwnershipNode {
                        owner: name.clone(),
                        owned_values: FastSet::default(),
                        state: OwnershipState::Owned,
                        location: self.get_source_location(0),
                        is_copy_type: is_copy,
                    },
                );

                // Track variable type
                if let Some(ty) = ty {
                    self.var_types.insert(name.clone(), ty.clone());
                } else if let Some(init_expr) = init {
                    if let Some(inferred) = self.infer_type(init_expr) {
                        self.var_types.insert(name.clone(), inferred);
                    }
                }

                if let Some(expr) = init {
                    if is_arc_decl {
                        // For ARC declarations, only check for pre-existing moved vars
                        // inside the expression — do NOT mark the source as moved.
                        self.check_expr_for_moved_vars(expr)?;
                    } else {
                        self.check_expr_ownership(expr, name, scope_id)?;
                    }
                }
            }

            HirStmt::Assign {
                target: _,
                value,
                is_move,
            } => {
                // Check if value expression uses any moved variables first
                self.check_expr_for_moved_vars(value)?;

                if *is_move {
                    // Track move
                    if let HirExpr::LoadVar(src_name) = value {
                        let current_location = self.get_source_location(0);
                        let suggestion =
                            crate::parsing::error::ownership_help::use_after_move(src_name);

                        if let Some(node) = self.ownership_graph.get_mut(src_name) {
                            if !node.is_copy_type {
                                if node.state == OwnershipState::Moved {
                                    self.errors.push(CompileTimeMemoryError::UseAfterMove {
                                        variable: src_name.clone(),
                                        moved_at: node.location.clone(),
                                        used_at: current_location.clone(),
                                        suggestion,
                                    });
                                } else {
                                    node.state = OwnershipState::Moved;
                                    node.location = current_location;
                                }
                            }
                        }
                    }
                }
            }

            HirStmt::Expr(expr) => {
                // Check if expression uses any moved variables
                self.check_expr_for_moved_vars(expr)?;
            }

            HirStmt::Return(Some(expr)) => {
                self.check_expr_for_moved_vars(expr)?;
            }

            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.check_expr_for_moved_vars(cond)?;
                self.process_stmt_ownership(then_branch, scope_id)?;
                if let Some(else_stmt) = else_branch {
                    self.process_stmt_ownership(else_stmt, scope_id)?;
                }
            }

            HirStmt::While { cond, body } => {
                self.check_expr_for_moved_vars(cond)?;
                self.process_stmt_ownership(body, scope_id)?;
            }

            HirStmt::ForIn { iter, body, .. } => {
                self.check_expr_for_moved_vars(iter)?;
                self.process_stmt_ownership(body, scope_id)?;
            }

            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.process_stmt_ownership(try_block, scope_id)?;
                self.process_stmt_ownership(catch_block, scope_id)?;
            }

            HirStmt::Block(stmts) => {
                *scope_id += 1;
                for s in stmts {
                    self.process_stmt_ownership(s, scope_id)?;
                }
            }

            HirStmt::LetTuple { names, init, .. } => {
                for name in names {
                    self.ownership_graph.insert(
                        name.clone(),
                        OwnershipNode {
                            owner: name.clone(),
                            owned_values: FastSet::default(),
                            state: OwnershipState::Owned,
                            location: self.get_source_location(0),
                            is_copy_type: false,
                        },
                    );
                }

                if let Some(expr) = init {
                    self.check_expr_for_moved_vars(expr)?;
                    if let HirExpr::Move(inner) = expr {
                        if let HirExpr::LoadVar(src_name) = inner.as_ref() {
                            let current_location = self.get_source_location(0);
                            if let Some(node) = self.ownership_graph.get_mut(src_name) {
                                if !node.is_copy_type {
                                    node.state = OwnershipState::Moved;
                                    node.location = current_location;
                                }
                            }
                        }
                    } else if let HirExpr::LoadVar(src_name) = expr {
                        let current_location = self.get_source_location(0);
                        if let Some(node) = self.ownership_graph.get_mut(src_name) {
                            if !node.is_copy_type {
                                node.state = OwnershipState::Moved;
                                node.location = current_location;
                            }
                        }
                    }
                }
            }

            _ => {}
        }

        Ok(())
    }

    /// Check if an expression uses any moved variables
    pub(super) fn check_expr_for_moved_vars(
        &mut self,
        expr: &HirExpr,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match expr {
            HirExpr::LoadVar(var_name) => {
                // Check if this variable has been moved
                if let Some(node) = self.ownership_graph.get(var_name) {
                    if node.state == OwnershipState::Moved {
                        self.errors.push(CompileTimeMemoryError::UseAfterMove {
                            variable: var_name.clone(),
                            moved_at: node.location.clone(),
                            used_at: self.get_source_location(0),
                            suggestion: crate::parsing::error::ownership_help::avoid_move(var_name),
                        });
                    }
                }
            }

            HirExpr::BinaryOp(left, _, right) => {
                self.check_expr_for_moved_vars(left)?;
                self.check_expr_for_moved_vars(right)?;
            }

            HirExpr::UnaryOp(_, operand) => {
                self.check_expr_for_moved_vars(operand)?;
            }

            HirExpr::Call(func, args, _) => {
                self.check_expr_for_moved_vars(func)?;
                for arg in args {
                    self.check_expr_for_moved_vars(arg)?;
                }
            }

            HirExpr::MethodCall(obj, _, args) => {
                self.check_expr_for_moved_vars(obj)?;
                for arg in args {
                    self.check_expr_for_moved_vars(arg)?;
                }
            }

            HirExpr::Index(obj, idx) => {
                self.check_expr_for_moved_vars(obj)?;
                self.check_expr_for_moved_vars(idx)?;
            }

            HirExpr::MemberAccess(obj, _) => {
                self.check_expr_for_moved_vars(obj)?;
            }

            HirExpr::ArrayLiteral(elements) => {
                for elem in elements {
                    self.check_expr_for_moved_vars(elem)?;
                }
            }

            HirExpr::DictLiteral(pairs) => {
                for (key, val) in pairs {
                    self.check_expr_for_moved_vars(key)?;
                    self.check_expr_for_moved_vars(val)?;
                }
            }

            HirExpr::TupleLiteral(elements) | HirExpr::SetLiteral(elements) => {
                for elem in elements {
                    self.check_expr_for_moved_vars(elem)?;
                }
            }

            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                for (_, val) in fields {
                    self.check_expr_for_moved_vars(val)?;
                }
            }

            HirExpr::Conditional(cond, then_expr, else_expr) => {
                self.check_expr_for_moved_vars(cond)?;
                self.check_expr_for_moved_vars(then_expr)?;
                self.check_expr_for_moved_vars(else_expr)?;
            }

            HirExpr::StoreVar(_, val) => {
                self.check_expr_for_moved_vars(val)?;
            }

            HirExpr::Borrow(target, _)
            | HirExpr::BorrowImmut(target)
            | HirExpr::BorrowMut(target) => {
                self.check_expr_for_moved_vars(target)?;
            }

            HirExpr::Deref(target) => {
                self.check_expr_for_moved_vars(target)?;
            }

            HirExpr::Free(target) => {
                self.check_expr_for_moved_vars(target)?;
            }

            HirExpr::Move(target) => {
                self.check_expr_for_moved_vars(target)?;
            }

            HirExpr::Await(target)
            | HirExpr::Spawn(target)
            | HirExpr::Share(target)
            | HirExpr::Downgrade(target) => {
                self.check_expr_for_moved_vars(target)?;
            }

            HirExpr::Range(start, end, _) => {
                self.check_expr_for_moved_vars(start)?;
                self.check_expr_for_moved_vars(end)?;
            }

            HirExpr::SetMember(obj, _, val) => {
                self.check_expr_for_moved_vars(obj)?;
                self.check_expr_for_moved_vars(val)?;
            }

            HirExpr::NewInstance(_, args) => {
                for arg in args {
                    self.check_expr_for_moved_vars(arg)?;
                }
            }

            HirExpr::Match(expr, arms) => {
                self.check_expr_for_moved_vars(expr)?;
                for (_, arm_expr) in arms {
                    self.check_expr_for_moved_vars(arm_expr)?;
                }
            }

            HirExpr::AssignTuple(_names, target) => {
                self.check_expr_for_moved_vars(target)?;
            }

            HirExpr::AssignObject(_properties, target) => {
                self.check_expr_for_moved_vars(target)?;
            }

            // Literals and other non-variable expressions don't use variables
            _ => {}
        }

        Ok(())
    }

    /// Process function body for ownership tracking
    pub(super) fn process_function_ownership(
        &mut self,
        func: &HirFunction,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        // Set function context for better error messages
        self.set_current_function(Some(func.name.clone()));

        let mut scope_id = 0;
        for (idx, stmt) in func.body.iter().enumerate() {
            // Map statement index to approximate line (index + 1 as placeholder)
            self.map_stmt_line(idx, idx + 1);
            self.process_stmt_ownership(stmt, &mut scope_id)?;
        }

        self.set_current_function(None);
        Ok(())
    }

    /// Process class methods for ownership tracking
    pub(super) fn process_class_ownership(
        &mut self,
        class: &HirClass,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        for method in &class.methods {
            let mut scope_id = 0;
            for stmt in method.body.iter() {
                self.process_stmt_ownership(stmt, &mut scope_id)?;
            }
        }

        for method in &class.static_methods {
            let mut scope_id = 0;
            for stmt in method.body.iter() {
                self.process_stmt_ownership(stmt, &mut scope_id)?;
            }
        }

        Ok(())
    }

    /// Check expression for ownership violations
    pub(super) fn check_expr_ownership(
        &mut self,
        expr: &HirExpr,
        owner: &str,
        _scope_id: &mut usize,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        // Check for use of moved variables in initialization
        self.check_expr_for_moved_vars(expr)?;

        // Track if this is a move operation for string/object assignment
        if let HirExpr::LoadVar(var_name) = expr {
            // For string assignments (non-Copy types), mark source as moved
            // Integers and other Copy types don't move
            if var_name != owner {
                // Check if the source variable is a Copy type
                let is_copy = self
                    .ownership_graph
                    .get(var_name)
                    .map(|n| n.is_copy_type)
                    .unwrap_or(false);

                // Only mark as moved if it's NOT a Copy type
                if !is_copy {
                    let current_location = self.get_source_location(0);
                    if let Some(node) = self.ownership_graph.get_mut(var_name) {
                        if node.state != OwnershipState::Moved {
                            node.state = OwnershipState::Moved;
                            node.location = current_location;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if expression is a Copy type literal
    pub(super) fn is_literal_copy_type(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Literal(lit) => match lit {
                HirLiteral::Int(_)
                | HirLiteral::I8(_)
                | HirLiteral::I16(_)
                | HirLiteral::I32(_)
                | HirLiteral::I64(_)
                | HirLiteral::U8(_)
                | HirLiteral::U16(_)
                | HirLiteral::U32(_)
                | HirLiteral::U64(_)
                | HirLiteral::Float(_)
                | HirLiteral::F32(_)
                | HirLiteral::F64(_)
                | HirLiteral::Bool(_)
                | HirLiteral::Char(_) => true,
                _ => false,
            },
            _ => false,
        }
    }

    /// Best-effort type inference
    pub(super) fn infer_type(&self, expr: &HirExpr) -> Option<HirType> {
        match expr {
            HirExpr::NewInstance(name, _) => Some(HirType::Class(name.clone())),
            HirExpr::LoadVar(name) => self.var_types.get(name).cloned(),
            HirExpr::Literal(lit) => match lit {
                HirLiteral::Int(_) | HirLiteral::I32(_) | HirLiteral::I64(_) => Some(HirType::Int),
                HirLiteral::Float(_) | HirLiteral::F32(_) | HirLiteral::F64(_) => {
                    Some(HirType::Float)
                }
                HirLiteral::Bool(_) => Some(HirType::Bool),
                HirLiteral::String(_) => Some(HirType::String),
                HirLiteral::Char(_) => Some(HirType::Char),
                _ => None,
            },
            HirExpr::Borrow(inner, is_mut) => self
                .infer_type(inner)
                .map(|t| HirType::Borrow(Box::new(t), *is_mut)),
            HirExpr::BorrowMut(inner) => self
                .infer_type(inner)
                .map(|t| HirType::Borrow(Box::new(t), true)),
            HirExpr::BorrowImmut(inner) => self
                .infer_type(inner)
                .map(|t| HirType::Borrow(Box::new(t), false)),
            HirExpr::BinaryOp(left, _, right) => {
                let lty = self.infer_type(left);
                let rty = self.infer_type(right);
                match (lty, rty) {
                    (Some(HirType::Int), Some(HirType::Int)) => Some(HirType::Int),
                    (Some(HirType::Float), _) | (_, Some(HirType::Float)) => Some(HirType::Float),
                    (Some(HirType::Int), _) | (_, Some(HirType::Int)) => Some(HirType::Int),
                    _ => Some(HirType::Int), // fallback
                }
            }
            _ => None,
        }
    }
}
