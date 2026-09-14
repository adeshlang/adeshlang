//! Place Table and Alias Analysis
//!
//! Tracks memory locations (places) with union-find for alias resolution.
//! Enables precise tracking when multiple variables refer to the same memory.

use super::mir::{AllocKind, OwnershipKind, PlaceId, SsaVar, TypeId};
use std::collections::HashMap;

/// Union-Find data structure for alias tracking
#[derive(Debug, Clone)]
pub struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    pub fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    pub fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]); // Path compression
        }
        self.parent[x]
    }

    pub fn find_immut(&self, mut x: usize) -> usize {
        while self.parent[x] != x {
            x = self.parent[x];
        }
        x
    }

    pub fn union(&mut self, x: usize, y: usize) {
        let px = self.find(x);
        let py = self.find(y);
        if px == py {
            return;
        }
        // Union by rank
        match self.rank[px].cmp(&self.rank[py]) {
            std::cmp::Ordering::Less => self.parent[px] = py,
            std::cmp::Ordering::Greater => self.parent[py] = px,
            std::cmp::Ordering::Equal => {
                self.parent[py] = px;
                self.rank[px] += 1;
            }
        }
    }

    pub fn resize(&mut self, new_size: usize) {
        if new_size > self.parent.len() {
            let old_size = self.parent.len();
            self.parent.extend(old_size..new_size);
            self.rank.resize(new_size, 0);
        }
    }
}

/// Information about a single place
#[derive(Debug, Clone)]
pub struct PlaceInfo {
    pub id: PlaceId,
    pub ty: TypeId,
    pub ownership: OwnershipKind,
    pub alloc_kind: AllocKind,
    /// SSA variable that defined this place
    pub defining_var: Option<SsaVar>,
    /// Human-readable name (for debugging)
    pub name: Option<String>,
    /// Is this place still live (not dropped)?
    pub is_live: bool,
}

impl PlaceInfo {
    pub fn new(id: PlaceId, ty: TypeId) -> Self {
        Self {
            id,
            ty,
            ownership: OwnershipKind::Owned,
            alloc_kind: AllocKind::Stack,
            defining_var: None,
            name: None,
            is_live: true,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_alloc(mut self, kind: AllocKind) -> Self {
        self.alloc_kind = kind;
        self
    }

    pub fn with_var(mut self, var: SsaVar) -> Self {
        self.defining_var = Some(var);
        self
    }
}

/// Place table with alias resolution
#[derive(Debug)]
pub struct PlaceTable {
    /// Place metadata indexed by PlaceId
    places: Vec<PlaceInfo>,
    /// Union-find for alias tracking
    alias_uf: UnionFind,
    /// Map from SSA variable to its primary place
    var_to_place: HashMap<SsaVar, PlaceId>,
    /// Next place ID to allocate
    next_id: u32,
}

impl PlaceTable {
    pub fn new() -> Self {
        Self {
            places: Vec::new(),
            alias_uf: UnionFind::new(0),
            var_to_place: HashMap::new(),
            next_id: 0,
        }
    }

    /// Create a new place
    pub fn create_place(&mut self, ty: TypeId) -> PlaceId {
        let id = PlaceId(self.next_id);
        self.next_id += 1;

        let info = PlaceInfo::new(id, ty);
        self.places.push(info);
        self.alias_uf.resize(self.next_id as usize);

        id
    }

    /// Create a place with additional info
    pub fn create_place_with_info(&mut self, info: PlaceInfo) -> PlaceId {
        let id = PlaceId(self.next_id);
        self.next_id += 1;

        let mut info = info;
        info.id = id;
        self.places.push(info);
        self.alias_uf.resize(self.next_id as usize);

        id
    }

    /// Get place info
    pub fn get(&self, id: PlaceId) -> Option<&PlaceInfo> {
        self.places.get(id.0 as usize)
    }

    /// Get mutable place info
    pub fn get_mut(&mut self, id: PlaceId) -> Option<&mut PlaceInfo> {
        self.places.get_mut(id.0 as usize)
    }

    /// Get canonical place ID (resolving aliases)
    pub fn canonical(&self, place: PlaceId) -> PlaceId {
        PlaceId(self.alias_uf.find_immut(place.0 as usize) as u32)
    }

    /// Get canonical place ID (with path compression)
    pub fn canonical_mut(&mut self, place: PlaceId) -> PlaceId {
        PlaceId(self.alias_uf.find(place.0 as usize) as u32)
    }

    /// Record that `new` aliases `existing` (e.g., let b = a)
    pub fn add_alias(&mut self, new: PlaceId, existing: PlaceId) {
        self.alias_uf.union(new.0 as usize, existing.0 as usize);
    }

    /// Register an SSA variable's place
    pub fn register_var(&mut self, var: SsaVar, place: PlaceId) {
        self.var_to_place.insert(var, place);
        if let Some(info) = self.get_mut(place) {
            info.defining_var = Some(var);
        }
    }

    /// Get place for an SSA variable
    pub fn place_for_var(&self, var: SsaVar) -> Option<PlaceId> {
        self.var_to_place.get(&var).copied()
    }

    /// Number of places
    pub fn len(&self) -> usize {
        self.places.len()
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// Mark a place as no longer live
    pub fn mark_dead(&mut self, place: PlaceId) {
        let canonical = self.canonical_mut(place);
        if let Some(info) = self.get_mut(canonical) {
            info.is_live = false;
        }
    }

    /// Check if place is still live
    pub fn is_live(&self, place: PlaceId) -> bool {
        let canonical = self.canonical(place);
        self.get(canonical).map(|i| i.is_live).unwrap_or(false)
    }

    /// Get all places that alias with the given place
    pub fn aliases(&self, place: PlaceId) -> Vec<PlaceId> {
        let canonical = self.canonical(place);
        self.places
            .iter()
            .filter(|p| self.canonical(p.id) == canonical)
            .map(|p| p.id)
            .collect()
    }

    /// Set ownership for a place
    pub fn set_ownership(&mut self, place: PlaceId, ownership: OwnershipKind) {
        let canonical = self.canonical_mut(place);
        if let Some(info) = self.get_mut(canonical) {
            info.ownership = ownership;
        }
    }

    /// Get ownership for a place
    pub fn ownership(&self, place: PlaceId) -> OwnershipKind {
        let canonical = self.canonical(place);
        self.get(canonical)
            .map(|i| i.ownership)
            .unwrap_or(OwnershipKind::Owned)
    }
}

impl Default for PlaceTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_union_find_basic() {
        let mut uf = UnionFind::new(5);

        assert_eq!(uf.find(0), 0);
        assert_eq!(uf.find(1), 1);

        uf.union(0, 1);
        assert_eq!(uf.find(0), uf.find(1));

        uf.union(2, 3);
        uf.union(0, 2);
        assert_eq!(uf.find(0), uf.find(3));
    }

    #[test]
    fn test_place_table_creation() {
        let mut table = PlaceTable::new();

        let p0 = table.create_place(TypeId(0));
        let p1 = table.create_place(TypeId(0));

        assert_eq!(p0.0, 0);
        assert_eq!(p1.0, 1);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_alias_resolution() {
        let mut table = PlaceTable::new();

        // let a = Obj();
        let p_a = table.create_place(TypeId(0));
        // let b = a;  -- b aliases a
        let p_b = table.create_place(TypeId(0));
        table.add_alias(p_b, p_a);

        // Both should resolve to same canonical place
        assert_eq!(table.canonical(p_a), table.canonical(p_b));
    }

    #[test]
    fn test_var_registration() {
        let mut table = PlaceTable::new();

        let p0 = table.create_place(TypeId(0));
        let v0 = SsaVar(0);

        table.register_var(v0, p0);

        assert_eq!(table.place_for_var(v0), Some(p0));
        assert_eq!(table.get(p0).unwrap().defining_var, Some(v0));
    }

    #[test]
    fn test_ownership_tracking() {
        let mut table = PlaceTable::new();

        let p0 = table.create_place(TypeId(0));
        assert_eq!(table.ownership(p0), OwnershipKind::Owned);

        table.set_ownership(p0, OwnershipKind::Moved);
        assert_eq!(table.ownership(p0), OwnershipKind::Moved);
    }

    #[test]
    fn test_liveness() {
        let mut table = PlaceTable::new();

        let p0 = table.create_place(TypeId(0));
        assert!(table.is_live(p0));

        table.mark_dead(p0);
        assert!(!table.is_live(p0));
    }
}
