//! Built-in function call handlers
//!
//! This module handles all CallBuiltin instructions with their extensive
//! builtin function implementations including:
//! - Array methods (map, filter, reduce, push, pop, etc.)
//! - I/O operations (print, println)
//! - Type conversions (str, int, float, u8-u128, i8-i128, f32, f64, bool)
//! - Utility functions (len, argc, argv, typeof, sizeof)
//! - Object operations (make_object, set_field, set_index)

#[allow(unused_imports)]
use cranelift::prelude::*;
#[allow(unused_imports)]
use cranelift_module::{Linkage, Module};
#[allow(unused_imports)]
use std::collections::HashMap;

use crate::backends::aot::cranelift::{AotValueType, FunctionCompileContext};
#[allow(unused_imports)]
use crate::backends::aot::cranelift_impl::helpers;
#[allow(unused_imports)]
use crate::backends::common::lir::ValueId;
use cranelift_codegen::ir::FuncRef;
use cranelift_codegen::isa;

struct RuntimeValueConstructors {
    make_i64: FuncRef,
    make_i32: FuncRef,
    make_i16: FuncRef,
    make_i8: FuncRef,
    make_u64: FuncRef,
    make_u32: FuncRef,
    make_u16: FuncRef,
    make_u8: FuncRef,
    make_f64: FuncRef,
    make_f32: FuncRef,
    make_bool: FuncRef,
    make_char: FuncRef,
    make_string: FuncRef,
    make_null: FuncRef,
    make_obj: FuncRef,
    make_arr: FuncRef,
    make_tuple: FuncRef,
    make_set: FuncRef,
    wrap_ptr: FuncRef,
}

impl RuntimeValueConstructors {
    fn declare(module: &mut dyn Module, builder: &mut FunctionBuilder) -> Result<Self, String> {
        let make_i64_sig = {
            let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            sig
        };
        let make_f64_sig = {
            let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
            sig.params.push(AbiParam::new(types::F64));
            sig.returns.push(AbiParam::new(types::I64));
            sig
        };
        let make_null_sig = {
            let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
            sig.returns.push(AbiParam::new(types::I64));
            sig
        };
        let make_obj_sig = {
            let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));
            sig
        };

        let make_i64_id = module
            .declare_function("aot_make_i64", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_i64: {}", e))?;
        let make_i64 = module.declare_func_in_func(make_i64_id, builder.func);

        let make_i32_id = module
            .declare_function("aot_make_i32", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_i32: {}", e))?;
        let make_i32 = module.declare_func_in_func(make_i32_id, builder.func);

        let make_i16_id = module
            .declare_function("aot_make_i16", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_i16: {}", e))?;
        let make_i16 = module.declare_func_in_func(make_i16_id, builder.func);

        let make_i8_id = module
            .declare_function("aot_make_i8", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_i8: {}", e))?;
        let make_i8 = module.declare_func_in_func(make_i8_id, builder.func);

        let make_u64_id = module
            .declare_function("aot_make_u64", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_u64: {}", e))?;
        let make_u64 = module.declare_func_in_func(make_u64_id, builder.func);

        let make_u32_id = module
            .declare_function("aot_make_u32", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_u32: {}", e))?;
        let make_u32 = module.declare_func_in_func(make_u32_id, builder.func);

        let make_u16_id = module
            .declare_function("aot_make_u16", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_u16: {}", e))?;
        let make_u16 = module.declare_func_in_func(make_u16_id, builder.func);

        let make_u8_id = module
            .declare_function("aot_make_u8", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_u8: {}", e))?;
        let make_u8 = module.declare_func_in_func(make_u8_id, builder.func);

        let make_f64_id = module
            .declare_function("aot_make_f64", Linkage::Import, &make_f64_sig)
            .map_err(|e| format!("Failed to declare aot_make_f64: {}", e))?;
        let make_f64 = module.declare_func_in_func(make_f64_id, builder.func);

        let make_f32_id = module
            .declare_function("aot_make_f32", Linkage::Import, &make_f64_sig)
            .map_err(|e| format!("Failed to declare aot_make_f32: {}", e))?;
        let make_f32 = module.declare_func_in_func(make_f32_id, builder.func);

        let make_bool_id = module
            .declare_function("aot_make_bool", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_bool: {}", e))?;
        let make_bool = module.declare_func_in_func(make_bool_id, builder.func);

        let make_char_id = module
            .declare_function("aot_make_char", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_char: {}", e))?;
        let make_char = module.declare_func_in_func(make_char_id, builder.func);

        let make_string_id = module
            .declare_function("aot_make_string", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_make_string: {}", e))?;
        let make_string = module.declare_func_in_func(make_string_id, builder.func);

        let make_null_id = module
            .declare_function("aot_make_null", Linkage::Import, &make_null_sig)
            .map_err(|e| format!("Failed to declare aot_make_null: {}", e))?;
        let make_null = module.declare_func_in_func(make_null_id, builder.func);

        let make_obj_id = module
            .declare_function("aot_make_object", Linkage::Import, &make_obj_sig)
            .map_err(|e| format!("Failed to declare aot_make_object: {}", e))?;
        let make_obj = module.declare_func_in_func(make_obj_id, builder.func);

        let make_arr_id = module
            .declare_function("aot_make_array", Linkage::Import, &make_obj_sig)
            .map_err(|e| format!("Failed to declare aot_make_array: {}", e))?;
        let make_arr = module.declare_func_in_func(make_arr_id, builder.func);

        let make_tuple_id = module
            .declare_function("aot_make_tuple", Linkage::Import, &make_obj_sig)
            .map_err(|e| format!("Failed to declare aot_make_tuple: {}", e))?;
        let make_tuple = module.declare_func_in_func(make_tuple_id, builder.func);

        let make_set_id = module
            .declare_function("aot_make_set", Linkage::Import, &make_obj_sig)
            .map_err(|e| format!("Failed to declare aot_make_set: {}", e))?;
        let make_set = module.declare_func_in_func(make_set_id, builder.func);

        let wrap_ptr_id = module
            .declare_function("aot_wrap_ptr", Linkage::Import, &make_i64_sig)
            .map_err(|e| format!("Failed to declare aot_wrap_ptr: {}", e))?;
        let wrap_ptr = module.declare_func_in_func(wrap_ptr_id, builder.func);

        Ok(Self {
            make_i64,
            make_i32,
            make_i16,
            make_i8,
            make_u64,
            make_u32,
            make_u16,
            make_u8,
            make_f64,
            make_f32,
            make_bool,
            make_char,
            make_string,
            make_null,
            make_obj,
            make_arr,
            make_tuple,
            make_set,
            wrap_ptr,
        })
    }
}

fn convert_val_to_handle(
    ctx: &mut FunctionCompileContext,
    builder: &mut FunctionBuilder,
    module: &mut dyn Module,
    val_id: &ValueId,
    val_raw: Value,
    val_ty: &AotValueType,
    ctors: &RuntimeValueConstructors,
) -> Result<Value, String> {
    if ctx.runtime_handle_values.contains(val_id) || *val_ty == AotValueType::Handle {
        return Ok(val_raw);
    }
    if let Some(&b) = ctx.const_bools.get(val_id) {
        let b_i64 = builder.ins().iconst(types::I64, if b { 1 } else { 0 });
        let call = builder.ins().call(ctors.make_bool, &[b_i64]);
        return Ok(builder.inst_results(call)[0]);
    }
    if ctx.const_strings.contains_key(val_id) || *val_ty == AotValueType::String {
        let call = builder.ins().call(ctors.make_string, &[val_raw]);
        return Ok(builder.inst_results(call)[0]);
    }
    match val_ty {
        AotValueType::Handle => Ok(val_raw),
        AotValueType::Bool => {
            let b_i64 = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().uextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_bool, &[b_i64]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::Char => {
            let c_i64 = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().uextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_char, &[c_i64]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::F32 => {
            let f64v = if builder.func.dfg.value_type(val_raw) == types::F64 {
                val_raw
            } else if builder.func.dfg.value_type(val_raw) == types::F32 {
                builder.ins().fpromote(types::F64, val_raw)
            } else if builder.func.dfg.value_type(val_raw).is_int() {
                let f32v = builder.ins().bitcast(types::F32, MemFlags::new(), val_raw);
                builder.ins().fpromote(types::F64, f32v)
            } else {
                builder.ins().f64const(0.0)
            };
            let call = builder.ins().call(ctors.make_f32, &[f64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::F64 | AotValueType::Float => {
            let f64v = if builder.func.dfg.value_type(val_raw) == types::F64 {
                val_raw
            } else if builder.func.dfg.value_type(val_raw) == types::F32 {
                builder.ins().fpromote(types::F64, val_raw)
            } else if builder.func.dfg.value_type(val_raw).is_int() {
                builder.ins().bitcast(types::F64, MemFlags::new(), val_raw)
            } else {
                builder.ins().f64const(0.0)
            };
            let call = builder.ins().call(ctors.make_f64, &[f64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::U8 => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().uextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_u8, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::U16 => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().uextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_u16, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::U32 => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().uextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_u32, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::U64 => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().uextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_u64, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::I8 => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().sextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_i8, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::I16 => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().sextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_i16, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::I32 => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else {
                builder.ins().sextend(types::I64, val_raw)
            };
            let call = builder.ins().call(ctors.make_i32, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::I64 | AotValueType::Int => {
            let i64v = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else if builder.func.dfg.value_type(val_raw).is_int() {
                builder.ins().sextend(types::I64, val_raw)
            } else {
                builder.ins().iconst(types::I64, 0)
            };
            let call = builder.ins().call(ctors.make_i64, &[i64v]);
            Ok(builder.inst_results(call)[0])
        }
        AotValueType::Tuple(elem_types) => {
            let mut elem_handles = Vec::with_capacity(elem_types.len());
            for (idx, elem_ty) in elem_types.iter().enumerate() {
                let off = idx as i32 * 8;
                let elem_raw = builder
                    .ins()
                    .load(types::I64, MemFlags::new(), val_raw, off);
                let elem_handle = match elem_ty {
                    AotValueType::String => {
                        let c = builder.ins().call(ctors.make_string, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Bool => {
                        let c = builder.ins().call(ctors.make_bool, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Char => {
                        let c = builder.ins().call(ctors.make_char, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::F32 => {
                        let f32v = builder.ins().bitcast(types::F32, MemFlags::new(), elem_raw);
                        let f64v = builder.ins().fpromote(types::F64, f32v);
                        let c = builder.ins().call(ctors.make_f32, &[f64v]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::F64 | AotValueType::Float => {
                        let f64v = builder.ins().bitcast(types::F64, MemFlags::new(), elem_raw);
                        let c = builder.ins().call(ctors.make_f64, &[f64v]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U8 => {
                        let c = builder.ins().call(ctors.make_u8, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U16 => {
                        let c = builder.ins().call(ctors.make_u16, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U32 => {
                        let c = builder.ins().call(ctors.make_u32, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U64 => {
                        let c = builder.ins().call(ctors.make_u64, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I8 => {
                        let c = builder.ins().call(ctors.make_i8, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I16 => {
                        let c = builder.ins().call(ctors.make_i16, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I32 => {
                        let c = builder.ins().call(ctors.make_i32, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I64 | AotValueType::Int => {
                        let c = builder.ins().call(ctors.make_i64, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Handle => elem_raw,
                    _ => {
                        let c = builder.ins().call(ctors.make_i64, &[elem_raw]);
                        builder.inst_results(c)[0]
                    }
                };
                elem_handles.push(elem_handle);
            }
            let tuple_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (elem_handles.len().max(1) as u32) * 8,
                8,
            ));
            let tuple_ptr = builder.ins().stack_addr(types::I64, tuple_slot, 0);
            for (i, h) in elem_handles.iter().enumerate() {
                builder
                    .ins()
                    .store(MemFlags::new(), *h, tuple_ptr, (i * 8) as i32);
            }
            let tuple_count = builder.ins().iconst(types::I64, elem_handles.len() as i64);
            let c = builder
                .ins()
                .call(ctors.make_tuple, &[tuple_ptr, tuple_count]);
            Ok(builder.inst_results(c)[0])
        }
        AotValueType::Set(elem_ty, arr_len) => {
            let elem_size: i64 = match **elem_ty {
                AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                AotValueType::U16 | AotValueType::I16 => 2,
                AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                _ => 8,
            };
            let meta = elem_ty.metadata_size();
            let mut elem_handles = Vec::with_capacity(*arr_len);
            for idx in 0..*arr_len {
                let off = meta + (idx as i64 * elem_size);
                let elem_ptr = builder.ins().iadd_imm(val_raw, off);
                let elem_handle = match **elem_ty {
                    AotValueType::String => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_string, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Bool => {
                        let ev = builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_bool, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Char => {
                        let ev = builder.ins().load(types::I32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_char, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::F32 => {
                        let ev = builder.ins().load(types::F32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().fpromote(types::F64, ev);
                        let c = builder.ins().call(ctors.make_f32, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::F64 | AotValueType::Float => {
                        let ev = builder.ins().load(types::F64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_f64, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U8 => {
                        let ev = builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_u8, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U16 => {
                        let ev = builder.ins().load(types::I16, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_u16, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U32 => {
                        let ev = builder.ins().load(types::I32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_u32, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U64 => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_u64, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I8 => {
                        let ev = builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().sextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_i8, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I16 => {
                        let ev = builder.ins().load(types::I16, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().sextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_i16, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I32 => {
                        let ev = builder.ins().load(types::I32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().sextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_i32, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Handle => {
                        builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0)
                    }
                    AotValueType::Ptr => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.wrap_ptr, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    _ => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_i64, &[ev]);
                        builder.inst_results(c)[0]
                    }
                };
                elem_handles.push(elem_handle);
            }
            let set_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (elem_handles.len().max(1) as u32) * 8,
                8,
            ));
            let set_ptr = builder.ins().stack_addr(types::I64, set_slot, 0);
            for (i, h) in elem_handles.iter().enumerate() {
                builder
                    .ins()
                    .store(MemFlags::new(), *h, set_ptr, (i * 8) as i32);
            }
            let set_count = builder.ins().iconst(types::I64, elem_handles.len() as i64);
            let c = builder.ins().call(ctors.make_set, &[set_ptr, set_count]);
            Ok(builder.inst_results(c)[0])
        }
        AotValueType::Array(elem_ty, arr_len) => {
            let elem_size: i64 = match **elem_ty {
                AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                AotValueType::U16 | AotValueType::I16 => 2,
                AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                _ => 8,
            };
            let meta = elem_ty.metadata_size();
            let mut elem_handles = Vec::with_capacity(*arr_len);
            for idx in 0..*arr_len {
                let off = meta + (idx as i64 * elem_size);
                let elem_ptr = builder.ins().iadd_imm(val_raw, off);
                let elem_handle = match **elem_ty {
                    AotValueType::String => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_string, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Bool => {
                        let ev = builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_bool, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Char => {
                        let ev = builder.ins().load(types::I32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_char, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::F32 => {
                        let ev = builder.ins().load(types::F32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().fpromote(types::F64, ev);
                        let c = builder.ins().call(ctors.make_f32, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::F64 | AotValueType::Float => {
                        let ev = builder.ins().load(types::F64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_f64, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U8 => {
                        let ev = builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_u8, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U16 => {
                        let ev = builder.ins().load(types::I16, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_u16, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U32 => {
                        let ev = builder.ins().load(types::I32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().uextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_u32, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::U64 => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_u64, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I8 => {
                        let ev = builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().sextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_i8, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I16 => {
                        let ev = builder.ins().load(types::I16, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().sextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_i16, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::I32 => {
                        let ev = builder.ins().load(types::I32, MemFlags::new(), elem_ptr, 0);
                        let e64 = builder.ins().sextend(types::I64, ev);
                        let c = builder.ins().call(ctors.make_i32, &[e64]);
                        builder.inst_results(c)[0]
                    }
                    AotValueType::Handle => {
                        builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0)
                    }
                    AotValueType::Ptr => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.wrap_ptr, &[ev]);
                        builder.inst_results(c)[0]
                    }
                    _ => {
                        let ev = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let c = builder.ins().call(ctors.make_i64, &[ev]);
                        builder.inst_results(c)[0]
                    }
                };
                elem_handles.push(elem_handle);
            }
            let arr_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (elem_handles.len().max(1) as u32) * 8,
                8,
            ));
            let arr_ptr = builder.ins().stack_addr(types::I64, arr_slot, 0);
            for (i, h) in elem_handles.iter().enumerate() {
                builder
                    .ins()
                    .store(MemFlags::new(), *h, arr_ptr, (i * 8) as i32);
            }
            let arr_count = builder.ins().iconst(types::I64, elem_handles.len() as i64);
            let c = builder.ins().call(ctors.make_arr, &[arr_ptr, arr_count]);
            Ok(builder.inst_results(c)[0])
        }
        AotValueType::Ptr => {
            if let Some(props) = ctx.object_properties.get(val_id).cloned() {
                if !props.is_empty() {
                    let mut kv_handles = Vec::with_capacity(props.len() * 2);
                    for (k, (field_id, field_ty)) in props {
                        let key_val = if let Some(&key_data_id) = ctx.string_data.get(&k) {
                            let key_gv = module.declare_data_in_func(key_data_id, builder.func);
                            builder.ins().global_value(types::I64, key_gv)
                        } else {
                            continue;
                        };
                        let key_call = builder.ins().call(ctors.make_string, &[key_val]);
                        let key_handle = builder.inst_results(key_call)[0];
                        let field_raw = ctx
                            .value_map
                            .get(&field_id)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let field_handle = convert_val_to_handle(
                            ctx, builder, module, &field_id, field_raw, &field_ty, ctors,
                        )?;
                        kv_handles.push(key_handle);
                        kv_handles.push(field_handle);
                    }
                    if !kv_handles.is_empty() {
                        let obj_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            (kv_handles.len() as u32) * 8,
                            8,
                        ));
                        let obj_ptr = builder.ins().stack_addr(types::I64, obj_slot, 0);
                        for (i, h) in kv_handles.iter().enumerate() {
                            builder
                                .ins()
                                .store(MemFlags::new(), *h, obj_ptr, (i * 8) as i32);
                        }
                        let obj_count = builder.ins().iconst(types::I64, kv_handles.len() as i64);
                        let c = builder.ins().call(ctors.make_obj, &[obj_ptr, obj_count]);
                        return Ok(builder.inst_results(c)[0]);
                    }
                }
            }
            let call = builder.ins().call(ctors.wrap_ptr, &[val_raw]);
            Ok(builder.inst_results(call)[0])
        }
        _ => {
            let i64_val = if builder.func.dfg.value_type(val_raw) == types::I64 {
                val_raw
            } else if builder.func.dfg.value_type(val_raw).is_int() {
                builder.ins().sextend(types::I64, val_raw)
            } else {
                builder.ins().iconst(types::I64, 0)
            };
            let call = builder.ins().call(ctors.make_i64, &[i64_val]);
            Ok(builder.inst_results(call)[0])
        }
    }
}

#[allow(dead_code)]
fn is_numeric_type(t: &AotValueType) -> bool {
    matches!(
        t,
        AotValueType::U8
            | AotValueType::U16
            | AotValueType::U32
            | AotValueType::U64
            | AotValueType::U128
            | AotValueType::I8
            | AotValueType::I16
            | AotValueType::I32
            | AotValueType::I64
            | AotValueType::I128
            | AotValueType::F32
            | AotValueType::F64
            | AotValueType::Int
            | AotValueType::Float
    )
}

fn unify_array_elem_types(elem_types: &[AotValueType]) -> AotValueType {
    if elem_types.is_empty() {
        return AotValueType::Int;
    }
    let mut current = elem_types[0].clone();
    for t in &elem_types[1..] {
        current = unify_two_types(&current, t);
    }
    current
}

fn unify_two_types(a: &AotValueType, b: &AotValueType) -> AotValueType {
    if a == b {
        return a.clone();
    }
    // Heterogeneous types unify to Handle to preserve each element's specific type tag
    AotValueType::Handle
}

/// Handle CallBuiltin instruction
///
/// This function dispatches to the appropriate handler based on the builtin name.
/// It handles a wide variety of builtin functions with complex implementations.
#[allow(unused_variables, clippy::too_many_lines)]
pub(crate) fn handle_call_builtin(
    ctx: &mut FunctionCompileContext,
    builder: &mut FunctionBuilder,
    module: &mut dyn Module,
    dst: &ValueId,
    builtin_name: &str,
    args: &[ValueId],
) -> Result<(), String> {
    let builtin_name = builtin_name.strip_prefix("std:").unwrap_or(builtin_name);
    match builtin_name {
        "__call_method" => {
            if args.len() >= 2 {
                let obj_id = args[0];
                let method_id = args[1];
                let method = ctx
                    .const_strings
                    .get(&method_id)
                    .cloned()
                    .unwrap_or_default();
                if let Some(AotValueType::Array(elem_type, arr_len)) =
                    ctx.value_types.get(&obj_id).cloned()
                {
                    let src_ptr = ctx
                        .value_map
                        .get(&obj_id)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                    let elem_size: i64 = match *elem_type {
                        AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                        AotValueType::U16 | AotValueType::I16 => 2,
                        AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                        AotValueType::U64
                        | AotValueType::I64
                        | AotValueType::F64
                        | AotValueType::Int
                        | AotValueType::Float
                        | AotValueType::Ptr => 8,
                        AotValueType::U128 | AotValueType::I128 => 16,
                        _ => 8,
                    };
                    let meta_size = elem_type.metadata_size() as i64;
                    let mut result_ptr = src_ptr;
                    match method.as_str() {
                        "map" => {
                            // map(callback) - apply callback to each element, produce new array
                            if args.len() >= 3 {
                                let cb_id = args[2];
                                if let Some(&func_id) = ctx.func_values.get(&cb_id) {
                                    let new_len = arr_len as i64;
                                    let total_size = (meta_size as i64) + (elem_size * new_len);
                                    let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                        StackSlotKind::ExplicitSlot,
                                        total_size as u32,
                                        8,
                                    ));
                                    result_ptr = builder.ins().stack_addr(types::I64, slot, 0);
                                    let len_i64 = builder.ins().iconst(types::I64, new_len);
                                    let cap_val =
                                        ctx.array_capacity.get(&obj_id).copied().unwrap_or(new_len);
                                    let cap_i64 = builder.ins().iconst(types::I64, cap_val);
                                    if meta_size == 4 {
                                        let len_i32 = builder.ins().ireduce(types::I32, len_i64);
                                        let cap_i32 = builder.ins().ireduce(types::I32, cap_i64);
                                        builder.ins().store(
                                            MemFlags::new(),
                                            len_i32,
                                            result_ptr,
                                            0,
                                        );
                                        builder.ins().store(
                                            MemFlags::new(),
                                            cap_i32,
                                            result_ptr,
                                            4,
                                        );
                                    } else {
                                        builder.ins().store(
                                            MemFlags::new(),
                                            len_i64,
                                            result_ptr,
                                            0,
                                        );
                                        builder.ins().store(
                                            MemFlags::new(),
                                            cap_i64,
                                            result_ptr,
                                            8,
                                        );
                                    }
                                    let src_meta = builder.ins().iconst(types::I64, meta_size);
                                    let es_const = builder.ins().iconst(types::I64, elem_size);
                                    let func_ref =
                                        module.declare_func_in_func(func_id, builder.func);
                                    for i in 0..arr_len as i64 {
                                        let i_val = builder.ins().iconst(types::I64, i);
                                        let off = builder.ins().imul(i_val, es_const);
                                        let src_off = builder.ins().iadd(src_meta, off);
                                        let src_elem = builder.ins().iadd(src_ptr, src_off);
                                        // Load element
                                        let elem_val = match elem_size {
                                            1 => {
                                                let v = builder.ins().load(
                                                    types::I8,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().sextend(types::I64, v)
                                            }
                                            2 => {
                                                let v = builder.ins().load(
                                                    types::I16,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().sextend(types::I64, v)
                                            }
                                            4 => {
                                                if matches!(*elem_type, AotValueType::F32) {
                                                    let v32 = builder.ins().load(
                                                        types::F32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    let v64 =
                                                        builder.ins().fpromote(types::F64, v32);
                                                    builder.ins().bitcast(
                                                        types::I64,
                                                        MemFlags::new(),
                                                        v64,
                                                    )
                                                } else {
                                                    let v = builder.ins().load(
                                                        types::I32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    builder.ins().sextend(types::I64, v)
                                                }
                                            }
                                            _ => builder.ins().load(
                                                types::I64,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            ),
                                        };
                                        // Call callback(elem)
                                        let call = builder.ins().call(func_ref, &[elem_val]);
                                        let cb_res = builder.inst_results(call)[0];
                                        // Store result
                                        let dst_off = builder.ins().iadd(src_meta, off);
                                        let dst_elem = builder.ins().iadd(result_ptr, dst_off);
                                        match elem_size {
                                            1 => {
                                                let r = builder.ins().ireduce(types::I8, cb_res);
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    r,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                            2 => {
                                                let r = builder.ins().ireduce(types::I16, cb_res);
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    r,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                            4 => {
                                                if matches!(*elem_type, AotValueType::F32) {
                                                    let f64v = builder.ins().bitcast(
                                                        types::F64,
                                                        MemFlags::new(),
                                                        cb_res,
                                                    );
                                                    let f32v =
                                                        builder.ins().fdemote(types::F32, f64v);
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        f32v,
                                                        dst_elem,
                                                        0,
                                                    );
                                                } else {
                                                    let r =
                                                        builder.ins().ireduce(types::I32, cb_res);
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        r,
                                                        dst_elem,
                                                        0,
                                                    );
                                                }
                                            }
                                            _ => {
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    cb_res,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                        }
                                    }
                                    ctx.value_map.insert(*dst, result_ptr);
                                    ctx.value_types.insert(
                                        *dst,
                                        AotValueType::Array(elem_type.clone(), new_len as usize),
                                    );
                                    ctx.array_capacity.insert(*dst, cap_val);
                                } else {
                                    ctx.value_map.insert(*dst, src_ptr);
                                    ctx.value_types.insert(
                                        *dst,
                                        AotValueType::Array(elem_type.clone(), arr_len),
                                    );
                                }
                            } else {
                                ctx.value_map.insert(*dst, src_ptr);
                                ctx.value_types
                                    .insert(*dst, AotValueType::Array(elem_type.clone(), arr_len));
                            }
                        }
                        "filter" => {
                            ctx.value_map.insert(*dst, result_ptr);
                            ctx.value_types
                                .insert(*dst, AotValueType::Array(elem_type.clone(), arr_len));
                        }
                        "reduce" => {
                            // reduce(callback, init) - return accumulator, array unchanged
                            if args.len() >= 4 {
                                let cb_id = args[2];
                                let init_id = args[3];
                                if let (Some(&func_id), Some(acc_init)) = (
                                    ctx.func_values.get(&cb_id),
                                    ctx.value_map.get(&init_id).copied(),
                                ) {
                                    // acc slot
                                    let acc_slot = builder.create_sized_stack_slot(
                                        StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 8),
                                    );
                                    let acc_ptr = builder.ins().stack_addr(types::I64, acc_slot, 0);
                                    builder.ins().store(MemFlags::new(), acc_init, acc_ptr, 0);
                                    let src_meta = builder.ins().iconst(types::I64, meta_size);
                                    let es_const = builder.ins().iconst(types::I64, elem_size);
                                    let func_ref =
                                        module.declare_func_in_func(func_id, builder.func);
                                    for i in 0..arr_len as i64 {
                                        let i_val = builder.ins().iconst(types::I64, i);
                                        let off = builder.ins().imul(i_val, es_const);
                                        let src_off = builder.ins().iadd(src_meta, off);
                                        let src_elem = builder.ins().iadd(src_ptr, src_off);
                                        let elem_val = match elem_size {
                                            1 => {
                                                let v = builder.ins().load(
                                                    types::I8,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().sextend(types::I64, v)
                                            }
                                            2 => {
                                                let v = builder.ins().load(
                                                    types::I16,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().sextend(types::I64, v)
                                            }
                                            4 => {
                                                if matches!(*elem_type, AotValueType::F32) {
                                                    let v32 = builder.ins().load(
                                                        types::F32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    let v64 =
                                                        builder.ins().fpromote(types::F64, v32);
                                                    builder.ins().bitcast(
                                                        types::I64,
                                                        MemFlags::new(),
                                                        v64,
                                                    )
                                                } else {
                                                    let v = builder.ins().load(
                                                        types::I32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    builder.ins().sextend(types::I64, v)
                                                }
                                            }
                                            _ => builder.ins().load(
                                                types::I64,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            ),
                                        };
                                        let acc_cur = builder.ins().load(
                                            types::I64,
                                            MemFlags::new(),
                                            acc_ptr,
                                            0,
                                        );
                                        let call =
                                            builder.ins().call(func_ref, &[acc_cur, elem_val]);
                                        let res = builder.inst_results(call)[0];
                                        builder.ins().store(MemFlags::new(), res, acc_ptr, 0);
                                    }
                                    let final_acc =
                                        builder.ins().load(types::I64, MemFlags::new(), acc_ptr, 0);
                                    ctx.value_map.insert(*dst, final_acc);
                                    ctx.value_types.insert(*dst, AotValueType::Int);
                                } else {
                                    let v = builder.ins().iconst(types::I64, 0);
                                    ctx.value_map.insert(*dst, v);
                                    ctx.value_types.insert(*dst, AotValueType::Int);
                                }
                            } else {
                                let v = builder.ins().iconst(types::I64, 0);
                                ctx.value_map.insert(*dst, v);
                                ctx.value_types.insert(*dst, AotValueType::Int);
                            }
                            // Array unchanged; callers not treating reduce as mutating
                        }

                        "append" | "push" => {
                            if args.len() >= 3 {
                                let val_id = args[2];
                                let new_len = arr_len as i64 + 1;
                                let total_size = (meta_size as i64) + (elem_size * new_len);
                                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                    StackSlotKind::ExplicitSlot,
                                    total_size as u32,
                                    8,
                                ));
                                result_ptr = builder.ins().stack_addr(types::I64, slot, 0);
                                let len_i64 = builder.ins().iconst(types::I64, new_len);
                                let cap_val =
                                    ctx.array_capacity.get(&obj_id).copied().unwrap_or(new_len);
                                let cap_i64 = builder.ins().iconst(types::I64, cap_val);
                                if meta_size == 4 {
                                    let len_i32 = builder.ins().ireduce(types::I32, len_i64);
                                    let cap_i32 = builder.ins().ireduce(types::I32, cap_i64);
                                    builder.ins().store(MemFlags::new(), len_i32, result_ptr, 0);
                                    builder.ins().store(MemFlags::new(), cap_i32, result_ptr, 4);
                                } else {
                                    builder.ins().store(MemFlags::new(), len_i64, result_ptr, 0);
                                    builder.ins().store(MemFlags::new(), cap_i64, result_ptr, 8);
                                }
                                let src_meta = builder.ins().iconst(types::I64, meta_size);
                                for i in 0..arr_len as i64 {
                                    let i_val = builder.ins().iconst(types::I64, i);
                                    let es = builder.ins().iconst(types::I64, elem_size);
                                    let off = builder.ins().imul(i_val, es);
                                    let src_off = builder.ins().iadd(src_meta, off);
                                    let src_elem = builder.ins().iadd(src_ptr, src_off);
                                    let dst_off = builder.ins().iadd(src_meta, off);
                                    let dst_elem = builder.ins().iadd(result_ptr, dst_off);
                                    match elem_size {
                                        1 => {
                                            let v = builder.ins().load(
                                                types::I8,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            );
                                            builder.ins().store(MemFlags::new(), v, dst_elem, 0);
                                        }
                                        2 => {
                                            let v = builder.ins().load(
                                                types::I16,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            );
                                            builder.ins().store(MemFlags::new(), v, dst_elem, 0);
                                        }
                                        4 => {
                                            if matches!(*elem_type, AotValueType::F32) {
                                                let v = builder.ins().load(
                                                    types::F32,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            } else {
                                                let v = builder.ins().load(
                                                    types::I32,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                        }
                                        _ => {
                                            if matches!(
                                                *elem_type,
                                                AotValueType::F64 | AotValueType::Float
                                            ) {
                                                let v = builder.ins().load(
                                                    types::F64,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            } else {
                                                let v = builder.ins().load(
                                                    types::I64,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                        }
                                    }
                                }
                                let append_off = meta_size + (elem_size * (arr_len as i64));
                                let append_off_val = builder.ins().iconst(types::I64, append_off);
                                let dst_elem = builder.ins().iadd(result_ptr, append_off_val);
                                if let Some(val) = ctx.value_map.get(&val_id).copied() {
                                    match elem_size {
                                        1 => {
                                            let r = builder.ins().ireduce(types::I8, val);
                                            builder.ins().store(MemFlags::new(), r, dst_elem, 0);
                                        }
                                        2 => {
                                            let r = builder.ins().ireduce(types::I16, val);
                                            builder.ins().store(MemFlags::new(), r, dst_elem, 0);
                                        }
                                        4 => {
                                            if matches!(*elem_type, AotValueType::F32) {
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    val,
                                                    dst_elem,
                                                    0,
                                                );
                                            } else {
                                                let r = builder.ins().ireduce(types::I32, val);
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    r,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                        }
                                        _ => {
                                            builder.ins().store(MemFlags::new(), val, dst_elem, 0);
                                        }
                                    }
                                }
                                // updated length accounted in metadata; no local tracking needed
                                ctx.value_map.insert(*dst, result_ptr);
                                ctx.value_types.insert(
                                    *dst,
                                    AotValueType::Array(elem_type.clone(), new_len as usize),
                                );
                                ctx.array_capacity.insert(*dst, cap_val);
                            } else {
                                ctx.value_map.insert(*dst, src_ptr);
                                ctx.value_types
                                    .insert(*dst, AotValueType::Array(elem_type.clone(), arr_len));
                            }
                        }
                        "pop" => {
                            if arr_len > 0 {
                                let new_len = arr_len as i64 - 1;
                                let total_size = (meta_size as i64) + (elem_size * new_len);
                                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                    StackSlotKind::ExplicitSlot,
                                    total_size as u32,
                                    8,
                                ));
                                result_ptr = builder.ins().stack_addr(types::I64, slot, 0);
                                let len_i64 = builder.ins().iconst(types::I64, new_len);
                                let cap_val =
                                    ctx.array_capacity.get(&obj_id).copied().unwrap_or(new_len);
                                let cap_i64 = builder.ins().iconst(types::I64, cap_val);
                                if meta_size == 4 {
                                    let len_i32 = builder.ins().ireduce(types::I32, len_i64);
                                    let cap_i32 = builder.ins().ireduce(types::I32, cap_i64);
                                    builder.ins().store(MemFlags::new(), len_i32, result_ptr, 0);
                                    builder.ins().store(MemFlags::new(), cap_i32, result_ptr, 4);
                                } else {
                                    builder.ins().store(MemFlags::new(), len_i64, result_ptr, 0);
                                    builder.ins().store(MemFlags::new(), cap_i64, result_ptr, 8);
                                }
                                let src_meta = builder.ins().iconst(types::I64, meta_size);
                                for i in 0..new_len {
                                    let i_val = builder.ins().iconst(types::I64, i);
                                    let es = builder.ins().iconst(types::I64, elem_size);
                                    let off = builder.ins().imul(i_val, es);
                                    let src_off = builder.ins().iadd(src_meta, off);
                                    let src_elem = builder.ins().iadd(src_ptr, src_off);
                                    let dst_off = builder.ins().iadd(src_meta, off);
                                    let dst_elem = builder.ins().iadd(result_ptr, dst_off);
                                    match elem_size {
                                        1 => {
                                            let v = builder.ins().load(
                                                types::I8,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            );
                                            builder.ins().store(MemFlags::new(), v, dst_elem, 0);
                                        }
                                        2 => {
                                            let v = builder.ins().load(
                                                types::I16,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            );
                                            builder.ins().store(MemFlags::new(), v, dst_elem, 0);
                                        }
                                        4 => {
                                            if matches!(*elem_type, AotValueType::F32) {
                                                let v = builder.ins().load(
                                                    types::F32,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            } else {
                                                let v = builder.ins().load(
                                                    types::I32,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                        }
                                        _ => {
                                            if matches!(
                                                *elem_type,
                                                AotValueType::F64 | AotValueType::Float
                                            ) {
                                                let v = builder.ins().load(
                                                    types::F64,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            } else {
                                                let v = builder.ins().load(
                                                    types::I64,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                        }
                                    }
                                }
                                // updated length accounted in metadata; no local tracking needed
                                ctx.value_map.insert(*dst, result_ptr);
                                ctx.value_types.insert(
                                    *dst,
                                    AotValueType::Array(elem_type.clone(), new_len as usize),
                                );
                                ctx.array_capacity.insert(*dst, cap_val);
                            } else {
                                ctx.value_map.insert(*dst, src_ptr);
                                ctx.value_types
                                    .insert(*dst, AotValueType::Array(elem_type.clone(), arr_len));
                            }
                        }
                        "clear" => {
                            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                meta_size as u32,
                                8,
                            ));
                            result_ptr = builder.ins().stack_addr(types::I64, slot, 0);
                            let zero = builder.ins().iconst(types::I64, 0);
                            if meta_size == 4 {
                                let z32 = builder.ins().ireduce(types::I32, zero);
                                builder.ins().store(MemFlags::new(), z32, result_ptr, 0);
                                builder.ins().store(MemFlags::new(), z32, result_ptr, 4);
                            } else {
                                builder.ins().store(MemFlags::new(), zero, result_ptr, 0);
                                builder.ins().store(MemFlags::new(), zero, result_ptr, 8);
                            }
                            // cleared length accounted in metadata; no local tracking needed
                            ctx.value_map.insert(*dst, result_ptr);
                            ctx.value_types
                                .insert(*dst, AotValueType::Array(elem_type.clone(), 0));
                            ctx.array_capacity.insert(*dst, 0);
                        }
                        "extend" => {
                            if args.len() >= 3 {
                                let other_id = args[2];
                                if let Some(AotValueType::Array(other_elem, other_len)) =
                                    ctx.value_types.get(&other_id).cloned()
                                {
                                    let other_ptr = ctx
                                        .value_map
                                        .get(&other_id)
                                        .copied()
                                        .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                                    let new_len = arr_len as i64 + other_len as i64;
                                    let total_size = (meta_size as i64) + (elem_size * new_len);
                                    let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                        StackSlotKind::ExplicitSlot,
                                        total_size as u32,
                                        8,
                                    ));
                                    result_ptr = builder.ins().stack_addr(types::I64, slot, 0);
                                    let len_i64 = builder.ins().iconst(types::I64, new_len);
                                    let cap_val =
                                        ctx.array_capacity.get(&obj_id).copied().unwrap_or(new_len);
                                    let cap_i64 = builder.ins().iconst(types::I64, cap_val);
                                    if meta_size == 4 {
                                        let len_i32 = builder.ins().ireduce(types::I32, len_i64);
                                        let cap_i32 = builder.ins().ireduce(types::I32, cap_i64);
                                        builder.ins().store(
                                            MemFlags::new(),
                                            len_i32,
                                            result_ptr,
                                            0,
                                        );
                                        builder.ins().store(
                                            MemFlags::new(),
                                            cap_i32,
                                            result_ptr,
                                            4,
                                        );
                                    } else {
                                        builder.ins().store(
                                            MemFlags::new(),
                                            len_i64,
                                            result_ptr,
                                            0,
                                        );
                                        builder.ins().store(
                                            MemFlags::new(),
                                            cap_i64,
                                            result_ptr,
                                            8,
                                        );
                                    }
                                    let src_meta = builder.ins().iconst(types::I64, meta_size);
                                    for i in 0..arr_len as i64 {
                                        let i_val = builder.ins().iconst(types::I64, i);
                                        let es = builder.ins().iconst(types::I64, elem_size);
                                        let off = builder.ins().imul(i_val, es);
                                        let src_off = builder.ins().iadd(src_meta, off);
                                        let src_elem = builder.ins().iadd(src_ptr, src_off);
                                        let dst_off = builder.ins().iadd(src_meta, off);
                                        let dst_elem = builder.ins().iadd(result_ptr, dst_off);
                                        match elem_size {
                                            1 => {
                                                let v = builder.ins().load(
                                                    types::I8,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                            2 => {
                                                let v = builder.ins().load(
                                                    types::I16,
                                                    MemFlags::new(),
                                                    src_elem,
                                                    0,
                                                );
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                            4 => {
                                                if matches!(*elem_type, AotValueType::F32) {
                                                    let v = builder.ins().load(
                                                        types::F32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        v,
                                                        dst_elem,
                                                        0,
                                                    );
                                                } else {
                                                    let v = builder.ins().load(
                                                        types::I32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        v,
                                                        dst_elem,
                                                        0,
                                                    );
                                                }
                                            }
                                            _ => {
                                                if matches!(
                                                    *elem_type,
                                                    AotValueType::F64 | AotValueType::Float
                                                ) {
                                                    let v = builder.ins().load(
                                                        types::F64,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        v,
                                                        dst_elem,
                                                        0,
                                                    );
                                                } else {
                                                    let v = builder.ins().load(
                                                        types::I64,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    );
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        v,
                                                        dst_elem,
                                                        0,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    let other_meta = builder.ins().iconst(types::I64, meta_size);
                                    let other_elem_size: i64 = match *other_elem {
                                        AotValueType::U8
                                        | AotValueType::I8
                                        | AotValueType::Bool => 1,
                                        AotValueType::U16 | AotValueType::I16 => 2,
                                        AotValueType::U32
                                        | AotValueType::I32
                                        | AotValueType::F32 => 4,
                                        AotValueType::U64
                                        | AotValueType::I64
                                        | AotValueType::F64
                                        | AotValueType::Int
                                        | AotValueType::Float
                                        | AotValueType::Ptr => 8,
                                        AotValueType::U128 | AotValueType::I128 => 16,
                                        _ => 8,
                                    };
                                    for i in 0..other_len as i64 {
                                        let i_val = builder.ins().iconst(types::I64, i);
                                        let es_src =
                                            builder.ins().iconst(types::I64, other_elem_size);
                                        let off = builder.ins().imul(i_val, es_src);
                                        let src_off = builder.ins().iadd(other_meta, off);
                                        let src_elem = builder.ins().iadd(other_ptr, src_off);
                                        let dst_index = arr_len as i64 + i;
                                        let dst_i = builder.ins().iconst(types::I64, dst_index);
                                        let es_dst = builder.ins().iconst(types::I64, elem_size);
                                        let dst_off = builder.ins().imul(dst_i, es_dst);
                                        let dst_off2 = builder.ins().iadd(other_meta, dst_off);
                                        let dst_elem = builder.ins().iadd(result_ptr, dst_off2);
                                        let loaded_val = match other_elem_size {
                                            1 => builder.ins().load(
                                                types::I8,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            ),
                                            2 => builder.ins().load(
                                                types::I16,
                                                MemFlags::new(),
                                                src_elem,
                                                0,
                                            ),
                                            4 => {
                                                if matches!(*other_elem, AotValueType::F32) {
                                                    builder.ins().load(
                                                        types::F32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    )
                                                } else {
                                                    builder.ins().load(
                                                        types::I32,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    )
                                                }
                                            }
                                            _ => {
                                                if matches!(
                                                    *other_elem,
                                                    AotValueType::F64 | AotValueType::Float
                                                ) {
                                                    builder.ins().load(
                                                        types::F64,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    )
                                                } else {
                                                    builder.ins().load(
                                                        types::I64,
                                                        MemFlags::new(),
                                                        src_elem,
                                                        0,
                                                    )
                                                }
                                            }
                                        };
                                        match elem_size {
                                            1 => {
                                                let v8 =
                                                    builder.ins().ireduce(types::I8, loaded_val);
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v8,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                            2 => {
                                                let v16 = if other_elem_size > 2 {
                                                    builder.ins().ireduce(types::I16, loaded_val)
                                                } else if other_elem_size < 2 {
                                                    builder.ins().uextend(types::I16, loaded_val)
                                                } else {
                                                    loaded_val
                                                };
                                                builder.ins().store(
                                                    MemFlags::new(),
                                                    v16,
                                                    dst_elem,
                                                    0,
                                                );
                                            }
                                            4 => {
                                                if matches!(*elem_type, AotValueType::F32) {
                                                    // If source is F64, demote; if integer, reduce then convert
                                                    let to_store = if matches!(
                                                        *other_elem,
                                                        AotValueType::F64 | AotValueType::Float
                                                    ) {
                                                        let f64v = loaded_val;
                                                        builder.ins().fdemote(types::F32, f64v)
                                                    } else if matches!(
                                                        *other_elem,
                                                        AotValueType::F32
                                                    ) {
                                                        loaded_val
                                                    } else {
                                                        let i32v = builder
                                                            .ins()
                                                            .ireduce(types::I32, loaded_val);
                                                        builder
                                                            .ins()
                                                            .fcvt_from_sint(types::F32, i32v)
                                                    };
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        to_store,
                                                        dst_elem,
                                                        0,
                                                    );
                                                } else {
                                                    let i32v = if other_elem_size > 4 {
                                                        builder
                                                            .ins()
                                                            .ireduce(types::I32, loaded_val)
                                                    } else if other_elem_size < 4 {
                                                        builder
                                                            .ins()
                                                            .uextend(types::I32, loaded_val)
                                                    } else {
                                                        loaded_val
                                                    };
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        i32v,
                                                        dst_elem,
                                                        0,
                                                    );
                                                }
                                            }
                                            _ => {
                                                if matches!(
                                                    *elem_type,
                                                    AotValueType::F64 | AotValueType::Float
                                                ) {
                                                    let to_store =
                                                        if matches!(*other_elem, AotValueType::F32)
                                                        {
                                                            let f32v = loaded_val;
                                                            builder.ins().fpromote(types::F64, f32v)
                                                        } else if matches!(
                                                            *other_elem,
                                                            AotValueType::F64 | AotValueType::Float
                                                        ) {
                                                            loaded_val
                                                        } else {
                                                            builder.ins().fcvt_from_sint(
                                                                types::F64,
                                                                loaded_val,
                                                            )
                                                        };
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        to_store,
                                                        dst_elem,
                                                        0,
                                                    );
                                                } else {
                                                    let i64v = if other_elem_size < 8 {
                                                        builder
                                                            .ins()
                                                            .uextend(types::I64, loaded_val)
                                                    } else {
                                                        loaded_val
                                                    };
                                                    builder.ins().store(
                                                        MemFlags::new(),
                                                        i64v,
                                                        dst_elem,
                                                        0,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    // updated length accounted in metadata; no local tracking needed
                                    ctx.value_map.insert(*dst, result_ptr);
                                    ctx.value_types.insert(
                                        *dst,
                                        AotValueType::Array(elem_type.clone(), new_len as usize),
                                    );
                                    ctx.array_capacity.insert(*dst, cap_val);
                                } else {
                                    ctx.value_map.insert(*dst, src_ptr);
                                    ctx.value_types.insert(
                                        *dst,
                                        AotValueType::Array(elem_type.clone(), arr_len),
                                    );
                                }
                            } else {
                                ctx.value_map.insert(*dst, src_ptr);
                                ctx.value_types
                                    .insert(*dst, AotValueType::Array(elem_type.clone(), arr_len));
                            }
                        }
                        _ => {
                            ctx.value_map.insert(*dst, src_ptr);
                            ctx.value_types
                                .insert(*dst, AotValueType::Array(elem_type.clone(), arr_len));
                        }
                    }
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "set_index" => {
            // Set element in array at index: set_index(container, index, value)
            if args.len() >= 3 {
                let container_id = args[0];
                let index_id = args[1];
                let value_id = args[2];
                let container_type = ctx.value_types.get(&container_id).cloned();
                let container_ptr = ctx
                    .value_map
                    .get(&container_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let index_val = ctx
                    .value_map
                    .get(&index_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                if let Some(AotValueType::Array(elem_type, arr_len)) = container_type.clone() {
                    let elem_size: i64 = match *elem_type {
                        AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                        AotValueType::U16 | AotValueType::I16 => 2,
                        AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                        AotValueType::U64
                        | AotValueType::I64
                        | AotValueType::F64
                        | AotValueType::Int
                        | AotValueType::Float
                        | AotValueType::Ptr => 8,
                        AotValueType::U128 | AotValueType::I128 => 16,
                        _ => 8,
                    };
                    let metadata_size = elem_type.metadata_size();
                    let elem_size_val = builder.ins().iconst(types::I64, elem_size);
                    let byte_offset = builder.ins().imul(index_val, elem_size_val);
                    let metadata_offset = builder.ins().iconst(types::I64, metadata_size);
                    let total_offset = builder.ins().iadd(metadata_offset, byte_offset);
                    let elem_ptr = builder.ins().iadd(container_ptr, total_offset);
                    if let Some(val) = ctx.value_map.get(&value_id).copied() {
                        match elem_size {
                            1 => {
                                let r = builder.ins().ireduce(types::I8, val);
                                builder.ins().store(MemFlags::new(), r, elem_ptr, 0);
                            }
                            2 => {
                                let r = builder.ins().ireduce(types::I16, val);
                                builder.ins().store(MemFlags::new(), r, elem_ptr, 0);
                            }
                            4 => {
                                if matches!(*elem_type, AotValueType::F32) {
                                    builder.ins().store(MemFlags::new(), val, elem_ptr, 0);
                                } else {
                                    let r = builder.ins().ireduce(types::I32, val);
                                    builder.ins().store(MemFlags::new(), r, elem_ptr, 0);
                                }
                            }
                            _ => {
                                builder.ins().store(MemFlags::new(), val, elem_ptr, 0);
                            }
                        }
                    }
                    // Return container unchanged
                    ctx.value_map.insert(*dst, container_ptr);
                    ctx.value_types
                        .insert(*dst, AotValueType::Array(elem_type.clone(), arr_len));
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "input" => {
            let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));

            let input_fn_id = module
                .declare_function("aot_input", Linkage::Import, &sig)
                .map_err(|e| format!("Failed to declare aot_input: {}", e))?;
            let input_fn = module.declare_func_in_func(input_fn_id, builder.func);

            let prompt_val = if let Some(arg0) = args.first() {
                ctx.value_map
                    .get(arg0)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0))
            } else {
                builder.ins().iconst(types::I64, 0)
            };
            let opts_val = if let Some(arg1) = args.get(1) {
                ctx.value_map
                    .get(arg1)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0))
            } else {
                builder.ins().iconst(types::I64, 0)
            };

            let call = builder.ins().call(input_fn, &[prompt_val, opts_val]);
            let res = builder.inst_results(call)[0];
            ctx.value_map.insert(*dst, res);
            ctx.value_types.insert(*dst, AotValueType::Handle);
            ctx.runtime_handle_values.insert(*dst);
        }
        name if name.starts_with("input.") => {
            let sub_method = name.strip_prefix("input.").unwrap_or(name);
            let aot_fn_name = match sub_method {
                "mock" => "aot_input_mock",
                "confirm" => "aot_input_confirm",
                "password" => "aot_input_password",
                "select" => "aot_input_select",
                "checkbox" => "aot_input_checkbox",
                "radio" => "aot_input_radio",
                "fuzzy" => "aot_input_fuzzy",
                "slider" => "aot_input_slider",
                "tree" => "aot_input_tree",
                "table" => "aot_input_table",
                "datepicker" => "aot_input_datepicker",
                "datetime" | "datetimepicker" => "aot_input_datetime",
                "timepicker" => "aot_input_timepicker",
                "color" => "aot_input_color",
                "pin" => "aot_input_pin",
                "diff" => "aot_input_diff",
                "hotkey" => "aot_input_hotkey",
                "form" => "aot_input_form",
                _ => "aot_input_mock",
            };

            let arg0 = args
                .get(0)
                .and_then(|id| ctx.value_map.get(id).copied())
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let arg1 = args
                .get(1)
                .and_then(|id| ctx.value_map.get(id).copied())
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let arg2 = args
                .get(2)
                .and_then(|id| ctx.value_map.get(id).copied())
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let arg3 = args
                .get(3)
                .and_then(|id| ctx.value_map.get(id).copied())
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let arg4 = args
                .get(4)
                .and_then(|id| ctx.value_map.get(id).copied())
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

            let num_params = match sub_method {
                "slider" => 5,
                "table" => 4,
                "checkbox" | "radio" | "select" | "diff" | "tree" | "pin" => 3,
                "confirm" | "password" | "fuzzy" | "datepicker" | "datetime" | "datetimepicker"
                | "timepicker" | "color" | "hotkey" | "form" => 2,
                _ => 1, // mock
            };

            let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
            for _ in 0..num_params {
                sig.params.push(AbiParam::new(types::I64));
            }
            sig.returns.push(AbiParam::new(types::I64));

            let fn_id = module
                .declare_function(aot_fn_name, Linkage::Import, &sig)
                .map_err(|e| format!("Failed to declare {}: {}", aot_fn_name, e))?;
            let fn_ref = module.declare_func_in_func(fn_id, builder.func);

            let call_args: Vec<cranelift_codegen::ir::Value> = match num_params {
                1 => vec![arg0],
                2 => vec![arg0, arg1],
                3 => vec![arg0, arg1, arg2],
                4 => vec![arg0, arg1, arg2, arg3],
                _ => vec![arg0, arg1, arg2, arg3, arg4],
            };

            let call = builder.ins().call(fn_ref, &call_args);
            let res = builder.inst_results(call)[0];
            ctx.value_map.insert(*dst, res);
            ctx.value_types.insert(*dst, AotValueType::Handle);
            ctx.runtime_handle_values.insert(*dst);
        }
        "print" => {
            let ctors = RuntimeValueConstructors::declare(module, builder)?;
            let mut options_handle = builder.ins().iconst(types::I64, 0);
            let mut values_for_print: &[ValueId] = args;

            if let Some(&last_arg) = args.last() {
                let static_options = ctx
                    .object_properties
                    .get(&last_arg)
                    .map(|props| {
                        props.keys().any(|k| {
                            matches!(
                                k.as_str(),
                                "pretty"
                                    | "sep"
                                    | "end"
                                    | "file"
                                    | "color"
                                    | "background"
                                    | "bold"
                                    | "italic"
                                    | "underline"
                                    | "strikethrough"
                                    | "flush"
                            )
                        })
                    })
                    .unwrap_or(false);

                if static_options && args.len() > 1 {
                    if let Some(props) = ctx.object_properties.get(&last_arg).cloned() {
                        let mut kv_handles: Vec<Value> = Vec::new();
                        for (key, (val_id, val_ty)) in props.iter() {
                            let is_print_opt = matches!(
                                key.as_str(),
                                "pretty"
                                    | "sep"
                                    | "end"
                                    | "file"
                                    | "color"
                                    | "background"
                                    | "bold"
                                    | "italic"
                                    | "underline"
                                    | "strikethrough"
                                    | "flush"
                            );
                            if !is_print_opt {
                                continue;
                            }
                            let key_val = if let Some(&key_data_id) = ctx.string_data.get(key) {
                                let key_gv = module.declare_data_in_func(key_data_id, builder.func);
                                builder.ins().global_value(types::I64, key_gv)
                            } else {
                                continue;
                            };
                            let key_call = builder.ins().call(ctors.make_string, &[key_val]);
                            let key_handle = builder.inst_results(key_call)[0];
                            let val_raw = ctx
                                .value_map
                                .get(val_id)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let val_handle = convert_val_to_handle(
                                ctx, builder, module, val_id, val_raw, val_ty, &ctors,
                            )?;
                            kv_handles.push(key_handle);
                            kv_handles.push(val_handle);
                        }
                        if !kv_handles.is_empty() {
                            let kv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                (kv_handles.len() as u32) * 8,
                                8,
                            ));
                            let kv_ptr = builder.ins().stack_addr(types::I64, kv_slot, 0);
                            for (i, handle) in kv_handles.iter().enumerate() {
                                builder.ins().store(
                                    MemFlags::new(),
                                    *handle,
                                    kv_ptr,
                                    (i * 8) as i32,
                                );
                            }
                            let kv_count =
                                builder.ins().iconst(types::I64, kv_handles.len() as i64);
                            let make_obj_call =
                                builder.ins().call(ctors.make_obj, &[kv_ptr, kv_count]);
                            options_handle = builder.inst_results(make_obj_call)[0];
                        }
                    }
                    values_for_print = &args[..args.len() - 1];
                }
            }

            let print_opts_sig = {
                let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
                sig.params.push(AbiParam::new(types::I64)); // values_ptr
                sig.params.push(AbiParam::new(types::I64)); // values_count
                sig.params.push(AbiParam::new(types::I64)); // options_handle
                sig.returns.push(AbiParam::new(types::I64));
                sig
            };
            let print_opts_id = module
                .declare_function("aot_print_with_options", Linkage::Import, &print_opts_sig)
                .map_err(|e| format!("Failed to declare aot_print_with_options: {}", e))?;
            let print_opts_ref = module.declare_func_in_func(print_opts_id, builder.func);

            let mut arg_values = Vec::new();
            for arg_id in values_for_print.iter() {
                let val = ctx
                    .value_map
                    .get(arg_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let ty = ctx
                    .value_types
                    .get(arg_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);
                let handle = convert_val_to_handle(ctx, builder, module, arg_id, val, &ty, &ctors)?;
                arg_values.push(handle);
            }

            let array_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (arg_values.len().max(1) as u32) * 8,
                8,
            ));
            let array_ptr = builder.ins().stack_addr(types::I64, array_slot, 0);
            for (i, val) in arg_values.iter().enumerate() {
                let offset = (i * 8) as i32;
                builder
                    .ins()
                    .store(MemFlags::new(), *val, array_ptr, offset);
            }
            let count = builder.ins().iconst(types::I64, arg_values.len() as i64);
            let opts_handle_i64 = if builder.func.dfg.value_type(options_handle) == types::I64 {
                options_handle
            } else if builder.func.dfg.value_type(options_handle).is_int() {
                builder.ins().uextend(types::I64, options_handle)
            } else {
                builder.ins().iconst(types::I64, 0)
            };
            builder
                .ins()
                .call(print_opts_ref, &[array_ptr, count, opts_handle_i64]);
            let zero = builder.ins().iconst(types::I64, 0);
            ctx.value_map.insert(*dst, zero);
        }
        "println" => {
            let ctors = RuntimeValueConstructors::declare(module, builder)?;
            let print_opts_sig = {
                let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
                sig.params.push(AbiParam::new(types::I64)); // values_ptr
                sig.params.push(AbiParam::new(types::I64)); // values_count
                sig.params.push(AbiParam::new(types::I64)); // options_handle
                sig.returns.push(AbiParam::new(types::I64));
                sig
            };
            let print_opts_id = module
                .declare_function("aot_print_with_options", Linkage::Import, &print_opts_sig)
                .map_err(|e| format!("Failed to declare aot_print_with_options: {}", e))?;
            let print_opts_ref = module.declare_func_in_func(print_opts_id, builder.func);

            let mut arg_values = Vec::new();
            for arg_id in args.iter() {
                let val = ctx
                    .value_map
                    .get(arg_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let ty = ctx
                    .value_types
                    .get(arg_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);
                let handle = convert_val_to_handle(ctx, builder, module, arg_id, val, &ty, &ctors)?;
                arg_values.push(handle);
            }

            let array_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (arg_values.len().max(1) as u32) * 8,
                8,
            ));
            let array_ptr = builder.ins().stack_addr(types::I64, array_slot, 0);
            for (i, val) in arg_values.iter().enumerate() {
                let offset = (i * 8) as i32;
                builder
                    .ins()
                    .store(MemFlags::new(), *val, array_ptr, offset);
            }
            let count = builder.ins().iconst(types::I64, arg_values.len() as i64);
            let zero_opts = builder.ins().iconst(types::I64, 0);
            builder
                .ins()
                .call(print_opts_ref, &[array_ptr, count, zero_opts]);
            let zero = builder.ins().iconst(types::I64, 0);
            ctx.value_map.insert(*dst, zero);
        }
        "len" => {
            // len() builtin - for arrays and strings, return length
            if !args.is_empty() {
                let arg_id = args[0];
                let val_type = ctx.value_types.get(&arg_id).cloned();
                let arg_val = ctx.value_map.get(&arg_id).copied();
                match (val_type, arg_val) {
                    (Some(AotValueType::Array(elem_t, _)), Some(ptr)) => {
                        let meta_size = elem_t.metadata_size();
                        let len_v = if meta_size == 4 {
                            let l32 = builder.ins().load(types::I32, MemFlags::new(), ptr, 0);
                            builder.ins().sextend(types::I64, l32)
                        } else {
                            builder.ins().load(types::I64, MemFlags::new(), ptr, 0)
                        };
                        ctx.value_map.insert(*dst, len_v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                    (Some(AotValueType::Tuple(elem_types)), _) => {
                        let v = builder.ins().iconst(types::I64, elem_types.len() as i64);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                    (Some(AotValueType::String), _) => {
                        let v = builder.ins().iconst(types::I64, 0);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                    _ => {
                        let v = builder.ins().iconst(types::I64, 0);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "argc" => {
            // argc builtin - return argument count
            if let Some(argc_global) = ctx.argc_global {
                let argc_gv = module.declare_data_in_func(argc_global, builder.func);
                let argc_addr = builder.ins().global_value(types::I64, argc_gv);
                let argc_val = builder
                    .ins()
                    .load(types::I32, MemFlags::new(), argc_addr, 0);
                // Convert i32 to i64
                let argc_i64 = builder.ins().sextend(types::I64, argc_val);
                ctx.value_map.insert(*dst, argc_i64);
                ctx.value_types.insert(*dst, AotValueType::Int);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "argv" => {
            // argv builtin - call adesh_get_argv() to get actual arguments as JSON array
            let mut get_argv_sig = module.make_signature();
            get_argv_sig.returns.push(AbiParam::new(types::I64)); // returns const char*

            let get_argv_func = module
                .declare_function(
                    "adesh_get_argv",
                    cranelift_module::Linkage::Import,
                    &get_argv_sig,
                )
                .map_err(|e| format!("Failed to declare adesh_get_argv: {}", e))?;
            let get_argv_ref = module.declare_func_in_func(get_argv_func, builder.func);

            let call = builder.ins().call(get_argv_ref, &[]);
            let result = builder.inst_results(call)[0];
            ctx.value_map.insert(*dst, result);
            ctx.value_types.insert(*dst, AotValueType::String);
        }
        "arg" => {
            // arg(index) builtin - call adesh_get_arg(index) to get specific argument
            if !args.is_empty() {
                let index_id = args[0];
                if let Some(index_val) = ctx.value_map.get(&index_id).copied() {
                    // Declare adesh_get_arg(int index) -> const char*
                    let mut get_arg_sig = module.make_signature();
                    get_arg_sig.params.push(AbiParam::new(types::I32)); // index
                    get_arg_sig.returns.push(AbiParam::new(types::I64)); // returns const char*

                    let get_arg_func = module
                        .declare_function(
                            "adesh_get_arg",
                            cranelift_module::Linkage::Import,
                            &get_arg_sig,
                        )
                        .map_err(|e| format!("Failed to declare adesh_get_arg: {}", e))?;
                    let get_arg_ref = module.declare_func_in_func(get_arg_func, builder.func);

                    // Convert index to i32
                    let index_i32 = builder.ins().ireduce(types::I32, index_val);

                    let call = builder.ins().call(get_arg_ref, &[index_i32]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::String);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "execName" => {
            // execName builtin - return executable name (argv[0])
            // Call C runtime function
            let exec_name_sig = module.make_signature();
            let mut exec_name_sig = exec_name_sig;
            exec_name_sig.returns.push(AbiParam::new(types::I64));
            let exec_name_func = module
                .declare_function("adesh_exec_name", Linkage::Import, &exec_name_sig)
                .expect("Failed to declare adesh_exec_name");
            let exec_name_ref = module.declare_func_in_func(exec_name_func, builder.func);
            let call = builder.ins().call(exec_name_ref, &[]);
            let result = builder.inst_results(call)[0];
            ctx.value_map.insert(*dst, result);
            ctx.value_types.insert(*dst, AotValueType::String);
        }
        "type" | "typeof" => {
            // typeof builtin - return type name as string
            if !args.is_empty() {
                let arg_id = args[0];
                let val_type = ctx.value_types.get(&arg_id);

                if ctx.object_properties.contains_key(&arg_id) {
                    let type_data_id = ctx
                        .string_data
                        .get("object")
                        .or_else(|| ctx.string_data.get("unknown"))
                        .copied();
                    if let Some(type_data_id) = type_data_id {
                        let type_gv = module.declare_data_in_func(type_data_id, builder.func);
                        let type_ptr = builder.ins().global_value(types::I64, type_gv);
                        ctx.value_map.insert(*dst, type_ptr);
                        ctx.value_types.insert(*dst, AotValueType::String);
                        return Ok(());
                    }
                }

                // Distinguish null pointers from non-null pointers at runtime.
                if matches!(val_type, Some(AotValueType::Ptr)) {
                    if let Some(&arg_val) = ctx.value_map.get(&arg_id) {
                        if let (Some(&null_data_id), Some(&ptr_data_id)) =
                            (ctx.string_data.get("null"), ctx.string_data.get("pointer"))
                        {
                            let null_gv = module.declare_data_in_func(null_data_id, builder.func);
                            let null_ptr = builder.ins().global_value(types::I64, null_gv);
                            let ptr_gv = module.declare_data_in_func(ptr_data_id, builder.func);
                            let ptr_ptr = builder.ins().global_value(types::I64, ptr_gv);

                            let is_null = builder.ins().icmp_imm(IntCC::Equal, arg_val, 0);
                            let type_ptr = builder.ins().select(is_null, null_ptr, ptr_ptr);
                            ctx.value_map.insert(*dst, type_ptr);
                            ctx.value_types.insert(*dst, AotValueType::String);
                            return Ok(());
                        }
                    }
                }

                let type_str: String = match val_type {
                    Some(AotValueType::Array(elem_type, _)) => {
                        // Format as [element_type]
                        format!("[{}]", elem_type.element_type_name())
                    }
                    Some(AotValueType::Set(elem_type, _)) => {
                        format!("{{{}}}", elem_type.element_type_name())
                    }
                    Some(AotValueType::Tuple(_)) => "tuple".to_string(),
                    Some(AotValueType::Int) => "number".to_string(),
                    Some(AotValueType::Float) => "number".to_string(),
                    Some(AotValueType::Bool) => "bool".to_string(),
                    Some(AotValueType::Char) => "char".to_string(),
                    Some(AotValueType::String) => "string".to_string(),
                    Some(AotValueType::Ptr) => "pointer".to_string(),
                    Some(AotValueType::Unknown) | None => "unknown".to_string(),
                    // Fixed-width types
                    Some(AotValueType::U8) => "u8".to_string(),
                    Some(AotValueType::U16) => "u16".to_string(),
                    Some(AotValueType::U32) => "u32".to_string(),
                    Some(AotValueType::U64) => "u64".to_string(),
                    Some(AotValueType::U128) => "u128".to_string(),
                    Some(AotValueType::I8) => "i8".to_string(),
                    Some(AotValueType::I16) => "i16".to_string(),
                    Some(AotValueType::I32) => "i32".to_string(),
                    Some(AotValueType::I64) => "i64".to_string(),
                    Some(AotValueType::I128) => "i128".to_string(),
                    Some(AotValueType::F32) => "f32".to_string(),
                    Some(AotValueType::F64) => "f64".to_string(),
                    Some(AotValueType::Handle) => "object".to_string(),
                };

                // Add the type string to data if not present
                if let Some(&type_data_id) = ctx.string_data.get(&type_str) {
                    let type_gv = module.declare_data_in_func(type_data_id, builder.func);
                    let type_ptr = builder.ins().global_value(types::I64, type_gv);
                    ctx.value_map.insert(*dst, type_ptr);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else if let Some(&unknown_data_id) = ctx.string_data.get("unknown") {
                    let type_gv = module.declare_data_in_func(unknown_data_id, builder.func);
                    let type_ptr = builder.ins().global_value(types::I64, type_gv);
                    ctx.value_map.insert(*dst, type_ptr);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::String);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "hasKey" => {
            // hasKey(object, key) -> bool
            if args.len() >= 2 {
                let obj_id = args[0];
                let key_id = args[1];

                let exists = if let Some(key) = ctx.const_strings.get(&key_id) {
                    ctx.object_properties
                        .get(&obj_id)
                        .map(|props| props.contains_key(key))
                        .unwrap_or(false)
                } else {
                    false
                };

                let result = builder.ins().iconst(types::I8, if exists { 1 } else { 0 });
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::Bool);
                ctx.const_bools.insert(*dst, exists);
            } else {
                let result = builder.ins().iconst(types::I8, 0);
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::Bool);
                ctx.const_bools.insert(*dst, false);
            }
        }
        "sizeof" => {
            // sizeof builtin - return size in bytes based on type
            if !args.is_empty() {
                let arg_id = args[0];
                let val_type = ctx.value_types.get(&arg_id);

                let size: i64 = if let Some(props) = ctx.object_properties.get(&arg_id) {
                    let base: i64 = 48;
                    let entries: i64 = props
                        .iter()
                        .map(|(k, (val_id, val_ty))| {
                            let val_sz: i64 = match val_ty {
                                AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                                AotValueType::U16 | AotValueType::I16 => 2,
                                AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                                AotValueType::U128 | AotValueType::I128 => 16,
                                AotValueType::String => {
                                    if let Some(s) = ctx.const_strings.get(val_id) {
                                        s.len() as i64
                                    } else {
                                        8
                                    }
                                }
                                _ => 8,
                            };
                            (k.len() as i64) + val_sz
                        })
                        .sum();
                    base + entries
                } else {
                    match val_type {
                        Some(AotValueType::U8)
                        | Some(AotValueType::I8)
                        | Some(AotValueType::Bool) => 1,
                        Some(AotValueType::Char) => 4,
                        Some(AotValueType::U16) | Some(AotValueType::I16) => 2,
                        Some(AotValueType::U32)
                        | Some(AotValueType::I32)
                        | Some(AotValueType::F32) => 4,
                        Some(AotValueType::U64)
                        | Some(AotValueType::I64)
                        | Some(AotValueType::F64)
                        | Some(AotValueType::Int)
                        | Some(AotValueType::Float)
                        | Some(AotValueType::Ptr) => 8,
                        Some(AotValueType::U128) | Some(AotValueType::I128) => 16,
                        Some(AotValueType::String) => 8, // pointer size
                        Some(AotValueType::Array(elem_type, len)) => {
                            // Calculate total array size: element_size * length + metadata
                            let elem_size = match **elem_type {
                                AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                                AotValueType::U16 | AotValueType::I16 => 2,
                                AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                                AotValueType::U64
                                | AotValueType::I64
                                | AotValueType::F64
                                | AotValueType::Int
                                | AotValueType::Float
                                | AotValueType::Ptr => 8,
                                AotValueType::U128 | AotValueType::I128 => 16,
                                _ => 8,
                            };
                            (elem_size * (*len as i64)) + elem_type.metadata_size()
                        }
                        Some(AotValueType::Set(elem_type, len)) => {
                            let elem_size = match **elem_type {
                                AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                                AotValueType::U16 | AotValueType::I16 => 2,
                                AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                                AotValueType::U64
                                | AotValueType::I64
                                | AotValueType::F64
                                | AotValueType::Int
                                | AotValueType::Float
                                | AotValueType::Ptr => 8,
                                AotValueType::U128 | AotValueType::I128 => 16,
                                _ => 8,
                            };
                            (elem_size * (*len as i64)) + elem_type.metadata_size()
                        }
                        Some(AotValueType::Tuple(elem_types)) => {
                            // Tuple size: 8 bytes per element (stored as i64)
                            8 * (elem_types.len() as i64)
                        }
                        Some(AotValueType::Handle) => 8, // handle is a u64
                        Some(AotValueType::Unknown) | None => 0,
                    }
                };
                let v = builder.ins().iconst(types::I64, size);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "str" => {
            // str() conversion - for now just pass through
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                ctx.value_map.insert(*dst, val);
                ctx.value_types.insert(*dst, AotValueType::String);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "char" => {
            // char() conversion to a codepoint stored as I32 for printing as '%c'.
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let char_val = match val_type {
                    AotValueType::String => {
                        // ASCII-first-byte fallback for AOT char conversion.
                        let first = builder.ins().load(types::I8, MemFlags::new(), val, 0);
                        builder.ins().uextend(types::I32, first)
                    }
                    AotValueType::Bool => {
                        let i64_val = builder.ins().uextend(types::I64, val);
                        builder.ins().ireduce(types::I32, i64_val)
                    }
                    AotValueType::U8
                    | AotValueType::U16
                    | AotValueType::U32
                    | AotValueType::U64
                    | AotValueType::U128
                    | AotValueType::I8
                    | AotValueType::I16
                    | AotValueType::I32
                    | AotValueType::I64
                    | AotValueType::I128
                    | AotValueType::Int => {
                        let i64_val = if builder.func.dfg.value_type(val).bits() < 64 {
                            builder.ins().uextend(types::I64, val)
                        } else {
                            val
                        };
                        builder.ins().ireduce(types::I32, i64_val)
                    }
                    AotValueType::F32 | AotValueType::F64 | AotValueType::Float => {
                        let i64_val = builder.ins().fcvt_to_uint(types::I64, val);
                        builder.ins().ireduce(types::I32, i64_val)
                    }
                    _ => builder.ins().iconst(types::I32, 0),
                };
                ctx.value_map.insert(*dst, char_val);
                ctx.value_types.insert(*dst, AotValueType::Char);
            } else {
                let v = builder.ins().iconst(types::I32, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Char);
            }
        }
        "int" => {
            // int() conversion
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let int_val = match val_type {
                    AotValueType::Float => builder.ins().fcvt_to_sint(types::I64, val),
                    AotValueType::Bool => builder.ins().uextend(types::I64, val),
                    _ => val,
                };
                ctx.value_map.insert(*dst, int_val);
                ctx.value_types.insert(*dst, AotValueType::Int);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "float" => {
            // float() conversion
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let float_val = match val_type {
                    AotValueType::Float => val,
                    AotValueType::Int => builder.ins().fcvt_from_sint(types::F64, val),
                    AotValueType::Bool => {
                        let int_val = builder.ins().uextend(types::I64, val);
                        builder.ins().fcvt_from_sint(types::F64, int_val)
                    }
                    _ => builder.ins().f64const(0.0),
                };
                ctx.value_map.insert(*dst, float_val);
                ctx.value_types.insert(*dst, AotValueType::Float);
            } else {
                let v = builder.ins().f64const(0.0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Float);
            }
        }
        "f32" => {
            // f32() conversion
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let float_val = match val_type {
                    AotValueType::F32 => val,
                    AotValueType::F64 | AotValueType::Float => {
                        builder.ins().fdemote(types::F32, val)
                    }
                    AotValueType::Int => {
                        let f64_val = builder.ins().fcvt_from_sint(types::F64, val);
                        builder.ins().fdemote(types::F32, f64_val)
                    }
                    AotValueType::I8
                    | AotValueType::I16
                    | AotValueType::I32
                    | AotValueType::I64
                    | AotValueType::I128 => {
                        let ext_val = builder.ins().sextend(types::I64, val);
                        let f64_val = builder.ins().fcvt_from_sint(types::F64, ext_val);
                        builder.ins().fdemote(types::F32, f64_val)
                    }
                    AotValueType::U8
                    | AotValueType::U16
                    | AotValueType::U32
                    | AotValueType::U64
                    | AotValueType::U128 => {
                        let ext_val = builder.ins().uextend(types::I64, val);
                        let f64_val = builder.ins().fcvt_from_uint(types::F64, ext_val);
                        builder.ins().fdemote(types::F32, f64_val)
                    }
                    _ => builder.ins().f32const(0.0),
                };
                ctx.value_map.insert(*dst, float_val);
                ctx.value_types.insert(*dst, AotValueType::F32);
            } else {
                let v = builder.ins().f32const(0.0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::F32);
            }
        }
        "f64" => {
            // f64() conversion
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let float_val = match val_type {
                    AotValueType::F64 | AotValueType::Float => val,
                    AotValueType::F32 => builder.ins().fpromote(types::F64, val),
                    AotValueType::Int => builder.ins().fcvt_from_sint(types::F64, val),
                    AotValueType::I8
                    | AotValueType::I16
                    | AotValueType::I32
                    | AotValueType::I64
                    | AotValueType::I128 => {
                        let ext_val = builder.ins().sextend(types::I64, val);
                        builder.ins().fcvt_from_sint(types::F64, ext_val)
                    }
                    AotValueType::U8
                    | AotValueType::U16
                    | AotValueType::U32
                    | AotValueType::U64
                    | AotValueType::U128 => {
                        let ext_val = builder.ins().uextend(types::I64, val);
                        builder.ins().fcvt_from_uint(types::F64, ext_val)
                    }
                    _ => builder.ins().f64const(0.0),
                };
                ctx.value_map.insert(*dst, float_val);
                ctx.value_types.insert(*dst, AotValueType::F64);
            } else {
                let v = builder.ins().f64const(0.0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::F64);
            }
        }
        "u8" | "u16" | "u32" | "u64" | "u128" => {
            // Unsigned integer conversion
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let (target_type, aot_type) = match builtin_name {
                    "u8" => (types::I8, AotValueType::U8),
                    "u16" => (types::I16, AotValueType::U16),
                    "u32" => (types::I32, AotValueType::U32),
                    "u64" => (types::I64, AotValueType::U64),
                    "u128" => (types::I128, AotValueType::U128),
                    _ => (types::I64, AotValueType::U64),
                };

                let int_val = match val_type {
                    AotValueType::U8
                    | AotValueType::U16
                    | AotValueType::U32
                    | AotValueType::U64
                    | AotValueType::U128
                    | AotValueType::I8
                    | AotValueType::I16
                    | AotValueType::I32
                    | AotValueType::I64
                    | AotValueType::I128
                    | AotValueType::Int => {
                        if target_type.bits() < 64 {
                            builder.ins().ireduce(target_type, val)
                        } else if target_type.bits() > 64 {
                            builder.ins().uextend(target_type, val)
                        } else {
                            val
                        }
                    }
                    AotValueType::F32 | AotValueType::F64 | AotValueType::Float => {
                        let i64_val = builder.ins().fcvt_to_uint(types::I64, val);
                        if target_type.bits() < 64 {
                            builder.ins().ireduce(target_type, i64_val)
                        } else if target_type.bits() > 64 {
                            builder.ins().uextend(target_type, i64_val)
                        } else {
                            i64_val
                        }
                    }
                    _ => builder.ins().iconst(target_type, 0),
                };
                ctx.value_map.insert(*dst, int_val);
                ctx.value_types.insert(*dst, aot_type);
            } else {
                let target_type = match builtin_name {
                    "u8" => types::I8,
                    "u16" => types::I16,
                    "u32" => types::I32,
                    "u64" => types::I64,
                    "u128" => types::I128,
                    _ => types::I64,
                };
                let aot_type = match builtin_name {
                    "u8" => AotValueType::U8,
                    "u16" => AotValueType::U16,
                    "u32" => AotValueType::U32,
                    "u64" => AotValueType::U64,
                    "u128" => AotValueType::U128,
                    _ => AotValueType::U64,
                };
                let v = builder.ins().iconst(target_type, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, aot_type);
            }
        }
        "i8" | "i16" | "i32" | "i64" | "i128" => {
            // Signed integer conversion
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let (target_type, aot_type) = match builtin_name {
                    "i8" => (types::I8, AotValueType::I8),
                    "i16" => (types::I16, AotValueType::I16),
                    "i32" => (types::I32, AotValueType::I32),
                    "i64" => (types::I64, AotValueType::I64),
                    "i128" => (types::I128, AotValueType::I128),
                    _ => (types::I64, AotValueType::I64),
                };

                let int_val = match val_type {
                    AotValueType::U8
                    | AotValueType::U16
                    | AotValueType::U32
                    | AotValueType::U64
                    | AotValueType::U128
                    | AotValueType::I8
                    | AotValueType::I16
                    | AotValueType::I32
                    | AotValueType::I64
                    | AotValueType::I128
                    | AotValueType::Int => {
                        if target_type.bits() < 64 {
                            builder.ins().ireduce(target_type, val)
                        } else if target_type.bits() > 64 {
                            builder.ins().sextend(target_type, val)
                        } else {
                            val
                        }
                    }
                    AotValueType::F32 | AotValueType::F64 | AotValueType::Float => {
                        let i64_val = builder.ins().fcvt_to_sint(types::I64, val);
                        if target_type.bits() < 64 {
                            builder.ins().ireduce(target_type, i64_val)
                        } else if target_type.bits() > 64 {
                            builder.ins().sextend(target_type, i64_val)
                        } else {
                            i64_val
                        }
                    }
                    _ => builder.ins().iconst(target_type, 0),
                };
                ctx.value_map.insert(*dst, int_val);
                ctx.value_types.insert(*dst, aot_type);
            } else {
                let target_type = match builtin_name {
                    "i8" => types::I8,
                    "i16" => types::I16,
                    "i32" => types::I32,
                    "i64" => types::I64,
                    "i128" => types::I128,
                    _ => types::I64,
                };
                let aot_type = match builtin_name {
                    "i8" => AotValueType::I8,
                    "i16" => AotValueType::I16,
                    "i32" => AotValueType::I32,
                    "i64" => AotValueType::I64,
                    "i128" => AotValueType::I128,
                    _ => AotValueType::I64,
                };
                let v = builder.ins().iconst(target_type, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, aot_type);
            }
        }
        "bool" => {
            // bool() conversion
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let bool_val = match val_type {
                    AotValueType::Bool => val,
                    AotValueType::Int => {
                        let zero = builder.ins().iconst(types::I64, 0);
                        let one = builder.ins().iconst(types::I8, 1);
                        let zero_i8 = builder.ins().iconst(types::I8, 0);
                        let cmp = builder.ins().icmp(IntCC::NotEqual, val, zero);
                        builder.ins().select(cmp, one, zero_i8)
                    }
                    AotValueType::Float => {
                        let zero = builder.ins().f64const(0.0);
                        let one = builder.ins().iconst(types::I8, 1);
                        let zero_i8 = builder.ins().iconst(types::I8, 0);
                        let cmp = builder.ins().fcmp(FloatCC::NotEqual, val, zero);
                        builder.ins().select(cmp, one, zero_i8)
                    }
                    _ => builder.ins().iconst(types::I8, 0),
                };
                ctx.value_map.insert(*dst, bool_val);
                ctx.value_types.insert(*dst, AotValueType::Bool);
            } else {
                let v = builder.ins().iconst(types::I8, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Bool);
            }
        }
        "add" => {
            if args.len() >= 2 {
                let a_id = args[0];
                let b_id = args[1];
                let mut va = ctx
                    .value_map
                    .get(&a_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let mut vb = ctx
                    .value_map
                    .get(&b_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let type_a = ctx
                    .value_types
                    .get(&a_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);
                let type_b = ctx
                    .value_types
                    .get(&b_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                if matches!(type_a, AotValueType::String) || matches!(type_b, AotValueType::String)
                {
                    if !matches!(type_a, AotValueType::String) {
                        va = match type_a {
                            AotValueType::Float | AotValueType::F64 => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::F64));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function(
                                        "adesh_double_to_string",
                                        Linkage::Import,
                                        &sig,
                                    )
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let f64v = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                                let c = builder.ins().call(rf, &[f64v]);
                                builder.inst_results(c)[0]
                            }
                            AotValueType::F32 => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::F64));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function(
                                        "adesh_double_to_string",
                                        Linkage::Import,
                                        &sig,
                                    )
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let f64v = builder.ins().fpromote(types::F64, va);
                                let c = builder.ins().call(rf, &[f64v]);
                                builder.inst_results(c)[0]
                            }
                            AotValueType::Bool => {
                                if let (Some(&t), Some(&f)) =
                                    (ctx.string_data.get("true"), ctx.string_data.get("false"))
                                {
                                    let tg = module.declare_data_in_func(t, builder.func);
                                    let fg = module.declare_data_in_func(f, builder.func);
                                    let tp = builder.ins().global_value(types::I64, tg);
                                    let fp = builder.ins().global_value(types::I64, fg);
                                    let vi64 = builder.ins().uextend(types::I64, va);
                                    let zero = builder.ins().iconst(types::I64, 0);
                                    let is_true = builder.ins().icmp(IntCC::NotEqual, vi64, zero);
                                    builder.ins().select(is_true, tp, fp)
                                } else {
                                    let mut sig = module.make_signature();
                                    sig.params.push(AbiParam::new(types::I64));
                                    sig.returns.push(AbiParam::new(types::I64));
                                    let f = module
                                        .declare_function(
                                            "adesh_int_to_string",
                                            Linkage::Import,
                                            &sig,
                                        )
                                        .unwrap();
                                    let rf = module.declare_func_in_func(f, builder.func);
                                    let vi64 = builder.ins().uextend(types::I64, va);
                                    let c = builder.ins().call(rf, &[vi64]);
                                    builder.inst_results(c)[0]
                                }
                            }
                            AotValueType::Array(ref elem_type, arr_len) => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::I64));
                                sig.params.push(AbiParam::new(types::I64));
                                sig.params.push(AbiParam::new(types::I64));
                                sig.params.push(AbiParam::new(types::I8));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function(
                                        "adesh_array_to_string",
                                        Linkage::Import,
                                        &sig,
                                    )
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let len_v = builder.ins().iconst(types::I64, arr_len as i64);
                                let elem_sz = match **elem_type {
                                    AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                                    AotValueType::U16 | AotValueType::I16 => 2,
                                    AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                                    AotValueType::U64
                                    | AotValueType::I64
                                    | AotValueType::F64
                                    | AotValueType::Int
                                    | AotValueType::Float
                                    | AotValueType::Ptr => 8,
                                    AotValueType::U128 | AotValueType::I128 => 16,
                                    _ => 8,
                                };
                                let es_v = builder.ins().iconst(types::I64, elem_sz as i64);
                                let isf_v = builder.ins().iconst(
                                    types::I8,
                                    if matches!(
                                        **elem_type,
                                        AotValueType::F32 | AotValueType::F64 | AotValueType::Float
                                    ) {
                                        1
                                    } else {
                                        0
                                    },
                                );
                                let c = builder.ins().call(rf, &[va, len_v, es_v, isf_v]);
                                builder.inst_results(c)[0]
                            }
                            _ => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::I64));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function("adesh_int_to_string", Linkage::Import, &sig)
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let vi64 = if builder.func.dfg.value_type(va).bits() < 64 {
                                    builder.ins().uextend(types::I64, va)
                                } else {
                                    va
                                };
                                let c = builder.ins().call(rf, &[vi64]);
                                builder.inst_results(c)[0]
                            }
                        };
                    }

                    if !matches!(type_b, AotValueType::String) {
                        vb = match type_b {
                            AotValueType::Float | AotValueType::F64 => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::F64));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function(
                                        "adesh_double_to_string",
                                        Linkage::Import,
                                        &sig,
                                    )
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let f64v = builder.ins().bitcast(types::F64, MemFlags::new(), vb);
                                let c = builder.ins().call(rf, &[f64v]);
                                builder.inst_results(c)[0]
                            }
                            AotValueType::F32 => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::F64));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function(
                                        "adesh_double_to_string",
                                        Linkage::Import,
                                        &sig,
                                    )
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let f64v = builder.ins().fpromote(types::F64, vb);
                                let c = builder.ins().call(rf, &[f64v]);
                                builder.inst_results(c)[0]
                            }
                            AotValueType::Bool => {
                                if let (Some(&t), Some(&f)) =
                                    (ctx.string_data.get("true"), ctx.string_data.get("false"))
                                {
                                    let tg = module.declare_data_in_func(t, builder.func);
                                    let fg = module.declare_data_in_func(f, builder.func);
                                    let tp = builder.ins().global_value(types::I64, tg);
                                    let fp = builder.ins().global_value(types::I64, fg);
                                    let vi64 = builder.ins().uextend(types::I64, vb);
                                    let zero = builder.ins().iconst(types::I64, 0);
                                    let is_true = builder.ins().icmp(IntCC::NotEqual, vi64, zero);
                                    builder.ins().select(is_true, tp, fp)
                                } else {
                                    let mut sig = module.make_signature();
                                    sig.params.push(AbiParam::new(types::I64));
                                    sig.returns.push(AbiParam::new(types::I64));
                                    let f = module
                                        .declare_function(
                                            "adesh_int_to_string",
                                            Linkage::Import,
                                            &sig,
                                        )
                                        .unwrap();
                                    let rf = module.declare_func_in_func(f, builder.func);
                                    let vi64 = builder.ins().uextend(types::I64, vb);
                                    let c = builder.ins().call(rf, &[vi64]);
                                    builder.inst_results(c)[0]
                                }
                            }
                            AotValueType::Array(ref elem_type, arr_len) => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::I64));
                                sig.params.push(AbiParam::new(types::I64));
                                sig.params.push(AbiParam::new(types::I64));
                                sig.params.push(AbiParam::new(types::I8));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function(
                                        "adesh_array_to_string",
                                        Linkage::Import,
                                        &sig,
                                    )
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let len_v = builder.ins().iconst(types::I64, arr_len as i64);
                                let elem_sz = match **elem_type {
                                    AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                                    AotValueType::U16 | AotValueType::I16 => 2,
                                    AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                                    AotValueType::U64
                                    | AotValueType::I64
                                    | AotValueType::F64
                                    | AotValueType::Int
                                    | AotValueType::Float
                                    | AotValueType::Ptr => 8,
                                    AotValueType::U128 | AotValueType::I128 => 16,
                                    _ => 8,
                                };
                                let es_v = builder.ins().iconst(types::I64, elem_sz as i64);
                                let isf_v = builder.ins().iconst(
                                    types::I8,
                                    if matches!(
                                        **elem_type,
                                        AotValueType::F32 | AotValueType::F64 | AotValueType::Float
                                    ) {
                                        1
                                    } else {
                                        0
                                    },
                                );
                                let c = builder.ins().call(rf, &[vb, len_v, es_v, isf_v]);
                                builder.inst_results(c)[0]
                            }
                            _ => {
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::I64));
                                sig.returns.push(AbiParam::new(types::I64));
                                let f = module
                                    .declare_function("adesh_int_to_string", Linkage::Import, &sig)
                                    .unwrap();
                                let rf = module.declare_func_in_func(f, builder.func);
                                let vi64 = if builder.func.dfg.value_type(vb).bits() < 64 {
                                    builder.ins().uextend(types::I64, vb)
                                } else {
                                    vb
                                };
                                let c = builder.ins().call(rf, &[vi64]);
                                builder.inst_results(c)[0]
                            }
                        };
                    }

                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(types::I64));
                    sig.params.push(AbiParam::new(types::I64));
                    sig.returns.push(AbiParam::new(types::I64));
                    let concat_func = module
                        .declare_function("adesh_string_concat", Linkage::Import, &sig)
                        .expect("Failed to declare adesh_string_concat");
                    let concat_ref = module.declare_func_in_func(concat_func, builder.func);
                    let call = builder.ins().call(concat_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else {
                    let result = builder.ins().iadd(va, vb);
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "sub" | "mul" | "div" | "mod" => {
            // Basic arithmetic operations
            if args.len() >= 2 {
                let va = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let vb = ctx
                    .value_map
                    .get(&args[1])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let result = match builtin_name {
                    "sub" => builder.ins().isub(va, vb),
                    "mul" => builder.ins().imul(va, vb),
                    "div" => builder.ins().sdiv(va, vb),
                    "mod" => builder.ins().srem(va, vb),
                    _ => builder.ins().iconst(types::I64, 0),
                };
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::Int);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "abs" => {
            // Absolute value
            if !args.is_empty() {
                let val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let abs_val = match val_type {
                    AotValueType::Float => builder.ins().fabs(val),
                    AotValueType::Int => {
                        // Integer abs: (x ^ (x >> 63)) - (x >> 63)
                        let shift = builder.ins().iconst(types::I64, 63);
                        let sign = builder.ins().sshr(val, shift);
                        let xor_val = builder.ins().bxor(val, sign);
                        builder.ins().isub(xor_val, sign)
                    }
                    _ => val,
                };
                ctx.value_map.insert(*dst, abs_val);
                ctx.value_types.insert(*dst, val_type);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "min" | "max" => {
            // Min/max of two values
            if args.len() >= 2 {
                let va = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let vb = ctx
                    .value_map
                    .get(&args[1])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let result = match val_type {
                    AotValueType::Float => {
                        if builtin_name == "min" {
                            builder.ins().fmin(va, vb)
                        } else {
                            builder.ins().fmax(va, vb)
                        }
                    }
                    _ => {
                        let cc = if builtin_name == "min" {
                            IntCC::SignedLessThan
                        } else {
                            IntCC::SignedGreaterThan
                        };
                        let cmp = builder.ins().icmp(cc, va, vb);
                        builder.ins().select(cmp, va, vb)
                    }
                };
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, val_type);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "make_object" => {
            // Create a new runtime object with optional key-value pairs.
            // Arguments come in pairs: key1, value1, key2, value2, ...
            let ctors = RuntimeValueConstructors::declare(module, builder)?;
            let mut new_props = HashMap::new();
            let mut kv_handles: Vec<Value> = Vec::new();

            // Process key-value pairs
            for chunk in args.chunks(2) {
                if chunk.len() == 2 {
                    let key_id = chunk[0];
                    let val_id = chunk[1];

                    // Get field name from const_strings
                    if let Some(field_name) = ctx.const_strings.get(&key_id).cloned() {
                        // Get the value type
                        let val_type = ctx
                            .value_types
                            .get(&val_id)
                            .cloned()
                            .unwrap_or(AotValueType::Unknown);

                        // Add the field
                        new_props.insert(field_name, (val_id, val_type.clone()));

                        if let Some(&key_ptr) = ctx.value_map.get(&key_id) {
                            let key_call = builder.ins().call(ctors.make_string, &[key_ptr]);
                            let key_handle = builder.inst_results(key_call)[0];
                            kv_handles.push(key_handle);

                            let value_handle = if let Some(&value_raw) = ctx.value_map.get(&val_id)
                            {
                                convert_val_to_handle(
                                    ctx, builder, module, &val_id, value_raw, &val_type, &ctors,
                                )?
                            } else {
                                let call = builder.ins().call(ctors.make_null, &[]);
                                builder.inst_results(call)[0]
                            };

                            kv_handles.push(value_handle);
                        }
                    }
                }
            }

            // Store properties for compile-time resolution
            ctx.object_properties.insert(*dst, new_props);

            let obj_handle = if kv_handles.is_empty() {
                let zero = builder.ins().iconst(types::I64, 0);
                let count = builder.ins().iconst(types::I64, 0);
                let call = builder.ins().call(ctors.make_obj, &[zero, count]);
                builder.inst_results(call)[0]
            } else {
                let handles_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (kv_handles.len() * 8) as u32,
                    8,
                ));
                let handles_ptr = builder.ins().stack_addr(types::I64, handles_slot, 0);
                for (idx, handle) in kv_handles.iter().enumerate() {
                    builder
                        .ins()
                        .store(MemFlags::new(), *handle, handles_ptr, (idx * 8) as i32);
                }
                let count = builder.ins().iconst(types::I64, kv_handles.len() as i64);
                let call = builder.ins().call(ctors.make_obj, &[handles_ptr, count]);
                builder.inst_results(call)[0]
            };

            ctx.value_map.insert(*dst, obj_handle);
            ctx.value_types.insert(*dst, AotValueType::Handle);
            ctx.runtime_handle_values.insert(*dst);
        }
        "set_field" => {
            // Set a field on an object - used for print options
            // args[0] = object, args[1] = field name, args[2] = value
            // Returns a new object with the field set (immutable style)
            if args.len() >= 3 {
                let obj_id = args[0];
                let field_id = args[1];
                let val_id = args[2];

                // Start with properties from the old object (if any)
                let mut new_props = ctx
                    .object_properties
                    .get(&obj_id)
                    .cloned()
                    .unwrap_or_default();

                // Get field name from const_strings
                if let Some(field_name) = ctx.const_strings.get(&field_id).cloned() {
                    // Get the value type
                    let val_type = ctx
                        .value_types
                        .get(&val_id)
                        .cloned()
                        .unwrap_or(AotValueType::Unknown);

                    // If this is a boolean value, ensure it's registered in const_bools
                    if val_type == AotValueType::Bool {
                        // Check if the value is already in const_bools
                        if !ctx.const_bools.contains_key(&val_id) {
                            // Try to get the actual boolean value from value_map
                            if let Some(val) = ctx.value_map.get(&val_id) {
                                // The value is stored as an i8 (0 or 1), convert to bool
                                // For now, we'll assume true if the value is non-zero
                                // This is a workaround - ideally we'd have the original bool value
                                let bool_val = match val {
                                    v if v.to_string().contains("1") => true,
                                    _ => false,
                                };
                                ctx.const_bools.insert(val_id, bool_val);
                            }
                        }
                    }

                    // Add the new field
                    new_props.insert(field_name, (val_id, val_type));
                }

                // Store properties for the new object (dst)
                ctx.object_properties.insert(*dst, new_props);

                if let (Some(&obj_handle), Some(&field_ptr)) =
                    (ctx.value_map.get(&obj_id), ctx.value_map.get(&field_id))
                {
                    let ctors = RuntimeValueConstructors::declare(module, builder)?;

                    let mut set_field_sig = module.make_signature();
                    set_field_sig.params.push(AbiParam::new(types::I64));
                    set_field_sig.params.push(AbiParam::new(types::I64));
                    set_field_sig.params.push(AbiParam::new(types::I64));
                    set_field_sig.returns.push(AbiParam::new(types::I64));
                    let set_field_func = module
                        .declare_function("aot_set_field", Linkage::Import, &set_field_sig)
                        .map_err(|e| format!("Failed to declare aot_set_field: {}", e))?;
                    let set_field_ref = module.declare_func_in_func(set_field_func, builder.func);

                    let field_call = builder.ins().call(ctors.make_string, &[field_ptr]);
                    let field_handle = builder.inst_results(field_call)[0];

                    let val_type = ctx
                        .value_types
                        .get(&val_id)
                        .cloned()
                        .unwrap_or(AotValueType::Unknown);
                    let value_handle = if let Some(&value_raw) = ctx.value_map.get(&val_id) {
                        convert_val_to_handle(
                            ctx, builder, module, &val_id, value_raw, &val_type, &ctors,
                        )?
                    } else {
                        let call = builder.ins().call(ctors.make_null, &[]);
                        builder.inst_results(call)[0]
                    };

                    let set_call = builder
                        .ins()
                        .call(set_field_ref, &[obj_handle, field_handle, value_handle]);
                    let new_handle = builder.inst_results(set_call)[0];
                    ctx.value_map.insert(*dst, new_handle);
                    ctx.value_types.insert(*dst, AotValueType::Handle);
                    ctx.runtime_handle_values.insert(*dst);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Handle);
                    ctx.runtime_handle_values.insert(*dst);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            }
        }

        "Date" => {
            // Date() constructor - return current timestamp (stub: return 0 for now)
            let v = builder.ins().iconst(types::I64, 0);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::Int);
        }
        "__method_toISOString" => {
            // Date.toISOString() - return ISO string (stub: return "1970-01-01T00:00:00.000Z")
            if let Some(&iso_str_id) = ctx.string_data.get("1970-01-01T00:00:00.000Z") {
                let iso_str_gv = module.declare_data_in_func(iso_str_id, builder.func);
                let iso_str_ptr = builder.ins().global_value(types::I64, iso_str_gv);
                ctx.value_map.insert(*dst, iso_str_ptr);
                ctx.value_types.insert(*dst, AotValueType::String);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
            }
        }
        "make_array" | "make_set" | "makeSet" => {
            let is_set = matches!(builtin_name, "make_set" | "makeSet");
            // Create an array from elements using stack allocation
            if !args.is_empty() {
                let elem_types: Vec<AotValueType> = args
                    .iter()
                    .map(|arg_id| {
                        ctx.value_types
                            .get(arg_id)
                            .cloned()
                            .unwrap_or(AotValueType::Int)
                    })
                    .collect();
                let first_type = unify_array_elem_types(&elem_types);

                let array_len = args.len();
                let array_type = if is_set {
                    AotValueType::Set(Box::new(first_type.clone()), array_len)
                } else {
                    AotValueType::Array(Box::new(first_type.clone()), array_len)
                };

                // Determine element size in bytes
                let elem_size: i64 = match first_type {
                    AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                    AotValueType::U16 | AotValueType::I16 => 2,
                    AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                    AotValueType::U64
                    | AotValueType::I64
                    | AotValueType::F64
                    | AotValueType::Int
                    | AotValueType::Float
                    | AotValueType::Ptr
                    | AotValueType::Handle => 8,
                    AotValueType::U128 | AotValueType::I128 => 16,
                    _ => 8,
                };

                // Calculate metadata size
                let metadata_size = first_type.metadata_size() as u32;

                // Total size = metadata + (element_size * length)
                let data_size = (elem_size as u32) * (array_len as u32);
                let total_size = metadata_size + data_size;

                // Use stack allocation for arrays
                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    total_size,
                    8, // 8-byte alignment
                ));

                // Get pointer to the stack slot
                let array_ptr = builder.ins().stack_addr(types::I64, stack_slot, 0);

                let len_i64 = builder.ins().iconst(types::I64, array_len as i64);
                if metadata_size == 4 {
                    let len_i32 = builder.ins().ireduce(types::I32, len_i64);
                    builder.ins().store(MemFlags::new(), len_i32, array_ptr, 0);
                    builder.ins().store(MemFlags::new(), len_i32, array_ptr, 4);
                } else {
                    builder.ins().store(MemFlags::new(), len_i64, array_ptr, 0);
                    builder.ins().store(MemFlags::new(), len_i64, array_ptr, 8);
                }

                // Data starts after metadata
                let data_offset = metadata_size as i64;

                let ctors = if first_type == AotValueType::Handle {
                    Some(RuntimeValueConstructors::declare(module, builder)?)
                } else {
                    None
                };

                // Store each element in the array
                for (i, arg_id) in args.iter().enumerate() {
                    let offset = data_offset + (i as i64 * elem_size);
                    let offset_val = builder.ins().iconst(types::I64, offset);
                    let elem_ptr = builder.ins().iadd(array_ptr, offset_val);

                    // Get the actual type of this element
                    let elem_type = ctx
                        .value_types
                        .get(arg_id)
                        .cloned()
                        .unwrap_or(AotValueType::Int);

                    if first_type == AotValueType::Handle {
                        let ctors_ref = ctors.as_ref().unwrap();
                        let raw_val = ctx
                            .value_map
                            .get(arg_id)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle_val = convert_val_to_handle(
                            ctx, builder, module, arg_id, raw_val, &elem_type, ctors_ref,
                        )?;
                        builder
                            .ins()
                            .store(MemFlags::new(), handle_val, elem_ptr, 0);
                        continue;
                    }

                    if let Some(elem_val) = ctx.value_map.get(arg_id).copied() {
                        let offset = data_offset + (i as i64 * elem_size);
                        let offset_val = builder.ins().iconst(types::I64, offset);
                        let elem_ptr = builder.ins().iadd(array_ptr, offset_val);

                        // Get the actual type of this element
                        let elem_type = ctx
                            .value_types
                            .get(arg_id)
                            .cloned()
                            .unwrap_or(AotValueType::Int);

                        // Check if the element is a float type first
                        let is_float = matches!(
                            elem_type,
                            AotValueType::F32 | AotValueType::F64 | AotValueType::Float
                        );

                        // Store based on element type
                        match first_type {
                            AotValueType::F32 => {
                                let fval =
                                    if matches!(elem_type, AotValueType::F64 | AotValueType::Float)
                                    {
                                        builder.ins().fdemote(types::F32, elem_val)
                                    } else {
                                        elem_val
                                    };
                                builder.ins().store(MemFlags::new(), fval, elem_ptr, 0);
                            }
                            AotValueType::F64 | AotValueType::Float => {
                                let fval = if matches!(elem_type, AotValueType::F32) {
                                    builder.ins().fpromote(types::F64, elem_val)
                                } else {
                                    elem_val
                                };
                                builder.ins().store(MemFlags::new(), fval, elem_ptr, 0);
                            }
                            _ if is_float => {
                                // Source is float but target is integer - just store directly
                                builder.ins().store(MemFlags::new(), elem_val, elem_ptr, 0);
                            }
                            _ => {
                                let val_type = builder.func.dfg.value_type(elem_val);
                                let is_signed = matches!(
                                    elem_type,
                                    AotValueType::I8
                                        | AotValueType::I16
                                        | AotValueType::I32
                                        | AotValueType::I64
                                        | AotValueType::Int
                                );

                                match elem_size {
                                    1 => {
                                        let v = if val_type == types::I8 {
                                            elem_val
                                        } else {
                                            builder.ins().ireduce(types::I8, elem_val)
                                        };
                                        builder.ins().store(MemFlags::new(), v, elem_ptr, 0);
                                    }
                                    2 => {
                                        let v = if val_type == types::I16 {
                                            elem_val
                                        } else if val_type.bits() > 16 {
                                            builder.ins().ireduce(types::I16, elem_val)
                                        } else if is_signed {
                                            builder.ins().sextend(types::I16, elem_val)
                                        } else {
                                            builder.ins().uextend(types::I16, elem_val)
                                        };
                                        builder.ins().store(MemFlags::new(), v, elem_ptr, 0);
                                    }
                                    4 => {
                                        let v = if val_type == types::I32 {
                                            elem_val
                                        } else if val_type.bits() > 32 {
                                            builder.ins().ireduce(types::I32, elem_val)
                                        } else if is_signed {
                                            builder.ins().sextend(types::I32, elem_val)
                                        } else {
                                            builder.ins().uextend(types::I32, elem_val)
                                        };
                                        builder.ins().store(MemFlags::new(), v, elem_ptr, 0);
                                    }
                                    8 => {
                                        let v = if val_type == types::I64 {
                                            elem_val
                                        } else if val_type.bits() > 64 {
                                            builder.ins().ireduce(types::I64, elem_val)
                                        } else if is_signed {
                                            builder.ins().sextend(types::I64, elem_val)
                                        } else {
                                            builder.ins().uextend(types::I64, elem_val)
                                        };
                                        builder.ins().store(MemFlags::new(), v, elem_ptr, 0);
                                    }
                                    16 => {
                                        builder.ins().store(MemFlags::new(), elem_val, elem_ptr, 0);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                ctx.value_map.insert(*dst, array_ptr);
                ctx.value_types.insert(*dst, array_type);
                if is_set {
                    ctx.array_capacity.insert(*dst, -(array_len as i64));
                } else {
                    ctx.array_capacity.insert(*dst, array_len as i64);
                }
            } else {
                // Empty array - just store metadata with length 0
                let array_type = if is_set {
                    AotValueType::Set(Box::new(AotValueType::Int), 0)
                } else {
                    AotValueType::Array(Box::new(AotValueType::Int), 0)
                };
                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    16, // Just metadata
                    8,
                ));
                let array_ptr = builder.ins().stack_addr(types::I64, stack_slot, 0);

                let zero = builder.ins().iconst(types::I64, 0);
                builder.ins().store(MemFlags::new(), zero, array_ptr, 0);
                builder.ins().store(MemFlags::new(), zero, array_ptr, 8);

                ctx.value_map.insert(*dst, array_ptr);
                ctx.value_types.insert(*dst, array_type);
                if is_set {
                    ctx.array_capacity.insert(*dst, 0);
                }
            }
        }
        "array_to_fixed" => {
            if args.len() >= 3 {
                let array_id = args[0];
                let type_id = args[1];
                let n_id = args[2];
                let src_ptr = ctx
                    .value_map
                    .get(&array_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let (src_elem_type, current_len) = match ctx.value_types.get(&array_id).cloned() {
                    Some(AotValueType::Array(et, len)) => (*et, len),
                    _ => (AotValueType::Int, 0),
                };
                let elem_type_str = ctx
                    .const_strings
                    .get(&type_id)
                    .cloned()
                    .unwrap_or_else(|| "any".to_string());
                let dst_elem_type = match elem_type_str.as_str() {
                    "u8" => AotValueType::U8,
                    "i8" => AotValueType::I8,
                    "u16" => AotValueType::U16,
                    "i16" => AotValueType::I16,
                    "u32" => AotValueType::U32,
                    "i32" => AotValueType::I32,
                    "u64" => AotValueType::U64,
                    "i64" => AotValueType::I64,
                    "u128" => AotValueType::U128,
                    "i128" => AotValueType::I128,
                    "f32" => AotValueType::F32,
                    "f64" | "float" => AotValueType::F64,
                    "string" | "str" => AotValueType::String,
                    "bool" | "boolean" => AotValueType::Bool,
                    _ => AotValueType::Int,
                };
                let dst_elem_size: i64 = match dst_elem_type {
                    AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                    AotValueType::U16 | AotValueType::I16 => 2,
                    AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                    AotValueType::U64
                    | AotValueType::I64
                    | AotValueType::F64
                    | AotValueType::Int
                    | AotValueType::Float
                    | AotValueType::Ptr => 8,
                    AotValueType::U128 | AotValueType::I128 => 16,
                    _ => 8,
                };
                let src_elem_size: i64 = match src_elem_type {
                    AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                    AotValueType::U16 | AotValueType::I16 => 2,
                    AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                    AotValueType::U64
                    | AotValueType::I64
                    | AotValueType::F64
                    | AotValueType::Int
                    | AotValueType::Float
                    | AotValueType::Ptr => 8,
                    AotValueType::U128 | AotValueType::I128 => 16,
                    _ => 8,
                };
                let dst_meta = dst_elem_type.metadata_size() as i64;
                let src_meta = src_elem_type.metadata_size() as i64;
                let data_size = dst_elem_size * (current_len as i64);
                let total_size = (dst_meta as i64) + data_size;
                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    total_size as u32,
                    8,
                ));
                let dst_ptr = builder.ins().stack_addr(types::I64, stack_slot, 0);
                let len_i64 = builder.ins().iconst(types::I64, current_len as i64);
                let cap_val = ctx
                    .const_ints
                    .get(&n_id)
                    .copied()
                    .unwrap_or(current_len as i64);
                let cap_i64 = builder.ins().iconst(types::I64, cap_val);
                if dst_meta == 4 {
                    let len_i32 = builder.ins().ireduce(types::I32, len_i64);
                    let cap_i32 = builder.ins().ireduce(types::I32, cap_i64);
                    builder.ins().store(MemFlags::new(), len_i32, dst_ptr, 0);
                    builder.ins().store(MemFlags::new(), cap_i32, dst_ptr, 4);
                } else {
                    builder.ins().store(MemFlags::new(), len_i64, dst_ptr, 0);
                    builder.ins().store(MemFlags::new(), cap_i64, dst_ptr, 8);
                }
                for i in 0..current_len {
                    let i_val = builder.ins().iconst(types::I64, i as i64);
                    // Avoid nested builder.ins() borrows by precomputing constants
                    let src_elem_size_val = builder.ins().iconst(types::I64, src_elem_size);
                    let src_off_bytes = builder.ins().imul(i_val, src_elem_size_val);
                    let src_meta_val = builder.ins().iconst(types::I64, src_meta);
                    let src_data_off = builder.ins().iadd(src_off_bytes, src_meta_val);
                    let src_elem_ptr = builder.ins().iadd(src_ptr, src_data_off);
                    let loaded = match src_elem_type {
                        AotValueType::F32 => {
                            builder
                                .ins()
                                .load(types::F32, MemFlags::new(), src_elem_ptr, 0)
                        }
                        AotValueType::F64 | AotValueType::Float => {
                            builder
                                .ins()
                                .load(types::F64, MemFlags::new(), src_elem_ptr, 0)
                        }
                        _ => match src_elem_size {
                            1 => {
                                let b =
                                    builder
                                        .ins()
                                        .load(types::I8, MemFlags::new(), src_elem_ptr, 0);
                                builder.ins().uextend(types::I64, b)
                            }
                            2 => {
                                let s = builder.ins().load(
                                    types::I16,
                                    MemFlags::new(),
                                    src_elem_ptr,
                                    0,
                                );
                                builder.ins().uextend(types::I64, s)
                            }
                            4 => {
                                let w = builder.ins().load(
                                    types::I32,
                                    MemFlags::new(),
                                    src_elem_ptr,
                                    0,
                                );
                                builder.ins().uextend(types::I64, w)
                            }
                            _ => builder
                                .ins()
                                .load(types::I64, MemFlags::new(), src_elem_ptr, 0),
                        },
                    };
                    let dst_elem_size_val = builder.ins().iconst(types::I64, dst_elem_size);
                    let dst_off_bytes = builder.ins().imul(i_val, dst_elem_size_val);
                    let dst_meta_val = builder.ins().iconst(types::I64, dst_meta);
                    let dst_data_off = builder.ins().iadd(dst_off_bytes, dst_meta_val);
                    let dst_elem_ptr = builder.ins().iadd(dst_ptr, dst_data_off);
                    match dst_elem_type {
                        AotValueType::F32 => {
                            let f = builder.ins().bitcast(types::F32, MemFlags::new(), loaded);
                            builder.ins().store(MemFlags::new(), f, dst_elem_ptr, 0);
                        }
                        AotValueType::F64 | AotValueType::Float => {
                            let f = builder.ins().bitcast(types::F64, MemFlags::new(), loaded);
                            builder.ins().store(MemFlags::new(), f, dst_elem_ptr, 0);
                        }
                        _ => match dst_elem_size {
                            1 => {
                                let r = builder.ins().ireduce(types::I8, loaded);
                                builder.ins().store(MemFlags::new(), r, dst_elem_ptr, 0);
                            }
                            2 => {
                                let r = builder.ins().ireduce(types::I16, loaded);
                                builder.ins().store(MemFlags::new(), r, dst_elem_ptr, 0);
                            }
                            4 => {
                                let r = builder.ins().ireduce(types::I32, loaded);
                                builder.ins().store(MemFlags::new(), r, dst_elem_ptr, 0);
                            }
                            _ => {
                                builder
                                    .ins()
                                    .store(MemFlags::new(), loaded, dst_elem_ptr, 0);
                            }
                        },
                    }
                }
                ctx.value_map.insert(*dst, dst_ptr);
                ctx.value_types.insert(
                    *dst,
                    AotValueType::Array(Box::new(dst_elem_type), current_len),
                );
                ctx.array_capacity.insert(*dst, cap_val);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "first" => {
            // Get first element of array (same as get_index(arr, 0))
            if !args.is_empty() {
                let array_id = args[0];

                if let Some(AotValueType::Array(elem_type, _)) =
                    ctx.value_types.get(&array_id).cloned()
                {
                    let array_ptr = ctx
                        .value_map
                        .get(&array_id)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

                    let elem_size: i64 = match *elem_type {
                        AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                        AotValueType::U16 | AotValueType::I16 => 2,
                        AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                        AotValueType::U64
                        | AotValueType::I64
                        | AotValueType::F64
                        | AotValueType::Int
                        | AotValueType::Float
                        | AotValueType::Ptr => 8,
                        _ => 8,
                    };

                    let metadata_size = elem_type.metadata_size();
                    let elem_ptr_offset = builder.ins().iconst(types::I64, metadata_size);
                    let elem_ptr = builder.ins().iadd(array_ptr, elem_ptr_offset);

                    let elem_val = match *elem_type {
                        AotValueType::F32 => {
                            builder.ins().load(types::F32, MemFlags::new(), elem_ptr, 0)
                        }
                        AotValueType::F64 | AotValueType::Float => {
                            builder.ins().load(types::F64, MemFlags::new(), elem_ptr, 0)
                        }
                        _ => match elem_size {
                            1 => {
                                let byte =
                                    builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                                builder.ins().uextend(types::I64, byte)
                            }
                            2 => {
                                let short =
                                    builder.ins().load(types::I16, MemFlags::new(), elem_ptr, 0);
                                builder.ins().uextend(types::I64, short)
                            }
                            4 => {
                                let int =
                                    builder.ins().load(types::I32, MemFlags::new(), elem_ptr, 0);
                                builder.ins().uextend(types::I64, int)
                            }
                            _ => builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0),
                        },
                    };

                    ctx.value_map.insert(*dst, elem_val);
                    ctx.value_types.insert(*dst, *elem_type);
                } else if let Some(AotValueType::Tuple(elem_types)) =
                    ctx.value_types.get(&array_id).cloned()
                {
                    let tuple_ptr = ctx
                        .value_map
                        .get(&array_id)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                    // First element at offset 0 (stored as i64 unless float)
                    let loaded = builder
                        .ins()
                        .load(types::I64, MemFlags::new(), tuple_ptr, 0);
                    let elem_type = elem_types.get(0).cloned().unwrap_or(AotValueType::Int);
                    let elem_val = match elem_type {
                        AotValueType::F32 => {
                            builder.ins().bitcast(types::F32, MemFlags::new(), loaded)
                        }
                        AotValueType::F64 | AotValueType::Float => {
                            builder.ins().bitcast(types::F64, MemFlags::new(), loaded)
                        }
                        _ => loaded,
                    };
                    ctx.value_map.insert(*dst, elem_val);
                    ctx.value_types.insert(*dst, elem_type);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "last" => {
            // Get last element of array
            if !args.is_empty() {
                let array_id = args[0];

                if let Some(AotValueType::Array(elem_type, arr_len)) =
                    ctx.value_types.get(&array_id).cloned()
                {
                    if arr_len > 0 {
                        let array_ptr = ctx
                            .value_map
                            .get(&array_id)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

                        let elem_size: i64 = match *elem_type {
                            AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                            AotValueType::U16 | AotValueType::I16 => 2,
                            AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                            AotValueType::U64
                            | AotValueType::I64
                            | AotValueType::F64
                            | AotValueType::Int
                            | AotValueType::Float
                            | AotValueType::Ptr => 8,
                            _ => 8,
                        };

                        let metadata_size = elem_type.metadata_size();
                        let last_offset = metadata_size + (elem_size * (arr_len as i64 - 1));
                        let elem_ptr_offset = builder.ins().iconst(types::I64, last_offset);
                        let elem_ptr = builder.ins().iadd(array_ptr, elem_ptr_offset);

                        let elem_val = match *elem_type {
                            AotValueType::F32 => {
                                builder.ins().load(types::F32, MemFlags::new(), elem_ptr, 0)
                            }
                            AotValueType::F64 | AotValueType::Float => {
                                builder.ins().load(types::F64, MemFlags::new(), elem_ptr, 0)
                            }
                            _ => match elem_size {
                                1 => {
                                    let byte =
                                        builder.ins().load(types::I8, MemFlags::new(), elem_ptr, 0);
                                    builder.ins().uextend(types::I64, byte)
                                }
                                2 => {
                                    let short = builder.ins().load(
                                        types::I16,
                                        MemFlags::new(),
                                        elem_ptr,
                                        0,
                                    );
                                    builder.ins().uextend(types::I64, short)
                                }
                                4 => {
                                    let int = builder.ins().load(
                                        types::I32,
                                        MemFlags::new(),
                                        elem_ptr,
                                        0,
                                    );
                                    builder.ins().uextend(types::I64, int)
                                }
                                _ => builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0),
                            },
                        };

                        ctx.value_map.insert(*dst, elem_val);
                        ctx.value_types.insert(*dst, *elem_type);
                    } else {
                        let v = builder.ins().iconst(types::I64, 0);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                } else if let Some(AotValueType::Tuple(elem_types)) =
                    ctx.value_types.get(&array_id).cloned()
                {
                    if !elem_types.is_empty() {
                        let tuple_ptr = ctx
                            .value_map
                            .get(&array_id)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let idx = elem_types.len() as i64 - 1;
                        let off = builder.ins().iconst(types::I64, idx * 8);
                        let elem_ptr = builder.ins().iadd(tuple_ptr, off);
                        let loaded = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);
                        let elem_type = elem_types.last().cloned().unwrap_or(AotValueType::Int);
                        let elem_val = match elem_type {
                            AotValueType::F32 => {
                                builder.ins().bitcast(types::F32, MemFlags::new(), loaded)
                            }
                            AotValueType::F64 | AotValueType::Float => {
                                builder.ins().bitcast(types::F64, MemFlags::new(), loaded)
                            }
                            _ => loaded,
                        };
                        ctx.value_map.insert(*dst, elem_val);
                        ctx.value_types.insert(*dst, elem_type);
                    } else {
                        let v = builder.ins().iconst(types::I64, 0);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "capacity" => {
            // Return array capacity; prefer fixed-size override when available
            if !args.is_empty() {
                let arg_id = args[0];
                if let Some(&cap) = ctx.array_capacity.get(&arg_id) {
                    let v = builder.ins().iconst(types::I64, cap);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                } else if let Some(AotValueType::Array(_, len)) = ctx.value_types.get(&arg_id) {
                    let v = builder.ins().iconst(types::I64, *len as i64);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "metadata_size" => {
            // Return array metadata overhead based on element type
            if !args.is_empty() {
                let arg_id = args[0];
                if let Some(AotValueType::Array(elem_type, _)) = ctx.value_types.get(&arg_id) {
                    let size = elem_type.metadata_size();
                    let v = builder.ins().iconst(types::I64, size);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "make_array_spread" => {
            if !args.is_empty() {
                // Args are pairs: (value, is_spread)
                let mut total_len: usize = 0;
                let mut elem_type: AotValueType = AotValueType::Int;
                let mut i = 0;
                while i + 1 < args.len() {
                    let val_id = args[i];
                    let spread_flag_id = args[i + 1];
                    let is_spread = ctx
                        .const_bools
                        .get(&spread_flag_id)
                        .copied()
                        .unwrap_or(false);
                    if is_spread {
                        if let Some(AotValueType::Array(et, len)) =
                            ctx.value_types.get(&val_id).cloned()
                        {
                            elem_type = *et;
                            total_len += len as usize;
                        } else {
                            total_len += 1;
                        }
                    } else {
                        // Not spread, single element
                        if let Some(t) = ctx.value_types.get(&val_id).cloned() {
                            elem_type = t;
                        }
                        total_len += 1;
                    }
                    i += 2;
                }

                // Determine element size
                let elem_size: i64 = match elem_type {
                    AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                    AotValueType::U16 | AotValueType::I16 => 2,
                    AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                    AotValueType::U64
                    | AotValueType::I64
                    | AotValueType::F64
                    | AotValueType::Int
                    | AotValueType::Float
                    | AotValueType::Ptr
                    | AotValueType::Handle => 8,
                    AotValueType::U128 | AotValueType::I128 => 16,
                    _ => 8,
                };

                let metadata_size = elem_type.metadata_size() as u32;
                let data_size = (elem_size as u32) * (total_len as u32);
                let total_size = metadata_size + data_size;

                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    total_size,
                    8,
                ));
                let array_ptr = builder.ins().stack_addr(types::I64, stack_slot, 0);

                let len_i64 = builder.ins().iconst(types::I64, total_len as i64);
                if metadata_size == 4 {
                    let len_i32 = builder.ins().ireduce(types::I32, len_i64);
                    builder.ins().store(MemFlags::new(), len_i32, array_ptr, 0);
                    builder.ins().store(MemFlags::new(), len_i32, array_ptr, 4);
                } else {
                    builder.ins().store(MemFlags::new(), len_i64, array_ptr, 0);
                    builder.ins().store(MemFlags::new(), len_i64, array_ptr, 8);
                }

                let mut dst_index: i64 = 0;
                let data_offset = metadata_size as i64;
                let mut j = 0;
                while j + 1 < args.len() {
                    let val_id = args[j];
                    let spread_flag_id = args[j + 1];
                    let is_spread = ctx
                        .const_bools
                        .get(&spread_flag_id)
                        .copied()
                        .unwrap_or(false);
                    if is_spread {
                        if let Some(AotValueType::Array(src_elem_type, src_len)) =
                            ctx.value_types.get(&val_id).cloned()
                        {
                            let src_ptr = ctx
                                .value_map
                                .get(&val_id)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let src_meta = src_elem_type.metadata_size() as i64;
                            let src_elem_size: i64 = match *src_elem_type {
                                AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                                AotValueType::U16 | AotValueType::I16 => 2,
                                AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                                AotValueType::U64
                                | AotValueType::I64
                                | AotValueType::F64
                                | AotValueType::Int
                                | AotValueType::Float
                                | AotValueType::Ptr
                                | AotValueType::Handle => 8,
                                AotValueType::U128 | AotValueType::I128 => 16,
                                _ => 8,
                            };
                            let copy_len = src_len as i64;
                            for k in 0..copy_len {
                                let src_off = src_meta + (k * src_elem_size);
                                let dst_off = data_offset + (dst_index * elem_size);
                                let src_off_val = builder.ins().iconst(types::I64, src_off);
                                let dst_off_val = builder.ins().iconst(types::I64, dst_off);
                                let src_elem_ptr = builder.ins().iadd(src_ptr, src_off_val);
                                let dst_elem_ptr = builder.ins().iadd(array_ptr, dst_off_val);
                                let loaded_val = match src_elem_size {
                                    1 => builder.ins().load(
                                        types::I8,
                                        MemFlags::new(),
                                        src_elem_ptr,
                                        0,
                                    ),
                                    2 => builder.ins().load(
                                        types::I16,
                                        MemFlags::new(),
                                        src_elem_ptr,
                                        0,
                                    ),
                                    4 => {
                                        if matches!(*src_elem_type, AotValueType::F32) {
                                            builder.ins().load(
                                                types::F32,
                                                MemFlags::new(),
                                                src_elem_ptr,
                                                0,
                                            )
                                        } else {
                                            builder.ins().load(
                                                types::I32,
                                                MemFlags::new(),
                                                src_elem_ptr,
                                                0,
                                            )
                                        }
                                    }
                                    _ => {
                                        if matches!(
                                            *src_elem_type,
                                            AotValueType::F64 | AotValueType::Float
                                        ) {
                                            builder.ins().load(
                                                types::F64,
                                                MemFlags::new(),
                                                src_elem_ptr,
                                                0,
                                            )
                                        } else {
                                            builder.ins().load(
                                                types::I64,
                                                MemFlags::new(),
                                                src_elem_ptr,
                                                0,
                                            )
                                        }
                                    }
                                };
                                match elem_size {
                                    1 => {
                                        let v8 = builder.ins().ireduce(types::I8, loaded_val);
                                        builder.ins().store(MemFlags::new(), v8, dst_elem_ptr, 0);
                                    }
                                    2 => {
                                        let v16 = if src_elem_size > 2 {
                                            builder.ins().ireduce(types::I16, loaded_val)
                                        } else if src_elem_size < 2 {
                                            builder.ins().uextend(types::I16, loaded_val)
                                        } else {
                                            loaded_val
                                        };
                                        builder.ins().store(MemFlags::new(), v16, dst_elem_ptr, 0);
                                    }
                                    4 => {
                                        if matches!(elem_type, AotValueType::F32) {
                                            let to_store = if matches!(
                                                *src_elem_type,
                                                AotValueType::F64 | AotValueType::Float
                                            ) {
                                                builder.ins().fdemote(types::F32, loaded_val)
                                            } else if matches!(*src_elem_type, AotValueType::F32) {
                                                loaded_val
                                            } else {
                                                let i32v = if src_elem_size > 4 {
                                                    builder.ins().ireduce(types::I32, loaded_val)
                                                } else if src_elem_size < 4 {
                                                    builder.ins().uextend(types::I32, loaded_val)
                                                } else {
                                                    loaded_val
                                                };
                                                builder.ins().fcvt_from_sint(types::F32, i32v)
                                            };
                                            builder.ins().store(
                                                MemFlags::new(),
                                                to_store,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        } else {
                                            let i32v = if src_elem_size > 4 {
                                                builder.ins().ireduce(types::I32, loaded_val)
                                            } else if src_elem_size < 4 {
                                                builder.ins().uextend(types::I32, loaded_val)
                                            } else {
                                                loaded_val
                                            };
                                            builder.ins().store(
                                                MemFlags::new(),
                                                i32v,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        }
                                    }
                                    _ => {
                                        if matches!(
                                            elem_type,
                                            AotValueType::F64 | AotValueType::Float
                                        ) {
                                            let to_store =
                                                if matches!(*src_elem_type, AotValueType::F32) {
                                                    builder.ins().fpromote(types::F64, loaded_val)
                                                } else if matches!(
                                                    *src_elem_type,
                                                    AotValueType::F64 | AotValueType::Float
                                                ) {
                                                    loaded_val
                                                } else {
                                                    builder
                                                        .ins()
                                                        .fcvt_from_sint(types::F64, loaded_val)
                                                };
                                            builder.ins().store(
                                                MemFlags::new(),
                                                to_store,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        } else {
                                            let i64v = if src_elem_size < 8 {
                                                builder.ins().uextend(types::I64, loaded_val)
                                            } else {
                                                loaded_val
                                            };
                                            builder.ins().store(
                                                MemFlags::new(),
                                                i64v,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        }
                                    }
                                }
                                dst_index += 1;
                            }
                        } else {
                            // Treat as single element
                            if let Some(elem_val) = ctx.value_map.get(&val_id).copied() {
                                let dst_off = data_offset + (dst_index * elem_size);
                                let dst_off_val = builder.ins().iconst(types::I64, dst_off);
                                let dst_elem_ptr = builder.ins().iadd(array_ptr, dst_off_val);
                                match elem_size {
                                    1 => {
                                        let v8 = builder.ins().ireduce(types::I8, elem_val);
                                        builder.ins().store(MemFlags::new(), v8, dst_elem_ptr, 0);
                                    }
                                    2 => {
                                        let v16 = builder.ins().ireduce(types::I16, elem_val);
                                        builder.ins().store(MemFlags::new(), v16, dst_elem_ptr, 0);
                                    }
                                    4 => {
                                        if matches!(elem_type, AotValueType::F32) {
                                            let f32v =
                                                builder.ins().fcvt_from_sint(types::F32, elem_val);
                                            builder.ins().store(
                                                MemFlags::new(),
                                                f32v,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        } else {
                                            let i32v = builder.ins().ireduce(types::I32, elem_val);
                                            builder.ins().store(
                                                MemFlags::new(),
                                                i32v,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        }
                                    }
                                    _ => {
                                        if matches!(
                                            elem_type,
                                            AotValueType::F64 | AotValueType::Float
                                        ) {
                                            let f64v =
                                                builder.ins().fcvt_from_sint(types::F64, elem_val);
                                            builder.ins().store(
                                                MemFlags::new(),
                                                f64v,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        } else {
                                            builder.ins().store(
                                                MemFlags::new(),
                                                elem_val,
                                                dst_elem_ptr,
                                                0,
                                            );
                                        }
                                    }
                                }
                                dst_index += 1;
                            }
                        }
                    } else {
                        if let Some(elem_val) = ctx.value_map.get(&val_id).copied() {
                            let dst_off = data_offset + (dst_index * elem_size);
                            let dst_off_val = builder.ins().iconst(types::I64, dst_off);
                            let dst_elem_ptr = builder.ins().iadd(array_ptr, dst_off_val);
                            let store_type = match elem_type {
                                AotValueType::F32 => types::F32,
                                AotValueType::F64 | AotValueType::Float => types::F64,
                                AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => {
                                    types::I8
                                }
                                AotValueType::U16 | AotValueType::I16 => types::I16,
                                AotValueType::U32 | AotValueType::I32 => types::I32,
                                _ => types::I64,
                            };
                            if store_type == types::F32 || store_type == types::F64 {
                                builder
                                    .ins()
                                    .store(MemFlags::new(), elem_val, dst_elem_ptr, 0);
                            }
                            dst_index += 1;
                        }
                    }
                    j += 2;
                }

                ctx.value_map.insert(*dst, array_ptr);
                ctx.value_types.insert(
                    *dst,
                    AotValueType::Array(Box::new(elem_type.clone()), total_len),
                );
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types
                    .insert(*dst, AotValueType::Array(Box::new(AotValueType::Int), 0));
            }
        }
        "array_get" | "get_index" => {
            // Get element from array or tuple at index: get_index(container, index) -> element
            if args.len() >= 2 {
                let container_id = args[0];
                let index_id = args[1];

                let container_type = ctx.value_types.get(&container_id).cloned();
                let container_ptr = ctx
                    .value_map
                    .get(&container_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let index_val = ctx
                    .value_map
                    .get(&index_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

                match container_type {
                    Some(AotValueType::Array(elem_type, _)) => {
                        // Handle arrays
                        // Determine element size
                        let elem_size: i64 = match *elem_type {
                            AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                            AotValueType::U16 | AotValueType::I16 => 2,
                            AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                            AotValueType::U64
                            | AotValueType::I64
                            | AotValueType::F64
                            | AotValueType::Int
                            | AotValueType::Float
                            | AotValueType::Ptr
                            | AotValueType::Handle => 8,
                            AotValueType::U128 | AotValueType::I128 => 16,
                            _ => 8,
                        };

                        let metadata_size = elem_type.metadata_size();

                        // Calculate offset: metadata_size + (index * elem_size)
                        let elem_size_val = builder.ins().iconst(types::I64, elem_size);
                        let byte_offset = builder.ins().imul(index_val, elem_size_val);
                        let metadata_offset = builder.ins().iconst(types::I64, metadata_size);
                        let total_offset = builder.ins().iadd(metadata_offset, byte_offset);

                        // Calculate element address
                        let elem_ptr = builder.ins().iadd(container_ptr, total_offset);

                        // Load element based on size and type
                        let elem_val = match *elem_type {
                            AotValueType::F32 => {
                                builder.ins().load(types::F32, MemFlags::new(), elem_ptr, 0)
                            }
                            AotValueType::F64 | AotValueType::Float => {
                                builder.ins().load(types::F64, MemFlags::new(), elem_ptr, 0)
                            }
                            _ => {
                                // Integer types - load and extend to I64
                                match elem_size {
                                    1 => {
                                        let byte = builder.ins().load(
                                            types::I8,
                                            MemFlags::new(),
                                            elem_ptr,
                                            0,
                                        );
                                        builder.ins().uextend(types::I64, byte)
                                    }
                                    2 => {
                                        let short = builder.ins().load(
                                            types::I16,
                                            MemFlags::new(),
                                            elem_ptr,
                                            0,
                                        );
                                        builder.ins().uextend(types::I64, short)
                                    }
                                    4 => {
                                        let int = builder.ins().load(
                                            types::I32,
                                            MemFlags::new(),
                                            elem_ptr,
                                            0,
                                        );
                                        builder.ins().uextend(types::I64, int)
                                    }
                                    8 | 16 => {
                                        builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0)
                                    }
                                    _ => builder.ins().iconst(types::I64, 0),
                                }
                            }
                        };

                        let is_handle = *elem_type == AotValueType::Handle;
                        ctx.value_map.insert(*dst, elem_val);
                        ctx.value_types.insert(*dst, *elem_type);
                        if is_handle {
                            ctx.runtime_handle_values.insert(*dst);
                        }
                    }
                    Some(AotValueType::Tuple(elem_types)) => {
                        // Handle tuples - elements are stored as i64 at fixed 8-byte offsets
                        // Calculate offset: index * 8
                        let eight = builder.ins().iconst(types::I64, 8);
                        let elem_offset = builder.ins().imul(index_val, eight);
                        let elem_ptr = builder.ins().iadd(container_ptr, elem_offset);

                        // Load element (stored as i64)
                        let stored_val =
                            builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);

                        // Get the element type - check if index is a constant
                        let elem_type = if let Some(&const_index) = ctx.const_ints.get(&index_id) {
                            let idx = const_index as usize;
                            elem_types.get(idx).cloned().unwrap_or(AotValueType::Int)
                        } else {
                            // Fallback for non-constant indices
                            AotValueType::Int
                        };

                        let elem_val = match elem_type {
                            AotValueType::F32 => {
                                // Bitcast back from i64 to f32
                                builder
                                    .ins()
                                    .bitcast(types::F32, MemFlags::new(), stored_val)
                            }
                            AotValueType::F64 | AotValueType::Float => {
                                // Bitcast back from i64 to f64
                                builder
                                    .ins()
                                    .bitcast(types::F64, MemFlags::new(), stored_val)
                            }
                            _ => stored_val,
                        };

                        ctx.value_map.insert(*dst, elem_val);
                        ctx.value_types.insert(*dst, elem_type);
                    }
                    _ => {
                        // Unknown container type
                        let v = builder.ins().iconst(types::I64, 0);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "array_to_raw" => {
            // Convert array to raw array - in AOT, arrays are already stored as pointers
            // so this is a no-op, just return the input
            if !args.is_empty() {
                let input_val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let input_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);
                ctx.value_map.insert(*dst, input_val);
                ctx.value_types.insert(*dst, input_type);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "array_to_dynamic" => {
            // Convert array to dynamic array - in AOT, arrays are already stored as pointers
            // so this is a no-op, just return the input
            if !args.is_empty() {
                let input_val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let input_type = ctx
                    .value_types
                    .get(&args[0])
                    .cloned()
                    .unwrap_or(AotValueType::Int);
                ctx.value_map.insert(*dst, input_val);
                ctx.value_types.insert(*dst, input_type);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "make_tuple" => {
            // Create a tuple from elements using stack allocation
            if !args.is_empty() {
                // Collect the actual types of each element
                let mut elem_types = Vec::new();
                for arg_id in args {
                    let elem_type = ctx
                        .value_types
                        .get(arg_id)
                        .cloned()
                        .unwrap_or(AotValueType::Int);
                    elem_types.push(elem_type);
                }

                let tuple_len = args.len();
                let tuple_type = AotValueType::Tuple(elem_types);

                // Calculate total size: 8 bytes per element (all stored as i64)
                let elem_size: i64 = 8;
                let total_size = elem_size * (tuple_len as i64);

                // Use stack allocation for tuples
                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    total_size as u32,
                    8, // 8-byte alignment
                ));

                // Get pointer to the stack slot
                let tuple_ptr = builder.ins().stack_addr(types::I64, stack_slot, 0);

                // Store each element in the tuple
                for (i, arg_id) in args.iter().enumerate() {
                    if let Some(elem_val) = ctx.value_map.get(arg_id).copied() {
                        let offset = (i as i64) * elem_size;
                        let offset_val = builder.ins().iconst(types::I64, offset);
                        let elem_ptr = builder.ins().iadd(tuple_ptr, offset_val);

                        // Store as i64 (bitcast floats if needed)
                        let elem_type = ctx
                            .value_types
                            .get(arg_id)
                            .cloned()
                            .unwrap_or(AotValueType::Int);
                        let store_val = match elem_type {
                            AotValueType::F32 => {
                                builder.ins().bitcast(types::I64, MemFlags::new(), elem_val)
                            }
                            AotValueType::F64 | AotValueType::Float => {
                                builder.ins().bitcast(types::I64, MemFlags::new(), elem_val)
                            }
                            _ => elem_val,
                        };

                        builder.ins().store(MemFlags::new(), store_val, elem_ptr, 0);
                    }
                }

                ctx.value_map.insert(*dst, tuple_ptr);
                ctx.value_types.insert(*dst, tuple_type);
            } else {
                // Empty tuple
                let tuple_type = AotValueType::Tuple(vec![]);
                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    0,
                    8,
                ));
                let tuple_ptr = builder.ins().stack_addr(types::I64, stack_slot, 0);

                ctx.value_map.insert(*dst, tuple_ptr);
                ctx.value_types.insert(*dst, tuple_type);
            }
        }
        "argsCount" => {
            // argsCount builtin - same as argc
            if let Some(argc_global) = ctx.argc_global {
                let argc_gv = module.declare_data_in_func(argc_global, builder.func);
                let argc_addr = builder.ins().global_value(types::I64, argc_gv);
                let argc_val = builder
                    .ins()
                    .load(types::I32, MemFlags::new(), argc_addr, 0);
                // Convert i32 to i64
                let argc_i64 = builder.ins().sextend(types::I64, argc_val);
                ctx.value_map.insert(*dst, argc_i64);
                ctx.value_types.insert(*dst, AotValueType::Int);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "args" => {
            // args builtin - alias for argv
            // Implement full argv array from C argv

            if let (Some(argc_global), Some(argv_global), Some(malloc_func)) =
                (ctx.argc_global, ctx.argv_global, ctx.malloc_func)
            {
                // Loop blocks
                let header_block = builder.create_block();
                let body_block = builder.create_block();
                let exit_block = builder.create_block();

                // Declare globals
                let argc_gv = module.declare_data_in_func(argc_global, builder.func);
                let argv_gv = module.declare_data_in_func(argv_global, builder.func);
                let malloc_ref = module.declare_func_in_func(malloc_func, builder.func);

                // Load argc
                let argc_addr = builder.ins().global_value(types::I64, argc_gv);
                let argc_val_i32 = builder
                    .ins()
                    .load(types::I32, MemFlags::new(), argc_addr, 0);
                let argc_val = builder.ins().sextend(types::I64, argc_val_i32);

                // Calculate size: 16 + argc * 8
                let const_16 = builder.ins().iconst(types::I64, 16);
                let const_8 = builder.ins().iconst(types::I64, 8);
                let data_size = builder.ins().imul(argc_val, const_8);
                let alloc_size = builder.ins().iadd(const_16, data_size);

                // Allocate array
                let call_inst = builder.ins().call(malloc_ref, &[alloc_size]);
                let array_ptr = builder.inst_results(call_inst)[0];

                // Store length and capacity
                builder.ins().store(MemFlags::new(), argc_val, array_ptr, 0);
                builder.ins().store(MemFlags::new(), argc_val, array_ptr, 8);

                // Load argv pointer (char**)
                let argv_addr_addr = builder.ins().global_value(types::I64, argv_gv);
                let argv_addr = builder
                    .ins()
                    .load(types::I64, MemFlags::new(), argv_addr_addr, 0);

                // Initialize loop
                let const_0 = builder.ins().iconst(types::I64, 0);
                builder.ins().jump(header_block, &[const_0]);

                // Loop Header
                builder.switch_to_block(header_block);
                let i_phi = builder.append_block_param(header_block, types::I64);

                let cmp = builder.ins().icmp(IntCC::SignedLessThan, i_phi, argc_val);
                builder.ins().brif(cmp, body_block, &[], exit_block, &[]);

                // Loop Body
                builder.switch_to_block(body_block);

                // offset = i * 8
                let offset = builder.ins().imul(i_phi, const_8);

                // src = argv[i]
                let src_ptr = builder.ins().iadd(argv_addr, offset);
                let str_ptr = builder.ins().load(types::I64, MemFlags::new(), src_ptr, 0);

                // dst = array.data[i] = array_ptr + 16 + offset
                let dst_base = builder.ins().iadd(array_ptr, const_16);
                let dst_ptr = builder.ins().iadd(dst_base, offset);
                builder.ins().store(MemFlags::new(), str_ptr, dst_ptr, 0);

                // Next iteration
                let const_1 = builder.ins().iconst(types::I64, 1);
                let next_i = builder.ins().iadd(i_phi, const_1);
                builder.ins().jump(header_block, &[next_i]); // loop back

                // Exit
                builder.switch_to_block(exit_block);
                builder.seal_block(header_block);
                builder.seal_block(body_block);
                builder.seal_block(exit_block);

                // Result
                ctx.value_map.insert(*dst, array_ptr);
                ctx.value_types
                    .insert(*dst, AotValueType::Array(Box::new(AotValueType::String), 0));
            } else {
                // Fallback to empty array
                let array_type = AotValueType::Array(Box::new(AotValueType::String), 0);

                // Use stack allocation for empty array
                let metadata_size = AotValueType::String.metadata_size() as u32;
                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    metadata_size,
                    8, // 8-byte alignment
                ));

                // Get pointer to the stack slot
                let array_ptr = builder.ins().stack_addr(types::I64, stack_slot, 0);

                // Zero initialize metadata (len=0, cap=0)
                let const_0 = builder.ins().iconst(types::I64, 0);
                builder.ins().store(MemFlags::new(), const_0, array_ptr, 0);
                builder.ins().store(MemFlags::new(), const_0, array_ptr, 8);

                ctx.value_map.insert(*dst, array_ptr);
                ctx.value_types.insert(*dst, array_type);
            }
        }
        "argsSlice" => {
            // argsSlice(start) builtin - return arguments from start index
            if !args.is_empty() {
                let start_id = args[0];
                if let Some(start_val) = ctx.value_map.get(&start_id).copied() {
                    // Declare external function
                    let args_slice_sig = module.make_signature();
                    let mut args_slice_sig = args_slice_sig;
                    args_slice_sig.params.push(AbiParam::new(types::I64)); // start index
                    args_slice_sig.returns.push(AbiParam::new(types::I64)); // result ptr

                    let args_slice_func = module
                        .declare_function(
                            "adesh_args_slice",
                            cranelift_module::Linkage::Import,
                            &args_slice_sig,
                        )
                        .map_err(|e| format!("Failed to declare adesh_args_slice: {}", e))?;
                    let args_slice_ref = module.declare_func_in_func(args_slice_func, builder.func);

                    let call = builder.ins().call(args_slice_ref, &[start_val]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::String);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "argsJoin" => {
            // argsJoin(separator) builtin - join arguments with separator
            if !args.is_empty() {
                let sep_id = args[0];
                if let Some(sep_val) = ctx.value_map.get(&sep_id).copied() {
                    // Declare external function
                    let args_join_sig = module.make_signature();
                    let mut args_join_sig = args_join_sig;
                    args_join_sig.params.push(AbiParam::new(types::I64)); // separator
                    args_join_sig.returns.push(AbiParam::new(types::I64)); // result ptr

                    let args_join_func = module
                        .declare_function(
                            "adesh_args_join",
                            cranelift_module::Linkage::Import,
                            &args_join_sig,
                        )
                        .map_err(|e| format!("Failed to declare adesh_args_join: {}", e))?;
                    let args_join_ref = module.declare_func_in_func(args_join_func, builder.func);

                    let call = builder.ins().call(args_join_ref, &[sep_val]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::String);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "argsIndexOf" => {
            // argsIndexOf(value) builtin - find index of argument
            if !args.is_empty() {
                let value_id = args[0];
                if let Some(value_val) = ctx.value_map.get(&value_id).copied() {
                    // Declare external function
                    let index_of_sig = module.make_signature();
                    let mut index_of_sig = index_of_sig;
                    index_of_sig.params.push(AbiParam::new(types::I64)); // value
                    index_of_sig.returns.push(AbiParam::new(types::I64)); // index or -1

                    if let Ok(index_of_func) = module.declare_function(
                        "adesh_args_index_of",
                        cranelift_module::Linkage::Import,
                        &index_of_sig,
                    ) {
                        let index_of_ref = module.declare_func_in_func(index_of_func, builder.func);
                        let call = builder.ins().call(index_of_ref, &[value_val]);
                        let result = builder.inst_results(call)[0];
                        ctx.value_map.insert(*dst, result);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    } else {
                        let v = builder.ins().iconst(types::I64, -1);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Int);
                    }
                } else {
                    let v = builder.ins().iconst(types::I64, -1);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, -1);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "parseArgs" => {
            // parseArgs() builtin - parse arguments into structured format
            let parse_args_sig = module.make_signature();
            let mut parse_args_sig = parse_args_sig;
            parse_args_sig.returns.push(AbiParam::new(types::I64)); // result ptr (JSON string)

            if let Ok(parse_args_func) = module.declare_function(
                "adesh_parse_args",
                cranelift_module::Linkage::Import,
                &parse_args_sig,
            ) {
                let parse_args_ref = module.declare_func_in_func(parse_args_func, builder.func);
                let call = builder.ins().call(parse_args_ref, &[]);
                let result = builder.inst_results(call)[0];
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::String);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "argGet" => {
            // argGet(key) builtin - get argument value by key
            if !args.is_empty() {
                let key_id = args[0];
                if let Some(key_val) = ctx.value_map.get(&key_id).copied() {
                    // Declare external function
                    let arg_get_sig = module.make_signature();
                    let mut arg_get_sig = arg_get_sig;
                    arg_get_sig.params.push(AbiParam::new(types::I64)); // key
                    arg_get_sig.returns.push(AbiParam::new(types::I64)); // result ptr

                    if let Ok(arg_get_func) = module.declare_function(
                        "adesh_arg_get",
                        cranelift_module::Linkage::Import,
                        &arg_get_sig,
                    ) {
                        let arg_get_ref = module.declare_func_in_func(arg_get_func, builder.func);
                        let call = builder.ins().call(arg_get_ref, &[key_val]);
                        let result = builder.inst_results(call)[0];
                        ctx.value_map.insert(*dst, result);
                        ctx.value_types.insert(*dst, AotValueType::String);
                    } else {
                        let v = builder.ins().iconst(types::I64, 0);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::String);
                    }
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::String);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "argHas" => {
            // argHas(flag) builtin - check if flag is present
            if !args.is_empty() {
                let flag_id = args[0];
                if let Some(flag_val) = ctx.value_map.get(&flag_id).copied() {
                    // Declare external function
                    let arg_has_sig = module.make_signature();
                    let mut arg_has_sig = arg_has_sig;
                    arg_has_sig.params.push(AbiParam::new(types::I64)); // flag
                    arg_has_sig.returns.push(AbiParam::new(types::I64)); // result (0 or 1)

                    if let Ok(arg_has_func) = module.declare_function(
                        "adesh_arg_has",
                        cranelift_module::Linkage::Import,
                        &arg_has_sig,
                    ) {
                        let arg_has_ref = module.declare_func_in_func(arg_has_func, builder.func);
                        let call = builder.ins().call(arg_has_ref, &[flag_val]);
                        let result_i64 = builder.inst_results(call)[0];
                        // Convert i64 to i8 for Bool type
                        let result_i8 = builder.ins().ireduce(types::I8, result_i64);
                        ctx.value_map.insert(*dst, result_i8);
                        ctx.value_types.insert(*dst, AotValueType::Bool);
                    } else {
                        let v = builder.ins().iconst(types::I8, 0);
                        ctx.value_map.insert(*dst, v);
                        ctx.value_types.insert(*dst, AotValueType::Bool);
                    }
                } else {
                    let v = builder.ins().iconst(types::I8, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Bool);
                }
            } else {
                let v = builder.ins().iconst(types::I8, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Bool);
            }
        }
        "env" => {
            // env(key) builtin - get environment variable
            // Call external runtime function to get env var
            if !args.is_empty() {
                let key_id = args[0];
                if let Some(key_val) = ctx.value_map.get(&key_id).copied() {
                    // Declare external get_env function if not already declared
                    let get_env_sig = module.make_signature();
                    let mut get_env_sig = get_env_sig;
                    get_env_sig.params.push(AbiParam::new(types::I64)); // key
                    get_env_sig.returns.push(AbiParam::new(types::I64)); // result ptr

                    let get_env_func = module
                        .declare_function(
                            "adesh_get_env",
                            cranelift_module::Linkage::Import,
                            &get_env_sig,
                        )
                        .map_err(|e| format!("Failed to declare adesh_get_env: {}", e))?;
                    let get_env_ref = module.declare_func_in_func(get_env_func, builder.func);

                    let call = builder.ins().call(get_env_ref, &[key_val]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::String);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "envFromFile" => {
            // envFromFile(path) builtin - load .env file and return as object
            if !args.is_empty() {
                let path_id = args[0];
                if let Some(path_val) = ctx.value_map.get(&path_id).copied() {
                    // Declare external function
                    let load_env_sig = module.make_signature();
                    let mut load_env_sig = load_env_sig;
                    load_env_sig.params.push(AbiParam::new(types::I64)); // path
                    load_env_sig.returns.push(AbiParam::new(types::I64)); // result ptr

                    let load_env_func = module
                        .declare_function(
                            "adesh_load_env_file",
                            cranelift_module::Linkage::Import,
                            &load_env_sig,
                        )
                        .map_err(|e| format!("Failed to declare adesh_load_env_file: {}", e))?;
                    let load_env_ref = module.declare_func_in_func(load_env_func, builder.func);

                    let call = builder.ins().call(load_env_ref, &[path_val]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::Ptr); // object
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Ptr);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Ptr);
            }
        }
        "envFileGet" => {
            // envFileGet(key, default, path) builtin - get key from .env file
            if args.len() >= 3 {
                let key_id = args[0];
                let _default_id = args[1];
                let path_id = args[2];

                if let (Some(key_val), Some(path_val)) = (
                    ctx.value_map.get(&key_id).copied(),
                    ctx.value_map.get(&path_id).copied(),
                ) {
                    // Declare external function
                    let get_env_file_sig = module.make_signature();
                    let mut get_env_file_sig = get_env_file_sig;
                    get_env_file_sig.params.push(AbiParam::new(types::I64)); // key
                    get_env_file_sig.params.push(AbiParam::new(types::I64)); // path
                    get_env_file_sig.returns.push(AbiParam::new(types::I64)); // result ptr

                    let get_env_file_func = module
                        .declare_function(
                            "adesh_get_env_file",
                            cranelift_module::Linkage::Import,
                            &get_env_file_sig,
                        )
                        .map_err(|e| format!("Failed to declare adesh_get_env_file: {}", e))?;
                    let get_env_file_ref =
                        module.declare_func_in_func(get_env_file_func, builder.func);

                    let call = builder.ins().call(get_env_file_ref, &[key_val, path_val]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::String);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::String);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "envRuntimeLoad" => {
            // envRuntimeLoad(envObject) builtin - load env object into runtime environment
            if !args.is_empty() {
                let env_obj_id = args[0];
                if let Some(env_obj_val) = ctx.value_map.get(&env_obj_id).copied() {
                    // Declare external function
                    let runtime_load_sig = module.make_signature();
                    let mut runtime_load_sig = runtime_load_sig;
                    runtime_load_sig.params.push(AbiParam::new(types::I64)); // env object ptr
                    runtime_load_sig.returns.push(AbiParam::new(types::I64)); // count

                    let runtime_load_func = module
                        .declare_function(
                            "adesh_env_runtime_load",
                            cranelift_module::Linkage::Import,
                            &runtime_load_sig,
                        )
                        .map_err(|e| format!("Failed to declare adesh_env_runtime_load: {}", e))?;
                    let runtime_load_ref =
                        module.declare_func_in_func(runtime_load_func, builder.func);

                    let call = builder.ins().call(runtime_load_ref, &[env_obj_val]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Int);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Int);
            }
        }
        "get_field" => {
            // get_field(obj, field) builtin.
            if args.len() >= 2 {
                let obj_id = args[0];
                let field_id = args[1];

                if let (Some(obj_val), Some(field_val)) = (
                    ctx.value_map.get(&obj_id).copied(),
                    ctx.value_map.get(&field_id).copied(),
                ) {
                    let mut make_string_sig = module.make_signature();
                    make_string_sig.params.push(AbiParam::new(types::I64));
                    make_string_sig.returns.push(AbiParam::new(types::I64));
                    let make_string_func = module
                        .declare_function("aot_make_string", Linkage::Import, &make_string_sig)
                        .map_err(|e| format!("Failed to declare aot_make_string: {}", e))?;
                    let make_string_ref =
                        module.declare_func_in_func(make_string_func, builder.func);

                    let mut get_field_sig = module.make_signature();
                    get_field_sig.params.push(AbiParam::new(types::I64));
                    get_field_sig.params.push(AbiParam::new(types::I64));
                    get_field_sig.returns.push(AbiParam::new(types::I64));
                    let get_field_func = module
                        .declare_function("aot_get_field", Linkage::Import, &get_field_sig)
                        .map_err(|e| format!("Failed to declare aot_get_field: {}", e))?;
                    let get_field_ref = module.declare_func_in_func(get_field_func, builder.func);

                    let field_call = builder.ins().call(make_string_ref, &[field_val]);
                    let field_handle = builder.inst_results(field_call)[0];
                    let call = builder.ins().call(get_field_ref, &[obj_val, field_handle]);
                    let result = builder.inst_results(call)[0];
                    ctx.value_map.insert(*dst, result);
                    ctx.value_types.insert(*dst, AotValueType::Handle);
                    ctx.runtime_handle_values.insert(*dst);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    ctx.value_map.insert(*dst, v);
                    ctx.value_types.insert(*dst, AotValueType::Handle);
                    ctx.runtime_handle_values.insert(*dst);
                }
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            }
        }
        "format" => {
            // format(value, spec) -> string pointer
            // Minimal implementation: convert value to string, ignore spec for now
            if !args.is_empty() {
                let val_id = args[0];
                let val = ctx
                    .value_map
                    .get(&val_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = ctx
                    .value_types
                    .get(&val_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let str_ptr = match val_type {
                    AotValueType::String => val,
                    AotValueType::Float | AotValueType::F64 => {
                        // Call adesh_double_to_string(double)
                        let mut sig = module.make_signature();
                        sig.params.push(AbiParam::new(types::F64));
                        sig.returns.push(AbiParam::new(types::I64));
                        let func = module
                            .declare_function("adesh_double_to_string", Linkage::Import, &sig)
                            .expect("Failed to declare adesh_double_to_string");
                        let func_ref = module.declare_func_in_func(func, builder.func);
                        let f64v = builder.ins().bitcast(types::F64, MemFlags::new(), val);
                        let call = builder.ins().call(func_ref, &[f64v]);
                        builder.inst_results(call)[0]
                    }
                    AotValueType::F32 => {
                        let mut sig = module.make_signature();
                        sig.params.push(AbiParam::new(types::F64));
                        sig.returns.push(AbiParam::new(types::I64));
                        let func = module
                            .declare_function("adesh_double_to_string", Linkage::Import, &sig)
                            .expect("Failed to declare adesh_double_to_string");
                        let func_ref = module.declare_func_in_func(func, builder.func);
                        let f64v = builder.ins().fpromote(types::F64, val);
                        let call = builder.ins().call(func_ref, &[f64v]);
                        builder.inst_results(call)[0]
                    }
                    AotValueType::Bool => {
                        // Select "true"/"false" strings
                        if let (Some(&true_id), Some(&false_id)) =
                            (ctx.string_data.get("true"), ctx.string_data.get("false"))
                        {
                            let true_gv = module.declare_data_in_func(true_id, builder.func);
                            let false_gv = module.declare_data_in_func(false_id, builder.func);
                            let true_ptr = builder.ins().global_value(types::I64, true_gv);
                            let false_ptr = builder.ins().global_value(types::I64, false_gv);
                            let val_i64 = builder.ins().uextend(types::I64, val);
                            let zero = builder.ins().iconst(types::I64, 0);
                            let is_true = builder.ins().icmp(IntCC::NotEqual, val_i64, zero);
                            builder.ins().select(is_true, true_ptr, false_ptr)
                        } else {
                            // Fallback: "0"/"1" via int to string
                            let mut sig = module.make_signature();
                            sig.params.push(AbiParam::new(types::I64));
                            sig.returns.push(AbiParam::new(types::I64));
                            let func = module
                                .declare_function("adesh_int_to_string", Linkage::Import, &sig)
                                .expect("Failed to declare adesh_int_to_string");
                            let func_ref = module.declare_func_in_func(func, builder.func);
                            let vi64 = builder.ins().uextend(types::I64, val);
                            let call = builder.ins().call(func_ref, &[vi64]);
                            builder.inst_results(call)[0]
                        }
                    }
                    AotValueType::Int
                    | AotValueType::I64
                    | AotValueType::I32
                    | AotValueType::I16
                    | AotValueType::I8
                    | AotValueType::U64
                    | AotValueType::U32
                    | AotValueType::U16
                    | AotValueType::U8
                    | AotValueType::I128
                    | AotValueType::U128 => {
                        let mut sig = module.make_signature();
                        sig.params.push(AbiParam::new(types::I64));
                        sig.returns.push(AbiParam::new(types::I64));
                        let func = module
                            .declare_function("adesh_int_to_string", Linkage::Import, &sig)
                            .expect("Failed to declare adesh_int_to_string");
                        let func_ref = module.declare_func_in_func(func, builder.func);
                        let vi64 = if builder.func.dfg.value_type(val).bits() < 64 {
                            builder.ins().uextend(types::I64, val)
                        } else {
                            val
                        };
                        let call = builder.ins().call(func_ref, &[vi64]);
                        builder.inst_results(call)[0]
                    }
                    AotValueType::Array(ref elem_type, arr_len) => {
                        // Call adesh_array_to_string(ptr, len, elem_size, is_float)
                        let mut sig = module.make_signature();
                        sig.params.push(AbiParam::new(types::I64));
                        sig.params.push(AbiParam::new(types::I64));
                        sig.params.push(AbiParam::new(types::I64));
                        sig.params.push(AbiParam::new(types::I8));
                        sig.returns.push(AbiParam::new(types::I64));
                        let func = module
                            .declare_function("adesh_array_to_string", Linkage::Import, &sig)
                            .expect("Failed to declare adesh_array_to_string");
                        let func_ref = module.declare_func_in_func(func, builder.func);
                        let len_v = builder.ins().iconst(types::I64, arr_len as i64);
                        let elem_sz = match **elem_type {
                            AotValueType::U8 | AotValueType::I8 | AotValueType::Bool => 1,
                            AotValueType::U16 | AotValueType::I16 => 2,
                            AotValueType::U32 | AotValueType::I32 | AotValueType::F32 => 4,
                            AotValueType::U64
                            | AotValueType::I64
                            | AotValueType::F64
                            | AotValueType::Int
                            | AotValueType::Float
                            | AotValueType::Ptr => 8,
                            AotValueType::U128 | AotValueType::I128 => 16,
                            _ => 8,
                        };
                        let es_v = builder.ins().iconst(types::I64, elem_sz as i64);
                        let is_float = matches!(
                            **elem_type,
                            AotValueType::F32 | AotValueType::F64 | AotValueType::Float
                        );
                        let isf_v = builder
                            .ins()
                            .iconst(types::I8, if is_float { 1 } else { 0 });
                        let call = builder.ins().call(func_ref, &[val, len_v, es_v, isf_v]);
                        builder.inst_results(call)[0]
                    }
                    _ => {
                        // Fallback: empty string
                        let zero = builder.ins().iconst(types::I64, 0);
                        zero
                    }
                };

                ctx.value_map.insert(*dst, str_ptr);
                ctx.value_types.insert(*dst, AotValueType::String);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "str_concat" | "concat" => {
            // str_concat(left, right) -> string (concatenate two values as strings)
            if args.len() >= 2 {
                let left_id = args[0];
                let right_id = args[1];
                let left_val = ctx
                    .value_map
                    .get(&left_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let right_val = ctx
                    .value_map
                    .get(&right_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

                // Ensure both arguments are converted to strings first
                let left_str = if let Some(AotValueType::String) = ctx.value_types.get(&left_id) {
                    left_val
                } else {
                    // Call value_to_string(val) for non-string types
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(types::I64));
                    sig.returns.push(AbiParam::new(types::I64));
                    let func = module
                        .declare_function("adesh_value_to_string", Linkage::Import, &sig)
                        .expect("Failed to declare adesh_value_to_string");
                    let func_ref = module.declare_func_in_func(func, builder.func);
                    let call = builder.ins().call(func_ref, &[left_val]);
                    builder.inst_results(call)[0]
                };

                let right_str = if let Some(AotValueType::String) = ctx.value_types.get(&right_id) {
                    right_val
                } else {
                    // Call value_to_string(val) for non-string types
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(types::I64));
                    sig.returns.push(AbiParam::new(types::I64));
                    let func = module
                        .declare_function("adesh_value_to_string", Linkage::Import, &sig)
                        .expect("Failed to declare adesh_value_to_string");
                    let func_ref = module.declare_func_in_func(func, builder.func);
                    let call = builder.ins().call(func_ref, &[right_val]);
                    builder.inst_results(call)[0]
                };

                // Call string concat: adesh_str_concat(left_ptr, right_ptr)
                let mut sig = module.make_signature();
                sig.params.push(AbiParam::new(types::I64));
                sig.params.push(AbiParam::new(types::I64));
                sig.returns.push(AbiParam::new(types::I64));
                let func = module
                    .declare_function("adesh_str_concat", Linkage::Import, &sig)
                    .expect("Failed to declare adesh_str_concat");
                let func_ref = module.declare_func_in_func(func, builder.func);
                let call = builder.ins().call(func_ref, &[left_str, right_str]);
                let result = builder.inst_results(call)[0];

                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::String);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::String);
            }
        }
        "range" => {
            // range(start, end, [step]) - create an array of integers from start to end
            // The range is computed at runtime
            if args.is_empty() {
                // range() with no args - create empty array
                let empty_array = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, empty_array);
                ctx.value_types
                    .insert(*dst, AotValueType::Array(Box::new(AotValueType::Int), 0));
            } else if args.len() == 1 {
                // range(end) - create range from 0 to end
                let end_val_raw = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .ok_or_else(|| format!("Value {} not found", args[0]))?;

                // Ensure end is i64
                let end_val_i64 = match builder.func.dfg.value_type(end_val_raw) {
                    types::I64 => end_val_raw,
                    _ => builder.ins().uextend(types::I64, end_val_raw),
                };

                // Call runtime builtin: adesh_rt_range(start, end, step)
                let mut sig = module.make_signature();
                sig.params.push(AbiParam::new(types::I64)); // start
                sig.params.push(AbiParam::new(types::I64)); // end
                sig.params.push(AbiParam::new(types::I64)); // step
                sig.returns.push(AbiParam::new(types::I64)); // array pointer

                let zero = builder.ins().iconst(types::I64, 0);
                let one = builder.ins().iconst(types::I64, 1);

                let func = module
                    .declare_function("adesh_rt_range", Linkage::Import, &sig)
                    .map_err(|e| format!("declare adesh_rt_range failed: {}", e))?;
                let func_ref = module.declare_func_in_func(func, builder.func);
                let call = builder.ins().call(func_ref, &[zero, end_val_i64, one]);
                let result = builder.inst_results(call)[0];

                ctx.value_map.insert(*dst, result);
                ctx.value_types
                    .insert(*dst, AotValueType::Array(Box::new(AotValueType::Int), 0));
            } else if args.len() >= 2 {
                // range(start, end, [step]) or range(start, end)
                let start_val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .ok_or_else(|| format!("Value {} not found", args[0]))?;
                let end_val = ctx
                    .value_map
                    .get(&args[1])
                    .copied()
                    .ok_or_else(|| format!("Value {} not found", args[1]))?;
                let step_val_raw = if args.len() > 2 {
                    ctx.value_map
                        .get(&args[2])
                        .copied()
                        .ok_or_else(|| format!("Value {} not found", args[2]))?
                } else {
                    builder.ins().iconst(types::I64, 1)
                };

                // Ensure all values are i64
                let start_val_i64 = match builder.func.dfg.value_type(start_val) {
                    types::I64 => start_val,
                    _ => builder.ins().uextend(types::I64, start_val),
                };
                let end_val_i64 = match builder.func.dfg.value_type(end_val) {
                    types::I64 => end_val,
                    _ => builder.ins().uextend(types::I64, end_val),
                };
                let step_val_i64 = match builder.func.dfg.value_type(step_val_raw) {
                    types::I64 => step_val_raw,
                    _ => builder.ins().uextend(types::I64, step_val_raw),
                };

                // Call runtime builtin: adesh_rt_range(start, end, step)
                let mut sig = module.make_signature();
                sig.params.push(AbiParam::new(types::I64)); // start
                sig.params.push(AbiParam::new(types::I64)); // end
                sig.params.push(AbiParam::new(types::I64)); // step
                sig.returns.push(AbiParam::new(types::I64)); // array pointer

                let func = module
                    .declare_function("adesh_rt_range", Linkage::Import, &sig)
                    .map_err(|e| format!("declare adesh_rt_range failed: {}", e))?;
                let func_ref = module.declare_func_in_func(func, builder.func);
                let call = builder
                    .ins()
                    .call(func_ref, &[start_val_i64, end_val_i64, step_val_i64]);
                let result = builder.inst_results(call)[0];

                ctx.value_map.insert(*dst, result);
                ctx.value_types
                    .insert(*dst, AotValueType::Array(Box::new(AotValueType::Int), 0));
            } else {
                let zero = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, zero);
                ctx.value_types
                    .insert(*dst, AotValueType::Array(Box::new(AotValueType::Int), 0));
            }
        }
        "fs.read" | "fs.exists" | "fs.isFile" | "fs.isDir" | "fs.mkdir" | "fs.delete"
        | "fs.path.basename" | "fs.path.dirname" | "fs.path.extname" => {
            let name_map = [
                ("fs.read", "aot_fs_read"),
                ("fs.exists", "aot_fs_exists"),
                ("fs.isFile", "aot_fs_is_file"),
                ("fs.isDir", "aot_fs_is_dir"),
                ("fs.mkdir", "aot_fs_mkdir"),
                ("fs.delete", "aot_fs_delete"),
                ("fs.path.basename", "aot_fs_path_basename"),
                ("fs.path.dirname", "aot_fs_path_dirname"),
                ("fs.path.extname", "aot_fs_path_extname"),
            ];
            let sym_name = name_map
                .iter()
                .find(|&&(n, _)| n == builtin_name)
                .map(|&(_, s)| s)
                .unwrap();

            if !args.is_empty() {
                let arg_val = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

                let mut sig = module.make_signature();
                sig.params.push(AbiParam::new(types::I64));
                sig.returns.push(AbiParam::new(types::I64));
                let func = module
                    .declare_function(sym_name, Linkage::Import, &sig)
                    .map_err(|e| format!("Failed to declare {}: {}", sym_name, e))?;
                let func_ref = module.declare_func_in_func(func, builder.func);
                let call = builder.ins().call(func_ref, &[arg_val]);
                let result = builder.inst_results(call)[0];
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            } else {
                let zero = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, zero);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            }
        }

        "fs.write" | "fs.copy" | "fs.move" => {
            let name_map = [
                ("fs.write", "aot_fs_write"),
                ("fs.copy", "aot_fs_copy"),
                ("fs.move", "aot_fs_move"),
            ];
            let sym_name = name_map
                .iter()
                .find(|&&(n, _)| n == builtin_name)
                .map(|&(_, s)| s)
                .unwrap();

            if args.len() >= 2 {
                let arg0 = ctx
                    .value_map
                    .get(&args[0])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let arg1 = ctx
                    .value_map
                    .get(&args[1])
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

                let mut sig = module.make_signature();
                sig.params.push(AbiParam::new(types::I64));
                sig.params.push(AbiParam::new(types::I64));
                sig.returns.push(AbiParam::new(types::I64));
                let func = module
                    .declare_function(sym_name, Linkage::Import, &sig)
                    .map_err(|e| format!("Failed to declare {}: {}", sym_name, e))?;
                let func_ref = module.declare_func_in_func(func, builder.func);
                let call = builder.ins().call(func_ref, &[arg0, arg1]);
                let result = builder.inst_results(call)[0];
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            } else {
                let zero = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, zero);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            }
        }

        "fs.path.join" => {
            // Gather arguments as a pointer array (similar to aot_make_array)
            let mut arg_vals = Vec::new();
            for arg_id in args {
                if let Some(&val) = ctx.value_map.get(arg_id) {
                    arg_vals.push(val);
                }
            }

            if !arg_vals.is_empty() {
                // Store values to stack slot and pass pointer
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (arg_vals.len() * 8) as u32,
                    8,
                ));
                let slot_addr = builder.ins().stack_addr(types::I64, slot, 0);

                for (i, &val) in arg_vals.iter().enumerate() {
                    builder
                        .ins()
                        .store(MemFlags::new(), val, slot_addr, (i * 8) as i32);
                }

                let count_val = builder.ins().iconst(types::I64, arg_vals.len() as i64);

                let mut sig = module.make_signature();
                sig.params.push(AbiParam::new(types::I64)); // args_ptr
                sig.params.push(AbiParam::new(types::I64)); // count
                sig.returns.push(AbiParam::new(types::I64));

                let func = module
                    .declare_function("aot_fs_path_join", Linkage::Import, &sig)
                    .map_err(|e| format!("Failed to declare aot_fs_path_join: {}", e))?;
                let func_ref = module.declare_func_in_func(func, builder.func);
                let call = builder.ins().call(func_ref, &[slot_addr, count_val]);
                let result = builder.inst_results(call)[0];
                ctx.value_map.insert(*dst, result);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            } else {
                let zero = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, zero);
                ctx.value_types.insert(*dst, AotValueType::Handle);
                ctx.runtime_handle_values.insert(*dst);
            }
        }
        _ => {
            // Unknown builtin - return 0
            let v = builder.ins().iconst(types::I64, 0);
            ctx.value_map.insert(*dst, v);
        }
    }
    Ok(())
}
