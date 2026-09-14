//! Standard Library
//!
//! Registers built-in functions and namespaces available to AdeshLang programs:
//! - `core`: base types, operators, printing
//! - `collections`: array helpers (`map`, `filter`, `reduce`)
//! - `system`: args, environment, time/date, process info
//! - `io`: file utilities
//! - `json`: parse/stringify
//! - `async_runtime`: Promise, timers, sleep/delay
//! - `math`: numeric utilities and RNG support
//! - `concurrency`: parallel operations and multi-threading
//! - `decorators`: built-in decorators (@memoize, @trace, @deprecated, etc.)
//!
//! Use `create_stdlib()` to assemble a `BuiltinRegistry` for the runtime.
pub mod async_runtime;
pub mod collections;
pub mod concurrency;
pub mod core;
pub mod decorators;
pub mod fs;
pub mod io;
pub mod json;
pub mod math;
pub mod path;
pub mod registry;
pub mod system;

#[cfg(not(target_arch = "wasm32"))]
pub mod atp;
#[cfg(not(target_arch = "wasm32"))]
pub mod compression;
pub mod crypto;
#[cfg(not(target_arch = "wasm32"))]
pub mod dns;
pub mod encoding;
#[cfg(not(target_arch = "wasm32"))]
pub mod http;
#[cfg(not(target_arch = "wasm32"))]
pub mod net;
pub mod random;
pub mod simd;
#[cfg(not(target_arch = "wasm32"))]
pub mod tls;
pub mod url;

use registry::BuiltinRegistry;

use once_cell::sync::Lazy;

static INSTANCE: Lazy<BuiltinRegistry> = Lazy::new(|| {
    let mut registry = BuiltinRegistry::new();
    core::register_all(&mut registry);
    collections::register_all(&mut registry);
    system::register_all(&mut registry);
    io::register_all(&mut registry);
    fs::register_all(&mut registry);
    path::register_all(&mut registry);
    async_runtime::register_all(&mut registry);
    json::register_all(&mut registry);
    math::register_all(&mut registry);
    simd::register_all(&mut registry);
    concurrency::register_all(&mut registry);
    decorators::register_all(&mut registry);
    random::register_all(&mut registry);
    crypto::register_all(&mut registry);
    encoding::register_all(&mut registry);
    #[cfg(not(target_arch = "wasm32"))]
    compression::register_all(&mut registry);
    url::register_all(&mut registry);
    #[cfg(not(target_arch = "wasm32"))]
    net::register_all(&mut registry);
    #[cfg(not(target_arch = "wasm32"))]
    dns::register_all(&mut registry);
    #[cfg(not(target_arch = "wasm32"))]
    tls::register_all(&mut registry);
    #[cfg(not(target_arch = "wasm32"))]
    http::register_all(&mut registry);
    #[cfg(not(target_arch = "wasm32"))]
    atp::register_all(&mut registry);
    registry
});

pub fn create_stdlib() -> BuiltinRegistry {
    INSTANCE.clone()
}
