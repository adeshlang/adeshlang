//! Native JIT Runtime Execution
//!
//! This module handles the execution of JIT-compiled native code.

use super::context::NativeJitContext;

/// Execute a JIT-compiled program
///
/// Finds and calls the main function, returning its result.
pub fn execute_native_jit(context: NativeJitContext) -> Result<(), String> {
    execute_native_jit_with_timing(context).map(|_| ())
}

/// Execute a JIT-compiled program and return precise CPU execution time
pub fn execute_native_jit_with_timing(
    context: NativeJitContext,
) -> Result<std::time::Duration, String> {
    // Prefer synthesized entry, then fall back to wrapper/user-main for partial modules.
    let main_ptr = context
        .get_function("main")
        .or_else(|| context.get_function("__top_level_wrapper"))
        .or_else(|| context.get_function("__user_main"))
        .ok_or_else(|| {
            "No runnable entry point found (expected one of: main, __top_level_wrapper, __user_main)"
                .to_string()
        })?;

    // Cast to the appropriate function signature
    // main() returns void in AdeshLang (exit code 0 on success)
    type MainFn = extern "C" fn() -> ();
    let main_fn: MainFn = unsafe { std::mem::transmute(main_ptr) };

    // Measure pure CPU execution time
    let start = std::time::Instant::now();
    main_fn();
    let exec_time = start.elapsed();
    Ok(exec_time)
}
