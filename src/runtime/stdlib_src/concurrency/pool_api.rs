//! Thread pool with per-worker queues and optional work stealing.
//! Tasks are ownership-transferred onto worker threads. Panic isolated per task.

use super::helpers::*;
use super::thread_api::spawn_with;
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::runtime::thread;
use rustc_hash::FxHashMap as HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

struct Job {
    func: CrossThread,
    result: Option<Arc<JobResult>>,
}

struct JobResult {
    done: AtomicBool,
    value: Mutex<Option<Result<CrossThread, String>>>,
    cvar: Condvar,
}

struct PoolInner {
    queues: Vec<Arc<Mutex<VecDeque<Job>>>>,
    global: Mutex<VecDeque<Job>>,
    cvar: Condvar,
    stop: AtomicBool,
    running: AtomicUsize,
    next: AtomicUsize,
    workers: usize,
}

pub fn thread_pool_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, args| {
        let n = args
            .first()
            .and_then(num_of)
            .map(|x| x as usize)
            .unwrap_or_else(thread::hardware_concurrency)
            .max(1);
        Ok(make_pool(n))
    });
    with_kind(m, "ThreadPoolType")
}

fn make_pool(workers: usize) -> Value {
    if !thread::platform::threading_supported() {
        // Return object whose execute errors clearly.
    }
    let mut queues = Vec::with_capacity(workers);
    for _ in 0..workers {
        queues.push(Arc::new(Mutex::new(VecDeque::new())));
    }
    let inner = Arc::new(PoolInner {
        queues,
        global: Mutex::new(VecDeque::new()),
        cvar: Condvar::new(),
        stop: AtomicBool::new(false),
        running: AtomicUsize::new(0),
        next: AtomicUsize::new(0),
        workers,
    });
    for i in 0..workers {
        inner.running.fetch_add(1, Ordering::Relaxed);
        let inn = inner.clone();
        if std::thread::Builder::new()
            .name(format!("adesh-pool-{i}"))
            .spawn(move || worker_loop(inn, i))
            .is_err()
        {
            inner.running.fetch_sub(1, Ordering::Relaxed);
            inner.cvar.notify_all();
        }
    }
    let mut m = HashMap::default();
    let p = inner.clone();
    insert_fn(&mut m, "execute", move |_env, args| {
        let func = require_fn(&args, 0, "ThreadPool.execute")?;
        submit(&p, func, false)?;
        Ok(Value::Null)
    });
    let p = inner.clone();
    insert_fn(&mut m, "submit", move |_env, args| {
        let func = require_fn(&args, 0, "ThreadPool.submit")?;
        let jr = submit(&p, func, true)?.expect("result slot");
        Ok(make_future(jr))
    });
    let p = inner.clone();
    insert_fn(&mut m, "shutdown", move |_env, _args| {
        p.stop.store(true, Ordering::Release);
        p.cvar.notify_all();
        Ok(Value::Null)
    });
    let p = inner.clone();
    insert_fn(&mut m, "join", move |_env, _args| pool_join(&p, None));
    let p = inner.clone();
    insert_fn(&mut m, "join_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("ThreadPool.join_timeout(duration)")?)?;
        pool_join(&p, Some(d))
    });
    let p = inner.clone();
    insert_fn(&mut m, "size", move |_env, _args| {
        Ok(Value::Number(p.workers as f64))
    });
    let p = inner.clone();
    insert_fn(&mut m, "map", move |_env, args| {
        let tasks = match args.first() {
            Some(Value::Array(a)) => a.clone(),
            Some(Value::DynArray(d)) => d.data.clone(),
            _ => return Err("ThreadPool.map(array_of_functions)".into()),
        };
        let mut futures = Vec::with_capacity(tasks.len());
        for t in tasks {
            require_fn(&[t.clone()], 0, "ThreadPool.map")?;
            let jr = submit(&p, t, true)?.expect("result slot");
            futures.push(make_future(jr));
        }
        let mut out = Vec::with_capacity(futures.len());
        for f in futures {
            if let Value::Object(map) = &f {
                if let Some(Value::Function(nf)) = map.get("get") {
                    out.push((nf.0)(_env, vec![])?);
                    continue;
                }
            }
            return Err("ThreadPool.map: invalid future".into());
        }
        Ok(Value::Array(out))
    });
    with_kind(m, "ThreadPool")
}

fn submit(
    pool: &Arc<PoolInner>,
    func: Value,
    want_result: bool,
) -> Result<Option<Arc<JobResult>>, String> {
    if pool.stop.load(Ordering::Acquire) {
        return Err("thread pool is shut down".into());
    }
    let result = if want_result {
        Some(Arc::new(JobResult {
            done: AtomicBool::new(false),
            value: Mutex::new(None),
            cvar: Condvar::new(),
        }))
    } else {
        None
    };
    let job = Job {
        func: CrossThread(func),
        result: result.clone(),
    };
    let idx = pool.next.fetch_add(1, Ordering::Relaxed) % pool.workers;
    pool.queues[idx]
        .lock()
        .map_err(|e| e.to_string())?
        .push_back(job);
    pool.cvar.notify_one();
    Ok(result)
}

fn pool_join(pool: &Arc<PoolInner>, timeout: Option<std::time::Duration>) -> Result<Value, String> {
    pool.stop.store(true, Ordering::Release);
    pool.cvar.notify_all();
    let start = std::time::Instant::now();
    while pool.running.load(Ordering::Acquire) > 0 {
        if let Some(d) = timeout {
            if start.elapsed() >= d {
                return Ok(timeout_err());
            }
        }
        let g = pool.global.lock().unwrap_or_else(|p| p.into_inner());
        if pool.running.load(Ordering::Acquire) == 0 {
            break;
        }
        let slice = timeout
            .map(|d| {
                d.saturating_sub(start.elapsed())
                    .min(std::time::Duration::from_millis(50))
            })
            .unwrap_or(std::time::Duration::from_millis(50));
        drop(pool.cvar.wait_timeout(g, slice));
    }
    if pool.running.load(Ordering::Acquire) > 0 {
        return Ok(timeout_err());
    }
    Ok(Value::Null)
}

struct RunningGuard(Arc<PoolInner>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::Relaxed);
        self.0.cvar.notify_all();
    }
}

fn worker_loop(pool: Arc<PoolInner>, id: usize) {
    let _running = RunningGuard(pool.clone());
    let tid = thread::ThreadId::allocate();
    thread::ThreadId::install(tid);
    thread::register_current(tid, Some(format!("adesh-pool-{id}")));
    loop {
        if pool.stop.load(Ordering::Acquire) {
            break;
        }
        let job = pop_job(&pool, id);
        match job {
            Some(job) => run_job(job),
            None => {
                let g = pool.global.lock().unwrap_or_else(|p| p.into_inner());
                if pool.stop.load(Ordering::Acquire) {
                    break;
                }
                drop(
                    pool.cvar
                        .wait_timeout(g, std::time::Duration::from_millis(50)),
                );
            }
        }
    }
    thread::unregister(tid);
}

fn pop_job(pool: &PoolInner, id: usize) -> Option<Job> {
    if let Ok(mut q) = pool.queues[id].lock() {
        if let Some(j) = q.pop_front() {
            return Some(j);
        }
    }
    if let Ok(mut g) = pool.global.lock() {
        if let Some(j) = g.pop_front() {
            return Some(j);
        }
    }
    // Work stealing
    for off in 1..pool.workers {
        let victim = (id + off) % pool.workers;
        if let Ok(mut q) = pool.queues[victim].lock() {
            if let Some(j) = q.pop_back() {
                return Some(j);
            }
        }
    }
    None
}

fn run_job(job: Job) {
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        call_fn_on_thread(job.func.0, vec![])
    }));
    if let Ok(Err(ref e)) = out {
        eprintln!("[ThreadPool job error]: {}", e);
    }
    if let Some(slot) = job.result {
        let stored = match out {
            Ok(Ok(v)) => Ok(CrossThread(v)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("task panicked".into()),
        };
        if let Ok(mut g) = slot.value.lock() {
            *g = Some(stored);
        }
        slot.done.store(true, Ordering::Release);
        slot.cvar.notify_all();
    }
}

fn make_future(jr: Arc<JobResult>) -> Value {
    let mut m = HashMap::default();
    let j = jr.clone();
    insert_fn(&mut m, "get", move |_env, _args| {
        let mut g = j.value.lock().map_err(|e| e.to_string())?;
        while !j.done.load(Ordering::Acquire) {
            g = j
                .cvar
                .wait_timeout(g, std::time::Duration::from_millis(50))
                .map_err(|e| e.to_string())?
                .0;
        }
        match g.take() {
            Some(Ok(CrossThread(v))) => Ok(v),
            Some(Err(e)) => Ok(err_obj("error", e)),
            None => Ok(Value::Null),
        }
    });
    let j = jr.clone();
    insert_fn(&mut m, "is_finished", move |_env, _args| {
        Ok(Value::Bool(j.done.load(Ordering::Acquire)))
    });
    with_kind(m, "PoolFuture")
}

pub fn builtin_parallel_map(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("thread.parallel_map(array, fn)".into());
    }
    let arr = match &args[0] {
        Value::Array(a) => a.clone(),
        Value::DynArray(d) => d.data.clone(),
        _ => return Err("parallel_map expects an array".into()),
    };
    let func = args[1].clone();
    let n = arr.len();
    if n == 0 {
        return Ok(Value::Array(vec![]));
    }
    if n < 32 {
        let mut out = Vec::with_capacity(n);
        for item in arr {
            out.push(call_fn(env, &func, vec![item])?);
        }
        return Ok(Value::Array(out));
    }
    let slots: Arc<Vec<Mutex<Option<Value>>>> =
        Arc::new((0..n).map(|_| Mutex::new(None)).collect());
    let mut handles = Vec::new();
    let workers = thread::hardware_concurrency().max(1);
    let chunk = (n / workers).max(1);
    for w in 0..workers {
        let start = w * chunk;
        let end = if w + 1 == workers {
            n
        } else {
            (start + chunk).min(n)
        };
        if start >= n {
            break;
        }
        let items: Vec<Value> = arr[start..end].to_vec();
        let f = func.clone();
        let sl = slots.clone();
        let h = spawn_with(
            Value::Function(crate::parsing::ast::NativeFn(std::sync::Arc::new(
                move |_e, _a| {
                    for (i, item) in items.iter().enumerate() {
                        let v = call_fn_on_thread(f.clone(), vec![item.clone()])?;
                        *sl[start + i].lock().map_err(|e| e.to_string())? = Some(v);
                    }
                    Ok(Value::Null)
                },
            ))),
            Default::default(),
        )?;
        handles.push(h);
    }
    for h in handles {
        if let Value::Object(map) = h {
            if let Some(Value::Function(nf)) = map.get("join") {
                let _ = (nf.0)(env, vec![]);
            }
        }
    }
    let mut out = Vec::with_capacity(n);
    for s in slots.iter() {
        out.push(
            s.lock()
                .map_err(|e| e.to_string())?
                .clone()
                .unwrap_or(Value::Null),
        );
    }
    Ok(Value::Array(out))
}

pub fn builtin_parallel_for(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("thread.parallel_for(start, end, fn)".into());
    }
    let start = num_of(&args[0]).ok_or("start")? as i64;
    let end = num_of(&args[1]).ok_or("end")? as i64;
    let func = args[2].clone();
    for i in start..end {
        call_fn(env, &func, vec![Value::Number(i as f64)])?;
    }
    Ok(Value::Null)
}

pub fn builtin_parallel_reduce(
    env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("thread.parallel_reduce(array, init, fn)".into());
    }
    let arr = match &args[0] {
        Value::Array(a) => a.clone(),
        Value::DynArray(d) => d.data.clone(),
        _ => return Err("expected array".into()),
    };
    let mut acc = args[1].clone();
    for item in arr {
        acc = call_fn(env, &args[2], vec![acc, item])?;
    }
    Ok(acc)
}

pub fn builtin_parallel_each(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("thread.parallel_each(array, fn)".into());
    }
    let arr = match &args[0] {
        Value::Array(a) => a.clone(),
        Value::DynArray(d) => d.data.clone(),
        _ => return Err("expected array".into()),
    };
    for item in arr {
        call_fn(env, &args[1], vec![item])?;
    }
    Ok(Value::Null)
}

pub fn concurrent_queue_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", |_env, _args| {
        let q: Arc<Mutex<VecDeque<CrossThread>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut o = HashMap::default();
        let qq = q.clone();
        insert_fn(&mut o, "push", move |_env, args| {
            qq.lock()
                .map_err(|e| e.to_string())?
                .push_back(CrossThread(args.first().cloned().unwrap_or(Value::Null)));
            Ok(Value::Null)
        });
        let qq = q.clone();
        insert_fn(&mut o, "pop", move |_env, _args| {
            Ok(match qq.lock().map_err(|e| e.to_string())?.pop_front() {
                Some(CrossThread(v)) => ok_obj(v),
                None => err_obj("empty", "queue empty"),
            })
        });
        let qq = q.clone();
        insert_fn(&mut o, "len", move |_env, _args| {
            Ok(Value::Number(
                qq.lock().map_err(|e| e.to_string())?.len() as f64
            ))
        });
        Ok(with_kind(o, "ConcurrentQueue"))
    });
    with_kind(m, "ConcurrentQueueType")
}
