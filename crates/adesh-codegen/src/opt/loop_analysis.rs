//! Natural Loop Analysis and Loop Tree Hierarchy for Machine IR.
//!
//! Identifies loop headers, latches, back-edges, loop exits, and nesting depth
//! to guide loop-invariant code motion (LICM), induction variable optimization, and bounds check elimination.

use crate::machine_ir::MachineFunction;
use crate::opt::dominance::DominatorTree;
use std::collections::{HashMap, HashSet};

/// Natural Loop Representation.
#[derive(Debug, Clone)]
pub struct NaturalLoop {
    pub header_id: u32,
    pub latch_ids: Vec<u32>,
    pub block_ids: HashSet<u32>,
    pub exit_ids: Vec<u32>,
    pub nesting_depth: usize,
    pub parent_header: Option<u32>,
    pub child_headers: Vec<u32>,
}

/// Complete Loop Analysis Information for a MachineFunction.
#[derive(Debug, Clone)]
pub struct LoopInfo {
    pub loops: HashMap<u32, NaturalLoop>,
    pub block_to_loop_header: HashMap<u32, u32>,
}

impl LoopInfo {
    /// Analyze all natural loops in the given function using its dominator tree.
    pub fn analyze(func: &MachineFunction, dom: &DominatorTree) -> Self {
        let mut loops: HashMap<u32, NaturalLoop> = HashMap::new();
        let mut block_to_loop_header: HashMap<u32, u32> = HashMap::new();

        // 1. Find all back-edges: edge (latch -> header) where header dominates latch.
        for block in &func.blocks {
            for &succ_id in &block.successors {
                if dom.dominates(succ_id, block.id) {
                    // Back-edge detected from block.id to succ_id (header)
                    let header_id = succ_id;
                    let latch_id = block.id;

                    let mut loop_blocks = HashSet::new();
                    loop_blocks.insert(header_id);
                    loop_blocks.insert(latch_id);

                    // Collect all blocks in the natural loop
                    let mut worklist = vec![latch_id];
                    while let Some(curr_id) = worklist.pop() {
                        if curr_id == header_id {
                            continue;
                        }
                        if let Some(curr_block) = func.blocks.iter().find(|b| b.id == curr_id) {
                            for &pred_id in &curr_block.predecessors {
                                if loop_blocks.insert(pred_id) {
                                    worklist.push(pred_id);
                                }
                            }
                        }
                    }

                    // Insert or update existing loop for this header
                    let entry = loops.entry(header_id).or_insert_with(|| NaturalLoop {
                        header_id,
                        latch_ids: Vec::new(),
                        block_ids: HashSet::new(),
                        exit_ids: Vec::new(),
                        nesting_depth: 1,
                        parent_header: None,
                        child_headers: Vec::new(),
                    });

                    if !entry.latch_ids.contains(&latch_id) {
                        entry.latch_ids.push(latch_id);
                    }
                    entry.block_ids.extend(loop_blocks);
                }
            }
        }

        // 2. Identify loop exits
        for nat_loop in loops.values_mut() {
            for &b_id in &nat_loop.block_ids {
                if let Some(b) = func.blocks.iter().find(|blk| blk.id == b_id) {
                    for &succ_id in &b.successors {
                        if !nat_loop.block_ids.contains(&succ_id)
                            && !nat_loop.exit_ids.contains(&succ_id)
                        {
                            nat_loop.exit_ids.push(succ_id);
                        }
                    }
                }
            }
        }

        // 3. Compute loop hierarchy (parent/children) and nesting depth
        let header_ids: Vec<u32> = loops.keys().copied().collect();
        for i in 0..header_ids.len() {
            for j in 0..header_ids.len() {
                if i == j {
                    continue;
                }
                let h1 = header_ids[i];
                let h2 = header_ids[j];
                let loop1_blocks = loops[&h1].block_ids.clone();
                let loop2_blocks = loops[&h2].block_ids.clone();

                // If loop2 is strictly contained in loop1, loop1 is an ancestor
                if loop2_blocks.is_subset(&loop1_blocks) && loop2_blocks.len() < loop1_blocks.len()
                {
                    loops.get_mut(&h2).unwrap().nesting_depth += 1;
                }
            }
        }

        for nat_loop in loops.values() {
            for &b_id in &nat_loop.block_ids {
                block_to_loop_header.insert(b_id, nat_loop.header_id);
            }
        }

        Self {
            loops,
            block_to_loop_header,
        }
    }

    #[inline]
    pub fn is_in_loop(&self, block_id: u32) -> bool {
        self.block_to_loop_header.contains_key(&block_id)
    }

    #[inline]
    pub fn get_loop_for_header(&self, header_id: u32) -> Option<&NaturalLoop> {
        self.loops.get(&header_id)
    }
}
