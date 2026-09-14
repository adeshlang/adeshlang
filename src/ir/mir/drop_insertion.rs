//! Drop Insertion
//!
//! Inserts explicit drop calls for values that need cleanup.

use super::{MirBlock, MirFunction, MirModule, MirStatement};

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
        for block in &mut func.body {
            Self::insert_drops_block(block, &func.locals)?;
        }
        Ok(())
    }

    /// Insert drops for a block
    fn insert_drops_block(block: &mut MirBlock, locals: &[super::MirLocal]) -> Result<(), String> {
        let mut new_statements = Vec::new();

        for stmt in &block.statements {
            new_statements.push(stmt.clone());

            // After assignments, check if we need to drop the old value
            if let MirStatement::Assign(local, _) = stmt {
                if let Some(local_info) = locals.get(*local as usize) {
                    if local_info.ty.needs_drop() {
                        // Insert drop for the old value
                        new_statements.push(MirStatement::Drop(*local));
                    }
                }
            }
        }

        block.statements = new_statements;
        Ok(())
    }
}
