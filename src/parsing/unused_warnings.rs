//! Unused Warning Pass
//!
//! Performs compile-time static analysis on the AST/HIR to find unused variables,
//! parameters, imports, functions, and classes, generating Rust-like warnings with
//! line:col location and support for inline/file comment directives and CLI suppression flags.

use crate::parsing::ast::{Expr, ExprKind, Pattern, Stmt, StmtKind};
use crate::parsing::hir::HirModule;
use crate::toolchain::RuntimeConfig;
use colored::Colorize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Variable,
    Parameter,
    Import,
    Function,
    Class,
}

impl EntityKind {
    pub fn description(&self) -> &'static str {
        match self {
            EntityKind::Variable => "variable",
            EntityKind::Parameter => "parameter",
            EntityKind::Import => "import",
            EntityKind::Function => "function",
            EntityKind::Class => "class",
        }
    }
}

#[derive(Debug, Clone)]
struct SymbolInfo {
    used: bool,
    kind: EntityKind,
    line: usize,
    col: usize,
}

pub struct UnusedWarningPass<'a> {
    // Stack of active scopes. Map name -> SymbolInfo
    scopes: Vec<HashMap<String, SymbolInfo>>,
    // Collected warnings: (message, line, col)
    warnings: Vec<(String, usize, usize)>,
    src: &'a str,
}

impl<'a> UnusedWarningPass<'a> {
    pub fn new(src: &'a str) -> Self {
        UnusedWarningPass {
            scopes: Vec::new(),
            warnings: Vec::new(),
            src,
        }
    }

    /// Check if warnings for a specific line or file are suppressed via directives
    pub fn is_suppressed(&self, line: usize) -> bool {
        let lines: Vec<&str> = self.src.lines().collect();
        if lines.is_empty() {
            return false;
        }

        // 1. Check file-level directives in the top 30 lines
        for l in lines.iter().take(30) {
            let trimmed = l.trim();
            if trimmed.contains("@allow(warnings)")
                || trimmed.contains("@allow(unused)")
                || trimmed.contains("adesh-allow: warnings")
                || trimmed.contains("adesh-allow: unused")
                || trimmed.contains("adesh-disable-warnings")
                || trimmed.contains("adesh-ignore-warnings")
            {
                return true;
            }
        }

        // 2. Check line-level directive on current line or preceding line
        if line > 0 && line <= lines.len() {
            let curr = lines[line - 1].trim();
            if curr.contains("@allow(warnings)")
                || curr.contains("@allow(unused)")
                || curr.contains("adesh-allow")
                || curr.contains("adesh-ignore")
            {
                return true;
            }
        }
        if line > 1 && line - 1 <= lines.len() {
            let prev = lines[line - 2].trim();
            if prev.contains("@allow(warnings)")
                || prev.contains("@allow(unused)")
                || prev.contains("adesh-allow")
                || prev.contains("adesh-ignore")
            {
                return true;
            }
        }

        false
    }

    /// Primary check entrypoint operating on AST with full line:col precision
    pub fn check_ast(
        stmts: &[Stmt],
        file: Option<&str>,
        src: &'a str,
        config: &RuntimeConfig,
    ) -> Vec<String> {
        if config.disable_warnings
            || std::env::var("ADESHLANG_DISABLE_WARNINGS").is_ok()
            || std::env::var("ADESHLANG_NO_WARNINGS").is_ok()
        {
            return Vec::new();
        }

        let mut pass = Self::new(src);
        pass.analyze_ast(stmts);

        let file_prefix = match file {
            Some(f) => f.to_string(),
            None => "".to_string(),
        };

        let warnings = std::mem::take(&mut pass.warnings);

        warnings
            .into_iter()
            .filter(|(_, line, _)| !pass.is_suppressed(*line))
            .map(|(w, line, col)| {
                let warning_tag = "warning".yellow().bold();
                if file_prefix.is_empty() {
                    format!("{}:{}:{}: {}", warning_tag, line, col, w)
                } else {
                    let location_tag = format!("{}:{}:{}", file_prefix, line, col).cyan();
                    format!("{}: {}: {}", warning_tag, location_tag, w)
                }
            })
            .collect()
    }

    /// Legacy module fallback check for compatibility
    pub fn check_module(module: &HirModule, file: Option<&str>) -> Vec<String> {
        if std::env::var("ADESHLANG_DISABLE_WARNINGS").is_ok()
            || std::env::var("ADESHLANG_NO_WARNINGS").is_ok()
        {
            return Vec::new();
        }
        let file_prefix = file.unwrap_or("");
        let mut pass = Self::new("");
        pass.push_scope();

        for func in &module.functions {
            if !func.is_exported && func.name != "main" {
                pass.declare(func.name.clone(), EntityKind::Function, 1, 1);
            }
        }
        for class in &module.classes {
            if class.name != "Main" {
                pass.declare(class.name.clone(), EntityKind::Class, 1, 1);
            }
        }
        pass.pop_scope();

        pass.warnings
            .into_iter()
            .map(|(w, line, col)| {
                let warning_tag = "warning".yellow().bold();
                if file_prefix.is_empty() {
                    format!("{}:{}:{}: {}", warning_tag, line, col, w)
                } else {
                    let location_tag = format!("{}:{}:{}", file_prefix, line, col).cyan();
                    format!("{}: {}: {}", warning_tag, location_tag, w)
                }
            })
            .collect()
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            for (name, info) in scope {
                if !info.used && !name.starts_with('_') {
                    if info.kind == EntityKind::Function && name == "main" {
                        continue;
                    }
                    if info.kind == EntityKind::Class && name == "Main" {
                        continue;
                    }
                    if self.is_suppressed(info.line) {
                        continue;
                    }
                    self.warnings.push((
                        format!("unused {} `{}`", info.kind.description(), name),
                        info.line,
                        info.col,
                    ));
                }
            }
        }
    }

    fn declare(&mut self, name: String, kind: EntityKind, line: usize, col: usize) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name,
                SymbolInfo {
                    used: false,
                    kind,
                    line: if line == 0 { 1 } else { line },
                    col: if col == 0 { 1 } else { col },
                },
            );
        }
    }

    fn mark_used(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.used = true;
                return;
            }
        }
    }

    fn analyze_ast(&mut self, stmts: &[Stmt]) {
        self.push_scope();

        for stmt in stmts {
            self.scan_stmt(stmt);
        }

        self.pop_scope();
    }

    fn scan_stmt(&mut self, stmt: &Stmt) {
        let line = stmt.span.line;
        let col = stmt.span.col;

        match &stmt.kind {
            StmtKind::Let(name, init, _, _, _, _) => {
                if let Some(expr) = init {
                    self.scan_expr(expr);
                }
                self.declare(name.clone(), EntityKind::Variable, line, col);
            }
            StmtKind::LetTuple(names, _, init, _, _, _) => {
                if let Some(expr) = init {
                    self.scan_expr(expr);
                }
                for name in names {
                    self.declare(name.clone(), EntityKind::Variable, line, col);
                }
            }
            StmtKind::LetObject(pairs, init, _, _, _) => {
                if let Some(expr) = init {
                    self.scan_expr(expr);
                }
                for (key, alias) in pairs {
                    let target_name = alias.as_ref().unwrap_or(key);
                    self.declare(target_name.clone(), EntityKind::Variable, line, col);
                }
            }
            StmtKind::ShareDeclaration(decl, _) => {
                self.scan_expr(&decl.expr);
                self.declare(decl.name.clone(), EntityKind::Variable, line, col);
            }
            StmtKind::StrongDeclaration(decl, _) => {
                self.scan_expr(&decl.expr);
                self.declare(decl.name.clone(), EntityKind::Variable, line, col);
            }
            StmtKind::WeakDeclaration(decl, _) => {
                self.scan_expr(&decl.expr);
                self.declare(decl.name.clone(), EntityKind::Variable, line, col);
            }
            StmtKind::ExprStmt(expr) => {
                self.scan_expr(expr);
            }
            StmtKind::Return(expr_opt) => {
                if let Some(expr) = expr_opt {
                    self.scan_expr(expr);
                }
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.scan_expr(cond);
                self.scan_stmt(then_branch);
                if let Some(else_stmt) = else_branch {
                    self.scan_stmt(else_stmt);
                }
            }
            StmtKind::While { cond, body } => {
                self.scan_expr(cond);
                self.scan_stmt(body);
            }
            StmtKind::ForIn { name, iter, body } => {
                self.scan_expr(iter);
                self.push_scope();
                self.declare(name.clone(), EntityKind::Variable, line, col);
                self.scan_stmt(body);
                self.pop_scope();
            }
            StmtKind::Block(stmts) => {
                self.push_scope();
                for s in stmts {
                    self.scan_stmt(s);
                }
                self.pop_scope();
            }
            StmtKind::Function(func, is_export) => {
                if !is_export && func.name != "main" {
                    self.declare(func.name.clone(), EntityKind::Function, line, col);
                }
                self.push_scope();
                for (param_name, default_val, _) in &func.params {
                    if let Some(dv) = default_val {
                        self.scan_expr(dv);
                    }
                    if param_name != "this" {
                        self.declare(param_name.clone(), EntityKind::Parameter, line, col);
                    }
                }
                for s in func.body.iter() {
                    self.scan_stmt(s);
                }
                self.pop_scope();
            }
            StmtKind::Class(class, _) => {
                if class.name != "Main" {
                    self.declare(class.name.clone(), EntityKind::Class, line, col);
                }
                for method in &class.methods {
                    self.push_scope();
                    for (param_name, default_val, _) in &method.params {
                        if let Some(dv) = default_val {
                            self.scan_expr(dv);
                        }
                        if param_name != "this" {
                            self.declare(param_name.clone(), EntityKind::Parameter, line, col);
                        }
                    }
                    for s in method.body.iter() {
                        self.scan_stmt(s);
                    }
                    self.pop_scope();
                }
            }
            StmtKind::Import { path, alias } | StmtKind::ImportDefault { path, alias } => {
                if path == "JSON" || path == "std:JSON" || path == "json" || path == "std:json" {
                    self.warnings.push((
                        "'JSON' is a built-in global library and does not need to be imported"
                            .to_string(),
                        line,
                        col,
                    ));
                } else {
                    self.declare(alias.clone(), EntityKind::Import, line, col);
                }
            }
            StmtKind::ImportNames { path, names } => {
                if path == "JSON" || path == "std:JSON" || path == "json" || path == "std:json" {
                    self.warnings.push((
                        "'JSON' is a built-in global library and does not need to be imported"
                            .to_string(),
                        line,
                        col,
                    ));
                } else {
                    for name in names {
                        self.declare(name.clone(), EntityKind::Import, line, col);
                    }
                }
            }
            StmtKind::TryCatch {
                try_block,
                err_name,
                catch_block,
            } => {
                self.scan_stmt(try_block);
                self.push_scope();
                self.declare(err_name.clone(), EntityKind::Variable, line, col);
                self.scan_stmt(catch_block);
                self.pop_scope();
            }
            StmtKind::Region { body, .. } | StmtKind::UnsafeBlock(body) | StmtKind::Defer(body) => {
                self.scan_stmt(body);
            }
            _ => {}
        }
    }

    fn scan_expr(&mut self, expr: &Expr) {
        let line = expr.span.line;
        let col = expr.span.col;

        match &expr.kind {
            ExprKind::Variable(name) => {
                self.mark_used(name);
            }
            ExprKind::Assign(name, val) => {
                self.scan_expr(val);
                self.mark_used(name);
            }
            ExprKind::AssignOp(target, _, val) => {
                self.scan_expr(target);
                self.scan_expr(val);
            }
            ExprKind::Unary(_, e)
            | ExprKind::Grouping(e)
            | ExprKind::Await(e)
            | ExprKind::Spawn(e)
            | ExprKind::Throw(e)
            | ExprKind::Spread(e)
            | ExprKind::Try(e)
            | ExprKind::NonNull(e)
            | ExprKind::Format(e, _)
            | ExprKind::Cast(e, _) => {
                self.scan_expr(e);
            }
            ExprKind::Binary(left, _, right) | ExprKind::Logical(left, _, right) => {
                self.scan_expr(left);
                self.scan_expr(right);
            }
            ExprKind::Call(func, args, _)
            | ExprKind::OptCall(func, args, _)
            | ExprKind::New(func, args) => {
                self.scan_expr(func);
                for arg in args {
                    self.scan_expr(arg);
                }
            }
            ExprKind::Array(exprs) | ExprKind::Tuple(exprs) | ExprKind::SetLiteral(exprs) => {
                for e in exprs {
                    self.scan_expr(e);
                }
            }
            ExprKind::Object(pairs) | ExprKind::StructLiteral(_, pairs) => {
                for (_, v) in pairs {
                    self.scan_expr(v);
                }
            }
            ExprKind::Index(obj, idx) => {
                self.scan_expr(obj);
                self.scan_expr(idx);
            }
            ExprKind::Get(obj, _) | ExprKind::OptGet(obj, _) => {
                self.scan_expr(obj);
            }
            ExprKind::Set(obj, _, val) => {
                self.scan_expr(obj);
                self.scan_expr(val);
            }
            ExprKind::Conditional(cond, then_expr, else_expr) => {
                self.scan_expr(cond);
                self.scan_expr(then_expr);
                self.scan_expr(else_expr);
            }
            ExprKind::Fn(params, body, _) => {
                self.push_scope();
                for (param_name, default_val, _) in params {
                    if let Some(dv) = default_val {
                        self.scan_expr(dv);
                    }
                    if param_name != "this" {
                        self.declare(param_name.clone(), EntityKind::Parameter, line, col);
                    }
                }
                for s in body.iter() {
                    self.scan_stmt(s);
                }
                self.pop_scope();
            }
            ExprKind::Match(e, arms) => {
                self.scan_expr(e);
                for (pattern, body_expr) in arms {
                    self.push_scope();
                    self.add_pattern_bindings(pattern, line, col);
                    self.scan_expr(body_expr);
                    self.pop_scope();
                }
            }
            _ => {}
        }
    }

    fn add_pattern_bindings(&mut self, pattern: &Pattern, line: usize, col: usize) {
        match pattern {
            Pattern::Variable(name) => {
                self.declare(name.clone(), EntityKind::Variable, line, col);
            }
            Pattern::Or(p1, p2) => {
                self.add_pattern_bindings(p1, line, col);
                self.add_pattern_bindings(p2, line, col);
            }
            Pattern::EnumVariant(_, subpatterns) => {
                for sp in subpatterns {
                    self.add_pattern_bindings(sp, line, col);
                }
            }
            _ => {}
        }
    }
}
