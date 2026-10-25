//! Static module linking shared by native JIT, native codegen and WebAssembly.
//!
//! Resolve bindings before backend lowering, rather than teaching each backend
//! that an arbitrary receiver named `math` is a module. Internal symbols have
//! module-unique names; dependencies are retained on demand, including private
//! helpers. Scalar constants are substituted without evaluating language code.
use crate::parsing::ast::{ClassDecl, Expr, ExprKind, Function, Pattern, Stmt, StmtKind};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
enum Binding {
    Namespace(usize),
    Symbol(usize, String),
}

#[derive(Default)]
struct Module {
    declarations: BTreeMap<String, Stmt>,
    exports: BTreeMap<String, String>,
    imports: HashMap<String, Binding>,
}

#[derive(Default)]
struct Linker {
    modules: Vec<Module>,
    paths: HashMap<PathBuf, usize>,
    emitted: HashSet<(usize, String)>,
    constants: HashSet<(usize, String)>,
    declarations: Vec<Stmt>,
    initializers: Vec<Stmt>,
    pending_initializers: Vec<(usize, Stmt)>,
}

/// The input must already have passed decorator/ownership/RAII checks.
/// Imported files pass the same gates once, before any of their code is used.
pub fn link_ast(ast: Vec<Stmt>, base_dir: &Path) -> Result<Vec<Stmt>, String> {
    if !ast.iter().any(|s| {
        matches!(
            &s.kind,
            StmtKind::Import { .. } | StmtKind::ImportDefault { .. } | StmtKind::ImportNames { .. }
        )
    }) {
        return Ok(ast);
    }
    let mut linker = Linker::default();
    linker.modules.push(Module::default());
    linker.index(0, &ast)?;
    linker.imports(0, &ast, base_dir)?;
    // Resolve initializer bodies only after the entire graph's bindings exist,
    // including back-edges in cyclic graphs.
    for (id, mut stmt) in std::mem::take(&mut linker.pending_initializers) {
        linker.stmt(id, &mut stmt, &mut Vec::new())?;
        linker.initializers.push(stmt);
    }
    let mut root = ast;
    root.retain(|s| !is_import(s));
    linker.block(0, &mut root, &mut Vec::new(), false)?;
    let mut result = linker.declarations;
    result.extend(linker.initializers);
    result.extend(root);
    Ok(result)
}

fn is_import(s: &Stmt) -> bool {
    matches!(
        &s.kind,
        StmtKind::Import { .. }
            | StmtKind::ImportDefault { .. }
            | StmtKind::ImportNames { .. }
            | StmtKind::ExportDefault(_)
    )
}

fn declaration(s: &Stmt) -> Option<(&str, bool)> {
    match &s.kind {
        StmtKind::Function(f, export) => Some((&f.name, *export)),
        StmtKind::ExportDefaultFunction(f) => Some((&f.name, false)),
        StmtKind::Class(c, export) => Some((&c.name, *export)),
        StmtKind::ExportDefaultClass(c) => Some((&c.name, false)),
        StmtKind::Let(name, _, _, export, _, _) => Some((name, *export)),
        StmtKind::Struct(d, export) => Some((&d.name, *export)),
        StmtKind::Enum(d, export) => Some((&d.name, *export)),
        StmtKind::TypeAlias(d, export) => Some((&d.name, *export)),
        _ => None,
    }
}

impl Linker {
    fn index(&mut self, id: usize, ast: &[Stmt]) -> Result<(), String> {
        let module = &mut self.modules[id];
        for s in ast {
            if let Some((name, export)) = declaration(s) {
                if module
                    .declarations
                    .insert(name.to_owned(), s.clone())
                    .is_some()
                {
                    return Err(format!("duplicate module declaration '{name}'"));
                }
                if export {
                    module.exports.insert(name.to_owned(), name.to_owned());
                }
            }
            let default = match &s.kind {
                StmtKind::ExportDefaultFunction(f) => Some(&f.name),
                StmtKind::ExportDefaultClass(c) => Some(&c.name),
                StmtKind::ExportDefault(name) => Some(name),
                _ => None,
            };
            if let Some(name) = default {
                if module
                    .exports
                    .insert("default".into(), name.clone())
                    .is_some()
                {
                    return Err("module has multiple default exports".into());
                }
            }
        }
        Ok(())
    }

    fn load(&mut self, path: &str, base: &Path) -> Result<usize, String> {
        let mut candidate = base.join(path);
        if candidate.extension().is_none() && !candidate.is_file() {
            for extension in ["adesh", "adl", "ind"] {
                let probe = candidate.with_extension(extension);
                if probe.is_file() {
                    candidate = probe;
                    break;
                }
            }
        }
        let file = candidate.canonicalize().map_err(|e| {
            format!(
                "cannot resolve import '{path}' from '{}': {e}",
                base.display()
            )
        })?;
        if let Some(id) = self.paths.get(&file) {
            return Ok(*id);
        }
        let source = std::fs::read_to_string(&file)
            .map_err(|e| format!("cannot read import '{}': {e}", file.display()))?;
        crate::types::type_system::check_module_in(&source, Some(&file.to_string_lossy()))
            .map_err(|e| format!("import '{}': {e}", file.display()))?;
        let (ast, _) = crate::cli::parsing::check_ownership_and_parse_in(
            &source,
            &Default::default(),
            Some(&file.to_string_lossy()),
        )?;
        let id = self.modules.len();
        self.paths.insert(file.clone(), id);
        self.modules.push(Module::default());
        self.index(id, &ast)?;
        self.imports(id, &ast, file.parent().unwrap_or(base))?;
        for s in ast {
            match &s.kind {
                StmtKind::Let(name, Some(init), ..) if !potential_scalar_constant(init) => {
                    return Err(format!(
                        "module binding '{name}' requires runtime initialization, which static imports do not yet support"
                    ));
                }
                StmtKind::ShareDeclaration(..)
                | StmtKind::StrongDeclaration(..)
                | StmtKind::WeakDeclaration(..) => {
                    return Err(
                        "managed module-level bindings are not yet supported by static imports"
                            .into(),
                    );
                }
                _ => {}
            }
            if declaration(&s).is_none() && !is_import(&s) {
                self.pending_initializers.push((id, s));
            }
        }
        Ok(id)
    }

    fn imports(&mut self, id: usize, ast: &[Stmt], base: &Path) -> Result<(), String> {
        for s in ast {
            let mut bindings = Vec::new();
            match &s.kind {
                StmtKind::Import { path, alias } => {
                    let target = self.load(path, base)?;
                    bindings.push((alias.clone(), Binding::Namespace(target)));
                }
                StmtKind::ImportDefault { path, alias } => {
                    let target = self.load(path, base)?;
                    let name = self.export(target, "default")?;
                    bindings.push((alias.clone(), Binding::Symbol(target, name)));
                }
                StmtKind::ImportNames { path, names } => {
                    let target = self.load(path, base)?;
                    for name in names {
                        let local = self.export(target, name)?;
                        bindings.push((name.clone(), Binding::Symbol(target, local)));
                    }
                }
                _ => {}
            }
            for (name, binding) in bindings {
                if self.modules[id].declarations.contains_key(&name)
                    || self.modules[id]
                        .imports
                        .insert(name.clone(), binding)
                        .is_some()
                {
                    return Err(format!("duplicate import binding '{name}'"));
                }
            }
        }
        Ok(())
    }

    fn export(&self, id: usize, name: &str) -> Result<String, String> {
        self.modules[id]
            .exports
            .get(name)
            .cloned()
            .ok_or_else(|| format!("module does not export '{name}'"))
    }

    fn symbol(
        &mut self,
        id: usize,
        name: &str,
        span: &crate::parsing::ast::Span,
    ) -> Result<Expr, String> {
        let key = (id, name.to_owned());
        let mut stmt = self.modules[id]
            .declarations
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unresolved module symbol '{name}'"))?;
        if let StmtKind::Let(_, init, _, _, is_const, _) = &stmt.kind {
            if !is_const {
                return Err(format!(
                    "native import of mutable module variable '{name}' is not supported"
                ));
            }
            let mut init = init
                .clone()
                .ok_or_else(|| format!("constant '{name}' has no initializer"))?;
            if !self.constants.insert(key.clone()) {
                return Err(format!("cyclic module constant '{name}'"));
            }
            self.expr(id, &mut init, &mut Vec::new())?;
            self.constants.remove(&key);
            if !scalar_constant(&init) {
                return Err(format!(
                    "module constant '{name}' requires runtime initialization, which is not supported by static imports"
                ));
            }
            return Ok(init);
        }
        let linked_name = format!("__module_{id}_{name}");
        if self.emitted.insert(key) {
            match &mut stmt.kind {
                StmtKind::Function(f, _) | StmtKind::ExportDefaultFunction(f) => {
                    self.function(id, f, &mut Vec::new())?;
                    f.name = linked_name.clone();
                    stmt.kind = StmtKind::Function(f.clone(), false);
                }
                StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
                    self.class(id, c, &mut Vec::new())?;
                    c.name = linked_name.clone();
                    stmt.kind = StmtKind::Class(c.clone(), false);
                }
                StmtKind::Struct(d, _) => {
                    d.name = linked_name.clone();
                    for (_, ty) in &mut d.fields {
                        self.ty(id, ty)?;
                    }
                }
                StmtKind::Enum(d, _) => {
                    d.name = linked_name.clone();
                    for (_, ty) in &mut d.variants {
                        if let Some(ty) = ty {
                            self.ty(id, ty)?;
                        }
                    }
                }
                StmtKind::TypeAlias(d, _) => {
                    d.name = linked_name.clone();
                    for (_, _, ty) in &mut d.fields {
                        self.ty(id, ty)?;
                    }
                }
                _ => return Err(format!("unsupported native import declaration '{name}'")),
            }
            self.declarations.push(stmt);
        }
        Ok(Expr {
            kind: ExprKind::Variable(linked_name),
            span: span.clone(),
        })
    }

    fn resolve(
        &mut self,
        id: usize,
        name: &str,
        span: &crate::parsing::ast::Span,
    ) -> Result<Option<Expr>, String> {
        match self.modules[id].imports.get(name).cloned() {
            Some(Binding::Symbol(target, symbol)) => Ok(Some(self.symbol(target, &symbol, span)?)),
            Some(Binding::Namespace(_)) => Err(format!(
                "module namespace '{name}' must be accessed through an exported member"
            )),
            None if id != 0 && self.modules[id].declarations.contains_key(name) => {
                Ok(Some(self.symbol(id, name, span)?))
            }
            None => Ok(None),
        }
    }

    fn ty(&mut self, id: usize, ty: &mut String) -> Result<(), String> {
        let trimmed = ty.trim().to_string();
        if trimmed.ends_with('?') {
            let mut inner = trimmed[..trimmed.len() - 1].to_string();
            self.ty(id, &mut inner)?;
            *ty = format!("{}?", inner);
            return Ok(());
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner_str = &trimmed[1..trimmed.len() - 1];
            if let Some((elem, size)) = inner_str.split_once(';') {
                let mut elem_ty = elem.trim().to_string();
                self.ty(id, &mut elem_ty)?;
                *ty = format!("[{}; {}]", elem_ty, size.trim());
            } else {
                let mut elem_ty = inner_str.trim().to_string();
                self.ty(id, &mut elem_ty)?;
                *ty = format!("[{}]", elem_ty);
            }
            return Ok(());
        }
        if let Some(open) = trimmed.find('<') {
            if trimmed.ends_with('>') {
                let base = &trimmed[..open];
                let mut resolved_base = base.trim().to_string();
                if let Some(expr) = self.resolve(id, &resolved_base, &Default::default())? {
                    if let ExprKind::Variable(name) = expr.kind {
                        resolved_base = name;
                    }
                }
                let inner = &trimmed[open + 1..trimmed.len() - 1];
                let mut resolved_args = Vec::new();
                for arg in inner.split(',') {
                    let mut arg_ty = arg.trim().to_string();
                    self.ty(id, &mut arg_ty)?;
                    resolved_args.push(arg_ty);
                }
                *ty = format!("{}<{}>", resolved_base, resolved_args.join(", "));
                return Ok(());
            }
        }
        if let Some(expr) = self.resolve(id, ty, &Default::default())? {
            if let ExprKind::Variable(name) = expr.kind {
                *ty = name;
            }
        }
        Ok(())
    }

    fn function(
        &mut self,
        id: usize,
        f: &mut Function,
        scopes: &mut Vec<HashSet<String>>,
    ) -> Result<(), String> {
        for decorator in &mut f.decorators {
            self.expr(id, decorator, scopes)?;
        }
        scopes.push(f.params.iter().map(|p| p.0.clone()).collect());
        for (_, default, ty) in &mut f.params {
            if let Some(e) = default {
                self.expr(id, e, scopes)?;
            }
            if let Some(ty) = ty {
                self.ty(id, ty)?;
            }
        }
        if let Some(ty) = &mut f.ret_type {
            self.ty(id, ty)?;
        }
        self.block(id, Arc::make_mut(&mut f.body).as_mut_slice(), scopes, true)?;
        scopes.pop();
        Ok(())
    }

    fn class(
        &mut self,
        id: usize,
        c: &mut ClassDecl,
        scopes: &mut Vec<HashSet<String>>,
    ) -> Result<(), String> {
        if let Some(base) = &mut c.extends {
            self.ty(id, base)?;
        }
        for d in &mut c.decorators {
            self.expr(id, d, scopes)?;
        }
        for f in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
            self.function(id, f, scopes)?;
        }
        for (_, value, decorators) in &mut c.static_properties {
            self.expr(id, value, scopes)?;
            for d in decorators {
                self.expr(id, d, scopes)?;
            }
        }
        for (_, ty, _, decorators, init) in &mut c.fields {
            self.ty(id, ty)?;
            for d in decorators {
                self.expr(id, d, scopes)?;
            }
            if let Some(e) = init {
                self.expr(id, e, scopes)?;
            }
        }
        Ok(())
    }

    fn block(
        &mut self,
        id: usize,
        stmts: &mut [Stmt],
        scopes: &mut Vec<HashSet<String>>,
        local: bool,
    ) -> Result<(), String> {
        if local {
            let mut names = HashSet::new();
            for s in stmts.iter() {
                local_names(s, &mut names);
            }
            scopes.push(names);
        }
        for s in stmts {
            self.stmt(id, s, scopes)?;
        }
        if local {
            scopes.pop();
        }
        Ok(())
    }

    fn stmt(
        &mut self,
        id: usize,
        s: &mut Stmt,
        scopes: &mut Vec<HashSet<String>>,
    ) -> Result<(), String> {
        match &mut s.kind {
            StmtKind::Let(_, init, ty, ..) => {
                if let Some(e) = init {
                    self.expr(id, e, scopes)?;
                }
                if let Some(ty) = ty {
                    self.ty(id, ty)?;
                }
            }
            StmtKind::LetTuple(_, _, init, ..)
            | StmtKind::LetObject(_, init, ..)
            | StmtKind::Return(init) => {
                if let Some(e) = init {
                    self.expr(id, e, scopes)?;
                }
            }
            StmtKind::ShareDeclaration(d, _) => self.expr(id, &mut d.expr, scopes)?,
            StmtKind::StrongDeclaration(d, _) => self.expr(id, &mut d.expr, scopes)?,
            StmtKind::WeakDeclaration(d, _) => self.expr(id, &mut d.expr, scopes)?,
            StmtKind::ExprStmt(e) | StmtKind::Jump(e) => self.expr(id, e, scopes)?,
            StmtKind::Block(body) => self.block(id, body, scopes, true)?,
            StmtKind::Function(f, _) | StmtKind::ExportDefaultFunction(f) => {
                self.function(id, f, scopes)?
            }
            StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => self.class(id, c, scopes)?,
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(id, cond, scopes)?;
                self.stmt(id, then_branch, scopes)?;
                if let Some(s) = else_branch {
                    self.stmt(id, s, scopes)?;
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(id, cond, scopes)?;
                self.stmt(id, body, scopes)?;
            }
            StmtKind::ForIn { name, iter, body } => {
                self.expr(id, iter, scopes)?;
                scopes.push(HashSet::from([name.clone()]));
                self.stmt(id, body, scopes)?;
                scopes.pop();
            }
            StmtKind::TryCatch {
                try_block,
                err_name,
                catch_block,
            } => {
                self.stmt(id, try_block, scopes)?;
                scopes.push(HashSet::from([err_name.clone()]));
                self.stmt(id, catch_block, scopes)?;
                scopes.pop();
            }
            StmtKind::Region { body, .. } | StmtKind::UnsafeBlock(body) | StmtKind::Defer(body) => {
                self.stmt(id, body, scopes)?
            }
            StmtKind::Extend(_, target, _, methods, _) => {
                self.ty(id, target)?;
                for method in methods {
                    self.function(id, method, scopes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn expr(
        &mut self,
        id: usize,
        e: &mut Expr,
        scopes: &mut Vec<HashSet<String>>,
    ) -> Result<(), String> {
        // Resolve namespace members before walking the receiver as a variable.
        if let ExprKind::Get(target, field) = &e.kind {
            if let ExprKind::Variable(alias) = &target.kind {
                if !shadowed(alias, scopes) {
                    if let Some(Binding::Namespace(target_id)) =
                        self.modules[id].imports.get(alias).cloned()
                    {
                        let symbol = self.export(target_id, field)?;
                        *e = self.symbol(target_id, &symbol, &e.span)?;
                        return Ok(());
                    }
                }
            }
        }
        if let ExprKind::Call(target, _, _) = &mut e.kind {
            if let ExprKind::Variable(alias) = &target.kind {
                if !shadowed(alias, scopes) {
                    if let Some(Binding::Namespace(target_id)) =
                        self.modules[id].imports.get(alias).cloned()
                    {
                        let symbol = self.export(target_id, "default")?;
                        **target = self.symbol(target_id, &symbol, &target.span)?;
                    }
                }
            }
        }
        match &mut e.kind {
            ExprKind::Variable(name) => {
                if !shadowed(name, scopes) {
                    if let Some(replacement) = self.resolve(id, name, &e.span)? {
                        *e = replacement;
                    }
                }
            }
            ExprKind::Assign(name, value) => {
                if !shadowed(name, scopes)
                    && (self.modules[id].imports.contains_key(name)
                        || (id != 0 && self.modules[id].declarations.contains_key(name)))
                {
                    return Err(format!("cannot assign to static module binding '{name}'"));
                }
                self.expr(id, value, scopes)?;
            }
            ExprKind::Call(callee, args, _)
            | ExprKind::OptCall(callee, args, _)
            | ExprKind::New(callee, args) => {
                self.expr(id, callee, scopes)?;
                for arg in args {
                    self.expr(id, arg, scopes)?;
                }
            }
            ExprKind::Binary(a, _, b)
            | ExprKind::Logical(a, _, b)
            | ExprKind::AssignOp(a, _, b)
            | ExprKind::Index(a, b)
            | ExprKind::Range(a, b, _) => {
                self.expr(id, a, scopes)?;
                self.expr(id, b, scopes)?;
            }
            ExprKind::Unary(_, a)
            | ExprKind::Grouping(a)
            | ExprKind::Await(a)
            | ExprKind::Spawn(a)
            | ExprKind::Throw(a)
            | ExprKind::Spread(a)
            | ExprKind::NonNull(a)
            | ExprKind::Try(a)
            | ExprKind::Update(_, _, a)
            | ExprKind::Format(a, _)
            | ExprKind::Cast(a, _)
            | ExprKind::Get(a, _)
            | ExprKind::OptGet(a, _)
            | ExprKind::AssignTuple(_, a)
            | ExprKind::AssignObject(_, a) => self.expr(id, a, scopes)?,
            ExprKind::Set(target, _, value) => {
                self.expr(id, target, scopes)?;
                self.expr(id, value, scopes)?;
            }
            ExprKind::Conditional(a, b, c) => {
                self.expr(id, a, scopes)?;
                self.expr(id, b, scopes)?;
                self.expr(id, c, scopes)?;
            }
            ExprKind::Array(elements)
            | ExprKind::Tuple(elements)
            | ExprKind::SetLiteral(elements) => {
                for e in elements {
                    self.expr(id, e, scopes)?;
                }
            }
            ExprKind::Object(fields) => {
                for (_, e) in fields {
                    self.expr(id, e, scopes)?;
                }
            }
            ExprKind::StructLiteral(name, fields) => {
                self.ty(id, name)?;
                for (_, e) in fields {
                    self.expr(id, e, scopes)?;
                }
            }
            ExprKind::Fn(params, body, _) => {
                scopes.push(params.iter().map(|p| p.0.clone()).collect());
                for (_, default, _) in params {
                    if let Some(e) = default {
                        self.expr(id, e, scopes)?;
                    }
                }
                self.block(id, Arc::make_mut(body).as_mut_slice(), scopes, true)?;
                scopes.pop();
            }
            ExprKind::Match(target, arms) => {
                self.expr(id, target, scopes)?;
                for (pattern, value) in arms {
                    let mut names = HashSet::new();
                    pattern_names(pattern, &mut names);
                    scopes.push(names);
                    self.expr(id, value, scopes)?;
                    scopes.pop();
                }
            }
            ExprKind::Literal(_) => {}
        }
        Ok(())
    }
}

fn shadowed(name: &str, scopes: &[HashSet<String>]) -> bool {
    scopes.iter().rev().any(|s| s.contains(name))
}

fn local_names(stmt: &Stmt, names: &mut HashSet<String>) {
    if let Some((name, _)) = declaration(stmt) {
        names.insert(name.to_owned());
    }
    match &stmt.kind {
        StmtKind::LetTuple(items, ..) => names.extend(items.iter().cloned()),
        StmtKind::LetObject(items, ..) => {
            names.extend(
                items
                    .iter()
                    .map(|(key, alias)| alias.as_ref().unwrap_or(key).clone()),
            );
        }
        StmtKind::ShareDeclaration(d, _) => {
            names.insert(d.name.clone());
        }
        StmtKind::StrongDeclaration(d, _) => {
            names.insert(d.name.clone());
        }
        StmtKind::WeakDeclaration(d, _) => {
            names.insert(d.name.clone());
        }
        _ => {}
    }
}

fn pattern_names(pattern: &Pattern, names: &mut HashSet<String>) {
    match pattern {
        Pattern::Variable(name) => {
            names.insert(name.clone());
        }
        Pattern::Or(a, b) => {
            pattern_names(a, names);
            pattern_names(b, names);
        }
        Pattern::EnumVariant(_, args) => {
            for arg in args {
                pattern_names(arg, names);
            }
        }
        _ => {}
    }
}

fn scalar_constant(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(_) => true,
        ExprKind::Grouping(a) | ExprKind::Unary(_, a) | ExprKind::Cast(a, _) => scalar_constant(a),
        ExprKind::Binary(a, _, b) | ExprKind::Logical(a, _, b) => {
            scalar_constant(a) && scalar_constant(b)
        }
        ExprKind::Conditional(a, b, c) => {
            scalar_constant(a) && scalar_constant(b) && scalar_constant(c)
        }
        _ => false,
    }
}

fn potential_scalar_constant(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Variable(_) => true,
        ExprKind::Get(target, _) => matches!(target.kind, ExprKind::Variable(_)),
        ExprKind::Grouping(a) | ExprKind::Unary(_, a) | ExprKind::Cast(a, _) => {
            potential_scalar_constant(a)
        }
        ExprKind::Binary(a, _, b) | ExprKind::Logical(a, _, b) => {
            potential_scalar_constant(a) && potential_scalar_constant(b)
        }
        ExprKind::Conditional(a, b, c) => {
            potential_scalar_constant(a)
                && potential_scalar_constant(b)
                && potential_scalar_constant(c)
        }
        _ => scalar_constant(e),
    }
}
