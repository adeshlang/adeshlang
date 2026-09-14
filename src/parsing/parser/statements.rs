//! Statement parsing
//!
//! This module handles parsing of:
//! - return statements
//! - try-catch blocks
//! - if/elif/else statements
//! - do-while, while, for loops
//! - break, continue, jump, region, defer, unsafe blocks
//! - defer validation (no await allowed)

use super::core::Parser;
use crate::parsing::ast::{Expr, ExprKind, Span, Stmt, StmtKind, TokenKind, Value};
use crate::parsing::error::{ErrorKind, LangError};

impl Parser {
    pub(super) fn return_stmt(&mut self) -> Result<Stmt, LangError> {
        let v = if !self.check(TokenKind::Semicolon) {
            let first = self.expression()?;
            if self.matchk(&[TokenKind::Comma]) {
                let mut elements = vec![first];
                loop {
                    elements.push(self.expression()?);
                    if !self.matchk(&[TokenKind::Comma]) {
                        break;
                    }
                }
                let span = elements[0].span.clone();
                Some(Expr {
                    kind: ExprKind::Tuple(elements),
                    span,
                })
            } else {
                Some(first)
            }
        } else {
            None
        };
        self.consume(TokenKind::Semicolon, "Expect ';' after return")?;
        Ok(Stmt {
            kind: StmtKind::Return(v),
            span: self.previous_span(),
        })
    }

    pub(super) fn try_catch(&mut self) -> Result<Stmt, LangError> {
        let start_span = self.previous_span(); // try token
        self.consume(TokenKind::LeftBrace, "Expect '{' after try")?;
        let try_b = self.block()?;
        let try_stmt = Stmt {
            kind: StmtKind::Block(try_b),
            span: self.previous_span(),
        };

        self.consume(TokenKind::Catch, "Expect 'catch' after try block")?;
        self.consume(TokenKind::LeftParen, "Expect '(' after catch")?;
        let err = self.consume_ident("Expect error name")?;
        self.consume(TokenKind::RightParen, "Expect ')' after catch param")?;
        self.consume(TokenKind::LeftBrace, "Expect '{' for catch block")?;
        let catch_b = self.block()?;
        let catch_stmt = Stmt {
            kind: StmtKind::Block(catch_b),
            span: self.previous_span(),
        };

        Ok(Stmt {
            kind: StmtKind::TryCatch {
                try_block: Box::new(try_stmt),
                err_name: err,
                catch_block: Box::new(catch_stmt),
            },
            span: start_span,
        })
    }

    pub(super) fn statement(&mut self) -> Result<Stmt, LangError> {
        if self.matchk(&[TokenKind::Semicolon]) {
            return Ok(Stmt {
                kind: StmtKind::ExprStmt(Expr {
                    kind: ExprKind::Literal(Value::Null),
                    span: self.previous_span(),
                }),
                span: self.previous_span(),
            });
        }
        if self.matchk(&[TokenKind::If]) {
            return self.if_stmt();
        }
        if self.matchk(&[TokenKind::Do]) {
            return self.do_while_stmt();
        }
        if self.matchk(&[TokenKind::While]) {
            return self.while_stmt();
        }
        if self.matchk(&[TokenKind::For]) {
            return self.for_stmt();
        }
        if self.matchk(&[TokenKind::Break]) {
            self.consume(TokenKind::Semicolon, "Expect ';' after break")
                .ok();
            return Ok(Stmt {
                kind: StmtKind::Break,
                span: self.previous_span(),
            });
        }
        if self.matchk(&[TokenKind::Continue]) {
            self.consume(TokenKind::Semicolon, "Expect ';' after continue")
                .ok();
            return Ok(Stmt {
                kind: StmtKind::Continue,
                span: self.previous_span(),
            });
        }
        if self.matchk(&[TokenKind::Jump]) {
            let e = self.expression()?;
            self.consume(TokenKind::Semicolon, "Expect ';' after jump")
                .ok();
            return Ok(Stmt {
                kind: StmtKind::Jump(e),
                span: self.previous_span(),
            });
        }
        if self.check(TokenKind::Identifier) && self.peek().lexeme == "delete" {
            self.advance();
            let start_span = self.previous_span();
            let expr = self.expression()?;
            self.consume(TokenKind::Semicolon, "Expect ';' after delete")?;
            return Ok(Stmt {
                kind: StmtKind::ExprStmt(Expr {
                    kind: ExprKind::AssignOp(
                        Box::new(expr),
                        TokenKind::Equal,
                        Box::new(Expr {
                            kind: ExprKind::Literal(Value::Null),
                            span: start_span.clone(),
                        }),
                    ),
                    span: start_span.clone(),
                }),
                span: start_span,
            });
        }
        if self.matchk(&[TokenKind::Region]) {
            let start_span = self.previous_span();
            let name = if self.check(TokenKind::Identifier) {
                Some(self.consume_ident("Expect region name")?)
            } else {
                None
            };
            self.consume(TokenKind::LeftBrace, "Expect '{' after region")?;
            let body_stmts = self.block()?;
            return Ok(Stmt {
                kind: StmtKind::Region {
                    name,
                    body: Box::new(Stmt {
                        kind: StmtKind::Block(body_stmts),
                        span: self.previous_span(),
                    }),
                },
                span: start_span,
            });
        }
        if self.matchk(&[TokenKind::Defer]) {
            return self.defer_stmt();
        }
        if self.matchk(&[TokenKind::Unsafe]) {
            let start_span = self.previous_span();
            self.consume(TokenKind::LeftBrace, "Expect '{' after unsafe")?;
            let body_stmts = self.block()?;
            return Ok(Stmt {
                kind: StmtKind::UnsafeBlock(Box::new(Stmt {
                    kind: StmtKind::Block(body_stmts),
                    span: self.previous_span(),
                })),
                span: start_span,
            });
        }
        if self.matchk(&[TokenKind::LeftBrace]) {
            let start_span = self.previous_span();
            return Ok(Stmt {
                kind: StmtKind::Block(self.block()?),
                span: start_span,
            });
        }
        // Special case: match expression as statement (optional semicolon)
        if self.matchk(&[TokenKind::Match]) {
            let m = self.match_expr()?;
            // Optional semicolon for match statements
            self.matchk(&[TokenKind::Semicolon]);
            return Ok(Stmt {
                kind: StmtKind::ExprStmt(m),
                span: self.previous_span(),
            });
        }
        let e = self.expression()?;
        self.consume(TokenKind::Semicolon, "Expect ';' after statement")?;
        Ok(Stmt {
            kind: StmtKind::ExprStmt(e),
            span: self.previous_span(),
        })
    }

    pub(super) fn defer_stmt(&mut self) -> Result<Stmt, LangError> {
        let start_span = self.previous_span();
        self.consume(TokenKind::LeftBrace, "Expect '{' after defer")?;
        let body_stmts = self.block()?;

        // Validate: defer blocks cannot contain await expressions
        let defer_block = Stmt {
            kind: StmtKind::Block(body_stmts),
            span: self.previous_span(),
        };

        if Self::contains_await(&defer_block) {
            return Err(LangError::new(
                ErrorKind::User,
                "defer blocks cannot contain 'await' expressions".to_string(),
                start_span.line,
                start_span.col,
                start_span.line_text.clone(),
            )
            .with_auto_hints()
            .with_file_opt(self.module.clone()));
        }

        Ok(Stmt {
            kind: StmtKind::Defer(Box::new(defer_block)),
            span: start_span,
        })
    }

    // Helper function to check if a statement contains await expressions
    pub(super) fn contains_await(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Block(stmts) => stmts.iter().any(Self::contains_await),
            StmtKind::ExprStmt(expr) => Self::expr_contains_await(expr),
            StmtKind::Let(_, Some(init), _, _, _, _) => Self::expr_contains_await(init),
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                Self::expr_contains_await(cond)
                    || Self::contains_await(then_branch)
                    || else_branch
                        .as_ref()
                        .map_or(false, |e| Self::contains_await(e))
            }
            StmtKind::While { cond, body } => {
                Self::expr_contains_await(cond) || Self::contains_await(body)
            }
            StmtKind::ForIn { iter, body, .. } => {
                Self::expr_contains_await(iter) || Self::contains_await(body)
            }
            StmtKind::Return(Some(expr)) => Self::expr_contains_await(expr),
            StmtKind::TryCatch {
                try_block,
                catch_block,
                ..
            } => Self::contains_await(try_block) || Self::contains_await(catch_block),
            _ => false,
        }
    }

    pub(super) fn expr_contains_await(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Await(_) => true,
            ExprKind::Binary(left, _, right) | ExprKind::Logical(left, _, right) => {
                Self::expr_contains_await(left) || Self::expr_contains_await(right)
            }
            ExprKind::Unary(_, inner)
            | ExprKind::Grouping(inner)
            | ExprKind::Spawn(inner)
            | ExprKind::Throw(inner)
            | ExprKind::Spread(inner)
            | ExprKind::NonNull(inner)
            | ExprKind::Try(inner) => Self::expr_contains_await(inner),
            ExprKind::Call(callee, args, _) | ExprKind::OptCall(callee, args, _) => {
                Self::expr_contains_await(callee) || args.iter().any(Self::expr_contains_await)
            }
            ExprKind::Index(obj, idx) | ExprKind::Range(obj, idx, _) => {
                Self::expr_contains_await(obj) || Self::expr_contains_await(idx)
            }
            ExprKind::Get(obj, _) | ExprKind::OptGet(obj, _) => Self::expr_contains_await(obj),
            ExprKind::Set(obj, _, val) => {
                Self::expr_contains_await(obj) || Self::expr_contains_await(val)
            }
            ExprKind::Fn(_, body, _) => body.iter().any(Self::contains_await),
            ExprKind::Array(elements)
            | ExprKind::Tuple(elements)
            | ExprKind::SetLiteral(elements) => elements.iter().any(Self::expr_contains_await),
            ExprKind::Object(pairs) | ExprKind::StructLiteral(_, pairs) => {
                pairs.iter().any(|(_, v)| Self::expr_contains_await(v))
            }
            ExprKind::Match(expr, arms) => {
                Self::expr_contains_await(expr)
                    || arms.iter().any(|(_, body)| Self::expr_contains_await(body))
            }
            ExprKind::Conditional(cond, then_expr, else_expr) => {
                Self::expr_contains_await(cond)
                    || Self::expr_contains_await(then_expr)
                    || Self::expr_contains_await(else_expr)
            }
            ExprKind::Update(_, _, expr) => Self::expr_contains_await(expr),
            ExprKind::New(constructor, args) => {
                Self::expr_contains_await(constructor) || args.iter().any(Self::expr_contains_await)
            }
            ExprKind::Format(expr, _) => Self::expr_contains_await(expr),
            ExprKind::Assign(_, expr) | ExprKind::AssignOp(expr, _, _) => {
                Self::expr_contains_await(expr)
            }
            _ => false,
        }
    }

    pub(super) fn if_stmt(&mut self) -> Result<Stmt, LangError> {
        let start_span = self.previous_span(); // if token

        let old_allow = self.allow_struct_literal;
        self.allow_struct_literal = false;
        let c = self.expression()?;
        self.allow_struct_literal = old_allow;

        self.consume(TokenKind::LeftBrace, "Expect '{'")?;
        let then = self.block()?;
        // Support Python-style elif chain by desugaring to nested If inside else_branch
        let mut elifs: Vec<(Expr, Vec<Stmt>, Span)> = Vec::new(); // Added Span for elif block
        while self.matchk(&[TokenKind::Elif]) {
            self.allow_struct_literal = false;
            let ec = self.expression()?;
            self.allow_struct_literal = old_allow;
            self.consume(TokenKind::LeftBrace, "Expect '{' after elif condition")?;
            let eb = self.block()?;
            elifs.push((ec, eb, self.previous_span()));
        }
        let mut el: Option<Box<Stmt>> = if self.matchk(&[TokenKind::Else]) {
            if self.matchk(&[TokenKind::If]) {
                Some(Box::new(self.if_stmt()?))
            } else {
                self.consume(TokenKind::LeftBrace, "Expect '{' after else")?;
                Some(Box::new(Stmt {
                    kind: StmtKind::Block(self.block()?),
                    span: self.previous_span(),
                }))
            }
        } else {
            None
        };
        for (ec, eb, sp) in elifs.into_iter().rev() {
            let nested = Stmt {
                kind: StmtKind::If {
                    cond: ec,
                    then_branch: Box::new(Stmt {
                        kind: StmtKind::Block(eb),
                        span: sp,
                    }),
                    else_branch: el.clone(),
                },
                span: Span::default(),
            }; // We might need better span tracking here for desugared elif
            el = Some(Box::new(nested));
        }
        Ok(Stmt {
            kind: StmtKind::If {
                cond: c,
                then_branch: Box::new(Stmt {
                    kind: StmtKind::Block(then),
                    span: self.previous_span(),
                }), // Approximate span for then block
                else_branch: el,
            },
            span: start_span,
        })
    }

    pub(super) fn do_while_stmt(&mut self) -> Result<Stmt, LangError> {
        let start_span = self.previous_span(); // do token
        // Parse: do { block } while (cond); or do { block } while cond;
        self.consume(TokenKind::LeftBrace, "Expect '{' after do")?;
        let body_vec = self.block()?;
        self.consume(TokenKind::While, "Expect 'while' after do block")?;

        let has_paren = self.matchk(&[TokenKind::LeftParen]);
        let old_allow = self.allow_struct_literal;
        self.allow_struct_literal = false;
        let cond = self.expression()?;
        self.allow_struct_literal = old_allow;

        if has_paren {
            self.consume(TokenKind::RightParen, "Expect ')' after condition")?;
        }
        // Semicolon after do-while is optional
        self.matchk(&[TokenKind::Semicolon]);

        // Desugar to:
        // {
        //     let mut __do_first = true;
        //     while (__do_first || cond) {
        //         __do_first = false;
        //         body;
        //     }
        // }
        // Why this desugaring is 100% defect-free:
        // 1. On iteration 1, `__do_first` is true, so `body` runs WITHOUT evaluating `cond`.
        // 2. `__do_first = false` sets flag for subsequent checks.
        // 3. Any `continue` inside `body` jumps to `while (__do_first || cond)`.
        //    Since `__do_first` is now false, it correctly evaluates `cond`!
        // 4. Any `break` inside `body` breaks out of the loop immediately!
        let flag_name = format!("__do_first_{}", start_span.line);
        let flag_decl = Stmt {
            kind: StmtKind::Let(
                flag_name.clone(),
                Some(Expr {
                    kind: ExprKind::Literal(Value::Bool(true)),
                    span: Span::default(),
                }),
                Some("bool".to_string()),
                false,
                false,
                false,
            ),
            span: Span::default(),
        };

        let flag_reset = Stmt {
            kind: StmtKind::ExprStmt(Expr {
                kind: ExprKind::AssignOp(
                    Box::new(Expr {
                        kind: ExprKind::Variable(flag_name.clone()),
                        span: Span::default(),
                    }),
                    TokenKind::Equal,
                    Box::new(Expr {
                        kind: ExprKind::Literal(Value::Bool(false)),
                        span: Span::default(),
                    }),
                ),
                span: Span::default(),
            }),
            span: Span::default(),
        };

        let while_cond = Expr {
            kind: ExprKind::Logical(
                Box::new(Expr {
                    kind: ExprKind::Variable(flag_name),
                    span: Span::default(),
                }),
                TokenKind::OrOr,
                Box::new(cond),
            ),
            span: Span::default(),
        };

        let mut loop_body = vec![flag_reset];
        loop_body.extend(body_vec);

        let while_stmt = Stmt {
            kind: StmtKind::While {
                cond: while_cond,
                body: Box::new(Stmt {
                    kind: StmtKind::Block(loop_body),
                    span: Span::default(),
                }),
            },
            span: start_span.clone(),
        };

        Ok(Stmt {
            kind: StmtKind::Block(vec![flag_decl, while_stmt]),
            span: start_span,
        })
    }

    pub(super) fn while_stmt(&mut self) -> Result<Stmt, LangError> {
        let start_span = self.previous_span(); // while token
        let old_allow = self.allow_struct_literal;
        self.allow_struct_literal = false;
        let c = self.expression()?;
        self.allow_struct_literal = old_allow;
        self.consume(TokenKind::LeftBrace, "Expect '{'")?;
        let b = self.block()?;
        Ok(Stmt {
            kind: StmtKind::While {
                cond: c,
                body: Box::new(Stmt {
                    kind: StmtKind::Block(b),
                    span: self.previous_span(),
                }),
            },
            span: start_span,
        })
    }

    pub(super) fn for_stmt(&mut self) -> Result<Stmt, LangError> {
        let start_span = self.previous_span(); // for token
        // Support two forms:
        // 1) Concise: for x in arr { ... }
        // 2) Parenthesized: for (x in arr) { ... } OR classic for (let i = 0; i < 10; i = i + 1) { ... }
        if !self.check(TokenKind::LeftParen) {
            // concise for-in/of without parentheses
            let name = self.consume_ident("Expect loop var")?;
            if self.matchk(&[TokenKind::In]) { /* ok */
            } else {
                self.consume(TokenKind::Of, "Expect 'in' or 'of'")?;
            }
            let old_allow = self.allow_struct_literal;
            self.allow_struct_literal = false;
            let it = self.expression()?;
            self.allow_struct_literal = old_allow;
            self.consume(TokenKind::LeftBrace, "Expect '{' for body")?;
            let b = self.block()?;
            return Ok(Stmt {
                kind: StmtKind::ForIn {
                    name,
                    iter: it,
                    body: Box::new(Stmt {
                        kind: StmtKind::Block(b),
                        span: self.previous_span(),
                    }),
                },
                span: start_span,
            });
        }

        // Parenthesized form: decide between for-in/of and classic for(init; cond; post)
        let saved = self.current;
        self.consume(TokenKind::LeftParen, "Expect '('")?;

        // Scan ahead to see if there's a semicolon before the matching right paren
        let mut i = self.current;
        let mut has_semi = false;
        let mut depth = 1;
        while i < self.tokens.len() && depth > 0 {
            if self.tokens[i].kind == TokenKind::LeftParen {
                depth += 1;
            } else if self.tokens[i].kind == TokenKind::RightParen {
                depth -= 1;
            } else if depth == 1 && self.tokens[i].kind == TokenKind::Semicolon {
                has_semi = true;
                break;
            }
            i += 1;
        }

        // Rewind to after '('
        self.current = saved;
        self.consume(TokenKind::LeftParen, "Expect '('")?;

        if !has_semi {
            // for-in/of: for (x in arr)
            let name = self.consume_ident("Expect loop var")?;
            if self.matchk(&[TokenKind::In]) { /* ok */
            } else {
                self.consume(TokenKind::Of, "Expect 'in' or 'of'")?;
            }
            let old_allow = self.allow_struct_literal;
            self.allow_struct_literal = false;
            let it = self.expression()?;
            self.allow_struct_literal = old_allow;
            self.consume(TokenKind::RightParen, "Expect ')' after for(...)")?;
            self.consume(TokenKind::LeftBrace, "Expect '{' for body")?;
            let b = self.block()?;
            return Ok(Stmt {
                kind: StmtKind::ForIn {
                    name,
                    iter: it,
                    body: Box::new(Stmt {
                        kind: StmtKind::Block(b),
                        span: self.previous_span(),
                    }),
                },
                span: start_span,
            });
        }

        // Classic C-style for: for (init; cond; post)
        let init_stmt: Option<Stmt> = if self.check(TokenKind::Semicolon) {
            self.consume(TokenKind::Semicolon, "Expect ';'")?;
            None
        } else if self.matchk(&[TokenKind::Let]) {
            // Support `let i = 0` or `let i = 0, j = 10` inside for (init; ...)
            Some(self.let_decl(false, false, false)?)
        } else {
            let e = self.parse_tuple_rhs()?;
            self.consume(TokenKind::Semicolon, "Expect ';'")?;
            Some(Stmt {
                kind: StmtKind::ExprStmt(e),
                span: self.previous_span(),
            })
        };

        let cond: Expr = if self.check(TokenKind::Semicolon) {
            self.consume(TokenKind::Semicolon, "Expect ';'")?;
            Expr {
                kind: ExprKind::Literal(Value::Bool(true)),
                span: Span::default(),
            }
        } else {
            let old_allow = self.allow_struct_literal;
            self.allow_struct_literal = false;
            let e = self.expression()?;
            self.allow_struct_literal = old_allow;
            self.consume(TokenKind::Semicolon, "Expect ';'")?;
            e
        };

        let post: Option<Expr> = if self.check(TokenKind::RightParen) {
            self.consume(TokenKind::RightParen, "Expect ')'")?;
            None
        } else {
            let e = self.parse_tuple_rhs()?;
            self.consume(TokenKind::RightParen, "Expect ')'")?;
            Some(e)
        };

        self.consume(TokenKind::LeftBrace, "Expect '{' for body")?;
        let mut body_vec = self.block()?;

        // Transform any `continue` statements inside `body_vec` so `post` runs before continuing
        if let Some(ref pe) = post {
            for stmt in &mut body_vec {
                Self::transform_continue_for_post(stmt, pe);
            }
        }

        let mut desugared: Vec<Stmt> = Vec::new();
        if let Some(istmt) = init_stmt {
            if let StmtKind::Block(stmts) = istmt.kind {
                desugared.extend(stmts);
            } else {
                desugared.push(istmt);
            }
        }

        let mut inner: Vec<Stmt> = body_vec;
        if let Some(pe) = post {
            inner.push(Stmt {
                kind: StmtKind::ExprStmt(pe),
                span: Span::default(),
            });
        }

        desugared.push(Stmt {
            kind: StmtKind::While {
                cond,
                body: Box::new(Stmt {
                    kind: StmtKind::Block(inner),
                    span: Span::default(),
                }),
            },
            span: start_span.clone(),
        });

        Ok(Stmt {
            kind: StmtKind::Block(desugared),
            span: start_span,
        })
    }

    /// Helper to recursively inject `post` step execution before any `continue` statement in a `for` loop body
    fn transform_continue_for_post(stmt: &mut Stmt, post_expr: &Expr) {
        match &mut stmt.kind {
            StmtKind::Continue => {
                stmt.kind = StmtKind::Block(vec![
                    Stmt {
                        kind: StmtKind::ExprStmt(post_expr.clone()),
                        span: Span::default(),
                    },
                    Stmt {
                        kind: StmtKind::Continue,
                        span: Span::default(),
                    },
                ]);
            }
            StmtKind::Block(stmts) => {
                for s in stmts {
                    Self::transform_continue_for_post(s, post_expr);
                }
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::transform_continue_for_post(then_branch, post_expr);
                if let Some(eb) = else_branch {
                    Self::transform_continue_for_post(eb, post_expr);
                }
            }
            StmtKind::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                Self::transform_continue_for_post(try_block, post_expr);
                Self::transform_continue_for_post(catch_block, post_expr);
            }
            // Nested loop constructs manage their own `continue` statements
            StmtKind::While { .. } | StmtKind::ForIn { .. } => {}
            _ => {}
        }
    }

    pub(super) fn block(&mut self) -> Result<Vec<Stmt>, LangError> {
        let mut s = Vec::new();
        while !self.check(TokenKind::RightBrace) && !self.is_end() {
            s.extend(self.declaration()?);
        }
        self.consume(TokenKind::RightBrace, "Expect '}' after block")?;
        Ok(s)
    }
}
