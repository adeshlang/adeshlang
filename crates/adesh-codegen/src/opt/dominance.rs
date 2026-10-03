//! Dominator Tree and Dominance Frontier Analysis for Machine IR.
//!
//! Provides immediate dominator (idom) computation using the Lengauer-Tarjan / Cooper algorithm,
//! dominance queries `dominates(a, b)`, and dominance frontier calculation for SSA/Global optimizations.

use crate::machine_ir::MachineFunction;
use std::collections::{HashMap, HashSet};

/// Dominator Tree data structure.
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// Mapping from block ID to its immediate dominator block ID (idom).
    pub idom: HashMap<u32, u32>,
    /// Mapping from block ID to list of blocks it immediately dominates (children in dom tree).
    pub dom_tree_children: HashMap<u32, Vec<u32>>,
    /// Dominance frontier for each block ID.
    pub dominance_frontiers: HashMap<u32, HashSet<u32>>,
    /// Entry block ID.
    pub entry_id: u32,
}

impl DominatorTree {
    /// Compute the Dominator Tree for a given MachineFunction.
    pub fn compute(func: &MachineFunction) -> Self {
        if func.blocks.is_empty() {
            return Self {
                idom: HashMap::new(),
                dom_tree_children: HashMap::new(),
                dominance_frontiers: HashMap::new(),
                entry_id: 0,
            };
        }

        let entry_id = func.blocks[0].id;
        let mut idom: HashMap<u32, u32> = HashMap::new();
        idom.insert(entry_id, entry_id);

        let mut block_by_id = HashMap::new();
        for block in &func.blocks {
            block_by_id.insert(block.id, block);
        }

        // Iterative fixed-point algorithm for computing immediate dominators (idom)
        let mut changed = true;
        while changed {
            changed = false;

            for block in &func.blocks {
                if block.id == entry_id {
                    continue;
                }

                // Find first processed predecessor
                let mut new_idom: Option<u32> = None;
                for &pred_id in &block.predecessors {
                    if idom.contains_key(&pred_id) {
                        new_idom = Some(pred_id);
                        break;
                    }
                }

                if let Some(mut current_idom) = new_idom {
                    for &pred_id in &block.predecessors {
                        if idom.contains_key(&pred_id) && pred_id != current_idom {
                            current_idom = Self::intersect(&idom, pred_id, current_idom);
                        }
                    }

                    if idom.get(&block.id).copied() != Some(current_idom) {
                        idom.insert(block.id, current_idom);
                        changed = true;
                    }
                }
            }
        }

        // Build dom_tree_children
        let mut dom_tree_children: HashMap<u32, Vec<u32>> = HashMap::new();
        for &block_id in idom.keys() {
            if block_id != entry_id {
                let parent = idom[&block_id];
                dom_tree_children.entry(parent).or_default().push(block_id);
            }
        }

        // Compute dominance frontiers
        let mut dominance_frontiers: HashMap<u32, HashSet<u32>> = HashMap::new();
        for block in &func.blocks {
            if block.predecessors.len() >= 2 {
                for &pred_id in &block.predecessors {
                    let mut runner = pred_id;
                    while runner != idom.get(&block.id).copied().unwrap_or(runner) {
                        dominance_frontiers
                            .entry(runner)
                            .or_default()
                            .insert(block.id);
                        if let Some(&next_runner) = idom.get(&runner) {
                            if next_runner == runner {
                                break;
                            }
                            runner = next_runner;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        Self {
            idom,
            dom_tree_children,
            dominance_frontiers,
            entry_id,
        }
    }

    fn intersect(idom: &HashMap<u32, u32>, b1: u32, b2: u32) -> u32 {
        let mut visited = HashSet::new();
        let mut curr = b1;
        while let Some(&parent) = idom.get(&curr) {
            visited.insert(curr);
            if parent == curr {
                break;
            }
            curr = parent;
        }

        curr = b2;
        while let Some(&parent) = idom.get(&curr) {
            if visited.contains(&curr) {
                return curr;
            }
            if parent == curr {
                break;
            }
            curr = parent;
        }

        b1
    }

    /// Check if block `a` dominates block `b` (every path to `b` passes through `a`).
    pub fn dominates(&self, a: u32, b: u32) -> bool {
        if a == b {
            return true;
        }
        let mut curr = b;
        while let Some(&parent) = self.idom.get(&curr) {
            if parent == a {
                return true;
            }
            if parent == curr {
                break;
            }
            curr = parent;
        }
        false
    }

    /// Check if block `a` strictly dominates block `b` (`a != b && dominates(a, b)`).
    pub fn strictly_dominates(&self, a: u32, b: u32) -> bool {
        a != b && self.dominates(a, b)
    }
}
