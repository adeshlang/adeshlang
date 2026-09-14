//! Statement Visitor Pattern
//!
//! Defines the StatementVisitor trait for traversing and executing statements
//! using the visitor pattern.

use crate::parsing::ast::{
    ClassDecl, DecoratorDef, EnumDecl, Expr, ExternFunctionDecl, Function, InterfaceDecl,
    ShareDecl, Stmt, StmtKind, StrongDecl, StructDecl, TypeAliasDecl, WeakDecl,
};

/// Visitor trait for statement execution.
///
/// Implementations of this trait can traverse and process statement AST nodes
/// without deep recursion. The visitor pattern provides:
/// - Clear separation between traversal and execution
/// - Easier testing and debugging
/// - Support for multiple execution strategies
/// - Reduced stack overflow risk
pub trait StatementVisitor {
    /// The output type produced by visiting statements
    type Output;

    /// Visit a type alias declaration
    fn visit_type_alias(&mut self, decl: &TypeAliasDecl, export: bool) -> Self::Output;

    /// Visit a let declaration
    fn visit_let(
        &mut self,
        name: &str,
        init: Option<&Expr>,
        type_ann: Option<&str>,
        export: bool,
        is_const: bool,
        is_readonly: bool,
    ) -> Self::Output;

    /// Visit a tuple destructuring let declaration
    fn visit_let_tuple(
        &mut self,
        names: &[String],
        type_anns: Option<&[String]>,
        init: Option<&Expr>,
        export: bool,
        is_const: bool,
        is_readonly: bool,
    ) -> Self::Output;

    /// Visit an object destructuring let declaration
    fn visit_let_object(
        &mut self,
        bindings: &[(String, Option<String>)],
        init: Option<&Expr>,
        export: bool,
        is_const: bool,
        is_readonly: bool,
    ) -> Self::Output;

    /// Visit an expression statement
    fn visit_expr_stmt(&mut self, expr: &Expr) -> Self::Output;

    /// Visit a block statement
    fn visit_block(&mut self, stmts: &[Stmt]) -> Self::Output;

    /// Visit an if statement
    fn visit_if(
        &mut self,
        cond: &Expr,
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
    ) -> Self::Output;

    /// Visit a while loop
    fn visit_while(&mut self, cond: &Expr, body: &Stmt) -> Self::Output;

    /// Visit a for-in loop
    fn visit_for_in(&mut self, name: &str, iter: &Expr, body: &Stmt) -> Self::Output;

    /// Visit a break statement
    fn visit_break(&mut self) -> Self::Output;

    /// Visit a continue statement
    fn visit_continue(&mut self) -> Self::Output;

    /// Visit a jump statement (goto)
    fn visit_jump(&mut self, target: &Expr) -> Self::Output;

    /// Visit an extend declaration (adds methods to existing class)
    fn visit_extend(
        &mut self,
        name: Option<&str>,
        type_name: &str,
        methods: &[Function],
        export: bool,
    ) -> Self::Output;

    /// Visit a struct declaration
    fn visit_struct(&mut self, decl: &StructDecl, export: bool) -> Self::Output;

    /// Visit an enum declaration
    fn visit_enum(&mut self, decl: &EnumDecl, export: bool) -> Self::Output;

    /// Visit an interface declaration
    fn visit_interface(&mut self, decl: &InterfaceDecl, export: bool) -> Self::Output;

    /// Visit a function declaration
    fn visit_function(&mut self, func: &Function, export: bool) -> Self::Output;

    /// Visit a class declaration
    fn visit_class(&mut self, decl: &ClassDecl, export: bool) -> Self::Output;

    /// Visit an export default function
    fn visit_export_default_function(&mut self, func: &Function) -> Self::Output;

    /// Visit an export default class
    fn visit_export_default_class(&mut self, decl: &ClassDecl) -> Self::Output;

    /// Visit an export default identifier
    fn visit_export_default(&mut self, name: &str) -> Self::Output;

    /// Visit a return statement
    fn visit_return(&mut self, value: Option<&Expr>) -> Self::Output;

    /// Visit an import statement (import alias from path)
    fn visit_import(&mut self, path: &str, alias: &str) -> Self::Output;

    /// Visit an import default statement
    fn visit_import_default(&mut self, path: &str, alias: &str) -> Self::Output;

    /// Visit an import names statement (import { a, b, c } from path)
    fn visit_import_names(&mut self, path: &str, names: &[String]) -> Self::Output;

    /// Visit a C header import (@cImport)
    fn visit_header_import(&mut self, path: &str) -> Self::Output;

    /// Visit an extern function declaration
    fn visit_extern_function(&mut self, decl: &ExternFunctionDecl) -> Self::Output;

    /// Visit an extern block
    fn visit_extern_block(&mut self, abi: &str, functions: &[ExternFunctionDecl]) -> Self::Output;

    /// Visit a try-catch statement
    fn visit_try_catch(
        &mut self,
        try_block: &Stmt,
        err_name: &str,
        catch_block: &Stmt,
    ) -> Self::Output;

    /// Visit a region block (arena memory management)
    fn visit_region(&mut self, name: Option<&str>, body: &Stmt) -> Self::Output;

    /// Visit an unsafe block
    fn visit_unsafe_block(&mut self, body: &Stmt) -> Self::Output;

    /// Visit a share declaration
    fn visit_share_declaration(&mut self, decl: &ShareDecl, export: bool) -> Self::Output;

    /// Visit a strong declaration
    fn visit_strong_declaration(&mut self, decl: &StrongDecl, export: bool) -> Self::Output;

    /// Visit a weak declaration
    fn visit_weak_declaration(&mut self, decl: &WeakDecl, export: bool) -> Self::Output;

    /// Visit a defer statement
    fn visit_defer(&mut self, stmt: &Stmt) -> Self::Output;

    /// Visit a decorator declaration
    fn visit_decorator(&mut self, decl: &DecoratorDef, export: bool) -> Self::Output;

    /// Main dispatch method that routes to the appropriate visit method
    ///
    /// This is the entry point for executing a statement using the visitor.
    fn visit_stmt(&mut self, stmt: &Stmt) -> Self::Output {
        match &stmt.kind {
            StmtKind::ShareDeclaration(inner_stmt, export) => {
                self.visit_share_declaration(inner_stmt, *export)
            }
            StmtKind::StrongDeclaration(inner_stmt, export) => {
                self.visit_strong_declaration(inner_stmt, *export)
            }
            StmtKind::WeakDeclaration(inner_stmt, export) => {
                self.visit_weak_declaration(inner_stmt, *export)
            }
            StmtKind::TypeAlias(decl, export) => self.visit_type_alias(decl, *export),
            StmtKind::Let(name, init, type_ann, export, is_const, is_readonly) => self.visit_let(
                name,
                init.as_ref(),
                type_ann.as_deref(),
                *export,
                *is_const,
                *is_readonly,
            ),
            StmtKind::LetTuple(names, type_anns, init, export, is_const, is_readonly) => self
                .visit_let_tuple(
                    names,
                    type_anns.as_deref(),
                    init.as_ref(),
                    *export,
                    *is_const,
                    *is_readonly,
                ),
            StmtKind::LetObject(bindings, init, export, is_const, is_readonly) => {
                self.visit_let_object(bindings, init.as_ref(), *export, *is_const, *is_readonly)
            }
            StmtKind::ExprStmt(expr) => self.visit_expr_stmt(expr),
            StmtKind::Block(stmts) => self.visit_block(stmts),
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => self.visit_if(cond, then_branch, else_branch.as_deref()),
            StmtKind::While { cond, body } => self.visit_while(cond, body),
            StmtKind::ForIn { name, iter, body } => self.visit_for_in(name, iter, body),
            StmtKind::Break => self.visit_break(),
            StmtKind::Continue => self.visit_continue(),
            StmtKind::Jump(target) => self.visit_jump(target),
            StmtKind::Extend(name, type_name, methods, export) => {
                self.visit_extend(name.as_deref(), type_name, methods, *export)
            }
            StmtKind::Struct(decl, export) => self.visit_struct(decl, *export),
            StmtKind::Enum(decl, export) => self.visit_enum(decl, *export),
            StmtKind::Interface(decl, export) => self.visit_interface(decl, *export),
            StmtKind::Function(func, export) => self.visit_function(func, *export),
            StmtKind::Class(decl, export) => self.visit_class(decl, *export),
            StmtKind::ExportDefaultFunction(func) => self.visit_export_default_function(func),
            StmtKind::ExportDefaultClass(decl) => self.visit_export_default_class(decl),
            StmtKind::ExportDefault(name) => self.visit_export_default(name),
            StmtKind::Return(value) => self.visit_return(value.as_ref()),
            StmtKind::Import { path, alias } => self.visit_import(path, alias),
            StmtKind::ImportDefault { path, alias } => self.visit_import_default(path, alias),
            StmtKind::ImportNames { path, names } => self.visit_import_names(path, names),
            StmtKind::HeaderImport { path } => self.visit_header_import(path),
            StmtKind::ExternFunction(decl) => self.visit_extern_function(decl),
            StmtKind::ExternBlock { abi, functions } => self.visit_extern_block(abi, functions),
            StmtKind::TryCatch {
                try_block,
                err_name,
                catch_block,
            } => self.visit_try_catch(try_block, err_name, catch_block),
            StmtKind::Region { name, body } => self.visit_region(name.as_deref(), body),
            StmtKind::UnsafeBlock(body) => self.visit_unsafe_block(body),
            StmtKind::Defer(stmt) => self.visit_defer(stmt),
            StmtKind::Decorator(decl, export) => self.visit_decorator(decl, *export),
        }
    }
}
