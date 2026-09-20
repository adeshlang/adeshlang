//! Unary operations evaluation

use crate::execution::runtime_core::err;
use crate::execution::runtime_core::interpreter::construction_helpers::err_with_span;
use crate::parsing::ast::{Expr, ExprKind, TokenKind, Value};
use crate::typesystem::value_optimized::BorrowHandle;
// Use unified runtime ABI for negation and not operations
use crate::runtime::abi::{
    abi_negate,
    // NanValue variants for dynamic operations
    abi_negate_nan,
    abi_not,
    abi_not_nan,
    nanvalue_to_value,
    value_to_nanvalue,
};

use super::super::core::Exec;

/// Helper: Check if value can benefit from NanValue optimization for unary ops
#[inline]
fn can_use_nanvalue_unary(v: &Value) -> bool {
    matches!(
        v,
        Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Char(_)
            | Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F32(_)
            | Value::F64(_)
            | Value::BigInt(_)
    )
}

impl Exec {
    pub(super) fn eval_unary(&mut self, op: &TokenKind, r: &Expr) -> Result<Value, String> {
        let rv = self.eval_expr(r)?;
        match op {
            TokenKind::Tilde => crate::runtime::abi::bitwise::complement(&rv),
            TokenKind::Typeof => {
                let t = match &rv {
                    Value::Null => "null",
                    Value::Bool(_) => "boolean",
                    Value::Number(n) => {
                        if n.fract().abs() < 1e-12 {
                            let i = *n as i128;
                            if i >= 0 {
                                if i <= u8::MAX as i128 {
                                    "u8"
                                } else if i <= u16::MAX as i128 {
                                    "u16"
                                } else if i <= u32::MAX as i128 {
                                    "u32"
                                } else if i <= u64::MAX as i128 {
                                    "u64"
                                } else {
                                    "u128"
                                }
                            } else if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                                "i8"
                            } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                                "i16"
                            } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                                "i32"
                            } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                                "i64"
                            } else {
                                "i128"
                            }
                        } else {
                            "f64"
                        }
                    }
                    Value::BigInt(_) => "bigint",
                    Value::Char(_) => "char",
                    Value::Str(_) => "string",
                    Value::Array(_) => "array",
                    Value::RawArray(elem_type, _) => {
                        return Ok(Value::Str(format!("[{};raw]", elem_type)));
                    }
                    Value::DynArray(da) => {
                        return Ok(Value::Str(format!("[{:?}]", da.element_type)));
                    }
                    Value::Tuple(_) => "tuple",
                    Value::Set(_) => "set",
                    Value::Object(_) => "object",
                    Value::Class(c) => &c.name,
                    Value::Instance(i) => &i.class.name,
                    Value::BoundMethod(_, _) => "function",
                    Value::UserFunction(_) | Value::Function(_) => "function",
                    Value::Enum(_) => "enum",
                    Value::EnumCtor(_, _) => "enumctor",
                    Value::Promise(_) => "promise",
                    Value::Super(_, _) => "super",
                    Value::Complex(_, _) => "complex",
                    Value::Struct(_user_struct) => "struct",
                    Value::Interface(_user_interface) => "interface",
                    Value::BoundNative(_, _value) => "function",
                    Value::Ref(_, _) => "ref",
                    Value::Error(_lang_error) => "error",
                    // Fixed-width integer types
                    Value::U8(_) => "u8",
                    Value::U16(_) => "u16",
                    Value::U32(_) => "u32",
                    Value::U64(_) => "u64",
                    Value::U128(_) => "u128",
                    Value::I8(_) => "i8",
                    Value::I16(_) => "i16",
                    Value::I32(_) => "i32",
                    Value::I64(_) => "i64",
                    Value::I128(_) => "i128",
                    Value::F32(_) => "f32",
                    Value::F64(_) => "f64",
                    Value::Share(_) => "share",
                    Value::Weak(_) => "weak",
                    Value::LazyRange(..) => "range",
                };
                Ok(Value::Str(t.to_string()))
            }
            TokenKind::Plus => match rv {
                Value::Number(n) => Ok(Value::Number(n)),
                _ => Err(err("+ type error")),
            },
            TokenKind::Minus => {
                // Optimization: Use NanValue path for dynamic primitive negation
                if can_use_nanvalue_unary(&rv) {
                    let rnan = value_to_nanvalue(&rv).map_err(|e| e.message)?;
                    let result_nan = abi_negate_nan(&rnan).map_err(|e| e.message)?;
                    nanvalue_to_value(&result_nan).map_err(|e| e.message)
                } else {
                    // Fallback for complex types
                    abi_negate(&rv).map_err(|e| e.message)
                }
            }
            TokenKind::Bang => {
                // Optimization: Use NanValue path for dynamic primitive logical not
                if can_use_nanvalue_unary(&rv) {
                    let rnan = value_to_nanvalue(&rv).map_err(|e| e.message)?;
                    let result_nan = abi_not_nan(&rnan); // Returns NanValue directly
                    nanvalue_to_value(&result_nan).map_err(|e| e.message)
                } else {
                    // Fallback for complex types
                    abi_not(&rv).map_err(|e| e.message)
                }
            }
            TokenKind::Ampersand => {
                // Shared borrow of a variable
                if let ExprKind::Variable(name) = &r.kind {
                    let (env_id, val) = self
                        .get_with_env(name)
                        .ok_or_else(|| err_with_span(format!("Undefined '{}'", name), &r.span))?;
                    let tracker: std::rc::Rc<crate::utils::memory::OwnershipTracker> =
                        if let Some(t) = self.envs[env_id].ownership.get(name) {
                            t.clone()
                        } else {
                            let t = std::rc::Rc::new(
                                crate::utils::memory::OwnershipTracker::new_unique(),
                            );
                            self.envs[env_id].ownership.insert(name.clone(), t.clone());
                            t
                        };
                    let handle = BorrowHandle::new_shared(tracker)
                        .map_err(|e| err(format!("cannot borrow '{}': {}", name, e)))?;
                    Ok(Value::Ref(Box::new(val), handle))
                } else {
                    Err(err("& can only borrow variables".to_string()))
                }
            }
            _ => Err(err("bad unary")),
        }
    }
}
