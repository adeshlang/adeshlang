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
        Value::RawArray(_, a) => a.clone(),
        _ => return Err("parallel_map expects an array".into()),
    };
    if arr.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    if arr.len() < 32 {
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            out.push(call_fn(env, &args[1], vec![item])?);
        }
        return Ok(Value::Array(out));
    }

    use rayon::prelude::*;
    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let sched = crate::runtime::scheduler::global_scheduler();
    let results: Result<Vec<CrossThread>, String> = sched.install(|| {
        cross_arr
            .into_par_iter()
            .map(|item| f.call(vec![item.get_value()]).map(CrossThread))
            .collect()
    });
    let values: Vec<Value> = results?.into_iter().map(|c| c.into_inner()).collect();
    Ok(Value::Array(values))
}

pub fn builtin_parallel_for(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("thread.parallel_for(start, end, fn)".into());
    }
    let start = num_of(&args[0]).ok_or("start")? as i64;
    let end = num_of(&args[1]).ok_or("end")? as i64;
    if start >= end {
        return Ok(Value::Null);
    }
    let func = args[2].clone();
    if end - start < 32 {
        for i in start..end {
            call_fn(env, &func, vec![Value::Number(i as f64)])?;
        }
        return Ok(Value::Null);
    }

    use rayon::prelude::*;
    let f = CrossThread(func);
    let error_slot = Arc::new(Mutex::new(None));
    let sched = crate::runtime::scheduler::global_scheduler();

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
        Value::RawArray(_, a) => a.clone(),
        _ => return Err("expected array".into()),
    };
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

    use rayon::prelude::*;
    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let workers = crate::runtime::scheduler::num_workers().max(1);
    let chunk_size = (cross_arr.len() / workers).max(16);
    let chunks: Vec<Vec<CrossThread>> = cross_arr.chunks(chunk_size).map(|c| c.to_vec()).collect();

    let f = CrossThread(args[2].clone());
    let init_val = CrossThread(args[1].clone());
    let sched = crate::runtime::scheduler::global_scheduler();

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

pub fn builtin_parallel_each(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("thread.parallel_each(array, fn)".into());
    }
    let arr = match &args[0] {
        Value::Array(a) => a.clone(),
        Value::DynArray(d) => d.data.clone(),
        Value::RawArray(_, a) => a.clone(),
        _ => return Err("expected array".into()),
    };
    if arr.is_empty() {
        return Ok(Value::Null);
    }
    if arr.len() < 32 {
        for item in arr {
            call_fn(env, &args[1], vec![item])?;
        }
        return Ok(Value::Null);
    }

    use rayon::prelude::*;
    let cross_arr: Vec<CrossThread> = arr.into_iter().map(CrossThread).collect();
    let f = CrossThread(args[1].clone());
    let error_slot = Arc::new(Mutex::new(None));
    let sched = crate::runtime::scheduler::global_scheduler();

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
