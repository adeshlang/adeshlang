//! Control flow expressions (Throw, Match, Format, Grouping, Spread, Await, Spawn)

use crate::execution::runtime_core::{err, format::fmt};
use crate::parsing::ast::{Expr, Pattern, Value};

use super::super::core::Exec;

impl Exec {
    pub(super) fn eval_throw(&mut self, v: &Expr) -> Result<Value, String> {
        let msg = fmt(&self.eval_expr(v)?);
        Err(msg)
    }

    pub(super) fn eval_match(
        &mut self,
        value: &Expr,
        arms: &[(Pattern, Expr)],
    ) -> Result<Value, String> {
        let v = self.eval_expr(value)?;
        for (pat, expr) in arms {
            if self.match_pattern(pat, &v) {
                // Rust-like scope: acquire, use, release (deterministic drop)
                let new_env = self.acquire_scope(Some(self.current));
                let prev = self.current;
                self.current = new_env;
                let res = self.eval_expr(expr);
                self.current = prev;
                self.release_scope(new_env); // Deterministic cleanup
                return res;
            }
        }
        Err(err("no pattern matched"))
    }

    pub(super) fn eval_format(&mut self, inner: &Expr, spec: &str) -> Result<Value, String> {
        let val = self.eval_expr(inner)?;
        let formatted = crate::execution::runtime_core::format::apply_format_spec(&val, spec);
        Ok(Value::Str(formatted))
    }

    pub(super) fn eval_grouping(&mut self, g: &Expr) -> Result<Value, String> {
        self.eval_expr(g)
    }

    pub(super) fn eval_spread(&mut self, expr: &Expr) -> Result<Value, String> {
        let val = self.eval_expr(expr)?;
        Ok(val)
    }

    pub(super) fn eval_await(&mut self, _r: &Expr) -> Result<Value, String> {
        Err(err("await is only valid inside async functions"))
    }

    pub(super) fn eval_spawn(&mut self, _r: &Expr) -> Result<Value, String> {
        Err(err("spawn requires async runtime"))
    }
}
