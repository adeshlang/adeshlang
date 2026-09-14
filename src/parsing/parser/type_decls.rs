//! Type declaration parsing
//!
//! This module handles parsing of:
//! - extend declarations for type extensions
//! - interface declarations with method signatures  
//! - struct declarations with fields
//! - enum declarations with variants
//! - type alias declarations

use super::core::Parser;
use crate::parsing::ast::{
    EnumDecl, Expr, Function, InterfaceDecl, Stmt, StmtKind, StructDecl, TokenKind, TypeAliasDecl,
    Visibility,
};
use crate::parsing::error::LangError;

impl Parser {
    pub(super) fn extend_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let start_span = self.previous_span();
        // syntax: extend <name>? on TypeName { pub(super) fn ... }
        // optionally accept an identifier before 'on' as the extension name
        let mut name: Option<String> = None;
        if self.check(TokenKind::Identifier) {
            // peek ahead: if next token is 'On' then this identifier is the name
            let saved = self.current;
            let ident = self.consume_ident("Expect identifier")?;
            if self.matchk(&[TokenKind::On]) {
                name = Some(ident);
            } else {
                // not a name; rewind and expect 'on'
                self.current = saved;
            }
        }
        if name.is_none() {
            self.consume(TokenKind::On, "Expect 'on' after extend")?;
        }
        let target = self.consume_ident("Expect target type name after 'on'")?;
        self.consume(TokenKind::LeftBrace, "Expect '{' before extension body")?;
        let mut methods = Vec::new();
        while !self.check(TokenKind::RightBrace) {
            let mut is_unsafe = false;
            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe = true;
            }
            // optional async modifier on extension method
            let mut method_is_async = false;
            if self.matchk(&[TokenKind::Async]) {
                method_is_async = true;
                if self.matchk(&[TokenKind::Unsafe]) {
                    is_unsafe = true;
                }
            }
            self.consume(TokenKind::Fn, "Expect 'fn' for method")?;
            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe = true;
            }
            let mname = self.consume_ident("Expect method name")?;
            // optional method-level generics on extension methods
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
            self.consume(TokenKind::RightParen, "Expect ')' for params")?;
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
            self.consume(TokenKind::LeftBrace, "Expect '{' for method body")?;
            let body = self.block()?;
            let mut func = Function::new(
                mname,
                method_type_params,
                params,
                std::sync::Arc::new(body),
                None,
                ret_type,
                method_is_async,
            );
            func.is_unsafe = is_unsafe;
            methods.push(func);
        }
        self.consume(TokenKind::RightBrace, "Expect '}' after extension body")?;
        Ok(Stmt {
            kind: StmtKind::Extend(name, target, methods, exp),
            span: start_span,
        })
    }

    pub(super) fn interface_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let start_span = self.previous_span();
        let name = self.consume_ident("Expect interface name")?;
        // optional generics on interface
        let mut type_params: Vec<String> = Vec::new();
        if self.matchk(&[TokenKind::Less]) {
            while !self.check(TokenKind::Greater) {
                let tname = self.consume_ident("Expect type parameter name")?;
                type_params.push(tname);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
            self.consume(TokenKind::Greater, "Expect '>' after type params")?;
        }
        let mut super_interfaces: Vec<String> = Vec::new();
        if self.matchk(&[TokenKind::Extends, TokenKind::Implements]) {
            loop {
                let sname = self.consume_ident("Expect interface name in extends/implements")?;
                super_interfaces.push(sname);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
        }
        self.consume(TokenKind::LeftBrace, "Expect '{' before interface body")?;
        let mut methods = Vec::new();
        while !self.check(TokenKind::RightBrace) {
            // optional visibility modifier
            let mut vis: Option<Visibility> = None;
            if self.matchk(&[TokenKind::Private]) {
                vis = Some(Visibility::Priv);
            } else if self.matchk(&[TokenKind::Protected]) {
                vis = Some(Visibility::Protected);
            } else if self.matchk(&[TokenKind::Public]) {
                vis = Some(Visibility::Pub);
            }
            let mut is_unsafe = false;
            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe = true;
            }
            // optional async modifier on interface method signature
            let mut method_is_async = false;
            if self.matchk(&[TokenKind::Async]) {
                method_is_async = true;
                if self.matchk(&[TokenKind::Unsafe]) {
                    is_unsafe = true;
                }
            }
            self.consume(TokenKind::Fn, "Expect 'fn' for interface method")?;
            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe = true;
            }
            let mname = self.consume_ident("Expect method name")?;
            // optional method-level generics in interface signature
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
            // accept parameter names (optionally with type) in interface method sigs
            let mut params: Vec<(String, Option<Expr>, Option<String>)> = Vec::new();
            if !self.check(TokenKind::RightParen) {
                loop {
                    let pname = self.consume_ident("Expect param name")?;
                    let mut typ: Option<String> = None;
                    if self.matchk(&[TokenKind::Colon]) {
                        typ = Some(self.parse_type_name("Expect type name after ':'")?);
                    }
                    params.push((pname, None, typ));
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(TokenKind::RightParen, "Expect ')' for params")?;
            // optional return type on interface method: : Type or -> Type
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
            // expect terminating semicolon for interface method declarations
            self.consume(TokenKind::Semicolon, "Expect ';' after interface method")?;
            // interface methods have no body in this declaration
            let mut func = Function::new(
                mname,
                method_type_params,
                params,
                std::sync::Arc::new(Vec::new()),
                vis,
                ret_type,
                method_is_async,
            );
            func.is_unsafe = is_unsafe;
            methods.push(func);
        }
        self.consume(TokenKind::RightBrace, "Expect '}' after interface body")?;
        Ok(Stmt {
            kind: StmtKind::Interface(
                InterfaceDecl {
                    name,
                    type_params,
                    super_interfaces,
                    methods,
                },
                exp,
            ),
            span: start_span,
        })
    }

    pub(super) fn struct_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let name = self.consume_ident("Expect struct name")?;
        // optional generics on struct: Struct<T>
        let mut type_params: Vec<String> = Vec::new();
        if self.matchk(&[TokenKind::Less]) {
            while !self.check(TokenKind::Greater) {
                let tname = self.consume_ident("Expect type parameter name")?;
                type_params.push(tname);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
            self.consume(TokenKind::Greater, "Expect '>' after type params")?;
        }
        self.consume(TokenKind::LeftBrace, "Expect '{' before struct body")?;
        let mut fields: Vec<(String, String)> = Vec::new();
        while !self.check(TokenKind::RightBrace) {
            let fname = self.consume_ident("Expect field name")?;
            self.consume(TokenKind::Colon, "Expect ':' after field name")?;
            let fty = self.parse_type_name("Expect type name for field")?;
            fields.push((fname, fty));
            // accept either comma or semicolon as a field separator
            if !self.matchk(&[TokenKind::Comma]) && !self.matchk(&[TokenKind::Semicolon]) {
                break;
            }
        }
        self.consume(TokenKind::RightBrace, "Expect '}' after struct body")?;
        // Semicolon after struct is optional (common in many languages)
        self.matchk(&[TokenKind::Semicolon]);
        Ok(Stmt {
            kind: StmtKind::Struct(
                StructDecl {
                    name,
                    type_params,
                    fields,
                },
                exp,
            ),
            span: self.previous_span(),
        })
    }

    pub(super) fn enum_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let name = self.consume_ident("Expect enum name")?;
        self.consume(TokenKind::LeftBrace, "Expect '{' before enum body")?;
        let mut variants: Vec<(String, Option<String>)> = Vec::new();
        while !self.check(TokenKind::RightBrace) {
            let vname = self.consume_ident("Expect variant name")?;
            let mut payload: Option<String> = None;
            if self.matchk(&[TokenKind::LeftParen]) {
                // one payload identifier allowed
                let pname = self.consume_ident("Expect payload name")?;
                payload = Some(pname);
                self.consume(TokenKind::RightParen, "Expect ')' after variant payload")?;
            }
            variants.push((vname, payload));
            if !self.matchk(&[TokenKind::Comma]) {
                break;
            }
        }
        self.consume(TokenKind::RightBrace, "Expect '}' after enum body")?;
        Ok(Stmt {
            kind: StmtKind::Enum(EnumDecl { name, variants }, exp),
            span: self.previous_span(),
        })
    }

    pub(super) fn type_alias_decl(&mut self, exp: bool) -> Result<Stmt, LangError> {
        let name = self.consume_ident("Expect type name")?;
        // optional generics on type alias: type Foo<T, U> = { ... }
        let mut type_params: Vec<String> = Vec::new();
        if self.matchk(&[TokenKind::Less]) {
            while !self.check(TokenKind::Greater) {
                let tname = self.consume_ident("Expect type parameter name")?;
                type_params.push(tname);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
            self.consume(TokenKind::Greater, "Expect '>' after type params")?;
        }
        if !self.check(TokenKind::LeftBrace) {
            self.consume(TokenKind::Equal, "Expect '=' after type name")?;
        } else {
            let _ = self.matchk(&[TokenKind::Equal]);
        }
        let mut fields: Vec<(String, bool, String)> = Vec::new();
        let mut defaults: Vec<(String, Expr)> = Vec::new();

        if self.matchk(&[TokenKind::LeftBrace]) {
            // Record literal type alias: type Foo = { key: Type }
            while !self.check(TokenKind::RightBrace) {
                let fname = self.consume_ident("Expect field name in type literal")?;
                let mut optional = false;
                if self.matchk(&[TokenKind::Question]) {
                    optional = true;
                }
                self.consume(TokenKind::Colon, "Expect ':' after field name")?;
                let fty = self.parse_type_name("Expect type name for field")?;

                if self.matchk(&[TokenKind::Equal]) {
                    let expr = self.expression()?;
                    defaults.push((fname.clone(), expr));
                }

                fields.push((fname, optional, fty));
                if !self.matchk(&[TokenKind::Comma]) && !self.matchk(&[TokenKind::Semicolon]) {
                    break;
                }
            }
            self.consume(TokenKind::RightBrace, "Expect '}' after type literal")?;
        } else {
            // Type alias to existing/named type: type OpcodeKind = int;
            let target_type = self.parse_type_name("Expect type name after '='")?;
            fields.push(("__alias_target__".to_string(), false, target_type));
        }

        // Semicolon after type alias is optional
        self.matchk(&[TokenKind::Semicolon]);
        Ok(Stmt {
            kind: StmtKind::TypeAlias(
                TypeAliasDecl {
                    name,
                    type_params,
                    fields,
                    defaults,
                },
                exp,
            ),
            span: self.previous_span(),
        })
    }
}
