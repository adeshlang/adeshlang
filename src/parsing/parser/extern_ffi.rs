//! External function interface (FFI) parsing
//!
//! Handles parsing of extern declarations for interfacing with C and other languages.
//! Supports both single function and block forms.

use super::core::Parser;
use crate::parsing::ast::{ExternFunctionDecl, Stmt, StmtKind, TokenKind};
use crate::parsing::error::LangError;

impl Parser {
    /// Parse extern declaration: extern "ABI" pub(super) fn ... or extern "ABI" { pub(super) fn ...; pub(super) fn ...; }
    pub(super) fn extern_decl(&mut self) -> Result<Stmt, LangError> {
        // extern "ABI" must come next
        let abi = if self.check(TokenKind::String) {
            self.consume_string("Expect ABI string after 'extern'")?
        } else {
            return Err(
                self.make_error("Expect string literal (ABI) after 'extern', e.g., extern \"C\"")
            );
        };

        // Check for block form: extern "C" { ... }
        if self.matchk(&[TokenKind::LeftBrace]) {
            let mut functions = Vec::new();
            while !self.check(TokenKind::RightBrace) && !self.is_end() {
                if self.matchk(&[TokenKind::Fn]) {
                    functions.push(self.parse_extern_fn_signature(&abi)?);
                } else {
                    return Err(self.make_error("Expect 'fn' in extern block"));
                }
                // semicolon optional in blocks
                self.matchk(&[TokenKind::Semicolon]);
            }
            self.consume(TokenKind::RightBrace, "Expect '}' after extern block")?;
            return Ok(Stmt {
                kind: StmtKind::ExternBlock { abi, functions },
                span: self.previous_span(),
            });
        }

        // Single function form: extern "C" pub(super) fn ...
        if self.matchk(&[TokenKind::Fn]) {
            let func = self.parse_extern_fn_signature(&abi)?;
            self.consume(TokenKind::Semicolon, "Expect ';' after extern function")?;
            return Ok(Stmt {
                kind: StmtKind::ExternFunction(func),
                span: self.previous_span(),
            });
        }

        Err(self.make_error("Expect 'fn' or '{' after extern ABI"))
    }

    /// Parse a single extern function signature: pub(super) fn name(params) -> ret_type
    pub(super) fn parse_extern_fn_signature(
        &mut self,
        abi: &str,
    ) -> Result<ExternFunctionDecl, LangError> {
        let name = self.consume_ident("Expect function name after 'fn'")?;

        self.consume(TokenKind::LeftParen, "Expect '(' after function name")?;
        let mut params: Vec<(String, String)> = Vec::new();

        if !self.check(TokenKind::RightParen) {
            loop {
                let param_name = self.consume_ident("Expect parameter name")?;
                self.consume(TokenKind::Colon, "Expect ':' after parameter name")?;
                let param_type = self.parse_type_name("Expect parameter type")?;
                params.push((param_name, param_type));

                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
        }

        self.consume(TokenKind::RightParen, "Expect ')' after parameters")?;

        // Parse return type if present
        let ret_type = if self.matchk(&[TokenKind::Minus]) {
            self.consume(TokenKind::Greater, "Expect '>' after '-' in return type")?;
            self.parse_type_name("Expect return type")?
        } else {
            "void".to_string()
        };

        Ok(ExternFunctionDecl {
            name,
            params,
            ret_type,
            abi: abi.to_string(),
        })
    }
}
