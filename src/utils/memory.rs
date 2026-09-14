//! Memory Management Infrastructure
//!
//! Provides foundational memory safety primitives for the language runtime:
//! - Reference counting for internal tracking (non-GC)
//! - Unique ownership API for safe value handling
//! - Borrow-like temporary reference tracking
//! - Lifetime validation support for HIR passes
//! - Debug mode poisoning for freed values
//! - Small String Optimization (SSO) for strings under 22 bytes
//! - Small Array Optimization (SAO) for arrays under 8 elements
//! - Bump allocator for short-lived AST/HIR nodes
//! - Arena allocator for runtime temporaries
//!
//! NOTE: This is foundational infrastructure, not a full borrow checker.
//! It provides internal safety mechanisms that can be extended in the future.

use std::alloc::{Layout, alloc, dealloc};
use std::cell::Cell;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::memory::policy::memory_policy;

#[inline]
fn assert_arc_enabled() {
    let policy = memory_policy();
    if !policy.allow_arc {
        panic!(
            "ARC is disabled in the current memory policy (embedded mode). Use stack/arena ownership instead of `share`."
        );
    }
}

#[inline]
fn assert_weak_enabled() {
    let policy = memory_policy();
    if !(policy.allow_arc && policy.allow_weak) {
        panic!("Weak references are disabled in the current memory policy (embedded mode).");
    }
}

// ============================================
// REFERENCE COUNTING INFRASTRUCTURE
// ============================================

/// Reference count tracker for runtime values
/// Uses atomic operations for thread-safety
#[derive(Debug)]
pub struct RefCount {
    /// Strong reference count
    strong: AtomicUsize,
    /// Weak reference count (for future weak pointer support)
    weak: AtomicUsize,
    /// Unique ID for debugging and tracking
    id: u64,
}

static REF_COUNT_ID: AtomicU64 = AtomicU64::new(1);

impl RefCount {
    /// Create a new reference count with initial strong count of 1
    #[inline]
    pub fn new() -> Self {
        RefCount {
            strong: AtomicUsize::new(1),
            weak: AtomicUsize::new(0),
            id: REF_COUNT_ID.fetch_add(1, Ordering::SeqCst),
        }
    }

    /// Increment strong reference count
    #[inline]
    pub fn inc_strong(&self) -> usize {
        self.strong.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Decrement strong reference count, returns true if count reached 0
    #[inline]
    pub fn dec_strong(&self) -> bool {
        let prev = self.strong.fetch_sub(1, Ordering::SeqCst);
        prev == 1
    }

    /// Get current strong reference count
    #[inline]
    pub fn strong_count(&self) -> usize {
        self.strong.load(Ordering::SeqCst)
    }

    /// Increment weak reference count
    #[inline]
    pub fn inc_weak(&self) -> usize {
        self.weak.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Decrement weak reference count
    #[inline]
    pub fn dec_weak(&self) -> usize {
        self.weak.fetch_sub(1, Ordering::SeqCst) - 1
    }

    /// Get unique ID for this reference
    #[inline]
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Default for RefCount {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// UNIQUE OWNERSHIP TRACKING
// ============================================

/// Ownership state for runtime values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipKind {
    /// Value is uniquely owned - can be moved or borrowed
    Unique,
    /// Value is shared (reference counted)
    Shared,
    /// Value has been moved and is no longer valid
    Moved,
    /// Value is temporarily borrowed (immutable)
    Borrowed,
    /// Value is temporarily borrowed (mutable)
    BorrowedMut,
    /// Value has been freed/poisoned (debug mode)
    Poisoned,
}

/// Tracks ownership state of a runtime value
#[derive(Debug)]
pub struct OwnershipTracker {
    /// Current ownership state
    kind: Cell<OwnershipKind>,
    /// Number of active borrows
    borrow_count: Cell<usize>,
    /// Number of active mutable borrows (should be 0 or 1)
    mut_borrow_count: Cell<usize>,
    /// Source location where value was created (for debugging)
    created_at: Option<String>,
}

impl OwnershipTracker {
    /// Create a new unique ownership tracker
    #[inline]
    pub fn new_unique() -> Self {
        OwnershipTracker {
            kind: Cell::new(OwnershipKind::Unique),
            borrow_count: Cell::new(0),
            mut_borrow_count: Cell::new(0),
            created_at: None,
        }
    }

    /// Create with source location for debugging
    pub fn new_with_location(loc: String) -> Self {
        OwnershipTracker {
            kind: Cell::new(OwnershipKind::Unique),
            borrow_count: Cell::new(0),
            mut_borrow_count: Cell::new(0),
            created_at: Some(loc),
        }
    }

    /// Get current ownership kind
    #[inline]
    pub fn kind(&self) -> OwnershipKind {
        self.kind.get()
    }

    /// Try to borrow immutably - returns false if already mutably borrowed or moved
    pub fn try_borrow(&self) -> bool {
        match self.kind.get() {
            OwnershipKind::Unique | OwnershipKind::Shared | OwnershipKind::Borrowed => {
                self.borrow_count.set(self.borrow_count.get() + 1);
                self.kind.set(OwnershipKind::Borrowed);
                true
            }
            OwnershipKind::BorrowedMut | OwnershipKind::Moved | OwnershipKind::Poisoned => false,
        }
    }

    /// Release an immutable borrow
    pub fn release_borrow(&self) {
        let count = self.borrow_count.get();
        if count > 0 {
            self.borrow_count.set(count - 1);
            if count == 1 && self.mut_borrow_count.get() == 0 {
                // No more borrows, revert to previous state
                self.kind.set(OwnershipKind::Unique);
            }
        }
    }

    /// Try to borrow mutably - returns false if any borrows exist or moved
    pub fn try_borrow_mut(&self) -> bool {
        match self.kind.get() {
            OwnershipKind::Unique => {
                if self.borrow_count.get() == 0 && self.mut_borrow_count.get() == 0 {
                    self.mut_borrow_count.set(1);
                    self.kind.set(OwnershipKind::BorrowedMut);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Release a mutable borrow
    pub fn release_borrow_mut(&self) {
        if self.mut_borrow_count.get() > 0 {
            self.mut_borrow_count.set(0);
            self.kind.set(OwnershipKind::Unique);
        }
    }

    /// Mark as moved - value is no longer valid through this tracker
    pub fn mark_moved(&self) {
        self.kind.set(OwnershipKind::Moved);
    }

    /// Mark as shared (reference counted)
    pub fn mark_shared(&self) {
        self.kind.set(OwnershipKind::Shared);
    }

    /// Poison the value in debug mode
    #[cfg(debug_assertions)]
    pub fn poison(&self) {
        self.kind.set(OwnershipKind::Poisoned);
    }

    /// Check if value is valid (not moved or poisoned)
    #[inline]
    pub fn is_valid(&self) -> bool {
        matches!(
            self.kind.get(),
            OwnershipKind::Unique
                | OwnershipKind::Shared
                | OwnershipKind::Borrowed
                | OwnershipKind::BorrowedMut
        )
    }

    /// Returns true if any immutable or mutable borrows are active
    #[inline]
    pub fn is_borrowed(&self) -> bool {
        self.borrow_count.get() > 0 || self.mut_borrow_count.get() > 0
    }

    /// Returns true if the value can be moved (no active borrows and not moved/poisoned)
    #[inline]
    pub fn can_move(&self) -> bool {
        self.is_valid() && !self.is_borrowed()
    }
}

impl Clone for OwnershipTracker {
    fn clone(&self) -> Self {
        OwnershipTracker {
            kind: Cell::new(OwnershipKind::Unique),
            borrow_count: Cell::new(0),
            mut_borrow_count: Cell::new(0),
            created_at: self.created_at.clone(),
        }
    }
}

impl Default for OwnershipTracker {
    fn default() -> Self {
        Self::new_unique()
    }
}

// ============================================
// SMALL STRING OPTIMIZATION (SSO)
// ============================================

/// Maximum inline string size (22 bytes to fit in 24-byte struct)
pub const SSO_MAX_LEN: usize = 22;

/// Small String Optimized string - stores small strings inline
#[derive(Clone)]
pub enum SsoString {
    /// Inline storage for strings <= SSO_MAX_LEN bytes
    Inline {
        /// Inline buffer (22 bytes max)
        buf: [u8; SSO_MAX_LEN],
        /// Length of the string
        len: u8,
    },
    /// Heap-allocated storage for larger strings
    Heap(Arc<str>),
}

impl SsoString {
    /// Create a new SSO string from a string slice
    #[inline]
    pub fn new(s: &str) -> Self {
        if s.len() <= SSO_MAX_LEN {
            let mut buf = [0u8; SSO_MAX_LEN];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            SsoString::Inline {
                buf,
                len: s.len() as u8,
            }
        } else {
            SsoString::Heap(Arc::from(s))
        }
    }

    /// Create from owned String
    #[inline]
    pub fn from_string(s: String) -> Self {
        if s.len() <= SSO_MAX_LEN {
            let mut buf = [0u8; SSO_MAX_LEN];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            SsoString::Inline {
                buf,
                len: s.len() as u8,
            }
        } else {
            SsoString::Heap(Arc::from(s.as_str()))
        }
    }

    /// Get string as &str
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            SsoString::Inline { buf, len } => {
                // SAFETY: We only ever create SsoString from valid &str or String,
                // both of which guarantee valid UTF-8. The length is tracked correctly
                // in the `len` field which is set during construction.
                #[cfg(debug_assertions)]
                {
                    // In debug mode, validate UTF-8 to catch any bugs in construction
                    debug_assert!(
                        std::str::from_utf8(&buf[..*len as usize]).is_ok(),
                        "SsoString contains invalid UTF-8"
                    );
                }
                unsafe { std::str::from_utf8_unchecked(&buf[..*len as usize]) }
            }
            SsoString::Heap(s) => s,
        }
    }

    /// Get string length
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            SsoString::Inline { len, .. } => *len as usize,
            SsoString::Heap(s) => s.len(),
        }
    }

    /// Check if string is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if using inline storage
    #[inline]
    pub fn is_inline(&self) -> bool {
        matches!(self, SsoString::Inline { .. })
    }
}

impl std::fmt::Debug for SsoString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl std::fmt::Display for SsoString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl PartialEq for SsoString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SsoString {}

impl std::hash::Hash for SsoString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

// ============================================
// SMALL ARRAY OPTIMIZATION (SAO)
// ============================================

/// Maximum number of elements for inline array storage
pub const SAO_MAX_ELEMENTS: usize = 8;

/// Small Array Optimized vector - stores small arrays inline
/// Note: This is a simplified version. The actual Value enum handles this inline.
#[derive(Clone, Debug)]
pub enum SaoArray<T: Clone> {
    /// Inline storage for arrays <= SAO_MAX_ELEMENTS elements
    Inline {
        /// Array of elements (fixed size)
        data: [Option<T>; SAO_MAX_ELEMENTS],
        /// Number of elements
        len: u8,
    },
    /// Heap-allocated storage for larger arrays
    Heap(Vec<T>),
}

impl<T: Clone + Default> SaoArray<T> {
    /// Create a new empty SAO array
    pub fn new() -> Self {
        SaoArray::Inline {
            data: Default::default(),
            len: 0,
        }
    }

    /// Create from a Vec
    pub fn from_vec(v: Vec<T>) -> Self {
        if v.len() <= SAO_MAX_ELEMENTS {
            let len = v.len() as u8;
            let mut data: [Option<T>; SAO_MAX_ELEMENTS] = Default::default();
            for (i, item) in v.into_iter().enumerate() {
                data[i] = Some(item);
            }
            SaoArray::Inline { data, len }
        } else {
            SaoArray::Heap(v)
        }
    }

    /// Get length
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            SaoArray::Inline { len, .. } => *len as usize,
            SaoArray::Heap(v) => v.len(),
        }
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if using inline storage
    #[inline]
    pub fn is_inline(&self) -> bool {
        matches!(self, SaoArray::Inline { .. })
    }

    /// Get element at index
    pub fn get(&self, index: usize) -> Option<&T> {
        match self {
            SaoArray::Inline { data, len } => {
                if index < *len as usize {
                    data[index].as_ref()
                } else {
                    None
                }
            }
            SaoArray::Heap(v) => v.get(index),
        }
    }

    /// Convert to Vec
    pub fn to_vec(&self) -> Vec<T> {
        match self {
            SaoArray::Inline { data, len } => data[..*len as usize]
                .iter()
                .filter_map(|x| x.clone())
                .collect(),
            SaoArray::Heap(v) => v.clone(),
        }
    }
}

impl<T: Clone + Default> Default for SaoArray<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// BUMP ALLOCATOR
// ============================================

/// Bump allocator for short-lived allocations (AST/HIR nodes)
///
/// This allocator is very fast for allocation but cannot free individual
/// objects - the entire arena is freed at once when reset or dropped.
pub struct BumpAllocator {
    /// Current chunk of memory
    chunk: NonNull<u8>,
    /// Size of current chunk
    chunk_size: usize,
    /// Current position in chunk
    position: usize,
    /// List of all allocated chunks for cleanup
    chunks: Vec<(NonNull<u8>, usize)>,
    /// Statistics
    total_allocated: usize,
    allocation_count: usize,
}

impl BumpAllocator {
    /// Default chunk size (64KB)
    pub const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;

    /// Create a new bump allocator with default chunk size
    pub fn new() -> Self {
        Self::with_chunk_size(Self::DEFAULT_CHUNK_SIZE)
    }

    /// Create a new bump allocator with specified chunk size
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        let chunk = Self::alloc_chunk(chunk_size);
        BumpAllocator {
            chunk,
            chunk_size,
            position: 0,
            chunks: vec![(chunk, chunk_size)],
            total_allocated: 0,
            allocation_count: 0,
        }
    }

    /// Allocate a new chunk
    fn alloc_chunk(size: usize) -> NonNull<u8> {
        let layout = Layout::from_size_align(size, 8).expect("Invalid layout");
        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                panic!("BumpAllocator: allocation failed");
            }
            NonNull::new_unchecked(ptr)
        }
    }

    /// Allocate memory for a value of type T
    pub fn alloc<T>(&mut self, value: T) -> &mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc_raw(layout);
        unsafe {
            let typed_ptr = ptr as *mut T;
            typed_ptr.write(value);
            &mut *typed_ptr
        }
    }

    /// Allocate raw memory with given layout
    pub fn alloc_raw(&mut self, layout: Layout) -> *mut u8 {
        // Align position
        let align = layout.align();
        let aligned_pos = (self.position + align - 1) & !(align - 1);
        let new_pos = aligned_pos + layout.size();

        if new_pos > self.chunk_size {
            // Need new chunk
            let new_size = self.chunk_size.max(layout.size() * 2);
            let new_chunk = Self::alloc_chunk(new_size);
            self.chunks.push((new_chunk, new_size));
            self.chunk = new_chunk;
            self.chunk_size = new_size;
            self.position = 0;
            return self.alloc_raw(layout);
        }

        self.total_allocated += layout.size();
        self.allocation_count += 1;
        self.position = new_pos;

        unsafe { self.chunk.as_ptr().add(aligned_pos) }
    }

    /// Reset the allocator (reuse memory without deallocation)
    pub fn reset(&mut self) {
        // Keep the first chunk, deallocate others
        while self.chunks.len() > 1 {
            let (chunk, size) = self.chunks.pop().unwrap();
            unsafe {
                let layout = Layout::from_size_align_unchecked(size, 8);
                dealloc(chunk.as_ptr(), layout);
            }
        }
        if let Some((chunk, size)) = self.chunks.first() {
            self.chunk = *chunk;
            self.chunk_size = *size;
        }
        self.position = 0;
        self.total_allocated = 0;
        self.allocation_count = 0;
    }

    /// Get statistics
    pub fn stats(&self) -> (usize, usize, usize) {
        (
            self.total_allocated,
            self.allocation_count,
            self.chunks.len(),
        )
    }
}

impl Default for BumpAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BumpAllocator {
    fn drop(&mut self) {
        for (chunk, size) in &self.chunks {
            unsafe {
                let layout = Layout::from_size_align_unchecked(*size, 8);
                dealloc(chunk.as_ptr(), layout);
            }
        }
    }
}

// SAFETY: BumpAllocator manages its own memory and uses proper synchronization
// However, it's not Send/Sync by default since the pointers aren't thread-safe
// For thread-safe usage, wrap in a Mutex

// ============================================
// ARENA ALLOCATOR
// ============================================

/// Thread-local arena for runtime temporaries
///
/// Provides fast allocation for values that have a bounded lifetime
/// (e.g., function call arguments, intermediate expression results).
pub struct Arena {
    /// Bump allocator for this arena
    bump: BumpAllocator,
    /// Arena generation (incremented on reset)
    generation: u64,
}

impl Arena {
    /// Create a new arena
    pub fn new() -> Self {
        Arena {
            bump: BumpAllocator::new(),
            generation: 0,
        }
    }

    /// Allocate a value in this arena
    pub fn alloc<T>(&mut self, value: T) -> &mut T {
        self.bump.alloc(value)
    }

    /// Reset the arena, invalidating all allocations
    pub fn reset(&mut self) {
        self.bump.reset();
        self.generation += 1;
    }

    /// Get current generation
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Get allocation statistics
    pub fn stats(&self) -> (usize, usize, usize) {
        self.bump.stats()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// LIFETIME SCOPE TRACKING (for HIR passes)
// ============================================

/// Represents a scope for lifetime tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

/// Represents a lifetime (bound to a scope)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifetime {
    /// Scope where this lifetime is valid
    pub scope: ScopeId,
    /// Whether this can escape the scope
    pub can_escape: bool,
}

impl Lifetime {
    /// Create a new lifetime bound to a scope
    pub fn new(scope: ScopeId) -> Self {
        Lifetime {
            scope,
            can_escape: false,
        }
    }

    /// Create a lifetime that can escape its scope
    pub fn escapable(scope: ScopeId) -> Self {
        Lifetime {
            scope,
            can_escape: true,
        }
    }

    /// Check if this lifetime outlives another
    pub fn outlives(&self, other: &Lifetime) -> bool {
        // A lifetime outlives another if its scope is the same or outer
        // For simplicity, we compare scope IDs (lower = outer)
        self.scope.0 <= other.scope.0
    }
}

/// Lifetime scope tracker for HIR analysis
#[derive(Debug, Default)]
pub struct LifetimeTracker {
    /// Current scope depth
    current_scope: u32,
    /// Stack of active scopes
    scope_stack: Vec<ScopeId>,
    /// Variables and their lifetimes
    var_lifetimes: std::collections::HashMap<String, Lifetime>,
}

impl LifetimeTracker {
    /// Create a new lifetime tracker
    pub fn new() -> Self {
        let mut tracker = LifetimeTracker::default();
        tracker.enter_scope(); // Global scope
        tracker
    }

    /// Enter a new scope
    pub fn enter_scope(&mut self) -> ScopeId {
        let id = ScopeId(self.current_scope);
        self.scope_stack.push(id);
        self.current_scope += 1;
        id
    }

    /// Exit current scope
    pub fn exit_scope(&mut self) -> Option<ScopeId> {
        self.scope_stack.pop()
    }

    /// Get current scope
    pub fn current_scope(&self) -> Option<ScopeId> {
        self.scope_stack.last().copied()
    }

    /// Register a variable with a lifetime
    pub fn register_var(&mut self, name: String, can_escape: bool) {
        if let Some(scope) = self.current_scope() {
            let lifetime = if can_escape {
                Lifetime::escapable(scope)
            } else {
                Lifetime::new(scope)
            };
            self.var_lifetimes.insert(name, lifetime);
        }
    }

    /// Get lifetime of a variable
    pub fn get_lifetime(&self, name: &str) -> Option<&Lifetime> {
        self.var_lifetimes.get(name)
    }

    /// Check if a value can escape current scope
    pub fn can_escape(&self, name: &str) -> bool {
        self.get_lifetime(name)
            .map(|lt| lt.can_escape)
            .unwrap_or(true) // Unknown variables assumed escapable
    }

    /// Validate that a reference doesn't escape its owner scope
    pub fn validate_no_escape(&self, var_name: &str, target_scope: ScopeId) -> Result<(), String> {
        if let Some(lt) = self.get_lifetime(var_name) {
            if !lt.can_escape && lt.scope.0 > target_scope.0 {
                return Err(format!(
                    "Reference to '{}' escapes its scope (scope {} -> {})",
                    var_name, lt.scope.0, target_scope.0
                ));
            }
        }
        Ok(())
    }
}

// ============================================
// DEBUG MODE POISONING
// ============================================

/// Poison pattern for freed values (debug mode)
#[cfg(debug_assertions)]
pub const POISON_PATTERN: u64 = 0xDEAD_BEEF_DEAD_BEEF;

/// Check if a value appears poisoned (debug mode)
///
/// # Safety
/// The caller must ensure that `ptr` points to valid, aligned memory.
#[cfg(debug_assertions)]
#[inline]
pub unsafe fn is_poisoned(ptr: *const u64) -> bool {
    unsafe { *ptr == POISON_PATTERN }
}

/// Poison a memory region (debug mode)
///
/// # Safety
/// The caller must ensure that `ptr` points to valid memory of at least `size` bytes.
#[cfg(debug_assertions)]
pub unsafe fn poison_memory(ptr: *mut u8, size: usize) {
    let pattern = POISON_PATTERN.to_ne_bytes();
    unsafe {
        for i in 0..size {
            *ptr.add(i) = pattern[i % 8];
        }
    }
}

// ============================================
// SMART POINTERS - USER-FACING API
// ============================================

/// Allocation source tag for debugging memory
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocSource {
    /// Stack-allocated value
    Stack,
    /// Heap-allocated value
    Heap,
    /// Arena-allocated value
    Arena,
    /// Bump allocator
    Bump,
    /// Unknown/untracked
    Unknown,
}

impl std::fmt::Display for AllocSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllocSource::Stack => write!(f, "stack"),
            AllocSource::Heap => write!(f, "heap"),
            AllocSource::Arena => write!(f, "arena"),
            AllocSource::Bump => write!(f, "bump"),
            AllocSource::Unknown => write!(f, "unknown"),
        }
    }
}

/// Shared smart pointer - allows multiple owners with reference counting
/// Similar to Rc<T> but tracks allocation source and supports weak refs
#[derive(Debug)]
pub struct Shared<T> {
    /// The actual data
    data: Arc<SharedInner<T>>,
}

#[derive(Debug)]
#[allow(dead_code)] // ref_count is infrastructure for future direct reference counting
struct SharedInner<T> {
    value: T,
    ref_count: RefCount,
    alloc_source: AllocSource,
    #[cfg(debug_assertions)]
    created_at: Option<String>,
}

impl<T> Shared<T> {
    /// Create a new shared value
    pub fn new(value: T) -> Self {
        assert_arc_enabled();
        Shared {
            data: Arc::new(SharedInner {
                value,
                ref_count: RefCount::new(),
                alloc_source: AllocSource::Heap,
                #[cfg(debug_assertions)]
                created_at: None,
            }),
        }
    }

    /// Create with allocation source tag
    pub fn with_source(value: T, source: AllocSource) -> Self {
        assert_arc_enabled();
        Shared {
            data: Arc::new(SharedInner {
                value,
                ref_count: RefCount::new(),
                alloc_source: source,
                #[cfg(debug_assertions)]
                created_at: None,
            }),
        }
    }

    /// Create with debug location
    #[cfg(debug_assertions)]
    pub fn with_location(value: T, location: String) -> Self {
        assert_arc_enabled();
        Shared {
            data: Arc::new(SharedInner {
                value,
                ref_count: RefCount::new(),
                alloc_source: AllocSource::Heap,
                created_at: Some(location),
            }),
        }
    }

    /// Get reference to inner value
    #[inline]
    pub fn get(&self) -> &T {
        &self.data.value
    }

    /// Get strong reference count
    #[inline]
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.data)
    }

    /// Get weak reference count
    #[inline]
    pub fn weak_count(&self) -> usize {
        Arc::weak_count(&self.data)
    }

    /// Get allocation source
    #[inline]
    pub fn alloc_source(&self) -> AllocSource {
        self.data.alloc_source
    }

    /// Create a weak reference to this value
    pub fn downgrade(&self) -> Weak<T> {
        assert_weak_enabled();
        Weak {
            data: Arc::downgrade(&self.data),
        }
    }

    /// Check if this is the only reference
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.data) == 1
    }

    /// Try to get mutable reference (only if unique)
    pub fn get_mut(&mut self) -> Option<&mut T> {
        Arc::get_mut(&mut self.data).map(|inner| &mut inner.value)
    }
}

impl<T: Clone> Shared<T> {
    /// Get a mutable reference, cloning if necessary
    pub fn make_mut(&mut self) -> &mut T {
        if !self.is_unique() {
            // Clone the data
            self.data = Arc::new(SharedInner {
                value: self.data.value.clone(),
                ref_count: RefCount::new(),
                alloc_source: self.data.alloc_source,
                #[cfg(debug_assertions)]
                created_at: self.data.created_at.clone(),
            });
        }
        Arc::get_mut(&mut self.data)
            .map(|inner| &mut inner.value)
            .unwrap()
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        assert_arc_enabled();
        Shared {
            data: Arc::clone(&self.data),
        }
    }
}

impl<T> std::ops::Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

/// Weak reference - non-owning reference that doesn't prevent deallocation
#[derive(Debug)]
pub struct Weak<T> {
    data: std::sync::Weak<SharedInner<T>>,
}

impl<T> Weak<T> {
    /// Create a new dangling weak reference
    pub fn new() -> Self {
        Weak {
            data: std::sync::Weak::new(),
        }
    }

    /// Try to upgrade to a strong reference
    pub fn upgrade(&self) -> Option<Shared<T>> {
        assert_arc_enabled();
        self.data.upgrade().map(|data| Shared { data })
    }

    /// Check if the value is still alive
    pub fn is_alive(&self) -> bool {
        self.data.strong_count() > 0
    }

    /// Get strong count (0 if dropped)
    pub fn strong_count(&self) -> usize {
        self.data.strong_count()
    }
}

impl<T> Clone for Weak<T> {
    fn clone(&self) -> Self {
        Weak {
            data: self.data.clone(),
        }
    }
}

impl<T> Default for Weak<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique smart pointer - move-only, no copy allowed
/// Similar to Box<T> but with ownership tracking
#[derive(Debug)]
pub struct Unique<T> {
    value: Box<T>,
    ownership: OwnershipTracker,
    alloc_source: AllocSource,
}

impl<T> Unique<T> {
    /// Create a new unique value
    pub fn new(value: T) -> Self {
        Unique {
            value: Box::new(value),
            ownership: OwnershipTracker::new_unique(),
            alloc_source: AllocSource::Heap,
        }
    }

    /// Create with allocation source
    pub fn with_source(value: T, source: AllocSource) -> Self {
        Unique {
            value: Box::new(value),
            ownership: OwnershipTracker::new_unique(),
            alloc_source: source,
        }
    }

    /// Get reference to inner value
    #[inline]
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference to inner value
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Convert to inner value, consuming the Unique
    ///
    /// NOTE: We use ManuallyDrop + ptr::read because Unique implements Drop.
    /// Simply using *self.value would trigger the Drop impl which poisons
    /// the value in debug mode. ManuallyDrop prevents this.
    pub fn into_inner(self) -> T {
        // Use ManuallyDrop to prevent Drop from running (which would poison the value)
        use std::mem::ManuallyDrop;
        let this = ManuallyDrop::new(self);
        // SAFETY: We're consuming self and only reading the value once.
        // ManuallyDrop ensures our Drop impl doesn't run.
        unsafe { std::ptr::read(&*this.value) }
    }

    /// Get allocation source
    #[inline]
    pub fn alloc_source(&self) -> AllocSource {
        self.alloc_source
    }

    /// Try to borrow immutably
    pub fn try_borrow(&self) -> bool {
        self.ownership.try_borrow()
    }

    /// Release immutable borrow
    pub fn release_borrow(&self) {
        self.ownership.release_borrow()
    }

    /// Convert to shared (transfers ownership)
    pub fn into_shared(self) -> Shared<T> {
        assert_arc_enabled();
        let value = self.into_inner();
        Shared::new(value)
    }
}

impl<T> std::ops::Deref for Unique<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Unique<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> Drop for Unique<T> {
    fn drop(&mut self) {
        // Poison in debug mode
        #[cfg(debug_assertions)]
        self.ownership.poison();
    }
}

// ============================================
// MEMORY REPORTING (--memory mode)
// ============================================

/// Memory statistics for reporting
#[derive(Debug, Clone, Default)]
pub struct MemoryReport {
    /// Total heap allocations
    pub heap_allocations: usize,
    /// Total heap bytes allocated
    pub heap_bytes: usize,
    /// Total arena allocations
    pub arena_allocations: usize,
    /// Total arena bytes
    pub arena_bytes: usize,
    /// Total bump allocations
    pub bump_allocations: usize,
    /// Total bump bytes
    pub bump_bytes: usize,
    /// Active shared pointers
    pub active_shared: usize,
    /// Active unique pointers
    pub active_unique: usize,
    /// Active weak references
    pub active_weak: usize,
    /// Peak memory usage (bytes)
    pub peak_bytes: usize,
    /// Current memory usage (bytes)
    pub current_bytes: usize,
    /// Allocation breakdown by source
    pub by_source: std::collections::HashMap<String, (usize, usize)>, // source -> (count, bytes)
}

impl MemoryReport {
    /// Create a new empty report
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a heap allocation
    pub fn record_heap_alloc(&mut self, bytes: usize) {
        self.heap_allocations += 1;
        self.heap_bytes += bytes;
        self.current_bytes += bytes;
        self.peak_bytes = self.peak_bytes.max(self.current_bytes);
    }

    /// Record a heap deallocation
    pub fn record_heap_dealloc(&mut self, bytes: usize) {
        self.current_bytes = self.current_bytes.saturating_sub(bytes);
    }

    /// Record an arena allocation
    pub fn record_arena_alloc(&mut self, bytes: usize) {
        self.arena_allocations += 1;
        self.arena_bytes += bytes;
        self.current_bytes += bytes;
        self.peak_bytes = self.peak_bytes.max(self.current_bytes);
    }

    /// Record a bump allocation
    pub fn record_bump_alloc(&mut self, bytes: usize) {
        self.bump_allocations += 1;
        self.bump_bytes += bytes;
        self.current_bytes += bytes;
        self.peak_bytes = self.peak_bytes.max(self.current_bytes);
    }

    /// Get total allocations
    pub fn total_allocations(&self) -> usize {
        self.heap_allocations + self.arena_allocations + self.bump_allocations
    }

    /// Get total bytes
    pub fn total_bytes(&self) -> usize {
        self.heap_bytes + self.arena_bytes + self.bump_bytes
    }

    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Memory Report:\n\
             ├─ Heap: {} allocations, {} bytes\n\
             ├─ Arena: {} allocations, {} bytes\n\
             ├─ Bump: {} allocations, {} bytes\n\
             ├─ Total: {} allocations, {} bytes\n\
             ├─ Peak: {} bytes\n\
             ├─ Current: {} bytes\n\
             └─ Smart Pointers: {} shared, {} unique, {} weak",
            self.heap_allocations,
            format_bytes(self.heap_bytes),
            self.arena_allocations,
            format_bytes(self.arena_bytes),
            self.bump_allocations,
            format_bytes(self.bump_bytes),
            self.total_allocations(),
            format_bytes(self.total_bytes()),
            format_bytes(self.peak_bytes),
            format_bytes(self.current_bytes),
            self.active_shared,
            self.active_unique,
            self.active_weak
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
// THREAD-LOCAL STORAGE FOR ARENAS
// ============================================

thread_local! {
    /// Thread-local arena for temporary allocations
    static THREAD_ARENA: std::cell::RefCell<Arena> = std::cell::RefCell::new(Arena::new());

    /// Thread-local bump allocator
    static THREAD_BUMP: std::cell::RefCell<BumpAllocator> = std::cell::RefCell::new(BumpAllocator::new());

    /// Thread-local memory report
    static THREAD_MEMORY_REPORT: std::cell::RefCell<MemoryReport> = std::cell::RefCell::new(MemoryReport::new());
}

/// Allocate in thread-local arena
pub fn arena_alloc<T>(value: T) -> *mut T {
    THREAD_ARENA.with(|arena| {
        let mut arena = arena.borrow_mut();
        arena.alloc(value) as *mut T
    })
}

/// Reset thread-local arena
pub fn arena_reset() {
    THREAD_ARENA.with(|arena| {
        arena.borrow_mut().reset();
    });
}

/// Allocate in thread-local bump allocator
pub fn bump_alloc<T>(value: T) -> *mut T {
    THREAD_BUMP.with(|bump| {
        let mut bump = bump.borrow_mut();
        bump.alloc(value) as *mut T
    })
}

/// Reset thread-local bump allocator
pub fn bump_reset() {
    THREAD_BUMP.with(|bump| {
        bump.borrow_mut().reset();
    });
}

/// Get thread-local memory report
pub fn get_memory_report() -> MemoryReport {
    THREAD_MEMORY_REPORT.with(|report| report.borrow().clone())
}

/// Record allocation in thread-local memory report
pub fn record_alloc(source: AllocSource, bytes: usize) {
    THREAD_MEMORY_REPORT.with(|report| {
        let mut report = report.borrow_mut();
        match source {
            AllocSource::Heap => report.record_heap_alloc(bytes),
            AllocSource::Arena => report.record_arena_alloc(bytes),
            AllocSource::Bump => report.record_bump_alloc(bytes),
            // Stack and Unknown allocations are not tracked in the memory report
            // since stack allocations don't go through the allocator and
            // unknown allocations represent legacy/untracked paths
            AllocSource::Stack | AllocSource::Unknown => {}
        }
    });
}

// ============================================
// BORROW CHECKER PREPASS SUPPORT
// ============================================

/// Borrow state for a variable during analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorrowState {
    /// Not borrowed
    Unborrowed,
    /// Immutably borrowed (count of active borrows)
    ImmutablyBorrowed(usize),
    /// Mutably borrowed
    MutablyBorrowed,
    /// Moved (no longer accessible)
    Moved,
}

/// Borrow checker context for HIR analysis
#[derive(Debug, Default)]
pub struct BorrowChecker {
    /// Variable borrow states
    states: std::collections::HashMap<String, BorrowState>,
    /// Errors accumulated during checking
    errors: Vec<BorrowError>,
    /// Current scope depth
    scope_depth: usize,
}

/// Error from borrow checking
#[derive(Debug, Clone)]
pub struct BorrowError {
    pub variable: String,
    pub message: String,
    pub location: Option<String>,
}

impl BorrowChecker {
    /// Create a new borrow checker
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a new scope
    pub fn enter_scope(&mut self) {
        self.scope_depth += 1;
    }

    /// Exit current scope (releases all borrows in scope)
    pub fn exit_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    /// Declare a new variable
    pub fn declare(&mut self, name: String) {
        self.states.insert(name, BorrowState::Unborrowed);
    }

    /// Try to borrow immutably
    pub fn try_borrow(&mut self, name: &str) -> bool {
        match self.states.get(name) {
            Some(BorrowState::Unborrowed) => {
                self.states
                    .insert(name.to_string(), BorrowState::ImmutablyBorrowed(1));
                true
            }
            Some(BorrowState::ImmutablyBorrowed(n)) => {
                self.states
                    .insert(name.to_string(), BorrowState::ImmutablyBorrowed(n + 1));
                true
            }
            Some(BorrowState::MutablyBorrowed) => {
                self.errors.push(BorrowError {
                    variable: name.to_string(),
                    message: format!("Cannot borrow '{}' immutably while mutably borrowed", name),
                    location: None,
                });
                false
            }
            Some(BorrowState::Moved) => {
                self.errors.push(BorrowError {
                    variable: name.to_string(),
                    message: format!("Cannot borrow '{}' - value has been moved", name),
                    location: None,
                });
                false
            }
            None => {
                self.errors.push(BorrowError {
                    variable: name.to_string(),
                    message: format!("Unknown variable '{}'", name),
                    location: None,
                });
                false
            }
        }
    }

    /// Try to borrow mutably
    pub fn try_borrow_mut(&mut self, name: &str) -> bool {
        match self.states.get(name) {
            Some(BorrowState::Unborrowed) => {
                self.states
                    .insert(name.to_string(), BorrowState::MutablyBorrowed);
                true
            }
            Some(BorrowState::ImmutablyBorrowed(_)) => {
                self.errors.push(BorrowError {
                    variable: name.to_string(),
                    message: format!("Cannot borrow '{}' mutably while immutably borrowed", name),
                    location: None,
                });
                false
            }
            Some(BorrowState::MutablyBorrowed) => {
                self.errors.push(BorrowError {
                    variable: name.to_string(),
                    message: format!("Cannot borrow '{}' mutably more than once", name),
                    location: None,
                });
                false
            }
            Some(BorrowState::Moved) => {
                self.errors.push(BorrowError {
                    variable: name.to_string(),
                    message: format!("Cannot borrow '{}' - value has been moved", name),
                    location: None,
                });
                false
            }
            None => {
                self.errors.push(BorrowError {
                    variable: name.to_string(),
                    message: format!("Unknown variable '{}'", name),
                    location: None,
                });
                false
            }
        }
    }

    /// Release an immutable borrow
    pub fn release_borrow(&mut self, name: &str) {
        if let Some(BorrowState::ImmutablyBorrowed(n)) = self.states.get(name) {
            if *n <= 1 {
                self.states
                    .insert(name.to_string(), BorrowState::Unborrowed);
            } else {
                self.states
                    .insert(name.to_string(), BorrowState::ImmutablyBorrowed(n - 1));
            }
        }
    }

    /// Release a mutable borrow
    pub fn release_borrow_mut(&mut self, name: &str) {
        if let Some(BorrowState::MutablyBorrowed) = self.states.get(name) {
            self.states
                .insert(name.to_string(), BorrowState::Unborrowed);
        }
    }

    /// Mark a variable as moved
    pub fn mark_moved(&mut self, name: &str) {
        if self.states.contains_key(name) {
            self.states.insert(name.to_string(), BorrowState::Moved);
        }
    }

    /// Get accumulated errors
    pub fn errors(&self) -> &[BorrowError] {
        &self.errors
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Clear errors
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }
}

// ============================================
// TESTS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ref_count() {
        let rc = RefCount::new();
        assert_eq!(rc.strong_count(), 1);

        rc.inc_strong();
        assert_eq!(rc.strong_count(), 2);

        assert!(!rc.dec_strong());
        assert_eq!(rc.strong_count(), 1);

        assert!(rc.dec_strong());
    }

    #[test]
    fn test_ownership_tracker() {
        let tracker = OwnershipTracker::new_unique();
        assert!(tracker.is_valid());
        assert_eq!(tracker.kind(), OwnershipKind::Unique);

        // Test borrowing
        assert!(tracker.try_borrow());
        assert_eq!(tracker.kind(), OwnershipKind::Borrowed);

        tracker.release_borrow();
        assert_eq!(tracker.kind(), OwnershipKind::Unique);

        // Test mutable borrowing
        assert!(tracker.try_borrow_mut());
        assert_eq!(tracker.kind(), OwnershipKind::BorrowedMut);

        // Can't borrow while mutably borrowed
        assert!(!tracker.try_borrow());

        tracker.release_borrow_mut();
        assert_eq!(tracker.kind(), OwnershipKind::Unique);
    }

    #[test]
    fn test_sso_string() {
        // Short string - inline
        let short = SsoString::new("hello");
        assert!(short.is_inline());
        assert_eq!(short.as_str(), "hello");
        assert_eq!(short.len(), 5);

        // Long string - heap
        let long_str = "this is a very long string that exceeds the SSO limit";
        let long = SsoString::new(long_str);
        assert!(!long.is_inline());
        assert_eq!(long.len(), long_str.len());
    }

    #[test]
    fn test_bump_allocator() {
        let mut bump = BumpAllocator::new();

        let a = bump.alloc(42i32);
        assert_eq!(*a, 42);

        let b = bump.alloc("hello".to_string());
        assert_eq!(b.as_str(), "hello");

        let (total, count, chunks) = bump.stats();
        assert!(total > 0);
        assert_eq!(count, 2);
        assert_eq!(chunks, 1);

        bump.reset();
        let (total, count, _) = bump.stats();
        assert_eq!(total, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_arena() {
        let mut arena = Arena::new();
        assert_eq!(arena.generation(), 0);

        let _x = arena.alloc(100i32);
        let _y = arena.alloc("test".to_string());

        arena.reset();
        assert_eq!(arena.generation(), 1);
    }

    #[test]
    fn test_lifetime_tracker() {
        let mut tracker = LifetimeTracker::new();

        // Register variable in global scope
        tracker.register_var("global".to_string(), true);

        // Enter function scope
        let fn_scope = tracker.enter_scope();
        tracker.register_var("local".to_string(), false);

        // local cannot escape to global
        assert!(tracker.validate_no_escape("local", ScopeId(0)).is_err());

        // global can be referenced anywhere
        assert!(tracker.validate_no_escape("global", fn_scope).is_ok());

        tracker.exit_scope();
    }

    #[test]
    fn test_shared_pointer() {
        let shared1 = Shared::new(42);
        assert_eq!(*shared1, 42);
        assert_eq!(shared1.strong_count(), 1);

        // Clone creates a new reference
        let shared2 = shared1.clone();
        assert_eq!(*shared2, 42);
        assert_eq!(shared1.strong_count(), 2);
        assert_eq!(shared2.strong_count(), 2);

        // Test weak reference
        let weak = shared1.downgrade();
        assert!(weak.is_alive());

        let upgraded = weak.upgrade();
        assert!(upgraded.is_some());
        assert_eq!(*upgraded.unwrap(), 42);
    }

    #[test]
    fn test_unique_pointer() {
        let mut unique = Unique::new(100);
        assert_eq!(*unique, 100);

        // Modify through mutable reference
        *unique.get_mut() = 200;
        assert_eq!(*unique, 200);

        // Convert to shared
        let shared = unique.into_shared();
        assert_eq!(*shared, 200);
    }

    #[test]
    fn test_memory_report() {
        let mut report = MemoryReport::new();

        report.record_heap_alloc(1024);
        report.record_arena_alloc(512);
        report.record_bump_alloc(256);

        assert_eq!(report.total_allocations(), 3);
        assert_eq!(report.total_bytes(), 1024 + 512 + 256);
        assert_eq!(report.peak_bytes, 1024 + 512 + 256);

        report.record_heap_dealloc(1024);
        assert_eq!(report.current_bytes, 512 + 256);
    }

    #[test]
    fn test_borrow_checker() {
        let mut checker = BorrowChecker::new();

        // Declare a variable
        checker.declare("x".to_string());

        // Multiple immutable borrows allowed
        assert!(checker.try_borrow("x"));
        assert!(checker.try_borrow("x"));

        // Mutable borrow not allowed while immutably borrowed
        assert!(!checker.try_borrow_mut("x"));
        assert!(checker.has_errors());

        // Release borrows
        checker.release_borrow("x");
        checker.release_borrow("x");
        checker.clear_errors();

        // Now mutable borrow works
        assert!(checker.try_borrow_mut("x"));

        // Can't borrow immutably while mutably borrowed
        assert!(!checker.try_borrow("x"));
    }

    #[test]
    fn test_alloc_source() {
        let shared_heap = Shared::with_source(42, AllocSource::Heap);
        assert_eq!(shared_heap.alloc_source(), AllocSource::Heap);

        let shared_arena = Shared::with_source(42, AllocSource::Arena);
        assert_eq!(shared_arena.alloc_source(), AllocSource::Arena);
    }
}
