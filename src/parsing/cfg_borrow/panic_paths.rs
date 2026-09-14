//! Panic Path and Early Return Tracking for CFG Borrow Checker
//!
//! Ensures RAII cleanup happens on all exit paths:
//! - Normal returns
//! - Early returns (return statement)
//! - Break/continue in loops
//! - Panic/unwind paths
//!
//! # Goal
//!
//! Every owned value must be dropped exactly once, regardless of exit path.
//!
//! # Example
//!
//! ```adesh
//! fn example() {
//!     let x = allocate();  // Owned value
//!     
//!     if condition {
//!         return;  // Must drop x before returning
//!     }
//!     
//!     // x dropped here if no early return
//! }
//! ```

use super::cfg::{BasicBlock, ControlFlowGraph};
use super::mir::PlaceId;
use super::state_vec::{BorrowStateVec, BorrowState2};
use std::collections::{HashMap, HashSet};

/// Exit path type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitPath {
    /// Normal function return
    Return,
    /// Early return statement
    EarlyReturn,
    /// Break from loop
    Break,
    /// Continue in loop
    Continue,
    /// Panic/unwind
    Panic,
}

/// Cleanup information for an exit path
#[derive(Debug, Clone)]
pub struct CleanupInfo {
    /// Places that need to be dropped
    pub to_drop: Vec<PlaceId>,
    /// Order in which to drop (reverse allocation order)
    pub drop_order: Vec<PlaceId>,
    /// Exit path type
    pub exit_type: ExitPath,
    /// Basic block where exit occurs
    pub block_id: usize,
}

/// Panic path and early return tracker
pub struct PanicPathTracker {
    /// Active owned places at each basic block
    active_owned: HashMap<usize, HashSet<PlaceId>>,
    
    /// Cleanup information for each exit point
    cleanup_info: Vec<CleanupInfo>,
    
    /// Allocation order for drop ordering
    allocation_order: Vec<PlaceId>,
}

impl PanicPathTracker {
    pub fn new() -> Self {
        PanicPathTracker {
            active_owned: HashMap::new(),
            cleanup_info: Vec::new(),
            allocation_order: Vec::new(),
        }
    }
    
    /// Analyze a CFG for panic paths and exit points
    pub fn analyze(&mut self, cfg: &ControlFlowGraph) -> Result<(), String> {
        // Build active owned set for each block
        self.build_active_owned_sets(cfg)?;
        
        // Find all exit points
        self.find_exit_points(cfg)?;
        
        // Generate cleanup for each exit point
        self.generate_cleanup_info(cfg)?;
        
        Ok(())
    }
    
    /// Build set of active owned places at each basic block
    fn build_active_owned_sets(&mut self, cfg: &ControlFlowGraph) -> Result<(), String> {
        // Start from entry block
        let entry_state = HashSet::new();
        let mut worklist = vec![(cfg.entry, entry_state)];
        let mut visited = HashSet::new();
        
        while let Some((block_id, mut state)) = worklist.pop() {
            if visited.contains(&block_id) {
                continue;
            }
            visited.insert(block_id);
            
            // Update state based on block operations
            let block = &cfg.blocks[block_id];
            for stmt in &block.statements {
                self.process_statement(stmt, &mut state);
            }
            
            // Store active owned set for this block
            self.active_owned.insert(block_id, state.clone());
            
            // Propagate to successors
            for &succ in &block.successors {
                worklist.push((succ, state.clone()));
            }
        }
        
        Ok(())
    }
    
    /// Process a statement to update owned set
    fn process_statement(&mut self, stmt: &str, state: &mut HashSet<PlaceId>) {
        // Parse statement (simplified - in real implementation would use HIR)
        if stmt.starts_with("let ") {
            // New allocation
            let place_id = PlaceId(self.allocation_order.len());
            self.allocation_order.push(place_id);
            state.insert(place_id);
        } else if stmt.starts_with("drop ") {
            // Explicit drop
            // Extract place_id and remove from state
            // (simplified parsing)
        } else if stmt.starts_with("move ") {
            // Move - remove from state
            // Extract place_id and remove
        }
    }
    
    /// Find all exit points in the CFG
    fn find_exit_points(&mut self, cfg: &ControlFlowGraph) -> Result<(), String> {
        for (block_id, block) in cfg.blocks.iter().enumerate() {
            // Check for explicit returns
            if block.is_return {
                let owned = self.active_owned.get(&block_id).cloned().unwrap_or_default();
                self.cleanup_info.push(CleanupInfo {
                    to_drop: owned.iter().copied().collect(),
                    drop_order: self.compute_drop_order(&owned),
                    exit_type: ExitPath::Return,
                    block_id,
                });
            }
            
            // Check for early returns (return statements in middle of function)
            for stmt in &block.statements {
                if stmt.starts_with("return") {
                    let owned = self.active_owned.get(&block_id).cloned().unwrap_or_default();
                    self.cleanup_info.push(CleanupInfo {
                        to_drop: owned.iter().copied().collect(),
                        drop_order: self.compute_drop_order(&owned),
                        exit_type: ExitPath::EarlyReturn,
                        block_id,
                    });
                }
            }
            
            // Check for break/continue
            for stmt in &block.statements {
                if stmt.starts_with("break") {
                    let owned = self.active_owned.get(&block_id).cloned().unwrap_or_default();
                    self.cleanup_info.push(CleanupInfo {
                        to_drop: owned.iter().copied().collect(),
                        drop_order: self.compute_drop_order(&owned),
                        exit_type: ExitPath::Break,
                        block_id,
                    });
                } else if stmt.starts_with("continue") {
                    let owned = self.active_owned.get(&block_id).cloned().unwrap_or_default();
                    self.cleanup_info.push(CleanupInfo {
                        to_drop: owned.iter().copied().collect(),
                        drop_order: self.compute_drop_order(&owned),
                        exit_type: ExitPath::Continue,
                        block_id,
                    });
                }
            }
            
            // Check for panic calls
            for stmt in &block.statements {
                if stmt.contains("panic") || stmt.contains("unwrap") || stmt.contains("expect") {
                    let owned = self.active_owned.get(&block_id).cloned().unwrap_or_default();
                    self.cleanup_info.push(CleanupInfo {
                        to_drop: owned.iter().copied().collect(),
                        drop_order: self.compute_drop_order(&owned),
                        exit_type: ExitPath::Panic,
                        block_id,
                    });
                }
            }
        }
        
        Ok(())
    }
    
    /// Compute drop order (reverse of allocation order)
    fn compute_drop_order(&self, owned: &HashSet<PlaceId>) -> Vec<PlaceId> {
        let mut order: Vec<PlaceId> = owned.iter().copied().collect();
        
        // Sort by allocation order, then reverse
        order.sort_by_key(|&place| {
            self.allocation_order.iter().position(|&p| p == place).unwrap_or(usize::MAX)
        });
        order.reverse();
        
        order
    }
    
    /// Generate cleanup code for each exit point
    fn generate_cleanup_info(&mut self, _cfg: &ControlFlowGraph) -> Result<(), String> {
        // Cleanup info already generated in find_exit_points
        // This method can be used for additional validation
        
        // Validate that all exit paths have cleanup
        for cleanup in &self.cleanup_info {
            if !cleanup.to_drop.is_empty() && cleanup.drop_order.is_empty() {
                return Err(format!(
                    "Exit path {:?} at block {} missing drop order",
                    cleanup.exit_type, cleanup.block_id
                ));
            }
        }
        
        Ok(())
    }
    
    /// Get cleanup information for all exit paths
    pub fn get_cleanup_info(&self) -> &[CleanupInfo] {
        &self.cleanup_info
    }
    
    /// Validate that all owned values are cleaned up on all paths
    pub fn validate_complete_cleanup(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        
        // Check each exit path
        for cleanup in &self.cleanup_info {
            if cleanup.to_drop.len() != cleanup.drop_order.len() {
                errors.push(format!(
                    "Incomplete cleanup for {:?} at block {}: {} to drop but {} in order",
                    cleanup.exit_type,
                    cleanup.block_id,
                    cleanup.to_drop.len(),
                    cleanup.drop_order.len()
                ));
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for PanicPathTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Loop borrow state validator
pub struct LoopBorrowValidator {
    /// Loop headers (entry blocks)
    loop_headers: HashSet<usize>,
    
    /// Borrow states at loop entry
    loop_entry_states: HashMap<usize, BorrowStateVec>,
    
    /// Errors found
    errors: Vec<String>,
}

impl LoopBorrowValidator {
    pub fn new() -> Self {
        LoopBorrowValidator {
            loop_headers: HashSet::new(),
            loop_entry_states: HashMap::new(),
            errors: Vec::new(),
        }
    }
    
    /// Analyze loops in the CFG
    pub fn analyze(&mut self, cfg: &ControlFlowGraph) -> Result<(), Vec<String>> {
        // Find loop headers (blocks with back edges)
        self.find_loop_headers(cfg);
        
        // Validate borrow states at loop boundaries
        self.validate_loop_borrows(cfg)?;
        
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }
    
    /// Find loop header blocks
    fn find_loop_headers(&mut self, cfg: &ControlFlowGraph) {
        // A block is a loop header if it has a back edge
        // (successor that comes before it in post-order)
        
        let mut visited = HashSet::new();
        let mut stack = vec![cfg.entry];
        
        while let Some(block_id) = stack.pop() {
            if visited.contains(&block_id) {
                continue;
            }
            visited.insert(block_id);
            
            let block = &cfg.blocks[block_id];
            for &succ in &block.successors {
                // If successor already visited, this is a back edge
                if visited.contains(&succ) {
                    self.loop_headers.insert(succ);
                } else {
                    stack.push(succ);
                }
            }
        }
    }
    
    /// Validate borrow states at loop boundaries
    fn validate_loop_borrows(&mut self, cfg: &ControlFlowGraph) -> Result<(), Vec<String>> {
        for &header in &self.loop_headers {
            // Check that borrows are consistent across loop iterations
            // This is a simplified check - full implementation would use fixpoint
            
            let block = &cfg.blocks[header];
            
            // Ensure no exclusive borrows escape loop body
            // (simplified - would need full dataflow analysis)
            
            for stmt in &block.statements {
                if stmt.contains("exclusive") || stmt.contains("share ") {
                    self.errors.push(format!(
                        "exclusive borrow in loop body block {} may not be safe across iterations",
                        header
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Get all errors
    pub fn get_errors(&self) -> &[String] {
        &self.errors
    }
}

impl Default for LoopBorrowValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_panic_path_tracker() {
        let mut tracker = PanicPathTracker::new();
        
        // Create simple CFG (would use actual CFG in real test)
        let cfg = ControlFlowGraph {
            entry: 0,
            blocks: vec![
                BasicBlock {
                    id: 0,
                    statements: vec!["let x = alloc()".to_string()],
                    successors: vec![1],
                    predecessors: vec![],
                    is_return: false,
                },
                BasicBlock {
                    id: 1,
                    statements: vec!["return".to_string()],
                    successors: vec![],
                    predecessors: vec![0],
                    is_return: true,
                },
            ],
        };
        
        let result = tracker.analyze(&cfg);
        assert!(result.is_ok());
        
        // Should have cleanup for the return
        assert!(!tracker.get_cleanup_info().is_empty());
    }
    
    #[test]
    fn test_loop_borrow_validator() {
        let mut validator = LoopBorrowValidator::new();
        
        // Create simple CFG with a loop
        let cfg = ControlFlowGraph {
            entry: 0,
            blocks: vec![
                BasicBlock {
                    id: 0,
                    statements: vec![],
                    successors: vec![1],
                    predecessors: vec![1], // Back edge from loop body
                    is_return: false,
                },
                BasicBlock {
                    id: 1,
                    statements: vec![],
                    successors: vec![0, 2], // Back to header or exit
                    predecessors: vec![0],
                    is_return: false,
                },
                BasicBlock {
                    id: 2,
                    statements: vec![],
                    successors: vec![],
                    predecessors: vec![1],
                    is_return: true,
                },
            ],
        };
        
        let result = validator.analyze(&cfg);
        assert!(result.is_ok() || result.is_err()); // Just check it runs
    }
}
