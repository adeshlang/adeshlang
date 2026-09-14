//! BitSet - Compact Bit Storage

use crate::stdlib::adesh_alloc::vec::Vec;

/// A space-efficient set of bits/booleans
pub struct BitSet {
    blocks: Vec<u64>,
    len: usize,
}

impl BitSet {
    /// Creates an empty BitSet
    pub fn new() -> Self {
        BitSet {
            blocks: Vec::new(),
            len: 0,
        }
    }

    /// Sets the bit at `index` to true
    pub fn set(&mut self, index: usize) {
        let block_idx = index / 64;
        let bit_idx = index % 64;
        while self.blocks.len() <= block_idx {
            self.blocks.push(0);
        }
        let block = self.blocks.get_mut(block_idx).unwrap();
        *block |= 1 << bit_idx;
        if index >= self.len {
            self.len = index + 1;
        }
    }

    /// Clears the bit at `index` (sets to false)
    pub fn clear(&mut self, index: usize) {
        let block_idx = index / 64;
        let bit_idx = index % 64;
        if block_idx < self.blocks.len() {
            let block = self.blocks.get_mut(block_idx).unwrap();
            *block &= !(1 << bit_idx);
        }
    }

    /// Toggles the bit at `index`
    pub fn toggle(&mut self, index: usize) {
        let block_idx = index / 64;
        let bit_idx = index % 64;
        while self.blocks.len() <= block_idx {
            self.blocks.push(0);
        }
        let block = self.blocks.get_mut(block_idx).unwrap();
        *block ^= 1 << bit_idx;
        if index >= self.len {
            self.len = index + 1;
        }
    }

    /// Tests the bit at `index`
    pub fn test(&self, index: usize) -> bool {
        let block_idx = index / 64;
        let bit_idx = index % 64;
        if block_idx < self.blocks.len() {
            let block = self.blocks.get(block_idx).unwrap();
            (*block & (1 << bit_idx)) != 0
        } else {
            false
        }
    }

    /// Returns the length (highest index set + 1)
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for BitSet {
    fn default() -> Self {
        Self::new()
    }
}
