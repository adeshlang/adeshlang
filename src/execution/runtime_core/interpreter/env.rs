//! Environment (scope) management for the interpreter.
//!
//! This module defines the `Env` structure used for variable scoping
//! with support for exports, constants, ownership tracking, and defer statements.

use crate::parsing::ast::{Stmt, Value};
use crate::utils::memory::OwnershipTracker;
use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

/// Environment for variable scopes - optimized with FxHashMap.
///
/// Most scopes only use values + enclosing; other fields use lazy initialization.
/// Supports:
/// - Variable storage with fast lookup
/// - Module exports
/// - Constant tracking
/// - Type annotations
/// - Ownership tracking
/// - Defer statements (LIFO execution)
pub struct Env {
    pub values: HashMap<String, Value>,
    pub enclosing: Option<usize>,
    pub exports: HashMap<String, Value>,
    pub consts: HashMap<String, bool>,
    pub type_ann: HashMap<String, Option<String>>,
    pub ownership: HashMap<String, Rc<OwnershipTracker>>,
    pub defers: Vec<Box<Stmt>>, // Defer stack for LIFO execution
    pub is_captured: bool,
}

impl Env {
    /// Create a new environment with optional enclosing scope.
    #[inline]
    pub fn new(enclosing: Option<usize>) -> Self {
        Self {
            values: HashMap::default(),
            enclosing,
            exports: HashMap::default(),
            consts: HashMap::default(),
            type_ann: HashMap::default(),
            ownership: HashMap::default(),
            defers: Vec::new(),
            is_captured: false,
        }
    }

    /// Create a new environment with pre-allocated capacity for values.
    #[inline]
    pub fn with_capacity(enclosing: Option<usize>, cap: usize) -> Self {
        Self {
            values: HashMap::with_capacity_and_hasher(cap, Default::default()),
            enclosing,
            exports: HashMap::default(),
            consts: HashMap::default(),
            type_ann: HashMap::default(),
            ownership: HashMap::default(),
            defers: Vec::new(),
            is_captured: false,
        }
    }

    /// Reset the environment for reuse (scope recycling).
    ///
    /// This allows environments to be pooled and reused for performance,
    /// while reclaiming pathological high-watermark memory allocations.
    #[inline]
    pub fn reset(&mut self, enclosing: Option<usize>) {
        if self.values.capacity() > 1024 {
            self.values.shrink_to(64);
        } else {
            self.values.clear();
        }
        self.enclosing = enclosing;
        if self.exports.capacity() > 256 {
            self.exports.shrink_to_fit();
        } else {
            self.exports.clear();
        }
        if self.consts.capacity() > 256 {
            self.consts.shrink_to_fit();
        } else {
            self.consts.clear();
        }
        if self.type_ann.capacity() > 256 {
            self.type_ann.shrink_to_fit();
        } else {
            self.type_ann.clear();
        }
        if self.ownership.capacity() > 256 {
            self.ownership.shrink_to_fit();
        } else {
            self.ownership.clear();
        }
        if self.defers.capacity() > 64 {
            self.defers.shrink_to_fit();
        } else {
            self.defers.clear();
        }
        self.is_captured = false;
    }
}
