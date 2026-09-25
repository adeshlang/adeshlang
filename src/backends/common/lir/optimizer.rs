//! Global and Local LIR Optimization Pipeline
//!
//! Transforms and optimizes LIR functions across all compiler backends (Native JIT, Cranelift JIT, AOT):
//! - Constant Folding & Inter-instruction Constant Propagation
//! - Strength Reduction & Algebraic Simplification
//! - Local Store-Load Forwarding & Copy Propagation
//! - Dead Store Elimination
//! - Control Flow Simplification & Jump Threading
//! - Dead / Unreachable Basic Block Pruning
//! - Dead Instruction Elimination (DCE)

use super::{BlockId, LirBlock, LirFunction, LirInst, LirModule, ValueId};
use std::collections::{HashMap, HashSet};

/// Internal representation of known constant values
#[derive(Debug, Clone, PartialEq)]
enum ConstVal {
    I64(i64),
    F64(f64),
    Bool(bool),
    Null,
}

pub struct LirOptimizer {
    max_iterations: usize,
}

impl Default for LirOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl LirOptimizer {
    pub fn new() -> Self {
        LirOptimizer { max_iterations: 8 }
    }

    /// Optimize an entire LIR module in-place
    pub fn optimize_module(&mut self, module: &mut LirModule) {
        for func in &mut module.functions {
            self.optimize_function(func);
        }
    }

    /// Optimize a single LIR function in-place
    pub fn optimize_function(&mut self, func: &mut LirFunction) {
        for _ in 0..self.max_iterations {
            let mut changed = false;

            changed |= self.pass_constant_folding_and_forwarding(func);
            changed |= self.pass_simplify_control_flow(func);
            changed |= self.pass_jump_threading(func);
            changed |= self.pass_eliminate_dead_blocks(func);
            changed |= self.pass_dead_code_elimination(func);

            if !changed {
                break;
            }
        }
    }

    /// Resolve variable aliases to their canonical root
    fn resolve_alias(aliases: &HashMap<ValueId, ValueId>, mut val: ValueId) -> ValueId {
        let mut depth = 0;
        while let Some(&next) = aliases.get(&val) {
            if next == val || depth > 100 {
                break;
            }
            val = next;
            depth += 1;
        }
        val
    }

    /// Pass 1: Constant folding, algebraic reduction, store-load forwarding, and copy propagation
    fn pass_constant_folding_and_forwarding(&mut self, func: &mut LirFunction) -> bool {
        let mut changed = false;
        let mut constants: HashMap<ValueId, ConstVal> = HashMap::new();
        let mut aliases: HashMap<ValueId, ValueId> = HashMap::new();

        for block in &mut func.blocks {
            let mut var_map: HashMap<String, ValueId> = HashMap::new();
            let mut new_instructions = Vec::with_capacity(block.instructions.len());

            for inst in block.instructions.drain(..) {
                // 1. Substitute operands with known aliases
                let inst = self.substitute_aliases(inst, &aliases);

                // 2. Perform optimization on the instruction
                match inst {
                    // Constant loads
                    LirInst::ConstI64(dst, n) => {
                        constants.insert(dst, ConstVal::I64(n));
                        new_instructions.push(LirInst::ConstI64(dst, n));
                    }
                    LirInst::ConstF64(dst, n) => {
                        constants.insert(dst, ConstVal::F64(n));
                        new_instructions.push(LirInst::ConstF64(dst, n));
                    }
                    LirInst::ConstBool(dst, b) => {
                        constants.insert(dst, ConstVal::Bool(b));
                        new_instructions.push(LirInst::ConstBool(dst, b));
                    }
                    LirInst::ConstNull(dst) => {
                        constants.insert(dst, ConstVal::Null);
                        new_instructions.push(LirInst::ConstNull(dst));
                    }

                    // Copy propagation
                    LirInst::Copy(dst, src) => {
                        let root_src = Self::resolve_alias(&aliases, src);
                        if dst == root_src {
                            // Redundant self-copy: skip
                            changed = true;
                            continue;
                        }
                        aliases.insert(dst, root_src);
                        if let Some(c) = constants.get(&root_src).cloned() {
                            constants.insert(dst, c);
                        }
                        new_instructions.push(LirInst::Copy(dst, root_src));
                    }

                    // StoreVar: update local variable tracking
                    LirInst::StoreVar(name, val) => {
                        let root_val = Self::resolve_alias(&aliases, val);
                        var_map.insert(name.clone(), root_val);
                        new_instructions.push(LirInst::StoreVar(name, root_val));
                    }

                    // LoadVar: Store-to-Load Forwarding
                    LirInst::LoadVar(dst, name) => {
                        if let Some(&cached_val) = var_map.get(&name) {
                            // Forward directly to the cached value!
                            aliases.insert(dst, cached_val);
                            if let Some(c) = constants.get(&cached_val).cloned() {
                                constants.insert(dst, c);
                            }
                            new_instructions.push(LirInst::Copy(dst, cached_val));
                            changed = true;
                        } else {
                            new_instructions.push(LirInst::LoadVar(dst, name));
                        }
                    }

                    // Arithmetic, bitwise, comparison & strength reduction
                    _ => {
                        if let Some(optimized) =
                            self.try_fold_or_reduce(&inst, &constants, &mut aliases)
                        {
                            if let Some(opt_inst) = optimized {
                                // Record new constant if produced
                                self.record_const_from_inst(&opt_inst, &mut constants);
                                new_instructions.push(opt_inst);
                            }
                            changed = true;
                        } else {
                            new_instructions.push(inst);
                        }
                    }
                }
            }

            block.instructions = new_instructions;
        }

        changed
    }

    /// Substitute value aliases in an instruction
    fn substitute_aliases(&self, inst: LirInst, aliases: &HashMap<ValueId, ValueId>) -> LirInst {
        if aliases.is_empty() {
            return inst;
        }
        match inst {
            LirInst::Copy(dst, src) => LirInst::Copy(dst, Self::resolve_alias(aliases, src)),
            LirInst::AddI64(dst, a, b) => LirInst::AddI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::SubI64(dst, a, b) => LirInst::SubI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::MulI64(dst, a, b) => LirInst::MulI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::DivI64(dst, a, b) => LirInst::DivI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::ModI64(dst, a, b) => LirInst::ModI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::NegI64(dst, a) => LirInst::NegI64(dst, Self::resolve_alias(aliases, a)),
            LirInst::Not(dst, a) => LirInst::Not(dst, Self::resolve_alias(aliases, a)),
            LirInst::AddF64(dst, a, b) => LirInst::AddF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::SubF64(dst, a, b) => LirInst::SubF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::MulF64(dst, a, b) => LirInst::MulF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::DivF64(dst, a, b) => LirInst::DivF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::NegF64(dst, a) => LirInst::NegF64(dst, Self::resolve_alias(aliases, a)),
            LirInst::CmpLtI64(dst, a, b) => LirInst::CmpLtI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpLeI64(dst, a, b) => LirInst::CmpLeI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpGtI64(dst, a, b) => LirInst::CmpGtI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpGeI64(dst, a, b) => LirInst::CmpGeI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpEqI64(dst, a, b) => LirInst::CmpEqI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpNeI64(dst, a, b) => LirInst::CmpNeI64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpLtF64(dst, a, b) => LirInst::CmpLtF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpLeF64(dst, a, b) => LirInst::CmpLeF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpGtF64(dst, a, b) => LirInst::CmpGtF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpGeF64(dst, a, b) => LirInst::CmpGeF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpEqF64(dst, a, b) => LirInst::CmpEqF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::CmpNeF64(dst, a, b) => LirInst::CmpNeF64(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::And(dst, a, b) => LirInst::And(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::Or(dst, a, b) => LirInst::Or(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::BitAnd(dst, a, b) => LirInst::BitAnd(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::BitOr(dst, a, b) => LirInst::BitOr(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::BitXor(dst, a, b) => LirInst::BitXor(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::Shl(dst, a, b) => LirInst::Shl(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::Shr(dst, a, b) => LirInst::Shr(
                dst,
                Self::resolve_alias(aliases, a),
                Self::resolve_alias(aliases, b),
            ),
            LirInst::I64ToF64(dst, a) => LirInst::I64ToF64(dst, Self::resolve_alias(aliases, a)),
            LirInst::F64ToI64(dst, a) => LirInst::F64ToI64(dst, Self::resolve_alias(aliases, a)),
            LirInst::StoreVar(name, a) => LirInst::StoreVar(name, Self::resolve_alias(aliases, a)),
            LirInst::JumpIf(cond, t1, t2) => {
                LirInst::JumpIf(Self::resolve_alias(aliases, cond), t1, t2)
            }
            LirInst::Return(Some(a)) => LirInst::Return(Some(Self::resolve_alias(aliases, a))),
            LirInst::Call(dst, name, args) => {
                let mapped_args = args
                    .into_iter()
                    .map(|a| Self::resolve_alias(aliases, a))
                    .collect();
                LirInst::Call(dst, name, mapped_args)
            }
            LirInst::CallBuiltin(dst, name, args) => {
                let mapped_args = args
                    .into_iter()
                    .map(|a| Self::resolve_alias(aliases, a))
                    .collect();
                LirInst::CallBuiltin(dst, name, mapped_args)
            }
            LirInst::CallBuiltinGeneric(dst, name, args, generic_ty) => {
                let mapped_args = args
                    .into_iter()
                    .map(|a| Self::resolve_alias(aliases, a))
                    .collect();
                LirInst::CallBuiltinGeneric(dst, name, mapped_args, generic_ty)
            }
            _ => inst,
        }
    }

    /// Helper to record newly produced constants
    fn record_const_from_inst(&self, inst: &LirInst, constants: &mut HashMap<ValueId, ConstVal>) {
        match inst {
            LirInst::ConstI64(dst, n) => {
                constants.insert(*dst, ConstVal::I64(*n));
            }
            LirInst::ConstF64(dst, n) => {
                constants.insert(*dst, ConstVal::F64(*n));
            }
            LirInst::ConstBool(dst, b) => {
                constants.insert(*dst, ConstVal::Bool(*b));
            }
            LirInst::ConstNull(dst) => {
                constants.insert(*dst, ConstVal::Null);
            }
            _ => {}
        }
    }

    /// Try to fold constants or perform strength reduction
    fn try_fold_or_reduce(
        &self,
        inst: &LirInst,
        constants: &HashMap<ValueId, ConstVal>,
        aliases: &mut HashMap<ValueId, ValueId>,
    ) -> Option<Option<LirInst>> {
        match inst {
            // Integer addition
            LirInst::AddI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstI64(*dst, va.wrapping_add(*vb))))
                }
                (_, Some(ConstVal::I64(0))) => {
                    aliases.insert(*dst, *a);
                    Some(Some(LirInst::Copy(*dst, *a)))
                }
                (Some(ConstVal::I64(0)), _) => {
                    aliases.insert(*dst, *b);
                    Some(Some(LirInst::Copy(*dst, *b)))
                }
                _ => None,
            },

            // Integer subtraction
            LirInst::SubI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstI64(*dst, va.wrapping_sub(*vb))))
                }
                (_, Some(ConstVal::I64(0))) => {
                    aliases.insert(*dst, *a);
                    Some(Some(LirInst::Copy(*dst, *a)))
                }
                _ if a == b => Some(Some(LirInst::ConstI64(*dst, 0))),
                _ => None,
            },

            // Integer multiplication
            LirInst::MulI64(dst, a, b) => {
                match (constants.get(a), constants.get(b)) {
                    (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                        Some(Some(LirInst::ConstI64(*dst, va.wrapping_mul(*vb))))
                    }
                    (Some(ConstVal::I64(0)), _) | (_, Some(ConstVal::I64(0))) => {
                        Some(Some(LirInst::ConstI64(*dst, 0)))
                    }
                    (_, Some(ConstVal::I64(1))) => {
                        aliases.insert(*dst, *a);
                        Some(Some(LirInst::Copy(*dst, *a)))
                    }
                    (Some(ConstVal::I64(1)), _) => {
                        aliases.insert(*dst, *b);
                        Some(Some(LirInst::Copy(*dst, *b)))
                    }
                    // x * 2 -> x + x
                    (_, Some(ConstVal::I64(2))) => Some(Some(LirInst::AddI64(*dst, *a, *a))),
                    (Some(ConstVal::I64(2)), _) => Some(Some(LirInst::AddI64(*dst, *b, *b))),
                    _ => None,
                }
            }

            // Integer division
            LirInst::DivI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) if *vb != 0 => {
                    Some(Some(LirInst::ConstI64(*dst, va / vb)))
                }
                (_, Some(ConstVal::I64(1))) => {
                    aliases.insert(*dst, *a);
                    Some(Some(LirInst::Copy(*dst, *a)))
                }
                _ if a == b => Some(Some(LirInst::ConstI64(*dst, 1))),
                _ => None,
            },

            // Integer modulo
            LirInst::ModI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) if *vb != 0 => {
                    Some(Some(LirInst::ConstI64(*dst, va % vb)))
                }
                _ => None,
            },

            // Integer negation
            LirInst::NegI64(dst, a) => {
                if let Some(ConstVal::I64(va)) = constants.get(a) {
                    Some(Some(LirInst::ConstI64(*dst, -va)))
                } else {
                    None
                }
            }

            // Boolean negation
            LirInst::Not(dst, a) => {
                if let Some(ConstVal::Bool(ba)) = constants.get(a) {
                    Some(Some(LirInst::ConstBool(*dst, !ba)))
                } else {
                    None
                }
            }

            // Bitwise operations
            LirInst::BitAnd(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstI64(*dst, va & vb)))
                }
                (Some(ConstVal::I64(0)), _) | (_, Some(ConstVal::I64(0))) => {
                    Some(Some(LirInst::ConstI64(*dst, 0)))
                }
                _ if a == b => {
                    aliases.insert(*dst, *a);
                    Some(Some(LirInst::Copy(*dst, *a)))
                }
                _ => None,
            },
            LirInst::BitOr(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstI64(*dst, va | vb)))
                }
                (_, Some(ConstVal::I64(0))) => {
                    aliases.insert(*dst, *a);
                    Some(Some(LirInst::Copy(*dst, *a)))
                }
                (Some(ConstVal::I64(0)), _) => {
                    aliases.insert(*dst, *b);
                    Some(Some(LirInst::Copy(*dst, *b)))
                }
                _ if a == b => {
                    aliases.insert(*dst, *a);
                    Some(Some(LirInst::Copy(*dst, *a)))
                }
                _ => None,
            },
            LirInst::BitXor(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstI64(*dst, va ^ vb)))
                }
                _ if a == b => Some(Some(LirInst::ConstI64(*dst, 0))),
                (_, Some(ConstVal::I64(0))) => {
                    aliases.insert(*dst, *a);
                    Some(Some(LirInst::Copy(*dst, *a)))
                }
                (Some(ConstVal::I64(0)), _) => {
                    aliases.insert(*dst, *b);
                    Some(Some(LirInst::Copy(*dst, *b)))
                }
                _ => None,
            },

            // Integer comparisons
            LirInst::CmpLtI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstBool(*dst, va < vb)))
                }
                _ => None,
            },
            LirInst::CmpLeI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstBool(*dst, va <= vb)))
                }
                _ => None,
            },
            LirInst::CmpGtI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstBool(*dst, va > vb)))
                }
                _ => None,
            },
            LirInst::CmpGeI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstBool(*dst, va >= vb)))
                }
                _ => None,
            },
            LirInst::CmpEqI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstBool(*dst, va == vb)))
                }
                _ if a == b => Some(Some(LirInst::ConstBool(*dst, true))),
                _ => None,
            },
            LirInst::CmpNeI64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::I64(va)), Some(ConstVal::I64(vb))) => {
                    Some(Some(LirInst::ConstBool(*dst, va != vb)))
                }
                _ if a == b => Some(Some(LirInst::ConstBool(*dst, false))),
                _ => None,
            },

            // Float arithmetic
            LirInst::AddF64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::F64(va)), Some(ConstVal::F64(vb))) => {
                    Some(Some(LirInst::ConstF64(*dst, va + vb)))
                }
                _ => None,
            },
            LirInst::SubF64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::F64(va)), Some(ConstVal::F64(vb))) => {
                    Some(Some(LirInst::ConstF64(*dst, va - vb)))
                }
                _ => None,
            },
            LirInst::MulF64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::F64(va)), Some(ConstVal::F64(vb))) => {
                    Some(Some(LirInst::ConstF64(*dst, va * vb)))
                }
                _ => None,
            },
            LirInst::DivF64(dst, a, b) => match (constants.get(a), constants.get(b)) {
                (Some(ConstVal::F64(va)), Some(ConstVal::F64(vb))) if *vb != 0.0 => {
                    Some(Some(LirInst::ConstF64(*dst, va / vb)))
                }
                _ => None,
            },

            // Conversions
            LirInst::I64ToF64(dst, a) => {
                if let Some(ConstVal::I64(va)) = constants.get(a) {
                    Some(Some(LirInst::ConstF64(*dst, *va as f64)))
                } else {
                    None
                }
            }
            LirInst::F64ToI64(dst, a) => {
                if let Some(ConstVal::F64(va)) = constants.get(a) {
                    Some(Some(LirInst::ConstI64(*dst, *va as i64)))
                } else {
                    None
                }
            }

            _ => None,
        }
    }

    /// Pass 2: Control flow simplification (simplify JumpIf with constant conditions)
    fn pass_simplify_control_flow(&mut self, func: &mut LirFunction) -> bool {
        let mut changed = false;

        // Collect known constant booleans/integers from all blocks
        let mut constants: HashMap<ValueId, ConstVal> = HashMap::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                self.record_const_from_inst(inst, &mut constants);
            }
        }

        for block in &mut func.blocks {
            let mut new_instructions = Vec::with_capacity(block.instructions.len());

            for inst in block.instructions.drain(..) {
                match inst {
                    LirInst::JumpIf(cond, target_true, target_false) => {
                        if target_true == target_false {
                            // Both branches target same block: unconditional Jump
                            new_instructions.push(LirInst::Jump(target_true));
                            changed = true;
                        } else if let Some(c) = constants.get(&cond) {
                            let is_true = match c {
                                ConstVal::Bool(b) => *b,
                                ConstVal::I64(n) => *n != 0,
                                ConstVal::F64(n) => *n != 0.0,
                                ConstVal::Null => false,
                            };
                            if is_true {
                                new_instructions.push(LirInst::Jump(target_true));
                            } else {
                                new_instructions.push(LirInst::Jump(target_false));
                            }
                            changed = true;
                        } else {
                            new_instructions.push(LirInst::JumpIf(cond, target_true, target_false));
                        }
                    }
                    _ => new_instructions.push(inst),
                }
            }

            block.instructions = new_instructions;
        }

        changed
    }

    /// Pass 3: Jump threading (forward jumps across trampoline/empty jump blocks)
    fn pass_jump_threading(&mut self, func: &mut LirFunction) -> bool {
        let mut changed = false;

        // Find blocks that only contain a single unconditional Jump
        let mut jump_targets: HashMap<BlockId, BlockId> = HashMap::new();
        for block in &func.blocks {
            if block.instructions.len() == 1 {
                if let LirInst::Jump(target) = &block.instructions[0] {
                    if *target != block.id {
                        jump_targets.insert(block.id, *target);
                    }
                }
            }
        }

        if jump_targets.is_empty() {
            return false;
        }

        // Thread jumps through trampolines
        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                match inst {
                    LirInst::Jump(target) => {
                        let mut curr = *target;
                        while let Some(&next) = jump_targets.get(&curr) {
                            if next == curr {
                                break;
                            }
                            curr = next;
                            changed = true;
                        }
                        *target = curr;
                    }
                    LirInst::JumpIf(_, t1, t2) => {
                        let mut curr1 = *t1;
                        while let Some(&next) = jump_targets.get(&curr1) {
                            if next == curr1 {
                                break;
                            }
                            curr1 = next;
                            changed = true;
                        }
                        *t1 = curr1;

                        let mut curr2 = *t2;
                        while let Some(&next) = jump_targets.get(&curr2) {
                            if next == curr2 {
                                break;
                            }
                            curr2 = next;
                            changed = true;
                        }
                        *t2 = curr2;
                    }
                    _ => {}
                }
            }
        }

        changed
    }

    /// Pass 4: Unreachable block elimination
    fn pass_eliminate_dead_blocks(&mut self, func: &mut LirFunction) -> bool {
        let mut reachable = HashSet::new();
        let mut queue = vec![func.entry_block];
        reachable.insert(func.entry_block);

        let block_map: HashMap<BlockId, &LirBlock> =
            func.blocks.iter().map(|b| (b.id, b)).collect();

        while let Some(curr_id) = queue.pop() {
            if let Some(block) = block_map.get(&curr_id) {
                for inst in &block.instructions {
                    match inst {
                        LirInst::Jump(t) => {
                            if reachable.insert(*t) {
                                queue.push(*t);
                            }
                        }
                        LirInst::JumpIf(_, t1, t2) => {
                            if reachable.insert(*t1) {
                                queue.push(*t1);
                            }
                            if reachable.insert(*t2) {
                                queue.push(*t2);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let initial_len = func.blocks.len();
        func.blocks.retain(|b| reachable.contains(&b.id));
        func.blocks.len() != initial_len
    }

    /// Pass 5: Dead code elimination (remove unused pure values)
    fn pass_dead_code_elimination(&mut self, func: &mut LirFunction) -> bool {
        // Collect use count of all values
        let mut use_counts: HashMap<ValueId, usize> = HashMap::new();

        for block in &func.blocks {
            for inst in &block.instructions {
                self.record_uses(inst, &mut use_counts);
            }
        }

        let mut changed = false;

        for block in &mut func.blocks {
            let mut new_instructions = Vec::with_capacity(block.instructions.len());

            for inst in block.instructions.drain(..) {
                if let Some(dst) = self.get_pure_def(&inst) {
                    if use_counts.get(&dst).copied().unwrap_or(0) == 0 {
                        // Value is never used and instruction is pure: drop it!
                        changed = true;
                        continue;
                    }
                }
                new_instructions.push(inst);
            }

            block.instructions = new_instructions;
        }

        changed
    }

    /// Record value uses in an instruction
    fn record_uses(&self, inst: &LirInst, uses: &mut HashMap<ValueId, usize>) {
        let mut record = |v: ValueId| {
            *uses.entry(v).or_insert(0) += 1;
        };

        match inst {
            LirInst::Copy(_, s)
            | LirInst::NegI64(_, s)
            | LirInst::Not(_, s)
            | LirInst::NegF64(_, s)
            | LirInst::I64ToF64(_, s)
            | LirInst::F64ToI64(_, s)
            | LirInst::StoreVar(_, s)
            | LirInst::JumpIf(s, _, _) => {
                record(*s);
            }
            LirInst::Return(Some(s)) => {
                record(*s);
            }
            LirInst::AddI64(_, a, b)
            | LirInst::SubI64(_, a, b)
            | LirInst::MulI64(_, a, b)
            | LirInst::DivI64(_, a, b)
            | LirInst::ModI64(_, a, b)
            | LirInst::AddF64(_, a, b)
            | LirInst::SubF64(_, a, b)
            | LirInst::MulF64(_, a, b)
            | LirInst::DivF64(_, a, b)
            | LirInst::CmpLtI64(_, a, b)
            | LirInst::CmpLeI64(_, a, b)
            | LirInst::CmpGtI64(_, a, b)
            | LirInst::CmpGeI64(_, a, b)
            | LirInst::CmpEqI64(_, a, b)
            | LirInst::CmpNeI64(_, a, b)
            | LirInst::CmpLtF64(_, a, b)
            | LirInst::CmpLeF64(_, a, b)
            | LirInst::CmpGtF64(_, a, b)
            | LirInst::CmpGeF64(_, a, b)
            | LirInst::CmpEqF64(_, a, b)
            | LirInst::CmpNeF64(_, a, b)
            | LirInst::And(_, a, b)
            | LirInst::Or(_, a, b)
            | LirInst::BitAnd(_, a, b)
            | LirInst::BitOr(_, a, b)
            | LirInst::BitXor(_, a, b)
            | LirInst::Shl(_, a, b)
            | LirInst::Shr(_, a, b) => {
                record(*a);
                record(*b);
            }
            LirInst::Call(_, _, args)
            | LirInst::CallBuiltin(_, _, args)
            | LirInst::CallBuiltinGeneric(_, _, args, _)
            | LirInst::TailCall(_, args) => {
                for arg in args {
                    record(*arg);
                }
            }
            LirInst::ArcNew(_, a)
            | LirInst::ArcClone(_, a)
            | LirInst::ArcDrop(a)
            | LirInst::WeakNew(_, a)
            | LirInst::WeakDrop(a)
            | LirInst::ArcGet(_, a)
            | LirInst::ArcStrongCount(_, a)
            | LirInst::ArcWeakCount(_, a)
            | LirInst::Alloc(_, a)
            | LirInst::Free(a) => {
                record(*a);
            }
            LirInst::ArcSet(a, b) => {
                record(*a);
                record(*b);
            }
            LirInst::AllocTyped(_, a, _) => {
                record(*a);
            }
            LirInst::PtrLoad(_, p, idx) => {
                record(*p);
                record(*idx);
            }
            LirInst::PtrStore(p, val, idx) => {
                record(*p);
                record(*val);
                record(*idx);
            }
            _ => {}
        }
    }

    /// Get destination ValueId of a pure (side-effect-free) instruction that can be eliminated if unused
    fn get_pure_def(&self, inst: &LirInst) -> Option<ValueId> {
        match inst {
            LirInst::ConstI64(d, _)
            | LirInst::ConstF64(d, _)
            | LirInst::ConstBool(d, _)
            | LirInst::ConstString(d, _)
            | LirInst::ConstNull(d)
            | LirInst::ConstU8(d, _)
            | LirInst::ConstU16(d, _)
            | LirInst::ConstU32(d, _)
            | LirInst::ConstU64(d, _)
            | LirInst::ConstU128(d, _)
            | LirInst::ConstI8(d, _)
            | LirInst::ConstI16(d, _)
            | LirInst::ConstI32(d, _)
            | LirInst::ConstI128(d, _)
            | LirInst::ConstF32(d, _)
            | LirInst::Copy(d, _)
            | LirInst::AddI64(d, _, _)
            | LirInst::SubI64(d, _, _)
            | LirInst::MulI64(d, _, _)
            | LirInst::DivI64(d, _, _)
            | LirInst::ModI64(d, _, _)
            | LirInst::NegI64(d, _)
            | LirInst::Not(d, _)
            | LirInst::AddF64(d, _, _)
            | LirInst::SubF64(d, _, _)
            | LirInst::MulF64(d, _, _)
            | LirInst::DivF64(d, _, _)
            | LirInst::NegF64(d, _)
            | LirInst::CmpLtI64(d, _, _)
            | LirInst::CmpLeI64(d, _, _)
            | LirInst::CmpGtI64(d, _, _)
            | LirInst::CmpGeI64(d, _, _)
            | LirInst::CmpEqI64(d, _, _)
            | LirInst::CmpNeI64(d, _, _)
            | LirInst::CmpLtF64(d, _, _)
            | LirInst::CmpLeF64(d, _, _)
            | LirInst::CmpGtF64(d, _, _)
            | LirInst::CmpGeF64(d, _, _)
            | LirInst::CmpEqF64(d, _, _)
            | LirInst::CmpNeF64(d, _, _)
            | LirInst::And(d, _, _)
            | LirInst::Or(d, _, _)
            | LirInst::BitAnd(d, _, _)
            | LirInst::BitOr(d, _, _)
            | LirInst::BitXor(d, _, _)
            | LirInst::Shl(d, _, _)
            | LirInst::Shr(d, _, _)
            | LirInst::I64ToF64(d, _)
            | LirInst::F64ToI64(d, _) => Some(*d),
            _ => None,
        }
    }
}
