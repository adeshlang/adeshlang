//! V1 Stack-based Virtual Machine.
//!
//! Implements the original language bytecode interpreter using a simple stack machine model.
//! Supports basic operations: push/pop, arithmetic, function calls, global variables,
//! defer statements, FFI calls, and unsafe memory operations.
//! Uses unified runtime ABI for all arithmetic and comparison operations.

use super::values::{VMValue, value_to_vm, vm_to_nanvalue, vm_to_value};
use crate::backends::ffi_import::{call_foreign, lookup_function};
use crate::execution::bytecode::OpCode;
use crate::parsing::ast::Value;
// Use NanValue ABI for optimized dynamic operations
use crate::runtime::abi::{
    abi_add_nan, abi_cmp_eq_nan, abi_cmp_ge_nan, abi_cmp_gt_nan, abi_cmp_le_nan, abi_cmp_lt_nan,
    abi_cmp_ne_nan, abi_div_nan, abi_mod_nan, abi_mul_nan, abi_sub_nan, nanvalue_to_value,
};
use std::io::Write;

/// Execute v1 bytecode from the provided data buffer.
///
/// # Arguments
/// * `data` - The raw bytecode bytes (including header)
/// * `idx` - Starting index (after magic and version)
/// * `out` - Output writer for print statements
///
/// # Returns
/// `Ok(())` on success, or an error message.
pub(super) fn execute_v1(data: &[u8], mut idx: usize, out: &mut dyn Write) -> Result<(), String> {
    let _version = data[idx];
    idx += 1;
    let const_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    let mut consts: Vec<VMValue> = Vec::new();
    for _ in 0..const_count {
        let tag = data[idx];
        idx += 1;
        match tag {
            0 => {
                let n = f64::from_le_bytes(data[idx..idx + 8].try_into().unwrap());
                idx += 8;
                consts.push(VMValue::Number(n));
            }
            1 => {
                let l = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                idx += 4;
                let s = String::from_utf8_lossy(&data[idx..idx + l]).to_string();
                idx += l;
                consts.push(VMValue::Str(s));
            }
            2 => {
                consts.push(VMValue::Null);
            }
            _ => return Err(format!("unknown const tag {}", tag)),
        }
    }
    let glob_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    let mut globals_names: Vec<String> = Vec::new();
    for _ in 0..glob_count {
        let l = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        let s = String::from_utf8_lossy(&data[idx..idx + l]).to_string();
        idx += l;
        globals_names.push(s);
    }
    let mut globals_vals: Vec<VMValue> = vec![VMValue::Null; globals_names.len()];
    let code_len = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    let code_end = idx + code_len;
    let mut stack: Vec<VMValue> = Vec::new();
    struct Frame {
        ret_pc: usize,
        base: usize,
        defers: Vec<(usize, usize)>, // (offset, length) pairs for defer blocks
    }
    let mut call_stack: Vec<Frame> = Vec::new();
    let mut current_defers: Vec<(usize, usize)> = Vec::new(); // Current scope's defers
    let mut pc = idx;
    while pc < code_end {
        let op = data[pc];
        pc += 1;
        match OpCode::from_u8(op) {
            Some(OpCode::LoadConst) => {
                let operand = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let c = consts.get(operand).cloned().unwrap_or(VMValue::Null);
                stack.push(c);
            }
            Some(OpCode::LoadGlobal) => {
                let operand = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let v = globals_vals.get(operand).cloned().unwrap_or(VMValue::Null);
                stack.push(v);
            }
            Some(OpCode::Print) => {
                if let Some(v) = stack.pop() {
                    match v {
                        VMValue::Number(n) => {
                            write!(out, "{}\n", n).map_err(|e| e.to_string())?;
                        }
                        VMValue::Bool(b) => {
                            write!(out, "{}\n", b).map_err(|e| e.to_string())?;
                        }
                        VMValue::Str(s) => {
                            out.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
                            out.write_all(b"\n").map_err(|e| e.to_string())?;
                        }
                        VMValue::Null => {
                            out.write_all(b"null\n").map_err(|e| e.to_string())?;
                        }
                        VMValue::BigInt(bi) => {
                            write!(out, "{}\n", bi).map_err(|e| e.to_string())?;
                        }
                        VMValue::U64(u) => {
                            write!(out, "0x{:x}\n", u).map_err(|e| e.to_string())?;
                        }
                        VMValue::Array(arr) => {
                            out.write_all(b"[").map_err(|e| e.to_string())?;
                            let mut first = true;
                            for elem in arr.iter() {
                                if !first { out.write_all(b", ").map_err(|e| e.to_string())?; }
                                first = false;
                                match elem {
                                    VMValue::Number(n) => write!(out, "{}", n).map_err(|e| e.to_string())?,
                                    VMValue::Bool(b) => write!(out, "{}", b).map_err(|e| e.to_string())?,
                                    VMValue::Str(s) => write!(out, "\"{}\"", s).map_err(|e| e.to_string())?,
                                    VMValue::Null => out.write_all(b"null").map_err(|e| e.to_string())?,
                                    _ => out.write_all(b"{...}").map_err(|e| e.to_string())?,
                                }
                            }
                            out.write_all(b"]\n").map_err(|e| e.to_string())?;
                        }
                        VMValue::Tuple(tup) => {
                            out.write_all(b"(").map_err(|e| e.to_string())?;
                            let mut first = true;
                            for elem in tup.iter() {
                                if !first { out.write_all(b", ").map_err(|e| e.to_string())?; }
                                first = false;
                                match elem {
                                    VMValue::Number(n) => write!(out, "{}", n).map_err(|e| e.to_string())?,
                                    VMValue::Bool(b) => write!(out, "{}", b).map_err(|e| e.to_string())?,
                                    VMValue::Str(s) => write!(out, "\"{}\"", s).map_err(|e| e.to_string())?,
                                    VMValue::Null => out.write_all(b"null").map_err(|e| e.to_string())?,
                                    _ => out.write_all(b"{...}").map_err(|e| e.to_string())?,
                                }
                            }
                            out.write_all(b")\n").map_err(|e| e.to_string())?;
                        }
                        VMValue::Object(o) => {
                            out.write_all(b"{").map_err(|e| e.to_string())?;
                            let mut first = true;
                            for (k, v) in o.iter() {
                                if !first {
                                    out.write_all(b", ").map_err(|e| e.to_string())?;
                                }
                                first = false;
                                write!(out, "\"{}\": ", k).map_err(|e| e.to_string())?;
                                match v {
                                    VMValue::Number(n) => {
                                        write!(out, "{}", n).map_err(|e| e.to_string())?
                                    }
                                    VMValue::Bool(b) => {
                                        write!(out, "{}", b).map_err(|e| e.to_string())?
                                    }
                                    VMValue::Str(s) => {
                                        write!(out, "\"{}\"", s).map_err(|e| e.to_string())?
                                    }
                                    VMValue::Null => {
                                        out.write_all(b"null").map_err(|e| e.to_string())?
                                    }
                                    VMValue::BigInt(bi) => {
                                        write!(out, "{}", bi).map_err(|e| e.to_string())?
                                    }
                                    VMValue::U64(u) => {
                                        write!(out, "0x{:x}", u).map_err(|e| e.to_string())?
                                    }
                                    VMValue::Object(_) => {
                                        out.write_all(b"{...}").map_err(|e| e.to_string())?
                                    }
                                    VMValue::Closure { .. } => {
                                        out.write_all(b"<closure>").map_err(|e| e.to_string())?
                                    }
                                    VMValue::Array(_) => {
                                        out.write_all(b"[...]").map_err(|e| e.to_string())?
                                    }
                                    VMValue::Tuple(_) => {
                                        out.write_all(b"(...)").map_err(|e| e.to_string())?
                                    }
                                }
                            }
                            out.write_all(b"}\n").map_err(|e| e.to_string())?;
                        }
                        VMValue::Closure { .. } => {
                            out.write_all(b"<closure>\n").map_err(|e| e.to_string())?;
                        }
                    }
                } else {
                    out.write_all(b"<nil>\n").map_err(|e| e.to_string())?;
                }
            }
            Some(OpCode::Add) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                // Use NanValue for optimized operations (8 bytes vs 40 bytes)
                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                match abi_add_nan(&nan_a, &nan_b) {
                    Ok(result) => {
                        let ast_result = nanvalue_to_value(&result).unwrap_or(Value::Null);
                        stack.push(value_to_vm(ast_result));
                    }
                    Err(_) => stack.push(VMValue::Null),
                }
            }
            Some(OpCode::Sub) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                match abi_sub_nan(&nan_a, &nan_b) {
                    Ok(result) => {
                        let ast_result = nanvalue_to_value(&result).unwrap_or(Value::Null);
                        stack.push(value_to_vm(ast_result));
                    }
                    Err(_) => stack.push(VMValue::Null),
                }
            }
            Some(OpCode::Mul) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                match abi_mul_nan(&nan_a, &nan_b) {
                    Ok(result) => {
                        let ast_result = nanvalue_to_value(&result).unwrap_or(Value::Null);
                        stack.push(value_to_vm(ast_result));
                    }
                    Err(_) => stack.push(VMValue::Null),
                }
            }
            Some(OpCode::Div) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                match abi_div_nan(&nan_a, &nan_b) {
                    Ok(result) => {
                        let ast_result = nanvalue_to_value(&result).unwrap_or(Value::Null);
                        stack.push(value_to_vm(ast_result));
                    }
                    Err(_) => stack.push(VMValue::Null),
                }
            }
            Some(OpCode::Mod) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                match abi_mod_nan(&nan_a, &nan_b) {
                    Ok(result) => {
                        let ast_result = nanvalue_to_value(&result).unwrap_or(Value::Null);
                        stack.push(value_to_vm(ast_result));
                    }
                    Err(_) => stack.push(VMValue::Null),
                }
            }
            Some(OpCode::CmpLT) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                let result = match abi_cmp_lt_nan(&nan_a, &nan_b) {
                    Ok(nan_result) => {
                        if let Some(b) = nanvalue_to_value(&nan_result).ok().and_then(|v| {
                            if let Value::Bool(b) = v {
                                Some(b)
                            } else {
                                None
                            }
                        }) {
                            VMValue::Number(if b { 1.0 } else { 0.0 })
                        } else {
                            VMValue::Number(0.0)
                        }
                    }
                    _ => VMValue::Number(0.0),
                };
                stack.push(result);
            }
            Some(OpCode::CmpLE) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                let result = match abi_cmp_le_nan(&nan_a, &nan_b) {
                    Ok(nan_result) => {
                        if let Some(b) = nanvalue_to_value(&nan_result).ok().and_then(|v| {
                            if let Value::Bool(b) = v {
                                Some(b)
                            } else {
                                None
                            }
                        }) {
                            VMValue::Number(if b { 1.0 } else { 0.0 })
                        } else {
                            VMValue::Number(0.0)
                        }
                    }
                    _ => VMValue::Number(0.0),
                };
                stack.push(result);
            }
            Some(OpCode::CmpGT) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                let result = match abi_cmp_gt_nan(&nan_a, &nan_b) {
                    Ok(nan_result) => {
                        if let Some(b) = nanvalue_to_value(&nan_result).ok().and_then(|v| {
                            if let Value::Bool(b) = v {
                                Some(b)
                            } else {
                                None
                            }
                        }) {
                            VMValue::Number(if b { 1.0 } else { 0.0 })
                        } else {
                            VMValue::Number(0.0)
                        }
                    }
                    _ => VMValue::Number(0.0),
                };
                stack.push(result);
            }
            Some(OpCode::CmpGE) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                let result = match abi_cmp_ge_nan(&nan_a, &nan_b) {
                    Ok(nan_result) => {
                        if let Some(b) = nanvalue_to_value(&nan_result).ok().and_then(|v| {
                            if let Value::Bool(b) = v {
                                Some(b)
                            } else {
                                None
                            }
                        }) {
                            VMValue::Number(if b { 1.0 } else { 0.0 })
                        } else {
                            VMValue::Number(0.0)
                        }
                    }
                    _ => VMValue::Number(0.0),
                };
                stack.push(result);
            }
            Some(OpCode::CmpEQ) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                let result = match abi_cmp_eq_nan(&nan_a, &nan_b) {
                    Ok(nan_result) => {
                        if let Some(b) = nanvalue_to_value(&nan_result).ok().and_then(|v| {
                            if let Value::Bool(b) = v {
                                Some(b)
                            } else {
                                None
                            }
                        }) {
                            VMValue::Number(if b { 1.0 } else { 0.0 })
                        } else {
                            VMValue::Number(0.0)
                        }
                    }
                    _ => VMValue::Number(0.0),
                };
                stack.push(result);
            }
            Some(OpCode::CmpNE) => {
                let b = stack.pop().unwrap_or(VMValue::Null);
                let a = stack.pop().unwrap_or(VMValue::Null);

                let nan_a = vm_to_nanvalue(&a);
                let nan_b = vm_to_nanvalue(&b);
                let result = match abi_cmp_ne_nan(&nan_a, &nan_b) {
                    Ok(nan_result) => {
                        if let Some(b) = nanvalue_to_value(&nan_result).ok().and_then(|v| {
                            if let Value::Bool(b) = v {
                                Some(b)
                            } else {
                                None
                            }
                        }) {
                            VMValue::Number(if b { 1.0 } else { 0.0 })
                        } else {
                            VMValue::Number(0.0)
                        }
                    }
                    _ => VMValue::Number(0.0),
                };
                stack.push(result);
            }
            Some(OpCode::Call) => {
                let operand = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let target = idx + operand;
                call_stack.push(Frame {
                    ret_pc: pc,
                    base: stack.len(),
                    defers: current_defers.clone(),
                });
                current_defers.clear(); // New scope starts with empty defers
                pc = target;
            }
            Some(OpCode::Return) => {
                // Execute defers in LIFO order before returning
                while let Some((defer_offset, defer_len)) = current_defers.pop() {
                    let saved_pc = pc;
                    pc = defer_offset;
                    let defer_end = defer_offset + defer_len;
                    // Execute defer block (simple execution, no nested calls)
                    while pc < defer_end && pc < code_end {
                        let defer_op = data[pc];
                        pc += 1;
                        // Handle defer block opcodes (simplified - just basic ops)
                        match OpCode::from_u8(defer_op) {
                            Some(OpCode::LoadConst) => {
                                let operand =
                                    u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap())
                                        as usize;
                                pc += 4;
                                let c = consts.get(operand).cloned().unwrap_or(VMValue::Null);
                                stack.push(c);
                            }
                            Some(OpCode::Print) => {
                                if let Some(v) = stack.pop() {
                                    match v {
                                        VMValue::Number(n) => {
                                            let _ = write!(out, "{}\n", n);
                                        }
                                        VMValue::Bool(b) => {
                                            let _ = write!(out, "{}\n", b);
                                        }
                                        VMValue::Str(s) => {
                                            let _ = out.write_all(s.as_bytes());
                                            let _ = out.write_all(b"\n");
                                        }
                                        VMValue::Null => {
                                            let _ = out.write_all(b"null\n");
                                        }
                                        VMValue::BigInt(bi) => {
                                            let _ = write!(out, "{}\n", bi);
                                        }
                                        VMValue::U64(u) => {
                                            let _ = write!(out, "0x{:x}\n", u);
                                        }
                                        VMValue::Array(_) => {
                                            let _ = out.write_all(b"[...]\n");
                                        }
                                        VMValue::Tuple(_) => {
                                            let _ = out.write_all(b"(...)\n");
                                        }
                                        VMValue::Object(_) => {
                                            let _ = out.write_all(b"{{...}}\n");
                                        }
                                        VMValue::Closure { .. } => {
                                            let _ = out.write_all(b"<closure>\n");
                                        }
                                    }
                                }
                            }
                            _ => {
                                // For now, skip unsupported opcodes in defer blocks
                                // Full implementation would handle all opcodes
                            }
                        }
                    }
                    pc = saved_pc; // Restore PC after defer execution
                }

                // Now do the actual return
                if let Some(f) = call_stack.pop() {
                    let ret = stack.pop();
                    stack.truncate(f.base);
                    if let Some(rv) = ret {
                        stack.push(rv);
                    }
                    current_defers = f.defers; // Restore parent scope's defers
                    pc = f.ret_pc;
                } else {
                    break;
                }
            }
            Some(OpCode::StoreGlobal) => {
                let operand = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let v = stack.pop().unwrap_or(VMValue::Null);
                if operand < globals_vals.len() {
                    globals_vals[operand] = v;
                }
            }
            Some(OpCode::CallDecorated) => {
                // Decorated function call with pipeline execution
                // Extract function and pipeline indices from bytecode
                let fn_idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let pipeline_idx =
                    u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;

                // TODO: Full implementation would:
                // 1. Look up the function by fn_idx
                // 2. Retrieve the decorator pipeline by pipeline_idx
                // 3. Execute decorator stages in order
                // 4. Call the original function within the pipeline context
                //
                // For now, this is a stub that recognizes the opcode
                // but doesn't execute decorator logic. The indices are
                // read to advance the program counter correctly.
                //
                // To complete: integrate with DecoratorRegistry and
                // implement pipeline execution in VM context.

                eprintln!(
                    "Warning: CallDecorated opcode encountered but full pipeline execution not yet implemented (fn={}, pipeline={})",
                    fn_idx, pipeline_idx
                );
            }
            Some(OpCode::Halt) => break,
            Some(OpCode::CallFfi) => {
                // Operands: u32 const index (symbol), u32 arg count
                let sym_idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let arg_count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;

                let sym = match consts.get(sym_idx) {
                    Some(VMValue::Str(s)) => s.clone(),
                    _ => return Err("FFI symbol must be a string constant".to_string()),
                };

                if stack.len() < arg_count {
                    return Err("Stack underflow for FFI call".to_string());
                }
                let mut args: Vec<crate::parsing::ast::Value> = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    let vmv = stack.pop().unwrap_or(VMValue::Null);
                    args.push(vm_to_value(vmv)?);
                }
                args.reverse();

                let func = lookup_function(&sym)
                    .ok_or_else(|| format!("Unknown foreign symbol '{}'", sym))?;
                let result = call_foreign(&func, &args)
                    .map_err(|e| format!("FFI call error for '{}': {}", sym, e))?;
                stack.push(value_to_vm(result));
            }
            Some(OpCode::Alloc) => {
                // Pop size from stack, allocate, push pointer
                let size_val = stack
                    .pop()
                    .ok_or_else(|| "Stack underflow for ALLOC".to_string())?;
                let size = match size_val {
                    VMValue::Number(n) => n as usize,
                    _ => return Err("ALLOC requires integer size".to_string()),
                };
                let ptr = crate::backends::unsafe_heap::alloc(size)
                    .map_err(|e| format!("Allocation failed: {}", e))?;
                stack.push(VMValue::U64(ptr));
            }
            Some(OpCode::Free) => {
                // Pop pointer from stack, free it
                let ptr_val = stack
                    .pop()
                    .ok_or_else(|| "Stack underflow for FREE".to_string())?;
                let ptr = match ptr_val {
                    VMValue::U64(p) => p,
                    _ => return Err("FREE requires pointer (U64)".to_string()),
                };
                crate::backends::unsafe_heap::free(ptr)
                    .map_err(|e| format!("Free failed: {}", e))?;
            }
            Some(OpCode::PtrLoad) => {
                // Pop index and pointer, load typed element, push as number (little-endian i64)
                let index_val = stack
                    .pop()
                    .ok_or_else(|| "Stack underflow for PTR_LOAD index".to_string())?;
                let ptr_val = stack
                    .pop()
                    .ok_or_else(|| "Stack underflow for PTR_LOAD pointer".to_string())?;

                let index = match index_val {
                    VMValue::Number(n) if n >= 0.0 => n as usize,
                    _ => return Err("PTR_LOAD requires non-negative integer index".to_string()),
                };
                let ptr = match ptr_val {
                    VMValue::U64(p) => p,
                    _ => return Err("PTR_LOAD requires pointer (U64)".to_string()),
                };

                let bytes = crate::backends::unsafe_heap::load_typed(ptr, index)
                    .map_err(|e| format!("Pointer load failed: {}", e))?;
                let mut buf = [0u8; 8];
                let copy_len = bytes.len().min(8);
                buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                let val = i64::from_le_bytes(buf);
                stack.push(VMValue::Number(val as f64));
            }
            Some(OpCode::PtrStore) => {
                // Pop value, index, and pointer; store typed element (little-endian packing)
                let value_val = stack
                    .pop()
                    .ok_or_else(|| "Stack underflow for PTR_STORE value".to_string())?;
                let index_val = stack
                    .pop()
                    .ok_or_else(|| "Stack underflow for PTR_STORE index".to_string())?;
                let ptr_val = stack
                    .pop()
                    .ok_or_else(|| "Stack underflow for PTR_STORE pointer".to_string())?;

                let index = match index_val {
                    VMValue::Number(n) if n >= 0.0 => n as usize,
                    _ => {
                        return Err("PTR_STORE requires non-negative integer index".to_string());
                    }
                };
                let ptr = match ptr_val {
                    VMValue::U64(p) => p,
                    _ => return Err("PTR_STORE requires pointer (U64)".to_string()),
                };

                // Determine element size from pointer metadata
                let elem_size = crate::backends::unsafe_heap::elem_size_of_ptr(ptr)
                    .map_err(|e| format!("Pointer store failed: {}", e))?;
                let mut buf = vec![0u8; elem_size];
                let val_i64 = match value_val {
                    VMValue::Number(n) => n as i64,
                    VMValue::U64(n) => n as i64,
                    _ => return Err("PTR_STORE requires numeric value".to_string()),
                };
                let bytes = val_i64.to_le_bytes();
                let copy_len = elem_size.min(bytes.len());
                buf[..copy_len].copy_from_slice(&bytes[..copy_len]);

                crate::backends::unsafe_heap::store_typed(ptr, index, &buf)
                    .map_err(|e| format!("Pointer store failed: {}", e))?;
            }
            Some(OpCode::DeferPush) => {
                // Read defer block offset and length
                let defer_offset =
                    u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let defer_len = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;

                // Push defer block info onto current scope's defer stack
                current_defers.push((defer_offset, defer_len));
                // Skip past the defer block code so it doesn't execute inline
                pc = defer_offset + defer_len;
            }
            Some(OpCode::DeferRun) => {
                // Execute all defers in LIFO order for current scope
                while let Some((defer_offset, defer_len)) = current_defers.pop() {
                    let saved_pc = pc;
                    pc = defer_offset;
                    let defer_end = defer_offset + defer_len;

                    // Execute defer block
                    while pc < defer_end && pc < code_end {
                        let defer_op = data[pc];
                        pc += 1;
                        match OpCode::from_u8(defer_op) {
                            Some(OpCode::LoadConst) => {
                                let operand =
                                    u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap())
                                        as usize;
                                pc += 4;
                                let c = consts.get(operand).cloned().unwrap_or(VMValue::Null);
                                stack.push(c);
                            }
                            Some(OpCode::Print) => {
                                if let Some(v) = stack.pop() {
                                    match v {
                                        VMValue::Number(n) => {
                                            let _ = write!(out, "{}\n", n);
                                        }
                                        VMValue::Bool(b) => {
                                            let _ = write!(out, "{}\n", b);
                                        }
                                        VMValue::Str(s) => {
                                            let _ = out.write_all(s.as_bytes());
                                            let _ = out.write_all(b"\n");
                                        }
                                        VMValue::Null => {
                                            let _ = out.write_all(b"null\n");
                                        }
                                        VMValue::BigInt(bi) => {
                                            let _ = write!(out, "{}\n", bi);
                                        }
                                        VMValue::U64(u) => {
                                            let _ = write!(out, "0x{:x}\n", u);
                                        }
                                        VMValue::Array(_) => {
                                            let _ = out.write_all(b"[...]\n");
                                        }
                                        VMValue::Tuple(_) => {
                                            let _ = out.write_all(b"(...)\n");
                                        }
                                        VMValue::Object(_) => {
                                            let _ = out.write_all(b"{{...}}\n");
                                        }
                                        VMValue::Closure { .. } => {
                                            let _ = out.write_all(b"<closure>\n");
                                        }
                                    }
                                }
                            }
                            _ => {
                                // Skip unsupported opcodes in defer blocks for now
                            }
                        }
                    }
                    pc = saved_pc; // Restore PC after defer execution
                }
            }
            None => {
                eprintln!(
                    "[DEBUG] Unknown opcode {} at pc={} (code_base+{})",
                    op,
                    pc - 1,
                    (pc - 1).saturating_sub(idx)
                );
                return Err(format!("unknown opcode {} at {}", op, pc - 1));
            }
        }
    }
    Ok(())
}
