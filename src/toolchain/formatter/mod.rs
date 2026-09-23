//! Adesh Code Formatter
//!
//! Provides functionality to format Adesh source code with consistent styling,
//! trivia & comment preservation, operator formatting, and full AST support.

use crate::parsing::ast::{
    ClassDecl, Expr, ExprKind, Function, Pattern, Stmt, StmtKind, TokenKind, Value, Visibility,
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

/// Represents a comment extracted from the source code
#[derive(Debug, Clone)]
pub struct SourceComment {
    pub text: String,
    pub line: usize,
    pub col: usize,
    pub is_trailing: bool,
    pub is_block: bool,
}

/// Scanner that extracts comments and their location from Adesh source code
pub struct CommentScanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
    line_has_code: bool,
}

impl<'a> CommentScanner<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            line_has_code: false,
        }
    }

    pub fn scan_comments(mut self) -> Vec<SourceComment> {
        let mut comments = Vec::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'\n' {
                self.pos += 1;
                self.line += 1;
                self.col = 1;
                self.line_has_code = false;
                continue;
            }
            if b == b'\r' {
                self.pos += 1;
                continue;
            }
            if b == b' ' || b == b'\t' {
                self.pos += 1;
                self.col += 1;
                continue;
            }

            // String & character literals - skip so comment characters inside strings are ignored
            if b == b'"' || b == b'\'' || b == b'`' {
                self.line_has_code = true;
                let quote = b;
                self.pos += 1;
                self.col += 1;
                while self.pos < self.bytes.len() {
                    let ch = self.bytes[self.pos];
                    if ch == b'\\' {
                        self.pos += 2;
                        self.col += 2;
                        continue;
                    }
                    if ch == quote {
                        self.pos += 1;
                        self.col += 1;
                        break;
                    }
                    if ch == b'\n' {
                        self.line += 1;
                        self.col = 1;
                    } else {
                        self.col += 1;
                    }
                    self.pos += 1;
                }
                continue;
            }

            // Comments
            if b == b'/' && self.pos + 1 < self.bytes.len() {
                let next = self.bytes[self.pos + 1];
                if next == b'/' {
                    // Single line comment
                    let start_line = self.line;
                    let start_col = self.col;
                    let is_trailing = self.line_has_code;
                    let start_pos = self.pos;
                    while self.pos < self.bytes.len()
                        && self.bytes[self.pos] != b'\n'
                        && self.bytes[self.pos] != b'\r'
                    {
                        self.pos += 1;
                        self.col += 1;
                    }
                    let text = self.src[start_pos..self.pos].to_string();
                    comments.push(SourceComment {
                        text,
                        line: start_line,
                        col: start_col,
                        is_trailing,
                        is_block: false,
                    });
                    continue;
                } else if next == b'*' {
                    // Block comment
                    let start_line = self.line;
                    let start_col = self.col;
                    let is_trailing = self.line_has_code;
                    let start_pos = self.pos;
                    self.pos += 2;
                    self.col += 2;
                    while self.pos + 1 < self.bytes.len() {
                        if self.bytes[self.pos] == b'\n' {
                            self.line += 1;
                            self.col = 1;
                        } else {
                            self.col += 1;
                        }
                        if self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/' {
                            self.pos += 2;
                            self.col += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                    let text = self.src[start_pos..self.pos.min(self.bytes.len())].to_string();
                    comments.push(SourceComment {
                        text,
                        line: start_line,
                        col: start_col,
                        is_trailing,
                        is_block: true,
                    });
                    continue;
                }
            }

            self.line_has_code = true;
            self.pos += 1;
            self.col += 1;
        }
        comments
    }
}

/// Adesh code formatter
pub struct Formatter {
    config: FormatConfig,
    output: String,
    indent_level: usize,
    comments: Vec<SourceComment>,
    comment_idx: usize,
    last_emitted_line: usize,
}

impl Formatter {
    pub fn new(config: FormatConfig) -> Self {
        Self {
            config,
            output: String::new(),
            indent_level: 0,
            comments: Vec::new(),
            comment_idx: 0,
            last_emitted_line: 0,
        }
    }

    pub fn with_comments(mut self, comments: Vec<SourceComment>) -> Self {
        self.comments = comments;
        self
    }

    /// Format an Adesh program from parsed statements
    pub fn format(&mut self, stmts: &[Stmt]) -> String {
        self.output.clear();
        self.indent_level = 0;
        self.last_emitted_line = 0;

        for (i, stmt) in stmts.iter().enumerate() {
            self.flush_comments_before(stmt.span.line);

            // Blank line before statement if there was one in source or between top-level decls
            if i > 0 && !self.output.ends_with("\n\n") && !self.output.is_empty() {
                if stmt.span.line > self.last_emitted_line + 1
                    || self.needs_blank_line_after(&stmts[i - 1])
                    || self.needs_blank_line_before(stmt)
                {
                    self.output.push('\n');
                }
            }

            self.format_stmt(stmt);

            // Check for trailing comment on the statement line
            self.emit_trailing_comment_for_line(stmt.span.line);
            self.last_emitted_line = stmt.span.line.max(self.last_emitted_line);
        }

        self.flush_all_remaining_comments();

        // Ensure file ends with a single newline
        while self.output.ends_with("\n\n") {
            self.output.pop();
        }
        if !self.output.ends_with('\n') && !self.output.is_empty() {
            self.output.push('\n');
        }

        self.output.clone()
    }

    fn flush_comments_before(&mut self, target_line: usize) {
        if target_line == 0 {
            return;
        }
        while self.comment_idx < self.comments.len() {
            let c = &self.comments[self.comment_idx];
            if c.line < target_line || (c.line == target_line && !c.is_trailing) {
                if self.last_emitted_line > 0
                    && c.line > self.last_emitted_line + 1
                    && !self.output.is_empty()
                    && !self.output.ends_with("\n\n")
                    && !self.output.ends_with("{\n")
                {
                    self.output.push('\n');
                }

                let indent = self.indent();
                if c.is_block {
                    let lines: Vec<&str> = c.text.lines().collect();
                    for (i, line) in lines.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(&indent);
                            self.output.push_str(line.trim_start());
                        } else {
                            self.output.push_str(&indent);
                            self.output.push_str(line);
                        }
                        self.output.push('\n');
                    }
                } else {
                    self.output.push_str(&indent);
                    self.output.push_str(&c.text);
                    self.output.push('\n');
                }

                self.last_emitted_line = c.line;
                self.comment_idx += 1;
            } else {
                break;
            }
        }
    }

    fn emit_trailing_comment_for_line(&mut self, line: usize) {
        if line == 0 {
            return;
        }
        if self.comment_idx < self.comments.len() {
            let c = &self.comments[self.comment_idx];
            if c.line == line && c.is_trailing {
                if self.output.ends_with('\n') {
                    self.output.pop();
                    self.output.push(' ');
                    self.output.push_str(&c.text);
                    self.output.push('\n');
                } else {
                    self.output.push(' ');
                    self.output.push_str(&c.text);
                }
                self.last_emitted_line = c.line;
                self.comment_idx += 1;
            }
        }
    }

    fn flush_all_remaining_comments(&mut self) {
        while self.comment_idx < self.comments.len() {
            let c = &self.comments[self.comment_idx];
            if self.last_emitted_line > 0
                && c.line > self.last_emitted_line + 1
                && !self.output.is_empty()
                && !self.output.ends_with("\n\n")
                && !self.output.ends_with("{\n")
            {
                self.output.push('\n');
            }

            let indent = self.indent();
            if c.is_block {
                let lines: Vec<&str> = c.text.lines().collect();
                for (i, line) in lines.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(&indent);
                        self.output.push_str(line.trim_start());
                    } else {
                        self.output.push_str(&indent);
                        self.output.push_str(line);
                    }
                    self.output.push('\n');
                }
            } else {
                self.output.push_str(&indent);
                self.output.push_str(&c.text);
                self.output.push('\n');
            }

            self.last_emitted_line = c.line;
            self.comment_idx += 1;
        }
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

    fn needs_blank_line_before(&self, stmt: &Stmt) -> bool {
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
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.format_let_without_indent(name, init, type_ann, *is_const, *is_readonly);
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
                    self.flush_comments_before(stmt.span.line);
                    self.format_stmt(stmt);
                    self.emit_trailing_comment_for_line(stmt.span.line);
                    self.last_emitted_line = stmt.span.line.max(self.last_emitted_line);
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
                self.format_if_chain(cond, then_branch, else_branch);
                self.output.push('\n');
            }
            StmtKind::While { cond, body } => {
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("while (");
                self.format_expr(Self::unwrap_grouping(cond));
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
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.format_function_without_indent(func);
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
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.format_class_without_indent(cd);
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
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.output.push_str("interface ");
                self.output.push_str(&iface.name);
                if !iface.type_params.is_empty() {
                    self.output.push('<');
                    self.output.push_str(&iface.type_params.join(", "));
                    self.output.push('>');
                }
                if !iface.super_interfaces.is_empty() {
                    self.output.push_str(" extends ");
                    self.output.push_str(&iface.super_interfaces.join(", "));
                }
                self.output.push_str(" {\n");
                self.indent_level += 1;
                for method in &iface.methods {
                    let indent = self.indent();
                    self.output.push_str(&indent);
                    self.output.push_str("fn ");
                    self.output.push_str(&method.name);
                    self.output.push('(');
                    self.format_params(&method.params);
                    self.output.push(')');
                    if let Some(ret) = &method.ret_type {
                        self.output.push_str(": ");
                        self.output.push_str(ret);
                    }
                    self.output.push_str(";\n");
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push_str("}\n");
            }
            StmtKind::Enum(en, export) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
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
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.output.push_str("struct ");
                self.output.push_str(&st.name);
                if !st.type_params.is_empty() {
                    self.output.push('<');
                    self.output.push_str(&st.type_params.join(", "));
                    self.output.push('>');
                }
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
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
                self.output.push_str("type ");
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
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
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
            StmtKind::LetTuple(names, type_anns, init, export, is_const, is_readonly) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
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
            StmtKind::LetObject(bindings, init, export, is_const, is_readonly) => {
                let indent = self.indent();
                self.output.push_str(&indent);
                if *export {
                    self.output.push_str("export ");
                }
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

    fn unwrap_grouping<'b>(expr: &'b Expr) -> &'b Expr {
        if let ExprKind::Grouping(inner) = &expr.kind {
            inner
        } else {
            expr
        }
    }

    fn format_if_chain(
        &mut self,
        cond: &Expr,
        then_branch: &Stmt,
        else_branch: &Option<Box<Stmt>>,
    ) {
        self.output.push_str("if (");
        self.format_expr(Self::unwrap_grouping(cond));
        self.output.push_str(") ");
        self.format_stmt_block(then_branch);
        if let Some(else_br) = else_branch {
            match &else_br.kind {
                StmtKind::If {
                    cond: next_cond,
                    then_branch: next_then,
                    else_branch: next_else,
                } => {
                    self.output.push_str(" else ");
                    self.format_if_chain(next_cond, next_then, next_else);
                }
                _ => {
                    self.output.push_str(" else ");
                    self.format_stmt_block(else_br);
                }
            }
        }
    }

    fn format_stmt_block(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Block(stmts) => {
                self.output.push_str("{\n");
                self.indent_level += 1;
                for s in stmts {
                    self.flush_comments_before(s.span.line);
                    self.format_stmt(s);
                    self.emit_trailing_comment_for_line(s.span.line);
                    self.last_emitted_line = s.span.line.max(self.last_emitted_line);
                }
                self.indent_level -= 1;
                let indent = self.indent();
                self.output.push_str(&indent);
                self.output.push('}');
            }
            _ => {
                self.output.push_str("{\n");
                self.indent_level += 1;
                self.flush_comments_before(stmt.span.line);
                self.format_stmt(stmt);
                self.emit_trailing_comment_for_line(stmt.span.line);
                self.last_emitted_line = stmt.span.line.max(self.last_emitted_line);
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
        for dec in &func.decorators {
            self.output.push('@');
            self.format_expr(dec);
            self.output.push(' ');
        }
        if let Some(vis) = &func.visibility {
            match vis {
                Visibility::Pub => self.output.push_str("pub "),
                Visibility::Priv => self.output.push_str("private "),
                Visibility::Protected => self.output.push_str("protected "),
                _ => {}
            }
        }
        if func.is_async {
            self.output.push_str("async ");
        }
        if func.is_abstract {
            self.output.push_str("abstract ");
        }
        self.output.push_str("fn ");
        self.output.push_str(&func.name);
        if !func.type_params.is_empty() {
            self.output.push('<');
            self.output.push_str(&func.type_params.join(", "));
            self.output.push('>');
        }
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
        if func.is_abstract || (func.body.is_empty() && func.name.is_empty()) {
            self.output.push_str(";\n");
        } else {
            self.output.push_str(" {\n");
            self.indent_level += 1;
            for stmt in func.body.iter() {
                self.flush_comments_before(stmt.span.line);
                self.format_stmt(stmt);
                self.emit_trailing_comment_for_line(stmt.span.line);
                self.last_emitted_line = stmt.span.line.max(self.last_emitted_line);
            }
            self.indent_level -= 1;
            let indent = self.indent();
            self.output.push_str(&indent);
            self.output.push_str("}\n");
        }
    }

    fn format_class_without_indent(&mut self, cd: &ClassDecl) {
        for dec in &cd.decorators {
            self.output.push('@');
            self.format_expr(dec);
            self.output.push('\n');
            let indent = self.indent();
            self.output.push_str(&indent);
        }

        if cd.is_abstract {
            self.output.push_str("abstract ");
        }
        if cd.is_sealed {
            self.output.push_str("sealed ");
        }
        self.output.push_str("class ");
        self.output.push_str(&cd.name);
        if !cd.type_params.is_empty() {
            self.output.push('<');
            self.output.push_str(&cd.type_params.join(", "));
            self.output.push('>');
        }
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

        // Static properties
        for (name, expr, decorators) in &cd.static_properties {
            let indent = self.indent();
            self.output.push_str(&indent);
            for dec in decorators {
                self.output.push('@');
                self.format_expr(dec);
                self.output.push(' ');
            }
            self.output.push_str("static ");
            self.output.push_str(name);
            self.output.push_str(" = ");
            self.format_expr(expr);
            self.output.push_str(";\n");
        }

        // Instance fields
        for (name, ty, vis, decorators, init) in &cd.fields {
            let indent = self.indent();
            self.output.push_str(&indent);
            for dec in decorators {
                self.output.push('@');
                self.format_expr(dec);
                self.output.push(' ');
            }
            match vis {
                Visibility::Priv => self.output.push_str("private "),
                Visibility::Protected => self.output.push_str("protected "),
                Visibility::Pub => {}
            }
            self.output.push_str(name);
            if ty != "any" && !ty.is_empty() {
                self.output.push_str(": ");
                self.output.push_str(ty);
            }
            if let Some(init_expr) = init {
                self.output.push_str(" = ");
                self.format_expr(init_expr);
            }
            self.output.push_str(";\n");
        }

        // Blank line before methods if fields exist
        if !cd.fields.is_empty() && (!cd.methods.is_empty() || !cd.static_methods.is_empty()) {
            self.output.push('\n');
        }

        // Static methods
        for method in &cd.static_methods {
            let indent = self.indent();
            self.output.push_str(&indent);
            self.output.push_str("static ");
            self.format_function_without_indent(method);
        }

        // Instance methods and constructor
        for (i, method) in cd.methods.iter().enumerate() {
            if i > 0 && !self.output.ends_with("\n\n") {
                self.output.push('\n');
            }
            // Check for constructor
            if method.name == "__ctor__" {
                let mut ctor = method.clone();
                ctor.name = cd.name.clone();
                let temp = StmtKind::Function(ctor, false);
                let first_line = method.body.first().map(|s| s.span.line).unwrap_or(0);
                if first_line > 0 {
                    self.flush_comments_before(first_line);
                }
                self.format_stmt_kind(&temp);
            } else {
                let first_line = method.body.first().map(|s| s.span.line).unwrap_or(0);
                if first_line > 0 {
                    self.flush_comments_before(first_line);
                }
                let temp = StmtKind::Function(method.clone(), false);
                self.format_stmt_kind(&temp);
            }
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
                let op_str = match op {
                    TokenKind::Equal => " = ",
                    TokenKind::PlusEqual => " += ",
                    TokenKind::MinusEqual => " -= ",
                    TokenKind::StarEqual => " *= ",
                    TokenKind::SlashEqual => " /= ",
                    TokenKind::PercentEqual => " %= ",
                    TokenKind::StarStarEqual => " **= ",
                    TokenKind::ShiftLeftEqual => " <<= ",
                    TokenKind::ShiftRightEqual => " >>= ",
                    TokenKind::AmpersandEqual => " &= ",
                    TokenKind::PipeEqual => " |= ",
                    TokenKind::CaretEqual => " ^= ",
                    TokenKind::NullCoalesceEqual => " ??= ",
                    _ => {
                        self.output.push(' ');
                        self.format_op(op);
                        self.output.push_str("= ");
                        ""
                    }
                };
                if !op_str.is_empty() {
                    self.output.push_str(op_str);
                }
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
                for stmt in body.as_ref() {
                    self.flush_comments_before(stmt.span.line);
                    self.format_stmt(stmt);
                    self.emit_trailing_comment_for_line(stmt.span.line);
                    self.last_emitted_line = stmt.span.line.max(self.last_emitted_line);
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
            Value::U8(n) => self.output.push_str(&format!("{}u8", n)),
            Value::U16(n) => self.output.push_str(&format!("{}u16", n)),
            Value::U32(n) => self.output.push_str(&format!("{}u32", n)),
            Value::U64(n) => self.output.push_str(&format!("{}u64", n)),
            Value::U128(n) => self.output.push_str(&format!("{}u128", n)),
            Value::I8(n) => self.output.push_str(&format!("{}i8", n)),
            Value::I16(n) => self.output.push_str(&format!("{}i16", n)),
            Value::I32(n) => self.output.push_str(&format!("{}i32", n)),
            Value::I64(n) => self.output.push_str(&format!("{}i64", n)),
            Value::I128(n) => self.output.push_str(&format!("{}i128", n)),
            Value::F32(f) => self.output.push_str(&format!("{}f32", f)),
            Value::F64(f) => self.output.push_str(&format!("{}f64", f)),
            Value::Complex(r, i) => self.output.push_str(&format!("{}+{}j", r, i)),
            Value::BigInt(bi) => self.output.push_str(&format!("{}n", bi)),
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
            TokenKind::StarStar => "**",
            TokenKind::Equal => "=",
            TokenKind::EqualEqual => "==",
            TokenKind::StrictEqual => "===",
            TokenKind::BangEqual => "!=",
            TokenKind::StrictNotEqual => "!==",
            TokenKind::Less => "<",
            TokenKind::LessEqual => "<=",
            TokenKind::Greater => ">",
            TokenKind::GreaterEqual => ">=",
            TokenKind::AndAnd | TokenKind::And => "&&",
            TokenKind::OrOr | TokenKind::Or => "||",
            TokenKind::Bang => "!",
            TokenKind::Ampersand => "&",
            TokenKind::Pipe => "|",
            TokenKind::Caret => "^",
            TokenKind::Tilde => "~",
            TokenKind::TildeSlash => "~/",
            TokenKind::ShiftLeft => "<<",
            TokenKind::ShiftRight => ">>",
            TokenKind::NullCoalesce => "??",
            TokenKind::DotDot => "..",
            TokenKind::DotDotDot => "...",
            TokenKind::PlusEqual => "+=",
            TokenKind::MinusEqual => "-=",
            TokenKind::StarEqual => "*=",
            TokenKind::SlashEqual => "/=",
            TokenKind::PercentEqual => "%=",
            TokenKind::StarStarEqual => "**=",
            TokenKind::ShiftLeftEqual => "<<=",
            TokenKind::ShiftRightEqual => ">>=",
            TokenKind::AmpersandEqual => "&=",
            TokenKind::PipeEqual => "|=",
            TokenKind::CaretEqual => "^=",
            TokenKind::NullCoalesceEqual => "??=",
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

    let scanner = CommentScanner::new(source);
    let comments = scanner.scan_comments();

    let mut lexer = Lexer::new(source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Lexer error: {}", e))?;
    let mut parser = Parser::new(tokens, None);
    let stmts = parser
        .parse_program()
        .map_err(|e| format!("Parser error: {}", e))?;

    let mut formatter = Formatter::new(config.unwrap_or_default()).with_comments(comments);
    Ok(formatter.format(&stmts))
}

/// Format an Adesh file in place
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
    fn test_format_compound_assignments() {
        let source = "let x = 1;\nx += 2;\nx -= 3;\nx *= 4;\nx /= 5;\nx %= 6;";
        let formatted = format_source(source, None).unwrap();
        assert!(formatted.contains("x += 2;"));
        assert!(formatted.contains("x -= 3;"));
        assert!(formatted.contains("x *= 4;"));
        assert!(formatted.contains("x /= 5;"));
        assert!(formatted.contains("x %= 6;"));
        assert!(!formatted.contains("?="));
    }

    #[test]
    fn test_format_comments_preservation() {
        let source = "// Top comment\nlet x = 1; // Trailing comment\n/* Block comment */\nfn add(a, b) {\n    // Inside function\n    return a + b;\n}";
        let formatted = format_source(source, None).unwrap();
        assert!(formatted.contains("// Top comment"));
        assert!(formatted.contains("// Trailing comment"));
        assert!(formatted.contains("/* Block comment */"));
        assert!(formatted.contains("// Inside function"));
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
        let source = "class Point{x: int; y: int; fn Point(x,y){this.x=x;this.y=y;}}";
        let formatted = format_source(source, None).unwrap();
        assert!(formatted.contains("class Point"));
        assert!(formatted.contains("x: int;"));
        assert!(formatted.contains("y: int;"));
        assert!(formatted.contains("fn Point(x, y)"));
        assert!(formatted.contains("this.x = x;"));
        assert!(formatted.contains("this.y = y;"));
        assert!(!formatted.contains("?="));
        assert!(!formatted.contains("__ctor__"));
    }

    #[test]
    fn test_format_if_else_if_else() {
        let source = "if (x > 0) { return 1; } else if (x < 0) { return -1; } else { return 0; }";
        let formatted = format_source(source, None).unwrap();
        assert!(formatted.contains("if (x > 0) {"));
        assert!(formatted.contains("} else if (x < 0) {"));
        assert!(formatted.contains("} else {"));
    }
}
