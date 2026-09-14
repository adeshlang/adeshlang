//! Dense Borrow State Storage
//!
//! High-performance borrow state tracking using dense vectors instead of HashMaps.
//! Indexed by PlaceId for O(1) access. Uses bitmask for fast equality comparison.

use super::mir::PlaceId;
use std::fmt;

/// Discriminant tag for borrow state
/// Using repr(u8) for a simple enum with no data is safe
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BorrowTag {
    Unborrowed = 0,
    SharedBorrowed = 1,
    ExclusiveBorrowed = 2,
    Moved = 3,
    /// Destructor ran, ownership ended (logical)
    Dropped = 4,
    /// Raw memory deallocated (physical, FFI/unsafe)
    Freed = 5,
    Error = 6,
}

/// Compact borrow state with explicit layout
///
/// Total size: 8 bytes (1 tag + 3 padding + 4 payload)
/// This uses repr(C) for predictable layout across platforms.
///
/// CRITICAL FIX: Previous design used #[repr(u8)] with data-carrying
/// variants which is INVALID in Rust. repr(u8) only specifies discriminant
/// size, not payload layout. This struct uses explicit tag+payload.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct BorrowState2 {
    /// Discriminant tag
    tag: BorrowTag,
    /// Padding for alignment (explicit for repr(C))
    _pad: [u8; 3],
    /// Union payload - interpretation depends on tag:
    /// - SharedBorrowed: count (u16 in lower bits)
    /// - ExclusiveBorrowed: borrow_id (u32)
    /// - Others: unused (0)
    payload: u32,
}

impl BorrowState2 {
    /// Unborrowed state (constant)
    pub const UNBORROWED: Self = Self {
        tag: BorrowTag::Unborrowed,
        _pad: [0; 3],
        payload: 0,
    };

    /// Moved state (constant)
    pub const MOVED: Self = Self {
        tag: BorrowTag::Moved,
        _pad: [0; 3],
        payload: 0,
    };

    /// Dropped state (constant) - destructor ran
    pub const DROPPED: Self = Self {
        tag: BorrowTag::Dropped,
        _pad: [0; 3],
        payload: 0,
    };

    /// Freed state (constant) - raw memory deallocated (FFI/unsafe)
    pub const FREED: Self = Self {
        tag: BorrowTag::Freed,
        _pad: [0; 3],
        payload: 0,
    };

    /// Error state (constant)
    pub const ERROR: Self = Self {
        tag: BorrowTag::Error,
        _pad: [0; 3],
        payload: 0,
    };

    /// Create shared borrowed state with count
    #[inline]
    pub const fn shared(count: u16) -> Self {
        Self {
            tag: BorrowTag::SharedBorrowed,
            _pad: [0; 3],
            payload: count as u32,
        }
    }

    /// Create exclusive borrowed state with borrow ID
    #[inline]
    pub const fn exclusive(borrow_id: u32) -> Self {
        Self {
            tag: BorrowTag::ExclusiveBorrowed,
            _pad: [0; 3],
            payload: borrow_id,
        }
    }

    /// Get the tag
    #[inline]
    pub fn tag(&self) -> BorrowTag {
        self.tag
    }

    /// Check if unborrowed
    #[inline]
    pub fn is_unborrowed(&self) -> bool {
        self.tag == BorrowTag::Unborrowed
    }

    /// Get shared borrow count (if shared)
    #[inline]
    pub fn shared_count(&self) -> Option<u16> {
        if self.tag == BorrowTag::SharedBorrowed {
            Some(self.payload as u16)
        } else {
            None
        }
    }

    /// Get exclusive borrow ID (if exclusive)
    #[inline]
    pub fn borrow_id(&self) -> Option<u32> {
        if self.tag == BorrowTag::ExclusiveBorrowed {
            Some(self.payload)
        } else {
            None
        }
    }

    /// Is this the bottom of the lattice?
    #[inline]
    pub fn is_bottom(&self) -> bool {
        self.tag == BorrowTag::Unborrowed
    }

    /// Is this an error state?
    #[inline]
    pub fn is_error(&self) -> bool {
        self.tag == BorrowTag::Error
    }

    /// Is this a terminal state (moved, dropped, error)?
    #[inline]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.tag,
            BorrowTag::Moved | BorrowTag::Dropped | BorrowTag::Error
        )
    }

    /// Can we read from this place?
    #[inline]
    pub fn can_read(&self) -> bool {
        matches!(
            self.tag,
            BorrowTag::Unborrowed | BorrowTag::SharedBorrowed | BorrowTag::ExclusiveBorrowed
        )
    }

    /// Can we write to this place?
    #[inline]
    pub fn can_write(&self) -> bool {
        matches!(
            self.tag,
            BorrowTag::Unborrowed | BorrowTag::ExclusiveBorrowed
        )
    }

    /// Lattice join: combine two states
    pub fn join(self, other: Self) -> Result<Self, BorrowConflict> {
        use BorrowTag::*;

        match (self.tag, other.tag) {
            // Error propagates
            (Error, _) | (_, Error) => Ok(Self::ERROR),

            // Freed is stricter than Dropped (FFI/unsafe boundary)
            (Freed, _) | (_, Freed) => Err(BorrowConflict::FreedMerge),

            // Dropped is always an error when merging
            (Dropped, _) | (_, Dropped) => Err(BorrowConflict::DroppedMerge),

            // Moved is sticky
            (Moved, _) | (_, Moved) => Ok(Self::MOVED),

            // Bottom absorbs
            (Unborrowed, _) => Ok(other),
            (_, Unborrowed) => Ok(self),

            // Shared + Shared = Shared(max)
            (SharedBorrowed, SharedBorrowed) => {
                let max_count = self.payload.max(other.payload);
                Ok(Self::shared(max_count as u16))
            }

            // Exclusive + Exclusive (same) = Exclusive
            (ExclusiveBorrowed, ExclusiveBorrowed) if self.payload == other.payload => Ok(self),

            // Exclusive + Exclusive (different) = Error
            (ExclusiveBorrowed, ExclusiveBorrowed) => Err(BorrowConflict::DifferentExclusives),

            // Shared + Exclusive = Error
            (SharedBorrowed, ExclusiveBorrowed) | (ExclusiveBorrowed, SharedBorrowed) => {
                Err(BorrowConflict::SharedExclusiveConflict)
            }
        }
    }

    /// Widen for loop back-edge (monotonic lattice operation)
    /// This ensures fixpoint termination by making states "sticky"
    pub fn widen(current: Self, incoming: Self) -> Result<Self, LoopBorrowConflict> {
        use BorrowTag::*;

        match (current.tag, incoming.tag) {
            // Error always propagates
            (Error, _) | (_, Error) => Ok(Self::ERROR),

            // Freed in loop is always error (memory gone)
            (_, Freed) | (Freed, _) => Err(LoopBorrowConflict::FreedInLoop),

            // Dropped in loop is always error
            (_, Dropped) | (Dropped, _) => Err(LoopBorrowConflict::DroppedInLoop),

            // Moved is sticky (cannot resurrect in loop)
            (Moved, _) | (_, Moved) => Ok(Self::MOVED),

            // Unborrowed absorbs incoming
            (Unborrowed, _) => Ok(incoming),

            // Sticky: once borrowed, stays borrowed even if incoming is unborrowed
            (SharedBorrowed, Unborrowed) => Ok(current),
            (ExclusiveBorrowed, Unborrowed) => Ok(current),

            // Shared + Shared = max count
            (SharedBorrowed, SharedBorrowed) => {
                let max_count = current.payload.max(incoming.payload);
                Ok(Self::shared(max_count as u16))
            }

            // Exclusive must be same ID
            (ExclusiveBorrowed, ExclusiveBorrowed) => {
                if current.payload == incoming.payload {
                    Ok(current)
                } else {
                    Err(LoopBorrowConflict::ConflictingExclusive)
                }
            }

            // Shared + Exclusive = Error
            (SharedBorrowed, ExclusiveBorrowed) | (ExclusiveBorrowed, SharedBorrowed) => {
                Err(LoopBorrowConflict::SharedExclusiveConflict)
            }
        }
    }
}

impl Default for BorrowState2 {
    fn default() -> Self {
        Self::UNBORROWED
    }
}

impl fmt::Debug for BorrowState2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.tag {
            BorrowTag::Unborrowed => write!(f, "Unborrowed"),
            BorrowTag::SharedBorrowed => write!(f, "SharedBorrowed({})", self.payload),
            BorrowTag::ExclusiveBorrowed => write!(f, "ExclusiveBorrowed({})", self.payload),
            BorrowTag::Moved => write!(f, "Moved"),
            BorrowTag::Dropped => write!(f, "Dropped"),
            BorrowTag::Freed => write!(f, "Freed"),
            BorrowTag::Error => write!(f, "Error"),
        }
    }
}

/// Conflict types for error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowConflict {
    DroppedMerge,
    /// v2.2: Use after free (stricter than Dropped)
    FreedMerge,
    DifferentExclusives,
    SharedExclusiveConflict,
}

/// Loop-specific borrow conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopBorrowConflict {
    DroppedInLoop,
    /// v2.2: Freed in loop body
    FreedInLoop,
    ConflictingExclusive,
    SharedExclusiveConflict,
}

/// Compact bitset for tracking active places
#[derive(Clone, PartialEq, Eq)]
pub struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub fn new() -> Self {
        Self { words: Vec::new() }
    }

    pub fn with_capacity(bits: usize) -> Self {
        let words = (bits + 63) / 64;
        Self {
            words: vec![0; words],
        }
    }

    #[inline]
    pub fn insert(&mut self, bit: usize) {
        let word = bit / 64;
        let idx = bit % 64;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= 1u64 << idx;
    }

    #[inline]
    pub fn remove(&mut self, bit: usize) {
        let word = bit / 64;
        let idx = bit % 64;
        if word < self.words.len() {
            self.words[word] &= !(1u64 << idx);
        }
    }

    #[inline]
    pub fn contains(&self, bit: usize) -> bool {
        let word = bit / 64;
        let idx = bit % 64;
        word < self.words.len() && (self.words[word] & (1u64 << idx)) != 0
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(word_idx, &word)| {
            (0..64).filter_map(move |bit_idx| {
                if word & (1u64 << bit_idx) != 0 {
                    Some(word_idx * 64 + bit_idx)
                } else {
                    None
                }
            })
        })
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    pub fn clear(&mut self) {
        self.words.iter_mut().for_each(|w| *w = 0);
    }

    /// Union with another bitset
    pub fn union(&mut self, other: &Self) {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        for (i, &word) in other.words.iter().enumerate() {
            self.words[i] |= word;
        }
    }
}

impl Default for BitSet {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for BitSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BitSet{{")?;
        let mut first = true;
        for idx in self.iter() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{}", idx)?;
            first = false;
        }
        write!(f, "}}")
    }
}

/// Dense state vector indexed by PlaceId
#[derive(Clone)]
pub struct BorrowStateVec {
    /// State for each place
    states: Vec<BorrowState2>,
    /// Bitmask of places with non-Unborrowed state
    active_mask: BitSet,
    /// Generation counter for change detection
    generation: u64,
}

impl BorrowStateVec {
    pub fn new(capacity: usize) -> Self {
        Self {
            states: vec![BorrowState2::UNBORROWED; capacity],
            active_mask: BitSet::with_capacity(capacity),
            generation: 0,
        }
    }

    /// Get state for a place
    #[inline]
    pub fn get(&self, place: PlaceId) -> BorrowState2 {
        self.states
            .get(place.0 as usize)
            .copied()
            .unwrap_or(BorrowState2::UNBORROWED)
    }

    /// Set state for a place
    #[inline]
    pub fn set(&mut self, place: PlaceId, state: BorrowState2) {
        let idx = place.0 as usize;
        if idx >= self.states.len() {
            self.states.resize(idx + 1, BorrowState2::UNBORROWED);
        }
        self.states[idx] = state;

        if !state.is_unborrowed() {
            self.active_mask.insert(idx);
        } else {
            self.active_mask.remove(idx);
        }

        self.generation += 1;
    }

    /// Fast structural equality
    /// Only compares active places, not the entire vector
    pub fn equals(&self, other: &Self) -> bool {
        if self.active_mask != other.active_mask {
            return false;
        }
        for idx in self.active_mask.iter() {
            if self.states[idx] != other.states[idx] {
                return false;
            }
        }
        true
    }

    /// Merge another state vector into this one
    pub fn merge(&mut self, other: &Self) -> Result<bool, (PlaceId, BorrowConflict)> {
        let mut changed = false;

        // Merge all active places from other
        let mut combined = self.active_mask.clone();
        combined.union(&other.active_mask);

        for idx in combined.iter() {
            let place = PlaceId(idx as u32);
            let self_state = self.get(place);
            let other_state = other.get(place);

            let merged = self_state.join(other_state).map_err(|e| (place, e))?;

            if merged != self_state {
                self.set(place, merged);
                changed = true;
            }
        }

        Ok(changed)
    }

    /// Get generation number for change detection
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Get number of active (non-Unborrowed) places
    pub fn active_count(&self) -> usize {
        self.active_mask.iter().count()
    }

    /// Iterate over active places
    pub fn active_places(&self) -> impl Iterator<Item = PlaceId> + '_ {
        self.active_mask.iter().map(|idx| PlaceId(idx as u32))
    }

    /// Clone from another state vector
    pub fn clone_from_other(&mut self, other: &Self) {
        self.states.clear();
        self.states.extend_from_slice(&other.states);
        self.active_mask = other.active_mask.clone();
        self.generation = other.generation;
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.states
            .iter_mut()
            .for_each(|s| *s = BorrowState2::UNBORROWED);
        self.active_mask.clear();
        self.generation += 1;
    }
}

impl fmt::Debug for BorrowStateVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BorrowStateVec{{")?;
        let mut first = true;
        for idx in self.active_mask.iter() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "p{}:{:?}", idx, self.states[idx])?;
            first = false;
        }
        write!(f, "}}")
    }
}

impl Default for BorrowStateVec {
    fn default() -> Self {
        Self::new(16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrow_state_lattice() {
        let unborrowed = BorrowState2::UNBORROWED;
        let shared1 = BorrowState2::shared(1);
        let shared2 = BorrowState2::shared(2);
        let shared3 = BorrowState2::shared(3);
        let excl1 = BorrowState2::exclusive(1);
        let excl2 = BorrowState2::exclusive(2);
        let moved = BorrowState2::MOVED;

        // Unborrowed absorbs
        assert_eq!(unborrowed.join(shared1).unwrap(), shared1);

        // Shared + Shared = max count
        assert_eq!(shared2.join(shared3).unwrap(), shared3);

        // Moved is sticky
        assert_eq!(moved.join(shared1).unwrap(), moved);

        // Shared + Exclusive = Error
        assert!(shared1.join(excl1).is_err());

        // Different exclusives = Error
        assert!(excl1.join(excl2).is_err());

        // Same exclusive = OK
        assert_eq!(excl1.join(excl1).unwrap(), excl1);
    }

    #[test]
    fn test_borrow_state_widen() {
        let unborrowed = BorrowState2::UNBORROWED;
        let shared1 = BorrowState2::shared(1);
        let shared2 = BorrowState2::shared(2);
        let excl1 = BorrowState2::exclusive(1);
        let moved = BorrowState2::MOVED;

        // Sticky: once shared, stays shared
        assert_eq!(BorrowState2::widen(shared1, unborrowed).unwrap(), shared1);

        // Sticky: once exclusive, stays exclusive
        assert_eq!(BorrowState2::widen(excl1, unborrowed).unwrap(), excl1);

        // Max count on shared
        assert_eq!(BorrowState2::widen(shared1, shared2).unwrap(), shared2);

        // Moved is sticky
        assert_eq!(BorrowState2::widen(shared1, moved).unwrap(), moved);

        // Shared + Exclusive = Error
        assert!(BorrowState2::widen(shared1, excl1).is_err());
    }

    #[test]
    fn test_bitset() {
        let mut bs = BitSet::new();

        assert!(!bs.contains(5));
        bs.insert(5);
        assert!(bs.contains(5));

        bs.insert(100);
        bs.insert(200);

        let items: Vec<_> = bs.iter().collect();
        assert_eq!(items, vec![5, 100, 200]);

        bs.remove(100);
        assert!(!bs.contains(100));
    }

    #[test]
    fn test_state_vec_basic() {
        let mut sv = BorrowStateVec::new(10);

        assert_eq!(sv.get(PlaceId(0)), BorrowState2::UNBORROWED);

        sv.set(PlaceId(0), BorrowState2::shared(1));
        assert_eq!(sv.get(PlaceId(0)), BorrowState2::shared(1));

        assert_eq!(sv.active_count(), 1);
    }

    #[test]
    fn test_state_vec_equality() {
        let mut sv1 = BorrowStateVec::new(10);
        let mut sv2 = BorrowStateVec::new(10);

        assert!(sv1.equals(&sv2));

        sv1.set(PlaceId(5), BorrowState2::MOVED);
        assert!(!sv1.equals(&sv2));

        sv2.set(PlaceId(5), BorrowState2::MOVED);
        assert!(sv1.equals(&sv2));
    }

    #[test]
    fn test_state_vec_merge() {
        let mut sv1 = BorrowStateVec::new(10);
        let mut sv2 = BorrowStateVec::new(10);

        sv1.set(PlaceId(0), BorrowState2::shared(1));
        sv2.set(PlaceId(0), BorrowState2::shared(2));

        sv1.merge(&sv2).unwrap();

        // Should have max count
        assert_eq!(sv1.get(PlaceId(0)), BorrowState2::shared(2));
    }

    #[test]
    fn test_state_vec_merge_conflict() {
        let mut sv1 = BorrowStateVec::new(10);
        let mut sv2 = BorrowStateVec::new(10);

        sv1.set(PlaceId(0), BorrowState2::shared(1));
        sv2.set(PlaceId(0), BorrowState2::exclusive(1));

        let result = sv1.merge(&sv2);
        assert!(result.is_err());
    }

    #[test]
    fn test_borrow_state_size() {
        // Verify our struct is the expected size (8 bytes)
        assert_eq!(std::mem::size_of::<BorrowState2>(), 8);
        assert_eq!(std::mem::align_of::<BorrowState2>(), 4);
    }
}
