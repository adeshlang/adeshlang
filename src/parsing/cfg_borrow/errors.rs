//! CFG-Specific Error Types
//!
//! Rich error types for borrow violations detected during CFG analysis.
//! These errors provide context about which branches caused conflicts.

use super::{BranchInfo, SourceSpan};
use crate::parsing::borrow_check::BorrowCheckError;

/// Error detected during CFG borrow state merge
#[derive(Debug, Clone)]
pub struct MergeError {
    /// Variable that has conflicting states
    pub variable: String,
    /// Kind of merge error
    pub kind: MergeErrorKind,
    /// Location of the join point
    pub join_location: SourceSpan,
}

/// Specific kinds of merge errors
#[derive(Debug, Clone)]
pub enum MergeErrorKind {
    /// Conflicting borrow kinds across branches (shared vs exclusive)
    BorrowKindConflict {
        shared_branch: BranchInfo,
        exclusive_branch: BranchInfo,
    },

    /// Different exclusive borrows in different branches
    DifferentExclusiveBorrows { branches: Vec<BranchInfo> },

    /// Variable was freed in one or more branches
    FreedAcrossBranches { freed_in: Vec<BranchInfo> },
}

impl MergeError {
    /// Convert to a human-readable error message
    pub fn message(&self) -> String {
        match &self.kind {
            MergeErrorKind::BorrowKindConflict {
                shared_branch,
                exclusive_branch,
            } => {
                format!(
                    "conflicting borrow states for `{}` across branches: \
                     shared borrow in {} (at {:?}), \
                     exclusive borrow in {} (at {:?})",
                    self.variable,
                    shared_branch.description,
                    shared_branch.span,
                    exclusive_branch.description,
                    exclusive_branch.span,
                )
            }
            MergeErrorKind::DifferentExclusiveBorrows { branches } => {
                let branch_info: Vec<String> = branches
                    .iter()
                    .map(|b| format!("{} (at {:?})", b.description, b.span))
                    .collect();
                format!(
                    "different exclusive borrows of `{}` in different branches: {}",
                    self.variable,
                    branch_info.join(", "),
                )
            }
            MergeErrorKind::FreedAcrossBranches { freed_in } => {
                let branch_info: Vec<String> = freed_in
                    .iter()
                    .map(|b| format!("{} (at {:?})", b.description, b.span))
                    .collect();
                format!(
                    "`{}` was freed in branch(es): {}; cannot safely use after join",
                    self.variable,
                    branch_info.join(", "),
                )
            }
        }
    }

    /// Convert to the standard BorrowCheckError type
    pub fn to_borrow_check_error(&self) -> BorrowCheckError {
        BorrowCheckError::BorrowStateMismatch {
            variable: self.variable.clone(),
            paths: Vec::new(), // Could be enhanced to include actual states
        }
    }
}

/// Comprehensive CFG borrow error
#[derive(Debug)]
pub enum CfgBorrowError {
    /// Error from state merge at join point
    MergeError(MergeError),

    /// Use of variable after it may have been moved
    UseAfterMaybeMoved {
        variable: String,
        moved_in_branch: BranchInfo,
        use_location: SourceSpan,
    },

    /// Use of variable after it may have been freed
    UseAfterMaybeFreed {
        variable: String,
        freed_in_branch: BranchInfo,
        use_location: SourceSpan,
    },

    /// Attempt to borrow while already borrowed incompatibly
    BorrowConflict {
        variable: String,
        existing_borrow: BranchInfo,
        new_borrow: SourceSpan,
        existing_is_exclusive: bool,
        new_is_exclusive: bool,
    },

    /// Attempt to free while borrowed
    FreeWhileBorrowed {
        variable: String,
        borrowed_at: SourceSpan,
        freed_at: SourceSpan,
    },

    /// Attempt to move while borrowed
    MoveWhileBorrowed {
        variable: String,
        borrowed_at: SourceSpan,
        move_at: SourceSpan,
    },

    /// Loop does not reach fixpoint (internal error)
    FixpointNotReached {
        function_name: String,
        iterations: usize,
    },
}

impl CfgBorrowError {
    /// Convert to a human-readable error message
    pub fn message(&self) -> String {
        match self {
            Self::MergeError(e) => e.message(),

            Self::UseAfterMaybeMoved {
                variable,
                moved_in_branch,
                use_location,
            } => {
                format!(
                    "use of possibly moved value `{}` at {:?}; \
                     moved in {} (at {:?})",
                    variable, use_location, moved_in_branch.description, moved_in_branch.span
                )
            }

            Self::UseAfterMaybeFreed {
                variable,
                freed_in_branch,
                use_location,
            } => {
                format!(
                    "use of possibly freed value `{}` at {:?}; \
                     freed in {} (at {:?})",
                    variable, use_location, freed_in_branch.description, freed_in_branch.span
                )
            }

            Self::BorrowConflict {
                variable,
                existing_borrow,
                new_borrow,
                existing_is_exclusive,
                new_is_exclusive,
            } => {
                let existing_kind = if *existing_is_exclusive {
                    "exclusive"
                } else {
                    "shared"
                };
                let new_kind = if *new_is_exclusive {
                    "exclusive"
                } else {
                    "shared"
                };
                format!(
                    "cannot {} borrow `{}` at {:?}; \
                     already {} borrowed in {} (at {:?})",
                    new_kind,
                    variable,
                    new_borrow,
                    existing_kind,
                    existing_borrow.description,
                    existing_borrow.span
                )
            }

            Self::FreeWhileBorrowed {
                variable,
                borrowed_at,
                freed_at,
            } => {
                format!(
                    "cannot free `{}` at {:?}; still borrowed (at {:?})",
                    variable, freed_at, borrowed_at
                )
            }

            Self::MoveWhileBorrowed {
                variable,
                borrowed_at,
                move_at,
            } => {
                format!(
                    "cannot move `{}` at {:?}; still borrowed (at {:?})",
                    variable, move_at, borrowed_at
                )
            }

            Self::FixpointNotReached {
                function_name,
                iterations,
            } => {
                format!(
                    "internal error: fixpoint not reached for function `{}` after {} iterations",
                    function_name, iterations
                )
            }
        }
    }

    /// Get the primary source span for this error
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::MergeError(e) => e.join_location,
            Self::UseAfterMaybeMoved { use_location, .. } => *use_location,
            Self::UseAfterMaybeFreed { use_location, .. } => *use_location,
            Self::BorrowConflict { new_borrow, .. } => *new_borrow,
            Self::FreeWhileBorrowed { freed_at, .. } => *freed_at,
            Self::MoveWhileBorrowed { move_at, .. } => *move_at,
            Self::FixpointNotReached { .. } => SourceSpan::default(),
        }
    }

    /// Convert to the standard BorrowCheckError type
    pub fn to_borrow_check_error(&self) -> BorrowCheckError {
        match self {
            Self::MergeError(e) => e.to_borrow_check_error(),

            Self::UseAfterMaybeMoved { variable, .. }
            | Self::UseAfterMaybeFreed { variable, .. } => BorrowCheckError::UseAfterFree {
                variable: variable.clone(),
                freed_at: 0,
                use_at: self.span().start,
            },

            Self::BorrowConflict {
                variable,
                existing_borrow,
                new_borrow,
                ..
            } => BorrowCheckError::MutableBorrowWhileBorrowed {
                variable: variable.clone(),
                previous_borrow: existing_borrow.span.start,
                mutable_borrow_at: new_borrow.start,
            },

            Self::FreeWhileBorrowed {
                variable,
                borrowed_at,
                freed_at,
            } => BorrowCheckError::FreeWhileBorrowed {
                variable: variable.clone(),
                borrowed_at: borrowed_at.start,
                freed_at: freed_at.start,
            },

            Self::MoveWhileBorrowed {
                variable,
                borrowed_at,
                move_at,
            } => BorrowCheckError::MoveWhileBorrowed {
                variable: variable.clone(),
                borrowed_at: borrowed_at.start,
                move_at: move_at.start,
            },

            Self::FixpointNotReached { .. } => {
                // This shouldn't happen in practice
                BorrowCheckError::BorrowStateMismatch {
                    variable: String::new(),
                    paths: Vec::new(),
                }
            }
        }
    }
}

impl From<MergeError> for CfgBorrowError {
    fn from(e: MergeError) -> Self {
        Self::MergeError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize) -> SourceSpan {
        SourceSpan::new(start, start + 1)
    }

    #[test]
    fn test_merge_error_message() {
        let error = MergeError {
            variable: "x".to_string(),
            kind: MergeErrorKind::BorrowKindConflict {
                shared_branch: BranchInfo::new("then branch", span(10)),
                exclusive_branch: BranchInfo::new("else branch", span(20)),
            },
            join_location: span(30),
        };

        let msg = error.message();
        assert!(msg.contains("conflicting borrow states"));
        assert!(msg.contains("x"));
        assert!(msg.contains("shared"));
        assert!(msg.contains("exclusive"));
    }

    #[test]
    fn test_cfg_borrow_error_messages() {
        let error = CfgBorrowError::UseAfterMaybeMoved {
            variable: "data".to_string(),
            moved_in_branch: BranchInfo::new("if branch", span(100)),
            use_location: span(200),
        };

        let msg = error.message();
        assert!(msg.contains("data"));
        assert!(msg.contains("moved"));
    }

    #[test]
    fn test_error_span() {
        let error = CfgBorrowError::FreeWhileBorrowed {
            variable: "ptr".to_string(),
            borrowed_at: span(10),
            freed_at: span(50),
        };

        assert_eq!(error.span().start, 50);
    }

    #[test]
    fn test_to_borrow_check_error() {
        let error = CfgBorrowError::MoveWhileBorrowed {
            variable: "x".to_string(),
            borrowed_at: span(10),
            move_at: span(20),
        };

        let check_error = error.to_borrow_check_error();
        assert!(matches!(
            check_error,
            BorrowCheckError::MoveWhileBorrowed { .. }
        ));
    }
}
