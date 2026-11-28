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
        // Track which local variables are declared, which borrow aliases point
        // to local variables, and which borrowed locals are returned.
        let mut local_vars: FastSet<String> = FastSet::default();
        let mut borrow_aliases: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut borrowed_returns: Vec<(String, String)> = Vec::new(); // (ref_name, source_var)

        // Collect all local variables declared in the function
        for stmt in func.body.iter() {
            self.collect_local_vars(stmt, &mut local_vars);
        }

        // Check for returning references to local variables (directly, via borrow
        // aliases, inside composite return values, or via explicit `&local` in
        // an escaped closure).
        for stmt in func.body.iter() {
            self.check_stmt_lifetimes_with_aliases(
                stmt,
                &local_vars,
                &mut borrow_aliases,
                &mut borrowed_returns,
            )?;
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
        let mut borrow_aliases: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut borrowed_returns: Vec<(String, String)> = Vec::new();

        for stmt in method.body.iter() {
            self.collect_local_vars(stmt, &mut local_vars);
        }

        for stmt in method.body.iter() {
            self.check_stmt_lifetimes_with_aliases(
                stmt,
                &local_vars,
                &mut borrow_aliases,
                &mut borrowed_returns,
            )?;
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
            HirStmt::While { body, .. }
            | HirStmt::ForIn { body, .. }
            | HirStmt::Defer(body)
            | HirStmt::Unsafe(body)
            | HirStmt::Region { body, .. } => {
                self.collect_local_vars(body, local_vars);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_local_vars(try_block, local_vars);
                self.collect_local_vars(catch_block, local_vars);
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
        let mut aliases = std::collections::HashMap::new();
        self.check_stmt_lifetimes_with_aliases(stmt, local_vars, &mut aliases, borrowed_returns)
    }

    fn check_stmt_lifetimes_with_aliases(
        &mut self,
        stmt: &HirStmt,
        local_vars: &FastSet<String>,
        borrow_aliases: &mut std::collections::HashMap<String, String>,
        borrowed_returns: &mut Vec<(String, String)>,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match stmt {
            HirStmt::Let {
                name,
                init: Some(expr),
                ..
            } => {
                if let Some((_ref_name, source_var)) =
                    self.extract_borrow_source_with_aliases(expr, borrow_aliases)
                {
                    if local_vars.contains(&source_var) {
                        borrow_aliases.insert(name.clone(), source_var);
                    }
                } else {
                    borrow_aliases.remove(name);
                }
            }
            HirStmt::Assign {
                target: HirExpr::LoadVar(name),
                value,
                ..
            } => {
                if let Some((_ref_name, source_var)) =
                    self.extract_borrow_source_with_aliases(value, borrow_aliases)
                {
                    if local_vars.contains(&source_var) {
                        borrow_aliases.insert(name.clone(), source_var);
                    }
                } else {
                    borrow_aliases.remove(name);
                }
            }
            HirStmt::Return(Some(expr)) => {
                self.collect_escaped_borrows_in_return(
                    expr,
                    local_vars,
                    borrow_aliases,
                    borrowed_returns,
                );
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.check_stmt_lifetimes_with_aliases(
                        s,
                        local_vars,
                        borrow_aliases,
                        borrowed_returns,
                    )?;
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.check_stmt_lifetimes_with_aliases(
                    then_branch,
                    local_vars,
                    borrow_aliases,
                    borrowed_returns,
                )?;
                if let Some(else_stmt) = else_branch {
                    self.check_stmt_lifetimes_with_aliases(
                        else_stmt,
                        local_vars,
                        borrow_aliases,
                        borrowed_returns,
                    )?;
                }
            }
            HirStmt::While { body, .. }
            | HirStmt::ForIn { body, .. }
            | HirStmt::Defer(body)
            | HirStmt::Unsafe(body)
            | HirStmt::Region { body, .. } => {
                self.check_stmt_lifetimes_with_aliases(
                    body,
                    local_vars,
                    borrow_aliases,
                    borrowed_returns,
                )?;
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.check_stmt_lifetimes_with_aliases(
                    try_block,
                    local_vars,
                    borrow_aliases,
                    borrowed_returns,
                )?;
                self.check_stmt_lifetimes_with_aliases(
                    catch_block,
                    local_vars,
                    borrow_aliases,
                    borrowed_returns,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn collect_escaped_borrows_in_return(
        &self,
        expr: &HirExpr,
        local_vars: &FastSet<String>,
        borrow_aliases: &std::collections::HashMap<String, String>,
        borrowed_returns: &mut Vec<(String, String)>,
    ) {
        if let Some((ref_name, source_var)) =
            self.extract_borrow_source_with_aliases(expr, borrow_aliases)
        {
            if local_vars.contains(&source_var) {
                borrowed_returns.push((ref_name, source_var));
                return;
            }
        }
        match expr {
            HirExpr::ArrayLiteral(items)
            | HirExpr::SetLiteral(items)
            | HirExpr::TupleLiteral(items) => {
                for item in items {
                    self.collect_escaped_borrows_in_return(
                        item,
                        local_vars,
                        borrow_aliases,
                        borrowed_returns,
                    );
                }
            }
            HirExpr::StructLiteral(_, fields) | HirExpr::ObjectLiteral(fields) => {
                for (_, val) in fields {
                    self.collect_escaped_borrows_in_return(
                        val,
                        local_vars,
                        borrow_aliases,
                        borrowed_returns,
                    );
                }
            }
            HirExpr::Call(callee, args, _) => {
                // Only variant constructors (like `Some(&x)`, `Ok(&x)`) or
                // expressions that embed a borrowed local in an escaped value
                // can carry the borrow out directly; check explicit borrows in
                // variant constructor payloads.
                if let HirExpr::LoadVar(name) = &**callee {
                    let is_variant = matches!(name.as_str(), "Some" | "Ok" | "Err");
                    if is_variant {
                        for arg in args {
                            self.collect_escaped_borrows_in_return(
                                arg,
                                local_vars,
                                borrow_aliases,
                                borrowed_returns,
                            );
                        }
                    }
                }
            }
            HirExpr::Conditional(_, t, e) => {
                self.collect_escaped_borrows_in_return(
                    t,
                    local_vars,
                    borrow_aliases,
                    borrowed_returns,
                );
                self.collect_escaped_borrows_in_return(
                    e,
                    local_vars,
                    borrow_aliases,
                    borrowed_returns,
                );
            }
            HirExpr::Match(_, arms) => {
                for (_, arm) in arms {
                    self.collect_escaped_borrows_in_return(
                        arm,
                        local_vars,
                        borrow_aliases,
                        borrowed_returns,
                    );
                }
            }
            HirExpr::Lambda(_, body, _) => {
                // If an escaped closure explicitly borrows an outer local (`&x`
                // / `&mut x`), the reference would dangle when this frame exits.
                let mut inner_locals = FastSet::default();
                for s in body.iter() {
                    self.collect_local_vars(s, &mut inner_locals);
                }
                for s in body.iter() {
                    self.collect_explicit_outer_borrows_in_stmt(
                        s,
                        local_vars,
                        &inner_locals,
                        borrow_aliases,
                        borrowed_returns,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_explicit_outer_borrows_in_stmt(
        &self,
        stmt: &HirStmt,
        outer_locals: &FastSet<String>,
        inner_locals: &FastSet<String>,
        borrow_aliases: &std::collections::HashMap<String, String>,
        borrowed_returns: &mut Vec<(String, String)>,
    ) {
        match stmt {
            HirStmt::Let {
                init: Some(expr), ..
            }
            | HirStmt::Assign { value: expr, .. }
            | HirStmt::Expr(expr)
            | HirStmt::Return(Some(expr))
            | HirStmt::Throw(expr) => {
                self.collect_explicit_outer_borrows_in_expr(
                    expr,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_explicit_outer_borrows_in_stmt(
                        s,
                        outer_locals,
                        inner_locals,
                        borrow_aliases,
                        borrowed_returns,
                    );
                }
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.collect_explicit_outer_borrows_in_expr(
                    cond,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
                self.collect_explicit_outer_borrows_in_stmt(
                    then_branch,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
                if let Some(e) = else_branch {
                    self.collect_explicit_outer_borrows_in_stmt(
                        e,
                        outer_locals,
                        inner_locals,
                        borrow_aliases,
                        borrowed_returns,
                    );
                }
            }
            HirStmt::While { cond, body } => {
                self.collect_explicit_outer_borrows_in_expr(
                    cond,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
                self.collect_explicit_outer_borrows_in_stmt(
                    body,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
            }
            _ => {}
        }
    }

    fn collect_explicit_outer_borrows_in_expr(
        &self,
        expr: &HirExpr,
        outer_locals: &FastSet<String>,
        inner_locals: &FastSet<String>,
        borrow_aliases: &std::collections::HashMap<String, String>,
        borrowed_returns: &mut Vec<(String, String)>,
    ) {
        match expr {
            HirExpr::Borrow(inner, _)
            | HirExpr::BorrowMut(inner)
            | HirExpr::BorrowImmut(inner) => {
                if let HirExpr::LoadVar(var_name) = &**inner {
                    if outer_locals.contains(var_name) && !inner_locals.contains(var_name) {
                        borrowed_returns.push((var_name.clone(), var_name.clone()));
                    }
                }
            }
            HirExpr::LoadVar(var_name) => {
                if let Some(source) = borrow_aliases.get(var_name) {
                    if outer_locals.contains(source) && !inner_locals.contains(var_name) {
                        borrowed_returns.push((var_name.clone(), source.clone()));
                    }
                }
            }
            HirExpr::Call(f, args, _) => {
                self.collect_explicit_outer_borrows_in_expr(
                    f,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
                for a in args {
                    self.collect_explicit_outer_borrows_in_expr(
                        a,
                        outer_locals,
                        inner_locals,
                        borrow_aliases,
                        borrowed_returns,
                    );
                }
            }
            HirExpr::BinaryOp(l, _, r) => {
                self.collect_explicit_outer_borrows_in_expr(
                    l,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
                self.collect_explicit_outer_borrows_in_expr(
                    r,
                    outer_locals,
                    inner_locals,
                    borrow_aliases,
                    borrowed_returns,
                );
            }
            _ => {}
        }
    }

    fn extract_borrow_source_with_aliases(
        &self,
        expr: &HirExpr,
        borrow_aliases: &std::collections::HashMap<String, String>,
    ) -> Option<(String, String)> {
        match expr {
            HirExpr::Borrow(inner, _) | HirExpr::BorrowMut(inner) | HirExpr::BorrowImmut(inner) => {
                if let HirExpr::LoadVar(var_name) = &**inner {
                    Some((var_name.clone(), var_name.clone()))
                } else {
                    None
                }
            }
            HirExpr::LoadVar(var_name) => {
                if let Some(source) = borrow_aliases.get(var_name) {
                    return Some((var_name.clone(), source.clone()));
                }
                self.extract_borrow_source(expr)
            }
            _ => None,
        }
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
