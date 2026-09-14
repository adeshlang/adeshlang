//! ARC Insertion
//!
//! Inserts ARC clone and drop operations where needed for shared ownership.

use super::{MirFunction, MirModule, MirStatement, MirType};

/// ARC insertion pass
pub struct ArcInsertion;

impl ArcInsertion {
    /// Insert ARC operations for a module
    pub fn insert_arc_ops(module: &mut MirModule) -> Result<(), String> {
        for func in &mut module.functions {
            Self::insert_arc_ops_function(func)?;
        }
        Ok(())
    }

    /// Insert ARC operations for a function
    fn insert_arc_ops_function(func: &mut MirFunction) -> Result<(), String> {
        // Find locals that need ARC
        let arc_locals: Vec<_> = func
            .locals
            .iter()
            .enumerate()
            .filter(|(_, local)| matches!(local.ty, MirType::Arc(_)))
            .map(|(i, _)| i as super::LocalId)
            .collect();

        if arc_locals.is_empty() {
            return Ok(());
        }

        // Insert ARC operations in blocks
        for block in &mut func.body {
            let mut new_statements = Vec::new();

            for stmt in &block.statements {
                match stmt {
                    MirStatement::Assign(dest, rvalue) => {
                        if arc_locals.contains(dest) {
                            let src_local_opt = match rvalue {
                                super::MirRvalue::Use(super::MirOperand::Move(place)) => {
                                    Some(place.local)
                                }
                                super::MirRvalue::Use(super::MirOperand::Copy(place)) => {
                                    Some(place.local)
                                }
                                _ => None,
                            };
                            if let Some(src_local) = src_local_opt {
                                if arc_locals.contains(&src_local) {
                                    new_statements.push(MirStatement::ArcClone(*dest, src_local));
                                }
                            }
                            new_statements.push(stmt.clone());
                        } else {
                            new_statements.push(stmt.clone());
                        }
                    }
                    _ => {
                        new_statements.push(stmt.clone());
                    }
                }
            }

            // Insert ARC drops at end of scope for ARC locals
            for &local in &arc_locals {
                new_statements.push(MirStatement::ArcDrop(local));
            }

            block.statements = new_statements;
        }

        Ok(())
    }
}
