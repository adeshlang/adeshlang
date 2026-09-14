//! Expression lowering
//!
//! This module handles lowering of HIR expressions to LIR instructions,
//! including literals, operators, function calls, and complex expressions.

use super::super::{LirFunction, LirInst, LirModule, LirType, ValueId};
use super::core::{LowerCtx, has_return, is_builtin, is_namespace, next_lambda_name};
use super::functions::{collect_free_vars, insert_exception_check};
use super::statements::lower_stmt;
use super::types::elem_size_from_hir_type;
use crate::parsing::hir::{BinOp, HirExpr, HirLiteral, HirType, UnaryOp};
use std::collections::HashSet;

/// Get the return type of a known builtin function
/// Used for type inference when lowering expressions
fn builtin_return_type(name: &str) -> LirType {
    let name = name.strip_prefix("std:").unwrap_or(name);
    match name {
        // Timing/performance builtins that return f64
        "clock" => LirType::F64,

        // Math functions that return f64
        "sqrt" | "pow" | "abs" | "floor" | "ceil" | "round" | "sin" | "cos" | "tan" | "asin"
        | "acos" | "atan" | "atan2" | "exp" | "log" | "log10" | "log2" | "trunc" | "sign"
        | "Math.PI" | "Math.E" | "Math.TAU" | "Math.SQRT2" | "Math.LN2" | "Math.LN10"
        | "Math.random" | "Math.floor" | "Math.ceil" | "Math.round" | "Math.abs" | "Math.sqrt"
        | "Math.pow" | "Math.sin" | "Math.cos" | "Math.tan" | "Math.asin" | "Math.acos"
        | "Math.atan" | "Math.atan2" | "Math.exp" | "Math.log" | "Math.log10" | "Math.log2"
        | "Math.trunc" | "Math.sign" | "div" => LirType::F64,

        // Functions that return integers
        "len" | "int" | "argc" | "argsCount" | "int_div" | "Math.randomInt" | "sizeof" => {
            LirType::I64
        }

        // Functions that return booleans
        "bool" | "is_null" | "eq" | "ne" | "strict_eq" | "strict_ne" | "in" | "instanceof"
        | "hasKey" | "fs.exists" | "fs.isFile" | "fs.isDir" | "fs.mkdir" | "fs.delete"
        | "fs.write" | "fs.copy" | "fs.move" | "Regex.test" | "Regex.isMatch"
        | "Regex.fullMatch" => LirType::Bool,

        // Functions that return strings
        "str" | "type" | "typeof" | "envGet" | "envFileGet" | "execName" | "substr" | "concat"
        | "format" | "fs.read" | "fs.path.basename" | "fs.path.dirname" | "fs.path.extname"
        | "fs.path.join" | "Regex.escape" | "Regex.replace" | "Regex.replaceAll" => LirType::Ptr,

        // Default to generic pointer type for other builtins
        _ => LirType::Ptr,
    }
}

fn resolve_namespace_prefix(
    expr: &HirExpr,
    imports: &crate::utils::collections::FastMap<String, String>,
) -> Option<String> {
    match expr {
        HirExpr::LoadVar(name) => {
            if is_namespace(name) {
                Some(name.clone())
            } else if name == "fs" || name == "std:fs" {
                Some("fs".to_string())
            } else if name == "Regex" || name == "std:Regex" {
                Some("Regex".to_string())
            } else if let Some(imported_path) = imports.get(name) {
                if imported_path == "fs" || imported_path == "std:fs" {
                    Some("fs".to_string())
                } else if imported_path == "Regex" || imported_path == "std:Regex" {
                    Some("Regex".to_string())
                } else {
                    None
                }
            } else {
                None
            }
        }
        HirExpr::MemberAccess(target, field) => {
            if let Some(parent) = resolve_namespace_prefix(target, imports) {
                Some(format!("{}.{}", parent, field))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn lower_expr(
    lir: &mut LirModule,
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    expr: &HirExpr,
) -> Result<ValueId, String> {
    match expr {
        HirExpr::Cast(inner, target_ty) => {
            let src = lower_expr(lir, func, ctx, inner)?;
            let dst = func.alloc_value();

            let to_lir_ty = match target_ty {
                HirType::Int
                | HirType::I64
                | HirType::I32
                | HirType::I16
                | HirType::I8
                | HirType::U8
                | HirType::U16
                | HirType::U32
                | HirType::U64
                | HirType::U128 => LirType::I64,
                HirType::Float | HirType::F64 | HirType::F32 => LirType::F64,
                HirType::Bool => LirType::Bool,
                _ => LirType::Ptr,
            };

            let from_lir_ty = ctx.value_types.get(&src).cloned().unwrap_or(LirType::I64);

            match (from_lir_ty, to_lir_ty) {
                (LirType::I64, LirType::F64) => {
                    func.push_to_block(ctx.current_block, LirInst::I64ToF64(dst, src));
                }
                (LirType::F64, LirType::I64) => {
                    func.push_to_block(ctx.current_block, LirInst::F64ToI64(dst, src));
                }
                _ => {
                    func.push_to_block(ctx.current_block, LirInst::Copy(dst, src));
                }
            }

            ctx.value_types.insert(dst, to_lir_ty);
            Ok(dst)
        }
        HirExpr::Literal(lit) => {
            let val = func.alloc_value();
            match lit {
                HirLiteral::Int(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstI64(val, *n));
                    ctx.value_types.insert(val, LirType::I64);
                }
                HirLiteral::Float(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstF64(val, *n));
                    ctx.value_types.insert(val, LirType::F64);
                }
                HirLiteral::Bool(b) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstBool(val, *b));
                    ctx.value_types.insert(val, LirType::Bool);
                }
                HirLiteral::String(s) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstString(val, s.clone()));
                    ctx.value_types.insert(val, LirType::Ptr);
                }
                HirLiteral::Char(c) => {
                    let char_str = c.to_string();
                    let str_val = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ConstString(str_val, char_str));
                    ctx.value_types.insert(str_val, LirType::Ptr);
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(val, "char".to_string(), vec![str_val]),
                    );
                    ctx.value_types.insert(val, LirType::I64);
                }
                HirLiteral::BigInt(bi) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstBigInt(val, bi.clone()));
                    ctx.value_types.insert(val, LirType::Ptr);
                }
                HirLiteral::Null => {
                    func.push_to_block(ctx.current_block, LirInst::ConstNull(val));
                    ctx.value_types.insert(val, LirType::Ptr);
                }
                // Fixed-width integer types (unsigned) - use proper LIR instructions
                HirLiteral::U8(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstU8(val, *n));
                    ctx.value_types.insert(val, LirType::U8);
                }
                HirLiteral::U16(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstU16(val, *n));
                    ctx.value_types.insert(val, LirType::U16);
                }
                HirLiteral::U32(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstU32(val, *n));
                    ctx.value_types.insert(val, LirType::U32);
                }
                HirLiteral::U64(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstU64(val, *n));
                    ctx.value_types.insert(val, LirType::U64);
                }
                HirLiteral::U128(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstU128(val, *n));
                    ctx.value_types.insert(val, LirType::U128);
                }
                // Fixed-width integer types (signed) - use proper LIR instructions
                HirLiteral::I8(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstI8(val, *n));
                    ctx.value_types.insert(val, LirType::I8);
                }
                HirLiteral::I16(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstI16(val, *n));
                    ctx.value_types.insert(val, LirType::I16);
                }
                HirLiteral::I32(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstI32(val, *n));
                    ctx.value_types.insert(val, LirType::I32);
                }
                HirLiteral::I64(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstI64(val, *n));
                    ctx.value_types.insert(val, LirType::I64);
                }
                HirLiteral::I128(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstI128(val, *n));
                    ctx.value_types.insert(val, LirType::I128);
                }
                // Fixed-width float types - use proper LIR instructions
                HirLiteral::F32(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstF32(val, *n));
                    ctx.value_types.insert(val, LirType::F32);
                }
                HirLiteral::F64(n) => {
                    func.push_to_block(ctx.current_block, LirInst::ConstF64(val, *n));
                    ctx.value_types.insert(val, LirType::F64);
                }
            }
            Ok(val)
        }

        HirExpr::LoadVar(name) => {
            // Always emit LoadVar to ensure we get the latest value (important for loops)
            let val = func.alloc_value();
            func.push_to_block(ctx.current_block, LirInst::LoadVar(val, name.clone()));
            // Track the type of this loaded value if we know the variable's type
            if let Some(var_type) = ctx.var_types.get(name) {
                ctx.value_types.insert(val, *var_type);
            }
            Ok(val)
        }

        HirExpr::StoreVar(name, value) => {
            let val = lower_expr(lir, func, ctx, value)?;
            func.push_to_block(ctx.current_block, LirInst::StoreVar(name.clone(), val));
            func.set_var(name.clone(), val);
            // Track the type of this variable based on the stored value
            if let Some(value_type) = ctx.value_types.get(&val) {
                ctx.var_types.insert(name.clone(), *value_type);
            }
            Ok(val)
        }

        HirExpr::BinaryOp(left, op, right) => {
            let left_val = lower_expr(lir, func, ctx, left)?;
            let right_val = lower_expr(lir, func, ctx, right)?;
            let result = func.alloc_value();

            // Get the type of the left operand to determine operation type
            let left_type = ctx
                .value_types
                .get(&left_val)
                .copied()
                .unwrap_or(LirType::I64);

            let inst = match op {
                BinOp::Add => {
                    // For addition, we need to handle both numeric and string concatenation
                    // At LIR level, we don't have full type information, so we check:
                    // 1. If either operand is a String literal -> use str_concat
                    // 2. If left type is marked as Ptr (could be string) -> use str_concat
                    // 3. Otherwise assume numeric
                    let is_string_or_unknown =
                        matches!(left.as_ref(), HirExpr::Literal(HirLiteral::String(_)))
                            || matches!(right.as_ref(), HirExpr::Literal(HirLiteral::String(_)))
                            || matches!(left_type, LirType::Ptr);

                    if is_string_or_unknown {
                        // Use polymorphic str_concat builtin for string concatenation
                        // This also handles numeric values by converting them to strings
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltin(
                                result,
                                "str_concat".to_string(),
                                vec![left_val, right_val],
                            ),
                        );
                        ctx.value_types.insert(result, LirType::Ptr);
                        return Ok(result);
                    }

                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => LirInst::AddF64(result, left_val, right_val),
                        _ => LirInst::AddI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst.clone());
                    ctx.value_types.insert(result, left_type);
                    return Ok(result);
                }
                BinOp::Sub => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => LirInst::SubF64(result, left_val, right_val),
                        _ => LirInst::SubI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst.clone());
                    // Track the result type
                    ctx.value_types.insert(result, left_type);
                    return Ok(result);
                }
                BinOp::Mul => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => LirInst::MulF64(result, left_val, right_val),
                        _ => LirInst::MulI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst.clone());
                    // Track the result type
                    ctx.value_types.insert(result, left_type);
                    return Ok(result);
                }
                BinOp::Div => {
                    // Use polymorphic division that returns float
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(result, "div".to_string(), vec![left_val, right_val]),
                    );
                    ctx.value_types.insert(result, LirType::F64);
                    return Ok(result);
                }
                BinOp::IntDiv => {
                    // Integer division: floor(a / b)
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "int_div".to_string(),
                            vec![left_val, right_val],
                        ),
                    );
                    ctx.value_types.insert(result, LirType::I64);
                    return Ok(result);
                }
                BinOp::Mod => {
                    let inst = LirInst::ModI64(result, left_val, right_val);
                    func.push_to_block(ctx.current_block, inst);
                    ctx.value_types.insert(result, LirType::I64);
                    return Ok(result);
                }
                BinOp::Lt => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => {
                            LirInst::CmpLtF64(result, left_val, right_val)
                        }
                        _ => LirInst::CmpLtI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst);
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::Le => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => {
                            LirInst::CmpLeF64(result, left_val, right_val)
                        }
                        _ => LirInst::CmpLeI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst);
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::Gt => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => {
                            LirInst::CmpGtF64(result, left_val, right_val)
                        }
                        _ => LirInst::CmpGtI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst);
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::Ge => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => {
                            LirInst::CmpGeF64(result, left_val, right_val)
                        }
                        _ => LirInst::CmpGeI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst);
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::Eq => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => {
                            LirInst::CmpEqF64(result, left_val, right_val)
                        }
                        _ => LirInst::CmpEqI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst);
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::Ne => {
                    let inst = match left_type {
                        LirType::F32 | LirType::F64 => {
                            LirInst::CmpNeF64(result, left_val, right_val)
                        }
                        _ => LirInst::CmpNeI64(result, left_val, right_val),
                    };
                    func.push_to_block(ctx.current_block, inst);
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::StrictEq => {
                    // Strict equality: type and value must match
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "strict_eq".to_string(),
                            vec![left_val, right_val],
                        ),
                    );
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::StrictNe => {
                    // Strict inequality: type or value differs
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "strict_ne".to_string(),
                            vec![left_val, right_val],
                        ),
                    );
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                BinOp::And => LirInst::And(result, left_val, right_val),
                BinOp::Or => LirInst::Or(result, left_val, right_val),
                BinOp::NullCoalesce => {
                    // Null coalescing: return left if not null, otherwise right
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "null_coalesce".to_string(),
                            vec![left_val, right_val],
                        ),
                    );
                    return Ok(result);
                }
                BinOp::In => {
                    // Membership test: value in container
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(result, "in".to_string(), vec![left_val, right_val]),
                    );
                    return Ok(result);
                }
                BinOp::Instanceof => {
                    // Check if value is instance of type/class
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "instanceof".to_string(),
                            vec![left_val, right_val],
                        ),
                    );
                    return Ok(result);
                }
                BinOp::BitAnd => LirInst::BitAnd(result, left_val, right_val),
                BinOp::BitOr => LirInst::BitOr(result, left_val, right_val),
                BinOp::BitXor => LirInst::BitXor(result, left_val, right_val),
                BinOp::ShiftLeft => LirInst::Shl(result, left_val, right_val),
                BinOp::ShiftRight => LirInst::Shr(result, left_val, right_val),
                BinOp::Pow => {
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(result, "pow".to_string(), vec![left_val, right_val]),
                    );
                    return Ok(result);
                }
            };

            func.push_to_block(ctx.current_block, inst);
            Ok(result)
        }

        HirExpr::UnaryOp(op, operand) => {
            let operand_val = lower_expr(lir, func, ctx, operand)?;
            let result = func.alloc_value();

            let inst = match op {
                UnaryOp::Neg => LirInst::NegI64(result, operand_val),
                UnaryOp::Not => LirInst::Not(result, operand_val),
                UnaryOp::BitNot => {
                    let neg_one = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ConstI64(neg_one, -1));
                    LirInst::BitXor(result, operand_val, neg_one)
                }
                UnaryOp::Typeof => {
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(result, "typeof".to_string(), vec![operand_val]),
                    );
                    return Ok(result);
                }
            };

            func.push_to_block(ctx.current_block, inst);
            Ok(result)
        }

        HirExpr::Call(callee, args, type_args) => {
            let result = func.alloc_value();

            if let HirExpr::LoadVar(name) = callee.as_ref() {
                // Special handling for Promise(executor)
                if name == "Promise" && args.len() == 1 {
                    if let HirExpr::Lambda(params, body, _is_async) = &args[0] {
                        // Generate unique function name for the executor
                        let executor_name = next_lambda_name();

                        // Collect free variables that need to be captured
                        let captured_vars = collect_free_vars(params, body);

                        // Create LIR parameters - captured vars come first as hidden params
                        let mut lir_params: Vec<(String, LirType)> = captured_vars
                            .iter()
                            .map(|name| (format!("__capture_{}", name), LirType::Ptr))
                            .collect();
                        lir_params.extend(
                            params
                                .iter()
                                .map(|(pname, _ty)| (pname.clone(), LirType::Ptr)),
                        );

                        // Create the executor function
                        let mut executor_func =
                            LirFunction::new(executor_name.clone(), lir_params, LirType::Ptr);
                        let entry_block = executor_func.entry_block;

                        // Set up captured variable mappings
                        for cap_var in captured_vars.iter() {
                            let param_val = executor_func.alloc_value();
                            executor_func.push_to_block(
                                entry_block,
                                LirInst::LoadVar(param_val, format!("__capture_{}", cap_var)),
                            );
                            executor_func.push_to_block(
                                entry_block,
                                LirInst::StoreVar(cap_var.clone(), param_val),
                            );
                            executor_func.set_var(cap_var.clone(), param_val);
                        }

                        // Set up regular parameter mappings
                        for (pname, _) in params.iter() {
                            let param_val = executor_func.alloc_value();
                            executor_func.set_var(pname.clone(), param_val);
                        }

                        // Lower the body statements
                        let mut executor_ctx = LowerCtx {
                            current_block: entry_block,
                            loop_exit: None,
                            loop_cond: None,
                            catch_block: None,
                            decorated_fn_names: HashSet::new(),
                            value_types: std::collections::HashMap::new(),
                            var_types: std::collections::HashMap::new(),
                            ptr_elem_sizes: std::collections::HashMap::new(),
                            var_ptr_elem_sizes: std::collections::HashMap::new(),
                            drop_kinds: std::collections::HashMap::new(),
                            in_unsafe_context: false,
                            defer_scopes: vec![Vec::new()],
                            loop_defer_depths: Vec::new(),
                        };

                        for stmt in body.iter() {
                            lower_stmt(
                                lir,
                                &mut executor_func,
                                &mut executor_ctx,
                                stmt,
                                "executor",
                                None,
                            )?;
                        }

                        // Ensure function has a return
                        if !has_return(&executor_func) {
                            executor_func
                                .push_to_block(executor_ctx.current_block, LirInst::Return(None));
                        }

                        // Add the executor function to the module
                        lir.add_function(executor_func);

                        // Now generate instructions to:
                        // 1. Capture current values of free variables
                        // 2. Create the promise
                        // 3. Create resolve/reject callbacks bound to this promise
                        // 4. Call the executor with captured vars + resolve/reject

                        // Load captured variable values from current scope
                        let mut captured_vals = Vec::new();
                        for cap_var in &captured_vars {
                            let cap_val = func.alloc_value();
                            func.push_to_block(
                                ctx.current_block,
                                LirInst::LoadVar(cap_val, cap_var.clone()),
                            );
                            captured_vals.push(cap_val);
                        }

                        // Create the promise - callbuiltin Promise.new returns Promise(id)
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltin(result, "Promise.new".to_string(), vec![]),
                        );

                        // Create resolve callback (stores promise id internally)
                        let resolve_val = func.alloc_value();
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltin(
                                resolve_val,
                                "__make_resolve".to_string(),
                                vec![result],
                            ),
                        );

                        // Create reject callback
                        let reject_val = func.alloc_value();
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltin(
                                reject_val,
                                "__make_reject".to_string(),
                                vec![result],
                            ),
                        );

                        // Call the executor with captured vars + resolve, reject
                        let mut call_args = captured_vals;
                        call_args.push(resolve_val);
                        call_args.push(reject_val);
                        let _call_result = func.alloc_value();
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::Call(_call_result, executor_name, call_args),
                        );

                        // Check if executor threw an exception and reject the promise if so
                        let check_exc_result = func.alloc_value();
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltin(
                                check_exc_result,
                                "__check_executor_exception".to_string(),
                                vec![result],
                            ),
                        );

                        return Ok(result);
                    }
                }

                // Regular builtin or function call
                let arg_vals: Result<Vec<_>, _> =
                    args.iter().map(|a| lower_expr(lir, func, ctx, a)).collect();
                let arg_vals = arg_vals?;

                // Special handling for memory operations: alloc(size), free(ptr)
                if name == "alloc" || name == "__alloc" {
                    // Check unsafe context
                    if !ctx.in_unsafe_context {
                        return Err(format!(
                            "Memory operation '{}' requires unsafe block. Use: unsafe {{ alloc(...) }}",
                            name
                        ));
                    }

                    // Emit LIR::Alloc instruction instead of CallBuiltin
                    if arg_vals.len() != 1 {
                        return Err(format!(
                            "alloc() expects exactly 1 argument (size), got {}",
                            arg_vals.len()
                        ));
                    }
                    func.push_to_block(ctx.current_block, LirInst::Alloc(result, arg_vals[0]));
                    return Ok(result);
                }

                if name == "free" || name == "__free" {
                    // Check unsafe context
                    if !ctx.in_unsafe_context {
                        return Err(format!(
                            "Memory operation '{}' requires unsafe block. Use: unsafe {{ free(...) }}",
                            name
                        ));
                    }

                    // Emit LIR::Free instruction
                    if arg_vals.len() != 1 {
                        return Err(format!(
                            "free() expects exactly 1 argument (pointer), got {}",
                            arg_vals.len()
                        ));
                    }
                    // free() returns null in LIR
                    func.push_to_block(ctx.current_block, LirInst::Free(arg_vals[0]));
                    func.push_to_block(ctx.current_block, LirInst::ConstNull(result));
                    return Ok(result);
                }

                if is_builtin(name) {
                    // Check if this is a generic builtin call (like input<u8>())
                    if !type_args.is_empty() {
                        // Use CallBuiltinGeneric with the first type argument
                        let generic_type = type_args[0].clone();
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltinGeneric(
                                result,
                                name.clone(),
                                arg_vals,
                                generic_type,
                            ),
                        );
                    } else {
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::CallBuiltin(result, name.clone(), arg_vals),
                        );
                    }
                    // Track return types for known builtins
                    let return_type = builtin_return_type(name);
                    ctx.value_types.insert(result, return_type);
                    insert_exception_check(func, ctx);
                    return Ok(result);
                }

                // Check if this is a decorated function - if so, use call_indirect
                // because the function is stored as a variable containing a lambda
                if ctx.decorated_fn_names.contains(name) {
                    let callee_val = func.alloc_value();
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::LoadVar(callee_val, name.clone()),
                    );
                    let mut call_indirect_args = vec![callee_val];
                    call_indirect_args.extend(arg_vals);
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "call_indirect".to_string(),
                            call_indirect_args,
                        ),
                    );
                    insert_exception_check(func, ctx);
                } else {
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::Call(result, name.clone(), arg_vals),
                    );
                    insert_exception_check(func, ctx);
                }
            } else {
                // Indirect call - callee is an expression (e.g., m.modAdd, obj.method, fn_var)
                // We need to include the callee value as the first argument to call_indirect
                let callee_val = lower_expr(lir, func, ctx, callee)?;
                let arg_vals: Result<Vec<_>, _> =
                    args.iter().map(|a| lower_expr(lir, func, ctx, a)).collect();
                let arg_vals = arg_vals?;

                // Prepend callee to arguments for call_indirect
                let mut call_indirect_args = vec![callee_val];
                call_indirect_args.extend(arg_vals);
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(result, "call_indirect".to_string(), call_indirect_args),
                );
                insert_exception_check(func, ctx);
            }

            Ok(result)
        }

        HirExpr::ArrayLiteral(elements) => {
            // Check if any element is a spread expression
            let has_spread = elements.iter().any(|e| matches!(e, HirExpr::Spread(_)));

            if has_spread {
                // Use spread-aware array construction
                let mut all_vals = Vec::new();
                for elem in elements {
                    if let HirExpr::Spread(inner) = elem {
                        // For spread, get the array value and mark it for spreading
                        let inner_val = lower_expr(lir, func, ctx, inner)?;
                        let marker = func.alloc_value();
                        func.push_to_block(ctx.current_block, LirInst::ConstBool(marker, true));
                        all_vals.push(inner_val);
                        all_vals.push(marker);
                    } else {
                        let elem_val = lower_expr(lir, func, ctx, elem)?;
                        let marker = func.alloc_value();
                        func.push_to_block(ctx.current_block, LirInst::ConstBool(marker, false));
                        all_vals.push(elem_val);
                        all_vals.push(marker);
                    }
                }
                let result = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(result, "make_array_spread".to_string(), all_vals),
                );
                Ok(result)
            } else {
                let elem_vals: Result<Vec<_>, _> = elements
                    .iter()
                    .map(|e| lower_expr(lir, func, ctx, e))
                    .collect();
                let elem_vals = elem_vals?;

                let result = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(result, "make_array".to_string(), elem_vals),
                );
                Ok(result)
            }
        }

        HirExpr::ObjectLiteral(fields) => {
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "make_object".to_string(), vec![]),
            );

            // Populate the object with fields
            let mut current_obj = result;
            for (key, value) in fields {
                let key_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::ConstString(key_val, key.clone()),
                );
                let val_val = lower_expr(lir, func, ctx, value)?;
                let new_obj = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(
                        new_obj,
                        "set_field".to_string(),
                        vec![current_obj, key_val, val_val],
                    ),
                );
                current_obj = new_obj;
            }

            Ok(current_obj)
        }

        HirExpr::StructLiteral(_name, fields) => {
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "make_object".to_string(), vec![]),
            );

            // Populate the object with fields
            let mut current_obj = result;
            for (key, value) in fields {
                let key_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::ConstString(key_val, key.clone()),
                );
                let val_val = lower_expr(lir, func, ctx, value)?;
                let new_obj = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(
                        new_obj,
                        "set_field".to_string(),
                        vec![current_obj, key_val, val_val],
                    ),
                );
                current_obj = new_obj;
            }

            Ok(current_obj)
        }

        HirExpr::Index(target, index) => {
            let target_val = lower_expr(lir, func, ctx, target)?;
            let index_val = lower_expr(lir, func, ctx, index)?;
            let result = func.alloc_value();

            // Check if target is a pointer: if target_val's type is Ptr and has elem_size metadata, emit PtrLoad
            let is_ptr = if let Some(LirType::Ptr) = ctx.value_types.get(&target_val) {
                ctx.ptr_elem_sizes.get(&target_val).is_some()
            } else if let HirExpr::LoadVar(name) = &**target {
                // Check if variable is a pointer with known elem_size
                if let Some(LirType::Ptr) = ctx.var_types.get(name) {
                    ctx.var_ptr_elem_sizes.get(name).is_some()
                } else {
                    false
                }
            } else {
                false
            };

            if is_ptr {
                // Pointer indexing: use PtrLoad(result, ptr, index)
                func.push_to_block(
                    ctx.current_block,
                    LirInst::PtrLoad(result, target_val, index_val),
                );
                ctx.value_types.insert(result, LirType::I64);
            } else {
                // Regular array/object indexing: use get_index builtin
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(
                        result,
                        "get_index".to_string(),
                        vec![target_val, index_val],
                    ),
                );
            }
            Ok(result)
        }

        HirExpr::MemberAccess(target, field) => {
            // Check if this is a namespace member access (e.g., Math.PI, Date.now)
            if let Some(ns_prefix) = resolve_namespace_prefix(target, &lir.imports) {
                let builtin_name = format!("{}.{}", ns_prefix, field);
                let result = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(result, builtin_name, vec![]),
                );
                return Ok(result);
            }

            // Check if field is an ARC method called as property/member
            let target_val = lower_expr(lir, func, ctx, target)?;
            match field.as_str() {
                "strong_count" => {
                    let result = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ArcStrongCount(result, target_val));
                    ctx.value_types.insert(result, LirType::I64);
                    return Ok(result);
                }
                "weak_count" => {
                    let result = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ArcWeakCount(result, target_val));
                    ctx.value_types.insert(result, LirType::I64);
                    return Ok(result);
                }
                "is_alive" => {
                    let sc = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ArcStrongCount(sc, target_val));
                    let zero = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ConstI64(zero, 0));
                    let result = func.alloc_value();
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CmpGtI64(result, sc, zero),
                    );
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                _ => {}
            }

            let field_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(field_val, field.clone()),
            );

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "get_field".to_string(), vec![target_val, field_val]),
            );
            Ok(result)
        }

        HirExpr::Conditional(cond, then_expr, else_expr) => {
            let cond_val = lower_expr(lir, func, ctx, cond)?;

            let then_block = func.create_block("cond_then".to_string());
            let else_block = func.create_block("cond_else".to_string());
            let merge_block = func.create_block("cond_merge".to_string());

            func.push_to_block(
                ctx.current_block,
                LirInst::JumpIf(cond_val, then_block, else_block),
            );

            ctx.current_block = then_block;
            let then_val = lower_expr(lir, func, ctx, then_expr)?;
            let then_end_block = ctx.current_block;
            func.push_to_block(ctx.current_block, LirInst::Jump(merge_block));

            ctx.current_block = else_block;
            let else_val = lower_expr(lir, func, ctx, else_expr)?;
            let else_end_block = ctx.current_block;
            func.push_to_block(ctx.current_block, LirInst::Jump(merge_block));

            ctx.current_block = merge_block;
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::Phi(
                    result,
                    vec![(then_end_block, then_val), (else_end_block, else_val)],
                ),
            );

            Ok(result)
        }

        HirExpr::Lambda(params, body, is_async) => {
            // Generate unique function name for this lambda
            let lambda_name = next_lambda_name();

            // Collect free variables that need to be captured
            let captured_vars = collect_free_vars(params, body);

            // Create LIR parameters - captured vars come first as hidden params
            let mut lir_params: Vec<(String, LirType)> = captured_vars
                .iter()
                .map(|name| (format!("__capture_{}", name), LirType::Ptr))
                .collect();
            lir_params.extend(
                params
                    .iter()
                    .map(|(name, _ty)| (name.clone(), LirType::Ptr)),
            );

            // Create a new function for this lambda
            let mut lambda_func =
                LirFunction::new(lambda_name.clone(), lir_params.clone(), LirType::Ptr);
            let entry_block = lambda_func.entry_block;

            // Set up captured variable mappings - load from hidden params and store to regular names
            for cap_var in captured_vars.iter() {
                let param_val = lambda_func.alloc_value();
                lambda_func.push_to_block(
                    entry_block,
                    LirInst::LoadVar(param_val, format!("__capture_{}", cap_var)),
                );
                lambda_func
                    .push_to_block(entry_block, LirInst::StoreVar(cap_var.clone(), param_val));
                lambda_func.set_var(cap_var.clone(), param_val);
            }

            // Set up regular parameter mappings
            for (name, _) in params.iter() {
                let param_val = lambda_func.alloc_value();
                lambda_func.set_var(name.clone(), param_val);
            }

            // Lower the body statements
            let mut lambda_ctx = LowerCtx {
                current_block: entry_block,
                loop_exit: None,
                loop_cond: None,
                catch_block: None,
                decorated_fn_names: HashSet::new(),
                value_types: std::collections::HashMap::new(),
                var_types: std::collections::HashMap::new(),
                ptr_elem_sizes: std::collections::HashMap::new(),
                var_ptr_elem_sizes: std::collections::HashMap::new(),
                drop_kinds: std::collections::HashMap::new(),
                in_unsafe_context: false,
                defer_scopes: vec![Vec::new()],
                loop_defer_depths: Vec::new(),
            };

            for (i, stmt) in body.iter().enumerate() {
                let lambda_loc = format!("lambda.{}", i);
                lower_stmt(
                    lir,
                    &mut lambda_func,
                    &mut lambda_ctx,
                    stmt,
                    &lambda_loc,
                    None,
                )?;
            }

            // Ensure function has a return
            if !has_return(&lambda_func) {
                lambda_func.push_to_block(lambda_ctx.current_block, LirInst::Return(None));
            }

            // Add the lambda function to the module
            lir.add_function(lambda_func);

            // Return a reference to the function with captured variable names and async flag
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstFunc(result, lambda_name, captured_vars, *is_async),
            );
            Ok(result)
        }

        // Handle new HIR expressions
        HirExpr::MethodCall(obj, method, args) => {
            if method == "mock" {
                let is_ns = match obj.as_ref() {
                    HirExpr::LoadVar(v) => v == "input" || v == "Input",
                    _ => false,
                };
                let arg_vals: Result<Vec<_>, _> =
                    args.iter().map(|a| lower_expr(lir, func, ctx, a)).collect();
                let arg_vals = arg_vals?;
                let dummy = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(dummy, "input.mock".to_string(), arg_vals),
                );
                if is_ns {
                    let null_val = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ConstNull(null_val));
                    return Ok(null_val);
                } else {
                    return lower_expr(lir, func, ctx, obj);
                }
            }

            // Check if this is a namespace method call (e.g., Math.random(), Math.floor(x))
            if let Some(ns_prefix) = resolve_namespace_prefix(obj, &lir.imports) {
                let builtin_name = format!("{}.{}", ns_prefix, method);
                let arg_vals: Result<Vec<_>, _> =
                    args.iter().map(|a| lower_expr(lir, func, ctx, a)).collect();
                let arg_vals = arg_vals?;

                let result = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(result, builtin_name, arg_vals),
                );
                return Ok(result);
            }

            // Regular object method call
            let obj_val = lower_expr(lir, func, ctx, obj)?;
            // Check for ARC handle builtin methods
            match method.as_str() {
                "strong_count" => {
                    let result = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ArcStrongCount(result, obj_val));
                    ctx.value_types.insert(result, LirType::I64);
                    return Ok(result);
                }
                "weak_count" => {
                    let result = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ArcWeakCount(result, obj_val));
                    ctx.value_types.insert(result, LirType::I64);
                    return Ok(result);
                }
                "is_alive" => {
                    let sc = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ArcStrongCount(sc, obj_val));
                    let zero = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ConstI64(zero, 0));
                    let result = func.alloc_value();
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CmpGtI64(result, sc, zero),
                    );
                    ctx.value_types.insert(result, LirType::Bool);
                    return Ok(result);
                }
                "upgrade" => {
                    let result = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ArcClone(result, obj_val));
                    ctx.value_types.insert(result, LirType::Ptr);
                    return Ok(result);
                }
                _ => {}
            }

            let arg_vals: Vec<ValueId> = match method.as_str() {
                "map" | "filter" => {
                    if let Some(cb) = args.get(0) {
                        let cb_val = lower_expr(lir, func, ctx, cb)?;
                        vec![cb_val]
                    } else {
                        let null_fn = func.alloc_value();
                        func.push_to_block(ctx.current_block, LirInst::ConstNull(null_fn));
                        vec![null_fn]
                    }
                }
                "reduce" => {
                    let cb_val = if let Some(cb) = args.get(0) {
                        lower_expr(lir, func, ctx, cb)?
                    } else {
                        let null_fn = func.alloc_value();
                        func.push_to_block(ctx.current_block, LirInst::ConstNull(null_fn));
                        null_fn
                    };
                    let init_val = if let Some(init) = args.get(1) {
                        lower_expr(lir, func, ctx, init)?
                    } else {
                        let zero = func.alloc_value();
                        func.push_to_block(ctx.current_block, LirInst::ConstI64(zero, 0));
                        zero
                    };
                    vec![cb_val, init_val]
                }
                _ => {
                    let vals: Result<Vec<_>, _> =
                        args.iter().map(|a| lower_expr(lir, func, ctx, a)).collect();
                    vals?
                }
            };

            let result = func.alloc_value();

            // Use __call_method which will check for extended methods first, then builtin methods
            let method_name_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(method_name_val, method.clone()),
            );
            let mut method_call_args = vec![obj_val, method_name_val];
            method_call_args.extend(arg_vals);
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "__call_method".to_string(), method_call_args),
            );
            insert_exception_check(func, ctx);

            // For mutating array methods, update the variable with the result
            if let HirExpr::LoadVar(var_name) = obj.as_ref() {
                match method.as_str() {
                    "push" | "append" | "pop" | "shift" | "unshift" | "insert" | "remove"
                    | "clear" | "extend" | "sort" | "reverse" | "slice" | "map" | "filter" => {
                        func.push_to_block(
                            ctx.current_block,
                            LirInst::StoreVar(var_name.clone(), result),
                        );
                        func.set_var(var_name.clone(), result);
                    }
                    _ => {}
                }
            }
            Ok(result)
        }

        HirExpr::DictLiteral(entries) => {
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "make_dict".to_string(), vec![]),
            );
            for (key, val) in entries {
                let key_val = lower_expr(lir, func, ctx, key)?;
                let val_val = lower_expr(lir, func, ctx, val)?;
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(
                        result,
                        "dict_set".to_string(),
                        vec![result, key_val, val_val],
                    ),
                );
            }
            Ok(result)
        }

        HirExpr::SetLiteral(elements) => {
            let elem_vals: Result<Vec<_>, _> = elements
                .iter()
                .map(|e| lower_expr(lir, func, ctx, e))
                .collect();
            let elem_vals = elem_vals?;

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "make_set".to_string(), elem_vals),
            );
            Ok(result)
        }

        HirExpr::TupleLiteral(elements) => {
            let elem_vals: Result<Vec<_>, _> = elements
                .iter()
                .map(|e| lower_expr(lir, func, ctx, e))
                .collect();
            let elem_vals = elem_vals?;

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "make_tuple".to_string(), elem_vals),
            );
            Ok(result)
        }

        HirExpr::SetMember(obj, field, value) => {
            let obj_val = lower_expr(lir, func, ctx, obj)?;
            let val_val = lower_expr(lir, func, ctx, value)?;
            let field_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(field_val, field.clone()),
            );

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(
                    result,
                    "set_field".to_string(),
                    vec![obj_val, field_val, val_val],
                ),
            );

            // Store the modified object back to the variable if obj is a simple variable
            if let HirExpr::LoadVar(var_name) = obj.as_ref() {
                let name = var_name.clone();
                func.push_to_block(ctx.current_block, LirInst::StoreVar(name.clone(), result));
                func.set_var(name, result);
            }

            Ok(val_val)
        }

        HirExpr::NewInstance(class_name, args) => {
            let arg_vals: Result<Vec<_>, _> =
                args.iter().map(|a| lower_expr(lir, func, ctx, a)).collect();
            let arg_vals = arg_vals?;

            // Add the class name as first argument
            let class_name_val = func.alloc_value();

            // For Date, we don't need the class object, and loading 'Date' might fail if it's not defined
            if class_name == "Date" {
                func.push_to_block(
                    ctx.current_block,
                    LirInst::ConstString(class_name_val, class_name.clone()),
                );
            } else {
                // For user classes, load the class object from the variable
                func.push_to_block(
                    ctx.current_block,
                    LirInst::LoadVar(class_name_val, class_name.clone()),
                );
            }

            let mut all_args = vec![class_name_val];
            all_args.extend(arg_vals);

            let result = func.alloc_value();
            // Try specific constructor first (like __new_Date), fall back to generic
            let builtin_name = if class_name == "Date" {
                "__new_Date".to_string()
            } else {
                "__new_class".to_string()
            };

            if builtin_name == "__new_Date" {
                // Date doesn't need class name
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(result, builtin_name, all_args[1..].to_vec()),
                );
            } else {
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(result, builtin_name, all_args),
                );
            }
            Ok(result)
        }

        HirExpr::This => {
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::LoadVar(result, "this".to_string()),
            );
            Ok(result)
        }

        HirExpr::Super => {
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::LoadVar(result, "super".to_string()),
            );
            Ok(result)
        }

        HirExpr::Await(inner) => {
            // Lower the inner expression first (should evaluate to a Promise)
            let promise_val = lower_expr(lir, func, ctx, inner)?;

            // Call the await builtin which blocks until the promise is settled
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "await".to_string(), vec![promise_val]),
            );

            // If we're inside a try block, check for exception and jump to catch if needed
            if let Some(catch_target) = ctx.catch_block {
                let has_exc = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(has_exc, "__has_exception".to_string(), vec![]),
                );

                // Create a continuation block for when there's no exception
                let continue_block = func.create_block("await_continue".to_string());
                func.push_to_block(
                    ctx.current_block,
                    LirInst::JumpIf(has_exc, catch_target, continue_block),
                );
                ctx.current_block = continue_block;
            }

            Ok(result)
        }

        HirExpr::Spawn(inner) => {
            // Lower the inner expression (should be a function call)
            let task_val = lower_expr(lir, func, ctx, inner)?;

            // Call the spawn builtin which creates a new task
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "spawn".to_string(), vec![task_val]),
            );
            Ok(result)
        }

        HirExpr::Range(start, end, inclusive) => {
            let start_val = lower_expr(lir, func, ctx, start)?;
            let end_val = lower_expr(lir, func, ctx, end)?;
            let inclusive_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstBool(inclusive_val, *inclusive),
            );

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(
                    result,
                    "range".to_string(),
                    vec![start_val, end_val, inclusive_val],
                ),
            );
            Ok(result)
        }

        HirExpr::Spread(inner) => {
            let inner_val = lower_expr(lir, func, ctx, inner)?;

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "spread".to_string(), vec![inner_val]),
            );
            Ok(result)
        }

        HirExpr::OptionalGet(inner, field) => {
            let inner_val = lower_expr(lir, func, ctx, inner)?;

            // Register the field name as a string constant
            let field_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(field_val, field.clone()),
            );

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(
                    result,
                    "optional_get".to_string(),
                    vec![inner_val, field_val],
                ),
            );
            Ok(result)
        }

        HirExpr::NonNull(inner) => {
            let inner_val = lower_expr(lir, func, ctx, inner)?;

            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "nonnull".to_string(), vec![inner_val]),
            );
            Ok(result)
        }

        HirExpr::Update(target, is_increment, is_prefix) => {
            // Get the current value of the target
            let target_val = lower_expr(lir, func, ctx, target)?;

            // Create the increment/decrement amount
            let one_val = func.alloc_value();
            func.push_to_block(ctx.current_block, LirInst::ConstI64(one_val, 1));

            // Compute the new value using AddI64 or SubI64
            let new_val = func.alloc_value();
            if *is_increment {
                func.push_to_block(
                    ctx.current_block,
                    LirInst::AddI64(new_val, target_val, one_val),
                );
            } else {
                func.push_to_block(
                    ctx.current_block,
                    LirInst::SubI64(new_val, target_val, one_val),
                );
            }

            // Store back to the variable (if target is a variable)
            if let HirExpr::LoadVar(var_name) = target.as_ref() {
                func.push_to_block(
                    ctx.current_block,
                    LirInst::StoreVar(var_name.clone(), new_val),
                );
                func.set_var(var_name.clone(), new_val);
            }

            // Return the appropriate value based on prefix/postfix
            if *is_prefix {
                Ok(new_val)
            } else {
                Ok(target_val)
            }
        }

        HirExpr::Match(_scrutinee, _arms) => {
            // Match expressions are complex - for now, use builtin call
            let result = func.alloc_value();
            func.push_to_block(ctx.current_block, LirInst::ConstNull(result));
            Ok(result)
        }

        HirExpr::Format(inner, spec) => {
            // Format expressions: evaluate inner value and apply format spec
            let inner_val = lower_expr(lir, func, ctx, inner)?;
            let spec_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(spec_val, spec.clone()),
            );
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "format".to_string(), vec![inner_val, spec_val]),
            );
            Ok(result)
        }

        // Memory model expressions
        HirExpr::BorrowImmut(inner) => {
            // Borrow immutable: enforce via builtin
            let inner_val = lower_expr(lir, func, ctx, inner)?;
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "borrow_immut".to_string(), vec![inner_val]),
            );
            ctx.value_types.insert(result, LirType::Ptr);
            Ok(result)
        }

        HirExpr::BorrowMut(inner) => {
            // Borrow mutable: enforce exclusive via builtin
            let inner_val = lower_expr(lir, func, ctx, inner)?;
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, "borrow_mut".to_string(), vec![inner_val]),
            );
            ctx.value_types.insert(result, LirType::Ptr);
            Ok(result)
        }

        HirExpr::Borrow(inner, is_exclusive) => {
            // Unified borrow: auto-inferred exclusive or shared access
            let inner_val = lower_expr(lir, func, ctx, inner)?;
            let result = func.alloc_value();
            let builtin_name = if *is_exclusive {
                "borrow_mut"
            } else {
                "borrow_immut"
            };
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(result, builtin_name.to_string(), vec![inner_val]),
            );
            ctx.value_types.insert(result, LirType::Ptr);
            Ok(result)
        }

        HirExpr::Deref(ptr) => {
            // Dereference - for now, just evaluate ptr
            lower_expr(lir, func, ctx, ptr)
        }

        HirExpr::Share(inner) => {
            // Create shared reference (ARC)
            let inner_val = lower_expr(lir, func, ctx, inner)?;
            let result = func.alloc_value();
            func.push_to_block(ctx.current_block, LirInst::ArcNew(result, inner_val));
            ctx.value_types.insert(result, LirType::Ptr);
            Ok(result)
        }

        HirExpr::Downgrade(shared) => {
            // Create weak reference from shared reference
            let shared_val = lower_expr(lir, func, ctx, shared)?;
            let result = func.alloc_value();
            func.push_to_block(ctx.current_block, LirInst::WeakNew(result, shared_val));
            ctx.value_types.insert(result, LirType::Ptr);
            Ok(result)
        }

        HirExpr::Alloc(elem_ty, size) => {
            // Allocate memory with unsafe block (typed element size)
            let size_val = lower_expr(lir, func, ctx, size)?;
            let elem_size = elem_size_from_hir_type(elem_ty)? as i64;
            let result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::AllocTyped(result, size_val, elem_size),
            );
            ctx.value_types.insert(result, LirType::Ptr);
            // Track the element size for this pointer value for later use in Index lowering
            ctx.ptr_elem_sizes.insert(result, elem_size);
            Ok(result)
        }

        HirExpr::Free(ptr) => {
            // Free allocated memory with unsafe block
            let ptr_val = lower_expr(lir, func, ctx, ptr)?;
            func.push_to_block(ctx.current_block, LirInst::Free(ptr_val));
            let result = func.alloc_value();
            func.push_to_block(ctx.current_block, LirInst::ConstNull(result));
            Ok(result)
        }

        HirExpr::Move(inner) => {
            // Move expression - evaluate inner (ownership transfer)
            lower_expr(lir, func, ctx, inner)
        }

        HirExpr::AssignTuple(names, target) => {
            let val = lower_expr(lir, func, ctx, target)?;
            for name in names {
                func.push_to_block(ctx.current_block, LirInst::StoreVar(name.clone(), val));
            }
            Ok(val)
        }

        HirExpr::AssignObject(properties, target) => {
            let val = lower_expr(lir, func, ctx, target)?;
            for (name, alias) in properties {
                let target_var = alias.as_ref().unwrap_or(name);
                func.push_to_block(
                    ctx.current_block,
                    LirInst::StoreVar(target_var.clone(), val),
                );
            }
            Ok(val)
        }
    }
}
