//! Interpreter-facing `thread` / `std:thread` module.

use super::atomic_api::atomic_namespace;
use super::channel_api::channel_type;
use super::helpers::*;
use super::pool_api::{
    builtin_parallel_each, builtin_parallel_for, builtin_parallel_map, builtin_parallel_reduce,
    concurrent_queue_type, thread_pool_type,
};
use super::sync_api::*;
use super::thread_api::*;
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::sync::OnceLock;

struct CachedModule(Value);
unsafe impl Send for CachedModule {}
unsafe impl Sync for CachedModule {}

fn cached_thread_module() -> Value {
    static MOD: OnceLock<CachedModule> = OnceLock::new();
    MOD.get_or_init(|| CachedModule(build_thread_module_object_inner()))
        .0
        .clone()
}

pub fn build_thread_module_object() -> Value {
    cached_thread_module()
}

fn build_thread_module_object_inner() -> Value {
    let mut m = HashMap::default();
    insert_fn(&mut m, "spawn", builtin_spawn);
    insert_fn(&mut m, "spawn_named", builtin_spawn_named);
    insert_fn(&mut m, "current", builtin_current);
    insert_fn(&mut m, "id", builtin_id);
    insert_fn(&mut m, "name", builtin_name);
    insert_fn(&mut m, "sleep", builtin_sleep);
    insert_fn(&mut m, "sleep_until", builtin_sleep_until);
    insert_fn(&mut m, "yield", builtin_yield);
    insert_fn(&mut m, "park", builtin_park);
    insert_fn(&mut m, "unpark", builtin_unpark);
    insert_fn(&mut m, "hardware_concurrency", builtin_hardware_concurrency);
    insert_fn(&mut m, "cpu_count", builtin_hardware_concurrency);
    insert_fn(&mut m, "builder", builtin_builder);
    insert_fn(&mut m, "scope", builtin_scope);
    insert_fn(&mut m, "set_affinity", builtin_set_affinity);
    insert_fn(&mut m, "get_affinity", builtin_get_affinity);
    insert_fn(&mut m, "set_priority", builtin_set_priority);
    insert_fn(&mut m, "get_priority", builtin_get_priority);
    insert_fn(&mut m, "list", builtin_list);
    insert_fn(&mut m, "watchdog", builtin_watchdog);
    insert_fn(&mut m, "disarm_watchdog", builtin_disarm_watchdog);
    insert_fn(&mut m, "parallel_map", builtin_parallel_map);
    insert_fn(&mut m, "parallel_for", builtin_parallel_for);
    insert_fn(&mut m, "parallel_reduce", builtin_parallel_reduce);
    insert_fn(&mut m, "parallel_each", builtin_parallel_each);

    m.insert("Mutex".into(), mutex_type());
    m.insert("RwLock".into(), rwlock_type());
    m.insert("Condvar".into(), condvar_type());
    m.insert("Event".into(), event_type());
    m.insert("Semaphore".into(), semaphore_type());
    m.insert("Barrier".into(), barrier_type());
    m.insert("CountDownLatch".into(), latch_type());
    m.insert("Latch".into(), latch_type());
    m.insert("Once".into(), once_type());
    m.insert("OnceCell".into(), once_cell_type());
    m.insert("Lazy".into(), lazy_type());
    m.insert("WaitGroup".into(), waitgroup_type());
    m.insert("ThreadLocal".into(), tls_object());
    m.insert("CancellationSource".into(), cancellation_source_type());
    m.insert("ThreadBuilder".into(), builder_object());
    m.insert("Atomic".into(), atomic_namespace());
    m.insert("channel".into(), channel_type());
    m.insert("Channel".into(), channel_type());
    m.insert("ThreadPool".into(), thread_pool_type());
    m.insert("ConcurrentQueue".into(), concurrent_queue_type());

    with_kind(m, "thread")
}

pub fn build_concurrency_module_object() -> Value {
    build_thread_module_object()
}

pub fn register_thread(registry: &mut BuiltinRegistry) {
    registry.register(
        "thread",
        "concurrency",
        "AdeshLang threading namespace — thread.spawn, Mutex, channels, pools",
        |_env: &mut dyn BuiltinEnv, _args| Ok(build_thread_module_object()),
    );
    registry.register(
        "Thread",
        "concurrency",
        "Alias of thread",
        |_env: &mut dyn BuiltinEnv, _args| Ok(build_thread_module_object()),
    );
    registry.register(
        "cpu_count",
        "concurrency",
        "Logical CPU count (hardware_concurrency)",
        builtin_hardware_concurrency,
    );
    for (name, build) in [
        ("Mutex", mutex_type as fn() -> Value),
        ("RwLock", rwlock_type),
        ("Condvar", condvar_type),
        ("Semaphore", semaphore_type),
        ("Barrier", barrier_type),
        ("CountDownLatch", latch_type),
        ("WaitGroup", waitgroup_type),
        ("ThreadPool", thread_pool_type),
        ("Channel", channel_type),
    ] {
        registry.register(
            name,
            "concurrency",
            name,
            move |_env: &mut dyn BuiltinEnv, _args| Ok(build()),
        );
    }
}

/// After `import thread` / `import Thread` / `import std:thread`.
pub fn thread_module_value() -> Value {
    build_thread_module_object()
}
