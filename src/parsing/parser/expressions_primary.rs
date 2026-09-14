//! Primary expression parsing
//!
//! This module handles parsing of:
//! - function literals (fn, async fn, arrow functions)
//! - match expressions
//! - literals (bool, null, numbers, bigint, typed numerics, strings, chars)
//! - template literals
//! - identifiers and variables
//! - struct literals
//! - special keywords (this, self, super, type, set, get)
//! - grouping and tuples
//! - arrays and objects
//! - set literals
//! - new expressions (constructors)
//! - throw expressions

use super::core::Parser;
use crate::parsing::ast::{Expr, ExprKind, Span, Stmt, StmtKind, TokenKind, Value};
use crate::parsing::error::LangError;

/// Helper function to parse integer numeric strings with different bases (hex, binary, octal, decimal).
/// This is used for typed integer literals (u8, i32, etc.) and BigInt.
/// Separate from f64 parsing in primary() which handles Number tokens without type suffixes.
fn parse_numeric_with_base<T>(s: &str) -> Result<T, std::num::ParseIntError>
where
    T: std::str::FromStr<Err = std::num::ParseIntError>
        + num_traits::Num<FromStrRadixErr = std::num::ParseIntError>,
{
    if s.starts_with("0x") || s.starts_with("0X") {
        // Hexadecimal
        let hex_str = s[2..].replace('_', "");
        return T::from_str_radix(&hex_str, 16);
    } else if s.starts_with("0b") || s.starts_with("0B") {
        // Binary
        let bin_str = s[2..].replace('_', "");
        return T::from_str_radix(&bin_str, 2);
    } else if s.starts_with("0o") || s.starts_with("0O") {
        // Octal
        let oct_str = s[2..].replace('_', "");
        return T::from_str_radix(&oct_str, 8);
    } else {
        // Decimal with possible underscores
        let s_clean = s.replace('_', "");
        return s_clean.parse::<T>();
    }
}

impl Parser {
    pub(super) fn primary(&mut self) -> Result<Expr, LangError> {
        use crate::parsing::ast::TokenKind::*;
        // Parse a (possibly compound) type name used in annotations.
        // Supports simple identifiers (e.g., `int`) or a function-type
        // syntax like `(int, float)->int` (parsed from tokens '(' ... ')' '-' '>').
        // The returned value is the textual representation, which will be
        // interpreted later by the type parser.
        // support function expressions: `fn(a, b) { ... }` and `async` / `unsafe` modifier
        let mut is_async = false;
        let mut is_unsafe_lambda = false;
        if self.matchk(&[TokenKind::Unsafe]) {
            if self.check(TokenKind::Fn)
                || (self.check(TokenKind::Async) && self.peek_next_kind(TokenKind::Fn))
            {
                is_unsafe_lambda = true;
                if self.matchk(&[TokenKind::Async]) {
                    is_async = true;
                }
            } else if self.matchk(&[TokenKind::LeftBrace]) {
                // unsafe { ... } block expression: evaluate block inside unsafe context
                let start_span = self.previous_span();
                let body = self.block()?;
                let span = self.previous_span();
                let fn_expr = Expr {
                    kind: ExprKind::Fn(
                        vec![],
                        std::sync::Arc::new(vec![Stmt {
                            kind: StmtKind::UnsafeBlock(Box::new(Stmt {
                                kind: StmtKind::Block(body),
                                span: span.clone(),
                            })),
                            span: span.clone(),
                        }]),
                        false,
                    ),
                    span: span.clone(),
                };
                return Ok(Expr {
                    kind: ExprKind::Call(Box::new(fn_expr), vec![], vec![]),
                    span: start_span,
                });
            } else {
                return Err(self.make_error("Expected 'fn' or '{' after 'unsafe'"));
            }
        }
        if self.matchk(&[TokenKind::Async]) {
            if self.matchk(&[TokenKind::LeftBrace]) {
                let start_span = self.previous_span();
                let body = self.block()?;
                let span = self.previous_span();
                let fn_expr = Expr {
                    kind: ExprKind::Fn(vec![], std::sync::Arc::new(body), true),
                    span: span.clone(),
                };
                return Ok(Expr {
                    kind: ExprKind::Call(Box::new(fn_expr), vec![], vec![]),
                    span: start_span,
                });
            }
            is_async = true;
            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe_lambda = true;
            }
        }
        if self.matchk(&[Fn]) {
            // anonymous/function literal: parse params and body (no name)
            self.consume(LeftParen, "Expect '(' after fn")?;
            let mut params: Vec<(
                std::string::String,
                Option<Expr>,
                Option<std::string::String>,
            )> = Vec::new();
            if !self.check(RightParen) {
                loop {
                    // Check for rest parameter: ...paramName
                    let is_rest = self.matchk(&[TokenKind::DotDotDot]);
                    let pname = self.consume_ident("Expect param")?;

                    // For rest parameters, append "..." prefix to distinguish them
                    let final_name = if is_rest {
                        format!("...{}", pname)
                    } else {
                        pname
                    };

                    let mut typ: Option<std::string::String> = None;
                    if self.matchk(&[TokenKind::Colon]) {
                        typ = Some(self.parse_type_name("Expect type name after ':'")?);
                    }
                    let mut def: Option<Expr> = None;
                    if self.matchk(&[TokenKind::Equal]) {
                        def = Some(self.expression()?);
                    }
                    params.push((final_name, def, typ));
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(RightParen, "Expect ')' after params")?;
            // Optional return type annotation for lambdas
            if self.matchk(&[TokenKind::Colon]) {
                let _ = self.parse_type_name("Expect return type after ':'")?;
            } else if (self.matchk(&[TokenKind::Minus]) && self.matchk(&[TokenKind::Greater]))
                || self.matchk(&[TokenKind::Arrow])
            {
                let _ = self.parse_type_name("Expect return type after '->'")?;
            }
            if self.matchk(&[TokenKind::Unsafe]) {
                is_unsafe_lambda = true;
            }
            // two alternatives for function literal body:
            //  - block: { ... }
            //  - concise expression: => expr  (expression-bodied lambda)
            if self.matchk(&[TokenKind::LeftBrace]) {
                let body = self.block()?;
                let span = self.previous_span();
                let final_body = if is_unsafe_lambda {
                    vec![Stmt {
                        kind: StmtKind::UnsafeBlock(Box::new(Stmt {
                            kind: StmtKind::Block(body),
                            span: span.clone(),
                        })),
                        span: span.clone(),
                    }]
                } else {
                    body
                };
                // Optimize: wrap body in Arc to avoid cloning
                return Ok(Expr {
                    kind: ExprKind::Fn(params, std::sync::Arc::new(final_body), is_async),
                    span,
                });
            } else if self.matchk(&[TokenKind::Arrow]) {
                // parse a single expression and wrap it as a return statement
                let e = self.expression()?;
                let mut body = vec![Stmt {
                    kind: StmtKind::Return(Some(e)),
                    span: self.previous_span(),
                }];
                let span = self.previous_span();
                if is_unsafe_lambda {
                    body = vec![Stmt {
                        kind: StmtKind::UnsafeBlock(Box::new(Stmt {
                            kind: StmtKind::Block(body),
                            span: span.clone(),
                        })),
                        span: span.clone(),
                    }];
                }
                // Optimize: wrap body in Arc
                return Ok(Expr {
                    kind: ExprKind::Fn(params, std::sync::Arc::new(body), is_async),
                    span,
                });
            } else {
                return Err(self.format_err(self.peek(), "Expect '{' or '=>' after fn(...)"));
            }
        }
        if self.matchk(&[TokenKind::Match]) {
            return self.match_expr();
        }
        if self.matchk(&[False]) {
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::Bool(false)),
                span,
            });
        }
        if self.matchk(&[True]) {
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::Bool(true)),
                span,
            });
        }
        if self.matchk(&[Null]) {
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::Null),
                span,
            });
        }
        if self.matchk(&[Number]) {
            let lex = self.prev().lexeme.clone();
            let span = self.previous_span();

            // Preserve precision for integer literals that exceed f64's exact range.
            let is_hex_or_bin_or_oct = lex.starts_with("0x")
                || lex.starts_with("0X")
                || lex.starts_with("0b")
                || lex.starts_with("0B")
                || lex.starts_with("0o")
                || lex.starts_with("0O");
            let is_integer_lexeme = is_hex_or_bin_or_oct
                || (!lex.contains('.') && !lex.contains('e') && !lex.contains('E'));
            if is_integer_lexeme {
                let parsed_u128 = if lex.starts_with("0x") || lex.starts_with("0X") {
                    u128::from_str_radix(&lex[2..].replace('_', ""), 16).ok()
                } else if lex.starts_with("0b") || lex.starts_with("0B") {
                    u128::from_str_radix(&lex[2..].replace('_', ""), 2).ok()
                } else if lex.starts_with("0o") || lex.starts_with("0O") {
                    u128::from_str_radix(&lex[2..].replace('_', ""), 8).ok()
                } else {
                    lex.replace('_', "").parse::<u128>().ok()
                };

                if let Some(v) = parsed_u128 {
                    // Keep unsuffixed integer literals neutral so the type checker can
                    // fit them to the expected numeric type at the use site.
                    if v <= i64::MAX as u128 {
                        return Ok(Expr {
                            kind: ExprKind::Literal(Value::Number(v as f64)),
                            span,
                        });
                    }

                    return Ok(Expr {
                        kind: ExprKind::Literal(Value::BigInt(num_bigint::BigInt::from(v))),
                        span,
                    });
                }
            }

            // Handle different numeric literal formats
            let value = if lex.starts_with("0x") || lex.starts_with("0X") {
                // Hexadecimal
                let hex_str = lex[2..].replace('_', "");
                match u64::from_str_radix(&hex_str, 16) {
                    Ok(n) => n as f64,
                    Err(_) => {
                        return Err(self.format_err(self.prev(), "Invalid hexadecimal literal"));
                    }
                }
            } else if lex.starts_with("0b") || lex.starts_with("0B") {
                // Binary
                let bin_str = lex[2..].replace('_', "");
                match u64::from_str_radix(&bin_str, 2) {
                    Ok(n) => n as f64,
                    Err(_) => return Err(self.format_err(self.prev(), "Invalid binary literal")),
                }
            } else if lex.starts_with("0o") || lex.starts_with("0O") {
                // Octal
                let oct_str = lex[2..].replace('_', "");
                match u64::from_str_radix(&oct_str, 8) {
                    Ok(n) => n as f64,
                    Err(_) => return Err(self.format_err(self.prev(), "Invalid octal literal")),
                }
            } else {
                // Decimal (with possible underscores)
                let dec_str = lex.replace('_', "");
                match dec_str.parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Err(self.format_err(self.prev(), "Invalid numeric literal")),
                }
            };

            let parsed_value = if !is_integer_lexeme {
                // Rust-like default: unsuffixed decimal/exponent literals are f64.
                Value::F64(value)
            } else {
                // Integer lexemes are returned above; keep this as a defensive fallback.
                Value::U8(value as u8)
            };

            return Ok(Expr {
                kind: ExprKind::Literal(parsed_value),
                span,
            });
        }
        if self.matchk(&[TokenKind::ComplexLit]) {
            let lex = self.prev().lexeme.trim_end_matches('j').replace('_', "");
            let imag: f64 = lex
                .parse()
                .map_err(|_| self.format_err(self.prev(), "Invalid complex literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::Complex(0.0, imag)),
                span,
            });
        }
        if self.matchk(&[TokenKind::BigIntLit]) {
            let lex = self.prev().lexeme.clone();
            let digits = lex.trim_end_matches('n');

            // Handle different bases for BigInt
            let bi = if digits.starts_with("0x") || digits.starts_with("0X") {
                // Hexadecimal BigInt
                let hex_str = digits[2..].replace('_', "");
                num_bigint::BigInt::parse_bytes(hex_str.as_bytes(), 16)
            } else if digits.starts_with("0b") || digits.starts_with("0B") {
                // Binary BigInt
                let bin_str = digits[2..].replace('_', "");
                num_bigint::BigInt::parse_bytes(bin_str.as_bytes(), 2)
            } else if digits.starts_with("0o") || digits.starts_with("0O") {
                // Octal BigInt
                let oct_str = digits[2..].replace('_', "");
                num_bigint::BigInt::parse_bytes(oct_str.as_bytes(), 8)
            } else {
                // Decimal BigInt
                let dec_str = digits.replace('_', "");
                num_bigint::BigInt::parse_bytes(dec_str.as_bytes(), 10)
            };

            if let Some(bi) = bi {
                let span = self.previous_span();
                return Ok(Expr {
                    kind: ExprKind::Literal(Value::BigInt(bi)),
                    span,
                });
            } else {
                return Err(self.format_err(self.prev(), "Invalid BigInt literal"));
            }
        }
        // Handle typed integer literals (unsigned)
        if self.matchk(&[TokenKind::U8Lit]) {
            let s = self.prev().lexeme.clone();
            let n: u8 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid u8 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::U8(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::U16Lit]) {
            let s = self.prev().lexeme.clone();
            let n: u16 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid u16 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::U16(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::U32Lit]) {
            let s = self.prev().lexeme.clone();
            let n: u32 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid u32 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::U32(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::U64Lit]) {
            let s = self.prev().lexeme.clone();
            let n: u64 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid u64 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::U64(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::U128Lit]) {
            let s = self.prev().lexeme.clone();
            let n: u128 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid u128 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::U128(n)),
                span,
            });
        }
        // Handle typed integer literals (signed)
        if self.matchk(&[TokenKind::I8Lit]) {
            let s = self.prev().lexeme.clone();
            let n: i8 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid i8 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::I8(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::I16Lit]) {
            let s = self.prev().lexeme.clone();
            let n: i16 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid i16 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::I16(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::I32Lit]) {
            let s = self.prev().lexeme.clone();
            let n: i32 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid i32 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::I32(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::I64Lit]) {
            let s = self.prev().lexeme.clone();
            let n: i64 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid i64 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::I64(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::I128Lit]) {
            let s = self.prev().lexeme.clone();
            let n: i128 = parse_numeric_with_base(&s)
                .map_err(|_| self.format_err(self.prev(), "Invalid i128 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::I128(n)),
                span,
            });
        }
        // Handle typed float literals
        if self.matchk(&[TokenKind::F32Lit]) {
            let s = self.prev().lexeme.clone();
            let n: f32 = s
                .parse()
                .map_err(|_| self.format_err(self.prev(), "Invalid f32 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::F32(n)),
                span,
            });
        }
        if self.matchk(&[TokenKind::F64Lit]) {
            let s = self.prev().lexeme.clone();
            let n: f64 = s
                .parse()
                .map_err(|_| self.format_err(self.prev(), "Invalid f64 literal"))?;
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::F64(n)),
                span,
            });
        }
        if self.matchk(&[String]) {
            let s = self.prev().lexeme.clone();
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::Str(s)),
                span,
            });
        }
        if self.matchk(&[TokenKind::Template]) {
            let raw = self.prev().lexeme.clone();
            let e = self.parse_template_literal(&raw)?;
            return Ok(e);
        }
        if self.matchk(&[TokenKind::CharLit]) {
            let s = self.prev().lexeme.clone();
            let ch = s.chars().next().unwrap_or('\0');
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Literal(Value::Char(ch)),
                span,
            });
        }
        // Support JS-style concise arrow: `x => expr` (single ident) or `(a, b) => expr`
        if self.matchk(&[Identifier]) {
            let ident = self.prev().lexeme.clone();
            let span = self.previous_span();
            if self.matchk(&[TokenKind::Arrow]) {
                // single-identifier arrow: build pub(super) fn with one param and expression body
                let param = (ident.clone(), None, None);
                let e = self.expression()?;
                let body = vec![Stmt {
                    kind: StmtKind::Return(Some(e)),
                    span: self.previous_span(),
                }];
                // Optimize: wrap in Arc
                return Ok(Expr {
                    kind: ExprKind::Fn(vec![param], std::sync::Arc::new(body), is_async),
                    span,
                });
            }
            // Parse struct literal if this identifier is followed by '{'
            // Try to parse as struct literal first, but fall back to variable if it fails
            if self.allow_struct_literal && self.check(TokenKind::LeftBrace) {
                // This looks like a struct literal: StructName { ... }
                self.consume(TokenKind::LeftBrace, "Expect '{' after struct name")?;
                let mut kv = Vec::new();
                if !self.check(TokenKind::RightBrace) {
                    loop {
                        let key = if self.check(TokenKind::Identifier) {
                            self.advance().lexeme.clone()
                        } else {
                            return Err(
                                self.format_err(self.peek(), "Expect key in struct literal")
                            );
                        };
                        self.consume(TokenKind::Colon, "Expect ':' after key")?;
                        // Use expression() with struct literals temporarily disabled to
                        // support arithmetic/method-call values like `this.x + other.x`
                        // while avoiding ambiguity from nested struct literals.
                        let old_struct = self.allow_struct_literal;
                        self.allow_struct_literal = false;
                        let v = self.expression()?;
                        self.allow_struct_literal = old_struct;
                        kv.push((key, v));
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                        if self.check(TokenKind::RightBrace) {
                            break;
                        }
                    }
                }
                self.consume(TokenKind::RightBrace, "Expect '}' after struct literal")?;
                return Ok(Expr {
                    kind: ExprKind::StructLiteral(ident, kv),
                    span,
                });
            }
            return Ok(Expr {
                kind: ExprKind::Variable(ident),
                span,
            });
        }
        // allow reserved type keywords ('type', 'int', 'uint') to be used as variables/identifiers in expression context
        if self.matchk(&[TokenKind::Type, TokenKind::Int, TokenKind::Uint]) {
            let ident = self.prev().lexeme.clone();
            let span = self.previous_span();
            if self.matchk(&[TokenKind::Arrow]) {
                let param = (ident.clone(), None, None);
                let e = self.expression()?;
                let body = vec![Stmt {
                    kind: StmtKind::Return(Some(e)),
                    span: self.previous_span(),
                }];
                // Optimize: wrap in Arc
                return Ok(Expr {
                    kind: ExprKind::Fn(vec![param], std::sync::Arc::new(body), is_async),
                    span,
                });
            }
            return Ok(Expr {
                kind: ExprKind::Variable(ident),
                span,
            });
        }
        // allow reserved 'set'/'get' to be used as identifiers in expression context
        if self.matchk(&[TokenKind::Set]) {
            let ident = "set".to_string();
            let span = self.previous_span();
            if self.matchk(&[TokenKind::Arrow]) {
                let param = (ident.clone(), None, None);
                let e = self.expression()?;
                let body = vec![Stmt {
                    kind: StmtKind::Return(Some(e)),
                    span: self.previous_span(),
                }];
                // Optimize: wrap in Arc
                return Ok(Expr {
                    kind: ExprKind::Fn(vec![param], std::sync::Arc::new(body), is_async),
                    span,
                });
            }
            return Ok(Expr {
                kind: ExprKind::Variable(ident),
                span,
            });
        }
        if self.matchk(&[TokenKind::Get]) {
            let ident = "get".to_string();
            let span = self.previous_span();
            if self.matchk(&[TokenKind::Arrow]) {
                let param = (ident.clone(), None, None);
                let e = self.expression()?;
                let body = vec![Stmt {
                    kind: StmtKind::Return(Some(e)),
                    span: self.previous_span(),
                }];
                // Optimize: wrap in Arc
                return Ok(Expr {
                    kind: ExprKind::Fn(vec![param], std::sync::Arc::new(body), is_async),
                    span,
                });
            }
            return Ok(Expr {
                kind: ExprKind::Variable(ident),
                span,
            });
        }
        if self.matchk(&[TokenKind::PlusPlus]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Update(true, false, Box::new(r)),
                span,
            });
        }
        if self.matchk(&[TokenKind::MinusMinus]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Update(false, false, Box::new(r)),
                span,
            });
        }
        if self.matchk(&[This]) {
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Variable("this".into()),
                span,
            });
        }
        if self.matchk(&[TokenKind::SelfKeyword]) {
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Variable("self".into()),
                span,
            });
        }
        if self.matchk(&[TokenKind::Super]) {
            let span = self.previous_span();
            return Ok(Expr {
                kind: ExprKind::Variable("super".into()),
                span,
            });
        }
        if self.matchk(&[LeftParen]) {
            // Could be grouping or a tuple literal. In Python-style syntax,
            // (a, b) is a tuple, (expr) is grouping, () is an empty tuple,
            // and (a,) is a single-element tuple.
            // Attempt to parse as an arrow function parameters list first:
            //   (a, b = 1) => expr
            // If the pattern fails (not identifiers/defaults or no '=>'),
            // fall back to grouping/tuple parsing.
            let span = self.previous_span();
            let saved = self.current;
            let skip_arrow_probe =
                self.check(TokenKind::Identifier) && self.peek_next_kind(TokenKind::LeftParen);

            // Enable struct literals inside parentheses
            let old_allow_struct = self.allow_struct_literal;
            self.allow_struct_literal = true;

            let mut is_param_list = true;
            let mut params: Vec<(
                std::string::String,
                Option<Expr>,
                Option<std::string::String>,
            )> = Vec::new();
            if !skip_arrow_probe && !self.check(RightParen) {
                loop {
                    // params must be identifiers (allow default via '=')
                    if !self.check(TokenKind::Identifier) {
                        is_param_list = false;
                        break;
                    }
                    let pname = self.consume_ident("Expect param")?;
                    let mut typ: Option<std::string::String> = None;
                    if self.matchk(&[TokenKind::Colon]) {
                        typ = Some(self.consume_ident("Expect type name after ':'")?);
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
            // require closing ')' to be present for param parse
            if !skip_arrow_probe
                && is_param_list
                && self.matchk(&[TokenKind::RightParen])
                && self.matchk(&[TokenKind::Arrow])
            {
                // arrow function body: either a block or an expression
                if self.matchk(&[TokenKind::LeftBrace]) {
                    let body = self.block()?;
                    self.allow_struct_literal = old_allow_struct; // Restore
                    // Optimize: wrap in Arc
                    return Ok(Expr {
                        kind: ExprKind::Fn(params, std::sync::Arc::new(body), is_async),
                        span,
                    });
                } else {
                    let e = self.expression()?;
                    self.allow_struct_literal = old_allow_struct; // Restore
                    let body = vec![Stmt {
                        kind: StmtKind::Return(Some(e)),
                        span: self.previous_span(),
                    }];
                    // Optimize: wrap in Arc
                    return Ok(Expr {
                        kind: ExprKind::Fn(params, std::sync::Arc::new(body), is_async),
                        span,
                    });
                }
            }
            // not an arrow: restore and parse as grouping/tuple
            self.current = saved;
            // self.allow_struct_literal is already true, but we need to restore it eventually.
            // But we backtracked (restored 'current'). 'allow_struct_literal' is irrelevant for the parse we just discarded?
            // Wait, we modified self.allow_struct_literal. It persists.
            // So we are good.

            let mut exprs: Vec<Expr> = Vec::new();
            let mut saw_comma = false;
            if !self.check(RightParen) {
                loop {
                    exprs.push(self.expression()?);
                    if self.matchk(&[TokenKind::Comma]) {
                        saw_comma = true;
                        // allow trailing comma before ')'
                        if self.check(TokenKind::RightParen) {
                            break;
                        } else {
                            continue;
                        }
                    } else {
                        break;
                    }
                }
            }
            self.consume(RightParen, "Expect ')' ")?;
            self.allow_struct_literal = old_allow_struct; // Restore
            if exprs.len() == 1 && !saw_comma {
                // grouping
                return Ok(Expr {
                    kind: ExprKind::Grouping(Box::new(exprs.into_iter().next().unwrap())),
                    span,
                });
            }
            return Ok(Expr {
                kind: ExprKind::Tuple(exprs),
                span,
            });
        }
        if self.matchk(&[LeftBracket]) {
            let span = self.previous_span();
            let mut xs = Vec::new();
            if !self.check(RightBracket) {
                loop {
                    xs.push(self.expression()?);
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                    // allow trailing comma
                    if self.check(RightBracket) {
                        break;
                    }
                }
            }
            self.consume(RightBracket, "Expect ']' after array")?;
            return Ok(Expr {
                kind: ExprKind::Array(xs),
                span,
            });
        }
        if self.matchk(&[LeftBrace]) {
            // Ambiguous: could be object/dict {k: v, ...} or set literal {a, b}
            // Disambiguate by peeking: if first entry is an identifier or string
            // followed by ':' treat as object; else treat as set.
            let span = self.previous_span();
            if self.check(TokenKind::RightBrace) {
                self.consume(TokenKind::RightBrace, "Expect '}' after object")?;
                return Ok(Expr {
                    kind: ExprKind::Object(Vec::new()),
                    span,
                });
            }
            // peek first token kind and next
            let first = self.peek().kind;
            let second = if self.current + 1 < self.tokens.len() {
                self.tokens[self.current + 1].kind
            } else {
                TokenKind::Eof
            };
            if matches!(
                first,
                TokenKind::Let
                    | TokenKind::Const
                    | TokenKind::Return
                    | TokenKind::If
                    | TokenKind::While
                    | TokenKind::For
                    | TokenKind::Break
                    | TokenKind::Continue
                    | TokenKind::Try
                    | TokenKind::Unsafe
                    | TokenKind::Defer
            ) {
                let body = self.block()?;
                let span = self.previous_span();
                let fn_expr = Expr {
                    kind: ExprKind::Fn(vec![], std::sync::Arc::new(body), false),
                    span: span.clone(),
                };
                return Ok(Expr {
                    kind: ExprKind::Call(Box::new(fn_expr), vec![], vec![]),
                    span,
                });
            }
            // Object if: any token followed by colon OR starts with spread (...)
            let is_object = (matches!(second, TokenKind::Colon)
                && !matches!(first, TokenKind::RightBrace | TokenKind::Eof))
                || matches!(first, TokenKind::DotDotDot);
            if is_object {
                let mut kv = Vec::new();
                loop {
                    // Check for spread syntax: ...expr
                    if self.matchk(&[TokenKind::DotDotDot]) {
                        let spread_expr = self.expr_primary()?;
                        kv.push(("...".to_string(), spread_expr)); // Use "..." as marker for spread
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                        if self.check(TokenKind::RightBrace) {
                            break;
                        }
                        continue;
                    }

                    let key = if !self.check(TokenKind::RightBrace)
                        && !self.check(TokenKind::Colon)
                        && !self.check(TokenKind::Comma)
                        && !self.check(TokenKind::Eof)
                    {
                        self.advance().lexeme.clone()
                    } else {
                        return Err(self.format_err(self.peek(), "Expect key in object"));
                    };
                    self.consume(Colon, "Expect ':'")?;
                    // Parse full expression to allow binary ops, calls, etc. in values
                    let v = self.expression()?;
                    kv.push((key, v));
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                    if self.check(TokenKind::RightBrace) {
                        break;
                    }
                }
                self.consume(RightBrace, "Expect '}' after object")?;
                return Ok(Expr {
                    kind: ExprKind::Object(kv),
                    span,
                });
            } else {
                // parse set literal
                let mut elems = Vec::new();
                loop {
                    elems.push(self.expression()?);
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                    if self.check(TokenKind::RightBrace) {
                        break;
                    }
                }
                self.consume(RightBrace, "Expect '}' after set")?;
                return Ok(Expr {
                    kind: ExprKind::SetLiteral(elems),
                    span,
                });
            }
        }
        if self.matchk(&[New]) {
            // Parse a constructor expression for `new` that allows dotted access
            // like `new mod.Person(...)`. We accept an identifier or a
            // parenthesized expression as the base, then consume any `.prop`
            // or indexing chains before parsing the argument list.
            let span = self.previous_span();
            let mut ctor = if self.matchk(&[Identifier]) {
                let s = self.previous_span();
                Expr {
                    kind: ExprKind::Variable(self.prev().lexeme.clone()),
                    span: s,
                }
            } else if self.matchk(&[LeftParen]) {
                let s = self.previous_span();
                let e = self.expression()?;
                self.consume(RightParen, "Expect ')' ")?;
                Expr {
                    kind: ExprKind::Grouping(Box::new(e)),
                    span: s,
                }
            } else {
                return Err(self.format_err(self.peek(), "Expect constructor after 'new'"));
            };

            // allow property access / indexing on the constructor expression
            loop {
                if self.matchk(&[Dot]) {
                    // allow property name to be any token with a lexeme (including keywords)
                    let tk = self.advance();
                    let k = tk.lexeme.clone();
                    let s = ctor.span.clone();
                    ctor = Expr {
                        kind: ExprKind::Get(Box::new(ctor), k),
                        span: s,
                    };
                } else if self.matchk(&[TokenKind::LeftBracket]) {
                    let idx_expr = self.expression()?;
                    self.consume(TokenKind::RightBracket, "Expect ']' after index")?;
                    let s = ctor.span.clone();
                    ctor = Expr {
                        kind: ExprKind::Index(Box::new(ctor), Box::new(idx_expr)),
                        span: s,
                    };
                } else if self.matchk(&[TokenKind::Less]) {
                    while !self.check(TokenKind::Greater)
                        && !self.check(TokenKind::RightParen)
                        && !self.check(TokenKind::Eof)
                    {
                        let _type_arg =
                            self.parse_type_name("Expect type argument in generic constructor")?;
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                    }
                    self.consume_generic_greater("Expect '>' after generic type arguments")?;
                } else {
                    break;
                }
            }

            self.consume(LeftParen, "Expect '(' after new")?;
            let mut args = Vec::new();
            if !self.check(RightParen) {
                loop {
                    // Use conditional() to allow arithmetic in constructor arguments
                    args.push(self.conditional()?);
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(RightParen, "Expect ')' after args")?;
            return Ok(Expr {
                kind: ExprKind::New(Box::new(ctor), args),
                span,
            });
        }
        if self.check(TokenKind::Throw) {
            // capture the throw token info before consuming the expression
            let tk = self.peek().clone();
            self.advance();
            let v = self.expression()?;
            let sp = Span {
                line: tk.line,
                col: tk.col,
                line_text: tk.line_text.clone(),
            };
            return Ok(Expr {
                kind: ExprKind::Throw(Box::new(v)),
                span: sp,
            });
        }
        // Provide a richer error message including the current token and line.
        let tk = self.peek();
        let lex = if tk.lexeme.is_empty() {
            format!("<{:?}>", tk.kind)
        } else {
            tk.lexeme.clone()
        };
        Err(self.format_err(tk, &format!("Expect expression at '{}'", lex)))
    }
}
