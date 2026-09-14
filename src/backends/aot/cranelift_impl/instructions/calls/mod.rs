//! Call instruction handlers
//!
//! Handles function calls including Call, CallBuiltin, CallBuiltinGeneric, and TailCall

use cranelift::prelude::*;
use cranelift_codegen::isa;
use cranelift_module::{Linkage, Module};

#[allow(unused_imports)]
use crate::backends::aot::cranelift::{AotValueType, FunctionCompileContext};
use crate::backends::common::lir::ValueId;

pub mod builtins;

fn adapt_arg_to_sig(
    builder: &mut FunctionBuilder,
    arg: Value,
    src_ty: Option<&AotValueType>,
    expected_ty: Type,
) -> Value {
    let actual_ty = builder.func.dfg.value_type(arg);
    if actual_ty == expected_ty {
        return arg;
    }

    match expected_ty {
        types::I64 => {
            if actual_ty.is_int() {
                match src_ty {
                    Some(
                        AotValueType::Bool
                        | AotValueType::U8
                        | AotValueType::U16
                        | AotValueType::U32,
                    ) => builder.ins().uextend(types::I64, arg),
                    Some(AotValueType::I8 | AotValueType::I16 | AotValueType::I32) => {
                        builder.ins().sextend(types::I64, arg)
                    }
                    _ => {
                        if actual_ty.bits() < 64 {
                            builder.ins().sextend(types::I64, arg)
                        } else if actual_ty.bits() > 64 {
                            builder.ins().ireduce(types::I64, arg)
                        } else {
                            arg
                        }
                    }
                }
            } else if actual_ty == types::F64 {
                builder.ins().fcvt_to_sint(types::I64, arg)
            } else if actual_ty == types::F32 {
                let p = builder.ins().fpromote(types::F64, arg);
                builder.ins().fcvt_to_sint(types::I64, p)
            } else {
                arg
            }
        }
        types::I32 | types::I16 | types::I8 => {
            if actual_ty.is_int() {
                if actual_ty.bits() > expected_ty.bits() {
                    builder.ins().ireduce(expected_ty, arg)
                } else if actual_ty.bits() < expected_ty.bits() {
                    builder.ins().sextend(expected_ty, arg)
                } else {
                    arg
                }
            } else {
                arg
            }
        }
        types::F64 => {
            if actual_ty == types::F32 {
                builder.ins().fpromote(types::F64, arg)
            } else if actual_ty.is_int() {
                builder.ins().fcvt_from_sint(types::F64, arg)
            } else {
                arg
            }
        }
        types::F32 => {
            if actual_ty == types::F64 {
                builder.ins().fdemote(types::F32, arg)
            } else if actual_ty.is_int() {
                let as_f64 = builder.ins().fcvt_from_sint(types::F64, arg);
                builder.ins().fdemote(types::F32, as_f64)
            } else {
                arg
            }
        }
        _ => arg,
    }
}

/// Handle Call instruction
pub(crate) fn handle_call(
    ctx: &mut FunctionCompileContext,
    builder: &mut FunctionBuilder,
    module: &mut dyn Module,
    dst: &ValueId,
    func_name: &str,
    args: &[ValueId],
) -> Result<(), String> {
    // Look up the function
    if let Some(&func_id) = ctx.user_funcs.get(func_name) {
        // Declare function reference
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        // Get expected parameter types from callee signature and adapt args.
        let sig_ref = builder.func.dfg.ext_funcs[func_ref].signature;
        let expected_params = builder.func.dfg.signatures[sig_ref].params.clone();

        let mut arg_vals: Vec<Value> = Vec::new();
        for (i, arg_id) in args.iter().enumerate() {
            if let Some(&raw_arg) = ctx.value_map.get(arg_id) {
                let src_ty = ctx.value_types.get(arg_id);
                if let Some(param) = expected_params.get(i) {
                    arg_vals.push(adapt_arg_to_sig(builder, raw_arg, src_ty, param.value_type));
                } else {
                    arg_vals.push(raw_arg);
                }
            }
        }

        // Call the function
        let call = builder.ins().call(func_ref, &arg_vals);
        let result = {
            let results = builder.inst_results(call);
            if !results.is_empty() {
                Some(results[0])
            } else {
                None
            }
        };
        if let Some(res) = result {
            ctx.value_map.insert(*dst, res);
        } else {
            let zero = builder.ins().iconst(types::I64, 0);
            ctx.value_map.insert(*dst, zero);
        }
    } else {
        // Unknown function - return 0
        let v = builder.ins().iconst(types::I64, 0);
        ctx.value_map.insert(*dst, v);
    }
    Ok(())
}

/// Handle CallBuiltinGeneric instruction
pub(crate) fn handle_call_builtin_generic(
    ctx: &mut FunctionCompileContext,
    builder: &mut FunctionBuilder,
    module: &mut dyn Module,
    dst: &ValueId,
    builtin_name: &str,
    args: &[ValueId],
    _generic_type: &str,
) -> Result<(), String> {
    let builtin_name = builtin_name.strip_prefix("std:").unwrap_or(builtin_name);
    let arg_vals: Vec<Value> = args
        .iter()
        .filter_map(|a| ctx.value_map.get(a).copied())
        .collect();

    let result = match builtin_name {
        "input" => {
            let mut sig = Signature::new(isa::CallConv::triple_default(module.isa().triple()));
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
            sig.returns.push(AbiParam::new(types::I64));

            let input_fn_id = module
                .declare_function("aot_input", Linkage::Import, &sig)
                .map_err(|e| format!("Failed to declare aot_input: {}", e))?;
            let input_fn = module.declare_func_in_func(input_fn_id, builder.func);

            let prompt_val = arg_vals.get(0).copied().unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let opts_val = arg_vals.get(1).copied().unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

            let call = builder.ins().call(input_fn, &[prompt_val, opts_val]);
            let res = builder.inst_results(call)[0];
            ctx.value_types.insert(*dst, AotValueType::Handle);
            res
        }
        "len" => {
            if !arg_vals.is_empty() {
                let zero = builder.ins().iconst(types::I64, 0);
                zero
            } else {
                builder.ins().iconst(types::I64, 0)
            }
        }
        _ => {
            builder.ins().iconst(types::I64, 0)
        }
    };

    ctx.value_map.insert(*dst, result);
    Ok(())
}

/// Handle TailCall instruction
pub(crate) fn handle_tail_call(
    ctx: &mut FunctionCompileContext,
    builder: &mut FunctionBuilder,
    module: &mut dyn Module,
    func_name: &str,
    args: &[ValueId],
) -> Result<(), String> {
    // Look up the function
    if let Some(&func_id) = ctx.user_funcs.get(func_name) {
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let sig_ref = builder.func.dfg.ext_funcs[func_ref].signature;
        let expected_params = builder.func.dfg.signatures[sig_ref].params.clone();

        let mut arg_vals: Vec<Value> = Vec::new();
        for (i, arg_id) in args.iter().enumerate() {
            if let Some(&raw_arg) = ctx.value_map.get(arg_id) {
                let src_ty = ctx.value_types.get(arg_id);
                if let Some(param) = expected_params.get(i) {
                    arg_vals.push(adapt_arg_to_sig(builder, raw_arg, src_ty, param.value_type));
                } else {
                    arg_vals.push(raw_arg);
                }
            }
        }

        let call = builder.ins().call(func_ref, &arg_vals);
        let result = {
            let results = builder.inst_results(call);
            if !results.is_empty() {
                Some(results[0])
            } else {
                None
            }
        };
        if let Some(res) = result {
            builder.ins().return_(&[res]);
        } else {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().return_(&[zero]);
        }
    } else {
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[zero]);
    }
    Ok(())
}
