//! Adesh Code Formatter
//!
//! Provides functionality to format Adesh source code with consistent styling.

use crate::parsing::ast::{
    ClassDecl, Expr, ExprKind, Function, Pattern, Stmt, StmtKind, TokenKind, Value,
};

/// Configuration for the formatter
#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// Number of spaces per indentation level
    pub indent_size: usize,
    /// Maximum line width before breaking
    pub max_line_width: usize,
    /// Use tabs instead of spaces
    pub use_tabs: bool,
    /// Add trailing commas in arrays/objects
    pub trailing_commas: bool,
    /// Space inside braces: { a } vs {a}
    pub space_in_braces: bool,
    /// Space inside brackets: [ a ] vs [a]
    pub space_in_brackets: bool,
    /// Space before function parentheses
    pub space_before_function_paren: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_size: 4,
            max_line_width: 80,
            use_tabs: false,
            trailing_commas: false,
            space_in_braces: true,
            space_in_brackets: false,
            space_before_function_paren: false,
        }
    }
}

/// Adesh code formatter
pub struct Formatter {
    config: FormatConfig,
    output: String,
    indent_level: usize,
}

impl Formatter {
    pub fn new(config: FormatConfig) -> Self {
        Self {
            config,
            output: String::new(),
            indent_level: 0,
        }
    }

    /// Format a Adesh program from parsed statements
    pub fn format(&mut self, stmts: &[Stmt]) -> String {
        self.output.clear();
        self.indent_level = 0;

        for (i, stmt) in stmts.iter().enumerate() {
            self.format_stmt(stmt);

            // Add blank line between top-level declarations
            if i < stmts.len() - 1 {
                if self.needs_blank_line_after(stmt) {
                    self.output.push('\n');
                }
            }
        }

        // Ensure file ends with newline
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }

        self.output.clone()
    }

    fn needs_blank_line_after(&self, stmt: &Stmt) -> bool {
        matches!(
            &stmt.kind,
            StmtKind::Function(_, _)
                | StmtKind::Class(_, _)
                | StmtKind::Interface(_, _)
                | StmtKind::Enum(_, _)
                | StmtKind::Struct(_, _)
        )
    }

    fn indent(&self) -> String {
        if self.config.use_tabs {
            "\t".repeat(self.indent_level)
        } else {
            " ".repeat(self.indent_level * self.config.indent_size)
        }
    }

    fn format_stmt(&mut self, stmt: &Stmt) {
        self.format_stmt_kind(&stmt.kind);
    }

    fn format_stmt_kind(&mut self, kind: &StmtKind) {
        match kind {
            StmtKind::ShareDeclaration(decl, export) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.output.push_str("share ");
                self.output.push_str(&decl.name);
                if let Some(ann) = &decl.type_ann {
                    self.output.push_str(": ");
                    self.output.push_str(ann);
                }
                self.output.push_str(" = ");
                self.format_expr(&decl.expr);
                self.output.push_str(";\n");
            }
            StmtKind::StrongDeclaration(decl, export) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.output.push_str("strong ");
                self.output.push_str(&decl.name);
                if let Some(ann) = &decl.type_ann {
                    self.output.push_str(": ");
                    self.output.push_str(ann);
                }
                self.output.push_str(" = ");
                self.format_expr(&decl.expr);
                self.output.push_str(";\n");
            }
            StmtKind::WeakDeclaration(decl, export) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.output.push_str("weak ");
                self.output.push_str(&decl.name);
                if let Some(ann) = &decl.type_ann {
                    self.output.push_str(": ");
                    self.output.push_str(ann);
                }
                self.output.push_str(" = ");
                self.format_expr(&decl.expr);
                self.output.push_str(";\n");
            }
            StmtKind::Let(name, init, type_ann, export, is_const, is_readonly) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export ");
                    self.format_let_without_indent(name, init, type_ann, *is_const, *is_readonly);
                } else {
                    self.output.push_str(&indent);
                    self.format_let_without_indent(name, init, type_ann, *is_const, *is_readonly);
                }
            }
            StmtKind::ExprStmt(expr) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.format_expr(expr);
                self.output.push_str(";\n");
            }
            StmtKind::Block(stmts) => {
                self.output.push_str("{\n");
                self.indent_level += 1;
                for stmt in stmts {
                    self.format_stmt(stmt);
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("}\n");
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("if (");
                self.format_expr(cond);
                self.output.push_str(") ");
                self.format_stmt_block(then_branch);
                if let Some(else_br) = else_branch {
                    self.output.push_str(" else ");
                    self.format_stmt_block(else_br);
                }
                self.output.push('\n');
            }
            StmtKind::While { cond, body } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("while (");
                self.format_expr(cond);
                self.output.push_str(") ");
                self.format_stmt_block(body);
                self.output.push('\n');
            }
            StmtKind::ForIn { name, iter, body } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("for (");
                self.output.push_str(name);
                self.output.push_str(" in ");
                self.format_expr(iter);
                self.output.push_str(") ");
                self.format_stmt_block(body);
                self.output.push('\n');
            }
            StmtKind::Function(func, export) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export ");
                    self.format_function_without_indent(func);
                } else {
                    self.output.push_str(&indent);
                    self.format_function_without_indent(func);
                }
            }
            StmtKind::Return(expr) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("return");
                if let Some(e) = expr {
                    self.output.push(' ');
                    self.format_expr(e);
                }
                self.output.push_str(";\n");
            }
            StmtKind::Break => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("break;\n");
            }
            StmtKind::Continue => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("continue;\n");
            }
            StmtKind::Class(cd, export) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export ");
                    self.format_class_without_indent(cd);
                } else {
                    self.output.push_str(&indent);
                    self.format_class_without_indent(cd);
                }
            }
            StmtKind::TryCatch {
                try_block,
                err_name,
                catch_block,
            } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("try ");
                self.format_stmt_block(try_block);
                self.output.push_str(" catch (");
                self.output.push_str(err_name);
                self.output.push_str(") ");
                self.format_stmt_block(catch_block);
                self.output.push('\n');
            }
            StmtKind::Import { path, alias } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("import \"");
                self.output.push_str(path);
                self.output.push_str("\" as ");
                self.output.push_str(alias);
                self.output.push_str(";\n");
            }
            StmtKind::ImportDefault { path, alias } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("import \"");
                self.output.push_str(path);
                self.output.push_str("\" as ");
                self.output.push_str(alias);
                self.output.push_str(";\n");
            }
            StmtKind::ImportNames { path, names } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("from \"");
                self.output.push_str(path);
                self.output.push_str("\" import { ");
                self.output.push_str(&names.join(", "));
                self.output.push_str(" };\n");
            }
            StmtKind::Interface(iface, export) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export ");
                } else {
                    self.output.push_str(&indent);
                }
                self.output.push_str("interface ");
                self.output.push_str(&iface.name);
                self.output.push_str(" {\n");
                self.indent_level += 1;
                for method in &iface.methods {
                    let indent = self.indent();
                    self.output.push_str(&indent);
                    self.output.push_str("fn ");
                    self.output.push_str(&method.name);
                    self.output.push('(');
                    self.format_params(&method.params);
                    self.output.push_str(");\n");
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("}\n");
            }
            StmtKind::Enum(en, export) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export ");
                } else {
                    self.output.push_str(&indent);
                }
                self.output.push_str("enum ");
                self.output.push_str(&en.name);
                self.output.push_str(" {\n");
                self.indent_level += 1;
                for (name, payload) in &en.variants {
                    let indent = self.indent();
                    self.output.push_str(&indent);
                    self.output.push_str(name);
                    if let Some(p) = payload {
                        self.output.push('(');
                        self.output.push_str(p);
                        self.output.push(')');
                    }
                    self.output.push_str(",\n");
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("}\n");
            }
            StmtKind::Struct(st, export) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export ");
                } else {
                    self.output.push_str(&indent);
                }
                self.output.push_str("struct ");
                self.output.push_str(&st.name);
                self.output.push_str(" {\n");
                self.indent_level += 1;
                for (name, ty) in &st.fields {
                    let indent = self.indent();
                    self.output.push_str(&indent);
                    self.output.push_str(name);
                    self.output.push_str(": ");
                    self.output.push_str(ty);
                    self.output.push_str(";\n");
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("}\n");
            }
            StmtKind::TypeAlias(alias, export) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export type ");
                } else {
                    self.output.push_str(&indent);
                    self.output.push_str("type ");
                }
                self.output.push_str(&alias.name);
                if !alias.type_params.is_empty() {
                    self.output.push('<');
                    self.output.push_str(&alias.type_params.join(", "));
                    self.output.push('>');
                }
                self.output.push_str(" = { ");
                let field_strs: Vec<String> = alias
                    .fields
                    .iter()
                    .map(|(name, optional, typ)| {
                        if *optional {
                            format!("{}?: {}", name, typ)
                        } else {
                            format!("{}: {}", name, typ)
                        }
                    })
                    .collect();
                self.output.push_str(&field_strs.join(", "));
                self.output.push_str(" };\n");
            }
            StmtKind::Extend(name, target, methods, export) => {
                let indent = self.indent();
                if *export {
                    self.output.push_str(&indent);
                    self.output.push_str("export ");
                } else {
                    self.output.push_str(&indent);
                }
                self.output.push_str("extend ");
                if let Some(n) = name.as_ref() {
                    self.output.push_str(n);
                    self.output.push(' ');
                }
                self.output.push_str("on ");
                self.output.push_str(target);
                self.output.push_str(" {\n");
                self.indent_level += 1;
                for method in methods {
                    let temp = StmtKind::Function(method.clone(), false);
                    self.format_stmt_kind(&temp);
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("}\n");
            }
            StmtKind::Jump(expr) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("jump ");
                self.format_expr(expr);
                self.output.push_str(";\n");
            }
            StmtKind::LetTuple(names, type_anns, init, _, is_const, is_readonly) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *is_readonly {
                    self.output.push_str("readonly ");
                }
                let keyword = if *is_const { "const" } else { "let" };
                self.output.push_str(keyword);
                self.output.push_str(" (");
                self.output.push_str(&names.join(", "));
                self.output.push(')');
                if let Some(types) = type_anns {
                    self.output.push_str(": (");
                    self.output.push_str(&types.join(", "));
                    self.output.push(')');
                }
                if let Some(expr) = init {
                    self.output.push_str(" = ");
                    self.format_expr(expr);
                }
                self.output.push_str(";\n");
            }
            StmtKind::LetObject(bindings, init, _, is_const, is_readonly) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *is_readonly {
                    self.output.push_str("readonly ");
                }
                let keyword = if *is_const { "const" } else { "let" };
                self.output.push_str(keyword);
                self.output.push_str(" { ");
                let formatted_bindings: Vec<String> = bindings
                    .iter()
                    .map(|(key, alias)| {
                        if let Some(a) = alias {
                            format!("{}: {}", key, a)
                        } else {
                            key.clone()
                        }
                    })
                    .collect();
                self.output.push_str(&formatted_bindings.join(", "));
                self.output.push_str(" }");
                if let Some(expr) = init {
                    self.output.push_str(" = ");
                    self.format_expr(expr);
                }
                self.output.push_str(";\n");
            }
            StmtKind::ExportDefaultFunction(func) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("export default ");
                self.format_function_without_indent(func);
            }
            StmtKind::ExportDefaultClass(cd) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("export default ");
                self.format_class_without_indent(cd);
            }
            StmtKind::ExportDefault(name) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("export default ");
                self.output.push_str(name);
                self.output.push_str(";\n");
            }
            // FFI statements
            StmtKind::HeaderImport { path } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str(&format!("$cImport(\"{}\");\n", path));
            }
            StmtKind::ExternFunction(decl) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output
                    .push_str(&format!("extern \"{}\" fn {}(", decl.abi, decl.name));
                for (i, (pname, ptype)) in decl.params.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(&format!("{}: {}", pname, ptype));
                }
                self.output.push_str(&format!(") -> {};\n", decl.ret_type));
            }
            StmtKind::ExternBlock { abi, functions } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str(&format!("extern \"{}\" {{\n", abi));
                self.indent_level += 1;
                for decl in functions {
                    let inner_indent = self.indent();
                    self.output.push_str(&inner_indent);
                    self.output.push_str(&format!("fn {}(", decl.name));
                    for (i, (pname, ptype)) in decl.params.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.output.push_str(&format!("{}: {}", pname, ptype));
                    }
                    self.output.push_str(&format!(") -> {};\n", decl.ret_type));
                }
                self.indent_level -= 1;
                self.output.push_str(&indent);
                self.output.push_str("}\n");
            }
            StmtKind::Region { name, body } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("region");
                if let Some(n) = name.as_ref() {
                    self.output.push(' ');
                    self.output.push_str(n);
                }
                self.output.push(' ');
                self.format_stmt(body);
            }
            StmtKind::UnsafeBlock(body) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("unsafe ");
                self.format_stmt(body);
            }
            StmtKind::Defer(body) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("defer ");
                self.format_stmt(body);
            }
            StmtKind::Decorator(def, _is_export) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("decorator ");
                self.output.push_str(&def.name);
                self.output.push_str("(...) { /* phases */ }\n");
            }
        }
    }

    fn format_stmt_block(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Block(stmts) => {
                self.output.push_str("{\n");
                self.indent_level += 1;
                for s in stmts {
                    self.format_stmt(s);
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push('}');
            }
            _ => {
                self.output.push_str("{\n");
                self.indent_level += 1;
                self.format_stmt(stmt);
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push('}');
            }
        }
    }

    fn format_let_without_indent(
        &mut self,
        name: &str,
        init: &Option<Expr>,
        type_ann: &Option<String>,
        is_const: bool,
        is_readonly: bool,
    ) {
        if is_readonly {
            self.output.push_str("readonly ");
        }
        if is_const {
            self.output.push_str("const ");
        } else {
            self.output.push_str("let ");
        }
        self.output.push_str(name);
        if let Some(ann) = type_ann {
            self.output.push_str(": ");
            self.output.push_str(ann);
        }
        if let Some(expr) = init {
            self.output.push_str(" = ");
            self.format_expr(expr);
        }
        self.output.push_str(";\n");
    }

    fn format_function_without_indent(&mut self, func: &Function) {
        if func.is_async {
            self.output.push_str("async ");
        }
        self.output.push_str("fn ");
        self.output.push_str(&func.name);
        if self.config.space_before_function_paren {
            self.output.push(' ');
        }
        self.output.push('(');
        self.format_params(&func.params);
        self.output.push(')');
        if let Some(ret) = &func.ret_type {
            self.output.push_str(": ");
            self.output.push_str(ret);
        }
        self.output.push_str(" {\n");
        self.indent_level += 1;
        for stmt in func.body.iter() {
            self.format_stmt(stmt);
        }
        self.indent_level -= 1;
        let indent = self.indent();
        self.output.push_str(&indent);
        self.output.push_str("}\n");
    }

    fn format_class_without_indent(&mut self, cd: &ClassDecl) {
        if cd.is_abstract {
            self.output.push_str("abstract ");
        }
        self.output.push_str("class ");
        self.output.push_str(&cd.name);
        if let Some(parent) = &cd.extends {
            self.output.push_str(" extends ");
            self.output.push_str(parent);
        }
        if !cd.implements.is_empty() {
            self.output.push_str(" implements ");
            self.output.push_str(&cd.implements.join(", "));
        }
        self.output.push_str(" {\n");
        self.indent_level += 1;
        for method in &cd.methods {
            let temp = StmtKind::Function(method.clone(), false);
            self.format_stmt_kind(&temp);
        }
        self.indent_level -= 1;
        let indent = self.indent();
        self.output.push_str(&indent);
        self.output.push_str("}\n");
    }

    fn format_params(&mut self, params: &[(String, Option<Expr>, Option<String>)]) {
        for (i, (name, default, type_ann)) in params.iter().enumerate() {
            if i > 0 {
                self.output.push_str(", ");
            }
            self.output.push_str(name);
            if let Some(ann) = type_ann {
                self.output.push_str(": ");
                self.output.push_str(ann);
            }
            if let Some(def) = default {
                self.output.push_str(" = ");
                self.format_expr(def);
            }
        }
    }

    fn format_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Literal(v) => self.format_value(v),
            ExprKind::Variable(name) => self.output.push_str(name),
            ExprKind::Assign(name, val) => {
                self.output.push_str(name);
                self.output.push_str(" = ");
                self.format_expr(val);
            }
            ExprKind::AssignOp(left, op, right) => {
                self.format_expr(left);
                self.output.push(' ');
                self.format_op(op);
                self.output.push_str("= ");
                self.format_expr(right);
            }
            ExprKind::AssignTuple(names, rhs) => {
                self.output.push('[');
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(name);
                }
                self.output.push_str("] = ");
                self.format_expr(rhs);
            }
            ExprKind::AssignObject(bindings, rhs) => {
                self.output.push_str("{ ");
                for (i, (key, alias)) in bindings.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    if let Some(a) = alias {
                        self.output.push_str(key);
                        self.output.push_str(": ");
                        self.output.push_str(a);
                    } else {
                        self.output.push_str(key);
                    }
                }
                self.output.push_str(" } = ");
                self.format_expr(rhs);
            }
            ExprKind::Unary(op, right) => {
                self.format_op(op);
                self.format_expr(right);
            }
            ExprKind::Binary(left, op, right) => {
                self.format_expr(left);
                self.output.push(' ');
                self.format_op(op);
                self.output.push(' ');
                self.format_expr(right);
            }
            ExprKind::Logical(left, op, right) => {
                self.format_expr(left);
                self.output.push(' ');
                self.format_op(op);
                self.output.push(' ');
                self.format_expr(right);
            }
            ExprKind::Grouping(inner) => {
                self.output.push('(');
                self.format_expr(inner);
                self.output.push(')');
            }
            ExprKind::Call(callee, args, type_args) => {
                self.format_expr(callee);
                if !type_args.is_empty() {
                    self.output.push('<');
                    self.output.push_str(&type_args.join(", "));
                    self.output.push('>');
                }
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.output.push(')');
            }
            ExprKind::Array(items) => {
                self.output.push('[');
                if self.config.space_in_brackets && !items.is_empty() {
                    self.output.push(' ');
                }
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(item);
                }
                if self.config.trailing_commas && !items.is_empty() {
                    self.output.push(',');
                }
                if self.config.space_in_brackets && !items.is_empty() {
                    self.output.push(' ');
                }
                self.output.push(']');
            }
            ExprKind::Tuple(items) => {
                self.output.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(item);
                }
                if items.len() == 1 {
                    self.output.push(',');
                }
                self.output.push(')');
            }
            ExprKind::Object(fields) => {
                if fields.is_empty() {
                    self.output.push_str("{}");
                } else {
                    self.output.push('{');
                    if self.config.space_in_braces {
                        self.output.push(' ');
                    }
                    for (i, (key, val)) in fields.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.output.push_str(key);
                        self.output.push_str(": ");
                        self.format_expr(val);
                    }
                    if self.config.trailing_commas && !fields.is_empty() {
                        self.output.push(',');
                    }
                    if self.config.space_in_braces {
                        self.output.push(' ');
                    }
                    self.output.push('}');
                }
            }
            ExprKind::StructLiteral(name, fields) => {
                self.output.push_str(name);
                self.output.push('{');
                if self.config.space_in_braces {
                    self.output.push(' ');
                }
                for (i, (key, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(key);
                    self.output.push_str(": ");
                    self.format_expr(val);
                }
                if self.config.trailing_commas && !fields.is_empty() {
                    self.output.push(',');
                }
                if self.config.space_in_braces {
                    self.output.push(' ');
                }
                self.output.push('}');
            }
            ExprKind::SetLiteral(items) => {
                self.output.push('{');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(item);
                }
                self.output.push('}');
            }
            ExprKind::Get(obj, prop) => {
                self.format_expr(obj);
                self.output.push('.');
                self.output.push_str(prop);
            }
            ExprKind::Set(obj, prop, val) => {
                self.format_expr(obj);
                self.output.push('.');
                self.output.push_str(prop);
                self.output.push_str(" = ");
                self.format_expr(val);
            }
            ExprKind::Fn(params, body, is_async) => {
                if *is_async {
                    self.output.push_str("async ");
                }
                self.output.push_str("fn(");
                self.format_params(params);
                self.output.push_str(") {\n");
                self.indent_level += 1;
                // Dereference Arc to iterate
                for stmt in body.as_ref() {
                    self.format_stmt(stmt);
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push('}');
            }
            ExprKind::New(ctor, args) => {
                self.output.push_str("new ");
                self.format_expr(ctor);
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.output.push(')');
            }
            ExprKind::Await(inner) => {
                self.output.push_str("await ");
                self.format_expr(inner);
            }
            ExprKind::Spawn(inner) => {
                self.output.push_str("spawn ");
                self.format_expr(inner);
            }
            ExprKind::Throw(inner) => {
                self.output.push_str("throw ");
                self.format_expr(inner);
            }
            ExprKind::Spread(inner) => {
                self.output.push_str("...");
                self.format_expr(inner);
            }
            ExprKind::Range(start, end, inclusive) => {
                self.format_expr(start);
                if *inclusive {
                    self.output.push_str("..=");
                } else {
                    self.output.push_str("..");
                }
                self.format_expr(end);
            }
            ExprKind::Index(obj, idx) => {
                self.format_expr(obj);
                self.output.push('[');
                self.format_expr(idx);
                self.output.push(']');
            }
            ExprKind::NonNull(inner) => {
                self.format_expr(inner);
                self.output.push('!');
            }
            ExprKind::OptGet(obj, prop) => {
                self.format_expr(obj);
                self.output.push_str("?.");
                self.output.push_str(prop);
            }
            ExprKind::OptCall(callee, args, type_args) => {
                self.format_expr(callee);
                self.output.push_str("?.");
                if !type_args.is_empty() {
                    self.output.push('<');
                    self.output.push_str(&type_args.join(", "));
                    self.output.push('>');
                }
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.output.push(')');
            }
            ExprKind::Try(inner) => {
                self.output.push_str("try ");
                self.format_expr(inner);
            }
            ExprKind::Conditional(cond, then_expr, else_expr) => {
                self.format_expr(cond);
                self.output.push_str(" ? ");
                self.format_expr(then_expr);
                self.output.push_str(" : ");
                self.format_expr(else_expr);
            }
            ExprKind::Update(pre, inc, target) => {
                if *pre {
                    self.output.push_str(if *inc { "++" } else { "--" });
                    self.format_expr(target);
                } else {
                    self.format_expr(target);
                    self.output.push_str(if *inc { "++" } else { "--" });
                }
            }
            ExprKind::Match(scrutinee, arms) => {
                self.output.push_str("match ");
                self.format_expr(scrutinee);
                self.output.push_str(" {\n");
                self.indent_level += 1;
                for (pattern, body) in arms {
                    let indent = self.indent();
                    self.output.push_str(&indent);
                    self.format_pattern(pattern);
                    self.output.push_str(" => ");
                    self.format_expr(body);
                    self.output.push_str(",\n");
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push('}');
            }
            ExprKind::Format(inner, spec) => {
                self.output.push_str("${");
                self.format_expr(inner);
                self.output.push(':');
                self.output.push_str(spec);
                self.output.push('}');
            }
            ExprKind::Cast(inner, target_ty) => {
                self.format_expr(inner);
                self.output.push_str(" as ");
                self.output.push_str(target_ty);
            }
        }
    }

    fn format_value(&mut self, v: &Value) {
        match v {
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    self.output.push_str(&format!("{}", *n as i64));
                } else {
                    self.output.push_str(&format!("{}", n));
                }
            }
            Value::BigInt(bi) => self.output.push_str(&format!("{}", bi)),
            Value::Bool(b) => self.output.push_str(if *b { "true" } else { "false" }),
            Value::Char(c) => {
                self.output.push('\'');
                self.output.push(*c);
                self.output.push('\'');
            }
            Value::Str(s) => {
                self.output.push('"');
                self.output.push_str(&escape_string(s));
                self.output.push('"');
            }
            Value::Null => self.output.push_str("null"),
            _ => self.output.push_str("/* complex value */"),
        }
    }

    fn format_op(&mut self, op: &TokenKind) {
        let s = match op {
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::EqualEqual => "==",
            TokenKind::BangEqual => "!=",
            TokenKind::Less => "<",
            TokenKind::LessEqual => "<=",
            TokenKind::Greater => ">",
            TokenKind::GreaterEqual => ">=",
            TokenKind::And => "&&",
            TokenKind::Or => "||",
            TokenKind::Bang => "!",
            TokenKind::Ampersand => "&",
            TokenKind::Pipe => "|",
            TokenKind::Caret => "^",
            TokenKind::Tilde => "~",
            TokenKind::ShiftLeft => "<<",
            TokenKind::ShiftRight => ">>",
            _ => "?",
        };
        self.output.push_str(s);
    }

    fn format_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Literal(v) => self.format_value(v),
            Pattern::Variable(name) => self.output.push_str(name),
            Pattern::Wildcard => self.output.push('_'),
            Pattern::Or(left, right) => {
                self.format_pattern(left);
                self.output.push_str(" | ");
                self.format_pattern(right);
            }
            Pattern::EnumVariant(name, args) => {
                self.output.push_str(name);
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_pattern(arg);
                }
                self.output.push(')');
            }
        }
    }
}

/// Escape special characters in a string
fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result
}

/// Format Adesh source code
pub fn format_source(source: &str, config: Option<FormatConfig>) -> Result<String, String> {
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    let mut lexer = Lexer::new(source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Lexer error: {}", e))?;
    let mut parser = Parser::new(tokens, None);
    let stmts = parser
        .parse_program()
        .map_err(|e| format!("Parser error: {}", e))?;

    let mut formatter = Formatter::new(config.unwrap_or_default());
    Ok(formatter.format(&stmts))
}

/// Format a Adesh file in place
pub fn format_file(path: &std::path::Path, config: Option<FormatConfig>) -> Result<(), String> {
    let source =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let formatted = format_source(&source, config)?;
    std::fs::write(path, formatted).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_simple_let() {
        let source = "let x=1;";
        let formatted = format_source(source, None).unwrap();
        assert!(formatted.contains("let x = 1;"));
    }

    #[test]
    fn test_format_function() {
        let source = "fn add(a,b){return a+b;}";
        let formatted = format_source(source, None).unwrap();
        assert!(formatted.contains("fn add(a, b)"));
        assert!(formatted.contains("return a + b;"));
    }

    #[test]
    fn test_format_class() {
        let source = "class Point{fn Point(x,y){this.x=x;this.y=y;}}";
        let formatted = format_source(source, None).unwrap();
        assert!(formatted.contains("class Point"));
        assert!(formatted.contains("__ctor__") || formatted.contains("Point("));
    }
}
