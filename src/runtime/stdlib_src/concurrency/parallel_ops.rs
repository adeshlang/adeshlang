//! Parallel standard library — high-performance multi-core work-stealing parallel engine
//!
//! Provides true parallel operations across CPU cores:
//! - Parallel map, flatMap, filter, reduce, forEach, for, zipWith, chunk, batch
//! - Short-circuiting parallel predicates: find, findIndex, any, all
//! - Multi-core parallel sorting: sort, sortBy
//! - Multi-core SIMD-accelerated math: sum, product, min, max, mean, dot
//! - Concurrency primitives: join, scope, workers, setWorkers
//!
//! ```adesh
//! import Parallel;
//! let mapped = Parallel.map(data, fn(x) { return x * 2; });
//! let filtered = Parallel.filter(data, fn(x) { return x > 10; });
//! let total = Parallel.sum(data);
//! Parallel.forEach(0, 1000, fn(i) { work(i); });
//! ```

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::scheduler::{global_scheduler, num_workers, set_global_workers};
use crate::runtime::stdlib_src::concurrency::helpers::{
    call_fn, CrossThread,
};
use crate::stdlib::registry::BuiltinRegistry;
use rayon::prelude::*;
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Arc, Mutex};

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "Parallel",
        "concurrency",
        "High-performance work-stealing parallel execution engine",
        |_env: &mut dyn BuiltinEnv, _args| Ok(build_parallel_module_object()),
    );
}

/// Build the Parallel module object bound by `import Parallel;`
pub fn build_parallel_module_object() -> Value {
    let mut methods = HashMap::default();

    // Core transform operations
    methods.insert("map".into(), Value::Function(NativeFn(Arc::new(builtin_map))));
    methods.insert("flatMap".into(), Value::Function(NativeFn(Arc::new(builtin_flat_map))));
    methods.insert("flat_map".into(), Value::Function(NativeFn(Arc::new(builtin_flat_map))));
    methods.insert("filter".into(), Value::Function(NativeFn(Arc::new(builtin_filter))));
    methods.insert("reduce".into(), Value::Function(NativeFn(Arc::new(builtin_reduce))));
    methods.insert("zipWith".into(), Value::Function(NativeFn(Arc::new(builtin_zip_with))));
    methods.insert("chunk".into(), Value::Function(NativeFn(Arc::new(builtin_chunk))));
    methods.insert("batch".into(), Value::Function(NativeFn(Arc::new(builtin_batch))));

    // Iteration & Loops
    methods.insert("forEach".into(), Value::Function(NativeFn(Arc::new(builtin_for_each))));
    methods.insert("for_each".into(), Value::Function(NativeFn(Arc::new(builtin_for_each))));
    methods.insert("for".into(), Value::Function(NativeFn(Arc::new(builtin_for))));

    // Predicates & Search
    methods.insert("find".into(), Value::Function(NativeFn(Arc::new(builtin_find))));
    methods.insert("findIndex".into(), Value::Function(NativeFn(Arc::new(builtin_find_index))));
    methods.insert("any".into(), Value::Function(NativeFn(Arc::new(builtin_any))));
    methods.insert("all".into(), Value::Function(NativeFn(Arc::new(builtin_all))));

    // Sorting
    methods.insert("sort".into(), Value::Function(NativeFn(Arc::new(builtin_sort))));
    methods.insert("sortBy".into(), Value::Function(NativeFn(Arc::new(builtin_sort_by))));

    // High-speed SIMD & numeric operations
    methods.insert("sum".into(), Value::Function(NativeFn(Arc::new(builtin_sum))));
    methods.insert("product".into(), Value::Function(NativeFn(Arc::new(builtin_product))));
    methods.insert("min".into(), Value::Function(NativeFn(Arc::new(builtin_min))));
    methods.insert("max".into(), Value::Function(NativeFn(Arc::new(builtin_max))));
    methods.insert("mean".into(), Value::Function(NativeFn(Arc::new(builtin_mean))));
    methods.insert("dot".into(), Value::Function(NativeFn(Arc::new(builtin_dot))));

    // Concurrency control & fork-join
    methods.insert("join".into(), Value::Function(NativeFn(Arc::new(builtin_join))));
    methods.insert("scope".into(), Value::Function(NativeFn(Arc::new(builtin_scope))));
    methods.insert("workers".into(), Value::Function(NativeFn(Arc::new(builtin_workers))));
    methods.insert("setWorkers".into(), Value::Function(NativeFn(Arc::new(builtin_set_workers))));
    methods.insert("num_workers".into(), Value::Function(NativeFn(Arc::new(builtin_workers))));

    Value::Object(Arc::new(methods))
}

// ============================================================================
// Core Transform Operations
// ============================================================================

/// Parallel.map(array, fn) -> array
fn builtin_map(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.map requires (array, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    if arr.len() < 32 {
        let mut results = Vec::with_capacity(arr.len());
        for item in arr {
            results.push(call_fn(env, &args[1], vec![item])?);
        }
        return Ok(Value::Array(results));
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let sched = global_scheduler();

    let results: Result<Vec<CrossThread>, String> = sched.install(|| {
        cross_arr
            .into_par_iter()
            .map(|item| f.call(vec![item.get_value()]).map(CrossThread))
            .collect()
    });

    let values: Vec<Value> = results?.into_iter().map(|c| c.into_inner()).collect();
    Ok(Value::Array(values))
}

/// Parallel.flatMap(array, fn) -> array
fn builtin_flat_map(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.flatMap requires (array, function)".to_string());
    }
    let mapped = builtin_map(env, args)?;
    let mut flat = Vec::new();
    if let Value::Array(items) = mapped {
        for item in items {
            match item {
                Value::Array(nested) => flat.extend(nested),
                Value::DynArray(d) => flat.extend(d.data),
                other => flat.push(other),
            }
        }
    }
    Ok(Value::Array(flat))
}

/// Parallel.filter(array, fn) -> array
fn builtin_filter(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.filter requires (array, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    if arr.len() < 32 {
        let mut results = Vec::new();
        for item in arr {
            let keep = call_fn(env, &args[1], vec![item.clone()])?;
            if is_truthy(&keep) {
                results.push(item);
            }
        }
        return Ok(Value::Array(results));
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let sched = global_scheduler();

    let flags: Result<Vec<bool>, String> = sched.install(|| {
        cross_arr
            .par_iter()
            .map(|item| {
                let res = f.call(vec![item.get_value()])?;
                Ok(is_truthy(&res))
            })
            .collect()
    });
    let flags = flags?;
    let mut results = Vec::with_capacity(cross_arr.len());
    for (item, keep) in cross_arr.into_iter().zip(flags) {
        if keep {
            results.push(item.into_inner());
        }
    }
    Ok(Value::Array(results))
}

/// Parallel.reduce(array, init, fn) -> value
fn builtin_reduce(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Parallel.reduce requires (array, init, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(args[1].clone());
    }
    if arr.len() < 32 {
        let mut acc = args[1].clone();
        for item in arr {
            acc = call_fn(env, &args[2], vec![acc, item])?;
        }
        return Ok(acc);
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let workers = num_workers().max(1);
    let chunk_size = (cross_arr.len() / workers).max(16);
    let chunks: Vec<Vec<CrossThread>> = cross_arr.chunks(chunk_size).map(|c| c.to_vec()).collect();

    let f = CrossThread(args[2].clone());
    let init_val = CrossThread(args[1].clone());
    let sched = global_scheduler();

    let partials: Result<Vec<CrossThread>, String> = sched.install(|| {
        chunks
            .into_par_iter()
            .map(|chunk| {
                let mut acc = init_val.get_value();
                for item in chunk {
                    acc = f.call(vec![acc, item.into_inner()])?;
                }
                Ok(CrossThread(acc))
            })
            .collect()
    });
    let partials = partials?;

    let mut final_acc = args[1].clone();
    for p in partials {
        final_acc = call_fn(env, &args[2], vec![final_acc, p.into_inner()])?;
    }
    Ok(final_acc)
}

/// Parallel.zipWith(arrayA, arrayB, fn) -> array
fn builtin_zip_with(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Parallel.zipWith requires (arrayA, arrayB, function)".to_string());
    }
    let arr_a = extract_array(&args[0])?;
    let arr_b = extract_array(&args[1])?;
    let len = arr_a.len().min(arr_b.len());
    if len == 0 {
        return Ok(Value::Array(vec![]));
    }
    if len < 32 {
        let mut results = Vec::with_capacity(len);
        for i in 0..len {
            results.push(call_fn(
                env,
                &args[2],
                vec![arr_a[i].clone(), arr_b[i].clone()],
            )?);
        }
        return Ok(Value::Array(results));
    }

    let cross_a: Vec<CrossThread> = arr_a.into_iter().map(CrossThread).collect();
    let cross_b: Vec<CrossThread> = arr_b.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[2].clone());
    let sched = global_scheduler();

    let results: Result<Vec<CrossThread>, String> = sched.install(|| {
        (0..len)
            .into_par_iter()
            .map(|i| {
                f.call(vec![cross_a[i].get_value(), cross_b[i].get_value()])
                    .map(CrossThread)
            })
            .collect()
    });

    let values: Vec<Value> = results?.into_iter().map(|c| c.into_inner()).collect();
    Ok(Value::Array(values))
}

/// Parallel.chunk(array, size) -> array of arrays
fn builtin_chunk(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.chunk requires (array, size)".to_string());
    }
    let arr = extract_array(&args[0])?;
    let size = as_u64(&args[1], "size")?.max(1) as usize;
    let chunks: Vec<Value> = arr
        .chunks(size)
        .map(|c| Value::Array(c.to_vec()))
        .collect();
    Ok(Value::Array(chunks))
}

/// Parallel.batch(array, batchSize, fn) -> array of batch results
fn builtin_batch(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Parallel.batch requires (array, batchSize, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    let size = as_u64(&args[1], "batchSize")?.max(1) as usize;
    let chunks: Vec<Value> = arr
        .chunks(size)
        .map(|c| Value::Array(c.to_vec()))
        .collect();
    builtin_map(env, vec![Value::Array(chunks), args[2].clone()])
}

// ============================================================================
// Iteration & Loops
// ============================================================================

/// Parallel.forEach(start, end, fn) OR Parallel.forEach(array, fn)
fn builtin_for_each(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.forEach requires arguments".to_string());
    }
    if matches!(args[0], Value::Array(_) | Value::DynArray(_)) {
        let arr = extract_array(&args[0])?;
        if args.len() < 2 {
            return Err("Parallel.forEach(array, fn) requires function".to_string());
        }
        if arr.is_empty() {
            return Ok(Value::Null);
        }
        if arr.len() < 32 {
            for item in arr {
                call_fn(env, &args[1], vec![item])?;
            }
            return Ok(Value::Null);
        }

        let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
        let f = CrossThread(args[1].clone());
        let error_slot = Arc::new(Mutex::new(None));
        let sched = global_scheduler();

        sched.install(|| {
            cross_arr.into_par_iter().for_each(|item| {
                if error_slot.lock().unwrap().is_some() {
                    return;
                }
                if let Err(e) = f.call(vec![item.into_inner()]) {
                    let mut guard = error_slot.lock().unwrap();
                    if guard.is_none() {
                        *guard = Some(e);
                    }
                }
            });
        });

        if let Some(e) = error_slot.lock().unwrap().take() {
            return Err(e);
        }
        return Ok(Value::Null);
    }

    if args.len() < 2 {
        return Err("Parallel.forEach requires (start, end, fn)".to_string());
    }
    let start = as_u64(&args[0], "start")?;
    let end = as_u64(&args[1], "end")?;
    if start >= end {
        return Ok(Value::Null);
    }
    if args.len() < 3 {
        return Ok(Value::Null);
    }

    let count = end - start;
    if count < 32 {
        for i in start..end {
            call_fn(env, &args[2], vec![Value::Number(i as f64)])?;
        }
        return Ok(Value::Null);
    }

    let f = CrossThread(args[2].clone());
    let error_slot = Arc::new(Mutex::new(None));
    let sched = global_scheduler();

    sched.install(|| {
        (start..end).into_par_iter().for_each(|i| {
            if error_slot.lock().unwrap().is_some() {
                return;
            }
            if let Err(e) = f.call(vec![Value::Number(i as f64)]) {
                let mut guard = error_slot.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(e);
                }
            }
        });
    });

    if let Some(e) = error_slot.lock().unwrap().take() {
        return Err(e);
    }
    Ok(Value::Null)
}

/// Parallel.for(start, end, [step,] fn)
fn builtin_for(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Parallel.for requires (start, end, fn) or (start, end, step, fn)".to_string());
    }
    let start = as_i64(&args[0], "start")?;
    let end = as_i64(&args[1], "end")?;
    let (step, func) = if args.len() >= 4 {
        (as_i64(&args[2], "step")?, args[3].clone())
    } else {
        (1i64, args[2].clone())
    };

    if step == 0 {
        return Err("Parallel.for step cannot be 0".to_string());
    }

    let mut indices = Vec::new();
    if step > 0 {
        let mut curr = start;
        while curr < end {
            indices.push(curr);
            curr += step;
        }
    } else {
        let mut curr = start;
        while curr > end {
            indices.push(curr);
            curr += step;
        }
    }

    if indices.is_empty() {
        return Ok(Value::Null);
    }
    if indices.len() < 32 {
        for idx in indices {
            call_fn(env, &func, vec![Value::Number(idx as f64)])?;
        }
        return Ok(Value::Null);
    }

    let f = CrossThread(func);
    let error_slot = Arc::new(Mutex::new(None));
    let sched = global_scheduler();

    sched.install(|| {
        indices.into_par_iter().for_each(|idx| {
            if error_slot.lock().unwrap().is_some() {
                return;
            }
            if let Err(e) = f.call(vec![Value::Number(idx as f64)]) {
                let mut guard = error_slot.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(e);
                }
            }
        });
    });

    if let Some(e) = error_slot.lock().unwrap().take() {
        return Err(e);
    }
    Ok(Value::Null)
}

// ============================================================================
// Predicates & Search
// ============================================================================

/// Parallel.find(array, fn) -> value or null (short-circuiting parallel search)
fn builtin_find(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.find requires (array, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Null);
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let sched = global_scheduler();

    let found = sched.install(|| {
        cross_arr.into_par_iter().find_any(|item| {
            match f.call(vec![item.get_value()]) {
                Ok(v) => is_truthy(&v),
                Err(_) => false,
            }
        })
    });

    Ok(found.map(|c| c.into_inner()).unwrap_or(Value::Null))
}

/// Parallel.findIndex(array, fn) -> number (-1 if not found)
fn builtin_find_index(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.findIndex requires (array, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Number(-1.0));
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let sched = global_scheduler();

    let found = sched.install(|| {
        cross_arr.into_par_iter().enumerate().find_any(|(_idx, item)| {
            match f.call(vec![item.get_value()]) {
                Ok(v) => is_truthy(&v),
                Err(_) => false,
            }
        })
    });

    match found {
        Some((idx, _)) => Ok(Value::Number(idx as f64)),
        None => Ok(Value::Number(-1.0)),
    }
}

/// Parallel.any(array, fn) -> bool
fn builtin_any(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.any requires (array, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Bool(false));
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let sched = global_scheduler();

    let any_matched = sched.install(|| {
        cross_arr.into_par_iter().any(|item| {
            match f.call(vec![item.into_inner()]) {
                Ok(v) => is_truthy(&v),
                Err(_) => false,
            }
        })
    });

    Ok(Value::Bool(any_matched))
}

/// Parallel.all(array, fn) -> bool
fn builtin_all(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.all requires (array, function)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Bool(true));
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let sched = global_scheduler();

    let all_matched = sched.install(|| {
        cross_arr.into_par_iter().all(|item| {
            match f.call(vec![item.into_inner()]) {
                Ok(v) => is_truthy(&v),
                Err(_) => false,
            }
        })
    });

    Ok(Value::Bool(all_matched))
}

// ============================================================================
// Sorting
// ============================================================================

/// Parallel.sort(array) -> sorted array
fn builtin_sort(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.sort requires 1 argument (array)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.len() <= 1 {
        return Ok(Value::Array(arr));
    }

    let mut cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let sched = global_scheduler();
    sched.install(|| {
        cross_arr.par_sort_by(|a, b| compare_cross_values(a, b));
    });

    let sorted: Vec<Value> = cross_arr.into_iter().map(|c| c.into_inner()).collect();
    Ok(Value::Array(sorted))
}

/// Parallel.sortBy(array, keyFn) -> sorted array
fn builtin_sort_by(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.sortBy requires (array, keyFunction)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.len() <= 1 {
        return Ok(Value::Array(arr));
    }

    let keys_val = builtin_map(env, vec![Value::Array(arr.clone()), args[1].clone()])?;
    let keys = match keys_val {
        Value::Array(k) => k,
        _ => return Err("Parallel.sortBy: key generation failed".to_string()),
    };

    let mut paired: Vec<(CrossThread, CrossThread)> = arr
        .into_iter()
        .zip(keys)
        .map(|(a, k)| (CrossThread(a), CrossThread(k)))
        .collect();

    let sched = global_scheduler();
    sched.install(|| {
        paired.par_sort_by(|(_, k1), (_, k2)| compare_cross_values(k1, k2));
    });

    let sorted: Vec<Value> = paired.into_iter().map(|(item, _)| item.into_inner()).collect();
    Ok(Value::Array(sorted))
}

// ============================================================================
// High-Speed Multi-Core SIMD & Numeric Operations
// ============================================================================

/// Parallel.sum(array) -> number
fn builtin_sum(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.sum requires 1 argument (array)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Number(0.0));
    }

    if let Some(f64_data) = extract_f64_slice(&arr) {
        if f64_data.len() < 1024 {
            return crate::runtime::simd::ops::array_sum(&args[0]);
        }
        let sched = global_scheduler();
        let total: f64 = sched.install(|| {
            f64_data
                .par_chunks(2048)
                .map(|chunk| {
                    let mut acc = 0.0f64;
                    for &val in chunk {
                        acc += val;
                    }
                    acc
                })
                .sum()
        });
        return Ok(Value::Number(total));
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let sched = global_scheduler();
    let total: f64 = sched.install(|| {
        cross_arr.par_iter().map(|v| value_to_f64(&v.get_value())).sum()
    });
    Ok(Value::Number(total))
}

/// Parallel.product(array) -> number
fn builtin_product(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.product requires 1 argument (array)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Number(1.0));
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let sched = global_scheduler();
    let prod: f64 = sched.install(|| {
        cross_arr.par_iter().map(|v| value_to_f64(&v.get_value())).product()
    });
    Ok(Value::Number(prod))
}

/// Parallel.min(array) -> number
fn builtin_min(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.min requires 1 argument (array)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Err("Parallel.min of empty array".to_string());
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let sched = global_scheduler();
    let min_val = sched.install(|| {
        cross_arr
            .par_iter()
            .map(|v| value_to_f64(&v.get_value()))
            .reduce(|| f64::INFINITY, |a, b| a.min(b))
    });
    Ok(Value::Number(min_val))
}

/// Parallel.max(array) -> number
fn builtin_max(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.max requires 1 argument (array)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Err("Parallel.max of empty array".to_string());
    }

    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let sched = global_scheduler();
    let max_val = sched.install(|| {
        cross_arr
            .par_iter()
            .map(|v| value_to_f64(&v.get_value()))
            .reduce(|| f64::NEG_INFINITY, |a, b| a.max(b))
    });
    Ok(Value::Number(max_val))
}

/// Parallel.mean(array) -> number
fn builtin_mean(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.mean requires 1 argument (array)".to_string());
    }
    let arr = extract_array(&args[0])?;
    if arr.is_empty() {
        return Ok(Value::Number(f64::NAN));
    }
    let sum_val = builtin_sum(_env, args)?;
    if let Value::Number(s) = sum_val {
        Ok(Value::Number(s / arr.len() as f64))
    } else {
        Err("Parallel.mean: unexpected sum value".to_string())
    }
}

/// Parallel.dot(arrA, arrB) -> number
fn builtin_dot(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Parallel.dot requires (arrayA, arrayB)".to_string());
    }
    let arr_a = extract_array(&args[0])?;
    let arr_b = extract_array(&args[1])?;
    if arr_a.len() != arr_b.len() {
        return Err(format!(
            "Parallel.dot: length mismatch ({} vs {})",
            arr_a.len(),
            arr_b.len()
        ));
    }
    if arr_a.is_empty() {
        return Ok(Value::Number(0.0));
    }

    let cross_a: Vec<CrossThread> = arr_a.into_iter().map(CrossThread).collect();
    let cross_b: Vec<CrossThread> = arr_b.into_iter().map(CrossThread).collect();
    let len = cross_a.len();
    let sched = global_scheduler();

    let total: f64 = sched.install(|| {
        (0..len)
            .into_par_iter()
            .map(|i| value_to_f64(&cross_a[i].get_value()) * value_to_f64(&cross_b[i].get_value()))
            .sum()
    });

    Ok(Value::Number(total))
}

// ============================================================================
// Concurrency Control & Fork-Join
// ============================================================================

/// Parallel.join(fn1, fn2, ...) -> [res1, res2, ...]
fn builtin_join(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    if args.len() == 1 {
        let f = CrossThread(args[0].clone());
        let res = f.call(vec![])?;
        return Ok(Value::Array(vec![res]));
    }
    if args.len() == 2 {
        let f1 = CrossThread(args[0].clone());
        let f2 = CrossThread(args[1].clone());
        let sched = global_scheduler();
        let (r1, r2) = sched.parallel_join(
            move || f1.call(vec![]).map(CrossThread),
            move || f2.call(vec![]).map(CrossThread),
        );
        return Ok(Value::Array(vec![r1?.into_inner(), r2?.into_inner()]));
    }

    let tasks: Vec<CrossThread> = args.into_iter().map(CrossThread).collect();
    let sched = global_scheduler();
    let results: Result<Vec<CrossThread>, String> = sched.install(|| {
        tasks
            .into_par_iter()
            .map(|task| task.call(vec![]).map(CrossThread))
            .collect()
    });

    let values: Vec<Value> = results?.into_iter().map(|c| c.into_inner()).collect();
    Ok(Value::Array(values))
}

/// Parallel.scope(fn(scope))
fn builtin_scope(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.scope requires (fn(scope))".to_string());
    }
    let tasks: Arc<Mutex<Vec<CrossThread>>> = Arc::new(Mutex::new(Vec::new()));
    let mut scope_map = HashMap::default();

    let t = tasks.clone();
    scope_map.insert(
        "spawn".to_string(),
        Value::Function(NativeFn(Arc::new(move |_e, a| {
            if a.is_empty() {
                return Err("scope.spawn requires function".to_string());
            }
            t.lock().unwrap().push(CrossThread(a[0].clone()));
            Ok(Value::Null)
        }))),
    );

    let scope_obj = Value::Object(Arc::new(scope_map));
    let ret = call_fn(env, &args[0], vec![scope_obj])?;

    let queued = tasks.lock().unwrap().drain(..).collect::<Vec<_>>();
    if !queued.is_empty() {
        let sched = global_scheduler();
        let results: Result<Vec<CrossThread>, String> = sched.install(|| {
            queued
                .into_par_iter()
                .map(|t| t.call(vec![]).map(CrossThread))
                .collect()
        });
        results?;
    }

    Ok(ret)
}

/// Parallel.workers() -> number
fn builtin_workers(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Number(num_workers() as f64))
}

/// Parallel.setWorkers(n)
fn builtin_set_workers(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Parallel.setWorkers requires 1 argument (number)".to_string());
    }
    let n = as_u64(&args[0], "workers")?.max(1) as usize;
    set_global_workers(n);
    Ok(Value::Number(n as f64))
}

// ============================================================================
// Helpers
// ============================================================================

fn extract_array(v: &Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Array(a) => Ok(a.clone()),
        Value::DynArray(d) => Ok(d.data.clone()),
        Value::RawArray(_, a) => Ok(a.clone()),
        _ => Err(format!(
            "expected array, got {:?}",
            crate::runtime::stdlib_src::concurrency::helpers::type_name(v)
        )),
    }
}

fn extract_f64_slice(arr: &[Value]) -> Option<Vec<f64>> {
    let mut data = Vec::with_capacity(arr.len());
    for v in arr {
        match v {
            Value::Number(n) | Value::F64(n) => data.push(*n),
            Value::F32(n) => data.push(*n as f64),
            Value::I64(n) => data.push(*n as f64),
            Value::I32(n) => data.push(*n as f64),
            Value::U64(n) => data.push(*n as f64),
            Value::U32(n) => data.push(*n as f64),
            Value::I16(n) => data.push(*n as f64),
            Value::U16(n) => data.push(*n as f64),
            Value::I8(n) => data.push(*n as f64),
            Value::U8(n) => data.push(*n as f64),
            _ => return None,
        }
    }
    Some(data)
}

fn value_to_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) | Value::F64(n) => *n,
        Value::F32(n) => *n as f64,
        Value::I64(n) => *n as f64,
        Value::I32(n) => *n as f64,
        Value::U64(n) => *n as f64,
        Value::U32(n) => *n as f64,
        Value::I16(n) => *n as f64,
        Value::U16(n) => *n as f64,
        Value::I8(n) => *n as f64,
        Value::U8(n) => *n as f64,
        _ => 0.0,
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        Value::Str(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::DynArray(d) => !d.data.is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}

fn compare_cross_values(a: &CrossThread, b: &CrossThread) -> std::cmp::Ordering {
    let va = a.get_value();
    let vb = b.get_value();
    if let (Some(na), Some(nb)) = (extract_num_opt(&va), extract_num_opt(&vb)) {
        return na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal);
    }
    match (&va, &vb) {
        (Value::Str(s1), Value::Str(s2)) => s1.cmp(s2),
        (Value::Bool(b1), Value::Bool(b2)) => b1.cmp(b2),
        _ => std::cmp::Ordering::Equal,
    }
}

fn extract_num_opt(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) | Value::F64(n) => Some(*n),
        Value::F32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::I8(n) => Some(*n as f64),
        Value::U8(n) => Some(*n as f64),
        _ => None,
    }
}

fn as_u64(v: &Value, name: &str) -> Result<u64, String> {
    match v {
        Value::Number(n) => Ok(*n as u64),
        Value::F64(f) => Ok(*f as u64),
        Value::F32(f) => Ok(*f as u64),
        Value::I64(i) => Ok(*i as u64),
        Value::I32(i) => Ok(*i as u64),
        Value::U64(u) => Ok(*u),
        Value::U32(u) => Ok(*u as u64),
        _ => Err(format!(
            "Parallel: {} must be a positive number (got {:?})",
            name, v
        )),
    }
}

fn as_i64(v: &Value, name: &str) -> Result<i64, String> {
    match v {
        Value::Number(n) => Ok(*n as i64),
        Value::F64(f) => Ok(*f as i64),
        Value::F32(f) => Ok(*f as i64),
        Value::I64(i) => Ok(*i),
        Value::I32(i) => Ok(*i as i64),
        Value::U64(u) => Ok(*u as i64),
        Value::U32(u) => Ok(*u as i64),
        _ => Err(format!(
            "Parallel: {} must be an integer number (got {:?})",
            name, v
        )),
    }
}
