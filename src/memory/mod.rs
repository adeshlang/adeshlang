//! Memory Module
//!
//! Provides memory management infrastructure for AdeshLang:
//! - Dynamic allocator with static, dynamic, and hybrid modes
//! - Slab allocation for small objects
//! - Arena pools for short-lived allocations
//! - Memory profiling and reporting
//! - Adaptive memory management with automatic growth
//! - ARC (Atomic Reference Counting) for shared ownership
//! - Concurrency-safe primitives (shared<T>, atomic<T>, mutex<T>, rwlock<T>)
//! - Arena and region allocators for scope-based allocation

pub mod adaptive;
pub mod allocators;
pub mod arc;
pub mod concurrency;
pub mod cycle;
pub mod dynamic_allocator;
pub mod policies;
pub mod raii;

pub use adaptive::*;
pub use allocators::*;
pub use arc::*;
pub use concurrency::*;
pub use cycle::*;
pub use dynamic_allocator::*;
pub use policies::*;
pub use raii::*;

// Backward compatibility re-exports for old module names
pub use arc as arc_manager;
pub use cycle as cycle_detect;
pub use policies as policy;
