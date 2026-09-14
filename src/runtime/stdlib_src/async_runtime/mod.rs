//! Async Runtime
//!
//! Promise construction and timer-based async primitives:
//! - `Promise(executor)`: create promise with resolve/reject callbacks
//! - `sleep`, `delay`: convenience promise creators
//! - `setTimeout/clearTimeout`, `setInterval/clearInterval`
//!
//! Integrates with interpreter microtask queue via native side effects.
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "Promise",
        "async",
        "Construct a Promise from executor",
        builtin_promise,
    );
    registry.register("sleep", "async", "Sleep for ms then resolve", builtin_sleep);
    registry.register("delay", "async", "Resolve after ms", builtin_delay);
    registry.register(
        "setTimeout",
        "async",
        "Schedule callback after ms",
        builtin_set_timeout,
    );
    registry.register(
        "clearTimeout",
        "async",
        "Cancel timeout by id",
        builtin_clear_timeout,
    );
    registry.register(
        "setInterval",
        "async",
        "Schedule repeating callback",
        builtin_set_interval,
    );
    registry.register(
        "clearInterval",
        "async",
        "Cancel interval by id",
        builtin_clear_interval,
    );
}

fn builtin_promise(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Promise(executor)".to_string());
    }
    env.create_promise_executor(args[0].clone())
}

fn builtin_sleep(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("sleep(ms)".to_string());
    }
    let ms = match args[0].as_f64() {
        Some(n) => n.max(0.0) as u64,
        None => return Err("sleep ms must be number".to_string()),
    };
    env.create_promise_executor(Value::Function(NativeFn(Arc::new(
        move |benv: &mut dyn BuiltinEnv, a: Vec<Value>| {
            let resolve = a.get(0).cloned().unwrap_or(Value::Null);
            let id = benv.alloc_timer_id()?;
            let cb = Value::Function(NativeFn(Arc::new(
                move |env2: &mut dyn BuiltinEnv, _ax: Vec<Value>| {
                    if let Value::Function(NativeFn(f)) = resolve.clone() {
                        let _ = (f)(env2, vec![Value::Null]);
                    }
                    let _ = env2.cancel_timer(id);
                    Ok(Value::Null)
                },
            )));
            let flag = Arc::new(AtomicBool::new(true));
            benv.register_timer(id, cb, flag.clone(), false, ms)?;
            Ok(Value::Null)
        },
    ))))
}

fn builtin_delay(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("delay(ms)".to_string());
    }
    let ms = match args[0].as_f64() {
        Some(n) => n.max(0.0) as u64,
        None => return Err("delay(ms) expects number".to_string()),
    };
    env.create_promise_executor(Value::Function(NativeFn(Arc::new(
        move |benv: &mut dyn BuiltinEnv, a: Vec<Value>| {
            let resolve = a.get(0).cloned().unwrap_or(Value::Null);
            let id = benv.alloc_timer_id()?;
            let cb = Value::Function(NativeFn(Arc::new(
                move |env2: &mut dyn BuiltinEnv, _ax: Vec<Value>| {
                    if let Value::Function(NativeFn(f)) = resolve.clone() {
                        let _ = (f)(env2, vec![Value::Null]);
                    }
                    let _ = env2.cancel_timer(id);
                    Ok(Value::Null)
                },
            )));
            let flag = Arc::new(AtomicBool::new(true));
            benv.register_timer(id, cb, flag.clone(), false, ms)?;
            Ok(Value::Null)
        },
    ))))
}

fn builtin_set_timeout(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("setTimeout(fn, ms, ...args)".to_string());
    }
    let cb = args[0].clone();
    let ms = match args[1].as_f64() {
        Some(n) => n.max(0.0) as u64,
        None => return Err("setTimeout delay must be number".to_string()),
    };
    let rest = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        Vec::new()
    };
    let id = env.alloc_timer_id()?;
    let cb_exec = Value::Function(NativeFn(Arc::new(
        move |env2: &mut dyn BuiltinEnv, _ax: Vec<Value>| {
            match cb.clone() {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(env2, rest.clone());
                }
                Value::UserFunction(u) => {
                    if let Some(ns) = env2.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        let rest_args = rest.clone();
                        let tid = id;
                        q.push(crate::parsing::ast::NativeEffect::EnqueueMicrotask(
                            Box::new(move |ienv: &mut dyn crate::parsing::ast::InterpreterEnv| {
                                let interp = ienv
                                    .as_any_mut()
                                    .downcast_mut::<crate::execution::runtime::Interpreter>()
                                    .unwrap();
                                let _ = interp.run_user_fn_in_interp(u.clone(), rest_args.clone());
                                let _ = interp.cancel_timer(tid);
                            }),
                        ));
                    }
                }
                _ => {}
            }
            let _ = env2.cancel_timer(id);
            Ok(Value::Null)
        },
    )));
    let flag = Arc::new(AtomicBool::new(true));
    env.register_timer(id, cb_exec, flag.clone(), false, ms)?;
    Ok(Value::Number(id as f64))
}

fn builtin_clear_timeout(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("clearTimeout(id)".to_string());
    }
    let id = match args[0].as_f64() {
        Some(n) => n as u64,
        None => return Err("clearTimeout expects numeric id".to_string()),
    };
    env.cancel_timer(id)?;
    Ok(Value::Null)
}

fn builtin_set_interval(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("setInterval(fn, ms, ...args)".to_string());
    }
    let cb = args[0].clone();
    let ms = match args[1].as_f64() {
        Some(n) => n.max(1.0) as u64,
        None => return Err("setInterval delay must be number".to_string()),
    };
    let rest = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        Vec::new()
    };
    let id = env.alloc_timer_id()?;
    let cb_exec = Value::Function(NativeFn(Arc::new(
        move |env2: &mut dyn BuiltinEnv, _ax: Vec<Value>| {
            match cb.clone() {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(env2, rest.clone());
                }
                Value::UserFunction(u) => {
                    if let Some(ns) = env2.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        let rest_args = rest.clone();
                        q.push(crate::parsing::ast::NativeEffect::EnqueueMicrotask(
                            Box::new(move |ienv: &mut dyn crate::parsing::ast::InterpreterEnv| {
                                let interp = ienv
                                    .as_any_mut()
                                    .downcast_mut::<crate::execution::runtime::Interpreter>()
                                    .unwrap();
                                let _ = interp.run_user_fn_in_interp(u.clone(), rest_args.clone());
                            }),
                        ));
                    }
                }
                _ => {}
            }
            Ok(Value::Null)
        },
    )));
    let flag = Arc::new(AtomicBool::new(true));
    env.register_timer(id, cb_exec, flag.clone(), true, ms)?;
    Ok(Value::Number(id as f64))
}

fn builtin_clear_interval(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("clearInterval(id)".to_string());
    }
    let id = match args[0].as_f64() {
        Some(n) => n as u64,
        None => return Err("clearInterval expects numeric id".to_string()),
    };
    env.cancel_timer(id)?;
    Ok(Value::Null)
}
