//! V2 Register-based Virtual Machine.
//!
//! Implements the register-based bytecode interpreter for the language.
//! Features a fixed register file with support for:
//! - Arithmetic operations (Add, Sub, Mul, Div) via unified runtime ABI
//! - Control flow (Jump, JumpIf, Call, Return)
//! - Memory operations (Alloc, Free, PtrLoad, PtrStore)
//! - FFI calls and builtin functions
//! - Defer statements
//! - BigInt arithmetic
//!
//! ## Performance Optimizations
//! - Dual register file (f64 + VMValue) for fast numeric operations
//! - Pre-decoded instructions to avoid byte parsing in hot loop
//! - Unsafe unchecked array access in hot paths
//! - Parallel f64 arrays for constants and globals

use super::values::{
    VMValue, builtin_runtime_value_to_vm_value, value_to_vm, vm_to_nanvalue,
    vm_value_to_builtin_runtime_value,
};
use crate::execution::bytecode::ROp;
// Use NanValue ABI for optimized dynamic operations
use crate::runtime::abi::{
    abi_add_nan, abi_cmp_eq_nan, abi_cmp_ge_nan, abi_cmp_gt_nan, abi_cmp_le_nan, abi_cmp_lt_nan,
    abi_cmp_ne_nan, abi_div_nan, abi_mod_nan, abi_mul_nan, abi_sub_nan, nanvalue_to_value,
};
use std::io::Write;

/// Execute v2 bytecode from the provided data buffer.
///
/// # Arguments
/// * `data` - The raw bytecode bytes (including header)
/// * `idx` - Starting index (after magic and version)
/// * `out` - Output writer for print statements
///
/// # Returns
/// `Ok(())` on success, or an error message.
pub(super) fn execute_v2(data: &[u8], mut idx: usize, out: &mut dyn Write) -> Result<(), String> {
    // v2 register VM path
    let _version = data[idx];
    idx += 1;
    // consts
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
    // globals
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

    // reg count
    let reg_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    // code
    let code_len = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    let code_end = idx + code_len;
    // single frame register file
    let mut regs: Vec<VMValue> = vec![VMValue::Null; reg_count.max(1)];
    let mut call_stack: Vec<(Vec<VMValue>, usize, usize)> = Vec::new();
    let mut current_defers: Vec<(usize, usize)> = Vec::new();
    // Base offset of the code section; defer block offsets in bytecode are relative to this
    let code_base = idx;
    let mut pc = idx;
    while pc < code_end {
        let op = data[pc];
        pc += 1;
        match ROp::from_u8(op) {
            Some(ROp::Move) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                regs[dst] = regs.get(src).cloned().unwrap_or(VMValue::Null);
            }
            Some(ROp::LoadConst) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let k = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                regs[dst] = consts.get(k).cloned().unwrap_or(VMValue::Null);
            }
            Some(ROp::LoadGlobal) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let g = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                regs[dst] = globals_vals.get(g).cloned().unwrap_or(VMValue::Null);
            }
            Some(ROp::StoreGlobal) => {
                let g = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                if g < globals_vals.len() {
                    globals_vals[g] = regs.get(src).cloned().unwrap_or(VMValue::Null);
                }
            }
            Some(ROp::LoadConstBigInt) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let k = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let s = match consts.get(k).cloned().unwrap_or(VMValue::Null) {
                    VMValue::Str(x) => x,
                    VMValue::Number(n) => format!("{}", n),
                    VMValue::Null => "0".to_string(),
                    _ => "0".to_string(),
                };
                let bi = num_bigint::BigInt::parse_bytes(s.as_bytes(), 10)
                    .unwrap_or(num_bigint::BigInt::from(0));
                regs[dst] = VMValue::BigInt(bi);
            }
            Some(ROp::Clock) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let secs = now.as_secs_f64();
                regs[dst] = VMValue::Number(secs);
            }
            Some(ROp::Add) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(na + nb);
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    match abi_add_nan(&nan_a, &nan_b) {
                        Ok(result) => {
                            let ast_result = nanvalue_to_value(&result)
                                .unwrap_or(crate::parsing::ast::Value::Null);
                            regs[dst] = value_to_vm(ast_result);
                        }
                        Err(_) => regs[dst] = VMValue::Null,
                    }
                }
            }
            Some(ROp::Sub) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(na - nb);
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    match abi_sub_nan(&nan_a, &nan_b) {
                        Ok(result) => {
                            let ast_result = nanvalue_to_value(&result)
                                .unwrap_or(crate::parsing::ast::Value::Null);
                            regs[dst] = value_to_vm(ast_result);
                        }
                        Err(_) => regs[dst] = VMValue::Null,
                    }
                }
            }
            Some(ROp::Mul) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(na * nb);
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    match abi_mul_nan(&nan_a, &nan_b) {
                        Ok(result) => {
                            let ast_result = nanvalue_to_value(&result)
                                .unwrap_or(crate::parsing::ast::Value::Null);
                            regs[dst] = value_to_vm(ast_result);
                        }
                        Err(_) => regs[dst] = VMValue::Null,
                    }
                }
            }
            Some(ROp::Div) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if *nb != 0.0 { na / nb } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    match abi_div_nan(&nan_a, &nan_b) {
                        Ok(result) => {
                            let ast_result = nanvalue_to_value(&result)
                                .unwrap_or(crate::parsing::ast::Value::Null);
                            regs[dst] = value_to_vm(ast_result);
                        }
                        Err(_) => regs[dst] = VMValue::Null,
                    }
                }
            }
            Some(ROp::Mod) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if *nb != 0.0 { na % nb } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    match abi_mod_nan(&nan_a, &nan_b) {
                        Ok(result) => {
                            let ast_result = nanvalue_to_value(&result)
                                .unwrap_or(crate::parsing::ast::Value::Null);
                            regs[dst] = value_to_vm(ast_result);
                        }
                        Err(_) => regs[dst] = VMValue::Null,
                    }
                }
            }
            Some(ROp::Print) => {
                let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                match regs.get(r).cloned().unwrap_or(VMValue::Null) {
                    VMValue::Number(n) => {
                        write!(out, "{}", n).map_err(|e| e.to_string())?;
                    }
                    VMValue::Bool(b) => {
                        write!(out, "{}", b).map_err(|e| e.to_string())?;
                    }
                    VMValue::Str(s) => {
                        write!(out, "{}", s).map_err(|e| e.to_string())?;
                    }
                    VMValue::Null => {
                        write!(out, "null").map_err(|e| e.to_string())?;
                    }
                    VMValue::BigInt(bi) => {
                        write!(out, "{}", bi).map_err(|e| e.to_string())?;
                    }
                    VMValue::U64(u) => {
                        write!(out, "0x{:x}", u).map_err(|e| e.to_string())?;
                    }
                    VMValue::Array(arr) => {
                        let elements: Vec<String> = arr
                            .iter()
                            .map(|v| match v {
                                VMValue::Number(n) => format!("{}", n),
                                VMValue::Bool(b) => format!("{}", b),
                                VMValue::Str(s) => format!("\"{}\"", s),
                                VMValue::Null => "null".to_string(),
                                _ => "{...}".to_string(),
                            })
                            .collect();
                        write!(out, "[{}]", elements.join(", ")).map_err(|e| e.to_string())?;
                    }
                    VMValue::Tuple(tup) => {
                        let elements: Vec<String> = tup
                            .iter()
                            .map(|v| match v {
                                VMValue::Number(n) => format!("{}", n),
                                VMValue::Bool(b) => format!("{}", b),
                                VMValue::Str(s) => format!("\"{}\"", s),
                                VMValue::Null => "null".to_string(),
                                _ => "{...}".to_string(),
                            })
                            .collect();
                        write!(out, "({})", elements.join(", ")).map_err(|e| e.to_string())?;
                    }
                    VMValue::Object(o) => {
                        let mut pairs: Vec<String> = Vec::new();
                        for (k, v) in o.iter() {
                            let vs = match v {
                                VMValue::Number(n) => format!("{}", n),
                                VMValue::Bool(b) => format!("{}", b),
                                VMValue::Str(s) => format!("\"{}\"", s),
                                VMValue::Null => "null".to_string(),
                                VMValue::BigInt(bi) => format!("{}", bi),
                                VMValue::U64(u) => format!("0x{:x}", u),
                                VMValue::Array(_) => "[...]".to_string(),
                                VMValue::Tuple(_) => "(...)".to_string(),
                                VMValue::Object(_) => "{...}".to_string(),
                                VMValue::Closure { .. } => "<closure>".to_string(),
                            };
                            pairs.push(format!("\"{}\": {}", k, vs));
                        }
                        write!(out, "{{{}}}", pairs.join(", ")).map_err(|e| e.to_string())?;
                    }
                    VMValue::Closure { .. } => {
                        write!(out, "<closure>").map_err(|e| e.to_string())?;
                    }
                }
            }
            Some(ROp::Jump) => {
                let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                pc = pc.wrapping_add(rel);
            }
            Some(ROp::JumpIfTrue) => {
                let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let cond = match regs.get(r).cloned().unwrap_or(VMValue::Null) {
                    VMValue::Str(s) => !s.is_empty(),
                    VMValue::Number(n) => n != 0.0,
                    VMValue::BigInt(bi) => bi != num_bigint::BigInt::from(0),
                    VMValue::Bool(b) => b,
                    VMValue::U64(u) => u != 0, // Pointers are truthy if non-null
                    VMValue::Object(m) => !m.is_empty(),
                    VMValue::Array(a) => !a.is_empty(),
                    VMValue::Tuple(t) => !t.is_empty(),
                    VMValue::Closure { .. } => true,
                    VMValue::Null => false,
                };
                if cond {
                    pc = pc.wrapping_add(rel);
                }
            }
            Some(ROp::JumpIfFalse) => {
                let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let cond = match regs.get(r).cloned().unwrap_or(VMValue::Null) {
                    VMValue::Str(s) => !s.is_empty(),
                    VMValue::Number(n) => n != 0.0,
                    VMValue::BigInt(bi) => bi != num_bigint::BigInt::from(0),
                    VMValue::Bool(b) => b,
                    VMValue::U64(u) => u != 0, // Pointers are truthy if non-null
                    VMValue::Object(m) => !m.is_empty(),
                    VMValue::Array(a) => !a.is_empty(),
                    VMValue::Tuple(t) => !t.is_empty(),
                    VMValue::Closure { .. } => true,
                    VMValue::Null => false,
                };
                if !cond {
                    pc = pc.wrapping_add(rel);
                }
            }
            Some(ROp::JumpAbs) => {
                let abs = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc = idx.wrapping_add(abs);
            }
            Some(ROp::CmpLT) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if na < nb { 1.0 } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    let res = match abi_cmp_lt_nan(&nan_a, &nan_b) {
                        Ok(nan_result) => nanvalue_to_value(&nan_result)
                            .ok()
                            .and_then(|v| {
                                if let crate::parsing::ast::Value::Bool(b) = v {
                                    Some(b)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    regs[dst] = VMValue::Number(if res { 1.0 } else { 0.0 });
                }
            }
            Some(ROp::CmpLE) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if na <= nb { 1.0 } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    let res = match abi_cmp_le_nan(&nan_a, &nan_b) {
                        Ok(nan_result) => nanvalue_to_value(&nan_result)
                            .ok()
                            .and_then(|v| {
                                if let crate::parsing::ast::Value::Bool(b) = v {
                                    Some(b)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    regs[dst] = VMValue::Number(if res { 1.0 } else { 0.0 });
                }
            }
            Some(ROp::CmpGT) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if na > nb { 1.0 } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    let res = match abi_cmp_gt_nan(&nan_a, &nan_b) {
                        Ok(nan_result) => nanvalue_to_value(&nan_result)
                            .ok()
                            .and_then(|v| {
                                if let crate::parsing::ast::Value::Bool(b) = v {
                                    Some(b)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    regs[dst] = VMValue::Number(if res { 1.0 } else { 0.0 });
                }
            }
            Some(ROp::CmpGE) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if na >= nb { 1.0 } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    let res = match abi_cmp_ge_nan(&nan_a, &nan_b) {
                        Ok(nan_result) => nanvalue_to_value(&nan_result)
                            .ok()
                            .and_then(|v| {
                                if let crate::parsing::ast::Value::Bool(b) = v {
                                    Some(b)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    regs[dst] = VMValue::Number(if res { 1.0 } else { 0.0 });
                }
            }
            Some(ROp::CmpEQ) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if na == nb { 1.0 } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    let res = match abi_cmp_eq_nan(&nan_a, &nan_b) {
                        Ok(nan_result) => nanvalue_to_value(&nan_result)
                            .ok()
                            .and_then(|v| {
                                if let crate::parsing::ast::Value::Bool(b) = v {
                                    Some(b)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    regs[dst] = VMValue::Number(if res { 1.0 } else { 0.0 });
                }
            }
            Some(ROp::CmpNE) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let va = regs.get(a).cloned().unwrap_or(VMValue::Null);
                let vb = regs.get(b).cloned().unwrap_or(VMValue::Null);

                if let (VMValue::Number(na), VMValue::Number(nb)) = (&va, &vb) {
                    regs[dst] = VMValue::Number(if na != nb { 1.0 } else { 0.0 });
                } else {
                    let nan_a = vm_to_nanvalue(&va);
                    let nan_b = vm_to_nanvalue(&vb);
                    let res = match abi_cmp_ne_nan(&nan_a, &nan_b) {
                        Ok(nan_result) => nanvalue_to_value(&nan_result)
                            .ok()
                            .and_then(|v| {
                                if let crate::parsing::ast::Value::Bool(b) = v {
                                    Some(b)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    regs[dst] = VMValue::Number(if res { 1.0 } else { 0.0 });
                }
            }
            Some(ROp::Call) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let abs = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut args: Vec<usize> = Vec::new();
                for _ in 0..argc {
                    let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    args.push(r);
                }
                let mut new_regs: Vec<VMValue> = vec![VMValue::Null; reg_count.max(1)];
                for (i, ar) in args.iter().enumerate() {
                    new_regs[i] = regs.get(*ar).cloned().unwrap_or(VMValue::Null);
                }
                let caller_regs = std::mem::replace(&mut regs, new_regs);
                call_stack.push((caller_regs, pc, dst));
                pc = idx.wrapping_add(abs);
            }
            Some(ROp::Return) => {
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                if let Some((caller_regs, ret_pc, ret_dst)) = call_stack.pop() {
                    let retv = regs.get(src).cloned().unwrap_or(VMValue::Null);
                    regs = caller_regs;
                    regs[ret_dst] = retv;
                    pc = ret_pc;
                } else {
                    break;
                }
            }
            Some(ROp::CallBuiltin) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let name_idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut arg_regs: Vec<usize> = Vec::new();
                for _ in 0..argc {
                    let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    arg_regs.push(r);
                }

                // Get builtin name from constants
                let builtin_name = if let Some(VMValue::Str(name)) = consts.get(name_idx) {
                    name.clone()
                } else {
                    return Err("Invalid builtin name index".to_string());
                };

                // Convert register values to RuntimeValue for builtin call
                use crate::backends::builtins::RuntimeValue;
                let mut args: Vec<RuntimeValue> = Vec::new();
                for ar in arg_regs {
                    let val = regs.get(ar).cloned().unwrap_or(VMValue::Null);
                    args.push(vm_value_to_builtin_runtime_value(val));
                }

                // Call the builtin function
                let builtins = crate::backends::builtins::BuiltinRegistry::new();
                let result = if let Some(func) = builtins.get(&builtin_name) {
                    func(&args)
                } else {
                    return Err(format!("Unknown builtin function: {}", builtin_name));
                };

                // Convert result back to VMValue
                regs[dst] = builtin_runtime_value_to_vm_value(result);
            }
            Some(ROp::EnvRuntimeLoadFile) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;

                let mut count = 0usize;

                // Get the path from src register - could be a string path or an Object from envFromFile
                let path_opt = match regs.get(src).cloned().unwrap_or(VMValue::Null) {
                    VMValue::Str(p) => Some(p),
                    VMValue::Object(_) => {
                        // If it's an Object from envFromFile, find the corresponding __envFromFile:path global
                        let obj_val = regs.get(src).cloned().unwrap_or(VMValue::Null);
                        globals_names.iter().enumerate().find_map(|(i, name)| {
                            if name.starts_with("__envFromFile:") {
                                if let Some(VMValue::Object(_)) = globals_vals.get(i) {
                                    if globals_vals[i] == obj_val {
                                        return Some(
                                            name.strip_prefix("__envFromFile:")
                                                .unwrap()
                                                .to_string(),
                                        );
                                    }
                                }
                            }
                            None
                        })
                    }
                    _ => None,
                };

                if let Some(path) = path_opt {
                    // Parse the .env file and update __env:* globals
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for raw_line in content.lines() {
                            let line = raw_line.trim();
                            if line.is_empty() || line.starts_with('#') {
                                continue;
                            }
                            let line = if line.starts_with("export ") {
                                &line[7..]
                            } else {
                                line
                            };
                            if let Some(eq_pos) = line.find('=') {
                                let key = line[..eq_pos].trim().to_string();
                                let mut val_str = line[eq_pos + 1..].trim().to_string();

                                // Remove surrounding quotes if present
                                if val_str.starts_with('"')
                                    && val_str.ends_with('"')
                                    && val_str.len() >= 2
                                {
                                    val_str = val_str[1..val_str.len() - 1].to_string();
                                    val_str = val_str
                                        .replace("\\n", "\n")
                                        .replace("\\t", "\t")
                                        .replace("\\r", "\r");
                                } else if val_str.starts_with('\'')
                                    && val_str.ends_with('\'')
                                    && val_str.len() >= 2
                                {
                                    val_str = val_str[1..val_str.len() - 1].to_string();
                                } else {
                                    // Handle inline comments
                                    if let Some(hash_pos) = val_str.find('#') {
                                        let before = &val_str[..hash_pos];
                                        if before.ends_with(' ') {
                                            val_str = before.trim_end().to_string();
                                        }
                                    }
                                }

                                // Find or create __env:KEY global
                                let global_name = format!("__env:{}", key);
                                let global_idx = if let Some(idx) =
                                    globals_names.iter().position(|g| g == &global_name)
                                {
                                    idx
                                } else {
                                    // Add new global if not exists
                                    globals_names.push(global_name.clone());
                                    globals_vals.push(VMValue::Null);
                                    globals_names.len() - 1
                                };

                                // Update the global value
                                globals_vals[global_idx] = VMValue::Str(val_str);
                                count += 1;
                            }
                        }
                    }
                }

                regs[dst] = VMValue::Number(count as f64);
            }
            Some(ROp::NewObject) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut map = std::collections::HashMap::new();
                for _ in 0..count {
                    let k_idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let v_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let key = match consts.get(k_idx) {
                        Some(VMValue::Str(s)) => s.clone(),
                        _ => String::new(),
                    };
                    let val = regs.get(v_reg).cloned().unwrap_or(VMValue::Null);
                    map.insert(key, val);
                }
                regs[dst] = VMValue::Object(map);
            }
            Some(ROp::NewArray) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut arr = Vec::with_capacity(count);
                for _ in 0..count {
                    let elem_reg =
                        u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let val = regs.get(elem_reg).cloned().unwrap_or(VMValue::Null);
                    arr.push(val);
                }
                regs[dst] = VMValue::Array(arr);
            }
            Some(ROp::NewTuple) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut tup = Vec::with_capacity(count);
                for _ in 0..count {
                    let elem_reg =
                        u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let val = regs.get(elem_reg).cloned().unwrap_or(VMValue::Null);
                    tup.push(val);
                }
                regs[dst] = VMValue::Tuple(tup);
            }
            Some(ROp::GetField) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let obj_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_kidx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_name = match consts.get(field_kidx) {
                    Some(VMValue::Str(s)) => s.as_str(),
                    _ => "",
                };
                let val = match regs.get(obj_reg) {
                    Some(VMValue::Object(map)) => {
                        map.get(field_name).cloned().unwrap_or(VMValue::Null)
                    }
                    Some(VMValue::Array(arr)) => match field_name {
                        "length" | "len" => VMValue::Number(arr.len() as f64),
                        "capacity" => VMValue::Number(arr.len() as f64),
                        _ => VMValue::Null,
                    },
                    Some(VMValue::Tuple(tup)) => match field_name {
                        "length" | "len" => VMValue::Number(tup.len() as f64),
                        _ => VMValue::Null,
                    },
                    Some(VMValue::Str(s)) => match field_name {
                        "length" | "len" => VMValue::Number(s.len() as f64),
                        _ => VMValue::Null,
                    },
                    _ => VMValue::Null,
                };
                regs[dst] = val;
            }
            Some(ROp::SetField) => {
                let obj_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_kidx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_name = match consts.get(field_kidx) {
                    Some(VMValue::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                let val = regs.get(val_reg).cloned().unwrap_or(VMValue::Null);
                if let Some(VMValue::Object(map)) = regs.get_mut(obj_reg) {
                    map.insert(field_name, val);
                }
            }
            Some(ROp::MakeClosure) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let fn_offset = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut env = Vec::with_capacity(count);
                for _ in 0..count {
                    let creg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    env.push(regs.get(creg).cloned().unwrap_or(VMValue::Null));
                }
                regs[dst] = VMValue::Closure { fn_offset, env };
            }
            Some(ROp::CallMethod) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let obj_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let method_kidx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    let ar = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    args.push(regs.get(ar).cloned().unwrap_or(VMValue::Null));
                }
                let method_name = match consts.get(method_kidx) {
                    Some(VMValue::Str(s)) => s.as_str(),
                    _ => "",
                };
                let res = match (regs.get_mut(obj_reg), method_name) {
                    (Some(VMValue::Array(arr)), "push") => {
                        if let Some(arg) = args.into_iter().next() {
                            arr.push(arg);
                        }
                        VMValue::Number(arr.len() as f64)
                    }
                    (Some(VMValue::Array(arr)), "pop") => arr.pop().unwrap_or(VMValue::Null),
                    (Some(VMValue::Array(arr)), "len") | (Some(VMValue::Array(arr)), "length") => {
                        VMValue::Number(arr.len() as f64)
                    }
                    (Some(VMValue::Array(arr)), "metadata_size") => {
                        let rval = vm_value_to_builtin_runtime_value(VMValue::Array(arr.clone()));
                        let res =
                            crate::backends::common::builtins::arrays::runtime_metadata_size(&[
                                rval,
                            ]);
                        match res {
                            crate::backends::builtins::RuntimeValue::Int(n) => {
                                VMValue::Number(n as f64)
                            }
                            _ => VMValue::Number(0.0),
                        }
                    }
                    (Some(VMValue::Array(arr)), "last") => {
                        arr.last().cloned().unwrap_or(VMValue::Null)
                    }
                    (Some(VMValue::Object(map)), "hasKey") => {
                        if let Some(VMValue::Str(k)) = args.first() {
                            VMValue::Bool(map.contains_key(k))
                        } else {
                            VMValue::Bool(false)
                        }
                    }
                    (Some(VMValue::Object(map)), "len")
                    | (Some(VMValue::Object(map)), "length") => VMValue::Number(map.len() as f64),
                    (Some(VMValue::Str(s)), "len") | (Some(VMValue::Str(s)), "length") => {
                        VMValue::Number(s.len() as f64)
                    }
                    (Some(receiver), "mock") => {
                        let mut mock_rvals: Vec<crate::backends::builtins::RuntimeValue> =
                            Vec::new();
                        for a in args {
                            mock_rvals.push(vm_value_to_builtin_runtime_value(a));
                        }
                        let builtins = crate::backends::builtins::BuiltinRegistry::new();
                        if let Some(func) = builtins.get("input.mock") {
                            func(&mock_rvals);
                        }
                        receiver.clone()
                    }
                    _ => VMValue::Null,
                };
                regs[dst] = res;
            }
            Some(ROp::GetIndex) => {
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let coll_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let idx_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val = match (regs.get(coll_reg), regs.get(idx_reg)) {
                    (Some(VMValue::Array(arr)), Some(VMValue::Number(n))) => {
                        let i = *n as usize;
                        arr.get(i).cloned().unwrap_or(VMValue::Null)
                    }
                    (Some(VMValue::Tuple(tup)), Some(VMValue::Number(n))) => {
                        let i = *n as usize;
                        tup.get(i).cloned().unwrap_or(VMValue::Null)
                    }
                    (Some(VMValue::Object(map)), Some(VMValue::Str(s))) => {
                        map.get(s).cloned().unwrap_or(VMValue::Null)
                    }
                    _ => VMValue::Null,
                };
                regs[dst] = val;
            }
            Some(ROp::SetIndex) => {
                let coll_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let idx_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val = regs.get(val_reg).cloned().unwrap_or(VMValue::Null);
                let idx_val = regs.get(idx_reg).cloned();
                match (regs.get_mut(coll_reg), idx_val) {
                    (Some(VMValue::Array(arr)), Some(VMValue::Number(n))) => {
                        let i = n as usize;
                        if i < arr.len() {
                            arr[i] = val;
                        }
                    }
                    (Some(VMValue::Object(map)), Some(VMValue::Str(s))) => {
                        map.insert(s, val);
                    }
                    _ => {}
                }
            }
            Some(ROp::DeferPush) => {
                let block_offset =
                    u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let block_len = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                current_defers.push((block_offset, block_len));
            }
            Some(ROp::DeferRun) => {
                // Execute all defers in LIFO order. Offsets in bytecode are relative to the code section start,
                // so translate them using code_base. Keep pc unchanged (pointing to next instruction).
                while let Some((block_offset, block_len)) = current_defers.pop() {
                    let mut block_pc = code_base + block_offset;
                    let block_end = block_pc + block_len;
                    while block_pc < block_end {
                        let op = data[block_pc];
                        block_pc += 1;
                        match ROp::from_u8(op) {
                            Some(ROp::Print) => {
                                let r = u32::from_le_bytes(
                                    data[block_pc..block_pc + 4].try_into().unwrap(),
                                ) as usize;
                                block_pc += 4;
                                if let Some(v) = regs.get(r) {
                                    match v {
                                        VMValue::Number(n) => {
                                            print!("{}", n);
                                        }
                                        VMValue::Str(s) => {
                                            print!("{}", s);
                                        }
                                        VMValue::Null => {
                                            print!("null");
                                        }
                                        VMValue::BigInt(bi) => {
                                            print!("{}", bi);
                                        }
                                        VMValue::U64(u) => {
                                            print!("0x{:x}", u);
                                        }
                                        VMValue::Bool(b) => {
                                            print!("{}", b);
                                        }
                                        VMValue::Array(_) => {
                                            print!("[...]");
                                        }
                                        VMValue::Tuple(_) => {
                                            print!("(...)");
                                        }
                                        VMValue::Object(_) => {
                                            print!("{{...}}");
                                        }
                                        VMValue::Closure { .. } => {
                                            print!("<closure>");
                                        }
                                    }
                                }
                            }
                            Some(ROp::PrintNewline) => {
                                println!();
                            }
                            Some(ROp::PrintSpace) => {
                                print!(" ");
                            }
                            Some(ROp::LoadConst) => {
                                let dst = u32::from_le_bytes(
                                    data[block_pc..block_pc + 4].try_into().unwrap(),
                                ) as usize;
                                block_pc += 4;
                                let k = u32::from_le_bytes(
                                    data[block_pc..block_pc + 4].try_into().unwrap(),
                                ) as usize;
                                block_pc += 4;
                                if dst < regs.len() {
                                    regs[dst] = consts.get(k).cloned().unwrap_or(VMValue::Null);
                                }
                            }
                            _ => {
                                // Skip unsupported opcodes in defer blocks for now
                            }
                        }
                    }
                }
            }
            Some(ROp::PrintNewline) => {
                let _ = writeln!(out);
            }
            Some(ROp::PrintSpace) => {
                let _ = write!(out, " ");
            }
            Some(ROp::Halt) => break,
            None => {
                let _ = std::fs::write(
                    "/tmp/adesh_debug.log",
                    format!(
                        "Unknown opcode {} at pc={} (code_base={})\n",
                        op,
                        pc - 1,
                        code_base
                    ),
                );
                return Err(format!("unknown v2 opcode {} at {}", op, pc - 1));
            }
        }
    }
    Ok(())
}

/// Execute v2 bytecode with program arguments from the provided data buffer.
///
/// # Arguments
/// * `data` - The raw bytecode bytes (including header)
/// * `idx` - Starting index (after magic and version)
/// * `out` - Output writer for print statements
/// * `args` - Program arguments
/// * `path_str` - Executable path string
///
/// # Returns
/// `Ok(())` on success, or an error message.
pub(super) fn execute_v2_with_args(
    data: &[u8],
    mut idx: usize,
    out: &mut dyn Write,
    args: &[String],
    path_str: String,
) -> Result<(), String> {
    let _version = data[idx];
    idx += 1;
    let const_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    let mut consts: Vec<VMValue> = Vec::new();
    // OPTIMIZED: Parallel f64 constants array for fast numeric access
    let mut consts_f64: Vec<f64> = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        let tag = data[idx];
        idx += 1;
        match tag {
            0 => {
                let n = f64::from_le_bytes(data[idx..idx + 8].try_into().unwrap());
                idx += 8;
                consts_f64.push(n);
                consts.push(VMValue::Number(n));
            }
            1 => {
                let l = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                idx += 4;
                let s = String::from_utf8_lossy(&data[idx..idx + l]).to_string();
                idx += l;
                consts_f64.push(0.0); // Non-numeric constant
                consts.push(VMValue::Str(s));
            }
            2 => {
                consts_f64.push(0.0); // Null
                consts.push(VMValue::Null);
            }
            _ => return Err(format!("unknown const tag {}", tag)),
        }
    }
    // globals
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
    // OPTIMIZED: Parallel f64 globals array for fast numeric access
    let mut globals_f64: Vec<f64> = vec![0.0; globals_names.len()];

    // Set program arguments for builtin functions
    crate::execution::runtime::set_program_args(args.to_vec(), Some(path_str.clone()));

    // Pre-populate argument globals
    for (i, name) in globals_names.iter().enumerate() {
        match name.as_str() {
            "__argc" => {
                globals_f64[i] = args.len() as f64;
                globals_vals[i] = VMValue::Number(args.len() as f64);
            }
            "__argv" => {
                globals_vals[i] =
                    VMValue::Array(args.iter().map(|a| VMValue::Str(a.clone())).collect());
            }
            "__execName" => {
                // Use the bytecode file path as executable name
                globals_vals[i] = VMValue::Str(path_str.clone());
            }
            "__argsJoined" => {
                // Store joined arguments with comma separator
                let joined = args.join(", ");
                globals_vals[i] = VMValue::Str(joined);
            }
            "__argsSlice1" => {
                // Store args slice from index 1 as array
                let slice = if args.len() > 1 {
                    args[1..].iter().map(|a| VMValue::Str(a.clone())).collect()
                } else {
                    Vec::new()
                };
                globals_vals[i] = VMValue::Array(slice);
            }
            _ => {}
        }
    }
    // Build a map name->index for globals to allow quick lookups
    let mut global_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (i, n) in globals_names.iter().enumerate() {
        global_index.insert(n.clone(), i);
    }

    // Additional glob types: env keys and envFromFile/object and envFileGet
    for (i, name) in globals_names.iter().enumerate() {
        if let Some(rest) = name.strip_prefix("__env:") {
            if let Ok(v) = std::env::var(rest) {
                globals_vals[i] = VMValue::Str(v);
            } else {
                globals_vals[i] = VMValue::Null;
            }
            continue;
        }
        if let Some(path) = name.strip_prefix("__envFromFile:") {
            // parse dotenv file into a hashmap
            let mut map: std::collections::HashMap<String, VMValue> =
                std::collections::HashMap::new();
            if let Ok(s) = std::fs::read_to_string(path) {
                for raw_line in s.lines() {
                    let line = raw_line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let line = if line.starts_with("export ") {
                        &line[7..]
                    } else {
                        line
                    };
                    if let Some(eq) = line.find('=') {
                        let key = line[..eq].trim();
                        let mut val = line[eq + 1..].trim().to_string();
                        if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                            val = val[1..val.len() - 1].to_string();
                            val = val
                                .replace("\\n", "\n")
                                .replace("\\t", "\t")
                                .replace("\\r", "\r");
                        } else if val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2 {
                            val = val[1..val.len() - 1].to_string();
                        } else {
                            if let Some(hash) = val.find('#') {
                                let before = &val[..hash];
                                if before.ends_with(' ') {
                                    val = before.trim_end().to_string();
                                }
                            }
                        }
                        map.insert(key.to_string(), VMValue::Str(val));
                    }
                }
            }
            globals_vals[i] = VMValue::Object(map);
            continue;
        }
        if let Some(rest) = name.strip_prefix("__envFile:") {
            // format: __envFile:<path>:<key> - find the key's value or null
            if let Some(col) = rest.rfind(':') {
                let path = &rest[..col];
                let key = &rest[col + 1..];
                if let Ok(s) = std::fs::read_to_string(path) {
                    let mut found: Option<String> = None;
                    for raw_line in s.lines() {
                        let line = raw_line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        let line = if line.starts_with("export ") {
                            &line[7..]
                        } else {
                            line
                        };
                        if let Some(eq) = line.find('=') {
                            let k = line[..eq].trim();
                            let mut val = line[eq + 1..].trim().to_string();
                            if k == key {
                                if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                                    val = val[1..val.len() - 1].to_string();
                                }
                                found = Some(val);
                                break;
                            }
                        }
                    }
                    if let Some(v) = found {
                        globals_vals[i] = VMValue::Str(v);
                    } else {
                        globals_vals[i] = VMValue::Null;
                    }
                } else {
                    globals_vals[i] = VMValue::Null;
                }
            } else {
                globals_vals[i] = VMValue::Null;
            }
            continue;
        }
    }
    // reg count
    let reg_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    // code
    let code_len = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    let code_end = idx + code_len;
    // OPTIMIZED: Dual register file - f64 for fast numeric ops, VMValue for complex types
    let mut regs_f64: Vec<f64> = vec![0.0; reg_count.max(1)];
    let mut regs: Vec<VMValue> = vec![VMValue::Null; reg_count.max(1)];
    let mut call_stack: Vec<(Vec<f64>, Vec<VMValue>, usize, usize)> = Vec::new();
    let mut current_defers: Vec<(usize, usize)> = Vec::new();
    // Base offset of the code section; defer block offsets in bytecode are relative to this
    let code_base = idx;
    let mut pc = idx;
    while pc < code_end {
        let op = data[pc];
        pc += 1;
        // Direct opcode matching (faster than ROp::from_u8 function call)
        match op {
            1 => {
                // Move - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                // SAFETY: bytecode compiler ensures valid register indices
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) = *regs_f64.get_unchecked(src);
                }
                regs[dst] = regs.get(src).cloned().unwrap_or(VMValue::Null);
            }
            2 => {
                // LoadConst - BLAZING FAST using consts_f64
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let k = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                // SAFETY: bytecode compiler ensures valid indices
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) = *consts_f64.get_unchecked(k);
                }
                if let Some(c) = consts.get(k) {
                    regs[dst] = c.clone();
                }
            }
            3 => {
                // LoadGlobal - BLAZING FAST using globals_f64
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let g = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                // SAFETY: bytecode compiler ensures valid indices
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) = *globals_f64.get_unchecked(g);
                }
                if let Some(v) = globals_vals.get(g) {
                    regs[dst] = v.clone();
                }
            }
            4 => {
                // StoreGlobal - BLAZING FAST syncing to globals_f64
                let g = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                // SAFETY: bytecode compiler ensures valid indices
                if g < globals_vals.len() {
                    unsafe {
                        *globals_f64.get_unchecked_mut(g) = *regs_f64.get_unchecked(src);
                    }
                    globals_vals[g] = regs.get(src).cloned().unwrap_or(VMValue::Null);
                }
            }
            10 => {
                // Add - BLAZING FAST using unchecked access
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                // SAFETY: bytecode compiler ensures valid register indices
                let res = unsafe {
                    let r = *regs_f64.get_unchecked(a) + *regs_f64.get_unchecked(b);
                    *regs_f64.get_unchecked_mut(dst) = r;
                    r
                };
                regs[dst] = VMValue::Number(res);
            }
            11 => {
                // Sub - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let res = unsafe {
                    let r = *regs_f64.get_unchecked(a) - *regs_f64.get_unchecked(b);
                    *regs_f64.get_unchecked_mut(dst) = r;
                    r
                };
                regs[dst] = VMValue::Number(res);
            }
            12 => {
                // Mul - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let res = unsafe {
                    let r = *regs_f64.get_unchecked(a) * *regs_f64.get_unchecked(b);
                    *regs_f64.get_unchecked_mut(dst) = r;
                    r
                };
                regs[dst] = VMValue::Number(res);
            }
            13 => {
                // Div - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let res = unsafe {
                    let divisor = *regs_f64.get_unchecked(b);
                    let r = if divisor != 0.0 {
                        *regs_f64.get_unchecked(a) / divisor
                    } else {
                        0.0
                    };
                    *regs_f64.get_unchecked_mut(dst) = r;
                    r
                };
                regs[dst] = VMValue::Number(res);
            }
            14 => {
                // Mod - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let res = unsafe {
                    let divisor = *regs_f64.get_unchecked(b);
                    let r = if divisor != 0.0 {
                        *regs_f64.get_unchecked(a) % divisor
                    } else {
                        0.0
                    };
                    *regs_f64.get_unchecked_mut(dst) = r;
                    r
                };
                regs[dst] = VMValue::Number(res);
            }
            20 => {
                // Print - prefer f64 for numeric output
                let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                // First try f64 register (fast path for numbers)
                if let Some(v) = regs.get(r) {
                    match v {
                        VMValue::Number(_) => {
                            // Use f64 register directly (faster)
                            let _ = write!(out, "{}", regs_f64[r]);
                        }
                        VMValue::Bool(b) => {
                            let _ = write!(out, "{}", b);
                        }
                        VMValue::Str(s) => {
                            let _ = write!(out, "{}", s);
                        }
                        VMValue::Null => {
                            let _ = write!(out, "null");
                        }
                        VMValue::BigInt(bi) => {
                            let _ = write!(out, "{}", bi);
                        }
                        VMValue::U64(u) => {
                            let _ = write!(out, "0x{:x}", u);
                        }
                        VMValue::Array(arr) => {
                            let elements: Vec<String> = arr
                                .iter()
                                .map(|v| match v {
                                    VMValue::Number(n) => format!("{}", n),
                                    VMValue::Bool(b) => format!("{}", b),
                                    VMValue::Str(s) => format!("\"{}\"", s),
                                    VMValue::Null => "null".to_string(),
                                    _ => "{...}".to_string(),
                                })
                                .collect();
                            let _ = write!(out, "[{}]", elements.join(", "));
                        }
                        VMValue::Tuple(tup) => {
                            let elements: Vec<String> = tup
                                .iter()
                                .map(|v| match v {
                                    VMValue::Number(n) => format!("{}", n),
                                    VMValue::Bool(b) => format!("{}", b),
                                    VMValue::Str(s) => format!("\"{}\"", s),
                                    VMValue::Null => "null".to_string(),
                                    _ => "{...}".to_string(),
                                })
                                .collect();
                            let _ = write!(out, "({})", elements.join(", "));
                        }
                        VMValue::Object(o) => {
                            let mut pairs: Vec<String> = Vec::new();
                            for (k, v) in o.iter() {
                                let vs = match v {
                                    VMValue::Number(n) => format!("{}", n),
                                    VMValue::Str(s) => format!("\"{}\"", s),
                                    VMValue::Null => "null".to_string(),
                                    VMValue::BigInt(bi) => format!("{}", bi),
                                    VMValue::U64(u) => format!("0x{:x}", u),
                                    VMValue::Bool(b) => format!("{}", b),
                                    VMValue::Array(_) => "[...]".to_string(),
                                    VMValue::Tuple(_) => "(...)".to_string(),
                                    VMValue::Object(_) => "{...}".to_string(),
                                    VMValue::Closure { .. } => "<closure>".to_string(),
                                };
                                pairs.push(format!("\"{}\": {}", k, vs));
                            }
                            let _ = write!(out, "{{{}}}", pairs.join(", "));
                        }
                        VMValue::Closure { .. } => {
                            let _ = write!(out, "<closure>");
                        }
                    }
                } else {
                    let _ = write!(out, "<nil>");
                }
            }
            27 => {
                // PrintNewline
                let _ = writeln!(out);
            }
            28 => {
                // PrintSpace
                let _ = write!(out, " ");
            }
            40 => {
                // CmpLT - ULTRA FAST PATH
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) =
                        if *regs_f64.get_unchecked(a) < *regs_f64.get_unchecked(b) {
                            1.0
                        } else {
                            0.0
                        };
                }
            }
            41 => {
                // CmpLE - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) =
                        if *regs_f64.get_unchecked(a) <= *regs_f64.get_unchecked(b) {
                            1.0
                        } else {
                            0.0
                        };
                }
            }
            42 => {
                // CmpGT - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) =
                        if *regs_f64.get_unchecked(a) > *regs_f64.get_unchecked(b) {
                            1.0
                        } else {
                            0.0
                        };
                }
            }
            43 => {
                // CmpGE - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) =
                        if *regs_f64.get_unchecked(a) >= *regs_f64.get_unchecked(b) {
                            1.0
                        } else {
                            0.0
                        };
                }
            }
            44 => {
                // CmpEQ - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) =
                        if *regs_f64.get_unchecked(a) == *regs_f64.get_unchecked(b) {
                            1.0
                        } else {
                            0.0
                        };
                }
            }
            45 => {
                // CmpNE - BLAZING FAST
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                unsafe {
                    *regs_f64.get_unchecked_mut(dst) =
                        if *regs_f64.get_unchecked(a) != *regs_f64.get_unchecked(b) {
                            1.0
                        } else {
                            0.0
                        };
                }
            }
            21 => {
                // Jump
                let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as i32;
                pc += 4;
                pc = (pc as i32 + rel) as usize;
            }
            22 => {
                // JumpIfTrue - BLAZING FAST
                let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as i32;
                pc += 4;
                unsafe {
                    if *regs_f64.get_unchecked(r) != 0.0 {
                        pc = (pc as i32 + rel) as usize;
                    }
                }
            }
            23 => {
                // JumpIfFalse - BLAZING FAST
                let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as i32;
                pc += 4;
                unsafe {
                    if *regs_f64.get_unchecked(r) == 0.0 {
                        pc = (pc as i32 + rel) as usize;
                    }
                }
            }
            24 => {
                // JumpAbs
                let abs = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc = idx + abs;
            }
            25 => {
                // LoadConstBigInt
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let k = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                regs[dst] = consts.get(k).cloned().unwrap_or(VMValue::Null);
            }
            26 => {
                // Clock - sync to both registers
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();
                regs_f64[dst] = now;
                regs[dst] = VMValue::Number(now);
            }
            60 => {
                // Call
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let abs = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut arg_indices: Vec<usize> = Vec::new();
                for _ in 0..argc {
                    let ar = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    arg_indices.push(ar);
                }
                // Copy argument values into new register frames
                let mut new_regs_f64: Vec<f64> = vec![0.0; reg_count.max(1)];
                let mut new_regs: Vec<VMValue> = vec![VMValue::Null; reg_count.max(1)];
                for (i, ar) in arg_indices.iter().enumerate() {
                    new_regs_f64[i] = regs_f64.get(*ar).copied().unwrap_or(0.0);
                    new_regs[i] = regs.get(*ar).cloned().unwrap_or(VMValue::Null);
                }
                let caller_regs_f64 = std::mem::replace(&mut regs_f64, new_regs_f64);
                let caller_regs = std::mem::replace(&mut regs, new_regs);
                call_stack.push((caller_regs_f64, caller_regs, dst, pc));
                pc = idx + abs; // Jump to function body
            }
            61 => {
                // Return
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                if let Some((old_regs_f64, old_regs, ret_dst, ret_pc)) = call_stack.pop() {
                    let retv_f64 = regs_f64.get(src).copied().unwrap_or(0.0);
                    let retv = regs.get(src).cloned().unwrap_or(VMValue::Null);
                    regs_f64 = old_regs_f64;
                    regs = old_regs;
                    regs_f64[ret_dst] = retv_f64;
                    regs[ret_dst] = retv;
                    pc = ret_pc;
                } else {
                    // End execution
                    break;
                }
            }
            62 => {
                // CallBuiltin
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let name_idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut arg_regs: Vec<usize> = Vec::new();
                for _ in 0..argc {
                    let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    arg_regs.push(r);
                }

                // Get builtin name from constants
                let builtin_name = if let Some(VMValue::Str(name)) = consts.get(name_idx) {
                    name.clone()
                } else {
                    return Err("Invalid builtin name index".to_string());
                };

                // Convert register values to RuntimeValue for builtin call
                use crate::backends::builtins::RuntimeValue;
                let mut args: Vec<RuntimeValue> = Vec::new();
                for ar in arg_regs {
                    let val = match regs.get(ar) {
                        Some(VMValue::Number(_)) => VMValue::Number(regs_f64[ar]),
                        Some(other) => other.clone(),
                        None => VMValue::Null,
                    };
                    args.push(vm_value_to_builtin_runtime_value(val));
                }

                // Call the builtin function
                let builtins = crate::backends::builtins::BuiltinRegistry::new();
                let result = if let Some(func) = builtins.get(&builtin_name) {
                    func(&args)
                } else {
                    return Err(format!("Unknown builtin function: {}", builtin_name));
                };

                // Convert result back to VMValue
                regs[dst] = builtin_runtime_value_to_vm_value(result);
                if let VMValue::Number(num) = regs[dst] {
                    regs_f64[dst] = num;
                }
            }
            70 => {
                // EnvRuntimeLoadFile
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;

                let mut count = 0usize;

                // Get the path from src register - could be a string path or an Object from envFromFile
                let path_opt = match regs.get(src).cloned().unwrap_or(VMValue::Null) {
                    VMValue::Str(p) => Some(p),
                    VMValue::Object(_) => {
                        // If it's an Object from envFromFile, find the corresponding __envFromFile:path global
                        // by searching for a global that contains this exact object
                        let obj_val = regs.get(src).cloned().unwrap_or(VMValue::Null);
                        globals_names.iter().enumerate().find_map(|(i, name)| {
                            if name.starts_with("__envFromFile:") {
                                if let Some(VMValue::Object(_)) = globals_vals.get(i) {
                                    if globals_vals[i] == obj_val {
                                        return Some(
                                            name.strip_prefix("__envFromFile:")
                                                .unwrap()
                                                .to_string(),
                                        );
                                    }
                                }
                            }
                            None
                        })
                    }
                    _ => None,
                };

                if let Some(path) = path_opt {
                    // Parse the .env file and update __env:* globals
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for raw_line in content.lines() {
                            let line = raw_line.trim();
                            if line.is_empty() || line.starts_with('#') {
                                continue;
                            }
                            let line = if line.starts_with("export ") {
                                &line[7..]
                            } else {
                                line
                            };
                            if let Some(eq_pos) = line.find('=') {
                                let key = line[..eq_pos].trim().to_string();
                                let mut val_str = line[eq_pos + 1..].trim().to_string();

                                // Remove surrounding quotes if present
                                if val_str.starts_with('"')
                                    && val_str.ends_with('"')
                                    && val_str.len() >= 2
                                {
                                    val_str = val_str[1..val_str.len() - 1].to_string();
                                    val_str = val_str
                                        .replace("\\n", "\n")
                                        .replace("\\t", "\t")
                                        .replace("\\r", "\r");
                                } else if val_str.starts_with('\'')
                                    && val_str.ends_with('\'')
                                    && val_str.len() >= 2
                                {
                                    val_str = val_str[1..val_str.len() - 1].to_string();
                                } else {
                                    // Handle inline comments
                                    if let Some(hash_pos) = val_str.find('#') {
                                        let before = &val_str[..hash_pos];
                                        if before.ends_with(' ') {
                                            val_str = before.trim_end().to_string();
                                        }
                                    }
                                }

                                // Find or create __env:KEY global
                                let global_name = format!("__env:{}", key);
                                let global_idx = if let Some(idx) =
                                    globals_names.iter().position(|g| g == &global_name)
                                {
                                    idx
                                } else {
                                    // Add new global if not exists
                                    globals_names.push(global_name.clone());
                                    globals_vals.push(VMValue::Null);
                                    globals_names.len() - 1
                                };

                                // Update the global value
                                globals_vals[global_idx] = VMValue::Str(val_str);
                                count += 1;
                            }
                        }
                    }
                }

                regs[dst] = VMValue::Number(count as f64);
            }
            80 => {
                // NewObject
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut map = std::collections::HashMap::new();
                for _ in 0..count {
                    let k_idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let v_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let key = match consts.get(k_idx) {
                        Some(VMValue::Str(s)) => s.clone(),
                        _ => String::new(),
                    };
                    let val = match regs.get(v_reg) {
                        Some(VMValue::Number(_)) => VMValue::Number(regs_f64[v_reg]),
                        Some(other) => other.clone(),
                        None => VMValue::Null,
                    };
                    map.insert(key, val);
                }
                regs[dst] = VMValue::Object(map);
            }
            81 => {
                // NewArray
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut arr = Vec::with_capacity(count);
                for _ in 0..count {
                    let elem_reg =
                        u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let val = match regs.get(elem_reg) {
                        Some(VMValue::Number(_)) => VMValue::Number(regs_f64[elem_reg]),
                        Some(other) => other.clone(),
                        None => VMValue::Null,
                    };
                    arr.push(val);
                }
                regs[dst] = VMValue::Array(arr);
            }
            82 => {
                // NewTuple
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut tup = Vec::with_capacity(count);
                for _ in 0..count {
                    let elem_reg =
                        u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let val = match regs.get(elem_reg) {
                        Some(VMValue::Number(_)) => VMValue::Number(regs_f64[elem_reg]),
                        Some(other) => other.clone(),
                        None => VMValue::Null,
                    };
                    tup.push(val);
                }
                regs[dst] = VMValue::Tuple(tup);
            }
            83 => {
                // GetField
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let obj_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_kidx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_name = match consts.get(field_kidx) {
                    Some(VMValue::Str(s)) => s.as_str(),
                    _ => "",
                };
                let val = match regs.get(obj_reg) {
                    Some(VMValue::Object(map)) => {
                        map.get(field_name).cloned().unwrap_or(VMValue::Null)
                    }
                    Some(VMValue::Array(arr)) => match field_name {
                        "length" | "len" => VMValue::Number(arr.len() as f64),
                        "capacity" => VMValue::Number(arr.len() as f64),
                        _ => VMValue::Null,
                    },
                    Some(VMValue::Tuple(tup)) => match field_name {
                        "length" | "len" => VMValue::Number(tup.len() as f64),
                        _ => VMValue::Null,
                    },
                    Some(VMValue::Str(s)) => match field_name {
                        "length" | "len" => VMValue::Number(s.len() as f64),
                        _ => VMValue::Null,
                    },
                    _ => VMValue::Null,
                };
                if let VMValue::Number(n) = val {
                    regs_f64[dst] = n;
                }
                regs[dst] = val;
            }
            84 => {
                // SetField
                let obj_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_kidx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let field_name = match consts.get(field_kidx) {
                    Some(VMValue::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                let val = match regs.get(val_reg) {
                    Some(VMValue::Number(_)) => VMValue::Number(regs_f64[val_reg]),
                    Some(other) => other.clone(),
                    None => VMValue::Null,
                };
                if let Some(VMValue::Object(map)) = regs.get_mut(obj_reg) {
                    map.insert(field_name, val);
                }
            }
            85 => {
                // MakeClosure
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let fn_offset = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                pc += 4;
                let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut env = Vec::with_capacity(count);
                for _ in 0..count {
                    let creg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let val = match regs.get(creg) {
                        Some(VMValue::Number(_)) => VMValue::Number(regs_f64[creg]),
                        Some(other) => other.clone(),
                        None => VMValue::Null,
                    };
                    env.push(val);
                }
                regs[dst] = VMValue::Closure { fn_offset, env };
            }
            86 => {
                // CallMethod
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let obj_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let method_kidx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    let ar = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    let val = match regs.get(ar) {
                        Some(VMValue::Number(_)) => VMValue::Number(regs_f64[ar]),
                        Some(other) => other.clone(),
                        None => VMValue::Null,
                    };
                    args.push(val);
                }
                let method_name = match consts.get(method_kidx) {
                    Some(VMValue::Str(s)) => s.as_str(),
                    _ => "",
                };
                let res = match (regs.get_mut(obj_reg), method_name) {
                    (Some(VMValue::Array(arr)), "push") => {
                        if let Some(arg) = args.into_iter().next() {
                            arr.push(arg);
                        }
                        VMValue::Number(arr.len() as f64)
                    }
                    (Some(VMValue::Array(arr)), "pop") => arr.pop().unwrap_or(VMValue::Null),
                    (Some(VMValue::Array(arr)), "len") | (Some(VMValue::Array(arr)), "length") => {
                        VMValue::Number(arr.len() as f64)
                    }
                    (Some(VMValue::Array(arr)), "metadata_size") => {
                        let rval = vm_value_to_builtin_runtime_value(VMValue::Array(arr.clone()));
                        let res =
                            crate::backends::common::builtins::arrays::runtime_metadata_size(&[
                                rval,
                            ]);
                        match res {
                            crate::backends::builtins::RuntimeValue::Int(n) => {
                                VMValue::Number(n as f64)
                            }
                            _ => VMValue::Number(0.0),
                        }
                    }
                    (Some(VMValue::Array(arr)), "last") => {
                        arr.last().cloned().unwrap_or(VMValue::Null)
                    }
                    (Some(VMValue::Object(map)), "hasKey") => {
                        if let Some(VMValue::Str(k)) = args.first() {
                            VMValue::Bool(map.contains_key(k))
                        } else {
                            VMValue::Bool(false)
                        }
                    }
                    (Some(VMValue::Object(map)), "len")
                    | (Some(VMValue::Object(map)), "length") => VMValue::Number(map.len() as f64),
                    (Some(VMValue::Str(s)), "len") | (Some(VMValue::Str(s)), "length") => {
                        VMValue::Number(s.len() as f64)
                    }
                    (Some(receiver), "mock") => {
                        let mut mock_rvals: Vec<crate::backends::builtins::RuntimeValue> =
                            Vec::new();
                        for a in args {
                            mock_rvals.push(vm_value_to_builtin_runtime_value(a));
                        }
                        let builtins = crate::backends::builtins::BuiltinRegistry::new();
                        if let Some(func) = builtins.get("input.mock") {
                            func(&mock_rvals);
                        }
                        receiver.clone()
                    }
                    _ => VMValue::Null,
                };
                if let VMValue::Number(n) = res {
                    regs_f64[dst] = n;
                }
                regs[dst] = res;
            }
            87 => {
                // GetIndex
                let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let coll_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let idx_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val = match (regs.get(coll_reg), regs.get(idx_reg)) {
                    (Some(VMValue::Array(arr)), Some(VMValue::Number(n))) => {
                        let i = *n as usize;
                        arr.get(i).cloned().unwrap_or(VMValue::Null)
                    }
                    (Some(VMValue::Tuple(tup)), Some(VMValue::Number(n))) => {
                        let i = *n as usize;
                        tup.get(i).cloned().unwrap_or(VMValue::Null)
                    }
                    (Some(VMValue::Object(map)), Some(VMValue::Str(s))) => {
                        map.get(s).cloned().unwrap_or(VMValue::Null)
                    }
                    _ => VMValue::Null,
                };
                if let VMValue::Number(n) = val {
                    regs_f64[dst] = n;
                }
                regs[dst] = val;
            }
            88 => {
                // SetIndex
                let coll_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let idx_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val_reg = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let val = match regs.get(val_reg) {
                    Some(VMValue::Number(_)) => VMValue::Number(regs_f64[val_reg]),
                    Some(other) => other.clone(),
                    None => VMValue::Null,
                };
                let idx_val = regs.get(idx_reg).cloned();
                match (regs.get_mut(coll_reg), idx_val) {
                    (Some(VMValue::Array(arr)), Some(VMValue::Number(n))) => {
                        let i = n as usize;
                        if i < arr.len() {
                            arr[i] = val;
                        }
                    }
                    (Some(VMValue::Object(map)), Some(VMValue::Str(s))) => {
                        map.insert(s, val);
                    }
                    _ => {}
                }
            }
            15 => {
                // DeferPush
                let block_offset =
                    u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                let block_len = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                pc += 4;
                current_defers.push((block_offset, block_len));
            }
            16 => {
                // DeferRun
                // Execute all defers in LIFO order; offsets are relative to code section start.
                // Keep pc unchanged (pointing to next instruction).
                while let Some((block_offset, block_len)) = current_defers.pop() {
                    let mut block_pc = code_base.checked_add(block_offset).ok_or("pc overflow")?;
                    let block_end = block_pc.checked_add(block_len).ok_or("pc overflow")?;
                    while block_pc < block_end {
                        let op = data[block_pc];
                        block_pc += 1;
                        match ROp::from_u8(op) {
                            Some(ROp::Print) => {
                                let r = u32::from_le_bytes(
                                    data[block_pc..block_pc + 4].try_into().unwrap(),
                                ) as usize;
                                block_pc += 4;
                                if let Some(v) = regs.get(r) {
                                    match v {
                                        VMValue::Number(n) => {
                                            let _ = write!(out, "{}", n);
                                        }
                                        VMValue::Str(s) => {
                                            let _ = write!(out, "{}", s);
                                        }
                                        VMValue::Null => {
                                            let _ = write!(out, "null");
                                        }
                                        VMValue::BigInt(bi) => {
                                            let _ = write!(out, "{}", bi);
                                        }
                                        VMValue::U64(u) => {
                                            let _ = write!(out, "0x{:x}", u);
                                        }
                                        VMValue::Bool(b) => {
                                            let _ = write!(out, "{}", b);
                                        }
                                        VMValue::Array(_) => {
                                            let _ = write!(out, "[...]");
                                        }
                                        VMValue::Tuple(_) => {
                                            let _ = write!(out, "(...)");
                                        }
                                        VMValue::Object(_) => {
                                            let _ = write!(out, "{{...}}");
                                        }
                                        VMValue::Closure { .. } => {
                                            let _ = write!(out, "<closure>");
                                        }
                                    }
                                }
                            }
                            Some(ROp::PrintNewline) => {
                                let _ = writeln!(out);
                            }
                            Some(ROp::PrintSpace) => {
                                let _ = write!(out, " ");
                            }
                            Some(ROp::LoadConst) => {
                                let dst = u32::from_le_bytes(
                                    data[block_pc..block_pc + 4].try_into().unwrap(),
                                ) as usize;
                                block_pc += 4;
                                let k = u32::from_le_bytes(
                                    data[block_pc..block_pc + 4].try_into().unwrap(),
                                ) as usize;
                                block_pc += 4;
                                if dst < regs.len() {
                                    regs[dst] = consts.get(k).cloned().unwrap_or(VMValue::Null);
                                }
                            }
                            _ => {
                                // Skip unsupported opcodes in defer blocks for now
                            }
                        }
                    }
                }
            }
            255 => break, // Halt
            _ => {
                eprintln!(
                    "[VM ERROR] Unknown/unmatched opcode {} at pc={} (offset from code_base={})",
                    op,
                    pc - 1,
                    (pc - 1) - code_base
                );
                return Err(format!("unknown opcode {}", op));
            }
        }
    }
    Ok(())
}
