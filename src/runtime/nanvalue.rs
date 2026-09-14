//! NaN-Boxing Value Implementation
//!
//! This module implements a space-efficient value representation using NaN-boxing,
//! reducing the Value size from ~40 bytes to 8 bytes.
//!
//! ## Encoding Scheme
//!
//! We use IEEE 754 double-precision floating-point quiet NaN patterns to encode non-float types:
//!
//! ```text
//! Regular doubles & infinities:
//!   - Non-NaN floats and infinities (+Inf, -Inf) are stored as-is with full 64-bit precision.
//!   - Float NaNs are canonicalized to 0x7FF8_0000_0000_0000.
//!
//! Tagged quiet NaN patterns (high 16 bits in 0xFFF8..0xFFFF):
//!   - High 16-bits in range 0xFFF8..0xFFFF guarantee bit 51 = 1 (quiet NaN), avoiding any collision
//!     with -Infinity (0xFFF0_0000_0000_0000) or valid float numbers.
//!   - Tag 0xFFFA... => 48-bit Object Handle (index + generation, registered in ObjectRegistry)
//!   - Tag 0xFFF8... => 48-bit signed integer (-140 trillion to +140 trillion)
//!   - Sub-tagged types under 0xFFFB... => Exact fixed-width integer/float types
//! ```
//!
//! ## Object Handle Packing (48-bit payload)
//!
//! ```text
//!   Tag (16 bits)  | Generation (16 bits) | Index (32 bits)
//!   0xFFFA         | gen                  | index
//! ```
//!
//! The 32-bit index allows up to 4 billion slots. The 16-bit generation detects
//! stale handles after ID recycling (ABA prevention). When a slot's generation
//! reaches `u16::MAX` (65535), the slot is permanently retired rather than
//! wrapping, ensuring ABA protection is never compromised. With 4 billion
//! slots, retiring a few exhausted ones is negligible.
//!
//! ## Memory Safety & Ownership
//!
//! - Heap values are managed by a chunked-slab ObjectRegistry with atomic reference counting.
//! - Object handles carry a generation to detect stale references after ID recycling.
//! - Clone/drop are lock-free: they use compare-and-swap on a packed (generation, refcount)
//!   atomic state word, avoiding hash-table locks on the hottest paths.
//! - Value insertion/removal is protected by per-shard RwLocks; the lock-free refcount
//!   path never touches them.

use num_bigint::BigInt;
#[cfg(test)]
use num_traits::ToPrimitive;
use std::sync::Arc;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use std::sync::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU32, AtomicUsize, Ordering};
use std::alloc::Layout;

// ============================================
// CONSTANTS
// ============================================

const SHARD_COUNT: usize = 32;
const CHUNK_SIZE: usize = 4096;

/// NaN-boxed value representation - always exactly 8 bytes
#[repr(transparent)]
pub struct NanValue(u64);

// 16-bit Tag constants using quiet NaN range (0xFFF8 to 0xFFFF)
const TAG_I64: u64     = 0xFFF8_0000_0000_0000;
const TAG_U64: u64     = 0xFFF9_0000_0000_0000;
const TAG_POINTER: u64 = 0xFFFA_0000_0000_0000;
const TAG_NULL: u64    = 0xFFFC_0000_0000_0000;
const TAG_FALSE: u64   = 0xFFFD_0000_0000_0000;
const TAG_TRUE: u64    = 0xFFFD_0000_0000_0001;
const TAG_CHAR: u64    = 0xFFFE_0000_0000_0000;

// Sub-tagged small types under 0xFFFB
const TAG_I8: u64      = 0xFFFB_0100_0000_0000;
const TAG_I16: u64    = 0xFFFB_0200_0000_0000;
const TAG_I32: u64    = 0xFFFB_0300_0000_0000;
const TAG_U8: u64     = 0xFFFB_0400_0000_0000;
const TAG_U16: u64    = 0xFFFB_0500_0000_0000;
const TAG_U32: u64    = 0xFFFB_0600_0000_0000;
const TAG_F32: u64    = 0xFFFB_0700_0000_0000;
const TAG_I128: u64   = 0xFFFB_0800_0000_0000;
const TAG_U128: u64   = 0xFFFB_0900_0000_0000;

// Masks for tag and payload extractions
const TAG_16_MASK: u64         = 0xFFFF_0000_0000_0000;
const TAG_24_MASK: u64         = 0xFFFF_FF00_0000_0000;
const PAYLOAD_48_MASK: u64     = 0x0000_FFFF_FFFF_FFFF;
const PAYLOAD_40_MASK: u64     = 0x0000_00FF_FFFF_FFFF;

// Handle packing: 32-bit index + 16-bit generation in the 48-bit payload
const INDEX_MASK: u64   = 0x0000_0000_FFFF_FFFF; // lower 32 bits of payload
const GEN_SHIFT: u32     = 32;                     // generation is in bits 32-47 of payload
// GEN_MASK omitted — not needed; generation is extracted via shift + mask inline.

// State packing for chunk slots: 16-bit generation (high) + 1 DYING bit + 47-bit refcount (low)
//
// Lifecycle states encoded in the AtomicU64 state word:
//
//   FREE:   (gen, 0, 0)       — slot is on the free list, no value
//   LIVE:   (gen, 0, refs>0)  — slot holds a value with active references
//   DYING:  (gen, 1, 0)       — last ref dropped, cleanup in progress under write lock
//
// Transitions:
//   LIVE → DYING:  CAS (gen, 0, refs=1) → (gen, DYING, 0)   [in Drop, lock-free]
//   DYING → FREE:  store (gen+1, 0, 0)                        [in cleanup, under write lock]
//   FREE → LIVE:   store (new_gen, 0, 1)                      [in from_heap, under write lock]
const STATE_GEN_SHIFT: u32 = 48;
const STATE_DYING_BIT: u64 = 0x0000_8000_0000_0000; // bit 47
const STATE_REFCOUNT_MASK: u64 = 0x0000_7FFF_FFFF_FFFF; // 47-bit refcount (max ~140 trillion)
const MAX_REFCOUNT: u64 = STATE_REFCOUNT_MASK;

// 48-bit integer bounds (for TAG_I64 / TAG_U64)
const INT48_MAX: i64 = (1i64 << 47) - 1;
const INT48_MIN: i64 = -(1i64 << 47);

// 40-bit integer bounds (for sub-tagged types under 0xFFFB, e.g. TAG_I128 / TAG_U128)
// Top 24 bits are reserved for 0xFFFB_XX tag, leaving 40 bits of inline payload.
const INT40_MAX: i64 = (1i64 << 39) - 1;
const INT40_MIN: i64 = -(1i64 << 39);
const UINT40_MAX: u64 = (1u64 << 40) - 1;

// ============================================
// HANDLE / STATE HELPERS
// ============================================

/// Pack an object handle (index + generation) into the 48-bit payload.
#[inline(always)]
fn pack_handle(index: u32, generation: u16) -> u64 {
    ((generation as u64) << GEN_SHIFT) | (index as u64)
}

/// Extract (index, generation) from a full 64-bit NanValue word.
#[inline(always)]
fn extract_handle_from_bits(bits: u64) -> (u32, u16) {
    let payload = bits & PAYLOAD_48_MASK;
    let index = (payload & INDEX_MASK) as u32;
    let generation = ((payload >> GEN_SHIFT) & 0xFFFF) as u16;
    (index, generation)
}

/// Pack a chunk-slot state: generation (high 16) + DYING bit + refcount (low 47).
#[inline(always)]
fn pack_state(generation: u16, refcount: u64) -> u64 {
    ((generation as u64) << STATE_GEN_SHIFT) | (refcount & STATE_REFCOUNT_MASK)
}

/// Pack a DYING state: generation + DYING bit, refcount 0.
#[inline(always)]
fn pack_dying_state(generation: u16) -> u64 {
    ((generation as u64) << STATE_GEN_SHIFT) | STATE_DYING_BIT
}

/// Unpack a chunk-slot state into (generation, is_dying, refcount).
#[inline(always)]
fn unpack_state(state: u64) -> (u16, bool, u64) {
    let generation = (state >> STATE_GEN_SHIFT) as u16;
    let is_dying = (state & STATE_DYING_BIT) != 0;
    let refcount = state & STATE_REFCOUNT_MASK;
    (generation, is_dying, refcount)
}

/// Check if a slot state represents a LIVE object with the given generation.
#[inline(always)]
fn is_live(state: u64, generation: u16) -> bool {
    let (slot_gen, is_dying, refcount) = unpack_state(state);
    slot_gen == generation && !is_dying && refcount > 0
}

// ============================================
// CHUNKED SLAB OBJECT REGISTRY
// ============================================

/// A chunk of 4096 object slots, each with an atomic state word.
///
/// Allocated on the heap via `alloc_zeroed` — the address is stable for the
/// lifetime of the program. Once allocated, a chunk is never freed or moved,
/// so raw pointers to slots remain valid without holding any lock.
struct Chunk {
    states: [AtomicU64; CHUNK_SIZE],
}

impl Chunk {
    fn new() -> Box<Chunk> {
        // SAFETY: AtomicU64 with all-zero bits is a valid atomic with value 0.
        // We allocate directly on the heap to avoid a 32 KB stack temporary.
        unsafe {
            let layout = Layout::new::<Chunk>();
            let ptr = std::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            Box::from_raw(ptr as *mut Chunk)
        }
    }
}

/// A free-list entry for a recycled object slot.
struct FreeEntry {
    index: u32,
    next_generation: u16,
}

/// Per-shard data: value storage and free list.
struct Shard {
    values: RwLock<HashMap<u32, Arc<HeapValue>>>,
    free_list: Mutex<Vec<FreeEntry>>,
}

/// Global object registry using a chunked-slab design.
///
/// - **Chunks** hold atomic (generation, refcount) state words, indexed by
///   `index / CHUNK_SIZE` (chunk) and `index % CHUNK_SIZE` (offset).
/// - **Shards** hold the actual `Arc<HeapValue>` values and free lists, indexed
///   by `index % SHARD_COUNT`.
/// - Clone/drop access only the chunk state (lock-free CAS).
/// - `as_heap`/`from_heap`/cleanup access the shard's value HashMap (RwLock).
struct ObjectRegistry {
    /// Global chunks — indexed by `index / CHUNK_SIZE`.
    /// Each `Box<Chunk>` has a stable heap address; once pushed, never removed.
    chunks: RwLock<Vec<Box<Chunk>>>,
    /// Per-shard value storage and free lists.
    shards: [Shard; SHARD_COUNT],
    /// Global monotonic index counter (never recycled directly — only via free list).
    next_id: AtomicU32,
    /// Round-robin counter for shard selection in `from_heap`.
    round_robin: AtomicUsize,
}

static REGISTRY: Lazy<ObjectRegistry> = Lazy::new(|| {
    let shards = std::array::from_fn(|_| Shard {
        values: RwLock::new(HashMap::new()),
        free_list: Mutex::new(Vec::new()),
    });
    // Pre-allocate the first chunk so the common case never needs the write lock.
    let chunks = vec![Chunk::new()];
    ObjectRegistry {
        chunks: RwLock::new(chunks),
        shards,
        next_id: AtomicU32::new(1), // 0 is reserved as "null/empty"
        round_robin: AtomicUsize::new(0),
    }
});

impl ObjectRegistry {
    /// Get a raw pointer to the chunk containing `index`.
    ///
    /// Takes a brief read lock on the chunks Vec, then returns a raw pointer.
    /// The pointer remains valid after the lock is released because chunks
    /// are heap-allocated and never freed.
    fn get_chunk(&self, index: u32) -> Option<*const Chunk> {
        let chunk_idx = (index as usize) / CHUNK_SIZE;
        let chunks = self.chunks.read().unwrap();
        chunks.get(chunk_idx).map(|c| &**c as *const Chunk)
    }

    /// Ensure the chunk for `index` exists, allocating if necessary.
    fn ensure_chunk(&self, index: u32) {
        let chunk_idx = (index as usize) / CHUNK_SIZE;
        // Fast path: chunk already exists (read lock only).
        {
            let chunks = self.chunks.read().unwrap();
            if chunk_idx < chunks.len() {
                return;
            }
        }
        // Slow path: allocate new chunk(s) under write lock.
        let mut chunks = self.chunks.write().unwrap();
        while chunks.len() <= chunk_idx {
            chunks.push(Chunk::new());
        }
    }

    /// Try to atomically increment the refcount of an object slot.
    ///
    /// Centralized CAS validation used by `Clone`, `try_from_bits()`, and all runtime retain paths:
    /// 1. Verifies the chunk exists for `index`.
    /// 2. Verifies the slot's generation matches `generation`.
    /// 3. Rejects slots in `DYING` state or with `refcount == 0` (freed).
    /// 4. Rejects when `refcount >= MAX_REFCOUNT` (overflow protection).
    /// 5. Performs a lock-free compare-exchange loop to increment refcount by 1.
    pub fn try_increment_ref(&self, index: u32, generation: u16) -> Result<(), IncrementRefError> {
        let chunk_ptr = self.get_chunk(index).ok_or(IncrementRefError::InvalidIndex)?;
        let offset = (index as usize) % CHUNK_SIZE;

        loop {
            let state = unsafe { (*chunk_ptr).states[offset].load(Ordering::Acquire) };
            let (slot_gen, is_dying, refcount) = unpack_state(state);
            if slot_gen != generation || is_dying || refcount == 0 {
                return Err(IncrementRefError::StaleOrFreed {
                    slot_gen,
                    is_dying,
                    refcount,
                });
            }
            if refcount >= MAX_REFCOUNT {
                return Err(IncrementRefError::RefcountOverflow);
            }
            let new_state = pack_state(generation, refcount + 1);
            match unsafe {
                (*chunk_ptr).states[offset].compare_exchange_weak(
                    state,
                    new_state,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
            } {
                Ok(_) => return Ok(()),
                Err(_) => continue,
            }
        }
    }
}

/// Errors returned when allocating an object in [`ObjectRegistry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocationError {
    /// Object index space exhausted (4 billion slots used).
    IndexSpaceExhausted,
}

impl std::fmt::Display for AllocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllocationError::IndexSpaceExhausted => {
                write!(f, "Object index space exhausted (4 billion slots used)")
            }
        }
    }
}

impl std::error::Error for AllocationError {}

/// Errors returned by [`ObjectRegistry::try_increment_ref`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncrementRefError {
    /// No chunk allocated for the specified slot index.
    InvalidIndex,
    /// Slot is stale (generation mismatch), dying, or already freed.
    StaleOrFreed {
        slot_gen: u16,
        is_dying: bool,
        refcount: u64,
    },
    /// Refcount has reached the 47-bit maximum limit (`MAX_REFCOUNT`).
    RefcountOverflow,
}

/// A RAII guard representing a borrowed reference to a heap-allocated `HeapValue`.
///
/// Ensures the underlying registry slot and its `Arc<HeapValue>` payload are
/// observed safely with borrow-lifetime checks without copying or leaking raw pointers.
pub struct HeapRef<'a> {
    _owner: &'a NanValue,
    value: Arc<HeapValue>,
}

impl<'a> HeapRef<'a> {
    #[inline(always)]
    pub fn get(&self) -> &HeapValue {
        &self.value
    }
}

impl<'a> std::ops::Deref for HeapRef<'a> {
    type Target = HeapValue;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a> std::fmt::Debug for HeapRef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HeapRef({:?})", self.value.as_ref())
    }
}

// ============================================
// HEAP VALUE
// ============================================

/// Heap-allocated values that don't fit in the NaN-box payload
#[derive(Clone, Debug)]
pub enum HeapValue {
    String(String),
    BigInt(BigInt),
    Array(Vec<NanValue>),
    Object(std::collections::HashMap<String, NanValue>),
}

// ============================================
// NANVALUE IMPLEMENTATION
// ============================================

impl NanValue {
    // ----------------------------------------
    // Constructors — primitive types
    // ----------------------------------------

    /// Create a NanValue from a 64-bit floating-point number, using canonical NaN representations
    #[inline(always)]
    pub fn from_f64(n: f64) -> Self {
        if n.is_nan() {
            NanValue(0x7FF8_0000_0000_0000)
        } else {
            NanValue(n.to_bits())
        }
    }

    /// Create a NanValue representing null
    #[inline(always)]
    pub fn null() -> Self {
        NanValue(TAG_NULL)
    }

    /// Create a NanValue from a boolean
    #[inline(always)]
    pub fn from_bool(b: bool) -> Self {
        if b {
            NanValue(TAG_TRUE)
        } else {
            NanValue(TAG_FALSE)
        }
    }

    /// Create type-preserving boxed integers
    #[inline(always)]
    pub fn from_i8(n: i8) -> Self {
        NanValue(TAG_I8 | ((n as u8) as u64))
    }

    #[inline(always)]
    pub fn from_i16(n: i16) -> Self {
        NanValue(TAG_I16 | ((n as u16) as u64))
    }

    #[inline(always)]
    pub fn from_i32(n: i32) -> Self {
        NanValue(TAG_I32 | ((n as u32) as u64))
    }

    #[inline]
    pub fn from_i64(i: i64) -> Option<Self> {
        if i >= INT48_MIN && i <= INT48_MAX {
            let payload = (i as u64) & PAYLOAD_48_MASK;
            Some(NanValue(TAG_I64 | payload))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn from_u8(n: u8) -> Self {
        NanValue(TAG_U8 | (n as u64))
    }

    #[inline(always)]
    pub fn from_u16(n: u16) -> Self {
        NanValue(TAG_U16 | (n as u64))
    }

    #[inline(always)]
    pub fn from_u32(n: u32) -> Self {
        NanValue(TAG_U32 | (n as u64))
    }

    #[inline]
    pub fn from_u64(n: u64) -> Option<Self> {
        if n <= PAYLOAD_48_MASK {
            Some(NanValue(TAG_U64 | n))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn from_f32(n: f32) -> Self {
        NanValue(TAG_F32 | (n.to_bits() as u64))
    }

    #[inline]
    pub fn from_i128(n: i128) -> Option<Self> {
        if n >= INT40_MIN as i128 && n <= INT40_MAX as i128 {
            let payload = (n as u64) & PAYLOAD_40_MASK;
            Some(NanValue(TAG_I128 | payload))
        } else {
            None
        }
    }

    #[inline]
    pub fn from_u128(n: u128) -> Option<Self> {
        if n <= UINT40_MAX as u128 {
            Some(NanValue(TAG_U128 | ((n as u64) & PAYLOAD_40_MASK)))
        } else {
            None
        }
    }

    /// Create a NanValue from an i128.
    ///
    /// Values that fit in 40 signed bits (-549,755,813,888 to 549,755,813,887)
    /// are stored inline without heap allocation under `TAG_I128`. Larger values
    /// are automatically allocated on the heap as `HeapValue::BigInt`.
    #[inline]
    pub fn from_int128(n: i128) -> Self {
        if let Some(v) = Self::from_i128(n) {
            v
        } else {
            Self::from_heap(HeapValue::BigInt(BigInt::from(n)))
        }
    }

    /// Create a NanValue from a u128.
    ///
    /// Values that fit in 40 unsigned bits (0 to 1,099,511,627,775)
    /// are stored inline without heap allocation under `TAG_U128`. Larger values
    /// are automatically allocated on the heap as `HeapValue::BigInt`.
    #[inline]
    pub fn from_uint128(n: u128) -> Self {
        if let Some(v) = Self::from_u128(n) {
            v
        } else {
            Self::from_heap(HeapValue::BigInt(BigInt::from(n)))
        }
    }

    /// Create a NanValue from a 48-bit signed integer (Legacy API compatibility)
    #[inline]
    pub fn from_i48(i: i64) -> Option<Self> {
        Self::from_i64(i)
    }

    /// Create a NanValue from an integer known to fit in 48 bits (Legacy API compatibility)
    #[inline]
    pub fn from_i48_unchecked(i: i64) -> Self {
        debug_assert!(i >= INT48_MIN && i <= INT48_MAX);
        let payload = (i as u64) & PAYLOAD_48_MASK;
        NanValue(TAG_I64 | payload)
    }

    /// Create a NanValue from any integer, using BigInt if necessary
    #[inline]
    pub fn from_int(i: i64) -> Self {
        if let Some(v) = Self::from_i64(i) {
            v
        } else {
            Self::from_heap(HeapValue::BigInt(BigInt::from(i)))
        }
    }

    /// Create a NanValue from a 32-bit Unicode character
    #[inline]
    pub fn from_char(c: char) -> Self {
        let code = c as u32 as u64;
        NanValue(TAG_CHAR | code)
    }

    // ----------------------------------------
    // Constructor — heap values
    // ----------------------------------------

    /// Try to create a NanValue from a heap-allocated value, returning an error if
    /// the registry index space is exhausted.
    ///
    /// Uses round-robin shard selection for the free list, only looking in
    /// one shard's free list (instead of scanning all 32). This reduces
    /// contention and makes the shard invariant explicit.
    pub fn try_from_heap(heap: HeapValue) -> Result<Self, AllocationError> {
        let arc_val = Arc::new(heap);

        // Round-robin shard selection — only look in ONE shard's free list.
        let target_shard = REGISTRY.round_robin.fetch_add(1, Ordering::Relaxed) % SHARD_COUNT;
        let recycled = REGISTRY.shards[target_shard]
            .free_list
            .try_lock()
            .ok()
            .and_then(|mut fl| fl.pop());

        let (index, generation) = match recycled {
            Some(entry) => (entry.index, entry.next_generation),
            None => {
                // Monotonically allocate index from next_id with CAS to prevent wrap-around.
                let mut current = REGISTRY.next_id.load(Ordering::Relaxed);
                loop {
                    if current == 0 || current == u32::MAX {
                        return Err(AllocationError::IndexSpaceExhausted);
                    }
                    match REGISTRY.next_id.compare_exchange_weak(
                        current,
                        current + 1,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(old) => {
                            break (old, 0u16);
                        }
                        Err(actual) => current = actual,
                    }
                }
            }
        };

        let shard_idx = (index % (SHARD_COUNT as u32)) as usize;

        // Ensure the chunk exists (allocates under write lock if needed).
        REGISTRY.ensure_chunk(index);
        let chunk_ptr = REGISTRY
            .get_chunk(index)
            .expect("chunk must exist after ensure_chunk");
        let offset = (index as usize) % CHUNK_SIZE;

        // Insert value under the shard's write lock, then set the chunk state.
        // The state is set AFTER the value is inserted so that any thread that
        // sees refcount > 0 is guaranteed to find the value in the HashMap.
        {
            let mut values = REGISTRY.shards[shard_idx].values.write().unwrap();
            values.insert(index, arc_val);
        }
        // Atomic store — no lock needed for the chunk state.
        unsafe {
            (*chunk_ptr).states[offset].store(pack_state(generation, 1), Ordering::Release);
        }

        Ok(NanValue(TAG_POINTER | pack_handle(index, generation)))
    }

    /// Create a NanValue from a heap-allocated value, panicking if index space is exhausted.
    pub fn from_heap(heap: HeapValue) -> Self {
        match Self::try_from_heap(heap) {
            Ok(v) => v,
            Err(e) => panic!("{}", e),
        }
    }

    // ----------------------------------------
    // Type classifiers
    // ----------------------------------------

    /// Check if this value is a regular double or float (not tagged NaN-boxed).
    ///
    /// This is the fast classifier: anything whose top 16 bits are not in the
    /// tagged range 0xFFF8..0xFFFF is treated as a double. For strict validation
    /// of arbitrary bit patterns (e.g. from `from_bits_unchecked`), use
    /// [`is_valid`](Self::is_valid) instead.
    #[inline(always)]
    pub fn is_double(&self) -> bool {
        (self.0 & 0xFFF8_0000_0000_0000) != 0xFFF8_0000_0000_0000
    }

    /// Strictly validate that this NanValue has a well-formed bit pattern.
    ///
    /// Returns `false` for bit patterns that don't correspond to any valid
    /// value type — unknown tags, invalid payloads, or malformed sub-tags.
    /// Use this to validate values from untrusted sources (e.g. `from_bits_unchecked`).
    pub fn is_valid(&self) -> bool {
        let top16 = (self.0 >> 48) as u16;
        if top16 >= 0xFFF8 {
            // Tagged value — validate against known tags
            match top16 {
                0xFFF8 => true, // TAG_I64
                0xFFF9 => true, // TAG_U64
                0xFFFA => true, // TAG_POINTER (object handle)
                0xFFFB => {
                    // Sub-tagged types — validate the 24-bit tag
                    let top24 = (self.0 >> 40) as u32;
                    matches!(
                        top24,
                        0xFFFB01 | 0xFFFB02 | 0xFFFB03 |
                        0xFFFB04 | 0xFFFB05 | 0xFFFB06 |
                        0xFFFB07 | 0xFFFB08 | 0xFFFB09
                    )
                }
                0xFFFC => self.0 == TAG_NULL, // TAG_NULL — payload must be exactly 0
                0xFFFD => self.0 == TAG_TRUE || self.0 == TAG_FALSE, // TAG_BOOL
                0xFFFE => {
                    // TAG_CHAR — validate the char code
                    let code = (self.0 & PAYLOAD_48_MASK) as u32;
                    char::from_u32(code).is_some()
                }
                _ => false, // Unknown tag in 0xFFF8..0xFFFF range (e.g. 0xFFFF)
            }
        } else {
            // Untagged — all 64-bit patterns are valid f64 representations
            true
        }
    }

    /// Check if this value is null
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        (self.0 & TAG_16_MASK) == TAG_NULL
    }

    /// Check if this value is a boolean
    #[inline(always)]
    pub fn is_bool(&self) -> bool {
        self.0 == TAG_TRUE || self.0 == TAG_FALSE
    }

    /// Check if this value is a character
    #[inline(always)]
    pub fn is_char(&self) -> bool {
        (self.0 & TAG_16_MASK) == TAG_CHAR
    }

    /// Check if this value is an object pointer
    #[inline(always)]
    pub fn is_pointer(&self) -> bool {
        (self.0 & TAG_16_MASK) == TAG_POINTER
    }

    /// Type-checking methods for individual integer/float types
    #[inline(always)] pub fn is_i8(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_I8 }
    #[inline(always)] pub fn is_i16(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_I16 }
    #[inline(always)] pub fn is_i32(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_I32 }
    #[inline(always)] pub fn is_i64(&self) -> bool { (self.0 & TAG_16_MASK) == TAG_I64 }
    #[inline(always)] pub fn is_u8(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_U8 }
    #[inline(always)] pub fn is_u16(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_U16 }
    #[inline(always)] pub fn is_u32(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_U32 }
    #[inline(always)] pub fn is_u64(&self) -> bool { (self.0 & TAG_16_MASK) == TAG_U64 }
    #[inline(always)] pub fn is_f32(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_F32 }
    #[inline(always)] pub fn is_i128(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_I128 }
    #[inline(always)] pub fn is_u128(&self) -> bool { (self.0 & TAG_24_MASK) == TAG_U128 }

    /// Check if this value is any boxed integer type
    #[inline(always)]
    pub fn is_int48(&self) -> bool {
        self.is_i64() || self.is_u64() || self.is_i8() || self.is_i16() || self.is_i32() ||
        self.is_u8() || self.is_u16() || self.is_u32() || self.is_i128() || self.is_u128()
    }

    // ----------------------------------------
    // Value extractors
    // ----------------------------------------

    /// Extract the double value (if it is one)
    #[inline(always)]
    pub fn as_f64(&self) -> Option<f64> {
        if self.is_double() {
            Some(f64::from_bits(self.0))
        } else {
            None
        }
    }

    /// Extract the boolean value (if it is one)
    #[inline(always)]
    pub fn as_bool(&self) -> Option<bool> {
        if self.0 == TAG_TRUE {
            Some(true)
        } else if self.0 == TAG_FALSE {
            Some(false)
        } else {
            None
        }
    }

    /// Getters for type-preserving values
    #[inline(always)] pub fn as_i8(&self) -> Option<i8> { if self.is_i8() { Some((self.0 & PAYLOAD_40_MASK) as u8 as i8) } else { None } }
    #[inline(always)] pub fn as_i16(&self) -> Option<i16> { if self.is_i16() { Some((self.0 & PAYLOAD_40_MASK) as u16 as i16) } else { None } }
    #[inline(always)] pub fn as_i32(&self) -> Option<i32> { if self.is_i32() { Some((self.0 & PAYLOAD_40_MASK) as u32 as i32) } else { None } }

    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.is_i64() {
            let payload = self.0 & PAYLOAD_48_MASK;
            let sign_bit = payload & (1 << 47);
            let extended = if sign_bit != 0 {
                payload | (!PAYLOAD_48_MASK)
            } else {
                payload
            };
            Some(extended as i64)
        } else {
            None
        }
    }

    #[inline(always)] pub fn as_u8(&self) -> Option<u8> { if self.is_u8() { Some((self.0 & PAYLOAD_40_MASK) as u8) } else { None } }
    #[inline(always)] pub fn as_u16(&self) -> Option<u16> { if self.is_u16() { Some((self.0 & PAYLOAD_40_MASK) as u16) } else { None } }
    #[inline(always)] pub fn as_u32(&self) -> Option<u32> { if self.is_u32() { Some((self.0 & PAYLOAD_40_MASK) as u32) } else { None } }
    #[inline(always)] pub fn as_u64(&self) -> Option<u64> { if self.is_u64() { Some(self.0 & PAYLOAD_48_MASK) } else { None } }
    #[inline(always)] pub fn as_f32(&self) -> Option<f32> { if self.is_f32() { Some(f32::from_bits((self.0 & PAYLOAD_40_MASK) as u32)) } else { None } }

    #[inline]
    pub fn as_i128(&self) -> Option<i128> {
        if self.is_i128() {
            let payload = self.0 & PAYLOAD_40_MASK;
            let sign_bit = payload & (1 << 39);
            let extended = if sign_bit != 0 {
                payload | (!PAYLOAD_40_MASK)
            } else {
                payload
            };
            Some(extended as i64 as i128)
        } else {
            None
        }
    }

    #[inline(always)] pub fn as_u128(&self) -> Option<u128> { if self.is_u128() { Some((self.0 & PAYLOAD_40_MASK) as u128) } else { None } }

    /// Extract any boxed integer value as i64 (Legacy API compatibility)
    #[inline]
    pub fn as_i48(&self) -> Option<i64> {
        if let Some(i) = self.as_i64() {
            Some(i)
        } else if let Some(u) = self.as_u64() {
            Some(u as i64)
        } else if let Some(i) = self.as_i32() {
            Some(i as i64)
        } else if let Some(u) = self.as_u32() {
            Some(u as i64)
        } else if let Some(i) = self.as_i16() {
            Some(i as i64)
        } else if let Some(u) = self.as_u16() {
            Some(u as i64)
        } else if let Some(i) = self.as_i8() {
            Some(i as i64)
        } else if let Some(u) = self.as_u8() {
            Some(u as i64)
        } else if let Some(i) = self.as_i128() {
            Some(i as i64)
        } else if let Some(u) = self.as_u128() {
            Some(u as i64)
        } else {
            None
        }
    }

    /// Extract the character value (if it is one)
    #[inline]
    pub fn as_char(&self) -> Option<char> {
        if self.is_char() {
            let code = (self.0 & PAYLOAD_48_MASK) as u32;
            char::from_u32(code)
        } else {
            None
        }
    }

    /// Get a reference to the heap value (if it is one).
    ///
    /// Uses a double-check pattern: first verifies the generation via the
    /// chunk state, then takes the shard's read lock and re-checks. This
    /// ensures the value can't be freed between the check and the lookup.
    pub fn as_heap(&self) -> Option<Arc<HeapValue>> {
        if !self.is_pointer() {
            return None;
        }
        let (index, generation) = extract_handle_from_bits(self.0);
        let shard_idx = (index % (SHARD_COUNT as u32)) as usize;

        // First check: is the slot LIVE with our generation? (no lock)
        let chunk_ptr = REGISTRY.get_chunk(index)?;
        let offset = (index as usize) % CHUNK_SIZE;
        let state = unsafe { (*chunk_ptr).states[offset].load(Ordering::Acquire) };
        if !is_live(state, generation) {
            return None; // Stale, dying, or freed
        }

        // Take the shard's read lock — prevents cleanup (DYING→FREE) from running.
        let values = REGISTRY.shards[shard_idx].values.read().unwrap();

        // Re-check: the slot must still be LIVE (not DYING) under the read lock.
        let state = unsafe { (*chunk_ptr).states[offset].load(Ordering::Acquire) };
        if !is_live(state, generation) {
            return None;
        }

        values.get(&index).cloned()
    }

    /// Borrow the heap value as a `HeapRef` guard tied to this `NanValue`'s lifetime.
    pub fn as_heap_ref(&self) -> Option<HeapRef<'_>> {
        self.as_heap().map(|arc| HeapRef {
            _owner: self,
            value: arc,
        })
    }

    /// Get the raw u64 bits (for debugging)
    #[inline(always)]
    pub fn as_bits(&self) -> u64 {
        self.0
    }

    // ----------------------------------------
    // Raw bit construction
    // ----------------------------------------

    /// Create from raw bits without validation (unsafe)
    #[inline(always)]
    pub unsafe fn from_bits_unchecked(bits: u64) -> Self {
        NanValue(bits)
    }

    /// Legacy / backward-compatible unsafe from_bits
    #[inline(always)]
    pub unsafe fn from_bits(bits: u64) -> Self {
        unsafe { Self::from_bits_unchecked(bits) }
    }

    /// Create from raw bits with full object registry and tag validation.
    ///
    /// For pointer values, this claims a reference (atomically increments the
    /// refcount) only if the generation matches and the slot is live. The
    /// returned NanValue owns this reference and will decrement it on drop.
    pub fn try_from_bits(bits: u64) -> Result<Self, String> {
        // Non-pointer: validate tag structure.
        if (bits & TAG_16_MASK) != TAG_POINTER {
            let val = NanValue(bits);
            if val.is_valid() {
                Ok(val)
            } else {
                Err(format!("Invalid NanValue bit pattern: 0x{:016X}", bits))
            }
        } else {
            // Pointer: claim a reference via centralized try_increment_ref.
            let (index, generation) = extract_handle_from_bits(bits);
            match REGISTRY.try_increment_ref(index, generation) {
                Ok(()) => Ok(NanValue(bits)),
                Err(IncrementRefError::InvalidIndex) => {
                    Err(format!("Invalid object index 0x{:08X} (no chunk)", index))
                }
                Err(IncrementRefError::StaleOrFreed {
                    slot_gen,
                    is_dying,
                    refcount,
                }) => Err(format!(
                    "Invalid or stale object handle: index={}, gen={}, slot_gen={}, dying={}, refcount={}",
                    index, generation, slot_gen, is_dying, refcount
                )),
                Err(IncrementRefError::RefcountOverflow) => {
                    Err("NanValue reference count overflow".to_string())
                }
            }
        }
    }
}

// ============================================
// LOCK-FREE CLONE
// ============================================

impl Clone for NanValue {
    /// Clone a NanValue. For heap pointers, this performs a lock-free
    /// compare-and-swap on the chunk's (generation, refcount) state word
    /// via the centralized `try_increment_ref` primitive.
    /// No RwLock is acquired — the operation is fully lock-free.
    ///
    /// In both debug and release builds, cloning a pointer whose slot is
    /// invalid, stale, dying, or overflowing is a fatal invariant violation
    /// and will panic immediately, preventing the creation of invalid owning handles.
    fn clone(&self) -> Self {
        if self.is_pointer() {
            let (index, generation) = extract_handle_from_bits(self.0);
            match REGISTRY.try_increment_ref(index, generation) {
                Ok(()) => {}
                Err(IncrementRefError::InvalidIndex) => {
                    panic!("NanValue::clone failed: invalid object index {}", index);
                }
                Err(IncrementRefError::StaleOrFreed {
                    slot_gen,
                    is_dying,
                    refcount,
                }) => {
                    panic!(
                        "NanValue::clone failed: stale/freed handle (index={}, gen={}, slot_gen={}, dying={}, refcount={})",
                        index, generation, slot_gen, is_dying, refcount
                    );
                }
                Err(IncrementRefError::RefcountOverflow) => {
                    panic!("NanValue::clone failed: reference count overflow (index={})", index);
                }
            }
        }
        NanValue(self.0)
    }
}

// ============================================
// LOCK-FREE DROP
// ============================================

impl Drop for NanValue {
    /// Drop a NanValue. For heap pointers, this performs a lock-free
    /// compare-and-swap to decrement the refcount. If the refcount reaches
    /// one (we are the last reference), the slot transitions to DYING:
    ///
    ///   LIVE(gen, refs=1) → DYING(gen)   [lock-free CAS]
    ///
    /// Cleanup then proceeds under the shard's write lock:
    ///
    ///   DYING(gen) → remove value → FREE(gen+1)   [under write lock]
    ///
    /// If the generation would wrap to 0 (exhaustion after 65535 cycles),
    /// the slot is permanently retired (not recycled).
    fn drop(&mut self) {
        if !self.is_pointer() {
            return;
        }
        let (index, generation) = extract_handle_from_bits(self.0);
        let chunk_ptr = match REGISTRY.get_chunk(index) {
            Some(p) => p,
            None => return,
        };
        let offset = (index as usize) % CHUNK_SIZE;

        // Lock-free refcount decrement via CAS.
        // If we are the last reference, transition to DYING state.
        let needs_cleanup = loop {
            let state = unsafe { (*chunk_ptr).states[offset].load(Ordering::Acquire) };
            let (slot_gen, is_dying, refcount) = unpack_state(state);
            if slot_gen != generation || is_dying || refcount == 0 {
                // Stale, dying, or already freed — no-op.
                return;
            }
            if refcount == 1 {
                // Last reference — transition to DYING state.
                let dying_state = pack_dying_state(generation);
                match unsafe {
                    (*chunk_ptr).states[offset].compare_exchange_weak(
                        state,
                        dying_state,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    )
                } {
                    Ok(_) => break true, // We transitioned to DYING — cleanup needed
                    Err(_) => continue,
                }
            } else {
                // Not the last reference — just decrement.
                let new_state = pack_state(generation, refcount - 1);
                match unsafe {
                    (*chunk_ptr).states[offset].compare_exchange_weak(
                        state,
                        new_state,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    )
                } {
                    Ok(_) => break false,
                    Err(_) => continue,
                }
            }
        };

        if needs_cleanup {
            // We transitioned the slot to DYING — now clean up under the write lock.
            let shard_idx = (index % (SHARD_COUNT as u32)) as usize;

            // Check for generation exhaustion: if generation is u16::MAX, the
            // next generation would wrap to 0. Retire the slot permanently
            // instead of recycling, to maintain ABA protection.
            let should_recycle = generation < u16::MAX;
            let next_gen = if should_recycle {
                generation + 1
            } else {
                // Slot exhausted — leave it in a non-recyclable FREE state.
                // Use generation 0 to signal "permanently retired."
                0
            };

            // Remove the value from the HashMap under the write lock, but
            // *do not* drop it while holding the lock.  If the HeapValue is an
            // Array containing NanValues that happen to live in the same shard,
            // dropping them would try to re-acquire this write lock and
            // deadlock.  Instead, extract the Arc and let it drop after the
            // lock guard is released.
            let removed = {
                let mut values = REGISTRY.shards[shard_idx].values.write().unwrap();
                // Verify the slot is still DYING with our generation.
                let state = unsafe { (*chunk_ptr).states[offset].load(Ordering::Acquire) };
                let (slot_gen, is_dying, _refcount) = unpack_state(state);
                if slot_gen != generation || !is_dying {
                    // Someone interfered (shouldn't happen — DYING blocks clone/drop).
                    // Just return without cleanup.
                    return;
                }
                // Remove the value from the HashMap and transition to FREE.
                let removed = values.remove(&index);
                // Transition DYING → FREE with bumped generation.
                unsafe {
                    (*chunk_ptr).states[offset].store(pack_state(next_gen, 0), Ordering::Release);
                }
                removed
            };
            // `removed` (the Arc<HeapValue>) drops here, outside the write lock.
            drop(removed);

            // Push the recycled index to the shard's free list (if not exhausted).
            if should_recycle {
                if let Ok(mut free_list) = REGISTRY.shards[shard_idx].free_list.lock() {
                    if free_list.len() < 100_000 {
                        free_list.push(FreeEntry {
                            index,
                            next_generation: next_gen,
                        });
                    }
                }
            }
        }
    }
}

// ============================================
// DEBUG + EQ
// ============================================

impl std::fmt::Debug for NanValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(d) = self.as_f64() {
            write!(f, "Number({})", d)
        } else if self.is_null() {
            write!(f, "Null")
        } else if let Some(b) = self.as_bool() {
            write!(f, "Bool({})", b)
        } else if let Some(i) = self.as_i48() {
            write!(f, "Int48({})", i)
        } else if let Some(c) = self.as_char() {
            write!(f, "Char('{}')", c)
        } else if let Some(heap) = self.as_heap() {
            write!(f, "Heap({:?})", heap.as_ref())
        } else {
            write!(f, "Unknown(0x{:016X})", self.0)
        }
    }
}

impl PartialEq for NanValue {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for NanValue {}

// ============================================
// TESTS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null() {
        let v = NanValue::null();
        assert!(v.is_null());
        assert!(!v.is_double());
        assert!(!v.is_bool());
        assert!(v.is_valid());
    }

    #[test]
    fn test_bool() {
        let t = NanValue::from_bool(true);
        let f = NanValue::from_bool(false);

        assert!(t.is_bool());
        assert!(f.is_bool());
        assert_eq!(t.as_bool(), Some(true));
        assert_eq!(f.as_bool(), Some(false));

        assert!(!t.is_null());
        assert!(!t.is_double());
        assert!(t.is_valid());
        assert!(f.is_valid());
    }

    #[test]
    fn test_double() {
        let v = NanValue::from_f64(3.14159);
        assert!(v.is_double());
        assert_eq!(v.as_f64(), Some(3.14159));

        assert!(!v.is_null());
        assert!(!v.is_bool());
        assert!(!v.is_int48());
        assert!(v.is_valid());
    }

    #[test]
    fn test_double_special_values() {
        let inf = NanValue::from_f64(f64::INFINITY);
        assert!(inf.is_double());
        assert_eq!(inf.as_f64(), Some(f64::INFINITY));

        let neg_inf = NanValue::from_f64(f64::NEG_INFINITY);
        assert!(neg_inf.is_double());
        assert_eq!(neg_inf.as_f64(), Some(f64::NEG_INFINITY));

        let zero = NanValue::from_f64(0.0);
        assert!(zero.is_double());
        assert_eq!(zero.as_f64(), Some(0.0));

        let neg_zero = NanValue::from_f64(-0.0);
        assert!(neg_zero.is_double());
        assert_eq!(neg_zero.as_f64(), Some(-0.0));

        let nan = NanValue::from_f64(f64::NAN);
        assert!(nan.is_double());
        assert!(nan.as_f64().unwrap().is_nan());
    }

    #[test]
    fn test_int48_small() {
        let v = NanValue::from_i48(42).unwrap();
        assert!(v.is_int48());
        assert_eq!(v.as_i48(), Some(42));

        assert!(!v.is_double());
        assert!(!v.is_null());
        assert!(v.is_valid());
    }

    #[test]
    fn test_int48_negative() {
        let v = NanValue::from_i48(-1000).unwrap();
        assert!(v.is_int48());
        assert_eq!(v.as_i48(), Some(-1000));
    }

    #[test]
    fn test_int48_limits() {
        let max = NanValue::from_i48(INT48_MAX).unwrap();
        assert!(max.is_int48());
        assert_eq!(max.as_i48(), Some(INT48_MAX));

        let min = NanValue::from_i48(INT48_MIN).unwrap();
        assert!(min.is_int48());
        assert_eq!(min.as_i48(), Some(INT48_MIN));

        assert!(NanValue::from_i48(INT48_MAX + 1).is_none());
        assert!(NanValue::from_i48(INT48_MIN - 1).is_none());
    }

    #[test]
    fn test_char() {
        let v = NanValue::from_char('A');
        assert!(v.is_char());
        assert_eq!(v.as_char(), Some('A'));

        let emoji = NanValue::from_char('🚀');
        assert!(emoji.is_char());
        assert_eq!(emoji.as_char(), Some('🚀'));
    }

    #[test]
    fn test_heap_string() {
        let heap = HeapValue::String("Hello, World!".to_string());
        let v = NanValue::from_heap(heap);

        assert!(v.is_pointer());
        assert!(!v.is_double());

        if let Some(arc) = v.as_heap() {
            match arc.as_ref() {
                HeapValue::String(s) => assert_eq!(s, "Hello, World!"),
                _ => panic!("Expected String"),
            }
        } else {
            panic!("Expected heap value");
        }
    }

    #[test]
    fn test_heap_bigint() {
        let big = BigInt::from(123456789012345678901234567890i128);
        let heap = HeapValue::BigInt(big.clone());
        let v = NanValue::from_heap(heap);

        assert!(v.is_pointer());

        if let Some(arc) = v.as_heap() {
            match arc.as_ref() {
                HeapValue::BigInt(bi) => assert_eq!(bi, &big),
                _ => panic!("Expected BigInt"),
            }
        } else {
            panic!("Expected heap value");
        }
    }

    #[test]
    fn test_from_int_auto() {
        let small = NanValue::from_int(42);
        assert!(small.is_int48());
        assert_eq!(small.as_i48(), Some(42));

        let large = NanValue::from_int(i64::MAX);
        assert!(large.is_pointer());
        if let Some(arc) = large.as_heap() {
            match arc.as_ref() {
                HeapValue::BigInt(bi) => {
                    assert_eq!(bi.to_i64(), Some(i64::MAX));
                }
                _ => panic!("Expected BigInt for large integer"),
            }
        }
    }

    #[test]
    fn test_equality() {
        let v1 = NanValue::from_f64(3.14);
        let v2 = NanValue::from_f64(3.14);
        assert_eq!(v1, v2);

        let n1 = NanValue::null();
        let n2 = NanValue::null();
        assert_eq!(n1, n2);

        let t1 = NanValue::from_bool(true);
        let t2 = NanValue::from_bool(true);
        assert_eq!(t1, t2);

        let f = NanValue::from_bool(false);
        assert_ne!(t1, f);
    }

    #[test]
    fn test_size() {
        assert_eq!(std::mem::size_of::<NanValue>(), 8);
    }

    #[test]
    fn test_clone() {
        let v1 = NanValue::from_f64(std::f64::consts::E);
        let v2 = v1.clone();
        assert_eq!(v1, v2);

        let heap = HeapValue::String("Test".to_string());
        let h1 = NanValue::from_heap(heap);
        let h2 = h1.clone();

        assert!(h1.is_pointer());
        assert!(h2.is_pointer());
        assert_eq!(h1, h2); // Same bits (index + generation)
    }

    #[test]
    fn test_concurrent_clones_and_drops() {
        use std::thread;

        let heap = HeapValue::String("Concurrent Test".to_string());
        let root = Arc::new(NanValue::from_heap(heap));

        let mut handles = vec![];
        for _ in 0..10 {
            let root_clone = root.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..1_000 {
                    let v = root_clone.as_ref().clone();
                    assert!(v.is_pointer());
                    drop(v);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_vector_reallocation_ownership() {
        let mut vec = Vec::new();
        for i in 0..500 {
            let val = NanValue::from_heap(HeapValue::String(format!("val_{}", i)));
            vec.push(val);
        }
        assert_eq!(vec.len(), 500);

        for (i, val) in vec.iter().enumerate() {
            if let Some(heap) = val.as_heap() {
                if let HeapValue::String(s) = heap.as_ref() {
                    assert_eq!(s, &format!("val_{}", i));
                }
            }
        }
    }

    // ----------------------------------------
    // Generation-based ID tests
    // ----------------------------------------

    #[test]
    fn test_generation_prevents_stale_handle() {
        // Create an object, get its handle bits, drop it, allocate a new object
        // that reuses the same index, and verify the stale handle doesn't
        // resolve to the new object.
        let v1 = NanValue::from_heap(HeapValue::String("first".to_string()));
        let stale_bits = v1.as_bits();
        drop(v1);

        // Allocate many objects to force recycling of the freed index.
        // (The free list is per-shard, so we may need several allocations.)
        let mut keep = Vec::new();
        for i in 0..256 {
            keep.push(NanValue::from_heap(HeapValue::String(format!("recycled_{}", i))));
        }

        // The stale bits should not resolve to a valid heap value.
        // Use mem::forget to prevent Drop from running on the stale handle,
        // since from_bits_unchecked did not claim a reference.
        let stale = unsafe { NanValue::from_bits_unchecked(stale_bits) };
        assert!(
            stale.as_heap().is_none(),
            "Stale handle should not resolve after ID recycling"
        );
        std::mem::forget(stale); // Stale handle owns no reference — don't drop it.

        // try_from_bits should also reject it.
        let result = NanValue::try_from_bits(stale_bits);
        assert!(result.is_err(), "try_from_bits should reject stale handle");
    }

    #[test]
    fn test_id_recycling_works() {
        // Allocate and drop many objects, then verify new allocations still work.
        for _ in 0..1000 {
            let v = NanValue::from_heap(HeapValue::String("temp".to_string()));
            drop(v);
        }
        // After recycling, new allocations should work.
        let v = NanValue::from_heap(HeapValue::String("after_recycle".to_string()));
        assert!(v.is_pointer());
        if let Some(arc) = v.as_heap() {
            if let HeapValue::String(s) = arc.as_ref() {
                assert_eq!(s, "after_recycle");
            }
        }
    }

    #[test]
    fn test_clone_drop_refcount_balance() {
        let v = NanValue::from_heap(HeapValue::String("balance".to_string()));
        // Clone and drop should balance — after all clones are dropped,
        // the original should still be valid.
        let c1 = v.clone();
        let c2 = v.clone();
        drop(c1);
        drop(c2);
        // v should still be valid.
        assert!(v.as_heap().is_some());
        drop(v);
    }

    // ----------------------------------------
    // is_valid() tests
    // ----------------------------------------

    #[test]
    fn test_is_valid_known_tags() {
        assert!(NanValue::null().is_valid());
        assert!(NanValue::from_bool(true).is_valid());
        assert!(NanValue::from_bool(false).is_valid());
        assert!(NanValue::from_f64(3.14).is_valid());
        assert!(NanValue::from_f64(f64::NAN).is_valid());
        assert!(NanValue::from_f64(f64::INFINITY).is_valid());
        assert!(NanValue::from_i48(42).unwrap().is_valid());
        assert!(NanValue::from_char('A').is_valid());
        assert!(NanValue::from_i8(-1).is_valid());
        assert!(NanValue::from_u32(42).is_valid());
        assert!(NanValue::from_f32(1.5).is_valid());
    }

    #[test]
    fn test_is_valid_rejects_unknown_tags() {
        // 0xFFFF tag is not a valid tag
        let bad = unsafe { NanValue::from_bits_unchecked(0xFFFF_0000_0000_0000) };
        assert!(!bad.is_valid());

        // 0xFFFB with unknown sub-tag
        let bad2 = unsafe { NanValue::from_bits_unchecked(0xFFFB_FF00_0000_0000) };
        assert!(!bad2.is_valid());

        // TAG_NULL with non-zero payload
        let bad3 = unsafe { NanValue::from_bits_unchecked(0xFFFC_0000_0000_0001) };
        assert!(!bad3.is_valid());

        // TAG_BOOL with invalid payload
        let bad4 = unsafe { NanValue::from_bits_unchecked(0xFFFD_0000_0000_0002) };
        assert!(!bad4.is_valid());
    }

    #[test]
    fn test_try_from_bits_validates_tags() {
        // Valid non-pointer
        assert!(NanValue::try_from_bits(NanValue::from_f64(3.14).as_bits()).is_ok());
        assert!(NanValue::try_from_bits(NanValue::null().as_bits()).is_ok());
        assert!(NanValue::try_from_bits(NanValue::from_bool(true).as_bits()).is_ok());

        // Invalid tag
        assert!(NanValue::try_from_bits(0xFFFF_0000_0000_0000).is_err());
        assert!(NanValue::try_from_bits(0xFFFC_0000_0000_0001).is_err()); // NULL with payload
    }

    // ----------------------------------------
    // Stress tests
    // ----------------------------------------

    #[test]
    fn test_stress_concurrent_clone_drop() {
        use std::thread;

        // 20 threads, each cloning and dropping the same object 5,000 times.
        let heap = HeapValue::String("stress_test".to_string());
        let root = Arc::new(NanValue::from_heap(heap));

        let mut handles = vec![];
        for _ in 0..20 {
            let root_clone = root.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..5_000 {
                    let v = root_clone.as_ref().clone();
                    assert!(v.is_pointer());
                    drop(v);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // After all threads finish, root should still be valid.
        assert!(root.as_heap().is_some());
    }

    #[test]
    fn test_stress_recycle_cycle() {
        // Repeatedly create → clone → drop-final → recycle → create.
        // This exercises the generation bump and free-list recycling.
        for _ in 0..10_000 {
            let v = NanValue::from_heap(HeapValue::String("cycle".to_string()));
            let c = v.clone();
            drop(c);
            drop(v);
        }
        // Verify the registry still works after 10k cycles.
        let v = NanValue::from_heap(HeapValue::String("post_cycle".to_string()));
        assert!(v.as_heap().is_some());
    }

    #[test]
    fn test_stress_concurrent_alloc_drop() {
        use std::thread;

        // 10 threads each creating and dropping 2,000 unique objects.
        let mut handles = vec![];
        for t in 0..10 {
            handles.push(thread::spawn(move || {
                for i in 0..2_000 {
                    let v = NanValue::from_heap(HeapValue::String(format!("t{}_{}", t, i)));
                    assert!(v.is_pointer());
                    // Clone a few times.
                    let c1 = v.clone();
                    let c2 = v.clone();
                    drop(c1);
                    drop(c2);
                    drop(v);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_stress_concurrent_as_heap_during_drop() {
        use std::thread;

        // One thread holds a reference and repeatedly calls as_heap.
        // Another thread clones and drops rapidly.
        let v = Arc::new(NanValue::from_heap(HeapValue::String("shared".to_string())));

        let v2 = v.clone();
        let reader = thread::spawn(move || {
            for _ in 0..10_000 {
                // as_heap should always succeed while we hold a reference.
                assert!(v2.as_heap().is_some());
            }
        });

        let v3 = v.clone();
        let writer = thread::spawn(move || {
            for _ in 0..10_000 {
                let c = v3.as_ref().clone();
                drop(c);
            }
        });

        reader.join().unwrap();
        writer.join().unwrap();
    }

    #[test]
    fn test_stress_large_nested_array() {
        // Create an array containing 1k NanValues, then verify integrity.
        let count = 1_000;
        let elements: Vec<NanValue> = (0..count)
            .map(|i| NanValue::from_heap(HeapValue::String(format!("elem_{}", i))))
            .collect();
        let arr = NanValue::from_heap(HeapValue::Array(elements));

        // Clone the array (refcount goes to 2).
        let arr2 = arr.clone();

        // Verify the array is accessible.
        if let Some(arc) = arr.as_heap() {
            if let HeapValue::Array(vec) = arc.as_ref() {
                assert_eq!(vec.len(), count);
                // Spot-check a few elements.
                for i in [0, 1, count / 2, count - 1] {
                    if let Some(elem_arc) = vec[i].as_heap() {
                        if let HeapValue::String(s) = elem_arc.as_ref() {
                            assert_eq!(s, &format!("elem_{}", i));
                        }
                    }
                }
            }
        }
        drop(arr2);
        drop(arr);
    }

    #[test]
    fn test_stress_cross_thread_sharing() {
        use std::thread;

        // Create an object on one thread, share it with another, verify access.
        let v = Arc::new(NanValue::from_heap(HeapValue::String("cross_thread".to_string())));

        let v_clone = v.clone();
        let handle = thread::spawn(move || {
            // Access from a different thread.
            if let Some(arc) = v_clone.as_heap() {
                if let HeapValue::String(s) = arc.as_ref() {
                    assert_eq!(s, "cross_thread");
                }
            } else {
                panic!("Failed to access cross-thread object");
            }
            // Clone from this thread.
            let c = v_clone.as_ref().clone();
            drop(c);
        });

        handle.join().unwrap();
    }

    #[test]
    fn test_stress_try_from_bits_concurrent() {
        use std::thread;

        // Concurrently create objects and try_from_bits their handles.
        let v = Arc::new(NanValue::from_heap(HeapValue::String("bits_test".to_string())));
        let bits = v.as_bits();

        let mut handles = vec![];
        for _ in 0..8 {
            let bits = bits;
            handles.push(thread::spawn(move || {
                for _ in 0..1_000 {
                    // try_from_bits should succeed (the object is alive).
                    let result = NanValue::try_from_bits(bits);
                    assert!(result.is_ok(), "try_from_bits failed on live object");
                    // Drop the claimed reference.
                    drop(result.unwrap());
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_torture_concurrent_alloc_clone_drop_recycle() {
        use std::thread;

        // 8 threads allocate, clone, read, and drop objects in tight loops.
        let thread_count = 8;
        let iters_per_thread = 2_500;

        let handles: Vec<_> = (0..thread_count)
            .map(|t_id| {
                thread::spawn(move || {
                    for i in 0..iters_per_thread {
                        let text = format!("thread_{}_iter_{}", t_id, i);
                        let val = NanValue::from_heap(HeapValue::String(text.clone()));

                        // Verify read under read lock
                        if let Some(arc) = val.as_heap() {
                            if let HeapValue::String(s) = arc.as_ref() {
                                assert_eq!(s, &text);
                            } else {
                                panic!("Unexpected heap value variant");
                            }
                        } else {
                            panic!("as_heap failed on freshly allocated value");
                        }

                        // Clone and verify refcount increment
                        let cloned = val.clone();
                        if let Some(arc) = cloned.as_heap() {
                            if let HeapValue::String(s) = arc.as_ref() {
                                assert_eq!(s, &text);
                            }
                        }

                        drop(val);
                        // Cloned should still be alive
                        if let Some(arc) = cloned.as_heap() {
                            if let HeapValue::String(s) = arc.as_ref() {
                                assert_eq!(s, &text);
                            }
                        }
                        drop(cloned);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_torture_deeply_nested_heap_value_drops() {
        // Construct deeply nested data structures spanning multiple shards.
        // Drops must occur outside the shard write lock to avoid deadlock when
        // child values reside in the same shard as the parent.
        let mut current_layer = Vec::new();
        for i in 0..50 {
            current_layer.push(NanValue::from_heap(HeapValue::String(format!("leaf_{}", i))));
        }

        for level in 0..10 {
            let mut next_layer = Vec::new();
            for i in 0..10 {
                let mut map = std::collections::HashMap::new();
                map.insert(format!("key_{}_{}", level, i), NanValue::from_heap(HeapValue::Array(current_layer.clone())));
                next_layer.push(NanValue::from_heap(HeapValue::Object(map)));
            }
            current_layer = next_layer;
        }

        // Dropping the top layer cascades drops across thousands of handles across all shards.
        drop(current_layer);
    }

    #[test]
    fn test_torture_rapid_slot_recycling() {
        // 50,000 allocate-and-immediate-drop iterations.
        // Ensures generations increment safely and free list recycling works under high throughput.
        for i in 0..50_000 {
            let val = NanValue::from_heap(HeapValue::String(format!("recycle_{}", i)));
            let bits = val.as_bits();
            drop(val);

            // Stale handle should fail try_from_bits or as_heap
            assert!(NanValue::try_from_bits(bits).is_err());
        }
    }

    #[test]
    fn test_torture_generation_retirement() {
        // Allocate an object, artificially set its generation in the slot to u16::MAX,
        // and verify that upon dropping the last reference, it is retired and not placed in the free list.
        let val = NanValue::from_heap(HeapValue::String("exhaustion_test".to_string()));
        let (index, _gen) = extract_handle_from_bits(val.as_bits());
        let chunk_ptr = REGISTRY.get_chunk(index).unwrap();
        let offset = (index as usize) % CHUNK_SIZE;

        // Forget val so its original handle isn't dropped with a now-outdated generation.
        std::mem::forget(val);

        // Set state to generation u16::MAX with refcount 1
        unsafe {
            (*chunk_ptr).states[offset].store(pack_state(u16::MAX, 1), Ordering::Release);
        }

        // Construct NanValue with generation u16::MAX matching the slot
        let handle_bits = TAG_POINTER | pack_handle(index, u16::MAX);
        let max_gen_val = unsafe { NanValue::from_bits_unchecked(handle_bits) };

        // Drop max_gen_val which transitions u16::MAX -> 0 (retired)
        drop(max_gen_val);

        // Slot state should now be permanently retired with gen 0, refcount 0
        let state = unsafe { (*chunk_ptr).states[offset].load(Ordering::Acquire) };
        let (slot_gen, is_dying, refcount) = unpack_state(state);
        assert_eq!(slot_gen, 0);
        assert!(!is_dying);
        assert_eq!(refcount, 0);

        // Free list for this shard should not contain this slot with next generation wrapped
        let shard_idx = (index % (SHARD_COUNT as u32)) as usize;
        let free_list = REGISTRY.shards[shard_idx].free_list.lock().unwrap();
        assert!(!free_list.iter().any(|e| e.index == index && e.next_generation == 0));
    }

    #[test]
    fn test_torture_try_increment_ref_errors() {
        // Test invalid index (unallocated chunk)
        let err = REGISTRY.try_increment_ref(u32::MAX - 1, 0);
        assert_eq!(err, Err(IncrementRefError::InvalidIndex));

        // Test stale/freed object
        let val = NanValue::from_heap(HeapValue::String("temp".to_string()));
        let (index, generation) = extract_handle_from_bits(val.as_bits());
        drop(val);

        // Generation mismatch or freed slot
        let err = REGISTRY.try_increment_ref(index, generation);
        assert!(matches!(err, Err(IncrementRefError::StaleOrFreed { .. })));
    }

    #[test]
    fn test_nan_tag_boundaries_exhaustive() {
        // Boundary sweep for NaN tag space
        let patterns: Vec<(u64, bool, bool, &'static str)> = vec![
            // (bits, expected_is_double, expected_is_valid, description)
            (0x0000_0000_0000_0000, true, true, "zero float"),
            (0x3FF0_0000_0000_0000, true, true, "1.0 float"),
            (0x7FF7_FFFF_FFFF_FFFF, true, true, "boundary before quiet NaN"),
            (0x7FF8_0000_0000_0000, true, true, "canonical quiet NaN"),
            (0x7FFF_FFFF_FFFF_FFFF, true, true, "max quiet NaN float"),
            (0xFFF0_0000_0000_0000, true, true, "-Infinity"),
            (0xFFF7_FFFF_FFFF_FFFF, true, true, "boundary before tagged space"),
            (TAG_I64, false, true, "TAG_I64"),
            (TAG_U64, false, true, "TAG_U64"),
            (TAG_POINTER, false, true, "TAG_POINTER"),
            (TAG_I8, false, true, "TAG_I8"),
            (TAG_I16, false, true, "TAG_I16"),
            (TAG_I32, false, true, "TAG_I32"),
            (TAG_U8, false, true, "TAG_U8"),
            (TAG_U16, false, true, "TAG_U16"),
            (TAG_U32, false, true, "TAG_U32"),
            (TAG_F32, false, true, "TAG_F32"),
            (TAG_I128, false, true, "TAG_I128"),
            (TAG_U128, false, true, "TAG_U128"),
            (0xFFFB_0000_0000_0000, false, false, "invalid subtype 00"),
            (0xFFFB_0A00_0000_0000, false, false, "invalid subtype 0A"),
            (TAG_NULL, false, true, "TAG_NULL"),
            (TAG_NULL | 0x1, false, false, "TAG_NULL with non-zero payload"),
            (TAG_FALSE, false, true, "TAG_FALSE"),
            (TAG_TRUE, false, true, "TAG_TRUE"),
            (0xFFFD_0000_0000_0002, false, false, "invalid boolean payload"),
            (TAG_CHAR | 0x41, false, true, "TAG_CHAR ('A')"),
            (TAG_CHAR | 0x0011_0000, false, false, "invalid char code point"),
            (0xFFFF_0000_0000_0000, false, false, "TAG 0xFFFF reserved"),
        ];

        for (bits, exp_double, exp_valid, desc) in patterns {
            let val = unsafe { NanValue::from_bits_unchecked(bits) };
            assert_eq!(
                val.is_double(),
                exp_double,
                "is_double mismatch for {}: 0x{:016X}",
                desc,
                bits
            );
            assert_eq!(
                val.is_valid(),
                exp_valid,
                "is_valid mismatch for {}: 0x{:016X}",
                desc,
                bits
            );
        }
    }

    #[test]
    #[should_panic(expected = "NanValue::clone failed")]
    fn test_clone_stale_handle_panics() {
        // Create an object, extract bits, drop it, and attempt to clone the stale handle.
        let val = NanValue::from_heap(HeapValue::String("stale_clone_target".to_string()));
        let bits = val.as_bits();
        drop(val);

        // Unsafely recreate handle from freed bits and attempt to clone.
        let stale_val = unsafe { NanValue::from_bits_unchecked(bits) };
        let _counterfeit = stale_val.clone(); // MUST PANIC unconditionally!
    }

    #[test]
    fn test_i128_smart_allocation() {
        // Inline small i128
        let small = NanValue::from_int128(123456789);
        assert!(small.is_i128());
        assert_eq!(small.as_i128(), Some(123456789));

        // Negative small i128
        let small_neg = NanValue::from_int128(-123456789);
        assert!(small_neg.is_i128());
        assert_eq!(small_neg.as_i128(), Some(-123456789));

        // Large i128 exceeding 40 bits -> BigInt promotion
        let large_num: i128 = 1_000_000_000_000_000_000;
        let large = NanValue::from_int128(large_num);
        assert!(large.is_pointer());
        if let Some(arc) = large.as_heap() {
            if let HeapValue::BigInt(bi) = arc.as_ref() {
                assert_eq!(bi, &BigInt::from(large_num));
            } else {
                panic!("Expected BigInt heap value");
            }
        } else {
            panic!("Expected heap value for large i128");
        }
    }

    #[test]
    fn test_heap_ref_guard() {
        let val = NanValue::from_heap(HeapValue::String("borrowed_guard".to_string()));
        {
            let guard = val.as_heap_ref().expect("expected HeapRef");
            if let HeapValue::String(s) = &*guard {
                assert_eq!(s, "borrowed_guard");
            } else {
                panic!("Expected string heap value");
            }
        }
        drop(val);
    }
}
