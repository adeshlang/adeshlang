//! Class declaration parsing
//!
//! This module handles parsing of:
//! - Class declarations with generics, extends, implements
//! - Methods with decorators, visibility, async, abstract, static modifiers
//! - Getters, setters, operators, constructors
//! - Instance fields and static properties with decorators
//! - Operator overloading symbol parsing

use super::core::Parser;
use crate::parsing::ast::{
    ClassDecl, Expr, ExprKind, Function, Stmt, StmtKind, TokenKind, Visibility,
};
use crate::parsing::error::LangError;

impl Parser {
    pub(super) fn class_decl(
        &mut self,
        exp: bool,
        is_abstract: bool,
        is_sealed: bool,
    ) -> Result<Stmt, LangError> {
        let start_span = self.previous_span();
        let name = self.consume_ident("Expect class name")?;
        // optional generics on class: MyClass<T,U>
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
        // optional extends
        let mut extends: Option<String> = None;
        let mut implements: Vec<String> = Vec::new();
        if self.matchk(&[TokenKind::Extends]) {
            extends = Some(self.consume_ident("Expect superclass name after 'extends'")?);
        }
        if self.matchk(&[TokenKind::Implements]) {
            loop {
                let iname = self.consume_ident("Expect interface name after 'implements'")?;
                implements.push(iname);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
        }
        self.consume(TokenKind::LeftBrace, "Expect '{' before class body")?;

        let mut methods = Vec::new();
        let mut static_methods = Vec::new();
        let mut static_properties = Vec::new();
        let mut instance_fields = Vec::new();

        while !self.check(TokenKind::RightBrace) {
            // collect method decorators
            let mut method_decorators: Vec<Expr> = Vec::new();
            while self.matchk(&[TokenKind::At]) {
                let at_span = self.previous_span();
                let name = self.consume_ident("Expect decorator name after '@'")?;
                let name_span = self.previous_span();
                if self.matchk(&[TokenKind::LeftParen]) {
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
                    method_decorators.push(Expr {
                        kind: ExprKind::Call(
                            Box::new(Expr {
                                kind: ExprKind::Variable(name),
                                span: name_span.clone(),
                            }),
                            args,
                            Vec::new(),
                        ),
                        span: at_span,
                    });
                } else {
                    method_decorators.push(Expr {
                        kind: ExprKind::Variable(name),
                        span: name_span,
                    });
                }
            }
            // Check for static modifier
            let mut is_static = false;
            if self.matchk(&[TokenKind::Static]) {
                is_static = true;
            }

            // Handle static properties (support property decorators)
            let mut prop_decorators: Vec<Expr> = method_decorators.clone();
            while self.matchk(&[TokenKind::At]) {
                let at_span = self.previous_span();
                let name = self.consume_ident("Expect decorator name after '@'")?;
                let name_span = self.previous_span();
                if self.matchk(&[TokenKind::LeftParen]) {
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
                    prop_decorators.push(Expr {
                        kind: ExprKind::Call(
                            Box::new(Expr {
                                kind: ExprKind::Variable(name),
                                span: name_span.clone(),
                            }),
                            args,
                            Vec::new(),
                        ),
                        span: at_span,
                    });
                } else {
                    prop_decorators.push(Expr {
                        kind: ExprKind::Variable(name),
                        span: name_span,
                    });
                }
            }
            if is_static && self.check(TokenKind::Identifier) {
                let prop_name = self.consume_ident("Expect property name")?;
                self.consume(TokenKind::Equal, "Expect '=' after static property name")?;
                let value = self.expression()?;
                self.consume(TokenKind::Semicolon, "Expect ';' after static property")?;
                static_properties.push((prop_name, value, prop_decorators));
                continue;
            }

            // optional visibility modifier
            let mut vis: Option<Visibility> = None;
            if self.matchk(&[TokenKind::Private]) {
                vis = Some(Visibility::Priv);
            } else if self.matchk(&[TokenKind::Protected]) {
                vis = Some(Visibility::Protected);
            } else if self.matchk(&[TokenKind::Public]) {
                vis = Some(Visibility::Pub);
            }

            // Check for abstract modifier
            let mut is_method_abstract = false;
            if self.matchk(&[TokenKind::Abstract]) {
                is_method_abstract = true;
            }

            // optional async modifier on methods
            let mut method_is_async = false;
            if self.matchk(&[TokenKind::Async]) {
                method_is_async = true;
            }

            // Check for special method types
            let mut is_constructor = false;
            let mut is_getter = false;
            let mut is_setter = false;
            let mut is_operator = false;
            let mut is_unsafe = false;
            let mut operator_symbol: Option<String> = None;

            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe = true;
            }

            if self.check(TokenKind::Constructor) {
                is_constructor = true;
                self.advance();
            } else if self.matchk(&[TokenKind::Get]) {
                is_getter = true;
            } else if self.matchk(&[TokenKind::Set]) {
                is_setter = true;
            } else if self.matchk(&[TokenKind::Operator]) {
                is_operator = true;
                operator_symbol = Some(self.parse_operator_symbol()?);
            } else if self.check(TokenKind::Identifier)
                && self.peek().lexeme == name
                && self.peek_next_kind(TokenKind::LeftParen)
            {
                is_constructor = true;
                self.advance();
            } else if self.matchk(&[TokenKind::Fn]) {
                if self.matchk(&[TokenKind::Unsafe]) {
                    is_unsafe = true;
                }
                if self.matchk(&[TokenKind::Operator]) {
                    is_operator = true;
                    operator_symbol = Some(self.parse_operator_symbol()?);
                } else if self.check(TokenKind::Identifier)
                    && (self.peek().lexeme == "init" || self.peek().lexeme == name)
                {
                    is_constructor = true;
                    self.advance();
                }
            } else if self.check(TokenKind::Identifier) && self.peek_next_kind(TokenKind::LeftParen)
            {
                // Method without 'fn' keyword (e.g. emit(event: string))
            } else {
                // Instance field declaration
                let _ = self.matchk(&[TokenKind::Let]);
                let _ = self.matchk(&[TokenKind::Const]);

                if self.check(TokenKind::Identifier) || self.check(TokenKind::Type) {
                    let fname = if self.check(TokenKind::Type) {
                        let tok = self.advance();
                        tok.lexeme.clone()
                    } else {
                        self.consume_ident("Expect field name")?
                    };
                    let mut fty = "any".to_string();
                    if self.matchk(&[TokenKind::Colon]) {
                        fty = self.parse_type_name("Expect type name for field")?;
                    }
                    let init_expr = if self.matchk(&[TokenKind::Equal]) {
                        if self.check(TokenKind::Identifier)
                            || self.check(TokenKind::String)
                            || self.check(TokenKind::Number)
                            || self.check(TokenKind::True)
                            || self.check(TokenKind::False)
                            || self.check(TokenKind::Null)
                            || self.check(TokenKind::LeftBracket)
                            || self.check(TokenKind::LeftBrace)
                            || self.check(TokenKind::LeftParen)
                        {
                            Some(self.expression()?)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    // Accept either semicolon or comma as field separator (both optional)
                    self.matchk(&[TokenKind::Semicolon]);
                    self.matchk(&[TokenKind::Comma]);
                    instance_fields.push((
                        fname,
                        fty,
                        vis.unwrap_or(Visibility::Pub),
                        prop_decorators,
                        init_expr,
                    ));
                    continue;
                } else {
                    return Err(self.make_error("Expect 'fn' or field declaration"));
                }
            }

            let mname = if is_constructor {
                "__ctor__".to_string()
            } else if is_operator {
                format!("operator{}", operator_symbol.as_ref().unwrap())
            } else if self.matchk(&[TokenKind::New]) {
                "new".to_string()
            } else if self.matchk(&[TokenKind::Spawn]) {
                "spawn".to_string()
            } else if self.matchk(&[TokenKind::Emit]) {
                "emit".to_string()
            } else {
                self.consume_ident("Expect method name")?
            };

            // optional method-level generics: pub(super) fn name<T, U>
            let mut method_type_params: Vec<String> = Vec::new();
            if self.matchk(&[TokenKind::Less]) {
                while !self.check(TokenKind::Greater) {
                    let tname = self.consume_ident("Expect type parameter name")?;
                    method_type_params.push(tname);
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
                self.consume(TokenKind::Greater, "Expect '>' after type params")?;
            }

            self.consume(TokenKind::LeftParen, "Expect '('")?;
            let mut params: Vec<(String, Option<Expr>, Option<String>)> = Vec::new();
            if !self.check(TokenKind::RightParen) {
                loop {
                    let pname = self.consume_ident("Expect param")?;
                    let mut typ: Option<String> = None;
                    if self.matchk(&[TokenKind::Colon]) {
                        typ = Some(self.parse_type_name("Expect type name after ':'")?);
                    }
                    let mut def: Option<Expr> = None;
                    if self.matchk(&[TokenKind::Equal]) {
                        def = Some(self.expression()?);
                    }
                    params.push((pname, def, typ));
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(TokenKind::RightParen, "Expect ')'")?;

            // optional return type on method: : Type or -> Type
            let mut ret_type: Option<String> = None;
            if self.matchk(&[TokenKind::Colon])
                || (self.matchk(&[TokenKind::Minus]) && self.matchk(&[TokenKind::Greater]))
                || self.matchk(&[TokenKind::Arrow])
            {
                ret_type = Some(self.parse_type_name("Expect return type name")?);
            }

            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe = true;
            }

            // Handle abstract methods (no body)
            let body = if is_method_abstract {
                self.consume(TokenKind::Semicolon, "Expect ';' after abstract method")?;
                std::sync::Arc::new(Vec::new())
            } else {
                self.consume(TokenKind::LeftBrace, "Expect '{' for method body")?;
                std::sync::Arc::new(self.block()?)
            };

            let mut func = Function::new(
                mname,
                method_type_params,
                params,
                body,
                vis,
                ret_type,
                method_is_async,
            );
            func.decorators = method_decorators;
            func.is_static = is_static;
            func.is_abstract = is_method_abstract;
            func.is_constructor = is_constructor;
            func.is_getter = is_getter;
            func.is_setter = is_setter;
            func.is_operator = is_operator;
            func.operator_symbol = operator_symbol;
            func.is_unsafe = is_unsafe;

            if is_static {
                static_methods.push(func);
            } else {
                methods.push(func);
            }
        }
        self.consume(TokenKind::RightBrace, "Expect '}' after class body")?;

        // produce ClassDecl with implements and abstract flag
        let cd = ClassDecl {
            name: name.clone(),
            type_params,
            extends,
            implements,
            methods,
            static_methods,
            static_properties,
            fields: instance_fields,
            is_abstract,
            is_sealed,
            decorators: Vec::new(),
        };
        Ok(Stmt {
            kind: StmtKind::Class(cd, exp),
            span: start_span,
        })
    }

    pub(super) fn parse_operator_symbol(&mut self) -> Result<String, LangError> {
        match self.peek().kind {
            TokenKind::Plus => {
                self.advance();
                Ok("+".to_string())
            }
            TokenKind::Minus => {
                self.advance();
                Ok("-".to_string())
            }
            TokenKind::Star => {
                self.advance();
                Ok("*".to_string())
            }
            TokenKind::Slash => {
                self.advance();
                Ok("/".to_string())
            }
            TokenKind::Percent => {
                self.advance();
                Ok("%".to_string())
            }
            TokenKind::EqualEqual => {
                self.advance();
                Ok("==".to_string())
            }
            TokenKind::BangEqual => {
                self.advance();
                Ok("!=".to_string())
            }
            TokenKind::Less => {
                self.advance();
                Ok("<".to_string())
            }
            TokenKind::LessEqual => {
                self.advance();
                Ok("<=".to_string())
            }
            TokenKind::Greater => {
                self.advance();
                Ok(">".to_string())
            }
            TokenKind::GreaterEqual => {
                self.advance();
                Ok(">=".to_string())
            }
            TokenKind::LeftBracket => {
                self.advance();
                if self.matchk(&[TokenKind::RightBracket]) {
                    if self.matchk(&[TokenKind::Equal]) {
                        Ok("[]=".to_string())
                    } else {
                        Ok("[]".to_string())
                    }
                } else {
                    Err(self.make_error("Expect ']' after '['"))
                }
            }
            TokenKind::Identifier => {
                let ident = self.advance().lexeme.clone();
                if ident == "toString" {
                    Ok("toString".to_string())
                } else {
                    Err(self.make_error(&format!("Unknown operator: {}", ident)))
                }
            }
            _ => Err(self.make_error("Expected operator symbol")),
        }
    }
}
