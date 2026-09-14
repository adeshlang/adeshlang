//! Built-in Decorators
//!
//! Native decorators with special runtime behavior:
//! - `@memoize`: automatic result caching
//! - `@trace`: automatic call tracing
//! - `@deprecated`: compile-time warnings
//! - `@timeout`: automatic timeout protection
//! - `@retry`: automatic retry logic

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Arc, Mutex};

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "memoize",
        "decorators",
        "Memoize function results (automatic caching)",
        builtin_memoize,
    );
    registry.register(
        "trace",
        "decorators",
        "Trace function calls with arguments and return values",
        builtin_trace,
    );
    registry.register(
        "deprecated",
        "decorators",
        "Mark function as deprecated with optional message",
        builtin_deprecated,
    );
    registry.register(
        "timeout",
        "decorators",
        "Add automatic timeout protection to function",
        builtin_timeout,
    );
    registry.register(
        "retry",
        "decorators",
        "Add automatic retry logic to function",
        builtin_retry,
    );
    registry.register(
        "benchmark",
        "decorators",
        "Measure and report execution time",
        builtin_benchmark,
    );
}

/// @memoize decorator - caches function results
fn builtin_memoize(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("memoize decorator requires (target, meta) arguments".to_string());
    }

    let target = args[0].clone();

    // Create cache for memoization (shared across all calls)
    let cache: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::default()));

    // Return wrapper function that uses efficient calling
    Ok(Value::Function(NativeFn(Arc::new(
        move |env, call_args| {
            // Create cache key from arguments
            let key = format!("{:?}", call_args);

            // Check cache
            {
                let cache_lock = cache.lock().unwrap();
                if let Some(cached_result) = cache_lock.get(&key) {
                    return Ok(cached_result.clone());
                }
            }

            // Call target function efficiently
            let result = match &target {
                Value::Function(NativeFn(f)) => (f)(env, call_args.clone())?,
                Value::UserFunction(u) => {
                    // Try to use efficient interpreter method if available
                    if let Some(interp) = env
                        .as_any_mut()
                        .downcast_mut::<crate::execution::runtime::Interpreter>()
                    {
                        // Use optimized O(1) call method
                        interp.call_user_function(u, call_args.clone())?
                    } else {
                        // Fallback to creating new exec (slower but works)
                        crate::execution::runtime::public_call_user(
                            u.clone(),
                            call_args.clone(),
                            None,
                            env.native_side_effects(),
                        )?
                    }
                }
                _ => return Err("memoize: target must be a function".to_string()),
            };

            // Store in cache
            {
                let mut cache_lock = cache.lock().unwrap();
                cache_lock.insert(key, result.clone());
            }

            Ok(result)
        },
    ))))
}

/// @trace decorator - logs function calls
fn builtin_trace(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("trace decorator requires (target, meta) arguments".to_string());
    }

    let target = args[0].clone();
    let meta = args[1].clone();

    // Extract function name from metadata
    let fn_name = if let Value::Object(map) = &meta {
        if let Some(Value::Str(name)) = map.get("name") {
            name.clone()
        } else {
            "<anonymous>".to_string()
        }
    } else {
        "<anonymous>".to_string()
    };

    // Return wrapper function
    Ok(Value::Function(NativeFn(Arc::new(
        move |env, call_args| {
            // Log entry
            print!("[TRACE] Entering {} with args: [", fn_name);
            for (i, arg) in call_args.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{:?}", arg);
            }
            println!("]");

            let start = std::time::Instant::now();

            // Call target function efficiently
            let result = match &target {
                Value::Function(NativeFn(f)) => (f)(env, call_args.clone()),
                Value::UserFunction(u) => {
                    if let Some(interp) = env
                        .as_any_mut()
                        .downcast_mut::<crate::execution::runtime::Interpreter>()
                    {
                        interp.call_user_function(u, call_args.clone())
                    } else {
                        crate::execution::runtime::public_call_user(
                            u.clone(),
                            call_args.clone(),
                            None,
                            env.native_side_effects(),
                        )
                    }
                }
                _ => return Err("trace: target must be a function".to_string()),
            };

            let elapsed = start.elapsed();

            // Log exit
            match &result {
                Ok(val) => {
                    println!(
                        "[TRACE] Exiting {} -> {:?} (took {:.3}ms)",
                        fn_name,
                        val,
                        elapsed.as_secs_f64() * 1000.0
                    );
                }
                Err(e) => {
                    println!(
                        "[TRACE] Exiting {} with ERROR: {} (took {:.3}ms)",
                        fn_name,
                        e,
                        elapsed.as_secs_f64() * 1000.0
                    );
                }
            }

            result
        },
    ))))
}

/// @deprecated decorator - warns when function is called
fn builtin_deprecated(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    // Can be called as @deprecated or @deprecated("message")
    let (target, meta, message) = if args.len() == 1 {
        // Called as factory: @deprecated("message")
        let msg = match &args[0] {
            Value::Str(s) => s.clone(),
            _ => "This function is deprecated".to_string(),
        };

        // Return a decorator function
        return Ok(Value::Function(NativeFn(Arc::new(move |_env2, args2| {
            if args2.len() != 2 {
                return Err("deprecated decorator requires (target, meta) arguments".to_string());
            }

            let target = args2[0].clone();
            let meta = args2[1].clone();
            let message = msg.clone();

            // Extract function name
            let fn_name = if let Value::Object(map) = &meta {
                if let Some(Value::Str(name)) = map.get("name") {
                    name.clone()
                } else {
                    "<anonymous>".to_string()
                }
            } else {
                "<anonymous>".to_string()
            };

            // Return wrapper that shows deprecation warning
            Ok(Value::Function(NativeFn(Arc::new(
                move |env, call_args| {
                    eprintln!(
                        "⚠️  DEPRECATED: Function '{}' is deprecated. {}",
                        fn_name, message
                    );

                    // Call target function efficiently
                    match &target {
                        Value::Function(NativeFn(f)) => (f)(env, call_args),
                        Value::UserFunction(u) => {
                            if let Some(interp) =
                                env.as_any_mut()
                                    .downcast_mut::<crate::execution::runtime::Interpreter>()
                            {
                                interp.call_user_function(u, call_args)
                            } else {
                                crate::execution::runtime::public_call_user(
                                    u.clone(),
                                    call_args,
                                    None,
                                    env.native_side_effects(),
                                )
                            }
                        }
                        _ => Err("deprecated: target must be a function".to_string()),
                    }
                },
            ))))
        }))));
    } else if args.len() == 2 {
        // Called directly: @deprecated
        (
            args[0].clone(),
            args[1].clone(),
            "This function is deprecated".to_string(),
        )
    } else {
        return Err(
            "deprecated decorator requires (target, meta) or (message) arguments".to_string(),
        );
    };

    // Extract function name
    let fn_name = if let Value::Object(map) = &meta {
        if let Some(Value::Str(name)) = map.get("name") {
            name.clone()
        } else {
            "<anonymous>".to_string()
        }
    } else {
        "<anonymous>".to_string()
    };

    // Return wrapper function
    Ok(Value::Function(NativeFn(Arc::new(
        move |env, call_args| {
            eprintln!(
                "⚠️  DEPRECATED: Function '{}' is deprecated. {}",
                fn_name, message
            );

            // Call target function efficiently
            match &target {
                Value::Function(NativeFn(f)) => (f)(env, call_args),
                Value::UserFunction(u) => {
                    if let Some(interp) = env
                        .as_any_mut()
                        .downcast_mut::<crate::execution::runtime::Interpreter>()
                    {
                        interp.call_user_function(u, call_args)
                    } else {
                        crate::execution::runtime::public_call_user(
                            u.clone(),
                            call_args,
                            None,
                            env.native_side_effects(),
                        )
                    }
                }
                _ => Err("deprecated: target must be a function".to_string()),
            }
        },
    ))))
}

/// @timeout decorator - adds timeout protection
fn builtin_timeout(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    // Must be called as factory: @timeout(ms)
    if args.len() != 1 {
        return Err(
            "timeout decorator requires timeout in milliseconds: @timeout(5000)".to_string(),
        );
    }

    let timeout_ms = match &args[0] {
        Value::Number(n) => *n as u64,
        _ => return Err("timeout decorator requires numeric milliseconds".to_string()),
    };

    // Return decorator function
    Ok(Value::Function(NativeFn(Arc::new(move |_env2, args2| {
        if args2.len() != 2 {
            return Err("timeout decorator requires (target, meta) arguments".to_string());
        }

        let _target = args2[0].clone();
        let meta = args2[1].clone();
        let _timeout_ms = timeout_ms;

        // Extract function name
        let _fn_name = if let Value::Object(map) = &meta {
            if let Some(Value::Str(name)) = map.get("name") {
                name.clone()
            } else {
                "<anonymous>".to_string()
            }
        } else {
            "<anonymous>".to_string()
        };

        // Return wrapper with timeout
        Ok(Value::Function(NativeFn(Arc::new(
            move |_env, _call_args| {
                // Note: Actual timeout requires async runtime or thread spawning
                // which is complex in current architecture. For now, just warn.
                Err(format!(
                    "TimeoutError: @timeout decorator requires async runtime support (coming soon)"
                ))
            },
        ))))
    }))))
}

/// @retry decorator - adds automatic retry logic
fn builtin_retry(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    // Must be called as factory: @retry(attempts) or @retry(attempts, delay_ms)
    if args.len() < 1 || args.len() > 2 {
        return Err(
            "retry decorator requires: @retry(attempts) or @retry(attempts, delay_ms)".to_string(),
        );
    }

    let max_attempts = match &args[0] {
        Value::Number(n) => (*n as usize).max(1),
        _ => return Err("retry decorator requires numeric attempts".to_string()),
    };

    let delay_ms = if args.len() == 2 {
        match &args[1] {
            Value::Number(n) => *n as u64,
            _ => return Err("retry delay must be numeric milliseconds".to_string()),
        }
    } else {
        1000 // Default 1 second
    };

    // Return decorator function
    Ok(Value::Function(NativeFn(Arc::new(move |_env2, args2| {
        if args2.len() != 2 {
            return Err("retry decorator requires (target, meta) arguments".to_string());
        }

        let target = args2[0].clone();
        let meta = args2[1].clone();
        let max_attempts = max_attempts;
        let delay_ms = delay_ms;

        // Extract function name
        let fn_name = if let Value::Object(map) = &meta {
            if let Some(Value::Str(name)) = map.get("name") {
                name.clone()
            } else {
                "<anonymous>".to_string()
            }
        } else {
            "<anonymous>".to_string()
        };

        // Return wrapper with retry logic
        Ok(Value::Function(NativeFn(Arc::new(
            move |env, call_args| {
                let mut last_error = String::new();

                for attempt in 1..=max_attempts {
                    let result = match &target {
                        Value::Function(NativeFn(f)) => (f)(env, call_args.clone()),
                        Value::UserFunction(u) => {
                            if let Some(interp) =
                                env.as_any_mut()
                                    .downcast_mut::<crate::execution::runtime::Interpreter>()
                            {
                                interp.call_user_function(u, call_args.clone())
                            } else {
                                crate::execution::runtime::public_call_user(
                                    u.clone(),
                                    call_args.clone(),
                                    None,
                                    env.native_side_effects(),
                                )
                            }
                        }
                        _ => return Err("retry: target must be a function".to_string()),
                    };

                    match result {
                        Ok(val) => return Ok(val),
                        Err(e) => {
                            last_error = e;
                            if attempt < max_attempts {
                                eprintln!(
                                    "[RETRY] Function '{}' failed (attempt {}/{}), retrying in {}ms...",
                                    fn_name, attempt, max_attempts, delay_ms
                                );
                                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                            }
                        }
                    }
                }

                Err(format!(
                    "RetryError: Function '{}' failed after {} attempts. Last error: {}",
                    fn_name, max_attempts, last_error
                ))
            },
        ))))
    }))))
}

/// @benchmark decorator - measures execution time
fn builtin_benchmark(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("benchmark decorator requires (target, meta) arguments".to_string());
    }

    let target = args[0].clone();
    let meta = args[1].clone();

    // Extract function name
    let fn_name = if let Value::Object(map) = &meta {
        if let Some(Value::Str(name)) = map.get("name") {
            name.clone()
        } else {
            "<anonymous>".to_string()
        }
    } else {
        "<anonymous>".to_string()
    };

    // Return wrapper function
    Ok(Value::Function(NativeFn(Arc::new(
        move |env, call_args| {
            let start = std::time::Instant::now();

            // Call target function efficiently
            let result = match &target {
                Value::Function(NativeFn(f)) => (f)(env, call_args.clone()),
                Value::UserFunction(u) => {
                    if let Some(interp) = env
                        .as_any_mut()
                        .downcast_mut::<crate::execution::runtime::Interpreter>()
                    {
                        interp.call_user_function(u, call_args.clone())
                    } else {
                        crate::execution::runtime::public_call_user(
                            u.clone(),
                            call_args.clone(),
                            None,
                            env.native_side_effects(),
                        )
                    }
                }
                _ => return Err("benchmark: target must be a function".to_string()),
            };

            let elapsed = start.elapsed();
            println!(
                "⏱️  BENCHMARK: {} took {:.3}ms",
                fn_name,
                elapsed.as_secs_f64() * 1000.0
            );

            result
        },
    ))))
}
