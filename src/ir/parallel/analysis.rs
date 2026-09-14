//! Parallel loop analysis — dependency and independence checking

/// Whether a loop iteration is independent of others
#[derive(Debug, Clone)]
pub struct IterationIndependence {
    pub is_independent: bool,
    pub read_vars: Vec<String>,
    pub write_vars: Vec<String>,
    pub has_reduction: bool,
    pub reduction_var: Option<String>,
    pub side_effects: bool,
}

/// Analyze a parallel loop for safety
pub struct ParallelLoopAnalyzer;

impl ParallelLoopAnalyzer {
    pub fn check_independence(
        body_reads: &[String],
        body_writes: &[String],
        index_var: &str,
        has_side_effects: bool,
    ) -> IterationIndependence {
        // Each iteration must only write to distinct elements indexed by index_var
        let write_conflicts = body_writes
            .iter()
            .any(|w| w != index_var && body_reads.contains(w));

        let is_independent = !write_conflicts && !has_side_effects;

        IterationIndependence {
            is_independent,
            read_vars: body_reads.to_vec(),
            write_vars: body_writes.to_vec(),
            has_reduction: false,
            reduction_var: None,
            side_effects: has_side_effects,
        }
    }

    /// Check if borrow splitting makes parallel access safe
    pub fn check_disjoint_ranges(left_end: usize, right_start: usize) -> bool {
        left_end <= right_start
    }
}

/// Range-aware borrow splitting for parallel algorithms
#[derive(Debug, Clone)]
pub struct BorrowSplit {
    pub left_var: String,
    pub right_var: String,
    pub split_point: usize,
}

impl BorrowSplit {
    pub fn new(var: &str, split_at: usize) -> (BorrowSplit, BorrowSplit) {
        (
            BorrowSplit {
                left_var: format!("{}_left", var),
                right_var: format!("{}_right", var),
                split_point: split_at,
            },
            BorrowSplit {
                left_var: format!("{}_left", var),
                right_var: format!("{}_right", var),
                split_point: split_at,
            },
        )
    }

    pub fn is_disjoint(&self) -> bool {
        ParallelLoopAnalyzer::check_disjoint_ranges(self.split_point, self.split_point)
    }
}
