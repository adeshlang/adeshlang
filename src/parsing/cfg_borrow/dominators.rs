//! Dominators, Post-Dominators, Reachability, and Natural Loop Analysis
//!
//! Operates directly on `ControlFlowGraph` (`src/parsing/cfg_borrow/cfg.rs`)
//! without introducing a separate duplicate graph representation.

use super::cfg::{BlockId, ControlFlowGraph};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// Reachability and traversal orders for a `ControlFlowGraph`.
#[derive(Debug, Clone)]
pub struct Reachability {
    /// Set of blocks reachable from `cfg.entry`.
    pub reachable: HashSet<BlockId>,
    /// Block IDs unreachable from `cfg.entry` (sorted by ID).
    pub unreachable: Vec<BlockId>,
    /// Postorder traversal of reachable blocks from `cfg.entry`.
    pub postorder: Vec<BlockId>,
    /// Reverse postorder traversal of reachable blocks from `cfg.entry`.
    pub rpo: Vec<BlockId>,
}

impl Reachability {
    /// Compute reachability, postorder, and RPO from `cfg.entry`.
    pub fn compute(cfg: &ControlFlowGraph) -> Self {
        let mut visited = HashSet::new();
        let mut postorder = Vec::new();

        if cfg.get_block(cfg.entry).is_some() {
            Self::dfs_postorder(cfg, cfg.entry, &mut visited, &mut postorder);
        }

        let mut rpo = postorder.clone();
        rpo.reverse();

        let mut unreachable: Vec<BlockId> = cfg
            .blocks
            .iter()
            .map(|b| b.id)
            .filter(|id| !visited.contains(id))
            .collect();
        unreachable.sort_unstable();

        Self {
            reachable: visited,
            unreachable,
            postorder,
            rpo,
        }
    }

    fn dfs_postorder(
        cfg: &ControlFlowGraph,
        block_id: BlockId,
        visited: &mut HashSet<BlockId>,
        postorder: &mut Vec<BlockId>,
    ) {
        if !visited.insert(block_id) {
            return;
        }
        if let Some(block) = cfg.get_block(block_id) {
            for &succ in &block.successors {
                Self::dfs_postorder(cfg, succ, visited, postorder);
            }
        }
        postorder.push(block_id);
    }

    /// Returns `true` if `block_id` is reachable from `cfg.entry`.
    pub fn is_reachable(&self, block_id: BlockId) -> bool {
        self.reachable.contains(&block_id)
    }
}

/// Dominator tree computed via the Cooper-Harvey-Kennedy iterative algorithm.
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// Entry block root.
    pub entry: BlockId,
    /// Immediate dominator map (`entry` maps to itself, other reachable blocks
    /// map to their strict immediate dominator).
    idom: HashMap<BlockId, BlockId>,
    /// Dominator tree children (`parent -> sorted children`).
    children: HashMap<BlockId, Vec<BlockId>>,
    /// Reachability metadata used during construction.
    reachability: Reachability,
}

impl DominatorTree {
    /// Compute the dominator tree for `cfg`.
    pub fn compute(cfg: &ControlFlowGraph) -> Self {
        let reachability = Reachability::compute(cfg);
        let mut idom: HashMap<BlockId, BlockId> = HashMap::new();

        if cfg.get_block(cfg.entry).is_none() {
            return Self {
                entry: cfg.entry,
                idom,
                children: HashMap::new(),
                reachability,
            };
        }

        // Map reachable BlockId -> RPO index (0 is entry, higher is later).
        let mut rpo_num: HashMap<BlockId, usize> = HashMap::new();
        for (idx, &id) in reachability.rpo.iter().enumerate() {
            rpo_num.insert(id, idx);
        }

        idom.insert(cfg.entry, cfg.entry);

        let intersect = |mut b1: BlockId, mut b2: BlockId, idom: &HashMap<BlockId, BlockId>| -> BlockId {
            while b1 != b2 {
                let mut r1 = rpo_num[&b1];
                let mut r2 = rpo_num[&b2];
                while r1 > r2 {
                    b1 = idom[&b1];
                    r1 = rpo_num[&b1];
                }
                while r2 > r1 {
                    b2 = idom[&b2];
                    r2 = rpo_num[&b2];
                }
            }
            b1
        };

        let mut changed = true;
        while changed {
            changed = false;
            for &b in &reachability.rpo {
                if b == cfg.entry {
                    continue;
                }
                let Some(block) = cfg.get_block(b) else {
                    continue;
                };
                // Consider only reachable predecessors that already have an idom.
                let mut processed_preds = block
                    .predecessors
                    .iter()
                    .copied()
                    .filter(|p| reachability.is_reachable(*p) && idom.contains_key(p));

                let Some(first_pred) = processed_preds.next() else {
                    continue;
                };
                let mut new_idom = first_pred;
                for p in processed_preds {
                    new_idom = intersect(p, new_idom, &idom);
                }

                if idom.get(&b).copied() != Some(new_idom) {
                    idom.insert(b, new_idom);
                    changed = true;
                }
            }
        }

        let mut children: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        for (&node, &parent) in &idom {
            if node != parent {
                children.entry(parent).or_default().push(node);
            }
        }
        for kids in children.values_mut() {
            kids.sort_unstable();
        }

        Self {
            entry: cfg.entry,
            idom,
            children,
            reachability,
        }
    }

    /// Immediate dominator of `block_id` (`None` for entry or unreachable blocks).
    pub fn immediate_dominator(&self, block_id: BlockId) -> Option<BlockId> {
        let dom = self.idom.get(&block_id).copied()?;
        if dom == block_id {
            None
        } else {
            Some(dom)
        }
    }

    /// Returns `true` if `a` dominates `b`.
    /// Unreachable nodes are not dominated by reachable nodes (except trivially `a == b`
    /// if both are in the tree).
    pub fn dominates(&self, a: BlockId, b: BlockId) -> bool {
        if !self.idom.contains_key(&a) || !self.idom.contains_key(&b) {
            return false;
        }
        let mut curr = b;
        loop {
            if curr == a {
                return true;
            }
            let Some(&parent) = self.idom.get(&curr) else {
                return false;
            };
            if parent == curr {
                return false;
            }
            curr = parent;
        }
    }

    /// Returns `true` if `a` strictly dominates `b` (`a != b` and `a` dominates `b`).
    pub fn strictly_dominates(&self, a: BlockId, b: BlockId) -> bool {
        a != b && self.dominates(a, b)
    }

    /// All dominators of `block_id`, ordered from `entry` down to `block_id`.
    pub fn dominators_of(&self, block_id: BlockId) -> Vec<BlockId> {
        if !self.idom.contains_key(&block_id) {
            return Vec::new();
        }
        let mut chain = Vec::new();
        let mut curr = block_id;
        loop {
            chain.push(curr);
            let parent = self.idom[&curr];
            if parent == curr {
                break;
            }
            curr = parent;
        }
        chain.reverse();
        chain
    }

    /// Direct children of `block_id` in the dominator tree.
    pub fn children(&self, block_id: BlockId) -> &[BlockId] {
        self.children.get(&block_id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Access underlying reachability info.
    pub fn reachability(&self) -> &Reachability {
        &self.reachability
    }
}

/// Post-dominator tree computed on the reverse CFG using a virtual super-exit
/// connected to all `cfg.exits` (and any reachable sink blocks with no successors).
#[derive(Debug, Clone)]
pub struct PostDominatorTree {
    /// Immediate post-dominator map (`None` or virtual exit means exits directly).
    ipdom: HashMap<BlockId, Option<BlockId>>,
    /// Children in the post-dominator tree (`parent -> children`).
    children: HashMap<BlockId, Vec<BlockId>>,
}

impl PostDominatorTree {
    /// Compute the post-dominator tree for `cfg`.
    pub fn compute(cfg: &ControlFlowGraph) -> Self {
        let reachability = Reachability::compute(cfg);
        if cfg.blocks.is_empty() || reachability.reachable.is_empty() {
            return Self {
                ipdom: HashMap::new(),
                children: HashMap::new(),
            };
        }

        // Virtual super-exit node ID = cfg.blocks.len()
        let vexit = cfg.blocks.len();

        // Build reverse graph among reachable nodes + virtual super-exit.
        let mut rev_succs: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        let mut rev_preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

        for &id in &reachability.reachable {
            let Some(block) = cfg.get_block(id) else {
                continue;
            };
            let mut has_reachable_succ = false;
            for &succ in &block.successors {
                if reachability.is_reachable(succ) {
                    rev_succs.entry(succ).or_default().push(id);
                    rev_preds.entry(id).or_default().push(succ);
                    has_reachable_succ = true;
                }
            }
            if cfg.exits.contains(&id) || !has_reachable_succ {
                rev_succs.entry(vexit).or_default().push(id);
                rev_preds.entry(id).or_default().push(vexit);
            }
        }

        // Postorder / RPO on reverse graph starting from `vexit`.
        let mut visited = HashSet::new();
        let mut postorder = Vec::new();
        Self::rev_dfs(vexit, &rev_succs, &mut visited, &mut postorder);
        let mut rpo = postorder;
        rpo.reverse();

        let mut rpo_num = HashMap::new();
        for (idx, &id) in rpo.iter().enumerate() {
            rpo_num.insert(id, idx);
        }

        let mut idom: HashMap<BlockId, BlockId> = HashMap::new();
        idom.insert(vexit, vexit);

        let intersect = |mut b1: BlockId, mut b2: BlockId, idom: &HashMap<BlockId, BlockId>| -> BlockId {
            while b1 != b2 {
                let mut r1 = rpo_num[&b1];
                let mut r2 = rpo_num[&b2];
                while r1 > r2 {
                    b1 = idom[&b1];
                    r1 = rpo_num[&b1];
                }
                while r2 > r1 {
                    b2 = idom[&b2];
                    r2 = rpo_num[&b2];
                }
            }
            b1
        };

        let mut changed = true;
        while changed {
            changed = false;
            for &b in &rpo {
                if b == vexit {
                    continue;
                }
                let preds = rev_preds.get(&b).map(|v| v.as_slice()).unwrap_or(&[]);
                let mut processed = preds
                    .iter()
                    .copied()
                    .filter(|p| visited.contains(p) && idom.contains_key(p));
                let Some(first) = processed.next() else {
                    continue;
                };
                let mut new_idom = first;
                for p in processed {
                    new_idom = intersect(p, new_idom, &idom);
                }
                if idom.get(&b).copied() != Some(new_idom) {
                    idom.insert(b, new_idom);
                    changed = true;
                }
            }
        }

        let mut ipdom = HashMap::new();
        let mut children: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        for (&node, &parent) in &idom {
            if node == vexit {
                continue;
            }
            if parent == vexit {
                ipdom.insert(node, None);
            } else {
                ipdom.insert(node, Some(parent));
                children.entry(parent).or_default().push(node);
            }
        }
        for kids in children.values_mut() {
            kids.sort_unstable();
        }

        Self { ipdom, children }
    }

    fn rev_dfs(
        node: BlockId,
        rev_succs: &HashMap<BlockId, Vec<BlockId>>,
        visited: &mut HashSet<BlockId>,
        postorder: &mut Vec<BlockId>,
    ) {
        if !visited.insert(node) {
            return;
        }
        if let Some(succs) = rev_succs.get(&node) {
            for &s in succs {
                Self::rev_dfs(s, rev_succs, visited, postorder);
            }
        }
        postorder.push(node);
    }

    /// Immediate post-dominator of `block_id` (`None` if immediate child of virtual exit
    /// or not post-dominated by any real block).
    pub fn immediate_post_dominator(&self, block_id: BlockId) -> Option<BlockId> {
        self.ipdom.get(&block_id).copied().flatten()
    }

    /// Returns `true` if `a` post-dominates `b`.
    pub fn post_dominates(&self, a: BlockId, b: BlockId) -> bool {
        if !self.ipdom.contains_key(&a) || !self.ipdom.contains_key(&b) {
            return false;
        }
        let mut curr = Some(b);
        while let Some(node) = curr {
            if node == a {
                return true;
            }
            curr = self.ipdom.get(&node).copied().flatten();
        }
        false
    }

    /// Children of `block_id` in the post-dominator tree.
    pub fn children(&self, block_id: BlockId) -> &[BlockId] {
        self.children.get(&block_id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// A natural loop identified by its header and back-edge sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NaturalLoop {
    /// Loop header block (dominates every block in `body`).
    pub header: BlockId,
    /// Tail block(s) with a back-edge `(tail -> header)`.
    pub back_edge_tails: Vec<BlockId>,
    /// All blocks belonging to this loop (including `header`), sorted.
    pub body: BTreeSet<BlockId>,
    /// Blocks outside `body` that are targets of edges leaving `body`.
    pub exit_targets: BTreeSet<BlockId>,
    /// Nesting depth (outermost loop = 1).
    pub depth: usize,
    /// Immediate enclosing loop header, if nested.
    pub parent_header: Option<BlockId>,
}

/// Natural loop analysis for a `ControlFlowGraph`.
#[derive(Debug, Clone)]
pub struct LoopAnalysis {
    /// All back edges `(tail, header)` where `header` dominates `tail`.
    pub back_edges: Vec<(BlockId, BlockId)>,
    /// Natural loops keyed by `header`.
    pub loops: BTreeMap<BlockId, NaturalLoop>,
}

impl LoopAnalysis {
    /// Identify back edges, natural loops, exit targets, and nesting depth.
    pub fn compute(cfg: &ControlFlowGraph, dom: &DominatorTree) -> Self {
        let mut back_edges = Vec::new();
        let mut header_tails: BTreeMap<BlockId, Vec<BlockId>> = BTreeMap::new();

        for &u in &dom.reachability().rpo {
            let Some(block) = cfg.get_block(u) else {
                continue;
            };
            for &v in &block.successors {
                if dom.dominates(v, u) {
                    back_edges.push((u, v));
                    header_tails.entry(v).or_default().push(u);
                }
            }
        }

        let mut loops: BTreeMap<BlockId, NaturalLoop> = BTreeMap::new();
        for (header, mut tails) in header_tails {
            tails.sort_unstable();
            tails.dedup();

            let mut body = BTreeSet::new();
            body.insert(header);

            let mut worklist = VecDeque::new();
            for &tail in &tails {
                if body.insert(tail) {
                    worklist.push_back(tail);
                }
            }

            while let Some(node) = worklist.pop_front() {
                if let Some(block) = cfg.get_block(node) {
                    for &pred in &block.predecessors {
                        if dom.reachability().is_reachable(pred) && body.insert(pred) {
                            worklist.push_back(pred);
                        }
                    }
                }
            }

            let mut exit_targets = BTreeSet::new();
            for &b in &body {
                if let Some(block) = cfg.get_block(b) {
                    for &succ in &block.successors {
                        if !body.contains(&succ) {
                            exit_targets.insert(succ);
                        }
                    }
                }
            }

            loops.insert(
                header,
                NaturalLoop {
                    header,
                    back_edge_tails: tails,
                    body,
                    exit_targets,
                    depth: 1,
                    parent_header: None,
                },
            );
        }

        // Compute loop nesting: loop L1 is nested inside L2 if L2.body strictly
        // contains L1.header (and L2.header != L1.header).
        let headers: Vec<BlockId> = loops.keys().copied().collect();
        for &h1 in &headers {
            let mut enclosing: Vec<(BlockId, usize)> = Vec::new();
            for &h2 in &headers {
                if h1 != h2 && loops[&h2].body.contains(&h1) {
                    enclosing.push((h2, loops[&h2].body.len()));
                }
            }
            if !enclosing.is_empty() {
                // Smallest enclosing loop body is the immediate parent.
                enclosing.sort_by_key(|&(_, len)| len);
                let parent = enclosing[0].0;
                let depth = enclosing.len() + 1;
                if let Some(l) = loops.get_mut(&h1) {
                    l.parent_header = Some(parent);
                    l.depth = depth;
                }
            }
        }

        Self { back_edges, loops }
    }

    /// Loop nesting depth of `block_id` (0 if not in any loop).
    pub fn nesting_depth(&self, block_id: BlockId) -> usize {
        self.loops
            .values()
            .filter(|l| l.body.contains(&block_id))
            .map(|l| l.depth)
            .max()
            .unwrap_or(0)
    }

    /// Returns `true` if `(tail, header)` is a back edge.
    pub fn is_back_edge(&self, tail: BlockId, header: BlockId) -> bool {
        self.back_edges.contains(&(tail, header))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::cfg_borrow::cfg::{BasicBlock, BlockKind};

    fn make_manual_cfg(name: &str, block_kinds: &[BlockKind], edges: &[(BlockId, BlockId)]) -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new_empty(name.to_string());
        for (id, &kind) in block_kinds.iter().enumerate() {
            cfg.blocks.push(BasicBlock::new_with_kind(id, kind));
            if kind == BlockKind::Exit {
                cfg.exits.push(id);
            }
        }
        cfg.entry = 0;
        for &(u, v) in edges {
            cfg.blocks[u].successors.push(v);
            cfg.blocks[v].predecessors.push(u);
        }
        cfg
    }

    #[test]
    fn test_diamond_dominators_and_postdominators() {
        // 0 (entry) -> 1, 2; 1 -> 3; 2 -> 3; 3 (exit)
        let cfg = make_manual_cfg(
            "diamond",
            &[
                BlockKind::Entry,
                BlockKind::Normal,
                BlockKind::Normal,
                BlockKind::Exit,
            ],
            &[(0, 1), (0, 2), (1, 3), (2, 3)],
        );

        let dom = DominatorTree::compute(&cfg);
        assert_eq!(dom.immediate_dominator(0), None);
        assert_eq!(dom.immediate_dominator(1), Some(0));
        assert_eq!(dom.immediate_dominator(2), Some(0));
        assert_eq!(dom.immediate_dominator(3), Some(0));
        assert!(dom.dominates(0, 3));
        assert!(!dom.dominates(1, 3));
        assert!(!dom.dominates(2, 3));

        let pdom = PostDominatorTree::compute(&cfg);
        assert!(pdom.post_dominates(3, 0));
        assert!(pdom.post_dominates(3, 1));
        assert!(pdom.post_dominates(3, 2));
        assert_eq!(pdom.immediate_post_dominator(1), Some(3));
        assert_eq!(pdom.immediate_post_dominator(2), Some(3));
        assert_eq!(pdom.immediate_post_dominator(0), Some(3));
    }

    #[test]
    fn test_nested_loops_and_back_edges() {
        // 0 (entry) -> 1 (outer header)
        // 1 -> 2 (inner header), 1 -> 5 (exit)
        // 2 -> 3 (inner body), 2 -> 4 (outer latch)
        // 3 -> 2 (back edge inner)
        // 4 -> 1 (back edge outer)
        let cfg = make_manual_cfg(
            "nested_loops",
            &[
                BlockKind::Entry,
                BlockKind::LoopHeader,
                BlockKind::LoopHeader,
                BlockKind::Normal,
                BlockKind::Normal,
                BlockKind::Exit,
            ],
            &[(0, 1), (1, 2), (1, 5), (2, 3), (2, 4), (3, 2), (4, 1)],
        );

        let dom = DominatorTree::compute(&cfg);
        let loops = LoopAnalysis::compute(&cfg, &dom);

        assert!(loops.is_back_edge(3, 2));
        assert!(loops.is_back_edge(4, 1));
        assert_eq!(loops.loops.len(), 2);
        assert_eq!(loops.loops[&1].depth, 1);
        assert_eq!(loops.loops[&2].depth, 2);
        assert_eq!(loops.loops[&2].parent_header, Some(1));
        assert_eq!(loops.nesting_depth(3), 2);
        assert_eq!(loops.nesting_depth(4), 1);
        assert_eq!(loops.nesting_depth(5), 0);
    }

    #[test]
    fn test_dominators_against_naive_path_definition_on_generated_cfgs() {
        // Deterministic PRNG generating small CFGs and comparing iterative dominators
        // against the textbook path-intersection definition of dominance.
        let mut seed: u64 = 0x5EED_CAFE_1914_u64;
        let mut next_rand = || -> u64 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed >> 32
        };

        for trial in 0..40 {
            let n = 5 + (next_rand() % 5) as usize; // 5..9 blocks
            let mut kinds = vec![BlockKind::Normal; n];
            kinds[0] = BlockKind::Entry;
            kinds[n - 1] = BlockKind::Exit;

            let mut edge_set = BTreeSet::new();
            // Ensure connected backbone + random forward/back edges.
            for i in 0..n - 1 {
                if next_rand() % 3 != 0 || i == 0 {
                    edge_set.insert((i, i + 1));
                }
            }
            for u in 0..n - 1 {
                let extra = (next_rand() % 3) as usize;
                for _ in 0..extra {
                    let v = 1 + (next_rand() as usize % (n - 1));
                    if u != v {
                        edge_set.insert((u, v));
                    }
                }
            }
            let edges: Vec<(BlockId, BlockId)> = edge_set.into_iter().collect();
            let cfg = make_manual_cfg(&format!("gen_{trial}"), &kinds, &edges);
            let dom = DominatorTree::compute(&cfg);

            // Naive definition: d dominates x iff every simple path from entry (0)
            // to x contains d (or x is unreachable).
            for x in 0..n {
                if !dom.reachability().is_reachable(x) {
                    continue;
                }
                for d in 0..n {
                    if !dom.reachability().is_reachable(d) {
                        assert!(!dom.dominates(d, x));
                        continue;
                    }
                    // Can 0 reach x without visiting d?
                    let can_bypass = if d == 0 {
                        false
                    } else {
                        let mut seen = HashSet::new();
                        let mut q = VecDeque::new();
                        q.push_back(0);
                        seen.insert(0);
                        let mut found = false;
                        while let Some(cur) = q.pop_front() {
                            if cur == x {
                                found = true;
                                break;
                            }
                            for &s in &cfg.blocks[cur].successors {
                                if s != d && seen.insert(s) {
                                    q.push_back(s);
                                }
                            }
                        }
                        found
                    };
                    assert_eq!(
                        dom.dominates(d, x),
                        !can_bypass,
                        "trial {trial}: mismatch for dom({d}, {x})"
                    );
                }
            }
        }
    }
}
