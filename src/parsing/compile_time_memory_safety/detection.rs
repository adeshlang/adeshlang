//! Data race and memory leak detection
//!
//! This module implements:
//! - Concurrent mutable access detection
//! - Reference cycle detection
//! - Spawn/thread safety checking

use super::error::CompileTimeMemoryError;
use crate::parsing::hir::{HirExpr, HirFunction, HirModule, HirStmt, HirType};
use std::collections::{HashMap, HashSet};

impl super::CompileTimeMemorySafety {
    /// Detect potential data races
    pub(super) fn detect_data_races(
        &mut self,
        module: &HirModule,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        for func in &module.functions {
            // Track variables accessed in spawn/thread contexts for this function only
            let mut spawn_accesses: HashMap<String, Vec<(String, bool)>> = HashMap::new(); // var -> [(context, is_mutable)]

            self.scan_for_data_races(func, &mut spawn_accesses)?;

            // Check for potential races: same var accessed mutably from multiple contexts within this function
            for (var_name, accesses) in &spawn_accesses {
                let mut mutable_contexts: Vec<&String> = Vec::new();
                let mut any_access_contexts: Vec<&String> = Vec::new();

                for (context, is_mutable) in accesses {
                    any_access_contexts.push(context);
                    if *is_mutable {
                        mutable_contexts.push(context);
                    }
                }

                // Data race: mutable access from one context + any access from another
                // We use a HashSet to count distinct contexts to avoid false positives
                // where a variable is accessed multiple times within the same single-threaded context.
                let distinct_contexts: HashSet<&String> =
                    any_access_contexts.iter().cloned().collect();

                if !mutable_contexts.is_empty() && distinct_contexts.len() > 1 {
                    let primary_mut_context = mutable_contexts[0];
                    let other_contexts: Vec<&String> = distinct_contexts
                        .iter()
                        .filter(|&&c| c != primary_mut_context)
                        .cloned()
                        .collect();

                    self.errors.push(CompileTimeMemoryError::PotentialDataRace {
                        variable: var_name.clone(),
                        locations: vec![self.get_source_location(0)],
                        thread_context: format!(
                            "Variable '{}' is accessed mutably in '{}' and also accessed in: {}",
                            var_name,
                            primary_mut_context,
                            other_contexts
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Scan function for data race patterns
    pub(super) fn scan_for_data_races(
        &mut self,
        func: &HirFunction,
        spawn_accesses: &mut HashMap<String, Vec<(String, bool)>>,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        let context = format!("fn:{}", func.name);

        for stmt in func.body.iter() {
            self.scan_stmt_for_races(stmt, &context, spawn_accesses)?;
        }

        Ok(())
    }

    /// Scan statement for data race patterns
    pub(super) fn scan_stmt_for_races(
        &mut self,
        stmt: &HirStmt,
        current_context: &str,
        spawn_accesses: &mut HashMap<String, Vec<(String, bool)>>,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match stmt {
            HirStmt::Expr(expr) => {
                self.scan_expr_for_races(expr, current_context, spawn_accesses)?;
            }
            HirStmt::Let {
                init: Some(expr), ..
            } => {
                self.scan_expr_for_races(expr, current_context, spawn_accesses)?;
            }
            HirStmt::Assign { target, value, .. } => {
                // Mutable access on target
                if let HirExpr::LoadVar(var_name) = target {
                    spawn_accesses
                        .entry(var_name.clone())
                        .or_default()
                        .push((current_context.to_string(), true));
                }
                self.scan_expr_for_races(value, current_context, spawn_accesses)?;
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.scan_stmt_for_races(s, current_context, spawn_accesses)?;
                }
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.scan_expr_for_races(cond, current_context, spawn_accesses)?;
                self.scan_stmt_for_races(then_branch, current_context, spawn_accesses)?;
                if let Some(else_stmt) = else_branch {
                    self.scan_stmt_for_races(else_stmt, current_context, spawn_accesses)?;
                }
            }
            HirStmt::While { cond, body } => {
                self.scan_expr_for_races(cond, current_context, spawn_accesses)?;
                self.scan_stmt_for_races(body, current_context, spawn_accesses)?;
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.scan_expr_for_races(iter, current_context, spawn_accesses)?;
                self.scan_stmt_for_races(body, current_context, spawn_accesses)?;
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.scan_stmt_for_races(try_block, current_context, spawn_accesses)?;
                self.scan_stmt_for_races(catch_block, current_context, spawn_accesses)?;
            }
            HirStmt::Return(Some(expr)) => {
                self.scan_expr_for_races(expr, current_context, spawn_accesses)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Scan expression for data race patterns
    pub(super) fn scan_expr_for_races(
        &mut self,
        expr: &HirExpr,
        current_context: &str,
        spawn_accesses: &mut HashMap<String, Vec<(String, bool)>>,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match expr {
            HirExpr::LoadVar(var_name) => {
                // Read access
                spawn_accesses
                    .entry(var_name.clone())
                    .or_default()
                    .push((current_context.to_string(), false));
            }
            HirExpr::Call(func_expr, args, _) => {
                // Check if this is a spawn call
                if let HirExpr::LoadVar(func_name) = &**func_expr {
                    if func_name == "spawn"
                        || func_name == "thread_spawn"
                        || func_name == "parallel"
                    {
                        // Create new context for spawned code
                        let spawn_context = format!("{}:spawn", current_context);

                        // Check arguments (especially closure bodies)
                        for arg in args {
                            self.scan_expr_for_races(arg, &spawn_context, spawn_accesses)?;
                        }
                        return Ok(());
                    }
                }

                // Regular call - scan function and args
                self.scan_expr_for_races(func_expr, current_context, spawn_accesses)?;
                for arg in args {
                    self.scan_expr_for_races(arg, current_context, spawn_accesses)?;
                }
            }
            HirExpr::Lambda(_, body, _) => {
                // Scan lambda body in current context
                for stmt in body.iter() {
                    self.scan_stmt_for_races(stmt, current_context, spawn_accesses)?;
                }
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.scan_expr_for_races(left, current_context, spawn_accesses)?;
                self.scan_expr_for_races(right, current_context, spawn_accesses)?;
            }
            HirExpr::BorrowMut(inner) => {
                // Mutable borrow is a mutable access
                if let HirExpr::LoadVar(var_name) = &**inner {
                    spawn_accesses
                        .entry(var_name.clone())
                        .or_default()
                        .push((current_context.to_string(), true));
                }
                self.scan_expr_for_races(inner, current_context, spawn_accesses)?;
            }
            HirExpr::Borrow(inner, is_mut) => {
                if let HirExpr::LoadVar(var_name) = &**inner {
                    spawn_accesses
                        .entry(var_name.clone())
                        .or_default()
                        .push((current_context.to_string(), *is_mut));
                }
                self.scan_expr_for_races(inner, current_context, spawn_accesses)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Detect memory leaks (reference cycles)
    pub(super) fn detect_memory_leaks(
        &mut self,
        module: &HirModule,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        // First, build the reference graph from the module
        self.build_reference_graph(module);

        // Detect cycles in reference graph using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path_stack: Vec<String> = Vec::new();

        for node in self.reference_graph.keys() {
            if !visited.contains(node) {
                if let Some(cycle) = self.detect_cycle_dfs_with_path(
                    node,
                    &mut visited,
                    &mut rec_stack,
                    &mut path_stack,
                ) {
                    // Found a cycle - report it as a potential memory leak
                    self.errors.push(CompileTimeMemoryError::PotentialMemoryLeak {
                        cycle: cycle.clone(),
                        location: self.get_source_location(0),
                        suggestion: format!(
                            "Consider using Weak<T> instead of Shared<T> for one of the references in the cycle: {}",
                            cycle.join(" -> ")
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Build reference graph from HIR module
    pub(super) fn build_reference_graph(&mut self, module: &HirModule) {
        for func in &module.functions {
            for stmt in func.body.iter() {
                self.scan_stmt_for_refs(stmt);
            }
        }

        for class in &module.classes {
            for method in &class.methods {
                for stmt in method.body.iter() {
                    self.scan_stmt_for_refs(stmt);
                }
            }
        }
    }

    /// Scan statement for reference relationships
    pub(super) fn scan_stmt_for_refs(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Let {
                name,
                init: Some(expr),
                ty,
                ..
            } => {
                // Check if assigning a Shared/Rc type
                let is_shared = ty
                    .as_ref()
                    .map(|t| matches!(t, HirType::Shared(_)))
                    .unwrap_or(false);

                if is_shared {
                    // Track references from this variable to others
                    let refs = self.extract_referenced_vars(expr);
                    for ref_var in refs {
                        self.reference_graph
                            .entry(name.clone())
                            .or_default()
                            .push(ref_var);
                    }
                }

                self.scan_expr_for_refs(expr);
            }
            HirStmt::Assign { target, value, .. } => {
                if let HirExpr::LoadVar(var_name) = target {
                    // Check if this is a Shared type assignment
                    if let Some(ty) = self.var_types.get(var_name) {
                        if matches!(ty, HirType::Shared(_)) {
                            let refs = self.extract_referenced_vars(value);
                            for ref_var in refs {
                                self.reference_graph
                                    .entry(var_name.clone())
                                    .or_default()
                                    .push(ref_var);
                            }
                        }
                    }
                }
                self.scan_expr_for_refs(value);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.scan_stmt_for_refs(s);
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.scan_stmt_for_refs(then_branch);
                if let Some(else_stmt) = else_branch {
                    self.scan_stmt_for_refs(else_stmt);
                }
            }
            _ => {}
        }
    }

    /// Scan expression for reference relationships
    pub(super) fn scan_expr_for_refs(&mut self, expr: &HirExpr) {
        match expr {
            HirExpr::Share(inner) => {
                // Sharing creates a Shared reference
                self.scan_expr_for_refs(inner);
            }
            HirExpr::Call(func, args, _) => {
                self.scan_expr_for_refs(func);
                for arg in args {
                    self.scan_expr_for_refs(arg);
                }
            }
            HirExpr::Lambda(_, body, _) => {
                for stmt in body.iter() {
                    self.scan_stmt_for_refs(stmt);
                }
            }
            _ => {}
        }
    }

    /// Extract referenced variable names from an expression
    pub(super) fn extract_referenced_vars(&self, expr: &HirExpr) -> Vec<String> {
        let mut refs = Vec::new();
        match expr {
            HirExpr::LoadVar(name) => {
                refs.push(name.clone());
            }
            HirExpr::Share(inner) => {
                refs.extend(self.extract_referenced_vars(inner));
            }
            HirExpr::Call(_, args, _) => {
                for arg in args {
                    refs.extend(self.extract_referenced_vars(arg));
                }
            }
            HirExpr::MemberAccess(obj, _) => {
                refs.extend(self.extract_referenced_vars(obj));
            }
            _ => {}
        }
        refs
    }

    /// DFS cycle detection that returns the actual cycle path
    pub(super) fn detect_cycle_dfs_with_path(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = self.reference_graph.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    if let Some(cycle) =
                        self.detect_cycle_dfs_with_path(neighbor, visited, rec_stack, path)
                    {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(neighbor) {
                    // Found cycle - extract it from path
                    let cycle_start = path.iter().position(|n| n == neighbor).unwrap_or(0);
                    let mut cycle: Vec<String> = path[cycle_start..].to_vec();
                    cycle.push(neighbor.clone()); // Complete the cycle
                    return Some(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
        None
    }

    /// Check concurrency safety: validate Send/Sync requirements
    pub(super) fn check_concurrency_safety(
        &mut self,
        module: &HirModule,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        for func in &module.functions {
            self.check_function_concurrency(func)?;
        }
        Ok(())
    }

    /// Check a function for concurrency safety violations
    pub(super) fn check_function_concurrency(
        &mut self,
        func: &HirFunction,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        for stmt in func.body.iter() {
            self.check_stmt_concurrency(stmt)?;
        }
        Ok(())
    }

    /// Check a statement for concurrency safety violations
    pub(super) fn check_stmt_concurrency(
        &mut self,
        stmt: &HirStmt,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match stmt {
            HirStmt::Expr(expr) => {
                self.check_expr_concurrency(expr)?;
            }
            HirStmt::Let {
                init: Some(expr), ..
            } => {
                self.check_expr_concurrency(expr)?;
            }
            HirStmt::Assign { value: expr, .. } => {
                self.check_expr_concurrency(expr)?;
            }
            HirStmt::Return(Some(expr)) => {
                self.check_expr_concurrency(expr)?;
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.check_expr_concurrency(cond)?;
                self.check_stmt_concurrency(then_branch)?;
                if let Some(else_stmt) = else_branch {
                    self.check_stmt_concurrency(else_stmt)?;
                }
            }
            HirStmt::While { cond, body } => {
                self.check_expr_concurrency(cond)?;
                self.check_stmt_concurrency(body)?;
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.check_expr_concurrency(iter)?;
                self.check_stmt_concurrency(body)?;
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.check_stmt_concurrency(try_block)?;
                self.check_stmt_concurrency(catch_block)?;
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.check_stmt_concurrency(s)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Check an expression for concurrency safety violations
    pub(super) fn check_expr_concurrency(
        &mut self,
        expr: &HirExpr,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        match expr {
            // spawn(|| { ... }) - check closure captures are Send
            HirExpr::Call(func, args, _) => {
                if is_thread_spawn_callee(func) {
                    if let Some(closure_expr) = args.first() {
                        self.validate_spawn_closure(closure_expr)?;
                    }
                    if args.len() > 1 {
                        self.validate_spawn_closure(&args[args.len() - 1])?;
                    }
                }

                self.check_expr_concurrency(func)?;
                for arg in args {
                    self.check_expr_concurrency(arg)?;
                }
            }

            // Recursively check other expression types
            HirExpr::BinaryOp(left, _, right) => {
                self.check_expr_concurrency(left)?;
                self.check_expr_concurrency(right)?;
            }
            HirExpr::UnaryOp(_, operand) => {
                self.check_expr_concurrency(operand)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Validate that a closure passed to spawn() only captures thread-safe types
    pub(super) fn validate_spawn_closure(
        &mut self,
        closure_expr: &HirExpr,
    ) -> Result<(), Vec<CompileTimeMemoryError>> {
        // Check that all variables referenced in the closure are safe to share across threads
        let captured_vars = self.extract_captured_variables(closure_expr);

        for var_name in captured_vars {
            // Look up the variable's type to determine if it is thread-safe
            if let Some(node) = self.ownership_graph.get(&var_name) {
                let var_type = self.var_types.get(&var_name);

                // Determine if this type is NOT safe to cross threads:
                // - Borrow references to non-Copy types cannot cross threads
                // - Non-ARC shared types cannot cross threads safely
                let is_not_thread_safe = match var_type {
                    Some(HirType::Borrow(_inner, _)) => {
                        // Borrows of non-Copy types cannot cross threads
                        !node.is_copy_type
                    }
                    Some(HirType::BorrowMut(_)) | Some(HirType::BorrowImmut(_)) => true,
                    // ARC types (share/strong/weak) are safe for concurrent access
                    Some(HirType::Shared(_)) | Some(HirType::Weak(_)) => false,
                    // Primitive Copy types are always thread-safe
                    Some(
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
                        | HirType::F64,
                    ) => false,
                    // String is thread-safe
                    Some(HirType::String) => false,
                    // Unknown types: conservatively assume not thread-safe to be strict
                    Some(_) => !node.is_copy_type,
                    None => false, // If no type info, don't error (avoid false positives)
                };

                if is_not_thread_safe {
                    // ERROR: variable cannot safely cross thread boundaries
                    self.errors.push(CompileTimeMemoryError::PotentialDataRace {
                        variable: var_name.clone(),
                        locations: vec![self.get_source_location(0)],
                        thread_context: format!(
                            "variable '{}' is used in a spawned task but cannot be safely shared across threads\n\
                             help: use `share` or `strong` for shared ownership across threads\n\
                             help: for mutable data, protect access with `Mutex` or `RwLock`",
                            var_name
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Extract variables captured by a closure (simplified)
    pub(super) fn extract_captured_variables(&self, expr: &HirExpr) -> Vec<String> {
        let mut vars = Vec::new();
        self.collect_var_refs(expr, &mut vars);
        vars
    }

    /// Recursively collect variable references
    pub(super) fn collect_var_refs(&self, expr: &HirExpr, vars: &mut Vec<String>) {
        match expr {
            HirExpr::LoadVar(name) => {
                vars.push(name.clone());
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.collect_var_refs(left, vars);
                self.collect_var_refs(right, vars);
            }
            HirExpr::UnaryOp(_, operand) => {
                self.collect_var_refs(operand, vars);
            }
            HirExpr::Call(func, args, _) => {
                self.collect_var_refs(func, vars);
                for arg in args {
                    self.collect_var_refs(arg, vars);
                }
            }
            HirExpr::Lambda(_, body, _) => {
                for stmt in body.iter() {
                    self.collect_stmt_var_refs(stmt, vars);
                }
            }
            _ => {}
        }
    }

    /// Collect variable references in statements
    pub(super) fn collect_stmt_var_refs(&self, stmt: &HirStmt, vars: &mut Vec<String>) {
        match stmt {
            HirStmt::Expr(expr) => self.collect_var_refs(expr, vars),
            HirStmt::Let {
                init: Some(expr), ..
            } => self.collect_var_refs(expr, vars),
            HirStmt::Assign { value: expr, .. } => self.collect_var_refs(expr, vars),
            HirStmt::Return(Some(expr)) => self.collect_var_refs(expr, vars),
            _ => {}
        }
    }
}

fn is_thread_spawn_callee(func: &HirExpr) -> bool {
    match func {
        HirExpr::LoadVar(name) => {
            name == "spawn" || name == "thread_spawn" || name == "thread_scope"
        }
        HirExpr::MemberAccess(obj, method) => {
            let method_ok = method == "spawn"
                || method == "spawn_named"
                || method == "scope"
                || method == "execute"
                || method == "submit";
            if !method_ok {
                return false;
            }
            matches!(
                &**obj,
                HirExpr::LoadVar(n) if n == "thread"
                    || n == "Thread"
                    || n == "scope"
                    || n == "pool"
                    || n == "ThreadPool"
            )
        }
        _ => false,
    }
}
