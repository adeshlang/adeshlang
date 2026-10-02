//! Native HIR to Machine IR Lowering Engine.
//!
//! Transforms typed Adesh HIR programs directly into `adesh_codegen::machine_ir::NativeModule`
//! with physical/virtual registers, stack frame allocation, C ABI runtime bindings,
//! string pool `.rodata` emission, composite data construction (objects, arrays, tuples, sets),
//! rich pretty-printing options, and branch/call relocation generation.

use crate::parsing::hir::{
    BinOp, HirClass, HirExpr, HirFunction, HirLiteral, HirModule, HirPattern, HirStmt, HirType,
    UnaryOp,
};
use adesh_codegen::calling_convention::{
    ArgumentLocation, CallingConvention, MoveLocation, MoveOperation, ParallelMoveResolver,
    SystemVX64CallingConvention, WindowsX64CallingConvention, resolve_call_arguments,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
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

pub fn is_float_type(ty: Option<&HirType>) -> bool {
    matches!(ty, Some(HirType::Float | HirType::F64 | HirType::F32))
}

pub fn is_f32_type(ty: Option<&HirType>) -> bool {
    matches!(ty, Some(HirType::F32))
}

/// Lowering context for a single function.
pub struct FunctionLoweringContext<'a> {
    pub func: &'a mut MachineFunction,
    pub module: &'a mut NativeModule,
    pub target: &'a TargetDescriptor,
    pub call_conv: Box<dyn CallingConvention>,
    pub local_vars: HashMap<String, (i32, Option<HirType>)>, // (stack_offset, type)
    pub loop_stack: Vec<LoopContext>,
    pub defer_stack: Vec<HirStmt>,
    pub current_stack_offset: i32,
    pub current_block_id: u32,
    pub label_counter: u32,
    pub func_ret_type: Option<HirType>,
    pub fn_signatures: HashMap<String, (Vec<HirType>, Option<HirType>)>,
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
        HirExpr::BinaryOp(lhs, op, rhs) => {
            if matches!(
                op,
                BinOp::Eq
                    | BinOp::StrictEq
                    | BinOp::Ne
                    | BinOp::StrictNe
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or
                    | BinOp::In
            ) {
                return Some(HirType::Bool);
            }
            let l = infer_hir_expr_type(lhs);
            let r = infer_hir_expr_type(rhs);
            if matches!(l, Some(HirType::Float | HirType::F64))
                || matches!(r, Some(HirType::Float | HirType::F64))
            {
                return Some(HirType::Float);
            }
            if matches!(l, Some(HirType::F32)) || matches!(r, Some(HirType::F32)) {
                return Some(HirType::F32);
            }
            l.or(r)
        }
        HirExpr::UnaryOp(op, inner) => {
            if matches!(op, UnaryOp::Not) {
                return Some(HirType::Bool);
            }
            if matches!(op, UnaryOp::Typeof) {
                return Some(HirType::String);
            }
            infer_hir_expr_type(inner)
        }
        HirExpr::Cast(_, ty) => Some(ty.clone()),
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
                    "float"
                        | "f64"
                        | "number"
                        | "Number"
                        | "clock"
                        | "sin"
                        | "cos"
                        | "tan"
                        | "sqrt"
                        | "exp"
                        | "log"
                        | "log10"
                        | "pow"
                        | "floor"
                        | "ceil"
                        | "round"
                        | "fabs"
                        | "fmod"
                        | "asin"
                        | "acos"
                        | "atan"
                        | "atan2"
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
            defer_stack: Vec::new(),
            current_stack_offset: 16, // after saved RBP and return address
            current_block_id: 0,
            label_counter: 0,
            func_ret_type: None,
            fn_signatures: HashMap::new(),
        }
    }

    pub fn with_signatures(
        func: &'a mut MachineFunction,
        module: &'a mut NativeModule,
        target: &'a TargetDescriptor,
        func_ret_type: Option<HirType>,
        fn_signatures: HashMap<String, (Vec<HirType>, Option<HirType>)>,
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
            defer_stack: Vec::new(),
            current_stack_offset: 16,
            current_block_id: 0,
            label_counter: 0,
            func_ret_type,
            fn_signatures,
        }
    }

    pub fn is_expr_float(&self, expr: &HirExpr, vreg: VirtualRegister) -> bool {
        if self.func.vreg_class(vreg) == RegisterClass::Float {
            return true;
        }
        if let HirExpr::LoadVar(name) = expr {
            if let Some(&(_, Some(ref ty))) = self.local_vars.get(name) {
                return is_float_type(Some(ty));
            }
        }
        is_float_type(infer_hir_expr_type(expr).as_ref())
    }

    pub fn is_expr_f32(&self, expr: &HirExpr) -> bool {
        if let HirExpr::LoadVar(name) = expr {
            if let Some(&(_, Some(ref ty))) = self.local_vars.get(name) {
                return is_f32_type(Some(ty));
            }
        }
        is_f32_type(infer_hir_expr_type(expr).as_ref())
    }

    pub fn run_defers(&mut self) {
        let defers = self.defer_stack.clone();
        for stmt in defers.iter().rev() {
            self.lower_statement(stmt);
        }
    }

    pub fn lower_literal(&mut self, lit: &HirLiteral) -> VirtualRegister {
        match lit {
            HirLiteral::Float(f) | HirLiteral::F64(f) => {
                let out_reg = self.func.alloc_fp_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::FloatImmediate(*f),
                });
                return out_reg;
            }
            HirLiteral::F32(f) => {
                let out_reg = self.func.alloc_fp_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::FloatImmediate(*f as f64),
                });
                return out_reg;
            }
            _ => {}
        }
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
            HirLiteral::BigInt(bi) => {
                use num_traits::ToPrimitive;
                let val = bi.to_u64().unwrap_or(0) as i64;
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Immediate(val),
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

    pub fn lower_pattern_check(
        &mut self,
        target_reg: &VirtualRegister,
        pattern: &HirPattern,
        match_lbl: &str,
        fail_lbl: &str,
    ) {
        match pattern {
            HirPattern::Wildcard | HirPattern::Variable(_) => {
                self.emit(MachineInstruction::Branch {
                    target: match_lbl.to_string(),
                });
            }
            HirPattern::Literal(lit) => {
                let lit_reg = self.lower_literal(lit);
                self.emit(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(*target_reg)),
                    rhs: MachineOperand::Register(MachineRegister::Virtual(lit_reg)),
                });
                self.emit(MachineInstruction::BranchCc {
                    cc: ConditionCode::Equal,
                    target: match_lbl.to_string(),
                });
                self.emit(MachineInstruction::Branch {
                    target: fail_lbl.to_string(),
                });
            }
            HirPattern::Or(p1, p2) => {
                let try_p2_lbl = self.fresh_label("match_or_p2");
                self.lower_pattern_check(target_reg, p1, match_lbl, &try_p2_lbl);

                let p2_id = self.func.create_block(&try_p2_lbl);
                self.current_block_id = p2_id;
                self.lower_pattern_check(target_reg, p2, match_lbl, fail_lbl);
            }
            _ => {
                self.emit(MachineInstruction::Branch {
                    target: match_lbl.to_string(),
                });
            }
        }
    }

    pub fn bind_pattern_variables(&mut self, pattern: &HirPattern, val_reg: VirtualRegister) {
        match pattern {
            HirPattern::Variable(var_name) => {
                let slot = self.alloc_stack_slot(8);
                let off = -(slot + 8);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(off),
                    src: MachineOperand::Register(MachineRegister::Virtual(val_reg)),
                });
                self.local_vars.insert(var_name.clone(), (off, None));
            }
            HirPattern::Or(p1, p2) => {
                self.bind_pattern_variables(p1, val_reg);
                self.bind_pattern_variables(p2, val_reg);
            }
            _ => {}
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

    /// Emit a C-ABI call sequence using the production-grade ParallelMoveResolver:
    /// parallel argument shuffling for register parameters (GPR and Float/XMM) and stack parameters,
    /// shadow space management, and 16-byte stack alignment.
    pub fn emit_call_with_args(&mut self, symbol: &str, args: &[VirtualRegister]) {
        let typed_args: Vec<(VirtualRegister, RegisterClass)> = args
            .iter()
            .map(|&v| (v, self.func.vreg_class(v)))
            .collect();
        self.emit_call_with_typed_args(symbol, &typed_args);
    }

    pub fn emit_call_with_typed_args(
        &mut self,
        symbol: &str,
        args: &[(VirtualRegister, RegisterClass)],
    ) {
        if !self.module.functions.iter().any(|f| f.name == symbol)
            && !self.module.imports.contains(&symbol.to_string())
        {
            self.module.imports.push(symbol.to_string());
        }

        let (moves, total) = resolve_call_arguments(
            self.call_conv.as_ref(),
            args,
            PhysicalRegister(4), // RSP
        )
        .expect("call argument resolution");

        if total > 0 {
            // Reserve outgoing argument area
            self.emit(MachineInstruction::Sub {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total as i64),
            });
        }

        // Emit resolved parallel moves for arguments
        for inst in moves {
            self.emit(inst);
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

    /// Emit an indirect call sequence through a register or memory location.
    pub fn emit_indirect_call_with_args(
        &mut self,
        callee_vreg: VirtualRegister,
        args: &[VirtualRegister],
    ) {
        let typed_args: Vec<(VirtualRegister, RegisterClass)> = args
            .iter()
            .map(|&v| (v, self.func.vreg_class(v)))
            .collect();
        self.emit_indirect_call_with_typed_args(callee_vreg, &typed_args);
    }

    pub fn emit_indirect_call_with_typed_args(
        &mut self,
        callee_vreg: VirtualRegister,
        args: &[(VirtualRegister, RegisterClass)],
    ) {
        let (moves, total) = resolve_call_arguments(
            self.call_conv.as_ref(),
            args,
            PhysicalRegister(4), // RSP
        )
        .expect("indirect call argument resolution");

        if total > 0 {
            self.emit(MachineInstruction::Sub {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total as i64),
            });
        }

        for inst in moves {
            self.emit(inst);
        }

        self.emit(MachineInstruction::Call {
            target: MachineOperand::Register(MachineRegister::Virtual(callee_vreg)),
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
                HirExpr::Deref(ptr_expr) => {
                    let ptr_reg = self.lower_expression(ptr_expr);
                    let val_reg = self.lower_expression(value);
                    self.emit(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Virtual(ptr_reg),
                            offset: 0,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(val_reg)),
                        size: 8,
                    });
                }
                _ => {
                    self.lower_expression(value);
                }
            },
            HirStmt::Expr(expr) => {
                self.lower_expression(expr);
            }
            HirStmt::Return(expr_opt) => {
                self.run_defers();
                if let Some(expr) = expr_opt {
                    let vreg = self.lower_expression(expr);
                    let is_fp = self.func.vreg_class(vreg) == RegisterClass::Float
                        || is_float_type(self.func_ret_type.as_ref());
                    let ret_phys = if is_fp {
                        PhysicalRegister::xmm(0)
                    } else {
                        PhysicalRegister::gpr(0)
                    };
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(ret_phys)),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                }
                self.emit(MachineInstruction::Return);
            }
            HirStmt::Throw(expr) => {
                let handle = self.lower_to_handle(expr);
                self.emit_call_with_args("aot_throw_exception", &[handle]);
                self.run_defers();
                self.emit(MachineInstruction::Return);
            }
            HirStmt::Defer(stmt) => {
                self.defer_stack.push(*stmt.clone());
            }
            HirStmt::Region { body, .. } => {
                self.lower_statement(body);
            }
            HirStmt::Unsafe(body) => {
                self.lower_statement(body);
            }
            HirStmt::ClassDef(cls) => {
                for m in &cls.methods {
                    let mut params = vec![("this".to_string(), Some(HirType::Instance(cls.name.clone())), None)];
                    params.extend(m.params.iter().map(|(n, t)| (n.clone(), t.clone(), None)));
                    let func = HirFunction {
                        name: format!("{}_{}", cls.name, m.name),
                        params,
                        body: m.body.clone(),
                        ret_type: m.ret_type.clone(),
                        is_async: m.is_async,
                        decorators: Vec::new(),
                        is_exported: true,
                        move_params: Vec::new(),
                        is_test: false,
                        test_ignore: false,
                        test_expect_fail: false,
                        test_timeout: None,
                        is_unsafe: m.is_unsafe,
                    };
                    lower_hir_function(&func, self.module, self.target);
                }
                for m in &cls.static_methods {
                    let func = HirFunction {
                        name: format!("{}_{}", cls.name, m.name),
                        params: m.params.iter().map(|(n, t)| (n.clone(), t.clone(), None)).collect(),
                        body: m.body.clone(),
                        ret_type: m.ret_type.clone(),
                        is_async: m.is_async,
                        decorators: Vec::new(),
                        is_exported: true,
                        move_params: Vec::new(),
                        is_test: false,
                        test_ignore: false,
                        test_expect_fail: false,
                        test_timeout: None,
                        is_unsafe: m.is_unsafe,
                    };
                    lower_hir_function(&func, self.module, self.target);
                }
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
                match lit {
                    HirLiteral::Float(f) | HirLiteral::F64(f) => {
                        let out_reg = self.func.alloc_fp_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::FloatImmediate(*f),
                        });
                        return out_reg;
                    }
                    HirLiteral::F32(f) => {
                        let out_reg = self.func.alloc_fp_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::FloatImmediate(*f as f64),
                        });
                        return out_reg;
                    }
                    _ => {}
                }
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
                let is_fp = if let Some(&(_, Some(ref ty))) = self.local_vars.get(name) {
                    is_float_type(Some(ty))
                } else {
                    false
                };
                let out_reg = if is_fp {
                    self.func.alloc_fp_vreg()
                } else {
                    self.func.alloc_vreg()
                };
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
                let mut l_reg = self.lower_expression(lhs);
                let mut r_reg = self.lower_expression(rhs);
                let l_is_fp = self.is_expr_float(lhs, l_reg);
                let r_is_fp = self.is_expr_float(rhs, r_reg);
                let is_fp = l_is_fp || r_is_fp;

                if !is_fp {
                    if let Some(folded_val) = eval_const_expr(expr) {
                        let out_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(folded_val),
                        });
                        return out_reg;
                    }
                }

                if is_fp {
                    let is_single = self.is_expr_f32(lhs) && self.is_expr_f32(rhs);

                    // If one operand is integer and one is float, cast the integer to float
                    if !l_is_fp {
                        let fp_l = self.func.alloc_fp_vreg();
                        self.emit(MachineInstruction::FCvtIntToFloat {
                            dst: MachineOperand::Register(MachineRegister::Virtual(fp_l)),
                            src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            is_f64: !is_single,
                            is_signed: true,
                        });
                        l_reg = fp_l;
                    }
                    if !r_is_fp {
                        let fp_r = self.func.alloc_fp_vreg();
                        self.emit(MachineInstruction::FCvtIntToFloat {
                            dst: MachineOperand::Register(MachineRegister::Virtual(fp_r)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                            is_f64: !is_single,
                            is_signed: true,
                        });
                        r_reg = fp_r;
                    }

                    let size = if is_single { 4 } else { 8 };
                    match op {
                        BinOp::Add => {
                            let out_reg = self.func.alloc_fp_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            });
                            self.emit(MachineInstruction::FAdd {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            return out_reg;
                        }
                        BinOp::Sub => {
                            let out_reg = self.func.alloc_fp_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            });
                            self.emit(MachineInstruction::FSub {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            return out_reg;
                        }
                        BinOp::Mul => {
                            let out_reg = self.func.alloc_fp_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            });
                            self.emit(MachineInstruction::FMul {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            return out_reg;
                        }
                        BinOp::Div | BinOp::IntDiv => {
                            let out_reg = self.func.alloc_fp_vreg();
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            });
                            self.emit(MachineInstruction::FDiv {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            return out_reg;
                        }
                        BinOp::Eq | BinOp::StrictEq => {
                            let out_reg = self.func.alloc_vreg();
                            let tmp_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::FCmp {
                                lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                                rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                cc: ConditionCode::Equal,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                                cc: ConditionCode::NotParity,
                            });
                            self.emit(MachineInstruction::And {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                            });
                            return out_reg;
                        }
                        BinOp::Ne | BinOp::StrictNe => {
                            let out_reg = self.func.alloc_vreg();
                            let tmp_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::FCmp {
                                lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                                rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                cc: ConditionCode::NotEqual,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                                cc: ConditionCode::Parity,
                            });
                            self.emit(MachineInstruction::Or {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                            });
                            return out_reg;
                        }
                        BinOp::Lt => {
                            let out_reg = self.func.alloc_vreg();
                            let tmp_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::FCmp {
                                lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                                rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                cc: ConditionCode::Below,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                                cc: ConditionCode::NotParity,
                            });
                            self.emit(MachineInstruction::And {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                            });
                            return out_reg;
                        }
                        BinOp::Le => {
                            let out_reg = self.func.alloc_vreg();
                            let tmp_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::FCmp {
                                lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                                rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                cc: ConditionCode::BelowOrEqual,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                                cc: ConditionCode::NotParity,
                            });
                            self.emit(MachineInstruction::And {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                            });
                            return out_reg;
                        }
                        BinOp::Gt => {
                            let out_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::FCmp {
                                lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                                rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                cc: ConditionCode::Above,
                            });
                            return out_reg;
                        }
                        BinOp::Ge => {
                            let out_reg = self.func.alloc_vreg();
                            let tmp_reg = self.func.alloc_vreg();
                            self.emit(MachineInstruction::FCmp {
                                lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                                rhs: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                                size,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                cc: ConditionCode::AboveOrEqual,
                            });
                            self.emit(MachineInstruction::SetCc {
                                dst: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                                cc: ConditionCode::NotParity,
                            });
                            self.emit(MachineInstruction::And {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(tmp_reg)),
                            });
                            return out_reg;
                        }
                        _ => {}
                    }
                }

                let out_reg = self.func.alloc_vreg();

                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                });

                match op {
                    BinOp::Add => {
                        let l_is_str = matches!(infer_hir_expr_type(lhs), Some(HirType::String));
                        let r_is_str = matches!(infer_hir_expr_type(rhs), Some(HirType::String));
                        if l_is_str || r_is_str {
                            let l_h = self.lower_to_handle(lhs);
                            let r_h = self.lower_to_handle(rhs);
                            self.emit_call_with_args("aot_string_concat", &[l_h, r_h]);
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                            });
                        } else {
                            self.emit(MachineInstruction::Add {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(r_reg)),
                            });
                        }
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
                    BinOp::In => {
                        let l_h = self.lower_to_handle(lhs);
                        let r_h = self.lower_to_handle(rhs);
                        self.emit_call_with_args("aot_contains", &[r_h, l_h]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                        });
                    }
                    BinOp::NullCoalesce => {
                        let null_lbl = self.fresh_label("coalesce_null");
                        let end_lbl = self.fresh_label("coalesce_end");

                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                            rhs: MachineOperand::Immediate(0),
                        });
                        self.emit(MachineInstruction::BranchCc {
                            cc: ConditionCode::Equal,
                            target: null_lbl.clone(),
                        });

                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(l_reg)),
                        });
                        self.emit(MachineInstruction::Branch {
                            target: end_lbl.clone(),
                        });

                        let null_id = self.func.create_block(&null_lbl);
                        self.current_block_id = null_id;
                        let r_val = self.lower_expression(rhs);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(r_val)),
                        });

                        let end_id = self.func.create_block(&end_lbl);
                        self.current_block_id = end_id;
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
                let is_fp = self.is_expr_float(inner, in_reg);

                if is_fp && matches!(op, UnaryOp::Neg) {
                    let is_single = self.is_expr_f32(inner);
                    let out_reg = self.func.alloc_fp_vreg();
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Register(MachineRegister::Virtual(in_reg)),
                    });
                    self.emit(MachineInstruction::FNeg {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        size: if is_single { 4 } else { 8 },
                    });
                    return out_reg;
                }

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

                // General function call (direct or indirect)
                let mut arg_regs = Vec::new();
                for arg in args {
                    let r = self.lower_expression(arg);
                    arg_regs.push(r);
                }

                if let HirExpr::LoadVar(target_sym) = &**callee {
                    if self.local_vars.contains_key(target_sym) {
                        let callee_reg = self.lower_expression(callee);
                        self.emit_indirect_call_with_args(callee_reg, &arg_regs);
                    } else {
                        self.emit_call_with_args(target_sym, &arg_regs);
                    }
                } else {
                    let callee_reg = self.lower_expression(callee);
                    self.emit_indirect_call_with_args(callee_reg, &arg_regs);
                }

                let is_ret_fp = if let HirExpr::LoadVar(fn_name) = &**callee {
                    if let Some((_, ret_ty)) = self.fn_signatures.get(fn_name) {
                        is_float_type(ret_ty.as_ref())
                    } else {
                        false
                    }
                } else {
                    false
                };

                let out_reg = if is_ret_fp {
                    self.func.alloc_fp_vreg()
                } else {
                    self.func.alloc_vreg()
                };

                let ret_phys = if is_ret_fp {
                    PhysicalRegister::xmm(0)
                } else {
                    PhysicalRegister::gpr(0)
                };

                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(ret_phys)),
                });

                out_reg
            }
            HirExpr::Match(target, arms) => {
                let target_reg = self.lower_expression(target);
                let target_slot = self.alloc_stack_slot(8);
                let target_off = -(target_slot + 8);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(target_off),
                    src: MachineOperand::Register(MachineRegister::Virtual(target_reg)),
                });

                let match_end_lbl = self.fresh_label("match_end");
                let out_reg = self.func.alloc_vreg();

                for (pattern, arm_expr) in arms {
                    let arm_body_lbl = self.fresh_label("match_body");
                    let next_arm_lbl = self.fresh_label("match_next");

                    self.lower_pattern_check(&target_reg, pattern, &arm_body_lbl, &next_arm_lbl);

                    let body_id = self.func.create_block(&arm_body_lbl);
                    self.current_block_id = body_id;
                    self.bind_pattern_variables(pattern, target_reg);
                    let arm_res = self.lower_expression(arm_expr);
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Register(MachineRegister::Virtual(arm_res)),
                    });
                    self.emit(MachineInstruction::Branch {
                        target: match_end_lbl.clone(),
                    });

                    let next_id = self.func.create_block(&next_arm_lbl);
                    self.current_block_id = next_id;
                }

                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Immediate(0),
                });

                let end_id = self.func.create_block(&match_end_lbl);
                self.current_block_id = end_id;

                out_reg
            }
            HirExpr::DictLiteral(entries) => {
                let dict_handle = self.func.alloc_vreg();
                self.emit_call_with_args("aot_make_dict", &[]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(dict_handle)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                for (k, v) in entries {
                    let k_h = self.lower_to_handle(k);
                    let v_h = self.lower_to_handle(v);
                    self.emit_call_with_args("aot_set_index", &[dict_handle, k_h, v_h]);
                }
                dict_handle
            }
            HirExpr::NewInstance(cls_name, args) => {
                let zero_vreg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(zero_vreg)),
                    src: MachineOperand::Immediate(0),
                });
                let obj_handle = self.func.alloc_vreg();
                self.emit_call_with_args("aot_make_object", &[zero_vreg, zero_vreg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(obj_handle)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                let mut arg_regs = vec![obj_handle];
                for arg in args {
                    arg_regs.push(self.lower_expression(arg));
                }
                let init_func = format!("{}_init", cls_name);
                if self.module.functions.iter().any(|f| f.name == init_func) {
                    self.emit_call_with_args(&init_func, &arg_regs);
                }
                obj_handle
            }
            HirExpr::Range(start, end, inclusive) => {
                let s_reg = self.lower_expression(start);
                let e_reg = self.lower_expression(end);
                let inc_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(inc_reg)),
                    src: MachineOperand::Immediate(if *inclusive { 1 } else { 0 }),
                });
                let out_reg = self.func.alloc_vreg();
                self.emit_call_with_args("aot_make_range", &[s_reg, e_reg, inc_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::Borrow(inner, _)
            | HirExpr::BorrowImmut(inner)
            | HirExpr::BorrowMut(inner) => {
                let out_reg = self.func.alloc_vreg();
                if let HirExpr::LoadVar(name) = &**inner {
                    let slot_opt = self.local_vars.get(name).map(|(s, _)| *s);
                    if let Some(slot) = slot_opt {
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))), // RBP
                        });
                        self.emit(MachineInstruction::Add {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(slot as i64),
                        });
                        return out_reg;
                    }
                }
                let val_reg = self.lower_expression(inner);
                let slot = self.alloc_stack_slot(8);
                let off = -(slot + 8);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::StackSlot(off),
                    src: MachineOperand::Register(MachineRegister::Virtual(val_reg)),
                });
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))), // RBP
                });
                self.emit(MachineInstruction::Add {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Immediate(off as i64),
                });
                out_reg
            }
            HirExpr::Deref(inner) => {
                let ptr_reg = self.lower_expression(inner);
                let out_reg = self.func.alloc_vreg();
                self.emit(MachineInstruction::Load {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Memory {
                        base: MachineRegister::Virtual(ptr_reg),
                        offset: 0,
                        index: None,
                    },
                    size: 8,
                });
                out_reg
            }
            HirExpr::Alloc(_ty, size_expr) => {
                let size_reg = self.lower_expression(size_expr);
                let out_reg = self.func.alloc_vreg();
                self.emit_call_with_args("aot_alloc", &[size_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::Free(ptr_expr) => {
                let ptr_reg = self.lower_expression(ptr_expr);
                let out_reg = self.func.alloc_vreg();
                self.emit_call_with_args("aot_free", &[ptr_reg]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Immediate(0),
                });
                out_reg
            }
            HirExpr::Cast(inner, target_ty) => {
                let inner_reg = self.lower_expression(inner);
                let inner_is_fp = self.is_expr_float(inner, inner_reg);
                let inner_is_f32 = self.is_expr_f32(inner);

                match target_ty {
                    HirType::Float | HirType::F64 => {
                        let out_reg = self.func.alloc_fp_vreg();
                        if inner_is_fp {
                            if inner_is_f32 {
                                self.emit(MachineInstruction::FCvtFloatToFloat {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                    src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                    to_f64: true,
                                });
                            } else {
                                self.emit(MachineInstruction::Move {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                    src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                });
                            }
                        } else {
                            self.emit(MachineInstruction::FCvtIntToFloat {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                is_f64: true,
                                is_signed: true,
                            });
                        }
                        out_reg
                    }
                    HirType::F32 => {
                        let out_reg = self.func.alloc_fp_vreg();
                        if inner_is_fp {
                            if !inner_is_f32 {
                                self.emit(MachineInstruction::FCvtFloatToFloat {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                    src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                    to_f64: false,
                                });
                            } else {
                                self.emit(MachineInstruction::Move {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                    src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                });
                            }
                        } else {
                            self.emit(MachineInstruction::FCvtIntToFloat {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                is_f64: false,
                                is_signed: true,
                            });
                        }
                        out_reg
                    }
                    HirType::I64 | HirType::Int => {
                        let out_reg = self.func.alloc_vreg();
                        if inner_is_fp {
                            self.emit(MachineInstruction::FCvtFloatToInt {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                is_f64: !inner_is_f32,
                                is_signed: true,
                            });
                        } else {
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                            });
                        }
                        out_reg
                    }
                    HirType::I32 | HirType::U32 => {
                        let out_reg = self.func.alloc_vreg();
                        if inner_is_fp {
                            self.emit(MachineInstruction::FCvtFloatToInt {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                is_f64: !inner_is_f32,
                                is_signed: true,
                            });
                        } else {
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                            });
                        }
                        self.emit(MachineInstruction::And {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0xFFFFFFFF),
                        });
                        out_reg
                    }
                    HirType::I16 | HirType::U16 => {
                        let out_reg = self.func.alloc_vreg();
                        if inner_is_fp {
                            self.emit(MachineInstruction::FCvtFloatToInt {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                is_f64: !inner_is_f32,
                                is_signed: true,
                            });
                        } else {
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                            });
                        }
                        self.emit(MachineInstruction::And {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0xFFFF),
                        });
                        out_reg
                    }
                    HirType::I8 | HirType::U8 => {
                        let out_reg = self.func.alloc_vreg();
                        if inner_is_fp {
                            self.emit(MachineInstruction::FCvtFloatToInt {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                                is_f64: !inner_is_f32,
                                is_signed: true,
                            });
                        } else {
                            self.emit(MachineInstruction::Move {
                                dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                                src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                            });
                        }
                        self.emit(MachineInstruction::And {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Immediate(0xFF),
                        });
                        out_reg
                    }
                    HirType::String => {
                        let out_reg = self.func.alloc_vreg();
                        let h = self.lower_to_handle(inner);
                        self.emit_call_with_args("aot_to_string", &[h]);
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                        });
                        out_reg
                    }
                    HirType::Bool => {
                        let out_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Compare {
                            lhs: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                            rhs: MachineOperand::Immediate(0),
                        });
                        self.emit(MachineInstruction::SetCc {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            cc: ConditionCode::NotEqual,
                        });
                        out_reg
                    }
                    _ => {
                        let out_reg = self.func.alloc_vreg();
                        self.emit(MachineInstruction::Move {
                            dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                            src: MachineOperand::Register(MachineRegister::Virtual(inner_reg)),
                        });
                        out_reg
                    }
                }
            }
            HirExpr::OptionalGet(target, field) => {
                let target_reg = self.lower_expression(target);
                let out_reg = self.func.alloc_vreg();
                let null_lbl = self.fresh_label("opt_null");
                let end_lbl = self.fresh_label("opt_end");

                self.emit(MachineInstruction::Compare {
                    lhs: MachineOperand::Register(MachineRegister::Virtual(target_reg)),
                    rhs: MachineOperand::Immediate(0),
                });
                self.emit(MachineInstruction::BranchCc {
                    cc: ConditionCode::Equal,
                    target: null_lbl.clone(),
                });

                let str_idx = self.module.add_string(field);
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
                let target_handle = self.lower_to_handle(target);
                self.emit_call_with_args("aot_get_field", &[target_handle, field_handle]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                self.emit(MachineInstruction::Branch {
                    target: end_lbl.clone(),
                });

                let null_id = self.func.create_block(&null_lbl);
                self.current_block_id = null_id;
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Immediate(0),
                });

                let end_id = self.func.create_block(&end_lbl);
                self.current_block_id = end_id;

                out_reg
            }
            HirExpr::Format(inner, _) => {
                let h = self.lower_to_handle(inner);
                let out_reg = self.func.alloc_vreg();
                self.emit_call_with_args("aot_to_string", &[h]);
                self.emit(MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                    src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                });
                out_reg
            }
            HirExpr::This | HirExpr::Super => {
                let out_reg = self.func.alloc_vreg();
                if let Some((slot, _)) = self.local_vars.get("this").cloned() {
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::StackSlot(slot),
                    });
                } else {
                    self.emit(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(out_reg)),
                        src: MachineOperand::Immediate(0),
                    });
                }
                out_reg
            }
            HirExpr::Share(inner)
            | HirExpr::Downgrade(inner)
            | HirExpr::Move(inner)
            | HirExpr::NonNull(inner)
            | HirExpr::Spread(inner)
            | HirExpr::Await(inner)
            | HirExpr::Spawn(inner) => self.lower_expression(inner),
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
    let mut sigs = HashMap::new();
    let param_tys = hir_func.params.iter().map(|(_, ty, _)| ty.clone().unwrap_or(HirType::Int)).collect();
    sigs.insert(hir_func.name.clone(), (param_tys, hir_func.ret_type.clone()));
    lower_hir_function_with_signatures(hir_func, module, target, &sigs);
}

pub fn lower_hir_function_with_signatures(
    hir_func: &HirFunction,
    module: &mut NativeModule,
    target: &TargetDescriptor,
    fn_signatures: &HashMap<String, (Vec<HirType>, Option<HirType>)>,
) {
    if module.functions.iter().any(|f| f.name == hir_func.name) {
        return;
    }
    let mut func = MachineFunction::new(&hir_func.name);
    func.is_exported = true;

    {
        let mut ctx = FunctionLoweringContext::with_signatures(
            &mut func,
            module,
            target,
            hir_func.ret_type.clone(),
            fn_signatures.clone(),
        );

        let param_classes: Vec<RegisterClass> = hir_func
            .params
            .iter()
            .map(|(_, ty, _)| {
                if is_float_type(ty.as_ref()) {
                    RegisterClass::Float
                } else {
                    RegisterClass::Gpr
                }
            })
            .collect();

        let arg_locations = ctx.call_conv.classify_incoming_args(&param_classes);

        for (idx, (p_name, p_ty, _)) in hir_func.params.iter().enumerate() {
            let slot = ctx.alloc_stack_slot(8);
            let off = -(slot + 8);
            match arg_locations[idx] {
                ArgumentLocation::Register(p_reg) => {
                    ctx.emit(MachineInstruction::Move {
                        dst: MachineOperand::StackSlot(off),
                        src: MachineOperand::Register(MachineRegister::Physical(p_reg)),
                    });
                }
                ArgumentLocation::Stack(stack_off) => {
                    ctx.emit(MachineInstruction::Load {
                        dst: MachineOperand::StackSlot(off),
                        src: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
                            offset: stack_off,
                            index: None,
                        },
                        size: 8,
                    });
                }
            }
            ctx.local_vars.insert(p_name.clone(), (off, p_ty.clone()));
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

    let mut fn_signatures = HashMap::new();
    for f in &hir.functions {
        let param_tys = f
            .params
            .iter()
            .map(|(_, ty, _)| ty.clone().unwrap_or(HirType::Int))
            .collect();
        fn_signatures.insert(f.name.clone(), (param_tys, f.ret_type.clone()));
    }

    // Lower user functions
    for hir_func in &hir.functions {
        lower_hir_function_with_signatures(hir_func, &mut module, target, &fn_signatures);
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

    // Filter out locally defined functions from imports
    module.imports.retain(|imp| !module.functions.iter().any(|f| &f.name == imp));

    module
}
