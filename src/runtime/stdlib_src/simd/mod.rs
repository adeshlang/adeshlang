//! SIMD standard library — import Simd; then Simd.sum(arr) or Simd.vector(arr).sum()
//!
//! ```adesh
//! import Simd;
//! let total = Simd.sum([1, 2, 3, 4]);
//! let v = Simd.vector([1, 2, 3, 4]);
//! print(v.sum());
//! print(v.dot([4, 3, 2, 1]));
//! ```

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::simd::ops::{
    array_abs, array_add, array_div, array_dot, array_fused_mul_add, array_fused_sqrt_mul_add,
    array_max, array_mean, array_min, array_mul, array_sqrt, array_sub, array_sum,
};
use crate::runtime::simd::value::{make_simd, SimdValue};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Arc, Mutex};

pub fn register_all(registry: &mut BuiltinRegistry) {
    register(registry);
}

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "Simd",
        "simd",
        "SIMD vector math namespace — import Simd; then Simd.sum(arr) or Simd.vector(arr).sum()",
        |_env: &mut dyn BuiltinEnv, _args| Ok(build_simd_module_object()),
    );
}

/// Build the Simd module object bound by `import Simd;`
pub fn build_simd_module_object() -> Value {
    let mut methods = HashMap::default();

    methods.insert(
        "sum".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_sum))),
    );
    methods.insert(
        "mean".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_mean))),
    );
    methods.insert(
        "min".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_min))),
    );
    methods.insert(
        "max".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_max))),
    );
    methods.insert(
        "abs".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_abs))),
    );
    methods.insert(
        "sqrt".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_sqrt))),
    );
    methods.insert(
        "dot".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dot))),
    );
    methods.insert(
        "add".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_add))),
    );
    methods.insert(
        "sub".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_sub))),
    );
    methods.insert(
        "mul".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_mul))),
    );
    methods.insert(
        "div".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_div))),
    );
    methods.insert(
        "fusedMulAdd".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_fused_mul_add))),
    );
    methods.insert(
        "fusedSqrtMulAdd".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_fused_sqrt_mul_add))),
    );
    methods.insert(
        "concat".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_concat))),
    );
    methods.insert(
        "vector".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_vector))),
    );
    methods.insert(
        "workers".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_workers))),
    );

    Value::Object(Arc::new(methods))
}

// --- Namespace-level builtins ---

fn builtin_sum(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 1, "Simd.sum")?;
    array_sum(&args[0])
}

fn builtin_mean(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 1, "Simd.mean")?;
    array_mean(&args[0])
}

fn builtin_min(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 1, "Simd.min")?;
    array_min(&args[0])
}

fn builtin_max(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 1, "Simd.max")?;
    array_max(&args[0])
}

fn builtin_abs(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 1, "Simd.abs")?;
    array_abs(&args[0])
}

fn builtin_sqrt(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 1, "Simd.sqrt")?;
    array_sqrt(&args[0])
}

fn builtin_dot(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 2, "Simd.dot")?;
    array_dot(&args[0], &args[1])
}

fn builtin_add(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 2, "Simd.add")?;
    array_add(&args[0], &args[1])
}

fn builtin_sub(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 2, "Simd.sub")?;
    array_sub(&args[0], &args[1])
}

fn builtin_mul(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 2, "Simd.mul")?;
    array_mul(&args[0], &args[1])
}

fn builtin_div(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 2, "Simd.div")?;
    array_div(&args[0], &args[1])
}

fn builtin_fused_mul_add(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 3, "Simd.fusedMulAdd")?;
    array_fused_mul_add(&args[0], &args[1], &args[2])
}

fn builtin_fused_sqrt_mul_add(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    require_len(&args, 3, "Simd.fusedSqrtMulAdd")?;
    array_fused_sqrt_mul_add(&args[0], &args[1], &args[2])
}

fn builtin_concat(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 2, "Simd.concat")?;
    let a = extract_array(&args[0])?;
    let b = extract_array(&args[1])?;
    let mut result = a;
    result.extend(b);
    Ok(Value::Array(result))
}

fn builtin_workers(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Number(
        crate::runtime::scheduler::num_workers() as f64,
    ))
}

/// Simd.vector(arr) — returns a vector handle with instance methods
fn builtin_vector(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    require_len(&args, 1, "Simd.vector")?;
    let data = extract_array(&args[0])?;
    Ok(wrap_vector(data))
}

// --- Vector instance object (object.method() pattern) ---

fn wrap_vector(data: Vec<Value>) -> Value {
    let state = Arc::new(Mutex::new(data));
    let mut methods = HashMap::default();

    let st = state.clone();
    methods.insert(
        "sum".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            array_sum(&Value::Array(st.lock().unwrap().clone()))
        }))),
    );

    let st = state.clone();
    methods.insert(
        "mean".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            array_mean(&Value::Array(st.lock().unwrap().clone()))
        }))),
    );

    let st = state.clone();
    methods.insert(
        "min".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            array_min(&Value::Array(st.lock().unwrap().clone()))
        }))),
    );

    let st = state.clone();
    methods.insert(
        "max".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            array_max(&Value::Array(st.lock().unwrap().clone()))
        }))),
    );

    let st = state.clone();
    methods.insert(
        "abs".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            array_abs(&Value::Array(st.lock().unwrap().clone()))
        }))),
    );

    let st = state.clone();
    methods.insert(
        "sqrt".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            array_sqrt(&Value::Array(st.lock().unwrap().clone()))
        }))),
    );

    let st = state.clone();
    methods.insert(
        "len".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            Ok(Value::Number(st.lock().unwrap().len() as f64))
        }))),
    );

    let st = state.clone();
    methods.insert(
        "dot".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            require_len(&args, 1, "vector.dot")?;
            array_dot(&Value::Array(st.lock().unwrap().clone()), &args[0])
        }))),
    );

    let st = state.clone();
    methods.insert(
        "add".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            require_len(&args, 1, "vector.add")?;
            array_add(&Value::Array(st.lock().unwrap().clone()), &args[0])
        }))),
    );

    let st = state.clone();
    methods.insert(
        "mul".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            require_len(&args, 1, "vector.mul")?;
            array_mul(&Value::Array(st.lock().unwrap().clone()), &args[0])
        }))),
    );

    let st = state.clone();
    methods.insert(
        "scale".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            require_len(&args, 1, "vector.scale")?;
            array_mul(
                &Value::Array(st.lock().unwrap().clone()),
                &args[0],
            )
        }))),
    );

    let st = state.clone();
    methods.insert(
        "toArray".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            Ok(Value::Array(st.lock().unwrap().clone()))
        }))),
    );

    Value::Object(Arc::new(methods))
}

// --- Helpers ---

fn require_len(args: &[Value], n: usize, name: &str) -> Result<(), String> {
    if args.len() < n {
        return Err(format!("{} requires {} argument(s)", name, n));
    }
    Ok(())
}

fn extract_array(v: &Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Array(a) => Ok(a.clone()),
        Value::DynArray(d) => Ok(d.data.clone()),
        Value::RawArray(_, a) => Ok(a.clone()),
        _ => Err("expected array".to_string()),
    }
}

/// Convert SimdValue to runtime Value
pub fn simd_value_to_runtime(sv: &SimdValue) -> Value {
    Value::Array(sv.lanes.iter().map(|&x| Value::Number(x)).collect())
}

/// Create SimdValue from runtime array
pub fn runtime_to_simd_value(arr: &Value, elem_type: &str, lanes: u32) -> Result<SimdValue, String> {
    let data: Vec<f64> = match arr {
        Value::Array(v) => v.iter().map(value_to_f64).collect(),
        _ => return Err("expected array for SIMD load".to_string()),
    };
    make_simd(elem_type, lanes, data)
}

fn value_to_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        Value::F64(n) => *n,
        Value::F32(n) => *n as f64,
        _ => 0.0,
    }
}
