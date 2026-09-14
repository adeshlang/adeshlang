//! HIR to MIR Lowering
//!
//! Lowers HIR (High-level IR) to MIR with full memory safety analysis.

use super::arc_insertion::ArcInsertion;
use super::drop_insertion::DropInsertion;
use super::{BlockId, LocalId};
use super::{BorrowAnalysis, LifetimeInference};
use super::{MirBinOp, MirConstant, MirOperand, MirPlace, MirRvalue, MirType, MirUnOp};
use super::{MirBlock, MirFunction, MirModule, MirStatement, MirTerminator};
use super::{MirLocal, MirParam, MirTypeDef, MirTypeDefKind, OwnershipKind, RefKind};
use crate::ir::hir::{
    BinOp, HirClass, HirExpr, HirFunction, HirLiteral, HirModule, HirStmt, HirType, UnaryOp,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// Lowering context for tracking variables and blocks
struct LoweringContext {
    /// Variable name to local ID mapping
    variables: HashMap<String, LocalId>,
    /// Next local ID
    next_local: LocalId,
    /// Next block ID
    next_block: BlockId,
    /// Current function being lowered
    current_function: Option<String>,
    /// Current block being populated (for control flow)
    current_block_id: BlockId,
    /// Stack of defer scopes for compile-time inlining (zero runtime overhead).
    /// Each scope is a list of defer blocks. At scope exit, defers are inlined
    /// in LIFO order — no runtime defer stack, no heap allocation.
    defer_scopes: Vec<Vec<Box<HirStmt>>>,
}

impl LoweringContext {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            next_local: 0,
            next_block: 0,
            current_function: None,
            current_block_id: 0,
            defer_scopes: Vec::new(),
        }
    }

    fn next_local(&mut self) -> LocalId {
        let id = self.next_local;
        self.next_local += 1;
        id
    }

    fn next_block(&mut self) -> BlockId {
        let id = self.next_block;
        self.next_block += 1;
        id
    }

    fn declare_var(&mut self, name: String) -> LocalId {
        let id = self.next_local();
        self.variables.insert(name, id);
        id
    }

    fn get_var(&self, name: &str) -> Option<LocalId> {
        self.variables.get(name).copied()
    }
}

/// Lower HIR module to MIR
pub fn lower_hir_to_mir(hir: &HirModule) -> Result<MirModule, String> {
    let mut mir = MirModule::new("module".to_string());
    let ctx = RefCell::new(LoweringContext::new());

    // Phase 1: Initial lowering (HIR → MIR structure)

    // Check if user defined a 'main' function
    let has_user_main = hir.functions.iter().any(|f| f.name == "main");

    // Lower functions (rename user's main to __user_main)
    for hir_func in &hir.functions {
        let mut func_clone = hir_func.clone();
        if func_clone.name == "main" {
            func_clone.name = "__user_main".to_string();
        }
        let mir_func = lower_function(&func_clone, &ctx)?;
        mir.functions.push(mir_func);
    }

    // Lower classes (convert methods to functions)
    for hir_class in &hir.classes {
        // Create type definition for the class
        let struct_fields = extract_class_fields(hir_class);
        mir.types.push(MirTypeDef {
            name: hir_class.name.clone(),
            kind: MirTypeDefKind::Struct {
                fields: struct_fields,
            },
        });

        // Lower each method as a function
        for method in &hir_class.methods {
            let method_func = HirFunction {
                name: format!("{}::{}", hir_class.name, method.name),
                params: method
                    .params
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.clone(), None))
                    .collect(),
                body: method.body.clone(),
                ret_type: method.ret_type.clone(),
                is_async: method.is_async,
                decorators: Vec::new(),
                is_exported: false,
                move_params: Vec::new(),
                is_test: false,
                test_ignore: false,
                test_expect_fail: false,
                test_timeout: None,
                is_unsafe: method.is_unsafe,
            };
            let mir_func = lower_function(&method_func, &ctx)?;
            mir.functions.push(mir_func);
        }
    }

    // Lower module-level statements (convert to __top_level_wrapper function)
    let has_content = !hir.statements.is_empty() || !hir.classes.is_empty();
    if has_content {
        let wrapper_func = create_top_level_wrapper(&hir.statements, &ctx)?;
        mir.functions.push(wrapper_func);
    }

    // Create main entry point that orchestrates execution
    // Now that MirStatement::Call is properly implemented, we can create the main entry
    if has_user_main || has_content {
        let main_func = create_main_entry(has_user_main, has_content, &ctx)?;
        mir.functions.push(main_func);
    }

    // Phase 2: Borrow analysis
    let _borrow_analysis = BorrowAnalysis::analyze_module(&mir)?;

    // Phase 3: Lifetime inference
    let _lifetime_inference = LifetimeInference::infer_module(&mir)?;

    // Phase 4: Drop insertion
    DropInsertion::insert_drops(&mut mir)?;

    // Phase 5: ARC insertion
    ArcInsertion::insert_arc_ops(&mut mir)?;

    // Phase 6: Validate ownership graph
    mir.ownership.validate()?;

    Ok(mir)
}

/// Lower HIR function to MIR function
fn lower_function(
    hir_func: &HirFunction,
    ctx: &RefCell<LoweringContext>,
) -> Result<MirFunction, String> {
    ctx.borrow_mut().current_function = Some(hir_func.name.clone());
    ctx.borrow_mut().variables.clear();
    ctx.borrow_mut().next_local = 0;
    ctx.borrow_mut().next_block = 0;

    let mut mir_func = MirFunction {
        name: hir_func.name.clone(),
        params: Vec::new(),
        return_type: hir_func
            .ret_type
            .as_ref()
            .map(|ty| lower_type(ty))
            .unwrap_or(MirType::Unit),
        body: Vec::new(),
        locals: Vec::new(),
        captures: Vec::new(),
        is_async: hir_func.is_async,
        ownership: super::OwnershipGraph::new(),
    };

    if let Some((class_name, _method_name)) = hir_func.name.split_once("::") {
        let receiver_ty = MirType::Struct(class_name.to_string());
        for receiver_name in ["self", "this"] {
            let _local_id = ctx.borrow_mut().declare_var(receiver_name.to_string());
            mir_func.params.push(MirParam {
                name: receiver_name.to_string(),
                ty: receiver_ty.clone(),
                ownership: OwnershipKind::Borrowed,
            });
            mir_func.locals.push(MirLocal {
                name: Some(receiver_name.to_string()),
                ty: receiver_ty.clone(),
                ownership: OwnershipKind::Borrowed,
            });
        }
    }

    // Lower parameters
    for (idx, (name, ty_opt, _default)) in hir_func.params.iter().enumerate() {
        let ty = ty_opt
            .as_ref()
            .map(|t| lower_type(t))
            .unwrap_or(MirType::I64); // Default to i64

        let ownership = if hir_func.move_params.contains(&idx) {
            OwnershipKind::Owned
        } else {
            OwnershipKind::Borrowed
        };

        let _local_id = ctx.borrow_mut().declare_var(name.clone());

        mir_func.params.push(MirParam {
            name: name.clone(),
            ty: ty.clone(),
            ownership,
        });

        mir_func.locals.push(MirLocal {
            name: Some(name.clone()),
            ty,
            ownership,
        });
    }

    // Create entry block
    let entry_block_id = ctx.borrow_mut().next_block();
    ctx.borrow_mut().current_block_id = entry_block_id;

    let entry_block = MirBlock {
        id: entry_block_id,
        statements: Vec::new(),
        terminator: MirTerminator::Return(None),
    };
    mir_func.body.push(entry_block);

    // Initialize defer scope for function body
    ctx.borrow_mut().defer_scopes.clear();
    ctx.borrow_mut().defer_scopes.push(Vec::new());

    // Lower function body
    for stmt in hir_func.body.iter() {
        lower_statement_cfg(stmt, &mut mir_func, ctx)?;
    }

    // Emit all remaining defers in LIFO order before function exit (compile-time inlining)
    let all_defers: Vec<Box<HirStmt>> = ctx
        .borrow_mut()
        .defer_scopes
        .iter_mut()
        .rev()
        .flat_map(|s| s.drain(..))
        .collect();
    ctx.borrow_mut().defer_scopes.clear();
    for defer_block in all_defers.iter().rev() {
        lower_statement_cfg(defer_block, &mut mir_func, ctx)?;
    }

    // Ensure the final block has a proper terminator
    let current_id = ctx.borrow().current_block_id;
    if let Some(block) = mir_func.body.iter_mut().find(|b| b.id == current_id) {
        if matches!(block.terminator, MirTerminator::Return(None)) {
            // No explicit return, use default
            block.terminator = MirTerminator::Return(None);
        }
    }

    Ok(mir_func)
}

/// Create __top_level_wrapper function from module statements
fn create_top_level_wrapper(
    statements: &[HirStmt],
    ctx: &RefCell<LoweringContext>,
) -> Result<MirFunction, String> {
    ctx.borrow_mut().current_function = Some("__top_level_wrapper".to_string());
    ctx.borrow_mut().variables.clear();
    ctx.borrow_mut().next_local = 0;
    ctx.borrow_mut().next_block = 0;

    let mut mir_func = MirFunction {
        name: "__top_level_wrapper".to_string(),
        params: Vec::new(),
        return_type: MirType::Unit,
        body: Vec::new(),
        locals: Vec::new(),
        captures: Vec::new(),
        is_async: false,
        ownership: super::OwnershipGraph::new(),
    };

    // Create entry block and use CFG-aware lowering so top-level control-flow
    // constructs (if/elif/while/etc.) are handled the same as in functions.
    let entry_block_id = ctx.borrow_mut().next_block();
    ctx.borrow_mut().current_block_id = entry_block_id;
    let entry_block = MirBlock {
        id: entry_block_id,
        statements: Vec::new(),
        terminator: MirTerminator::Return(None),
    };
    mir_func.body.push(entry_block);

    // Initialize defer scope for top-level statements
    ctx.borrow_mut().defer_scopes.clear();
    ctx.borrow_mut().defer_scopes.push(Vec::new());

    // Lower each top-level statement using CFG lowering so `if/elif/else` is
    // desugared into proper MIR blocks and backends that require CFG lowering
    // (JIT, VM, AOT) can compile module-level conditionals.
    for stmt in statements {
        lower_statement_cfg(stmt, &mut mir_func, ctx)?;
    }

    // Emit all remaining defers in LIFO order before exit (compile-time inlining)
    let all_defers: Vec<Box<HirStmt>> = ctx
        .borrow_mut()
        .defer_scopes
        .iter_mut()
        .rev()
        .flat_map(|s| s.drain(..))
        .collect();
    ctx.borrow_mut().defer_scopes.clear();
    for defer_block in all_defers.iter().rev() {
        lower_statement_cfg(defer_block, &mut mir_func, ctx)?;
    }

    Ok(mir_func)
}

/// Create main entry point that calls __user_main and/or __top_level_wrapper  
fn create_main_entry(
    has_user_main: bool,
    has_wrapper: bool,
    ctx: &RefCell<LoweringContext>,
) -> Result<MirFunction, String> {
    ctx.borrow_mut().current_function = Some("main".to_string());
    ctx.borrow_mut().variables.clear();
    ctx.borrow_mut().next_local = 0;
    ctx.borrow_mut().next_block = 0;

    let mut mir_func = MirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        return_type: MirType::Unit,
        body: Vec::new(),
        locals: Vec::new(),
        captures: Vec::new(),
        is_async: false,
        ownership: super::OwnershipGraph::new(),
    };

    let entry_block_id = ctx.borrow_mut().next_block();
    let mut entry_block = MirBlock {
        id: entry_block_id,
        statements: Vec::new(),
        terminator: MirTerminator::Return(None),
    };

    // Create a temporary local to hold call results (for statements that don't need the result)
    let temp_local = ctx.borrow_mut().next_local;
    ctx.borrow_mut().next_local += 1;
    mir_func.locals.push(MirLocal {
        name: Some("_call_result".to_string()),
        ty: MirType::Unit,
        ownership: OwnershipKind::Owned,
    });

    // Call __top_level_wrapper first (global init — runs before main)
    if has_wrapper {
        entry_block.statements.push(MirStatement::Call {
            dest: temp_local,
            func: MirOperand::Constant(MirConstant::String("__top_level_wrapper".to_string())),
            args: vec![],
        });
    }

    // Then call __user_main (if exists)
    if has_user_main {
        entry_block.statements.push(MirStatement::Call {
            dest: temp_local,
            func: MirOperand::Constant(MirConstant::String("__user_main".to_string())),
            args: vec![],
        });
    }

    mir_func.body.push(entry_block);

    Ok(mir_func)
}

/// Lower HIR statement to MIR statements
#[allow(dead_code)]
fn lower_statement(
    stmt: &HirStmt,
    block: &mut MirBlock,
    func: &mut MirFunction,
    ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    // Legacy single-block lowering - kept for simple cases
    match stmt {
        HirStmt::Let {
            name,
            ty,
            init,
            is_const: _,
            is_borrowed: _,
        } => {
            // Infer type from annotation; if absent, infer from initializer expression
            let mir_ty = ty.as_ref().map(|t| lower_type(t)).unwrap_or_else(|| {
                init.as_ref()
                    .map(|e| infer_expr_mir_type(e))
                    .unwrap_or(MirType::I64)
            });

            let local_id = ctx.borrow_mut().declare_var(name.clone());
            func.locals.push(MirLocal {
                name: Some(name.clone()),
                ty: mir_ty.clone(),
                ownership: OwnershipKind::Owned,
            });

            // Generate storage live marker
            block.statements.push(MirStatement::StorageLive(local_id));

            if let Some(init_expr) = init {
                let rvalue = lower_expr_to_rvalue(init_expr, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(local_id, rvalue));
            }
        }

        HirStmt::Assign {
            target,
            value,
            is_move: _,
        } => {
            match target {
                HirExpr::LoadVar(name) => {
                    let local_id = ctx
                        .borrow()
                        .get_var(name)
                        .ok_or_else(|| format!("Undefined variable: {}", name))?;

                    let rvalue = lower_expr_to_rvalue(value, block, func, ctx)?;
                    block
                        .statements
                        .push(MirStatement::Assign(local_id, rvalue));
                }
                HirExpr::MemberAccess(obj, field) => {
                    let set_expr =
                        HirExpr::SetMember(obj.clone(), field.clone(), Box::new(value.clone()));
                    let _ = lower_expr_to_rvalue(&set_expr, block, func, ctx)?;
                }
                HirExpr::Index(arr, idx) => {
                    let arr_temp = ctx.borrow_mut().next_local();
                    let arr_rval = lower_expr_to_rvalue(arr, block, func, ctx)?;
                    block
                        .statements
                        .push(MirStatement::Assign(arr_temp, arr_rval));

                    let idx_temp = ctx.borrow_mut().next_local();
                    let idx_rval = lower_expr_to_rvalue(idx, block, func, ctx)?;
                    block
                        .statements
                        .push(MirStatement::Assign(idx_temp, idx_rval));

                    let val_temp = ctx.borrow_mut().next_local();
                    let val_rval = lower_expr_to_rvalue(value, block, func, ctx)?;
                    block
                        .statements
                        .push(MirStatement::Assign(val_temp, val_rval));

                    let dummy = ctx.borrow_mut().next_local();
                    block.statements.push(MirStatement::Call {
                        dest: dummy,
                        func: MirOperand::Constant(MirConstant::String("set_index".to_string())),
                        args: vec![
                            MirOperand::Copy(MirPlace {
                                local: arr_temp,
                                projection: Vec::new(),
                            }),
                            MirOperand::Copy(MirPlace {
                                local: idx_temp,
                                projection: Vec::new(),
                            }),
                            MirOperand::Copy(MirPlace {
                                local: val_temp,
                                projection: Vec::new(),
                            }),
                        ],
                    });
                }
                _ => {
                    return Err(format!("Unsupported assignment target: {:?}", target));
                }
            }
        }

        HirStmt::Expr(expr) => {
            // Expression statement - evaluate for side effects
            let _ = lower_expr_to_rvalue(expr, block, func, ctx)?;
        }

        HirStmt::Return(expr_opt) => {
            let operand = if let Some(expr) = expr_opt {
                let temp = ctx.borrow_mut().next_local();
                let rvalue = lower_expr_to_rvalue(expr, block, func, ctx)?;
                block.statements.push(MirStatement::Assign(temp, rvalue));
                Some(MirOperand::Move(MirPlace {
                    local: temp,
                    projection: Vec::new(),
                }))
            } else {
                None
            };
            block.terminator = MirTerminator::Return(operand);
        }

        HirStmt::Block(stmts) => {
            for s in stmts {
                lower_statement(s, block, func, ctx)?;
            }
        }

        _ => {
            // Defer to CFG lowering for control flow
            return Err("Statement requires CFG lowering".to_string());
        }
    }

    Ok(())
}

/// Lower statement with CFG (Control Flow Graph) support
/// This version works with the function's block list to handle control flow
fn lower_statement_cfg(
    stmt: &HirStmt,
    func: &mut MirFunction,
    ctx: &RefCell<LoweringContext>,
) -> Result<(), String> {
    match stmt {
        HirStmt::Let {
            name,
            ty,
            init,
            is_const: _,
            is_borrowed: _,
        } => {
            // Infer type from annotation; if absent, infer from initializer expression
            let mir_ty = ty.as_ref().map(|t| lower_type(t)).unwrap_or_else(|| {
                init.as_ref()
                    .map(|e| infer_expr_mir_type(e))
                    .unwrap_or(MirType::I64)
            });

            let local_id = ctx.borrow_mut().declare_var(name.clone());
            func.locals.push(MirLocal {
                name: Some(name.clone()),
                ty: mir_ty.clone(),
                ownership: OwnershipKind::Owned,
            });

            // Generate storage live marker in current block
            let current_id = ctx.borrow().current_block_id;
            {
                let current_block = func
                    .body
                    .iter_mut()
                    .find(|b| b.id == current_id)
                    .ok_or("Current block not found")?;
                current_block
                    .statements
                    .push(MirStatement::StorageLive(local_id));
            }

            if let Some(init_expr) = init {
                // Use unsafe pointer to work around borrow checker
                // This is safe because we're not accessing func.body while using the block pointer
                let current_id = ctx.borrow().current_block_id;
                let block_ptr =
                    func.body
                        .iter_mut()
                        .find(|b| b.id == current_id)
                        .ok_or("Current block not found")? as *mut MirBlock;

                let rvalue =
                    unsafe { lower_expr_to_rvalue(init_expr, &mut *block_ptr, func, ctx)? };

                unsafe {
                    (*block_ptr)
                        .statements
                        .push(MirStatement::Assign(local_id, rvalue));
                }
            }
        }

        HirStmt::Assign {
            target,
            value,
            is_move: _,
        } => {
            let current_id = ctx.borrow().current_block_id;
            let block_ptr =
                func.body
                    .iter_mut()
                    .find(|b| b.id == current_id)
                    .ok_or("Current block not found")? as *mut MirBlock;

            match target {
                HirExpr::LoadVar(name) => {
                    let local_id = ctx
                        .borrow()
                        .get_var(name)
                        .ok_or_else(|| format!("Undefined variable: {}", name))?;

                    let rvalue =
                        unsafe { lower_expr_to_rvalue(value, &mut *block_ptr, func, ctx)? };

                    unsafe {
                        (*block_ptr)
                            .statements
                            .push(MirStatement::Assign(local_id, rvalue));
                    }
                }
                HirExpr::MemberAccess(obj, field) => {
                    let set_expr =
                        HirExpr::SetMember(obj.clone(), field.clone(), Box::new(value.clone()));
                    unsafe {
                        let _ = lower_expr_to_rvalue(&set_expr, &mut *block_ptr, func, ctx)?;
                    }
                }
                HirExpr::Index(arr, idx) => {
                    unsafe {
                        let b = &mut *block_ptr;
                        let arr_temp = ctx.borrow_mut().next_local();
                        let arr_rval = lower_expr_to_rvalue(arr, b, func, ctx)?;
                        b.statements
                            .push(MirStatement::Assign(arr_temp, arr_rval));

                        let idx_temp = ctx.borrow_mut().next_local();
                        let idx_rval = lower_expr_to_rvalue(idx, b, func, ctx)?;
                        b.statements
                            .push(MirStatement::Assign(idx_temp, idx_rval));

                        let val_temp = ctx.borrow_mut().next_local();
                        let val_rval = lower_expr_to_rvalue(value, b, func, ctx)?;
                        b.statements
                            .push(MirStatement::Assign(val_temp, val_rval));

                        let dummy = ctx.borrow_mut().next_local();
                        b.statements.push(MirStatement::Call {
                            dest: dummy,
                            func: MirOperand::Constant(MirConstant::String("set_index".to_string())),
                            args: vec![
                                MirOperand::Copy(MirPlace {
                                    local: arr_temp,
                                    projection: Vec::new(),
                                }),
                                MirOperand::Copy(MirPlace {
                                    local: idx_temp,
                                    projection: Vec::new(),
                                }),
                                MirOperand::Copy(MirPlace {
                                    local: val_temp,
                                    projection: Vec::new(),
                                }),
                            ],
                        });
                    }
                }
                _ => {
                    return Err(format!("Unsupported assignment target: {:?}", target));
                }
            }
        }

        HirStmt::Expr(expr) => {
            let current_id = ctx.borrow().current_block_id;
            let block_ptr = func
                .body
                .iter_mut()
                .find(|b| b.id == current_id)
                .ok_or("Current block not found")? as *mut MirBlock;

            // Expression statement - evaluate for side effects
            unsafe {
                let _ = lower_expr_to_rvalue(expr, &mut *block_ptr, func, ctx)?;
            }
        }

        HirStmt::Return(expr_opt) => {
            let operand = if let Some(expr) = expr_opt {
                let temp = ctx.borrow_mut().next_local();

                let current_id = ctx.borrow().current_block_id;
                let block_ptr =
                    func.body
                        .iter_mut()
                        .find(|b| b.id == current_id)
                        .ok_or("Current block not found")? as *mut MirBlock;

                let rvalue = unsafe { lower_expr_to_rvalue(expr, &mut *block_ptr, func, ctx)? };

                unsafe {
                    (*block_ptr)
                        .statements
                        .push(MirStatement::Assign(temp, rvalue));
                }

                Some(MirOperand::Move(MirPlace {
                    local: temp,
                    projection: Vec::new(),
                }))
            } else {
                None
            };

            // Emit ALL defers from ALL active scopes in LIFO order before returning
            let all_defers: Vec<Box<HirStmt>> = ctx
                .borrow_mut()
                .defer_scopes
                .iter_mut()
                .rev()
                .flat_map(|s| s.drain(..))
                .collect();
            ctx.borrow_mut().defer_scopes.clear();
            for defer_block in all_defers.iter().rev() {
                lower_statement_cfg(defer_block, func, ctx)?;
            }

            let current_id = ctx.borrow().current_block_id;
            let current_block = func
                .body
                .iter_mut()
                .find(|b| b.id == current_id)
                .ok_or("Current block not found")?;
            current_block.terminator = MirTerminator::Return(operand);
        }

        HirStmt::Block(stmts) => {
            // Push a new defer scope for this block
            ctx.borrow_mut().defer_scopes.push(Vec::new());
            // Recursively lower each statement in the block
            for s in stmts {
                lower_statement_cfg(s, func, ctx)?;
            }
            // Emit this scope's defers in LIFO order (compile-time inlining)
            let defers = ctx.borrow_mut().defer_scopes.pop().unwrap_or_default();
            for defer_block in defers.iter().rev() {
                lower_statement_cfg(defer_block, func, ctx)?;
            }
        }

        HirStmt::Defer(block) => {
            // Register the defer block in the current scope for compile-time inlining.
            // Zero runtime overhead — defers are inlined at scope exit, not stored on a runtime stack.
            if let Some(scope) = ctx.borrow_mut().defer_scopes.last_mut() {
                scope.push(block.clone());
            }
        }

        HirStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            // Phase 2.1: If/Else control flow lowering
            // Generate CFG structure:
            //   current_block → [condition eval] → SwitchInt
            //     ├─ then_block → statements → Goto merge_block
            //     ├─ else_block → statements → Goto merge_block
            //     └─ merge_block → continue

            let then_block_id = ctx.borrow_mut().next_block();
            let else_block_id = ctx.borrow_mut().next_block();
            let merge_block_id = ctx.borrow_mut().next_block();

            // Evaluate condition in current block
            let cond_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::Bool,
                ownership: OwnershipKind::Owned,
            });

            let current_id = ctx.borrow().current_block_id;
            let block_ptr = func
                .body
                .iter_mut()
                .find(|b| b.id == current_id)
                .ok_or("Current block not found")? as *mut MirBlock;

            let cond_rvalue = unsafe { lower_expr_to_rvalue(cond, &mut *block_ptr, func, ctx)? };

            unsafe {
                (*block_ptr)
                    .statements
                    .push(MirStatement::Assign(cond_temp, cond_rvalue));

                // Create terminator: switch on boolean (1 = then, 0 = else)
                (*block_ptr).terminator = MirTerminator::SwitchInt {
                    discriminant: MirOperand::Move(MirPlace {
                        local: cond_temp,
                        projection: Vec::new(),
                    }),
                    targets: vec![(1, then_block_id)],
                    otherwise: else_block_id,
                };
            }

            // Create then block
            let then_block = MirBlock {
                id: then_block_id,
                statements: Vec::new(),
                terminator: MirTerminator::Goto(merge_block_id),
            };
            // Switch context to then block
            ctx.borrow_mut().current_block_id = then_block_id;
            func.body.push(then_block);
            // Push defer scope for then branch
            ctx.borrow_mut().defer_scopes.push(Vec::new());
            lower_statement_cfg(then_branch, func, ctx)?;
            // Emit then branch's defers in LIFO order
            let then_defers = ctx.borrow_mut().defer_scopes.pop().unwrap_or_default();
            for defer_block in then_defers.iter().rev() {
                lower_statement_cfg(defer_block, func, ctx)?;
            }
            // Ensure then block goes to merge (unless it has return/etc)
            {
                let then_blk = func
                    .body
                    .iter_mut()
                    .find(|b| b.id == then_block_id)
                    .ok_or("Then block not found")?;
                if matches!(then_blk.terminator, MirTerminator::Return(None)) {
                    then_blk.terminator = MirTerminator::Goto(merge_block_id);
                }
            }

            // Create else block
            let else_block = MirBlock {
                id: else_block_id,
                statements: Vec::new(),
                terminator: MirTerminator::Goto(merge_block_id),
            };
            // Switch context to else block
            ctx.borrow_mut().current_block_id = else_block_id;
            func.body.push(else_block);
            if let Some(else_stmt) = else_branch {
                // Push defer scope for else branch
                ctx.borrow_mut().defer_scopes.push(Vec::new());
                lower_statement_cfg(else_stmt, func, ctx)?;
                // Emit else branch's defers in LIFO order
                let else_defers = ctx.borrow_mut().defer_scopes.pop().unwrap_or_default();
                for defer_block in else_defers.iter().rev() {
                    lower_statement_cfg(defer_block, func, ctx)?;
                }
            }
            // Ensure else block goes to merge (unless it has return/etc)
            {
                let else_blk = func
                    .body
                    .iter_mut()
                    .find(|b| b.id == else_block_id)
                    .ok_or("Else block not found")?;
                if matches!(else_blk.terminator, MirTerminator::Return(None)) {
                    else_blk.terminator = MirTerminator::Goto(merge_block_id);
                }
            }

            // Create merge block (continuation point)
            let merge_block = MirBlock {
                id: merge_block_id,
                statements: Vec::new(),
                terminator: MirTerminator::Return(None), // Placeholder, will be updated
            };
            func.body.push(merge_block);

            // Continue lowering in merge block
            ctx.borrow_mut().current_block_id = merge_block_id;
        }

        HirStmt::While { cond, body } => {
            // Phase 2.2: While loop control flow lowering
            // Generate CFG structure:
            //   current_block → Goto loop_header
            //   loop_header → [condition eval] → SwitchInt
            //     ├─ loop_body → statements → Goto loop_header
            //     └─ loop_exit → continue

            let loop_header_id = ctx.borrow_mut().next_block();
            let loop_body_id = ctx.borrow_mut().next_block();
            let loop_exit_id = ctx.borrow_mut().next_block();

            // Jump to loop header from current block
            {
                let current_id = ctx.borrow().current_block_id;
                let current_block = func
                    .body
                    .iter_mut()
                    .find(|b| b.id == current_id)
                    .ok_or("Current block not found")?;
                current_block.terminator = MirTerminator::Goto(loop_header_id);
            }

            // Create loop header (condition check)
            let cond_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::Bool,
                ownership: OwnershipKind::Owned,
            });

            let mut loop_header = MirBlock {
                id: loop_header_id,
                statements: Vec::new(),
                terminator: MirTerminator::Return(None),
            };

            let cond_rvalue = lower_expr_to_rvalue(cond, &mut loop_header, func, ctx)?;
            loop_header
                .statements
                .push(MirStatement::Assign(cond_temp, cond_rvalue));

            loop_header.terminator = MirTerminator::SwitchInt {
                discriminant: MirOperand::Move(MirPlace {
                    local: cond_temp,
                    projection: Vec::new(),
                }),
                targets: vec![(1, loop_body_id)],
                otherwise: loop_exit_id,
            };
            func.body.push(loop_header);

            // Create loop body
            let loop_body_blk = MirBlock {
                id: loop_body_id,
                statements: Vec::new(),
                terminator: MirTerminator::Goto(loop_header_id),
            };
            // Switch context to loop body
            ctx.borrow_mut().current_block_id = loop_body_id;
            func.body.push(loop_body_blk);
            // Push defer scope for loop body
            ctx.borrow_mut().defer_scopes.push(Vec::new());
            lower_statement_cfg(body, func, ctx)?;
            // Emit loop body's defers in LIFO order (compile-time inlining)
            let body_defers = ctx.borrow_mut().defer_scopes.pop().unwrap_or_default();
            for defer_block in body_defers.iter().rev() {
                lower_statement_cfg(defer_block, func, ctx)?;
            }
            // Always jump back to header after loop body
            {
                let body_blk = func
                    .body
                    .iter_mut()
                    .find(|b| b.id == loop_body_id)
                    .ok_or("Loop body block not found")?;
                if matches!(body_blk.terminator, MirTerminator::Return(None)) {
                    body_blk.terminator = MirTerminator::Goto(loop_header_id);
                }
            }

            // Create loop exit block
            let loop_exit = MirBlock {
                id: loop_exit_id,
                statements: Vec::new(),
                terminator: MirTerminator::Return(None), // Placeholder
            };
            func.body.push(loop_exit);

            // Continue lowering in loop exit block
            ctx.borrow_mut().current_block_id = loop_exit_id;
        }

        // For any unhandled statement types, insert a nop
        _ => {
            let current_id = ctx.borrow().current_block_id;
            let current_block = func
                .body
                .iter_mut()
                .find(|b| b.id == current_id)
                .ok_or("Current block not found")?;
            current_block.statements.push(MirStatement::Nop);
        }
    }

    Ok(())
}

/// Lower HIR expression to MIR rvalue
fn lower_expr_to_rvalue(
    expr: &HirExpr,
    block: &mut MirBlock,
    func: &mut MirFunction,
    ctx: &RefCell<LoweringContext>,
) -> Result<MirRvalue, String> {
    match expr {
        HirExpr::Literal(HirLiteral::Char(c)) => {
            let char_temp = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Assign(
                char_temp,
                MirRvalue::Use(MirOperand::Constant(MirConstant::String(c.to_string()))),
            ));

            let result_local = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String("char".to_string())),
                args: vec![MirOperand::Copy(MirPlace {
                    local: char_temp,
                    projection: Vec::new(),
                })],
            });

            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::Literal(lit) => Ok(MirRvalue::Use(MirOperand::Constant(lower_literal(lit)))),

        HirExpr::LoadVar(name) => {
            let local = ctx
                .borrow()
                .get_var(name)
                .ok_or_else(|| format!("Undefined variable: {}", name))?;
            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local,
                projection: Vec::new(),
            })))
        }

        HirExpr::BinaryOp(lhs, op, rhs) => {
            let lhs_ty = infer_expr_mir_type(lhs);
            let rhs_ty = infer_expr_mir_type(rhs);

            let lhs_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: lhs_ty.clone(),
                ownership: OwnershipKind::Owned,
            });
            let lhs_rvalue = lower_expr_to_rvalue(lhs, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(lhs_temp, lhs_rvalue));

            let rhs_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: rhs_ty.clone(),
                ownership: OwnershipKind::Owned,
            });
            let rhs_rvalue = lower_expr_to_rvalue(rhs, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(rhs_temp, rhs_rvalue));

            let mir_op = lower_binop(*op);
            Ok(MirRvalue::BinaryOp(
                mir_op,
                MirOperand::Copy(MirPlace {
                    local: lhs_temp,
                    projection: Vec::new(),
                }),
                MirOperand::Copy(MirPlace {
                    local: rhs_temp,
                    projection: Vec::new(),
                }),
            ))
        }

        HirExpr::UnaryOp(op, operand) => {
            let operand_ty = infer_expr_mir_type(operand);
            let operand_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: operand_ty,
                ownership: OwnershipKind::Owned,
            });
            let operand_rvalue = lower_expr_to_rvalue(operand, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(operand_temp, operand_rvalue));

            let mir_op = lower_unaryop(*op);
            Ok(MirRvalue::UnaryOp(
                mir_op,
                MirOperand::Copy(MirPlace {
                    local: operand_temp,
                    projection: Vec::new(),
                }),
            ))
        }

        HirExpr::Cast(inner, target_ty) => {
            let inner_ty = infer_expr_mir_type(inner);
            let inner_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: inner_ty,
                ownership: OwnershipKind::Owned,
            });
            let inner_rvalue = lower_expr_to_rvalue(inner, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(inner_temp, inner_rvalue));

            let mir_target_ty = lower_type(target_ty);
            Ok(MirRvalue::Cast(
                MirOperand::Copy(MirPlace {
                    local: inner_temp,
                    projection: Vec::new(),
                }),
                mir_target_ty,
            ))
        }

        HirExpr::Call(func_expr, args, _type_args) => {
            // Extract function name from the expression
            let func_name = match &**func_expr {
                HirExpr::LoadVar(name) => name.clone(),
                _ => {
                    // For complex function expressions (e.g., method calls), use a placeholder
                    return Ok(MirRvalue::Use(MirOperand::Constant(MirConstant::Int(0))));
                }
            };

            // Lower arguments to operands
            let mut arg_operands = Vec::new();
            for arg in args {
                let arg_ty = infer_expr_mir_type(arg);
                let arg_temp = ctx.borrow_mut().next_local();
                // Push a typed MirLocal so operand_type() can look it up by index
                func.locals.push(MirLocal {
                    name: None,
                    ty: arg_ty,
                    ownership: OwnershipKind::Owned,
                });
                let arg_rvalue = lower_expr_to_rvalue(arg, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(arg_temp, arg_rvalue));
                arg_operands.push(MirOperand::Copy(MirPlace {
                    local: arg_temp,
                    projection: Vec::new(),
                }));
            }

            // Create local for result with proper return type
            let result_ty = builtin_return_mir_type(&func_name);
            let result_local = ctx.borrow_mut().next_local();
            // Push a typed MirLocal so operand_type() can look it up by index
            func.locals.push(MirLocal {
                name: None,
                ty: result_ty,
                ownership: OwnershipKind::Owned,
            });

            // Add call statement to block
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String(func_name)),
                args: arg_operands,
            });

            // Return reference to result
            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::MethodCall(obj, method, args) => {
            if method == "mock" {
                let is_ns = match obj.as_ref() {
                    HirExpr::LoadVar(v) => v == "input" || v == "Input",
                    _ => false,
                };
                let mut arg_operands = Vec::new();
                for arg in args {
                    let arg_ty = infer_expr_mir_type(arg);
                    let arg_temp = ctx.borrow_mut().next_local();
                    func.locals.push(MirLocal {
                        name: None,
                        ty: arg_ty,
                        ownership: OwnershipKind::Owned,
                    });
                    let arg_rvalue = lower_expr_to_rvalue(arg, block, func, ctx)?;
                    block
                        .statements
                        .push(MirStatement::Assign(arg_temp, arg_rvalue));
                    arg_operands.push(MirOperand::Copy(MirPlace {
                        local: arg_temp,
                        projection: Vec::new(),
                    }));
                }

                let dummy_local = ctx.borrow_mut().next_local();
                func.locals.push(MirLocal {
                    name: None,
                    ty: MirType::Tuple(Vec::new()),
                    ownership: OwnershipKind::Owned,
                });
                block.statements.push(MirStatement::Call {
                    dest: dummy_local,
                    func: MirOperand::Constant(MirConstant::String("input.mock".to_string())),
                    args: arg_operands,
                });
                if is_ns {
                    return Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                        local: dummy_local,
                        projection: Vec::new(),
                    })));
                } else {
                    return lower_expr_to_rvalue(obj, block, func, ctx);
                }
            }

            if let Some(builtin_name) = get_method_call_builtin_name(obj, method) {
                // Compile as a namespace builtin call (like fs.read or Regex.new)
                let mut arg_operands = Vec::new();
                for arg in args {
                    let arg_ty = infer_expr_mir_type(arg);
                    let arg_temp = ctx.borrow_mut().next_local();
                    func.locals.push(MirLocal {
                        name: None,
                        ty: arg_ty,
                        ownership: OwnershipKind::Owned,
                    });
                    let arg_rvalue = lower_expr_to_rvalue(arg, block, func, ctx)?;
                    block
                        .statements
                        .push(MirStatement::Assign(arg_temp, arg_rvalue));
                    arg_operands.push(MirOperand::Copy(MirPlace {
                        local: arg_temp,
                        projection: Vec::new(),
                    }));
                }

                let result_ty = builtin_return_mir_type(&builtin_name);
                let result_local = ctx.borrow_mut().next_local();
                func.locals.push(MirLocal {
                    name: None,
                    ty: result_ty,
                    ownership: OwnershipKind::Owned,
                });

                block.statements.push(MirStatement::Call {
                    dest: result_local,
                    func: MirOperand::Constant(MirConstant::String(builtin_name)),
                    args: arg_operands,
                });

                Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                    local: result_local,
                    projection: Vec::new(),
                })))
            } else {
                // Compile as a regular dynamic method call: __call_method(obj, method, args...)
                let obj_ty = infer_expr_mir_type(obj);
                let obj_temp = ctx.borrow_mut().next_local();
                func.locals.push(MirLocal {
                    name: None,
                    ty: obj_ty,
                    ownership: OwnershipKind::Owned,
                });
                let obj_rvalue = lower_expr_to_rvalue(obj, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(obj_temp, obj_rvalue));

                let method_temp = ctx.borrow_mut().next_local();
                func.locals.push(MirLocal {
                    name: None,
                    ty: MirType::String,
                    ownership: OwnershipKind::Owned,
                });
                block.statements.push(MirStatement::Assign(
                    method_temp,
                    MirRvalue::Use(MirOperand::Constant(MirConstant::String(method.clone()))),
                ));

                let mut arg_operands = Vec::new();
                arg_operands.push(MirOperand::Copy(MirPlace {
                    local: obj_temp,
                    projection: Vec::new(),
                }));
                arg_operands.push(MirOperand::Copy(MirPlace {
                    local: method_temp,
                    projection: Vec::new(),
                }));

                for arg in args {
                    let arg_ty = infer_expr_mir_type(arg);
                    let arg_temp = ctx.borrow_mut().next_local();
                    func.locals.push(MirLocal {
                        name: None,
                        ty: arg_ty,
                        ownership: OwnershipKind::Owned,
                    });
                    let arg_rvalue = lower_expr_to_rvalue(arg, block, func, ctx)?;
                    block
                        .statements
                        .push(MirStatement::Assign(arg_temp, arg_rvalue));
                    arg_operands.push(MirOperand::Copy(MirPlace {
                        local: arg_temp,
                        projection: Vec::new(),
                    }));
                }

                let result_local = ctx.borrow_mut().next_local();
                func.locals.push(MirLocal {
                    name: None,
                    ty: MirType::I64,
                    ownership: OwnershipKind::Owned,
                });

                block.statements.push(MirStatement::Call {
                    dest: result_local,
                    func: MirOperand::Constant(MirConstant::String("__call_method".to_string())),
                    args: arg_operands,
                });

                Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                    local: result_local,
                    projection: Vec::new(),
                })))
            }
        }

        HirExpr::MemberAccess(obj_expr, field) => {
            if let HirExpr::LoadVar(name) = obj_expr.as_ref() {
                let name = name.strip_prefix("std:").unwrap_or(name);
                if name == "Regex" || name == "fs" || name == "Math" {
                    let builtin_name = format!("{}.{}", name, field);
                    let result_local = ctx.borrow_mut().next_local();
                    func.locals.push(MirLocal {
                        name: None,
                        ty: MirType::I64,
                        ownership: OwnershipKind::Owned,
                    });
                    block.statements.push(MirStatement::Call {
                        dest: result_local,
                        func: MirOperand::Constant(MirConstant::String(builtin_name)),
                        args: vec![],
                    });
                    return Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                        local: result_local,
                        projection: Vec::new(),
                    })));
                }
            }

            let obj_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::I64,
                ownership: OwnershipKind::Owned,
            });
            let obj_rvalue = lower_expr_to_rvalue(obj_expr, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(obj_temp, obj_rvalue));

            let field_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::String,
                ownership: OwnershipKind::Owned,
            });
            block.statements.push(MirStatement::Assign(
                field_temp,
                MirRvalue::Use(MirOperand::Constant(MirConstant::String(field.clone()))),
            ));

            let result_local = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::I64,
                ownership: OwnershipKind::Owned,
            });
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String("get_field".to_string())),
                args: vec![
                    MirOperand::Copy(MirPlace {
                        local: obj_temp,
                        projection: Vec::new(),
                    }),
                    MirOperand::Copy(MirPlace {
                        local: field_temp,
                        projection: Vec::new(),
                    }),
                ],
            });

            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::SetMember(obj_expr, field, value_expr) => {
            let obj_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::I64,
                ownership: OwnershipKind::Owned,
            });
            let obj_rvalue = lower_expr_to_rvalue(obj_expr, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(obj_temp, obj_rvalue));

            let field_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::String,
                ownership: OwnershipKind::Owned,
            });
            block.statements.push(MirStatement::Assign(
                field_temp,
                MirRvalue::Use(MirOperand::Constant(MirConstant::String(field.clone()))),
            ));

            let value_ty = infer_expr_mir_type(value_expr);
            let value_temp = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: value_ty,
                ownership: OwnershipKind::Owned,
            });
            let value_rvalue = lower_expr_to_rvalue(value_expr, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(value_temp, value_rvalue));

            let result_local = ctx.borrow_mut().next_local();
            func.locals.push(MirLocal {
                name: None,
                ty: MirType::I64,
                ownership: OwnershipKind::Owned,
            });
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String("set_field".to_string())),
                args: vec![
                    MirOperand::Copy(MirPlace {
                        local: obj_temp,
                        projection: Vec::new(),
                    }),
                    MirOperand::Copy(MirPlace {
                        local: field_temp,
                        projection: Vec::new(),
                    }),
                    MirOperand::Copy(MirPlace {
                        local: value_temp,
                        projection: Vec::new(),
                    }),
                ],
            });

            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::OptionalGet(obj_expr, field) => {
            let obj_temp = ctx.borrow_mut().next_local();
            let obj_rvalue = lower_expr_to_rvalue(obj_expr, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(obj_temp, obj_rvalue));

            let field_temp = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Assign(
                field_temp,
                MirRvalue::Use(MirOperand::Constant(MirConstant::String(field.clone()))),
            ));

            let result_local = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String("optional_get".to_string())),
                args: vec![
                    MirOperand::Copy(MirPlace {
                        local: obj_temp,
                        projection: Vec::new(),
                    }),
                    MirOperand::Copy(MirPlace {
                        local: field_temp,
                        projection: Vec::new(),
                    }),
                ],
            });

            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::ObjectLiteral(fields) => {
            // Lower object literal to aggregate
            let mut keys = Vec::new();
            let mut value_operands = Vec::new();

            for (key, value_expr) in fields {
                keys.push(key.clone());

                // Lower each value expression
                let value_temp = ctx.borrow_mut().next_local();
                let value_rvalue = lower_expr_to_rvalue(value_expr, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(value_temp, value_rvalue));
                value_operands.push(MirOperand::Copy(MirPlace {
                    local: value_temp,
                    projection: Vec::new(),
                }));
            }

            Ok(MirRvalue::Aggregate(
                super::AggregateKind::Object(keys),
                value_operands,
            ))
        }

        HirExpr::ArrayLiteral(elements) => {
            // Lower array literal to aggregate
            let mut element_operands = Vec::new();

            for elem_expr in elements {
                let elem_temp = ctx.borrow_mut().next_local();
                let elem_rvalue = lower_expr_to_rvalue(elem_expr, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(elem_temp, elem_rvalue));
                element_operands.push(MirOperand::Copy(MirPlace {
                    local: elem_temp,
                    projection: Vec::new(),
                }));
            }

            // Use dynamic type for now (could be inferred)
            Ok(MirRvalue::Aggregate(
                super::AggregateKind::Array(MirType::I64, elements.len()),
                element_operands,
            ))
        }

        HirExpr::TupleLiteral(elements) => {
            // Lower tuple literal to aggregate
            let mut element_operands = Vec::new();

            for elem_expr in elements {
                let elem_temp = ctx.borrow_mut().next_local();
                let elem_rvalue = lower_expr_to_rvalue(elem_expr, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(elem_temp, elem_rvalue));
                element_operands.push(MirOperand::Copy(MirPlace {
                    local: elem_temp,
                    projection: Vec::new(),
                }));
            }

            Ok(MirRvalue::Aggregate(
                super::AggregateKind::Tuple,
                element_operands,
            ))
        }

        HirExpr::StructLiteral(name, fields) => {
            // Lower struct literal to aggregate
            let mut field_names = Vec::new();
            let mut value_operands = Vec::new();

            for (key, value_expr) in fields {
                field_names.push(key.clone());
                let value_temp = ctx.borrow_mut().next_local();
                let value_rvalue = lower_expr_to_rvalue(value_expr, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(value_temp, value_rvalue));
                value_operands.push(MirOperand::Copy(MirPlace {
                    local: value_temp,
                    projection: Vec::new(),
                }));
            }

            Ok(MirRvalue::Aggregate(
                super::AggregateKind::Struct(name.clone(), field_names),
                value_operands,
            ))
        }

        HirExpr::DictLiteral(pairs) => {
            let mut keys = Vec::new();
            let mut value_operands = Vec::new();
            for (key_expr, val_expr) in pairs {
                let key_str = match key_expr {
                    HirExpr::Literal(HirLiteral::String(s)) => s.clone(),
                    _ => "key".to_string(),
                };
                keys.push(key_str);
                let val_temp = ctx.borrow_mut().next_local();
                let val_rvalue = lower_expr_to_rvalue(val_expr, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(val_temp, val_rvalue));
                value_operands.push(MirOperand::Copy(MirPlace {
                    local: val_temp,
                    projection: Vec::new(),
                }));
            }
            Ok(MirRvalue::Aggregate(
                super::AggregateKind::Object(keys),
                value_operands,
            ))
        }

        HirExpr::SetLiteral(elements) => {
            let mut element_operands = Vec::new();
            for elem_expr in elements {
                let elem_temp = ctx.borrow_mut().next_local();
                let elem_rvalue = lower_expr_to_rvalue(elem_expr, block, func, ctx)?;
                block
                    .statements
                    .push(MirStatement::Assign(elem_temp, elem_rvalue));
                element_operands.push(MirOperand::Copy(MirPlace {
                    local: elem_temp,
                    projection: Vec::new(),
                }));
            }
            Ok(MirRvalue::Aggregate(
                super::AggregateKind::Array(MirType::I64, elements.len()),
                element_operands,
            ))
        }

        HirExpr::This | HirExpr::Super => {
            let local = ctx.borrow().get_var("this").unwrap_or(0);
            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local,
                projection: Vec::new(),
            })))
        }

        HirExpr::NonNull(inner) | HirExpr::Spread(inner) | HirExpr::Await(inner) => {
            lower_expr_to_rvalue(inner, block, func, ctx)
        }

        HirExpr::Spawn(inner) => {
            let inner_temp = ctx.borrow_mut().next_local();
            let inner_rvalue = lower_expr_to_rvalue(inner, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(inner_temp, inner_rvalue));
            let result_local = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String("thread.spawn".to_string())),
                args: vec![MirOperand::Copy(MirPlace {
                    local: inner_temp,
                    projection: Vec::new(),
                })],
            });
            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::Range(start, end, inclusive) => {
            let start_temp = ctx.borrow_mut().next_local();
            let start_rvalue = lower_expr_to_rvalue(start, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(start_temp, start_rvalue));

            let end_temp = ctx.borrow_mut().next_local();
            let end_rvalue = lower_expr_to_rvalue(end, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(end_temp, end_rvalue));

            let inc_temp = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Assign(
                inc_temp,
                MirRvalue::Use(MirOperand::Constant(MirConstant::Bool(*inclusive))),
            ));

            let result_local = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String("std:range".to_string())),
                args: vec![
                    MirOperand::Copy(MirPlace {
                        local: start_temp,
                        projection: Vec::new(),
                    }),
                    MirOperand::Copy(MirPlace {
                        local: end_temp,
                        projection: Vec::new(),
                    }),
                    MirOperand::Copy(MirPlace {
                        local: inc_temp,
                        projection: Vec::new(),
                    }),
                ],
            });
            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::Format(expr, _spec) => {
            let expr_temp = ctx.borrow_mut().next_local();
            let expr_rvalue = lower_expr_to_rvalue(expr, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(expr_temp, expr_rvalue));

            let result_local = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Call {
                dest: result_local,
                func: MirOperand::Constant(MirConstant::String("format".to_string())),
                args: vec![MirOperand::Copy(MirPlace {
                    local: expr_temp,
                    projection: Vec::new(),
                })],
            });
            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: result_local,
                projection: Vec::new(),
            })))
        }

        HirExpr::Update(target, is_increment, is_prefix) => {
            let target_temp = ctx.borrow_mut().next_local();
            let target_rvalue = lower_expr_to_rvalue(target, block, func, ctx)?;
            block
                .statements
                .push(MirStatement::Assign(target_temp, target_rvalue));

            let one_temp = ctx.borrow_mut().next_local();
            block.statements.push(MirStatement::Assign(
                one_temp,
                MirRvalue::Use(MirOperand::Constant(MirConstant::Int(1))),
            ));

            let new_temp = ctx.borrow_mut().next_local();
            let op = if *is_increment {
                MirBinOp::Add
            } else {
                MirBinOp::Sub
            };
            block.statements.push(MirStatement::Assign(
                new_temp,
                MirRvalue::BinaryOp(
                    op,
                    MirOperand::Copy(MirPlace {
                        local: target_temp,
                        projection: Vec::new(),
                    }),
                    MirOperand::Copy(MirPlace {
                        local: one_temp,
                        projection: Vec::new(),
                    }),
                ),
            ));

            if let HirExpr::LoadVar(name) = target.as_ref() {
                if let Some(var_local) = ctx.borrow().get_var(name) {
                    block.statements.push(MirStatement::Assign(
                        var_local,
                        MirRvalue::Use(MirOperand::Copy(MirPlace {
                            local: new_temp,
                            projection: Vec::new(),
                        })),
                    ));
                }
            }

            let ret_local = if *is_prefix {
                new_temp
            } else {
                target_temp
            };
            Ok(MirRvalue::Use(MirOperand::Copy(MirPlace {
                local: ret_local,
                projection: Vec::new(),
            })))
        }

        _ => {
            // Placeholder for any other expression
            Ok(MirRvalue::Use(MirOperand::Constant(MirConstant::Int(0))))
        }
    }
}

/// Return the MIR type that a known builtin function returns
fn builtin_return_mir_type(name: &str) -> MirType {
    let name = name.strip_prefix("std:").unwrap_or(name);
    match name {
        // Float-returning builtins
        "clock" | "sqrt" | "pow" | "abs" | "floor" | "ceil" | "round" | "sin" | "cos" | "tan"
        | "asin" | "acos" | "atan" | "atan2" | "exp" | "log" | "log10" | "log2" | "trunc"
        | "sign" | "Math.PI" | "Math.E" | "Math.TAU" | "Math.SQRT2" | "Math.LN2" | "Math.LN10"
        | "Math.random" | "Math.floor" | "Math.ceil" | "Math.round" | "Math.abs" | "Math.sqrt"
        | "Math.pow" | "Math.sin" | "Math.cos" | "Math.tan" | "Math.asin" | "Math.acos"
        | "Math.atan" | "Math.atan2" | "Math.exp" | "Math.log" | "Math.log10" | "Math.log2"
        | "Math.trunc" | "Math.sign" | "div" => MirType::F64,
        // Integer-returning builtins
        "len" | "int" | "argc" | "argsCount" | "int_div" | "Math.randomInt" | "sizeof" => {
            MirType::I64
        }
        // Bool-returning builtins
        "bool" | "is_null" | "eq" | "ne" | "strict_eq" | "strict_ne" | "in" | "instanceof"
        | "hasKey" | "fs.exists" | "fs.isFile" | "fs.isDir" | "fs.mkdir" | "fs.delete"
        | "fs.write" | "fs.copy" | "fs.move" | "Regex.test" | "Regex.isMatch"
        | "Regex.fullMatch" => MirType::Bool,
        // String-returning builtins
        "str" | "type" | "typeof" | "envGet" | "envFileGet" | "execName" | "substr" | "concat"
        | "format" | "fs.read" | "fs.path.basename" | "fs.path.dirname" | "fs.path.extname"
        | "fs.path.join" | "Regex.escape" | "Regex.replace" | "Regex.replaceAll" => MirType::String,
        // Default: dynamic/pointer type
        _ => MirType::I64,
    }
}

fn get_method_call_builtin_name(obj: &HirExpr, method: &str) -> Option<String> {
    match obj {
        HirExpr::LoadVar(name) => {
            let name = name.strip_prefix("std:").unwrap_or(name);
            if name == "fs" {
                return Some(format!("fs.{}", method));
            } else if name == "Regex" {
                return Some(format!("Regex.{}", method));
            }
        }
        HirExpr::MemberAccess(target, subfield) => {
            if let HirExpr::LoadVar(tname) = target.as_ref() {
                let tname = tname.strip_prefix("std:").unwrap_or(tname);
                if tname == "fs" && subfield == "path" {
                    return Some(format!("fs.path.{}", method));
                }
            }
        }
        _ => {}
    }
    None
}

/// Infer the MIR type of a HIR expression (best-effort, conservative)
fn infer_expr_mir_type(expr: &HirExpr) -> MirType {
    match expr {
        HirExpr::Literal(HirLiteral::Float(_))
        | HirExpr::Literal(HirLiteral::F64(_))
        | HirExpr::Literal(HirLiteral::F32(_)) => MirType::F64,
        HirExpr::Literal(HirLiteral::Int(_))
        | HirExpr::Literal(HirLiteral::I64(_))
        | HirExpr::Literal(HirLiteral::I32(_))
        | HirExpr::Literal(HirLiteral::I8(_))
        | HirExpr::Literal(HirLiteral::I16(_)) => MirType::I64,
        HirExpr::Literal(HirLiteral::U8(_))
        | HirExpr::Literal(HirLiteral::U16(_))
        | HirExpr::Literal(HirLiteral::U32(_))
        | HirExpr::Literal(HirLiteral::U64(_)) => MirType::U64,
        HirExpr::Literal(HirLiteral::Bool(_)) => MirType::Bool,
        HirExpr::Literal(HirLiteral::String(_)) => MirType::String,
        HirExpr::Call(func_expr, _, _) => {
            let func_name = match func_expr.as_ref() {
                HirExpr::LoadVar(name) => Some(name.clone()),
                HirExpr::MemberAccess(target, field) => {
                    if let HirExpr::LoadVar(tname) = target.as_ref() {
                        Some(format!("{}.{}", tname, field))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(name) = func_name {
                builtin_return_mir_type(&name)
            } else {
                MirType::I64
            }
        }
        HirExpr::MethodCall(obj, method, _) => {
            if let Some(builtin_name) = get_method_call_builtin_name(obj, method) {
                builtin_return_mir_type(&builtin_name)
            } else {
                MirType::I64
            }
        }
        HirExpr::BinaryOp(lhs, op, rhs) => {
            let lhs_ty = infer_expr_mir_type(lhs);
            let rhs_ty = infer_expr_mir_type(rhs);
            if matches!(op, BinOp::Add)
                && (matches!(lhs_ty, MirType::String) || matches!(rhs_ty, MirType::String))
            {
                MirType::String
            } else {
                match (lhs_ty, rhs_ty) {
                    (MirType::F64, _) | (_, MirType::F64) => match op {
                        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne => {
                            MirType::Bool
                        }
                        _ => MirType::F64,
                    },
                    _ => MirType::I64,
                }
            }
        }
        HirExpr::Cast(_, target_ty) => lower_type(target_ty),
        _ => MirType::I64,
    }
}

/// Lower HIR type to MIR type
fn lower_type(hir_type: &HirType) -> MirType {
    match hir_type {
        HirType::Int | HirType::I64 => MirType::I64,
        HirType::I8 => MirType::I8,
        HirType::I16 => MirType::I16,
        HirType::I32 => MirType::I32,
        HirType::I128 => MirType::I128,
        HirType::U8 => MirType::U8,
        HirType::U16 => MirType::U16,
        HirType::U32 => MirType::U32,
        HirType::U64 => MirType::U64,
        HirType::U128 => MirType::U128,
        HirType::Float | HirType::F64 => MirType::F64,
        HirType::F32 => MirType::F32,
        HirType::Bool => MirType::Bool,
        HirType::Char => MirType::String,
        HirType::String => MirType::String,
        HirType::Null => MirType::Unit,
        HirType::Tuple(types) => MirType::Tuple(types.iter().map(lower_type).collect()),
        HirType::Array(elem, _kind) => {
            MirType::Array(Box::new(lower_type(elem)), 0) // Size TBD
        }
        HirType::Class(name) | HirType::Instance(name) => MirType::Struct(name.clone()),
        HirType::Borrow(inner, is_mut) => MirType::Ref {
            kind: if *is_mut {
                RefKind::Mut
            } else {
                RefKind::Shared
            },
            inner: Box::new(lower_type(inner)),
        },
        HirType::BorrowImmut(inner) => MirType::Ref {
            kind: RefKind::Shared,
            inner: Box::new(lower_type(inner)),
        },
        HirType::BorrowMut(inner) => MirType::Ref {
            kind: RefKind::Mut,
            inner: Box::new(lower_type(inner)),
        },
        HirType::Shared(inner) => MirType::Arc(Box::new(lower_type(inner))),
        HirType::Weak(inner) => MirType::Weak(Box::new(lower_type(inner))),
        _ => MirType::I64, // Default fallback
    }
}

/// Lower HIR literal to MIR constant
fn lower_literal(lit: &HirLiteral) -> MirConstant {
    match lit {
        HirLiteral::Int(i) => MirConstant::Int(*i),
        HirLiteral::I8(i) => MirConstant::Int(*i as i64),
        HirLiteral::I16(i) => MirConstant::Int(*i as i64),
        HirLiteral::I32(i) => MirConstant::Int(*i as i64),
        HirLiteral::I64(i) => MirConstant::Int(*i),
        HirLiteral::I128(i) => MirConstant::Int(*i as i64),
        HirLiteral::U8(u) => MirConstant::UInt(*u as u64),
        HirLiteral::U16(u) => MirConstant::UInt(*u as u64),
        HirLiteral::U32(u) => MirConstant::UInt(*u as u64),
        HirLiteral::U64(u) => MirConstant::UInt(*u),
        HirLiteral::U128(u) => MirConstant::UInt(*u as u64),
        HirLiteral::Float(f) => MirConstant::Float(*f),
        HirLiteral::F32(f) => MirConstant::Float(*f as f64),
        HirLiteral::F64(f) => MirConstant::Float(*f),
        HirLiteral::Bool(b) => MirConstant::Bool(*b),
        HirLiteral::String(s) => MirConstant::String(s.clone()),
        HirLiteral::Null => MirConstant::Null,
        HirLiteral::Char(c) => MirConstant::Int(*c as i64),
        HirLiteral::BigInt(bi) => bi
            .to_string()
            .parse::<i64>()
            .map(MirConstant::Int)
            .unwrap_or_else(|_| MirConstant::String(bi.to_string())),
    }
}

/// Lower HIR binary operator to MIR binary operator
fn lower_binop(op: BinOp) -> MirBinOp {
    match op {
        BinOp::Add => MirBinOp::Add,
        BinOp::Sub => MirBinOp::Sub,
        BinOp::Mul => MirBinOp::Mul,
        BinOp::Div | BinOp::IntDiv => MirBinOp::Div,
        BinOp::Mod => MirBinOp::Rem,
        BinOp::Lt => MirBinOp::Lt,
        BinOp::Le => MirBinOp::Le,
        BinOp::Gt => MirBinOp::Gt,
        BinOp::Ge => MirBinOp::Ge,
        BinOp::Eq | BinOp::StrictEq => MirBinOp::Eq,
        BinOp::Ne | BinOp::StrictNe => MirBinOp::Ne,
        BinOp::And => MirBinOp::And,
        BinOp::Or => MirBinOp::Or,
        BinOp::BitAnd => MirBinOp::BitAnd,
        BinOp::BitOr => MirBinOp::BitOr,
        BinOp::BitXor => MirBinOp::BitXor,
        BinOp::ShiftLeft => MirBinOp::Shl,
        BinOp::ShiftRight => MirBinOp::Shr,
        _ => MirBinOp::Add, // Default fallback
    }
}

/// Lower HIR unary operator to MIR unary operator
fn lower_unaryop(op: UnaryOp) -> MirUnOp {
    match op {
        UnaryOp::Neg => MirUnOp::Neg,
        UnaryOp::Not | UnaryOp::BitNot => MirUnOp::Not,
        _ => MirUnOp::Not, // Default fallback
    }
}

/// Extract member fields from class methods (e.g. assignments to this.field)
fn extract_class_fields(hir_class: &HirClass) -> Vec<(String, MirType)> {
    let mut fields = Vec::new();
    let mut seen = std::collections::HashSet::new();

    fn scan_expr(
        expr: &HirExpr,
        fields: &mut Vec<(String, MirType)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match expr {
            HirExpr::SetMember(obj, field, _) => {
                if matches!(**obj, HirExpr::This) && !seen.contains(field) {
                    seen.insert(field.clone());
                    fields.push((field.clone(), MirType::I64));
                }
            }
            HirExpr::MemberAccess(obj, field) => {
                if matches!(**obj, HirExpr::This) && !seen.contains(field) {
                    seen.insert(field.clone());
                    fields.push((field.clone(), MirType::I64));
                }
            }
            _ => {}
        }
    }

    fn scan_stmt(
        stmt: &HirStmt,
        fields: &mut Vec<(String, MirType)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            HirStmt::Assign { target, .. } => {
                scan_expr(target, fields, seen);
            }
            HirStmt::Expr(expr) => {
                scan_expr(expr, fields, seen);
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                scan_expr(cond, fields, seen);
                scan_stmt(then_branch, fields, seen);
                if let Some(eb) = else_branch {
                    scan_stmt(eb, fields, seen);
                }
            }
            HirStmt::While { cond, body } => {
                scan_expr(cond, fields, seen);
                scan_stmt(body, fields, seen);
            }
            HirStmt::ForIn { iter, body, .. } => {
                scan_expr(iter, fields, seen);
                scan_stmt(body, fields, seen);
            }
            HirStmt::Block(stmts) => {
                for s in stmts.iter() {
                    scan_stmt(s, fields, seen);
                }
            }
            _ => {}
        }
    }

    for method in &hir_class.methods {
        for stmt in method.body.iter() {
            scan_stmt(stmt, &mut fields, &mut seen);
        }
    }

    fields
}
