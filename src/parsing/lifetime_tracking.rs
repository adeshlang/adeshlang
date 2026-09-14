use crate::parsing::hir::*;
/// Lifetime tracking system for AdeshLang
/// Enforces reference validity across function boundaries
///
/// Rules:
/// - References cannot outlive their referents
/// - Lifetime parameters make borrow relationships explicit
/// - Elision rules infer lifetimes in common cases
/// - Returned references must not reference local data
use std::collections::HashMap;

/// Lifetime identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub struct Lifetime(u64);

impl Lifetime {
    pub fn new(id: u64) -> Self {
        Lifetime(id)
    }

    /// 'static lifetime (lives for entire program)
    pub fn static_lifetime() -> Self {
        Lifetime(0)
    }

    pub fn is_static(&self) -> bool {
        self.0 == 0
    }

    /// Get lifetime ID for display
    pub fn id(&self) -> u64 {
        self.0
    }
}

/// Reference type with lifetime
#[derive(Debug, Clone, PartialEq)]
pub struct LifetimeRef {
    pub lifetime: Lifetime,
    pub is_mutable: bool,
    pub inner_type: Box<HirType>,
}

/// Lifetime bounds for type parameters
#[derive(Debug, Clone)]
pub struct LifetimeBound {
    pub param: String,
    pub bounds: Vec<Lifetime>, // Outlives constraints
}

/// Function signature with lifetimes
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub lifetime_params: Vec<Lifetime>,
    pub param_lifetimes: Vec<(String, Lifetime)>, // param name -> lifetime
    pub return_lifetime: Option<Lifetime>,
    pub lifetime_bounds: Vec<LifetimeBound>,
}

/// Lifetime inference context
#[derive(Debug)]
pub struct LifetimeContext {
    /// Map from variable name to its lifetime
    variable_lifetimes: HashMap<String, Lifetime>,
    /// Map from function name to its signature
    function_sigs: HashMap<String, FunctionSignature>,
    /// Current scope's lifetime
    current_lifetime: Lifetime,
    /// Counter for generating fresh lifetimes
    lifetime_counter: u64,
    /// Outlives constraint graph: lifetime a -> set of lifetimes that a outlives
    outlives_graph: HashMap<Lifetime, Vec<Lifetime>>,
    /// Last-use positions for NLL: variable -> last use statement index
    last_use_positions: HashMap<String, usize>,
}

impl LifetimeContext {
    pub fn new() -> Self {
        LifetimeContext {
            variable_lifetimes: HashMap::new(),
            function_sigs: HashMap::new(),
            current_lifetime: Lifetime::new(1),
            lifetime_counter: 2,
            outlives_graph: HashMap::new(),
            last_use_positions: HashMap::new(),
        }
    }

    /// Generate a fresh lifetime
    pub fn fresh_lifetime(&mut self) -> Lifetime {
        let lt = Lifetime::new(self.lifetime_counter);
        self.lifetime_counter += 1;
        lt
    }

    /// Register a function signature
    pub fn register_function(&mut self, sig: FunctionSignature) {
        self.function_sigs.insert(sig.name.clone(), sig);
    }

    /// Get function signature
    pub fn get_function(&self, name: &str) -> Option<&FunctionSignature> {
        self.function_sigs.get(name)
    }

    /// Set variable lifetime
    pub fn set_variable_lifetime(&mut self, name: String, lifetime: Lifetime) {
        self.variable_lifetimes.insert(name, lifetime);
    }

    /// Get variable lifetime
    pub fn get_variable_lifetime(&self, name: &str) -> Option<Lifetime> {
        self.variable_lifetimes.get(name).copied()
    }

    /// Enter a new scope with fresh lifetime
    pub fn enter_scope(&mut self) {
        self.current_lifetime = self.fresh_lifetime();
    }

    /// Exit current scope
    pub fn exit_scope(&mut self) {
        self.current_lifetime = Lifetime::new(1); // Return to root
    }

    /// Add an outlives constraint: `a` outlives `b` (a >= b)
    pub fn add_outlives_constraint(&mut self, a: Lifetime, b: Lifetime) {
        self.outlives_graph.entry(a).or_default().push(b);
    }

    /// Record the last use position of a variable (for NLL)
    pub fn record_last_use(&mut self, var: &str, stmt_idx: usize) {
        let current = self.last_use_positions.get(var).copied().unwrap_or(0);
        if stmt_idx > current {
            self.last_use_positions.insert(var.to_string(), stmt_idx);
        }
    }

    /// Get the last use position of a variable
    pub fn get_last_use(&self, var: &str) -> Option<usize> {
        self.last_use_positions.get(var).copied()
    }

    /// Check if lifetime `a` outlives lifetime `b` using the constraint graph
    /// This implements a proper partial-order: a outlives b if there is a path
    /// from a to b in the outlives graph, or a is 'static, or a == b.
    pub fn outlives(&self, a: Lifetime, b: Lifetime) -> bool {
        // 'static outlives everything
        if a.is_static() {
            return true;
        }
        // Same lifetime trivially outlives
        if a == b {
            return true;
        }
        // Check the constraint graph for a path from a to b
        self.reaches(a, b, &mut std::collections::HashSet::new())
    }

    /// DFS to check if lifetime `a` reaches lifetime `b` in the outlives graph
    fn reaches(
        &self,
        a: Lifetime,
        b: Lifetime,
        visited: &mut std::collections::HashSet<Lifetime>,
    ) -> bool {
        if a == b {
            return true;
        }
        if !visited.insert(a) {
            return false; // Already visited, avoid cycles
        }
        if let Some(neighbors) = self.outlives_graph.get(&a) {
            for &neighbor in neighbors {
                if neighbor == b || self.reaches(neighbor, b, visited) {
                    return true;
                }
            }
        }
        false
    }
}

/// Error during lifetime checking
#[derive(Debug)]
pub enum LifetimeError {
    /// Reference outlived its referent
    OutliveViolation {
        reference: String,
        referent: String,
        reference_lifetime: Lifetime,
        referent_lifetime: Lifetime,
    },
    /// Returned reference to local data
    ReturnLocalRef { variable: String, function: String },
    /// Ambiguous lifetime in function signature
    AmbiguousLifetime {
        function: String,
        param_count: usize,
    },
    /// Mismatched lifetimes in function call
    LifetimeMismatch {
        function: String,
        param: String,
        expected: Lifetime,
        actual: Lifetime,
    },
}

/// Lifetime checker for HIR
pub struct LifetimeChecker {
    context: LifetimeContext,
    errors: Vec<LifetimeError>,
    /// Track which variables are function parameters (not locals)
    param_vars: std::collections::HashSet<String>,
    /// Track local variables declared in the current function
    local_vars: std::collections::HashSet<String>,
    /// Current statement index for NLL tracking
    stmt_index: usize,
}

impl LifetimeChecker {
    pub fn new() -> Self {
        LifetimeChecker {
            context: LifetimeContext::new(),
            errors: Vec::new(),
            param_vars: std::collections::HashSet::new(),
            local_vars: std::collections::HashSet::new(),
            stmt_index: 0,
        }
    }

    /// Check a module for lifetime violations
    pub fn check_module(&mut self, module: &HirModule) -> Result<(), Vec<LifetimeError>> {
        // First pass: register all function signatures
        for func in &module.functions {
            let sig = self.extract_function_signature(func);
            self.context.register_function(sig);
        }

        // Second pass: check function bodies
        for func in &module.functions {
            self.check_function(func)?;
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    fn extract_function_signature(&mut self, func: &HirFunction) -> FunctionSignature {
        // Generate fresh lifetimes for parameters instead of hardcoded placeholders
        let param_lifetime = self.context.fresh_lifetime();
        let return_lifetime = self.context.fresh_lifetime();

        let param_lifetimes = func
            .params
            .iter()
            .map(|(name, _ty, _default)| {
                let lt = self.context.fresh_lifetime();
                (name.clone(), lt)
            })
            .collect();

        FunctionSignature {
            name: func.name.clone(),
            lifetime_params: vec![param_lifetime],
            param_lifetimes,
            return_lifetime: Some(return_lifetime),
            lifetime_bounds: Vec::new(),
        }
    }

    fn check_function(&mut self, func: &HirFunction) -> Result<(), Vec<LifetimeError>> {
        self.context.enter_scope();
        self.param_vars.clear();
        self.local_vars.clear();
        self.stmt_index = 0;

        // Register function parameters and track them as params (not locals)
        for (name, _ty, _default) in &func.params {
            let lifetime = self.context.fresh_lifetime();
            self.context.set_variable_lifetime(name.clone(), lifetime);
            self.param_vars.insert(name.clone());
        }

        // Check function body
        for (idx, stmt) in func.body.iter().enumerate() {
            self.stmt_index = idx;
            self.check_stmt(stmt)?;
        }

        self.context.exit_scope();
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &HirStmt) -> Result<(), Vec<LifetimeError>> {
        match stmt {
            HirStmt::Let { name, init, .. } => {
                if let Some(init_expr) = init {
                    self.check_expr(init_expr)?;
                    // Record last use of variables in init expression (NLL)
                    self.record_uses_in_expr(init_expr);
                }
                let lifetime = self.context.fresh_lifetime();
                self.context.set_variable_lifetime(name.clone(), lifetime);
                // Track as a local variable (not a parameter)
                self.local_vars.insert(name.clone());
                Ok(())
            }
            HirStmt::Return(Some(expr)) => {
                self.check_expr(expr)?;
                // Check that return expression doesn't reference local variables
                self.check_return_expr(expr)?;
                Ok(())
            }
            HirStmt::Expr(expr) => {
                self.check_expr(expr)?;
                self.record_uses_in_expr(expr);
                Ok(())
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.check_expr(cond)?;
                self.check_stmt(then_branch)?;
                if let Some(else_br) = else_branch {
                    self.check_stmt(else_br)?;
                }
                Ok(())
            }
            HirStmt::While { cond, body } => {
                self.check_expr(cond)?;
                self.check_stmt(body)?;
                Ok(())
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.check_expr(iter)?;
                self.check_stmt(body)?;
                Ok(())
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.check_stmt(try_block)?;
                self.check_stmt(catch_block)?;
                Ok(())
            }
            HirStmt::Block(stmts) => {
                for stmt in stmts {
                    self.check_stmt(stmt)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn check_expr(&mut self, expr: &HirExpr) -> Result<(), Vec<LifetimeError>> {
        match expr {
            HirExpr::LoadVar(name) => {
                // Record last use for NLL
                self.context.record_last_use(name, self.stmt_index);
                Ok(())
            }
            HirExpr::Call(func, args, _) => {
                self.check_expr(func)?;
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(())
            }
            HirExpr::BinaryOp(left, _op, right) => {
                self.check_expr(left)?;
                self.check_expr(right)?;
                Ok(())
            }
            HirExpr::UnaryOp(_op, operand) => self.check_expr(operand),
            HirExpr::MethodCall(obj, _method, args) => {
                self.check_expr(obj)?;
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Record variable uses in an expression for NLL tracking
    fn record_uses_in_expr(&mut self, expr: &HirExpr) {
        match expr {
            HirExpr::LoadVar(name) => {
                self.context.record_last_use(name, self.stmt_index);
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.record_uses_in_expr(left);
                self.record_uses_in_expr(right);
            }
            HirExpr::Call(func, args, _) => {
                self.record_uses_in_expr(func);
                for arg in args {
                    self.record_uses_in_expr(arg);
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.record_uses_in_expr(obj);
                for arg in args {
                    self.record_uses_in_expr(arg);
                }
            }
            _ => {}
        }
    }

    fn check_return_expr(&mut self, expr: &HirExpr) -> Result<(), Vec<LifetimeError>> {
        // Check that returned references don't point to local stack variables
        // Uses the local_vars set instead of magic lifetime thresholds
        match expr {
            HirExpr::Borrow(inner, _is_exclusive) => {
                // Returning a borrow - check if it references a local
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    // If it's a local variable (not a parameter), it's an error
                    if self.local_vars.contains(var_name) {
                        self.errors.push(LifetimeError::ReturnLocalRef {
                            variable: var_name.clone(),
                            function: "current".to_string(),
                        });
                    }
                }
            }
            HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                // Check if variable is a reference to a local
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    if self.local_vars.contains(var_name) {
                        self.errors.push(LifetimeError::ReturnLocalRef {
                            variable: var_name.clone(),
                            function: "current".to_string(),
                        });
                    }
                }
            }
            HirExpr::Conditional(_, then_expr, else_expr) => {
                // Check both branches
                self.check_return_expr(then_expr)?;
                self.check_return_expr(else_expr)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Validate that a function's return type doesn't escape local lifetimes
    pub fn validate_function_returns(
        &mut self,
        func: &HirFunction,
    ) -> Result<(), Vec<LifetimeError>> {
        // Populate local_vars and param_vars for this function
        self.param_vars.clear();
        self.local_vars.clear();
        for (name, _, _) in &func.params {
            self.param_vars.insert(name.clone());
        }
        // Collect local variables
        for stmt in func.body.iter() {
            self.collect_local_vars_for_validation(stmt);
        }
        // Find all return statements
        for stmt in func.body.iter() {
            self.validate_returns_in_stmt(stmt, &func.name)?;
        }
        Ok(())
    }

    /// Collect local variable declarations for validation
    fn collect_local_vars_for_validation(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Let { name, .. } => {
                self.local_vars.insert(name.clone());
            }
            HirStmt::LetTuple { names, .. } => {
                for name in names {
                    self.local_vars.insert(name.clone());
                }
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_local_vars_for_validation(s);
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_local_vars_for_validation(then_branch);
                if let Some(else_stmt) = else_branch {
                    self.collect_local_vars_for_validation(else_stmt);
                }
            }
            HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
                self.collect_local_vars_for_validation(body);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_local_vars_for_validation(try_block);
                self.collect_local_vars_for_validation(catch_block);
            }
            _ => {}
        }
    }

    fn validate_returns_in_stmt(
        &mut self,
        stmt: &HirStmt,
        func_name: &str,
    ) -> Result<(), Vec<LifetimeError>> {
        match stmt {
            HirStmt::Return(Some(expr)) => {
                self.validate_return_expr(expr, func_name)?;
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.validate_returns_in_stmt(then_branch, func_name)?;
                if let Some(else_br) = else_branch {
                    self.validate_returns_in_stmt(else_br, func_name)?;
                }
            }
            HirStmt::While { body, .. } => {
                self.validate_returns_in_stmt(body, func_name)?;
            }
            HirStmt::ForIn { body, .. } => {
                self.validate_returns_in_stmt(body, func_name)?;
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.validate_returns_in_stmt(try_block, func_name)?;
                self.validate_returns_in_stmt(catch_block, func_name)?;
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.validate_returns_in_stmt(s, func_name)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_return_expr(
        &mut self,
        expr: &HirExpr,
        func_name: &str,
    ) -> Result<(), Vec<LifetimeError>> {
        match expr {
            // Check for borrowing local variables in return
            HirExpr::Borrow(inner, _) => {
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    // Use local_vars set instead of magic threshold
                    if self.local_vars.contains(var_name) {
                        self.errors.push(LifetimeError::ReturnLocalRef {
                            variable: var_name.clone(),
                            function: func_name.to_string(),
                        });
                    }
                }
                Ok(())
            }
            HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                if let HirExpr::LoadVar(var_name) = inner.as_ref() {
                    if self.local_vars.contains(var_name) {
                        self.errors.push(LifetimeError::ReturnLocalRef {
                            variable: var_name.clone(),
                            function: func_name.to_string(),
                        });
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl Default for LifetimeChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifetime_creation() {
        let lt = Lifetime::new(42);
        assert_eq!(lt.0, 42);
        assert!(!lt.is_static());
    }

    #[test]
    fn test_static_lifetime() {
        let lt = Lifetime::static_lifetime();
        assert!(lt.is_static());
    }

    #[test]
    fn test_lifetime_context() {
        let mut ctx = LifetimeContext::new();
        let lt1 = ctx.fresh_lifetime();
        let lt2 = ctx.fresh_lifetime();
        assert_ne!(lt1, lt2);
    }

    #[test]
    fn test_lifetime_checker_creation() {
        let checker = LifetimeChecker::new();
        assert!(checker.errors.is_empty());
    }

    #[test]
    fn test_lifetime_outlives() {
        let ctx = LifetimeContext::new();
        let static_lt = Lifetime::static_lifetime();
        let lt = Lifetime::new(5);

        assert!(ctx.outlives(static_lt, lt));
        assert!(ctx.outlives(lt, lt));
        assert!(!ctx.outlives(lt, static_lt));
    }

    #[test]
    fn test_lifetime_partial_order() {
        let mut ctx = LifetimeContext::new();
        let a = Lifetime::new(10);
        let b = Lifetime::new(20);
        let c = Lifetime::new(30);

        // Add constraints: a outlives b, b outlives c
        ctx.add_outlives_constraint(a, b);
        ctx.add_outlives_constraint(b, c);

        // a should outlive b and c (transitively)
        assert!(ctx.outlives(a, b));
        assert!(ctx.outlives(a, c)); // transitive
        // b should outlive c
        assert!(ctx.outlives(b, c));
        // c should NOT outlive a or b
        assert!(!ctx.outlives(c, b));
        assert!(!ctx.outlives(c, a));
    }
}
