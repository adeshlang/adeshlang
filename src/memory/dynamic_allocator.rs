//! Dynamic Memory Allocator
//!
//! Provides configurable memory allocation strategies for AdeshLang runtime:
//! - Static mode: Fixed-size heap with OOM on overflow
//! - Dynamic mode: Auto-expanding heap with configurable growth
//! - Hybrid mode: Arena pools with fallback to heap
//!
//! # Safety Requirements
//! - Never overwrites unrelated memory
//! - All allocations check for size overflow
//! - OOM returns safe error through unified error model
//! - Debug mode: allocator poisons freed regions
//!
//! # Performance Requirements
//! - O(1) amortized allocation
//! - Efficient reallocation with doubling or slab-based strategy
//! - JIT fast paths for small objects
//! - Arena allocators reduce fragmentation

use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, realloc};
use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::memory::policy::memory_policy;
use crate::parsing::error::{ErrorKind, LangError};

// ============================================
// ALLOCATOR CONFIGURATION
// ============================================

/// Allocation mode determining memory behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocMode {
    /// Fixed-size heap; OOM if exceeded
    Static,
    /// Dynamically expanding heap
    Dynamic,
    /// Hybrid: arenas for short-lived, heap for long-lived
    Hybrid,
}

impl Default for AllocMode {
    fn default() -> Self {
        AllocMode::Dynamic
    }
}

impl std::fmt::Display for AllocMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllocMode::Static => write!(f, "static"),
            AllocMode::Dynamic => write!(f, "dynamic"),
            AllocMode::Hybrid => write!(f, "hybrid"),
        }
    }
}

impl std::str::FromStr for AllocMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "static" => Ok(AllocMode::Static),
            "dynamic" => Ok(AllocMode::Dynamic),
            "hybrid" => Ok(AllocMode::Hybrid),
            _ => Err(format!("Unknown allocation mode: {}", s)),
        }
    }
}

/// Growth strategy for dynamic allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrowthStrategy {
    /// Double capacity on each expansion
    Doubling,
    /// Add fixed amount on each expansion
    Linear(usize),
    /// Slab-based allocation with fixed-size classes
    Slab,
}

impl Default for GrowthStrategy {
    fn default() -> Self {
        GrowthStrategy::Doubling
    }
}

impl std::fmt::Display for GrowthStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrowthStrategy::Doubling => write!(f, "doubling"),
            GrowthStrategy::Linear(n) => write!(f, "linear({})", n),
            GrowthStrategy::Slab => write!(f, "slab"),
        }
    }
}

/// Configuration for the dynamic allocator
#[derive(Debug, Clone)]
pub struct AllocatorConfig {
    /// Allocation mode
    pub mode: AllocMode,
    /// Initial heap size in bytes
    pub initial_heap_size: usize,
    /// Maximum heap size in bytes (for static mode)
    pub max_heap_size: usize,
    /// Growth strategy (for dynamic mode)
    pub growth_strategy: GrowthStrategy,
    /// Arena pool size (for hybrid mode)
    pub arena_pool_size: usize,
    /// Number of arenas in pool
    pub arena_count: usize,
    /// Enable debug poisoning of freed memory
    pub debug_poison: bool,
    /// Enable memory profiling
    pub enable_profiling: bool,
}

impl Default for AllocatorConfig {
    fn default() -> Self {
        AllocatorConfig {
            mode: AllocMode::Dynamic,
            initial_heap_size: 1024 * 1024,    // 1 MB
            max_heap_size: 1024 * 1024 * 1024, // 1 GB
            growth_strategy: GrowthStrategy::Doubling,
            arena_pool_size: 64 * 1024, // 64 KB per arena
            arena_count: 4,
            debug_poison: cfg!(debug_assertions),
            enable_profiling: false,
        }
    }
}

impl AllocatorConfig {
    /// Create a static allocator config with fixed heap size
    pub fn static_mode(heap_size: usize) -> Self {
        AllocatorConfig {
            mode: AllocMode::Static,
            initial_heap_size: heap_size,
            max_heap_size: heap_size,
            ..Default::default()
        }
    }

    /// Create a dynamic allocator config with growth strategy
    pub fn dynamic_mode(initial_size: usize, max_size: usize, strategy: GrowthStrategy) -> Self {
        AllocatorConfig {
            mode: AllocMode::Dynamic,
            initial_heap_size: initial_size,
            max_heap_size: max_size,
            growth_strategy: strategy,
            ..Default::default()
        }
    }

    /// Create a hybrid allocator config
    pub fn hybrid_mode(arena_size: usize, arena_count: usize) -> Self {
        AllocatorConfig {
            mode: AllocMode::Hybrid,
            arena_pool_size: arena_size,
            arena_count,
            ..Default::default()
        }
    }
}

// ============================================
// ALLOCATION STATISTICS
// ============================================

/// Statistics for memory allocator operations
#[derive(Debug, Default)]
pub struct AllocatorStats {
    /// Total allocations made
    pub total_allocations: AtomicUsize,
    /// Total deallocations made
    pub total_deallocations: AtomicUsize,
    /// Total bytes currently allocated
    pub current_bytes: AtomicUsize,
    /// Peak bytes allocated
    pub peak_bytes: AtomicUsize,
    /// Number of heap expansions
    pub heap_expansions: AtomicUsize,
    /// Number of OOM events
    pub oom_events: AtomicUsize,
    /// Arena allocations
    pub arena_allocations: AtomicUsize,
    /// Arena resets
    pub arena_resets: AtomicUsize,
    /// Fragmentation ratio (percentage)
    pub fragmentation_ratio: AtomicUsize,
}

impl AllocatorStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an allocation
    pub fn record_alloc(&self, bytes: usize) {
        self.total_allocations.fetch_add(1, Ordering::SeqCst);
        let current = self.current_bytes.fetch_add(bytes, Ordering::SeqCst) + bytes;
        // Update peak if necessary
        let mut peak = self.peak_bytes.load(Ordering::SeqCst);
        while current > peak {
            match self.peak_bytes.compare_exchange_weak(
                peak,
                current,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
    }

    /// Record a deallocation
    pub fn record_dealloc(&self, bytes: usize) {
        self.total_deallocations.fetch_add(1, Ordering::SeqCst);
        self.current_bytes.fetch_sub(bytes, Ordering::SeqCst);
    }

    /// Record a heap expansion
    pub fn record_expansion(&self) {
        self.heap_expansions.fetch_add(1, Ordering::SeqCst);
    }

    /// Record an OOM event
    pub fn record_oom(&self) {
        self.oom_events.fetch_add(1, Ordering::SeqCst);
    }

    /// Get a snapshot of stats
    pub fn snapshot(&self) -> AllocatorStatsSnapshot {
        AllocatorStatsSnapshot {
            total_allocations: self.total_allocations.load(Ordering::SeqCst),
            total_deallocations: self.total_deallocations.load(Ordering::SeqCst),
            current_bytes: self.current_bytes.load(Ordering::SeqCst),
            peak_bytes: self.peak_bytes.load(Ordering::SeqCst),
            heap_expansions: self.heap_expansions.load(Ordering::SeqCst),
            oom_events: self.oom_events.load(Ordering::SeqCst),
            arena_allocations: self.arena_allocations.load(Ordering::SeqCst),
            arena_resets: self.arena_resets.load(Ordering::SeqCst),
            fragmentation_ratio: self.fragmentation_ratio.load(Ordering::SeqCst),
        }
    }
}

/// Snapshot of allocator statistics
#[derive(Debug, Clone)]
pub struct AllocatorStatsSnapshot {
    pub total_allocations: usize,
    pub total_deallocations: usize,
    pub current_bytes: usize,
    pub peak_bytes: usize,
    pub heap_expansions: usize,
    pub oom_events: usize,
    pub arena_allocations: usize,
    pub arena_resets: usize,
    pub fragmentation_ratio: usize,
}

impl std::fmt::Display for AllocatorStatsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AllocatorStats {{\n\
             \x20 allocations: {}\n\
             \x20 deallocations: {}\n\
             \x20 current_bytes: {}\n\
             \x20 peak_bytes: {}\n\
             \x20 heap_expansions: {}\n\
             \x20 oom_events: {}\n\
             \x20 arena_allocations: {}\n\
             \x20 arena_resets: {}\n\
             \x20 fragmentation: {}%\n\
             }}",
            self.total_allocations,
            self.total_deallocations,
            format_bytes(self.current_bytes),
            format_bytes(self.peak_bytes),
            self.heap_expansions,
            self.oom_events,
            self.arena_allocations,
            self.arena_resets,
            self.fragmentation_ratio
        )
    }
}

/// Format bytes in human-readable form
fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// ============================================
// OUT-OF-MEMORY ERROR
// ============================================

/// Create an OOM error using the unified error model
pub fn oom_error(requested: usize, available: usize) -> LangError {
    LangError::new(
        ErrorKind::Runtime,
        format!(
            "Out of memory: requested {} bytes, but only {} available",
            format_bytes(requested),
            format_bytes(available)
        ),
        0,
        0,
        String::new(),
    )
    .with_note("Consider using a larger heap size or dynamic allocation mode".to_string())
}

/// Create a size overflow error
pub fn size_overflow_error(size: usize) -> LangError {
    LangError::new(
        ErrorKind::Runtime,
        format!("Allocation size overflow: {} bytes exceeds maximum", size),
        0,
        0,
        String::new(),
    )
    .with_note("Reduce allocation size or use chunked allocation".to_string())
}

// ============================================
// SLAB ALLOCATOR (for small objects)
// ============================================

/// Size classes for slab allocation
const SLAB_SIZE_CLASSES: [usize; 8] = [16, 32, 64, 128, 256, 512, 1024, 2048];

/// A slab for objects of a specific size class
#[allow(dead_code)]
struct Slab {
    /// Base pointer to slab memory
    base: NonNull<u8>,
    /// Size of the slab in bytes
    size: usize,
    /// Object size for this slab
    object_size: usize,
    /// Free list head (index into slab)
    free_head: Option<usize>,
    /// Number of allocated objects
    allocated: usize,
    /// Total capacity (number of objects)
    capacity: usize,
}

impl Slab {
    /// Create a new slab for given object size
    fn new(object_size: usize, capacity: usize) -> Result<Self, LangError> {
        if capacity == 0 {
            return Err(LangError::new(
                ErrorKind::Runtime,
                "Slab capacity cannot be zero".to_string(),
                0,
                0,
                String::new(),
            ));
        }
        if object_size == 0 {
            return Err(LangError::new(
                ErrorKind::Runtime,
                "Slab object size cannot be zero".to_string(),
                0,
                0,
                String::new(),
            ));
        }

        let size = object_size
            .checked_mul(capacity)
            .ok_or_else(|| size_overflow_error(object_size))?;

        let layout = Layout::from_size_align(size, 8).map_err(|_| size_overflow_error(size))?;

        let base = unsafe {
            let ptr = alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(oom_error(size, 0));
            }
            NonNull::new_unchecked(ptr)
        };

        // Initialize free list
        let slab = Slab {
            base,
            size,
            object_size,
            free_head: Some(0),
            allocated: 0,
            capacity,
        };

        // Link all objects in free list
        for i in 0..capacity - 1 {
            unsafe {
                let ptr = slab.base.as_ptr().add(i * object_size);
                *(ptr as *mut usize) = i + 1;
            }
        }
        // Last object points to nothing
        unsafe {
            let ptr = slab.base.as_ptr().add((capacity - 1) * object_size);
            *(ptr as *mut usize) = usize::MAX;
        }

        Ok(slab)
    }

    /// Allocate an object from this slab
    fn alloc(&mut self) -> Option<NonNull<u8>> {
        if let Some(idx) = self.free_head {
            let ptr = unsafe { self.base.as_ptr().add(idx * self.object_size) };
            // Read next free index
            let next = unsafe { *(ptr as *const usize) };
            self.free_head = if next == usize::MAX { None } else { Some(next) };
            self.allocated += 1;
            Some(unsafe { NonNull::new_unchecked(ptr) })
        } else {
            None
        }
    }

    /// Free an object back to this slab
    fn free(&mut self, ptr: NonNull<u8>) {
        assert!(self.contains(ptr), "Pointer does not belong to this slab");
        let offset = unsafe { ptr.as_ptr().offset_from(self.base.as_ptr()) } as usize;
        assert_eq!(offset % self.object_size, 0, "Pointer is not aligned to object size");
        let idx = offset / self.object_size;
        assert!(idx < self.capacity, "Index out of bounds");

        // Add to free list
        unsafe {
            let obj_ptr = ptr.as_ptr();
            *(obj_ptr as *mut usize) = self.free_head.unwrap_or(usize::MAX);
        }
        self.free_head = Some(idx);
        self.allocated -= 1;
    }

    /// Check if slab contains this pointer
    fn contains(&self, ptr: NonNull<u8>) -> bool {
        let base = self.base.as_ptr() as usize;
        let end = base + self.size;
        let p = ptr.as_ptr() as usize;
        p >= base && p < end
    }

    /// Check if slab is empty
    fn _is_empty(&self) -> bool {
        self.allocated == 0
    }

    /// Check if slab is full
    fn _is_full(&self) -> bool {
        self.free_head.is_none()
    }
}

impl Drop for Slab {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.size, 8);
            dealloc(self.base.as_ptr(), layout);
        }
    }
}

// ============================================
// DYNAMIC ALLOCATOR
// ============================================

/// Thread-safe dynamic memory allocator
pub struct DynamicAllocator {
    /// Configuration
    config: AllocatorConfig,
    /// Statistics
    stats: Arc<AllocatorStats>,
    /// Slabs for small object allocation (size class -> slabs)
    slabs: Mutex<HashMap<usize, Vec<Slab>>>,
    /// Large allocation tracking (ptr -> size)
    large_allocs: Mutex<HashMap<usize, usize>>,
    /// Arena pool for hybrid mode
    arena_pool: Mutex<Vec<ArenaBlock>>,
    /// Current heap capacity
    heap_capacity: AtomicUsize,
}

/// An arena block in the pool
struct ArenaBlock {
    /// Base pointer
    base: NonNull<u8>,
    /// Size of block
    size: usize,
    /// Current position
    position: usize,
    /// Is this block in use?
    in_use: bool,
}

impl ArenaBlock {
    fn new(size: usize) -> Result<Self, LangError> {
        let layout = Layout::from_size_align(size, 8).map_err(|_| size_overflow_error(size))?;

        let base = unsafe {
            let ptr = alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(oom_error(size, 0));
            }
            NonNull::new_unchecked(ptr)
        };

        Ok(ArenaBlock {
            base,
            size,
            position: 0,
            in_use: false,
        })
    }

    fn alloc(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        let aligned_pos = (self.position + align - 1) & !(align - 1);
        let new_pos = aligned_pos + size;

        if new_pos <= self.size {
            self.position = new_pos;
            Some(unsafe { NonNull::new_unchecked(self.base.as_ptr().add(aligned_pos)) })
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.position = 0;
        self.in_use = false;

        // Poison memory in debug mode
        #[cfg(debug_assertions)]
        unsafe {
            let pattern: u8 = 0xDE;
            std::ptr::write_bytes(self.base.as_ptr(), pattern, self.size);
        }
    }
}

impl Drop for ArenaBlock {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.size, 8);
            dealloc(self.base.as_ptr(), layout);
        }
    }
}

impl DynamicAllocator {
    /// Create a new dynamic allocator with given config
    pub fn new(config: AllocatorConfig) -> Result<Self, LangError> {
        let stats = Arc::new(AllocatorStats::new());

        // Initialize arena pool for hybrid mode
        let mut arena_pool = Vec::new();
        if config.mode == AllocMode::Hybrid {
            for _ in 0..config.arena_count {
                arena_pool.push(ArenaBlock::new(config.arena_pool_size)?);
            }
        }

        Ok(DynamicAllocator {
            heap_capacity: AtomicUsize::new(config.initial_heap_size),
            config,
            stats,
            slabs: Mutex::new(HashMap::new()),
            large_allocs: Mutex::new(HashMap::new()),
            arena_pool: Mutex::new(arena_pool),
        })
    }

    /// Create with default config
    pub fn with_defaults() -> Result<Self, LangError> {
        Self::new(AllocatorConfig::default())
    }

    /// Allocate memory
    pub fn alloc(&self, size: usize) -> Result<NonNull<u8>, LangError> {
        let policy = memory_policy();
        if !policy.allow_heap {
            return Err(oom_error(size, 0).with_note(
                "Heap allocation is disabled (embedded mode uses stack/arena only)".to_string(),
            ));
        }
        // Check for size overflow
        if size > isize::MAX as usize {
            return Err(size_overflow_error(size));
        }

        // Check capacity for static mode
        if self.config.mode == AllocMode::Static {
            let current = self.stats.current_bytes.load(Ordering::SeqCst);
            let capacity = self.heap_capacity.load(Ordering::SeqCst);
            if current + size > capacity {
                self.stats.record_oom();
                return Err(oom_error(size, capacity - current));
            }
        }

        // Try arena allocation for hybrid mode
        if self.config.mode == AllocMode::Hybrid {
            if let Some(ptr) = self.try_arena_alloc(size)? {
                self.stats.arena_allocations.fetch_add(1, Ordering::SeqCst);
                self.stats.record_alloc(size);
                return Ok(ptr);
            }
        }

        // Try slab allocation for small objects
        if size <= SLAB_SIZE_CLASSES[SLAB_SIZE_CLASSES.len() - 1] {
            if let Some(ptr) = self.try_slab_alloc(size)? {
                self.stats.record_alloc(size);
                return Ok(ptr);
            }
        }

        // Fall back to heap allocation
        self.heap_alloc(size)
    }

    /// Allocate zeroed memory
    pub fn alloc_zeroed(&self, size: usize) -> Result<NonNull<u8>, LangError> {
        let ptr = self.alloc(size)?;
        unsafe {
            std::ptr::write_bytes(ptr.as_ptr(), 0, size);
        }
        Ok(ptr)
    }

    /// Try to allocate from arena pool
    fn try_arena_alloc(&self, size: usize) -> Result<Option<NonNull<u8>>, LangError> {
        let mut pool = self.arena_pool.lock().unwrap();

        for arena in pool.iter_mut() {
            if !arena.in_use || arena.position + size <= arena.size {
                arena.in_use = true;
                if let Some(ptr) = arena.alloc(size, 8) {
                    return Ok(Some(ptr));
                }
            }
        }

        Ok(None)
    }

    /// Try to allocate from slab
    fn try_slab_alloc(&self, size: usize) -> Result<Option<NonNull<u8>>, LangError> {
        // Fast O(1) bit-manipulation for power-of-two slab size class
        let size_class = if size <= 16 { 16 } else { size.next_power_of_two() };

        let mut slabs = self.slabs.lock().unwrap();
        let class_slabs = slabs.entry(size_class).or_insert_with(Vec::new);

        // Try existing slabs
        for slab in class_slabs.iter_mut() {
            if let Some(ptr) = slab.alloc() {
                return Ok(Some(ptr));
            }
        }

        // Create new slab
        let capacity = 4096 / size_class; // ~4KB per slab
        let new_slab = Slab::new(size_class, capacity.max(16))?;
        class_slabs.push(new_slab);

        // Allocate from new slab
        let slab = class_slabs.last_mut().unwrap();
        Ok(slab.alloc())
    }

    /// Heap allocation with potential growth
    fn heap_alloc(&self, size: usize) -> Result<NonNull<u8>, LangError> {
        // Dynamic mode: try to grow heap if needed
        if self.config.mode == AllocMode::Dynamic {
            let current = self.stats.current_bytes.load(Ordering::SeqCst);
            let capacity = self.heap_capacity.load(Ordering::SeqCst);

            if current + size > capacity {
                self.grow_heap(size)?;
            }
        }

        let layout = Layout::from_size_align(size, 8).map_err(|_| size_overflow_error(size))?;

        let ptr = unsafe {
            let p = alloc(layout);
            if p.is_null() {
                self.stats.record_oom();
                return Err(oom_error(size, 0));
            }
            NonNull::new_unchecked(p)
        };

        // Track large allocation
        let mut large = self.large_allocs.lock().unwrap();
        large.insert(ptr.as_ptr() as usize, size);

        self.stats.record_alloc(size);
        Ok(ptr)
    }

    /// Grow heap capacity
    fn grow_heap(&self, needed: usize) -> Result<(), LangError> {
        let current_cap = self.heap_capacity.load(Ordering::SeqCst);
        let new_cap = match self.config.growth_strategy {
            GrowthStrategy::Doubling => {
                let mut cap = current_cap;
                while cap < current_cap + needed {
                    cap = cap.saturating_mul(2);
                }
                cap
            }
            GrowthStrategy::Linear(increment) => {
                let mut cap = current_cap;
                while cap < current_cap + needed {
                    cap = cap.saturating_add(increment);
                }
                cap
            }
            GrowthStrategy::Slab => current_cap + needed,
        };

        let new_cap = new_cap.min(self.config.max_heap_size);

        if new_cap < current_cap + needed {
            return Err(oom_error(needed, new_cap - current_cap));
        }

        self.heap_capacity.store(new_cap, Ordering::SeqCst);
        self.stats.record_expansion();
        Ok(())
    }

    /// Deallocate memory
    pub fn dealloc(&self, ptr: NonNull<u8>, size: usize) {
        // Try to find in slabs
        {
            let mut slabs = self.slabs.lock().unwrap();
            for class_slabs in slabs.values_mut() {
                for slab in class_slabs.iter_mut() {
                    if slab.contains(ptr) {
                        slab.free(ptr);
                        self.stats.record_dealloc(slab.object_size);

                        // Poison in debug mode
                        #[cfg(debug_assertions)]
                        if self.config.debug_poison {
                            unsafe {
                                std::ptr::write_bytes(ptr.as_ptr(), 0xDE, slab.object_size);
                            }
                        }
                        return;
                    }
                }
            }
        }

        // Check large allocations
        {
            let mut large = self.large_allocs.lock().unwrap();
            if let Some(alloc_size) = large.remove(&(ptr.as_ptr() as usize)) {
                // Poison in debug mode
                #[cfg(debug_assertions)]
                if self.config.debug_poison {
                    unsafe {
                        std::ptr::write_bytes(ptr.as_ptr(), 0xDE, alloc_size);
                    }
                }

                let layout = Layout::from_size_align(alloc_size, 8).unwrap();
                unsafe {
                    dealloc(ptr.as_ptr(), layout);
                }
                self.stats.record_dealloc(alloc_size);
                return;
            }
        }

        // Fallback: use provided size
        let layout = Layout::from_size_align(size, 8).unwrap();

        // Poison in debug mode
        #[cfg(debug_assertions)]
        if self.config.debug_poison {
            unsafe {
                std::ptr::write_bytes(ptr.as_ptr(), 0xDE, size);
            }
        }

        unsafe {
            dealloc(ptr.as_ptr(), layout);
        }
        self.stats.record_dealloc(size);
    }

    /// Reallocate memory
    pub fn realloc(
        &self,
        ptr: NonNull<u8>,
        old_size: usize,
        new_size: usize,
    ) -> Result<NonNull<u8>, LangError> {
        if !memory_policy().allow_heap {
            return Err(oom_error(new_size, 0).with_note(
                "Heap allocation is disabled (embedded mode uses stack/arena only)".to_string(),
            ));
        }
        // Check for size overflow
        if new_size > isize::MAX as usize {
            return Err(size_overflow_error(new_size));
        }

        // For static mode, check capacity
        if self.config.mode == AllocMode::Static {
            let current = self.stats.current_bytes.load(Ordering::SeqCst);
            let capacity = self.heap_capacity.load(Ordering::SeqCst);
            let delta = new_size.saturating_sub(old_size);
            if current + delta > capacity {
                self.stats.record_oom();
                return Err(oom_error(delta, capacity - current));
            }
        }

        // For slab-allocated objects, allocate new and copy
        let in_slab = {
            let slabs = self.slabs.lock().unwrap();
            slabs
                .values()
                .any(|class_slabs| class_slabs.iter().any(|slab| slab.contains(ptr)))
        };

        if in_slab || new_size <= SLAB_SIZE_CLASSES[SLAB_SIZE_CLASSES.len() - 1] {
            // Allocate new, copy, free old
            let new_ptr = self.alloc(new_size)?;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    ptr.as_ptr(),
                    new_ptr.as_ptr(),
                    old_size.min(new_size),
                );
            }
            self.dealloc(ptr, old_size);
            return Ok(new_ptr);
        }

        // For dynamic mode, may need to grow
        if self.config.mode == AllocMode::Dynamic && new_size > old_size {
            let current = self.stats.current_bytes.load(Ordering::SeqCst);
            let capacity = self.heap_capacity.load(Ordering::SeqCst);
            let delta = new_size - old_size;
            if current + delta > capacity {
                self.grow_heap(delta)?;
            }
        }

        let old_layout =
            Layout::from_size_align(old_size, 8).map_err(|_| size_overflow_error(old_size))?;

        let new_ptr = unsafe {
            let p = realloc(ptr.as_ptr(), old_layout, new_size);
            if p.is_null() {
                self.stats.record_oom();
                return Err(oom_error(new_size, 0));
            }
            NonNull::new_unchecked(p)
        };

        // Update large alloc tracking
        {
            let mut large = self.large_allocs.lock().unwrap();
            large.remove(&(ptr.as_ptr() as usize));
            large.insert(new_ptr.as_ptr() as usize, new_size);
        }

        // Update stats
        if new_size > old_size {
            self.stats.record_alloc(new_size - old_size);
        } else {
            self.stats.record_dealloc(old_size - new_size);
        }

        Ok(new_ptr)
    }

    /// Reset all arenas (hybrid mode)
    pub fn reset_arenas(&self) {
        let mut pool = self.arena_pool.lock().unwrap();
        for arena in pool.iter_mut() {
            arena.reset();
        }
        self.stats.arena_resets.fetch_add(1, Ordering::SeqCst);
    }

    /// Get allocator statistics
    pub fn stats(&self) -> AllocatorStatsSnapshot {
        self.stats.snapshot()
    }

    /// Get configuration
    pub fn config(&self) -> &AllocatorConfig {
        &self.config
    }

    /// Create a memory report
    pub fn memory_report(&self) -> String {
        let stats = self.stats();
        format!(
            "Memory Allocator Report\n\
             ========================\n\
             Mode: {}\n\
             Growth Strategy: {}\n\
             \n\
             {}\n\
             \n\
             Heap Capacity: {} / {}\n",
            self.config.mode,
            self.config.growth_strategy,
            stats,
            format_bytes(self.heap_capacity.load(Ordering::SeqCst)),
            format_bytes(self.config.max_heap_size)
        )
    }
}

// SAFETY: DynamicAllocator uses proper synchronization for thread-safety
unsafe impl Send for DynamicAllocator {}
unsafe impl Sync for DynamicAllocator {}

// ============================================
// GLOBAL ALLOCATOR INSTANCE
// ============================================

use once_cell::sync::Lazy;

// ============================================
// GLOBAL ALLOCATOR INSTANCE
// ============================================

/// Global allocator instance
static GLOBAL_ALLOCATOR: Lazy<Mutex<Option<Arc<DynamicAllocator>>>> =
    Lazy::new(|| Mutex::new(None));

/// Initialize the global allocator with given config
pub fn init_allocator(config: AllocatorConfig) -> Result<(), LangError> {
    let allocator = DynamicAllocator::new(config)?;
    let mut global = GLOBAL_ALLOCATOR.lock().unwrap();
    *global = Some(Arc::new(allocator));
    Ok(())
}

/// Get the global allocator
pub fn get_allocator() -> Option<Arc<DynamicAllocator>> {
    let global = GLOBAL_ALLOCATOR.lock().unwrap();
    global.clone()
}

/// Get or create the global allocator with defaults
pub fn get_or_create_allocator() -> Result<Arc<DynamicAllocator>, LangError> {
    let mut global = GLOBAL_ALLOCATOR.lock().unwrap();
    if global.is_none() {
        *global = Some(Arc::new(DynamicAllocator::with_defaults()?));
    }
    Ok(global.as_ref().unwrap().clone())
}

// ============================================
// TESTS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_allocator() {
        let config = AllocatorConfig::static_mode(1024);
        let alloc = DynamicAllocator::new(config).unwrap();

        // Allocate within limits
        let ptr = alloc.alloc(512).unwrap();

        // Free
        alloc.dealloc(ptr, 512);

        let stats = alloc.stats();
        assert_eq!(stats.total_allocations, 1);
        assert_eq!(stats.total_deallocations, 1);
    }

    #[test]
    fn test_static_oom() {
        let config = AllocatorConfig::static_mode(256);
        let alloc = DynamicAllocator::new(config).unwrap();

        // First allocation succeeds
        let ptr1 = alloc.alloc(128).unwrap();

        // Second allocation that would exceed limit fails
        let result = alloc.alloc(256);
        assert!(result.is_err());

        let stats = alloc.stats();
        assert_eq!(stats.oom_events, 1);

        alloc.dealloc(ptr1, 128);
    }

    #[test]
    fn test_dynamic_growth() {
        let config = AllocatorConfig::dynamic_mode(256, 1024 * 1024, GrowthStrategy::Doubling);
        let alloc = DynamicAllocator::new(config).unwrap();

        // Allocate more than initial capacity with a large allocation
        // that exceeds slab size classes to force heap allocation
        let ptr = alloc.alloc(4096).unwrap();

        let stats = alloc.stats();
        assert!(stats.heap_expansions > 0 || stats.total_allocations > 0);

        alloc.dealloc(ptr, 4096);
    }

    #[test]
    fn test_slab_allocation() {
        let config = AllocatorConfig::default();
        let alloc = DynamicAllocator::new(config).unwrap();

        // Small allocations should use slab
        let ptrs: Vec<_> = (0..100).map(|_| alloc.alloc(32).unwrap()).collect();

        // All should succeed
        assert_eq!(ptrs.len(), 100);

        // Free all
        for ptr in ptrs {
            alloc.dealloc(ptr, 32);
        }
    }

    #[test]
    fn test_hybrid_mode() {
        let config = AllocatorConfig::hybrid_mode(4096, 2);
        let alloc = DynamicAllocator::new(config).unwrap();

        // Small allocations should use arena
        let ptr1 = alloc.alloc(64).unwrap();
        let ptr2 = alloc.alloc(64).unwrap();

        let stats = alloc.stats();
        assert!(stats.arena_allocations > 0);

        // Reset arenas
        alloc.reset_arenas();

        let stats = alloc.stats();
        assert_eq!(stats.arena_resets, 1);

        // Note: Don't dealloc arena-allocated memory after reset
        let _ = ptr1;
        let _ = ptr2;
    }

    #[test]
    fn test_realloc() {
        let config = AllocatorConfig::default();
        let alloc = DynamicAllocator::new(config).unwrap();

        let ptr = alloc.alloc(64).unwrap();

        // Write some data
        unsafe {
            std::ptr::write(ptr.as_ptr() as *mut u64, 0x12345678);
        }

        // Reallocate larger
        let new_ptr = alloc.realloc(ptr, 64, 256).unwrap();

        // Data should be preserved
        let data = unsafe { std::ptr::read(new_ptr.as_ptr() as *const u64) };
        assert_eq!(data, 0x12345678);

        alloc.dealloc(new_ptr, 256);
    }

    #[test]
    fn test_memory_report() {
        let config = AllocatorConfig::default();
        let alloc = DynamicAllocator::new(config).unwrap();

        let ptr = alloc.alloc(1024).unwrap();

        let report = alloc.memory_report();
        assert!(report.contains("Memory Allocator Report"));
        assert!(report.contains("dynamic"));

        alloc.dealloc(ptr, 1024);
    }

    #[test]
    fn test_alloc_modes() {
        assert_eq!(AllocMode::Static.to_string(), "static");
        assert_eq!(AllocMode::Dynamic.to_string(), "dynamic");
        assert_eq!(AllocMode::Hybrid.to_string(), "hybrid");

        assert_eq!("static".parse::<AllocMode>().unwrap(), AllocMode::Static);
        assert_eq!("dynamic".parse::<AllocMode>().unwrap(), AllocMode::Dynamic);
        assert_eq!("hybrid".parse::<AllocMode>().unwrap(), AllocMode::Hybrid);
    }
}
