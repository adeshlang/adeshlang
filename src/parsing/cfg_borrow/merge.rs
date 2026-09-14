//! Borrow State Merge Rules
//!
//! Implements the merge function for combining borrow states at CFG join points.
//! This is the critical component for sound borrow checking across control flow.
//!
//! # Merge Rules (Summary)
//!
//! | Left | Right | Result |
//! |------|-------|--------|
//! | Unborrowed | X | X |
//! | Shared | Shared | Shared(max count) |
//! | Exclusive(same) | Exclusive(same) | Exclusive |
//! | Shared | Exclusive | ERROR |
//! | Moved | Any | Moved |
//! | Freed | Any | ERROR |

use super::errors::{MergeError, MergeErrorKind};
use super::{BorrowId, BorrowStateMap, BranchInfo, CfgBorrowState, SourceSpan};
use std::collections::{HashMap, HashSet};

/// Result of merging borrow states
pub type MergeResult = Result<BorrowStateMap, MergeError>;

/// Merge borrow states from multiple predecessor blocks
///
/// This is called at CFG join points (after if/else, loops, etc.)
/// to compute the combined borrow state that must be valid for
/// code after the join.
pub fn merge_states(predecessors: &[&BorrowStateMap], join_span: SourceSpan) -> MergeResult {
    if predecessors.is_empty() {
        return Ok(HashMap::new());
    }

    if predecessors.len() == 1 {
        return Ok(predecessors[0].clone());
    }

    // Collect all variables from all predecessors
    let all_vars: HashSet<&String> = predecessors.iter().flat_map(|p| p.keys()).collect();

    let mut result = HashMap::new();
    let mut errors = Vec::new();

    for var in all_vars {
        let states: Vec<&CfgBorrowState> = predecessors
            .iter()
            .map(|p| p.get(var).unwrap_or(&CfgBorrowState::Unborrowed))
            .collect();

        match merge_single_var(var.clone(), &states, join_span) {
            Ok(merged) => {
                result.insert(var.clone(), merged);
            }
            Err(e) => {
                errors.push(e);
                // Continue processing other variables to collect all errors
                // Use a conservative state for the variable
                result.insert(
                    var.clone(),
                    CfgBorrowState::Moved {
                        moved_at: join_span,
                    },
                );
            }
        }
    }

    if errors.is_empty() {
        Ok(result)
    } else {
        // Return the first error (could aggregate in future)
        Err(errors.into_iter().next().unwrap())
    }
}

/// Merge borrow states for a single variable from multiple paths
///
/// # Rules
///
/// 1. Unborrowed is the "bottom" of the lattice - if one path has Unborrowed
///    and another has a constraint, we take the constraint.
///
/// 2. Shared borrows can merge (take max count, union origins)
///
/// 3. Exclusive borrows can only merge if they're the same borrow (same ID)
///
/// 4. Shared + Exclusive = ERROR (incompatible access modes)
///
/// 5. Moved in any path = Moved after join (conservative)
///
/// 6. Freed in any path = ERROR (cannot safely use after)
pub fn merge_single_var(
    var: String,
    states: &[&CfgBorrowState],
    join_span: SourceSpan,
) -> Result<CfgBorrowState, MergeError> {
    // Filter out Unborrowed (they don't constrain)
    let non_trivial: Vec<&CfgBorrowState> = states
        .iter()
        .filter(|s| !matches!(s, CfgBorrowState::Unborrowed))
        .cloned()
        .collect();

    // All paths have Unborrowed
    if non_trivial.is_empty() {
        return Ok(CfgBorrowState::Unborrowed);
    }

    // Check for Freed in any path - this is always an error
    if let Some(freed) = non_trivial
        .iter()
        .find(|s| matches!(s, CfgBorrowState::Freed { .. }))
    {
        let freed_at = match freed {
            CfgBorrowState::Freed { freed_at } => *freed_at,
            _ => unreachable!(),
        };
        return Err(MergeError {
            variable: var,
            kind: MergeErrorKind::FreedAcrossBranches {
                freed_in: vec![BranchInfo::new("branch containing free", freed_at)],
            },
            join_location: join_span,
        });
    }

    // Check for Moved in any path
    let moved: Vec<&&CfgBorrowState> = non_trivial
        .iter()
        .filter(|s| matches!(s, CfgBorrowState::Moved { .. }))
        .collect();

    if !moved.is_empty() {
        // If ANY path moves the variable, it must be considered moved after the join
        // This is conservative but sound
        let moved_at = match moved[0] {
            CfgBorrowState::Moved { moved_at } => *moved_at,
            _ => unreachable!(),
        };
        return Ok(CfgBorrowState::Moved { moved_at });
    }

    // Now we only have SharedBorrowed or ExclusiveBorrowed
    let shared: Vec<&CfgBorrowState> = non_trivial
        .iter()
        .filter(|s| matches!(s, CfgBorrowState::SharedBorrowed { .. }))
        .cloned()
        .collect();

    let exclusive: Vec<&CfgBorrowState> = non_trivial
        .iter()
        .filter(|s| matches!(s, CfgBorrowState::ExclusiveBorrowed { .. }))
        .cloned()
        .collect();

    // Conflict: shared in one path, exclusive in another
    if !shared.is_empty() && !exclusive.is_empty() {
        let shared_info = match shared[0] {
            CfgBorrowState::SharedBorrowed { borrow_origins, .. } => BranchInfo::new(
                "branch with shared borrow",
                borrow_origins.first().copied().unwrap_or_default(),
            ),
            _ => unreachable!(),
        };
        let exclusive_info = match exclusive[0] {
            CfgBorrowState::ExclusiveBorrowed { borrow_origin, .. } => {
                BranchInfo::new("branch with exclusive borrow", *borrow_origin)
            }
            _ => unreachable!(),
        };

        return Err(MergeError {
            variable: var,
            kind: MergeErrorKind::BorrowKindConflict {
                shared_branch: shared_info,
                exclusive_branch: exclusive_info,
            },
            join_location: join_span,
        });
    }

    // All paths have exclusive borrows
    if !exclusive.is_empty() {
        // Extract borrow IDs
        let borrow_ids: Vec<BorrowId> = exclusive
            .iter()
            .filter_map(|s| match s {
                CfgBorrowState::ExclusiveBorrowed { borrow_id, .. } => Some(*borrow_id),
                _ => None,
            })
            .collect();

        // Check if all exclusive borrows are the same
        if borrow_ids.iter().all(|id| *id == borrow_ids[0]) {
            // Same borrow across all paths - OK
            return Ok(exclusive[0].clone());
        } else {
            // Different exclusive borrows in different branches
            let branches: Vec<BranchInfo> = exclusive
                .iter()
                .map(|s| match s {
                    CfgBorrowState::ExclusiveBorrowed {
                        borrow_origin,
                        borrow_id,
                    } => BranchInfo::new(
                        format!("branch with exclusive borrow (id={})", borrow_id),
                        *borrow_origin,
                    ),
                    _ => unreachable!(),
                })
                .collect();

            return Err(MergeError {
                variable: var,
                kind: MergeErrorKind::DifferentExclusiveBorrows { branches },
                join_location: join_span,
            });
        }
    }

    // All paths have shared borrows - merge them
    if !shared.is_empty() {
        let mut max_count = 0;
        let mut all_origins = Vec::new();

        for state in &shared {
            if let CfgBorrowState::SharedBorrowed {
                count,
                borrow_origins,
            } = state
            {
                max_count = max_count.max(*count);
                all_origins.extend(borrow_origins.iter().cloned());
            }
        }

        // Deduplicate origins
        all_origins.sort_by_key(|s| (s.start, s.end));
        all_origins.dedup();

        return Ok(CfgBorrowState::SharedBorrowed {
            count: max_count,
            borrow_origins: all_origins,
        });
    }

    // Fallback - should not reach here
    Ok(CfgBorrowState::Unborrowed)
}

/// Helper to check if two borrow states are compatible for merging
pub fn are_compatible(left: &CfgBorrowState, right: &CfgBorrowState) -> bool {
    match (left, right) {
        // Freed is never compatible with anything - check first
        (CfgBorrowState::Freed { .. }, _) | (_, CfgBorrowState::Freed { .. }) => false,
        // Unborrowed is compatible with everything else
        (CfgBorrowState::Unborrowed, _) | (_, CfgBorrowState::Unborrowed) => true,
        (CfgBorrowState::SharedBorrowed { .. }, CfgBorrowState::SharedBorrowed { .. }) => true,
        (
            CfgBorrowState::ExclusiveBorrowed { borrow_id: id1, .. },
            CfgBorrowState::ExclusiveBorrowed { borrow_id: id2, .. },
        ) => id1 == id2,
        (CfgBorrowState::Moved { .. }, _) | (_, CfgBorrowState::Moved { .. }) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize) -> SourceSpan {
        SourceSpan::new(start, start + 1)
    }

    #[test]
    fn test_merge_empty() {
        let result = merge_states(&[], span(0)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_merge_single() {
        let mut state = HashMap::new();
        state.insert("x".to_string(), CfgBorrowState::Unborrowed);
        let result = merge_states(&[&state], span(0)).unwrap();
        assert_eq!(result.get("x"), Some(&CfgBorrowState::Unborrowed));
    }

    #[test]
    fn test_merge_unborrowed_with_shared() {
        let mut state1 = HashMap::new();
        state1.insert("x".to_string(), CfgBorrowState::Unborrowed);

        let mut state2 = HashMap::new();
        state2.insert(
            "x".to_string(),
            CfgBorrowState::SharedBorrowed {
                count: 1,
                borrow_origins: vec![span(10)],
            },
        );

        let result = merge_states(&[&state1, &state2], span(0)).unwrap();
        assert!(matches!(
            result.get("x"),
            Some(CfgBorrowState::SharedBorrowed { .. })
        ));
    }

    #[test]
    fn test_merge_shared_with_shared() {
        let mut state1 = HashMap::new();
        state1.insert(
            "x".to_string(),
            CfgBorrowState::SharedBorrowed {
                count: 1,
                borrow_origins: vec![span(10)],
            },
        );

        let mut state2 = HashMap::new();
        state2.insert(
            "x".to_string(),
            CfgBorrowState::SharedBorrowed {
                count: 2,
                borrow_origins: vec![span(20)],
            },
        );

        let result = merge_states(&[&state1, &state2], span(0)).unwrap();
        if let Some(CfgBorrowState::SharedBorrowed {
            count,
            borrow_origins,
        }) = result.get("x")
        {
            assert_eq!(*count, 2);
            assert_eq!(borrow_origins.len(), 2);
        } else {
            panic!("Expected SharedBorrowed");
        }
    }

    #[test]
    fn test_merge_exclusive_same_id() {
        let mut state1 = HashMap::new();
        state1.insert(
            "x".to_string(),
            CfgBorrowState::ExclusiveBorrowed {
                borrow_origin: span(10),
                borrow_id: 1,
            },
        );

        let mut state2 = HashMap::new();
        state2.insert(
            "x".to_string(),
            CfgBorrowState::ExclusiveBorrowed {
                borrow_origin: span(20),
                borrow_id: 1,
            },
        );

        let result = merge_states(&[&state1, &state2], span(0)).unwrap();
        assert!(matches!(
            result.get("x"),
            Some(CfgBorrowState::ExclusiveBorrowed { borrow_id: 1, .. })
        ));
    }

    #[test]
    fn test_merge_exclusive_different_id_error() {
        let mut state1 = HashMap::new();
        state1.insert(
            "x".to_string(),
            CfgBorrowState::ExclusiveBorrowed {
                borrow_origin: span(10),
                borrow_id: 1,
            },
        );

        let mut state2 = HashMap::new();
        state2.insert(
            "x".to_string(),
            CfgBorrowState::ExclusiveBorrowed {
                borrow_origin: span(20),
                borrow_id: 2,
            },
        );

        let result = merge_states(&[&state1, &state2], span(0));
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(
                e.kind,
                MergeErrorKind::DifferentExclusiveBorrows { .. }
            ));
        }
    }

    #[test]
    fn test_merge_shared_exclusive_error() {
        let mut state1 = HashMap::new();
        state1.insert(
            "x".to_string(),
            CfgBorrowState::SharedBorrowed {
                count: 1,
                borrow_origins: vec![span(10)],
            },
        );

        let mut state2 = HashMap::new();
        state2.insert(
            "x".to_string(),
            CfgBorrowState::ExclusiveBorrowed {
                borrow_origin: span(20),
                borrow_id: 1,
            },
        );

        let result = merge_states(&[&state1, &state2], span(0));
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e.kind, MergeErrorKind::BorrowKindConflict { .. }));
        }
    }

    #[test]
    fn test_merge_moved_with_any() {
        let mut state1 = HashMap::new();
        state1.insert(
            "x".to_string(),
            CfgBorrowState::Moved { moved_at: span(10) },
        );

        let mut state2 = HashMap::new();
        state2.insert(
            "x".to_string(),
            CfgBorrowState::SharedBorrowed {
                count: 1,
                borrow_origins: vec![span(20)],
            },
        );

        let result = merge_states(&[&state1, &state2], span(0)).unwrap();
        assert!(matches!(
            result.get("x"),
            Some(CfgBorrowState::Moved { .. })
        ));
    }

    #[test]
    fn test_merge_moved_with_unborrowed() {
        let mut state1 = HashMap::new();
        state1.insert(
            "x".to_string(),
            CfgBorrowState::Moved { moved_at: span(10) },
        );

        let mut state2 = HashMap::new();
        state2.insert("x".to_string(), CfgBorrowState::Unborrowed);

        let result = merge_states(&[&state1, &state2], span(0)).unwrap();
        // Conservative: if moved in any path, consider moved
        assert!(matches!(
            result.get("x"),
            Some(CfgBorrowState::Moved { .. })
        ));
    }

    #[test]
    fn test_merge_freed_error() {
        let mut state1 = HashMap::new();
        state1.insert(
            "x".to_string(),
            CfgBorrowState::Freed { freed_at: span(10) },
        );

        let mut state2 = HashMap::new();
        state2.insert("x".to_string(), CfgBorrowState::Unborrowed);

        let result = merge_states(&[&state1, &state2], span(0));
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e.kind, MergeErrorKind::FreedAcrossBranches { .. }));
        }
    }

    #[test]
    fn test_are_compatible() {
        let unb = CfgBorrowState::Unborrowed;
        let shared = CfgBorrowState::SharedBorrowed {
            count: 1,
            borrow_origins: vec![],
        };
        let excl1 = CfgBorrowState::ExclusiveBorrowed {
            borrow_origin: span(0),
            borrow_id: 1,
        };
        let excl2 = CfgBorrowState::ExclusiveBorrowed {
            borrow_origin: span(0),
            borrow_id: 2,
        };
        let moved = CfgBorrowState::Moved { moved_at: span(0) };
        let freed = CfgBorrowState::Freed { freed_at: span(0) };

        // Unborrowed is compatible with everything
        assert!(are_compatible(&unb, &shared));
        assert!(are_compatible(&unb, &excl1));
        assert!(are_compatible(&unb, &moved));

        // Shared with shared
        assert!(are_compatible(&shared, &shared));

        // Exclusive same ID
        assert!(are_compatible(&excl1, &excl1));

        // Exclusive different ID
        assert!(!are_compatible(&excl1, &excl2));

        // Shared with exclusive
        assert!(!are_compatible(&shared, &excl1));

        // Freed is not compatible
        assert!(!are_compatible(&freed, &shared));
        assert!(!are_compatible(&freed, &unb));
    }
}
