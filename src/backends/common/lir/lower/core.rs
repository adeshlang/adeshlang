//! Core lowering infrastructure and main entry point
//!
//! This module contains the main HIR to LIR transformation entry point,
//! context structures, and core initialization logic.

use super::super::{BlockId, LirFunction, LirInst, LirModule, LirType, ValueId};
use crate::parsing::drop_insertion::DropPlan;
use crate::parsing::hir::{HirClass, HirFunction, HirModule, HirStmt};
use crate::parsing::hir_passes::plan_function_drops;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use super::functions::{lower_class_def, lower_decorator_application, lower_hir_function};
use super::statements::lower_stmt;

/// Global counter for generating unique lambda names
static LAMBDA_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique lambda function name
#[inline]
pub(super) fn next_lambda_name() -> String {
    let id = LAMBDA_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__lambda_{}", id)
}

/// Context for lowering HIR to LIR
pub(super) struct LowerCtx {
    pub(super) current_block: BlockId,
    pub(super) loop_exit: Option<BlockId>,
    pub(super) loop_cond: Option<BlockId>, // Loop condition block for `continue`
    pub(super) catch_block: Option<BlockId>, // For try/catch exception handling
    pub(super) decorated_fn_names: HashSet<String>, // Names of decorated functions (use call_indirect)
    pub(super) value_types: std::collections::HashMap<ValueId, LirType>, // Track the type of each value
    pub(super) var_types: std::collections::HashMap<String, LirType>, // Track the type of each variable by name
    pub(super) ptr_elem_sizes: std::collections::HashMap<ValueId, i64>, // Track pointer element sizes by value
    pub(super) var_ptr_elem_sizes: std::collections::HashMap<String, i64>, // Track pointer element sizes per variable
    pub(super) drop_kinds: std::collections::HashMap<String, DropLoweringKind>, // Track RC vs Weak for drop lowering
    pub(super) in_unsafe_context: bool, // Track whether we're inside an unsafe block
    /// Stack of defer scopes. Each scope is a list of defer blocks registered in that scope.
    /// Defers execute in LIFO order at scope exit. This is compile-time inlining (zero runtime overhead).
    pub(super) defer_scopes: Vec<Vec<Box<HirStmt>>>,
    /// Records `defer_scopes.len()` at each loop body entry, so `break`/`continue`
    /// know how many scopes to unwind (emit defers) before jumping.
    pub(super) loop_defer_depths: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DropLoweringKind {
    Shared,
    Weak,
    Borrow,
    Unknown,
}

/// Main entry point: Convert HIR module to LIR module
pub fn hir_to_lir(hir: &HirModule) -> Result<LirModule, String> {
    let mut lir = LirModule::new();

    // Collect global variables from top-level statements
    // This allows backends (like Native JIT) to allocate storage for them
    for stmt in &hir.statements {
        match stmt {
            HirStmt::Let { name, ty, .. } => {
                let lir_type = super::types::hir_type_to_lir(ty.as_ref());
                lir.globals.insert(name.clone(), lir_type);
            }
            HirStmt::LetTuple { names, .. } => {
                for name in names {
                    // Tuples are usually generic, default to I64 (dynamic)
                    lir.globals.insert(name.clone(), LirType::I64);
                }
            }
            HirStmt::ClassDef(class) => {
                // Class objects are handles/pointers
                lir.globals.insert(class.name.clone(), LirType::Ptr);
            }
            _ => {}
        }
    }

    let drop_plans = plan_function_drops(hir);

    // Collect decorated functions - they need special handling
    let mut decorated_functions: Vec<&HirFunction> = Vec::new();

    // Check if user defined a 'main' function
    let has_user_main = hir.functions.iter().any(|f| f.name == "main");

    // Lower all functions - decorated functions get a mangled name
    // Also rename user's main to __user_main if it exists
    for func in &hir.functions {
        let drop_plan = drop_plans.get(&func.name);
        if !func.decorators.is_empty() {
            decorated_functions.push(func);
            // Lower with mangled name - the real name will be assigned after decorator application
            let mut modified_func = func.clone();
            modified_func.name = format!("__decorated_original_{}", func.name);
            let lir_func = lower_hir_function(&mut lir, &modified_func, drop_plan)?;
            lir.add_function(lir_func);
        } else {
            let mut modified_func = func.clone();
            // Rename user's main to __user_main for automatic invocation
            if func.name == "main" {
                modified_func.name = "__user_main".to_string();
            }
            let lir_func = lower_hir_function(&mut lir, &modified_func, drop_plan)?;
            lir.add_function(lir_func);
        }
    }

    let has_content =
        !hir.statements.is_empty() || !hir.classes.is_empty() || !decorated_functions.is_empty();
    if has_content {
        let wrapper_name = "__top_level_wrapper".to_string();
        let drop_plan = drop_plans.get(&wrapper_name);
        let main_func = lower_main(
            &mut lir,
            &hir.statements,
            &hir.classes,
            &decorated_functions,
            wrapper_name,
            drop_plan,
        )?;
        lir.add_function(main_func);
    }

    // Create a new "main" entry point that calls __user_main then __top_level_wrapper (if exists)
    // This provides C/C++/Rust-style automatic main() execution
    let entry_main = create_entry_main(&mut lir, has_user_main, has_content)?;
    lir.add_function(entry_main);

    Ok(lir)
}

fn lower_main(
    lir: &mut LirModule,
    stmts: &[HirStmt],
    classes: &[HirClass],
    decorated_functions: &[&HirFunction],
    func_name: String,
    drop_plan: Option<&DropPlan>,
) -> Result<LirFunction, String> {
    let mut func = LirFunction::new(func_name, vec![], LirType::Void);
    let entry_block = func.entry_block;

    // Collect names of decorated functions - these must use call_indirect
    let decorated_fn_names: HashSet<String> =
        decorated_functions.iter().map(|f| f.name.clone()).collect();

    let mut ctx = LowerCtx {
        current_block: entry_block,
        loop_exit: None,
        loop_cond: None,
        catch_block: None,
        decorated_fn_names,
        value_types: std::collections::HashMap::new(),
        var_types: std::collections::HashMap::new(),
        ptr_elem_sizes: std::collections::HashMap::new(),
        var_ptr_elem_sizes: std::collections::HashMap::new(),
        drop_kinds: std::collections::HashMap::new(),
        in_unsafe_context: false,
        defer_scopes: vec![Vec::new()],
        loop_defer_depths: Vec::new(),
    };

    // First, lower class definitions so class variables are available
    for class in classes {
        lower_class_def(lir, &mut func, &mut ctx, class)?;
    }

    // Apply decorators to decorated functions BEFORE processing statements
    // This ensures that when statements call decorated functions, they get the decorated versions
    // Decorator functions are already available as they were lowered as regular functions
    for hir_func in decorated_functions {
        lower_decorator_application(lir, &mut func, &mut ctx, hir_func)?;
    }

    // Process top-level statements
    for (idx, stmt) in stmts.iter().enumerate() {
        let loc = format!("fn.{}", idx);
        lower_stmt(lir, &mut func, &mut ctx, stmt, &loc, drop_plan)?;
    }

    // Add return at the end if not already present
    if !has_return(&func) {
        func.push_to_block(ctx.current_block, LirInst::Return(None));
    }

    Ok(func)
}

/// Create the entry point "main" function that orchestrates execution
/// This implements the requested semantics: if the user defines `main`,
/// call `__user_main` first, then run `__top_level_wrapper` (initialization).
fn create_entry_main(
    _lir: &mut LirModule,
    has_user_main: bool,
    has_top_level: bool,
) -> Result<LirFunction, String> {
    let mut func = LirFunction::new("main".to_string(), vec![], LirType::Void);
    let entry_block = func.entry_block;

    // First run the top-level initialization wrapper if it exists (to initialize globals)
    if has_top_level {
        let wrapper_result = func.alloc_value();
        func.push_to_block(
            entry_block,
            LirInst::Call(wrapper_result, "__top_level_wrapper".to_string(), vec![]),
        );
    }

    // Then call __user_main if the user defined a main() function
    if has_user_main {
        let user_main_result = func.alloc_value();
        func.push_to_block(
            entry_block,
            LirInst::Call(user_main_result, "__user_main".to_string(), vec![]),
        );
    }

    // Return void
    func.push_to_block(entry_block, LirInst::Return(None));

    Ok(func)
}

pub(super) fn has_return(func: &LirFunction) -> bool {
    func.blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .any(|i| matches!(i, LirInst::Return(_)))
}

/// Check if the current block is already terminated (ends with Return, Jump, or JumpIf).
/// Used to avoid emitting dead code after early exits (return, break, continue).
#[inline]
pub(super) fn is_block_terminated(func: &LirFunction, block: BlockId) -> bool {
    func.get_block(block)
        .and_then(|b| b.instructions.last())
        .map_or(false, |last| {
            matches!(
                last,
                LirInst::Return(_) | LirInst::Jump(_) | LirInst::JumpIf(..)
            )
        })
}

/// Push a new defer scope. Called when entering a block.
#[inline]
pub(super) fn push_defer_scope(ctx: &mut LowerCtx) {
    ctx.defer_scopes.push(Vec::new());
}

/// Pop the current defer scope, returning its defers for inlining.
/// Called at normal block exit. The caller is responsible for emitting the defers.
#[inline]
pub(super) fn pop_defer_scope(ctx: &mut LowerCtx) -> Vec<Box<HirStmt>> {
    ctx.defer_scopes.pop().unwrap_or_default()
}

pub(super) fn is_builtin(name: &str) -> bool {
    let name = name.strip_prefix("std:").unwrap_or(name);
    if name.starts_with("fs.")
        || name.starts_with("FS.")
        || name.starts_with("input.")
        || name.starts_with("Input.")
        || name.starts_with("path.")
        || name.starts_with("Path.")
        || name.starts_with("crypto.")
        || name.starts_with("Crypto.")
        || name.starts_with("http.")
        || name.starts_with("HTTP.")
        || name.starts_with("Math.")
        || name.starts_with("JSON.")
        || name.starts_with("Regex.")
    {
        return true;
    }
    matches!(
        name,
        "print" | "len" | "input" | "type" | "str" | "int" | "float" |
        "concat" |
        "abs" | "sqrt" | "pow" | "min" | "max" | "floor" | "ceil" |
        "push" | "pop" | "range" |
        "clock" | "println" | "bool" | "Promise" | "setTimeout" | "sleep" |
        "Date" | "makeSet" | "typeof" | "sizeof" |
        "Error" | "error" |
        // Array metadata and access functions
        "capacity" | "metadata_size" | "get_index" | "array_get" | "first" | "last" |
        "insert" | "remove" | "reverse" | "hasKey" |
        // Command-line arguments
        "argc" | "argv" | "arg" | "args" | "execName" | "argsCount" |
        "argsSlice" | "argsJoin" | "argsIndexOf" | "parseArgs" | "argGet" | "argHas" |
        // Environment variables
        "env" | "envGet" | "envHas" | "envAll" | "envFromFile" | "envFileGet" |
        "envRuntimeGet" | "envRuntimeHas" | "envRuntimeAll" | "envRuntimeLoad" |
        "envSystemInfo" | "envCount" | "envFilter" | "envGetMany" | "envSetIfAbsent" |
        "envCommandLine" | "envUserInfo" |
        // Memory operations - will be lowered to special LIR instructions
        "alloc" | "free" | "__alloc" | "__free"
    )
}

pub(super) fn is_namespace(name: &str) -> bool {
    matches!(
        name,
        "input"
            | "Input"
            | "Math"
            | "Date"
            | "Time"
            | "time"
            | "JSON"
            | "File"
            | "System"
            | "Promise"
            | "Regex"
            | "Path"
            | "path"
            | "FS"
            | "fs"
            | "thread"
            | "Thread"
            | "Mutex"
            | "Channel"
            | "ThreadPool"
    )
}
