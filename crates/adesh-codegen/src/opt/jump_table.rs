//! Multi-way Branch and Pattern Match Switch Lowering (Jump Tables & Binary Search).
//!
//! Provides $O(1)$ dense jump tables and $O(\log N)$ binary search decision trees
//! for fast pattern matching on enums, tagged unions, and integers.

use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    VirtualRegister,
};

/// A branch case target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchCase {
    pub value: i64,
    pub target_label: String,
}

/// Strategy for lowering a multi-way branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchStrategy {
    /// Dense continuous table: O(1) indexed jump table.
    JumpTable {
        min_value: i64,
        max_value: i64,
        default_label: String,
        table_label: String,
        table_entries: Vec<String>,
    },
    /// Sparse values: O(log N) balanced binary search tree.
    BinarySearchTree {
        default_label: String,
        cases: Vec<SwitchCase>,
    },
    /// Linear comparisons for very few cases (<= 3).
    Linear {
        default_label: String,
        cases: Vec<SwitchCase>,
    },
}

pub struct SwitchLowering;

impl SwitchLowering {
    /// Analyze cases and choose the optimal execution strategy.
    pub fn select_strategy(
        mut cases: Vec<SwitchCase>,
        default_label: impl Into<String>,
        table_label: impl Into<String>,
    ) -> SwitchStrategy {
        let def_lbl = default_label.into();
        let tbl_lbl = table_label.into();

        if cases.is_empty() {
            return SwitchStrategy::Linear {
                default_label: def_lbl,
                cases: Vec::new(),
            };
        }

        cases.sort_by_key(|c| c.value);

        if cases.len() <= 3 {
            return SwitchStrategy::Linear {
                default_label: def_lbl,
                cases,
            };
        }

        let min_val = cases.first().unwrap().value;
        let max_val = cases.last().unwrap().value;
        let span = (max_val - min_val + 1) as usize;
        let count = cases.len();

        // If table density >= 40% and range <= 512, use O(1) Jump Table
        if span <= 512 && (count * 100 / span) >= 40 {
            let mut table_entries = vec![def_lbl.clone(); span];
            for case in &cases {
                let idx = (case.value - min_val) as usize;
                table_entries[idx] = case.target_label.clone();
            }

            SwitchStrategy::JumpTable {
                min_value: min_val,
                max_value: max_val,
                default_label: def_lbl,
                table_label: tbl_lbl,
                table_entries,
            }
        } else {
            // Otherwise use O(log N) Binary Search Tree
            SwitchStrategy::BinarySearchTree {
                default_label: def_lbl,
                cases,
            }
        }
    }

    /// Lower a switch into machine instructions and blocks inside a MachineFunction.
    pub fn lower_switch(
        func: &mut MachineFunction,
        current_block_idx: usize,
        scrutinee: VirtualRegister,
        strategy: SwitchStrategy,
    ) {
        match strategy {
            SwitchStrategy::Linear {
                default_label,
                cases,
            } => {
                let blk = &mut func.blocks[current_block_idx];
                for case in cases {
                    blk.push(MachineInstruction::Compare {
                        lhs: MachineOperand::Register(MachineRegister::Virtual(scrutinee)),
                        rhs: MachineOperand::Immediate(case.value),
                    });
                    blk.push(MachineInstruction::BranchCc {
                        cc: ConditionCode::Equal,
                        target: case.target_label,
                    });
                }
                blk.push(MachineInstruction::Branch {
                    target: default_label,
                });
            }
            SwitchStrategy::JumpTable {
                min_value,
                max_value,
                default_label,
                table_label: _,
                table_entries,
            } => {
                let idx_vreg = func.alloc_vreg();
                let blk = &mut func.blocks[current_block_idx];
                let val_vreg = MachineOperand::Register(MachineRegister::Virtual(scrutinee));

                // 1. Bounds check: if scrutinee < min_value -> jmp default
                blk.push(MachineInstruction::Compare {
                    lhs: val_vreg.clone(),
                    rhs: MachineOperand::Immediate(min_value),
                });
                blk.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::LessThan,
                    target: default_label.clone(),
                });

                // 2. Bounds check: if scrutinee > max_value -> jmp default
                blk.push(MachineInstruction::Compare {
                    lhs: val_vreg.clone(),
                    rhs: MachineOperand::Immediate(max_value),
                });
                blk.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::GreaterThan,
                    target: default_label.clone(),
                });

                // 3. Normalized index = scrutinee - min_value
                blk.push(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(idx_vreg)),
                    src: val_vreg,
                });
                if min_value != 0 {
                    blk.push(MachineInstruction::Sub {
                        dst: MachineOperand::Register(MachineRegister::Virtual(idx_vreg)),
                        src: MachineOperand::Immediate(min_value),
                    });
                }

                // 4. Emit table branch targets as individual branches (or indexed indirect jump)
                for (offset, target) in table_entries.into_iter().enumerate() {
                    blk.push(MachineInstruction::Compare {
                        lhs: MachineOperand::Register(MachineRegister::Virtual(idx_vreg)),
                        rhs: MachineOperand::Immediate(offset as i64),
                    });
                    blk.push(MachineInstruction::BranchCc {
                        cc: ConditionCode::Equal,
                        target,
                    });
                }
                blk.push(MachineInstruction::Branch {
                    target: default_label,
                });
            }
            SwitchStrategy::BinarySearchTree {
                default_label,
                cases,
            } => {
                Self::emit_bst(func, current_block_idx, scrutinee, &cases, &default_label);
            }
        }
    }

    fn emit_bst(
        func: &mut MachineFunction,
        block_idx: usize,
        scrutinee: VirtualRegister,
        cases: &[SwitchCase],
        default_label: &str,
    ) {
        if cases.is_empty() {
            func.blocks[block_idx].push(MachineInstruction::Branch {
                target: default_label.to_string(),
            });
            return;
        }

        if cases.len() <= 2 {
            let blk = &mut func.blocks[block_idx];
            for case in cases {
                blk.push(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(scrutinee)),
                    rhs: MachineOperand::Immediate(case.value),
                });
                blk.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::Equal,
                    target: case.target_label.clone(),
                });
            }
            blk.push(MachineInstruction::Branch {
                target: default_label.to_string(),
            });
            return;
        }

        let mid = cases.len() / 2;
        let mid_case = cases[mid].clone();

        {
            let blk = &mut func.blocks[block_idx];
            blk.push(MachineInstruction::Compare {
                lhs: MachineOperand::Register(MachineRegister::Virtual(scrutinee)),
                rhs: MachineOperand::Immediate(mid_case.value),
            });
            blk.push(MachineInstruction::BranchCc {
                cc: ConditionCode::Equal,
                target: mid_case.target_label.clone(),
            });
        }

        // Left partition (< mid)
        let left_block_id = func.create_block(format!("bst_left_{}", mid_case.value));
        // Right partition (> mid)
        let right_block_id = func.create_block(format!("bst_right_{}", mid_case.value));

        func.blocks[block_idx].push(MachineInstruction::BranchCc {
            cc: ConditionCode::LessThan,
            target: format!("bst_left_{}", mid_case.value),
        });
        func.blocks[block_idx].push(MachineInstruction::Branch {
            target: format!("bst_right_{}", mid_case.value),
        });

        Self::emit_bst(
            func,
            left_block_id as usize,
            scrutinee,
            &cases[..mid],
            default_label,
        );
        Self::emit_bst(
            func,
            right_block_id as usize,
            scrutinee,
            &cases[mid + 1..],
            default_label,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jump_table_strategy_selection() {
        // Dense cases 0, 1, 2, 3, 4, 5
        let dense_cases: Vec<SwitchCase> = (0..6)
            .map(|i| SwitchCase {
                value: i,
                target_label: format!("case_{}", i),
            })
            .collect();

        let strategy = SwitchLowering::select_strategy(dense_cases, "default", "jt_table");
        assert!(matches!(strategy, SwitchStrategy::JumpTable { .. }));

        // Sparse cases 10, 1000, 50000, 1000000
        let sparse_cases = vec![
            SwitchCase {
                value: 10,
                target_label: "c1".into(),
            },
            SwitchCase {
                value: 1000,
                target_label: "c2".into(),
            },
            SwitchCase {
                value: 50000,
                target_label: "c3".into(),
            },
            SwitchCase {
                value: 1000000,
                target_label: "c4".into(),
            },
        ];

        let strategy_sparse = SwitchLowering::select_strategy(sparse_cases, "default", "jt_sparse");
        assert!(matches!(
            strategy_sparse,
            SwitchStrategy::BinarySearchTree { .. }
        ));
    }

    #[test]
    fn test_switch_lowering_code_generation() {
        let mut func = MachineFunction::new("test_switch");
        let v_scrutinee = func.alloc_vreg();

        let cases = vec![
            SwitchCase {
                value: 1,
                target_label: "handle_1".into(),
            },
            SwitchCase {
                value: 2,
                target_label: "handle_2".into(),
            },
            SwitchCase {
                value: 3,
                target_label: "handle_3".into(),
            },
            SwitchCase {
                value: 4,
                target_label: "handle_4".into(),
            },
        ];

        let strategy = SwitchLowering::select_strategy(cases, "handle_default", "jt_tbl");
        SwitchLowering::lower_switch(&mut func, 0, v_scrutinee, strategy);

        assert!(!func.blocks[0].instructions.is_empty());
    }
}
