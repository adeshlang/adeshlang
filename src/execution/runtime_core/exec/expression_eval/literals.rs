//! Literal expression evaluation (arrays, tuples, objects, sets, ranges, etc.)

use crate::execution::runtime_core::interpreter::construction_helpers::err_with_span;
use crate::execution::runtime_core::{err, ops::equals};
use crate::parsing::ast::{Expr, ExprKind, Span, Value};
use rustc_hash::FxHashMap as HashMap;

use super::super::core::Exec;

impl Exec {
    pub(super) fn eval_literal(&mut self, v: &Value) -> Result<Value, String> {
        Ok(v.clone())
    }

    pub(super) fn eval_array(&mut self, es: &[Expr]) -> Result<Value, String> {
        let mut v = Vec::new();
        for e in es {
            match &e.kind {
                ExprKind::Spread(inner) => {
                    let spread_val = self.eval_expr(inner)?;
                    match spread_val {
                        Value::Array(arr) => v.extend(arr),
                        Value::DynArray(da) => v.extend(da.data),
                        Value::RawArray(_, raw) => v.extend(raw),
                        Value::Tuple(tup) => v.extend(tup),
                        _ => {
                            return Err(
                                "Spread operator can only be used on arrays or tuples".to_string()
                            );
                        }
                    }
                }
                _ => v.push(self.eval_expr(e)?),
            }
        }
        // Create a DynamicArray with automatic type inference
        use crate::parsing::ast::DynamicArray;
        Ok(Value::DynArray(Box::new(DynamicArray::new(v))))
    }

    pub(super) fn eval_tuple(&mut self, es: &[Expr]) -> Result<Value, String> {
        let mut v = Vec::new();
        for e in es {
            match &e.kind {
                ExprKind::Spread(inner) => {
                    let spread_val = self.eval_expr(inner)?;
                    match spread_val {
                        Value::Array(arr) => v.extend(arr),
                        Value::DynArray(da) => v.extend(da.data),
                        Value::RawArray(_, raw) => v.extend(raw),
                        Value::Tuple(tup) => v.extend(tup),
                        _ => {
                            return Err(
                                "Spread operator can only be used on arrays or tuples".to_string()
                            );
                        }
                    }
                }
                _ => v.push(self.eval_expr(e)?),
            }
        }
        Ok(Value::Tuple(v))
    }

    pub(super) fn eval_set_literal(&mut self, es: &[Expr]) -> Result<Value, String> {
        let mut out: Vec<Value> = Vec::new();
        for e in es {
            let val = self.eval_expr(e)?;
            if !out.iter().any(|x| equals(x, &val)) {
                out.push(val);
            }
        }
        Ok(Value::Set(out))
    }

    pub(super) fn eval_object(&mut self, kv: &[(String, Expr)]) -> Result<Value, String> {
        let mut m = HashMap::default();
        for (k, e) in kv {
            m.insert(k.clone(), self.eval_expr(e)?);
        }
        Ok(Value::Object(m.into()))
    }

    pub(super) fn eval_struct_literal(
        &mut self,
        name: &str,
        kv: &[(String, Expr)],
        span: &Span,
    ) -> Result<Value, String> {
        // look up the struct
        let s_val = self
            .get_fast(&name)
            .ok_or_else(|| err_with_span(format!("Undefined '{}'", name), span))?;
        if let Value::Struct(_s) = s_val {
            let mut m = HashMap::default();
            for (k, e) in kv {
                m.insert(k.clone(), self.eval_expr(e)?);
            }
            Ok(Value::Object(m.into()))
        } else {
            Err(err(format!("{} is not a struct", name)))
        }
    }

    pub(super) fn eval_range(
        &mut self,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
    ) -> Result<Value, String> {
        let start_val = self.eval_expr(start)?;
        let end_val = self.eval_expr(end)?;

        // Convert to f64 using num() to handle all numeric types
        let s = crate::execution::runtime::ops::num(start_val.clone())
            .map_err(|_| format!("Range start must be a number, got: {:?}", start_val))?;
        let e = crate::execution::runtime::ops::num(end_val.clone())
            .map_err(|_| format!("Range end must be a number, got: {:?}", end_val))?;

        let mut range = Vec::new();
        let mut current = s;
        if inclusive {
            while current <= e {
                range.push(Value::Number(current));
                current += 1.0;
            }
        } else {
            while current < e {
                range.push(Value::Number(current));
                current += 1.0;
            }
        }
        Ok(Value::Array(range))
    }
}
