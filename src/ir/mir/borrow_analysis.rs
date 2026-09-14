//! Borrow Analysis
//!
//! Scans MIR for borrow operations and validates that no local is
//! simultaneously shared and mutably borrowed, or moved while borrowed.
//! This is a secondary validation pass — the primary borrow checking
//! happens at the HIR level (cfg_borrow, compile_time_memory_safety).

use super::{BorrowKind, LocalId, MirFunction, MirModule, MirRvalue, MirStatement};
use std::collections::{HashMap, HashSet};

/// Borrow analysis for MIR
pub struct BorrowAnalysis {
    /// Active borrows per function: function name → set of (local, kind)
    borrows: HashMap<String, Vec<(LocalId, BorrowKind)>>,
    /// Locals that have been moved (for use-after-move detection)
    moved_locals: HashMap<String, HashSet<LocalId>>,
}

impl BorrowAnalysis {
    pub fn new() -> Self {
        Self {
            borrows: HashMap::new(),
            moved_locals: HashMap::new(),
        }
    }

    /// Analyze borrows for a module
    pub fn analyze_module(module: &MirModule) -> Result<Self, String> {
        let mut analysis = Self::new();
        for func in &module.functions {
            analysis.analyze_function(func)?;
        }
        Ok(analysis)
    }

    /// Analyze borrows for a function — scans all blocks for Ref rvalues
    /// and Move operands, tracking which locals are borrowed or moved.
    pub fn analyze_function(&mut self, func: &MirFunction) -> Result<(), String> {
        let mut func_borrows: Vec<(LocalId, BorrowKind)> = Vec::new();
        let mut func_moved: HashSet<LocalId> = HashSet::new();

        for block in &func.body {
            for stmt in &block.statements {
                self.analyze_statement(stmt, &mut func_borrows, &mut func_moved)?;
            }
        }

        self.borrows.insert(func.name.clone(), func_borrows);
        self.moved_locals.insert(func.name.clone(), func_moved);
        Ok(())
    }

    fn analyze_statement(
        &self,
        stmt: &MirStatement,
        borrows: &mut Vec<(LocalId, BorrowKind)>,
        moved: &mut HashSet<LocalId>,
    ) -> Result<(), String> {
        match stmt {
            MirStatement::Assign(_, rvalue) => {
                self.analyze_rvalue(rvalue, borrows, moved)?;
            }
            MirStatement::Drop(local) => {
                // Dropping a moved local is fine; dropping a borrowed local is an error
                if borrows.iter().any(|(l, _)| l == local) {
                    return Err(format!(
                        "Cannot drop local {} while it is borrowed",
                        local
                    ));
                }
            }
            MirStatement::ArcClone(_, _) | MirStatement::ArcDrop(_) => {}
            MirStatement::Call { dest, args, .. } => {
                // Check that moved args are not used after move
                for arg in args {
                    if let super::MirOperand::Move(place) = arg {
                        if moved.contains(&place.local) {
                            return Err(format!(
                                "Use of moved local {} in function call",
                                place.local
                            ));
                        }
                    }
                }
                // The destination is a new binding — mark as defined
                moved.remove(dest);
            }
            MirStatement::StorageDead(local) => {
                // Storage dead means the local is no longer live
                borrows.retain(|(l, _)| l != local);
                moved.remove(local);
            }
            _ => {}
        }
        Ok(())
    }

    fn analyze_rvalue(
        &self,
        rvalue: &MirRvalue,
        borrows: &mut Vec<(LocalId, BorrowKind)>,
        moved: &mut HashSet<LocalId>,
    ) -> Result<(), String> {
        match rvalue {
            MirRvalue::Ref(kind, place) => {
                // Record the borrow
                borrows.push((place.local, kind.clone()));

                // Check for conflicting borrows
                let has_shared = borrows
                    .iter()
                    .any(|(l, k)| l == &place.local && matches!(k, BorrowKind::Shared));
                let has_mut = borrows
                    .iter()
                    .any(|(l, k)| l == &place.local && matches!(k, BorrowKind::Mut));

                if has_mut && has_shared {
                    return Err(format!(
                        "Cannot have both shared and mutable borrow of local {}",
                        place.local
                    ));
                }

                // Check that the local hasn't been moved
                if moved.contains(&place.local) {
                    return Err(format!(
                        "Cannot borrow local {} after it has been moved",
                        place.local
                    ));
                }
            }
            MirRvalue::Use(operand) => {
                if let super::MirOperand::Move(place) = operand {
                    // Moving a borrowed local is an error
                    if borrows.iter().any(|(l, _)| l == &place.local) {
                        return Err(format!(
                            "Cannot move local {} while it is borrowed",
                            place.local
                        ));
                    }
                    moved.insert(place.local);
                }
            }
            MirRvalue::Deref(place) => {
                // Deref of a moved local is use-after-move
                if moved.contains(&place.local) {
                    return Err(format!(
                        "Cannot dereference local {} after it has been moved",
                        place.local
                    ));
                }
            }
            MirRvalue::BinaryOp(_, lhs, rhs) => {
                for operand in [lhs, rhs] {
                    if let super::MirOperand::Move(place) = operand {
                        if moved.contains(&place.local) {
                            return Err(format!(
                                "Use of moved local {} in binary operation",
                                place.local
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Check if a local is borrowed in a given function
    pub fn is_borrowed(&self, func_name: &str, local: LocalId) -> bool {
        self.borrows
            .get(func_name)
            .map(|b| b.iter().any(|(l, _)| l == &local))
            .unwrap_or(false)
    }

    /// Check if a local is mutably borrowed in a given function
    pub fn is_mut_borrowed(&self, func_name: &str, local: LocalId) -> bool {
        self.borrows
            .get(func_name)
            .map(|b| {
                b.iter()
                    .any(|(l, k)| l == &local && matches!(k, BorrowKind::Mut))
            })
            .unwrap_or(false)
    }

    /// Check if a local has been moved in a given function
    pub fn is_moved(&self, func_name: &str, local: LocalId) -> bool {
        self.moved_locals
            .get(func_name)
            .map(|m| m.contains(&local))
            .unwrap_or(false)
    }
}

impl Default for BorrowAnalysis {
    fn default() -> Self {
        Self::new()
    }
}
