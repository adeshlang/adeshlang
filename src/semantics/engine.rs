//! Semantic Analysis Engine
//!
//! Provides a reusable semantic analysis infrastructure that sits on top of the
//! compiler's existing parser, AST, and type system. This module is designed to
//! be shared between the compiler backend and the Adesh Language Server (ALS).
//!
//! ## Architecture
//!
//! ```text
//!  Source → Lexer → Parser → AST → SemanticIndex
//!                                       │
//!                 ┌──────────────────────┼──────────────────────┐
//!                 │                      │                      │
//!           Type Index            Symbol Table            Scope Tree
//!           (classes,             (all symbols             (lexical
//!            structs,               with spans)            scopes)
//!            enums, etc.)
//!                 │                      │                      │
//!                 └──────────────────────┼──────────────────────┘
//!                                        │
//!                              Enhanced Type Inference
//!                              (resolves member access,
//!                               method calls, generics)
//!                                        │
//!                          ┌─────────────┼─────────────┐
//!                          │             │             │
//!                     Completion     Hover        Go-to-Def
//!                     Diagnostics   Inlay Hints   References
//! ```
//!
//! The key innovation over the raw compiler type checker is **member resolution**:
//! when inferring `user.name`, the engine resolves `user → User`, then looks up
//! `name` in the `User` type definition to determine its type — something the base
//! `infer_expr_type` returns as `Ty::Any`.

use crate::parsing::ast::{TokenKind, Visibility};
use crate::parsing::error::LangError;
use crate::parsing::lexer::{Lexer, Token};
use crate::parsing::parser::Parser;
use crate::typesystem::checker::{Ty, union_merge};
use crate::typesystem::type_system::entry::collect_sigs;
use crate::typesystem::type_system::expression_inference::infer_expr_type;
use crate::typesystem::type_system::resolution::{resolve_type_name, type_from_name};
use crate::utils::collections::{FastMap, FastSet};

// Bring AST types into scope
use crate::parsing::ast::{
    ClassDecl, Expr, ExprKind, Function, Span, Stmt, StmtKind, TypeAliasDecl,
};

/// The kind of a symbol in the semantic model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticSymbolKind {
    Variable,
    Constant,
    Parameter,
    Function,
    Method,
    StaticMethod,
    Field,
    Property,
    Class,
    Struct,
    Enum,
    EnumVariant,
    Interface,
    TypeAlias,
    Module,
    Import,
}

impl SemanticSymbolKind {
    pub fn is_type(&self) -> bool {
        matches!(
            self,
            SemanticSymbolKind::Class
                | SemanticSymbolKind::Struct
                | SemanticSymbolKind::Enum
                | SemanticSymbolKind::Interface
                | SemanticSymbolKind::TypeAlias
        )
    }
}

/// Visibility of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityKind {
    Public,
    Private,
    Protected,
    Default,
}

/// A symbol entry in the semantic model — the result of indexing a declaration.
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    /// The symbol name.
    pub name: String,
    /// What kind of symbol this is.
    pub kind: SemanticSymbolKind,
    /// The resolved type (as a `Ty`), if known.
    pub ty: Option<Ty>,
    /// The type annotation string from the source, if present.
    pub type_annotation: Option<String>,
    /// Source span: 1-based line.
    pub line: usize,
    /// Source span: 1-based column.
    pub col: usize,
    /// End column (exclusive).
    pub end_col: usize,
    /// Visibility.
    pub visibility: VisibilityKind,
    /// Whether the symbol is mutable (for variables/fields).
    pub mutable: bool,
    /// Documentation comment if available.
    pub documentation: Option<String>,
    /// The signature string for functions/methods.
    pub signature: Option<String>,
    /// Function parameters (name, type annotation) for functions/methods.
    pub params: Vec<(String, Option<String>)>,
    /// Return type annotation for functions/methods.
    pub return_type: Option<String>,
    /// Generic type parameters.
    pub type_params: Vec<String>,
    /// Whether this is a static method.
    pub is_static: bool,
    /// Whether this is async.
    pub is_async: bool,
    /// Parent type name (for methods/fields — e.g. "User" for User.save).
    pub parent_type: Option<String>,
    /// Whether the symbol is exported.
    pub exported: bool,
}

/// A member of a type (field or method).
#[derive(Debug, Clone)]
pub struct TypeMember {
    pub name: String,
    pub kind: SemanticSymbolKind,
    pub ty: Option<Ty>,
    pub type_annotation: Option<String>,
    pub signature: Option<String>,
    pub params: Vec<(String, Option<String>)>,
    pub return_type: Option<String>,
    pub is_static: bool,
    pub is_async: bool,
    pub visibility: VisibilityKind,
    pub line: usize,
    pub col: usize,
}

/// A type definition in the semantic model.
#[derive(Debug, Clone)]
pub struct TypeDefinition {
    /// The type name.
    pub name: String,
    /// What kind of type.
    pub kind: SemanticSymbolKind,
    /// Fields (name, type annotation, visibility).
    pub fields: Vec<(String, String, VisibilityKind)>,
    /// Methods.
    pub methods: Vec<TypeMember>,
    /// Static methods.
    pub static_methods: Vec<TypeMember>,
    /// Static properties (name, type).
    pub static_properties: Vec<(String, String)>,
    /// Enum variants (name, optional associated type).
    pub variants: Vec<(String, Option<String>)>,
    /// Parent class (if extends).
    pub extends: Option<String>,
    /// Implemented interfaces.
    pub implements: Vec<String>,
    /// Generic type parameters.
    pub type_params: Vec<String>,
    /// Source location.
    pub line: usize,
    pub col: usize,
    /// Whether the type is abstract.
    pub is_abstract: bool,
    /// Whether the type is sealed.
    pub is_sealed: bool,
    /// Whether the type is exported.
    pub exported: bool,
}

/// Information about an import.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Import path / module name.
    pub path: String,
    /// Alias for the import (or the module name for plain imports).
    pub alias: String,
    /// Named imports (if any).
    pub names: Vec<String>,
    /// Import kind.
    pub kind: ImportKind,
    /// Source location.
    pub line: usize,
    pub col: usize,
}

/// Kind of import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `import "path" as alias`
    Named,
    /// `import "path"` (default)
    Default,
    /// `import { name1, name2 } from "path"`
    Names,
}

/// A scope in the lexical scope tree.
#[derive(Debug, Clone)]
pub struct ScopeInfo {
    /// Unique scope ID.
    pub id: usize,
    /// Parent scope ID (None for the root scope).
    pub parent: Option<usize>,
    /// Symbols declared in this scope.
    pub symbols: Vec<SymbolEntry>,
    /// Scope kind.
    pub kind: ScopeKind,
}

/// What kind of scope this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Top-level / module scope.
    Module,
    /// Function body scope.
    Function,
    /// Block scope (if/while/for/try-catch body).
    Block,
    /// Class/struct body scope.
    Class,
    /// For-loop scope (for the loop variable).
    ForLoop,
}

/// The result of resolving an identifier at a source position.
#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    /// The symbol entry that was resolved.
    pub symbol: SymbolEntry,
    /// The scope where it was found.
    pub scope_id: usize,
}

/// A completion candidate from the semantic engine.
#[derive(Debug, Clone)]
pub struct CompletionCandidate {
    pub label: String,
    pub kind: SemanticSymbolKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
    pub type_annotation: Option<String>,
}

/// The complete semantic index of a source file.
///
/// This is the central data structure that the LSP uses for all IDE features.
/// It is built by [`index_source`] and caches:
/// - The parsed AST
/// - All type definitions (classes, structs, enums, interfaces, aliases)
/// - All symbols with source spans
/// - The lexical scope tree
/// - Function signatures (for type inference)
/// - Type aliases and generic aliases
/// - Import/export information
/// - Parse errors
pub struct SemanticIndex {
    /// The source text that was indexed.
    pub source: String,
    /// The file path (if known).
    pub file: Option<String>,
    /// Parsed statements (AST).
    pub statements: Vec<Stmt>,
    /// All tokens from the lexer (for position mapping).
    pub tokens: Vec<Token>,
    /// Parse errors.
    pub errors: Vec<LangError>,
    /// Type definitions indexed by name.
    pub types: FastMap<String, TypeDefinition>,
    /// All symbols (flat list with source spans).
    pub symbols: Vec<SymbolEntry>,
    /// Scope tree (indexed by scope ID).
    pub scopes: Vec<ScopeInfo>,
    /// Function signatures: name → parameter type annotations.
    pub fns: FastMap<String, Vec<Option<String>>>,
    /// Function type parameters: name → generic param names.
    pub fns_type_params: FastMap<String, Vec<String>>,
    /// Function return types: name → return type annotation.
    pub fns_ret_types: FastMap<String, Option<String>>,
    /// Type aliases (simple): name → resolved Ty.
    pub aliases: FastMap<String, Ty>,
    /// Generic type aliases.
    pub generic_aliases: FastMap<String, TypeAliasDecl>,
    /// Imports.
    pub imports: Vec<ImportInfo>,
    /// Exported symbol names.
    pub exports: FastSet<String>,
}

impl SemanticIndex {
    /// Look up a type definition by name.
    pub fn get_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.types.get(name)
    }

    /// Get all members of a type (fields + methods, including inherited).
    pub fn get_type_members(&self, ty_name: &str) -> Vec<TypeMember> {
        let mut members = Vec::new();
        let mut visited = FastSet::<String>::default();
        self.collect_type_members(ty_name, &mut members, &mut visited);
        members
    }

    fn collect_type_members(
        &self,
        ty_name: &str,
        members: &mut Vec<TypeMember>,
        visited: &mut FastSet<String>,
    ) {
        if visited.contains(ty_name) {
            return;
        }
        visited.insert(ty_name.to_string());

        if let Some(td) = self.types.get(ty_name) {
            // Add fields
            for (fname, ftype, fvis) in &td.fields {
                members.push(TypeMember {
                    name: fname.clone(),
                    kind: SemanticSymbolKind::Field,
                    ty: type_from_name(ftype),
                    type_annotation: Some(ftype.clone()),
                    signature: Some(format!("{}: {}", fname, ftype)),
                    params: vec![],
                    return_type: None,
                    is_static: false,
                    is_async: false,
                    visibility: *fvis,
                    line: 0,
                    col: 0,
                });
            }

            // Add methods
            members.extend(td.methods.iter().cloned());
            // Add static methods
            members.extend(td.static_methods.iter().cloned());

            // Add static properties
            for (name, ty) in &td.static_properties {
                members.push(TypeMember {
                    name: name.clone(),
                    kind: SemanticSymbolKind::StaticMethod,
                    ty: type_from_name(ty),
                    type_annotation: Some(ty.clone()),
                    signature: Some(format!("static {}: {}", name, ty)),
                    params: vec![],
                    return_type: None,
                    is_static: true,
                    is_async: false,
                    visibility: VisibilityKind::Default,
                    line: 0,
                    col: 0,
                });
            }

            // Add enum variants
            for (vname, vtype) in &td.variants {
                members.push(TypeMember {
                    name: vname.clone(),
                    kind: SemanticSymbolKind::EnumVariant,
                    ty: vtype.as_ref().and_then(|t| type_from_name(t)),
                    type_annotation: vtype.clone(),
                    signature: Some(vname.clone()),
                    params: vec![],
                    return_type: None,
                    is_static: true,
                    is_async: false,
                    visibility: VisibilityKind::Public,
                    line: 0,
                    col: 0,
                });
            }

            // Recurse into parent class
            if let Some(parent) = &td.extends {
                self.collect_type_members(parent, members, visited);
            }

            // Recurse into implemented interfaces
            for iface in &td.implements {
                self.collect_type_members(iface, members, visited);
            }
        }
    }

    /// Resolve a type name string to a `Ty`, using the index's aliases.
    pub fn resolve_type(&self, type_str: &str) -> Option<Ty> {
        let empty_type_params: FastMap<String, Ty> = FastMap::default();
        resolve_type_name(
            type_str,
            &self.aliases,
            &self.generic_aliases,
            &empty_type_params,
        )
        .or_else(|| type_from_name(type_str))
    }

    /// Resolve the type of a symbol (variable/parameter/constant) as robustly
    /// as possible. Resolution order:
    ///
    /// 1. The symbol's resolved `ty` (e.g. `let user: User` → `User`).
    /// 2. The type annotation string, mapped through the type table.
    /// 3. Type inference of the initializer expression, so `let user = User(1, "Ajay")`
    ///    and `let p = Point(1.0, 2.0)` also resolve to their nominal types.
    pub fn resolve_symbol_type(&self, name: &str) -> Option<Ty> {
        let sym = self.find_declaration(name)?;

        // 1. Already-resolved type annotation.
        if let Some(ty) = &sym.ty {
            if !matches!(ty, Ty::Any | Ty::Unknown) {
                // `let user: User` resolves to `GenericParam("User")`; map it to
                // a nominal generic instance when the name is a known type.
                if let Ty::GenericParam(tname) = ty {
                    if let Some(td) = self.types.get(tname) {
                        return Some(nominal_type(td));
                    }
                }
                return Some(ty.clone());
            }
        }

        // 2. Type annotation string, preferring the type table for user types
        //    (resolve_type returns `Ty::Any` for class/interface/enum names).
        if let Some(type_str) = &sym.type_annotation {
            let base = base_type_name(type_str);
            if let Some(td) = self.types.get(base) {
                let nominal = nominal_type(td);
                let nullable = type_str.trim().ends_with('?') || type_str.trim().starts_with('?');
                return Some(if nullable {
                    Ty::Nullable(Box::new(nominal))
                } else {
                    nominal
                });
            }
            if let Some(ty) = self.resolve_type(type_str) {
                if !matches!(ty, Ty::Any | Ty::Unknown) {
                    return Some(ty);
                }
            }
        }

        // 3. Infer from the initializer expression (`let x = ...`).
        if let Some(init) = self.find_variable_initializer(name) {
            let inferred = self.infer_type(init);
            if !matches!(inferred, Ty::Any | Ty::Unknown) {
                return Some(inferred);
            }
        }

        None
    }

    /// Find the initializer expression of a `let` binding with the given name.
    fn find_variable_initializer<'a>(&'a self, name: &str) -> Option<&'a Expr> {
        self.statements.iter().find_map(|s| find_let_init(s, name))
    }

    /// Find a symbol at a given source position (1-based line, 1-based col).
    pub fn find_symbol_at(&self, line: usize, col: usize) -> Option<&SymbolEntry> {
        // Find the token at this position
        let token = self
            .tokens
            .iter()
            .find(|t| t.line == line && col >= t.col && col < t.col + t.lexeme.len())?;

        if let TokenKind::Identifier = token.kind {
            // Find matching symbol
            self.symbols
                .iter()
                .find(|s| s.name == token.lexeme && s.line == token.line && s.col == token.col)
                .or_else(|| {
                    // If no exact position match, find any symbol with this name
                    // that could be the declaration
                    self.symbols.iter().find(|s| s.name == token.lexeme)
                })
        } else {
            None
        }
    }

    /// Find all references to a symbol by name.
    pub fn find_references_by_name(&self, name: &str) -> Vec<(usize, usize, usize)> {
        let mut refs = Vec::new();
        for tok in &self.tokens {
            if tok.kind == TokenKind::Identifier && tok.lexeme == name {
                refs.push((tok.line, tok.col, tok.col + tok.lexeme.len()));
            }
        }
        refs
    }

    /// Find a token at a given position.
    pub fn token_at(&self, line: usize, col: usize) -> Option<&Token> {
        self.tokens
            .iter()
            .find(|t| t.line == line && col >= t.col && col < t.col + t.lexeme.len())
    }

    /// Get the identifier at a given position, if any.
    pub fn identifier_at(&self, line: usize, col: usize) -> Option<&str> {
        let tok = self.token_at(line, col)?;
        if matches!(tok.kind, TokenKind::Identifier) {
            Some(&tok.lexeme)
        } else {
            None
        }
    }

    /// Get the word at a given position, whether it is a plain identifier or a
    /// language keyword (`let`, `fn`, `for`, ...). Used by hover/docs so that
    /// keywords get documentation, not just user symbols.
    pub fn identifier_or_keyword_at(&self, line: usize, col: usize) -> Option<&str> {
        let tok = self.token_at(line, col)?;
        if matches!(tok.kind, TokenKind::Identifier) || is_keyword_token(tok.kind) {
            Some(tok.lexeme.as_str())
        } else {
            None
        }
    }

    /// Find the declaration of a symbol by name (preferring the earliest declaration).
    pub fn find_declaration(&self, name: &str) -> Option<&SymbolEntry> {
        self.symbols
            .iter()
            .filter(|s| s.name == name)
            .min_by_key(|s| (s.line, s.col))
    }

    /// Enhanced type inference for an expression.
    ///
    /// This wraps the compiler's `infer_expr_type` but adds **member resolution**:
    /// - `ExprKind::Get(obj, key)` resolves the object's type and looks up the member
    /// - `ExprKind::New(ctor, args)` resolves the constructor to its type
    /// - `ExprKind::Call(Get(obj, method), args)` resolves the method return type
    pub fn infer_type(&self, expr: &Expr) -> Ty {
        let empty_type_params: FastMap<String, Ty> = FastMap::default();
        self.infer_type_internal(expr, &empty_type_params)
    }

    fn infer_type_internal(&self, expr: &Expr, type_params: &FastMap<String, Ty>) -> Ty {
        match &expr.kind {
            // Enhanced: member access resolution
            ExprKind::Get(obj, key) => {
                let obj_ty = self.infer_type_internal(obj, type_params);
                self.resolve_member_type(&obj_ty, key, type_params)
                    .unwrap_or(Ty::Any)
            }

            // Enhanced: optional member access
            ExprKind::OptGet(obj, key) => {
                let obj_ty = self.infer_type_internal(obj, type_params);
                let inner_ty = if let Ty::Nullable(inner) = &obj_ty {
                    (**inner).clone()
                } else {
                    obj_ty
                };
                self.resolve_member_type(&inner_ty, key, type_params)
                    .map(|t| Ty::Nullable(Box::new(t)))
                    .unwrap_or(Ty::Any)
            }

            // Enhanced: constructor
            ExprKind::New(ctor, _args) => {
                if let ExprKind::Variable(name) = &ctor.kind {
                    if let Some(td) = self.types.get(name) {
                        return if td.type_params.is_empty() {
                            Ty::GenericInstance {
                                name: name.clone(),
                                args: vec![],
                            }
                        } else {
                            Ty::GenericInstance {
                                name: name.clone(),
                                args: td
                                    .type_params
                                    .iter()
                                    .map(|p| Ty::GenericParam(p.clone()))
                                    .collect(),
                            }
                        };
                    }
                    // Check aliases
                    if let Some(ty) = self.resolve_type(name) {
                        return ty;
                    }
                }
                Ty::Any
            }

            // Enhanced: method call resolution
            ExprKind::Call(callee, _args, _type_args) => {
                // First try the base inference (handles plain function calls)
                let mut env: Vec<FastMap<String, Ty>> = vec![FastMap::default()];
                if let Ok(ty) = infer_expr_type(
                    expr,
                    &mut env,
                    &self.fns,
                    &self.fns_type_params,
                    &self.fns_ret_types,
                    &self.aliases,
                    &self.generic_aliases,
                    type_params,
                ) {
                    if ty != Ty::Any {
                        return ty;
                    }
                }

                // If base inference returned Any, try method call resolution
                match &callee.kind {
                    ExprKind::Get(obj, method_name) => {
                        let obj_ty = self.infer_type_internal(obj, type_params);
                        if let Some(member) = self.find_member(&obj_ty, method_name) {
                            if let Some(ret) = &member.return_type {
                                return self.resolve_type(ret).unwrap_or(Ty::Any);
                            }
                            return member.ty.unwrap_or(Ty::Any);
                        }
                        Ty::Any
                    }
                    ExprKind::Variable(name) => {
                        // Check if it's a constructor call (like User(...))
                        if let Some(td) = self.types.get(name) {
                            return if td.type_params.is_empty() {
                                Ty::GenericInstance {
                                    name: name.clone(),
                                    args: vec![],
                                }
                            } else {
                                Ty::GenericInstance {
                                    name: name.clone(),
                                    args: td
                                        .type_params
                                        .iter()
                                        .map(|p| Ty::GenericParam(p.clone()))
                                        .collect(),
                                }
                            };
                        }
                        Ty::Any
                    }
                    _ => Ty::Any,
                }
            }

            // Enhanced: index access (array element type)
            ExprKind::Index(target, _index) => {
                let target_ty = self.infer_type_internal(target, type_params);
                if let Ty::Array(elem) = target_ty {
                    return (*elem).clone();
                }
                if let Ty::Map(_k, v) = target_ty {
                    return (*v).clone();
                }
                // For generic instances like Array<User>, try to resolve
                if let Ty::GenericInstance { name, args } = &target_ty {
                    if name == "Array" && !args.is_empty() {
                        return args[0].clone();
                    }
                    if name == "Map" && args.len() >= 2 {
                        return args[1].clone();
                    }
                }
                Ty::Any
            }

            // Enhanced: cast expression
            ExprKind::Cast(_inner, type_str) => self.resolve_type(type_str).unwrap_or(Ty::Any),

            // Enhanced: struct literal
            ExprKind::StructLiteral(name, _fields) => {
                if let Some(ty) = self.resolve_type(name) {
                    return ty;
                }
                Ty::GenericInstance {
                    name: name.clone(),
                    args: vec![],
                }
            }

            // Enhanced: conditional (ternary)
            ExprKind::Conditional(_cond, then_expr, else_expr) => {
                let then_ty = self.infer_type_internal(then_expr, type_params);
                let else_ty = self.infer_type_internal(else_expr, type_params);
                if then_ty == else_ty {
                    then_ty
                } else if then_ty == Ty::Any {
                    else_ty
                } else if else_ty == Ty::Any {
                    then_ty
                } else {
                    union_merge(then_ty.clone(), else_ty.clone())
                }
            }

            // For all other expression kinds, fall back to the base inference
            _ => {
                let mut env: Vec<FastMap<String, Ty>> = vec![FastMap::default()];

                // Build a basic environment from top-level symbols
                for sym in &self.symbols {
                    if sym.kind == SemanticSymbolKind::Variable
                        || sym.kind == SemanticSymbolKind::Constant
                        || sym.kind == SemanticSymbolKind::Parameter
                    {
                        if let Some(ty) = &sym.ty {
                            if let Some(scope) = env.last_mut() {
                                scope.insert(sym.name.clone(), ty.clone());
                            }
                        }
                    }
                }

                infer_expr_type(
                    expr,
                    &mut env,
                    &self.fns,
                    &self.fns_type_params,
                    &self.fns_ret_types,
                    &self.aliases,
                    &self.generic_aliases,
                    type_params,
                )
                .unwrap_or(Ty::Any)
            }
        }
    }

    /// Resolve the type of a member access: given an object type and a member name,
    /// return the member's type.
    fn resolve_member_type(
        &self,
        obj_ty: &Ty,
        member_name: &str,
        _type_params: &FastMap<String, Ty>,
    ) -> Option<Ty> {
        // Dereference pointer/reference types
        let effective_ty = match obj_ty {
            Ty::Ptr(inner) | Ty::PtrOwning(inner) | Ty::PtrShared(inner) | Ty::PtrMut(inner) => {
                &**inner
            }
            Ty::Nullable(inner) => &**inner,
            _ => obj_ty,
        };

        // Get the type name from the Ty
        let type_name = match effective_ty {
            Ty::GenericInstance { name, .. } => name.clone(),
            // `let user: User` — the type annotation resolves to a nominal
            // type name; look it up in the type table.
            Ty::GenericParam(name) => {
                if self.types.contains_key(name) {
                    name.clone()
                } else {
                    return None;
                }
            }
            Ty::Record { .. } => {
                // For records, look up the field directly
                if let Ty::Record { required, optional } = effective_ty {
                    for (fname, fty) in required {
                        if fname == member_name {
                            return Some(fty.clone());
                        }
                    }
                    for (fname, fty) in optional {
                        if fname == member_name {
                            return Some(Ty::Nullable(Box::new(fty.clone())));
                        }
                    }
                }
                return None;
            }
            _ => {
                // Check if this is a known type name
                // For simple types like Ty::Any, we can't resolve
                return None;
            }
        };

        // Look up the type definition
        let td = self.types.get(&type_name)?;

        // Check fields
        for (fname, ftype, _vis) in &td.fields {
            if fname == member_name {
                return self.resolve_type(ftype).or_else(|| type_from_name(ftype));
            }
        }

        // Check methods
        for method in &td.methods {
            if method.name == member_name {
                if let Some(ret) = &method.return_type {
                    return self.resolve_type(ret).or_else(|| type_from_name(ret));
                }
                return method.ty.clone();
            }
        }

        // Check static methods
        for method in &td.static_methods {
            if method.name == member_name {
                if let Some(ret) = &method.return_type {
                    return self.resolve_type(ret).or_else(|| type_from_name(ret));
                }
                return method.ty.clone();
            }
        }

        // Check static properties
        for (name, ty) in &td.static_properties {
            if name == member_name {
                return self.resolve_type(ty).or_else(|| type_from_name(ty));
            }
        }

        // Check enum variants
        for (vname, vtype) in &td.variants {
            if vname == member_name {
                if let Some(vt) = vtype {
                    return self.resolve_type(vt).or_else(|| type_from_name(vt));
                }
                return Some(Ty::GenericInstance {
                    name: type_name.clone(),
                    args: vec![],
                });
            }
        }

        // Check parent class
        if let Some(parent) = &td.extends {
            return self.resolve_member_type_by_name(parent, member_name);
        }

        // Check implemented interfaces
        for iface in &td.implements {
            if let Some(ty) = self.resolve_member_type_by_name(iface, member_name) {
                return Some(ty);
            }
        }

        None
    }

    fn resolve_member_type_by_name(&self, type_name: &str, member_name: &str) -> Option<Ty> {
        let td = self.types.get(type_name)?;

        for (fname, ftype, _vis) in &td.fields {
            if fname == member_name {
                return self.resolve_type(ftype).or_else(|| type_from_name(ftype));
            }
        }

        for method in &td.methods {
            if method.name == member_name {
                if let Some(ret) = &method.return_type {
                    return self.resolve_type(ret).or_else(|| type_from_name(ret));
                }
                return method.ty.clone();
            }
        }

        if let Some(parent) = &td.extends {
            return self.resolve_member_type_by_name(parent, member_name);
        }

        None
    }

    /// Find a member of a type by name.
    fn find_member(&self, obj_ty: &Ty, member_name: &str) -> Option<TypeMember> {
        let effective_ty = match obj_ty {
            Ty::Ptr(inner) | Ty::PtrOwning(inner) | Ty::PtrShared(inner) | Ty::PtrMut(inner) => {
                &**inner
            }
            Ty::Nullable(inner) => &**inner,
            _ => obj_ty,
        };

        let type_name = match effective_ty {
            Ty::GenericInstance { name, .. } => name.clone(),
            Ty::GenericParam(name) if self.types.contains_key(name) => name.clone(),
            _ => return None,
        };

        let members = self.get_type_members(&type_name);
        members.into_iter().find(|m| m.name == member_name)
    }

    /// Get completions for member access on a type.
    pub fn completions_for_type(&self, ty: &Ty) -> Vec<CompletionCandidate> {
        let effective_ty = match ty {
            Ty::Ptr(inner) | Ty::PtrOwning(inner) | Ty::PtrShared(inner) | Ty::PtrMut(inner) => {
                &**inner
            }
            Ty::Nullable(inner) => &**inner,
            // User-defined type annotations surface as `GenericParam` (e.g.
            // `let user: User` resolves to `User` through the type table).
            Ty::GenericParam(name) => return self.completions_for_type_name(name),
            _ => ty,
        };

        match effective_ty {
            Ty::GenericInstance { name, .. } => self.completions_for_type_name(name),
            Ty::GenericParam(name) => self.completions_for_type_name(name),
            Ty::Record { .. } => {
                // For records, return fields as completions
                let mut candidates = Vec::new();
                if let Ty::Record { required, optional } = effective_ty {
                    for (fname, fty) in required {
                        candidates.push(CompletionCandidate {
                            label: fname.clone(),
                            kind: SemanticSymbolKind::Field,
                            detail: Some(format!("field: {}", fty)),
                            documentation: None,
                            insert_text: Some(fname.clone()),
                            type_annotation: Some(fty.to_string()),
                        });
                    }
                    for (fname, fty) in optional {
                        candidates.push(CompletionCandidate {
                            label: fname.clone(),
                            kind: SemanticSymbolKind::Field,
                            detail: Some(format!("field?: {}", fty)),
                            documentation: None,
                            insert_text: Some(fname.clone()),
                            type_annotation: Some(format!("{}?", fty)),
                        });
                    }
                }
                candidates
            }
            Ty::Array(_elem) => self.builtin_array_completions(),
            Ty::Str => self.builtin_string_completions(),
            _ => vec![],
        }
    }

    /// Get completions for member access on a type looked up by name
    /// (fields, methods, static members, enum variants, including inherited).
    fn completions_for_type_name(&self, type_name: &str) -> Vec<CompletionCandidate> {
        let members = self.get_type_members(type_name);
        members
            .into_iter()
            .map(|m| {
                let is_method = matches!(
                    m.kind,
                    SemanticSymbolKind::Method | SemanticSymbolKind::StaticMethod
                );
                CompletionCandidate {
                    label: m.name.clone(),
                    kind: m.kind,
                    detail: m.signature.clone(),
                    documentation: None,
                    insert_text: if is_method {
                        Some(format!("{}($0)", m.name))
                    } else {
                        Some(m.name.clone())
                    },
                    type_annotation: m.type_annotation.clone(),
                }
            })
            .collect()
    }

    fn builtin_array_completions(&self) -> Vec<CompletionCandidate> {
        vec![
            simple_completion(
                "length",
                SemanticSymbolKind::Field,
                "number",
                "Length of array",
            ),
            simple_completion(
                "push",
                SemanticSymbolKind::Method,
                "fn push(item)",
                "Add item to end",
            ),
            simple_completion(
                "pop",
                SemanticSymbolKind::Method,
                "fn pop(): any",
                "Remove and return last item",
            ),
            simple_completion(
                "map",
                SemanticSymbolKind::Method,
                "fn map(fn): array",
                "Transform each element",
            ),
            simple_completion(
                "filter",
                SemanticSymbolKind::Method,
                "fn filter(fn): array",
                "Filter by predicate",
            ),
            simple_completion(
                "reduce",
                SemanticSymbolKind::Method,
                "fn reduce(fn, init): any",
                "Reduce to single value",
            ),
            simple_completion(
                "clone",
                SemanticSymbolKind::Method,
                "fn clone(): Self",
                "Deep copy",
            ),
            simple_completion(
                "slice",
                SemanticSymbolKind::Method,
                "fn slice(start, end?): array",
                "Extract subarray",
            ),
            simple_completion(
                "concat",
                SemanticSymbolKind::Method,
                "fn concat(other): array",
                "Concatenate arrays",
            ),
            simple_completion(
                "reverse",
                SemanticSymbolKind::Method,
                "fn reverse(): array",
                "Reverse array",
            ),
            simple_completion(
                "sort",
                SemanticSymbolKind::Method,
                "fn sort(fn?): array",
                "Sort array",
            ),
            simple_completion(
                "indexOf",
                SemanticSymbolKind::Method,
                "fn indexOf(item): number",
                "Find index of item",
            ),
            simple_completion(
                "contains",
                SemanticSymbolKind::Method,
                "fn contains(item): bool",
                "Check if contains item",
            ),
            simple_completion(
                "join",
                SemanticSymbolKind::Method,
                "fn join(sep?): string",
                "Join elements as string",
            ),
            simple_completion(
                "forEach",
                SemanticSymbolKind::Method,
                "fn forEach(fn)",
                "Iterate over elements",
            ),
            simple_completion(
                "find",
                SemanticSymbolKind::Method,
                "fn find(fn): any?",
                "Find first matching element",
            ),
            simple_completion(
                "flatMap",
                SemanticSymbolKind::Method,
                "fn flatMap(fn): array",
                "Map and flatten",
            ),
        ]
    }

    fn builtin_string_completions(&self) -> Vec<CompletionCandidate> {
        vec![
            simple_completion(
                "length",
                SemanticSymbolKind::Field,
                "number",
                "Length of string",
            ),
            simple_completion(
                "toUpperCase",
                SemanticSymbolKind::Method,
                "fn toUpperCase(): string",
                "Convert to uppercase",
            ),
            simple_completion(
                "toLowerCase",
                SemanticSymbolKind::Method,
                "fn toLowerCase(): string",
                "Convert to lowercase",
            ),
            simple_completion(
                "trim",
                SemanticSymbolKind::Method,
                "fn trim(): string",
                "Trim whitespace",
            ),
            simple_completion(
                "split",
                SemanticSymbolKind::Method,
                "fn split(sep): array",
                "Split into array",
            ),
            simple_completion(
                "replace",
                SemanticSymbolKind::Method,
                "fn replace(old, new): string",
                "Replace substring",
            ),
            simple_completion(
                "contains",
                SemanticSymbolKind::Method,
                "fn contains(s): bool",
                "Check if contains substring",
            ),
            simple_completion(
                "startsWith",
                SemanticSymbolKind::Method,
                "fn startsWith(s): bool",
                "Check prefix",
            ),
            simple_completion(
                "endsWith",
                SemanticSymbolKind::Method,
                "fn endsWith(s): bool",
                "Check suffix",
            ),
            simple_completion(
                "slice",
                SemanticSymbolKind::Method,
                "fn slice(start, end?): string",
                "Extract substring",
            ),
            simple_completion(
                "indexOf",
                SemanticSymbolKind::Method,
                "fn indexOf(s): number",
                "Find index of substring",
            ),
            simple_completion(
                "repeat",
                SemanticSymbolKind::Method,
                "fn repeat(n): string",
                "Repeat string n times",
            ),
            simple_completion(
                "charAt",
                SemanticSymbolKind::Method,
                "fn charAt(i): char",
                "Get character at index",
            ),
            simple_completion(
                "charCodeAt",
                SemanticSymbolKind::Method,
                "fn charCodeAt(i): number",
                "Get char code at index",
            ),
            simple_completion(
                "concat",
                SemanticSymbolKind::Method,
                "fn concat(s): string",
                "Concatenate strings",
            ),
        ]
    }

    /// Get all symbols visible at a given position (for completion in expression context).
    pub fn visible_symbols_at(&self, line: usize, _col: usize) -> Vec<CompletionCandidate> {
        let mut candidates = Vec::new();
        let mut seen = FastSet::<String>::default();

        // Add all symbols (in a real implementation, we'd filter by scope)
        for sym in &self.symbols {
            if !seen.contains(&sym.name) && sym.line <= line {
                seen.insert(sym.name.clone());
                let insert_text = match sym.kind {
                    SemanticSymbolKind::Function | SemanticSymbolKind::Method => {
                        Some(format!("{}($0)", sym.name))
                    }
                    _ => Some(sym.name.clone()),
                };
                candidates.push(CompletionCandidate {
                    label: sym.name.clone(),
                    kind: sym.kind,
                    detail: sym.signature.clone().or_else(|| {
                        sym.type_annotation
                            .as_ref()
                            .map(|t| format!("{}: {}", sym.name, t))
                    }),
                    documentation: sym.documentation.clone(),
                    insert_text,
                    type_annotation: sym.type_annotation.clone(),
                });
            }
        }

        candidates
    }

    /// Get completions for a type name (in type position).
    pub fn type_completions(&self) -> Vec<CompletionCandidate> {
        let mut candidates = Vec::new();

        // Built-in types
        for (name, desc) in &[
            ("int", "32-bit signed integer"),
            ("float", "64-bit floating point"),
            ("f32", "32-bit floating point"),
            ("f64", "64-bit floating point"),
            ("bool", "Boolean type"),
            ("string", "UTF-8 string type"),
            ("char", "Unicode character"),
            ("i8", "8-bit signed integer"),
            ("i16", "16-bit signed integer"),
            ("i32", "32-bit signed integer"),
            ("i64", "64-bit signed integer"),
            ("i128", "128-bit signed integer"),
            ("u8", "8-bit unsigned integer"),
            ("u16", "16-bit unsigned integer"),
            ("u32", "32-bit unsigned integer"),
            ("u64", "64-bit unsigned integer"),
            ("u128", "128-bit unsigned integer"),
            ("void", "Unit/void type"),
            ("any", "Any type (dynamic)"),
            ("never", "Never type (unreachable)"),
            ("null", "Null type"),
        ] {
            candidates.push(CompletionCandidate {
                label: name.to_string(),
                kind: SemanticSymbolKind::TypeAlias,
                detail: Some(desc.to_string()),
                documentation: None,
                insert_text: Some(name.to_string()),
                type_annotation: None,
            });
        }

        // User-defined types
        for (name, td) in &self.types {
            let detail = match td.kind {
                SemanticSymbolKind::Class => format!("class {}", name),
                SemanticSymbolKind::Struct => format!("struct {}", name),
                SemanticSymbolKind::Enum => format!("enum {}", name),
                SemanticSymbolKind::Interface => format!("interface {}", name),
                SemanticSymbolKind::TypeAlias => format!("type {}", name),
                _ => name.clone(),
            };
            candidates.push(CompletionCandidate {
                label: name.clone(),
                kind: td.kind,
                detail: Some(detail),
                documentation: None,
                insert_text: Some(name.clone()),
                type_annotation: None,
            });
        }

        candidates
    }
}

/// Build the nominal type of a type definition (for use in member resolution
/// and completions): `User` → `GenericInstance { name: "User" }`.
fn nominal_type(td: &TypeDefinition) -> Ty {
    if td.type_params.is_empty() {
        Ty::GenericInstance {
            name: td.name.clone(),
            args: vec![],
        }
    } else {
        Ty::GenericInstance {
            name: td.name.clone(),
            args: td
                .type_params
                .iter()
                .map(|p| Ty::GenericParam(p.clone()))
                .collect(),
        }
    }
}

/// Extract the base type name from a type annotation string:
/// `"User"` → `"User"`, `"User<int>"` → `"User"`, `"User?"` → `"User"`, `"*User"` → `"User"`.
fn base_type_name(type_str: &str) -> &str {
    let s = type_str
        .trim()
        .trim_start_matches('*')
        .trim_end_matches('?');
    let s = s.trim();
    if let Some(idx) = s.find('<') {
        s[..idx].trim()
    } else {
        s
    }
}

/// Find the initializer of a `let` binding by name, recursing through nested scopes.
fn find_let_init<'a>(s: &'a Stmt, name: &str) -> Option<&'a Expr> {
    match &s.kind {
        StmtKind::Let(n, init, _, _, _, _) if n == name => init.as_ref(),
        StmtKind::Block(bs) => bs.iter().find_map(|b| find_let_init(b, name)),
        StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) => {
            func.body.iter().find_map(|b| find_let_init(b, name))
        }
        StmtKind::Class(class, _) | StmtKind::ExportDefaultClass(class) => {
            for m in &class.methods {
                if let Some(e) = m.body.iter().find_map(|b| find_let_init(b, name)) {
                    return Some(e);
                }
            }
            for m in &class.static_methods {
                if let Some(e) = m.body.iter().find_map(|b| find_let_init(b, name)) {
                    return Some(e);
                }
            }
            None
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => find_let_init(then_branch, name)
            .or_else(|| else_branch.as_ref().and_then(|e| find_let_init(e, name))),
        StmtKind::While { body, .. } => find_let_init(body, name),
        StmtKind::ForIn { body, .. } => find_let_init(body, name),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => find_let_init(try_block, name).or_else(|| find_let_init(catch_block, name)),
        StmtKind::Region { body, .. } => find_let_init(body, name),
        StmtKind::UnsafeBlock(body) => find_let_init(body, name),
        StmtKind::Defer(body) => find_let_init(body, name),
        _ => None,
    }
}

/// Whether a token kind is a language keyword word (not punctuation/operators).
fn is_keyword_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Let
            | TokenKind::Const
            | TokenKind::Fn
            | TokenKind::Return
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::Elif
            | TokenKind::Do
            | TokenKind::While
            | TokenKind::For
            | TokenKind::In
            | TokenKind::Of
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Null
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Instanceof
            | TokenKind::Typeof
            | TokenKind::Not
            | TokenKind::Class
            | TokenKind::New
            | TokenKind::This
            | TokenKind::SelfKeyword
            | TokenKind::Uint
            | TokenKind::Int
            | TokenKind::Extend
            | TokenKind::Extends
            | TokenKind::Extern
            | TokenKind::Private
            | TokenKind::Protected
            | TokenKind::Public
            | TokenKind::Super
            | TokenKind::Abstract
            | TokenKind::Sealed
            | TokenKind::Interface
            | TokenKind::Implements
            | TokenKind::Type
            | TokenKind::Decorator
            | TokenKind::On
            | TokenKind::Import
            | TokenKind::As
            | TokenKind::Export
            | TokenKind::From
            | TokenKind::Default
            | TokenKind::Try
            | TokenKind::Catch
            | TokenKind::Throw
            | TokenKind::Async
            | TokenKind::Await
            | TokenKind::Spawn
            | TokenKind::Static
            | TokenKind::Constructor
            | TokenKind::Get
            | TokenKind::Set
            | TokenKind::Operator
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Jump
            | TokenKind::Struct
            | TokenKind::Enum
            | TokenKind::Match
            | TokenKind::Readonly
            | TokenKind::Raw
            | TokenKind::VecType
            | TokenKind::Region
            | TokenKind::Unsafe
            | TokenKind::Share
            | TokenKind::Strong
            | TokenKind::Weak
            | TokenKind::Alloc
            | TokenKind::Free
            | TokenKind::Defer
            | TokenKind::Compile
            | TokenKind::Runtime
            | TokenKind::Typecheck
            | TokenKind::Emit
            | TokenKind::Require
            | TokenKind::Proceed
            | TokenKind::Test
            | TokenKind::Ignore
            | TokenKind::ExpectFail
    )
}

fn simple_completion(
    name: &str,
    kind: SemanticSymbolKind,
    detail: &str,
    doc: &str,
) -> CompletionCandidate {
    let is_method = matches!(kind, SemanticSymbolKind::Method);
    CompletionCandidate {
        label: name.to_string(),
        kind,
        detail: Some(detail.to_string()),
        documentation: Some(doc.to_string()),
        insert_text: if is_method {
            Some(format!("{}($0)", name))
        } else {
            Some(name.to_string())
        },
        type_annotation: None,
    }
}

fn map_visibility(v: &Visibility) -> VisibilityKind {
    match v {
        Visibility::Pub => VisibilityKind::Public,
        Visibility::Priv => VisibilityKind::Private,
        Visibility::Protected => VisibilityKind::Protected,
    }
}

fn map_visibility_kind(v: &Option<Visibility>) -> VisibilityKind {
    match v {
        Some(Visibility::Pub) => VisibilityKind::Public,
        Some(Visibility::Priv) => VisibilityKind::Private,
        Some(Visibility::Protected) => VisibilityKind::Protected,
        None => VisibilityKind::Default,
    }
}

/// Build a semantic index from source code.
///
/// This parses the source, collects all type definitions, function signatures,
/// symbols, scopes, and imports, and returns a [`SemanticIndex`] that the LSP
/// can query for all IDE features.
pub fn index_source(source: &str) -> SemanticIndex {
    index_source_in(source, None)
}

/// Build a semantic index from source code with a file path.
pub fn index_source_in(source: &str, file: Option<&str>) -> SemanticIndex {
    let mut lexer = Lexer::new(source);
    if let Some(f) = file {
        lexer.set_file(f);
    }

    let (statements, tokens, errors) = match lexer.tokenize() {
        Ok(toks) => {
            let toks_clone = toks.clone();
            let mut parser = Parser::new(toks, file.map(|f| f.to_string()));
            // Use recovering parser to handle incomplete/invalid source (IDE scenario)
            let (stmts, errs) = parser.parse_program_recovering();
            (stmts, toks_clone, errs)
        }
        Err(e) => (vec![], vec![], vec![e]),
    };

    let mut index = SemanticIndex {
        source: source.to_string(),
        file: file.map(|f| f.to_string()),
        statements,
        tokens,
        errors,
        types: FastMap::default(),
        symbols: Vec::new(),
        scopes: vec![ScopeInfo {
            id: 0,
            parent: None,
            symbols: vec![],
            kind: ScopeKind::Module,
        }],
        fns: FastMap::default(),
        fns_type_params: FastMap::default(),
        fns_ret_types: FastMap::default(),
        aliases: FastMap::default(),
        generic_aliases: FastMap::default(),
        imports: Vec::new(),
        exports: FastSet::default(),
    };

    // Collect function signatures (reusing the compiler's collect_sigs)
    for s in &index.statements {
        collect_sigs(
            s,
            &mut index.fns,
            &mut index.fns_type_params,
            &mut index.fns_ret_types,
        );
    }

    // Collect type names as aliases (Ty::Any initially, like check_module_in does)
    for s in &index.statements {
        collect_custom_types_names(s, &mut index.aliases);
    }

    // Collect type aliases
    collect_type_aliases(
        &index.statements,
        &mut index.aliases,
        &mut index.generic_aliases,
    );

    // Collect struct declarations as record types
    {
        let aliases_snapshot = index.aliases.clone();
        collect_struct_types(
            &index.statements,
            &aliases_snapshot,
            &index.generic_aliases,
            &mut index.aliases,
        );
    }

    // Collect full type definitions
    for s in &index.statements {
        collect_type_definition(s, &mut index.types, &index.tokens);
    }

    // Collect all symbols with positions
    for s in &index.statements {
        collect_symbols(s, &mut index.symbols, &index.tokens, None);
    }

    // Collect imports
    for s in &index.statements {
        collect_imports(s, &mut index.imports);
    }

    // Collect exports
    for s in &index.statements {
        collect_exports(s, &mut index.exports);
    }

    // Build the scope tree
    index.scopes.clear();
    index.scopes.push(ScopeInfo {
        id: 0,
        parent: None,
        symbols: vec![],
        kind: ScopeKind::Module,
    });
    for s in &index.statements {
        build_scope_tree(s, &mut index.scopes, 0, &index.tokens);
    }

    index
}

fn collect_custom_types_names(s: &Stmt, aliases: &mut FastMap<String, Ty>) {
    match &s.kind {
        StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
            aliases.insert(c.name.clone(), Ty::Class(c.name.clone()));
        }
        StmtKind::Interface(i, _) => {
            aliases.insert(i.name.clone(), Ty::Interface(i.name.clone()));
        }
        StmtKind::Enum(e, _) => {
            aliases.insert(e.name.clone(), Ty::Enum(e.name.clone()));
        }
        StmtKind::Block(bs) => {
            for b in bs {
                collect_custom_types_names(b, aliases);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_custom_types_names(then_branch, aliases);
            if let Some(e) = else_branch {
                collect_custom_types_names(e, aliases);
            }
        }
        StmtKind::While { body, .. } => collect_custom_types_names(body, aliases),
        StmtKind::ForIn { body, .. } => collect_custom_types_names(body, aliases),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            collect_custom_types_names(try_block, aliases);
            collect_custom_types_names(catch_block, aliases);
        }
        StmtKind::Region { body, .. } => collect_custom_types_names(body, aliases),
        StmtKind::UnsafeBlock(body) => collect_custom_types_names(body, aliases),
        StmtKind::Defer(body) => collect_custom_types_names(body, aliases),
        _ => {}
    }
}

fn collect_type_aliases(
    statements: &[Stmt],
    aliases: &mut FastMap<String, Ty>,
    generic_aliases: &mut FastMap<String, TypeAliasDecl>,
) {
    for s in statements {
        if let StmtKind::TypeAlias(ta, _exp) = &s.kind {
            if ta.type_params.is_empty() {
                let mut req: Vec<(String, Ty)> = Vec::new();
                let mut opt: Vec<(String, Ty)> = Vec::new();
                for (fname, optional, fty) in &ta.fields {
                    if let Some(ft) = type_from_name(fty) {
                        if *optional {
                            opt.push((fname.clone(), ft));
                        } else {
                            req.push((fname.clone(), ft));
                        }
                    }
                }
                aliases.insert(
                    ta.name.clone(),
                    Ty::Record {
                        required: req,
                        optional: opt,
                    },
                );
            } else {
                generic_aliases.insert(ta.name.clone(), ta.clone());
            }
        }
    }
}

fn collect_struct_types(
    statements: &[Stmt],
    existing_aliases: &FastMap<String, Ty>,
    generic_aliases: &FastMap<String, TypeAliasDecl>,
    aliases: &mut FastMap<String, Ty>,
) {
    let empty_type_params: FastMap<String, Ty> = FastMap::default();
    for s in statements {
        if let StmtKind::Struct(sd, _export) = &s.kind {
            let mut req: Vec<(String, Ty)> = Vec::new();
            for (fname, fty) in &sd.fields {
                let resolved =
                    resolve_type_name(fty, existing_aliases, generic_aliases, &empty_type_params)
                        .or_else(|| type_from_name(fty));
                if let Some(ft) = resolved {
                    req.push((fname.clone(), ft));
                }
            }
            aliases.insert(
                sd.name.clone(),
                Ty::Record {
                    required: req,
                    optional: Vec::new(),
                },
            );
        }
    }
}

fn collect_type_definition(
    s: &Stmt,
    types: &mut FastMap<String, TypeDefinition>,
    tokens: &[Token],
) {
    match &s.kind {
        StmtKind::Class(c, exported) => {
            let (line, col) = find_identifier_position(tokens, &c.name, TokenKind::Class)
                .unwrap_or((s.span.line, s.span.col));

            let mut fields = Vec::new();
            for (fname, ftype, fvis, _, _) in &c.fields {
                fields.push((fname.clone(), ftype.clone(), map_visibility(fvis)));
            }

            let methods = c
                .methods
                .iter()
                .map(|m| function_to_type_member(m, &c.name, tokens, false))
                .collect();
            let static_methods = c
                .static_methods
                .iter()
                .map(|m| function_to_type_member(m, &c.name, tokens, true))
                .collect();
            let static_properties = c
                .static_properties
                .iter()
                .map(|(name, _init, _)| {
                    // Try to infer type from the initializer expression
                    (name.clone(), "any".to_string())
                })
                .collect();

            types.insert(
                c.name.clone(),
                TypeDefinition {
                    name: c.name.clone(),
                    kind: SemanticSymbolKind::Class,
                    fields,
                    methods,
                    static_methods,
                    static_properties,
                    variants: vec![],
                    extends: c.extends.clone(),
                    implements: c.implements.clone(),
                    type_params: c.type_params.clone(),
                    line,
                    col,
                    is_abstract: c.is_abstract,
                    is_sealed: c.is_sealed,
                    exported: *exported,
                },
            );
        }
        StmtKind::Struct(sd, exported) => {
            let (line, col) = find_identifier_position(tokens, &sd.name, TokenKind::Struct)
                .unwrap_or((s.span.line, s.span.col));

            let fields = sd
                .fields
                .iter()
                .map(|(fname, ftype)| (fname.clone(), ftype.clone(), VisibilityKind::Default))
                .collect();

            types.insert(
                sd.name.clone(),
                TypeDefinition {
                    name: sd.name.clone(),
                    kind: SemanticSymbolKind::Struct,
                    fields,
                    methods: vec![],
                    static_methods: vec![],
                    static_properties: vec![],
                    variants: vec![],
                    extends: None,
                    implements: vec![],
                    type_params: sd.type_params.clone(),
                    line,
                    col,
                    is_abstract: false,
                    is_sealed: false,
                    exported: *exported,
                },
            );
        }
        StmtKind::Enum(e, exported) => {
            let (line, col) = find_identifier_position(tokens, &e.name, TokenKind::Enum)
                .unwrap_or((s.span.line, s.span.col));

            types.insert(
                e.name.clone(),
                TypeDefinition {
                    name: e.name.clone(),
                    kind: SemanticSymbolKind::Enum,
                    fields: vec![],
                    methods: vec![],
                    static_methods: vec![],
                    static_properties: vec![],
                    variants: e.variants.clone(),
                    extends: None,
                    implements: vec![],
                    type_params: vec![],
                    line,
                    col,
                    is_abstract: false,
                    is_sealed: false,
                    exported: *exported,
                },
            );
        }
        StmtKind::ExportDefaultClass(c) => {
            let (line, col) = find_identifier_position(tokens, &c.name, TokenKind::Class)
                .unwrap_or((s.span.line, s.span.col));

            let mut fields = Vec::new();
            for (fname, ftype, fvis, _, _) in &c.fields {
                fields.push((fname.clone(), ftype.clone(), map_visibility(fvis)));
            }

            let methods = c
                .methods
                .iter()
                .map(|m| function_to_type_member(m, &c.name, tokens, false))
                .collect();
            let static_methods = c
                .static_methods
                .iter()
                .map(|m| function_to_type_member(m, &c.name, tokens, true))
                .collect();
            let static_properties = c
                .static_properties
                .iter()
                .map(|(name, _init, _)| (name.clone(), "any".to_string()))
                .collect();

            types.insert(
                c.name.clone(),
                TypeDefinition {
                    name: c.name.clone(),
                    kind: SemanticSymbolKind::Class,
                    fields,
                    methods,
                    static_methods,
                    static_properties,
                    variants: vec![],
                    extends: c.extends.clone(),
                    implements: c.implements.clone(),
                    type_params: c.type_params.clone(),
                    line,
                    col,
                    is_abstract: c.is_abstract,
                    is_sealed: c.is_sealed,
                    exported: true,
                },
            );
        }
        StmtKind::Interface(i, exported) => {
            let (line, col) = find_identifier_position(tokens, &i.name, TokenKind::Interface)
                .unwrap_or((s.span.line, s.span.col));

            let methods = i
                .methods
                .iter()
                .map(|m| function_to_type_member(m, &i.name, tokens, false))
                .collect();

            types.insert(
                i.name.clone(),
                TypeDefinition {
                    name: i.name.clone(),
                    kind: SemanticSymbolKind::Interface,
                    fields: vec![],
                    methods,
                    static_methods: vec![],
                    static_properties: vec![],
                    variants: vec![],
                    extends: None,
                    implements: vec![],
                    type_params: i.type_params.clone(),
                    line,
                    col,
                    is_abstract: false,
                    is_sealed: false,
                    exported: *exported,
                },
            );
        }
        StmtKind::TypeAlias(ta, exported) => {
            let (line, col) = find_identifier_position(tokens, &ta.name, TokenKind::Type)
                .unwrap_or((s.span.line, s.span.col));

            let fields = ta
                .fields
                .iter()
                .map(|(fname, _opt, ftype)| (fname.clone(), ftype.clone(), VisibilityKind::Default))
                .collect();

            types.insert(
                ta.name.clone(),
                TypeDefinition {
                    name: ta.name.clone(),
                    kind: SemanticSymbolKind::TypeAlias,
                    fields,
                    methods: vec![],
                    static_methods: vec![],
                    static_properties: vec![],
                    variants: vec![],
                    extends: None,
                    implements: vec![],
                    type_params: ta.type_params.clone(),
                    line,
                    col,
                    is_abstract: false,
                    is_sealed: false,
                    exported: *exported,
                },
            );
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                collect_type_definition(s, types, tokens);
            }
        }
        _ => {}
    }
}

fn function_to_type_member(
    func: &Function,
    _parent: &str,
    tokens: &[Token],
    is_static: bool,
) -> TypeMember {
    let params: Vec<(String, Option<String>)> = func
        .params
        .iter()
        .map(|(name, _, type_ann)| (name.clone(), type_ann.clone()))
        .collect();

    let param_str: Vec<String> = func
        .params
        .iter()
        .map(|(name, _, type_ann)| {
            if let Some(t) = type_ann {
                format!("{}: {}", name, t)
            } else {
                name.clone()
            }
        })
        .collect();

    let signature = format!(
        "fn {}({}){}",
        func.name,
        param_str.join(", "),
        func.ret_type
            .as_ref()
            .map(|t| format!(" -> {}", t))
            .unwrap_or_default()
    );

    let (line, col) = find_identifier_position(tokens, &func.name, TokenKind::Fn).unwrap_or((0, 0));

    TypeMember {
        name: func.name.clone(),
        kind: if is_static {
            SemanticSymbolKind::StaticMethod
        } else {
            SemanticSymbolKind::Method
        },
        ty: func.ret_type.as_ref().and_then(|t| type_from_name(t)),
        type_annotation: func.ret_type.clone(),
        signature: Some(signature),
        params,
        return_type: func.ret_type.clone(),
        is_static,
        is_async: func.is_async,
        visibility: map_visibility_kind(&func.visibility),
        line,
        col,
    }
}

fn find_identifier_position(
    tokens: &[Token],
    name: &str,
    keyword: TokenKind,
) -> Option<(usize, usize)> {
    for (i, tok) in tokens.iter().enumerate() {
        if tok.kind == keyword {
            if i + 1 < tokens.len() {
                if tokens[i + 1].kind == TokenKind::Identifier && tokens[i + 1].lexeme == name {
                    return Some((tokens[i + 1].line, tokens[i + 1].col));
                }
            }
        }
    }
    None
}

fn collect_symbols(
    s: &Stmt,
    symbols: &mut Vec<SymbolEntry>,
    tokens: &[Token],
    parent_type: Option<&str>,
) {
    match &s.kind {
        StmtKind::Let(name, init, type_ann, export, is_const, readonly) => {
            let (line, col) = find_let_position(tokens, name).unwrap_or((s.span.line, s.span.col));
            let ty = type_ann.as_ref().and_then(|t| type_from_name(t));
            symbols.push(SymbolEntry {
                name: name.clone(),
                kind: if *is_const {
                    SemanticSymbolKind::Constant
                } else {
                    SemanticSymbolKind::Variable
                },
                ty,
                type_annotation: type_ann.clone(),
                line,
                col,
                end_col: col + name.len(),
                visibility: VisibilityKind::Default,
                mutable: !*is_const && !*readonly,
                documentation: None,
                signature: type_ann.as_ref().map(|t| format!("{}: {}", name, t)),
                params: vec![],
                return_type: None,
                type_params: vec![],
                is_static: false,
                is_async: false,
                parent_type: None,
                exported: *export,
            });
            if let Some(e) = init {
                collect_symbols_from_expr(e, symbols, tokens);
            }
        }
        StmtKind::LetTuple(names, type_anns, init, _export, _is_const, _readonly) => {
            for (i, name) in names.iter().enumerate() {
                let type_ann = type_anns.as_ref().and_then(|v| v.get(i)).cloned();
                let (line, col) =
                    find_let_position(tokens, name).unwrap_or((s.span.line, s.span.col));
                symbols.push(SymbolEntry {
                    name: name.clone(),
                    kind: SemanticSymbolKind::Variable,
                    ty: type_ann.as_ref().and_then(|t| type_from_name(t)),
                    type_annotation: type_ann,
                    line,
                    col,
                    end_col: col + name.len(),
                    visibility: VisibilityKind::Default,
                    mutable: true,
                    documentation: None,
                    signature: None,
                    params: vec![],
                    return_type: None,
                    type_params: vec![],
                    is_static: false,
                    is_async: false,
                    parent_type: None,
                    exported: false,
                });
            }
            if let Some(e) = init {
                collect_symbols_from_expr(e, symbols, tokens);
            }
        }
        StmtKind::LetObject(pairs, init, _export, _is_const, _readonly) => {
            for (key, alias) in pairs {
                let name = alias.clone().unwrap_or_else(|| key.clone());
                let name_len = name.len();
                let (line, col) =
                    find_let_position(tokens, &name).unwrap_or((s.span.line, s.span.col));
                symbols.push(SymbolEntry {
                    name,
                    kind: SemanticSymbolKind::Variable,
                    ty: None,
                    type_annotation: None,
                    line,
                    col,
                    end_col: col + name_len,
                    visibility: VisibilityKind::Default,
                    mutable: true,
                    documentation: None,
                    signature: None,
                    params: vec![],
                    return_type: None,
                    type_params: vec![],
                    is_static: false,
                    is_async: false,
                    parent_type: None,
                    exported: false,
                });
            }
            if let Some(e) = init {
                collect_symbols_from_expr(e, symbols, tokens);
            }
        }
        StmtKind::Function(func, export) => {
            collect_function_symbols(func, symbols, tokens, parent_type, *export);
        }
        StmtKind::Class(class, export) => {
            collect_class_symbols(class, symbols, tokens, *export);
        }
        StmtKind::ExportDefaultClass(class) => {
            collect_class_symbols(class, symbols, tokens, true);
        }
        StmtKind::ExportDefaultFunction(func) => {
            collect_function_symbols(func, symbols, tokens, None, true);
        }
        StmtKind::Struct(sd, export) => {
            let (line, col) = find_identifier_position(tokens, &sd.name, TokenKind::Struct)
                .unwrap_or((s.span.line, s.span.col));
            symbols.push(SymbolEntry {
                name: sd.name.clone(),
                kind: SemanticSymbolKind::Struct,
                ty: None,
                type_annotation: None,
                line,
                col,
                end_col: col + sd.name.len(),
                visibility: VisibilityKind::Default,
                mutable: false,
                documentation: None,
                signature: Some(format!("struct {}", sd.name)),
                params: vec![],
                return_type: None,
                type_params: sd.type_params.clone(),
                is_static: false,
                is_async: false,
                parent_type: None,
                exported: *export,
            });
        }
        StmtKind::Enum(enum_decl, export) => {
            let (line, col) = find_identifier_position(tokens, &enum_decl.name, TokenKind::Enum)
                .unwrap_or((s.span.line, s.span.col));
            symbols.push(SymbolEntry {
                name: enum_decl.name.clone(),
                kind: SemanticSymbolKind::Enum,
                ty: None,
                type_annotation: None,
                line,
                col,
                end_col: col + enum_decl.name.len(),
                visibility: VisibilityKind::Default,
                mutable: false,
                documentation: None,
                signature: Some(format!("enum {}", enum_decl.name)),
                params: vec![],
                return_type: None,
                type_params: vec![],
                is_static: false,
                is_async: false,
                parent_type: None,
                exported: *export,
            });
            // Add enum variants as symbols
            for (vname, vtype) in &enum_decl.variants {
                symbols.push(SymbolEntry {
                    name: vname.clone(),
                    kind: SemanticSymbolKind::EnumVariant,
                    ty: vtype.as_ref().and_then(|t| type_from_name(t)),
                    type_annotation: vtype.clone(),
                    line: s.span.line,
                    col: s.span.col,
                    end_col: s.span.col + vname.len(),
                    visibility: VisibilityKind::Public,
                    mutable: false,
                    documentation: None,
                    signature: Some(vname.clone()),
                    params: vec![],
                    return_type: None,
                    type_params: vec![],
                    is_static: true,
                    is_async: false,
                    parent_type: Some(enum_decl.name.clone()),
                    exported: *export,
                });
            }
        }
        StmtKind::Interface(iface, export) => {
            let (line, col) = find_identifier_position(tokens, &iface.name, TokenKind::Interface)
                .unwrap_or((s.span.line, s.span.col));
            symbols.push(SymbolEntry {
                name: iface.name.clone(),
                kind: SemanticSymbolKind::Interface,
                ty: None,
                type_annotation: None,
                line,
                col,
                end_col: col + iface.name.len(),
                visibility: VisibilityKind::Default,
                mutable: false,
                documentation: None,
                signature: Some(format!("interface {}", iface.name)),
                params: vec![],
                return_type: None,
                type_params: iface.type_params.clone(),
                is_static: false,
                is_async: false,
                parent_type: None,
                exported: *export,
            });
        }
        StmtKind::TypeAlias(ta, export) => {
            let (line, col) = find_identifier_position(tokens, &ta.name, TokenKind::Type)
                .unwrap_or((s.span.line, s.span.col));
            symbols.push(SymbolEntry {
                name: ta.name.clone(),
                kind: SemanticSymbolKind::TypeAlias,
                ty: None,
                type_annotation: None,
                line,
                col,
                end_col: col + ta.name.len(),
                visibility: VisibilityKind::Default,
                mutable: false,
                documentation: None,
                signature: Some(format!("type {}", ta.name)),
                params: vec![],
                return_type: None,
                type_params: ta.type_params.clone(),
                is_static: false,
                is_async: false,
                parent_type: None,
                exported: *export,
            });
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                collect_symbols(s, symbols, tokens, parent_type);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_symbols(then_branch, symbols, tokens, parent_type);
            if let Some(e) = else_branch {
                collect_symbols(e, symbols, tokens, parent_type);
            }
        }
        StmtKind::While { body, .. } => {
            collect_symbols(body, symbols, tokens, parent_type);
        }
        StmtKind::ForIn {
            name, iter, body, ..
        } => {
            // Add the loop variable as a symbol
            symbols.push(SymbolEntry {
                name: name.clone(),
                kind: SemanticSymbolKind::Variable,
                ty: None,
                type_annotation: None,
                line: s.span.line,
                col: s.span.col,
                end_col: s.span.col + name.len(),
                visibility: VisibilityKind::Default,
                mutable: false,
                documentation: None,
                signature: None,
                params: vec![],
                return_type: None,
                type_params: vec![],
                is_static: false,
                is_async: false,
                parent_type: None,
                exported: false,
            });
            collect_symbols_from_expr(iter, symbols, tokens);
            collect_symbols(body, symbols, tokens, parent_type);
        }
        StmtKind::Region { body, .. } => {
            collect_symbols(body, symbols, tokens, parent_type);
        }
        StmtKind::UnsafeBlock(body) => {
            collect_symbols(body, symbols, tokens, parent_type);
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            collect_symbols(try_block, symbols, tokens, parent_type);
            collect_symbols(catch_block, symbols, tokens, parent_type);
        }
        StmtKind::Defer(body) => {
            collect_symbols(body, symbols, tokens, parent_type);
        }
        StmtKind::Return(expr) => {
            if let Some(e) = expr {
                collect_symbols_from_expr(e, symbols, tokens);
            }
        }
        StmtKind::ExprStmt(expr) => {
            collect_symbols_from_expr(expr, symbols, tokens);
        }
        _ => {}
    }
}

fn collect_function_symbols(
    func: &Function,
    symbols: &mut Vec<SymbolEntry>,
    tokens: &[Token],
    parent_type: Option<&str>,
    exported: bool,
) {
    let (line, col) = find_identifier_position(tokens, &func.name, TokenKind::Fn).unwrap_or((0, 0));

    let params: Vec<(String, Option<String>)> = func
        .params
        .iter()
        .map(|(name, _, type_ann)| (name.clone(), type_ann.clone()))
        .collect();

    let param_str: Vec<String> = func
        .params
        .iter()
        .map(|(name, _, type_ann)| {
            if let Some(t) = type_ann {
                format!("{}: {}", name, t)
            } else {
                name.clone()
            }
        })
        .collect();

    let signature = format!(
        "fn {}({}){}",
        func.name,
        param_str.join(", "),
        func.ret_type
            .as_ref()
            .map(|t| format!(" -> {}", t))
            .unwrap_or_default()
    );

    let kind = if parent_type.is_some() {
        if func.is_static {
            SemanticSymbolKind::StaticMethod
        } else {
            SemanticSymbolKind::Method
        }
    } else {
        SemanticSymbolKind::Function
    };

    symbols.push(SymbolEntry {
        name: func.name.clone(),
        kind,
        ty: func.ret_type.as_ref().and_then(|t| type_from_name(t)),
        type_annotation: func.ret_type.clone(),
        line,
        col,
        end_col: col + func.name.len(),
        visibility: map_visibility_kind(&func.visibility),
        mutable: false,
        documentation: None,
        signature: Some(signature),
        params: params.clone(),
        return_type: func.ret_type.clone(),
        type_params: func.type_params.clone(),
        is_static: func.is_static,
        is_async: func.is_async,
        parent_type: parent_type.map(|p| p.to_string()),
        exported,
    });

    // Add parameters as symbols
    for (name, _, type_ann) in &func.params {
        symbols.push(SymbolEntry {
            name: name.clone(),
            kind: SemanticSymbolKind::Parameter,
            ty: type_ann.as_ref().and_then(|t| type_from_name(t)),
            type_annotation: type_ann.clone(),
            line: s_line(func),
            col: 0,
            end_col: name.len(),
            visibility: VisibilityKind::Default,
            mutable: true,
            documentation: None,
            signature: type_ann.as_ref().map(|t| format!("{}: {}", name, t)),
            params: vec![],
            return_type: None,
            type_params: vec![],
            is_static: false,
            is_async: false,
            parent_type: None,
            exported: false,
        });
    }

    // Recurse into function body
    for stmt in func.body.iter() {
        collect_symbols(stmt, symbols, tokens, None);
    }
}

fn s_line(func: &Function) -> usize {
    // We don't have the span here, use a default
    let _ = func;
    0
}

fn collect_class_symbols(
    class: &ClassDecl,
    symbols: &mut Vec<SymbolEntry>,
    tokens: &[Token],
    exported: bool,
) {
    let (line, col) =
        find_identifier_position(tokens, &class.name, TokenKind::Class).unwrap_or((0, 0));

    let mut sig = format!("class {}", class.name);
    if let Some(parent) = &class.extends {
        sig.push_str(&format!(" extends {}", parent));
    }
    if !class.implements.is_empty() {
        sig.push_str(&format!(" implements {}", class.implements.join(", ")));
    }

    symbols.push(SymbolEntry {
        name: class.name.clone(),
        kind: SemanticSymbolKind::Class,
        ty: None,
        type_annotation: None,
        line,
        col,
        end_col: col + class.name.len(),
        visibility: VisibilityKind::Default,
        mutable: false,
        documentation: None,
        signature: Some(sig),
        params: vec![],
        return_type: None,
        type_params: class.type_params.clone(),
        is_static: false,
        is_async: false,
        parent_type: None,
        exported,
    });

    // Add fields as symbols
    for (fname, ftype, fvis, _, _) in &class.fields {
        symbols.push(SymbolEntry {
            name: fname.clone(),
            kind: SemanticSymbolKind::Field,
            ty: type_from_name(ftype),
            type_annotation: Some(ftype.clone()),
            line: 0,
            col: 0,
            end_col: fname.len(),
            visibility: map_visibility(fvis),
            mutable: true,
            documentation: None,
            signature: Some(format!("{}: {}", fname, ftype)),
            params: vec![],
            return_type: None,
            type_params: vec![],
            is_static: false,
            is_async: false,
            parent_type: Some(class.name.clone()),
            exported,
        });
    }

    // Add methods
    for method in &class.methods {
        collect_function_symbols(method, symbols, tokens, Some(&class.name), exported);
    }
    for method in &class.static_methods {
        collect_function_symbols(method, symbols, tokens, Some(&class.name), exported);
    }
}

fn collect_symbols_from_expr(expr: &Expr, symbols: &mut Vec<SymbolEntry>, _tokens: &[Token]) {
    match &expr.kind {
        ExprKind::Call(callee, args, _) => {
            collect_symbols_from_expr(callee, symbols, _tokens);
            for a in args {
                collect_symbols_from_expr(a, symbols, _tokens);
            }
        }
        ExprKind::Binary(l, _, r) | ExprKind::Logical(l, _, r) => {
            collect_symbols_from_expr(l, symbols, _tokens);
            collect_symbols_from_expr(r, symbols, _tokens);
        }
        ExprKind::Unary(_, e)
        | ExprKind::Grouping(e)
        | ExprKind::Await(e)
        | ExprKind::Spawn(e)
        | ExprKind::Throw(e)
        | ExprKind::Spread(e)
        | ExprKind::NonNull(e)
        | ExprKind::Try(e) => {
            collect_symbols_from_expr(e, symbols, _tokens);
        }
        ExprKind::Assign(_, e) | ExprKind::AssignOp(_, _, e) => {
            collect_symbols_from_expr(e, symbols, _tokens);
        }
        ExprKind::Index(base, idx) => {
            collect_symbols_from_expr(base, symbols, _tokens);
            collect_symbols_from_expr(idx, symbols, _tokens);
        }
        ExprKind::Get(obj, _) => collect_symbols_from_expr(obj, symbols, _tokens),
        ExprKind::Set(obj, _, val) => {
            collect_symbols_from_expr(obj, symbols, _tokens);
            collect_symbols_from_expr(val, symbols, _tokens);
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) | ExprKind::SetLiteral(items) => {
            for i in items {
                collect_symbols_from_expr(i, symbols, _tokens);
            }
        }
        ExprKind::Object(fields) | ExprKind::StructLiteral(_, fields) => {
            for (_, e) in fields {
                collect_symbols_from_expr(e, symbols, _tokens);
            }
        }
        ExprKind::New(ctor, args) => {
            collect_symbols_from_expr(ctor, symbols, _tokens);
            for a in args {
                collect_symbols_from_expr(a, symbols, _tokens);
            }
        }
        ExprKind::Conditional(c, t, e) => {
            collect_symbols_from_expr(c, symbols, _tokens);
            collect_symbols_from_expr(t, symbols, _tokens);
            collect_symbols_from_expr(e, symbols, _tokens);
        }
        ExprKind::Match(scrut, arms) => {
            collect_symbols_from_expr(scrut, symbols, _tokens);
            for (_, body) in arms {
                collect_symbols_from_expr(body, symbols, _tokens);
            }
        }
        ExprKind::Fn(_params, body, _) => {
            for stmt in body.iter() {
                collect_symbols(stmt, symbols, _tokens, None);
            }
        }
        ExprKind::Range(s, e, _) => {
            collect_symbols_from_expr(s, symbols, _tokens);
            collect_symbols_from_expr(e, symbols, _tokens);
        }
        ExprKind::Cast(e, _) | ExprKind::Format(e, _) => {
            collect_symbols_from_expr(e, symbols, _tokens);
        }
        _ => {}
    }
}

fn collect_imports(s: &Stmt, imports: &mut Vec<ImportInfo>) {
    match &s.kind {
        StmtKind::Import { path, alias } => {
            imports.push(ImportInfo {
                path: path.clone(),
                alias: alias.clone(),
                names: vec![],
                kind: ImportKind::Named,
                line: s.span.line,
                col: s.span.col,
            });
        }
        StmtKind::ImportDefault { path, alias } => {
            imports.push(ImportInfo {
                path: path.clone(),
                alias: alias.clone(),
                names: vec![],
                kind: ImportKind::Default,
                line: s.span.line,
                col: s.span.col,
            });
        }
        StmtKind::ImportNames { path, names } => {
            imports.push(ImportInfo {
                path: path.clone(),
                alias: String::new(),
                names: names.clone(),
                kind: ImportKind::Names,
                line: s.span.line,
                col: s.span.col,
            });
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                collect_imports(s, imports);
            }
        }
        _ => {}
    }
}

fn collect_exports(s: &Stmt, exports: &mut FastSet<String>) {
    match &s.kind {
        StmtKind::Let(name, _, _, export, _, _) => {
            if *export {
                exports.insert(name.clone());
            }
        }
        StmtKind::Function(func, export) => {
            if *export {
                exports.insert(func.name.clone());
            }
        }
        StmtKind::Class(class, export) => {
            if *export {
                exports.insert(class.name.clone());
            }
        }
        StmtKind::Struct(sd, export) => {
            if *export {
                exports.insert(sd.name.clone());
            }
        }
        StmtKind::Enum(e, export) => {
            if *export {
                exports.insert(e.name.clone());
            }
        }
        StmtKind::Interface(i, export) => {
            if *export {
                exports.insert(i.name.clone());
            }
        }
        StmtKind::TypeAlias(ta, export) => {
            if *export {
                exports.insert(ta.name.clone());
            }
        }
        StmtKind::ExportDefault(name) => {
            exports.insert(name.clone());
        }
        StmtKind::ExportDefaultFunction(func) => {
            exports.insert(func.name.clone());
        }
        StmtKind::ExportDefaultClass(class) => {
            exports.insert(class.name.clone());
        }
        _ => {}
    }
}

fn build_scope_tree(s: &Stmt, scopes: &mut Vec<ScopeInfo>, parent_id: usize, tokens: &[Token]) {
    match &s.kind {
        StmtKind::Function(func, _) => {
            // Create a function scope
            let scope_id = scopes.len();
            scopes.push(ScopeInfo {
                id: scope_id,
                parent: Some(parent_id),
                symbols: vec![],
                kind: ScopeKind::Function,
            });

            // Add parameters to the function scope
            for (name, _, type_ann) in &func.params {
                scopes[scope_id].symbols.push(SymbolEntry {
                    name: name.clone(),
                    kind: SemanticSymbolKind::Parameter,
                    ty: type_ann.as_ref().and_then(|t| type_from_name(t)),
                    type_annotation: type_ann.clone(),
                    line: 0,
                    col: 0,
                    end_col: name.len(),
                    visibility: VisibilityKind::Default,
                    mutable: true,
                    documentation: None,
                    signature: type_ann.as_ref().map(|t| format!("{}: {}", name, t)),
                    params: vec![],
                    return_type: None,
                    type_params: vec![],
                    is_static: false,
                    is_async: false,
                    parent_type: None,
                    exported: false,
                });
            }

            // Process function body
            for stmt in func.body.iter() {
                build_scope_tree(stmt, scopes, scope_id, tokens);
            }
        }
        StmtKind::Class(class, _) => {
            // Create a class scope
            let scope_id = scopes.len();
            scopes.push(ScopeInfo {
                id: scope_id,
                parent: Some(parent_id),
                symbols: vec![],
                kind: ScopeKind::Class,
            });

            // Add fields to class scope
            for (fname, ftype, _, _, _) in &class.fields {
                scopes[scope_id].symbols.push(SymbolEntry {
                    name: fname.clone(),
                    kind: SemanticSymbolKind::Field,
                    ty: type_from_name(ftype),
                    type_annotation: Some(ftype.clone()),
                    line: 0,
                    col: 0,
                    end_col: fname.len(),
                    visibility: VisibilityKind::Default,
                    mutable: true,
                    documentation: None,
                    signature: Some(format!("{}: {}", fname, ftype)),
                    params: vec![],
                    return_type: None,
                    type_params: vec![],
                    is_static: false,
                    is_async: false,
                    parent_type: Some(class.name.clone()),
                    exported: false,
                });
            }

            // Process methods
            for method in &class.methods {
                build_scope_tree(
                    &Stmt {
                        kind: StmtKind::Function(method.clone(), false),
                        span: Span::default(),
                    },
                    scopes,
                    scope_id,
                    tokens,
                );
            }
        }
        StmtKind::Block(stmts) => {
            let scope_id = scopes.len();
            scopes.push(ScopeInfo {
                id: scope_id,
                parent: Some(parent_id),
                symbols: vec![],
                kind: ScopeKind::Block,
            });
            for s in stmts {
                build_scope_tree(s, scopes, scope_id, tokens);
            }
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            build_scope_tree(then_branch, scopes, parent_id, tokens);
            if let Some(e) = else_branch {
                build_scope_tree(e, scopes, parent_id, tokens);
            }
        }
        StmtKind::While { body, .. } => {
            build_scope_tree(body, scopes, parent_id, tokens);
        }
        StmtKind::ForIn { name, body, .. } => {
            let scope_id = scopes.len();
            scopes.push(ScopeInfo {
                id: scope_id,
                parent: Some(parent_id),
                symbols: vec![SymbolEntry {
                    name: name.clone(),
                    kind: SemanticSymbolKind::Variable,
                    ty: None,
                    type_annotation: None,
                    line: 0,
                    col: 0,
                    end_col: name.len(),
                    visibility: VisibilityKind::Default,
                    mutable: false,
                    documentation: None,
                    signature: None,
                    params: vec![],
                    return_type: None,
                    type_params: vec![],
                    is_static: false,
                    is_async: false,
                    parent_type: None,
                    exported: false,
                }],
                kind: ScopeKind::ForLoop,
            });
            build_scope_tree(body, scopes, scope_id, tokens);
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            build_scope_tree(try_block, scopes, parent_id, tokens);
            build_scope_tree(catch_block, scopes, parent_id, tokens);
        }
        StmtKind::Region { body, .. } => {
            build_scope_tree(body, scopes, parent_id, tokens);
        }
        StmtKind::UnsafeBlock(body) => {
            build_scope_tree(body, scopes, parent_id, tokens);
        }
        StmtKind::Defer(body) => {
            build_scope_tree(body, scopes, parent_id, tokens);
        }
        _ => {}
    }
}

fn find_let_position(tokens: &[Token], name: &str) -> Option<(usize, usize)> {
    for (i, tok) in tokens.iter().enumerate() {
        if (tok.kind == TokenKind::Let || tok.kind == TokenKind::Const) && i + 1 < tokens.len() {
            if tokens[i + 1].kind == TokenKind::Identifier && tokens[i + 1].lexeme == name {
                return Some((tokens[i + 1].line, tokens[i + 1].col));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_simple() {
        let source = r#"
struct User {
    id: i32,
    name: string,
}

let user: User = User(1, "Ajay")
user.
"#;
        let index = index_source(source);
        assert!(index.types.contains_key("User"));
        let td = index.get_type("User").unwrap();
        assert_eq!(td.kind, SemanticSymbolKind::Struct);
        assert_eq!(td.fields.len(), 2);
    }

    #[test]
    fn test_member_resolution() {
        let source = r#"
struct User {
    id: i32,
    name: string,
}

let user: User = User(1, "Ajay")
"#;
        let index = index_source(source);

        // Infer type of `user` variable
        let user_sym = index.find_declaration("user").unwrap();
        assert_eq!(user_sym.type_annotation.as_deref(), Some("User"));

        // Get members of User
        let members = index.get_type_members("User");
        let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"name"));
    }

    #[test]
    fn test_class_methods() {
        let source = r#"class User {
    id: i32;
    name: string;

    fn save(): bool {
        return true;
    }

    fn get_name(): string {
        return this.name;
    }
}

let user = User(1, "Ajay");
"#;
        let index = index_source(source);
        let td = index.get_type("User").unwrap();
        assert_eq!(td.kind, SemanticSymbolKind::Class);
        assert_eq!(td.fields.len(), 2);
        assert_eq!(td.methods.len(), 2);

        let members = index.get_type_members("User");
        let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"name"));
        assert!(names.contains(&"save"));
        assert!(names.contains(&"get_name"));
    }

    #[test]
    fn test_enum_variants() {
        let source = r#"
enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let index = index_source(source);
        let td = index.get_type("Color").unwrap();
        assert_eq!(td.kind, SemanticSymbolKind::Enum);
        assert_eq!(td.variants.len(), 3);

        let members = index.get_type_members("Color");
        let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Red"));
        assert!(names.contains(&"Green"));
        assert!(names.contains(&"Blue"));
    }

    #[test]
    fn test_completions_for_type() {
        let source = r#"class User {
    id: i32;
    name: string;

    fn save(): bool {
        return true;
    }
}
"#;
        let index = index_source(source);
        let ty = Ty::GenericInstance {
            name: "User".to_string(),
            args: vec![],
        };
        let completions = index.completions_for_type(&ty);
        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"id"), "Labels: {:?}", labels);
        assert!(labels.contains(&"name"));
        assert!(labels.contains(&"save"));
    }

    #[test]
    fn test_completions_for_type_generic_param() {
        // `let user: User` resolves the annotation to `GenericParam("User")`
        // via the type table — member completion must still work.
        let source = r#"class User {
    id: i32;
    name: string;

    fn save(): bool {
        return true;
    }
}

let user: User = User(1, "Ajay");
"#;
        let index = index_source(source);
        let ty = Ty::GenericParam("User".to_string());
        let completions = index.completions_for_type(&ty);
        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"id"), "Labels: {:?}", labels);
        assert!(labels.contains(&"name"));
        assert!(labels.contains(&"save"));
    }

    #[test]
    fn test_resolve_symbol_type_from_annotation() {
        let source = r#"class User {
    id: i32;
    name: string;
}

let user: User = User(1, "Ajay");
"#;
        let index = index_source(source);
        let ty = index.resolve_symbol_type("user").unwrap();
        assert_eq!(
            ty,
            Ty::GenericInstance {
                name: "User".to_string(),
                args: vec![]
            }
        );
    }

    #[test]
    fn test_resolve_symbol_type_from_constructor() {
        // No annotation — the type is inferred from `User(1, "Ajay")`.
        let source = r#"class User {
    id: i32;
    name: string;

    fn save(): bool {
        return true;
    }
}

let user = User(1, "Ajay");
"#;
        let index = index_source(source);
        let ty = index.resolve_symbol_type("user").unwrap();
        assert_eq!(
            ty,
            Ty::GenericInstance {
                name: "User".to_string(),
                args: vec![]
            }
        );
    }

    #[test]
    fn test_resolve_symbol_type_from_constructor_struct() {
        let source = r#"struct Point {
    x: f64,
    y: f64,
}

let p = Point(1.0, 2.0);
"#;
        let index = index_source(source);
        let ty = index.resolve_symbol_type("p").unwrap();
        assert_eq!(
            ty,
            Ty::GenericInstance {
                name: "Point".to_string(),
                args: vec![]
            }
        );

        let completions = index.completions_for_type(&ty);
        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"x"), "Labels: {:?}", labels);
        assert!(labels.contains(&"y"));
    }

    #[test]
    fn test_identifier_or_keyword_at() {
        let source = "let x = 42;";
        let index = index_source(source);
        // internal positions are 1-based
        assert_eq!(index.identifier_or_keyword_at(1, 1), Some("let"));
        assert_eq!(index.identifier_at(1, 1), None);
        assert_eq!(index.identifier_or_keyword_at(1, 5), Some("x"));
    }

    #[test]
    fn test_member_resolution_via_generic_param_usage() {
        let source = r#"class User {
    id: i32;
    name: string;
}

let u: User = User(1, "Ajay");
"#;
        let index = index_source(source);
        let u_sym = index.find_declaration("u").unwrap();
        let ty = u_sym.ty.clone().unwrap();
        assert_eq!(ty, Ty::GenericParam("User".to_string()));
        // Resolving `u.name` must walk through the type table.
        let resolved = index.resolve_symbol_type("u");
        assert!(resolved.is_some());
    }
}
