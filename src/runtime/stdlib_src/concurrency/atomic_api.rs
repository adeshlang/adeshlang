//! Atomic types with explicit memory ordering and wait/notify.
//!
//! Classification: NON-BLOCKING for load/store/CAS/fetch_*; BLOCKING for wait().
//! Not wait-free (CAS loops may retry). fetch_add on supported widths is lock-free
//! on the target CPU.

use super::helpers::*;
use crate::parsing::ast::Value;
use rustc_hash::FxHashMap as HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

struct WaitCell {
    cvar: Condvar,
    lock: Mutex<()>,
}

impl WaitCell {
    fn new() -> Self {
        Self {
            cvar: Condvar::new(),
            lock: Mutex::new(()),
        }
    }
    fn wait_timeout(&self, d: Option<Duration>) {
        let g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(d) = d {
            drop(self.cvar.wait_timeout(g, d));
        } else {
            drop(self.cvar.wait(g));
        }
    }
    fn notify_one(&self) {
        self.cvar.notify_one();
    }
    fn notify_all(&self) {
        self.cvar.notify_all();
    }
}

fn i64_atom(init: i64) -> Value {
    let a = Arc::new(AtomicI64::new(init));
    let wait = Arc::new(WaitCell::new());
    let mut m = HashMap::default();
    let x = a.clone();
    insert_fn(&mut m, "load", move |_env, args| {
        let o = memory_order_from(args.first())?;
        Ok(Value::Number(x.load(o) as f64))
    });
    let x = a.clone();
    let w = wait.clone();
    insert_fn(&mut m, "store", move |_env, args| {
        let v = args.first().and_then(num_of).ok_or("store(value)")? as i64;
        let o = memory_order_from(args.get(1))?;
        x.store(v, o);
        w.notify_all();
        Ok(Value::Null)
    });
    let x = a.clone();
    insert_fn(&mut m, "swap", move |_env, args| {
        let v = args.first().and_then(num_of).ok_or("swap(value)")? as i64;
        let o = memory_order_from(args.get(1))?;
        Ok(Value::Number(x.swap(v, o) as f64))
    });
    let x = a.clone();
    insert_fn(&mut m, "compare_exchange", move |_env, args| {
        if args.len() < 2 {
            return Err("compare_exchange(current, new, [success], [failure])".into());
        }
        let cur = num_of(&args[0]).ok_or("current")? as i64;
        let new = num_of(&args[1]).ok_or("new")? as i64;
        let suc = memory_order_from(args.get(2))?;
        let fail = memory_order_from(args.get(3)).unwrap_or(Ordering::Relaxed);
        match x.compare_exchange(cur, new, suc, fail) {
            Ok(v) => Ok(ok_obj(Value::Number(v as f64))),
            Err(v) => Ok(err_obj("mismatch", format!("found {v}"))),
        }
    });
    let x = a.clone();
    insert_fn(&mut m, "compare_exchange_weak", move |_env, args| {
        if args.len() < 2 {
            return Err("compare_exchange_weak(current, new)".into());
        }
        let cur = num_of(&args[0]).ok_or("current")? as i64;
        let new = num_of(&args[1]).ok_or("new")? as i64;
        match x.compare_exchange_weak(cur, new, Ordering::SeqCst, Ordering::Relaxed) {
            Ok(v) => Ok(ok_obj(Value::Number(v as f64))),
            Err(v) => Ok(err_obj("mismatch", format!("found {v}"))),
        }
    });
    for (name, op) in [
        ("fetch_add", 0u8),
        ("fetch_sub", 1),
        ("fetch_and", 2),
        ("fetch_or", 3),
        ("fetch_xor", 4),
        ("fetch_min", 5),
        ("fetch_max", 6),
    ] {
        let x = a.clone();
        insert_fn(&mut m, name, move |_env, args| {
            let v = args.first().and_then(num_of).unwrap_or(1.0) as i64;
            let o = memory_order_from(args.get(1))?;
            let prev = match op {
                0 => x.fetch_add(v, o),
                1 => x.fetch_sub(v, o),
                2 => x.fetch_and(v, o),
                3 => x.fetch_or(v, o),
                4 => x.fetch_xor(v, o),
                5 => x.fetch_min(v, o),
                _ => x.fetch_max(v, o),
            };
            Ok(Value::Number(prev as f64))
        });
    }
    let x = a.clone();
    let w = wait.clone();
    insert_fn(&mut m, "wait", move |_env, args| {
        let expected = args.first().and_then(num_of).ok_or("wait(expected)")? as i64;
        let timeout = args.get(1).map(parse_duration).transpose()?;
        loop {
            if x.load(Ordering::Acquire) != expected {
                return Ok(Value::Null);
            }
            w.wait_timeout(timeout);
            if timeout.is_some() {
                return Ok(Value::Null);
            }
        }
    });
    let w = wait.clone();
    insert_fn(&mut m, "notify_one", move |_env, _args| {
        w.notify_one();
        Ok(Value::Null)
    });
    let w = wait;
    insert_fn(&mut m, "notify_all", move |_env, _args| {
        w.notify_all();
        Ok(Value::Null)
    });
    with_kind(m, "AtomicI64")
}

fn bool_atom(init: bool) -> Value {
    let a = Arc::new(AtomicBool::new(init));
    let mut m = HashMap::default();
    let x = a.clone();
    insert_fn(&mut m, "load", move |_env, args| {
        Ok(Value::Bool(x.load(memory_order_from(args.first())?)))
    });
    let x = a.clone();
    insert_fn(&mut m, "store", move |_env, args| {
        let v = as_bool(args.first().ok_or("store(bool)")?).ok_or("expected bool")?;
        x.store(v, memory_order_from(args.get(1))?);
        Ok(Value::Null)
    });
    let x = a.clone();
    insert_fn(&mut m, "swap", move |_env, args| {
        let v = as_bool(args.first().ok_or("swap(bool)")?).ok_or("expected bool")?;
        Ok(Value::Bool(x.swap(v, memory_order_from(args.get(1))?)))
    });
    let x = a.clone();
    insert_fn(&mut m, "compare_exchange", move |_env, args| {
        if args.len() < 2 {
            return Err("compare_exchange(current, new)".into());
        }
        let cur = as_bool(&args[0]).ok_or("bool")?;
        let new = as_bool(&args[1]).ok_or("bool")?;
        match x.compare_exchange(cur, new, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(v) => Ok(ok_obj(Value::Bool(v))),
            Err(v) => Ok(err_obj("mismatch", format!("found {v}"))),
        }
    });
    with_kind(m, "AtomicBool")
}

fn u64_atom(init: u64) -> Value {
    let a = Arc::new(AtomicU64::new(init));
    let mut m = HashMap::default();
    let x = a.clone();
    insert_fn(&mut m, "load", move |_env, args| {
        Ok(Value::Number(
            x.load(memory_order_from(args.first())?) as f64
        ))
    });
    let x = a.clone();
    insert_fn(&mut m, "store", move |_env, args| {
        let v = args.first().and_then(num_of).ok_or("store")? as u64;
        x.store(v, memory_order_from(args.get(1))?);
        Ok(Value::Null)
    });
    let x = a.clone();
    insert_fn(&mut m, "fetch_add", move |_env, args| {
        let v = args.first().and_then(num_of).unwrap_or(1.0) as u64;
        Ok(Value::Number(
            x.fetch_add(v, memory_order_from(args.get(1))?) as f64,
        ))
    });
    let x = a.clone();
    insert_fn(&mut m, "compare_exchange", move |_env, args| {
        if args.len() < 2 {
            return Err("compare_exchange(current, new)".into());
        }
        let cur = num_of(&args[0]).ok_or("current")? as u64;
        let new = num_of(&args[1]).ok_or("new")? as u64;
        match x.compare_exchange(cur, new, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(v) => Ok(ok_obj(Value::Number(v as f64))),
            Err(v) => Ok(err_obj("mismatch", format!("found {v}"))),
        }
    });
    with_kind(m, "AtomicU64")
}

fn ptr_atom() -> Value {
    let a = Arc::new(AtomicPtr::<()>::new(std::ptr::null_mut()));
    let mut m = HashMap::default();
    let x = a.clone();
    insert_fn(&mut m, "load", move |_env, args| {
        let p = x.load(memory_order_from(args.first())?) as u64;
        Ok(Value::Number(p as f64))
    });
    let x = a.clone();
    insert_fn(&mut m, "store", move |_env, args| {
        let p = args.first().and_then(num_of).unwrap_or(0.0) as u64 as *mut ();
        x.store(p, memory_order_from(args.get(1))?);
        Ok(Value::Null)
    });
    with_kind(m, "AtomicPtr")
}

fn ctor(kind: &'static str) -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "new", move |_env, args| {
        Ok(match kind {
            "AtomicBool" => bool_atom(args.first().and_then(as_bool).unwrap_or(false)),
            "AtomicU64" | "AtomicU32" | "AtomicU16" | "AtomicU8" => {
                u64_atom(args.first().and_then(num_of).unwrap_or(0.0) as u64)
            }
            "AtomicPtr" => ptr_atom(),
            _ => i64_atom(args.first().and_then(num_of).unwrap_or(0.0) as i64),
        })
    });
    with_kind(m, kind)
}

pub fn atomic_namespace() -> Value {
    let mut m = HashMap::default();
    for k in [
        "AtomicBool",
        "AtomicI8",
        "AtomicI16",
        "AtomicI32",
        "AtomicI64",
        "AtomicU8",
        "AtomicU16",
        "AtomicU32",
        "AtomicU64",
        "AtomicPtr",
    ] {
        m.insert(k.to_string(), ctor(k));
    }
    m.insert("Relaxed".into(), Value::Str("Relaxed".into()));
    m.insert("Acquire".into(), Value::Str("Acquire".into()));
    m.insert("Release".into(), Value::Str("Release".into()));
    m.insert("AcqRel".into(), Value::Str("AcqRel".into()));
    m.insert("SeqCst".into(), Value::Str("SeqCst".into()));
    with_kind(m, "Atomic")
}
