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
        self.enter_recursion()?;
        let res = self.assignment();
        self.leave_recursion();
        res
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
        let mut e = self.or()?;
        if self.matchk(&[TokenKind::Question]) {
            let then_e = self.expression()?;
            self.consume(TokenKind::Colon, "Expect ':' in ternary")?;
            let else_e = self.expression()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Conditional(Box::new(e), Box::new(then_e), Box::new(else_e)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn or(&mut self) -> Result<Expr, LangError> {
        let mut e = self.and()?;
        while self.matchk(&[TokenKind::Or, TokenKind::OrOr]) {
            let r = self.and()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Logical(Box::new(e), TokenKind::Or, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn and(&mut self) -> Result<Expr, LangError> {
        let mut e = self.nullish()?;
        while self.matchk(&[TokenKind::And, TokenKind::AndAnd]) {
            let r = self.nullish()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Logical(Box::new(e), TokenKind::And, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn nullish(&mut self) -> Result<Expr, LangError> {
        let mut e = self.bit_or()?;
        while self.matchk(&[TokenKind::NullCoalesce]) {
            let r = self.bit_or()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), TokenKind::NullCoalesce, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn bit_or(&mut self) -> Result<Expr, LangError> {
        let mut e = self.bit_xor()?;
        while self.matchk(&[TokenKind::Pipe]) {
            let r = self.bit_xor()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), TokenKind::Pipe, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn bit_xor(&mut self) -> Result<Expr, LangError> {
        let mut e = self.bit_and()?;
        while self.matchk(&[TokenKind::Caret]) {
            let r = self.bit_and()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), TokenKind::Caret, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn bit_and(&mut self) -> Result<Expr, LangError> {
        let mut e = self.equality()?;
        while self.matchk(&[TokenKind::Ampersand]) {
            let r = self.equality()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), TokenKind::Ampersand, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn equality(&mut self) -> Result<Expr, LangError> {
        let mut e = self.comparison()?;
        while self.matchk(&[
            TokenKind::BangEqual,
            TokenKind::EqualEqual,
            TokenKind::StrictEqual,
            TokenKind::StrictNotEqual,
        ]) {
            let op = self.prev().kind.clone();
            let r = self.comparison()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), op, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn comparison(&mut self) -> Result<Expr, LangError> {
        let mut e = self.shift()?;
        while self.matchk(&[
            TokenKind::Greater,
            TokenKind::GreaterEqual,
            TokenKind::Less,
            TokenKind::LessEqual,
            TokenKind::Instanceof,
            TokenKind::In,
        ]) {
            let op = self.prev().kind.clone();
            let r = self.shift()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), op, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn shift(&mut self) -> Result<Expr, LangError> {
        let mut e = self.term()?;
        while self.matchk(&[TokenKind::ShiftLeft, TokenKind::ShiftRight]) {
            let op = self.prev().kind.clone();
            let r = self.term()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), op, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn term(&mut self) -> Result<Expr, LangError> {
        let mut e = self.factor()?;
        while self.matchk(&[TokenKind::Plus, TokenKind::Minus]) {
            let op = self.prev().kind.clone();
            let r = self.factor()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), op, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn factor(&mut self) -> Result<Expr, LangError> {
        let mut e = self.power()?;
        while self.matchk(&[
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::TildeSlash,
        ]) {
            let op = self.prev().kind.clone();
            let r = self.power()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), op, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn power(&mut self) -> Result<Expr, LangError> {
        let mut e = self.cast()?;
        while self.matchk(&[TokenKind::StarStar]) {
            let r = self.power()?;
            let span = e.span.clone();
            e = Expr {
                kind: ExprKind::Binary(Box::new(e), TokenKind::StarStar, Box::new(r)),
                span,
            };
        }
        Ok(e)
    }

    pub(super) fn cast(&mut self) -> Result<Expr, LangError> {
        let mut e = self.unary()?;
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
