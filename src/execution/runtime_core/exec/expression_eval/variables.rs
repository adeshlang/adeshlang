//! Variable access, assignment, and assign-ops evaluation

use super::super::core::Exec;
use super::super::stmt::is_copy_value;
use crate::execution::runtime_core::interpreter::construction_helpers::err_with_span;
use crate::execution::runtime_core::{
    err, get_prop,
    ops::{apply_assign_op, set_index_prop},
    set_prop,
};
use crate::parsing::ast::{Expr, ExprKind, Span, TokenKind, Value};
use crate::utils::memory::OwnershipTracker;
use std::rc::Rc;

impl Exec {
    pub(super) fn eval_variable(&mut self, name: &str, span: &Span) -> Result<Value, String> {
        // Disallow use-after-move
        if let Some(tr) = self.get_tracker(name) {
            if !tr.is_valid() {
                return Err(err(format!("use of moved value '{}'", name)));
            }
        }
        self.get_fast(name)
            .ok_or_else(|| err_with_span(format!("Undefined '{}'", name), span))
    }

    pub(super) fn eval_assign(
        &mut self,
        name: &str,
        rhs: &Expr,
        span: &Span,
    ) -> Result<Value, String> {
        // Move semantics when assigning from a variable with non-Copy value
        match &rhs.kind {
            ExprKind::Variable(src) => {
                let src_val = self
                    .get_fast(src)
                    .ok_or_else(|| err_with_span(format!("Undefined '{}'", src), &rhs.span))?;
                let is_copy = is_copy_value(&src_val);
                if !is_copy {
                    // mark source as moved if it has a tracker
                    if let Some(tr) = self.get_tracker(src) {
                        tr.mark_moved();
                    }
                }
                if !self.assign(name, src_val.clone()) {
                    if self.get_fast(name).is_some() {
                        return Err(err_with_span(
                            format!("Cannot assign to const '{}'", name),
                            span,
                        ));
                    }
                    return Err(err(format!("Undefined '{}'", name)));
                }
                // Initialize ownership for destination if non-Copy
                if !is_copy {
                    self.envs[self.current]
                        .ownership
                        .insert(name.to_string(), Rc::new(OwnershipTracker::new_unique()));
                } else {
                    self.envs[self.current].ownership.remove(name);
                }
                Ok(src_val)
            }
            _ => {
                let v = self.eval_expr(rhs)?;
                if !self.assign(name, v.clone()) {
                    if self.get_fast(name).is_some() {
                        return Err(err(format!("Cannot assign to const '{}'", name)));
                    }
                    return Err(err(format!("Undefined '{}'", name)));
                }
                // Initialize ownership for destination based on value kind
                if is_copy_value(&v) {
                    self.envs[self.current].ownership.remove(name);
                } else {
                    self.envs[self.current]
                        .ownership
                        .insert(name.to_string(), Rc::new(OwnershipTracker::new_unique()));
                }
                Ok(v)
            }
        }
    }

    pub(super) fn eval_assign_op(
        &mut self,
        target: &Expr,
        op: &TokenKind,
        rhs: &Expr,
    ) -> Result<Value, String> {
        let rv = self.eval_expr(rhs)?;
        match &target.kind {
            ExprKind::Variable(name) => {
                let current = self
                    .get_fast(name)
                    .ok_or_else(|| err(format!("Undefined '{}'", name)))?;
                let new_val = apply_assign_op(current, op.clone(), rv)?;
                if !self.assign(name, new_val.clone()) {
                    return Err(err(format!("Cannot assign to const '{}'", name)));
                }
                Ok(new_val)
            }
            ExprKind::Get(obj, key) => {
                let ov = self.eval_expr(obj)?;
                let current = get_prop(ov.clone(), key, self.current_class_context.as_deref())?;
                let new_val = apply_assign_op(current, op.clone(), rv)?;
                let mut o = ov;
                set_prop(
                    &mut o,
                    key,
                    new_val.clone(),
                    self.current_class_context.as_deref(),
                )?;
                Ok(new_val)
            }
            ExprKind::Index(obj, idx) => {
                let ov = self.eval_expr(obj)?;
                let iv = self.eval_expr(idx)?;
                let current = match (&ov, &iv) {
                    (Value::Object(m), Value::Str(k)) => m.get(k).cloned().unwrap_or(Value::Null),
                    (Value::Array(a), Value::Number(n)) => {
                        if (n - n.trunc()).abs() > 1e-12 {
                            return Err(err("index must be integer"));
                        }
                        let i = *n as usize;
                        a.get(i).cloned().unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                };
                let new_val = apply_assign_op(current, op.clone(), rv)?;
                set_index_prop(self.current, ov, iv, new_val.clone())?;
                Ok(new_val)
            }
            ExprKind::Unary(TokenKind::Star, ptr_expr) => {
                let pv = self.eval_expr(ptr_expr)?;
                match pv {
                    Value::Ref(inner, _handle) => {
                        let new_val = apply_assign_op(*inner, op.clone(), rv)?;
                        if let ExprKind::Variable(var_name) = &ptr_expr.kind {
                            self.assign(var_name, new_val.clone());
                        }
                        Ok(new_val)
                    }
                    _ => {
                        let new_val = rv;
                        set_index_prop(self.current, pv, Value::Number(0.0), new_val.clone())?;
                        Ok(new_val)
                    }
                }
            }
            _ => Err(err("Invalid assignment target")),
        }
    }

    pub(super) fn eval_assign_tuple(
        &mut self,
        names: &[String],
        rhs: &Expr,
        span: &Span,
    ) -> Result<Value, String> {
        let val = self.eval_expr(rhs)?;

        // Extract values from tuple or array
        let values = match &val {
            Value::Tuple(elems) => elems.clone(),
            Value::Array(elems) => elems.clone(),
            Value::DynArray(da) => da.data.clone(),
            _ => {
                let type_name = match &val {
                    Value::Number(_) => "number",
                    Value::Str(_) => "string",
                    Value::Bool(_) => "bool",
                    Value::Null => "null",
                    Value::Object(_) => "object",
                    _ => "other",
                };
                return Err(err_with_span(
                    format!(
                        "Destructuring assignment requires tuple or array on right side, got {}",
                        type_name
                    ),
                    span,
                ));
            }
        };

        if names.len() != values.len() {
            return Err(err_with_span(
                format!(
                    "Destructuring assignment: expected {} values, got {}",
                    names.len(),
                    values.len()
                ),
                span,
            ));
        }

        for (name, value) in names.iter().zip(values.iter()) {
            if !self.assign(name, value.clone()) {
                if self.get_fast(name).is_some() {
                    return Err(err_with_span(
                        format!("Cannot assign to const '{}'", name),
                        span,
                    ));
                }
                return Err(err_with_span(format!("Undefined '{}'", name), span));
            }
        }

        Ok(val)
    }

    pub(super) fn eval_assign_object(
        &mut self,
        bindings: &[(String, Option<String>)],
        rhs: &Expr,
        span: &Span,
    ) -> Result<Value, String> {
        let val = self.eval_expr(rhs)?;

        // Extract object properties
        let obj_map = match &val {
            Value::Object(map) => map.clone(),
            _ => {
                return Err(err_with_span(
                    "Object destructuring assignment requires an object on right side".to_string(),
                    span,
                ));
            }
        };

        for (key, alias) in bindings {
            let var_name = alias.as_ref().unwrap_or(key);
            let value = obj_map.get(key).cloned().unwrap_or(Value::Null);

            if !self.assign(var_name, value) {
                if self.get_fast(var_name).is_some() {
                    return Err(err_with_span(
                        format!("Cannot assign to const '{}'", var_name),
                        span,
                    ));
                }
                return Err(err_with_span(format!("Undefined '{}'", var_name), span));
            }
        }

        Ok(val)
    }
}
