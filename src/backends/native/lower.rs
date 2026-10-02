//! Native HIR to Machine IR Lowering Engine.
//!
//! Transforms typed Adesh HIR programs directly into `adesh_codegen::machine_ir::NativeModule`
//! with physical/virtual registers, stack frame allocation, C ABI runtime bindings,
//! string pool `.rodata` emission, composite data construction (objects, arrays, tuples, sets),
//! rich pretty-printing options, and branch/call relocation generation.

use crate::parsing::hir::{
    BinOp, HirExpr, HirFunction, HirLiteral, HirModule, HirStmt, HirType, UnaryOp,
};
use adesh_codegen::calling_convention::{
    CallingConvention, SystemVX64CallingConvention, WindowsX64CallingConvention,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, VirtualRegister,
};
use adesh_object::{OperatingSystem, TargetDescriptor};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct LoopContext {
    pub start_label: String,
    pub step_label: String,
    pub end_label: String,
    pub idx_slot: Option<i32>,
}

/// Lowering context for a single function.
pub struct FunctionLoweringContext<'a> {
    pub func: &'a mut MachineFunction,
    pub module: &'a mut NativeModule,
    pub target: &'a TargetDescriptor,
    pub call_conv: Box<dyn CallingConvention>,
    pub local_vars: HashMap<String, (i32, Option<HirType>)>, // (stack_offset, type)
    pub loop_stack: Vec<LoopContext>,
    pub current_stack_offset: i32,
    pub current_block_id: u32,
    pub label_counter: u32,
}

fn infer_hir_expr_type(expr: &HirExpr) -> Option<HirType> {
    match expr {
        HirExpr::Literal(lit) => match lit {
            HirLiteral::Int(_) => Some(HirType::Int),
            HirLiteral::Float(_) | HirLiteral::F64(_) => Some(HirType::Float),
            HirLiteral::F32(_) => Some(HirType::F32),
            HirLiteral::Bool(_) => Some(HirType::Bool),
            HirLiteral::Char(_) => Some(HirType::Char),
            HirLiteral::String(_) => Some(HirType::String),
            HirLiteral::Null => Some(HirType::Null),
            HirLiteral::U8(_) => Some(HirType::U8),
            HirLiteral::U16(_) => Some(HirType::U16),
            HirLiteral::U32(_) => Some(HirType::U32),
            HirLiteral::U64(_) => Some(HirType::U64),
            HirLiteral::I8(_) => Some(HirType::I8),
            HirLiteral::I16(_) => Some(HirType::I16),
            HirLiteral::I32(_) => Some(HirType::I32),
            HirLiteral::I64(_) => Some(HirType::I64),
            HirLiteral::BigInt(_) => Some(HirType::U64),
            _ => None,
        },
        HirExpr::ArrayLiteral(_) => Some(HirType::Array(
            Box::new(HirType::Any),
            crate::ir::hir::ArrayKind::Dynamic,
        )),
        HirExpr::ObjectLiteral(_) => Some(HirType::Object),
        HirExpr::TupleLiteral(elems) => Some(HirType::Tuple(vec![HirType::Any; elems.len()])),
        HirExpr::SetLiteral(_) => Some(HirType::Set(Box::new(HirType::Any))),
        HirExpr::Call(callee, _, _) => {
            if let HirExpr::LoadVar(fn_name) = &**callee {
                if matches!(fn_name.as_str(), "int" | "i64" | "sizeof" | "sizeOf") {
                    return Some(HirType::Int);
                }
                if matches!(fn_name.as_str(), "i32") {
                    return Some(HirType::I32);
                }
                if matches!(fn_name.as_str(), "i16") {
                    return Some(HirType::I16);
                }
                if matches!(fn_name.as_str(), "i8") {
                    return Some(HirType::I8);
                }
                if matches!(fn_name.as_str(), "u64" | "BigInt" | "bigint") {
                    return Some(HirType::U64);
                }
                if matches!(fn_name.as_str(), "u32") {
                    return Some(HirType::U32);
                }
                if matches!(fn_name.as_str(), "u16") {
                    return Some(HirType::U16);
                }
                if matches!(fn_name.as_str(), "u8") {
                    return Some(HirType::U8);
                }
                if matches!(
                    fn_name.as_str(),
                    "float" | "f64" | "number" | "Number" | "clock"
                ) {
                    return Some(HirType::Float);
                }
                if matches!(fn_name.as_str(), "f32") {
                    return Some(HirType::F32);
                }
                if matches!(fn_name.as_str(), "bool" | "Boolean") {
                    return Some(HirType::Bool);
                }
                if matches!(
                    fn_name.as_str(),
                    "str" | "string" | "String" | "typeof" | "type" | "typeOf" | "input"
                ) {
                    return Some(HirType::String);
                }
            }
            None
        }
        _ => None,
    }
}

fn eval_const_expr(expr: &HirExpr) -> Option<i64> {
    match expr {
        HirExpr::Literal(HirLiteral::Int(n)) => Some(*n),
        HirExpr::Literal(HirLiteral::I64(n)) => Some(*n),
        HirExpr::Literal(HirLiteral::I32(n)) => Some(*n as i64),
        HirExpr::Literal(HirLiteral::I16(n)) => Some(*n as i64),
        HirExpr::Literal(HirLiteral::I8(n)) => Some(*n as i64),
        HirExpr::Literal(HirLiteral::U64(n)) => Some(*n as i64),
        HirExpr::Literal(HirLiteral::U32(n)) => Some(*n as i64),
        HirExpr::Literal(HirLiteral::U16(n)) => Some(*n as i64),
        HirExpr::Literal(HirLiteral::U8(n)) => Some(*n as i64),
        HirExpr::Literal(HirLiteral::BigInt(bi)) => {
            use num_traits::ToPrimitive;
            Some(bi.to_u64().unwrap_or(0) as i64)
        }
        HirExpr::BinaryOp(lhs, op, rhs) => {
            let l = eval_const_expr(lhs)?;
            let r = eval_const_expr(rhs)?;
            match op {
                BinOp::Add => Some(l.wrapping_add(r)),
                BinOp::Sub => Some(l.wrapping_sub(r)),
                BinOp::Mul => Some(l.wrapping_mul(r)),
                BinOp::Div | BinOp::IntDiv => (r != 0).then(|| l.wrapping_div(r)),
                BinOp::Mod => (r != 0).then(|| l.wrapping_rem(r)),
                BinOp::And => Some(if l != 0 && r != 0 { 1 } else { 0 }),
                BinOp::Or => Some(if l != 0 || r != 0 { 1 } else { 0 }),
                BinOp::BitAnd => Some(l & r),
                BinOp::BitOr => Some(l | r),
                BinOp::BitXor => Some(l ^ r),
                BinOp::ShiftLeft => (r >= 0 && r < 64).then(|| l << r),
                BinOp::ShiftRight => (r >= 0 && r < 64).then(|| l >> r),
                BinOp::Eq | BinOp::StrictEq => Some(if l == r { 1 } else { 0 }),
                BinOp::Ne | BinOp::StrictNe => Some(if l != r { 1 } else { 0 }),
                BinOp::Lt => Some(if l < r { 1 } else { 0 }),
                BinOp::Le => Some(if l <= r { 1 } else { 0 }),
                BinOp::Gt => Some(if l > r { 1 } else { 0 }),
                BinOp::Ge => Some(if l >= r { 1 } else { 0 }),
                _ => None,
            }
        }
        HirExpr::UnaryOp(UnaryOp::Neg, inner) => eval_const_expr(inner).map(|v| -v),
        HirExpr::UnaryOp(UnaryOp::BitNot, inner) => eval_const_expr(inner).map(|v| !v),
        _ => None,
    }
}

impl<'a> FunctionLoweringContext<'a> {
    pub fn new(
        func: &'a mut MachineFunction,
        module: &'a mut NativeModule,
        target: &'a TargetDescriptor,
    ) -> Self {
        let call_conv: Box<dyn CallingConvention> = match target.operating_system {
            OperatingSystem::Windows => Box::new(WindowsX64CallingConvention),
            _ => Box::new(SystemVX64CallingConvention),
        };

        Self {
            func,
            module,
            target,
            call_conv,
            local_vars: HashMap::new(),
            loop_stack: Vec::new(),
            current_stack_offset: 16, // after saved RBP and return address
            current_block_id: 0,
            label_counter: 0,
        }
    }

    pub fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("{}_{}", prefix, self.label_counter)
    }

    pub fn alloc_stack_slot(&mut self, size: i32) -> i32 {
        let slot = self.current_stack_offset;
        self.current_stack_offset += size;
        // Keep 8-byte aligned
        if self.current_stack_offset % 8 != 0 {
            self.current_stack_offset += 8 - (self.current_stack_offset % 8);
        }
        self.func.stack_size = self.current_stack_offset as u64;
        slot
    }

    /// Emit a C-ABI call sequence: register arguments, stack arguments beyond
    /// the register file, the Win64 32-byte shadow space, and post-call stack
    /// cleanup. `total` is 16-byte aligned so RSP stays aligned at the call
    /// instruction (required by both SysV and Win64).
    pub fn emit_call_with_args(&mut self, symbol: &str, args: &[VirtualRegister]) {
        if !self.module.imports.contains(&symbol.to_string()) {
            self.module.imports.push(symbol.to_string());
        }

        let param_regs = self.call_conv.arg_registers().to_vec();
        let num_reg_args = args.len().min(param_regs.len());
        let shadow = self.call_conv.shadow_space() as i32;
        let stack_arg_bytes = ((args.len() - num_reg_args) * 8) as i32;
        let reg_area = (num_reg_args * 8) as i32;
        let total = ((shadow.max(reg_area) + stack_arg_bytes) + 15) & !15;

        if total > 0 {
            // Reserve the outgoing argument area.
            self.emit(MachineInstruction::Sub {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total as i64),
            });

            // Store register arguments to outgoing area staging slots to prevent cross-clobbering
            for (i, &r) in args.iter().take(num_reg_args).enumerate() {
                self.emit(MachineInstruction::Store {
                    dst: MachineOperand::Memory {
                        base: MachineRegister::Physical(PhysicalRegister(4)), // RSP
                        offset: (i as i32) * 8,
                        index: None,
                    },
                    src: MachineOperand::Register(MachineRegister::Virtual(r)),
                    size: 8,
                });
            }

            // Store stack arguments beyond the register file
            for (i, &r) in args.iter().skip(num_reg_args).enumerate() {
                self.emit(MachineInstruction::Store {
                    dst: MachineOperand::Memory {
                        base: MachineRegister::Physical(PhysicalRegister(4)), // RSP
                        offset: shadow + (i as i32) * 8,
                        index: None,
                    },
                    src: MachineOperand::Register(MachineRegister::Virtual(r)),
                    size: 8,
                });
            }

            // Load staging slots into actual physical parameter registers safely
            for (i, _) in args.iter().take(num_reg_args).enumerate() {
                self.emit(MachineInstruction::Load {
                    dst: MachineOperand::Register(MachineRegister::Physical(param_regs[i])),
                    src: MachineOperand::Memory {
                        base: MachineRegister::Physical(PhysicalRegister(4)), // RSP
                        offset: (i as i32) * 8,
                        index: None,
                    },
                    size: 8,
                });
            }
        }

        self.emit(MachineInstruction::Call {
            target: MachineOperand::Symbol(symbol.to_string()),
            num_args: args.len(),
        });

        if total > 0 {
            self.emit(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total as i64),
            });
        }
    }

    pub fn emit(&mut self, inst: MachineInstruction) {
        if let Some(block) = self
            .func
            .blocks
            .iter_mut()
            .find(|b| b.id == self.current_block_id)
        {
            block.push(inst);
        }
    }

    /// Convert any expression into a runtime handle (for composite values or typed runtime storage).
    pub fn lower_to_handle(&mut self, expr: &HirExpr) -> VirtualRegister {
        self.lower_to_handle_with_type(expr, None)
    }

    /// Convert an expression into a typed runtime handle if a type hint is present.
    pub fn lower_to_handle_with_type(
        &mut self,
        expr: &HirExpr,
        ty: Option<&HirType>,
    ) -> VirtualRegister {
        if let Some(t) = ty {
            match t {
                HirType::I8 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_i8", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::I16 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_i16", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::I32 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_i32", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::I64 | HirType::Int => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_i64", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::U8 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_u8", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::U16 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_u16", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::U32 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_u32", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::U64 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_u64", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::F32 => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_f32", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::F64 | HirType::Float => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_f64", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::Bool => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_bool", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                HirType::Char => {
                    let vreg = self.lower_expression(expr);
                    let out = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_char", &[vreg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out;
                }
                _ => {}
            }
        }

        let out_handle = self.func.alloc_vreg();
        match expr {
            HirExpr::Literal(lit) => {
                match lit {
                    HirLiteral::String(s) => {
                        let str_idx = self.module.add_string(s);
                        let sym_name = format!("__str_{}", str_idx);
                        let str_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                            src: MachineOperand::Symbol(sym_name),
                        });
                        self.emit_call_with_args("aot_make_string", &[str_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::Int(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n),
                        });
                        self.emit_call_with_args("aot_make_i64", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::U8(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                        self.emit_call_with_args("aot_make_u8", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::U16(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                        self.emit_call_with_args("aot_make_u16", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::U32(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                        self.emit_call_with_args("aot_make_u32", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::U64(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                        self.emit_call_with_args("aot_make_u64", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::I8(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                        self.emit_call_with_args("aot_make_i8", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::I16(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                        self.emit_call_with_args("aot_make_i16", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::I32(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                        self.emit_call_with_args("aot_make_i32", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::I64(n) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*n),
                        });
                        self.emit_call_with_args("aot_make_i64", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::Float(f) | HirLiteral::F64(f) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(f.to_bits() as i64),
                        });
                        self.emit_call_with_args("aot_make_f64", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::F32(f) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(f.to_bits() as i64),
                        });
                        self.emit_call_with_args("aot_make_f32", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::Bool(b) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(if *b { 1 } else { 0 }),
                        });
                        self.emit_call_with_args("aot_make_bool", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::Char(c) => {
                        let arg_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(*c as i64),
                        });
                        self.emit_call_with_args("aot_make_char", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::BigInt(bi) => {
                        use num_traits::ToPrimitive;
                        let arg_reg = self.func.alloc_vreg();
                        let val = bi.to_u64().unwrap_or(0) as i64;
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(arg_reg)),
                            src: MachineOperand::Immediate(val),
                        });
                        self.emit_call_with_args("aot_make_u64", &[arg_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    HirLiteral::Null => {
                        self.emit_call_with_args("aot_make_null", &[]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    _ => {
                        self.emit_call_with_args("aot_make_null", &[]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                }
                out_handle
            }
            HirExpr::LoadVar(name) => {
                let val_reg = self.lower_expression(expr);
                if let Some(&(_, Some(ref ty))) = self.local_vars.get(name) {
                    match ty {
                        HirType::I8 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_i8", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::I16 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_i16", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::I32 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_i32", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::I64 | HirType::Int => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_i64", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::U8 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_u8", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::U16 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_u16", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::U32 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_u32", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::U64 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_u64", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::F32 => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_f32", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::F64 | HirType::Float => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_f64", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::Bool => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_bool", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::Char => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_char", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        HirType::String => {
                            let out = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_string", &[val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            return out;
                        }
                        _ => return val_reg,
                    }
                }
                val_reg
            }
            HirExpr::ObjectLiteral(_)
            | HirExpr::StructLiteral(..)
            | HirExpr::ArrayLiteral(_)
            | HirExpr::TupleLiteral(_)
            | HirExpr::SetLiteral(_)
            | HirExpr::DictLiteral(_)
            | HirExpr::MemberAccess(..)
            | HirExpr::Index(..)
            | HirExpr::Call(..)
            | HirExpr::MethodCall(..) => self.lower_expression(expr),
            HirExpr::Lambda(..) => {
                let fn_ptr_reg = self.lower_expression(expr);
                let out_handle = self.func.alloc_vreg();
                self.emit_call_with_args("aot_make_function", &[fn_ptr_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_handle
            }
            _ => {
                let val_reg = self.lower_expression(expr);
                let out_handle = self.func.alloc_vreg();
                self.emit_call_with_args("aot_wrap_ptr", &[val_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_handle)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_handle
            }
        }
    }

    pub fn lower_statement(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Let { name, ty, init, .. } => {
                let slot = self.alloc_stack_slot(8);
                let off = -(slot + 8);
                let inferred_ty = if let Some(t) = ty {
                    Some(t.clone())
                } else if let Some(expr) = init {
                    infer_hir_expr_type(expr)
                } else {
                    None
                };
                if let Some(expr) = init {
                    let vreg = self.lower_expression(expr);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::StackSlot(off),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                }
                self.local_vars.insert(name.clone(), (off, inferred_ty));
            }
            HirStmt::LetTuple { names, init, .. } => {
                if let Some(expr) = init {
                    if let HirExpr::TupleLiteral(elements) = expr {
                        let mut regs = Vec::new();
                        for elem in elements {
                            let r = self.lower_expression(elem);
                            regs.push((r, infer_hir_expr_type(elem)));
                        }
                        for (name, (r, ty)) in names.iter().zip(regs) {
                            let slot = self.alloc_stack_slot(8);
                            let off = -(slot + 8);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(off),
                                src: MachineOperand::Register(MachineRegister::Virtual(r)),
                            });
                            self.local_vars.insert(name.clone(), (off, ty));
                        }
                    } else {
                        let tup_handle = self.lower_to_handle(expr);
                        for (i, name) in names.iter().enumerate() {
                            let idx_val_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(
                                    idx_val_reg,
                                )),
                                src: MachineOperand::Immediate(i as i64),
                            });
                            let idx_handle = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_make_i64", &[idx_val_reg]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(idx_handle)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            let elem_reg = self.func.alloc_vreg();
                            self.emit_call_with_args("aot_get_index", &[tup_handle, idx_handle]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
                                src: MachineOperand::Register(MachineRegister::Physical(
                                    PhysicalRegister(0),
                                )),
                            });
                            let slot = self.alloc_stack_slot(8);
                            let off = -(slot + 8);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(off),
                                src: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
                            });
                            self.local_vars
                                .insert(name.clone(), (off, Some(HirType::Any)));
                        }
                    }
                }
            }
            HirStmt::Assign { target, value, .. } => match target {
                HirExpr::LoadVar(name) => {
                    let val_vreg = self.lower_expression(value);
                    if let Some(&(slot, _)) = self.local_vars.get(name) {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::StackSlot(slot),
                            src: MachineOperand::Register(MachineRegister::Virtual(val_vreg)),
                        });
                    } else {
                        let slot = self.alloc_stack_slot(8);
                        let off = -(slot + 8);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::StackSlot(off),
                            src: MachineOperand::Register(MachineRegister::Virtual(val_vreg)),
                        });
                        let ty = infer_hir_expr_type(value);
                        self.local_vars.insert(name.clone(), (off, ty));
                    }
                }
                HirExpr::MemberAccess(obj_expr, field_name) => {
                    let obj_reg = self.lower_expression(obj_expr);
                    let str_idx = self.module.add_string(field_name);
                    let sym_name = format!("__str_{}", str_idx);
                    let str_reg = self.func.alloc_vreg();
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                        src: MachineOperand::Symbol(sym_name),
                    });
                    let field_handle = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_string", &[str_reg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(field_handle)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    let val_handle = self.lower_to_handle(value);
                    self.emit_call_with_args("aot_set_field", &[obj_reg, field_handle, val_handle]);
                }
                HirExpr::Index(obj_expr, index_expr) => {
                    let obj_reg = self.lower_expression(obj_expr);
                    let index_handle = self.lower_to_handle(index_expr);
                    let val_handle = self.lower_to_handle(value);
                    self.emit_call_with_args("aot_set_index", &[obj_reg, index_handle, val_handle]);
                }
                _ => {
                    self.lower_expression(value);
                }
            },
            HirStmt::Expr(expr) => {
                self.lower_expression(expr);
            }
            HirStmt::Return(expr_opt) => {
                if let Some(expr) = expr_opt {
                    let ret_vreg = self.lower_expression(expr);
                    // Move to return register (RAX = phys 0)
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                        src: MachineOperand::Register(MachineRegister::Virtual(ret_vreg)),
                    });
                }
                self.emit(MachineInstruction::Return);
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_vreg = self.lower_expression(cond);
                let else_lbl = self.fresh_label("else_branch");
                let end_lbl = self.fresh_label("end_if");

                // Test condition != 0
                self.emit(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(cond_vreg)),
                    rhs: MachineOperand::Immediate(0),
                });

                if else_branch.is_some() {
                    self.emit(MachineInstruction::BranchCc {
                        cc: ConditionCode::Equal,
                        target: else_lbl.clone(),
                    });
                } else {
                    self.emit(MachineInstruction::BranchCc {
                        cc: ConditionCode::Equal,
                        target: end_lbl.clone(),
                    });
                }

                // Then branch
                self.lower_statement(then_branch);

                if let Some(else_b) = else_branch {
                    self.emit(MachineInstruction::Branch {
                        target: end_lbl.clone(),
                    });

                    // Start else block
                    let else_id = self.func.create_block(&else_lbl);
                    self.current_block_id = else_id;
                    self.lower_statement(else_b);
                }

                // Start end block
                let end_id = self.func.create_block(&end_lbl);
                self.current_block_id = end_id;
            }
            HirStmt::While { cond, body } => {
                let loop_start_lbl = self.fresh_label("while_start");
                let loop_end_lbl = self.fresh_label("while_end");

                self.loop_stack.push(LoopContext {
                    start_label: loop_start_lbl.clone(),
                    step_label: loop_start_lbl.clone(),
                    end_label: loop_end_lbl.clone(),
                    idx_slot: None,
                });

                self.emit(MachineInstruction::Branch {
                    target: loop_start_lbl.clone(),
                });

                let loop_id = self.func.create_block(&loop_start_lbl);
                self.current_block_id = loop_id;

                let cond_vreg = self.lower_expression(cond);
                self.emit(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(cond_vreg)),
                    rhs: MachineOperand::Immediate(0),
                });
                self.emit(MachineInstruction::BranchCc {
                    cc: ConditionCode::Equal,
                    target: loop_end_lbl.clone(),
                });

                self.lower_statement(body);
                self.emit(MachineInstruction::Branch {
                    target: loop_start_lbl,
                });

                let end_id = self.func.create_block(&loop_end_lbl);
                self.current_block_id = end_id;

                self.loop_stack.pop();
            }
            HirStmt::ForIn { var, iter, body } => {
                let iter_handle = self.lower_to_handle(iter);
                let iter_slot = self.alloc_stack_slot(8);
                let iter_off = -(iter_slot + 8);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(iter_off),
                    src: MachineOperand::Register(MachineRegister::Virtual(iter_handle)),
                });

                let len_reg = self.func.alloc_vreg();
                self.emit_call_with_args("aot_len", &[iter_handle]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(len_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                let len_slot = self.alloc_stack_slot(8);
                let len_off = -(len_slot + 8);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(len_off),
                    src: MachineOperand::Register(MachineRegister::Virtual(len_reg)),
                });

                let idx_slot = self.alloc_stack_slot(8);
                let idx_off = -(idx_slot + 8);
                let zero_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(zero_reg)),
                    src: MachineOperand::Immediate(0),
                });
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(idx_off),
                    src: MachineOperand::Register(MachineRegister::Virtual(zero_reg)),
                });

                let start_lbl = self.fresh_label("for_start");
                let step_lbl = self.fresh_label("for_step");
                let end_lbl = self.fresh_label("for_end");

                self.loop_stack.push(LoopContext {
                    start_label: start_lbl.clone(),
                    step_label: step_lbl.clone(),
                    end_label: end_lbl.clone(),
                    idx_slot: Some(idx_off),
                });

                self.emit(MachineInstruction::Branch {
                    target: start_lbl.clone(),
                });

                let start_id = self.func.create_block(&start_lbl);
                self.current_block_id = start_id;

                let cur_idx_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(cur_idx_reg)),
                    src: MachineOperand::StackSlot(idx_off),
                });
                let cur_len_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(cur_len_reg)),
                    src: MachineOperand::StackSlot(len_off),
                });
                self.emit(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(cur_idx_reg)),
                    rhs: MachineOperand::Register(MachineRegister::Virtual(cur_len_reg)),
                });
                self.emit(MachineInstruction::BranchCc {
                    cc: ConditionCode::GreaterOrEqual,
                    target: end_lbl.clone(),
                });

                let cur_iter_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(cur_iter_reg)),
                    src: MachineOperand::StackSlot(iter_off),
                });

                let elem_reg = self.func.alloc_vreg();
                self.emit_call_with_args("aot_get_index", &[cur_iter_reg, cur_idx_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });

                let slot = if let Some(&(s, _)) = self.local_vars.get(var) {
                    s
                } else {
                    let s = self.alloc_stack_slot(8);
                    let off = -(s + 8);
                    self.local_vars.insert(var.clone(), (off, None));
                    off
                };
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(slot),
                    src: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
                });

                self.lower_statement(body);

                self.emit(MachineInstruction::Branch {
                    target: step_lbl.clone(),
                });

                let step_id = self.func.create_block(&step_lbl);
                self.current_block_id = step_id;
                let inc_idx_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(inc_idx_reg)),
                    src: MachineOperand::StackSlot(idx_off),
                });
                self.emit(MachineInstruction::Add {
                    dst: MachineOperand::Register(MachineRegister::Virtual(inc_idx_reg)),
                    src: MachineOperand::Immediate(1),
                });
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(idx_off),
                    src: MachineOperand::Register(MachineRegister::Virtual(inc_idx_reg)),
                });
                self.emit(MachineInstruction::Branch { target: start_lbl });

                let end_id = self.func.create_block(&end_lbl);
                self.current_block_id = end_id;

                self.loop_stack.pop();
            }
            HirStmt::Break => {
                if let Some(ctx) = self.loop_stack.last() {
                    self.emit(MachineInstruction::Branch {
                        target: ctx.end_label.clone(),
                    });
                }
            }
            HirStmt::Continue => {
                if let Some(ctx) = self.loop_stack.last() {
                    self.emit(MachineInstruction::Branch {
                        target: ctx.step_label.clone(),
                    });
                }
            }
            HirStmt::Jump(expr) => {
                if let Some(ctx) = self.loop_stack.last().cloned() {
                    let target_idx_reg = self.lower_expression(expr);
                    if let Some(idx_slot) = ctx.idx_slot {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::StackSlot(idx_slot),
                            src: MachineOperand::Register(MachineRegister::Virtual(target_idx_reg)),
                        });
                        self.emit(MachineInstruction::Branch {
                            target: ctx.start_label,
                        });
                    } else {
                        self.emit(MachineInstruction::Branch {
                            target: ctx.step_label,
                        });
                    }
                }
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.lower_statement(s);
                }
            }
            HirStmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.lower_statement(try_block);
                // In native code, non-faulting execution continues normally
                let _ = catch_block;
            }
            HirStmt::Defer(stmt) => {
                self.lower_statement(stmt);
            }
            HirStmt::Extend { methods, .. } => {
                for m in methods {
                    lower_hir_function(m, self.module, self.target);
                }
            }
            HirStmt::FunctionDef {
                name,
                params,
                body,
                ret_type,
                is_async,
                decorators,
                move_params,
                is_unsafe,
            } => {
                let func = HirFunction {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    ret_type: ret_type.clone(),
                    is_async: *is_async,
                    decorators: decorators.clone(),
                    is_exported: true,
                    move_params: move_params.clone(),
                    is_test: false,
                    test_ignore: false,
                    test_expect_fail: false,
                    test_timeout: None,
                    is_unsafe: *is_unsafe,
                };
                lower_hir_function(&func, self.module, self.target);
            }
            _ => {}
        }
    }

    pub fn lower_expression(&mut self, expr: &HirExpr) -> VirtualRegister {
        match expr {
            HirExpr::Literal(lit) => {
                let out_reg = self.func.alloc_vreg();
                match lit {
                    HirLiteral::Int(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n),
                        });
                    }
                    HirLiteral::I8(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                    }
                    HirLiteral::I16(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                    }
                    HirLiteral::I32(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                    }
                    HirLiteral::I64(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n),
                        });
                    }
                    HirLiteral::U8(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                    }
                    HirLiteral::U16(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                    }
                    HirLiteral::U32(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                    }
                    HirLiteral::U64(n) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*n as i64),
                        });
                    }
                    HirLiteral::Bool(b) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(if *b { 1 } else { 0 }),
                        });
                    }
                    HirLiteral::String(s) => {
                        let str_idx = self.module.add_string(s);
                        let sym_name = format!("__str_{}", str_idx);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Symbol(sym_name),
                        });
                    }
                    HirLiteral::Char(c) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(*c as i64),
                        });
                    }
                    HirLiteral::Float(f) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::FloatImmediate(*f),
                        });
                    }
                    HirLiteral::F64(f) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::FloatImmediate(*f),
                        });
                    }
                    HirLiteral::F32(f) => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::FloatImmediate(*f as f64),
                        });
                    }
                    HirLiteral::BigInt(bi) => {
                        use num_traits::ToPrimitive;
                        let val = bi.to_u64().unwrap_or(0) as i64;
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(val),
                        });
                    }
                    HirLiteral::Null => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });
                    }
                    _ => {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });
                    }
                }
                out_reg
            }
            HirExpr::LoadVar(name) => {
                let out_reg = self.func.alloc_vreg();
                if let Some(&(slot, _)) = self.local_vars.get(name) {
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::StackSlot(slot),
                    });
                } else {
                    // Fallback to 0 if undeclared
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Immediate(0),
                    });
                }
                out_reg
            }
            HirExpr::StoreVar(name, expr) => {
                let val_reg = self.lower_expression(expr);
                let ty = infer_hir_expr_type(expr);
                if let Some((slot, existing_ty)) = self.local_vars.get(name).cloned() {
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::StackSlot(slot),
                        src: MachineOperand::Register(MachineRegister::Virtual(val_reg)),
                    });
                    self.local_vars
                        .insert(name.clone(), (slot, ty.or(existing_ty)));
                } else {
                    let slot = self.alloc_stack_slot(8);
                    let off = -(slot + 8);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::StackSlot(off),
                        src: MachineOperand::Register(MachineRegister::Virtual(val_reg)),
                    });
                    self.local_vars.insert(name.clone(), (off, ty));
                }
                val_reg
            }
            HirExpr::ObjectLiteral(entries) => {
                let mut pairs = Vec::new();
                for (key, val_expr) in entries {
                    let k_idx = self.module.add_string(key);
                    let k_sym = format!("__str_{}", k_idx);
                    let k_str_reg = self.func.alloc_vreg();
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(k_str_reg)),
                        src: MachineOperand::Symbol(k_sym),
                    });
                    let k_handle = self.func.alloc_vreg();
                    self.emit_call_with_args("aot_make_string", &[k_str_reg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(k_handle)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });

                    let v_handle = self.lower_to_handle(val_expr);
                    pairs.push((k_handle, v_handle));
                }

                let count = (pairs.len() * 2) as i32;
                let size = count.max(2) * 8;
                let slot = self.alloc_stack_slot(size);
                let base_off = -(slot + size);

                for (i, (k_h, v_h)) in pairs.iter().enumerate() {
                    let k_off = base_off + (i as i32 * 16);
                    let v_off = base_off + (i as i32 * 16 + 8);
                    self.emit(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                            offset: k_off,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(*k_h)),
                        size: 8,
                    });
                    self.emit(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                            offset: v_off,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(*v_h)),
                        size: 8,
                    });
                }

                let ptr_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))), // RBP
                });
                self.emit(MachineInstruction::Sub {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Immediate((slot + size) as i64),
                });

                let count_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
                    src: MachineOperand::Immediate(count as i64),
                });

                self.emit_call_with_args("aot_make_object", &[ptr_reg, count_reg]);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::StructLiteral(name, fields) => {
                let mut all_fields = fields.clone();
                all_fields.push((
                    "__struct".to_string(),
                    HirExpr::Literal(HirLiteral::String(name.clone())),
                ));
                self.lower_expression(&HirExpr::ObjectLiteral(all_fields))
            }
            HirExpr::ArrayLiteral(elements) => {
                let mut handles = Vec::new();
                for elem in elements {
                    let h = self.lower_to_handle(elem);
                    handles.push(h);
                }

                let count = handles.len() as i32;
                let size = count.max(1) * 8;
                let slot = self.alloc_stack_slot(size);
                let base_off = -(slot + size);

                for (i, h) in handles.iter().enumerate() {
                    let off = base_off + (i as i32 * 8);
                    self.emit(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                            offset: off,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(*h)),
                        size: 8,
                    });
                }

                let ptr_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))), // RBP
                });
                self.emit(MachineInstruction::Sub {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Immediate((slot + size) as i64),
                });

                let count_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
                    src: MachineOperand::Immediate(count as i64),
                });

                self.emit_call_with_args("aot_make_array", &[ptr_reg, count_reg]);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::TupleLiteral(elements) => {
                let mut handles = Vec::new();
                for elem in elements {
                    let h = self.lower_to_handle(elem);
                    handles.push(h);
                }

                let count = handles.len() as i32;
                let size = count.max(1) * 8;
                let slot = self.alloc_stack_slot(size);
                let base_off = -(slot + size);

                for (i, h) in handles.iter().enumerate() {
                    let off = base_off + (i as i32 * 8);
                    self.emit(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)),
                            offset: off,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(*h)),
                        size: 8,
                    });
                }

                let ptr_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))),
                });
                self.emit(MachineInstruction::Sub {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Immediate((slot + size) as i64),
                });

                let count_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
                    src: MachineOperand::Immediate(count as i64),
                });

                self.emit_call_with_args("aot_make_tuple", &[ptr_reg, count_reg]);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::SetLiteral(elements) => {
                let mut handles = Vec::new();
                for elem in elements {
                    let h = self.lower_to_handle(elem);
                    handles.push(h);
                }

                let count = handles.len() as i32;
                let size = count.max(1) * 8;
                let slot = self.alloc_stack_slot(size);
                let base_off = -(slot + size);

                for (i, h) in handles.iter().enumerate() {
                    let off = base_off + (i as i32 * 8);
                    self.emit(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)),
                            offset: off,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(*h)),
                        size: 8,
                    });
                }

                let ptr_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))),
                });
                self.emit(MachineInstruction::Sub {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Immediate((slot + size) as i64),
                });

                let count_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
                    src: MachineOperand::Immediate(count as i64),
                });

                self.emit_call_with_args("aot_make_set", &[ptr_reg, count_reg]);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::MemberAccess(target, field_name) => {
                let target_reg = self.lower_expression(target);
                let str_idx = self.module.add_string(field_name);
                let sym_name = format!("__str_{}", str_idx);
                let str_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                    src: MachineOperand::Symbol(sym_name),
                });
                let field_handle = self.func.alloc_vreg();
                self.emit_call_with_args("aot_make_string", &[str_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(field_handle)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });

                self.emit_call_with_args("aot_get_field", &[target_reg, field_handle]);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::Index(target, index_expr) => {
                let target_reg = self.lower_expression(target);
                let index_handle = self.lower_to_handle(index_expr);

                self.emit_call_with_args("aot_get_index", &[target_reg, index_handle]);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::SetMember(target, field_name, val_expr) => {
                let target_reg = self.lower_expression(target);
                let str_idx = self.module.add_string(field_name);
                let sym_name = format!("__str_{}", str_idx);
                let str_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                    src: MachineOperand::Symbol(sym_name),
                });
                let field_handle = self.func.alloc_vreg();
                self.emit_call_with_args("aot_make_string", &[str_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(field_handle)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });

                let val_handle = self.lower_to_handle(val_expr);

                self.emit_call_with_args("aot_set_field", &[target_reg, field_handle, val_handle]);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::MethodCall(target, method_name, args) => {
                let out_reg = self.func.alloc_vreg();
                if let HirExpr::LoadVar(name) = &**target {
                    if name == "input" && (method_name == "mock" || method_name == "play") {
                        let handle = if !args.is_empty() {
                            self.lower_to_handle(&args[0])
                        } else {
                            let zero_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(zero_reg)),
                                src: MachineOperand::Immediate(0),
                            });
                            zero_reg
                        };
                        self.emit_call_with_args("aot_input_mock", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });
                        return out_reg;
                    }
                    if name == "Parallel" && method_name == "forEach" && args.len() >= 3 {
                        let start_reg = self.lower_expression(&args[0]);
                        let end_reg = self.lower_expression(&args[1]);
                        let cb_reg = self.lower_expression(&args[2]);
                        self.emit_call_with_args(
                            "aot_parallel_for_each",
                            &[start_reg, end_reg, cb_reg],
                        );
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });
                        return out_reg;
                    }
                    if name == "Collections" || name == "std:Collections" || name == "collections" {
                        let str_idx = self.module.add_string(method_name);
                        let sym_name = format!("__str_{}", str_idx);
                        let str_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                            src: MachineOperand::Symbol(sym_name),
                        });
                        let type_handle = self.func.alloc_vreg();
                        self.emit_call_with_args("aot_make_string", &[str_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(type_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });

                        let mut handles = Vec::new();
                        for arg in args {
                            let h = self.lower_to_handle(arg);
                            handles.push(h);
                        }

                        let count = handles.len() as i32;
                        let size = count.max(1) * 8;
                        let slot = self.alloc_stack_slot(size);
                        let base_off = -(slot + size);

                        for (i, h) in handles.iter().enumerate() {
                            let off = base_off + (i as i32 * 8);
                            self.emit(MachineInstruction::Store {
                                dst: MachineOperand::Memory {
                                    base: MachineRegister::Physical(PhysicalRegister(5)),
                                    offset: off,
                                    index: None,
                                },
                                src: MachineOperand::Register(MachineRegister::Virtual(*h)),
                                size: 8,
                            });
                        }

                        let ptr_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(5),
                            )),
                        });
                        self.emit(MachineInstruction::Sub {
                            dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                            src: MachineOperand::Immediate((slot + size) as i64),
                        });

                        let count_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
                            src: MachineOperand::Immediate(count as i64),
                        });

                        self.emit_call_with_args(
                            "aot_collections_new",
                            &[type_handle, ptr_reg, count_reg],
                        );
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                }
                if method_name == "forEach" && !args.is_empty() {
                    let target_handle = self.lower_to_handle(target);
                    let cb_reg = self.lower_expression(&args[0]);
                    self.emit_call_with_args("aot_array_for_each", &[target_handle, cb_reg]);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Immediate(0),
                    });
                    return out_reg;
                }

                // If user defined a function with this name in the module, call it directly
                if self.module.functions.iter().any(|f| f.name == *method_name) {
                    let target_reg = self.lower_expression(target);
                    let mut arg_regs = vec![target_reg];
                    for arg in args {
                        arg_regs.push(self.lower_expression(arg));
                    }
                    self.emit_call_with_args(method_name, &arg_regs);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                    return out_reg;
                }

                // Otherwise dispatch via runtime aot_call_method
                let target_handle = self.lower_to_handle(target);
                let str_idx = self.module.add_string(method_name);
                let sym_name = format!("__str_{}", str_idx);
                let str_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(str_reg)),
                    src: MachineOperand::Symbol(sym_name),
                });
                let method_handle = self.func.alloc_vreg();
                self.emit_call_with_args("aot_make_string", &[str_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(method_handle)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });

                let mut handles = Vec::new();
                for arg in args {
                    let h = self.lower_to_handle(arg);
                    handles.push(h);
                }

                let count = handles.len() as i32;
                let size = count.max(1) * 8;
                let slot = self.alloc_stack_slot(size);
                let base_off = -(slot + size);

                for (i, h) in handles.iter().enumerate() {
                    let off = base_off + (i as i32 * 8);
                    self.emit(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)),
                            offset: off,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(*h)),
                        size: 8,
                    });
                }

                let ptr_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))),
                });
                self.emit(MachineInstruction::Sub {
                    dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                    src: MachineOperand::Immediate((slot + size) as i64),
                });

                let count_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
                    src: MachineOperand::Immediate(count as i64),
                });

                self.emit_call_with_args(
                    "aot_call_method",
                    &[target_handle, method_handle, ptr_reg, count_reg],
                );
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::Lambda(params, body, _) => {
                let lambda_name = format!("__lambda_{}_{}", self.func.name, self.label_counter);
                self.label_counter += 1;
                let mut lambda_func = MachineFunction::new(&lambda_name);
                lambda_func.is_exported = true;
                {
                    let mut lambda_ctx =
                        FunctionLoweringContext::new(&mut lambda_func, self.module, self.target);
                    for (i, (p_name, p_ty)) in params.iter().enumerate() {
                        let slot = lambda_ctx.alloc_stack_slot(8);
                        let off = -(slot + 8);
                        if i < lambda_ctx.call_conv.arg_registers().len() {
                            let p_reg = lambda_ctx.call_conv.arg_registers()[i];
                            lambda_ctx.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(off),
                                src: MachineOperand::Register(MachineRegister::Physical(p_reg)),
                            });
                        } else {
                            let caller_arg_offset =
                                16 + ((i - lambda_ctx.call_conv.arg_registers().len()) as i32 * 8);
                            let scratch_reg = lambda_ctx.func.alloc_vreg();
                            lambda_ctx.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(
                                    scratch_reg,
                                )),
                                src: MachineOperand::StackSlot(caller_arg_offset),
                            });
                            lambda_ctx.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(off),
                                src: MachineOperand::Register(MachineRegister::Virtual(
                                    scratch_reg,
                                )),
                            });
                        }
                        lambda_ctx
                            .local_vars
                            .insert(p_name.clone(), (off, p_ty.clone()));
                    }
                    for stmt in body.iter() {
                        lambda_ctx.lower_statement(stmt);
                    }
                    lambda_ctx.emit(MachineInstruction::Return);
                }
                self.module.add_function(lambda_func);

                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Symbol(lambda_name),
                });
                out_reg
            }
            HirExpr::Conditional(cond, then_b, else_b) => {
                let cond_reg = self.lower_expression(cond);
                let else_lbl = self.fresh_label("cond_else");
                let end_lbl = self.fresh_label("cond_end");
                let out_reg = self.func.alloc_vreg();

                self.emit(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(cond_reg)),
                    rhs: MachineOperand::Immediate(0),
                });
                self.emit(MachineInstruction::BranchCc {
                    cc: ConditionCode::Equal,
                    target: else_lbl.clone(),
                });

                let then_reg = self.lower_expression(then_b);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Virtual(then_reg)),
                });
                self.emit(MachineInstruction::Branch {
                    target: end_lbl.clone(),
                });

                let else_id = self.func.create_block(&else_lbl);
                self.current_block_id = else_id;
                let else_reg = self.lower_expression(else_b);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Virtual(else_reg)),
                });

                let end_id = self.func.create_block(&end_lbl);
                self.current_block_id = end_id;

                out_reg
            }
            HirExpr::Update(inner, is_inc, is_prefix) => {
                if let HirExpr::LoadVar(name) = &**inner {
                    if let Some((slot, existing_ty)) = self.local_vars.get(name).cloned() {
                        let val_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(val_reg)),
                            src: MachineOperand::StackSlot(slot),
                        });
                        let new_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(new_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(val_reg)),
                        });
                        if *is_inc {
                            self.emit(MachineInstruction::Add {
                                dst: MachineOperand::Register(MachineRegister::Virtual(new_reg)),
                                src: MachineOperand::Immediate(1),
                            });
                        } else {
                            self.emit(MachineInstruction::Sub {
                                dst: MachineOperand::Register(MachineRegister::Virtual(new_reg)),
                                src: MachineOperand::Immediate(1),
                            });
                        }
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::StackSlot(slot),
                            src: MachineOperand::Register(MachineRegister::Virtual(new_reg)),
                        });
                        self.local_vars.insert(name.clone(), (slot, existing_ty));
                        if *is_prefix { new_reg } else { val_reg }
                    } else {
                        let out_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });
                        out_reg
                    }
                } else {
                    let out_reg = self.func.alloc_vreg();
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Immediate(0),
                    });
                    out_reg
                }
            }
            HirExpr::BinaryOp(lhs, op, rhs) => {
                if let Some(folded_val) = eval_const_expr(expr) {
                    let out_reg = self.func.alloc_vreg();
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Immediate(folded_val),
                    });
                    return out_reg;
                }

                let l_reg = self.lower_expression(lhs);
                let r_reg = self.lower_expression(rhs);
                let out_reg = self.func.alloc_vreg();

                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                });

                match op {
                    BinOp::Add => {
                        self.emit(MachineInstruction::Add {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::Sub => {
                        self.emit(MachineInstruction::Sub {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::Mul => {
                        self.emit(MachineInstruction::Mul {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::Div => {
                        self.emit(MachineInstruction::Div {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::Mod => {
                        self.emit(MachineInstruction::Mod {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::BitAnd => {
                        self.emit(MachineInstruction::And {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::BitOr => {
                        self.emit(MachineInstruction::Or {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::BitXor => {
                        self.emit(MachineInstruction::Xor {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::ShiftLeft => {
                        self.emit(MachineInstruction::Shl {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::ShiftRight => {
                        self.emit(MachineInstruction::Shr {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::Eq | BinOp::StrictEq => {
                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                        self.emit(MachineInstruction::SetCc {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            cc: ConditionCode::Equal,
                        });
                    }
                    BinOp::Ne | BinOp::StrictNe => {
                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                        self.emit(MachineInstruction::SetCc {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            cc: ConditionCode::NotEqual,
                        });
                    }
                    BinOp::Lt => {
                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                        self.emit(MachineInstruction::SetCc {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            cc: ConditionCode::LessThan,
                        });
                    }
                    BinOp::Le => {
                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                        self.emit(MachineInstruction::SetCc {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            cc: ConditionCode::LessOrEqual,
                        });
                    }
                    BinOp::Gt => {
                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                        self.emit(MachineInstruction::SetCc {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            cc: ConditionCode::GreaterThan,
                        });
                    }
                    BinOp::Ge => {
                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                        self.emit(MachineInstruction::SetCc {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            cc: ConditionCode::GreaterOrEqual,
                        });
                    }
                    BinOp::IntDiv => {
                        self.emit(MachineInstruction::Div {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                        });
                    }
                    BinOp::Or => {
                        let true_lbl = self.fresh_label("or_true");
                        let end_lbl = self.fresh_label("or_end");

                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Immediate(0),
                        });
                        self.emit(MachineInstruction::BranchCc {
                            cc: ConditionCode::NotEqual,
                            target: true_lbl.clone(),
                        });

                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                            rhs: MachineOperand::Immediate(0),
                        });
                        self.emit(MachineInstruction::BranchCc {
                            cc: ConditionCode::NotEqual,
                            target: true_lbl.clone(),
                        });

                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });
                        self.emit(MachineInstruction::Branch {
                            target: end_lbl.clone(),
                        });

                        let t_id = self.func.create_block(&true_lbl);
                        self.current_block_id = t_id;
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(1),
                        });

                        let end_id = self.func.create_block(&end_lbl);
                        self.current_block_id = end_id;
                    }
                    BinOp::And => {
                        let false_lbl = self.fresh_label("and_false");
                        let end_lbl = self.fresh_label("and_end");

                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Immediate(0),
                        });
                        self.emit(MachineInstruction::BranchCc {
                            cc: ConditionCode::Equal,
                            target: false_lbl.clone(),
                        });

                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                            rhs: MachineOperand::Immediate(0),
                        });
                        self.emit(MachineInstruction::BranchCc {
                            cc: ConditionCode::Equal,
                            target: false_lbl.clone(),
                        });

                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(1),
                        });
                        self.emit(MachineInstruction::Branch {
                            target: end_lbl.clone(),
                        });

                        let f_id = self.func.create_block(&false_lbl);
                        self.current_block_id = f_id;
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });

                        let end_id = self.func.create_block(&end_lbl);
                        self.current_block_id = end_id;
                    }
                    _ => {}
                }
                out_reg
            }
            HirExpr::UnaryOp(op, inner) => {
                let in_reg = self.lower_expression(inner);
                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Virtual(in_reg)),
                });

                match op {
                    UnaryOp::Neg => {
                        self.emit(MachineInstruction::Neg {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        });
                    }
                    UnaryOp::Not => {
                        self.emit(MachineInstruction::Xor {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(1),
                        });
                    }
                    UnaryOp::BitNot => {
                        self.emit(MachineInstruction::Not {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        });
                    }
                    UnaryOp::Typeof => {
                        let handle = self.lower_to_handle(inner);
                        self.emit_call_with_args("aot_typeof", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                    }
                    _ => {}
                }
                out_reg
            }
            HirExpr::Call(callee, args, _) => {
                let out_reg = self.func.alloc_vreg();

                if let HirExpr::LoadVar(fn_name) = &**callee {
                    if (fn_name == "sizeof" || fn_name == "sizeOf") && !args.is_empty() {
                        let handle = self.lower_to_handle(&args[0]);
                        self.emit_call_with_args("aot_sizeof", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if (fn_name == "typeof" || fn_name == "type" || fn_name == "typeOf")
                        && !args.is_empty()
                    {
                        let handle = self.lower_to_handle(&args[0]);
                        self.emit_call_with_args("aot_typeof", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if matches!(
                        fn_name.as_str(),
                        "int"
                            | "i64"
                            | "i32"
                            | "i16"
                            | "i8"
                            | "u64"
                            | "u32"
                            | "u16"
                            | "u8"
                            | "BigInt"
                            | "bigint"
                    ) && !args.is_empty()
                    {
                        let handle = self.lower_to_handle(&args[0]);
                        self.emit_call_with_args("aot_to_int", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if matches!(
                        fn_name.as_str(),
                        "float" | "f64" | "f32" | "number" | "Number"
                    ) && !args.is_empty()
                    {
                        let handle = self.lower_to_handle(&args[0]);
                        self.emit_call_with_args("aot_to_float", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if matches!(fn_name.as_str(), "str" | "string" | "String") && !args.is_empty() {
                        let handle = self.lower_to_handle(&args[0]);
                        self.emit_call_with_args("aot_to_string", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if matches!(fn_name.as_str(), "bool" | "Boolean") && !args.is_empty() {
                        let handle = self.lower_to_handle(&args[0]);
                        self.emit_call_with_args("aot_to_bool", &[handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if fn_name == "input" {
                        let prompt_handle = if !args.is_empty() {
                            self.lower_to_handle(&args[0])
                        } else {
                            let zero_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(zero_reg)),
                                src: MachineOperand::Immediate(0),
                            });
                            zero_reg
                        };
                        self.emit_call_with_args("aot_input", &[prompt_handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if fn_name == "clock" {
                        self.emit_call_with_args("clock", &[]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        return out_reg;
                    }
                    if fn_name == "print" || fn_name == "println" {
                        if args.is_empty() {
                            self.emit_call_with_args("aot_print_newline", &[]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Immediate(0),
                            });
                            return out_reg;
                        }

                        // Check if single string without options
                        if args.len() == 1 {
                            if let HirExpr::Literal(HirLiteral::String(s)) = &args[0] {
                                let str_idx = self.module.add_string(s);
                                let sym_name = format!("__str_{}", str_idx);
                                let str_reg = self.func.alloc_vreg();
                                self.emit(MachineInstruction::Move {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(
                                        str_reg,
                                    )),
                                    src: MachineOperand::Symbol(sym_name),
                                });
                                let nl_reg = self.func.alloc_vreg();
                                self.emit(MachineInstruction::Move {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(nl_reg)),
                                    src: MachineOperand::Immediate(1),
                                });
                                self.emit_call_with_args("aot_print_str", &[str_reg, nl_reg]);
                                self.emit(MachineInstruction::Move {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(
                                        out_reg,
                                    )),
                                    src: MachineOperand::Immediate(0),
                                });
                                return out_reg;
                            }
                        }

                        // General printing with options / multiple arguments
                        let mut arg_handles = Vec::new();
                        for arg in args {
                            let h = self.lower_to_handle(arg);
                            arg_handles.push(h);
                        }

                        let count = arg_handles.len() as i32;
                        let size = count.max(1) * 8;
                        let slot = self.alloc_stack_slot(size);
                        let base_off = -(slot + size);

                        for (i, h) in arg_handles.iter().enumerate() {
                            let off = base_off + (i as i32 * 8);
                            self.emit(MachineInstruction::Store {
                                dst: MachineOperand::Memory {
                                    base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                                    offset: off,
                                    index: None,
                                },
                                src: MachineOperand::Register(MachineRegister::Virtual(*h)),
                                size: 8,
                            });
                        }

                        let ptr_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(5),
                            )), // RBP
                        });
                        self.emit(MachineInstruction::Sub {
                            dst: MachineOperand::Register(MachineRegister::Virtual(ptr_reg)),
                            src: MachineOperand::Immediate((slot + size) as i64),
                        });

                        let count_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
                            src: MachineOperand::Immediate(count as i64),
                        });

                        let has_opts = matches!(args.last(), Some(HirExpr::ObjectLiteral(_)));
                        let opts_reg = if has_opts {
                            *arg_handles.last().unwrap()
                        } else {
                            let zero_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(zero_reg)),
                                src: MachineOperand::Immediate(0),
                            });
                            zero_reg
                        };

                        self.emit_call_with_args(
                            "aot_print_with_options",
                            &[ptr_reg, count_reg, opts_reg],
                        );

                        if fn_name == "println" && !has_opts {
                            self.emit_call_with_args("aot_print_newline", &[]);
                        }

                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0),
                        });
                        return out_reg;
                    }
                }

                // General function call
                let mut arg_regs = Vec::new();
                for arg in args {
                    let r = self.lower_expression(arg);
                    arg_regs.push(r);
                }

                let target_sym = match &**callee {
                    HirExpr::LoadVar(name) => name.clone(),
                    _ => "unknown_callee".to_string(),
                };

                self.emit_call_with_args(&target_sym, &arg_regs);

                // Retrieve return value from RAX (phys 0)
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });

                out_reg
            }
            HirExpr::AssignTuple(names, value) => {
                if let HirExpr::TupleLiteral(elements) = &**value {
                    let mut regs = Vec::new();
                    for elem in elements {
                        let r = self.lower_expression(elem);
                        regs.push((r, infer_hir_expr_type(elem)));
                    }
                    let out_reg = self.func.alloc_vreg();
                    for (name, (r, ty)) in names.iter().zip(regs) {
                        if let Some((slot, existing_ty)) = self.local_vars.get(name).cloned() {
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(slot),
                                src: MachineOperand::Register(MachineRegister::Virtual(r)),
                            });
                            self.local_vars
                                .insert(name.clone(), (slot, ty.or(existing_ty)));
                        } else {
                            let slot = self.alloc_stack_slot(8);
                            let off = -(slot + 8);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(off),
                                src: MachineOperand::Register(MachineRegister::Virtual(r)),
                            });
                            self.local_vars.insert(name.clone(), (off, ty));
                        }
                    }
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Immediate(0),
                    });
                    out_reg
                } else {
                    let tup_handle = self.lower_to_handle(value);
                    for (i, name) in names.iter().enumerate() {
                        let idx_val_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(idx_val_reg)),
                            src: MachineOperand::Immediate(i as i64),
                        });
                        let idx_handle = self.func.alloc_vreg();
                        self.emit_call_with_args("aot_make_i64", &[idx_val_reg]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(idx_handle)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        let elem_reg = self.func.alloc_vreg();
                        self.emit_call_with_args("aot_get_index", &[tup_handle, idx_handle]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(
                                PhysicalRegister(0),
                            )),
                        });
                        if let Some((slot, existing_ty)) = self.local_vars.get(name).cloned() {
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(slot),
                                src: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
                            });
                            self.local_vars.insert(name.clone(), (slot, existing_ty));
                        } else {
                            let slot = self.alloc_stack_slot(8);
                            let off = -(slot + 8);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::StackSlot(off),
                                src: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
                            });
                            self.local_vars.insert(name.clone(), (off, None));
                        }
                    }
                    tup_handle
                }
            }
            _ => {
                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Immediate(0),
                });
                out_reg
            }
        }
    }
}

/// Lower a single HirFunction into a NativeModule.
pub fn lower_hir_function(
    hir_func: &HirFunction,
    module: &mut NativeModule,
    target: &TargetDescriptor,
) {
    if module.functions.iter().any(|f| f.name == hir_func.name) {
        return;
    }
    let mut func = MachineFunction::new(&hir_func.name);
    func.is_exported = true;

    {
        let mut ctx = FunctionLoweringContext::new(&mut func, module, target);
        let param_regs = ctx.call_conv.arg_registers().to_vec();
        for (idx, (p_name, _, _)) in hir_func.params.iter().enumerate() {
            let slot = ctx.alloc_stack_slot(8);
            if idx < param_regs.len() {
                ctx.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(-slot),
                    src: MachineOperand::Register(MachineRegister::Physical(param_regs[idx])),
                });
            } else {
                let shadow = ctx.call_conv.shadow_space() as i32;
                let stack_off = 16 + shadow + 8 * (idx - param_regs.len()) as i32;
                ctx.emit(MachineInstruction::Load {
                    dst: MachineOperand::StackSlot(-slot),
                    src: MachineOperand::Memory {
                        base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                        offset: stack_off,
                        index: None,
                    },
                    size: 8,
                });
            }
            ctx.local_vars
                .insert(p_name.clone(), (-slot, None::<HirType>));
        }

        for stmt in hir_func.body.iter() {
            ctx.lower_statement(stmt);
        }

        // Ensure last instruction is return
        let last_is_ret = ctx
            .func
            .blocks
            .last()
            .and_then(|b| b.instructions.last())
            .map_or(false, |i| matches!(i, MachineInstruction::Return));
        if !last_is_ret {
            ctx.emit(MachineInstruction::Return);
        }
    }

    module.add_function(func);
}

/// Lower entire HirModule to NativeModule.
pub fn lower_hir_module(hir: &HirModule, target: &TargetDescriptor) -> NativeModule {
    let mut module = NativeModule::new("main_module");

    // Lower user functions
    for hir_func in &hir.functions {
        lower_hir_function(hir_func, &mut module, target);
    }

    // If top-level statements exist, lower them into `main`
    if !hir.statements.is_empty() {
        let mut main_func = MachineFunction::new("main");
        main_func.is_exported = true;
        {
            let mut ctx = FunctionLoweringContext::new(&mut main_func, &mut module, target);
            for stmt in &hir.statements {
                ctx.lower_statement(stmt);
            }
            let last_is_ret = ctx
                .func
                .blocks
                .last()
                .and_then(|b| b.instructions.last())
                .map_or(false, |i| matches!(i, MachineInstruction::Return));
            if !last_is_ret {
                ctx.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                    src: MachineOperand::Immediate(0),
                });
                ctx.emit(MachineInstruction::Return);
            }
        }
        module.add_function(main_func);
    } else if !module.functions.iter().any(|f| f.name == "main") {
        let mut main_func = MachineFunction::new("main");
        main_func.is_exported = true;
        main_func.entry_block_mut().push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            src: MachineOperand::Immediate(0),
        });
        main_func.entry_block_mut().push(MachineInstruction::Return);
        module.add_function(main_func);
    }

    module
}
