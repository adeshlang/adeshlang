//! Array Builtins
//!
//! Implements `map`, `filter`, and `reduce` over `Value::Array` using
//! native or user functions, with support for bound methods and closures.
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register("map", "collections", "Map over array elements", builtin_map);
    registry.register(
        "filter",
        "collections",
        "Filter array elements by predicate",
        builtin_filter,
    );
    registry.register(
        "reduce",
        "collections",
        "Reduce array elements",
        builtin_reduce,
    );
}

fn builtin_map(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("map(list, fn)".to_string());
    }
    let list = args[0].clone();
    let func = args[1].clone();

    match list {
        Value::Array(xs) => {
            let mut out = Vec::new();
            for v in xs {
                let r = match &func {
                    Value::Function(NativeFn(f)) => (f)(env, vec![v.clone()])?,
                    Value::UserFunction(u) => crate::execution::runtime::public_call_user(
                        u.clone(),
                        vec![v.clone()],
                        None,
                        env.native_side_effects(),
                    )?,
                    Value::BoundMethod(u, inst) => {
                        crate::execution::runtime::public_call_user_with_this(
                            u.clone(),
                            vec![v.clone()],
                            *inst.clone(),
                            None,
                            env.native_side_effects(),
                        )?
                    }
                    _ => return Err("map requires a function".to_string()),
                };
                out.push(r);
            }
            Ok(Value::Array(out))
        }
        Value::DynArray(da) => {
            let mut out = Vec::new();
            for v in da.data.iter() {
                let r = match &func {
                    Value::Function(NativeFn(f)) => (f)(env, vec![v.clone()])?,
                    Value::UserFunction(u) => crate::execution::runtime::public_call_user(
                        u.clone(),
                        vec![v.clone()],
                        None,
                        env.native_side_effects(),
                    )?,
                    Value::BoundMethod(u, inst) => {
                        crate::execution::runtime::public_call_user_with_this(
                            u.clone(),
                            vec![v.clone()],
                            *inst.clone(),
                            None,
                            env.native_side_effects(),
                        )?
                    }
                    _ => return Err("map requires a function".to_string()),
                };
                out.push(r);
            }
            Ok(Value::DynArray(Box::new(
                crate::parsing::ast::DynamicArray::new(out),
            )))
        }
        _ => Err("map requires array".to_string()),
    }
}

fn builtin_filter(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("filter(list, fn)".to_string());
    }
    let list = args[0].clone();
    let func = args[1].clone();

    match list {
        Value::Array(xs) => {
            let mut out = Vec::new();
            for v in xs {
                let r = match &func {
                    Value::Function(NativeFn(f)) => (f)(env, vec![v.clone()])?,
                    Value::UserFunction(u) => crate::execution::runtime::public_call_user(
                        u.clone(),
                        vec![v.clone()],
                        None,
                        env.native_side_effects(),
                    )?,
                    Value::BoundMethod(u, inst) => {
                        crate::execution::runtime::public_call_user_with_this(
                            u.clone(),
                            vec![v.clone()],
                            *inst.clone(),
                            None,
                            env.native_side_effects(),
                        )?
                    }
                    _ => return Err("filter requires a function".to_string()),
                };
                if r.truthy() {
                    out.push(v);
                }
            }
            Ok(Value::Array(out))
        }
        Value::DynArray(da) => {
            let mut out = Vec::new();
            for v in da.data.iter() {
                let r = match &func {
                    Value::Function(NativeFn(f)) => (f)(env, vec![v.clone()])?,
                    Value::UserFunction(u) => crate::execution::runtime::public_call_user(
                        u.clone(),
                        vec![v.clone()],
                        None,
                        env.native_side_effects(),
                    )?,
                    Value::BoundMethod(u, inst) => {
                        crate::execution::runtime::public_call_user_with_this(
                            u.clone(),
                            vec![v.clone()],
                            *inst.clone(),
                            None,
                            env.native_side_effects(),
                        )?
                    }
                    _ => return Err("filter requires a function".to_string()),
                };
                if r.truthy() {
                    out.push(v.clone());
                }
            }
            Ok(Value::DynArray(Box::new(
                crate::parsing::ast::DynamicArray::new(out),
            )))
        }
        _ => Err("filter requires array".to_string()),
    }
}

fn builtin_reduce(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("reduce(list, fn, initial?)".to_string());
    }
    let list = args[0].clone();
    let func = args[1].clone();
    let initial = if args.len() == 3 {
        args[2].clone()
    } else {
        Value::Null
    };

    match list {
        Value::Array(xs) => {
            if xs.is_empty() && args.len() == 2 {
                return Err("reduce of empty array with no initial value".to_string());
            }
            let mut acc = if args.len() == 3 {
                initial
            } else {
                xs[0].clone()
            };
            let start_idx = if args.len() == 3 { 0 } else { 1 };
            for i in start_idx..xs.len() {
                let v = xs[i].clone();
                let r = match &func {
                    Value::Function(NativeFn(f)) => (f)(env, vec![acc.clone(), v.clone()])?,
                    Value::UserFunction(u) => crate::execution::runtime::public_call_user(
                        u.clone(),
                        vec![acc.clone(), v.clone()],
                        None,
                        env.native_side_effects(),
                    )?,
                    Value::BoundMethod(u, inst) => {
                        crate::execution::runtime::public_call_user_with_this(
                            u.clone(),
                            vec![acc.clone(), v.clone()],
                            *inst.clone(),
                            None,
                            env.native_side_effects(),
                        )?
                    }
                    _ => return Err("reduce requires a function".to_string()),
                };
                acc = r;
            }
            Ok(acc)
        }
        Value::DynArray(da) => {
            if da.data.is_empty() && args.len() == 2 {
                return Err("reduce of empty array with no initial value".to_string());
            }
            let mut acc = if args.len() == 3 {
                initial
            } else {
                da.data[0].clone()
            };
            let start_idx = if args.len() == 3 { 0 } else { 1 };
            for i in start_idx..da.data.len() {
                let v = da.data[i].clone();
                let r = match &func {
                    Value::Function(NativeFn(f)) => (f)(env, vec![acc.clone(), v.clone()])?,
                    Value::UserFunction(u) => crate::execution::runtime::public_call_user(
                        u.clone(),
                        vec![acc.clone(), v.clone()],
                        None,
                        env.native_side_effects(),
                    )?,
                    Value::BoundMethod(u, inst) => {
                        crate::execution::runtime::public_call_user_with_this(
                            u.clone(),
                            vec![acc.clone(), v.clone()],
                            *inst.clone(),
                            None,
                            env.native_side_effects(),
                        )?
                    }
                    _ => return Err("reduce requires a function".to_string()),
                };
                acc = r;
            }
            Ok(acc)
        }
        _ => Err("reduce requires array".to_string()),
    }
}
