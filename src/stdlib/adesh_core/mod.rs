//! Adesh Core Library
//!
//! This is Layer 1 of the Adesh standard library.
//!
//! ## Principles
//! - No allocator required
//! - No OS dependencies
//! - No heap allocations
//! - Purely compile-time focused
//! - MIR-aware
//! - Usable on GPU
//! - Usable in no_std environments
//! - Embedded compatible
//!
//! ## Contents
//! - Primitive traits and operations
//! - Option and Result types (re-exported from std for now)
//! - Iterator traits
//! - Slice operations
//! - Memory layout traits
//! - Intrinsics
//! - Ownership helpers

pub mod intrinsics;
pub mod iter;
pub mod layout;
pub mod ownership;
pub mod slice;
pub mod traits;

// Re-export std types for now (will be replaced with custom implementations)
pub use std::option::Option;
pub use std::result::Result;

// Re-export commonly used items
pub use iter::Iterator;
pub use layout::{Align, Layout};
pub use ownership::{Borrow, BorrowMut, Own};
pub use slice::Slice;
pub use traits::*;
