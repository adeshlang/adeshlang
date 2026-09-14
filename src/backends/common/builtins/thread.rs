//! JIT/AOT thread builtins that do not require interpreter closures.
//! Closure spawn remains an interpreter (and `public_call_user`) path.

use super::RuntimeValue;
use crate::runtime::thread;

pub(crate) fn runtime_thread_sleep(args: &[RuntimeValue]) -> RuntimeValue {
    let ms = args.first().and_then(|v| v.as_int()).unwrap_or(0).max(0) as u64;
    thread::sleep(std::time::Duration::from_millis(ms));
    RuntimeValue::Null
}

pub(crate) fn runtime_thread_yield(_args: &[RuntimeValue]) -> RuntimeValue {
    thread::yield_now();
    RuntimeValue::Null
}

pub(crate) fn runtime_thread_hardware_concurrency(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Int(thread::hardware_concurrency() as i64)
}

pub(crate) fn runtime_thread_id(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Int(thread::ThreadId::current().as_u64() as i64)
}

pub(crate) fn runtime_thread_park(args: &[RuntimeValue]) -> RuntimeValue {
    if let Some(ms) = args.first().and_then(|v| v.as_int()) {
        thread::park_timeout(std::time::Duration::from_millis(ms.max(0) as u64));
    } else {
        thread::park();
    }
    RuntimeValue::Null
}

pub(crate) fn runtime_cpu_count(_args: &[RuntimeValue]) -> RuntimeValue {
    runtime_thread_hardware_concurrency(_args)
}
