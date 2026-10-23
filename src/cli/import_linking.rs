//! Native import/export linking for the non-interpreter runtimes (njit, vm, aot, wasm).
//!
//! This module resolves `import` statements, loads each imported module through the
//! same mandatory compile-time gates as the main module (parse, decorator phases,
//! RAII transformation, compile-time memory safety), lowers it to HIR, and collects
//! the imported module's functions/classes/enums so they can be merged into the main
//! module and compiled natively — with no interpreter fallback.
//!
//! ## Selective (tree-shaken) import
//! Instead of importing an entire module, only the symbols that `main` actually
//! references are merged into the main compilation unit:
//! - `import "m.adesh" as alias;` — only `alias.X` members that are accessed are
//!   imported (functions/classes/enums named `X`, plus exported constants named `X`).
//! - `from "m.adesh" import a, b;` — only the explicitly-named `a` and `b` are imported.
//! - `import default "m.adesh" as alias;` — the default export is imported only if
//!   `alias` is used as a value.
//!
//! Every imported module is still fully parsed and run through the compile-time
//! memory-safety gate (`check_ownership_and_parse_in`) so ownership/RAII safety is
//! preserved even for unmerged code — but only the used symbols are compiled into
//! `main`, keeping the emitted code small and memory usage bounded.
//!
//! A namespace object (exported constants) is built for each import alias, containing
//! only the constants that are actually referenced, and bound as a prebound global.

use crate::cli::parsing::check_ownership_and_parse_in;
use crate::parsing::hir::{HirClass, HirEnum, HirFunction, HirModule, HirStmt};
use crate::parsing::lexer::Lexer;
use crate::parsing::parser::Parser;
use crate::toolchain::config::RuntimeConfig;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A resolved namespace binding: `alias` -> imported module path.
#[derive(Debug, Clone)]
pub struct NamespaceBinding {
    pub alias: String,
    /// The canonical (absolute) path of the imported module.
    pub path: PathBuf,
}

/// Which symbols of an imported module are actually referenced by `main`.
#[derive(Debug, Clone, Default)]
pub struct ImportUsage {
    /// For `import "m" as alias;`: member names `X` used via `alias.X` calls/reads.
    pub members: HashSet<String>,
    /// For `from "m" import a, b;`: the explicitly imported names.
    pub direct_names: HashSet<String>,
    /// Whether the alias itself is used as a value (e.g. default import `alias()`).
    pub used_as_value: bool,
}

impl ImportUsage {
    pub fn is_empty(&self) -> bool {
        self.members.is_empty() && self.direct_names.is_empty() && !self.used_as_value
    }

    /// Whether symbol `name` should be imported from this module.
    pub fn should_import(&self, name: &str) -> bool {
        self.members.contains(name) || self.direct_names.contains(name)
    }
}

/// Result of loading all imports in a source file.
#[derive(Debug, Clone, Default)]
pub struct ImportLoadResult {
    /// Imported functions (gated, and filtered to only the symbols `main` uses).
    pub functions: Vec<HirFunction>,
    /// Imported classes (filtered to only the symbols `main` uses).
    pub classes: Vec<HirClass>,
    /// Imported enums (filtered to only the symbols `main` uses).
    pub enums: Vec<HirEnum>,
    /// Namespace bindings for `import "path" as alias` / `import default ... as alias`.
    pub bindings: Vec<NamespaceBinding>,
    /// For `import { a, b } from "path"`, name -> (module path, resolved path).
    pub named_imports: HashMap<String, (String, String)>,
    /// alias -> set of symbols actually used from that module (for namespace objects).
    pub usage: HashMap<String, ImportUsage>,
}

impl ImportLoadResult {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Top-level import statement shape collected from a source file.
#[derive(Debug, Clone)]
struct TopImport {
    path: String,
    alias: String,
    names: Option<Vec<String>>, // Some(names) for `from "p" import a, b`; None for namespace import
}

/// Load and gate every `import` in `src` (relative to `root_dir`), recursively resolving
/// transitive imports. Only the symbols that `main` actually references are merged into
/// the result. Returns the merged declarations plus namespace bindings.
pub fn load_imports(src: &str, root_dir: &Path) -> Result<ImportLoadResult, String> {
    let config = RuntimeConfig::default();

    // Parse + gate the main module too, so we have its HIR for usage analysis.
    let (_main_ast, main_hir) = check_ownership_and_parse_in(src, &config, None)
        .map_err(|e| format!("failed to gate main module: {}", e))?;

    // Collect top-level import statements from the AST.
    let mut lexer = Lexer::new(src);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("import lex error: {}", e))?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser
        .parse_program()
        .map_err(|e| format!("import parse error: {}", e))?;

    let import_stmts: Vec<TopImport> = ast
        .iter()
        .filter_map(|s| match &s.kind {
            crate::parsing::ast::StmtKind::Import { path, alias } => Some(TopImport {
                path: path.clone(),
                alias: alias.clone(),
                names: None,
            }),
            crate::parsing::ast::StmtKind::ImportDefault { path, alias } => Some(TopImport {
                path: path.clone(),
                alias: alias.clone(),
                names: None,
            }),
            crate::parsing::ast::StmtKind::ImportNames { path, names } => Some(TopImport {
                path: path.clone(),
                alias: String::new(),
                names: Some(names.clone()),
            }),
            _ => None,
        })
        .collect();

    // Figure out which symbols of each import `main` actually uses.
    let usage = collect_import_usage(&main_hir, &import_stmts);

    let mut result = ImportLoadResult::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();

    for imp in &import_stmts {
        let resolved = crate::execution::runtime_core::interpreter_impl::utilities::resolve_path(
            &imp.path,
            &root_dir.to_string_lossy(),
        );
        let resolved_path = PathBuf::from(&resolved);

        let used = usage.get(&imp.alias).cloned().unwrap_or_default();

        // For namespace imports with no recorded usage (e.g. the import is present
        // but nothing references alias.X), still record the binding so the alias is
        // defined, but import no symbols.
        if !visited.insert(resolved_path.clone()) {
            if !imp.alias.is_empty() {
                result.bindings.push(NamespaceBinding {
                    alias: imp.alias.clone(),
                    path: resolved_path.clone(),
                });
                if !used.is_empty() {
                    result.usage.insert(imp.alias.clone(), used);
                }
            }
            continue;
        }

        let module_src = std::fs::read_to_string(&resolved).map_err(|e| {
            format!(
                "cannot read imported module '{}' (resolved '{}'): {}",
                imp.path, resolved, e
            )
        })?;

        // Run the imported module through the same mandatory gates as the main module
        // (parse, decorator compile-time phases, RAII, compile-time memory safety).
        // The whole module is gated for safety, but only used symbols are merged.
        let (_module_ast, module_hir) =
            check_ownership_and_parse_in(&module_src, &config, Some(&resolved))
                .map_err(|e| format!("failed to gate imported module '{}': {}", imp.path, e))?;

        // Recursively resolve this module's own imports (transitive imports are always
        // fully resolved so the dependency graph is complete and memory-safe).
        let parent_dir = resolved_path.parent().unwrap_or(Path::new("."));
        let sub = load_imports_from_hir(module_hir, parent_dir, &mut visited, &config)?;

        // Tree-shake: only merge the symbols `main` actually uses from this module.
        let used_set = &used;
        for f in sub.functions {
            if used_set.should_import(&f.name) || used_set.used_as_value {
                result.functions.push(f);
            }
        }
        for c in sub.classes {
            if used_set.should_import(&c.name) || used_set.used_as_value {
                result.classes.push(c);
            }
        }
        for e in sub.enums {
            if used_set.should_import(&e.name) || used_set.used_as_value {
                result.enums.push(e);
            }
        }

        if !imp.alias.is_empty() {
            result.bindings.push(NamespaceBinding {
                alias: imp.alias.clone(),
                path: resolved_path.clone(),
            });
            if !used.is_empty() {
                result.usage.insert(imp.alias.clone(), used);
            }
        }
        if let Some(names) = &imp.names {
            for name in names {
                result
                    .named_imports
                    .insert(name.clone(), (imp.path.clone(), resolved.clone()));
            }
        }
    }

    Ok(result)
}

/// Recursively resolve imports inside an already-loaded HIR module.
///
/// Transitive imports are always fully resolved (the whole dependency graph is loaded
/// and gated) so that memory safety is preserved across module boundaries, but each
/// module's symbols are only merged into `main` when referenced.
fn load_imports_from_hir(
    mut hir: HirModule,
    base_dir: &Path,
    visited: &mut HashSet<PathBuf>,
    config: &RuntimeConfig,
) -> Result<ImportLoadResult, String> {
    let mut result = ImportLoadResult::new();

    let mut retained = Vec::new();
    let original = std::mem::take(&mut hir.statements);
    for stmt in original {
        match stmt {
            HirStmt::Import { path, alias } => {
                let resolved =
                    crate::execution::runtime_core::interpreter_impl::utilities::resolve_path(
                        &path,
                        &base_dir.to_string_lossy(),
                    );
                let resolved_path = PathBuf::from(&resolved);

                if visited.insert(resolved_path.clone()) {
                    let module_src = std::fs::read_to_string(&resolved)
                        .map_err(|e| format!("cannot read imported module '{}': {}", path, e))?;
                    let (_module_ast, sub_hir) =
                        check_ownership_and_parse_in(&module_src, config, Some(&resolved))
                            .map_err(|e| {
                                format!("failed to gate imported module '{}': {}", path, e)
                            })?;
                    let parent_dir = resolved_path.parent().unwrap_or(Path::new("."));
                    let sub = load_imports_from_hir(sub_hir, parent_dir, visited, config)?;
                    result.functions.extend(sub.functions);
                    result.classes.extend(sub.classes);
                    result.enums.extend(sub.enums);
                }
                result.bindings.push(NamespaceBinding {
                    alias,
                    path: resolved_path.clone(),
                });
            }
            HirStmt::ImportDefault { path, alias } => {
                let resolved =
                    crate::execution::runtime_core::interpreter_impl::utilities::resolve_path(
                        &path,
                        &base_dir.to_string_lossy(),
                    );
                let resolved_path = PathBuf::from(&resolved);

                if visited.insert(resolved_path.clone()) {
                    let module_src = std::fs::read_to_string(&resolved)
                        .map_err(|e| format!("cannot read imported module '{}': {}", path, e))?;
                    let (_module_ast, sub_hir) =
                        check_ownership_and_parse_in(&module_src, config, Some(&resolved))
                            .map_err(|e| {
                                format!("failed to gate imported module '{}': {}", path, e)
                            })?;
                    let parent_dir = resolved_path.parent().unwrap_or(Path::new("."));
                    let sub = load_imports_from_hir(sub_hir, parent_dir, visited, config)?;
                    result.functions.extend(sub.functions);
                    result.classes.extend(sub.classes);
                    result.enums.extend(sub.enums);
                }
                result.bindings.push(NamespaceBinding {
                    alias,
                    path: resolved_path.clone(),
                });
            }
            HirStmt::ImportNames { path, names } => {
                let resolved =
                    crate::execution::runtime_core::interpreter_impl::utilities::resolve_path(
                        &path,
                        &base_dir.to_string_lossy(),
                    );
                let resolved_path = PathBuf::from(&resolved);

                if visited.insert(resolved_path.clone()) {
                    let module_src = std::fs::read_to_string(&resolved)
                        .map_err(|e| format!("cannot read imported module '{}': {}", path, e))?;
                    let (_module_ast, sub_hir) =
                        check_ownership_and_parse_in(&module_src, config, Some(&resolved))
                            .map_err(|e| {
                                format!("failed to gate imported module '{}': {}", path, e)
                            })?;
                    let parent_dir = resolved_path.parent().unwrap_or(Path::new("."));
                    let sub = load_imports_from_hir(sub_hir, parent_dir, visited, config)?;
                    result.functions.extend(sub.functions);
                    result.classes.extend(sub.classes);
                    result.enums.extend(sub.enums);
                }
                for name in names {
                    result
                        .named_imports
                        .insert(name, (path.clone(), resolved.clone()));
                }
            }
            other => retained.push(other),
        }
    }
    hir.statements = retained;
    result.functions.extend(hir.functions);
    result.classes.extend(hir.classes);
    result.enums.extend(hir.enums);
    Ok(result)
}

/// Merge the imported declarations into the main module's HIR.
///
/// Only the symbols already filtered by `load_imports` (the tree-shaken subset) are
/// present, so this merges exactly the symbols `main` uses.
pub fn merge_imports_into_hir(main_hir: &mut HirModule, imports: &ImportLoadResult) {
    main_hir.functions.extend(imports.functions.clone());
    main_hir.classes.extend(imports.classes.clone());
    main_hir.enums.extend(imports.enums.clone());
}

/// Build a namespace object (as an `ast::Value::Object`) holding the imported module's
/// exported constants that `main` actually references. Suitable for binding as a
/// prebound global, so `main` reads only the constants it uses.
pub fn build_namespace_object(
    src: &str,
    root_dir: &Path,
    usage: &ImportUsage,
) -> Result<crate::parsing::ast::Value, String> {
    use crate::parsing::ast::{StmtKind, Value};
    use crate::utils::collections::FastMap;
    use std::sync::Arc;

    load_imports(src, root_dir)?;

    let mut map: FastMap<String, Value> = FastMap::default();
    // Add exported top-level consts that `main` actually references (e.g. `export const PI = 3.14`).
    let mut lexer = Lexer::new(src);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("import lex error: {}", e))?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser
        .parse_program()
        .map_err(|e| format!("import parse error: {}", e))?;
    for stmt in &ast {
        if let StmtKind::Let(name, init, _ty, is_export, _is_const, _readonly) = &stmt.kind {
            if *is_export && usage.should_import(name) {
                if let Some(init) = init {
                    if let Some(v) = expr_to_value(init) {
                        map.insert(name.clone(), v);
                    }
                }
            }
        }
    }

    Ok(Value::Object(Arc::new(map)))
}

/// Build a namespace object for a single import alias by locating that import in `src`,
/// reading the referenced module, and collecting only the exported constants that
/// `main` references. Used to bind `math` -> `{ PI: 3.14 }` as a prebound global.
pub fn build_namespace_object_for_module(
    src: &str,
    root_dir: &Path,
    alias: &str,
) -> Result<crate::parsing::ast::Value, String> {
    let usage = load_imports(src, root_dir)?
        .usage
        .get(alias)
        .cloned()
        .unwrap_or_default();

    build_namespace_object_for_module_with_usage(src, root_dir, alias, &usage)
}

/// Build a namespace object for a single import alias using an explicit usage set.
pub fn build_namespace_object_for_module_with_usage(
    src: &str,
    root_dir: &Path,
    alias: &str,
    usage: &ImportUsage,
) -> Result<crate::parsing::ast::Value, String> {
    use crate::parsing::ast::{StmtKind, Value};
    use crate::utils::collections::FastMap;
    use std::sync::Arc;

    let mut lexer = Lexer::new(src);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("import lex error: {}", e))?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser
        .parse_program()
        .map_err(|e| format!("import parse error: {}", e))?;

    // Find the import statement matching this alias.
    let target: Option<(String, bool)> = ast.iter().find_map(|s| match &s.kind {
        StmtKind::Import { path, alias: a } if a == alias => Some((path.clone(), false)),
        StmtKind::ImportDefault { path, alias: a } if a == alias => Some((path.clone(), true)),
        _ => None,
    });

    let (path, _is_default) = target.ok_or_else(|| format!("import alias '{alias}' not found"))?;
    let resolved = crate::execution::runtime_core::interpreter_impl::utilities::resolve_path(
        &path,
        &root_dir.to_string_lossy(),
    );
    let module_src = std::fs::read_to_string(&resolved).map_err(|e| {
        format!("cannot read imported module '{path}' (resolved '{resolved}'): {e}")
    })?;

    let mut lexer = Lexer::new(&module_src);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("import lex error: {}", e))?;
    let mut parser = Parser::new(tokens, None);
    let module_ast = parser
        .parse_program()
        .map_err(|e| format!("import parse error: {}", e))?;

    let mut map: FastMap<String, Value> = FastMap::default();
    for stmt in &module_ast {
        if let StmtKind::Let(name, init, _ty, is_export, _is_const, _readonly) = &stmt.kind {
            if *is_export && usage.should_import(name) {
                if let Some(init) = init {
                    if let Some(v) = expr_to_value(init) {
                        map.insert(name.clone(), v);
                    }
                }
            }
        }
    }

    Ok(Value::Object(Arc::new(map)))
}

/// Analyze `main`'s HIR to determine which symbols of each import are actually used.
///
/// Returns a map: import alias -> `ImportUsage` describing the referenced members.
fn collect_import_usage(
    hir: &HirModule,
    import_stmts: &[TopImport],
) -> HashMap<String, ImportUsage> {
    let mut usage: HashMap<String, ImportUsage> = HashMap::new();

    // Register an entry for every import so that aliases are tracked even if unused.
    for imp in import_stmts {
        let mut u = ImportUsage::default();
        if let Some(names) = &imp.names {
            // `from "p" import a, b;` — the explicit names ARE the selection.
            for name in names {
                u.direct_names.insert(name.clone());
            }
        }
        usage.insert(imp.alias.clone(), u);
    }

    // Walk main's HIR to find `alias.X` member references and direct import-name uses.
    let mut collector = UsageCollector { usage: &mut usage };
    collector.collect_module(hir);

    usage
}

/// Walks HIR statements/expressions to record which import members are referenced.
struct UsageCollector<'a> {
    usage: &'a mut HashMap<String, ImportUsage>,
}

impl<'a> UsageCollector<'a> {
    fn collect_module(&mut self, hir: &HirModule) {
        // Top-level statements.
        for stmt in &hir.statements {
            self.collect_stmt(stmt);
        }
        // Top-level functions (e.g. `fn main()`), whose bodies may use imports.
        for f in &hir.functions {
            self.collect_block_of_stmts(&f.body);
        }
        // Class methods (static and instance) may reference imported namespaces.
        for c in &hir.classes {
            for m in &c.methods {
                self.collect_block_of_stmts(&m.body);
            }
            for m in &c.static_methods {
                self.collect_block_of_stmts(&m.body);
            }
        }
    }

    fn collect_stmt(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Let { init, .. } => {
                if let Some(e) = init {
                    self.collect_expr(e);
                }
            }
            HirStmt::LetTuple { init, .. } => {
                if let Some(e) = init {
                    self.collect_expr(e);
                }
            }
            HirStmt::Assign {
                target,
                value,
                is_move: _,
            } => {
                self.collect_expr(target);
                self.collect_expr(value);
            }
            HirStmt::Expr(expr) => {
                self.collect_expr(expr);
            }
            HirStmt::Return(expr) => {
                if let Some(e) = expr {
                    self.collect_expr(e);
                }
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.collect_expr(cond);
                self.collect_stmt(then_branch);
                if let Some(e) = else_branch {
                    self.collect_stmt(e);
                }
            }
            HirStmt::While { cond, body } => {
                self.collect_expr(cond);
                self.collect_stmt(body);
            }
            HirStmt::ForIn { iter, body, .. } => {
                self.collect_expr(iter);
                self.collect_stmt(body);
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.collect_stmt(s);
                }
            }
            HirStmt::FunctionDef { body, .. } => {
                self.collect_block_of_stmts(body);
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.collect_stmt(try_block);
                self.collect_stmt(catch_block);
            }
            HirStmt::Throw(expr) => {
                self.collect_expr(expr);
            }
            HirStmt::Extend { methods, .. } => {
                for m in methods {
                    self.collect_block_of_stmts(&m.body);
                }
            }
            HirStmt::Region { body, .. } => {
                self.collect_stmt(body);
            }
            HirStmt::Defer(block) => {
                self.collect_stmt(block);
            }
            HirStmt::Unsafe(stmt) => {
                self.collect_stmt(stmt);
            }
            // Import statements themselves are not "usage" of their members.
            HirStmt::Import { .. }
            | HirStmt::ImportDefault { .. }
            | HirStmt::ImportNames { .. }
            | HirStmt::ClassDef(_)
            | HirStmt::Break
            | HirStmt::Continue
            | HirStmt::Jump(_) => {}
        }
    }

    fn collect_block_of_stmts(&mut self, body: &Arc<Vec<HirStmt>>) {
        for s in body.iter() {
            self.collect_stmt(s);
        }
    }

    fn collect_expr(&mut self, expr: &crate::parsing::hir::HirExpr) {
        use crate::parsing::hir::HirExpr;
        match expr {
            HirExpr::LoadVar(name) => {
                // A direct import name used as a value (e.g. `from "m" import a; a()`).
                for u in self.usage.values_mut() {
                    if u.direct_names.contains(name) {
                        u.direct_names.remove(name);
                        u.members.insert(name.clone());
                    }
                }
            }
            HirExpr::Call(callee, args, _type_args) => {
                // `alias.method(...)` — record the member.
                if let HirExpr::MemberAccess(obj, method) = callee.as_ref() {
                    if let HirExpr::LoadVar(alias) = obj.as_ref() {
                        if let Some(u) = self.usage.get_mut(alias) {
                            u.members.insert(method.clone());
                        }
                    }
                }
                // `alias(...)` — alias used directly as a value (default import).
                if let HirExpr::LoadVar(alias) = callee.as_ref() {
                    if let Some(u) = self.usage.get_mut(alias) {
                        u.used_as_value = true;
                    }
                }
                self.collect_expr(callee);
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            HirExpr::MemberAccess(obj, field) => {
                // `alias.field` — record the member.
                if let HirExpr::LoadVar(alias) = obj.as_ref() {
                    if let Some(u) = self.usage.get_mut(alias) {
                        u.members.insert(field.clone());
                    }
                }
                self.collect_expr(obj);
            }
            HirExpr::BinaryOp(l, _op, r) => {
                self.collect_expr(l);
                self.collect_expr(r);
            }
            HirExpr::UnaryOp(_op, operand) => {
                self.collect_expr(operand);
            }
            HirExpr::MethodCall(obj, method, args) => {
                // `alias.method(...)` — record the imported member.
                if let HirExpr::LoadVar(alias) = obj.as_ref() {
                    if let Some(u) = self.usage.get_mut(alias) {
                        u.members.insert(method.clone());
                    }
                }
                self.collect_expr(obj);
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            HirExpr::Conditional(cond, then_expr, else_expr) => {
                self.collect_expr(cond);
                self.collect_expr(then_expr);
                self.collect_expr(else_expr);
            }
            HirExpr::Lambda(_params, body, _is_async) => {
                self.collect_block_of_stmts(body);
            }
            HirExpr::ArrayLiteral(elements) => {
                for e in elements {
                    self.collect_expr(e);
                }
            }
            HirExpr::DictLiteral(entries) => {
                for (k, v) in entries {
                    self.collect_expr(k);
                    self.collect_expr(v);
                }
            }
            HirExpr::SetLiteral(elements) => {
                for e in elements {
                    self.collect_expr(e);
                }
            }
            HirExpr::TupleLiteral(elements) => {
                for e in elements {
                    self.collect_expr(e);
                }
            }
            HirExpr::ObjectLiteral(_entries) => {}
            HirExpr::AssignTuple(_names, target) => {
                self.collect_expr(target);
            }
            HirExpr::AssignObject(_props, target) => {
                self.collect_expr(target);
            }
            HirExpr::StructLiteral(_class, fields) => {
                for (_name, v) in fields {
                    self.collect_expr(v);
                }
            }
            HirExpr::Index(obj, idx) => {
                self.collect_expr(obj);
                self.collect_expr(idx);
            }
            HirExpr::SetMember(obj, _field, value) => {
                self.collect_expr(obj);
                self.collect_expr(value);
            }
            HirExpr::NewInstance(_class, args) => {
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            HirExpr::OptionalGet(inner, _field) => {
                self.collect_expr(inner);
            }
            HirExpr::NonNull(inner) | HirExpr::Deref(inner) | HirExpr::Move(inner) => {
                self.collect_expr(inner);
            }
            HirExpr::BorrowImmut(inner)
            | HirExpr::BorrowMut(inner)
            | HirExpr::Borrow(inner, _)
            | HirExpr::Share(inner)
            | HirExpr::Downgrade(inner) => {
                self.collect_expr(inner);
            }
            HirExpr::Alloc(_ty, size) => {
                self.collect_expr(size);
            }
            HirExpr::Free(ptr) => {
                self.collect_expr(ptr);
            }
            HirExpr::Cast(inner, _ty) => {
                self.collect_expr(inner);
            }
            HirExpr::Range(start, end, _inclusive) => {
                self.collect_expr(start);
                self.collect_expr(end);
            }
            HirExpr::Spread(inner) => {
                self.collect_expr(inner);
            }
            HirExpr::Format(inner, _spec) => {
                self.collect_expr(inner);
            }
            HirExpr::This | HirExpr::Super => {}
            HirExpr::Await(inner) | HirExpr::Spawn(inner) => {
                self.collect_expr(inner);
            }
            HirExpr::Literal(_)
            | HirExpr::StoreVar(_, _)
            | HirExpr::Update(_, _, _)
            | HirExpr::Match(_, _) => {}
        }
    }
}

/// Best-effort conversion of an AST expression to an `ast::Value` for namespace constants.
fn expr_to_value(expr: &crate::parsing::ast::Expr) -> Option<crate::parsing::ast::Value> {
    use crate::parsing::ast::{ExprKind, Value};
    match &expr.kind {
        ExprKind::Literal(v) => match v {
            Value::I64(n) => Some(Value::I64(*n)),
            Value::F64(n) => Some(Value::F64(*n)),
            Value::I32(n) => Some(Value::I64(*n as i64)),
            Value::I16(n) => Some(Value::I64(*n as i64)),
            Value::I8(n) => Some(Value::I64(*n as i64)),
            Value::U64(n) => Some(Value::I64(*n as i64)),
            Value::U32(n) => Some(Value::I64(*n as i64)),
            Value::U16(n) => Some(Value::I64(*n as i64)),
            Value::U8(n) => Some(Value::I64(*n as i64)),
            Value::Bool(b) => Some(Value::Bool(*b)),
            Value::Str(s) => Some(Value::Str(s.clone())),
            _ => None,
        },
        _ => None,
    }
}
