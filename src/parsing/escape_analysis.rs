use crate::parsing::hir::*;
/// Escape analysis for closures and async tasks
/// Prevents unsafe reference escaping across thread/scope boundaries
///
/// Rules:
/// - References cannot escape closure if referent is local
/// - Closures capturing mutable references must be exclusive
/// - References in async tasks must be 'static or sent between threads
/// - Global references are not allowed unless explicitly 'static
use std::collections::HashMap;

/// Reference escape context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeContext {
    /// Reference doesn't escape (local use)
    Local,
    /// Reference escapes to closure
    Closure,
    /// Reference escapes to async task
    AsyncTask,
    /// Reference escapes to global scope
    Global,
    /// Reference escapes across threads
    ThreadBoundary,
}

/// Escape classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeClass {
    /// Doesn't escape current scope
    NoEscape,
    /// Escapes to immediate enclosing closure
    EscapeLocal,
    /// Escapes to async context (requires Send + Sync)
    EscapeAsync,
    /// Escapes to thread (requires 'static)
    EscapeThread,
    /// Escapes globally (only 'static allowed)
    EscapeGlobal,
}

/// Variable capture info
#[derive(Debug, Clone)]
pub struct CaptureInfo {
    pub variable: String,
    pub is_mutable: bool,
    pub captured_at: usize, // Source location
    pub context: EscapeContext,
}

/// Closure analysis result
#[derive(Debug, Clone)]
pub struct ClosureAnalysis {
    pub captures: Vec<CaptureInfo>,
    pub move_semantics: bool,         // Captures owned values
    pub unsafe_captures: Vec<String>, // Captures that may be unsafe
}

/// Escape analysis error
#[derive(Debug)]
pub enum EscapeError {
    /// Reference with limited lifetime escapes
    ReferenceEscapes {
        variable: String,
        context: EscapeContext,
        lifetime: String,
    },
    /// Mutable reference captured in non-exclusive context
    MutableRefCapture {
        variable: String,
        closure_count: usize,
    },
    /// Reference escapes to async without Send + Sync
    AsyncEscapeWithoutSync { variable: String, type_name: String },
    /// Reference escapes globally without 'static
    GlobalEscapeWithoutStatic { variable: String },
}

/// Escape analyzer
pub struct EscapeAnalyzer {
    captured_variables: HashMap<String, CaptureInfo>,
    errors: Vec<EscapeError>,
}

impl EscapeAnalyzer {
    pub fn new() -> Self {
        EscapeAnalyzer {
            captured_variables: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Analyze a closure expression - scan body for captured variables
    pub fn analyze_closure(
        &mut self,
        body: &[HirStmt],
    ) -> Result<ClosureAnalysis, Vec<EscapeError>> {
        // Scan the closure body for variable references that are captured
        // (i.e., variables used in the body that are not declared within the body)
        let mut local_decls: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut captured: std::collections::HashSet<String> = std::collections::HashSet::new();

        // First pass: collect local declarations
        for stmt in body {
            self.collect_local_decls(stmt, &mut local_decls);
        }

        // Second pass: find variable references that are not local declarations
        for stmt in body {
            self.collect_captured_vars(stmt, &local_decls, &mut captured);
        }

        // Register captured variables
        for var in &captured {
            let is_mutable = self.is_mutated_in_body(body, var);
            self.captured_variables.insert(
                var.clone(),
                CaptureInfo {
                    variable: var.clone(),
                    is_mutable,
                    captured_at: 0,
                    context: EscapeContext::Closure,
                },
            );
        }

        // Check for unsafe captures
        let mut unsafe_captures = Vec::new();
        for capture in self.captured_variables.values() {
            if capture.is_mutable {
                unsafe_captures.push(capture.variable.clone());
            }
        }

        // Validate mutable captures (only one mutable capture allowed)
        self.check_mutable_captures()?;

        let analysis = ClosureAnalysis {
            captures: self.captured_variables.values().cloned().collect(),
            move_semantics: false,
            unsafe_captures,
        };

        if self.errors.is_empty() {
            Ok(analysis)
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Collect local variable declarations in a statement
    fn collect_local_decls(
        &self,
        stmt: &HirStmt,
        local_decls: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            HirStmt::Let { name, .. } => {
                local_decls.insert(name.clone());
            }
            HirStmt::LetTuple { names, .. } => {
                for name in names {
                    local_decls.insert(name.clone());
                }
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_local_decls(s, local_decls);
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_local_decls(then_branch, local_decls);
                if let Some(else_stmt) = else_branch {
                    self.collect_local_decls(else_stmt, local_decls);
                }
            }
            HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
                self.collect_local_decls(body, local_decls);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_local_decls(try_block, local_decls);
                self.collect_local_decls(catch_block, local_decls);
            }
            HirStmt::FunctionDef { name, params, body, .. } => {
                local_decls.insert(name.clone());
                for (param_name, _, _) in params {
                    local_decls.insert(param_name.clone());
                }
                for s in body.iter() {
                    self.collect_local_decls(s, local_decls);
                }
            }
            _ => {}
        }
    }

    /// Collect variables referenced in a statement that are not locally declared
    fn collect_captured_vars(
        &self,
        stmt: &HirStmt,
        local_decls: &std::collections::HashSet<String>,
        captured: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            HirStmt::Let { init, .. } => {
                if let Some(expr) = init {
                    self.collect_expr_captures(expr, local_decls, captured);
                }
            }
            HirStmt::Expr(expr) => {
                self.collect_expr_captures(expr, local_decls, captured);
            }
            HirStmt::Assign { target, value, .. } => {
                self.collect_expr_captures(target, local_decls, captured);
                self.collect_expr_captures(value, local_decls, captured);
            }
            HirStmt::Return(Some(expr)) => {
                self.collect_expr_captures(expr, local_decls, captured);
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.collect_expr_captures(cond, local_decls, captured);
                self.collect_stmt_captures(then_branch, local_decls, captured);
                if let Some(else_stmt) = else_branch {
                    self.collect_stmt_captures(else_stmt, local_decls, captured);
                }
            }
            HirStmt::While { cond, body } => {
                self.collect_expr_captures(cond, local_decls, captured);
                self.collect_stmt_captures(body, local_decls, captured);
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.collect_expr_captures(iter, local_decls, captured);
                self.collect_stmt_captures(body, local_decls, captured);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_stmt_captures(try_block, local_decls, captured);
                self.collect_stmt_captures(catch_block, local_decls, captured);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_stmt_captures(s, local_decls, captured);
                }
            }
            HirStmt::Throw(expr) => {
                self.collect_expr_captures(expr, local_decls, captured);
            }
            _ => {}
        }
    }

    /// Collect captures from a statement (wrapper for recursive calls)
    fn collect_stmt_captures(
        &self,
        stmt: &HirStmt,
        local_decls: &std::collections::HashSet<String>,
        captured: &mut std::collections::HashSet<String>,
    ) {
        self.collect_captured_vars(stmt, local_decls, captured);
    }

    /// Collect variable references from an expression
    fn collect_expr_captures(
        &self,
        expr: &HirExpr,
        local_decls: &std::collections::HashSet<String>,
        captured: &mut std::collections::HashSet<String>,
    ) {
        match expr {
            HirExpr::LoadVar(name) => {
                // If not a local declaration, it's a captured variable
                if !local_decls.contains(name) {
                    captured.insert(name.clone());
                }
            }
            HirExpr::Call(func, args, _) => {
                self.collect_expr_captures(func, local_decls, captured);
                for arg in args {
                    self.collect_expr_captures(arg, local_decls, captured);
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.collect_expr_captures(obj, local_decls, captured);
                for arg in args {
                    self.collect_expr_captures(arg, local_decls, captured);
                }
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.collect_expr_captures(left, local_decls, captured);
                self.collect_expr_captures(right, local_decls, captured);
            }
            HirExpr::UnaryOp(_, operand) => {
                self.collect_expr_captures(operand, local_decls, captured);
            }
            HirExpr::Index(obj, idx) => {
                self.collect_expr_captures(obj, local_decls, captured);
                self.collect_expr_captures(idx, local_decls, captured);
            }
            HirExpr::MemberAccess(obj, _) => {
                self.collect_expr_captures(obj, local_decls, captured);
            }
            HirExpr::SetMember(obj, _, val) => {
                self.collect_expr_captures(obj, local_decls, captured);
                self.collect_expr_captures(val, local_decls, captured);
            }
            HirExpr::ArrayLiteral(items)
            | HirExpr::SetLiteral(items)
            | HirExpr::TupleLiteral(items) => {
                for item in items {
                    self.collect_expr_captures(item, local_decls, captured);
                }
            }
            HirExpr::DictLiteral(pairs) => {
                for (k, v) in pairs {
                    self.collect_expr_captures(k, local_decls, captured);
                    self.collect_expr_captures(v, local_decls, captured);
                }
            }
            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                for (_, v) in fields {
                    self.collect_expr_captures(v, local_decls, captured);
                }
            }
            HirExpr::Borrow(inner, _) | HirExpr::BorrowImmut(inner) | HirExpr::BorrowMut(inner) => {
                self.collect_expr_captures(inner, local_decls, captured);
            }
            HirExpr::Deref(inner) | HirExpr::Await(inner) | HirExpr::Spawn(inner) => {
                self.collect_expr_captures(inner, local_decls, captured);
            }
            HirExpr::Share(inner) | HirExpr::Downgrade(inner) | HirExpr::Free(inner) => {
                self.collect_expr_captures(inner, local_decls, captured);
            }
            HirExpr::Move(inner) => {
                self.collect_expr_captures(inner, local_decls, captured);
            }
            HirExpr::Conditional(a, b, c) => {
                self.collect_expr_captures(a, local_decls, captured);
                self.collect_expr_captures(b, local_decls, captured);
                self.collect_expr_captures(c, local_decls, captured);
            }
            HirExpr::Range(a, b, _) => {
                self.collect_expr_captures(a, local_decls, captured);
                self.collect_expr_captures(b, local_decls, captured);
            }
            HirExpr::Spread(inner) | HirExpr::OptionalGet(inner, _) | HirExpr::Format(inner, _)
            | HirExpr::NonNull(inner) | HirExpr::Cast(inner, _) => {
                self.collect_expr_captures(inner, local_decls, captured);
            }
            HirExpr::Match(expr, arms) => {
                self.collect_expr_captures(expr, local_decls, captured);
                for (_, arm_expr) in arms {
                    self.collect_expr_captures(arm_expr, local_decls, captured);
                }
            }
            HirExpr::Lambda(_, body, _) => {
                // Nested lambda: its body may capture from outer scope
                let mut nested_local_decls = local_decls.clone();
                for s in body.iter() {
                    self.collect_local_decls(s, &mut nested_local_decls);
                }
                for s in body.iter() {
                    self.collect_captured_vars(s, &nested_local_decls, captured);
                }
            }
            HirExpr::StoreVar(_, val) => {
                self.collect_expr_captures(val, local_decls, captured);
            }
            HirExpr::NewInstance(_, args) => {
                for arg in args {
                    self.collect_expr_captures(arg, local_decls, captured);
                }
            }
            HirExpr::AssignTuple(_, target) | HirExpr::AssignObject(_, target) => {
                self.collect_expr_captures(target, local_decls, captured);
            }
            _ => {}
        }
    }

    /// Check if a variable is mutated in the body (assigned to)
    fn is_mutated_in_body(&self, body: &[HirStmt], var_name: &str) -> bool {
        for stmt in body {
            if self.is_mutated_in_stmt(stmt, var_name) {
                return true;
            }
        }
        false
    }

    /// Check if a variable is mutated in a statement
    fn is_mutated_in_stmt(&self, stmt: &HirStmt, var_name: &str) -> bool {
        match stmt {
            HirStmt::Assign { target, .. } => {
                if let HirExpr::LoadVar(name) = target {
                    if name == var_name {
                        return true;
                    }
                }
                // Check nested statements in target/value
                self.is_mutated_in_expr(target, var_name)
            }
            HirStmt::Block(stmts) => {
                stmts.iter().any(|s| self.is_mutated_in_stmt(s, var_name))
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.is_mutated_in_stmt(then_branch, var_name)
                    || else_branch
                        .as_ref()
                        .map(|e| self.is_mutated_in_stmt(e, var_name))
                        .unwrap_or(false)
            }
            HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
                self.is_mutated_in_stmt(body, var_name)
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.is_mutated_in_stmt(try_block, var_name)
                    || self.is_mutated_in_stmt(catch_block, var_name)
            }
            _ => false,
        }
    }

    /// Check if a variable is mutated in an expression
    fn is_mutated_in_expr(&self, expr: &HirExpr, var_name: &str) -> bool {
        match expr {
            HirExpr::SetMember(obj, _, _) => {
                if let HirExpr::LoadVar(name) = obj.as_ref() {
                    if name == var_name {
                        return true;
                    }
                }
                false
            }
            HirExpr::BorrowMut(inner) => {
                if let HirExpr::LoadVar(name) = inner.as_ref() {
                    if name == var_name {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Analyze a variable's escape class
    pub fn analyze_escape_class(&mut self, var: &str, context: EscapeContext) -> EscapeClass {
        // Check if variable is captured
        if let Some(_capture) = self.captured_variables.get(var) {
            match context {
                EscapeContext::Local => EscapeClass::NoEscape,
                EscapeContext::Closure => EscapeClass::EscapeLocal,
                EscapeContext::AsyncTask => EscapeClass::EscapeAsync,
                EscapeContext::Global => EscapeClass::EscapeGlobal,
                EscapeContext::ThreadBoundary => EscapeClass::EscapeThread,
            }
        } else {
            EscapeClass::NoEscape
        }
    }

    /// Register a captured variable
    pub fn capture_variable(&mut self, info: CaptureInfo) {
        self.captured_variables.insert(info.variable.clone(), info);
    }

    /// Check for mutable reference capture conflicts
    pub fn check_mutable_captures(&mut self) -> Result<(), Vec<EscapeError>> {
        let mut mutable_count = 0;

        for capture in self.captured_variables.values() {
            if capture.is_mutable {
                mutable_count += 1;
            }
        }

        if mutable_count > 1 {
            for capture in self.captured_variables.values() {
                if capture.is_mutable {
                    self.errors.push(EscapeError::MutableRefCapture {
                        variable: capture.variable.clone(),
                        closure_count: 1,
                    });
                }
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Check for static lifetime requirement
    pub fn check_static_requirement(&mut self, var: &str, context: EscapeContext) {
        match context {
            EscapeContext::Global | EscapeContext::ThreadBoundary => {
                self.errors.push(EscapeError::GlobalEscapeWithoutStatic {
                    variable: var.to_string(),
                });
            }
            _ => {}
        }
    }

    /// Analyze reference propagation in function calls
    /// Checks if arguments that are references escape to global/thread context
    pub fn analyze_call_escape(
        &mut self,
        func_name: &str,
        args: &[HirExpr],
    ) -> Result<(), Vec<EscapeError>> {
        // Check if the function is a spawning/async function that crosses boundaries
        let is_spawning = func_name == "spawn"
            || func_name == "thread_spawn"
            || func_name == "parallel"
            || func_name == "async";

        if is_spawning {
            for arg in args {
                // Check for captured references that would escape across thread boundaries
                if let HirExpr::Lambda(_, body, _) = arg {
                    // Analyze the closure body for captures
                    self.analyze_closure(body)?;
                    // Check that all captures are safe for thread crossing
                    for capture in self.captured_variables.values() {
                        if capture.is_mutable {
                            self.errors.push(EscapeError::AsyncEscapeWithoutSync {
                                variable: capture.variable.clone(),
                                type_name: "mutable reference".to_string(),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Default for EscapeAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Async task context analyzer
pub struct AsyncAnalyzer {
    errors: Vec<EscapeError>,
}

impl AsyncAnalyzer {
    pub fn new() -> Self {
        AsyncAnalyzer { errors: Vec::new() }
    }

    /// Check async block for lifetime violations
    /// All captured references must be safe for async context
    pub fn check_async_block(&mut self, body: &[HirStmt]) -> Result<(), Vec<EscapeError>> {
        // Collect local declarations
        let mut local_decls: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut captured: std::collections::HashSet<String> = std::collections::HashSet::new();

        for stmt in body {
            self.collect_local_decls(stmt, &mut local_decls);
        }

        for stmt in body {
            self.collect_captured_vars(stmt, &local_decls, &mut captured);
        }

        // Check that captured variables are safe for async context
        // Mutable references to locals are unsafe in async (may be accessed after suspend)
        for var in &captured {
            let is_mutable = self.is_mutated_in_body(body, var);
            if is_mutable {
                self.errors.push(EscapeError::AsyncEscapeWithoutSync {
                    variable: var.clone(),
                    type_name: "mutable capture".to_string(),
                });
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Verify if a type is safe to send across threads/async boundaries
    /// Primitive Copy types and ARC types are safe; borrows and raw pointers are not
    pub fn is_send_sync(type_name: &str) -> bool {
        // Primitive integer/float types are safe
        let primitives = [
            "int", "i8", "i16", "i32", "i64", "i128",
            "u8", "u16", "u32", "u64", "u128",
            "f32", "f64", "float", "bool", "char",
        ];
        if primitives.contains(&type_name.to_lowercase().as_str()) {
            return true;
        }
        // String is safe
        if type_name.eq_ignore_ascii_case("string") {
            return true;
        }
        // ARC types (share/strong/weak) are safe for concurrent access
        if type_name.starts_with("Shared<")
            || type_name.starts_with("Weak<")
            || type_name.starts_with("Strong<")
        {
            return true;
        }
        // Borrow references and raw pointers are NOT safe across thread boundaries
        if type_name.starts_with('&') || type_name.starts_with('*') {
            return false;
        }
        // Default: conservatively not safe for unknown types
        false
    }

    fn collect_local_decls(
        &self,
        stmt: &HirStmt,
        local_decls: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            HirStmt::Let { name, .. } => {
                local_decls.insert(name.clone());
            }
            HirStmt::LetTuple { names, .. } => {
                for name in names {
                    local_decls.insert(name.clone());
                }
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_local_decls(s, local_decls);
                }
            }
            HirStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_local_decls(then_branch, local_decls);
                if let Some(else_stmt) = else_branch {
                    self.collect_local_decls(else_stmt, local_decls);
                }
            }
            HirStmt::While { body, .. } | HirStmt::ForIn { body, .. } => {
                self.collect_local_decls(body, local_decls);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_local_decls(try_block, local_decls);
                self.collect_local_decls(catch_block, local_decls);
            }
            _ => {}
        }
    }

    fn collect_captured_vars(
        &self,
        stmt: &HirStmt,
        local_decls: &std::collections::HashSet<String>,
        captured: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            HirStmt::Let { init, .. } => {
                if let Some(expr) = init {
                    self.collect_expr_captures(expr, local_decls, captured);
                }
            }
            HirStmt::Expr(expr) => {
                self.collect_expr_captures(expr, local_decls, captured);
            }
            HirStmt::Assign { value, .. } => {
                self.collect_expr_captures(value, local_decls, captured);
            }
            HirStmt::Return(Some(expr)) => {
                self.collect_expr_captures(expr, local_decls, captured);
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.collect_expr_captures(cond, local_decls, captured);
                self.collect_captured_vars(then_branch, local_decls, captured);
                if let Some(else_stmt) = else_branch {
                    self.collect_captured_vars(else_stmt, local_decls, captured);
                }
            }
            HirStmt::While { cond, body } => {
                self.collect_expr_captures(cond, local_decls, captured);
                self.collect_captured_vars(body, local_decls, captured);
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.collect_expr_captures(iter, local_decls, captured);
                self.collect_captured_vars(body, local_decls, captured);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_captured_vars(s, local_decls, captured);
                }
            }
            _ => {}
        }
    }

    fn collect_expr_captures(
        &self,
        expr: &HirExpr,
        local_decls: &std::collections::HashSet<String>,
        captured: &mut std::collections::HashSet<String>,
    ) {
        match expr {
            HirExpr::LoadVar(name) => {
                if !local_decls.contains(name) {
                    captured.insert(name.clone());
                }
            }
            HirExpr::Call(func, args, _) => {
                self.collect_expr_captures(func, local_decls, captured);
                for arg in args {
                    self.collect_expr_captures(arg, local_decls, captured);
                }
            }
            HirExpr::BinaryOp(left, _, right) => {
                self.collect_expr_captures(left, local_decls, captured);
                self.collect_expr_captures(right, local_decls, captured);
            }
            HirExpr::UnaryOp(_, operand) => {
                self.collect_expr_captures(operand, local_decls, captured);
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.collect_expr_captures(obj, local_decls, captured);
                for arg in args {
                    self.collect_expr_captures(arg, local_decls, captured);
                }
            }
            HirExpr::ArrayLiteral(items)
            | HirExpr::SetLiteral(items)
            | HirExpr::TupleLiteral(items) => {
                for item in items {
                    self.collect_expr_captures(item, local_decls, captured);
                }
            }
            _ => {}
        }
    }

    fn is_mutated_in_body(&self, body: &[HirStmt], var_name: &str) -> bool {
        for stmt in body {
            if let HirStmt::Assign { target, .. } = stmt {
                if let HirExpr::LoadVar(name) = target {
                    if name == var_name {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_analyzer_creation() {
        let analyzer = EscapeAnalyzer::new();
        assert!(analyzer.errors.is_empty());
    }

    #[test]
    fn test_capture_registration() {
        let mut analyzer = EscapeAnalyzer::new();
        let capture = CaptureInfo {
            variable: "x".to_string(),
            is_mutable: false,
            captured_at: 0,
            context: EscapeContext::Local,
        };
        analyzer.capture_variable(capture.clone());
        assert!(analyzer.captured_variables.contains_key("x"));
    }

    #[test]
    fn test_escape_class_analysis() {
        let mut analyzer = EscapeAnalyzer::new();
        let escape = analyzer.analyze_escape_class("x", EscapeContext::Local);
        assert_eq!(escape, EscapeClass::NoEscape);
    }

    #[test]
    fn test_async_analyzer_creation() {
        let analyzer = AsyncAnalyzer::new();
        assert!(analyzer.errors.is_empty());
    }

    #[test]
    fn test_send_sync_check() {
        // Primitive types are safe
        assert!(AsyncAnalyzer::is_send_sync("i32"));
        assert!(AsyncAnalyzer::is_send_sync("String"));
        assert!(AsyncAnalyzer::is_send_sync("bool"));

        // References and raw pointers are not safe
        assert!(!AsyncAnalyzer::is_send_sync("&int"));
        assert!(!AsyncAnalyzer::is_send_sync("*u8"));
    }

    #[test]
    fn test_mutable_capture_conflict() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.capture_variable(CaptureInfo {
            variable: "x".to_string(),
            is_mutable: true,
            captured_at: 0,
            context: EscapeContext::Closure,
        });
        analyzer.capture_variable(CaptureInfo {
            variable: "y".to_string(),
            is_mutable: true,
            captured_at: 1,
            context: EscapeContext::Closure,
        });

        let result = analyzer.check_mutable_captures();
        assert!(result.is_err());
    }
}
