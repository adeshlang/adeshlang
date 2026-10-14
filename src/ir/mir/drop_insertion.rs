//! Drop Insertion
//!
//! Inserts explicit drop calls for values that need cleanup:
//! 1. On re-assignment of an already-initialized owned local.
//! 2. At function exit (`MirTerminator::Return`), dropping all live owned locals
//!    in reverse initialization order (LIFO), excluding the returned value and
//!    caller-owned parameters.

use super::{
    LocalId, MirBlock, MirFunction, MirLocal, MirModule, MirOperand, MirPlace, MirStatement,
    MirTerminator, OwnershipKind,
};
use std::collections::HashSet;

/// Drop insertion pass
pub struct DropInsertion;

impl DropInsertion {
    /// Insert drops for a module
    pub fn insert_drops(module: &mut MirModule) -> Result<(), String> {
        for func in &mut module.functions {
            Self::insert_drops_function(func)?;
        }
        Ok(())
    }

    /// Insert drops for a function
    fn insert_drops_function(func: &mut MirFunction) -> Result<(), String> {
        // Collect caller-borrowed parameter local IDs so they are NEVER dropped
        let borrowed_params: HashSet<LocalId> = func
            .params
            .iter()
            .enumerate()
            .filter_map(|(idx, p)| {
                if p.ownership == OwnershipKind::Borrowed
                    || p.ownership == OwnershipKind::BorrowedMut
                {
                    Some(idx as LocalId)
                } else {
                    None
                }
            })
            .collect();

        for block in &mut func.body {
            Self::insert_drops_block(block, &func.locals, &borrowed_params)?;
        }
        Ok(())
    }

    /// Insert drops for a block
    fn insert_drops_block(
        block: &mut MirBlock,
        locals: &[MirLocal],
        borrowed_params: &HashSet<LocalId>,
    ) -> Result<(), String> {
        let mut new_statements = Vec::new();
        let mut assigned_in_block = HashSet::new();

        for stmt in &block.statements {
            match stmt {
                MirStatement::Assign(local, _) => {
                    // If this local was already assigned earlier in the block,
                    // drop the previous value before overwriting it.
                    if assigned_in_block.contains(local) && !borrowed_params.contains(local) {
                        if let Some(local_info) = locals.get(*local as usize) {
                            if local_info.ownership == OwnershipKind::Owned
                                && local_info.ty.needs_drop()
                            {
                                new_statements.push(MirStatement::Drop(*local));
                            }
                        }
                    }
                    assigned_in_block.insert(*local);
                    new_statements.push(stmt.clone());
                }
                MirStatement::StorageDead(local) => {
                    // When a local's storage dies, drop it if owned
                    if !borrowed_params.contains(local) {
                        if let Some(local_info) = locals.get(*local as usize) {
                            if local_info.ownership == OwnershipKind::Owned
                                && local_info.ty.needs_drop()
                            {
                                new_statements.push(MirStatement::Drop(*local));
                            }
                        }
                    }
                    new_statements.push(stmt.clone());
                }
                _ => {
                    new_statements.push(stmt.clone());
                }
            }
        }

        // At function return, drop all initialized owned locals in LIFO order,
        // excluding the returned local (if any) and borrowed parameters.
        if let MirTerminator::Return(ref ret_op) = block.terminator {
            let ret_local = match ret_op {
                Some(MirOperand::Move(MirPlace { local, .. }))
                | Some(MirOperand::Copy(MirPlace { local, .. })) => Some(*local),
                _ => None,
            };

            // Drop in reverse allocation order (LIFO)
            for (idx, local_info) in locals.iter().enumerate().rev() {
                let local_id = idx as LocalId;
                if Some(local_id) == ret_local {
                    continue; // Do not drop returned value
                }
                if borrowed_params.contains(&local_id) {
                    continue; // Caller owns parameters
                }
                if local_info.ownership == OwnershipKind::Owned && local_info.ty.needs_drop() {
                    // Only drop if it was assigned/initialized
                    if assigned_in_block.contains(&local_id) {
                        new_statements.push(MirStatement::Drop(local_id));
                    }
                }
            }
        }

        block.statements = new_statements;
        Ok(())
    }
}
