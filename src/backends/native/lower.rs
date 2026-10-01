//! Native HIR to Machine IR Lowering Engine.
//!
//! Transforms typed Adesh HIR programs directly into `adesh_codegen::machine_ir::NativeModule`
//! with physical/virtual registers, stack frame allocation, C ABI runtime bindings,
//! string pool `.rodata` emission, and branch/call relocation generation.

use crate::ir::hir::{BinOp, HirExpr, HirLiteral, HirModule, HirStmt, UnaryOp};
use adesh_codegen::calling_convention::{
    CallingConvention, SystemVX64CallingConvention, WindowsX64CallingConvention,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, VirtualRegister,
};
use adesh_object::{OperatingSystem, TargetDescriptor};
use std::collections::HashMap;

/// Lowering context for a single function.
pub struct FunctionLoweringContext<'a> {
    pub func: &'a mut MachineFunction,
    pub module: &'a mut NativeModule,
    pub target: &'a TargetDescriptor,
    pub call_conv: Box<dyn CallingConvention>,
    pub local_vars: HashMap<String, (i32, Option<VirtualRegister>)>, // (stack_offset, last_vreg)
    pub current_stack_offset: i32,
    pub current_block_id: u32,
    pub label_counter: u32,
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
        HirExpr::BinaryOp(lhs, op, rhs) => {
            let l = eval_const_expr(lhs)?;
            let r = eval_const_expr(rhs)?;
            match op {
                BinOp::Add => Some(l.wrapping_add(r)),
                BinOp::Sub => Some(l.wrapping_sub(r)),
                BinOp::Mul => Some(l.wrapping_mul(r)),
                BinOp::Div => (r != 0).then(|| l.wrapping_div(r)),
                BinOp::Mod => (r != 0).then(|| l.wrapping_rem(r)),
                BinOp::BitAnd => Some(l & r),
                BinOp::BitOr => Some(l | r),
                BinOp::BitXor => Some(l ^ r),
                BinOp::ShiftLeft => (r >= 0 && r < 64).then(|| l << r),
                BinOp::ShiftRight => (r >= 0 && r < 64).then(|| l >> r),
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

    pub fn lower_statement(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Let { name, init, .. } => {
                let slot = self.alloc_stack_slot(8);
                if let Some(expr) = init {
                    let vreg = self.lower_expression(expr);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::StackSlot(-slot),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                    self.local_vars.insert(name.clone(), (-slot, Some(vreg)));
                } else {
                    self.local_vars.insert(name.clone(), (-slot, None));
                }
            }
            HirStmt::Assign { target, value, .. } => {
                let val_vreg = self.lower_expression(value);
                if let HirExpr::LoadVar(name) = target {
                    if let Some(&(slot, _)) = self.local_vars.get(name) {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::StackSlot(slot),
                            src: MachineOperand::Register(MachineRegister::Virtual(val_vreg)),
                        });
                        self.local_vars.insert(name.clone(), (slot, Some(val_vreg)));
                    } else {
                        let slot = self.alloc_stack_slot(8);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::StackSlot(-slot),
                            src: MachineOperand::Register(MachineRegister::Virtual(val_vreg)),
                        });
                        self.local_vars
                            .insert(name.clone(), (-slot, Some(val_vreg)));
                    }
                }
            }
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
            }
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.lower_statement(s);
                }
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
                    _ => {}
                }
                out_reg
            }
            HirExpr::Call(callee, args, _) => {
                let out_reg = self.func.alloc_vreg();

                // Special-case print / println
                if let HirExpr::LoadVar(fn_name) = &**callee {
                    if fn_name == "print" || fn_name == "println" {
                        // Print each argument with the type-specific native runtime call
                        for arg in args {
                            let arg_vreg = self.lower_expression(arg);
                            // Setup 1st argument (RCX on Windows, RDI on SysV)
                            let arg0_reg =
                                if self.target.operating_system == OperatingSystem::Windows {
                                    PhysicalRegister(1) // RCX
                                } else {
                                    PhysicalRegister(7) // RDI
                                };

                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Physical(arg0_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(arg_vreg)),
                            });

                            let print_sym = match arg {
                                HirExpr::Literal(HirLiteral::String(_)) => "adesh_print_str",
                                HirExpr::Literal(HirLiteral::Float(_))
                                | HirExpr::Literal(HirLiteral::F64(_))
                                | HirExpr::Literal(HirLiteral::F32(_)) => "adesh_print_f64",
                                HirExpr::Literal(HirLiteral::Bool(_)) => "adesh_print_bool",
                                _ => "adesh_print_i64",
                            };

                            if !self.module.imports.contains(&print_sym.to_string()) {
                                self.module.imports.push(print_sym.to_string());
                            }

                            self.emit(MachineInstruction::Call {
                                target: MachineOperand::Symbol(print_sym.to_string()),
                                num_args: 1,
                            });
                        }

                        if fn_name == "println" {
                            let newline_sym = "adesh_print_newline";
                            if !self.module.imports.contains(&newline_sym.to_string()) {
                                self.module.imports.push(newline_sym.to_string());
                            }
                            self.emit(MachineInstruction::Call {
                                target: MachineOperand::Symbol(newline_sym.to_string()),
                                num_args: 0,
                            });
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

                let param_regs = self.call_conv.arg_registers().to_vec();
                for (i, &r) in arg_regs.iter().enumerate() {
                    if i < param_regs.len() {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Physical(param_regs[i])),
                            src: MachineOperand::Register(MachineRegister::Virtual(r)),
                        });
                    }
                }

                let target_sym = match &**callee {
                    HirExpr::LoadVar(name) => name.clone(),
                    _ => "unknown_callee".to_string(),
                };

                self.emit(MachineInstruction::Call {
                    target: MachineOperand::Symbol(target_sym),
                    num_args: args.len(),
                });

                // Retrieve return value from RAX (phys 0)
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });

                out_reg
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

/// Lower entire HirModule to NativeModule.
pub fn lower_hir_module(hir: &HirModule, target: &TargetDescriptor) -> NativeModule {
    let mut module = NativeModule::new("main_module");

    // Lower user functions
    for hir_func in &hir.functions {
        let mut func = MachineFunction::new(&hir_func.name);
        func.is_exported = true;

        {
            let mut ctx = FunctionLoweringContext::new(&mut func, &mut module, target);
            // Assign parameters to stack slots
            let param_regs = ctx.call_conv.arg_registers().to_vec();
            for (idx, (p_name, _, _)) in hir_func.params.iter().enumerate() {
                let slot = ctx.alloc_stack_slot(8);
                if idx < param_regs.len() {
                    ctx.emit(MachineInstruction::Move {
                        dst: MachineOperand::StackSlot(-slot),
                        src: MachineOperand::Register(MachineRegister::Physical(param_regs[idx])),
                    });
                }
                ctx.local_vars.insert(p_name.clone(), (-slot, None));
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
