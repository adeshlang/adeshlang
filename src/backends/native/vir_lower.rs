//! VIR to Machine IR Lowering Pass
//!
//! Lowers backend-neutral SSA-form VIR (`VirModule`) directly into `NativeModule`
//! (`MachineFunction`, `MachineBlock`, and `MachineInstruction`).
//!
//! Connects the middle-end pipeline:
//!   AST -> HIR -> MIR (CFG, Drops, Borrows) -> VIR (SSA, PHI, Optimizations) -> Machine IR -> ADOB -> Linker
//!
//! Replaces the monolithic bypass in `lower.rs` with clean, modular, machine-independent lowering.

use std::collections::HashMap;

use adesh_codegen::calling_convention::{
    CallingConvention, SystemVX64CallingConvention, WindowsX64CallingConvention,
    resolve_call_arguments,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_object::{OperatingSystem, TargetDescriptor};

use crate::ir::vir::{
    BlockId, CmpOp, FloatBinOp, FloatUnOp, IntBinOp, IntUnOp, Intrinsic, LocalId, ValueId,
    VirFunction, VirInstruction, VirModule, VirTerminator, VirType,
};

/// Lowers a VIR module into a target-neutral NativeModule of Machine IR.
pub fn vir_to_machine_ir(
    vir: &VirModule,
    target: &TargetDescriptor,
) -> Result<NativeModule, String> {
    let mut module = NativeModule::new(&vir.name);

    // Pre-populate string pool from VIR string table
    let mut string_id_map: HashMap<u32, usize> = HashMap::new();
    let mut sorted_strings: Vec<_> = vir.strings.iter().collect();
    sorted_strings.sort_by_key(|&(id, _)| *id);
    for (&id, s) in sorted_strings {
        let idx = module.add_string(s);
        string_id_map.insert(id, idx);
    }

    let call_conv: Box<dyn CallingConvention> = match target.operating_system {
        OperatingSystem::Windows => Box::new(WindowsX64CallingConvention),
        _ => Box::new(SystemVX64CallingConvention),
    };

    let all_func_names: Vec<String> = vir.functions.iter().map(|f| f.name.clone()).collect();

    for vir_func in &vir.functions {
        let machine_func = lower_vir_function(
            vir_func,
            &mut module,
            call_conv.as_ref(),
            &string_id_map,
            &vir.strings,
            &all_func_names,
        )?;
        module.add_function(machine_func);
    }

    Ok(module)
}

struct VirLoweringContext<'a> {
    func: MachineFunction,
    current_block_id: u32,
    val_map: HashMap<ValueId, VirtualRegister>,
    module: &'a mut NativeModule,
    call_conv: &'a dyn CallingConvention,
    string_id_map: &'a HashMap<u32, usize>,
    vir_strings: &'a HashMap<u32, String>,
    string_val_map: HashMap<ValueId, String>,
    local_slots: HashMap<LocalId, i32>,
    block_labels: HashMap<BlockId, String>,
    all_func_names: &'a [String],
}

impl<'a> VirLoweringContext<'a> {
    fn new(
        name: &str,
        module: &'a mut NativeModule,
        call_conv: &'a dyn CallingConvention,
        string_id_map: &'a HashMap<u32, usize>,
        vir_strings: &'a HashMap<u32, String>,
        all_func_names: &'a [String],
    ) -> Self {
        let mut func = MachineFunction::new(name);
        func.is_exported = true;
        Self {
            func,
            current_block_id: 0,
            val_map: HashMap::new(),
            module,
            call_conv,
            string_id_map,
            vir_strings,
            string_val_map: HashMap::new(),
            local_slots: HashMap::new(),
            block_labels: HashMap::new(),
            all_func_names,
        }
    }

    fn get_local_slot(&mut self, local: LocalId) -> i32 {
        if let Some(&slot) = self.local_slots.get(&local) {
            slot
        } else {
            let offset = -((self.local_slots.len() as i32 + 1) * 8);
            self.local_slots.insert(local, offset);
            offset
        }
    }

    fn emit(&mut self, inst: MachineInstruction) {
        if let Some(block) = self
            .func
            .blocks
            .iter_mut()
            .find(|b| b.id == self.current_block_id)
        {
            block.push(inst);
        }
    }

    fn set_current_block(&mut self, block_id: u32) {
        self.current_block_id = block_id;
    }

    fn emit_runtime_call(&mut self, symbol: &str, args: &[VirtualRegister]) -> VirtualRegister {
        if !self.module.functions.iter().any(|f| f.name == symbol)
            && !self.module.imports.contains(&symbol.to_string())
        {
            self.module.imports.push(symbol.to_string());
        }

        let typed_args: Vec<(VirtualRegister, RegisterClass)> =
            args.iter().map(|&v| (v, self.func.vreg_class(v))).collect();

        let (moves, total) =
            resolve_call_arguments(self.call_conv, &typed_args, PhysicalRegister(4))
                .expect("call argument resolution");

        if total > 0 {
            self.emit(MachineInstruction::Sub {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))),
                src: MachineOperand::Immediate(total as i64),
            });
        }

        for inst in moves {
            self.emit(inst);
        }

        self.emit(MachineInstruction::Call {
            target: MachineOperand::Symbol(symbol.to_string()),
            num_args: args.len(),
        });

        if total > 0 {
            self.emit(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))),
                src: MachineOperand::Immediate(total as i64),
            });
        }

        let ret_vreg = self.func.alloc_vreg();
        self.emit(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(ret_vreg)),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        });
        ret_vreg
    }
}

fn lower_vir_function(
    vir_func: &VirFunction,
    module: &mut NativeModule,
    call_conv: &dyn CallingConvention,
    string_id_map: &HashMap<u32, usize>,
    vir_strings: &HashMap<u32, String>,
    all_func_names: &[String],
) -> Result<MachineFunction, String> {
    let mut ctx = VirLoweringContext::new(
        &vir_func.name,
        module,
        call_conv,
        string_id_map,
        vir_strings,
        all_func_names,
    );

    // Map VIR BlockId to MachineBlock ID in `func` and populate block_labels
    let mut block_map: HashMap<BlockId, u32> = HashMap::new();
    let mut block_labels: HashMap<BlockId, String> = HashMap::new();
    for (i, vblock) in vir_func.blocks.iter().enumerate() {
        let lbl = vblock
            .label
            .clone()
            .unwrap_or_else(|| format!("{}_bb{}", vir_func.name, vblock.id));
        let block_id = if i == 0 {
            if let Some(b0) = ctx.func.blocks.get_mut(0) {
                b0.label = lbl.clone();
            }
            0
        } else {
            ctx.func.create_block(&lbl)
        };
        block_map.insert(vblock.id, block_id);
        block_labels.insert(vblock.id, lbl);
    }
    ctx.block_labels = block_labels;

    // 1. Function parameter setup in entry block
    let param_classes: Vec<RegisterClass> = vir_func
        .params
        .iter()
        .map(|p| match p.ty {
            VirType::F32 | VirType::F64 => RegisterClass::Float,
            _ => RegisterClass::Gpr,
        })
        .collect();

    let incoming_args = call_conv.classify_incoming_args(&param_classes);
    let entry_id = block_map.get(&0).copied().unwrap_or(0);
    ctx.set_current_block(entry_id);

    for (i, param) in vir_func.params.iter().enumerate() {
        let vreg = match param.ty {
            VirType::F32 | VirType::F64 => ctx.func.alloc_fp_vreg(),
            _ => ctx.func.alloc_vreg(),
        };
        // Parameters in VIR start at ValueId 0 .. N
        let param_val_id = i as u32;
        ctx.val_map.insert(param_val_id, vreg);

        if let Some(loc) = incoming_args.get(i) {
            match loc {
                adesh_codegen::abi::ArgumentLocation::Register(p) => {
                    ctx.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                        src: MachineOperand::Register(MachineRegister::Physical(*p)),
                    });
                }
                adesh_codegen::abi::ArgumentLocation::Stack(stack_arg) => {
                    ctx.emit(MachineInstruction::Load {
                        dst: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                        src: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                            offset: stack_arg.offset,
                            index: None,
                        },
                        size: 8,
                    });
                }
                _ => {}
            }
        }
    }

    // 2. Pre-allocate VirtualRegisters for all Phi node destinations
    for vblock in &vir_func.blocks {
        for phi in &vblock.phis {
            let vreg = match phi.ty {
                VirType::F32 | VirType::F64 => ctx.func.alloc_fp_vreg(),
                _ => ctx.func.alloc_vreg(),
            };
            ctx.val_map.insert(phi.dest, vreg);
        }
    }

    // 3. Lower each basic block
    for vblock in &vir_func.blocks {
        let m_block_id = *block_map
            .get(&vblock.id)
            .ok_or_else(|| format!("Unknown VIR block ID {}", vblock.id))?;
        ctx.set_current_block(m_block_id);

        for inst in &vblock.instructions {
            lower_vir_instruction(inst, &mut ctx)?;
        }

        // Lower Terminator
        lower_vir_terminator(
            &vblock.terminator,
            &mut ctx,
            &block_map,
            &vir_func.return_type,
        )?;
    }

    // 4. Resolve Phi nodes: insert parallel moves into predecessor blocks
    for vblock in &vir_func.blocks {
        for phi in &vblock.phis {
            let phi_dest_vreg = ctx.val_map[&phi.dest];
            for &(pred_block_id, incoming_val_id) in &phi.incoming {
                if let Some(&pred_m_id) = block_map.get(&pred_block_id) {
                    if let Some(&src_vreg) = ctx.val_map.get(&incoming_val_id) {
                        ctx.set_current_block(pred_m_id);
                        let mblock = &mut ctx.func.blocks[pred_m_id as usize];
                        let insert_pos = if mblock.instructions.is_empty() {
                            0
                        } else {
                            mblock.instructions.len() - 1
                        };
                        mblock.instructions.insert(
                            insert_pos,
                            MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(
                                    phi_dest_vreg,
                                )),
                                src: MachineOperand::Register(MachineRegister::Virtual(src_vreg)),
                            },
                        );
                    }
                }
            }
        }
    }

    ctx.func.rebuild_cfg();
    Ok(ctx.func)
}

fn lower_vir_instruction(
    inst: &VirInstruction,
    ctx: &mut VirLoweringContext,
) -> Result<(), String> {
    match inst {
        VirInstruction::ConstInt { dest, value, .. } => {
            let v = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                src: MachineOperand::Immediate(*value),
            });
            ctx.val_map.insert(*dest, v);
        }
        VirInstruction::ConstFloat { dest, value, .. } => {
            let v = ctx.func.alloc_fp_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                src: MachineOperand::FloatImmediate(*value),
            });
            ctx.val_map.insert(*dest, v);
        }
        VirInstruction::ConstBool { dest, value } => {
            let v = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                src: MachineOperand::Immediate(if *value { 1 } else { 0 }),
            });
            ctx.val_map.insert(*dest, v);
        }
        VirInstruction::ConstString { dest, string_id } => {
            let pool_idx = ctx.string_id_map.get(string_id).copied().unwrap_or(0);
            if let Some(s) = ctx.vir_strings.get(string_id) {
                ctx.string_val_map.insert(*dest, s.clone());
            }
            let v = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                src: MachineOperand::Symbol(format!("__str_{}", pool_idx)),
            });
            ctx.val_map.insert(*dest, v);
        }
        VirInstruction::ConstNull { dest } => {
            let v = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                src: MachineOperand::Immediate(0),
            });
            ctx.val_map.insert(*dest, v);
        }
        VirInstruction::IntBinOp {
            dest, op, lhs, rhs, ..
        } => {
            let lhs_v = ctx.val_map[lhs];
            let rhs_v = ctx.val_map[rhs];
            let out_v = ctx.func.alloc_vreg();

            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                src: MachineOperand::Register(MachineRegister::Virtual(lhs_v)),
            });

            let rhs_op = MachineOperand::Register(MachineRegister::Virtual(rhs_v));
            let out_op = MachineOperand::Register(MachineRegister::Virtual(out_v));

            match op {
                IntBinOp::Add => ctx.emit(MachineInstruction::Add {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Sub => ctx.emit(MachineInstruction::Sub {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Mul => ctx.emit(MachineInstruction::Mul {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Div => ctx.emit(MachineInstruction::Div {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Rem => ctx.emit(MachineInstruction::Mod {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::And => ctx.emit(MachineInstruction::And {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Or => ctx.emit(MachineInstruction::Or {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Xor => ctx.emit(MachineInstruction::Xor {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Shl => ctx.emit(MachineInstruction::Shl {
                    dst: out_op,
                    src: rhs_op,
                }),
                IntBinOp::Shr => ctx.emit(MachineInstruction::Sar {
                    dst: out_op,
                    src: rhs_op,
                }),
            }
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::FloatBinOp {
            dest, op, lhs, rhs, ..
        } => {
            let lhs_v = ctx.val_map[lhs];
            let rhs_v = ctx.val_map[rhs];
            let out_v = ctx.func.alloc_fp_vreg();

            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                src: MachineOperand::Register(MachineRegister::Virtual(lhs_v)),
            });

            let rhs_op = MachineOperand::Register(MachineRegister::Virtual(rhs_v));
            let out_op = MachineOperand::Register(MachineRegister::Virtual(out_v));

            match op {
                FloatBinOp::Add => ctx.emit(MachineInstruction::FAdd {
                    dst: out_op,
                    src: rhs_op,
                    size: 8,
                }),
                FloatBinOp::Sub => ctx.emit(MachineInstruction::FSub {
                    dst: out_op,
                    src: rhs_op,
                    size: 8,
                }),
                FloatBinOp::Mul => ctx.emit(MachineInstruction::FMul {
                    dst: out_op,
                    src: rhs_op,
                    size: 8,
                }),
                FloatBinOp::Div => ctx.emit(MachineInstruction::FDiv {
                    dst: out_op,
                    src: rhs_op,
                    size: 8,
                }),
            }
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::IntUnOp {
            dest, op, operand, ..
        } => {
            let op_v = ctx.val_map[operand];
            let out_v = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                src: MachineOperand::Register(MachineRegister::Virtual(op_v)),
            });
            match op {
                IntUnOp::Neg => ctx.emit(MachineInstruction::Neg {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                }),
                IntUnOp::Not => ctx.emit(MachineInstruction::Not {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                }),
            }
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::FloatUnOp {
            dest, op, operand, ..
        } => {
            let op_v = ctx.val_map[operand];
            let out_v = ctx.func.alloc_fp_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                src: MachineOperand::Register(MachineRegister::Virtual(op_v)),
            });
            match op {
                FloatUnOp::Neg => ctx.emit(MachineInstruction::FNeg {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                    size: 8,
                }),
                FloatUnOp::Abs => ctx.emit(MachineInstruction::Custom {
                    name: "fabs".into(),
                    operands: vec![MachineOperand::Register(MachineRegister::Virtual(out_v))],
                }),
                FloatUnOp::Sqrt => ctx.emit(MachineInstruction::Custom {
                    name: "fsqrt".into(),
                    operands: vec![MachineOperand::Register(MachineRegister::Virtual(out_v))],
                }),
            }
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::IntCmp { dest, op, lhs, rhs } => {
            let lhs_v = ctx.val_map[lhs];
            let rhs_v = ctx.val_map[rhs];
            ctx.emit(MachineInstruction::Compare {
                lhs: MachineOperand::Register(MachineRegister::Virtual(lhs_v)),
                rhs: MachineOperand::Register(MachineRegister::Virtual(rhs_v)),
            });
            let out_v = ctx.func.alloc_vreg();
            let cc = match op {
                CmpOp::Eq => ConditionCode::Equal,
                CmpOp::Ne => ConditionCode::NotEqual,
                CmpOp::Lt => ConditionCode::LessThan,
                CmpOp::Le => ConditionCode::LessOrEqual,
                CmpOp::Gt => ConditionCode::GreaterThan,
                CmpOp::Ge => ConditionCode::GreaterOrEqual,
            };
            ctx.emit(MachineInstruction::SetCc {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                cc,
            });
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::FloatCmp { dest, op, lhs, rhs } => {
            let lhs_v = ctx.val_map[lhs];
            let rhs_v = ctx.val_map[rhs];
            ctx.emit(MachineInstruction::FCmp {
                lhs: MachineOperand::Register(MachineRegister::Virtual(lhs_v)),
                rhs: MachineOperand::Register(MachineRegister::Virtual(rhs_v)),
                size: 8,
            });
            let out_v = ctx.func.alloc_vreg();
            let cc = match op {
                CmpOp::Eq => ConditionCode::Equal,
                CmpOp::Ne => ConditionCode::NotEqual,
                CmpOp::Lt => ConditionCode::LessThan,
                CmpOp::Le => ConditionCode::LessOrEqual,
                CmpOp::Gt => ConditionCode::GreaterThan,
                CmpOp::Ge => ConditionCode::GreaterOrEqual,
            };
            ctx.emit(MachineInstruction::SetCc {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                cc,
            });
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::Load { dest, ptr, ty } => {
            let ptr_v = ctx.val_map[ptr];
            let out_v = match ty {
                VirType::F32 | VirType::F64 => ctx.func.alloc_fp_vreg(),
                _ => ctx.func.alloc_vreg(),
            };
            ctx.emit(MachineInstruction::Load {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                src: MachineOperand::Memory {
                    base: MachineRegister::Virtual(ptr_v),
                    offset: 0,
                    index: None,
                },
                size: 8,
            });
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::Store { ptr, value } => {
            let ptr_v = ctx.val_map[ptr];
            let val_v = ctx.val_map[value];
            ctx.emit(MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Virtual(ptr_v),
                    offset: 0,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Virtual(val_v)),
                size: 8,
            });
        }
        VirInstruction::Copy { dest, src } | VirInstruction::Move { dest, src } => {
            let src_v = ctx.val_map[src];
            let out_v = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
            });
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::Drop { value } => {
            let v = ctx.val_map[value];
            ctx.emit_runtime_call("aot_release_handle", &[v]);
        }
        VirInstruction::ArcIncrement { ptr } => {
            let p = ctx.val_map[ptr];
            ctx.emit_runtime_call("adesh_rt_arc_clone", &[p]);
        }
        VirInstruction::ArcDecrement { ptr } | VirInstruction::ArcDrop { ptr } => {
            let p = ctx.val_map[ptr];
            ctx.emit_runtime_call("adesh_rt_arc_drop", &[p]);
        }
        VirInstruction::Alloc { dest, size, .. } => {
            let sz_v = ctx.val_map[size];
            let out_v = ctx.emit_runtime_call("adesh_mem_alloc", &[sz_v]);
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::Free { ptr } => {
            let p_v = ctx.val_map[ptr];
            ctx.emit_runtime_call("adesh_mem_free", &[p_v]);
        }
        VirInstruction::BuildObject { dest, keys, values } => {
            let zero_reg = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(zero_reg)),
                src: MachineOperand::Immediate(0),
            });
            let obj_handle = ctx.emit_runtime_call("aot_make_object", &[zero_reg, zero_reg]);

            for (key, val_id) in keys.iter().zip(values.iter()) {
                let str_idx = ctx.module.add_string(key);
                let str_reg = ctx.func.alloc_vreg();
                ctx.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                    src: MachineOperand::Symbol(format!("__str_{}", str_idx)),
                });
                let val_v = ctx.val_map[val_id];
                ctx.emit_runtime_call("aot_set_field_str", &[obj_handle, str_reg, val_v]);
            }
            ctx.val_map.insert(*dest, obj_handle);
        }
        VirInstruction::ExtractField {
            dest,
            struct_val,
            field,
        } => {
            let obj_v = ctx.val_map[struct_val];
            let field_str = format!("field_{}", field);
            let str_idx = ctx.module.add_string(&field_str);
            let str_reg = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                src: MachineOperand::Symbol(format!("__str_{}", str_idx)),
            });
            let out_v = ctx.emit_runtime_call("aot_get_field_str", &[obj_v, str_reg]);
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::InsertField {
            dest,
            struct_val,
            field,
            value,
        } => {
            let obj_v = ctx.val_map[struct_val];
            let val_v = ctx.val_map[value];
            let field_str = format!("field_{}", field);
            let str_idx = ctx.module.add_string(&field_str);
            let str_reg = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                src: MachineOperand::Symbol(format!("__str_{}", str_idx)),
            });
            let out_v = ctx.emit_runtime_call("aot_set_field_str", &[obj_v, str_reg, val_v]);
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::Intrinsic {
            dest,
            intrinsic,
            args,
        } => {
            let arg_vregs: Vec<VirtualRegister> = args.iter().map(|a| ctx.val_map[a]).collect();
            let ret = match intrinsic {
                Intrinsic::MemCopy => ctx.emit_runtime_call("memcpy", &arg_vregs),
                Intrinsic::MemSet => ctx.emit_runtime_call("memset", &arg_vregs),
                _ => ctx.func.alloc_vreg(),
            };
            if let Some(d) = dest {
                ctx.val_map.insert(*d, ret);
            }
        }
        VirInstruction::LoadLocal { dest, local } => {
            let slot = ctx.get_local_slot(*local);
            let out_v = ctx.func.alloc_vreg();
            ctx.emit(MachineInstruction::Load {
                dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                src: MachineOperand::Memory {
                    base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                    offset: slot,
                    index: None,
                },
                size: 8,
            });
            ctx.val_map.insert(*dest, out_v);
        }
        VirInstruction::StoreLocal { local, value } => {
            let val_v = ctx.val_map[value];
            let slot = ctx.get_local_slot(*local);
            ctx.emit(MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                    offset: slot,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Virtual(val_v)),
                size: 8,
            });
        }
        VirInstruction::Call { dest, func, args } => {
            let sym_name = ctx.string_val_map.get(func).cloned();
            let is_external = if let Some(ref sym) = sym_name {
                !ctx.all_func_names.iter().any(|f| f == sym)
            } else {
                false
            };

            if is_external {
                if let Some(ref sym) = sym_name {
                    if !ctx.module.imports.contains(sym) {
                        ctx.module.imports.push(sym.clone());
                    }
                }
            }

            let arg_vregs: Vec<VirtualRegister> = args.iter().map(|a| ctx.val_map[a]).collect();
            let typed_args: Vec<(VirtualRegister, RegisterClass)> = arg_vregs
                .iter()
                .map(|&v| (v, ctx.func.vreg_class(v)))
                .collect();

            let (moves, total) =
                resolve_call_arguments(ctx.call_conv, &typed_args, PhysicalRegister(4))
                    .expect("call argument resolution");

            if total > 0 {
                ctx.emit(MachineInstruction::Sub {
                    dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))),
                    src: MachineOperand::Immediate(total as i64),
                });
            }

            for inst in moves {
                ctx.emit(inst);
            }

            let call_target = if let Some(ref sym) = sym_name {
                MachineOperand::Symbol(sym.clone())
            } else if let Some(&fn_vreg) = ctx.val_map.get(func) {
                MachineOperand::Register(MachineRegister::Virtual(fn_vreg))
            } else {
                MachineOperand::Symbol(format!("func_{}", func))
            };

            ctx.emit(MachineInstruction::Call {
                target: call_target,
                num_args: args.len(),
            });

            if total > 0 {
                ctx.emit(MachineInstruction::Add {
                    dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))),
                    src: MachineOperand::Immediate(total as i64),
                });
            }

            if let Some(d) = dest {
                let out_v = ctx.func.alloc_vreg();
                ctx.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_v)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                ctx.val_map.insert(*d, out_v);
            }
        }
        VirInstruction::Nop => {
            ctx.emit(MachineInstruction::Nop);
        }
        _ => {}
    }
    Ok(())
}

fn lower_vir_terminator(
    term: &VirTerminator,
    ctx: &mut VirLoweringContext,
    _block_map: &HashMap<BlockId, u32>,
    ret_ty: &VirType,
) -> Result<(), String> {
    match term {
        VirTerminator::Return { value } => {
            if let Some(val_id) = value {
                if let Some(&vreg) = ctx.val_map.get(val_id) {
                    match ret_ty {
                        VirType::F32 | VirType::F64 => {
                            ctx.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister::xmm(0),
                                )),
                                src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                            });
                        }
                        _ => {
                            ctx.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )), // RAX
                                src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                            });
                        }
                    }
                }
            }
            ctx.emit(MachineInstruction::Return);
        }
        VirTerminator::Jump { target } => {
            let target_lbl = ctx
                .block_labels
                .get(target)
                .cloned()
                .unwrap_or_else(|| format!("{}_bb{}", ctx.func.name, target));
            ctx.emit(MachineInstruction::Branch { target: target_lbl });
        }
        VirTerminator::Branch {
            cond,
            true_target,
            false_target,
        } => {
            let cond_v = ctx.val_map[cond];
            ctx.emit(MachineInstruction::Compare {
                lhs: MachineOperand::Register(MachineRegister::Virtual(cond_v)),
                rhs: MachineOperand::Immediate(0),
            });
            let true_lbl = ctx
                .block_labels
                .get(true_target)
                .cloned()
                .unwrap_or_else(|| format!("{}_bb{}", ctx.func.name, true_target));
            let false_lbl = ctx
                .block_labels
                .get(false_target)
                .cloned()
                .unwrap_or_else(|| format!("{}_bb{}", ctx.func.name, false_target));
            ctx.emit(MachineInstruction::BranchCc {
                cc: ConditionCode::NotEqual,
                target: true_lbl,
            });
            ctx.emit(MachineInstruction::Branch { target: false_lbl });
        }
        VirTerminator::Switch {
            value,
            cases,
            default,
        } => {
            let val_v = ctx.val_map[value];
            for (case_val, target_bb) in cases {
                ctx.emit(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(val_v)),
                    rhs: MachineOperand::Immediate(*case_val),
                });
                let target_lbl = ctx
                    .block_labels
                    .get(target_bb)
                    .cloned()
                    .unwrap_or_else(|| format!("{}_bb{}", ctx.func.name, target_bb));
                ctx.emit(MachineInstruction::BranchCc {
                    cc: ConditionCode::Equal,
                    target: target_lbl,
                });
            }
            let default_lbl = ctx
                .block_labels
                .get(default)
                .cloned()
                .unwrap_or_else(|| format!("{}_bb{}", ctx.func.name, default));
            ctx.emit(MachineInstruction::Branch {
                target: default_lbl,
            });
        }
        VirTerminator::Unreachable => {
            ctx.emit(MachineInstruction::Nop);
        }
    }
    Ok(())
}
