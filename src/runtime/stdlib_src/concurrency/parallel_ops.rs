//! Parallel standard library — import Parallel; then Parallel.forEach(0, n, fn)
//!
//! ```adesh
//! import Parallel;
//! Parallel.forEach(0, 10, fn(i) { print(i); });
//! let total = Parallel.sum(data);
//! let mapped = Parallel.map(data, fn(x) { return x * 2; });
//! ```

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::scheduler::parallel_for as sched_parallel_for;
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "Parallel",
        "concurrency",
        "Parallel execution namespace — import Parallel; then Parallel.forEach(start, end, fn)",
        |_env: &mut dyn BuiltinEnv, _args| Ok(build_parallel_module_object()),
    );
}

/// Build the Parallel module object bound by `import Parallel;`
pub fn build_parallel_module_object() -> Value {
    let mut methods = HashMap::default();

    methods.insert(
        "map".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_map))),
    );
    methods.insert(
        "reduce".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_reduce))),
    );
    methods.insert(
        "forEach".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_for_each))),
    );
    methods.insert(
        "sum".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_sum))),
    );
    methods.insert(
        "workers".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_workers))),
    );

    Value::Object(Arc::new(methods))
}

fn builtin_map(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.map requires (array, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    let mut results = Vec::with_capacity(arr.len());
    for item in arr {
        results.push(call_fn(env, &args[1], vec![item])?);
    }
    Ok(Value::Array(results))
}

fn builtin_reduce(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Parallel.reduce requires (array, init, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    let mut acc = args[1].clone();
    for item in arr {
        acc = call_fn(env, &args[2], vec![acc, item])?;
    }
    Ok(acc)
}

fn builtin_for_each(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.forEach requires (start, end) or (start, end, function)".to_string());
    }
    let start = as_u64(&args[0], "start")?;
    let end = as_u64(&args[1], "end")?;

    if args.len() > 2 {
        if end - start < 256 {
            for i in start..end {
                call_fn(env, &args[2], vec![Value::Number(i as f64)])?;
            }
        } else {
            let chunk_size =
                ((end - start) / crate::runtime::scheduler::num_workers() as u64).max(64);
            let mut pos = start;
            while pos < end {
                let chunk_end = (pos + chunk_size).min(end);
                sched_parallel_for(pos, chunk_end, |_| {});
                for i in pos..chunk_end {
                    call_fn(env, &args[2], vec![Value::Number(i as f64)])?;
                }
                pos = chunk_end;
            }
        }
    }
    Ok(Value::Null)
}

fn builtin_sum(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.sum requires 1 argument (array)".to_string());
    }
    crate::runtime::simd::ops::array_sum(&args[0])
}

fn builtin_workers(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Number(
        crate::runtime::scheduler::num_workers() as f64
    ))
}

fn call_fn(env: &mut dyn BuiltinEnv, func: &Value, args: Vec<Value>) -> Result<Value, String> {
    match func {
        Value::Function(NativeFn(f)) => (f)(env, args),
        Value::UserFunction(u) => crate::execution::runtime::public_call_user(
            u.clone(),
            args,
            None,
            env.native_side_effects(),
        ),
        Value::BoundMethod(u, inst) => crate::execution::runtime::public_call_user_with_this(
            u.clone(),
            args,
            *inst.clone(),
            None,
            env.native_side_effects(),
        ),
        _ => Err("expected function".to_string()),
    }
}

fn extract_array(v: &Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Array(a) => Ok(a.clone()),
        Value::DynArray(d) => Ok(d.data.clone()),
        _ => Err("expected array".to_string()),
    }
}

fn as_u64(v: &Value, name: &str) -> Result<u64, String> {
    match v {
        Value::Number(n) => Ok(*n as u64),
        Value::F64(f) => Ok(*f as u64),
        Value::F32(f) => Ok(*f as u64),
        Value::I64(i) => Ok(*i as u64),
        Value::I32(i) => Ok(*i as u64),
        Value::I16(i) => Ok(*i as u64),
        Value::I8(i) => Ok(*i as u64),
        Value::U64(u) => Ok(*u),
        Value::U32(u) => Ok(*u as u64),
        Value::U16(u) => Ok(*u as u64),
        Value::U8(u) => Ok(*u as u64),
        _ => Err(format!(
            "Parallel.forEach: {} must be number (got {:?})",
            name, v
        )),
    }
}
