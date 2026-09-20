use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::parsing::error::{ErrorKind, LangError};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;

/// Namespaced `std` module exposing the bit-manipulation intrinsics.
///
/// Use `std.bit_count(x)`, `std.rotate_left(x, n)`, `std.bit_extract(x, o, w)`,
/// and so on. The bare names are intentionally *not* installed as global
/// functions so they cannot shadow or collide with user-defined functions;
/// the namespace is injected at interpreter startup and needs no import.
pub fn build_std_module_object() -> Value {
    let mut methods: HashMap<String, Value> = HashMap::default();
    for &(name, intrinsic) in crate::runtime::abi::bitwise::BitIntrinsic::ALL {
        methods.insert(
            name.to_string(),
            Value::Function(NativeFn(std::sync::Arc::new(move |_env, args| {
                intrinsic.eval(&args)
            }))),
        );
    }
    Value::Object(std::sync::Arc::new(methods))
}

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register("set", "core", "Construct set from array", builtin_set);
    registry.register(
        "makeSet",
        "core",
        "Construct set from array (alias)",
        builtin_make_set,
    );
    registry.register("tuple", "core", "Construct tuple from array", builtin_tuple);
    registry.register(
        "complex",
        "core",
        "Construct complex number",
        builtin_complex,
    );
    registry.register(
        "Error",
        "core",
        "Create error instance",
        builtin_error_instance,
    );
    registry.register(
        "error",
        "core",
        "Create error instance (alias)",
        builtin_error_instance,
    );
    registry.register(
        "alloc",
        "core",
        "Classification of value allocation kind",
        builtin_alloc,
    );
}

fn builtin_set(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("set(array)".to_string());
    }
    match &args[0] {
        Value::Array(arr) => {
            let mut out: Vec<Value> = Vec::new();
            for v in arr {
                let mut exists = false;
                for x in out.iter() {
                    if crate::execution::runtime::public_equals(x, v) {
                        exists = true;
                        break;
                    }
                }
                if !exists {
                    out.push(v.clone());
                }
            }
            Ok(Value::Set(out))
        }
        Value::DynArray(da) => {
            let mut out: Vec<Value> = Vec::new();
            for v in &da.data {
                let mut exists = false;
                for x in out.iter() {
                    if crate::execution::runtime::public_equals(x, v) {
                        exists = true;
                        break;
                    }
                }
                if !exists {
                    out.push(v.clone());
                }
            }
            Ok(Value::Set(out))
        }
        Value::Tuple(t) => {
            let mut out: Vec<Value> = Vec::new();
            for v in t {
                let mut exists = false;
                for x in out.iter() {
                    if crate::execution::runtime::public_equals(x, v) {
                        exists = true;
                        break;
                    }
                }
                if !exists {
                    out.push(v.clone());
                }
            }
            Ok(Value::Set(out))
        }
        _ => Err("set requires array, tuple, or dynamic array".to_string()),
    }
}

fn builtin_make_set(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    builtin_set(env, args)
}

fn builtin_tuple(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("tuple(array)".to_string());
    }
    match &args[0] {
        Value::Array(arr) => Ok(Value::Tuple(arr.clone())),
        Value::DynArray(da) => Ok(Value::Tuple(da.data.clone())),
        Value::Tuple(tup) => Ok(Value::Tuple(tup.clone())),
        Value::Set(s) => Ok(Value::Tuple(s.clone())),
        _ => Err("tuple requires array, tuple or set".to_string()),
    }
}

fn builtin_complex(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("complex(real, imag)".to_string());
    }
    let r = match &args[0] {
        Value::Number(n) => *n,
        _ => return Err("real must be number".to_string()),
    };
    let i = match &args[1] {
        Value::Number(n) => *n,
        _ => return Err("imag must be number".to_string()),
    };
    Ok(Value::Complex(r, i))
}

fn builtin_error_instance(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let msg = if args.is_empty() {
        "Error".to_string()
    } else {
        let parts: Vec<String> = args
            .iter()
            .map(|v| crate::execution::runtime::format::fmt(v))
            .collect();
        parts.join(" ")
    };
    let le = LangError::new(ErrorKind::User, msg, 0, 0, String::new());
    Ok(Value::Error(le))
}

fn builtin_alloc(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("alloc(value)".to_string());
    }
    let s = match &args[0] {
        Value::Number(_) | Value::Bool(_) | Value::Char(_) => "inline",
        _ => "heap",
    };
    Ok(Value::Str(s.to_string()))
}
