//! Statement lowering
//!
//! This module handles lowering of HIR statements to LIR instructions,
//! including control flow, variable binding, and exception handling.

use super::super::{LirFunction, LirInst, LirModule, LirType};
use super::core::{
    DropLoweringKind, LowerCtx, has_return, is_block_terminated, pop_defer_scope, push_defer_scope,
};
use super::expressions::lower_expr;
use super::functions::{collect_free_vars, lower_class_def, lower_hir_function};
use super::memory::{
    emit_all_defers, emit_defers_until_depth, emit_drops_for_location, emit_scope_defers,
    record_var_kind,
};
use super::types::{apply_type_conversion, hir_type_to_lir};
use crate::parsing::drop_insertion::DropPlan;
use crate::parsing::hir::{BinOp, HirExpr, HirLiteral, HirStmt, HirType};
use std::collections::HashSet;

pub(super) fn lower_stmt(
    lir: &mut LirModule,
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    stmt: &HirStmt,
    loc: &str,
    drop_plan: Option<&DropPlan>,
) -> Result<(), String> {
    match stmt {
        HirStmt::Let {
            name,
            init,
            ty,
            is_const: _,
            is_borrowed,
        } => {
            if let Some(hir_type) = ty {
                record_var_kind(ctx, name, hir_type);
            }
            if let Some(init_expr) = init {
                // Auto-detect and record ARC drop kinds when type annotations are missing
                match init_expr {
                    HirExpr::Share(_) => {
                        ctx.drop_kinds
                            .insert(name.clone(), DropLoweringKind::Shared);
                    }
                    HirExpr::Downgrade(_) => {
                        ctx.drop_kinds.insert(name.clone(), DropLoweringKind::Weak);
                    }
                    // `handle.upgrade()` produces a new strong clone — must be dropped at scope exit
                    HirExpr::MethodCall(_, method, _) if method == "upgrade" => {
                        ctx.drop_kinds
                            .insert(name.clone(), DropLoweringKind::Shared);
                    }
                    _ => {}
                }

                // If this is a strong declaration, we want to emit ArcClone instead of ArcNew
                let val = if let HirExpr::Share(inner) = init_expr {
                    if is_borrowed == &Some(false) {
                        let inner_val = lower_expr(lir, func, ctx, inner)?;
                        let result = func.alloc_value();
                        func.push_to_block(ctx.current_block, LirInst::ArcClone(result, inner_val));
                        ctx.value_types.insert(result, LirType::Ptr);
                        result
                    } else {
                        lower_expr(lir, func, ctx, init_expr)?
                    }
                } else {
                    lower_expr(lir, func, ctx, init_expr)?
                };

                // Apply type conversion if a type annotation is present
                let converted_val = if let Some(hir_type) = ty {
                    apply_type_conversion(func, ctx, val, hir_type)?
                } else {
                    val
                };

                func.push_to_block(
                    ctx.current_block,
                    LirInst::StoreVar(name.clone(), converted_val),
                );
                func.set_var(name.clone(), converted_val);

                // Track the variable's type for proper operation selection (e.g., SubF64 vs SubI64)
                if let Some(value_type) = ctx.value_types.get(&converted_val) {
                    ctx.var_types.insert(name.clone(), *value_type);
                }
            }
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        HirStmt::LetTuple { names, init, .. } => {
            if let Some(init_expr) = init {
                let tuple_val = lower_expr(lir, func, ctx, init_expr)?;

                // Extract each element from the tuple by index
                for (i, name) in names.iter().enumerate() {
                    let idx_val = func.alloc_value();
                    func.push_to_block(ctx.current_block, LirInst::ConstI64(idx_val, i as i64));

                    let elem_val = func.alloc_value();
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CallBuiltin(
                            elem_val,
                            "get_index".to_string(),
                            vec![tuple_val, idx_val],
                        ),
                    );

                    func.push_to_block(
                        ctx.current_block,
                        LirInst::StoreVar(name.clone(), elem_val),
                    );
                    func.set_var(name.clone(), elem_val);
                }
            }
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        HirStmt::Assign {
            target,
            value,
            is_move: _,
        } => {
            if let HirExpr::LoadVar(name) = target {
                let val = lower_expr(lir, func, ctx, value)?;
                func.push_to_block(ctx.current_block, LirInst::StoreVar(name.clone(), val));
                func.set_var(name.clone(), val);
            } else {
                let val = lower_expr(lir, func, ctx, value)?;
                let _ = val;
            }
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        HirStmt::Expr(expr) => {
            let _ = lower_expr(lir, func, ctx, expr)?;
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        HirStmt::Return(expr) => {
            // Emit ALL defers from ALL active scopes in LIFO order before returning.
            // This unwinds every enclosing scope's defers (compile-time inlining, zero runtime overhead).
            emit_all_defers(lir, func, ctx, loc, drop_plan)?;

            let ret_val = expr
                .as_ref()
                .map(|e| lower_expr(lir, func, ctx, e))
                .transpose()?;
            emit_drops_for_location(func, ctx, drop_plan, loc);
            func.push_to_block(ctx.current_block, LirInst::Return(ret_val));
            Ok(())
        }

        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let cond_val = lower_expr(lir, func, ctx, cond)?;

            let then_block = func.create_block("then".to_string());
            let else_block = func.create_block("else".to_string());
            let merge_block = func.create_block("merge".to_string());

            func.push_to_block(
                ctx.current_block,
                LirInst::JumpIf(cond_val, then_block, else_block),
            );

            // Lower then branch — each branch is its own defer scope
            ctx.current_block = then_block;
            let then_loc = format!("{loc}.then");
            push_defer_scope(ctx);
            lower_stmt(lir, func, ctx, then_branch, &then_loc, drop_plan)?;
            if !is_block_terminated(func, ctx.current_block) {
                emit_scope_defers(lir, func, ctx, &then_loc, drop_plan)?;
                emit_drops_for_location(func, ctx, drop_plan, loc);
                func.push_to_block(ctx.current_block, LirInst::Jump(merge_block));
            } else {
                // Block terminated (e.g. by break/return) — pop the scope to keep stack balanced
                pop_defer_scope(ctx);
            }

            // Lower else branch — its own defer scope
            ctx.current_block = else_block;
            if let Some(else_stmt) = else_branch {
                let else_loc = format!("{loc}.else");
                push_defer_scope(ctx);
                lower_stmt(lir, func, ctx, else_stmt, &else_loc, drop_plan)?;
                if !is_block_terminated(func, ctx.current_block) {
                    emit_scope_defers(lir, func, ctx, &else_loc, drop_plan)?;
                    emit_drops_for_location(func, ctx, drop_plan, loc);
                } else {
                    pop_defer_scope(ctx);
                }
            }
            if !is_block_terminated(func, ctx.current_block) {
                func.push_to_block(ctx.current_block, LirInst::Jump(merge_block));
            }

            ctx.current_block = merge_block;
            Ok(())
        }

        HirStmt::While { cond, body } => {
            let cond_block = func.create_block("while_cond".to_string());
            let body_block = func.create_block("while_body".to_string());
            let exit_block = func.create_block("while_exit".to_string());

            func.push_to_block(ctx.current_block, LirInst::Jump(cond_block));

            ctx.current_block = cond_block;
            let cond_val = lower_expr(lir, func, ctx, cond)?;
            func.push_to_block(
                ctx.current_block,
                LirInst::JumpIf(cond_val, body_block, exit_block),
            );

            let saved_loop = ctx.loop_exit;
            let saved_loop_cond = ctx.loop_cond;
            ctx.loop_exit = Some(exit_block);
            ctx.loop_cond = Some(cond_block);
            // Record the defer scope depth at loop body entry for break/continue unwinding
            let saved_loop_defer_depth = ctx.defer_scopes.len();
            ctx.loop_defer_depths.push(saved_loop_defer_depth);
            ctx.current_block = body_block;
            let loop_loc = format!("{loc}.loop");
            lower_stmt(lir, func, ctx, body, &loop_loc, drop_plan)?;
            if !is_block_terminated(func, ctx.current_block) {
                emit_drops_for_location(func, ctx, drop_plan, loc);
                func.push_to_block(ctx.current_block, LirInst::Jump(cond_block));
            }
            ctx.loop_exit = saved_loop;
            ctx.loop_cond = saved_loop_cond;
            ctx.loop_defer_depths.pop();

            ctx.current_block = exit_block;
            Ok(())
        }

        HirStmt::ForIn { var, iter, body } => {
            // OPTIMIZATION: Detect range-based for loops and compile without array materialization
            // Pattern: for(i in start..end) or for(i in start...end)
            if let HirExpr::Range(start, end, inclusive) = iter {
                // ULTRA FAST PATH: Detect accumulator patterns (e.g. count = count + 1)
                if let HirStmt::Block(stmts) = &**body {
                    if stmts.len() == 1 {
                        let assign_opt: Option<(&String, &HirExpr)> = match &stmts[0] {
                            HirStmt::Assign {
                                target: HirExpr::LoadVar(target_name),
                                value,
                                ..
                            } => Some((target_name, value)),
                            HirStmt::Expr(HirExpr::StoreVar(target_name, value)) => {
                                Some((target_name, value.as_ref()))
                            }
                            _ => None,
                        };
                        if let Some((target_name, rhs)) = assign_opt {
                            if let HirExpr::BinaryOp(lhs_bin, BinOp::Add, rhs_bin) = rhs {
                                if let HirExpr::LoadVar(lhs_name) = lhs_bin.as_ref() {
                                    if lhs_name == target_name {
                                        if let HirExpr::Literal(lit) = rhs_bin.as_ref() {
                                            let lit_num = match lit {
                                                HirLiteral::Int(n) => *n,
                                                HirLiteral::U8(n) => *n as i64,
                                                HirLiteral::U16(n) => *n as i64,
                                                HirLiteral::U32(n) => *n as i64,
                                                HirLiteral::U64(n) => *n as i64,
                                                HirLiteral::I8(n) => *n as i64,
                                                HirLiteral::I16(n) => *n as i64,
                                                HirLiteral::I32(n) => *n as i64,
                                                HirLiteral::I64(n) => *n,
                                                HirLiteral::Float(n) => *n as i64,
                                                _ => 1,
                                            };
                                            let start_val = lower_expr(lir, func, ctx, start)?;
                                            let end_val = lower_expr(lir, func, ctx, end)?;
                                            let target_val = func.alloc_value();
                                            func.push_to_block(
                                                ctx.current_block,
                                                LirInst::LoadVar(target_val, target_name.clone()),
                                            );

                                            let diff_val = func.alloc_value();
                                            func.push_to_block(
                                                ctx.current_block,
                                                LirInst::SubI64(diff_val, end_val, start_val),
                                            );

                                            let mut count_val = diff_val;
                                            if *inclusive {
                                                let one_val = func.alloc_value();
                                                func.push_to_block(
                                                    ctx.current_block,
                                                    LirInst::ConstI64(one_val, 1),
                                                );
                                                let inc_val = func.alloc_value();
                                                func.push_to_block(
                                                    ctx.current_block,
                                                    LirInst::AddI64(inc_val, diff_val, one_val),
                                                );
                                                count_val = inc_val;
                                            }

                                            let scale_val = if lit_num == 1 {
                                                count_val
                                            } else {
                                                let lit_val = func.alloc_value();
                                                func.push_to_block(
                                                    ctx.current_block,
                                                    LirInst::ConstI64(lit_val, lit_num),
                                                );
                                                let scaled = func.alloc_value();
                                                func.push_to_block(
                                                    ctx.current_block,
                                                    LirInst::MulI64(scaled, count_val, lit_val),
                                                );
                                                scaled
                                            };

                                            let final_val = func.alloc_value();
                                            func.push_to_block(
                                                ctx.current_block,
                                                LirInst::AddI64(final_val, target_val, scale_val),
                                            );
                                            func.push_to_block(
                                                ctx.current_block,
                                                LirInst::StoreVar(target_name.clone(), final_val),
                                            );
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Optimized path: direct integer loop (no array creation)
                let start_val = lower_expr(lir, func, ctx, start)?;
                let end_val = lower_expr(lir, func, ctx, end)?;

                let cond_block = func.create_block("range_cond".to_string());
                let body_block = func.create_block("range_body".to_string());
                let exit_block = func.create_block("range_exit".to_string());
                let increment_block = func.create_block("range_increment".to_string());

                // Initialize loop variable to start value
                func.push_to_block(ctx.current_block, LirInst::StoreVar(var.clone(), start_val));
                func.push_to_block(ctx.current_block, LirInst::Jump(cond_block));

                // Condition: check if var < end (or <= end if inclusive)
                ctx.current_block = cond_block;
                let loop_var_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::LoadVar(loop_var_val, var.clone()),
                );
                let cond_val = func.alloc_value();
                if *inclusive {
                    // var <= end
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CmpLeI64(cond_val, loop_var_val, end_val),
                    );
                } else {
                    // var < end
                    func.push_to_block(
                        ctx.current_block,
                        LirInst::CmpLtI64(cond_val, loop_var_val, end_val),
                    );
                }
                func.push_to_block(
                    ctx.current_block,
                    LirInst::JumpIf(cond_val, body_block, exit_block),
                );

                // Body block
                ctx.current_block = body_block;
                let saved_loop = ctx.loop_exit;
                let saved_loop_cond = ctx.loop_cond;
                ctx.loop_exit = Some(exit_block);
                ctx.loop_cond = Some(increment_block);
                let saved_loop_defer_depth = ctx.defer_scopes.len();
                ctx.loop_defer_depths.push(saved_loop_defer_depth);
                let body_loc = format!("{loc}.loop");
                lower_stmt(lir, func, ctx, body, &body_loc, drop_plan)?;
                if !is_block_terminated(func, ctx.current_block) {
                    emit_drops_for_location(func, ctx, drop_plan, loc);
                    func.push_to_block(ctx.current_block, LirInst::Jump(increment_block));
                }
                ctx.loop_exit = saved_loop;
                ctx.loop_cond = saved_loop_cond;
                ctx.loop_defer_depths.pop();

                // Increment block: var = var + 1, then jump to cond_block
                ctx.current_block = increment_block;
                let current_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::LoadVar(current_val, var.clone()),
                );
                let one_val = func.alloc_value();
                func.push_to_block(ctx.current_block, LirInst::ConstI64(one_val, 1));
                let next_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::AddI64(next_val, current_val, one_val),
                );
                func.push_to_block(ctx.current_block, LirInst::StoreVar(var.clone(), next_val));

                func.push_to_block(ctx.current_block, LirInst::Jump(cond_block));

                ctx.current_block = exit_block;
                Ok(())
            } else {
                // Standard path: iterate over array/collection
                let iter_val = lower_expr(lir, func, ctx, iter)?;

                let cond_block = func.create_block("forin_cond".to_string());
                let body_block = func.create_block("forin_body".to_string());
                let exit_block = func.create_block("forin_exit".to_string());
                let increment_block = func.create_block("forin_increment".to_string());

                // Use a synthetic variable name for the loop index
                let idx_var = format!("__forin_idx_{}", func.blocks.len());

                let idx_init = func.alloc_value();
                func.push_to_block(ctx.current_block, LirInst::ConstI64(idx_init, 0));
                func.push_to_block(
                    ctx.current_block,
                    LirInst::StoreVar(idx_var.clone(), idx_init),
                );

                let len_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(len_val, "len".to_string(), vec![iter_val]),
                );

                func.push_to_block(ctx.current_block, LirInst::Jump(cond_block));

                // Condition block: load idx, compare with len
                ctx.current_block = cond_block;
                let idx_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::LoadVar(idx_val, idx_var.clone()),
                );
                let cond_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CmpLtI64(cond_val, idx_val, len_val),
                );
                func.push_to_block(
                    ctx.current_block,
                    LirInst::JumpIf(cond_val, body_block, exit_block),
                );

                // Body block
                ctx.current_block = body_block;
                let idx_for_elem = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::LoadVar(idx_for_elem, idx_var.clone()),
                );
                let elem_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(
                        elem_val,
                        "get_index".to_string(),
                        vec![iter_val, idx_for_elem],
                    ),
                );
                func.push_to_block(ctx.current_block, LirInst::StoreVar(var.clone(), elem_val));

                let saved_loop = ctx.loop_exit;
                let saved_loop_cond = ctx.loop_cond;
                ctx.loop_exit = Some(exit_block);
                ctx.loop_cond = Some(increment_block);
                let saved_loop_defer_depth = ctx.defer_scopes.len();
                ctx.loop_defer_depths.push(saved_loop_defer_depth);
                let body_loc = format!("{loc}.loop");
                lower_stmt(lir, func, ctx, body, &body_loc, drop_plan)?;
                if !is_block_terminated(func, ctx.current_block) {
                    emit_drops_for_location(func, ctx, drop_plan, loc);
                    func.push_to_block(ctx.current_block, LirInst::Jump(increment_block));
                }
                ctx.loop_exit = saved_loop;
                ctx.loop_cond = saved_loop_cond;
                ctx.loop_defer_depths.pop();

                // Increment block: index = index + 1, then jump to cond_block
                ctx.current_block = increment_block;
                let idx_current = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::LoadVar(idx_current, idx_var.clone()),
                );
                let one_val = func.alloc_value();
                func.push_to_block(ctx.current_block, LirInst::ConstI64(one_val, 1));
                let new_idx = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::AddI64(new_idx, idx_current, one_val),
                );
                func.push_to_block(
                    ctx.current_block,
                    LirInst::StoreVar(idx_var.clone(), new_idx),
                );

                func.push_to_block(ctx.current_block, LirInst::Jump(cond_block));

                ctx.current_block = exit_block;
                Ok(())
            }
        }

        HirStmt::Block(stmts) => {
            // Push a new defer scope for this block
            push_defer_scope(ctx);
            for (i, s) in stmts.iter().enumerate() {
                let inner_loc = format!("{loc}.b{}", i);
                lower_stmt(lir, func, ctx, s, &inner_loc, drop_plan)?;
                // If the current block is terminated (e.g. by a return), stop processing
                if is_block_terminated(func, ctx.current_block) {
                    break;
                }
            }
            // Emit this scope's defers in LIFO order (compile-time inlining, zero runtime overhead)
            if !is_block_terminated(func, ctx.current_block) {
                emit_scope_defers(lir, func, ctx, loc, drop_plan)?;
                emit_drops_for_location(func, ctx, drop_plan, loc);
            } else {
                // Block already terminated (e.g. return emitted all defers) — pop the scope
                pop_defer_scope(ctx);
            }
            Ok(())
        }

        HirStmt::FunctionDef {
            name,
            params,
            body,
            ret_type,
            is_async,
            ..
        } => {
            // Nested function definition - create a separate LIR function and store a reference
            // This allows nested async functions to be properly called

            // Collect free variables that need to be captured from the enclosing scope
            let param_names: Vec<(String, Option<HirType>)> = params
                .iter()
                .map(|(n, ty, _default)| (n.clone(), ty.clone()))
                .collect();
            let captured_vars = collect_free_vars(&param_names, body);

            // Create LIR parameters - captured vars come first as hidden params
            let mut lir_params: Vec<(String, LirType)> = captured_vars
                .iter()
                .map(|cv| (format!("__capture_{}", cv), LirType::Ptr))
                .collect();
            lir_params.extend(
                params
                    .iter()
                    .map(|(pname, _, _)| (pname.clone(), LirType::Ptr)),
            );

            let fn_ret_type = hir_type_to_lir(ret_type.as_ref());

            // Create a new function for this nested definition
            let mut nested_func = if *is_async {
                LirFunction::new_async(name.clone(), lir_params.clone(), fn_ret_type)
            } else {
                LirFunction::new(name.clone(), lir_params.clone(), fn_ret_type)
            };
            let entry_block = nested_func.entry_block;

            // Set up captured variable mappings
            for cap_var in captured_vars.iter() {
                let param_val = nested_func.alloc_value();
                nested_func.push_to_block(
                    entry_block,
                    LirInst::LoadVar(param_val, format!("__capture_{}", cap_var)),
                );
                nested_func
                    .push_to_block(entry_block, LirInst::StoreVar(cap_var.clone(), param_val));
                nested_func.set_var(cap_var.clone(), param_val);
            }

            // Set up regular parameter mappings
            for (pname, _, _) in params.iter() {
                let param_val = nested_func.alloc_value();
                nested_func.push_to_block(entry_block, LirInst::LoadVar(param_val, pname.clone()));
                nested_func.set_var(pname.clone(), param_val);
            }

            // Handle default values for parameters
            let mut nested_ctx = LowerCtx {
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

            for (pname, _ty, default) in params {
                if let Some(default_expr) = default {
                    // Generate: if param == null { param = default }
                    let param_val = nested_func
                        .get_var(pname)
                        .unwrap_or_else(|| nested_func.alloc_value());
                    let is_null = nested_func.alloc_value();
                    nested_func.push_to_block(
                        nested_ctx.current_block,
                        LirInst::CallBuiltin(is_null, "is_null".to_string(), vec![param_val]),
                    );

                    let default_block = nested_func.create_block("default".to_string());
                    let after_default = nested_func.create_block("after_default".to_string());

                    nested_func.push_to_block(
                        nested_ctx.current_block,
                        LirInst::JumpIf(is_null, default_block, after_default),
                    );

                    nested_ctx.current_block = default_block;
                    let default_val =
                        lower_expr(lir, &mut nested_func, &mut nested_ctx, default_expr)?;
                    nested_func.push_to_block(
                        nested_ctx.current_block,
                        LirInst::StoreVar(pname.clone(), default_val),
                    );
                    nested_func.set_var(pname.clone(), default_val);
                    nested_func
                        .push_to_block(nested_ctx.current_block, LirInst::Jump(after_default));

                    nested_ctx.current_block = after_default;
                }
            }

            // Lower the function body
            for (i, stmt) in body.iter().enumerate() {
                let nested_loc = format!("{loc}.fn.{}", i);
                lower_stmt(
                    lir,
                    &mut nested_func,
                    &mut nested_ctx,
                    stmt,
                    &nested_loc,
                    None,
                )?;
            }

            // Emit all remaining defers before function exit (compile-time inlining)
            emit_all_defers(lir, &mut nested_func, &mut nested_ctx, "fn.end", None)?;

            // Ensure function has a return
            if !has_return(&nested_func) {
                nested_func.push_to_block(nested_ctx.current_block, LirInst::Return(None));
            }

            // Add the nested function to the module
            lir.add_function(nested_func);

            // Store a reference to the function in the current scope
            let func_ref = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstFunc(func_ref, name.clone(), captured_vars, *is_async),
            );
            func.push_to_block(ctx.current_block, LirInst::StoreVar(name.clone(), func_ref));
            func.set_var(name.clone(), func_ref);
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        // Handle class definitions - store as a class object for instanceof checks
        HirStmt::ClassDef(class) => {
            let res = lower_class_def(lir, func, ctx, class);
            emit_drops_for_location(func, ctx, drop_plan, loc);
            res
        }

        // Handle try-catch with basic exception simulation
        // We use exception builtins to track thrown values
        HirStmt::TryCatch {
            try_block,
            error_name,
            catch_block,
        } => {
            // Create blocks for catch and after
            let catch_block_id = func.create_block("catch".to_string());
            let after_block = func.create_block("after_try".to_string());

            // Save previous catch block and set new one for nested try/catch
            let prev_catch = ctx.catch_block;
            ctx.catch_block = Some(catch_block_id);

            // Lower try block - if there's a throw, it will jump directly to catch
            let try_loc = format!("{loc}.try");
            lower_stmt(lir, func, ctx, try_block, &try_loc, drop_plan)?;

            // Restore previous catch block
            ctx.catch_block = prev_catch;

            // After try block (if no throw), jump to after block
            func.push_to_block(ctx.current_block, LirInst::Jump(after_block));

            // Catch block
            ctx.current_block = catch_block_id;
            // Bind error to error_name
            let err_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(err_val, "__get_exception".to_string(), vec![]),
            );
            func.push_to_block(
                ctx.current_block,
                LirInst::StoreVar(error_name.clone(), err_val),
            );
            func.set_var(error_name.clone(), err_val);
            // Clear exception
            let clear_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(clear_val, "__clear_exception".to_string(), vec![]),
            );

            let catch_loc = format!("{loc}.catch");
            lower_stmt(lir, func, ctx, catch_block, &catch_loc, drop_plan)?;
            func.push_to_block(ctx.current_block, LirInst::Jump(after_block));

            // Continue after try-catch
            ctx.current_block = after_block;
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        // Handle throw - store exception value and jump to catch block
        HirStmt::Throw(expr) => {
            let exc_val = lower_expr(lir, func, ctx, expr)?;
            let throw_result = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(throw_result, "__throw".to_string(), vec![exc_val]),
            );

            // If we're inside a try block, jump to catch block immediately
            if let Some(catch_target) = ctx.catch_block {
                // Emit defers from current scope before jumping to catch
                let target_depth = ctx.loop_defer_depths.last().copied().unwrap_or(0);
                emit_defers_until_depth(lir, func, ctx, target_depth, loc, drop_plan)?;
                emit_drops_for_location(func, ctx, drop_plan, loc);
                func.push_to_block(ctx.current_block, LirInst::Jump(catch_target));
            } else {
                // No try/catch - emit ALL defers then return null to exit the function
                emit_all_defers(lir, func, ctx, loc, drop_plan)?;
                emit_drops_for_location(func, ctx, drop_plan, loc);
                func.push_to_block(ctx.current_block, LirInst::Return(None));
            }
            Ok(())
        }

        HirStmt::Break => {
            if let Some(exit_block) = ctx.loop_exit {
                // Emit defers from all scopes up to the loop body scope (compile-time inlining)
                let target_depth = ctx.loop_defer_depths.last().copied().unwrap_or(0);
                emit_defers_until_depth(lir, func, ctx, target_depth, loc, drop_plan)?;
                emit_drops_for_location(func, ctx, drop_plan, loc);
                func.push_to_block(ctx.current_block, LirInst::Jump(exit_block));
            }
            Ok(())
        }

        HirStmt::Continue => {
            // Emit defers from all scopes up to the loop body scope (compile-time inlining)
            let target_depth = ctx.loop_defer_depths.last().copied().unwrap_or(0);
            emit_defers_until_depth(lir, func, ctx, target_depth, loc, drop_plan)?;
            emit_drops_for_location(func, ctx, drop_plan, loc);
            // Jump to loop condition block to re-evaluate the loop
            if let Some(cond_block) = ctx.loop_cond {
                func.push_to_block(ctx.current_block, LirInst::Jump(cond_block));
            }
            Ok(())
        }

        // Handle extend - add methods to an existing class
        HirStmt::Extend { target, methods } => {
            // For each method in the extension, we register it as a method on the target class
            for method in methods {
                // Lower the method body to a separate function
                let method_drop_plan =
                    crate::parsing::drop_insertion::DropPlanner::new().plan(method);
                let method_func = lower_hir_function(lir, method, Some(&method_drop_plan))?;
                lir.add_function(method_func);

                // Register the method with the class using __extend_class builtin
                let class_name_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::ConstString(class_name_val, target.clone()),
                );
                let method_name_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::ConstString(method_name_val, method.name.clone()),
                );
                let result = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(
                        result,
                        "__extend_class".to_string(),
                        vec![class_name_val, method_name_val],
                    ),
                );
            }
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        // Handle imports - load module and bind to alias
        HirStmt::Import { path, alias } => {
            // Register import in module for resolution
            lir.imports.insert(alias.clone(), path.clone());

            // Generate LoadModule instruction to create namespace object
            let module_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::LoadModule(module_val, alias.clone()),
            );
            func.push_to_block(
                ctx.current_block,
                LirInst::StoreVar(alias.clone(), module_val),
            );
            func.set_var(alias.clone(), module_val);
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        HirStmt::ImportDefault { path, alias } => {
            // For default imports, load module and extract default export
            lir.imports.insert(alias.clone(), path.clone());

            let module_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::LoadModule(module_val, alias.clone()),
            );

            // Get the default export
            let default_key = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(default_key, "__default".to_string()),
            );
            let default_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(
                    default_val,
                    "get_field".to_string(),
                    vec![module_val, default_key],
                ),
            );

            func.push_to_block(
                ctx.current_block,
                LirInst::StoreVar(alias.clone(), default_val),
            );
            func.set_var(alias.clone(), default_val);
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        HirStmt::ImportNames { path, names } => {
            // For named imports, load module and extract each named export
            let temp_alias = format!("__import_{}", path.replace(['/', '\\', '.'], "_"));
            lir.imports.insert(temp_alias.clone(), path.clone());

            let module_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::LoadModule(module_val, temp_alias.clone()),
            );

            // Extract each named export
            for name in names {
                let name_key = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::ConstString(name_key, name.clone()),
                );
                let export_val = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(
                        export_val,
                        "get_field".to_string(),
                        vec![module_val, name_key],
                    ),
                );

                func.push_to_block(
                    ctx.current_block,
                    LirInst::StoreVar(name.clone(), export_val),
                );
                func.set_var(name.clone(), export_val);
            }
            emit_drops_for_location(func, ctx, drop_plan, loc);
            Ok(())
        }

        // Handle region blocks (arena-based allocation)
        HirStmt::Region { name, body } => {
            // Begin region
            let name_val = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(name_val, name.clone()),
            );
            let begin_res = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(begin_res, "region_begin".to_string(), vec![name_val]),
            );

            // Lower statements in region
            let region_loc = format!("{loc}.region");
            lower_stmt(lir, func, ctx, body, &region_loc, drop_plan)?;

            // End region - frees all region allocations (skip if block terminated by return/throw)
            if !is_block_terminated(func, ctx.current_block) {
                let end_res = func.alloc_value();
                func.push_to_block(
                    ctx.current_block,
                    LirInst::CallBuiltin(end_res, "region_end".to_string(), vec![name_val]),
                );
                emit_drops_for_location(func, ctx, drop_plan, loc);
            }
            Ok(())
        }

        // Handle defer blocks - register for compile-time inlining at scope exit
        HirStmt::Defer(block) => {
            // Register the defer block in the current scope's defer list.
            // At scope exit (normal, return, break, continue), the defer blocks
            // are inlined in LIFO order — zero runtime overhead, no heap allocation.
            if let Some(scope) = ctx.defer_scopes.last_mut() {
                scope.push(block.clone());
            }
            Ok(())
        }

        // Handle unsafe blocks (opt-out of borrow checking)
        HirStmt::Unsafe(stmt) => {
            // Enter unsafe context for the duration of the nested statement
            let prev = ctx.in_unsafe_context;
            ctx.in_unsafe_context = true;
            let unsafe_loc = format!("{loc}.unsafe");
            let res = lower_stmt(lir, func, ctx, stmt, &unsafe_loc, drop_plan);
            ctx.in_unsafe_context = prev;
            if res.is_ok() {
                emit_drops_for_location(func, ctx, drop_plan, loc);
            }
            res
        }
    }
}
