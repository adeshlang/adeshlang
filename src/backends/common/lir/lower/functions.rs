//! Function and decorator lowering
//!
//! This module handles lowering of HIR functions, decorators, class definitions,
//! and lambda capture analysis.

use super::super::{LirFunction, LirInst, LirModule, LirType};
use super::core::{LowerCtx, has_return, is_block_terminated};
use super::expressions::lower_expr;
use super::memory::{emit_all_defers, record_var_kind};
use super::statements::lower_stmt;
use super::types::hir_type_to_lir;
use crate::parsing::drop_insertion::DropPlan;
use crate::parsing::hir::{HirClass, HirExpr, HirFunction, HirStmt, HirType};
use std::collections::HashSet;

/// Collect free variables from a lambda body
/// Returns variables that are used but not defined locally or as parameters
pub(super) fn collect_free_vars(
    params: &[(String, Option<HirType>)],
    body: &[HirStmt],
) -> Vec<String> {
    let mut defined: HashSet<String> = HashSet::new();
    let mut used: HashSet<String> = HashSet::new();

    // Parameters are defined
    for (name, _) in params {
        defined.insert(name.clone());
    }

    // Collect used and defined variables from body
    for stmt in body {
        collect_vars_from_stmt(stmt, &mut defined, &mut used);
    }

    // Free vars = used - defined - builtins
    let builtins: HashSet<&str> = [
        "print",
        "println",
        "len",
        "input",
        "type",
        "str",
        "int",
        "float",
        "bool",
        "abs",
        "sqrt",
        "pow",
        "min",
        "max",
        "floor",
        "ceil",
        "push",
        "pop",
        "map",
        "filter",
        "reduce",
        "range",
        "clock",
        "Promise",
        "setTimeout",
        "sleep",
        "Math",
        "Date",
        "Time",
        "JSON",
        "File",
        "System",
    ]
    .into_iter()
    .collect();

    let result: Vec<String> = used
        .into_iter()
        .filter(|v| !defined.contains(v) && !builtins.contains(v.as_str()))
        .collect();

    result
}

fn collect_vars_from_stmt(
    stmt: &HirStmt,
    defined: &mut HashSet<String>,
    used: &mut HashSet<String>,
) {
    match stmt {
        HirStmt::Let { name, init, .. } => {
            if let Some(expr) = init {
                collect_vars_from_expr(expr, used);
            }
            defined.insert(name.clone());
        }
        HirStmt::LetTuple { names, init, .. } => {
            if let Some(expr) = init {
                collect_vars_from_expr(expr, used);
            }
            for name in names {
                defined.insert(name.clone());
            }
        }
        HirStmt::Assign { target, value, .. } => {
            collect_vars_from_expr(target, used);
            collect_vars_from_expr(value, used);
        }
        HirStmt::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            collect_vars_from_expr(cond, used);
            collect_vars_from_stmt(then_branch, defined, used);
            if let Some(else_stmt) = else_branch {
                collect_vars_from_stmt(else_stmt, defined, used);
            }
        }
        HirStmt::While { cond, body, .. } => {
            collect_vars_from_expr(cond, used);
            collect_vars_from_stmt(body, defined, used);
        }
        HirStmt::ForIn {
            var, iter, body, ..
        } => {
            collect_vars_from_expr(iter, used);
            defined.insert(var.clone());
            collect_vars_from_stmt(body, defined, used);
        }
        HirStmt::Block(stmts) => {
            for s in stmts {
                collect_vars_from_stmt(s, defined, used);
            }
        }
        HirStmt::Return(Some(expr)) => {
            collect_vars_from_expr(expr, used);
        }
        HirStmt::Expr(expr) => {
            collect_vars_from_expr(expr, used);
        }
        HirStmt::FunctionDef {
            name, body, params, ..
        } => {
            defined.insert(name.clone());
            // Recursively check function body for captures
            let mut inner_defined: HashSet<String> =
                params.iter().map(|(n, _, _)| n.clone()).collect();
            for s in body.iter() {
                collect_vars_from_stmt(s, &mut inner_defined, used);
            }
        }
        HirStmt::TryCatch {
            try_block,
            error_name,
            catch_block,
        } => {
            collect_vars_from_stmt(try_block, defined, used);
            defined.insert(error_name.clone());
            collect_vars_from_stmt(catch_block, defined, used);
        }
        HirStmt::Throw(expr) => {
            collect_vars_from_expr(expr, used);
        }
        _ => {}
    }
}

pub(super) fn collect_vars_from_expr(expr: &HirExpr, used: &mut HashSet<String>) {
    match expr {
        HirExpr::LoadVar(name) => {
            used.insert(name.clone());
        }
        HirExpr::BinaryOp(left, _, right) => {
            collect_vars_from_expr(left, used);
            collect_vars_from_expr(right, used);
        }
        HirExpr::UnaryOp(_, inner) => {
            collect_vars_from_expr(inner, used);
        }
        HirExpr::Call(callee, args, _) => {
            collect_vars_from_expr(callee, used);
            for arg in args {
                collect_vars_from_expr(arg, used);
            }
        }
        HirExpr::MethodCall(obj, _, args) => {
            collect_vars_from_expr(obj, used);
            for arg in args {
                collect_vars_from_expr(arg, used);
            }
        }
        HirExpr::ArrayLiteral(elements)
        | HirExpr::SetLiteral(elements)
        | HirExpr::TupleLiteral(elements) => {
            for elem in elements {
                collect_vars_from_expr(elem, used);
            }
        }
        HirExpr::ObjectLiteral(fields) => {
            for (_, value) in fields {
                collect_vars_from_expr(value, used);
            }
        }
        HirExpr::StructLiteral(_, fields) => {
            for (_, value) in fields {
                collect_vars_from_expr(value, used);
            }
        }
        HirExpr::DictLiteral(entries) => {
            for (k, v) in entries {
                collect_vars_from_expr(k, used);
                collect_vars_from_expr(v, used);
            }
        }
        HirExpr::Index(target, index) => {
            collect_vars_from_expr(target, used);
            collect_vars_from_expr(index, used);
        }
        HirExpr::MemberAccess(obj, _) => {
            collect_vars_from_expr(obj, used);
        }
        HirExpr::SetMember(obj, _, value) => {
            collect_vars_from_expr(obj, used);
            collect_vars_from_expr(value, used);
        }
        HirExpr::Conditional(cond, then_expr, else_expr) => {
            collect_vars_from_expr(cond, used);
            collect_vars_from_expr(then_expr, used);
            collect_vars_from_expr(else_expr, used);
        }
        HirExpr::StoreVar(_, value) => {
            collect_vars_from_expr(value, used);
        }
        HirExpr::Lambda(params, body, _) => {
            // Collect free variables from the lambda body
            // This is needed for higher-order functions: nested lambdas may reference outer scope
            let lambda_free_vars = collect_free_vars(params, body);
            for var in lambda_free_vars {
                used.insert(var);
            }
        }
        HirExpr::NewInstance(_, args) => {
            for arg in args {
                collect_vars_from_expr(arg, used);
            }
        }
        HirExpr::Await(inner) => {
            collect_vars_from_expr(inner, used);
        }
        HirExpr::Spawn(inner) => {
            collect_vars_from_expr(inner, used);
        }
        HirExpr::Range(start, end, _) => {
            collect_vars_from_expr(start, used);
            collect_vars_from_expr(end, used);
        }
        HirExpr::Spread(inner) => {
            collect_vars_from_expr(inner, used);
        }
        HirExpr::OptionalGet(inner, _) => {
            collect_vars_from_expr(inner, used);
        }
        HirExpr::NonNull(inner) => {
            collect_vars_from_expr(inner, used);
        }
        HirExpr::Update(target, _, _) => {
            collect_vars_from_expr(target, used);
        }
        _ => {}
    }
}

/// Insert exception check after a call if inside try/catch
pub(super) fn insert_exception_check(func: &mut LirFunction, ctx: &mut LowerCtx) {
    let has_exc = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(has_exc, "__has_exception".to_string(), vec![]),
    );

    if let Some(catch_target) = ctx.catch_block {
        let continue_block = func.create_block("call_continue".to_string());
        func.push_to_block(
            ctx.current_block,
            LirInst::JumpIf(has_exc, catch_target, continue_block),
        );
        ctx.current_block = continue_block;
    } else {
        let exit_block = func.create_block("unhandled_exception_exit".to_string());
        let continue_block = func.create_block("call_continue".to_string());
        func.push_to_block(
            ctx.current_block,
            LirInst::JumpIf(has_exc, exit_block, continue_block),
        );
        ctx.current_block = exit_block;
        func.push_to_block(ctx.current_block, LirInst::Return(None));
        ctx.current_block = continue_block;
    }
}

/// Apply decorators to a function
/// This generates code to:
/// 1. Load the original (mangled) function
/// 2. Create a metadata object
/// 3. Call each decorator in reverse order with (target, meta)
/// 4. Store the final result as the function's real name
pub(super) fn lower_decorator_application(
    lir: &mut LirModule,
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    hir_func: &HirFunction,
) -> Result<(), String> {
    let original_name = format!("__decorated_original_{}", hir_func.name);

    // Load the original function
    let mut current_fn = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::LoadVar(current_fn, original_name),
    );

    // Create metadata object
    let meta_obj = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(meta_obj, "make_object".to_string(), vec![]),
    );

    // Add name field
    let name_key = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::ConstString(name_key, "name".to_string()),
    );
    let name_val = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::ConstString(name_val, hir_func.name.clone()),
    );
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(
            meta_obj,
            "set_field".to_string(),
            vec![meta_obj, name_key, name_val],
        ),
    );

    // Add type field
    let type_key = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::ConstString(type_key, "type".to_string()),
    );
    let type_val = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::ConstString(type_val, "function".to_string()),
    );
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(
            meta_obj,
            "set_field".to_string(),
            vec![meta_obj, type_key, type_val],
        ),
    );

    // Add params field as an array
    let params_key = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::ConstString(params_key, "params".to_string()),
    );
    let mut param_vals = Vec::new();
    for (param_name, param_type, _) in &hir_func.params {
        let pv = func.alloc_value();
        if let Some(ty) = param_type {
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(pv, format!("{}: {:?}", param_name, ty)),
            );
        } else {
            func.push_to_block(
                ctx.current_block,
                LirInst::ConstString(pv, param_name.clone()),
            );
        }
        param_vals.push(pv);
    }
    let params_arr = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(params_arr, "make_array".to_string(), param_vals),
    );
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(
            meta_obj,
            "set_field".to_string(),
            vec![meta_obj, params_key, params_arr],
        ),
    );

    // Apply decorators in reverse order (innermost first)
    for decorator in hir_func.decorators.iter().rev() {
        // Evaluate the decorator expression (could be a variable like `log` or a call like `memoize(maxSize: 100)`)
        let decorator_fn = lower_expr(lir, func, ctx, decorator)?;

        // Call decorator(current_fn, meta) using call_indirect
        let result = func.alloc_value();
        func.push_to_block(
            ctx.current_block,
            LirInst::CallBuiltin(
                result,
                "call_indirect".to_string(),
                vec![decorator_fn, current_fn, meta_obj],
            ),
        );

        current_fn = result;
    }

    // Store the decorated function with its real name
    func.push_to_block(
        ctx.current_block,
        LirInst::StoreVar(hir_func.name.clone(), current_fn),
    );
    func.set_var(hir_func.name.clone(), current_fn);

    Ok(())
}

pub(super) fn lower_hir_function(
    lir: &mut LirModule,
    hir_func: &HirFunction,
    drop_plan: Option<&DropPlan>,
) -> Result<LirFunction, String> {
    let params: Vec<(String, LirType)> = hir_func
        .params
        .iter()
        .map(|(name, ty, _default)| (name.clone(), hir_type_to_lir(ty.as_ref())))
        .collect();

    let ret_type = hir_type_to_lir(hir_func.ret_type.as_ref());
    let mut func = if hir_func.is_async {
        LirFunction::new_async(hir_func.name.clone(), params.clone(), ret_type)
    } else {
        LirFunction::new(hir_func.name.clone(), params.clone(), ret_type)
    };

    // Mark function as exported if it's marked as exported in HIR
    if hir_func.is_exported {
        func.is_exported = true;
    }
    let entry_block = func.entry_block;

    // Bind parameters to value IDs
    for (name, _ty) in &params {
        let param_val = func.alloc_value();
        func.push_to_block(entry_block, LirInst::LoadVar(param_val, name.clone()));
        func.set_var(name.clone(), param_val);
    }

    // Set up context for lowering
    let mut ctx = LowerCtx {
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

    // Track parameter types
    for (param_name, param_type, _) in &hir_func.params {
        if let Some(ty) = param_type {
            record_var_kind(&mut ctx, param_name, ty);
        }
    }

    // Apply default values for parameters that are null
    for (name, _ty, default) in &hir_func.params {
        if let Some(default_expr) = default {
            // Generate: if param == null { param = default }
            let param_val = func.get_var(name).unwrap_or_else(|| func.alloc_value());
            let is_null = func.alloc_value();
            func.push_to_block(
                ctx.current_block,
                LirInst::CallBuiltin(is_null, "is_null".to_string(), vec![param_val]),
            );

            // Create blocks for default assignment and after
            let default_block = func.create_block("default".to_string());
            let after_default = func.create_block("after_default".to_string());

            func.push_to_block(
                ctx.current_block,
                LirInst::JumpIf(is_null, default_block, after_default),
            );

            // Default block - assign default value
            ctx.current_block = default_block;
            let default_val = lower_expr(lir, &mut func, &mut ctx, default_expr)?;
            func.push_to_block(
                ctx.current_block,
                LirInst::StoreVar(name.clone(), default_val),
            );
            func.set_var(name.clone(), default_val);
            func.push_to_block(ctx.current_block, LirInst::Jump(after_default));

            ctx.current_block = after_default;
        }
    }

    for (idx, stmt) in hir_func.body.iter().enumerate() {
        let loc = format!("fn.{}", idx);
        lower_stmt(lir, &mut func, &mut ctx, stmt, &loc, drop_plan)?;
    }

    // Emit all remaining defers before function end (compile-time inlining).
    // Only emit if the current block is not already terminated (avoids dead code).
    if !is_block_terminated(&func, ctx.current_block) {
        emit_all_defers(lir, &mut func, &mut ctx, "fn.end", drop_plan)?;
    }
    // Clear all defer scopes to release memory (scopes are no longer needed)
    ctx.defer_scopes.clear();

    // Add return at the end if not already present
    if !has_return(&func) {
        func.push_to_block(ctx.current_block, LirInst::Return(None));
    }

    Ok(func)
}

/// Lower a class definition to create a class object stored as a variable
pub(super) fn lower_class_def(
    lir: &mut LirModule,
    func: &mut LirFunction,
    ctx: &mut LowerCtx,
    class: &HirClass,
) -> Result<(), String> {
    // Create a class object with the class name for instanceof checks
    let class_obj_val = func.alloc_value();
    // Store class as an object with name property: { __type__: "class", name: "ClassName" }
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(class_obj_val, "__make_class_object".to_string(), vec![]),
    );
    // Store the class name
    let name_val = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::ConstString(name_val, class.name.clone()),
    );

    // We must track the current class handle because runtime setters might return a NEW handle
    // (if they use remove_value/store_value logic).
    let mut current_class_hand = func.alloc_value();
    func.push_to_block(
        ctx.current_block,
        LirInst::CallBuiltin(
            current_class_hand,
            "__set_class_name".to_string(),
            vec![class_obj_val, name_val],
        ),
    );

    // Lower methods
    for method in &class.methods {
        let mangled_name = format!("{}_{}", class.name, method.name);

        // Create HirFunction from HirMethod
        let mut fn_params = Vec::new();
        // Add implicit 'this' parameter
        fn_params.push((
            "this".to_string(),
            Some(HirType::Instance(class.name.clone())),
            None, // No default value
        ));

        for (pname, ptype) in &method.params {
            fn_params.push((pname.clone(), ptype.clone(), None));
        }

        let hir_func = HirFunction {
            name: mangled_name.clone(),
            params: fn_params,
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

        // Lower the function
        let lir_func = lower_hir_function(lir, &hir_func, None)?;
        lir.add_function(lir_func);

        // Register method on class object
        let method_val = func.alloc_value();
        // Use ConstString to store the mangled name which will be used for lookup
        func.push_to_block(
            ctx.current_block,
            LirInst::ConstString(method_val, mangled_name.clone()),
        );

        let method_name_val = func.alloc_value();
        func.push_to_block(
            ctx.current_block,
            LirInst::ConstString(method_name_val, method.name.clone()),
        );

        let next_class_hand = func.alloc_value();
        func.push_to_block(
            ctx.current_block,
            LirInst::CallBuiltin(
                next_class_hand,
                "set_field".to_string(),
                vec![current_class_hand, method_name_val, method_val],
            ),
        );
        current_class_hand = next_class_hand;
    }

    // Store the updated class object (latest handle)
    func.push_to_block(
        ctx.current_block,
        LirInst::StoreVar(class.name.clone(), current_class_hand),
    );
    func.set_var(class.name.clone(), current_class_hand);
    Ok(())
}
