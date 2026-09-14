//! ARC (Automatic Reference Counting) FFI bridge for AOT compilation.
//!
//! Provides C-compatible runtime functions for managing reference-counted values
//! in AOT-compiled code (via Cranelift).

use crate::memory::arc_manager::ArcManager;
use crate::parsing::ast::Value;
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Mutex, OnceLock};

// Thread-local unsafe context tracking
thread_local! {
    pub(in crate::execution::runtime_core) static UNSAFE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Check if we're currently in an unsafe context.
pub(in crate::execution::runtime_core) fn in_unsafe_context() -> bool {
    UNSAFE_DEPTH.with(|c| c.get() > 0)
}

// Interpreter allocator (currently unused but kept for future use)
#[allow(dead_code)]
static INTERP_ALLOC: OnceLock<crate::memory::dynamic_allocator::DynamicAllocator> = OnceLock::new();

#[allow(dead_code)]
fn interp_allocator() -> &'static crate::memory::dynamic_allocator::DynamicAllocator {
    INTERP_ALLOC.get_or_init(|| {
        crate::memory::dynamic_allocator::DynamicAllocator::with_defaults()
            .expect("Failed to initialize interpreter allocator")
    })
}

// Global ARC manager for reference counting
static ARC_MANAGER: OnceLock<Mutex<ArcManager>> = OnceLock::new();

/// Get the global ARC manager instance.
pub(crate) fn arc_manager() -> &'static Mutex<ArcManager> {
    ARC_MANAGER.get_or_init(|| {
        let policy = crate::memory::memory_policy();
        Mutex::new(ArcManager::new(policy.thread_mode))
    })
}

// Heap allocations tracking (legacy, superseded by crate::backends::unsafe_heap)
#[allow(dead_code)]
static HEAP_ALLOCS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();

/// Get heap allocations tracker.
#[allow(dead_code)]
pub(crate) fn heap_allocs() -> &'static Mutex<HashMap<u64, usize>> {
    HEAP_ALLOCS.get_or_init(|| Mutex::new(HashMap::default()))
}

// Value conversion helpers for FFI bridge
fn arc_value_from_i64(raw: i64) -> Value {
    // Treat the raw 64-bit payload as an unsigned handle
    Value::U64(raw as u64)
}

fn arc_value_to_i64(v: Value) -> i64 {
    match v {
        Value::U64(n) => n as i64,
        Value::I64(n) => n,
        Value::Number(n) => n as i64,
        Value::Bool(b) => i64::from(b),
        _ => {
            eprintln!("adesh_rt_arc_get: unsupported value kind; returning 0");
            0
        }
    }
}

#[allow(dead_code)]
fn runtime_value_to_ast_value(rv: &crate::backends::common::builtins::RuntimeValue) -> Value {
    match rv {
        crate::backends::common::builtins::RuntimeValue::Float(n) => Value::Number(*n),
        crate::backends::common::builtins::RuntimeValue::Int(n) => Value::I64(*n),
        crate::backends::common::builtins::RuntimeValue::U64(n) => Value::U64(*n),
        crate::backends::common::builtins::RuntimeValue::Bool(b) => Value::Bool(*b),
        crate::backends::common::builtins::RuntimeValue::Char(c) => Value::Char(*c),
        crate::backends::common::builtins::RuntimeValue::String(s) => Value::Str(s.clone()),
        crate::backends::common::builtins::RuntimeValue::Object(m) => {
            let mut map = HashMap::default();
            for (k, v) in m.iter() {
                map.insert(k.clone(), runtime_value_to_ast_value(v));
            }
            Value::Object(std::sync::Arc::new(map))
        }
        crate::backends::common::builtins::RuntimeValue::Array(a) => {
            let mut arr = Vec::new();
            for v in a {
                arr.push(runtime_value_to_ast_value(v));
            }
            Value::Array(arr)
        }
        _ => Value::Null,
    }
}

// FFI exports for AOT runtime

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_new(value: i64) -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    let val = if let Some(rv) = crate::backends::jit::native::runtime_bridge::get_runtime_value(value as u64) {
        runtime_value_to_ast_value(&rv)
    } else if let Some(rv) = crate::backends::aot::runtime_bridge::aot_get_value(value as u64) {
        runtime_value_to_ast_value(&rv)
    } else {
        arc_value_from_i64(value)
    };
    #[cfg(target_arch = "wasm32")]
    let val = arc_value_from_i64(value);
    match arc_manager().lock() {
        Ok(mut mgr) => {
            mgr.allocate_arc(val)
        }
        Err(e) => {
            eprintln!("adesh_rt_arc_new: lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_clone(handle: u64) -> u64 {
    match arc_manager().lock() {
        Ok(guard) => match guard.clone_arc(handle) {
            Ok(()) => handle,
            Err(e) => {
                eprintln!("adesh_rt_arc_clone: {}", e);
                0
            }
        },
        Err(e) => {
            eprintln!("adesh_rt_arc_clone: lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_drop(handle: u64) -> u64 {
    match arc_manager().lock() {
        Ok(mut guard) => {
            if let Err(e) = guard.drop_arc(handle) {
                eprintln!("adesh_rt_arc_drop: {}", e);
            }
            0
        }
        Err(e) => {
            eprintln!("adesh_rt_arc_drop: lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_weak_new(handle: u64) -> u64 {
    match arc_manager().lock() {
        Ok(guard) => match guard.create_weak(handle) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("adesh_rt_weak_new: {}", e);
                0
            }
        },
        Err(e) => {
            eprintln!("adesh_rt_weak_new: lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_weak_drop(handle: u64) -> u64 {
    match arc_manager().lock() {
        Ok(mut guard) => {
            if let Err(e) = guard.drop_weak(handle) {
                eprintln!("adesh_rt_weak_drop: {}", e);
            }
            0
        }
        Err(e) => {
            eprintln!("adesh_rt_weak_drop: lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_get(handle: u64) -> i64 {
    match arc_manager().lock() {
        Ok(guard) => match guard.get_value(handle) {
            Ok(v) => arc_value_to_i64(v),
            Err(e) => {
                eprintln!("adesh_rt_arc_get: {}", e);
                0
            }
        },
        Err(e) => {
            eprintln!("adesh_rt_arc_get: lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_set(handle: u64, value: i64) -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    let val = if let Some(rv) = crate::backends::jit::native::runtime_bridge::get_runtime_value(value as u64) {
        runtime_value_to_ast_value(&rv)
    } else if let Some(rv) = crate::backends::aot::runtime_bridge::aot_get_value(value as u64) {
        runtime_value_to_ast_value(&rv)
    } else {
        arc_value_from_i64(value)
    };
    #[cfg(target_arch = "wasm32")]
    let val = arc_value_from_i64(value);
    match arc_manager().lock() {
        Ok(mut guard) => match guard.set_value(handle, val) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("adesh_rt_arc_set: {}", e);
                0
            }
        },
        Err(e) => {
            eprintln!("adesh_rt_arc_set: lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_strong_count(handle: u64) -> i64 {
    match arc_manager().lock() {
        Ok(guard) => match guard.strong_count(handle) {
            Ok(n) => n as i64,
            Err(e) => {
                eprintln!("adesh_rt_arc_strong_count error: {}", e);
                0
            }
        },
        Err(e) => {
            eprintln!("adesh_rt_arc_strong_count lock failed: {}", e);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_weak_count(handle: u64) -> i64 {
    match arc_manager().lock() {
        Ok(guard) => match guard.weak_count(handle) {
            Ok(n) => n as i64,
            Err(e) => {
                eprintln!("adesh_rt_arc_weak_count: {}", e);
                0
            }
        },
        Err(e) => {
            eprintln!("adesh_rt_arc_weak_count: lock failed: {}", e);
            0
        }
    }
}

/// Runtime heap allocation guard
/// Returns 1 if heap allocation is allowed, 0 otherwise
#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_assert_heap_allowed() -> i32 {
    // For now, always allow heap allocation
    // This could be extended to enforce compile-time safety policies
    1
}

// adesh_init_args is provided by the C runtime (src/runtime/c_runtime/adesh_runtime.c)
// which stores argc/argv for runtime access. The C version is compiled via build.rs
// and linked into the static library for AOT binaries.
