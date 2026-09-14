//! Shared helpers for the AdeshLang threading stdlib (interpreter `Value` API).

use crate::execution::runtime::public_call_user;
use crate::parsing::ast::{BuiltinEnv, NativeFn, NoopEnv, Value};
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Values moved onto OS threads. Interpreter `Value` contains `NativeFn`
/// trait objects that are not auto-`Send`; the language Send/Sync checker
/// is the safety gate, matching the HTTP/WebSocket worker pattern.
pub struct CrossThread(pub Value);
unsafe impl Send for CrossThread {}
unsafe impl Sync for CrossThread {}

impl Clone for CrossThread {
    fn clone(&self) -> Self {
        CrossThread(self.0.clone())
    }
}

impl CrossThread {
    pub fn into_inner(self) -> Value {
        self.0
    }
}

/// Opaque Send wrapper for spawn payloads. `Value` is not auto-Send.
pub struct SendJob {
    pub func: CrossThread,
}
unsafe impl Send for SendJob {}
unsafe impl Sync for SendJob {}

impl SendJob {
    pub fn new(func: Value) -> Self {
        Self {
            func: CrossThread(func),
        }
    }
}

pub fn insert_fn<F>(methods: &mut HashMap<String, Value>, name: &str, f: F)
where
    F: Fn(&mut dyn BuiltinEnv, Vec<Value>) -> Result<Value, String> + 'static,
{
    methods.insert(name.to_string(), Value::Function(NativeFn(Arc::new(f))));
}

pub fn num_of(v: &Value) -> Option<f64> {
    v.as_f64()
}

pub fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

pub fn as_str(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) => Some(s),
        _ => None,
    }
}

pub fn err_obj(kind: &str, message: impl Into<String>) -> Value {
    let mut m = HashMap::default();
    m.insert("ok".into(), Value::Bool(false));
    m.insert("kind".into(), Value::Str(kind.into()));
    m.insert("message".into(), Value::Str(message.into()));
    Value::Object(Arc::new(m))
}

pub fn ok_obj(value: Value) -> Value {
    let mut m = HashMap::default();
    m.insert("ok".into(), Value::Bool(true));
    m.insert("value".into(), value);
    Value::Object(Arc::new(m))
}

pub fn timeout_err() -> Value {
    err_obj("timeout", "operation timed out")
}

pub fn closed_err() -> Value {
    err_obj("closed", "channel is closed")
}

pub fn parse_duration(v: &Value) -> Result<Duration, String> {
    if let Some(ms) = num_of(v) {
        return crate::runtime::thread::duration_from_millis_f64(ms);
    }
    match v {
        Value::Number(n) | Value::F64(n) => crate::runtime::thread::duration_from_millis_f64(*n),
        Value::F32(n) => crate::runtime::thread::duration_from_millis_f64(*n as f64),
        Value::I64(n) => crate::runtime::thread::duration_from_millis_f64(*n as f64),
        Value::I32(n) => crate::runtime::thread::duration_from_millis_f64(*n as f64),
        Value::U64(n) => crate::runtime::thread::duration_from_millis_f64(*n as f64),
        Value::U32(n) => crate::runtime::thread::duration_from_millis_f64(*n as f64),
        Value::BigInt(bi) => {
            use num_traits::ToPrimitive;
            let ns = bi.to_u128().unwrap_or(0);
            Ok(Duration::from_nanos(ns.min(u64::MAX as u128) as u64))
        }
        Value::Instance(inst) => {
            let fields = inst
                .fields
                .read()
                .map_err(|_| "Duration instance poisoned".to_string())?;
            if let Some(nanos) = fields.get("_nanos") {
                return parse_duration(nanos);
            }
            Err("expected Duration instance with _nanos".into())
        }
        Value::Object(map) => {
            if let Some(ns) = map.get("nanos").and_then(num_of) {
                return crate::runtime::thread::duration_from_millis_f64(ns / 1_000_000.0);
            }
            if let Some(ms) = map.get("millis").and_then(num_of) {
                return crate::runtime::thread::duration_from_millis_f64(ms);
            }
            if let Some(s) = map.get("seconds").and_then(num_of) {
                return crate::runtime::thread::duration_from_millis_f64(s * 1000.0);
            }
            Err("duration object needs nanos, millis, or seconds".into())
        }
        _ => Err("expected duration (milliseconds number, Duration, or {millis: n})".into()),
    }
}

pub fn call_fn(env: &mut dyn BuiltinEnv, func: &Value, args: Vec<Value>) -> Result<Value, String> {
    match func {
        Value::Function(NativeFn(f)) => (f)(env, args),
        Value::UserFunction(u) => {
            public_call_user(u.clone(), args, None, env.native_side_effects())
        }
        Value::BoundMethod(u, inst) => crate::execution::runtime::public_call_user_with_this(
            u.clone(),
            args,
            *inst.clone(),
            None,
            env.native_side_effects(),
        ),
        other => Err(format!("expected function, got {:?}", type_name(other))),
    }
}

pub fn call_fn_on_thread(func: Value, args: Vec<Value>) -> Result<Value, String> {
    let mut env = NoopEnv;
    call_fn(&mut env, &func, args)
}

pub fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Number(_) => "number",
        Value::Bool(_) => "bool",
        Value::Str(_) => "string",
        Value::Null => "null",
        Value::Array(_) | Value::DynArray(_) | Value::RawArray(_, _) => "array",
        Value::Object(_) => "object",
        Value::Function(_) => "native_fn",
        Value::UserFunction(_) => "fn",
        Value::Instance(_) => "instance",
        _ => "value",
    }
}

pub fn require_fn(args: &[Value], idx: usize, what: &str) -> Result<Value, String> {
    args.get(idx)
        .cloned()
        .ok_or_else(|| format!("{what} requires a function argument"))
}

pub fn memory_order_from(v: Option<&Value>) -> Result<std::sync::atomic::Ordering, String> {
    use std::sync::atomic::Ordering;
    let Some(v) = v else {
        return Ok(Ordering::SeqCst);
    };
    let s = match v {
        Value::Str(s) => s.as_str(),
        Value::Number(n) => {
            return Ok(match *n as i32 {
                0 => Ordering::Relaxed,
                1 => Ordering::Acquire,
                2 => Ordering::Release,
                3 => Ordering::AcqRel,
                _ => Ordering::SeqCst,
            });
        }
        _ => return Err("memory order must be a string or number".into()),
    };
    Ok(match s {
        "Relaxed" | "relaxed" => Ordering::Relaxed,
        "Acquire" | "acquire" => Ordering::Acquire,
        "Release" | "release" => Ordering::Release,
        "AcqRel" | "acqrel" => Ordering::AcqRel,
        "SeqCst" | "seqcst" | "seq_cst" => Ordering::SeqCst,
        other => return Err(format!("unknown memory order '{other}'")),
    })
}

/// Tag an object so property dumps can identify it.
pub fn with_kind(mut methods: HashMap<String, Value>, kind: &str) -> Value {
    methods.insert("__kind".into(), Value::Str(kind.into()));
    Value::Object(Arc::new(methods))
}
