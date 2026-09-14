//! Code analysis module
//!
//! Provides semantic analysis using Adesh's parsing infrastructure.
//! Updated for AdeshLang CFG v2.1/v2.2 with ownership and borrowing support.

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use adeshlang::parsing::ast::{ClassDecl, Expr, ExprKind, Function, Stmt, StmtKind, TokenKind};
use adeshlang::parsing::error::LangError;
use adeshlang::parsing::lexer::{Lexer, Token};
use adeshlang::parsing::parser::Parser;

/// Analysis result containing parsed AST, errors, and token positions
pub struct AnalysisResult {
    pub statements: Vec<Stmt>,
    pub errors: Vec<LangError>,
    pub tokens: Vec<Token>,
}

/// Analyze Adesh source code
pub fn analyze(source: &str) -> AnalysisResult {
    let mut lexer = Lexer::new(source);

    match lexer.tokenize() {
        Ok(tokens) => {
            let tokens_clone = tokens.clone();
            let mut parser = Parser::new(tokens, None);
            match parser.parse_program() {
                Ok(statements) => AnalysisResult {
                    statements,
                    errors: vec![],
                    tokens: tokens_clone,
                },
                Err(e) => AnalysisResult {
                    statements: vec![],
                    errors: vec![e],
                    tokens: tokens_clone,
                },
            }
        }
        Err(e) => AnalysisResult {
            statements: vec![],
            errors: vec![e],
            tokens: vec![],
        },
    }
}

/// Position of a symbol in source
#[derive(Debug, Clone, Copy)]
pub struct SymbolPosition {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Check if the next token after position i is an identifier matching name
fn check_next_token_is_identifier(
    tokens: &[Token],
    i: usize,
    name: &str,
) -> Option<SymbolPosition> {
    if i + 1 < tokens.len() {
        if let TokenKind::Identifier = tokens[i + 1].kind {
            if tokens[i + 1].lexeme == name {
                let tok = &tokens[i + 1];
                return Some(SymbolPosition {
                    start_line: tok.line,
                    start_col: tok.col,
                    end_line: tok.line,
                    end_col: tok.col + name.len(),
                });
            }
        }
    }
    None
}

/// Find position of a symbol in source by name and kind using tokens
pub fn find_symbol_position(
    tokens: &[Token],
    name: &str,
    kind: SymbolKind,
) -> Option<SymbolPosition> {
    // Search for declaration pattern based on kind
    for (i, tok) in tokens.iter().enumerate() {
        let matches_keyword = match kind {
            SymbolKind::Function | SymbolKind::Method => tok.kind == TokenKind::Fn,
            SymbolKind::Class => tok.kind == TokenKind::Class,
            SymbolKind::Variable | SymbolKind::Constant => {
                tok.kind == TokenKind::Let || tok.kind == TokenKind::Const
            }
            SymbolKind::Enum => tok.kind == TokenKind::Enum,
            SymbolKind::Interface => tok.kind == TokenKind::Interface,
            _ => false,
        };

        if matches_keyword {
            if let Some(pos) = check_next_token_is_identifier(tokens, i, name) {
                return Some(pos);
            }
        }
    }
    None
}

/// Find all occurrences of an identifier in tokens
pub fn find_all_identifier_occurrences(tokens: &[Token], name: &str) -> Vec<(usize, usize)> {
    let mut occurrences = Vec::new();
    for tok in tokens {
        if tok.kind == TokenKind::Identifier && tok.lexeme == name {
            occurrences.push((tok.line, tok.col));
        }
    }
    occurrences
}

/// Convert LangError to LSP Diagnostic
pub fn error_to_diagnostic(error: &LangError) -> Diagnostic {
    let start = Position {
        line: error.line.saturating_sub(1) as u32,
        character: error.col.saturating_sub(1) as u32,
    };

    let end = Position {
        line: error.end_line.saturating_sub(1) as u32,
        character: error.end_col as u32,
    };

    let severity = match error.kind {
        adeshlang::parsing::error::ErrorKind::Lexical
        | adeshlang::parsing::error::ErrorKind::Parse
        | adeshlang::parsing::error::ErrorKind::Type
        | adeshlang::parsing::error::ErrorKind::Compile => DiagnosticSeverity::ERROR,
        adeshlang::parsing::error::ErrorKind::Runtime | adeshlang::parsing::error::ErrorKind::Jit => {
            DiagnosticSeverity::WARNING
        }
        _ => DiagnosticSeverity::INFORMATION,
    };

    Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        code: None,
        code_description: None,
        source: Some("als".to_string()),
        message: error.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Symbol information extracted from AST
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub documentation: Option<String>,
    pub signature: Option<String>,
    pub type_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Variable,
    Constant,
    Parameter,
    Method,
    Property,
    Enum,
    Interface,
    Module,
}

impl SymbolKind {
    pub fn to_lsp_kind(&self) -> lsp_types::SymbolKind {
        match self {
            SymbolKind::Function => lsp_types::SymbolKind::FUNCTION,
            SymbolKind::Class => lsp_types::SymbolKind::CLASS,
            SymbolKind::Variable => lsp_types::SymbolKind::VARIABLE,
            SymbolKind::Constant => lsp_types::SymbolKind::CONSTANT,
            SymbolKind::Parameter => lsp_types::SymbolKind::VARIABLE,
            SymbolKind::Method => lsp_types::SymbolKind::METHOD,
            SymbolKind::Property => lsp_types::SymbolKind::PROPERTY,
            SymbolKind::Enum => lsp_types::SymbolKind::ENUM,
            SymbolKind::Interface => lsp_types::SymbolKind::INTERFACE,
            SymbolKind::Module => lsp_types::SymbolKind::MODULE,
        }
    }
}

/// Extract all symbols from AST with token position support
#[allow(dead_code)]
pub fn extract_symbols(statements: &[Stmt]) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();

    for stmt in statements {
        extract_symbols_from_stmt(stmt, &mut symbols);
    }

    symbols
}

/// Extract all symbols from AST with accurate positions from tokens
pub fn extract_symbols_with_positions(statements: &[Stmt], tokens: &[Token]) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();

    for stmt in statements {
        extract_symbols_from_stmt(stmt, &mut symbols);
    }

    // Update positions from tokens
    for sym in &mut symbols {
        if let Some(pos) = find_symbol_position(tokens, &sym.name, sym.kind) {
            sym.line = pos.start_line;
            sym.col = pos.start_col;
            sym.end_line = pos.end_line;
            sym.end_col = pos.end_col;
        }
    }

    symbols
}

fn extract_symbols_from_stmt(stmt: &Stmt, symbols: &mut Vec<SymbolInfo>) {
    match &stmt.kind {
        StmtKind::Let(name, _expr, type_ann, _export, is_const, _readonly) => {
            symbols.push(SymbolInfo {
                name: name.clone(),
                kind: if *is_const {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Variable
                },
                line: stmt.span.line,
                col: stmt.span.col,
                end_line: stmt.span.line,
                end_col: stmt.span.col + name.len(),
                documentation: None,
                signature: type_ann.clone(),
                type_name: type_ann.clone(),
            });
        }
        StmtKind::Function(func, _export) => {
            extract_function_symbol(func, symbols, &stmt.span);
        }
        StmtKind::Class(class, _export) => {
            extract_class_symbol(class, symbols, &stmt.span);
        }
        StmtKind::Enum(enum_decl, _export) => {
            symbols.push(SymbolInfo {
                name: enum_decl.name.clone(),
                kind: SymbolKind::Enum,
                line: stmt.span.line,
                col: stmt.span.col,
                end_line: stmt.span.line,
                end_col: stmt.span.col + enum_decl.name.len(),
                documentation: None,
                signature: None,
                type_name: None,
            });
        }
        StmtKind::Interface(iface, _export) => {
            symbols.push(SymbolInfo {
                name: iface.name.clone(),
                kind: SymbolKind::Interface,
                line: stmt.span.line,
                col: stmt.span.col,
                end_line: stmt.span.line,
                end_col: stmt.span.col + iface.name.len(),
                documentation: None,
                signature: None,
                type_name: None,
            });
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                extract_symbols_from_stmt(s, symbols);
            }
        }
        StmtKind::Region { body, .. } => {
            extract_symbols_from_stmt(body, symbols);
        }
        StmtKind::UnsafeBlock(body) => {
            extract_symbols_from_stmt(body, symbols);
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            extract_symbols_from_stmt(then_branch, symbols);
            if let Some(else_br) = else_branch {
                extract_symbols_from_stmt(else_br, symbols);
            }
        }
        StmtKind::While { body, .. } => {
            extract_symbols_from_stmt(body, symbols);
        }
        StmtKind::ForIn { body, .. } => {
            extract_symbols_from_stmt(body, symbols);
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            extract_symbols_from_stmt(try_block, symbols);
            extract_symbols_from_stmt(catch_block, symbols);
        }
        _ => {}
    }
}

fn extract_function_symbol(
    func: &Function,
    symbols: &mut Vec<SymbolInfo>,
    span: &adeshlang::parsing::ast::Span,
) {
    let params: Vec<String> = func
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
        params.join(", "),
        func.ret_type
            .as_ref()
            .map(|t| format!(": {}", t))
            .unwrap_or_default()
    );

    symbols.push(SymbolInfo {
        name: func.name.clone(),
        kind: SymbolKind::Function,
        line: span.line,
        col: span.col,
        end_line: span.line,
        end_col: span.col + func.name.len(),
        documentation: None,
        signature: Some(signature),
        type_name: func.ret_type.clone(),
    });

    // Add parameters as symbols
    for (name, _, type_ann) in &func.params {
        symbols.push(SymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Parameter,
            line: span.line,
            col: span.col,
            end_line: span.line,
            end_col: span.col + name.len(),
            documentation: None,
            signature: type_ann.clone(),
            type_name: type_ann.clone(),
        });
    }
}

fn extract_class_symbol(
    class: &ClassDecl,
    symbols: &mut Vec<SymbolInfo>,
    span: &adeshlang::parsing::ast::Span,
) {
    let mut signature = format!("class {}", class.name);

    if let Some(parent) = &class.extends {
        signature.push_str(&format!(" extends {}", parent));
    }

    if !class.implements.is_empty() {
        signature.push_str(&format!(" implements {}", class.implements.join(", ")));
    }

    symbols.push(SymbolInfo {
        name: class.name.clone(),
        kind: SymbolKind::Class,
        line: span.line,
        col: span.col,
        end_line: span.line,
        end_col: span.col + class.name.len(),
        documentation: None,
        signature: Some(signature),
        type_name: None,
    });

    // Add methods
    for method in &class.methods {
        let params: Vec<String> = method
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

        let sig = format!(
            "fn {}({}){}",
            method.name,
            params.join(", "),
            method
                .ret_type
                .as_ref()
                .map(|t| format!(": {}", t))
                .unwrap_or_default()
        );

        symbols.push(SymbolInfo {
            name: method.name.clone(),
            kind: SymbolKind::Method,
            line: span.line,
            col: span.col,
            end_line: span.line,
            end_col: span.col + method.name.len(),
            documentation: None,
            signature: Some(sig),
            type_name: method.ret_type.clone(),
        });
    }
}

/// Find symbol at position in AST
#[allow(dead_code)]
pub fn find_symbol_at_position(statements: &[Stmt], name: &str) -> Option<SymbolInfo> {
    let symbols = extract_symbols(statements);
    symbols.into_iter().find(|s| s.name == name)
}

/// Find all references to a symbol
#[allow(dead_code)]
pub fn find_references(statements: &[Stmt], name: &str) -> Vec<(usize, usize)> {
    let mut refs = Vec::new();

    for stmt in statements {
        find_references_in_stmt(stmt, name, &mut refs);
    }

    refs
}

#[allow(dead_code)]
fn find_references_in_stmt(stmt: &Stmt, name: &str, refs: &mut Vec<(usize, usize)>) {
    match &stmt.kind {
        StmtKind::Let(var_name, expr, _, _, _, _) => {
            if var_name == name {
                refs.push((stmt.span.line, stmt.span.col));
            }
            if let Some(e) = expr {
                find_references_in_expr(e, name, refs);
            }
        }
        StmtKind::ExprStmt(expr) => {
            find_references_in_expr(expr, name, refs);
        }
        StmtKind::Return(Some(expr)) => {
            find_references_in_expr(expr, name, refs);
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                find_references_in_stmt(s, name, refs);
            }
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            find_references_in_expr(cond, name, refs);
            find_references_in_stmt(then_branch, name, refs);
            if let Some(else_br) = else_branch {
                find_references_in_stmt(else_br, name, refs);
            }
        }
        StmtKind::While { cond, body } => {
            find_references_in_expr(cond, name, refs);
            find_references_in_stmt(body, name, refs);
        }
        StmtKind::ForIn { iter, body, .. } => {
            find_references_in_expr(iter, name, refs);
            find_references_in_stmt(body, name, refs);
        }
        StmtKind::Function(func, _) => {
            if func.name == name {
                refs.push((stmt.span.line, stmt.span.col));
            }
            for stmt in func.body.iter() {
                find_references_in_stmt(stmt, name, refs);
            }
        }
        StmtKind::Region { body, .. } => {
            find_references_in_stmt(body, name, refs);
        }
        StmtKind::UnsafeBlock(body) => {
            find_references_in_stmt(body, name, refs);
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            find_references_in_stmt(try_block, name, refs);
            find_references_in_stmt(catch_block, name, refs);
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn find_references_in_expr(expr: &Expr, name: &str, refs: &mut Vec<(usize, usize)>) {
    match &expr.kind {
        ExprKind::Variable(var_name) => {
            if var_name == name {
                refs.push((expr.span.line, expr.span.col));
            }
        }
        ExprKind::Binary(left, _, right) | ExprKind::Logical(left, _, right) => {
            find_references_in_expr(left, name, refs);
            find_references_in_expr(right, name, refs);
        }
        ExprKind::Call(callee, args, _) => {
            find_references_in_expr(callee, name, refs);
            for arg in args {
                find_references_in_expr(arg, name, refs);
            }
        }
        ExprKind::Get(obj, _) => {
            find_references_in_expr(obj, name, refs);
        }
        ExprKind::Array(items) => {
            for item in items {
                find_references_in_expr(item, name, refs);
            }
        }
        ExprKind::Grouping(inner) => {
            find_references_in_expr(inner, name, refs);
        }
        ExprKind::Unary(_, expr) => {
            find_references_in_expr(expr, name, refs);
        }
        ExprKind::Assign(_, expr) | ExprKind::AssignOp(_, _, expr) => {
            find_references_in_expr(expr, name, refs);
        }
        ExprKind::Index(base, index) => {
            find_references_in_expr(base, name, refs);
            find_references_in_expr(index, name, refs);
        }
        ExprKind::Set(obj, _, val) => {
            find_references_in_expr(obj, name, refs);
            find_references_in_expr(val, name, refs);
        }
        ExprKind::Conditional(cond, then_expr, else_expr) => {
            find_references_in_expr(cond, name, refs);
            find_references_in_expr(then_expr, name, refs);
            find_references_in_expr(else_expr, name, refs);
        }
        ExprKind::Range(start, end, _) => {
            find_references_in_expr(start, name, refs);
            find_references_in_expr(end, name, refs);
        }
        ExprKind::New(constructor, args) => {
            find_references_in_expr(constructor, name, refs);
            for arg in args {
                find_references_in_expr(arg, name, refs);
            }
        }
        ExprKind::Await(inner)
        | ExprKind::Spawn(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Spread(inner)
        | ExprKind::NonNull(inner)
        | ExprKind::Try(inner) => {
            find_references_in_expr(inner, name, refs);
        }
        ExprKind::Object(fields) | ExprKind::StructLiteral(_, fields) => {
            for (_, expr) in fields {
                find_references_in_expr(expr, name, refs);
            }
        }
        ExprKind::Tuple(items) | ExprKind::SetLiteral(items) => {
            for item in items {
                find_references_in_expr(item, name, refs);
            }
        }
        ExprKind::OptGet(obj, _) => {
            find_references_in_expr(obj, name, refs);
        }
        ExprKind::OptCall(callee, args, _) => {
            find_references_in_expr(callee, name, refs);
            for arg in args {
                find_references_in_expr(arg, name, refs);
            }
        }
        ExprKind::Match(expr, arms) => {
            find_references_in_expr(expr, name, refs);
            for (_, body) in arms {
                find_references_in_expr(body, name, refs);
            }
        }
        ExprKind::Update(_, _, expr) => {
            find_references_in_expr(expr, name, refs);
        }
        ExprKind::Fn(_, body, _) => {
            for stmt in body.iter() {
                find_references_in_stmt(stmt, name, refs);
            }
        }
        ExprKind::Format(expr, _) => {
            find_references_in_expr(expr, name, refs);
        }
        _ => {}
    }
}
