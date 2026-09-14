use std::cell::RefCell;
use std::ptr::NonNull;
use std::alloc::{Layout, alloc_zeroed, dealloc};
/// Advanced memory allocators for AdeshLang
/// - Arena allocators: fast bump allocation in fixed regions
/// - Region allocators: automatic deallocation on scope exit
/// - Stack allocators: temporary stack-like allocation
/// - Per-thread allocators: thread-local allocation caches
use std::sync::{Arc, Mutex};

/// Arena allocator: bump allocation within a pre-allocated, page-aligned buffer.
///
/// The backing memory is allocated via `alloc_zeroed` with a 4096-byte alignment
/// so that the base address itself satisfies the page-alignment contract. This
/// replaces the previous approach of adjusting an offset inside a `Vec`, which
/// did not guarantee the underlying allocation was page-aligned.
pub struct Arena {
    /// Page-aligned base pointer (4096-byte alignment).
    base: NonNull<u8>,
    /// Layout used for deallocation in `Drop`.
    layout: Layout,
    /// Requested logical capacity in bytes.
    requested_capacity: usize,
    /// Total committed usable capacity in bytes (rounded up to 4096-byte page multiple).
    capacity: usize,
    /// Bump cursor (byte offset from `base`).
    cursor: Mutex<usize>,
}

impl std::fmt::Debug for Arena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cursor = self.cursor.lock().map(|c| *c).unwrap_or(0);
        f.debug_struct("Arena")
            .field("base", &format_args!("{:p}", self.base.as_ptr()))
            .field("requested_capacity", &self.requested_capacity)
            .field("allocated_capacity", &self.capacity)
            .field("cursor", &cursor)
            .finish()
    }
}

impl Arena {
    /// Create a new arena with specified capacity and page-aligned base memory.
    ///
    /// The capacity is rounded up to a multiple of 4096 with checked arithmetic
    /// to prevent overflow. The backing allocation uses `Layout` with 4096-byte
    /// alignment, guaranteeing the base pointer itself is page-aligned.
    ///
    /// Both `requested_capacity` (original caller request) and `allocated_capacity`
    /// (the 4096-byte rounded memory commitment) are tracked and exposed.
    pub fn new(capacity: usize) -> Result<Arc<Self>, String> {
        if capacity == 0 {
            return Err("Arena capacity cannot be zero".to_string());
        }

        // Maximum allowed arena capacity: 1 GB.
        // Prevents runaway allocations from overcommitting host virtual memory.
        const MAX_ARENA_CAPACITY: usize = 1024 * 1024 * 1024;
        if capacity > MAX_ARENA_CAPACITY {
            return Err(format!(
                "Requested arena capacity {} exceeds maximum allowed limit ({} bytes)",
                capacity, MAX_ARENA_CAPACITY
            ));
        }

        // Checked alignment rounding — no unchecked size arithmetic.
        let aligned_cap = capacity
            .checked_add(4095)
            .ok_or_else(|| "Arena capacity overflow: capacity + 4095".to_string())?
            & !4095;

        let layout = Layout::from_size_align(aligned_cap, 4096)
            .map_err(|_| format!("Invalid arena layout: size={}, align=4096", aligned_cap))?;

        let base = unsafe {
            let ptr = alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(format!(
                    "Arena allocation failed: {} bytes",
                    aligned_cap
                ));
            }
            NonNull::new_unchecked(ptr)
        };

        Ok(Arc::new(Arena {
            base,
            layout,
            requested_capacity: capacity,
            capacity: aligned_cap,
            cursor: Mutex::new(0),
        }))
    }

    /// Allocate bytes from the arena with explicit alignment validation.
    ///
    /// Since the base is page-aligned (4096), any alignment up to 4096 is
    /// satisfiable by adjusting the cursor within the buffer. Alignments
    /// greater than 4096 cannot be guaranteed by the 4096-aligned base and
    /// are rejected.
    pub fn alloc(&self, size: usize, alignment: usize) -> Result<NonNull<u8>, String> {
        if alignment == 0 || (alignment & (alignment - 1)) != 0 {
            return Err("Alignment must be a non-zero power of 2".to_string());
        }
        if alignment > 4096 {
            return Err(format!(
                "Alignment {} exceeds maximum supported page alignment (4096)",
                alignment
            ));
        }
        let mut cursor = self.cursor.lock().map_err(|e| e.to_string())?;

        let base_addr = self.base.as_ptr() as usize;
        let current_addr = base_addr
            .checked_add(*cursor)
            .ok_or_else(|| "Cursor overflow".to_string())?;

        let aligned_addr = current_addr
            .checked_add(alignment - 1)
            .ok_or_else(|| "Alignment arithmetic overflow".to_string())?
            & !(alignment - 1);
        let aligned_offset = aligned_addr
            .checked_sub(base_addr)
            .ok_or_else(|| "Offset calculation underflow".to_string())?;

        let end_offset = aligned_offset
            .checked_add(size)
            .ok_or_else(|| "Allocation size overflow".to_string())?;

        if end_offset > self.capacity {
            return Err(format!(
                "Arena exhausted: {} + {} > {}",
                aligned_offset, size, self.capacity
            ));
        }

        let ptr = unsafe { self.base.as_ptr().add(aligned_offset) };
        *cursor = end_offset;

        Ok(unsafe { NonNull::new_unchecked(ptr as *mut u8) })
    }

    /// Reset arena to empty state (deallocates all allocations)
    pub fn reset(&self) -> Result<(), String> {
        *self.cursor.lock().map_err(|e| e.to_string())? = 0;
        Ok(())
    }

    /// Get the original requested capacity in bytes passed to `Arena::new`.
    #[inline(always)]
    pub fn requested_capacity(&self) -> usize {
        self.requested_capacity
    }

    /// Get the actual allocated / committed memory capacity in bytes (page-aligned to 4096).
    #[inline(always)]
    pub fn allocated_capacity(&self) -> usize {
        self.capacity
    }

    /// Total usable allocated capacity in bytes (alias for `allocated_capacity`).
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get remaining committed capacity
    pub fn remaining(&self) -> Result<usize, String> {
        let cursor = *self.cursor.lock().map_err(|e| e.to_string())?;
        Ok(self.capacity - cursor)
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.base.as_ptr(), self.layout);
        }
    }
}

// SAFETY: Arena's base pointer and layout are immutable after construction.
// The cursor is protected by a Mutex. Send + Sync are safe.
unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}

/// Region allocator: scope-based allocation with automatic cleanup
///
/// # Scope Invariants
/// Regions are stack-disciplined allocation scopes. A region explicitly tracks
/// its `start_cursor` and `end_cursor`. On `Drop`, if `arena.cursor == self.end_cursor`
/// (confirming this region is the active tail and all child scopes have exited),
/// the arena cursor is rolled back to `start_cursor`. Out-of-order region drops
/// safely defer arena reclamation to parent regions.
pub struct Region {
    allocations: RefCell<Vec<(usize, usize)>>, // (ptr, size) pairs
    arena: Arc<Arena>,
    start_cursor: usize,
    end_cursor: std::cell::Cell<usize>,
}

impl Region {
    /// Create a region within an arena
    pub fn new(arena: Arc<Arena>) -> Self {
        let start_cursor = *arena.cursor.lock().unwrap();
        Region {
            allocations: RefCell::new(Vec::new()),
            arena,
            start_cursor,
            end_cursor: std::cell::Cell::new(start_cursor),
        }
    }

    /// Allocate within this region (auto-freed at region drop)
    pub fn alloc(&self, size: usize, alignment: usize) -> Result<NonNull<u8>, String> {
        let ptr = self.arena.alloc(size, alignment)?;
        let current_cursor = *self.arena.cursor.lock().unwrap();
        self.end_cursor.set(current_cursor);
        self.allocations
            .borrow_mut()
            .push((ptr.as_ptr() as usize, size));
        Ok(ptr)
    }

    /// Get all allocations in this region
    pub fn allocations(&self) -> Vec<(usize, usize)> {
        self.allocations.borrow().clone()
    }

    /// Get start cursor for this region
    #[inline(always)]
    pub fn start_cursor(&self) -> usize {
        self.start_cursor
    }

    /// Get current end cursor for this region
    #[inline(always)]
    pub fn end_cursor(&self) -> usize {
        self.end_cursor.get()
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        if let Ok(mut cursor) = self.arena.cursor.lock() {
            // Strictly check if we are the current tail owner of the arena
            if *cursor == self.end_cursor.get() {
                *cursor = self.start_cursor;
            }
        }
    }
}

/// Stack allocator: LIFO allocation pattern using a single pre-allocated buffer
pub struct StackAllocator {
    buffer: RefCell<Vec<u8>>,
    cursor: RefCell<usize>,
    frames: RefCell<Vec<usize>>, // Saved cursors representing frames
}

impl StackAllocator {
    pub fn new() -> Self {
        StackAllocator {
            buffer: RefCell::new(vec![0u8; 1024 * 1024]), // 1 MB pre-allocated stack
            cursor: RefCell::new(0),
            frames: RefCell::new(Vec::new()),
        }
    }

    /// Push a new frame onto the stack (saves current cursor)
    pub fn push_frame(&self) -> Result<(), String> {
        let current_cursor = *self.cursor.borrow();
        self.frames.borrow_mut().push(current_cursor);
        Ok(())
    }

    /// Pop the current frame (restores saved cursor)
    pub fn pop_frame(&self) -> Result<usize, String> {
        let mut frames = self.frames.borrow_mut();
        if let Some(saved_cursor) = frames.pop() {
            let mut cursor = self.cursor.borrow_mut();
            let freed = *cursor - saved_cursor;
            *cursor = saved_cursor;
            Ok(freed)
        } else {
            Err("Cannot pop frame: stack is empty".to_string())
        }
    }

    /// Allocate aligned memory within the current frame (bumps cursor)
    pub fn alloc_aligned(&self, size: usize, alignment: usize) -> Result<usize, String> {
        if alignment == 0 || (alignment & (alignment - 1)) != 0 {
            return Err("Alignment must be a non-zero power of 2".to_string());
        }
        let frames = self.frames.borrow();
        if frames.is_empty() {
            return Err("Cannot allocate: no active frame".to_string());
        }

        let mut cursor = self.cursor.borrow_mut();
        let buffer = self.buffer.borrow();

        let current_ptr = (buffer.as_ptr() as usize)
            .checked_add(*cursor)
            .ok_or_else(|| "Stack cursor overflow".to_string())?;
        let aligned_ptr = current_ptr
            .checked_add(alignment - 1)
            .ok_or_else(|| "Stack alignment arithmetic overflow".to_string())?
            & !(alignment - 1);
        let padding = aligned_ptr - current_ptr;

        let start = *cursor + padding;
        let end = start
            .checked_add(size)
            .ok_or_else(|| "Allocation size overflow".to_string())?;

        if end > buffer.len() {
            return Err("Stack allocator overflow".to_string());
        }

        *cursor = end;
        Ok(start)
    }

    /// Allocate unaligned/default-aligned bytes within the current frame
    pub fn alloc(&self, size: usize) -> Result<usize, String> {
        self.alloc_aligned(size, 8)
    }
}

impl Default for StackAllocator {
    fn default() -> Self {
        Self::new()
    }
}

const NUM_CLASSES: usize = 9;
const SIZE_CLASSES: [usize; NUM_CLASSES] = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
const MAX_CACHED_BLOCKS_PER_CLASS: usize = 64;

// Per-thread allocator with thread-local cache
thread_local! {
    static THREAD_CACHE: RefCell<[Vec<Vec<u8>>; NUM_CLASSES]> = const {
        RefCell::new([
            Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(),
            Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        ])
    };
}

pub struct ThreadLocalAllocator;

impl ThreadLocalAllocator {
    #[inline]
    fn size_class_idx(size: usize) -> Option<usize> {
        for (idx, &class_size) in SIZE_CLASSES.iter().enumerate() {
            if size <= class_size {
                return Some(idx);
            }
        }
        None
    }

    /// Allocate from thread-local cache
    pub fn alloc(size: usize) -> Vec<u8> {
        if let Some(idx) = Self::size_class_idx(size) {
            THREAD_CACHE.with(|cache| {
                let mut c = cache.borrow_mut();
                if let Some(mut block) = c[idx].pop() {
                    block.resize(size, 0);
                    return block;
                }
                vec![0u8; size]
            })
        } else {
            // Larger than maximum class size, allocate fresh
            vec![0u8; size]
        }
    }

    /// Return block to thread-local cache for reuse (capped per class)
    pub fn free(mut block: Vec<u8>) {
        let capacity = block.capacity();
        if let Some(idx) = Self::size_class_idx(capacity) {
            THREAD_CACHE.with(|cache| {
                let mut c = cache.borrow_mut();
                if c[idx].len() < MAX_CACHED_BLOCKS_PER_CLASS {
                    block.clear();
                    c[idx].push(block);
                }
            });
        }
    }

    /// Clear the thread-local cache
    pub fn clear() {
        THREAD_CACHE.with(|cache| {
            for list in cache.borrow_mut().iter_mut() {
                list.clear();
            }
        });
    }
}

/// Allocator configuration for different scenarios
#[derive(Debug, Clone, Copy)]
pub enum AllocatorMode {
    /// Direct heap allocation (slowest, most flexible)
    Direct,
    /// Arena allocation (fast, fixed capacity)
    Arena,
    /// Stack allocation (fastest, LIFO only)
    Stack,
    /// Thread-local cache (medium speed, thread-local)
    ThreadLocal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_creation() {
        let arena = Arena::new(1024).unwrap();
        assert_eq!(arena.requested_capacity(), 1024);
        assert_eq!(arena.allocated_capacity(), 4096);
        assert_eq!(arena.capacity(), 4096);
        assert!(arena.remaining().is_ok());
    }

    #[test]
    fn test_arena_alloc() {
        let arena = Arena::new(1024).unwrap();
        let ptr = arena.alloc(64, 8);
        assert!(ptr.is_ok());
    }

    #[test]
    fn test_arena_page_aligned() {
        let arena = Arena::new(1024).unwrap();
        let base = arena.base.as_ptr() as usize;
        assert_eq!(base % 4096, 0, "Arena base must be page-aligned (4096)");
    }

    #[test]
    fn test_arena_overflow_check() {
        // Capacity near usize::MAX must fail via checked_add, not silently wrap.
    let result = Arena::new(usize::MAX);
        assert!(result.is_err());
    }

    #[test]
    fn test_arena_exhaustion() {
        // Capacity 64 rounds up to 4096 (page-aligned minimum).
        let arena = Arena::new(64).unwrap();
        let _ptr1 = arena.alloc(4090, 1).unwrap();
        let ptr2 = arena.alloc(64, 1);
        assert!(ptr2.is_err());
    }

    #[test]
    fn test_region_alloc() {
        let arena = Arena::new(1024).unwrap();
        let region = Region::new(arena);
        let ptr = region.alloc(64, 8);
        assert!(ptr.is_ok());
    }

    #[test]
    fn test_arena_alignment_limits() {
        let arena = Arena::new(16384).unwrap();
        assert!(arena.alloc(64, 1).is_ok());
        assert!(arena.alloc(64, 8).is_ok());
        assert!(arena.alloc(64, 64).is_ok());
        assert!(arena.alloc(64, 4096).is_ok());

        // Alignment > 4096 must be rejected
        assert!(arena.alloc(64, 8192).is_err());
        assert!(arena.alloc(64, 16384).is_err());

        // Non-power of two or 0 must be rejected
        assert!(arena.alloc(64, 0).is_err());
        assert!(arena.alloc(64, 3).is_err());
    }

    #[test]
    fn test_arena_max_capacity_limit() {
        // Requesting 2 GB should exceed the 1 GB maximum capacity limit
        let res = Arena::new(2 * 1024 * 1024 * 1024);
        assert!(res.is_err());
    }

    #[test]
    fn test_region_nesting_stack_discipline() {
        let arena = Arena::new(8192).unwrap();
        assert_eq!(*arena.cursor.lock().unwrap(), 0);

        {
            let outer_region = Region::new(arena.clone());
            let _p1 = outer_region.alloc(128, 8).unwrap();
            let after_outer = *arena.cursor.lock().unwrap();
            assert!(after_outer >= 128);

            {
                let inner_region = Region::new(arena.clone());
                let _p2 = inner_region.alloc(256, 8).unwrap();
                let after_inner = *arena.cursor.lock().unwrap();
                assert!(after_inner >= after_outer + 256);
            }
            // Inner region dropped — should roll cursor back to outer region's end
            assert_eq!(*arena.cursor.lock().unwrap(), after_outer);
        }
        // Outer region dropped — should roll cursor back to 0
        assert_eq!(*arena.cursor.lock().unwrap(), 0);
    }

    #[test]
    fn test_stack_allocator() {
        let alloc = StackAllocator::new();
        alloc.push_frame().unwrap();
        let offset = alloc.alloc(32).unwrap();
        assert_eq!(offset, 0);
        let freed = alloc.pop_frame().unwrap();
        assert!(freed > 0);
    }

    #[test]
    fn test_thread_local_alloc() {
        let block = ThreadLocalAllocator::alloc(64);
        assert_eq!(block.len(), 64);
        ThreadLocalAllocator::free(block);
        ThreadLocalAllocator::clear();
    }
}
