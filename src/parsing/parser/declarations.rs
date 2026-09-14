//! Declaration parsing module
//!
//! Handles parsing of top-level declarations including:
//! - Program entry point
//! - Import statements (import, from...import)
//! - Variable declarations (let, const)
//! - Export declarations

use super::core::Parser;
use super::derive_alias;
use crate::parsing::ast::{
    Expr, ExprKind, ShareDecl, Stmt, StmtKind, StrongDecl, TokenKind, WeakDecl,
};
use crate::parsing::error::LangError;

impl Parser {
    /// Parse a complete program (list of statements)
    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, LangError> {
        let mut s = Vec::new();
        while !self.is_end() {
            s.extend(self.declaration()?);
        }
        Ok(s)
    }

    /// Parse a program with error recovery — collects as many statements as possible,
    /// recording errors instead of aborting. Designed for IDE/LSP use where the source
    /// may be incomplete or syntactically invalid.
    pub fn parse_program_recovering(&mut self) -> (Vec<Stmt>, Vec<LangError>) {
        let mut statements = Vec::new();
        let mut errors = Vec::new();

        while !self.is_end() {
            match self.declaration() {
                Ok(stmts) => statements.extend(stmts),
                Err(e) => {
                    errors.push(e);
                    // Skip to the next likely statement boundary
                    self.synchronize();
                }
            }
        }

        (statements, errors)
    }

    /// Synchronize the parser after an error by skipping tokens until we reach
    /// a likely statement boundary.
    fn synchronize(&mut self) {
        // Skip the current token
        self.advance();

        while !self.is_end() {
            // Stop at semicolons (statement end)
            if self.prev().kind == TokenKind::Semicolon {
                return;
            }

            // Stop at keywords that typically start a new statement
            match self.peek().kind {
                TokenKind::Let
                | TokenKind::Const
                | TokenKind::Fn
                | TokenKind::Class
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Interface
                | TokenKind::Type
                | TokenKind::If
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Return
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Try
                | TokenKind::Import
                | TokenKind::Export
                | TokenKind::Extend
                | TokenKind::Decorator
                | TokenKind::Test
                | TokenKind::Region
                | TokenKind::Unsafe
                | TokenKind::Defer
                | TokenKind::RightBrace => return,
                _ => {}
            }

            self.advance();
        }
    }

    /// Parse a top-level declaration
    pub(super) fn declaration(&mut self) -> Result<Vec<Stmt>, LangError> {
        while self.matchk(&[TokenKind::DocComment]) {}

        // Check for $cImport directive
        // FFI: $cImport("header.h")
        if self.matchk(&[TokenKind::CImport]) {
            let start_span = self.previous_span();
            self.consume(TokenKind::LeftParen, "Expect '(' after $cImport")?;
            let path = self.consume_string("Expect string path in $cImport")?;
            self.consume(TokenKind::RightParen, "Expect ')' after $cImport path")?;
            return Ok(vec![Stmt {
                kind: StmtKind::HeaderImport { path },
                span: start_span,
            }]);
        }

        // collect optional decorators preceding declarations
        let mut pending_decorators: Vec<Expr> = Vec::new();
        while self.matchk(&[TokenKind::At]) {
            // parse decorator reference: name or name(args)
            let name = self.consume_ident("Expect decorator name after '@'")?;
            if self.matchk(&[TokenKind::LeftParen]) {
                // parse argument expressions
                let mut args: Vec<Expr> = Vec::new();
                if !self.check(TokenKind::RightParen) {
                    loop {
                        let e = self.expression()?;
                        args.push(e);
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(TokenKind::RightParen, "Expect ')' after decorator args")?;
                let call_span = self.previous_span();
                pending_decorators.push(Expr {
                    kind: ExprKind::Call(
                        Box::new(Expr {
                            kind: ExprKind::Variable(name),
                            span: call_span.clone(),
                        }),
                        args,
                        Vec::new(),
                    ),
                    span: call_span,
                });
            } else {
                pending_decorators.push(Expr {
                    kind: ExprKind::Variable(name),
                    span: self.previous_span(),
                });
            }
        }
        let mut is_export = false;
        if self.matchk(&[TokenKind::Export]) {
            is_export = true;
            if self.matchk(&[TokenKind::Default]) {
                let mut is_unsafe_default = false;
                if self.matchk(&[TokenKind::Unsafe]) {
                    is_unsafe_default = true;
                }
                let mut is_async_default = false;
                if self.matchk(&[TokenKind::Async]) {
                    is_async_default = true;
                    if self.matchk(&[TokenKind::Unsafe]) {
                        is_unsafe_default = true;
                    }
                }
                if self.matchk(&[TokenKind::Fn]) {
                    let fstmt = self.fn_decl(false, is_async_default, is_unsafe_default)?; // parses function into Stmt::Function
                    if let StmtKind::Function(f, _exp) = fstmt.kind {
                        return Ok(vec![Stmt {
                            kind: StmtKind::ExportDefaultFunction(f),
                            span: fstmt.span,
                        }]);
                    }
                } else if self.matchk(&[TokenKind::Class]) {
                    // no abstract allowed for default class here
                    let cstmt = self.class_decl(false, false, false)?;
                    if let StmtKind::Class(c, _exp) = cstmt.kind {
                        return Ok(vec![Stmt {
                            kind: StmtKind::ExportDefaultClass(c),
                            span: cstmt.span,
                        }]);
                    }
                } else {
                    let name = self.consume_ident("Expect identifier after 'export default'")?;
                    self.consume(TokenKind::Semicolon, "Expect ';' after export default")?;
                    return Ok(vec![Stmt {
                        kind: StmtKind::ExportDefault(name),
                        span: self.previous_span(),
                    }]);
                }
            }
        }
        if self.matchk(&[TokenKind::Decorator]) {
            let s = self.decorator_decl(is_export)?;
            return Ok(vec![s]);
        }
        // support optional 'abstract' modifier before class
        let mut is_abstract = false;
        if self.matchk(&[TokenKind::Abstract]) {
            is_abstract = true;
        }
        // support optional 'sealed' modifier before class
        let mut is_sealed = false;
        if self.matchk(&[TokenKind::Sealed]) {
            is_sealed = true;
        }

        if self.matchk(&[TokenKind::Import]) {
            return Ok(vec![self.import_stmt()?]);
        }
        if self.matchk(&[TokenKind::From]) {
            return Ok(vec![self.from_import_stmt()?]);
        }
        if self.matchk(&[TokenKind::Extend]) {
            return Ok(vec![self.extend_decl(is_export)?]);
        }
        if self.matchk(&[TokenKind::Struct]) {
            return Ok(vec![self.struct_decl(is_export)?]);
        }
        if self.matchk(&[TokenKind::Enum]) {
            return Ok(vec![self.enum_decl(is_export)?]);
        }
        if self.matchk(&[TokenKind::Share]) {
            return Ok(vec![self.share_decl(is_export)?]);
        }
        if self.matchk(&[TokenKind::Strong]) {
            return Ok(vec![self.strong_decl(is_export)?]);
        }
        if self.matchk(&[TokenKind::Weak]) {
            return Ok(vec![self.weak_decl(is_export)?]);
        }
        // Check for readonly modifier before let/const
        let is_readonly = self.matchk(&[TokenKind::Readonly]);
        if self.matchk(&[TokenKind::Let]) {
            return Ok(vec![self.let_decl(is_export, false, is_readonly)?]);
        }
        if self.matchk(&[TokenKind::Const]) {
            return Ok(vec![self.let_decl(is_export, true, is_readonly)?]);
        }
        // If we consumed readonly but no let/const follows, it's an error
        if is_readonly {
            return Err(self.make_error("Expected 'let' or 'const' after 'readonly' keyword"));
        }
        // optional 'async' modifier before a free function declaration
        let mut is_async = false;
        if self.matchk(&[TokenKind::Async]) {
            is_async = true;
        }
        // optional 'test' keyword before a function declaration
        let mut is_test = false;
        if self.matchk(&[TokenKind::Test]) {
            is_test = true;
        }
        if is_test && self.check(TokenKind::Identifier) && self.peek_next_kind(TokenKind::LeftBrace)
        {
            let group_name = self.consume_ident("Expect test group name after 'test'")?;
            self.consume(TokenKind::LeftBrace, "Expect '{' after test group name")?;
            let body = self.block()?;
            let mut out: Vec<Stmt> = Vec::new();
            for stmt in body {
                match stmt.kind {
                    StmtKind::Function(mut f, exp) => {
                        f.is_test = true;
                        f.name = format!("{}__{}", group_name, f.name);
                        out.push(Stmt {
                            kind: StmtKind::Function(f, exp),
                            span: stmt.span,
                        });
                    }
                    _ => {
                        return Err(
                            self.make_error("Only test functions are allowed inside a test block")
                        );
                    }
                }
            }
            return Ok(out);
        }
        // FFI: extern "ABI" pub(super) fn ... or extern "ABI" { ... }
        if self.matchk(&[TokenKind::Extern]) {
            return Ok(vec![self.extern_decl()?]);
        }
        // optional 'unsafe' modifier before a function declaration or unsafe block
        let mut is_unsafe = false;
        if self.matchk(&[TokenKind::Unsafe]) {
            if self.check(TokenKind::Fn)
                || (self.check(TokenKind::Async) && self.peek_next_kind(TokenKind::Fn))
            {
                is_unsafe = true;
                if self.matchk(&[TokenKind::Async]) {
                    is_async = true;
                }
            } else if self.check(TokenKind::LeftBrace) {
                // It's an unsafe block: unsafe { ... }
                let start_span = self.previous_span();
                self.consume(TokenKind::LeftBrace, "Expect '{' after unsafe")?;
                let body_stmts = self.block()?;
                return Ok(vec![Stmt {
                    kind: StmtKind::UnsafeBlock(Box::new(Stmt {
                        kind: StmtKind::Block(body_stmts),
                        span: self.previous_span(),
                    })),
                    span: start_span,
                }]);
            } else {
                return Err(self.make_error("Expected 'fn' or '{' after 'unsafe'"));
            }
        }
        if is_async && self.matchk(&[TokenKind::Unsafe]) {
            is_unsafe = true;
        }
        if self.matchk(&[TokenKind::Fn]) {
            let mut s = self.fn_decl(is_export, is_async, is_unsafe)?;
            if let StmtKind::Function(ref mut f, _exp) = s.kind {
                f.decorators = pending_decorators;
                f.is_test = is_test;
            }
            return Ok(vec![s]);
        }
        if self.matchk(&[TokenKind::Class]) {
            let s = self.class_decl(is_export, is_abstract, is_sealed)?;
            if let StmtKind::Class(mut cd, exp) = s.kind {
                cd.decorators = pending_decorators;
                return Ok(vec![Stmt {
                    kind: StmtKind::Class(cd, exp),
                    span: s.span,
                }]);
            }
            return Ok(vec![s]);
        }
        if self.matchk(&[TokenKind::Interface]) {
            return Ok(vec![self.interface_decl(is_export)?]);
        }
        if self.matchk(&[TokenKind::Type]) {
            return Ok(vec![self.type_alias_decl(is_export)?]);
        }
        if self.matchk(&[TokenKind::Return]) {
            return Ok(vec![self.return_stmt()?]);
        }
        if self.matchk(&[TokenKind::Try]) {
            return Ok(vec![self.try_catch()?]);
        }

        Ok(vec![self.statement()?])
    }

    /// Parse a module path, supporting both string paths and namespace:module syntax
    pub(super) fn parse_module_path(&mut self, context: &str) -> Result<String, LangError> {
        if self.check(TokenKind::String) {
            self.consume_string(&format!("Expect string path after {}", context))
        } else {
            let mut path_parts = self.consume_ident(&format!(
                "Expect module name or string path after {}",
                context
            ))?;
            // Check for namespace:module syntax (e.g., std:math)
            while self.matchk(&[TokenKind::Colon]) {
                let next_part = self.consume_ident("Expect module name after ':'")?;
                path_parts.push(':');
                path_parts.push_str(&next_part);
            }
            Ok(path_parts)
        }
    }

    /// Parse import statement: import <path> [as alias]; or import { name1, name2 } from <path>;
    pub(super) fn import_stmt(&mut self) -> Result<Stmt, LangError> {
        // Support JS/TS style named imports: import { name1, name2 } from "path";
        if self.matchk(&[TokenKind::LeftBrace]) {
            let mut names: Vec<String> = Vec::new();
            loop {
                let n = self.consume_ident("Expect name to import")?;
                names.push(n);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
            self.consume(TokenKind::RightBrace, "Expect '}' after import names")?;
            self.consume(TokenKind::From, "Expect 'from' after import list")?;
            let path = self.parse_module_path("import")?;
            self.consume(TokenKind::Semicolon, "Expect ';' after import")?;
            return Ok(Stmt {
                kind: StmtKind::ImportNames { path, names },
                span: self.previous_span(),
            });
        }

        // import <pathOrIdent> [as alias] ;
        // Also supports namespace:module syntax like "std:math"
        let path = self.parse_module_path("import")?;
        if self.matchk(&[TokenKind::As]) {
            let alias = self.consume_ident("Expect alias identifier")?;
            self.consume(TokenKind::Semicolon, "Expect ';' after import")?;
            Ok(Stmt {
                kind: StmtKind::Import { path, alias },
                span: self.previous_span(),
            })
        } else {
            // no alias: default import; alias derived from path
            self.consume(TokenKind::Semicolon, "Expect ';' after import")?;
            let alias = derive_alias(&path);
            Ok(Stmt {
                kind: StmtKind::ImportDefault { path, alias },
                span: self.previous_span(),
            })
        }
    }

    /// Parse from...import statement: from <path> import name[, name]*;
    pub(super) fn from_import_stmt(&mut self) -> Result<Stmt, LangError> {
        // from <pathOrIdent> import name[, name]* ;
        // Also supports namespace:module syntax like "std:math"
        let path = self.parse_module_path("'from'")?;
        self.consume(
            TokenKind::Import,
            "Expect 'import' after module in 'from' statement",
        )?;
        let mut names: Vec<String> = Vec::new();
        loop {
            let n = self.consume_ident("Expect name to import")?;
            names.push(n);
            if !self.matchk(&[TokenKind::Comma]) {
                break;
            }
        }
        self.consume(TokenKind::Semicolon, "Expect ';' after import list")?;
        Ok(Stmt {
            kind: StmtKind::ImportNames { path, names },
            span: self.previous_span(),
        })
    }

    /// Parse let/const declaration with optional tuple destructuring
    pub(super) fn let_decl(
        &mut self,
        exp: bool,
        is_const: bool,
        is_readonly: bool,
    ) -> Result<Stmt, LangError> {
        // Check for object destructuring: let { x, y } = obj
        if self.check(TokenKind::LeftBrace) {
            self.advance();
            let mut bindings: Vec<(String, Option<String>)> = Vec::new();

            if !self.check(TokenKind::RightBrace) {
                loop {
                    let key = self.consume_ident("Expect property name in object pattern")?;

                    // Check for alias: { x: newName } or shorthand { x }
                    let alias = if self.matchk(&[TokenKind::Colon]) {
                        Some(self.consume_ident("Expect variable name after ':'")?)
                    } else {
                        None
                    };

                    bindings.push((key, alias));

                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(TokenKind::RightBrace, "Expect '}' after object pattern")?;

            let init = if self.matchk(&[TokenKind::Equal]) {
                Some(self.expression()?)
            } else {
                None
            };
            self.consume(TokenKind::Semicolon, "Expect ';'")?;
            return Ok(Stmt {
                kind: StmtKind::LetObject(bindings, init, exp, is_const, is_readonly),
                span: self.previous_span(),
            });
        }

        // Check for tuple destructuring: let (a, b, c) = ... or let (a, b, c): (T1, T2, T3) = ...
        if self.check(TokenKind::LeftParen) {
            self.advance();
            let mut names = Vec::new();
            if !self.check(TokenKind::RightParen) {
                loop {
                    // Check for rest pattern: ...rest
                    let name = if self.matchk(&[TokenKind::DotDotDot]) {
                        let rest_name = self.consume_ident("Expect variable name after '...'")?;
                        format!("...{}", rest_name)
                    } else {
                        self.consume_ident("Expect variable name in tuple pattern")?
                    };

                    names.push(name.clone());

                    // Rest pattern must be last
                    if name.starts_with("...") {
                        if self.check(TokenKind::Comma) {
                            return Err(self.format_err(
                                self.peek(),
                                "Rest pattern must be last in destructuring",
                            ));
                        }
                        break;
                    }

                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(TokenKind::RightParen, "Expect ')' after tuple pattern")?;

            // Optional type annotation for tuple: : (T1, T2, T3)
            let type_anns: Option<Vec<String>> = if self.matchk(&[TokenKind::Colon]) {
                self.consume(
                    TokenKind::LeftParen,
                    "Expect '(' after ':' for tuple type annotation",
                )?;
                let mut types = Vec::new();
                if !self.check(TokenKind::RightParen) {
                    loop {
                        let type_name =
                            self.parse_type_name("Expect type name in tuple type annotation")?;
                        types.push(type_name);
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(
                    TokenKind::RightParen,
                    "Expect ')' after tuple type annotation",
                )?;
                Some(types)
            } else {
                None
            };

            let init = if self.matchk(&[TokenKind::Equal]) {
                Some(self.expression()?)
            } else {
                None
            };
            self.consume(TokenKind::Semicolon, "Expect ';'")?;
            return Ok(Stmt {
                kind: StmtKind::LetTuple(names, type_anns, init, exp, is_const, is_readonly),
                span: self.previous_span(),
            });
        }

        // Regular or multi-variable declaration
        let first_name = self.consume_ident("Expect variable name")?;
        let first_type = if self.matchk(&[TokenKind::Colon]) {
            Some(self.parse_type_name("Expect type name after ':'")?)
        } else {
            None
        };

        // Multi-variable declaration: let a, b, c = 10, 20, 30; or let a: i32, b: f64 = 1, 2.5;
        if self.check(TokenKind::Comma) {
            let mut names = vec![first_name];
            let mut type_anns = vec![first_type];
            while self.matchk(&[TokenKind::Comma]) {
                let name =
                    self.consume_ident("Expect variable name in multi-variable declaration")?;
                let type_ann = if self.matchk(&[TokenKind::Colon]) {
                    Some(self.parse_type_name("Expect type name after ':'")?)
                } else {
                    None
                };
                names.push(name);
                type_anns.push(type_ann);
            }

            let has_type_anns = type_anns.iter().any(|t| t.is_some());
            let final_type_anns = if has_type_anns {
                Some(
                    type_anns
                        .into_iter()
                        .map(|t| t.unwrap_or_else(|| "any".to_string()))
                        .collect(),
                )
            } else {
                None
            };

            let final_readonly = is_readonly || self.matchk(&[TokenKind::Readonly]);
            let init = if self.matchk(&[TokenKind::Equal]) {
                let old_allow = self.allow_struct_literal;
                self.allow_struct_literal = false;
                let res = self.parse_tuple_rhs()?;
                self.allow_struct_literal = old_allow;
                Some(res)
            } else {
                None
            };
            self.consume(TokenKind::Semicolon, "Expect ';'")?;
            return Ok(Stmt {
                kind: StmtKind::LetTuple(
                    names,
                    final_type_anns,
                    init,
                    exp,
                    is_const,
                    final_readonly,
                ),
                span: self.previous_span(),
            });
        }

        let name = first_name;
        let type_ann = first_type;
        // Check for 'readonly' after type annotation (for array types: let arr: [u8] readonly = ...)
        let final_readonly = is_readonly || self.matchk(&[TokenKind::Readonly]);
        let init = if self.matchk(&[TokenKind::Equal]) {
            let old_allow = self.allow_struct_literal;
            if type_ann.is_some() {
                self.allow_struct_literal = false;
            }
            let res = self.expression()?;
            self.allow_struct_literal = old_allow;
            Some(res)
        } else {
            None
        };

        // Support C-style multi-binding initialization: let i = 0, j = 10;
        if self.matchk(&[TokenKind::Comma]) {
            let mut stmts = vec![Stmt {
                kind: StmtKind::Let(name, init, type_ann, exp, is_const, final_readonly),
                span: self.previous_span(),
            }];

            while !self.check(TokenKind::Semicolon)
                && !self.check(TokenKind::RightParen)
                && !self.is_end()
            {
                let next_name = self.consume_ident("Expect variable name in multi-declaration")?;
                let next_type = if self.matchk(&[TokenKind::Colon]) {
                    Some(self.parse_type_name("Expect type name after ':'")?)
                } else {
                    None
                };
                let next_readonly = is_readonly || self.matchk(&[TokenKind::Readonly]);
                let next_init = if self.matchk(&[TokenKind::Equal]) {
                    let old_allow = self.allow_struct_literal;
                    if next_type.is_some() {
                        self.allow_struct_literal = false;
                    }
                    let res = self.expression()?;
                    self.allow_struct_literal = old_allow;
                    Some(res)
                } else {
                    None
                };

                stmts.push(Stmt {
                    kind: StmtKind::Let(
                        next_name,
                        next_init,
                        next_type,
                        exp,
                        is_const,
                        next_readonly,
                    ),
                    span: self.previous_span(),
                });

                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }

            self.matchk(&[TokenKind::Semicolon]);
            return Ok(Stmt {
                kind: StmtKind::Block(stmts),
                span: self.previous_span(),
            });
        }

        self.matchk(&[TokenKind::Semicolon]);
        Ok(Stmt {
            kind: StmtKind::Let(name, init, type_ann, exp, is_const, final_readonly),
            span: self.previous_span(),
        })
    }

    pub(super) fn share_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let name = self.consume_ident("Expect variable name after 'share'")?;
        let type_ann = if self.matchk(&[TokenKind::Colon]) {
            Some(self.parse_type_name("Expect type name after ':'")?)
        } else {
            None
        };
        self.consume(
            TokenKind::Equal,
            "Expect '=' after variable name in share declaration",
        )?;
        let init = self.expression()?;
        self.consume(TokenKind::Semicolon, "Expect ';'")?;
        Ok(Stmt {
            kind: StmtKind::ShareDeclaration(
                ShareDecl {
                    name,
                    expr: init,
                    type_ann,
                },
                exp,
            ),
            span: self.previous_span(),
        })
    }

    pub(super) fn strong_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let name = self.consume_ident("Expect variable name after 'strong'")?;
        let type_ann = if self.matchk(&[TokenKind::Colon]) {
            Some(self.parse_type_name("Expect type name after ':'")?)
        } else {
            None
        };
        self.consume(
            TokenKind::Equal,
            "Expect '=' after variable name in strong declaration",
        )?;
        let init = self.expression()?;
        self.consume(TokenKind::Semicolon, "Expect ';'")?;
        Ok(Stmt {
            kind: StmtKind::StrongDeclaration(
                StrongDecl {
                    name,
                    expr: init,
                    type_ann,
                },
                exp,
            ),
            span: self.previous_span(),
        })
    }

    pub(super) fn weak_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let name = self.consume_ident("Expect variable name after 'weak'")?;
        let type_ann = if self.matchk(&[TokenKind::Colon]) {
            Some(self.parse_type_name("Expect type name after ':'")?)
        } else {
            None
        };
        self.consume(
            TokenKind::Equal,
            "Expect '=' after variable name in weak declaration",
        )?;
        let init = self.expression()?;
        self.consume(TokenKind::Semicolon, "Expect ';'")?;
        Ok(Stmt {
            kind: StmtKind::WeakDeclaration(
                WeakDecl {
                    name,
                    expr: init,
                    type_ann,
                },
                exp,
            ),
            span: self.previous_span(),
        })
    }
}
