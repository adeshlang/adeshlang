//! Rng Stateful Instance Wrapper
//!
//! Creates stateful Rng objects containing bound native methods that share an encapsulated PRNG generator state.

use super::bounded;
use super::collections;
use super::prng::Xoshiro256PlusPlus;
use super::strings;
use crate::parsing::ast::{NativeFn, Value};
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Arc, Mutex};

/// Create a new stateful Rng object from a Xoshiro256PlusPlus generator instance.
pub fn make_rng_object(prng: Xoshiro256PlusPlus) -> Value {
    let state = Arc::new(Mutex::new(prng));
    let mut map: HashMap<String, Value> = HashMap::default();

    // int(min, max) or int()
    let st = Arc::clone(&state);
    map.insert(
        "int".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            let mut g = st.lock().unwrap();
            if args.is_empty() {
                Ok(Value::I64(g.next_u64() as i64))
            } else if args.len() == 2 {
                let min = as_i64(&args[0])?;
                let max = as_i64(&args[1])?;
                let val = bounded::next_i64_range(&mut g, min, max)?;
                Ok(Value::I64(val))
            } else {
                Err("rng.int requires 0 or 2 arguments: (min, max)".to_string())
            }
        }))),
    );

    // intInclusive(min, max)
    let st = Arc::clone(&state);
    map.insert(
        "intInclusive".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 2 {
                return Err("rng.intInclusive requires 2 arguments: (min, max)".to_string());
            }
            let min = as_i64(&args[0])?;
            let max = as_i64(&args[1])?;
            let mut g = st.lock().unwrap();
            let val = bounded::next_i64_range_inclusive(&mut g, min, max)?;
            Ok(Value::I64(val))
        }))),
    );

    // float() or floatRange(min, max)
    let st = Arc::clone(&state);
    map.insert(
        "float".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            let mut g = st.lock().unwrap();
            if args.is_empty() {
                Ok(Value::Number(bounded::next_f64(&mut g)))
            } else if args.len() == 2 {
                let min = as_f64(&args[0])?;
                let max = as_f64(&args[1])?;
                let val = bounded::next_f64_range(&mut g, min, max)?;
                Ok(Value::Number(val))
            } else {
                Err("rng.float requires 0 or 2 arguments: (min, max)".to_string())
            }
        }))),
    );

    // bool() or bernoulli(p)
    let st = Arc::clone(&state);
    map.insert(
        "bool".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            let mut g = st.lock().unwrap();
            if args.is_empty() {
                Ok(Value::Bool(bounded::next_bool(&mut g)))
            } else if args.len() == 1 {
                let p = as_f64(&args[0])?;
                let val = bounded::next_bernoulli(&mut g, p)?;
                Ok(Value::Bool(val))
            } else {
                Err("rng.bool requires 0 or 1 argument".to_string())
            }
        }))),
    );

    // byte()
    let st = Arc::clone(&state);
    map.insert(
        "byte".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if !args.is_empty() {
                return Err("rng.byte expects 0 arguments".to_string());
            }
            let mut g = st.lock().unwrap();
            let b = bounded::next_u64_bounded(&mut g, 256) as u8;
            Ok(Value::U8(b))
        }))),
    );

    // bytes(count)
    let st = Arc::clone(&state);
    map.insert(
        "bytes".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 1 {
                return Err("rng.bytes requires 1 argument (count)".to_string());
            }
            let count = as_usize(&args[0])?;
            let mut g = st.lock().unwrap();
            let mut buf = vec![0u8; count];
            g.fill_bytes(&mut buf);
            let vals: Vec<Value> = buf.into_iter().map(Value::U8).collect();
            Ok(Value::Array(vals))
        }))),
    );

    // string(length)
    let st = Arc::clone(&state);
    map.insert(
        "string".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 1 {
                return Err("rng.string requires 1 argument (length)".to_string());
            }
            let len = as_usize(&args[0])?;
            let mut g = st.lock().unwrap();
            let s = strings::random_alphanumeric(&mut g, len);
            Ok(Value::Str(s))
        }))),
    );

    // choice(collection)
    let st = Arc::clone(&state);
    map.insert(
        "choice".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 1 {
                return Err("rng.choice requires 1 collection argument".to_string());
            }
            let items = as_array_slice(&args[0])?;
            let mut g = st.lock().unwrap();
            let val = collections::choice(&mut g, &items)?;
            Ok(val.clone())
        }))),
    );

    // shuffle(collection)
    let st = Arc::clone(&state);
    map.insert(
        "shuffle".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 1 {
                return Err("rng.shuffle requires 1 collection argument".to_string());
            }
            let mut items = as_array_slice(&args[0])?;
            let mut g = st.lock().unwrap();
            collections::shuffle(&mut g, &mut items);
            Ok(Value::Array(items))
        }))),
    );

    // sample(collection, count)
    let st = Arc::clone(&state);
    map.insert(
        "sample".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 2 {
                return Err("rng.sample requires 2 arguments: (collection, count)".to_string());
            }
            let items = as_array_slice(&args[0])?;
            let count = as_usize(&args[1])?;
            let mut g = st.lock().unwrap();
            let result = collections::sample(&mut g, &items, count)?;
            Ok(Value::Array(result))
        }))),
    );

    // clone()
    let st = Arc::clone(&state);
    map.insert(
        "clone".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let g = st.lock().unwrap();
            let cloned_prng = g.clone();
            Ok(make_rng_object(cloned_prng))
        }))),
    );

    // fork()
    let st = Arc::clone(&state);
    map.insert(
        "fork".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let mut g = st.lock().unwrap();
            let forked_prng = g.fork();
            Ok(make_rng_object(forked_prng))
        }))),
    );

    // state()
    let st = Arc::clone(&state);
    map.insert(
        "state".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let g = st.lock().unwrap();
            let s = g.get_state();
            let state_vals: Vec<Value> = s.iter().map(|&v| Value::U64(v)).collect();
            Ok(Value::Array(state_vals))
        }))),
    );

    // restore(stateArr)
    let st = Arc::clone(&state);
    map.insert(
        "restore".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 1 {
                return Err("rng.restore requires 1 state array argument".to_string());
            }
            let arr = as_array_slice(&args[0])?;
            if arr.len() != 4 {
                return Err(
                    "rng.restore state array must contain exactly 4 u64 elements".to_string(),
                );
            }
            let mut s = [0u64; 4];
            for i in 0..4 {
                s[i] = as_u64(&arr[i])?;
            }
            let mut g = st.lock().unwrap();
            g.set_state(s);
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}

fn as_i64(v: &Value) -> Result<i64, String> {
    match v {
        Value::I64(n) => Ok(*n),
        Value::I32(n) => Ok(*n as i64),
        Value::I16(n) => Ok(*n as i64),
        Value::I8(n) => Ok(*n as i64),
        Value::U64(n) => Ok(*n as i64),
        Value::U32(n) => Ok(*n as i64),
        Value::U16(n) => Ok(*n as i64),
        Value::U8(n) => Ok(*n as i64),
        Value::Number(n) => Ok(*n as i64),
        _ => Err(format!("Expected integer, got {:?}", v)),
    }
}

fn as_u64(v: &Value) -> Result<u64, String> {
    match v {
        Value::U64(n) => Ok(*n),
        Value::U32(n) => Ok(*n as u64),
        Value::U16(n) => Ok(*n as u64),
        Value::U8(n) => Ok(*n as u64),
        Value::I64(n) => Ok(*n as u64),
        Value::I32(n) => Ok(*n as u64),
        Value::Number(n) => Ok(*n as u64),
        _ => Err(format!("Expected u64 integer, got {:?}", v)),
    }
}

fn as_usize(v: &Value) -> Result<usize, String> {
    let n = as_i64(v)?;
    if n < 0 {
        Err(format!("Count/length cannot be negative, got {}", n))
    } else {
        Ok(n as usize)
    }
}

fn as_f64(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => Ok(*n),
        Value::F64(n) => Ok(*n),
        Value::F32(n) => Ok(*n as f64),
        Value::I64(n) => Ok(*n as f64),
        Value::I32(n) => Ok(*n as f64),
        Value::U64(n) => Ok(*n as f64),
        Value::U32(n) => Ok(*n as f64),
        _ => Err(format!("Expected float number, got {:?}", v)),
    }
}

fn as_array_slice(v: &Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Array(arr) => Ok(arr.clone()),
        Value::RawArray(_, arr) => Ok(arr.clone()),
        Value::Tuple(arr) => Ok(arr.clone()),
        Value::Set(arr) => Ok(arr.clone()),
        Value::DynArray(da) => Ok(da.data.clone()),
        _ => Err(format!("Expected array collection, got {:?}", v)),
    }
}
