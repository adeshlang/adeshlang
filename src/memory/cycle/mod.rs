//! Cycle Detection Utilities (Phase 3)
//!
//! Provides lightweight cycle detection helpers for ownership graphs.
//! Intended for use with reference-counted structures to prevent leaks.

use std::collections::{HashMap, HashSet, VecDeque};

/// Detect cycles in a directed graph represented as adjacency list.
/// Nodes are `usize` identifiers for simplicity.
pub fn has_cycle(node_count: usize, edges: &[(usize, usize)]) -> bool {
    if edges.is_empty() || node_count <= 1 {
        return false;
    }
    // Kahn's algorithm for cycle detection via topological ordering
    let mut indeg = vec![0usize; node_count];
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(u, v) in edges {
        *indeg.get_mut(v).unwrap() += 1;
        adj.entry(u).or_default().push(v);
    }

    let mut q = VecDeque::new();
    for i in 0..node_count {
        if indeg[i] == 0 {
            q.push_back(i);
        }
    }

    let mut seen = 0usize;
    while let Some(u) = q.pop_front() {
        seen += 1;
        if let Some(nei) = adj.get(&u) {
            for &v in nei {
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    q.push_back(v);
                }
            }
        }
    }

    seen != node_count
}

/// DFS-based cycle detection; useful for smaller graphs.
pub fn has_cycle_dfs(node_count: usize, edges: &[(usize, usize)]) -> bool {
    if edges.is_empty() || node_count <= 1 {
        return false;
    }
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(u, v) in edges {
        adj.entry(u).or_default().push(v);
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();

    fn dfs(
        u: usize,
        adj: &HashMap<usize, Vec<usize>>,
        visiting: &mut HashSet<usize>,
        visited: &mut HashSet<usize>,
    ) -> bool {
        if !visiting.insert(u) {
            return true;
        }
        if let Some(nei) = adj.get(&u) {
            for &v in nei {
                if !visited.contains(&v) {
                    if dfs(v, adj, visiting, visited) {
                        return true;
                    }
                }
            }
        }
        visiting.remove(&u);
        visited.insert(u);
        false
    }

    for u in 0..node_count {
        if !visited.contains(&u) {
            if dfs(u, &adj, &mut visiting, &mut visited) {
                return true;
            }
        }
    }

    false
}

/// Node color for Bacon-Rajan cycle collection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeColor {
    Black,  // In use or free
    Gray,   // Possible member of cycle
    White,  // Member of cycle (garbage)
    Purple, // Possible root of cycle
}

/// Automatic Cycle Collector for reference-counted object graphs
#[derive(Debug, Clone, Default)]
pub struct CycleCollector {
    pub roots: HashSet<usize>,
    pub adj: HashMap<usize, Vec<usize>>,
    pub ref_counts: HashMap<usize, usize>,
    pub colors: HashMap<usize, NodeColor>,
}

impl CycleCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a node with initial reference count
    pub fn register_node(&mut self, node: usize, rc: usize) {
        self.ref_counts.insert(node, rc);
        self.colors.insert(node, NodeColor::Black);
    }

    /// Add a directed reference edge from -> to
    pub fn add_reference(&mut self, from: usize, to: usize) {
        self.adj.entry(from).or_default().push(to);
    }

    /// Mark a node as a candidate cycle root (e.g. when its RC decreases but remains > 0)
    pub fn mark_possible_root(&mut self, node: usize) {
        self.colors.insert(node, NodeColor::Purple);
        self.roots.insert(node);
    }

    /// Run the cycle collection algorithm. Returns the list of node IDs that belong to unreachable cycles.
    pub fn collect_cycles(&mut self) -> Vec<usize> {
        let roots: Vec<usize> = self.roots.drain().collect();
        for &s in &roots {
            if self.colors.get(&s) == Some(&NodeColor::Purple) {
                self.mark_gray(s);
            }
        }

        for &s in &roots {
            self.scan(s);
        }

        let mut dead = Vec::new();
        for &s in &roots {
            self.collect_white(s, &mut dead);
        }

        dead
    }

    fn mark_gray(&mut self, s: usize) {
        if self.colors.get(&s) != Some(&NodeColor::Gray) {
            self.colors.insert(s, NodeColor::Gray);
            let neighbors = self.adj.get(&s).cloned().unwrap_or_default();
            for t in neighbors {
                if let Some(rc) = self.ref_counts.get_mut(&t) {
                    *rc = rc.saturating_sub(1);
                }
                self.mark_gray(t);
            }
        }
    }

    fn scan(&mut self, s: usize) {
        if self.colors.get(&s) == Some(&NodeColor::Gray) {
            let rc = self.ref_counts.get(&s).copied().unwrap_or(0);
            if rc > 0 {
                self.scan_black(s);
            } else {
                self.colors.insert(s, NodeColor::White);
                let neighbors = self.adj.get(&s).cloned().unwrap_or_default();
                for t in neighbors {
                    self.scan(t);
                }
            }
        }
    }

    fn scan_black(&mut self, s: usize) {
        self.colors.insert(s, NodeColor::Black);
        let neighbors = self.adj.get(&s).cloned().unwrap_or_default();
        for t in neighbors {
            if let Some(rc) = self.ref_counts.get_mut(&t) {
                *rc += 1;
            }
            if self.colors.get(&t) != Some(&NodeColor::Black) {
                self.scan_black(t);
            }
        }
    }

    fn collect_white(&mut self, s: usize, dead: &mut Vec<usize>) {
        if self.colors.get(&s) == Some(&NodeColor::White) && !dead.contains(&s) {
            self.colors.insert(s, NodeColor::Black);
            dead.push(s);
            let neighbors = self.adj.get(&s).cloned().unwrap_or_default();
            for t in neighbors {
                self.collect_white(t, dead);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_kahn_positive() {
        let edges = vec![(0, 1), (1, 2), (2, 0)];
        assert!(has_cycle(3, &edges));
    }

    #[test]
    fn cycle_kahn_negative() {
        let edges = vec![(0, 1), (1, 2)];
        assert!(!has_cycle(3, &edges));
    }

    #[test]
    fn test_cycle_collector() {
        let mut gc = CycleCollector::new();
        // Create an isolated cycle of 2 nodes: 1 -> 2 -> 1, both with internal rc = 1
        gc.register_node(1, 1);
        gc.register_node(2, 1);
        gc.add_reference(1, 2);
        gc.add_reference(2, 1);
        gc.mark_possible_root(1);

        let garbage = gc.collect_cycles();
        assert!(garbage.contains(&1));
        assert!(garbage.contains(&2));
    }
}
