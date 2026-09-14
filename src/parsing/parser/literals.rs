//! Template literals and pattern matching
//!
//! This module handles:
//! - template literal parsing with ${...} interpolation and format specifiers
//! - match expression parsing
//! - pattern parsing (wildcard, variable, literals, or-patterns)

use super::core::Parser;
use crate::parsing::ast::{Expr, ExprKind, Pattern, TokenKind, Value};
use crate::parsing::error::LangError;

impl Parser {
    pub(super) fn parse_template_literal(&mut self, raw: &str) -> Result<Expr, LangError> {
        let span = self.previous_span(); // Span of the template token
        let mut parts: Vec<Expr> = Vec::new();
        let mut lit = String::new();
        let mut i = 0usize;
        let bs: Vec<char> = raw.chars().collect();
        while i < bs.len() {
            if bs[i] == '$' && i + 1 < bs.len() && bs[i + 1] == '{' {
                if !lit.is_empty() {
                    parts.push(Expr {
                        kind: ExprKind::Literal(Value::Str(lit.clone())),
                        span: span.clone(),
                    });
                    lit.clear();
                }
                i += 2;
                let start = i;
                let mut depth = 1i32;
                while i < bs.len() {
                    match bs[i] {
                        '{' => {
                            depth += 1;
                            i += 1;
                        }
                        '}' => {
                            depth -= 1;
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        '"' | '\'' => {
                            let quote = bs[i];
                            i += 1;
                            while i < bs.len() {
                                if bs[i] == '\\' {
                                    i += 2;
                                    continue;
                                }
                                if bs[i] == quote {
                                    i += 1;
                                    break;
                                }
                                i += 1;
                            }
                        }
                        '/' => {
                            if i + 1 < bs.len() && bs[i + 1] == '/' {
                                i += 2;
                                while i < bs.len() && bs[i] != '\n' {
                                    i += 1;
                                }
                            } else if i + 1 < bs.len() && bs[i + 1] == '*' {
                                i += 2;
                                while i + 1 < bs.len() {
                                    if bs[i] == '*' && bs[i + 1] == '/' {
                                        i += 2;
                                        break;
                                    }
                                    i += 1;
                                }
                            } else {
                                i += 1;
                            }
                        }
                        _ => {
                            i += 1;
                        }
                    }
                }
                if depth != 0 {
                    return Err(self.format_err(self.peek(), "Unterminated ${...} in template"));
                }
                let inner = bs[start..(i - 1)].iter().collect::<String>();

                // Check for format specifier: expr:format_spec
                // We need to find the last top-level colon (not inside strings or parens)
                let (expr_part, format_spec) = self.split_format_spec(&inner);

                let mut lx = crate::parsing::lexer::Lexer::new(&expr_part);
                let toks = lx.tokenize()?;
                let mut p = crate::parsing::parser::Parser::new(toks, self.module.clone());
                let mut expr = p.expression()?;

                // If there's a format specifier, wrap the expression
                if let Some(spec) = format_spec {
                    let s = expr.span.clone();
                    expr = Expr {
                        kind: ExprKind::Format(Box::new(expr), spec),
                        span: s,
                    };
                }

                parts.push(expr);
            } else {
                lit.push(bs[i]);
                i += 1;
            }
        }
        if !lit.is_empty() {
            parts.push(Expr {
                kind: ExprKind::Literal(Value::Str(lit.clone())),
                span: span.clone(),
            });
        }

        match parts.len() {
            0 => Ok(Expr {
                kind: ExprKind::Literal(Value::Str(String::new())),
                span,
            }),
            1 => Ok(parts.remove(0)),
            _ => Ok(Expr {
                kind: ExprKind::Call(
                    Box::new(Expr {
                        kind: ExprKind::Variable("concat".to_string()),
                        span: span.clone(),
                    }),
                    parts,
                    vec![],
                ),
                span,
            }),
        }
    }

    /// Split an expression string into (expression, optional format_spec)
    /// Handles nested parens, strings, and object literals
    pub(super) fn split_format_spec(&self, s: &str) -> (String, Option<String>) {
        let chars: Vec<char> = s.chars().collect();
        let mut depth_paren = 0i32;
        let mut depth_brace = 0i32;
        let mut depth_ternary = 0i32;
        let mut in_string = false;
        let mut string_char = '"';

        // Scan from start to find the last top-level colon (not inside ternary)
        let mut colon_pos: Option<usize> = None;
        let mut j = 0usize;

        while j < chars.len() {
            let c = chars[j];

            if in_string {
                if c == '\\' && j + 1 < chars.len() {
                    j += 2;
                    continue;
                }
                if c == string_char {
                    in_string = false;
                }
                j += 1;
                continue;
            }

            match c {
                '"' | '\'' => {
                    in_string = true;
                    string_char = c;
                }
                '(' => depth_paren += 1,
                ')' => depth_paren -= 1,
                '{' => depth_brace += 1,
                '}' => depth_brace -= 1,
                '?' => {
                    // Track ternary operator depth - ? at top level starts a ternary
                    if depth_paren == 0 && depth_brace == 0 {
                        depth_ternary += 1;
                    }
                }
                ':' => {
                    if depth_paren == 0 && depth_brace == 0 {
                        if depth_ternary > 0 {
                            // This colon belongs to a ternary operator, not a format spec
                            depth_ternary -= 1;
                        } else {
                            // Top-level colon - could be format specifier
                            colon_pos = Some(j);
                        }
                    }
                }
                _ => {}
            }
            j += 1;
        }

        match colon_pos {
            Some(pos) => {
                let expr_part = chars[..pos].iter().collect::<String>().trim().to_string();
                let format_spec = chars[pos + 1..]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                if format_spec.is_empty() {
                    (s.to_string(), None)
                } else {
                    (expr_part, Some(format_spec))
                }
            }
            None => (s.to_string(), None),
        }
    }

    pub(super) fn match_expr(&mut self) -> Result<Expr, LangError> {
        let span = self.previous_span();
        let old_allow_struct = self.allow_struct_literal;
        self.allow_struct_literal = false;
        let value = self.expression()?;
        self.allow_struct_literal = old_allow_struct;
        self.consume(TokenKind::LeftBrace, "Expect '{' after match value")?;
        let mut arms = Vec::new();
        while !self.check(TokenKind::RightBrace) && !self.is_end() {
            let pat = self.parse_pattern()?;
            self.consume(TokenKind::Arrow, "Expect '=>' after pattern")?;

            let expr = if self.check(TokenKind::LeftBrace) {
                let block_span = self.peek_span();
                self.consume(TokenKind::LeftBrace, "Expect '{' for match arm block")?;
                let body = self.block()?;
                // Wrap block as: (() => { ... })()
                let lambda = Expr {
                    kind: ExprKind::Fn(Vec::new(), std::sync::Arc::new(body), false),
                    span: block_span.clone(),
                };
                Expr {
                    kind: ExprKind::Call(Box::new(lambda), Vec::new(), Vec::new()),
                    span: block_span,
                }
            } else if self.matchk(&[TokenKind::Return]) {
                // `pattern => return expr` — wrap into an IIFE block so
                // the return propagates correctly through the match arm.
                let ret_span = self.previous_span();
                let ret_val = if self.check(TokenKind::Comma) || self.check(TokenKind::RightBrace) {
                    None
                } else {
                    Some(self.expression()?)
                };
                use crate::parsing::ast::{Stmt, StmtKind};
                let ret_stmt = Stmt {
                    kind: StmtKind::Return(ret_val),
                    span: ret_span.clone(),
                };
                let body = vec![ret_stmt];
                let lambda = Expr {
                    kind: ExprKind::Fn(Vec::new(), std::sync::Arc::new(body), false),
                    span: ret_span.clone(),
                };
                Expr {
                    kind: ExprKind::Call(Box::new(lambda), Vec::new(), Vec::new()),
                    span: ret_span,
                }
            } else {
                self.expression()?
            };
            arms.push((pat, expr));
            if !self.matchk(&[TokenKind::Comma]) {
                break;
            }
        }
        self.consume(TokenKind::RightBrace, "Expect '}' after match arms")?;
        Ok(Expr {
            kind: ExprKind::Match(Box::new(value), arms),
            span,
        })
    }

    pub(super) fn parse_pattern(&mut self) -> Result<Pattern, LangError> {
        if self.matchk(&[TokenKind::Underscore]) {
            Ok(Pattern::Wildcard)
        } else if self.matchk(&[TokenKind::Identifier]) {
            let mut name = self.prev().lexeme.clone();
            if self.check(TokenKind::Colon) && self.peek_next_kind(TokenKind::Colon) {
                self.advance();
                self.advance();
                let variant = self.consume_ident("Expect variant name after '::'")?;
                name = format!("{}::{}", name, variant);
            } else if self.matchk(&[TokenKind::Dot]) {
                let variant = self.consume_ident("Expect variant name after '.'")?;
                name = format!("{}.{}", name, variant);
            }
            if self.matchk(&[TokenKind::LeftParen]) {
                let mut patterns = Vec::new();
                if !self.check(TokenKind::RightParen) {
                    loop {
                        patterns.push(self.parse_pattern()?);
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(TokenKind::RightParen, "Expect ')' after pattern args")?;
                Ok(Pattern::EnumVariant(name, patterns))
            } else if name == "None" || name.contains("::") || name.contains('.') {
                Ok(Pattern::EnumVariant(name, Vec::new()))
            } else {
                Ok(Pattern::Variable(name))
            }
        } else if self.matchk(&[TokenKind::Number]) {
            let n: f64 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::Number(n)))
        // Handle typed integer literals in patterns (unsigned)
        } else if self.matchk(&[TokenKind::U8Lit]) {
            let n: u8 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::U8(n)))
        } else if self.matchk(&[TokenKind::U16Lit]) {
            let n: u16 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::U16(n)))
        } else if self.matchk(&[TokenKind::U32Lit]) {
            let n: u32 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::U32(n)))
        } else if self.matchk(&[TokenKind::U64Lit]) {
            let n: u64 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::U64(n)))
        } else if self.matchk(&[TokenKind::U128Lit]) {
            let n: u128 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::U128(n)))
        // Handle typed integer literals in patterns (signed)
        } else if self.matchk(&[TokenKind::I8Lit]) {
            let n: i8 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::I8(n)))
        } else if self.matchk(&[TokenKind::I16Lit]) {
            let n: i16 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::I16(n)))
        } else if self.matchk(&[TokenKind::I32Lit]) {
            let n: i32 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::I32(n)))
        } else if self.matchk(&[TokenKind::I64Lit]) {
            let n: i64 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::I64(n)))
        } else if self.matchk(&[TokenKind::I128Lit]) {
            let n: i128 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::I128(n)))
        // Handle typed float literals in patterns
        } else if self.matchk(&[TokenKind::F32Lit]) {
            let n: f32 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::F32(n)))
        } else if self.matchk(&[TokenKind::F64Lit]) {
            let n: f64 = self.prev().lexeme.parse().unwrap();
            Ok(Pattern::Literal(Value::F64(n)))
        } else if self.matchk(&[TokenKind::String]) {
            let s = self.prev().lexeme.clone();
            Ok(Pattern::Literal(Value::Str(s)))
        } else if self.matchk(&[TokenKind::True]) {
            Ok(Pattern::Literal(Value::Bool(true)))
        } else if self.matchk(&[TokenKind::False]) {
            Ok(Pattern::Literal(Value::Bool(false)))
        } else if self.matchk(&[TokenKind::Null]) {
            Ok(Pattern::Literal(Value::Null))
        } else if self.matchk(&[TokenKind::LeftParen]) {
            let left = self.parse_pattern()?;
            self.consume(TokenKind::Pipe, "Expect '|' in or pattern")?;
            let right = self.parse_pattern()?;
            self.consume(TokenKind::RightParen, "Expect ')' after or pattern")?;
            Ok(Pattern::Or(Box::new(left), Box::new(right)))
        } else {
            Err(self.format_err(self.peek(), "Expect pattern"))
        }
    }

    // Parse a type annotation into a string. Supports identifiers and a
    // simple function-type grammar like `(t1,t2)->ret` where '->' is expressed
    // as the '-' and '>' tokens in the lexer. Also supports tuple types like `(t1, t2, t3)`.
    // Extended to support:
    //   - [T]        - dynamic array
    //   - [T;N]      - fixed-size array
    //   - [T;raw]    - raw dynamic array (no metadata)
    //   - [T;N;raw]  - raw fixed-size array
    //   - &[T]       - slice type
    //   - vecN<T>    - SIMD vector types (vec2, vec3, vec4, etc.)
    //   - [N,M]T     - pattern/shape-typed arrays (matrices)
}
