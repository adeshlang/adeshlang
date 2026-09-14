//! Production-Grade Random Standard Library
//!
//! Provides pseudo-random number generation, sampling, distributions, Fisher-Yates shuffling, UUIDs,
//! seeded PRNG streams, and OS cryptographic entropy integration.

pub mod api;
pub mod bounded;
pub mod collections;
pub mod distributions;
pub mod prng;
pub mod rng_object;
pub mod strings;

use crate::stdlib::registry::BuiltinRegistry;

pub fn register_all(registry: &mut BuiltinRegistry) {
    api::register(registry);
}
