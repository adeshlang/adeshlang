//! Core parser structure and utilities
//!
//! This module contains the Parser struct definition and utility methods
//! for token manipulation, error formatting, and basic parsing operations.

use crate::parsing::ast::{Span, TokenKind};
use crate::parsing::error::{ErrorKind, LangError};
use crate::parsing::lexer::Token;

/// The main parser struct that maintains parsing state
pub struct Parser {
    pub(super) tokens: Vec<Token>,
    pub(super) current: usize,
    pub(super) module: Option<String>,
    pub(super) pending_gt: usize,
    pub(super) allow_struct_literal: bool,
    pub(super) recursion_depth: usize,
    pub(super) max_recursion_depth: usize,
}

impl Parser {
    /// Create a new parser from a list of tokens
    pub fn new(tokens: Vec<Token>, module: Option<String>) -> Self {
        Self {
            tokens,
            current: 0,
            module,
            pending_gt: 0,
            allow_struct_literal: true,
            recursion_depth: 0,
            max_recursion_depth: 1000,
        }
    }

    /// Enter recursion frame and check recursion limit
    pub(super) fn enter_recursion(&mut self) -> Result<(), LangError> {
        self.recursion_depth += 1;
        if self.recursion_depth > self.max_recursion_depth {
            return Err(self.make_error("Recursion limit exceeded during parsing"));
        }
        Ok(())
    }

    /// Leave recursion frame
    pub(super) fn leave_recursion(&mut self) {
        if self.recursion_depth > 0 {
            self.recursion_depth -= 1;
        }
    }

    /// Create a Span from a token's location information
    pub(super) fn span_from(&self, token: &Token) -> Span {
        Span {
            line: token.line,
            col: token.col,
            line_text: token.line_text.clone(),
        }
    }

    /// Get the span of the current token being peeked at
    #[allow(dead_code)]
    pub(super) fn peek_span(&self) -> Span {
        if self.is_end() {
            if let Some(last) = self.tokens.last() {
                return self.span_from(last);
            }
            return Span::default();
        }
        self.span_from(&self.tokens[self.current])
    }

    /// Get the span of the previous token
    pub(super) fn previous_span(&self) -> Span {
        if self.current > 0 {
            self.span_from(&self.tokens[self.current - 1])
        } else {
            Span::default()
        }
    }

    /// Match and consume one of the given token kinds
    pub(super) fn matchk(&mut self, kinds: &[TokenKind]) -> bool {
        for k in kinds {
            if self.check(k.clone()) {
                self.advance();
                return true;
            }
        }
        false
    }

    /// Check if the current Question token is part of a ternary expression `cond ? then : else`.
    pub(super) fn is_ternary_question(&self) -> bool {
        if !self.check(TokenKind::Question) {
            return false;
        }
        let mut idx = self.current + 1;
        if idx >= self.tokens.len() {
            return false;
        }
        // If the token immediately after `?` is `:`, `;`, `,`, `)`, `]`, `}`, `.`, `?.`, it is postfix `?`
        match &self.tokens[idx].kind {
            TokenKind::Colon
            | TokenKind::Semicolon
            | TokenKind::Comma
            | TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
            | TokenKind::Dot
            | TokenKind::QuestionDot => return false,
            _ => {}
        }
        // Scan ahead for an unnested `:` before `;` or EOF
        let mut paren_depth: i32 = 0;
        let mut bracket_depth: i32 = 0;
        let mut brace_depth: i32 = 0;
        let mut nested_ternaries: i32 = 0;
        while idx < self.tokens.len() {
            match &self.tokens[idx].kind {
                TokenKind::LeftParen => paren_depth += 1,
                TokenKind::RightParen => {
                    if paren_depth == 0 {
                        return false;
                    }
                    paren_depth -= 1;
                }
                TokenKind::LeftBracket => bracket_depth += 1,
                TokenKind::RightBracket => {
                    if bracket_depth == 0 {
                        return false;
                    }
                    bracket_depth -= 1;
                }
                TokenKind::LeftBrace => brace_depth += 1,
                TokenKind::RightBrace => {
                    if brace_depth == 0 {
                        return false;
                    }
                    brace_depth -= 1;
                }
                TokenKind::Semicolon | TokenKind::Eof => {
                    if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                        return false;
                    }
                }
                TokenKind::Question
                    if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
                {
                    if idx + 1 < self.tokens.len()
                        && !matches!(
                            &self.tokens[idx + 1].kind,
                            TokenKind::Colon
                                | TokenKind::Semicolon
                                | TokenKind::Comma
                                | TokenKind::RightParen
                                | TokenKind::RightBracket
                                | TokenKind::RightBrace
                        )
                    {
                        nested_ternaries += 1;
                    }
                }
                TokenKind::Colon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    if nested_ternaries > 0 {
                        nested_ternaries -= 1;
                    } else {
                        return true;
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        false
    }

    /// Consume a specific token kind or return an error
    pub(super) fn consume(&mut self, k: TokenKind, msg: &str) -> Result<&Token, LangError> {
        if self.check(k) {
            Ok(self.advance())
        } else {
            Err(self.format_err(self.peek(), msg))
        }
    }

    /// Consume an identifier token (including special keywords that can be used as identifiers)
    pub(super) fn consume_ident(&mut self, msg: &str) -> Result<String, LangError> {
        if self.check(TokenKind::Identifier)
            || self.check(TokenKind::Type)
            || self.check(TokenKind::Get)
            || self.check(TokenKind::Set)
            || self.check(TokenKind::Underscore)
            || self.check(TokenKind::Compile)
            || self.check(TokenKind::Public)
            || self.check(TokenKind::Free)
            || self.check(TokenKind::Alloc)
            || self.check(TokenKind::Raw)
            || self.check(TokenKind::Default)
            || self.check(TokenKind::From)
            || self.check(TokenKind::As)
            || self.check(TokenKind::Share)
            || self.check(TokenKind::Weak)
            || self.check(TokenKind::Strong)
            || self.check(TokenKind::SelfKeyword)
            || self.check(TokenKind::Uint)
            || self.check(TokenKind::Int)
            || self.check(TokenKind::Test)
        {
            Ok(self.advance().lexeme.clone())
        } else {
            Err(self.format_err(self.peek(), msg))
        }
    }

    /// Consume a string literal token
    pub(super) fn consume_string(&mut self, msg: &str) -> Result<String, LangError> {
        if self.check(TokenKind::String) {
            Ok(self.advance().lexeme.clone())
        } else {
            Err(self.format_err(self.peek(), msg))
        }
    }

    /// Create an error from the current parsing position
    pub(super) fn make_error(&self, msg: &str) -> LangError {
        let token = self.peek();
        self.format_err(token, msg)
    }

    /// Format an error with token location information
    pub(super) fn format_err(&self, token: &Token, msg: &str) -> LangError {
        let mut e = LangError::new(
            ErrorKind::Parse,
            msg.to_string(),
            token.line,
            token.col,
            token.line_text.clone(),
        )
        .with_auto_hints();
        if let Some(m) = &self.module {
            e = e.with_file(m.clone());
        }
        e
    }

    /// Check if the current token matches the given kind
    pub(super) fn check(&self, k: TokenKind) -> bool {
        if self.is_end() {
            return false;
        }
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(&k)
    }

    /// Advance to the next token and return the previous one
    pub(super) fn advance(&mut self) -> &Token {
        if !self.is_end() {
            self.current += 1;
        }
        self.prev()
    }

    /// Check if we've reached the end of tokens
    pub(super) fn is_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    /// Peek at the current token without consuming it
    pub(super) fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    /// Check if the next token matches the given kind (lookahead by one)
    pub(super) fn peek_next_kind(&self, k: TokenKind) -> bool {
        let next = self.current + 1;
        if next >= self.tokens.len() {
            return false;
        }
        std::mem::discriminant(&self.tokens[next].kind) == std::mem::discriminant(&k)
    }

    /// Get the previous token
    pub(super) fn prev(&self) -> &Token {
        &self.tokens[self.current - 1]
    }
}
