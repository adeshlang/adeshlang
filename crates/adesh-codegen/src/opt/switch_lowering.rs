//! Multi-way Branch and Pattern Match Switch Lowering.
//!
//! Chooses between a compare-chain dispatch for small or dense case sets and a
//! balanced binary decision tree for sparse ones.
//!
//! Note: a true O(1) jump table (an indexed indirect branch through a rodata
//! table of block addresses) is not expressible in Machine IR: there is no
//! indexed-indirect-branch instruction, and per-entry relocations against
//! function-local block labels are not modelled. Dense case sets therefore
//! lower to an O(N) compare chain over the case values, not to a table jump.

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
    /// Dense continuous range of case values: O(N) compare-chain dispatch over
    /// the case values (see the module docs for why this is not a jump table).
    Dense {
        default_label: String,
        /// Lowest case value in the range; entry `i` covers the value
        /// `min_value + i`.
        min_value: i64,
        /// Entry `i` is the label branched to for value `min_value + i`;
        /// entries equal to `default_label` are holes in the range.
        entries: Vec<String>,
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
    /// Analyze cases and choose the lowering strategy.
    ///
    /// - no or very few cases (<= 3): `Linear`
    /// - dense range (span <= 512 and density >= 40%): `Dense`
    /// - otherwise: `BinarySearchTree`
    pub fn select_strategy(
        mut cases: Vec<SwitchCase>,
        default_label: impl Into<String>,
    ) -> SwitchStrategy {
        let def_lbl = default_label.into();

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

        // Dense range: compare-chain dispatch over the values.
        if span <= 512 && (count * 100 / span) >= 40 {
            let mut entries = vec![def_lbl.clone(); span];
            for case in &cases {
                let idx = (case.value - min_val) as usize;
                entries[idx] = case.target_label.clone();
            }

            SwitchStrategy::Dense {
                default_label: def_lbl,
                min_value: min_val,
                entries,
            }
        } else {
            // Sparse values: O(log N) binary search tree.
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
            SwitchStrategy::Dense {
                default_label,
                min_value,
                entries,
            } => {
                // Compare the scrutinee against every covered case value and
                // branch to the matching label. Holes (entries pointing at the
                // default) need no comparison: they fall through to the
                // default branch at the end.
                let blk = &mut func.blocks[current_block_idx];
                let val = MachineOperand::Register(MachineRegister::Virtual(scrutinee));
                for (offset, target) in entries.into_iter().enumerate() {
                    if target == default_label {
                        continue;
                    }
                    blk.push(MachineInstruction::Compare {
                        lhs: val.clone(),
                        rhs: MachineOperand::Immediate(min_value + offset as i64),
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

        // Labels are keyed by block index: two switches in one function can
        // share a pivot value, and duplicate block labels misroute branches.
        let left_label = format!("bst_left_b{}", func.blocks.len());
        let left_block_id = func.create_block(left_label.clone());
        let right_label = format!("bst_right_b{}", func.blocks.len());
        let right_block_id = func.create_block(right_label.clone());

        func.blocks[block_idx].push(MachineInstruction::BranchCc {
            cc: ConditionCode::LessThan,
            target: left_label,
        });
        func.blocks[block_idx].push(MachineInstruction::Branch {
            target: right_label,
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
    fn test_strategy_selection_dense_sparse_few() {
        // Dense cases 0, 1, 2, 3, 4, 5
        let dense_cases: Vec<SwitchCase> = (0..6)
            .map(|i| SwitchCase {
                value: i,
                target_label: format!("case_{}", i),
            })
            .collect();

        let strategy = SwitchLowering::select_strategy(dense_cases, "default");
        assert!(matches!(strategy, SwitchStrategy::Dense { .. }));

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

        let strategy_sparse = SwitchLowering::select_strategy(sparse_cases, "default");
        assert!(matches!(
            strategy_sparse,
            SwitchStrategy::BinarySearchTree { .. }
        ));

        // Very few cases stay linear.
        let few = vec![
            SwitchCase {
                value: 7,
                target_label: "c1".into(),
            },
            SwitchCase {
                value: 9,
                target_label: "c2".into(),
            },
        ];
        let strategy_few = SwitchLowering::select_strategy(few, "default");
        assert!(matches!(strategy_few, SwitchStrategy::Linear { .. }));
    }

    #[test]
    fn test_dense_lowering_emits_one_compare_per_covered_value() {
        let mut func = MachineFunction::new("test_switch_dense");
        let v_scrutinee = func.alloc_vreg();

        // Values 1..=6 with values 3 and 5 falling through to the default
        // (4 cases is enough to leave the `Linear` strategy, and the range is
        // dense enough for `Dense`).
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
                value: 4,
                target_label: "handle_4".into(),
            },
            SwitchCase {
                value: 6,
                target_label: "handle_6".into(),
            },
        ];
        let strategy = SwitchLowering::select_strategy(cases, "handle_default");

        match &strategy {
            SwitchStrategy::Dense {
                min_value,
                entries,
                default_label,
            } => {
                assert_eq!(*min_value, 1);
                assert_eq!(entries.len(), 6);
                assert_eq!(entries[2], "handle_default");
                assert_eq!(entries[4], "handle_default");
                assert_eq!(default_label, "handle_default");
            }
            other => panic!("expected Dense strategy, got {other:?}"),
        }

        SwitchLowering::lower_switch(&mut func, 0, v_scrutinee, strategy);

        let insts = &func.blocks[0].instructions;
        // One Compare per *covered* value (1, 2, 4, 6 - holes are skipped),
        // one BranchCc each, and the trailing default Branch.
        let compares = insts
            .iter()
            .filter(|i| matches!(i, MachineInstruction::Compare { .. }))
            .count();
        let branches = insts
            .iter()
            .filter(|i| matches!(i, MachineInstruction::BranchCc { .. }))
            .count();
        let defaults = insts
            .iter()
            .filter(
                |i| matches!(i, MachineInstruction::Branch { target } if target == "handle_default"),
            )
            .count();
        assert_eq!(compares, 4);
        assert_eq!(branches, 4);
        assert_eq!(defaults, 1);
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

        let strategy = SwitchLowering::select_strategy(cases, "handle_default");
        SwitchLowering::lower_switch(&mut func, 0, v_scrutinee, strategy);

        assert!(!func.blocks[0].instructions.is_empty());
    }
}
