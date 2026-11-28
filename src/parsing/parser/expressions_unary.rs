//! Unary expression and postfix parsing
//!
//! This module handles:
//! - unary operators (await, spawn, ++/--, +/-, typeof, !)
//! - memory management operators (&, *, share, weak, alloc, free)
//! - spread operator (...)
//! - range operators (.., ...)
//! - call expressions with optional generic type arguments
//! - property access (., ?.)
//! - indexing ([])
//! - post increment/decrement (++, --)
//! - non-null assertion (!)

use super::core::{PResult, Parser};
use crate::parsing::ast::{Expr, ExprKind, TokenKind};

impl Parser {
    pub(super) fn unary(&mut self) -> PResult<Expr> {
        let k = self.peek().kind;
        let is_prefix = matches!(
            k,
            TokenKind::Await
                | TokenKind::Spawn
                | TokenKind::PlusPlus
                | TokenKind::MinusMinus
                | TokenKind::Plus
                | TokenKind::Typeof
                | TokenKind::Ampersand
                | TokenKind::Star
                | TokenKind::Share
                | TokenKind::Weak
                | TokenKind::Alloc
                | TokenKind::Free
                | TokenKind::Tilde
                | TokenKind::Bang
                | TokenKind::Minus
                | TokenKind::DotDotDot
        ) || (k == TokenKind::Identifier
            && (self.peek().lexeme == "transfer" || self.peek().lexeme == "move")
            && !self.peek_next_kind(TokenKind::LeftParen)
            && !self.peek_next_kind(TokenKind::Equal)
            && !self.peek_next_kind(TokenKind::Semicolon)
            && !self.peek_next_kind(TokenKind::Comma)
            && !self.peek_next_kind(TokenKind::RightParen)
            && !self.peek_next_kind(TokenKind::RightBracket)
            && !self.peek_next_kind(TokenKind::RightBrace));

        if is_prefix {
            self.unary_prefix()
        } else {
            let mut e = self.primary()?;
            if matches!(
                self.peek().kind,
                TokenKind::Less
                    | TokenKind::LeftParen
                    | TokenKind::Dot
                    | TokenKind::QuestionDot
                    | TokenKind::LeftBracket
                    | TokenKind::PlusPlus
                    | TokenKind::MinusMinus
                    | TokenKind::Bang
                    | TokenKind::Question
            ) {
                e = self.call_postfix(e)?;
            }
            if self.check(TokenKind::DotDot) || self.check(TokenKind::DotDotDot) {
                self.check_range_after(e)
            } else {
                Ok(e)
            }
        }
    }

    #[inline(never)]
    fn unary_prefix(&mut self) -> PResult<Expr> {
        if self.matchk(&[TokenKind::Await]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Await(Box::new(r)),
                span,
            });
        }
        if self.matchk(&[TokenKind::Spawn]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Spawn(Box::new(r)),
                span,
            });
        }
        if self.matchk(&[TokenKind::PlusPlus]) {
            let span = self.previous_span();
            let target = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Update(true, false, Box::new(target)),
                span,
            });
        }
        if self.matchk(&[TokenKind::MinusMinus]) {
            let span = self.previous_span();
            let target = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Update(false, false, Box::new(target)),
                span,
            });
        }
        if self.matchk(&[TokenKind::Plus]) {
            let span = self.previous_span();
            // For unary plus, parse operand then check for range to allow +5..-1
            let r = self.unary_operand()?;
            let unary_expr = Expr {
                kind: ExprKind::Unary(TokenKind::Plus, Box::new(r)),
                span,
            };
            return self.check_range_after(unary_expr);
        }
        if self.check(TokenKind::Identifier)
            && (self.peek().lexeme == "transfer" || self.peek().lexeme == "move")
            && !self.peek_next_kind(TokenKind::LeftParen)
            && !self.peek_next_kind(TokenKind::Equal)
            && !self.peek_next_kind(TokenKind::Semicolon)
            && !self.peek_next_kind(TokenKind::Comma)
            && !self.peek_next_kind(TokenKind::RightParen)
            && !self.peek_next_kind(TokenKind::RightBracket)
            && !self.peek_next_kind(TokenKind::RightBrace)
        {
            self.advance();
            return self.unary();
        }
        if self.matchk(&[TokenKind::Typeof]) {
            let span = self.previous_span();
            if self.matchk(&[TokenKind::LeftParen]) {
                let mut args = Vec::new();
                if !self.check(TokenKind::RightParen) {
                    loop {
                        args.push(self.expr()?);
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(TokenKind::RightParen, "Expect ')' after typeof arguments")?;
                return Ok(Expr {
                    kind: ExprKind::Call(
                        Box::new(Expr {
                            kind: ExprKind::Variable("type".to_string()),
                            span: span.clone(),
                        }),
                        args,
                        vec![],
                    ),
                    span,
                });
            }

            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Unary(TokenKind::Typeof, Box::new(r)),
                span,
            });
        }
        // Memory management expressions
        if self.matchk(&[TokenKind::Ampersand]) {
            let start_span = self.previous_span();
            // Accept `&mut expr` or legacy `& let expr` forms; store mutability as a marker token
            let is_mut = if self.check(TokenKind::Identifier) && self.peek().lexeme == "mut" {
                self.advance();
                true
            } else {
                self.matchk(&[TokenKind::Let])
            };
            let r = self.unary()?;
            let op = if is_mut {
                TokenKind::BangEqual
            } else {
                TokenKind::Ampersand
            };
            return Ok(Expr {
                kind: ExprKind::Unary(op, Box::new(r)),
                span: start_span,
            });
        }
        if self.matchk(&[TokenKind::Star]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Unary(TokenKind::Star, Box::new(r)),
                span,
            }); // Deref marker
        }
        if self.matchk(&[TokenKind::Share]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Call(
                    Box::new(Expr {
                        kind: ExprKind::Variable("__share".to_string()),
                        span: span.clone(),
                    }),
                    vec![r],
                    vec![],
                ),
                span,
            });
        }
        if self.matchk(&[TokenKind::Weak]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Call(
                    Box::new(Expr {
                        kind: ExprKind::Variable("__weak".to_string()),
                        span: span.clone(),
                    }),
                    vec![r],
                    vec![],
                ),
                span,
            });
        }
        if self.matchk(&[TokenKind::Alloc]) {
            let start_span = self.previous_span();
            // Support alloc<u8>(size) and alloc size (unary) forms
            let mut call_type_args: Vec<String> = Vec::new();
            if self.check(TokenKind::Less) {
                let saved = self.current;
                if self.matchk(&[TokenKind::Less]) {
                    let mut parsed_any = false;
                    if !self.check(TokenKind::Greater) {
                        loop {
                            match self.parse_type_name("Expect type argument") {
                                Ok(tn) => {
                                    call_type_args.push(tn);
                                    parsed_any = true;
                                    if !self.matchk(&[TokenKind::Comma]) {
                                        break;
                                    }
                                }
                                Err(_) => {
                                    parsed_any = false;
                                    break;
                                }
                            }
                        }
                    }
                    if parsed_any {
                        if let Err(_) = self.consume_generic_greater("Expect '>' after type args") {
                            self.current = saved;
                            call_type_args.clear();
                        }
                    } else {
                        self.current = saved;
                    }
                }
            }

            // If followed by parens, parse as normal call; otherwise unary alloc expr
            if self.matchk(&[TokenKind::LeftParen]) {
                let mut args = Vec::new();
                if !self.check(TokenKind::RightParen) {
                    loop {
                        // Use conditional() to allow arithmetic in arguments while still
                        // tracking recursion depth so deeply nested calls hit max_recursion_depth.
                        self.enter_recursion()?;
                        let arg_res = self.conditional();
                        self.leave_recursion();
                        args.push(arg_res?);
                        if !self.matchk(&[TokenKind::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(TokenKind::RightParen, "Expect ')' after alloc arguments")?;
                return Ok(Expr {
                    kind: ExprKind::Call(
                        Box::new(Expr {
                            kind: ExprKind::Variable("__alloc".to_string()),
                            span: start_span.clone(),
                        }),
                        args,
                        call_type_args,
                    ),
                    span: start_span,
                });
            } else {
                let r = self.unary()?;
                return Ok(Expr {
                    kind: ExprKind::Call(
                        Box::new(Expr {
                            kind: ExprKind::Variable("__alloc".to_string()),
                            span: start_span.clone(),
                        }),
                        vec![r],
                        call_type_args,
                    ),
                    span: start_span,
                });
            }
        }
        if self.matchk(&[TokenKind::Free]) {
            let span = self.previous_span();
            let r = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Call(
                    Box::new(Expr {
                        kind: ExprKind::Variable("__free".to_string()),
                        span: span.clone(),
                    }),
                    vec![r],
                    vec![],
                ),
                span,
            });
        }
        if self.matchk(&[TokenKind::Tilde]) {
            let span = self.previous_span();
            let r = self.unary()?;
            Ok(Expr {
                kind: ExprKind::Unary(TokenKind::Tilde, Box::new(r)),
                span,
            })
        } else if self.matchk(&[TokenKind::Bang]) {
            let span = self.previous_span();
            let r = self.unary()?;
            Ok(Expr {
                kind: ExprKind::Unary(TokenKind::Bang, Box::new(r)),
                span,
            })
        } else if self.matchk(&[TokenKind::Minus]) {
            let span = self.previous_span();
            // For unary minus, parse operand then check for range to allow -5..-1
            let r = self.unary_operand()?;
            let unary_expr = Expr {
                kind: ExprKind::Unary(TokenKind::Minus, Box::new(r)),
                span,
            };
            self.check_range_after(unary_expr)
        } else if self.matchk(&[TokenKind::DotDotDot]) {
            let span = self.previous_span();
            // Spread operator: ...expr
            let r = self.unary()?;
            Ok(Expr {
                kind: ExprKind::Spread(Box::new(r)),
                span,
            })
        } else {
            self.range()
        }
    }

    /// Parse unary operand - doesn't include range to ensure -5.. parses as (-5).. not -(5..)
    pub(super) fn unary_operand(&mut self) -> PResult<Expr> {
        if self.matchk(&[TokenKind::Plus]) {
            let span = self.previous_span();
            let r = self.unary_operand()?;
            return Ok(Expr {
                kind: ExprKind::Unary(TokenKind::Plus, Box::new(r)),
                span,
            });
        }
        if self.matchk(&[TokenKind::Minus]) {
            let span = self.previous_span();
            let r = self.unary_operand()?;
            return Ok(Expr {
                kind: ExprKind::Unary(TokenKind::Minus, Box::new(r)),
                span,
            });
        }
        if self.matchk(&[TokenKind::Bang]) {
            let span = self.previous_span();
            let r = self.unary_operand()?;
            return Ok(Expr {
                kind: ExprKind::Unary(TokenKind::Bang, Box::new(r)),
                span,
            });
        }
        self.call()
    }

    /// Check if there's a range operator following an expression and parse it
    pub(super) fn check_range_after(&mut self, e: Expr) -> PResult<Expr> {
        if self.matchk(&[TokenKind::DotDot]) {
            let end = self.unary_operand()?;
            let span = e.span.clone();
            Ok(Expr {
                kind: ExprKind::Range(Box::new(e), Box::new(end), false),
                span,
            }) // exclusive range
        } else if self.matchk(&[TokenKind::DotDotDot]) {
            let end = self.unary_operand()?;
            let span = e.span.clone();
            Ok(Expr {
                kind: ExprKind::Range(Box::new(e), Box::new(end), true),
                span,
            }) // inclusive range
        } else {
            Ok(e)
        }
    }

    pub(super) fn range(&mut self) -> PResult<Expr> {
        let e = self.call()?;
        if self.check(TokenKind::DotDot) || self.check(TokenKind::DotDotDot) {
            self.check_range_after(e)
        } else {
            Ok(e)
        }
    }

    pub(super) fn call(&mut self) -> PResult<Expr> {
        let e = self.primary()?;
        if matches!(
            self.peek().kind,
            TokenKind::Less
                | TokenKind::LeftParen
                | TokenKind::Dot
                | TokenKind::QuestionDot
                | TokenKind::LeftBracket
                | TokenKind::PlusPlus
                | TokenKind::MinusMinus
                | TokenKind::Bang
                | TokenKind::Question
        ) {
            self.call_postfix(e)
        } else {
            Ok(e)
        }
    }

    #[inline(never)]
    fn call_postfix_type_args(&mut self) -> Vec<String> {
        let mut call_type_args: Vec<String> = Vec::new();
        let saved = self.current;
        if self.matchk(&[TokenKind::Less]) {
            let mut parsed_any = false;
            if !self.check(TokenKind::Greater) {
                loop {
                    match self.parse_type_name("Expect type argument") {
                        Ok(tn) => {
                            call_type_args.push(tn);
                            parsed_any = true;
                            if !self.matchk(&[TokenKind::Comma]) {
                                break;
                            }
                        }
                        Err(_) => {
                            parsed_any = false;
                            break;
                        }
                    }
                }
            }
            if parsed_any {
                if self
                    .consume_generic_greater("Expect '>' after type args")
                    .is_err()
                {
                    self.current = saved;
                    call_type_args.clear();
                }
            } else {
                self.current = saved;
            }
        }
        call_type_args
    }

    #[inline(never)]
    fn call_postfix_args(&mut self, e: Expr, call_type_args: Vec<String>) -> PResult<Expr> {
        let mut args = Vec::new();
        if !self.check(TokenKind::RightParen) {
            loop {
                // Use conditional() to allow arithmetic in arguments while still
                // tracking recursion depth so deeply nested calls hit max_recursion_depth.
                self.enter_recursion()?;
                let arg_res = self.conditional();
                self.leave_recursion();
                args.push(arg_res?);
                if !self.matchk(&[TokenKind::Comma]) {
                    break;
                }
            }
        }
        self.consume(TokenKind::RightParen, "Expect ')' after arguments")?;
        let span = e.span.clone();
        Ok(Expr {
            kind: ExprKind::Call(Box::new(e), args, call_type_args),
            span,
        })
    }

    #[inline(never)]
    fn call_postfix_index(&mut self, e: Expr) -> PResult<Expr> {
        let idx_expr = self.expr()?;
        self.consume(TokenKind::RightBracket, "Expect ']' after index")?;
        let span = e.span.clone();
        Ok(Expr {
            kind: ExprKind::Index(Box::new(e), Box::new(idx_expr)),
            span,
        })
    }

    #[inline(never)]
    fn call_postfix(&mut self, mut e: Expr) -> PResult<Expr> {
        loop {
            // optional call-site type args: `<T, U>` only when immediately followed by '(' to avoid ambiguity with binary '<'
            let call_type_args = if self.check(TokenKind::Less) {
                self.call_postfix_type_args()
            } else {
                Vec::new()
            };

            if self.matchk(&[TokenKind::LeftParen]) {
                e = self.call_postfix_args(e, call_type_args)?;
            } else if self.matchk(&[TokenKind::Dot]) {
                // allow property name to be any token with a lexeme (including keywords)
                let tk = self.advance();
                let k = tk.lexeme.clone();
                let span = e.span.clone();
                e = Expr {
                    kind: ExprKind::Get(Box::new(e), k),
                    span,
                };
            } else if self.matchk(&[TokenKind::QuestionDot]) {
                let tk = self.advance();
                let k = tk.lexeme.clone();
                let span = e.span.clone();
                e = Expr {
                    kind: ExprKind::OptGet(Box::new(e), k),
                    span,
                };
            } else if self.matchk(&[TokenKind::LeftBracket]) {
                // general indexing: e[expr] -> Expr::Index(e, expr)
                e = self.call_postfix_index(e)?;
            } else if self.matchk(&[TokenKind::PlusPlus]) {
                let span = e.span.clone();
                e = Expr {
                    kind: ExprKind::Update(true, true, Box::new(e)),
                    span,
                };
            } else if self.matchk(&[TokenKind::MinusMinus]) {
                let span = e.span.clone();
                e = Expr {
                    kind: ExprKind::Update(false, true, Box::new(e)),
                    span,
                };
            } else if self.matchk(&[TokenKind::Bang]) {
                let span = e.span.clone();
                e = Expr {
                    kind: ExprKind::NonNull(Box::new(e)),
                    span,
                };
            } else if self.check(TokenKind::Question) && !self.is_ternary_question() {
                self.advance();
                let span = e.span.clone();
                e = Expr {
                    kind: ExprKind::Try(Box::new(e)),
                    span,
                };
            } else {
                break;
            }
        }
        Ok(e)
    }
}
