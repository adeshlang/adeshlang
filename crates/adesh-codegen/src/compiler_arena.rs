//! Phase 10 — Compiler Memory Arena & Resource Tracking.
//!
//! Provides:
//! - Arena allocation for compiler IR structures avoiding excessive deep cloning.
//! - Comprehensive memory tracking across AST, HIR, MIR, MachineIR, and Linker phases.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Global or scoped compiler memory tracker.
#[derive(Debug, Default)]
pub struct MemoryTracker {
    ast_bytes: AtomicUsize,
    hir_bytes: AtomicUsize,
    mir_bytes: AtomicUsize,
    machine_ir_bytes: AtomicUsize,
    linker_bytes: AtomicUsize,
    peak_bytes: AtomicUsize,
}

impl MemoryTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_ast(&self, bytes: usize) {
        self.ast_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.update_peak();
    }

    pub fn record_hir(&self, bytes: usize) {
        self.hir_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.update_peak();
    }

    pub fn record_mir(&self, bytes: usize) {
        self.mir_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.update_peak();
    }

    pub fn record_machine_ir(&self, bytes: usize) {
        self.machine_ir_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.update_peak();
    }

    pub fn record_linker(&self, bytes: usize) {
        self.linker_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.update_peak();
    }

    fn update_peak(&self) {
        let total = self.total_allocated();
        let mut current_peak = self.peak_bytes.load(Ordering::Relaxed);
        while total > current_peak {
            match self.peak_bytes.compare_exchange_weak(
                current_peak,
                total,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_peak = actual,
            }
        }
    }

    pub fn total_allocated(&self) -> usize {
        self.ast_bytes.load(Ordering::Relaxed)
            + self.hir_bytes.load(Ordering::Relaxed)
            + self.mir_bytes.load(Ordering::Relaxed)
            + self.machine_ir_bytes.load(Ordering::Relaxed)
            + self.linker_bytes.load(Ordering::Relaxed)
    }

    pub fn peak_allocated(&self) -> usize {
        self.peak_bytes.load(Ordering::Relaxed)
    }
}

/// Node handle in a typed compiler arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaId<T>(usize, std::marker::PhantomData<T>);

impl<T> ArenaId<T> {
    pub const fn new(index: usize) -> Self {
        Self(index, std::marker::PhantomData)
    }

    pub fn index(&self) -> usize {
        self.0
    }
}

/// Generic arena allocator for compiler IR and symbol tables.
pub struct TypedArena<T> {
    items: Vec<T>,
}

impl<T> TypedArena<T> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
        }
    }

    pub fn alloc(&mut self, item: T) -> ArenaId<T> {
        let id = ArenaId::new(self.items.len());
        self.items.push(item);
        id
    }

    pub fn get(&self, id: ArenaId<T>) -> Option<&T> {
        self.items.get(id.index())
    }

    pub fn get_mut(&mut self, id: ArenaId<T>) -> Option<&mut T> {
        self.items.get_mut(id.index())
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl<T> Default for TypedArena<T> {
    fn default() -> Self {
        Self::new()
    }
}
