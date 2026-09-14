use crate::execution::runtime_core::{
    err,
    interpreter::property_access::{get_prop_exec, set_prop_exec},
    ops::to_index,
};
use crate::parsing::ast::{Expr, ExprKind, Value};

use super::super::core::Exec;

impl Exec {
    pub(super) fn eval_get(&mut self, obj: &Expr, key: &str) -> Result<Value, String> {
        let mut o = self.eval_expr(obj)?;
        if let Value::Ref(inner, _handle) = o {
            o = (*inner).clone();
        }
        let class_context = self.current_class_context.clone();
        get_prop_exec(o, key, class_context.as_deref(), self)
    }

    pub(super) fn eval_set(&mut self, obj: &Expr, key: &str, v: &Expr) -> Result<Value, String> {
        let vv = self.eval_expr(v)?;
        match &obj.kind {
            ExprKind::Variable(name) => {
                let mut c = Some(self.current);
                while let Some(id) = c {
                    if let Some(mut target) = self.envs[id].values.remove(name) {
                        let class_context = self.current_class_context.clone();
                        let res = set_prop_exec(
                            &mut target,
                            key,
                            vv.clone(),
                            class_context.as_deref(),
                            self,
                        );
                        self.envs[id].values.insert(name.clone(), target);
                        res?;
                        return Ok(vv);
                    }
                    c = self.envs[id].enclosing;
                }
                return Err(err(format!("Undefined '{}'", name)));
            }
            _ => {
                let mut o = self.eval_expr(obj)?;
                let class_context = self.current_class_context.clone();
                set_prop_exec(&mut o, key, vv.clone(), class_context.as_deref(), self)?;
                Ok(vv)
            }
        }
    }

    pub(super) fn eval_index(&mut self, target: &Expr, index: &Expr) -> Result<Value, String> {
        let tv = self.eval_expr(target)?;
        // unwrap shared borrow refs so indexing delegates to inner value
        let tv = match tv {
            Value::Ref(inner, _handle) => (*inner).clone(),
            other => other,
        };
        let iv = self.eval_expr(index)?;
        match (tv, iv) {
            (Value::I64(ptr), idx) => {
                if !crate::execution::runtime_core::in_unsafe_context() {
                    return Err(err("pointer dereference requires unsafe { ... } block"));
                }
                let offset = to_index(&idx)?;
                let ptr_u64 = ptr as u64;
                let elem_size = crate::backends::unsafe_heap::elem_size_of_ptr(ptr_u64)
                    .map_err(|e| err(e))?;
                if elem_size == 1 {
                    let byte_val = crate::backends::unsafe_heap::load_u8(ptr_u64, offset)
                        .map_err(|e| err(e))?;
                    Ok(Value::U8(byte_val))
                } else {
                    let bytes = crate::backends::unsafe_heap::load_typed(ptr_u64, offset)
                        .map_err(|e| err(e))?;
                    match elem_size {
                        2 => {
                            let val = i16::from_le_bytes([bytes[0], bytes[1]]);
                            Ok(Value::I16(val))
                        }
                        4 => {
                            let val = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                            Ok(Value::I32(val))
                        }
                        8 => {
                            let val = i64::from_le_bytes(bytes[0..8].try_into().unwrap());
                            Ok(Value::I64(val))
                        }
                        _ => {
                            let byte_val = bytes.first().copied().unwrap_or(0);
                            Ok(Value::U8(byte_val))
                        }
                    }
                }
            }
            (Value::U64(ptr), idx) => {
                if !crate::execution::runtime_core::in_unsafe_context() {
                    return Err(err("pointer dereference requires unsafe { ... } block"));
                }
                let offset = to_index(&idx)?;
                let ptr_u64 = ptr;
                let elem_size = crate::backends::unsafe_heap::elem_size_of_ptr(ptr_u64)
                    .map_err(|e| err(e))?;
                if elem_size == 1 {
                    let byte_val = crate::backends::unsafe_heap::load_u8(ptr_u64, offset)
                        .map_err(|e| err(e))?;
                    Ok(Value::U8(byte_val))
                } else {
                    let bytes = crate::backends::unsafe_heap::load_typed(ptr_u64, offset)
                        .map_err(|e| err(e))?;
                    match elem_size {
                        2 => {
                            let val = i16::from_le_bytes([bytes[0], bytes[1]]);
                            Ok(Value::I16(val))
                        }
                        4 => {
                            let val = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                            Ok(Value::I32(val))
                        }
                        8 => {
                            let val = i64::from_le_bytes(bytes[0..8].try_into().unwrap());
                            Ok(Value::I64(val))
                        }
                        _ => {
                            let byte_val = bytes.first().copied().unwrap_or(0);
                            Ok(Value::U8(byte_val))
                        }
                    }
                }
            }
            (Value::Object(m), Value::Str(k)) => Ok(m.get(&k).cloned().unwrap_or(Value::Null)),
            (Value::Array(a), i) => {
                let idx = to_index(&i)?;
                Ok(a.get(idx).cloned().unwrap_or(Value::Null))
            }
            (Value::DynArray(da), i) => {
                let idx = to_index(&i)?;
                Ok(da.data.get(idx).cloned().unwrap_or(Value::Null))
            }
            (Value::RawArray(_, raw), i) => {
                let idx = to_index(&i)?;
                Ok(raw.get(idx).cloned().unwrap_or(Value::Null))
            }
            (Value::Tuple(t), i) => {
                let idx = to_index(&i)?;
                Ok(t.get(idx).cloned().unwrap_or(Value::Null))
            }
            (Value::Str(s), i) => {
                let idx = to_index(&i)?;
                if let Some(ch) = s.chars().nth(idx) { Ok(Value::Char(ch)) } else { Ok(Value::Null) }
            }
            (Value::Instance(inst), idx) => {
                // Check if operator[] overloading is implemented
                if let Some(fns) = inst.class.methods.get("operator[]") {
                    if let Some(sel) = fns.first() {
                        use crate::execution::runtime_core::interpreter_core::call_user_with_this;
                        let (val, _updated) = call_user_with_this(
                            sel.clone(),
                            vec![idx.clone()],
                            inst.clone(),
                            Some(self.envs[0].values.clone()),
                            self.native_side_effects.clone(),
                        )?;
                        return Ok(val);
                    }
                }
                match idx {
                    Value::Str(k) => {
                        let class_context = self.current_class_context.clone();
                        get_prop_exec(Value::Instance(inst), &k, class_context.as_deref(), self)
                    }
                    _ => Err(err("indexing types not supported - array[number], string[number], tuple[number], object[string], instance[string] are supported".to_string())),
                }
            }
            _ => Err(err("indexing types not supported - array[number], string[number], tuple[number], object[string], instance[string] are supported".to_string())),
        }
    }
}
