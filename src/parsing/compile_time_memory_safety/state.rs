//! Internal data structures for compile-time memory safety tracking
//!
//! This module contains the internal state structures used to track
//! ownership, borrows, lifetimes, and thread contexts during analysis.

use crate::parsing::compile_time_memory_safety::error::SourceLocation;
use crate::utils::collections::FastSet;

/// Ownership tracking node
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct OwnershipNode {
    pub(super) owner: String,
    pub(super) owned_values: FastSet<String>,
    pub(super) state: OwnershipState,
    pub(super) is_copy_type: bool, // Track if this variable holds a Copy type
    pub(super) location: SourceLocation,
}

/// Ownership state of a variable
#[derive(Debug, Clone, PartialEq)]
pub(super) enum OwnershipState {
    Owned,
    Moved,
    Freed,
    #[allow(dead_code)]
    Borrowed,
}

/// Borrow tracking information
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct BorrowInfo {
    pub(super) borrower: String,
    pub(super) is_mutable: bool,
    pub(super) scope_id: usize,
    pub(super) location: SourceLocation,
}

/// Lifetime scope tracking
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct LifetimeScope {
    pub(super) id: usize,
    pub(super) parent: Option<usize>,
    pub(super) variables: FastSet<String>,
    pub(super) start_location: SourceLocation,
    pub(super) end_location: Option<SourceLocation>,
}

/// Thread context for data race detection
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct ThreadContext {
    pub(super) thread_id: String,
    pub(super) accessed_variables: FastSet<String>,
    pub(super) is_concurrent: bool,
}
