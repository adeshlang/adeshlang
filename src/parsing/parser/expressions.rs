//! Expression parsing - entry point and binary operators
//!
//! This module handles:
//! - expression entry point
//! - assignment operators
//! - conditional (ternary) operator
//! - logical operators (or, and, nullish coalescing)
//! - bitwise operators (|, ^, &)
//! - equality and comparison operators
//! - shift operators
//! - arithmetic operators (term, factor, power)

use super::core::Parser;
use crate::parsing::ast::{Expr, ExprKind, TokenKind};
use crate::parsing::error::LangError;

impl Parser {
    pub fn expression(&mut self) -> Result<Expr, LangError> {
        self.with_recursion(|this| this.assignment())
    }

    /// Ultra-lightweight expression parser for deeply nested structures.
    /// Skips most precedence levels to minimize stack usage.
    /// Used in object/array literal values to prevent stack overflow.
    /// Goes straight to unary() to save ~10 stack frames per level of nesting.
    #[inline]
    pub(super) fn expr_primary(&mut self) -> Result<Expr, LangError> {
        self.unary()
    }

    pub(super) fn parse_tuple_rhs(&mut self) -> Result<Expr, LangError> {
        let first = self.expression()?;
        let span = first.span.clone();
        if self.check(TokenKind::Comma) {
            let mut exprs = vec![first];
            while self.matchk(&[TokenKind::Comma]) {
                exprs.push(self.expression()?);
            }
            Ok(Expr {
                kind: ExprKind::Tuple(exprs),
                span,
            })
        } else {
            Ok(first)
        }
    }

    pub(super) fn assignment(&mut self) -> Result<Expr, LangError> {
        let expr = self.conditional()?;

        if (matches!(expr.kind, ExprKind::Variable(_)) && self.check(TokenKind::Comma))
            || matches!(
                self.peek().kind,
                TokenKind::Equal
                    | TokenKind::PlusEqual
                    | TokenKind::MinusEqual
                    | TokenKind::StarEqual
                    | TokenKind::SlashEqual
                    | TokenKind::PercentEqual
                    | TokenKind::StarStarEqual
                    | TokenKind::ShiftLeftEqual
                    | TokenKind::ShiftRightEqual
                    | TokenKind::AmpersandEqual
                    | TokenKind::PipeEqual
                    | TokenKind::CaretEqual
                    | TokenKind::NullCoalesceEqual
            )
        {
            return self.assignment_rhs(expr);
        }

        Ok(expr)
    }

    #[inline(never)]
    fn assignment_rhs(&mut self, expr: Expr) -> Result<Expr, LangError> {
        // Unparenthesized multi-variable assignment: a, b, c = 10, 20, 30 or a, b = b, a
        if matches!(expr.kind, ExprKind::Variable(_)) && self.check(TokenKind::Comma) {
            let saved = self.current;
            let mut lhs_exprs = vec![expr.clone()];
            let mut valid_multi_assign = true;
            while self.matchk(&[TokenKind::Comma]) {
                match self.conditional() {
                    Ok(next_expr) => {
                        if !matches!(next_expr.kind, ExprKind::Variable(_)) {
                            valid_multi_assign = false;
                        }
                        lhs_exprs.push(next_expr);
                        if self.check(TokenKind::Equal) {
                            break;
                        }
                    }
                    Err(_) => {
                        valid_multi_assign = false;
                        break;
                    }
                }
            }
            if valid_multi_assign && self.matchk(&[TokenKind::Equal]) {
                let span = self.previous_span();
                let names = self.extract_variable_names(&lhs_exprs)?;
                let rhs_val = self.parse_tuple_rhs()?;
                return Ok(Expr {
                    kind: ExprKind::AssignTuple(names, Box::new(rhs_val)),
                    span,
                });
            } else {
                // Not a multi-variable assignment; rewind so the comma is preserved for
                // enclosing constructs like match arms, struct literals, or argument lists.
                self.current = saved;
            }
        }

        if self.matchk(&[
            TokenKind::Equal,
            TokenKind::PlusEqual,
            TokenKind::MinusEqual,
            TokenKind::StarEqual,
            TokenKind::SlashEqual,
            TokenKind::PercentEqual,
            TokenKind::StarStarEqual,
            TokenKind::ShiftLeftEqual,
            TokenKind::ShiftRightEqual,
            TokenKind::AmpersandEqual,
            TokenKind::PipeEqual,
            TokenKind::CaretEqual,
            TokenKind::NullCoalesceEqual,
        ]) {
            let op = self.prev().kind.clone();
            let v = self.assignment()?;
            let span = expr.span.clone();

            // Handle destructuring assignment for simple = operator
            if op == TokenKind::Equal {
                match &expr.kind {
                    // Array destructuring: [a, b, c] = expr
                    ExprKind::Array(elems) => {
                        let names = self.extract_variable_names(elems)?;
                        return Ok(Expr {
                            kind: ExprKind::AssignTuple(names, Box::new(v)),
                            span,
                        });
                    }
                    // Tuple destructuring: (a, b, c) = expr
                    ExprKind::Tuple(elems) => {
                        let names = self.extract_variable_names(elems)?;
                        return Ok(Expr {
                            kind: ExprKind::AssignTuple(names, Box::new(v)),
                            span,
                        });
                    }
                    // Object destructuring: {x, y: newY} = expr
                    ExprKind::Object(pairs) => {
                        let bindings = self.extract_object_bindings(pairs)?;
                        return Ok(Expr {
                            kind: ExprKind::AssignObject(bindings, Box::new(v)),
                            span,
                        });
                    }
                    _ => {}
                }
            }

            match expr.kind {
                ExprKind::Variable(_name) => Ok(Expr {
                    kind: ExprKind::AssignOp(
                        Box::new(Expr {
                            kind: ExprKind::Variable(_name),
                            span: span.clone(),
                        }),
                        op,
                        Box::new(v),
                    ),
                    span,
                }),
                ExprKind::Get(obj, key) => Ok(Expr {
                    kind: ExprKind::AssignOp(
                        Box::new(Expr {
                            kind: ExprKind::Get(obj, key),
                            span: span.clone(),
                        }),
                        op,
                        Box::new(v),
                    ),
                    span,
                }),
                ExprKind::Index(obj, idx_expr) => Ok(Expr {
                    kind: ExprKind::AssignOp(
                        Box::new(Expr {
                            kind: ExprKind::Index(obj, idx_expr),
                            span: span.clone(),
                        }),
                        op,
                        Box::new(v),
                    ),
                    span,
                }),
                ExprKind::Unary(TokenKind::Star, target) => Ok(Expr {
                    kind: ExprKind::AssignOp(
                        Box::new(Expr {
                            kind: ExprKind::Unary(TokenKind::Star, target),
                            span: span.clone(),
                        }),
                        op,
                        Box::new(v),
                    ),
                    span,
                }),
                _ => Err(self.format_err(self.peek(), "Invalid assignment target")),
            }
        } else {
            Ok(expr)
        }
    }

    pub(super) fn conditional(&mut self) -> Result<Expr, LangError> {
        let e = self.binary_prec(1)?;
        if self.check(TokenKind::Question) {
            self.conditional_tail(e)
        } else {
            Ok(e)
        }
    }

    #[inline(never)]
    fn conditional_tail(&mut self, e: Expr) -> Result<Expr, LangError> {
        self.advance();
        let then_e = self.expression()?;
        self.consume(TokenKind::Colon, "Expect ':' in ternary")?;
        let else_e = self.expression()?;
        let span = e.span.clone();
        Ok(Expr {
            kind: ExprKind::Conditional(Box::new(e), Box::new(then_e), Box::new(else_e)),
            span,
        })
    }

    fn binary_op_info(kind: TokenKind) -> Option<(u8, bool, bool)> {
        // (precedence, is_right_assoc, is_logical)
        match kind {
            TokenKind::Or | TokenKind::OrOr => Some((1, false, true)),
            TokenKind::And | TokenKind::AndAnd => Some((2, false, true)),
            TokenKind::NullCoalesce => Some((3, false, false)),
            TokenKind::Pipe => Some((4, false, false)),
            TokenKind::Caret => Some((5, false, false)),
            TokenKind::Ampersand => Some((6, false, false)),
            TokenKind::BangEqual
            | TokenKind::EqualEqual
            | TokenKind::StrictEqual
            | TokenKind::StrictNotEqual => Some((7, false, false)),
            TokenKind::Greater
            | TokenKind::GreaterEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Instanceof
            | TokenKind::In => Some((8, false, false)),
            TokenKind::ShiftLeft | TokenKind::ShiftRight => Some((9, false, false)),
            TokenKind::Plus | TokenKind::Minus => Some((10, false, false)),
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent | TokenKind::TildeSlash => {
                Some((11, false, false))
            }
            TokenKind::StarStar => Some((12, true, false)),
            _ => None,
        }
    }

    fn binary_prec(&mut self, min_prec: u8) -> Result<Expr, LangError> {
        let lhs = self.cast()?;
        if Self::binary_op_info(self.peek().kind).is_some_and(|(prec, _, _)| prec >= min_prec) {
            self.binary_prec_loop(lhs, min_prec)
        } else {
            Ok(lhs)
        }
    }

    #[inline(never)]
    fn binary_prec_loop(&mut self, mut lhs: Expr, min_prec: u8) -> Result<Expr, LangError> {
        while let Some((prec, right_assoc, is_logical)) = Self::binary_op_info(self.peek().kind) {
            if prec < min_prec {
                break;
            }
            let op_tok = self.advance().kind;
            let next_min = if right_assoc { prec } else { prec + 1 };
            let rhs = self.binary_prec(next_min)?;
            let span = lhs.span.clone();
            lhs = if is_logical {
                let logical_op = match op_tok {
                    TokenKind::Or | TokenKind::OrOr => TokenKind::Or,
                    _ => TokenKind::And,
                };
                Expr {
                    kind: ExprKind::Logical(Box::new(lhs), logical_op, Box::new(rhs)),
                    span,
                }
            } else {
                Expr {
                    kind: ExprKind::Binary(Box::new(lhs), op_tok, Box::new(rhs)),
                    span,
                }
            };
        }
        Ok(lhs)
    }

    pub(super) fn cast(&mut self) -> Result<Expr, LangError> {
        let e = self.unary()?;
        if self.check(TokenKind::As) {
            self.cast_tail(e)
        } else {
            Ok(e)
        }
    }

    #[inline(never)]
    fn cast_tail(&mut self, mut e: Expr) -> Result<Expr, LangError> {
        while self.matchk(&[TokenKind::As]) {
            let target_type = self.parse_type_name("Expect type name after 'as'")?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Cast(Box::new(e), target_type),
                span,
            };
        }
        Ok(e)
    }

    /// Extract variable names from array/tuple elements for destructuring assignment
    fn extract_variable_names(&self, elems: &[Expr]) -> Result<Vec<String>, LangError> {
        let mut names = Vec::new();
        for elem in elems {
            match &elem.kind {
                ExprKind::Variable(name) => names.push(name.clone()),
                _ => {
                    return Err(self.format_err(
                        self.peek(),
                        "Destructuring assignment requires variable names on left side",
                    ));
                }
            }
        }
        Ok(names)
    }

    /// Extract object bindings from object literal for destructuring assignment
    fn extract_object_bindings(
        &self,
        pairs: &[(String, Expr)],
    ) -> Result<Vec<(String, Option<String>)>, LangError> {
        let mut bindings = Vec::new();
        for (key, value) in pairs {
            match &value.kind {
                ExprKind::Variable(var_name) => {
                    // {x: y} means extract property x and assign to variable y
                    if key == var_name {
                        // Shorthand: {x} means {x: x}
                        bindings.push((key.clone(), None));
                    } else {
                        bindings.push((key.clone(), Some(var_name.clone())));
                    }
                }
                _ => {
                    return Err(self.format_err(
                        self.peek(),
                        "Object destructuring assignment requires variable names on left side",
                    ));
                }
            }
        }
        Ok(bindings)
    }
}
