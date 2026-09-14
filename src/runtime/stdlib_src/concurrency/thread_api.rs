//! Thread lifecycle: spawn, join, detach, builder, scoped threads, TLS.

use super::helpers::*;
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::runtime::thread::{self as native, ThreadId};
use rustc_hash::FxHashMap as HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

struct ThreadResult {
    done: AtomicBool,
    value: Mutex<Option<Result<CrossThread, String>>>,
    cvar: Condvar,
}

struct JoinOnDrop {
    handle: Mutex<Option<JoinHandle<()>>>,
    detached: AtomicBool,
    result: Arc<ThreadResult>,
    id: ThreadId,
    name: Option<String>,
}

impl Drop for JoinOnDrop {
    fn drop(&mut self) {
        if self.detached.load(Ordering::Acquire) {
            native::unregister(self.id);
            return;
        }
        if !self.result.done.load(Ordering::Acquire) {
            if let Ok(guard) = self.result.value.lock() {
                let _ = self
                    .result
                    .cvar
                    .wait_timeout(guard, native::DROP_JOIN_TIMEOUT);
            }
        }
        if let Ok(mut h) = self.handle.lock() {
            if let Some(join) = h.take() {
                if self.result.done.load(Ordering::Acquire) {
                    let _ = join.join();
                } else {
                    // Timed out: detach so interpreter teardown cannot hang the process.
                    self.detached.store(true, Ordering::Release);
                    drop(join);
                }
            }
        }
        native::unregister(self.id);
    }
}

fn make_handle_object(inner: Arc<JoinOnDrop>) -> Value {
    let mut m = HashMap::default();
    let a = inner.clone();
    insert_fn(&mut m, "join", move |_env, _args| join_inner(&a, None));
    let a = inner.clone();
    insert_fn(&mut m, "join_timeout", move |_env, args| {
        let dur = parse_duration(args.first().ok_or("join_timeout(duration)")?)?;
        join_inner(&a, Some(dur))
    });
    let a = inner.clone();
    insert_fn(&mut m, "detach", move |_env, _args| {
        a.detached.store(true, Ordering::Release);
        if let Ok(mut h) = a.handle.lock() {
            let _ = h.take();
        }
        Ok(Value::Null)
    });
    let a = inner.clone();
    insert_fn(&mut m, "is_finished", move |_env, _args| {
        Ok(Value::Bool(a.result.done.load(Ordering::Acquire)))
    });
    let a = inner.clone();
    insert_fn(&mut m, "id", move |_env, _args| {
        Ok(Value::Number(a.id.as_u64() as f64))
    });
    let a = inner.clone();
    insert_fn(&mut m, "name", move |_env, _args| {
        Ok(match &a.name {
            Some(n) => Value::Str(n.clone()),
            None => Value::Null,
        })
    });
    let a = inner.clone();
    insert_fn(&mut m, "unpark", move |_env, _args| {
        native::unpark_id(a.id.as_u64()).map(|_| Value::Null)
    });
    with_kind(m, "ThreadHandle")
}

fn join_inner(inner: &Arc<JoinOnDrop>, timeout: Option<Duration>) -> Result<Value, String> {
    if inner.detached.load(Ordering::Acquire) {
        return Err("cannot join a detached thread".into());
    }
    if let Some(dur) = timeout {
        let guard = inner
            .result
            .value
            .lock()
            .map_err(|_| "thread result poisoned".to_string())?;
        if inner.result.done.load(Ordering::Acquire) {
            return take_result(inner, guard);
        }
        let (g, wait) = inner
            .result
            .cvar
            .wait_timeout(guard, dur)
            .map_err(|e| e.to_string())?;
        if wait.timed_out() && !inner.result.done.load(Ordering::Acquire) {
            return Ok(timeout_err());
        }
        return take_result(inner, g);
    }
    if let Ok(mut h) = inner.handle.lock() {
        if let Some(join) = h.take() {
            match join.join() {
                Ok(()) => {}
                Err(_) => {
                    return Ok(err_obj("panic", "thread panicked"));
                }
            }
        }
    }
    let guard = inner
        .result
        .value
        .lock()
        .map_err(|_| "thread result poisoned".to_string())?;
    take_result(inner, guard)
}

fn take_result(
    _inner: &Arc<JoinOnDrop>,
    mut guard: std::sync::MutexGuard<'_, Option<Result<CrossThread, String>>>,
) -> Result<Value, String> {
    match guard.take() {
        Some(Ok(CrossThread(v))) => Ok(v),
        Some(Err(e)) => Ok(err_obj("error", e)),
        None => {
            if !_inner.result.done.load(Ordering::Acquire) {
                Ok(err_obj("error", "thread has no result"))
            } else {
                Ok(Value::Null)
            }
        }
    }
}

pub struct SpawnOpts {
    pub name: Option<String>,
    pub stack_size: Option<usize>,
}

impl Default for SpawnOpts {
    fn default() -> Self {
        Self {
            name: None,
            stack_size: None,
        }
    }
}

pub fn spawn_with(func: Value, opts: SpawnOpts) -> Result<Value, String> {
    if !native::platform::threading_supported() {
        return Err(native::platform::unsupported_msg("spawn"));
    }
    let id = ThreadId::allocate();
    let result = Arc::new(ThreadResult {
        done: AtomicBool::new(false),
        value: Mutex::new(None),
        cvar: Condvar::new(),
    });
    let result_t = result.clone();
    let name = opts.name.clone();
    let name_t = name.clone();
    let job = SendJob::new(func);

    let mut builder = std::thread::Builder::new();
    if let Some(ref n) = name {
        builder = builder.name(n.clone());
    }
    if let Some(sz) = opts.stack_size {
        if sz < 16 * 1024 {
            return Err("stack_size must be at least 16 KiB".into());
        }
        builder = builder.stack_size(sz);
    }

    let handle = builder
        .spawn(move || {
            run_thread_job(job, result_t, id, name_t);
        })
        .map_err(|e| format!("thread creation failed: {e}"))?;

    let inner = Arc::new(JoinOnDrop {
        handle: Mutex::new(Some(handle)),
        detached: AtomicBool::new(false),
        result,
        id,
        name,
    });
    Ok(make_handle_object(inner))
}

fn run_thread_job(job: SendJob, result_t: Arc<ThreadResult>, id: ThreadId, name_t: Option<String>) {
    ThreadId::install(id);
    native::register_current(id, name_t.clone());
    let payload = job.func.into_inner();
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        call_fn_on_thread(payload, vec![])
    }));
    let stored = match out {
        Ok(Ok(v)) => Ok(CrossThread(v)),
        Ok(Err(e)) => {
            eprintln!(
                "[thread '{}' (id={:?}) error]: {}",
                name_t.as_deref().unwrap_or("unnamed"),
                id,
                e
            );
            Err(e)
        }
        Err(_) => {
            eprintln!(
                "[thread '{}' (id={:?}) panicked]",
                name_t.as_deref().unwrap_or("unnamed"),
                id
            );
            Err("thread panicked".into())
        }
    };
    if let Ok(mut g) = result_t.value.lock() {
        *g = Some(stored);
    }
    result_t.done.store(true, Ordering::Release);
    result_t.cvar.notify_all();
    native::unregister(id);
}

pub fn builtin_spawn(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let _ = env;
    let func = require_fn(&args, 0, "thread.spawn")?;
    spawn_with(func, SpawnOpts::default())
}

pub fn builtin_spawn_named(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let _ = env;
    if args.len() < 2 {
        return Err("thread.spawn_named(name, fn)".into());
    }
    let name = as_str(&args[0])
        .ok_or("spawn_named: name must be a string")?
        .to_string();
    spawn_with(
        args[1].clone(),
        SpawnOpts {
            name: Some(name),
            stack_size: None,
        },
    )
}

pub fn builtin_current(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut m = HashMap::default();
    m.insert(
        "id".into(),
        Value::Number(ThreadId::current().as_u64() as f64),
    );
    m.insert(
        "name".into(),
        match native::current_name() {
            Some(n) => Value::Str(n),
            None => Value::Null,
        },
    );
    Ok(Value::Object(Arc::new(m)))
}

pub fn builtin_id(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Number(ThreadId::current().as_u64() as f64))
}

pub fn builtin_name(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if let Some(v) = args.first() {
        if let Some(s) = as_str(v) {
            native::set_current_name(Some(s.to_string()));
            return Ok(Value::Null);
        }
        if matches!(v, Value::Null) {
            native::set_current_name(None);
            return Ok(Value::Null);
        }
    }
    Ok(match native::current_name() {
        Some(n) => Value::Str(n),
        None => Value::Null,
    })
}

pub fn builtin_sleep(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let dur = parse_duration(args.first().ok_or("thread.sleep(duration)")?)?;
    native::sleep(dur);
    Ok(Value::Null)
}

pub fn builtin_sleep_until(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let ms = num_of(args.first().ok_or("thread.sleep_until(epoch_ms)")?)
        .ok_or("sleep_until expects milliseconds timestamp")?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    if ms > now_ms {
        native::sleep(Duration::from_secs_f64((ms - now_ms) / 1000.0));
    }
    Ok(Value::Null)
}

pub fn builtin_yield(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    native::yield_now();
    Ok(Value::Null)
}

pub fn builtin_park(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if let Some(v) = args.first() {
        native::park_timeout(parse_duration(v)?);
    } else {
        native::park();
    }
    Ok(Value::Null)
}

pub fn builtin_unpark(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let id = num_of(args.first().ok_or("thread.unpark(id)")?).ok_or("unpark expects thread id")?;
    native::unpark_id(id as u64).map(|_| Value::Null)
}

pub fn builtin_hardware_concurrency(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    Ok(Value::Number(native::hardware_concurrency() as f64))
}

pub fn builtin_set_affinity(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let mask = num_of(args.first().ok_or("thread.set_affinity(mask)")?)
        .ok_or("affinity mask must be a number")?;
    native::platform::set_current_affinity(mask as u64).map(|_| Value::Null)
}

pub fn builtin_get_affinity(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    native::platform::get_current_affinity().map(|m| Value::Number(m as f64))
}

pub fn builtin_set_priority(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let p =
        num_of(args.first().ok_or("thread.set_priority(n)")?).ok_or("priority must be a number")?;
    native::platform::set_current_priority(p as i32).map(|_| Value::Null)
}

pub fn builtin_get_priority(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    native::platform::get_current_priority().map(|p| Value::Number(p as f64))
}

pub fn builtin_list(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let list = native::list_thread_ids();
    let vals: Vec<Value> = list
        .into_iter()
        .map(|(id, name)| {
            let mut m = HashMap::default();
            m.insert("id".into(), Value::Number(id as f64));
            m.insert("name".into(), name.map(Value::Str).unwrap_or(Value::Null));
            Value::Object(Arc::new(m))
        })
        .collect();
    Ok(Value::Array(vals))
}

pub fn builder_object() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, _args| Ok(make_builder(None, None)));
    with_kind(m, "ThreadBuilderType")
}

fn make_builder(name: Option<String>, stack: Option<usize>) -> Value {
    let name_s = name.clone();
    let stack_s = stack;
    let mut m = HashMap::default();
    insert_fn(&mut m, "name", move |_env, args| {
        let n = as_str(args.first().ok_or("builder.name(string)")?)
            .ok_or("name must be string")?
            .to_string();
        Ok(make_builder(Some(n), stack_s))
    });
    let name_s2 = name.clone();
    insert_fn(&mut m, "stack_size", move |_env, args| {
        let n = num_of(args.first().ok_or("builder.stack_size(bytes)")?)
            .ok_or("stack_size must be a number")? as usize;
        Ok(make_builder(name_s2.clone(), Some(n)))
    });
    insert_fn(&mut m, "spawn", move |_env, args| {
        let func = require_fn(&args, 0, "builder.spawn")?;
        spawn_with(
            func,
            SpawnOpts {
                name: name_s.clone(),
                stack_size: stack,
            },
        )
    });
    with_kind(m, "ThreadBuilder")
}

pub fn builtin_builder(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(make_builder(None, None))
}

/// Scoped threads: runtime joins all spawned children before returning.
/// Borrowing of parent locals is allowed only because the scope waits.
pub fn builtin_scope(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let body = require_fn(&args, 0, "thread.scope")?;
    let children: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let scope_obj = make_scope(children.clone());
    let result = call_fn(env, &body, vec![scope_obj])?;
    let handles = children
        .lock()
        .map_err(|_| "scope poisoned".to_string())?
        .drain(..)
        .collect::<Vec<_>>();
    for h in handles {
        if let Value::Object(map) = &h {
            if let Some(Value::Function(nf)) = map.get("join") {
                let _ = (nf.0)(env, vec![]);
            }
        }
    }
    Ok(result)
}

fn make_scope(children: Arc<Mutex<Vec<Value>>>) -> Value {
    let mut m = HashMap::default();
    let ch = children.clone();
    insert_fn(&mut m, "spawn", move |_env, args| {
        let func = require_fn(&args, 0, "scope.spawn")?;
        let handle = spawn_with(func, SpawnOpts::default())?;
        ch.lock()
            .map_err(|_| "scope poisoned".to_string())?
            .push(handle.clone());
        Ok(handle)
    });
    let ch = children.clone();
    insert_fn(&mut m, "spawn_named", move |_env, args| {
        if args.len() < 2 {
            return Err("scope.spawn_named(name, fn)".into());
        }
        let name = as_str(&args[0]).ok_or("name must be string")?.to_string();
        let handle = spawn_with(
            args[1].clone(),
            SpawnOpts {
                name: Some(name),
                stack_size: None,
            },
        )?;
        ch.lock()
            .map_err(|_| "scope poisoned".to_string())?
            .push(handle.clone());
        Ok(handle)
    });
    with_kind(m, "ThreadScope")
}

// ---- TLS ----

static NEXT_TLS: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static TLS_SLOTS: std::cell::RefCell<HashMap<u64, Value>> = std::cell::RefCell::new(HashMap::default());
}

pub fn tls_object() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        let init = args.first().cloned().unwrap_or(Value::Null);
        Ok(make_tls(NEXT_TLS.fetch_add(1, Ordering::Relaxed), init))
    });
    with_kind(m, "ThreadLocalType")
}

fn make_tls(id: u64, init: Value) -> Value {
    let init = CrossThread(init);
    let mut m = HashMap::default();
    let init_c = init.clone();
    insert_fn(&mut m, "get", move |_env, _args| {
        TLS_SLOTS.with(|slots| {
            let mut s = slots.borrow_mut();
            Ok(s.entry(id).or_insert_with(|| init_c.0.clone()).clone())
        })
    });
    insert_fn(&mut m, "set", move |_env, args| {
        let v = args.first().cloned().unwrap_or(Value::Null);
        TLS_SLOTS.with(|slots| {
            slots.borrow_mut().insert(id, v);
        });
        Ok(Value::Null)
    });
    let init_c = init.clone();
    insert_fn(&mut m, "with", move |env, args| {
        let func = require_fn(&args, 0, "ThreadLocal.with")?;
        let current = TLS_SLOTS.with(|slots| {
            let mut s = slots.borrow_mut();
            s.entry(id).or_insert_with(|| init_c.0.clone()).clone()
        });
        let out = call_fn(env, &func, vec![current.clone()])?;
        TLS_SLOTS.with(|slots| {
            slots.borrow_mut().insert(id, current);
        });
        Ok(out)
    });
    with_kind(m, "ThreadLocal")
}

// ---- Cancellation ----

pub fn cancellation_source_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, _args| {
        Ok(make_source(Arc::new(native::CancellationInner::new())))
    });
    with_kind(m, "CancellationSourceType")
}

fn make_source(inner: Arc<native::CancellationInner>) -> Value {
    let mut m = HashMap::default();
    let i = inner.clone();
    insert_fn(&mut m, "cancel", move |_env, _args| {
        i.cancel();
        Ok(Value::Null)
    });
    let i = inner.clone();
    insert_fn(&mut m, "is_cancelled", move |_env, _args| {
        Ok(Value::Bool(i.is_cancelled()))
    });
    let i = inner.clone();
    insert_fn(&mut m, "token", move |_env, _args| {
        Ok(make_token(i.clone()))
    });
    with_kind(m, "CancellationSource")
}

fn make_token(inner: Arc<native::CancellationInner>) -> Value {
    let mut m = HashMap::default();
    let i = inner.clone();
    insert_fn(&mut m, "is_cancelled", move |_env, _args| {
        Ok(Value::Bool(i.is_cancelled()))
    });
    with_kind(m, "CancellationToken")
}

pub fn builtin_watchdog(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let dur = parse_duration(args.first().ok_or("thread.watchdog(duration_ms)")?)?;
    native::install_process_watchdog(dur);
    let mut m = HashMap::default();
    insert_fn(&mut m, "disarm", |_env, _args| {
        native::disarm_watchdog();
        Ok(Value::Null)
    });
    Ok(with_kind(m, "Watchdog"))
}

pub fn builtin_disarm_watchdog(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    native::disarm_watchdog();
    Ok(Value::Null)
}
