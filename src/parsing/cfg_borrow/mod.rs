//! CFG-Based Borrow Checking
//!
//! This module implements Control Flow Graph (CFG) based borrow state propagation
//! and merging for sound detection of borrow violations across control flow constructs.
//!
//! # Architecture
//!
//! ## Phase 1 (Current - HIR-based)
//! - `cfg`: CFG data structures and construction from HIR
//! - `dataflow`: Forward dataflow analysis with fixpoint iteration
//! - `merge`: Borrow state merge rules at join points
//! - `errors`: CFG-specific error types with rich diagnostics
//!
//! ## Phase 2 (SSA-based - In Progress)
//! - `mir`: SSA-form Mid-level IR with phi nodes
//! - `places`: Place-based tracking with union-find alias resolution
//! - `state_vec`: Dense indexed storage for borrow states
//!
//! # Usage
//!
//! ```ignore
//! use cfg_borrow::CfgBorrowChecker;
//!
//! let cfg = CfgBuilder::build(&hir_function);
//! let checker = CfgBorrowChecker::new();
//! checker.analyze(&cfg)?;
//! ```

// Phase 1: HIR-based analysis (current)
pub mod cfg;
pub mod dataflow;
pub mod errors;
pub mod merge;

// Phase 2: SSA-based analysis (new)
pub mod mir;
pub mod places;
pub mod state_vec;

pub use cfg::{BasicBlock, BlockId, BlockKind, CfgBuilder, ControlFlowGraph};
pub use dataflow::CfgBorrowChecker;
pub use errors::CfgBorrowError;
pub use merge::{merge_single_var, merge_states};

use std::collections::HashMap;

/// Source span for error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Unique identifier for borrow instances
pub type BorrowId = u64;

/// Enhanced borrow state with tracking metadata for CFG analysis
#[derive(Debug, Clone, PartialEq)]
pub enum CfgBorrowState {
    /// Variable is unborrowed and accessible
    Unborrowed,

    /// Variable has one or more shared (immutable) borrows
    SharedBorrowed {
        /// Source locations where borrows were created
        borrow_origins: Vec<SourceSpan>,
        /// Active borrow count
        count: usize,
    },

    /// Variable has an exclusive (mutable) borrow
    ExclusiveBorrowed {
        /// Source location where the exclusive borrow was created
        borrow_origin: SourceSpan,
        /// Unique identifier for this specific borrow instance
        borrow_id: BorrowId,
    },

    /// Variable has been moved and is no longer accessible
    Moved {
        /// Source location where the move occurred
        moved_at: SourceSpan,
    },

    /// Variable has been freed (unsafe block)
    Freed {
        /// Source location where the free occurred
        freed_at: SourceSpan,
    },
}

impl Default for CfgBorrowState {
    fn default() -> Self {
        Self::Unborrowed
    }
}

impl CfgBorrowState {
    /// Returns true if this state represents an error condition
    pub fn is_error_state(&self) -> bool {
        matches!(self, Self::Freed { .. })
    }

    /// Returns true if variable is currently borrowed (shared or exclusive)
    pub fn is_borrowed(&self) -> bool {
        matches!(
            self,
            Self::SharedBorrowed { .. } | Self::ExclusiveBorrowed { .. }
        )
    }

    /// Returns true if variable has been moved
    pub fn is_moved(&self) -> bool {
        matches!(self, Self::Moved { .. })
    }

    /// Returns true if variable has been freed
    pub fn is_freed(&self) -> bool {
        matches!(self, Self::Freed { .. })
    }

    /// Returns true if variable is accessible (not moved or freed)
    pub fn is_accessible(&self) -> bool {
        !matches!(self, Self::Moved { .. } | Self::Freed { .. })
    }
}

/// Map from variable names to their borrow states
pub type BorrowStateMap = HashMap<String, CfgBorrowState>;

/// Information about a branch for error reporting
#[derive(Debug, Clone)]
pub struct BranchInfo {
    /// Which branch (e.g., "then branch of if at line 10")
    pub description: String,
    /// Source span of the relevant operation
    pub span: SourceSpan,
    /// Block ID in the CFG
    pub block_id: Option<usize>,
}

impl BranchInfo {
    pub fn new(description: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            description: description.into(),
            span,
            block_id: None,
        }
    }

    pub fn with_block(mut self, block_id: usize) -> Self {
        self.block_id = Some(block_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrow_state_default() {
        let state = CfgBorrowState::default();
        assert_eq!(state, CfgBorrowState::Unborrowed);
    }

    #[test]
    fn test_borrow_state_checks() {
        let unborrowed = CfgBorrowState::Unborrowed;
        assert!(!unborrowed.is_borrowed());
        assert!(!unborrowed.is_moved());
        assert!(unborrowed.is_accessible());

        let shared = CfgBorrowState::SharedBorrowed {
            borrow_origins: vec![],
            count: 1,
        };
        assert!(shared.is_borrowed());
        assert!(!shared.is_moved());
        assert!(shared.is_accessible());

        let moved = CfgBorrowState::Moved {
            moved_at: SourceSpan::new(0, 1),
        };
        assert!(!moved.is_borrowed());
        assert!(moved.is_moved());
        assert!(!moved.is_accessible());

        let freed = CfgBorrowState::Freed {
            freed_at: SourceSpan::new(0, 1),
        };
        assert!(freed.is_error_state());
        assert!(freed.is_freed());
        assert!(!freed.is_accessible());
    }

    #[test]
    fn test_source_span() {
        let span = SourceSpan::new(10, 20);
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
    }
}
