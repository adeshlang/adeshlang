//! Type annotation parsing
//!
//! This module handles parsing of:
//! - slice types (&[T])
//! - array types ([T], [T;N], [T;raw], [T;N;raw])
//! - function types ((t1,t2)->ret)
//! - tuple types ((t1, t2, t3))
//! - SIMD vector types (vec2<T>, vec3<T>, etc.)
//! - identifier types with optional generics (MyType<T, U>)
//! - nullable types (T?)
//! - pointer types (*T)
//! - generic argument parsing with shift-right handling

use super::core::Parser;
use crate::parsing::ast::TokenKind;
use crate::parsing::error::LangError;

impl Parser {
    pub(super) fn parse_type_name(&mut self, err_msg: &str) -> Result<String, LangError> {
        let mut base = self.parse_primary_type_name(err_msg)?;
        while self.matchk(&[TokenKind::Pipe]) {
            let next = self.parse_primary_type_name(err_msg)?;
            base = format!("{} | {}", base, next);
        }
        Ok(base)
    }

    fn parse_primary_type_name(&mut self, err_msg: &str) -> Result<String, LangError> {
        // Function type syntax: fn(t1, t2) -> ret or fn(t1, t2): ret
        let mut res = if self.matchk(&[TokenKind::Fn]) {
            self.consume(
                TokenKind::LeftParen,
                "Expect '(' after 'fn' for function type annotation",
            )?;
            let mut parts: Vec<String> = Vec::new();
            if !self.check(TokenKind::RightParen) {
                loop {
                    let part = self.parse_type_name("Expect type in parameter list")?;
                    parts.push(part);
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(TokenKind::RightParen, "Expect ')' in type annotation")?;

            if self.matchk(&[TokenKind::Colon]) {
                let ret =
                    self.parse_type_name("Expect return type after function parameter list")?;
                let params = parts.join(",");
                format!("fn({})->{}", params, ret)
            } else if self.matchk(&[TokenKind::Minus]) && self.matchk(&[TokenKind::Greater]) {
                let ret =
                    self.parse_type_name("Expect return type after function parameter list")?;
                let params = parts.join(",");
                format!("fn({})->{}", params, ret)
            } else {
                let params = parts.join(",");
                format!("fn({})->null", params)
            }
        } else if self.matchk(&[TokenKind::Ampersand]) {
            // Slice type: &[T]
            if self.matchk(&[TokenKind::LeftBracket]) {
                let elem_type = self.parse_type_name("Expect element type in slice")?;
                self.consume(
                    TokenKind::RightBracket,
                    "Expect ']' after slice element type",
                )?;
                format!("&[{}]", elem_type)
            } else {
                return Err(self.format_err(self.peek(), "Expect '[' after '&' for slice type"));
            }
        } else if self.matchk(&[TokenKind::LeftBracket]) {
            // Check for empty array type `[]`
            if self.matchk(&[TokenKind::RightBracket]) {
                "[]".to_string()
            } else {
                // Array types: [T], [T;N], [T;raw], [T;N;raw], and matrix forms like [T;N;M] or comma equivalents
                let elem_type = self.parse_type_name("Expect element type in array")?;

                // Check for `;` or `,` which indicates fixed-size, raw, or matrix dimensions
                if self.matchk(&[TokenKind::Semicolon, TokenKind::Comma]) {
                    let mut specifiers: Vec<String> = Vec::new();
                    let mut is_raw = false;
                    
                    loop {
                        if self.matchk(&[TokenKind::Raw]) {
                            is_raw = true;
                            break;
                        }
                        if self.check(TokenKind::Number) {
                            let tok = self.advance();
                            specifiers.push(tok.lexeme.clone());
                        } else {
                            return Err(self.format_err(
                                self.peek(),
                                "Expect size (number) or 'raw' in array type annotation",
                            ));
                        }
                        
                        if !self.matchk(&[TokenKind::Semicolon, TokenKind::Comma]) {
                            break;
                        }
                    }
                    
                    self.consume(TokenKind::RightBracket, "Expect ']' after array type")?;
                    
                    if specifiers.is_empty() {
                        if is_raw {
                            format!("[{};raw]", elem_type)
                        } else {
                            return Err(self.format_err(self.peek(), "Invalid empty array specifiers"));
                        }
                    } else {
                        // Reconstruct nested arrays from right to left (outermost size is leftmost)
                        let mut current = elem_type;
                        for size in specifiers.iter().rev() {
                            if is_raw {
                                current = format!("[{};{};raw]", current, size);
                            } else {
                                current = format!("[{};{}]", current, size);
                            }
                        }
                        current
                    }
                } else {
                    // [T] - dynamic array
                    self.consume(TokenKind::RightBracket, "Expect ']' after array type")?;
                    format!("[{}]", elem_type)
                }
            }
        } else if self.matchk(&[TokenKind::LeftBrace]) {
            // Check for empty record/object `{}` or object type
            if self.matchk(&[TokenKind::RightBrace]) {
                "{}".to_string()
            } else {
                let mut fields: Vec<String> = Vec::new();
                while !self.check(TokenKind::RightBrace) && !self.is_end() {
                    let fname = self.consume_ident("Expect field name in record type")?;
                    let opt = if self.matchk(&[TokenKind::Question]) { "?" } else { "" };
                    self.consume(TokenKind::Colon, "Expect ':' after field name in record type")?;
                    let ftype = self.parse_type_name("Expect field type")?;
                    fields.push(format!("{}{}: {}", fname, opt, ftype));
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
                self.consume(TokenKind::RightBrace, "Expect '}' after record type")?;
                format!("{{{}}}", fields.join(", "))
            }
        } else if self.matchk(&[TokenKind::LeftParen]) {
            // Parenthesized type or tuple: (t1,t2)->ret or (t1, t2, t3) or (t1)
            let mut parts: Vec<String> = Vec::new();
            if !self.check(TokenKind::RightParen) {
                loop {
                    let part = self.parse_type_name("Expect type in parameter list")?;
                    parts.push(part);
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
            }
            self.consume(TokenKind::RightParen, "Expect ')' in type annotation")?;
            // Check if this is a function type (has ->) or a tuple/parenthesized type
            if self.matchk(&[TokenKind::Minus]) && self.matchk(&[TokenKind::Greater]) {
                let ret = self.parse_type_name("Expect return type after '->'")?;
                let params = parts.join(",");
                format!("({})->{}", params, ret)
            } else if parts.len() == 1 {
                // Single parenthesized type e.g. (int | string)
                parts[0].clone()
            } else {
                // This is a tuple type
                format!("({})", parts.join(","))
            }
        } else if self.matchk(&[TokenKind::VecType]) {
            // SIMD vector types: vec keyword followed by a number and generic
            if self.check(TokenKind::Number) {
                let lanes_tok = self.advance();
                let lanes = lanes_tok.lexeme.clone();
                if self.matchk(&[TokenKind::Less]) {
                    let elem_type = self.parse_type_name("Expect element type in vec")?;
                    self.consume_generic_greater("Expect '>' after vec type argument")?;
                    let nullable = if self.matchk(&[TokenKind::Question]) {
                        "?"
                    } else {
                        ""
                    };
                    format!("vec{}<{}>{}", lanes, elem_type, nullable)
                } else {
                    return Err(self.format_err(self.peek(), "Expect '<' after vec dimension"));
                }
            } else {
                return Err(self.format_err(self.peek(), "Expect number after 'vec' for SIMD type"));
            }
        } else if self.matchk(&[TokenKind::Null]) {
            "null".to_string()
        } else if self.check(TokenKind::Identifier)
            || self.check(TokenKind::Raw)
            || self.check(TokenKind::Uint)
            || self.check(TokenKind::Int)
            || self.check(TokenKind::Set)
            || self.check(TokenKind::Share)
            || self.check(TokenKind::Weak)
            || self.check(TokenKind::Strong)
            || self.check(TokenKind::SelfKeyword)
            || self.check(TokenKind::Type)
            || self.check(TokenKind::Struct)
        {
            let mut base = if self.check(TokenKind::Raw) {
                self.advance();
                "raw".to_string()
            } else if self.check(TokenKind::Uint) {
                self.advance();
                "uint".to_string()
            } else if self.check(TokenKind::Int) {
                self.advance();
                "int".to_string()
            } else if self.check(TokenKind::Set) {
                self.advance();
                "set".to_string()
            } else {
                self.consume_ident(err_msg)?
            };
            // Simd<T, N> type: Simd<f32, 8>
            if base == "Simd" && self.matchk(&[TokenKind::Less]) {
                let elem_type = self.parse_type_name("Expect element type in Simd")?;
                self.consume(TokenKind::Comma, "Expect ',' after element type in Simd<T, N>")?;
                let lanes_tok = self.advance();
                if !matches!(lanes_tok.kind, TokenKind::Number) {
                    return Err(self.format_err(
                        self.peek(),
                        "Expect lane count (number) in Simd<T, N>",
                    ));
                }
                let lanes = lanes_tok.lexeme.clone();
                self.consume_generic_greater("Expect '>' after Simd type arguments")?;
                let nullable = if self.matchk(&[TokenKind::Question]) {
                    "?"
                } else {
                    ""
                };
                return Ok(format!("Simd<{}, {}{}>", elem_type, lanes, nullable));
            }
            // Support dot-qualified type identifiers e.g. WebSocket.ClientConfig
            while self.matchk(&[TokenKind::Dot]) {
                let sub = self.consume_ident("Expect identifier after '.' in type name")?;
                base.push('.');
                base.push_str(&sub);
            }
            // Check for vec prefix patterns like vec2, vec3, vec4
            if base.starts_with("vec") && base.len() > 3 {
                let lanes = &base[3..];
                if lanes.chars().all(|c| c.is_ascii_digit()) {
                    if self.matchk(&[TokenKind::Less]) {
                        let elem_type = self.parse_type_name("Expect element type in vec")?;
                        self.consume_generic_greater("Expect '>' after vec type argument")?;
                        let nullable = if self.matchk(&[TokenKind::Question]) {
                            "?"
                        } else {
                            ""
                        };
                        return Ok(format!("{}<{}>{}", base, elem_type, nullable));
                    }
                }
            }
            // optional generic arg list: <T, U, ...>
            let mut out = base.clone();
            if self.matchk(&[TokenKind::Less]) {
                let mut args: Vec<String> = Vec::new();
                if !self.check(TokenKind::Greater) {
                    loop {
                        let arg = self.parse_type_name("Expect type argument")?;
                        args.push(arg);
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                    }
                }
                self.consume_generic_greater("Expect '>' after type arguments")?;
                out.push('<');
                out.push_str(&args.join(","));
                out.push('>');
            }
            // optional nullable suffix '?'
            if self.matchk(&[TokenKind::Question]) {
                out.push('?');
            }
            out
        } else if self.matchk(&[TokenKind::Star]) {
            // Pointer type: *T
            let pointee = self.parse_type_name("Expect type after '*' for pointer type")?;
            format!("*{}", pointee)
        } else {
            return Err(self.format_err(self.peek(), err_msg));
        };

        // Postfix array syntax: e.g. T[], T[][], etc.
        while self.matchk(&[TokenKind::LeftBracket]) {
            self.consume(TokenKind::RightBracket, "Expect ']' after '[' for array type")?;
            res = format!("[{}]", res);
        }

        // Postfix optional: T?
        if self.matchk(&[TokenKind::Question]) && !res.ends_with('?') {
            res.push('?');
        }

        Ok(res)
    }

    pub(super) fn consume_generic_greater(&mut self, msg: &str) -> Result<(), LangError> {
        if self.pending_gt > 0 {
            self.pending_gt -= 1;
            return Ok(());
        }
        if self.check(TokenKind::Greater) {
            let _ = self.advance();
            Ok(())
        } else if self.check(TokenKind::ShiftRight) {
            let _ = self.advance();
            self.pending_gt += 1;
            Ok(())
        } else {
            Err(self.format_err(self.peek(), msg))
        }
    }

    // helpers
}
