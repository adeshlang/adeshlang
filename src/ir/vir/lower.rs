//! MIR to VIR Lowering
//!
//! Lower MIR (with ownership/borrow info) to VIR (explicit operations).

use super::{CmpOp, FloatBinOp, IntBinOp, IntUnOp};
use super::{ValueId, VirBlock, VirFunction, VirInstruction, VirModule, VirTerminator, VirType};
use crate::ir::mir::{
    MirBinOp, MirConstant, MirModule, MirOperand, MirType as MirTypeEnum, MirUnOp,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// String pool for interning string constants
#[derive(Default)]
struct StringPool {
    strings: HashMap<String, u32>,
    next_id: u32,
}

impl StringPool {
    fn new() -> Self {
        Self {
            strings: HashMap::new(),
            next_id: 0,
        }
    }

    fn intern(&mut self, value: &str) -> u32 {
        if let Some(&id) = self.strings.get(value) {
            id
        } else {
            let id = self.next_id;
            self.strings.insert(value.to_string(), id);
            self.next_id += 1;
            id
        }
    }

    #[allow(dead_code)]
    fn finish(self) -> HashMap<u32, String> {
        let mut result = HashMap::new();
        for (value, id) in self.strings {
            result.insert(id, value);
        }
        result
    }
}

/// Lowering context for value ID generation and  PHI node tracking
struct LoweringContext {
    next_value_id: ValueId,
    strings: StringPool,
    /// Maps MIR LocalId to VIR ValueId (tracks current variable version for each block)
    local_to_value: HashMap<u32, ValueId>,
    /// Track block predecessors for PHI node generation
    predecessors: HashMap<u32, Vec<u32>>,
    /// Track which locals are assigned in each block
    block_assignments: HashMap<u32, Vec<u32>>,
    /// Track the active local variable versions at the end of each block
    block_local_values: HashMap<u32, HashMap<u32, ValueId>>,
    /// Placeholders for Phi nodes to resolve incoming values later: (block_id, local_id, phi_dest)
    phi_placeholders: Vec<(u32, u32, ValueId)>,
}

impl LoweringContext {
    fn new() -> Self {
        Self {
            next_value_id: 0,
            strings: StringPool::new(),
            local_to_value: HashMap::new(),
            predecessors: HashMap::new(),
            block_assignments: HashMap::new(),
            block_local_values: HashMap::new(),
            phi_placeholders: Vec::new(),
        }
    }

    fn next_value(&mut self) -> ValueId {
        let id = self.next_value_id;
        self.next_value_id += 1;
        id
    }

    /// Map a MIR LocalId to a newly generated VIR ValueId
    #[allow(dead_code)]
    fn allocate_value_for_local(&mut self, local_id: u32) -> ValueId {
        let value_id = self.next_value();
        self.local_to_value.insert(local_id, value_id);
        value_id
    }

    /// Get the VIR ValueId for a MIR LocalId (returns the local ID as fallback)
    fn get_value_for_local(&self, local_id: u32) -> ValueId {
        self.local_to_value
            .get(&local_id)
            .copied()
            .unwrap_or(local_id)
    }

    /// Record that a local was assigned in a block
    fn record_assignment(&mut self, block_id: u32, local_id: u32) {
        self.block_assignments
            .entry(block_id)
            .or_insert_with(Vec::new)
            .push(local_id);
    }

    /// Add a predecessor relationship
    fn add_predecessor(&mut self, block_id: u32, pred_id: u32) {
        self.predecessors
            .entry(block_id)
            .or_insert_with(Vec::new)
            .push(pred_id);
    }
}

/// Lower MIR module to VIR
pub fn lower_mir_to_vir(mir: &MirModule) -> Result<VirModule, String> {
    let ctx = RefCell::new(LoweringContext::new());
    let mut vir = VirModule::new(mir.name.clone());

    // Lower each function
    for func in &mir.functions {
        let vir_func = lower_function(func, &ctx)?;
        vir.functions.push(vir_func);
    }

    // Lower type definitions
    for typedef in &mir.types {
        vir.types.push(lower_typedef(typedef));
    }

    // Extract string pool from context
    {
        let ctx_ref = ctx.borrow();
        let mut result = HashMap::new();
        for (value, id) in &ctx_ref.strings.strings {
            result.insert(*id, value.clone());
        }
        vir.strings = result;
    }

    Ok(vir)
}

fn lower_function(
    mir_func: &crate::ir::mir::MirFunction,
    ctx: &RefCell<LoweringContext>,
) -> Result<VirFunction, String> {
    let mut vir_func = VirFunction::new(mir_func.name.clone(), lower_type(&mir_func.return_type));

    vir_func.is_async = mir_func.is_async;

    // Reset context for this function (preserve string pool)
    {
        let mut c = ctx.borrow_mut();
        c.next_value_id = 0;
        c.local_to_value.clear();
        c.predecessors.clear();
        c.block_assignments.clear();
        c.block_local_values.clear();
        c.phi_placeholders.clear();
    }

    // Lower parameters and reserve VIR ValueIds for them
    for (idx, param) in mir_func.params.iter().enumerate() {
        vir_func.params.push(super::VirParam {
            name: param.name.clone(),
            ty: lower_type(&param.ty),
        });
        // Reserve a VIR ValueId for this parameter
        let param_value_id = ctx.borrow_mut().next_value();
        // Map the MIR local for this parameter to the reserved VIR ValueId
        // Params are the first N locals in MIR
        ctx.borrow_mut()
            .local_to_value
            .insert(idx as u32, param_value_id);
    }

    // Lower locals
    for local in &mir_func.locals {
        vir_func.locals.push(super::VirLocal {
            name: local.name.clone(),
            ty: lower_type(&local.ty),
        });
    }

    // Build CFG predecessor information
    build_cfg_info(mir_func, ctx);

    // Lower blocks
    for mir_block in &mir_func.body {
        let vir_block = lower_block(mir_block, mir_func, ctx)?;
        vir_func.blocks.push(vir_block);
    }

    // Insert PHI nodes at merge points
    insert_phi_nodes(&mut vir_func, mir_func, ctx)?;

    Ok(vir_func)
}

fn lower_block(
    mir_block: &crate::ir::mir::MirBlock,
    mir_func: &crate::ir::mir::MirFunction,
    ctx: &RefCell<LoweringContext>,
) -> Result<VirBlock, String> {
    use crate::ir::vir::VirPhi;

    let mut block_phis = Vec::new();

    // Restore or initialize local_to_value state for this block
    {
        let preds_opt = ctx.borrow().predecessors.get(&mir_block.id).cloned();
        if let Some(preds) = preds_opt {
            if !preds.is_empty() {
                // Initialize local_to_value to the state of the first predecessor
                let first_pred = preds[0];
                let initial_mappings = ctx.borrow().block_local_values.get(&first_pred).cloned();
                if let Some(mappings) = initial_mappings {
                    ctx.borrow_mut().local_to_value = mappings;
                }

                // If there are multiple predecessors, we may need Phi nodes
                if preds.len() > 1 {
                    let block_assignments = ctx.borrow().block_assignments.clone();
                    let mut assigned_locals = std::collections::HashSet::new();
                    for &pred_id in &preds {
                        if let Some(assignments) = block_assignments.get(&pred_id) {
                            assigned_locals.extend(assignments.iter().copied());
                        }
                    }

                    for local_id in assigned_locals {
                        let local_ty = mir_func
                            .locals
                            .get(local_id as usize)
                            .map(|l| lower_type(&l.ty))
                            .unwrap_or(VirType::I64);

                        let phi_dest = ctx.borrow_mut().next_value();

                        // Push a placeholder Phi node (will fill incoming later)
                        let vir_phi = VirPhi {
                            dest: phi_dest,
                            ty: local_ty,
                            incoming: Vec::new(),
                        };
                        block_phis.push(vir_phi);

                        // Update local_to_value mapping for this block
                        ctx.borrow_mut().local_to_value.insert(local_id, phi_dest);
                        // Record placeholder to resolve later
                        ctx.borrow_mut()
                            .phi_placeholders
                            .push((mir_block.id, local_id, phi_dest));
                    }
                }
            }
        }
    }

    let mut vir_block = VirBlock {
        id: mir_block.id,
        label: None,
        phis: block_phis,
        instructions: Vec::new(),
        terminator: VirTerminator::Unreachable,
    };

    // Lower statements to VIR instructions
    for stmt in &mir_block.statements {
        lower_statement(stmt, mir_func, &mut vir_block, mir_block.id, ctx)?;
    }

    // Lower terminator
    vir_block.terminator = lower_terminator(&mir_block.terminator, &mut vir_block, ctx)?;

    // Record the final mappings of local_to_value for this block
    {
        let final_mappings = ctx.borrow().local_to_value.clone();
        ctx.borrow_mut()
            .block_local_values
            .insert(mir_block.id, final_mappings);
    }

    Ok(vir_block)
}

fn lower_statement(
    stmt: &crate::ir::mir::MirStatement,
    mir_func: &crate::ir::mir::MirFunction,
    block: &mut VirBlock,
    block_id: u32,
    ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    use crate::ir::mir::MirStatement;

    match stmt {
        MirStatement::Assign(local, rvalue) => {
            // Allocate a fresh VIR ValueId for this assignment
            let vir_value = ctx.borrow_mut().next_value();
            // Record that this local was assigned in this block
            ctx.borrow_mut().record_assignment(block_id, *local);
            // Lower rvalue producing the VIR value
            lower_rvalue(vir_value, *local, rvalue, mir_func, block, ctx)?;
        }
        MirStatement::Drop(local) => {
            // Explicit drop operation - look up the VIR value for this local
            let value = ctx.borrow().get_value_for_local(*local);
            block.instructions.push(VirInstruction::Drop { value });
        }
        MirStatement::ArcClone(dest_local, src_local) => {
            // Explicit ARC clone - allocate fresh VIR value for dest
            let dest = ctx.borrow_mut().next_value();
            let src = ctx.borrow().get_value_for_local(*src_local);
            block
                .instructions
                .push(VirInstruction::ArcClone { dest, src });
            // Update mapping for dest local and record assignment
            ctx.borrow_mut().local_to_value.insert(*dest_local, dest);
            ctx.borrow_mut().record_assignment(block_id, *dest_local);
        }
        MirStatement::ArcDrop(ptr_local) => {
            // Explicit ARC drop - look up the VIR value for this local
            let ptr = ctx.borrow().get_value_for_local(*ptr_local);
            block.instructions.push(VirInstruction::ArcDrop { ptr });
        }
        MirStatement::Call { dest, func, args } => {
            // Allocate fresh VIR value for call result
            let vir_dest = ctx.borrow_mut().next_value();
            // Record assignment
            ctx.borrow_mut().record_assignment(block_id, *dest);
            // Lower function call
            lower_call(vir_dest, *dest, func, args, block, ctx)?;
        }
        MirStatement::StorageLive(_) | MirStatement::StorageDead(_) => {
            // These are hints, can be ignored in VIR
        }
        MirStatement::Nop => {
            block.instructions.push(VirInstruction::Nop);
        }
    }

    Ok(())
}

fn lower_terminator(
    term: &crate::ir::mir::MirTerminator,
    block: &mut VirBlock,
    ctx: &RefCell<LoweringContext>,
) -> Result<VirTerminator, String> {
    use crate::ir::mir::MirTerminator;

    Ok(match term {
        MirTerminator::Return(operand) => VirTerminator::Return {
            value: operand.as_ref().map(|op| lower_operand(op, block, ctx)),
        },
        MirTerminator::Goto(block_id) => VirTerminator::Jump { target: *block_id },
        MirTerminator::SwitchInt {
            discriminant,
            targets,
            otherwise,
        } => VirTerminator::Switch {
            value: lower_operand(discriminant, block, ctx),
            cases: targets.clone(),
            default: *otherwise,
        },
        MirTerminator::Call {
            func,
            args,
            destination,
            ..
        } => {
            // Lower call: emit Call instruction, then jump to continuation block
            let func_val = lower_operand(func, block, ctx);
            let arg_vals: Vec<ValueId> =
                args.iter().map(|a| lower_operand(a, block, ctx)).collect();

            let dest = destination.as_ref().and_then(|(place, _)| {
                // Map the destination place to a VIR ValueId
                Some(ctx.borrow().get_value_for_local(place.local))
            });

            block.instructions.push(VirInstruction::Call {
                dest,
                func: func_val,
                args: arg_vals,
            });

            // Jump to the continuation block
            if let Some((_, target)) = destination {
                VirTerminator::Jump { target: *target }
            } else {
                // No destination means void call — still need a terminator.
                // If there's no continuation block, this is a noreturn call.
                VirTerminator::Unreachable
            }
        }
        MirTerminator::Drop { place, target, .. } => {
            // Emit Drop instruction for the place, then jump to target
            let place_val = ctx.borrow().get_value_for_local(place.local);
            block.instructions.push(VirInstruction::Drop { value: place_val });
            VirTerminator::Jump { target: *target }
        }
        MirTerminator::Unreachable => VirTerminator::Unreachable,
    })
}

/// Lower MIR rvalue to VIR instructions
/// dest: the fresh VIR ValueId to store the result
/// mir_local: the MIR LocalId being assigned to (for tracking)
fn operand_type(operand: &MirOperand, mir_func: &crate::ir::mir::MirFunction) -> MirTypeEnum {
    match operand {
        MirOperand::Move(place) | MirOperand::Copy(place) => {
            if (place.local as usize) < mir_func.locals.len() {
                let ty = mir_func.locals[place.local as usize].ty.clone();
                if cfg!(debug_assertions) {
                    eprintln!(
                        "[operand_type] local {} ({:?}) type: {:?}",
                        place.local, mir_func.locals[place.local as usize].name, ty
                    );
                }
                ty
            } else {
                MirTypeEnum::I64
            }
        }
        MirOperand::Constant(c) => match c {
            MirConstant::Float(_) => MirTypeEnum::F64,
            MirConstant::Int(_) => MirTypeEnum::I64,
            MirConstant::UInt(_) => MirTypeEnum::U64,
            MirConstant::Bool(_) => MirTypeEnum::Bool,
            MirConstant::String(_) => MirTypeEnum::String,
            MirConstant::Null => MirTypeEnum::Unit,
        },
    }
}

fn lower_rvalue(
    dest: ValueId,
    mir_local: u32,
    rvalue: &crate::ir::mir::MirRvalue,
    mir_func: &crate::ir::mir::MirFunction,
    block: &mut VirBlock,
    ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    use crate::ir::mir::MirRvalue;

    match rvalue {
        MirRvalue::Use(operand) => {
            // Simple value use - copy/move
            let src = lower_operand(operand, block, ctx);
            // Track assignment: the MIR local now holds this VIR value
            ctx.borrow_mut().local_to_value.insert(mir_local, dest);
            block.instructions.push(VirInstruction::Copy { dest, src });
        }
        MirRvalue::Ref(_borrow_kind, place) => {
            // Take reference - load address of place
            let local = place.local;
            // In VIR, we just load the local address
            block
                .instructions
                .push(VirInstruction::LoadLocal { dest, local });
        }
        MirRvalue::Deref(place) => {
            // Dereference - load from pointer
            let ptr = place.local;
            let ptr_val = ctx.borrow_mut().next_value();
            block.instructions.push(VirInstruction::LoadLocal {
                dest: ptr_val,
                local: ptr,
            });
            // Determine type for load (default to I64 for now)
            let ty = VirType::I64;
            block.instructions.push(VirInstruction::Load {
                dest,
                ptr: ptr_val,
                ty,
            });
        }
        MirRvalue::BinaryOp(op, lhs, rhs) => {
            // Binary operation
            let lhs_val = lower_operand(lhs, block, ctx);
            let rhs_val = lower_operand(rhs, block, ctx);

            let lhs_ty = operand_type(lhs, mir_func);
            let rhs_ty = operand_type(rhs, mir_func);
            let is_float = matches!(lhs_ty, MirTypeEnum::F64 | MirTypeEnum::F32)
                || matches!(rhs_ty, MirTypeEnum::F64 | MirTypeEnum::F32);

            if is_float {
                match op {
                    MirBinOp::Eq
                    | MirBinOp::Ne
                    | MirBinOp::Lt
                    | MirBinOp::Le
                    | MirBinOp::Gt
                    | MirBinOp::Ge => {
                        let cmp_op = match op {
                            MirBinOp::Eq => CmpOp::Eq,
                            MirBinOp::Ne => CmpOp::Ne,
                            MirBinOp::Lt => CmpOp::Lt,
                            MirBinOp::Le => CmpOp::Le,
                            MirBinOp::Gt => CmpOp::Gt,
                            MirBinOp::Ge => CmpOp::Ge,
                            // Non-comparison ops can't reach here (outer match filters)
                            _ => {
                                return Err(format!(
                                    "Non-comparison operator {:?} in float comparison context",
                                    op
                                ));
                            }
                        };
                        block.instructions.push(VirInstruction::FloatCmp {
                            dest,
                            op: cmp_op,
                            lhs: lhs_val,
                            rhs: rhs_val,
                        });
                    }
                    MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul | MirBinOp::Div => {
                        let f_op = match op {
                            MirBinOp::Add => FloatBinOp::Add,
                            MirBinOp::Sub => FloatBinOp::Sub,
                            MirBinOp::Mul => FloatBinOp::Mul,
                            MirBinOp::Div => FloatBinOp::Div,
                            // Non-arithmetic ops can't reach here (outer match filters)
                            _ => {
                                return Err(format!(
                                    "Non-arithmetic operator {:?} in float arithmetic context",
                                    op
                                ));
                            }
                        };
                        let ty = if matches!(lhs_ty, MirTypeEnum::F32) {
                            VirType::F32
                        } else {
                            VirType::F64
                        };
                        block.instructions.push(VirInstruction::FloatBinOp {
                            dest,
                            op: f_op,
                            lhs: lhs_val,
                            rhs: rhs_val,
                            ty,
                        });
                    }
                    _ => {
                        lower_binop(dest, *op, lhs_val, rhs_val, block, ctx)?;
                    }
                }
            } else {
                lower_binop(dest, *op, lhs_val, rhs_val, block, ctx)?;
            }
        }
        MirRvalue::UnaryOp(op, operand) => {
            // Unary operation
            let operand_val = lower_operand(operand, block, ctx);
            lower_unop(dest, *op, operand_val, block, ctx)?;
        }
        MirRvalue::Cast(operand, target_ty) => {
            // Type cast
            let src = lower_operand(operand, block, ctx);
            let to_ty = lower_type(target_ty);
            // Determine source type (default to I64)
            let from_ty = VirType::I64;
            block.instructions.push(VirInstruction::Cast {
                dest,
                value: src,
                from_ty,
                to_ty,
            });
        }
        MirRvalue::Aggregate(kind, operands) => {
            // Aggregate construction
            let values: Vec<ValueId> = operands
                .iter()
                .map(|op| lower_operand(op, block, ctx))
                .collect();

            use crate::ir::mir::AggregateKind;
            match kind {
                AggregateKind::Tuple => {
                    block.instructions.push(VirInstruction::BuildTuple {
                        dest,
                        elements: values,
                    });
                }
                AggregateKind::Struct(name, field_names) => {
                    block.instructions.push(VirInstruction::BuildStruct {
                        dest,
                        ty: name.clone(),
                        field_names: field_names.clone(),
                        fields: values,
                    });
                }
                AggregateKind::Array(elem_ty, _size) => {
                    block.instructions.push(VirInstruction::BuildArray {
                        dest,
                        elem_ty: lower_type(elem_ty),
                        elements: values,
                    });
                }
                AggregateKind::Object(keys) => {
                    block.instructions.push(VirInstruction::BuildObject {
                        dest,
                        keys: keys.clone(),
                        values,
                    });
                }
            }
        }
    }

    // Track assignment: the MIR local now holds this VIR value
    // (Use case already updates this, but update for all other cases)
    if !matches!(rvalue, crate::ir::mir::MirRvalue::Use(_)) {
        ctx.borrow_mut().local_to_value.insert(mir_local, dest);
    }

    Ok(())
}

/// Lower function call
/// vir_dest: fresh VIR ValueId for the call result
/// mir_dest: MIR LocalId being assigned to (for tracking)
fn lower_call(
    vir_dest: ValueId,
    mir_dest: u32,
    func: &crate::ir::mir::MirOperand,
    args: &[crate::ir::mir::MirOperand],
    block: &mut VirBlock,
    ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    // Lower function operand
    let func_val = lower_operand(func, block, ctx);

    // Lower arguments
    let arg_vals: Vec<ValueId> = args
        .iter()
        .map(|arg| lower_operand(arg, block, ctx))
        .collect();

    // Generate call instruction
    block.instructions.push(VirInstruction::Call {
        dest: Some(vir_dest),
        func: func_val,
        args: arg_vals,
    });

    // Track assignment: the MIR local now holds this VIR value
    ctx.borrow_mut().local_to_value.insert(mir_dest, vir_dest);

    Ok(())
}

/// Lower MIR operand to VIR value
fn lower_operand(
    operand: &crate::ir::mir::MirOperand,
    block: &mut VirBlock,
    ctx: &RefCell<LoweringContext>,
) -> ValueId {
    use crate::ir::mir::MirConstant;
    use crate::ir::mir::MirOperand;

    match operand {
        MirOperand::Move(place) | MirOperand::Copy(place) => {
            // Look up what VIR value was assigned to this local
            ctx.borrow().get_value_for_local(place.local)
        }
        MirOperand::Constant(constant) => {
            // Emit VIR constant instruction and return the destination value ID
            let dest = ctx.borrow_mut().next_value();
            match constant {
                MirConstant::Int(value) => {
                    block.instructions.push(VirInstruction::ConstInt {
                        dest,
                        value: *value,
                        ty: VirType::I64,
                    });
                }
                MirConstant::UInt(value) => {
                    block.instructions.push(VirInstruction::ConstInt {
                        dest,
                        value: *value as i64,
                        ty: VirType::U64,
                    });
                }
                MirConstant::Float(value) => {
                    block.instructions.push(VirInstruction::ConstFloat {
                        dest,
                        value: *value,
                        ty: VirType::F64,
                    });
                }
                MirConstant::Bool(value) => {
                    block.instructions.push(VirInstruction::ConstBool {
                        dest,
                        value: *value,
                    });
                }
                MirConstant::String(value) => {
                    // Intern the string value in the pool
                    let string_id = ctx.borrow_mut().strings.intern(value);
                    block
                        .instructions
                        .push(VirInstruction::ConstString { dest, string_id });
                }
                MirConstant::Null => {
                    block.instructions.push(VirInstruction::ConstNull { dest });
                }
            }
            dest
        }
    }
}

/// Lower binary operation
fn lower_binop(
    dest: ValueId,
    op: MirBinOp,
    lhs: ValueId,
    rhs: ValueId,
    block: &mut VirBlock,
    _ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    use MirBinOp::*;

    match op {
        // Integer arithmetic
        Add => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Add,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        Sub => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Sub,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        Mul => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Mul,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        Div => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Div,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        Rem => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Rem,
            lhs,
            rhs,
            ty: VirType::I64,
        }),

        // Bitwise operations
        BitAnd => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::And,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        BitOr => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Or,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        BitXor => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Xor,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        Shl => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Shl,
            lhs,
            rhs,
            ty: VirType::I64,
        }),
        Shr => block.instructions.push(VirInstruction::IntBinOp {
            dest,
            op: IntBinOp::Shr,
            lhs,
            rhs,
            ty: VirType::I64,
        }),

        // Comparisons
        Eq => block.instructions.push(VirInstruction::IntCmp {
            dest,
            op: CmpOp::Eq,
            lhs,
            rhs,
        }),
        Ne => block.instructions.push(VirInstruction::IntCmp {
            dest,
            op: CmpOp::Ne,
            lhs,
            rhs,
        }),
        Lt => block.instructions.push(VirInstruction::IntCmp {
            dest,
            op: CmpOp::Lt,
            lhs,
            rhs,
        }),
        Le => block.instructions.push(VirInstruction::IntCmp {
            dest,
            op: CmpOp::Le,
            lhs,
            rhs,
        }),
        Gt => block.instructions.push(VirInstruction::IntCmp {
            dest,
            op: CmpOp::Gt,
            lhs,
            rhs,
        }),
        Ge => block.instructions.push(VirInstruction::IntCmp {
            dest,
            op: CmpOp::Ge,
            lhs,
            rhs,
        }),

        // Logical operations (convert to comparisons)
        And | Or => {
            // These should be handled as control flow in MIR
            return Err("Logical And/Or should not appear in MIR rvalues".to_string());
        }
    }

    Ok(())
}

/// Lower unary operation
fn lower_unop(
    dest: ValueId,
    op: MirUnOp,
    operand: ValueId,
    block: &mut VirBlock,
    _ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    use MirUnOp::*;

    match op {
        Not => block.instructions.push(VirInstruction::IntUnOp {
            dest,
            op: IntUnOp::Not,
            operand,
            ty: VirType::I64,
        }),
        Neg => block.instructions.push(VirInstruction::IntUnOp {
            dest,
            op: IntUnOp::Neg,
            operand,
            ty: VirType::I64,
        }),
    }

    Ok(())
}

fn lower_type(mir_type: &crate::ir::mir::MirType) -> VirType {
    use crate::ir::mir::MirType;

    match mir_type {
        MirType::Unit => VirType::Void,
        MirType::Bool => VirType::Bool,
        MirType::I8 => VirType::I8,
        MirType::I16 => VirType::I16,
        MirType::I32 => VirType::I32,
        MirType::I64 => VirType::I64,
        MirType::I128 => VirType::I128,
        MirType::U8 => VirType::U8,
        MirType::U16 => VirType::U16,
        MirType::U32 => VirType::U32,
        MirType::U64 => VirType::U64,
        MirType::U128 => VirType::U128,
        MirType::F32 => VirType::F32,
        MirType::F64 => VirType::F64,
        MirType::String => VirType::Struct("String".to_string()),
        MirType::Ref { inner, .. } => {
            // References become pointers in VIR
            VirType::TypedPtr(Box::new(lower_type(inner)))
        }
        MirType::Ptr(inner) => VirType::TypedPtr(Box::new(lower_type(inner))),
        MirType::Tuple(types) => VirType::Tuple(types.iter().map(lower_type).collect()),
        MirType::Array(elem, size) => VirType::Array {
            elem: Box::new(lower_type(elem)),
            size: *size,
        },
        MirType::Slice(_) => {
            // Slices become struct { ptr, len }
            VirType::Struct("Slice".to_string())
        }
        MirType::Struct(name) => VirType::Struct(name.clone()),
        MirType::Enum(name) => VirType::Enum(name.clone()),
        MirType::Function { params, ret } => VirType::FuncPtr {
            params: params.iter().map(lower_type).collect(),
            ret: Box::new(lower_type(ret)),
        },
        MirType::Arc(_) => {
            // ARC becomes a pointer in VIR (reference counting is explicit ops)
            VirType::Ptr
        }
        MirType::Weak(_) => {
            // Weak becomes a pointer
            VirType::Ptr
        }
        MirType::Never => VirType::Void,
    }
}

/// Build CFG information by analyzing terminators
fn build_cfg_info(mir_func: &crate::ir::mir::MirFunction, ctx: &RefCell<LoweringContext>) {
    use crate::ir::mir::MirTerminator;

    for block in &mir_func.body {
        match &block.terminator {
            MirTerminator::Goto(target) => {
                ctx.borrow_mut().add_predecessor(*target, block.id);
            }
            MirTerminator::SwitchInt {
                targets, otherwise, ..
            } => {
                // Add predecessors for all switch targets
                for (_, target) in targets {
                    ctx.borrow_mut().add_predecessor(*target, block.id);
                }
                ctx.borrow_mut().add_predecessor(*otherwise, block.id);
            }
            _ => {}
        }
    }
}

/// Insert PHI nodes at blocks with multiple predecessors
fn insert_phi_nodes(
    vir_func: &mut VirFunction,
    _mir_func: &crate::ir::mir::MirFunction,
    ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    let predecessors = ctx.borrow().predecessors.clone();
    let block_local_values = ctx.borrow().block_local_values.clone();
    let phi_placeholders = ctx.borrow().phi_placeholders.clone();

    for (block_id, local_id, phi_dest) in phi_placeholders {
        if let Some(block) = vir_func.blocks.iter_mut().find(|b| b.id == block_id) {
            if let Some(phi) = block.phis.iter_mut().find(|p| p.dest == phi_dest) {
                if let Some(preds) = predecessors.get(&block_id) {
                    for &pred_id in preds {
                        let incoming_val = if let Some(mappings) = block_local_values.get(&pred_id)
                        {
                            *mappings.get(&local_id).unwrap_or(&local_id)
                        } else {
                            local_id
                        };
                        phi.incoming.push((pred_id, incoming_val));
                    }
                }
            }
        }
    }

    Ok(())
}

fn lower_typedef(mir_typedef: &crate::ir::mir::MirTypeDef) -> super::VirTypeDef {
    use crate::ir::mir::MirTypeDefKind;

    let kind = match &mir_typedef.kind {
        MirTypeDefKind::Struct { fields } => super::VirTypeDefKind::Struct {
            fields: fields
                .iter()
                .map(|(name, ty)| (name.clone(), lower_type(ty)))
                .collect(),
        },
        MirTypeDefKind::Enum { variants } => super::VirTypeDefKind::Enum {
            variants: variants
                .iter()
                .map(|(name, tys)| (name.clone(), tys.iter().map(lower_type).collect()))
                .collect(),
        },
        MirTypeDefKind::Alias(ty) => {
            // Aliases are resolved, so we create a struct wrapper
            super::VirTypeDefKind::Struct {
                fields: vec![("inner".to_string(), lower_type(ty))],
            }
        }
    };

    super::VirTypeDef {
        name: mir_typedef.name.clone(),
        kind,
    }
}
