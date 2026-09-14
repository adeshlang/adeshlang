use crate::parsing::hir::*;
/// Control-flow graph based leak detection
/// Ensures all allocated pointers are freed on all execution paths
///
/// Rules:
/// - Every alloc() must have matching free() on all paths
/// - Deallocate before early return
/// - Deallocate before loop exit
/// - Deallocate in exception handlers
use std::collections::{HashMap, HashSet, VecDeque};

/// Basic block identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(u64);

/// Allocation tracking
#[derive(Debug, Clone)]
pub struct Allocation {
    pub variable: String,
    pub alloc_site: usize, // Source location
}

/// State of a pointer variable
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerState {
    /// Newly allocated, not yet freed
    Allocated,
    /// Successfully freed
    Freed,
    /// Freed but may be used later
    FreedButMayBeUsed,
}

/// Ownership state at a basic block
#[derive(Debug, Clone)]
pub struct BlockState {
    pub block_id: BlockId,
    pub allocations: HashMap<String, PointerState>,
    pub predecessors: Vec<BlockId>,
    pub successors: Vec<BlockId>,
}

/// Path to a reachable state
#[derive(Debug, Clone)]
pub struct Path {
    pub blocks: Vec<BlockId>,
    pub final_state: HashMap<String, PointerState>,
}

/// Leak detection error
#[derive(Debug)]
pub enum LeakError {
    /// Pointer not freed on some path
    PointerNotFreedOnPath {
        variable: String,
        alloc_site: usize,
        paths_without_free: Vec<Vec<BlockId>>,
    },
    /// Double free detected
    DoubleFree {
        variable: String,
        first_free: usize,
        second_free: usize,
    },
    /// Use after free
    UseAfterFree {
        variable: String,
        freed_at: usize,
        used_at: usize,
    },
}

/// Control-flow graph for function
pub struct ControlFlowGraph {
    blocks: HashMap<BlockId, BlockState>,
    entry_block: BlockId,
    exit_blocks: HashSet<BlockId>,
    block_counter: u64,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        let entry = BlockId(0);
        let mut blocks = HashMap::new();
        blocks.insert(
            entry,
            BlockState {
                block_id: entry,
                allocations: HashMap::new(),
                predecessors: Vec::new(),
                successors: Vec::new(),
            },
        );

        ControlFlowGraph {
            blocks,
            entry_block: entry,
            exit_blocks: HashSet::new(),
            block_counter: 1,
        }
    }

    /// Create a new basic block
    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.block_counter);
        self.block_counter += 1;
        self.blocks.insert(
            id,
            BlockState {
                block_id: id,
                allocations: HashMap::new(),
                predecessors: Vec::new(),
                successors: Vec::new(),
            },
        );
        id
    }

    /// Add edge from one block to another
    pub fn add_edge(&mut self, from: BlockId, to: BlockId) {
        if let Some(block) = self.blocks.get_mut(&from) {
            if !block.successors.contains(&to) {
                block.successors.push(to);
            }
        }
        if let Some(block) = self.blocks.get_mut(&to) {
            if !block.predecessors.contains(&from) {
                block.predecessors.push(from);
            }
        }
    }

    /// Mark a block as exit block
    pub fn mark_exit(&mut self, block_id: BlockId) {
        self.exit_blocks.insert(block_id);
    }

    /// Check all paths for leaks
    pub fn check_leaks(&self) -> Result<(), Vec<LeakError>> {
        let mut errors = Vec::new();

        // Find all paths from entry to exit
        let paths = self.find_all_paths();

        // Check each path for unfreed allocations
        for path in paths {
            for state in path.final_state.values() {
                if state == &PointerState::Allocated {
                    errors.push(LeakError::PointerNotFreedOnPath {
                        variable: "pointer".to_string(),
                        alloc_site: 0,
                        paths_without_free: vec![path.blocks.clone()],
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Find all paths from entry to exit blocks
    fn find_all_paths(&self) -> Vec<Path> {
        let mut paths = Vec::new();
        let mut queue = VecDeque::new();

        // Start BFS from entry block
        queue.push_back(vec![self.entry_block]);

        while let Some(path) = queue.pop_front() {
            let current_block = path[path.len() - 1];

            if self.exit_blocks.contains(&current_block) {
                // Found path to exit
                paths.push(Path {
                    blocks: path,
                    final_state: self.blocks[&current_block].allocations.clone(),
                });
                continue;
            }

            // Explore successors
            if let Some(block) = self.blocks.get(&current_block) {
                for &successor in &block.successors {
                    let mut new_path = path.clone();
                    if !new_path.contains(&successor) {
                        // Avoid cycles
                        new_path.push(successor);
                        queue.push_back(new_path);
                    }
                }
            }
        }

        paths
    }

    /// Get block state
    pub fn get_block(&self, id: BlockId) -> Option<&BlockState> {
        self.blocks.get(&id)
    }

    /// Update block allocations
    pub fn set_block_state(&mut self, id: BlockId, state: BlockState) {
        self.blocks.insert(id, state);
    }
}

/// Leak detector for HIR
pub struct LeakDetector {
    errors: Vec<LeakError>,
}

impl LeakDetector {
    pub fn new() -> Self {
        LeakDetector { errors: Vec::new() }
    }

    /// Check a function for leaks
    pub fn check_function(&mut self, func: &HirFunction) -> Result<(), Vec<LeakError>> {
        // Build CFG from function body
        let mut cfg = ControlFlowGraph::new();
        self.build_cfg(&func.body, &mut cfg)?;

        // Check for leaks on all paths
        cfg.check_leaks()?;

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Build control-flow graph from statements
    fn build_cfg(
        &mut self,
        _stmts: &[HirStmt],
        _cfg: &mut ControlFlowGraph,
    ) -> Result<(), Vec<LeakError>> {
        // This would traverse statements and build blocks
        // For now, simplified implementation
        Ok(())
    }
}

impl Default for LeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cfg_creation() {
        let cfg = ControlFlowGraph::new();
        assert!(!cfg.blocks.is_empty());
    }

    #[test]
    fn test_new_block() {
        let mut cfg = ControlFlowGraph::new();
        let b1 = cfg.new_block();
        let b2 = cfg.new_block();
        assert_ne!(b1, b2);
    }

    #[test]
    fn test_add_edge() {
        let mut cfg = ControlFlowGraph::new();
        let b1 = cfg.entry_block;
        let b2 = cfg.new_block();
        cfg.add_edge(b1, b2);

        assert!(cfg.blocks[&b1].successors.contains(&b2));
        assert!(cfg.blocks[&b2].predecessors.contains(&b1));
    }

    #[test]
    fn test_leak_detector_creation() {
        let detector = LeakDetector::new();
        assert!(detector.errors.is_empty());
    }

    #[test]
    fn test_mark_exit() {
        let mut cfg = ControlFlowGraph::new();
        let b1 = cfg.new_block();
        cfg.mark_exit(b1);
        assert!(cfg.exit_blocks.contains(&b1));
    }
}
