//! Interprocedural Borrow and Lifetime Analysis
//!
//! Validates memory safety across function boundaries by tracking:
//! 1. Function signatures with borrow/lifetime contracts
//! 2. Reference escaping through return values
//! 3. Cross-function borrow relationships
//! 4. Closure capture lifetimes
//!
//! # Example Violations Caught
//!
//! ```adesh
//! fn leak_reference(data: &MyStruct) -> &int {
//!     return &data.field;  // ❌ ERROR: reference escapes function
//! }
//!
//! fn capture_local() -> fn() -> &int {
//!     let x = 42;
//!     return || &x;  // ❌ ERROR: closure captures reference to local
//! }
//! ```

use crate::parsing::hir::{HirExpr, HirFunction, HirModule, HirStmt, HirType};
use crate::parsing::lifetime_tracking::{FunctionSignature, LifetimeContext};
use std::collections::{HashMap, HashSet};

/// Errors from interprocedural analysis
#[derive(Debug, Clone)]
pub enum InterproceduralError {
    /// Reference escapes function boundary
    ReferenceEscape {
        function: String,
        param_name: String,
        return_location: String,
    },

    /// Closure captures reference that outlives the closure
    ClosureCaptureEscape {
        closure_location: String,
        captured_var: String,
        var_lifetime: String,
    },

    /// Function call with incompatible lifetimes
    LifetimeMismatch {
        function: String,
        param_name: String,
        expected_lifetime: String,
        actual_lifetime: String,
    },

    /// Returning reference to local variable
    ReturnLocalReference {
        function: String,
        local_var: String,
        return_location: String,
    },
}

impl InterproceduralError {
    pub fn format_error(&self) -> String {
        match self {
            InterproceduralError::ReferenceEscape {
                function,
                param_name,
                return_location,
            } => {
                format!(
                    "reference to `{}` escapes function `{}`\n\
                     note: reference returned at {}\n\
                     help: return an owned value instead; or share ownership with \
                     `share`/`strong` if multiple references are needed",
                    param_name, function, return_location
                )
            }

            InterproceduralError::ClosureCaptureEscape {
                closure_location,
                captured_var,
                var_lifetime,
            } => {
                format!(
                    "closure captures reference that outlives its scope\n\
                     note: closure at {} captures `{}`\n\
                     note: variable lifetime: {}\n\
                     help: move the value into the closure with `share`/`strong`, or restructure \
                     so the closure owns the data it needs",
                    closure_location, captured_var, var_lifetime
                )
            }

            InterproceduralError::LifetimeMismatch {
                function,
                param_name,
                expected_lifetime,
                actual_lifetime,
            } => {
                format!(
                    "lifetime mismatch in call to `{}`\n\
                     note: parameter `{}` expects {}\n\
                     note: but argument has {}\n\
                     help: ensure the argument outlives the parameter, or return an owned value",
                    function, param_name, expected_lifetime, actual_lifetime
                )
            }

            InterproceduralError::ReturnLocalReference {
                function,
                local_var,
                return_location,
            } => {
                format!(
                    "cannot return reference to local variable `{}`\n\
                     note: in function `{}` at {}\n\
                     help: return an owned value instead; local variables do not outlive \
                     the function scope",
                    local_var, function, return_location
                )
            }
        }
    }
}

/// Interprocedural analyzer
pub struct InterproceduralAnalyzer {
    /// Lifetime context for tracking lifetimes
    lifetime_ctx: LifetimeContext,

    /// Errors found during analysis
    errors: Vec<InterproceduralError>,

    /// Map from function name to local variables (for escape detection)
    function_locals: HashMap<String, HashSet<String>>,

    /// Map from function name to parameters
    function_params: HashMap<String, HashSet<String>>,
}

impl InterproceduralAnalyzer {
    pub fn new() -> Self {
        InterproceduralAnalyzer {
            lifetime_ctx: LifetimeContext::new(),
            errors: Vec::new(),
            function_locals: HashMap::new(),
            function_params: HashMap::new(),
        }
    }

    /// Analyze a module for interprocedural safety violations
    pub fn analyze(&mut self, module: &HirModule) -> Result<(), Vec<InterproceduralError>> {
        // Phase 1: Collect function signatures
        for func in &module.functions {
            self.collect_function_signature(func);
        }

        // Phase 2: Validate each function
        for func in &module.functions {
            self.validate_function(func)?;
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Collect function signature for later validation
    fn collect_function_signature(&mut self, func: &HirFunction) {
        // Create lifetime for each parameter
        let mut param_lifetimes = Vec::new();
        let mut param_names = HashSet::new();

        for (param_name, param_type_opt, _default) in &func.params {
            param_names.insert(param_name.clone());

            // If parameter is a reference, assign it a lifetime
            if let Some(param_type) = param_type_opt {
                if Self::is_reference_type(param_type) {
                    let lt = self.lifetime_ctx.fresh_lifetime();
                    param_lifetimes.push((param_name.clone(), lt));
                    self.lifetime_ctx
                        .set_variable_lifetime(param_name.clone(), lt);
                }
            }
        }

        self.function_params.insert(func.name.clone(), param_names);

        // Determine return lifetime
        let return_lifetime = if let Some(ret_type) = &func.ret_type {
            if Self::is_reference_type(ret_type) {
                // Return reference must come from one of the parameters
                // For now, use first parameter's lifetime (simplified)
                param_lifetimes.first().map(|(_, lt)| *lt)
            } else {
                None
            }
        } else {
            None
        };

        let sig = FunctionSignature {
            name: func.name.clone(),
            lifetime_params: param_lifetimes.iter().map(|(_, lt)| *lt).collect(),
            param_lifetimes,
            return_lifetime,
            lifetime_bounds: Vec::new(),
        };

        self.lifetime_ctx.register_function(sig);
    }

    /// Check if a type is a reference
    fn is_reference_type(ty: &HirType) -> bool {
        matches!(
            ty,
            HirType::Borrow(_, _) | HirType::BorrowImmut(_) | HirType::BorrowMut(_)
        )
    }

    /// Validate a function for safety violations
    fn validate_function(&mut self, func: &HirFunction) -> Result<(), Vec<InterproceduralError>> {
        // Collect local variables
        let mut locals = HashSet::new();
        self.collect_locals(&func.body, &mut locals);
        self.function_locals.insert(func.name.clone(), locals);

        // Validate return statements
        for stmt in func.body.iter() {
            self.validate_statement(stmt, &func.name)?;
        }

        Ok(())
    }

    /// Collect local variable names
    fn collect_locals(&self, body: &[HirStmt], locals: &mut HashSet<String>) {
        for stmt in body {
            self.collect_locals_from_stmt(stmt, locals);
        }
    }

    /// Collect locals from a single statement (recursive)
    fn collect_locals_from_stmt(&self, stmt: &HirStmt, locals: &mut HashSet<String>) {
        match stmt {
            HirStmt::Let { name, .. } => {
                locals.insert(name.clone());
            }
            HirStmt::LetTuple { names, .. } => {
                for name in names {
                    locals.insert(name.clone());
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_locals_from_stmt(then_branch, locals);
                if let Some(else_stmt) = else_branch {
                    self.collect_locals_from_stmt(else_stmt, locals);
                }
            }
            HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
                self.collect_locals_from_stmt(body, locals);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_locals_from_stmt(try_block, locals);
                self.collect_locals_from_stmt(catch_block, locals);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_locals_from_stmt(s, locals);
                }
            }
            _ => {}
        }
    }

    /// Validate a statement
    fn validate_statement(
        &mut self,
        stmt: &HirStmt,
        func_name: &str,
    ) -> Result<(), Vec<InterproceduralError>> {
        match stmt {
            HirStmt::Return(Some(expr)) => {
                self.validate_return_expression(expr, func_name)?;
            }

            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.validate_expression(cond, func_name)?;
                self.validate_statement(then_branch, func_name)?;
                if let Some(else_stmt) = else_branch {
                    self.validate_statement(else_stmt, func_name)?;
                }
            }

            HirStmt::While { cond, body } => {
                self.validate_expression(cond, func_name)?;
                self.validate_statement(body, func_name)?;
            }

            HirStmt::ForIn { iter, body, .. } => {
                self.validate_expression(iter, func_name)?;
                self.validate_statement(body, func_name)?;
            }

            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.validate_statement(try_block, func_name)?;
                self.validate_statement(catch_block, func_name)?;
            }

            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.validate_statement(s, func_name)?;
                }
            }

            HirStmt::Expr(expr) => {
                self.validate_expression(expr, func_name)?;
            }

            HirStmt::Let {
                init: Some(expr), ..
            } => {
                self.validate_expression(expr, func_name)?;
            }

            HirStmt::Assign { value, .. } => {
                self.validate_expression(value, func_name)?;
            }

            HirStmt::Throw(expr) => {
                self.validate_expression(expr, func_name)?;
            }

            _ => {}
        }

        Ok(())
    }

    /// Validate return expression doesn't escape local references
    fn validate_return_expression(
        &mut self,
        expr: &HirExpr,
        func_name: &str,
    ) -> Result<(), Vec<InterproceduralError>> {
        // Check if expression creates a reference to a local variable
        if let Some(var_name) = self.extract_referenced_variable(expr) {
            // Check if this is a local variable (not a parameter)
            if let Some(locals) = self.function_locals.get(func_name) {
                if locals.contains(&var_name) {
                    self.errors
                        .push(InterproceduralError::ReturnLocalReference {
                            function: func_name.to_string(),
                            local_var: var_name.clone(),
                            return_location: "return statement".to_string(),
                        });
                }
            }

            // Check if this is a field access on a parameter
            if let HirExpr::Borrow(inner, _) = expr {
                if let HirExpr::MemberAccess(obj, field) = &**inner {
                    if let HirExpr::LoadVar(obj_name) = &**obj {
                        // Check if obj_name is a parameter
                        if let Some(params) = self.function_params.get(func_name) {
                            if params.contains(obj_name) {
                                self.errors.push(InterproceduralError::ReferenceEscape {
                                    function: func_name.to_string(),
                                    param_name: obj_name.clone(),
                                    return_location: format!(
                                        "field access: {}.{}",
                                        obj_name, field
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate expression (check for closures with escaping captures and cross-function reference escapes)
    fn validate_expression(
        &mut self,
        expr: &HirExpr,
        func_name: &str,
    ) -> Result<(), Vec<InterproceduralError>> {
        match expr {
            HirExpr::Lambda(params, body, _) => {
                // Analyze closure captures
                let captures = self.analyze_closure_captures(body, params);

                // Check if any captured variable is a local reference
                if let Some(locals) = self.function_locals.get(func_name) {
                    for captured_var in &captures {
                        if locals.contains(captured_var) {
                            // Check if the captured variable is a reference type
                            if let Some(lifetime) =
                                self.lifetime_ctx.get_variable_lifetime(captured_var)
                            {
                                if !lifetime.is_static() {
                                    self.errors
                                        .push(InterproceduralError::ClosureCaptureEscape {
                                            closure_location: "lambda expression".to_string(),
                                            captured_var: captured_var.clone(),
                                            var_lifetime: format!("'{}", lifetime.id()),
                                        });
                                }
                            }
                        }
                    }
                }
            }

            HirExpr::Call(func, args, _) => {
                // Check if this is a call to a known function
                if let HirExpr::LoadVar(callee_name) = &**func {
                    // Cross-function analysis: check if we're passing a reference to a local
                    // to a function that might store or return it
                    if let Some(callee_sig) = self.lifetime_ctx.get_function(callee_name) {
                        // Check each argument against the callee's parameter lifetimes
                        for (arg_idx, arg) in args.iter().enumerate() {
                            if let HirExpr::Borrow(inner, _) = arg {
                                if let HirExpr::LoadVar(var_name) = &**inner {
                                    // If we're passing a reference to a local variable,
                                    // and the callee returns a reference, this is a potential escape
                                    if let Some(locals) = self.function_locals.get(func_name) {
                                        if locals.contains(var_name)
                                            && callee_sig.return_lifetime.is_some()
                                        {
                                            // The reference could escape through the callee's return value
                                            self.errors
                                                .push(InterproceduralError::ReferenceEscape {
                                                    function: func_name.to_string(),
                                                    param_name: var_name.clone(),
                                                    return_location: format!(
                                                        "call to `{}` at argument {}",
                                                        callee_name, arg_idx + 1
                                                    ),
                                                });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Recursively validate
                self.validate_expression(func, func_name)?;
                for arg in args {
                    self.validate_expression(arg, func_name)?;
                }
            }

            HirExpr::MethodCall(obj, _, args) => {
                self.validate_expression(obj, func_name)?;
                for arg in args {
                    self.validate_expression(arg, func_name)?;
                }
            }

            HirExpr::BinaryOp(left, _, right) => {
                self.validate_expression(left, func_name)?;
                self.validate_expression(right, func_name)?;
            }

            HirExpr::UnaryOp(_, operand) => {
                self.validate_expression(operand, func_name)?;
            }

            HirExpr::Conditional(cond, then_expr, else_expr) => {
                self.validate_expression(cond, func_name)?;
                self.validate_expression(then_expr, func_name)?;
                self.validate_expression(else_expr, func_name)?;
            }

            HirExpr::Index(obj, idx) => {
                self.validate_expression(obj, func_name)?;
                self.validate_expression(idx, func_name)?;
            }

            HirExpr::MemberAccess(obj, _) => {
                self.validate_expression(obj, func_name)?;
            }

            HirExpr::ArrayLiteral(items)
            | HirExpr::SetLiteral(items)
            | HirExpr::TupleLiteral(items) => {
                for item in items {
                    self.validate_expression(item, func_name)?;
                }
            }

            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                for (_, value) in fields {
                    self.validate_expression(value, func_name)?;
                }
            }

            HirExpr::Match(expr, arms) => {
                self.validate_expression(expr, func_name)?;
                for (_, arm_expr) in arms {
                    self.validate_expression(arm_expr, func_name)?;
                }
            }

            HirExpr::Borrow(inner, _) | HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                self.validate_expression(inner, func_name)?;
            }

            HirExpr::Deref(inner) | HirExpr::Await(inner) | HirExpr::Spawn(inner) => {
                self.validate_expression(inner, func_name)?;
            }

            HirExpr::Share(inner) | HirExpr::Downgrade(inner) | HirExpr::Free(inner) => {
                self.validate_expression(inner, func_name)?;
            }

            HirExpr::Move(inner) => {
                self.validate_expression(inner, func_name)?;
            }

            HirExpr::Cast(inner, _) => {
                self.validate_expression(inner, func_name)?;
            }

            _ => {}
        }

        Ok(())
    }

    /// Extract the variable name from a borrow expression
    fn extract_referenced_variable(&self, expr: &HirExpr) -> Option<String> {
        match expr {
            HirExpr::Borrow(inner, _) | HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                match &**inner {
                    HirExpr::LoadVar(name) => Some(name.clone()),
                    HirExpr::MemberAccess(obj, _) => {
                        if let HirExpr::LoadVar(name) = &**obj {
                            Some(name.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Analyze what variables a closure captures
    fn analyze_closure_captures(
        &self,
        body: &[HirStmt],
        params: &[(String, Option<HirType>)],
    ) -> Vec<String> {
        let mut captures = Vec::new();
        let param_names: HashSet<String> = params.iter().map(|(name, _)| name.clone()).collect();

        // Collect all variable references in the closure body
        for stmt in body {
            self.collect_var_refs_in_stmt(stmt, &mut captures, &param_names);
        }

        captures
    }

    /// Recursively collect variable references (excluding closure parameters)
    fn collect_var_refs_in_stmt(
        &self,
        stmt: &HirStmt,
        captures: &mut Vec<String>,
        params: &HashSet<String>,
    ) {
        match stmt {
            HirStmt::Expr(expr) => {
                self.collect_var_refs_in_expr(expr, captures, params);
            }
            HirStmt::Let {
                init: Some(expr), ..
            } => {
                self.collect_var_refs_in_expr(expr, captures, params);
            }
            HirStmt::Return(Some(expr)) => {
                self.collect_var_refs_in_expr(expr, captures, params);
            }
            HirStmt::Assign { value, .. } => {
                self.collect_var_refs_in_expr(value, captures, params);
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.collect_var_refs_in_expr(cond, captures, params);
                self.collect_var_refs_in_stmt(then_branch, captures, params);
                if let Some(else_stmt) = else_branch {
                    self.collect_var_refs_in_stmt(else_stmt, captures, params);
                }
            }
            HirStmt::While { cond, body } => {
                self.collect_var_refs_in_expr(cond, captures, params);
                self.collect_var_refs_in_stmt(body, captures, params);
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.collect_var_refs_in_expr(iter, captures, params);
                self.collect_var_refs_in_stmt(body, captures, params);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_var_refs_in_stmt(try_block, captures, params);
                self.collect_var_refs_in_stmt(catch_block, captures, params);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_var_refs_in_stmt(s, captures, params);
                }
            }
            HirStmt::Throw(expr) => {
                self.collect_var_refs_in_expr(expr, captures, params);
            }
            _ => {}
        }
    }

    /// Collect variable references in an expression
    fn collect_var_refs_in_expr(
        &self,
        expr: &HirExpr,
        captures: &mut Vec<String>,
        params: &HashSet<String>,
    ) {
        match expr {
            HirExpr::LoadVar(name) => {
                // Only capture if not a closure parameter
                if !params.contains(name) && !captures.contains(name) {
                    captures.push(name.clone());
                }
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.collect_var_refs_in_expr(left, captures, params);
                self.collect_var_refs_in_expr(right, captures, params);
            }
            HirExpr::Call(func, args, _) => {
                self.collect_var_refs_in_expr(func, captures, params);
                for arg in args {
                    self.collect_var_refs_in_expr(arg, captures, params);
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.collect_var_refs_in_expr(obj, captures, params);
                for arg in args {
                    self.collect_var_refs_in_expr(arg, captures, params);
                }
            }
            HirExpr::UnaryOp(_, operand) => {
                self.collect_var_refs_in_expr(operand, captures, params);
            }
            HirExpr::Index(obj, idx) => {
                self.collect_var_refs_in_expr(obj, captures, params);
                self.collect_var_refs_in_expr(idx, captures, params);
            }
            HirExpr::MemberAccess(obj, _) => {
                self.collect_var_refs_in_expr(obj, captures, params);
            }
            HirExpr::ArrayLiteral(items)
            | HirExpr::SetLiteral(items)
            | HirExpr::TupleLiteral(items) => {
                for item in items {
                    self.collect_var_refs_in_expr(item, captures, params);
                }
            }
            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                for (_, value) in fields {
                    self.collect_var_refs_in_expr(value, captures, params);
                }
            }
            HirExpr::Borrow(inner, _) | HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                self.collect_var_refs_in_expr(inner, captures, params);
            }
            HirExpr::Deref(inner) | HirExpr::Await(inner) | HirExpr::Spawn(inner) => {
                self.collect_var_refs_in_expr(inner, captures, params);
            }
            HirExpr::Conditional(cond, then_expr, else_expr) => {
                self.collect_var_refs_in_expr(cond, captures, params);
                self.collect_var_refs_in_expr(then_expr, captures, params);
                self.collect_var_refs_in_expr(else_expr, captures, params);
            }
            HirExpr::Match(expr, arms) => {
                self.collect_var_refs_in_expr(expr, captures, params);
                for (_, arm_expr) in arms {
                    self.collect_var_refs_in_expr(arm_expr, captures, params);
                }
            }
            HirExpr::Share(inner) | HirExpr::Downgrade(inner) | HirExpr::Move(inner) => {
                self.collect_var_refs_in_expr(inner, captures, params);
            }
            HirExpr::Cast(inner, _) => {
                self.collect_var_refs_in_expr(inner, captures, params);
            }
            _ => {}
        }
    }

    /// Get all errors found
    pub fn get_errors(&self) -> &[InterproceduralError] {
        &self.errors
    }
}

impl Default for InterproceduralAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_return_local_reference() {
        let mut analyzer = InterproceduralAnalyzer::new();

        // fn leak() -> &int { let x = 42; return &x; }
        let func = HirFunction {
            name: "leak".to_string(),
            params: vec![],
            body: Arc::new(vec![
                HirStmt::Let {
                    name: "x".to_string(),
                    ty: Some(HirType::Int),
                    init: Some(HirExpr::Literal(crate::parsing::hir::HirLiteral::Int(42))),
                    is_const: false,
                    is_borrowed: None,
                },
                HirStmt::Return(Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    false,
                ))),
            ]),
            ret_type: Some(HirType::Borrow(Box::new(HirType::Int), false)),
            is_async: false,
            decorators: vec![],
            is_exported: false,
            move_params: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };

        let module = HirModule {
            functions: vec![func],
            classes: vec![],
            statements: vec![],
        };

        let result = analyzer.analyze(&module);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);

        match &errors[0] {
            InterproceduralError::ReturnLocalReference {
                function,
                local_var,
                ..
            } => {
                assert_eq!(function, "leak");
                assert_eq!(local_var, "x");
            }
            _ => panic!("Expected ReturnLocalReference error"),
        }
    }
}
