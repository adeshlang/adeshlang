//! Ownership Graph
//!
//! Tracks ownership relationships between values in MIR.

use super::{LocalId, ValueId};
use std::collections::HashMap;

/// Ownership graph tracks ownership relationships
#[derive(Debug, Clone)]
pub struct OwnershipGraph {
    /// Nodes represent values/places
    nodes: HashMap<ValueId, OwnershipNode>,

    /// Edges represent ownership transfer or borrowing
    edges: Vec<OwnershipEdge>,

    /// Active borrows at each program point
    active_borrows: HashMap<LocalId, Vec<BorrowInfo>>,
}

#[derive(Debug, Clone)]
pub struct OwnershipNode {
    pub id: ValueId,
    pub kind: NodeKind,
    pub moved: bool,
    pub borrowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Local(LocalId),
    Temporary(ValueId),
    Field { base: LocalId, field: usize },
    Index { base: LocalId, index: ValueId },
}

#[derive(Debug, Clone)]
pub struct OwnershipEdge {
    pub from: ValueId,
    pub to: ValueId,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Ownership transfer (move)
    Move,

    /// Shared borrow
    BorrowShared,

    /// Mutable borrow
    BorrowMut,

    /// ARC clone (shared ownership)
    ArcClone,
}

#[derive(Debug, Clone)]
pub struct BorrowInfo {
    pub borrowed: LocalId,
    pub borrow_kind: super::BorrowKind,
    pub active: bool,
}

impl OwnershipGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            active_borrows: HashMap::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, id: ValueId, kind: NodeKind) {
        self.nodes.insert(
            id,
            OwnershipNode {
                id,
                kind,
                moved: false,
                borrowed: false,
            },
        );
    }

    /// Record an ownership transfer (move)
    pub fn record_move(&mut self, from: ValueId, to: ValueId) {
        self.edges.push(OwnershipEdge {
            from,
            to,
            kind: EdgeKind::Move,
        });

        if let Some(node) = self.nodes.get_mut(&from) {
            node.moved = true;
        }
    }

    /// Record a borrow
    pub fn record_borrow(&mut self, from: ValueId, to: ValueId, kind: super::BorrowKind) {
        let edge_kind = match kind {
            super::BorrowKind::Shared => EdgeKind::BorrowShared,
            super::BorrowKind::Mut => EdgeKind::BorrowMut,
            super::BorrowKind::Unique => EdgeKind::BorrowMut,
        };

        self.edges.push(OwnershipEdge {
            from,
            to,
            kind: edge_kind,
        });

        if let Some(node) = self.nodes.get_mut(&from) {
            node.borrowed = true;
        }
    }

    /// Record an ARC clone
    pub fn record_arc_clone(&mut self, from: ValueId, to: ValueId) {
        self.edges.push(OwnershipEdge {
            from,
            to,
            kind: EdgeKind::ArcClone,
        });
    }

    /// Check if a value has been moved
    pub fn is_moved(&self, id: ValueId) -> bool {
        self.nodes.get(&id).map(|n| n.moved).unwrap_or(false)
    }

    /// Check if a value is currently borrowed
    pub fn is_borrowed(&self, id: ValueId) -> bool {
        self.nodes.get(&id).map(|n| n.borrowed).unwrap_or(false)
    }

    /// Get active borrows for a local
    pub fn get_active_borrows(&self, local: LocalId) -> &[BorrowInfo] {
        self.active_borrows
            .get(&local)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Start a new borrow
    pub fn start_borrow(&mut self, local: LocalId, kind: super::BorrowKind) {
        self.active_borrows
            .entry(local)
            .or_insert_with(Vec::new)
            .push(BorrowInfo {
                borrowed: local,
                borrow_kind: kind,
                active: true,
            });
    }

    /// End a borrow
    pub fn end_borrow(&mut self, local: LocalId) {
        if let Some(borrows) = self.active_borrows.get_mut(&local) {
            if let Some(last) = borrows.last_mut() {
                last.active = false;
            }
        }
    }

    /// Validate the ownership graph for errors
    pub fn validate(&self) -> Result<(), String> {
        // Check for use-after-move
        for (i, edge) in self.edges.iter().enumerate() {
            if edge.kind == EdgeKind::Move {
                if let Some(from_node) = self.nodes.get(&edge.from) {
                    // Check if there are any subsequent uses
                    for (j, other_edge) in self.edges.iter().enumerate() {
                        if other_edge.from == edge.from && i != j {
                            return Err(format!(
                                "Use after move detected for value {:?}",
                                from_node.kind
                            ));
                        }
                    }
                }
            }
        }

        // Check for conflicting borrows
        for (local, borrows) in &self.active_borrows {
            let active: Vec<_> = borrows.iter().filter(|b| b.active).collect();

            // Check for mut + any other borrow
            let mut_count = active
                .iter()
                .filter(|b| matches!(b.borrow_kind, super::BorrowKind::Mut))
                .count();
            if mut_count > 0 && active.len() > 1 {
                return Err(format!("Conflicting borrows for local {}", local));
            }
        }

        Ok(())
    }
}

impl Default for OwnershipGraph {
    fn default() -> Self {
        Self::new()
    }
}
