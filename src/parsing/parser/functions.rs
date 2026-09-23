//! Function and decorator declaration parsing
//!
//! This module handles parsing of:
//! - Function declarations with optional async, generics, parameters, and return types
//! - Decorator declarations with compile-time and runtime phases

use super::core::Parser;
use crate::parsing::ast::{DecoratorDef, DecoratorPhase, Function, Stmt, StmtKind, TokenKind};
use crate::parsing::error::LangError;

impl Parser {
    pub(super) fn fn_decl(
        &mut self,
        exp: bool,
        is_async: bool,
        is_unsafe: bool,
    ) -> Result<Stmt, LangError> {
        let start_span = self.previous_span();
        let name = if self.matchk(&[TokenKind::New]) {
            "new".to_string()
        } else {
            self.consume_ident("Expect function name")?
        };
        // optional generic type parameter list: <T, U>
        let mut type_params: Vec<String> = Vec::new();
        if self.matchk(&[TokenKind::Less]) {
            while !self.check(TokenKind::Greater) {
                let tname = self.consume_ident("Expect type parameter name")?;
                type_params.push(tname);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
            self.consume_generic_greater("Expect '>' after type params")?;
        }
        self.consume(TokenKind::LeftParen, "Expect '('")?;
        let mut params: Vec<(String, Option<crate::parsing::ast::Expr>, Option<String>)> =
            Vec::new();
        if !self.check(TokenKind::RightParen) {
            loop {
                // Check for rest parameter syntax (...param)
                let is_rest = self.matchk(&[TokenKind::DotDotDot]);

                // Check for tuple destructuring: [a, b] or (a, b)
                let final_name: String;
                let mut typ: Option<String> = None;
                let mut def: Option<crate::parsing::ast::Expr> = None;

                if !is_rest
                    && (self.check(TokenKind::LeftBracket) || self.check(TokenKind::LeftParen))
                {
                    // Tuple destructuring pattern
                    let is_paren = self.check(TokenKind::LeftParen);
                    self.advance(); // consume [ or (

                    let mut pattern_names = Vec::new();
                    if !(self.check(TokenKind::RightBracket) || self.check(TokenKind::RightParen)) {
                        loop {
                            let pname = self.consume_ident("Expect parameter name in pattern")?;
                            pattern_names.push(pname);
                            if !self.matchk(&[TokenKind::Comma]) {
                                break;
                            }
                        }
                    }

                    if is_paren {
                        self.consume(TokenKind::RightParen, "Expect ')' after tuple pattern")?;
                    } else {
                        self.consume(TokenKind::RightBracket, "Expect ']' after tuple pattern")?;
                    }

                    // Store as special marker: #tuple#name1,name2,name3
                    final_name = format!("#tuple#{}", pattern_names.join(","));

                    // Optional type annotation for the whole tuple
                    if self.matchk(&[TokenKind::Colon]) {
                        typ = Some(self.parse_type_name("Expect type annotation after ':'")?);
                    }

                    // No defaults for tuple patterns
                } else {
                    // Simple parameter or rest parameter
                    let pname = self.consume_ident("Expect param")?;
                    final_name = if is_rest {
                        format!("...{}", pname)
                    } else {
                        pname
                    };

                    if self.matchk(&[TokenKind::Colon]) {
                        typ = Some(self.parse_type_name("Expect type name after ':'")?);
                    }
                    if self.matchk(&[TokenKind::Equal]) {
                        // allow arbitrary expression as default (evaluated at call time)
                        def = Some(self.expression()?);
                    }
                }

                params.push((final_name, def, typ));
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
        }
        self.consume(TokenKind::RightParen, "Expect ')'")?;
        // optional return type annotation: fn foo(...): Type or fn foo(...) -> Type
        let mut ret_type: Option<String> = None;
        if self.matchk(&[TokenKind::Colon])
            || (self.matchk(&[TokenKind::Minus]) && self.matchk(&[TokenKind::Greater]))
            || self.matchk(&[TokenKind::Arrow])
        {
            ret_type = Some(self.parse_type_name("Expect return type name")?);
        }
        // optional 'unsafe' modifier after params/return type: fn foo(...) unsafe { ... }
        let mut final_is_unsafe = is_unsafe;
        if self.matchk(&[TokenKind::Unsafe]) {
            final_is_unsafe = true;
        }
        self.consume(TokenKind::LeftBrace, "Expect '{'")?;
        let body = self.block()?;
        // allow optional semicolon after a function declaration (top-level or inside blocks)
        let _ = self.matchk(&[TokenKind::Semicolon]);
        let mut func = Function::new(
            name,
            type_params,
            params,
            std::sync::Arc::new(body),
            None,
            ret_type,
            is_async,
        );
        func.is_unsafe = final_is_unsafe;
        Ok(Stmt {
            kind: StmtKind::Function(func, exp),
            span: start_span,
        })
    }
    pub(super) fn decorator_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let name = self.consume_ident("Expect decorator name")?;

        // Parse decorator parameters
        let mut params: Vec<(String, Option<crate::parsing::ast::Expr>, Option<String>)> =
            Vec::new();
        let mut requires_unsafe = false;

        // Check for optional parameter list
        if self.matchk(&[TokenKind::LeftParen]) {
            if !self.check(TokenKind::RightParen) {
                loop {
                    let pname = self.consume_ident("Expect parameter name")?;
                    let mut typ: Option<String> = None;
                    if self.matchk(&[TokenKind::Colon]) {
                        typ = Some(self.parse_type_name("Expect type name after ':'")?);
                    }
                    let mut def: Option<crate::parsing::ast::Expr> = None;
                    if self.matchk(&[TokenKind::Equal]) {
                        def = Some(self.expression()?);
                    }
                    params.push((pname, def, typ));
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(
                TokenKind::RightParen,
                "Expect ')' after decorator parameters",
            )?;
        }

        // Check for "requires unsafe" modifier
        if self.matchk(&[TokenKind::Require]) {
            self.consume(TokenKind::Unsafe, "Expect 'unsafe' after 'requires'")?;
            requires_unsafe = true;
        }

        self.consume(TokenKind::LeftBrace, "Expect '{' to start decorator body")?;

        // Parse decorator phases
        let mut phases: Vec<DecoratorPhase> = Vec::new();

        // If we see phase keywords, parse as phase-based decorator (NEW SYNTAX)
        if self.check(TokenKind::Compile)
            || self.check(TokenKind::Runtime)
            || self.check(TokenKind::Typecheck)
            || self.check(TokenKind::Emit)
        {
            while !self.check(TokenKind::RightBrace) && !self.is_end() {
                if self.matchk(&[TokenKind::Compile]) {
                    // compile(fn) { ... }
                    self.consume(TokenKind::LeftParen, "Expect '(' after 'compile'")?;
                    let _param = self.consume_ident("Expect parameter name")?;
                    self.consume(TokenKind::RightParen, "Expect ')' after parameter")?;
                    self.consume(TokenKind::LeftBrace, "Expect '{' for compile block")?;
                    let body = self.block()?;
                    phases.push(DecoratorPhase::Compile(std::sync::Arc::new(body)));
                } else if self.matchk(&[TokenKind::Runtime]) {
                    // runtime(call) { ... }
                    self.consume(TokenKind::LeftParen, "Expect '(' after 'runtime'")?;
                    let _param = self.consume_ident("Expect parameter name")?;
                    self.consume(TokenKind::RightParen, "Expect ')' after parameter")?;
                    self.consume(TokenKind::LeftBrace, "Expect '{' for runtime block")?;
                    let body = self.block()?;
                    phases.push(DecoratorPhase::Runtime(std::sync::Arc::new(body)));
                } else if self.matchk(&[TokenKind::Typecheck]) {
                    // typecheck(fn) { ... }
                    self.consume(TokenKind::LeftParen, "Expect '(' after 'typecheck'")?;
                    let _param = self.consume_ident("Expect parameter name")?;
                    self.consume(TokenKind::RightParen, "Expect ')' after parameter")?;
                    self.consume(TokenKind::LeftBrace, "Expect '{' for typecheck block")?;
                    let body = self.block()?;
                    phases.push(DecoratorPhase::Typecheck(std::sync::Arc::new(body)));
                } else if self.matchk(&[TokenKind::Emit]) {
                    // emit(ir) { ... }
                    self.consume(TokenKind::LeftParen, "Expect '(' after 'emit'")?;
                    let _param = self.consume_ident("Expect parameter name")?;
                    self.consume(TokenKind::RightParen, "Expect ')' after parameter")?;
                    self.consume(TokenKind::LeftBrace, "Expect '{' for emit block")?;
                    let body = self.block()?;
                    phases.push(DecoratorPhase::Emit(std::sync::Arc::new(body)));
                } else {
                    let span = self.previous_span();
                    return Err(crate::parsing::error::LangError::new(
                        crate::parsing::error::ErrorKind::Parse,
                        "Expected decorator phase (compile, runtime, typecheck, or emit)"
                            .to_string(),
                        span.line,
                        span.col,
                        span.line_text,
                    )
                    .with_auto_hints()
                    .with_file_opt(self.module.clone()));
                }
            }

            self.consume(TokenKind::RightBrace, "Expect '}' after decorator phases")?;

            // Return as new-style decorator
            Ok(Stmt {
                kind: StmtKind::Decorator(
                    DecoratorDef {
                        name,
                        params,
                        phases,
                        requires_unsafe,
                        is_new_style: true,
                    },
                    exp,
                ),
                span: self.previous_span(),
            })
        } else {
            // OLD SYNTAX: decorator name(target, meta) { body }
            // Parse as a function body and convert to runtime phase for backward compatibility
            let body = self.block()?;

            // Create a runtime phase with the body
            phases.push(DecoratorPhase::Runtime(std::sync::Arc::new(body)));

            let _ = self.matchk(&[TokenKind::Semicolon]);

            // Return as decorator with single runtime phase for backward compatibility
            Ok(Stmt {
                kind: StmtKind::Decorator(
                    DecoratorDef {
                        name,
                        params,
                        phases,
                        requires_unsafe,
                        is_new_style: false,
                    },
                    exp,
                ),
                span: self.previous_span(),
            })
        }
    }
}
