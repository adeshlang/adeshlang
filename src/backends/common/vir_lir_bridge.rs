//! VIR to LIR Bridge Adapter
//!
//! This module provides a bridge from VIR to LIR, allowing backends to gradually
//! migrate from LIR consumption to direct VIR consumption. This keeps LIR as an
//! optional intermediate layer during the transition period.

use crate::backends::common::lir::{
    BlockId as LirBlockId, LirFunction, LirInst, LirModule, LirType, ValueId as LirValueId,
};
use crate::ir::vir::{
    BlockId, CmpOp, FloatBinOp, FloatUnOp, IntBinOp, IntUnOp, ValueId, VirFunction, VirInstruction,
    VirModule, VirParam, VirTerminator, VirType,
};
use std::collections::HashMap;

fn is_runtime_builtin(name: &str) -> bool {
    let name = name.strip_prefix("std:").unwrap_or(name);
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    registry.has(name)
        || name.starts_with("fs.")
        || name.starts_with("Regex.")
        || matches!(
            name,
            "Promise" | "Date" | "panic" | "exit" | "argc" | "argv" | "arg"
        )
}

/// Bridge for converting VIR to LIR
pub struct VirToLirBridge {
    /// Value ID mapping from VIR to LIR
    value_map: HashMap<ValueId, LirValueId>,

    /// Block ID mapping from VIR to LIR
    block_map: HashMap<BlockId, LirBlockId>,

    /// Mapping from ValueId to string values (for ConstString instructions)
    value_to_string: HashMap<ValueId, String>,

    /// Next LIR value ID
    next_value: LirValueId,

    /// Next LIR block ID
    next_block: LirBlockId,
}

impl VirToLirBridge {
    /// Create a new VIR to LIR bridge
    pub fn new() -> Self {
        Self {
            value_map: HashMap::new(),
            block_map: HashMap::new(),
            value_to_string: HashMap::new(),
            next_value: 0,
            next_block: 0,
        }
    }

    /// Convert a VIR module to LIR module
    pub fn convert_module(&mut self, vir_module: &VirModule) -> Result<LirModule, String> {
        let mut lir_module = LirModule::new();

        for vir_func in &vir_module.functions {
            let lir_func = self.convert_function(vir_func, &vir_module.strings)?;
            lir_module.add_function(lir_func);
        }

        Ok(lir_module)
    }

    /// Convert a VIR function to LIR function  
    fn convert_function(
        &mut self,
        vir_func: &VirFunction,
        strings: &std::collections::HashMap<u32, String>,
    ) -> Result<LirFunction, String> {
        self.value_map.clear();
        self.block_map.clear();
        self.value_to_string.clear();

        // Create function with params and return type
        let lir_params: Vec<(String, LirType)> = vir_func
            .params
            .iter()
            .map(|p| self.convert_param(p))
            .collect();
        let lir_ret_type = self.convert_type(&vir_func.return_type);

        let mut lir_func = LirFunction::new(vir_func.name.clone(), lir_params, lir_ret_type);

        // Map parameters and populate var_map so backends can find them
        for idx in 0..vir_func.params.len() {
            let vir_id = idx as ValueId;
            let lir_id = idx as LirValueId;
            self.value_map.insert(vir_id, lir_id);
            // Set var_map so the native JIT compiler can map parameter names to value IDs
            lir_func.set_var(vir_func.params[idx].name.clone(), lir_id);
        }
        self.next_value = vir_func.params.len() as LirValueId;

        // Create LIR blocks for each VIR block
        // Use the automatically created entry block for the first VIR block
        let mut first = true;
        for vir_block in &vir_func.blocks {
            let block_id = if first {
                first = false;
                lir_func.entry_block // Reuse the automatically created entry block
            } else {
                lir_func.create_block(format!("block_{}", vir_block.id))
            };
            self.block_map.insert(vir_block.id, block_id);
        }
        self.next_block = lir_func.blocks.len() as LirBlockId;

        // Convert VIR blocks to LIR instructions
        for vir_block in &vir_func.blocks {
            let lir_block_id = *self
                .block_map
                .get(&vir_block.id)
                .ok_or_else(|| format!("Block {} not found in mapping", vir_block.id))?;

            // Convert phi nodes
            for phi in &vir_block.phis {
                let dest = self.get_lir_value(phi.dest);
                let incoming: Vec<(LirBlockId, LirValueId)> = phi
                    .incoming
                    .iter()
                    .map(|(b, v)| {
                        let lir_b = *self.block_map.get(b).unwrap_or(&0);
                        let lir_v = self.get_lir_value(*v);
                        (lir_b, lir_v)
                    })
                    .collect();
                lir_func.push_to_block(lir_block_id, LirInst::Phi(dest, incoming));
            }

            // Convert block instructions
            for inst in &vir_block.instructions {
                let lir_insts = self.convert_instruction(inst, strings)?;
                for lir_inst in lir_insts {
                    lir_func.push_to_block(lir_block_id, lir_inst);
                }
            }

            // Convert terminator
            let term_insts = self.convert_terminator(&vir_block.terminator, strings)?;
            for lir_inst in term_insts {
                lir_func.push_to_block(lir_block_id, lir_inst);
            }
        }

        Ok(lir_func)
    }

    /// Convert VIR parameter to LIR parameter
    fn convert_param(&self, param: &VirParam) -> (String, LirType) {
        (param.name.clone(), self.convert_type(&param.ty))
    }

    /// Convert VIR type to LIR type
    fn convert_type(&self, vir_type: &VirType) -> LirType {
        match vir_type {
            VirType::Void => LirType::Void,
            VirType::I8 => LirType::I8,
            VirType::I16 => LirType::I16,
            VirType::I32 => LirType::I32,
            VirType::I64 => LirType::I64,
            VirType::I128 => LirType::I128,
            VirType::U8 => LirType::U8,
            VirType::U16 => LirType::U16,
            VirType::U32 => LirType::U32,
            VirType::U64 => LirType::U64,
            VirType::U128 => LirType::U128,
            VirType::F32 => LirType::F32,
            VirType::F64 => LirType::F64,
            VirType::Bool => LirType::Bool,
            VirType::Ptr | VirType::TypedPtr(_) => LirType::Ptr,
            // String types are pointers in native code
            VirType::Struct(name) if name == "String" || name == "string" => LirType::Ptr,
            // Other structs, arrays, tuples etc. are also pointers at the LIR/native level
            VirType::Struct(_) | VirType::Array { .. } | VirType::Tuple(_) => LirType::Ptr,
            _ => LirType::I64, // Default fallback for remaining types
        }
    }

    /// Get or create LIR value ID for VIR value
    fn get_lir_value(&mut self, vir_value: ValueId) -> LirValueId {
        if let Some(&lir_val) = self.value_map.get(&vir_value) {
            lir_val
        } else {
            let lir_val = self.next_value;
            self.value_map.insert(vir_value, lir_val);
            self.next_value += 1;
            lir_val
        }
    }

    /// Convert VIR instruction to LIR instructions
    fn convert_instruction(
        &mut self,
        inst: &VirInstruction,
        strings: &std::collections::HashMap<u32, String>,
    ) -> Result<Vec<LirInst>, String> {
        let lir_inst = match inst {
            // Constants
            VirInstruction::ConstInt { dest, value, ty } => {
                let dst = self.get_lir_value(*dest);
                match ty {
                    VirType::I64 => LirInst::ConstI64(dst, *value),
                    VirType::I32 => LirInst::ConstI32(dst, *value as i32),
                    VirType::I16 => LirInst::ConstI16(dst, *value as i16),
                    VirType::I8 => LirInst::ConstI8(dst, *value as i8),
                    VirType::U8 => LirInst::ConstU8(dst, *value as u8),
                    VirType::U16 => LirInst::ConstU16(dst, *value as u16),
                    VirType::U32 => LirInst::ConstU32(dst, *value as u32),
                    VirType::U64 => LirInst::ConstU64(dst, *value as u64),
                    VirType::U128 => LirInst::ConstU128(dst, *value as u128),
                    _ => LirInst::ConstI64(dst, *value),
                }
            }

            VirInstruction::ConstFloat { dest, value, ty } => {
                let dst = self.get_lir_value(*dest);
                match ty {
                    VirType::F64 => LirInst::ConstF64(dst, *value),
                    VirType::F32 => LirInst::ConstF32(dst, *value as f32),
                    _ => LirInst::ConstF64(dst, *value),
                }
            }

            VirInstruction::ConstBool { dest, value } => {
                let dst = self.get_lir_value(*dest);
                LirInst::ConstBool(dst, *value)
            }

            VirInstruction::ConstNull { dest } => {
                let dst = self.get_lir_value(*dest);
                LirInst::ConstNull(dst)
            }

            VirInstruction::ConstString { dest, string_id } => {
                // Look up the actual string value from the pool
                if let Some(string_val) = strings.get(string_id) {
                    // Store the string value for this ValueId
                    self.value_to_string.insert(*dest, string_val.clone());
                    let dst = self.get_lir_value(*dest);
                    LirInst::ConstString(dst, string_val.clone())
                } else {
                    let dst = self.get_lir_value(*dest);
                    LirInst::ConstString(dst, format!("UNKNOWN_STRING_{}", string_id))
                }
            }

            // Integer binary operations
            VirInstruction::IntBinOp {
                dest, op, lhs, rhs, ..
            } => {
                let dst = self.get_lir_value(*dest);
                let l = self.get_lir_value(*lhs);
                let r = self.get_lir_value(*rhs);
                match op {
                    IntBinOp::Add => LirInst::AddI64(dst, l, r),
                    IntBinOp::Sub => LirInst::SubI64(dst, l, r),
                    IntBinOp::Mul => LirInst::MulI64(dst, l, r),
                    IntBinOp::Div => LirInst::DivI64(dst, l, r),
                    IntBinOp::Rem => LirInst::ModI64(dst, l, r),
                    IntBinOp::And => LirInst::BitAnd(dst, l, r),
                    IntBinOp::Or => LirInst::BitOr(dst, l, r),
                    IntBinOp::Xor => LirInst::BitXor(dst, l, r),
                    IntBinOp::Shl => LirInst::Shl(dst, l, r),
                    IntBinOp::Shr => LirInst::Shr(dst, l, r),
                }
            }

            // Float binary operations
            VirInstruction::FloatBinOp {
                dest, op, lhs, rhs, ..
            } => {
                let dst = self.get_lir_value(*dest);
                let l = self.get_lir_value(*lhs);
                let r = self.get_lir_value(*rhs);
                match op {
                    FloatBinOp::Add => LirInst::AddF64(dst, l, r),
                    FloatBinOp::Sub => LirInst::SubF64(dst, l, r),
                    FloatBinOp::Mul => LirInst::MulF64(dst, l, r),
                    FloatBinOp::Div => LirInst::DivF64(dst, l, r),
                }
            }

            // Integer unary operations
            VirInstruction::IntUnOp {
                dest, op, operand, ..
            } => {
                let dst = self.get_lir_value(*dest);
                let src = self.get_lir_value(*operand);
                match op {
                    IntUnOp::Neg => LirInst::NegI64(dst, src),
                    IntUnOp::Not => LirInst::Not(dst, src),
                }
            }

            // Float unary operations
            VirInstruction::FloatUnOp {
                dest, op, operand, ..
            } => {
                let dst = self.get_lir_value(*dest);
                let src = self.get_lir_value(*operand);
                match op {
                    FloatUnOp::Neg => LirInst::NegF64(dst, src),
                    // Note: LIR doesn't have Abs or Sqrt, skip for now
                    _ => return Ok(vec![]),
                }
            }

            // Integer comparisons
            VirInstruction::IntCmp { dest, op, lhs, rhs } => {
                let dst = self.get_lir_value(*dest);
                let l = self.get_lir_value(*lhs);
                let r = self.get_lir_value(*rhs);
                match op {
                    CmpOp::Eq => LirInst::CmpEqI64(dst, l, r),
                    CmpOp::Ne => LirInst::CmpNeI64(dst, l, r),
                    CmpOp::Lt => LirInst::CmpLtI64(dst, l, r),
                    CmpOp::Le => LirInst::CmpLeI64(dst, l, r),
                    CmpOp::Gt => LirInst::CmpGtI64(dst, l, r),
                    CmpOp::Ge => LirInst::CmpGeI64(dst, l, r),
                }
            }

            // Float comparisons
            VirInstruction::FloatCmp { dest, op, lhs, rhs } => {
                let dst = self.get_lir_value(*dest);
                let l = self.get_lir_value(*lhs);
                let r = self.get_lir_value(*rhs);
                match op {
                    CmpOp::Eq => LirInst::CmpEqF64(dst, l, r),
                    CmpOp::Ne => LirInst::CmpNeF64(dst, l, r),
                    CmpOp::Lt => LirInst::CmpLtF64(dst, l, r),
                    CmpOp::Le => LirInst::CmpLeF64(dst, l, r),
                    CmpOp::Gt => LirInst::CmpGtF64(dst, l, r),
                    CmpOp::Ge => LirInst::CmpGeF64(dst, l, r),
                }
            }

            // Memory operations
            VirInstruction::Alloc { dest, size, .. } => {
                let dst = self.get_lir_value(*dest);
                let sz = self.get_lir_value(*size);
                LirInst::Alloc(dst, sz)
            }

            VirInstruction::Free { ptr } => {
                let p = self.get_lir_value(*ptr);
                LirInst::Free(p)
            }

            VirInstruction::Load { dest, ptr, .. } => {
                let dst = self.get_lir_value(*dest);
                let p = self.get_lir_value(*ptr);
                // LIR uses PtrLoad(dst, ptr, index) - use 0 as index
                let zero = self.next_value;
                self.next_value += 1;
                return Ok(vec![
                    LirInst::ConstI64(zero, 0),
                    LirInst::PtrLoad(dst, p, zero),
                ]);
            }

            VirInstruction::Store { ptr, value } => {
                let p = self.get_lir_value(*ptr);
                let v = self.get_lir_value(*value);
                // LIR uses PtrStore(ptr, value, index) - use 0 as index
                let zero = self.next_value;
                self.next_value += 1;
                return Ok(vec![
                    LirInst::ConstI64(zero, 0),
                    LirInst::PtrStore(p, v, zero),
                ]);
            }

            // ARC operations
            VirInstruction::ArcClone { dest, src } => {
                let dst = self.get_lir_value(*dest);
                let s = self.get_lir_value(*src);
                LirInst::ArcClone(dst, s)
            }

            VirInstruction::ArcDrop { ptr } => {
                let p = self.get_lir_value(*ptr);
                LirInst::ArcDrop(p)
            }

            VirInstruction::ArcIncrement { ptr } => {
                // LIR doesn't have ArcIncrement, use ArcGet as placeholder
                let p = self.get_lir_value(*ptr);
                let temp = self.next_value;
                self.next_value += 1;
                LirInst::ArcGet(temp, p)
            }

            VirInstruction::ArcDecrement { ptr } => {
                // LIR doesn't have ArcDecrement, use ArcGet as placeholder
                let p = self.get_lir_value(*ptr);
                let temp = self.next_value;
                self.next_value += 1;
                LirInst::ArcGet(temp, p)
            }

            // Copy/Move
            VirInstruction::Copy { dest, src } | VirInstruction::Move { dest, src } => {
                let dst = self.get_lir_value(*dest);
                let s = self.get_lir_value(*src);
                LirInst::Copy(dst, s)
            }

            // Local variable operations - convert to LoadVar/StoreVar
            VirInstruction::LoadLocal { dest, local } => {
                let dst = self.get_lir_value(*dest);
                // Use local ID as variable name for LoadVar
                let var_name = format!("_local_{}", local);
                LirInst::LoadVar(dst, var_name)
            }

            VirInstruction::StoreLocal { local, value } => {
                let v = self.get_lir_value(*value);
                let var_name = format!("_local_{}", local);
                LirInst::StoreVar(var_name, v)
            }

            // Call
            VirInstruction::Call { dest, func, args } => {
                // Try to resolve func ValueId to a function name
                let func_name = if let Some(name) = self.value_to_string.get(func) {
                    // Found the string value from ConstString tracking
                    name.clone()
                } else if let Some(name) = strings.get(func) {
                    // Try direct StringId lookup (fallback)
                    name.clone()
                } else {
                    // Fallback: use a placeholder
                    format!("func_{}", func)
                };

                let is_builtin = is_runtime_builtin(&func_name);

                let arg_ids: Vec<LirValueId> =
                    args.iter().map(|a| self.get_lir_value(*a)).collect();
                let result = self.get_lir_value(dest.unwrap_or(0));

                if is_builtin {
                    LirInst::CallBuiltin(result, func_name, arg_ids)
                } else {
                    LirInst::Call(result, func_name, arg_ids)
                }
            }

            // Cast operation
            VirInstruction::Cast {
                dest,
                value,
                from_ty,
                to_ty,
            } => {
                let dst = self.get_lir_value(*dest);
                let src = self.get_lir_value(*value);
                // Generate appropriate conversion instruction
                match (from_ty, to_ty) {
                    (
                        VirType::I64 | VirType::I32 | VirType::I16 | VirType::I8,
                        VirType::F64 | VirType::F32,
                    ) => {
                        return Ok(vec![LirInst::I64ToF64(dst, src)]);
                    }
                    (
                        VirType::F64 | VirType::F32,
                        VirType::I64 | VirType::I32 | VirType::I16 | VirType::I8,
                    ) => {
                        return Ok(vec![LirInst::F64ToI64(dst, src)]);
                    }
                    _ => {
                        // Default: just copy
                        return Ok(vec![LirInst::Copy(dst, src)]);
                    }
                }
            }

            // Aggregate operations - these use CallBuiltin to work with runtime builtins

            // BuildArray -> CallBuiltin("make_array", elements)
            VirInstruction::BuildArray { dest, elements, .. } => {
                let dst = self.get_lir_value(*dest);
                let elem_ids: Vec<LirValueId> =
                    elements.iter().map(|e| self.get_lir_value(*e)).collect();
                LirInst::CallBuiltin(dst, "make_array".to_string(), elem_ids)
            }

            // BuildStruct -> CallBuiltin("make_object") + set_field for each field
            VirInstruction::BuildStruct {
                dest,
                field_names,
                fields,
                ..
            } => {
                let dst = self.get_lir_value(*dest);
                if fields.is_empty() {
                    return Ok(vec![LirInst::CallBuiltin(
                        dst,
                        "make_object".to_string(),
                        vec![],
                    )]);
                }
                let mut insts = Vec::with_capacity(1 + fields.len());
                insts.push(LirInst::CallBuiltin(dst, "make_object".to_string(), vec![]));
                // For each field, we need a field name constant and the field value
                for (idx, field_val) in fields.iter().enumerate() {
                    let fv = self.get_lir_value(*field_val);
                    // Create a string constant for the field name
                    // Prefer original field names when available; fall back to synthetic keys.
                    let name_id = self.next_value;
                    self.next_value += 1;
                    let field_name = field_names
                        .get(idx)
                        .cloned()
                        .unwrap_or_else(|| format!("field_{}", idx));
                    insts.push(LirInst::ConstString(name_id, field_name));
                    insts.push(LirInst::CallBuiltin(
                        dst,
                        "set_field".to_string(),
                        vec![dst, name_id, fv],
                    ));
                }
                return Ok(insts);
            }

            // ExtractField -> CallBuiltin("get_field", [struct, field_name])
            VirInstruction::ExtractField {
                dest,
                struct_val,
                field,
            } => {
                let dst = self.get_lir_value(*dest);
                let sv = self.get_lir_value(*struct_val);
                let name_id = self.next_value;
                self.next_value += 1;
                return Ok(vec![
                    LirInst::ConstString(name_id, format!("field_{}", field)),
                    LirInst::CallBuiltin(dst, "get_field".to_string(), vec![sv, name_id]),
                ]);
            }

            // InsertField -> CallBuiltin("set_field", [struct, field_name, value])
            VirInstruction::InsertField {
                dest,
                struct_val,
                field,
                value,
            } => {
                let dst = self.get_lir_value(*dest);
                let sv = self.get_lir_value(*struct_val);
                let v = self.get_lir_value(*value);
                let name_id = self.next_value;
                self.next_value += 1;
                return Ok(vec![
                    LirInst::ConstString(name_id, format!("field_{}", field)),
                    LirInst::Copy(dst, sv), // copy the struct reference
                    LirInst::CallBuiltin(dst, "set_field".to_string(), vec![dst, name_id, v]),
                ]);
            }

            // ArrayIndex -> CallBuiltin("get_index", [array, index])
            VirInstruction::ArrayIndex { dest, array, index } => {
                let dst = self.get_lir_value(*dest);
                let arr = self.get_lir_value(*array);
                let idx = self.get_lir_value(*index);
                LirInst::CallBuiltin(dst, "get_index".to_string(), vec![arr, idx])
            }

            // BuildTuple -> CallBuiltin("make_tuple", elements)
            VirInstruction::BuildTuple { dest, elements } => {
                let dst = self.get_lir_value(*dest);
                let elem_ids: Vec<LirValueId> =
                    elements.iter().map(|e| self.get_lir_value(*e)).collect();
                LirInst::CallBuiltin(dst, "make_tuple".to_string(), elem_ids)
            }

            // BuildObject -> CallBuiltin("make_object", keys_and_values)
            VirInstruction::BuildObject { dest, keys, values } => {
                // BuildObject reached
                let dst = self.get_lir_value(*dest);
                // Build list of key-value argument pairs
                let mut args: Vec<LirValueId> = Vec::new();
                let mut key_instructions: Vec<LirInst> = Vec::new();

                // For each key-value pair, create ConstString for the key
                for (key, value) in keys.iter().zip(values.iter()) {
                    // Allocate LIR value for key
                    let key_lir = self.next_value;
                    self.next_value += 1;
                    // Add ConstString instruction for key
                    key_instructions.push(LirInst::ConstString(key_lir, key.clone()));
                    args.push(key_lir);

                    // Add value
                    args.push(self.get_lir_value(*value));
                }

                // Return all instructions: keys first, then the call
                key_instructions.push(LirInst::CallBuiltin(dst, "make_object".to_string(), args));
                return Ok(key_instructions);
            }

            // ExtractTuple -> CallBuiltin("get_index", [tuple, const_index])
            VirInstruction::ExtractTuple { dest, tuple, index } => {
                let dst = self.get_lir_value(*dest);
                let tup = self.get_lir_value(*tuple);
                let idx_val = self.next_value;
                self.next_value += 1;
                return Ok(vec![
                    LirInst::ConstI64(idx_val, *index as i64),
                    LirInst::CallBuiltin(dst, "get_index".to_string(), vec![tup, idx_val]),
                ]);
            }

            // Enum operations - use make_object as a proxy
            VirInstruction::BuildEnum {
                dest,
                variant,
                payload,
                ..
            } => {
                let dst = self.get_lir_value(*dest);
                let mut insts = Vec::new();
                insts.push(LirInst::CallBuiltin(dst, "make_object".to_string(), vec![]));
                // Store discriminant
                let disc_name = self.next_value;
                self.next_value += 1;
                let disc_val = self.next_value;
                self.next_value += 1;
                let tmp = self.next_value;
                self.next_value += 1;
                insts.push(LirInst::ConstString(
                    disc_name,
                    "__discriminant".to_string(),
                ));
                insts.push(LirInst::ConstI64(disc_val, *variant as i64));
                insts.push(LirInst::CallBuiltin(
                    tmp,
                    "set_field".to_string(),
                    vec![dst, disc_name, disc_val],
                ));
                // Store payload elements
                for (i, pv) in payload.iter().enumerate() {
                    let pval = self.get_lir_value(*pv);
                    let pname = self.next_value;
                    self.next_value += 1;
                    let ptmp = self.next_value;
                    self.next_value += 1;
                    insts.push(LirInst::ConstString(pname, format!("payload_{}", i)));
                    insts.push(LirInst::CallBuiltin(
                        ptmp,
                        "set_field".to_string(),
                        vec![dst, pname, pval],
                    ));
                }
                return Ok(insts);
            }

            VirInstruction::GetDiscriminant { dest, enum_val } => {
                let dst = self.get_lir_value(*dest);
                let ev = self.get_lir_value(*enum_val);
                let name_id = self.next_value;
                self.next_value += 1;
                return Ok(vec![
                    LirInst::ConstString(name_id, "__discriminant".to_string()),
                    LirInst::CallBuiltin(dst, "get_field".to_string(), vec![ev, name_id]),
                ]);
            }

            VirInstruction::ExtractPayload {
                dest,
                enum_val,
                variant,
            } => {
                let dst = self.get_lir_value(*dest);
                let ev = self.get_lir_value(*enum_val);
                let name_id = self.next_value;
                self.next_value += 1;
                return Ok(vec![
                    LirInst::ConstString(name_id, format!("payload_{}", variant)),
                    LirInst::CallBuiltin(dst, "get_field".to_string(), vec![ev, name_id]),
                ]);
            }

            // Drop and Nop are runtime no-ops at the LIR level
            VirInstruction::Drop { .. } | VirInstruction::Nop => {
                return Ok(vec![]);
            }

            // Bitcast is a reinterpret - just copy at LIR level
            VirInstruction::Bitcast { dest, value, .. } => {
                let dst = self.get_lir_value(*dest);
                let v = self.get_lir_value(*value);
                LirInst::Copy(dst, v)
            }

            // Intrinsic - forward as CallBuiltin with intrinsic name
            VirInstruction::Intrinsic {
                dest,
                intrinsic,
                args,
            } => {
                let intrinsic_name = format!("{:?}", intrinsic).to_lowercase();
                let dst = self.get_lir_value(dest.unwrap_or(0));
                let arg_ids: Vec<LirValueId> =
                    args.iter().map(|a| self.get_lir_value(*a)).collect();
                LirInst::CallBuiltin(dst, intrinsic_name, arg_ids)
            }
        };

        Ok(vec![lir_inst])
    }

    /// Convert VIR terminator to LIR instructions
    fn convert_terminator(
        &mut self,
        term: &VirTerminator,
        _strings: &std::collections::HashMap<u32, String>,
    ) -> Result<Vec<LirInst>, String> {
        let lir_insts = match term {
            VirTerminator::Return { value } => {
                vec![LirInst::Return(value.map(|v| self.get_lir_value(v)))]
            }

            VirTerminator::Jump { target } => {
                let block_id = self
                    .block_map
                    .get(target)
                    .ok_or_else(|| format!("Block {} not found in mapping", target))?;
                vec![LirInst::Jump(*block_id)]
            }

            VirTerminator::Branch {
                cond,
                true_target,
                false_target,
            } => {
                let c = self.get_lir_value(*cond);
                let true_block = self
                    .block_map
                    .get(true_target)
                    .ok_or_else(|| format!("Block {} not found", true_target))?;
                let false_block = self
                    .block_map
                    .get(false_target)
                    .ok_or_else(|| format!("Block {} not found", false_target))?;
                vec![LirInst::JumpIf(c, *true_block, *false_block)]
            }

            VirTerminator::Switch {
                value,
                cases,
                default,
            } => {
                let cond = self.get_lir_value(*value);
                let default_block = *self
                    .block_map
                    .get(default)
                    .ok_or_else(|| format!("Block {} not found in mapping", default))?;
                if cases.len() == 1 && cases[0].0 == 1 {
                    // Boolean switch: case 1 = true_target, default = false_target
                    let true_block = *self
                        .block_map
                        .get(&cases[0].1)
                        .ok_or_else(|| format!("Block {} not found in mapping", cases[0].1))?;
                    vec![LirInst::JumpIf(cond, true_block, default_block)]
                } else if cases.is_empty() {
                    vec![LirInst::Jump(default_block)]
                } else {
                    // Multi-case fallback: jump to default
                    vec![LirInst::Jump(default_block)]
                }
            }

            VirTerminator::Unreachable => {
                vec![]
            }
        };

        Ok(lir_insts)
    }
}

impl Default for VirToLirBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_creation() {
        let bridge = VirToLirBridge::new();
        assert_eq!(bridge.next_value, 0);
        assert_eq!(bridge.next_block, 0);
    }

    #[test]
    fn test_type_conversion() {
        let bridge = VirToLirBridge::new();
        assert!(matches!(bridge.convert_type(&VirType::I64), LirType::I64));
        assert!(matches!(bridge.convert_type(&VirType::F64), LirType::F64));
        assert!(matches!(bridge.convert_type(&VirType::Bool), LirType::Bool));
    }
}
