//! Synchronization primitives with RAII-style `with` plus explicit lock/unlock.
//!
//! Classification:
//! - Mutex / RwLock / Condvar / Semaphore / Barrier / Latch / WaitGroup: BLOCKING
//! - Once / OnceCell: BLOCKING during init, then wait-free-ish loads
//! - Event: BLOCKING wait, lock-free notify fast path (atomic + condvar)

use super::helpers::*;
use crate::parsing::ast::Value;
use rustc_hash::FxHashMap as HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::Duration;

struct MutexState {
    data: Mutex<Value>,
    held: Mutex<bool>,
    cvar: Condvar,
    poisoned: AtomicBool,
}

pub fn mutex_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        let v = args.first().cloned().unwrap_or(Value::Null);
        Ok(make_mutex(v))
    });
    with_kind(m, "MutexType")
}

fn make_mutex(initial: Value) -> Value {
    let state = Arc::new(MutexState {
        data: Mutex::new(initial),
        held: Mutex::new(false),
        cvar: Condvar::new(),
        poisoned: AtomicBool::new(false),
    });
    let mut m = HashMap::default();
    let s = state.clone();
    insert_fn(&mut m, "lock", move |_env, _args| lock_mutex(&s, None));
    let s = state.clone();
    insert_fn(&mut m, "try_lock", move |_env, _args| {
        if s.poisoned.load(Ordering::Acquire) {
            return Ok(err_obj("poisoned", "mutex poisoned"));
        }
        let mut held = s.held.lock().map_err(|_| "mutex poisoned".to_string())?;
        if *held {
            return Ok(err_obj("would_block", "mutex busy"));
        }
        *held = true;
        Ok(ok_obj(make_guard(s.clone())))
    });
    let s = state.clone();
    insert_fn(&mut m, "lock_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("lock_timeout(duration)")?)?;
        lock_mutex(&s, Some(d))
    });
    let s = state.clone();
    insert_fn(&mut m, "with", move |env, args| {
        let func = require_fn(&args, 0, "Mutex.with")?;
        {
            let mut held = s.held.lock().map_err(|_| "mutex poisoned".to_string())?;
            while *held {
                held = s
                    .cvar
                    .wait_timeout(held, Duration::from_millis(50))
                    .map_err(|e| e.to_string())?
                    .0;
            }
            *held = true;
        }
        let result = (|| {
            let mut g = s.data.lock().map_err(|_| "mutex poisoned".to_string())?;
            let out = call_fn(env, &func, vec![g.clone()])?;
            *g = out.clone();
            Ok::<_, String>(out)
        })();
        if let Ok(mut held) = s.held.lock() {
            *held = false;
        }
        s.cvar.notify_all();
        result
    });
    with_kind(m, "Mutex")
}

fn lock_mutex(state: &Arc<MutexState>, timeout: Option<Duration>) -> Result<Value, String> {
    if state.poisoned.load(Ordering::Acquire) {
        return Ok(err_obj("poisoned", "mutex poisoned"));
    }
    let mut held = state.held.lock().map_err(|_| {
        state.poisoned.store(true, Ordering::Release);
        "mutex poisoned".to_string()
    })?;
    let start = std::time::Instant::now();
    while *held {
        if let Some(d) = timeout {
            let remain = d.saturating_sub(start.elapsed());
            if remain.is_zero() {
                return Ok(timeout_err());
            }
            let (h, w) = state
                .cvar
                .wait_timeout(held, remain)
                .map_err(|e| e.to_string())?;
            held = h;
            if w.timed_out() && *held {
                return Ok(timeout_err());
            }
        } else {
            held = state
                .cvar
                .wait_timeout(held, Duration::from_millis(50))
                .map_err(|e| e.to_string())?
                .0;
        }
    }
    *held = true;
    Ok(ok_obj(make_guard(state.clone())))
}

struct UnlockOnDrop {
    state: Arc<MutexState>,
    live: Arc<AtomicBool>,
}

impl Drop for UnlockOnDrop {
    fn drop(&mut self) {
        if self.live.swap(false, Ordering::AcqRel) {
            if let Ok(mut h) = self.state.held.lock() {
                *h = false;
            }
            self.state.cvar.notify_all();
        }
    }
}

fn make_guard(state: Arc<MutexState>) -> Value {
    let live = Arc::new(AtomicBool::new(true));
    let dropper = Arc::new(UnlockOnDrop {
        state: state.clone(),
        live: live.clone(),
    });
    let mut m = HashMap::default();
    let s = state.clone();
    let l = live.clone();
    let _d = dropper.clone();
    insert_fn(&mut m, "get", move |_env, _args| {
        let _keep = &_d;
        if !l.load(Ordering::Acquire) {
            return Err("mutex guard already unlocked".into());
        }
        let g = s.data.lock().map_err(|_| "mutex poisoned".to_string())?;
        Ok(g.clone())
    });
    let s = state.clone();
    let l = live.clone();
    let _d = dropper.clone();
    insert_fn(&mut m, "set", move |_env, args| {
        let _keep = &_d;
        if !l.load(Ordering::Acquire) {
            return Err("mutex guard already unlocked".into());
        }
        let v = args.first().cloned().unwrap_or(Value::Null);
        *s.data.lock().map_err(|_| "mutex poisoned".to_string())? = v;
        Ok(Value::Null)
    });
    let s = state.clone();
    let l = live.clone();
    let _d = dropper.clone();
    insert_fn(&mut m, "unlock", move |_env, _args| {
        let _keep = &_d;
        if l.swap(false, Ordering::AcqRel) {
            *s.held.lock().map_err(|_| "mutex poisoned".to_string())? = false;
            s.cvar.notify_all();
        }
        Ok(Value::Null)
    });
    with_kind(m, "MutexGuard")
}

// ---- RwLock ----

struct RwState {
    data: RwLock<Value>,
}

pub fn rwlock_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        Ok(make_rwlock(
            args.first().cloned().unwrap_or(Value::Null),
        ))
    });
    with_kind(m, "RwLockType")
}

fn make_rwlock(initial: Value) -> Value {
    let state = Arc::new(RwState {
        data: RwLock::new(initial),
    });
    let mut m = HashMap::default();
    let s = state.clone();
    insert_fn(&mut m, "read", move |_env, _args| {
        let g = s.data.read().map_err(|_| "rwlock poisoned".to_string())?;
        Ok(g.clone())
    });
    let s = state.clone();
    insert_fn(&mut m, "write", move |_env, args| {
        let mut g = s.data.write().map_err(|_| "rwlock poisoned".to_string())?;
        if let Some(v) = args.first() {
            *g = v.clone();
            Ok(Value::Null)
        } else {
            Ok(g.clone())
        }
    });
    let s = state.clone();
    insert_fn(&mut m, "try_read", move |_env, _args| match s.data.try_read() {
        Ok(g) => Ok(ok_obj(g.clone())),
        Err(_) => Ok(err_obj("would_block", "rwlock busy")),
    });
    let s = state.clone();
    insert_fn(&mut m, "try_write", move |_env, args| match s.data.try_write() {
        Ok(mut g) => {
            if let Some(v) = args.first() {
                *g = v.clone();
            }
            Ok(ok_obj(g.clone()))
        }
        Err(_) => Ok(err_obj("would_block", "rwlock busy")),
    });
    let s = state.clone();
    insert_fn(&mut m, "with_read", move |env, args| {
        let func = require_fn(&args, 0, "RwLock.with_read")?;
        let g = s.data.read().map_err(|_| "rwlock poisoned".to_string())?;
        call_fn(env, &func, vec![g.clone()])
    });
    let s = state.clone();
    insert_fn(&mut m, "with_write", move |env, args| {
        let func = require_fn(&args, 0, "RwLock.with_write")?;
        let mut g = s.data.write().map_err(|_| "rwlock poisoned".to_string())?;
        let out = call_fn(env, &func, vec![g.clone()])?;
        *g = out.clone();
        Ok(out)
    });
    with_kind(m, "RwLock")
}

// ---- Condvar ----

pub fn condvar_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, _args| Ok(make_condvar()));
    with_kind(m, "CondvarType")
}

struct CondPair {
    ready: Mutex<bool>,
    cvar: Condvar,
}

fn make_condvar() -> Value {
    let inner = Arc::new(CondPair {
        ready: Mutex::new(false),
        cvar: Condvar::new(),
    });
    let mut m = HashMap::default();
    let i = inner.clone();
    insert_fn(&mut m, "wait", move |_env, _args| {
        let mut g = i.ready.lock().map_err(|e| e.to_string())?;
        while !*g {
            g = i
                .cvar
                .wait_timeout(g, Duration::from_millis(50))
                .map_err(|e| e.to_string())?
                .0;
        }
        Ok(Value::Null)
    });
    let i = inner.clone();
    insert_fn(&mut m, "wait_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("wait_timeout(duration)")?)?;
        let g = i.ready.lock().map_err(|e| e.to_string())?;
        if *g {
            return Ok(Value::Bool(true));
        }
        let (g, w) = i.cvar.wait_timeout(g, d).map_err(|e| e.to_string())?;
        Ok(Value::Bool(*g && !w.timed_out()))
    });
    let i = inner.clone();
    insert_fn(&mut m, "notify_one", move |_env, _args| {
        *i.ready.lock().map_err(|e| e.to_string())? = true;
        i.cvar.notify_one();
        Ok(Value::Null)
    });
    let i = inner.clone();
    insert_fn(&mut m, "notify_all", move |_env, _args| {
        *i.ready.lock().map_err(|e| e.to_string())? = true;
        i.cvar.notify_all();
        Ok(Value::Null)
    });
    let i = inner.clone();
    insert_fn(&mut m, "reset", move |_env, _args| {
        *i.ready.lock().map_err(|e| e.to_string())? = false;
        Ok(Value::Null)
    });
    with_kind(m, "Condvar")
}

/// Mutex + Condvar wait used as `condition.wait(mutex)` in the spec.
/// We expose `thread.cond_wait(mutex, condvar)` via pairing objects:
/// `mutex.wait(cond)` is implemented as Event-style wait on a dedicated pair.
pub fn event_type() -> Value {
    condvar_type()
}

// ---- Semaphore ----

struct SemState {
    permits: Mutex<usize>,
    cvar: Condvar,
}

pub fn semaphore_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        let n = args
            .first()
            .and_then(num_of)
            .ok_or("Semaphore.new(permits)")? as usize;
        Ok(make_sem(n))
    });
    with_kind(m, "SemaphoreType")
}

fn make_sem(permits: usize) -> Value {
    let st = Arc::new(SemState {
        permits: Mutex::new(permits),
        cvar: Condvar::new(),
    });
    let mut m = HashMap::default();
    let s = st.clone();
    insert_fn(&mut m, "acquire", move |_env, args| {
        let n = args.first().and_then(num_of).unwrap_or(1.0) as usize;
        let mut p = s.permits.lock().map_err(|e| e.to_string())?;
        while *p < n {
            p = s
                .cvar
                .wait_timeout(p, Duration::from_millis(50))
                .map_err(|e| e.to_string())?
                .0;
        }
        *p -= n;
        Ok(Value::Null)
    });
    let s = st.clone();
    insert_fn(&mut m, "try_acquire", move |_env, args| {
        let n = args.first().and_then(num_of).unwrap_or(1.0) as usize;
        let mut p = s.permits.lock().map_err(|e| e.to_string())?;
        if *p >= n {
            *p -= n;
            Ok(Value::Bool(true))
        } else {
            Ok(Value::Bool(false))
        }
    });
    let s = st.clone();
    insert_fn(&mut m, "acquire_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("acquire_timeout(duration)")?)?;
        let n = args.get(1).and_then(num_of).unwrap_or(1.0) as usize;
        let p = s.permits.lock().map_err(|e| e.to_string())?;
        if *p >= n {
            let mut p = p;
            *p -= n;
            return Ok(Value::Bool(true));
        }
        let (mut p, w) = s.cvar.wait_timeout(p, d).map_err(|e| e.to_string())?;
        if w.timed_out() || *p < n {
            Ok(Value::Bool(false))
        } else {
            *p -= n;
            Ok(Value::Bool(true))
        }
    });
    let s = st.clone();
    insert_fn(&mut m, "release", move |_env, args| {
        let n = args.first().and_then(num_of).unwrap_or(1.0) as usize;
        let mut p = s.permits.lock().map_err(|e| e.to_string())?;
        *p = p.saturating_add(n);
        s.cvar.notify_all();
        Ok(Value::Null)
    });
    let s = st.clone();
    insert_fn(&mut m, "available_permits", move |_env, _args| {
        let p = s.permits.lock().map_err(|e| e.to_string())?;
        Ok(Value::Number(*p as f64))
    });
    with_kind(m, "Semaphore")
}

// ---- Barrier ----

struct BarrierState {
    n: usize,
    arrived: Mutex<(usize, u64)>,
    cvar: Condvar,
}

pub fn barrier_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        let n = args.first().and_then(num_of).ok_or("Barrier.new(count)")? as usize;
        if n == 0 {
            return Err("Barrier count must be > 0".into());
        }
        Ok(make_barrier(n))
    });
    with_kind(m, "BarrierType")
}

fn make_barrier(n: usize) -> Value {
    let st = Arc::new(BarrierState {
        n,
        arrived: Mutex::new((0, 0)),
        cvar: Condvar::new(),
    });
    let mut m = HashMap::default();
    let s = st.clone();
    insert_fn(&mut m, "wait", move |_env, _args| {
        let mut g = s.arrived.lock().map_err(|e| e.to_string())?;
        let generation = g.1;
        g.0 += 1;
        if g.0 >= s.n {
            g.0 = 0;
            g.1 = generation.wrapping_add(1);
            s.cvar.notify_all();
            Ok(Value::Bool(true))
        } else {
            while g.1 == generation {
                g = s
                    .cvar
                    .wait_timeout(g, Duration::from_millis(50))
                    .map_err(|e| e.to_string())?
                    .0;
            }
            Ok(Value::Bool(false))
        }
    });
    let s = st.clone();
    insert_fn(&mut m, "wait_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("Barrier.wait_timeout(duration)")?)?;
        let mut g = s.arrived.lock().map_err(|e| e.to_string())?;
        let generation = g.1;
        g.0 += 1;
        if g.0 >= s.n {
            g.0 = 0;
            g.1 = generation.wrapping_add(1);
            s.cvar.notify_all();
            return Ok(Value::Bool(true));
        }
        let start = std::time::Instant::now();
        while g.1 == generation {
            let remain = d.saturating_sub(start.elapsed());
            if remain.is_zero() {
                g.0 = g.0.saturating_sub(1);
                return Ok(Value::Bool(false));
            }
            let (ng, w) = s
                .cvar
                .wait_timeout(g, remain.min(Duration::from_millis(50)))
                .map_err(|e| e.to_string())?;
            g = ng;
            if w.timed_out() && g.1 == generation && start.elapsed() >= d {
                g.0 = g.0.saturating_sub(1);
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(false))
    });
    with_kind(m, "Barrier")
}

// ---- Latch ----

struct LatchState {
    remaining: Mutex<usize>,
    cvar: Condvar,
}

pub fn latch_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        let n = args
            .first()
            .and_then(num_of)
            .ok_or("CountDownLatch.new(count)")? as usize;
        Ok(make_latch(n))
    });
    with_kind(m, "CountDownLatchType")
}

fn make_latch(n: usize) -> Value {
    let st = Arc::new(LatchState {
        remaining: Mutex::new(n),
        cvar: Condvar::new(),
    });
    let mut m = HashMap::default();
    let s = st.clone();
    insert_fn(&mut m, "count_down", move |_env, args| {
        let k = args.first().and_then(num_of).unwrap_or(1.0) as usize;
        let mut r = s.remaining.lock().map_err(|e| e.to_string())?;
        *r = r.saturating_sub(k);
        if *r == 0 {
            s.cvar.notify_all();
        }
        Ok(Value::Number(*r as f64))
    });
    let s = st.clone();
    insert_fn(&mut m, "wait", move |_env, _args| {
        let mut r = s.remaining.lock().map_err(|e| e.to_string())?;
        while *r > 0 {
            r = s
                .cvar
                .wait_timeout(r, Duration::from_millis(50))
                .map_err(|e| e.to_string())?
                .0;
        }
        Ok(Value::Null)
    });
    let s = st.clone();
    insert_fn(&mut m, "try_wait", move |_env, _args| {
        let r = s.remaining.lock().map_err(|e| e.to_string())?;
        Ok(Value::Bool(*r == 0))
    });
    let s = st.clone();
    insert_fn(&mut m, "wait_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("wait_timeout(duration)")?)?;
        let r = s.remaining.lock().map_err(|e| e.to_string())?;
        if *r == 0 {
            return Ok(Value::Bool(true));
        }
        let (r, w) = s.cvar.wait_timeout(r, d).map_err(|e| e.to_string())?;
        Ok(Value::Bool(*r == 0 && !w.timed_out()))
    });
    with_kind(m, "CountDownLatch")
}

// ---- Once / OnceCell / Lazy ----

struct OnceState {
    done: AtomicBool,
    lock: Mutex<()>,
    cell: Mutex<Option<Value>>,
}

pub fn once_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, _args| Ok(make_once()));
    with_kind(m, "OnceType")
}

fn make_once() -> Value {
    let st = Arc::new(OnceState {
        done: AtomicBool::new(false),
        lock: Mutex::new(()),
        cell: Mutex::new(None),
    });
    let mut m = HashMap::default();
    let s = st.clone();
    insert_fn(&mut m, "call_once", move |env, args| {
        let func = require_fn(&args, 0, "Once.call_once")?;
        if s.done.load(Ordering::Acquire) {
            return Ok(s.cell.lock().map_err(|e| e.to_string())?.clone().unwrap_or(Value::Null));
        }
        let _g = s.lock.lock().map_err(|e| e.to_string())?;
        if s.done.load(Ordering::Acquire) {
            return Ok(s.cell.lock().map_err(|e| e.to_string())?.clone().unwrap_or(Value::Null));
        }
        let v = call_fn(env, &func, vec![])?;
        *s.cell.lock().map_err(|e| e.to_string())? = Some(v.clone());
        s.done.store(true, Ordering::Release);
        Ok(v)
    });
    let s = st.clone();
    insert_fn(&mut m, "is_completed", move |_env, _args| {
        Ok(Value::Bool(s.done.load(Ordering::Acquire)))
    });
    with_kind(m, "Once")
}

pub fn once_cell_type() -> Value {
    once_type()
}

pub fn lazy_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        let func = require_fn(&args, 0, "Lazy.new")?;
        let st = Arc::new(OnceState {
            done: AtomicBool::new(false),
            lock: Mutex::new(()),
            cell: Mutex::new(None),
        });
        let mut obj = HashMap::default();
        let s = st.clone();
        insert_fn(&mut obj, "get", move |env, _args| {
            if s.done.load(Ordering::Acquire) {
                return Ok(s.cell.lock().map_err(|e| e.to_string())?.clone().unwrap_or(Value::Null));
            }
            let _g = s.lock.lock().map_err(|e| e.to_string())?;
            if s.done.load(Ordering::Acquire) {
                return Ok(s.cell.lock().map_err(|e| e.to_string())?.clone().unwrap_or(Value::Null));
            }
            let v = call_fn(env, &func, vec![])?;
            *s.cell.lock().map_err(|e| e.to_string())? = Some(v.clone());
            s.done.store(true, Ordering::Release);
            Ok(v)
        });
        Ok(with_kind(obj, "Lazy"))
    });
    with_kind(m, "LazyType")
}

// ---- WaitGroup ----

struct WgState {
    count: AtomicUsize,
    lock: Mutex<()>,
    cvar: Condvar,
}

pub fn waitgroup_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, _args| Ok(make_wg()));
    with_kind(m, "WaitGroupType")
}

fn make_wg() -> Value {
    let st = Arc::new(WgState {
        count: AtomicUsize::new(0),
        lock: Mutex::new(()),
        cvar: Condvar::new(),
    });
    let mut m = HashMap::default();
    let s = st.clone();
    insert_fn(&mut m, "add", move |_env, args| {
        let n = args.first().and_then(num_of).unwrap_or(1.0);
        if n < 0.0 {
            return Err("WaitGroup.add: count must be non-negative".into());
        }
        s.count.fetch_add(n as usize, Ordering::SeqCst);
        Ok(Value::Null)
    });
    let s = st.clone();
    insert_fn(&mut m, "done", move |_env, _args| {
        let prev = s.count.fetch_sub(1, Ordering::SeqCst);
        if prev == 0 {
            s.count.fetch_add(1, Ordering::SeqCst);
            return Err("WaitGroup.done: counter would go negative".into());
        }
        if prev == 1 {
            let _g = s.lock.lock().map_err(|e| e.to_string())?;
            s.cvar.notify_all();
        }
        Ok(Value::Null)
    });
    let s = st.clone();
    insert_fn(&mut m, "wait", move |_env, _args| {
        let mut g = s.lock.lock().map_err(|e| e.to_string())?;
        while s.count.load(Ordering::SeqCst) != 0 {
            g = s
                .cvar
                .wait_timeout(g, Duration::from_millis(50))
                .map_err(|e| e.to_string())?
                .0;
        }
        Ok(Value::Null)
    });
    let s = st.clone();
    insert_fn(&mut m, "wait_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("WaitGroup.wait_timeout(duration)")?)?;
        let mut g = s.lock.lock().map_err(|e| e.to_string())?;
        let start = std::time::Instant::now();
        while s.count.load(Ordering::SeqCst) != 0 {
            let remain = d.saturating_sub(start.elapsed());
            if remain.is_zero() {
                return Ok(Value::Bool(false));
            }
            let (ng, w) = s
                .cvar
                .wait_timeout(g, remain.min(Duration::from_millis(50)))
                .map_err(|e| e.to_string())?;
            g = ng;
            if w.timed_out() && s.count.load(Ordering::SeqCst) != 0 && start.elapsed() >= d {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    });
    let s = st.clone();
    insert_fn(&mut m, "count", move |_env, _args| {
        Ok(Value::Number(s.count.load(Ordering::SeqCst) as f64))
    });
    with_kind(m, "WaitGroup")
}
