//! MPMC channels with ownership transfer. BLOCKING send/recv; try_* are NON-BLOCKING.
//!
//! Semantics:
//! - send moves the value to the receiver (clone of interpreter `Value`)
//! - bounded: send blocks when full
//! - unbounded: send never blocks except on close
//! - close: further send fails; recv drains then returns closed
//! - Sender is cloneable (multi-producer); Receiver is cloneable (multi-consumer)
//! - FIFO among messages that complete send
//! - mutex unlock of the queue establishes happens-before for the payload

use super::helpers::*;
use crate::parsing::ast::Value;
use rustc_hash::FxHashMap as HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

struct ChanInner {
    q: Mutex<VecDeque<CrossThread>>,
    cap: Option<usize>,
    closed: AtomicBool,
    senders: AtomicUsize,
    receivers: AtomicUsize,
    not_empty: Condvar,
    not_full: Condvar,
}

fn make_chan(cap: Option<usize>) -> (Value, Value) {
    let inner = Arc::new(ChanInner {
        q: Mutex::new(VecDeque::new()),
        cap,
        closed: AtomicBool::new(false),
        senders: AtomicUsize::new(1),
        receivers: AtomicUsize::new(1),
        not_empty: Condvar::new(),
        not_full: Condvar::new(),
    });
    (make_sender(inner.clone()), make_receiver(inner))
}

fn make_sender(inner: Arc<ChanInner>) -> Value {
    let mut m = HashMap::default();
    let i = inner.clone();
    insert_fn(&mut m, "send", move |_env, args| {
        send_inner(&i, args.first().cloned().unwrap_or(Value::Null), None)
    });
    let i = inner.clone();
    insert_fn(&mut m, "try_send", move |_env, args| {
        send_inner(
            &i,
            args.first().cloned().unwrap_or(Value::Null),
            Some(Duration::ZERO),
        )
    });
    let i = inner.clone();
    insert_fn(&mut m, "send_timeout", move |_env, args| {
        let v = args.first().cloned().unwrap_or(Value::Null);
        let d = parse_duration(args.get(1).ok_or("send_timeout(value, duration)")?)?;
        send_inner(&i, v, Some(d))
    });
    let i = inner.clone();
    insert_fn(&mut m, "close", move |_env, _args| {
        i.closed.store(true, Ordering::Release);
        i.not_empty.notify_all();
        i.not_full.notify_all();
        Ok(Value::Null)
    });
    let i = inner.clone();
    insert_fn(&mut m, "is_closed", move |_env, _args| {
        Ok(Value::Bool(i.closed.load(Ordering::Acquire)))
    });
    let i = inner.clone();
    insert_fn(&mut m, "clone", move |_env, _args| {
        i.senders.fetch_add(1, Ordering::Relaxed);
        Ok(make_sender(i.clone()))
    });
    with_kind(m, "Sender")
}

fn make_receiver(inner: Arc<ChanInner>) -> Value {
    let mut m = HashMap::default();
    let i = inner.clone();
    insert_fn(&mut m, "recv", move |_env, _args| recv_inner(&i, None));
    let i = inner.clone();
    insert_fn(&mut m, "try_recv", move |_env, _args| {
        recv_inner(&i, Some(Duration::ZERO))
    });
    let i = inner.clone();
    insert_fn(&mut m, "recv_timeout", move |_env, args| {
        let d = parse_duration(args.first().ok_or("recv_timeout(duration)")?)?;
        recv_inner(&i, Some(d))
    });
    let i = inner.clone();
    insert_fn(&mut m, "is_closed", move |_env, _args| {
        Ok(Value::Bool(i.closed.load(Ordering::Acquire)))
    });
    let i = inner.clone();
    insert_fn(&mut m, "clone", move |_env, _args| {
        i.receivers.fetch_add(1, Ordering::Relaxed);
        Ok(make_receiver(i.clone()))
    });
    with_kind(m, "Receiver")
}

fn send_inner(inner: &ChanInner, v: Value, timeout: Option<Duration>) -> Result<Value, String> {
    if inner.closed.load(Ordering::Acquire) {
        return Ok(closed_err());
    }
    let mut q = inner.q.lock().map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    loop {
        if inner.closed.load(Ordering::Acquire) {
            return Ok(closed_err());
        }
        let full = inner.cap.map(|c| q.len() >= c).unwrap_or(false);
        if !full {
            q.push_back(CrossThread(v));
            inner.not_empty.notify_one();
            return Ok(ok_obj(Value::Null));
        }
        match timeout {
            Some(d) if d.is_zero() => return Ok(err_obj("full", "channel is full")),
            Some(d) => {
                let remain = d.saturating_sub(start.elapsed());
                if remain.is_zero() {
                    return Ok(timeout_err());
                }
                let (nq, w) = inner
                    .not_full
                    .wait_timeout(q, remain)
                    .map_err(|e| e.to_string())?;
                q = nq;
                if w.timed_out() {
                    return Ok(timeout_err());
                }
            }
            None => {
                q = inner.not_full.wait(q).map_err(|e| e.to_string())?;
            }
        }
    }
}

fn recv_inner(inner: &ChanInner, timeout: Option<Duration>) -> Result<Value, String> {
    let mut q = inner.q.lock().map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    loop {
        if let Some(CrossThread(v)) = q.pop_front() {
            inner.not_full.notify_one();
            return Ok(ok_obj(v));
        }
        if inner.closed.load(Ordering::Acquire) {
            return Ok(closed_err());
        }
        match timeout {
            Some(d) if d.is_zero() => return Ok(err_obj("empty", "channel is empty")),
            Some(d) => {
                let remain = d.saturating_sub(start.elapsed());
                if remain.is_zero() {
                    return Ok(timeout_err());
                }
                let (nq, w) = inner
                    .not_empty
                    .wait_timeout(q, remain)
                    .map_err(|e| e.to_string())?;
                q = nq;
                if w.timed_out() && q.is_empty() {
                    return Ok(timeout_err());
                }
            }
            None => {
                q = inner.not_empty.wait(q).map_err(|e| e.to_string())?;
            }
        }
    }
}

pub fn channel_type() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "create", |_env, args| {
        let cap = args.first().and_then(num_of).map(|n| n as usize);
        let (tx, rx) = make_chan(cap);
        Ok(Value::Array(vec![tx, rx]))
    });
    insert_fn(&mut m, "bounded", |_env, args| {
        let cap = args
            .first()
            .and_then(num_of)
            .ok_or("channel.bounded(capacity)")? as usize;
        if cap == 0 {
            return Err("bounded channel capacity must be > 0".into());
        }
        let (tx, rx) = make_chan(Some(cap));
        Ok(Value::Array(vec![tx, rx]))
    });
    insert_fn(&mut m, "unbounded", |_env, _args| {
        let (tx, rx) = make_chan(None);
        Ok(Value::Array(vec![tx, rx]))
    });
    insert_fn(&mut m, "select", |_env, args| {
        // Non-syntax select: channel.select([{recv: rx}, {recv: rx2}, {timeout: ms}, {default: true}])
        select_impl(&args)
    });
    with_kind(m, "Channel")
}

fn select_impl(args: &[Value]) -> Result<Value, String> {
    let branches = match args.first() {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::DynArray(d)) => d.data.clone(),
        _ => return Err("channel.select([{recv: rx}, ...])".into()),
    };
    let mut timeout: Option<Duration> = None;
    let mut has_default = false;
    let mut recvs: Vec<Value> = Vec::new();
    for b in &branches {
        if let Value::Object(map) = b {
            if let Some(rx) = map.get("recv") {
                recvs.push(rx.clone());
            }
            if let Some(t) = map.get("timeout") {
                timeout = Some(parse_duration(t)?);
            }
            if map.contains_key("default") {
                has_default = true;
            }
        }
    }
    let start = std::time::Instant::now();
    loop {
        for (idx, rx) in recvs.iter().enumerate() {
            if let Value::Object(map) = rx {
                if let Some(Value::Function(nf)) = map.get("try_recv") {
                    let mut env = crate::parsing::ast::NoopEnv;
                    let got = (nf.0)(&mut env, vec![])?;
                    if let Value::Object(res) = &got {
                        if let Some(Value::Bool(true)) = res.get("ok") {
                            let mut out = HashMap::default();
                            out.insert("index".into(), Value::Number(idx as f64));
                            if let Some(v) = res.get("value") {
                                out.insert("value".into(), v.clone());
                            }
                            return Ok(Value::Object(Arc::new(out)));
                        }
                    }
                }
            }
        }
        if has_default {
            let mut out = HashMap::default();
            out.insert("index".into(), Value::Number(-1.0));
            out.insert("default".into(), Value::Bool(true));
            return Ok(Value::Object(Arc::new(out)));
        }
        if let Some(d) = timeout {
            if start.elapsed() >= d {
                return Ok(timeout_err());
            }
        }
        std::thread::yield_now();
        if timeout.is_none() && recvs.is_empty() {
            return Err("select has no recv branches".into());
        }
        if timeout.is_none() {
            std::thread::sleep(Duration::from_micros(50));
        }
    }
}
