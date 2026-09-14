//! Memory management instruction lowering for Cranelift AOT backend
//!
//! This module handles lowering of all memory-related LIR instructions to Cranelift IR,
//! including:
//! - Raw memory allocation/deallocation (malloc/free)
//! - Typed array allocation  
//! - Pointer load/store operations
//! - ARC (Atomic Reference Counting) operations for shared ownership
//! - Weak reference operations

use cranelift::prelude::*;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use crate::backends::common::lir::{LirInst, ValueId};

/// Lower a memory-related LIR instruction to Cranelift IR
///
/// Uses generics to work with any AotValueType definition from the calling module.
///
/// # Returns
/// * `Ok(true)` - Instruction was a memory operation and was handled
/// * `Ok(false)` - Not a memory operation (caller should handle it)
/// * `Err(msg)` - Failed to lower the instruction
pub(in crate::backends::aot) fn lower_memory_instruction<T: Clone>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    malloc_func: Option<FuncId>,
    free_func: Option<FuncId>,
    heap_guard_func: Option<FuncId>,
    inst: &LirInst,
    ptr_type: T,
    i64_type: T,
    handle_type: T,
) -> Result<bool, String> {
    match inst {
        LirInst::Alloc(dst, size_id) => {
            lower_alloc(
                module,
                builder,
                value_map,
                value_types,
                malloc_func,
                heap_guard_func,
                *dst,
                *size_id,
                ptr_type,
            )?;
            Ok(true)
        }
        LirInst::AllocTyped(dst, size_id, elem_size_const) => {
            lower_alloc_typed(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *size_id,
                *elem_size_const,
                ptr_type,
            )?;
            Ok(true)
        }
        LirInst::Free(ptr_id) => {
            lower_free(module, builder, value_map, free_func, *ptr_id)?;
            Ok(true)
        }
        LirInst::PtrLoad(dst, ptr_id, index_id) => {
            lower_ptr_load(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *ptr_id,
                *index_id,
                i64_type.clone(),
            )?;
            Ok(true)
        }
        LirInst::PtrStore(ptr_id, index_id, value_id) => {
            lower_ptr_store(module, builder, value_map, *ptr_id, *index_id, *value_id)?;
            Ok(true)
        }
        LirInst::ArcNew(dst, val_id) => {
            lower_arc_new(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *val_id,
                handle_type.clone(),
            )?;
            Ok(true)
        }
        LirInst::ArcClone(dst, arc_id) => {
            lower_arc_clone(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *arc_id,
                handle_type.clone(),
            )?;
            Ok(true)
        }
        LirInst::ArcDrop(arc_id) => {
            lower_arc_drop(module, builder, value_map, *arc_id)?;
            Ok(true)
        }
        LirInst::ArcGet(dst, arc_id) => {
            lower_arc_get(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *arc_id,
                handle_type.clone(),
            )?;
            Ok(true)
        }
        LirInst::ArcSet(arc_id, val_id) => {
            lower_arc_set(module, builder, value_map, *arc_id, *val_id)?;
            Ok(true)
        }
        LirInst::ArcStrongCount(dst, arc_id) => {
            lower_arc_strong_count(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *arc_id,
                i64_type.clone(),
            )?;
            Ok(true)
        }
        LirInst::ArcWeakCount(dst, arc_id) => {
            lower_arc_weak_count(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *arc_id,
                i64_type.clone(),
            )?;
            Ok(true)
        }
        LirInst::WeakNew(dst, arc_id) => {
            lower_weak_new(
                module,
                builder,
                value_map,
                value_types,
                *dst,
                *arc_id,
                handle_type,
            )?;
            Ok(true)
        }
        LirInst::WeakDrop(weak_id) => {
            lower_weak_drop(module, builder, value_map, *weak_id)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn lower_alloc<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    malloc_func: Option<FuncId>,
    heap_guard_func: Option<FuncId>,
    dst: ValueId,
    size_id: ValueId,
    ptr_type: T,
) -> Result<(), String> {
    if let Some(heap_guard) = heap_guard_func {
        let heap_guard_ref = module.declare_func_in_func(heap_guard, builder.func);
        let _ = builder.ins().call(heap_guard_ref, &[]);
    }
    let size_val = *value_map
        .get(&size_id)
        .ok_or_else(|| "Alloc size value missing".to_string())?;
    let malloc_id = malloc_func.ok_or_else(|| "malloc not declared".to_string())?;
    let malloc_ref = module.declare_func_in_func(malloc_id, builder.func);
    let call = builder.ins().call(malloc_ref, &[size_val]);
    let results = builder.inst_results(call);
    let ptr = results[0];
    value_map.insert(dst, ptr);
    value_types.insert(dst, ptr_type);
    Ok(())
}

fn lower_alloc_typed<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    size_id: ValueId,
    elem_size_const: i64,
    ptr_type: T,
) -> Result<(), String> {
    let size_val = *value_map
        .get(&size_id)
        .ok_or_else(|| "AllocTyped size value missing".to_string())?;
    let elem_size_val = builder.ins().iconst(types::I64, elem_size_const);
    let func = module
        .declare_function(
            "adesh_rt_alloc_typed",
            Linkage::Import,
            &module.make_signature(),
        )
        .map_err(|e| format!("declare adesh_rt_alloc_typed failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[size_val, elem_size_val]);
    let results = builder.inst_results(call);
    let ptr = results[0];
    value_map.insert(dst, ptr);
    value_types.insert(dst, ptr_type);
    Ok(())
}

fn lower_free(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    free_func: Option<FuncId>,
    ptr_id: ValueId,
) -> Result<(), String> {
    let ptr_val = *value_map
        .get(&ptr_id)
        .ok_or_else(|| "Free pointer value missing".to_string())?;
    let free_id = free_func.ok_or_else(|| "free not declared".to_string())?;
    let free_ref = module.declare_func_in_func(free_id, builder.func);
    let call = builder.ins().call(free_ref, &[ptr_val]);
    let _ = builder.inst_results(call);
    Ok(())
}

fn lower_ptr_load<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    ptr_id: ValueId,
    index_id: ValueId,
    i64_type: T,
) -> Result<(), String> {
    let ptr_val = *value_map
        .get(&ptr_id)
        .ok_or_else(|| "PtrLoad ptr value missing".to_string())?;
    let index_val = *value_map
        .get(&index_id)
        .ok_or_else(|| "PtrLoad index value missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_load_typed", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_load_typed failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[ptr_val, index_val]);
    let results = builder.inst_results(call);
    let value = results[0];
    value_map.insert(dst, value);
    value_types.insert(dst, i64_type);
    Ok(())
}

fn lower_ptr_store(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    ptr_id: ValueId,
    index_id: ValueId,
    value_id: ValueId,
) -> Result<(), String> {
    let ptr_val = *value_map
        .get(&ptr_id)
        .ok_or_else(|| "PtrStore ptr value missing".to_string())?;
    let index_val = *value_map
        .get(&index_id)
        .ok_or_else(|| "PtrStore index value missing".to_string())?;
    let val = *value_map
        .get(&value_id)
        .ok_or_else(|| "PtrStore value missing".to_string())?;
    let func = module
        .declare_function(
            "adesh_rt_store_typed",
            Linkage::Import,
            &module.make_signature(),
        )
        .map_err(|e| format!("declare adesh_rt_store_typed failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let _ = builder.ins().call(fref, &[ptr_val, index_val, val]);
    Ok(())
}

fn lower_arc_new<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    val_id: ValueId,
    ptr_type: T,
) -> Result<(), String> {
    let v = *value_map
        .get(&val_id)
        .ok_or_else(|| "ArcNew value missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_arc_new", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_arc_new failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[v]);
    let res = builder.inst_results(call)[0];
    value_map.insert(dst, res);
    value_types.insert(dst, ptr_type);
    Ok(())
}

fn lower_arc_clone<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    arc_id: ValueId,
    ptr_type: T,
) -> Result<(), String> {
    let a = *value_map
        .get(&arc_id)
        .ok_or_else(|| "ArcClone handle missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_arc_clone", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_arc_clone failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[a]);
    let res = builder.inst_results(call)[0];
    value_map.insert(dst, res);
    value_types.insert(dst, ptr_type);
    Ok(())
}

fn lower_arc_drop(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    arc_id: ValueId,
) -> Result<(), String> {
    let a = *value_map
        .get(&arc_id)
        .ok_or_else(|| "ArcDrop handle missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_arc_drop", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_arc_drop failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let _ = builder.ins().call(fref, &[a]);
    Ok(())
}

fn lower_arc_get<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    arc_id: ValueId,
    ptr_type: T,
) -> Result<(), String> {
    let a = *value_map
        .get(&arc_id)
        .ok_or_else(|| "ArcGet arc handle missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_arc_get", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_arc_get failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[a]);
    let res = builder.inst_results(call)[0];
    value_map.insert(dst, res);
    value_types.insert(dst, ptr_type);
    Ok(())
}

fn lower_arc_set(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    arc_id: ValueId,
    val_id: ValueId,
) -> Result<(), String> {
    let a = *value_map
        .get(&arc_id)
        .ok_or_else(|| "ArcSet arc handle missing".to_string())?;
    let v = *value_map
        .get(&val_id)
        .ok_or_else(|| "ArcSet value missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_arc_set", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_arc_set failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let _ = builder.ins().call(fref, &[a, v]);
    Ok(())
}

fn lower_arc_strong_count<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    arc_id: ValueId,
    i64_type: T,
) -> Result<(), String> {
    let a = *value_map
        .get(&arc_id)
        .ok_or_else(|| "ArcStrongCount arc handle missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_arc_strong_count", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_arc_strong_count failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[a]);
    let res = builder.inst_results(call)[0];
    value_map.insert(dst, res);
    value_types.insert(dst, i64_type);
    Ok(())
}

fn lower_arc_weak_count<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    arc_id: ValueId,
    i64_type: T,
) -> Result<(), String> {
    let a = *value_map
        .get(&arc_id)
        .ok_or_else(|| "ArcWeakCount arc handle missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_arc_weak_count", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_arc_weak_count failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[a]);
    let res = builder.inst_results(call)[0];
    value_map.insert(dst, res);
    value_types.insert(dst, i64_type);
    Ok(())
}

fn lower_weak_new<T>(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, T>,
    dst: ValueId,
    arc_id: ValueId,
    ptr_type: T,
) -> Result<(), String> {
    let a = *value_map
        .get(&arc_id)
        .ok_or_else(|| "WeakNew arc handle missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_weak_new", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_weak_new failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[a]);
    let res = builder.inst_results(call)[0];
    value_map.insert(dst, res);
    value_types.insert(dst, ptr_type);
    Ok(())
}

fn lower_weak_drop(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    weak_id: ValueId,
) -> Result<(), String> {
    let w = *value_map
        .get(&weak_id)
        .ok_or_else(|| "WeakDrop weak handle missing".to_string())?;
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    let func = module
        .declare_function("adesh_rt_weak_drop", Linkage::Import, &sig)
        .map_err(|e| format!("declare adesh_rt_weak_drop failed: {}", e))?;
    let fref = module.declare_func_in_func(func, builder.func);
    let _ = builder.ins().call(fref, &[w]);
    Ok(())
}
