//! Concurrency Stdlib
//!
//! Native OS threads, synchronization, atomics, channels, and thread pools.
//! Exposed as `thread` / `std:thread` / `std:concurrency`.

pub mod api;
pub mod atomic_api;
pub mod channel_api;
pub mod helpers;
pub mod parallel_ops;
pub mod pool_api;
pub mod sync_api;
pub mod thread_api;

pub use api::{
    build_concurrency_module_object, build_thread_module_object, register_thread,
    thread_module_value,
};
pub use parallel_ops::build_parallel_module_object;

use crate::stdlib::registry::BuiltinRegistry;

pub fn register_all(registry: &mut BuiltinRegistry) {
    parallel_ops::register(registry);
    register_thread(registry);
}
